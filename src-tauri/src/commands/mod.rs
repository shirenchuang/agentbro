// Tauri IPC Commands — Bridge between frontend and Rust backend

pub mod buddy;
pub mod persistence;

use crate::agents::{AdapterInfo, AgentAdapter};
use crate::config::{AppConfig, ConfigStore};
use crate::hooks::conversation_parser::{
    all_projects_dirs, discover_session_file_in_dirs, ParsedMessage,
};
use crate::hooks::diagnostics::DiagnosticRingBuffer;
use crate::hooks::file_watcher::ConversationWatcher;
use crate::hooks::server::HookServer;
use crate::hooks::session_store::{SessionState, SessionStore};
use crate::license::{LicenseManager, LicenseStatus};
use crate::platform::display_controller::DisplayController;
use crate::remote::RemoteManager;
use crate::sound::SoundEngine;
use std::sync::{Arc, Mutex};

use tauri::State;

/// Shared app state accessible from Tauri commands
pub struct AppState {
    pub session_store: Arc<SessionStore>,
    pub hook_server: Arc<HookServer>,
    pub config_store: ConfigStore,
    pub adapters: Vec<Arc<dyn AgentAdapter>>,
    pub license_manager: LicenseManager,
    pub sound_engine: Option<Arc<SoundEngine>>,
    /// Conversation file watcher — watches JSONL files for real-time chat updates.
    /// Wrapped in Mutex because RecommendedWatcher is not Sync on all platforms.
    pub conversation_watcher: Arc<Mutex<Option<ConversationWatcher>>>,
    pub display_controller: Arc<DisplayController>,
    pub remote_manager: Arc<RemoteManager>,
    pub diagnostic_buffer: Arc<DiagnosticRingBuffer>,
}

// ── Session Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn get_sessions(state: State<'_, AppState>) -> Result<Vec<SessionState>, String> {
    Ok(state.session_store.get_all_sessions())
}

#[tauri::command]
pub async fn respond_permission(
    state: State<'_, AppState>,
    session_id: String,
    allowed: bool,
    always: Option<bool>,
) -> Result<(), String> {
    let always = always.unwrap_or(false);
    log::info!(
        "Permission response: session={}, allowed={}, always={}",
        session_id,
        allowed,
        always
    );

    // Try hook socket first
    let hook_result = state
        .hook_server
        .respond_permission(&session_id, allowed, always)
        .await;

    match hook_result {
        Ok(()) => Ok(()),
        Err(e) => {
            // Hook socket failed — fall back to tmux send-keys
            log::warn!(
                "Hook socket response failed for {}: {}. Falling back to tmux.",
                session_id,
                e
            );

            let session = state
                .session_store
                .get_session(&session_id)
                .ok_or_else(|| format!("Session {} not found", session_id))?;

            let pid = session
                .pid
                .ok_or_else(|| "Session has no PID for tmux fallback".to_string())?;

            let tmux_target = crate::terminal::approval::resolve_tmux_target(pid)
                .ok_or_else(|| "Could not find tmux pane for session".to_string())?;

            if allowed {
                if always {
                    crate::terminal::approval::approve_always(&tmux_target)
                        .map_err(|e| e.to_string())?;
                } else {
                    crate::terminal::approval::approve_once(&tmux_target)
                        .map_err(|e| e.to_string())?;
                }
            } else {
                crate::terminal::approval::reject(&tmux_target, None).map_err(|e| e.to_string())?;
            }

            // Clear pending permission since we handled it via tmux
            state
                .session_store
                .set_pending_permission(&session_id, None);
            Ok(())
        }
    }
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    session_id: String,
    message: String,
) -> Result<(), String> {
    log::info!("Send message: session={}, msg={}", session_id, message);

    let session = state
        .session_store
        .get_session(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    if is_codex_desktop_session(&session) {
        match send_message_to_codex_desktop(&session.id, &message) {
            Ok(()) => return Ok(()),
            Err(err) if session.tty.is_some() => {
                log::warn!("Codex Desktop send failed, falling back to TTY: {}", err);
            }
            Err(err) => return Err(err),
        }
    }

    let tty = session
        .tty
        .as_deref()
        .ok_or_else(|| "Session has no TTY".to_string())?;

    crate::agents::claude_code::send_message_to_terminal(tty, &message).map_err(|e| e.to_string())
}

fn is_codex_desktop_session(session: &SessionState) -> bool {
    session.agent_type == "codex"
        && (session.tty.is_none() || session.terminal.to_ascii_lowercase().contains("codex"))
}

fn send_message_to_codex_desktop(session_id: &str, message: &str) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("Codex Desktop message sending is only supported on macOS".to_string());
    }

    let opened_thread = std::process::Command::new("/usr/bin/open")
        .arg(format!("codex://threads/{}", session_id))
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !opened_thread {
        let opened_app = std::process::Command::new("/usr/bin/open")
            .args(["-a", "Codex"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        if !opened_app {
            return Err("Failed to activate Codex Desktop".to_string());
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(300));

    let script = build_codex_desktop_send_message_script(message);
    let output = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("Failed to run osascript: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn build_codex_desktop_send_message_script(message: &str) -> String {
    format!(
        r#"tell application "System Events"
  set previousClipboard to the clipboard as text
  set the clipboard to {message}
  keystroke "v" using command down
  delay 0.35
  set didSend to false
  try
    set frontProcess to first application process whose frontmost is true
    set frontWindow to front window of frontProcess
    set sendButtons to (entire contents of frontWindow) whose role is "AXButton" and (name is "Send" or description is "Send" or name is "发送" or description is "发送")
    if (count of sendButtons) > 0 then
      click item 1 of sendButtons
      set didSend to true
    end if
  end try
  if didSend is false then
    key code 36
  end if
  delay 0.1
  set the clipboard to previousClipboard
end tell"#,
        message = apple_script_string(message),
    )
}

fn apple_script_string(value: &str) -> String {
    let parts: Vec<String> = value
        .split('\n')
        .map(|part| format!("\"{}\"", part.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    parts.join(" & linefeed & ")
}

// ── Question Response Command ────────────────────────────────────

#[tauri::command]
pub async fn respond_question(
    state: State<'_, AppState>,
    session_id: String,
    answer: String,
) -> Result<(), String> {
    log::info!(
        "Question response: session={}, answer={}",
        session_id,
        answer
    );

    state
        .hook_server
        .respond_question(&session_id, answer)
        .await
        .map_err(|e| e.to_string())
}

// ── Plan Response Command ────────────────────────────────────────

#[tauri::command]
pub async fn respond_plan(
    state: State<'_, AppState>,
    session_id: String,
    mode: String,
    message: Option<String>,
) -> Result<(), String> {
    log::info!(
        "Plan response: session={}, mode={}, has_message={}",
        session_id,
        mode,
        message.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
    );

    state
        .hook_server
        .respond_plan(&session_id, mode, message)
        .await
        .map_err(|e| e.to_string())
}

// ── Auto-Approve Command ─────────────────────────────────────────

#[tauri::command]
pub async fn respond_auto_approve(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    log::info!("Auto-approve: session={}", session_id);

    let hook_result = state.hook_server.respond_auto_approve(&session_id).await;

    match hook_result {
        Ok(()) => Ok(()),
        Err(e) => {
            log::warn!(
                "Hook socket auto-approve failed for {}: {}. Falling back to tmux.",
                session_id,
                e
            );

            let session = state
                .session_store
                .get_session(&session_id)
                .ok_or_else(|| format!("Session {} not found", session_id))?;

            let pid = session
                .pid
                .ok_or_else(|| "Session has no PID for tmux fallback".to_string())?;

            let tmux_target = crate::terminal::approval::resolve_tmux_target(pid)
                .ok_or_else(|| "Could not find tmux pane for session".to_string())?;

            crate::terminal::approval::approve_always(&tmux_target).map_err(|e| e.to_string())?;

            state
                .session_store
                .set_pending_permission(&session_id, None);
            Ok(())
        }
    }
}

// ── Hook Verification Commands ───────────────────────────────────

#[tauri::command]
pub async fn verify_hooks(
    state: State<'_, AppState>,
    agent: String,
) -> Result<crate::agents::claude_code::HookVerificationResult, String> {
    log::info!("Verifying hooks for agent: {}", agent);
    let _adapter = state
        .adapters
        .iter()
        .find(|a| a.name() == agent)
        .ok_or_else(|| format!("Unknown agent: {}", agent))?;

    // Currently only claude-code supports verification
    if agent == "claude-code" {
        // Downcast isn't possible through dyn AgentAdapter, but we know
        // only ClaudeCodeAdapter exists — construct one to verify.
        let cc = crate::agents::claude_code::ClaudeCodeAdapter::new();
        Ok(cc.verify_hooks())
    } else {
        Err(format!(
            "Hook verification not supported for agent: {}",
            agent
        ))
    }
}

// ── Terminal Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn jump_to_terminal(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    log::info!("Jump to terminal: session={}", session_id);

    let session = state
        .session_store
        .get_session(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let pid = session.pid.unwrap_or(0);
    if pid == 0 && session.tty.as_deref().unwrap_or("").is_empty() {
        return Err("Session has no PID or TTY".to_string());
    }

    let tree = crate::terminal::process_tree::build_tree();
    let terminal_env = if pid == 0 {
        Default::default()
    } else {
        crate::terminal::process_tree::read_terminal_env(pid, &tree)
    };
    let tmux_pane = if pid == 0 {
        None
    } else {
        crate::terminal::tmux::find_pane_for_pid(pid).map(|pane| pane.target_string())
    };
    let jump_context = crate::terminal::jump::JumpContext {
        pid,
        iterm_session_id: terminal_env.iterm_session_id,
        kitty_window_id: terminal_env.kitty_window_id,
        tmux_pane,
        tmux_env: terminal_env.tmux,
        cwd: Some(session.cwd.clone()).filter(|cwd| !cwd.is_empty()),
        tty_path: session.tty.clone(),
    };

    match crate::terminal::jump::jump_to_terminal_with_context(&jump_context) {
        crate::terminal::jump::JumpResult::Success => Ok(()),
        crate::terminal::jump::JumpResult::SessionNotFound => Err("Session not found".to_string()),
        crate::terminal::jump::JumpResult::TerminalNotFound => {
            Err("Terminal not found in process tree".to_string())
        }
        crate::terminal::jump::JumpResult::Failed(msg) => Err(format!("Jump failed: {}", msg)),
    }
}

#[tauri::command]
pub async fn is_terminal_focused(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<bool, String> {
    let session = state
        .session_store
        .get_session(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let pid = session.pid.unwrap_or(0);
    if pid == 0 {
        return Ok(false);
    }

    Ok(crate::terminal::suppression::is_terminal_focused(pid))
}

// ── Config Commands ───────────────────────────────────────────────

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.config_store.get())
}

#[tauri::command]
pub async fn update_config(state: State<'_, AppState>, config: AppConfig) -> Result<(), String> {
    state.config_store.update(config)
}

#[tauri::command]
pub async fn set_island_feature_flags(
    state: State<'_, AppState>,
    tips_enabled: bool,
    pixel_cursor_enabled: bool,
    confetti_enabled: bool,
    follow_focus: bool,
) -> Result<(), String> {
    let mut config = state.config_store.get();
    config.tips_enabled = tips_enabled;
    config.pixel_cursor_enabled = pixel_cursor_enabled;
    config.confetti_enabled = confetti_enabled;
    config.follow_focus = follow_focus;
    state.config_store.update(config)
}

#[tauri::command]
pub async fn set_island_surface_options(
    state: State<'_, AppState>,
    island_surface_mode: String,
    island_pet_scale: u32,
) -> Result<(), String> {
    if !matches!(island_surface_mode.as_str(), "island" | "pet") {
        return Err(format!(
            "Unknown island surface mode: {}",
            island_surface_mode
        ));
    }
    let mut config = state.config_store.get();
    config.island_surface_mode = island_surface_mode;
    config.island_pet_scale = island_pet_scale.clamp(50, 120);
    state.config_store.update(config)
}

// ── Hook Management Commands ──────────────────────────────────────

#[tauri::command]
pub async fn install_hooks(state: State<'_, AppState>, agent: String) -> Result<(), String> {
    log::info!("Installing hooks for agent: {}", agent);
    let adapter = state
        .adapters
        .iter()
        .find(|a| a.name() == agent)
        .ok_or_else(|| format!("Unknown agent: {}", agent))?;
    adapter.install_hooks().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_hooks(state: State<'_, AppState>, agent: String) -> Result<(), String> {
    log::info!("Removing hooks for agent: {}", agent);
    let adapter = state
        .adapters
        .iter()
        .find(|a| a.name() == agent)
        .ok_or_else(|| format!("Unknown agent: {}", agent))?;
    adapter.remove_hooks().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_adapter_status(state: State<'_, AppState>) -> Result<Vec<AdapterInfo>, String> {
    let infos: Vec<AdapterInfo> = state
        .adapters
        .iter()
        .map(|a| AdapterInfo {
            name: a.name().to_string(),
            display_name: a.display_name().to_string(),
            icon: a.icon().to_string(),
            status: a.status(),
        })
        .collect();
    Ok(infos)
}

// ── Chat History Commands ────────────────────────────────────────

#[tauri::command]
pub async fn get_chat_history(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<ParsedMessage>, String> {
    // Look up the session to get its cwd and custom engine root for file discovery.
    let session = state.session_store.get_session(&session_id);
    let cwd = session.as_ref().map(|s| s.cwd.clone()).unwrap_or_default();

    let mut projects_dirs = all_projects_dirs();
    if let Some(root) = session
        .as_ref()
        .and_then(|s| s.engine_config_root.as_ref())
        .filter(|root| !root.is_empty())
    {
        let custom_projects = crate::agents::claude_code::expand_tilde(root).join("projects");
        if custom_projects.is_dir() && !projects_dirs.iter().any(|d| d == &custom_projects) {
            projects_dirs.push(custom_projects);
        }
    }

    // Try to discover the JSONL file for this session
    let file_path = discover_session_file_in_dirs(&session_id, &cwd, &projects_dirs)
        .ok_or_else(|| format!("No JSONL file found for session {}", session_id))?;

    // Try the watcher's parser first (it may already have state)
    if let Ok(watcher_guard) = state.conversation_watcher.lock() {
        if let Some(ref watcher) = *watcher_guard {
            if let Some(result) = watcher.parse_session_full(&session_id, file_path.clone()) {
                return Ok(result.all_messages);
            }
        }
    }

    // Fallback: create a one-off parser
    let mut parser = crate::hooks::conversation_parser::ConversationParser::new(file_path);
    parser
        .parse_full()
        .map_err(|e| format!("Failed to parse conversation: {}", e))
}

#[tauri::command]
pub async fn get_subagent_chat_history(
    state: State<'_, AppState>,
    session_id: String,
    transcript_path: String,
) -> Result<Vec<ParsedMessage>, String> {
    let session = state
        .session_store
        .get_session(&session_id)
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    parse_subagent_chat_history_for_session(&session, &transcript_path)
}

fn parse_subagent_chat_history_for_session(
    session: &SessionState,
    transcript_path: &str,
) -> Result<Vec<ParsedMessage>, String> {
    let requested_path = crate::agents::claude_code::expand_tilde(&transcript_path);
    let requested_path = requested_path
        .canonicalize()
        .map_err(|e| format!("Subagent transcript not found: {}", e))?;

    let allowed = session.subagents.iter().any(|subagent| {
        subagent
            .agent_transcript_path
            .as_ref()
            .or(subagent.transcript_path.as_ref())
            .map(|path| crate::agents::claude_code::expand_tilde(path))
            .and_then(|path| path.canonicalize().ok())
            .map(|path| path == requested_path)
            .unwrap_or(false)
    });

    if !allowed {
        return Err("Subagent transcript is not registered on this session".to_string());
    }

    let mut parser = crate::hooks::conversation_parser::ConversationParser::new(requested_path);
    parser
        .parse_full()
        .map_err(|e| format!("Failed to parse subagent conversation: {}", e))
}

// ── License Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn get_license_status(state: State<'_, AppState>) -> Result<LicenseStatus, String> {
    Ok(state.license_manager.check())
}

#[tauri::command]
pub async fn activate_license(
    state: State<'_, AppState>,
    license_key: String,
) -> Result<LicenseStatus, String> {
    state.license_manager.activate(&license_key)
}

#[tauri::command]
pub async fn deactivate_license(state: State<'_, AppState>) -> Result<LicenseStatus, String> {
    state.license_manager.deactivate()
}

// ── Diagnostics Commands ────────────────────────────────────────

#[tauri::command]
pub async fn export_diagnostics(state: State<'_, AppState>) -> Result<String, String> {
    let config = state.config_store.get();
    let sessions = state.session_store.get_all_sessions();
    let adapters: Vec<serde_json::Value> = state
        .adapters
        .iter()
        .map(|a| {
            serde_json::json!({
                "name": a.name(),
                "displayName": a.display_name(),
                "status": a.status(),
            })
        })
        .collect();
    let license = state.license_manager.check();

    let diagnostics = serde_json::json!({
        "appVersion": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "config": serde_json::to_value(&config).unwrap_or_default(),
        "sessionCount": sessions.len(),
        "adapters": adapters,
        "licenseStatus": serde_json::to_value(&license).unwrap_or_default(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    serde_json::to_string_pretty(&diagnostics).map_err(|e| e.to_string())
}

// ── Engine Instance Commands ────────────────────────────────────

#[tauri::command]
pub async fn add_engine_instance(
    state: State<'_, AppState>,
    label: String,
    config_root: String,
) -> Result<crate::config::EngineInstance, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let instance = crate::config::EngineInstance {
        id: id.clone(),
        label: label.clone(),
        config_root: config_root.clone(),
        enabled: true,
    };

    let mut config = state.config_store.get();
    config.engine_instances.push(instance.clone());
    state.config_store.update(config)?;

    // Install hooks for the new instance
    let root = crate::agents::claude_code::expand_tilde(&config_root);
    let adapter = crate::agents::claude_code::ClaudeCodeAdapter::with_config_root(root, label);
    if let Err(e) = adapter.install_hooks() {
        log::warn!("Failed to install hooks for new engine instance: {}", e);
    }

    Ok(instance)
}

#[tauri::command]
pub async fn remove_engine_instance(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let config = state.config_store.get();

    // Find the instance to remove hooks before deleting
    if let Some(inst) = config.engine_instances.iter().find(|i| i.id == id) {
        let root = crate::agents::claude_code::expand_tilde(&inst.config_root);
        let adapter = crate::agents::claude_code::ClaudeCodeAdapter::with_config_root(
            root,
            inst.label.clone(),
        );
        if let Err(e) = adapter.remove_hooks() {
            log::warn!("Failed to remove hooks for engine instance {}: {}", id, e);
        }
    }

    let mut config = config;
    config.engine_instances.retain(|i| i.id != id);
    state.config_store.update(config)
}

#[tauri::command]
pub async fn set_engine_instance_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut config = state.config_store.get();
    let Some(instance) = config.engine_instances.iter_mut().find(|i| i.id == id) else {
        return Err(format!("Engine instance {} not found", id));
    };
    instance.enabled = enabled;
    state.config_store.update(config)
}

#[tauri::command]
pub async fn verify_engine_path(path: String) -> Result<bool, String> {
    let expanded = crate::agents::claude_code::expand_tilde(&path);
    if !expanded.is_dir() {
        return Ok(false);
    }
    let settings = expanded.join("settings.json");
    Ok(settings.exists())
}

#[cfg(test)]
mod tests {
    use super::{
        apple_script_string, build_codex_desktop_send_message_script, is_codex_desktop_session,
        parse_subagent_chat_history_for_session,
    };
    use crate::hooks::session_store::{SessionState, SubagentInfo};
    use std::fs;

    fn session(agent_type: &str, terminal: &str, tty: Option<&str>) -> SessionState {
        let mut session = SessionState::new(
            "s1".to_string(),
            agent_type.to_string(),
            "project".to_string(),
            "/tmp/project".to_string(),
            terminal.to_string(),
        );
        session.tty = tty.map(ToString::to_string);
        session
    }

    #[test]
    fn apple_script_string_escapes_quotes_and_newlines() {
        assert_eq!(
            apple_script_string("say \"hi\"\nnext"),
            "\"say \\\"hi\\\"\" & linefeed & \"next\""
        );
    }

    #[test]
    fn codex_desktop_script_pastes_and_clicks_send_with_enter_fallback() {
        let script = build_codex_desktop_send_message_script("hello");

        assert!(script.contains("keystroke \"v\" using command down"));
        assert!(script.contains("name is \"Send\""));
        assert!(script.contains("name is \"发送\""));
        assert!(script.contains("key code 36"));
        assert!(script.contains("set the clipboard to previousClipboard"));
    }

    #[test]
    fn codex_desktop_detection_uses_missing_tty_or_codex_terminal() {
        assert!(is_codex_desktop_session(&session(
            "codex",
            "Codex",
            Some("/dev/ttys001")
        )));
        assert!(is_codex_desktop_session(&session(
            "codex", "AgentBro", None
        )));
        assert!(!is_codex_desktop_session(&session(
            "codex",
            "iTerm2",
            Some("/dev/ttys001")
        )));
        assert!(!is_codex_desktop_session(&session(
            "claude-code",
            "Codex",
            None
        )));
    }

    #[test]
    fn subagent_chat_history_requires_registered_transcript_path() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let transcript_path =
            std::env::temp_dir().join(format!("agentbro-subagent-history-{nonce}.jsonl"));
        let other_path =
            std::env::temp_dir().join(format!("agentbro-subagent-history-other-{nonce}.jsonl"));
        fs::write(
            &transcript_path,
            format!(
                "{}\n",
                serde_json::json!({
                    "type": "assistant",
                    "uuid": "assistant-1",
                    "timestamp": "2026-01-01T00:00:00.000Z",
                    "message": {
                        "role": "assistant",
                        "content": "Subagent result"
                    }
                })
            ),
        )
        .expect("write transcript");
        fs::write(&other_path, "").expect("write other transcript");

        let transcript_path = transcript_path.to_string_lossy().to_string();
        let mut session = session("claude-code", "iTerm", Some("/dev/ttys001"));
        session.subagents.push(SubagentInfo {
            agent_id: "agent-1".to_string(),
            agent_type: Some("research".to_string()),
            description: "Audit".to_string(),
            transcript_path: None,
            agent_transcript_path: Some(transcript_path.clone()),
            last_assistant_message: None,
            started_at: 1,
            completed_at: Some(2),
            status: "completed".to_string(),
            tools: Vec::new(),
        });

        let messages =
            parse_subagent_chat_history_for_session(&session, &transcript_path).expect("parse");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "assistant-1");

        let err = parse_subagent_chat_history_for_session(&session, &other_path.to_string_lossy())
            .expect_err("unregistered transcript should be rejected");
        assert!(err.contains("not registered"));

        let _ = fs::remove_file(transcript_path);
        let _ = fs::remove_file(other_path);
    }
}
