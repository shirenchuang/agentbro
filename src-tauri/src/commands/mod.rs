// Tauri IPC Commands — Bridge between frontend and Rust backend

pub mod persistence;

use crate::agents::{AdapterInfo, AgentAdapter};
use crate::config::{AppConfig, ConfigStore};
use crate::hooks::conversation_parser::{discover_session_file, ParsedMessage};
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
        session_id, allowed, always
    );

    // Try hook socket first
    let hook_result = state.hook_server
        .respond_permission(&session_id, allowed)
        .await;

    match hook_result {
        Ok(()) => Ok(()),
        Err(e) => {
            // Hook socket failed — fall back to tmux send-keys
            log::warn!(
                "Hook socket response failed for {}: {}. Falling back to tmux.",
                session_id, e
            );

            let session = state.session_store
                .get_session(&session_id)
                .ok_or_else(|| format!("Session {} not found", session_id))?;

            let pid = session.pid
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
                crate::terminal::approval::reject(&tmux_target, None)
                    .map_err(|e| e.to_string())?;
            }

            // Clear pending permission since we handled it via tmux
            state.session_store.set_pending_permission(&session_id, None);
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

    let session = state.session_store
        .get_session(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let tty = session.tty
        .as_deref()
        .ok_or_else(|| "Session has no TTY".to_string())?;

    crate::agents::claude_code::send_message_to_terminal(tty, &message)
        .map_err(|e| e.to_string())
}

// ── Auto-Approve Command ─────────────────────────────────────────

#[tauri::command]
pub async fn respond_auto_approve(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    log::info!("Auto-approve: session={}", session_id);

    let hook_result = state.hook_server
        .respond_auto_approve(&session_id)
        .await;

    match hook_result {
        Ok(()) => Ok(()),
        Err(e) => {
            log::warn!(
                "Hook socket auto-approve failed for {}: {}. Falling back to tmux.",
                session_id, e
            );

            let session = state.session_store
                .get_session(&session_id)
                .ok_or_else(|| format!("Session {} not found", session_id))?;

            let pid = session.pid
                .ok_or_else(|| "Session has no PID for tmux fallback".to_string())?;

            let tmux_target = crate::terminal::approval::resolve_tmux_target(pid)
                .ok_or_else(|| "Could not find tmux pane for session".to_string())?;

            crate::terminal::approval::approve_always(&tmux_target)
                .map_err(|e| e.to_string())?;

            state.session_store.set_pending_permission(&session_id, None);
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
    let _adapter = state.adapters
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
        Err(format!("Hook verification not supported for agent: {}", agent))
    }
}

// ── Terminal Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn jump_to_terminal(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    log::info!("Jump to terminal: session={}", session_id);

    let session = state.session_store
        .get_session(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let pid = session.pid
        .ok_or_else(|| "Session has no PID".to_string())?;

    match crate::terminal::jump::jump_to_terminal(pid) {
        crate::terminal::jump::JumpResult::Success => Ok(()),
        crate::terminal::jump::JumpResult::SessionNotFound => {
            Err("Session not found".to_string())
        }
        crate::terminal::jump::JumpResult::TerminalNotFound => {
            Err("Terminal not found in process tree".to_string())
        }
        crate::terminal::jump::JumpResult::Failed(msg) => {
            Err(format!("Jump failed: {}", msg))
        }
    }
}

#[tauri::command]
pub async fn is_terminal_focused(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<bool, String> {
    let session = state.session_store
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
pub async fn update_config(
    state: State<'_, AppState>,
    config: AppConfig,
) -> Result<(), String> {
    state.config_store.update(config)
}

// ── Hook Management Commands ──────────────────────────────────────

#[tauri::command]
pub async fn install_hooks(
    state: State<'_, AppState>,
    agent: String,
) -> Result<(), String> {
    log::info!("Installing hooks for agent: {}", agent);
    let adapter = state.adapters
        .iter()
        .find(|a| a.name() == agent)
        .ok_or_else(|| format!("Unknown agent: {}", agent))?;
    adapter.install_hooks().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_hooks(
    state: State<'_, AppState>,
    agent: String,
) -> Result<(), String> {
    log::info!("Removing hooks for agent: {}", agent);
    let adapter = state.adapters
        .iter()
        .find(|a| a.name() == agent)
        .ok_or_else(|| format!("Unknown agent: {}", agent))?;
    adapter.remove_hooks().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_adapter_status(
    state: State<'_, AppState>,
) -> Result<Vec<AdapterInfo>, String> {
    let infos: Vec<AdapterInfo> = state.adapters
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
    // Look up the session to get its cwd for file discovery
    let cwd = state
        .session_store
        .get_session(&session_id)
        .map(|s| s.cwd.clone())
        .unwrap_or_default();

    // Try to discover the JSONL file for this session
    let file_path = discover_session_file(&session_id, &cwd)
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
    let mut parser =
        crate::hooks::conversation_parser::ConversationParser::new(file_path);
    parser
        .parse_full()
        .map_err(|e| format!("Failed to parse conversation: {}", e))
}

// ── License Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn get_license_status(
    state: State<'_, AppState>,
) -> Result<LicenseStatus, String> {
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
pub async fn deactivate_license(
    state: State<'_, AppState>,
) -> Result<LicenseStatus, String> {
    state.license_manager.deactivate()
}

// ── Diagnostics Commands ────────────────────────────────────────

#[tauri::command]
pub async fn export_diagnostics(state: State<'_, AppState>) -> Result<String, String> {
    let config = state.config_store.get();
    let sessions = state.session_store.get_all_sessions();
    let adapters: Vec<serde_json::Value> = state.adapters.iter().map(|a| {
        serde_json::json!({
            "name": a.name(),
            "displayName": a.display_name(),
            "status": a.status(),
        })
    }).collect();
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
pub async fn remove_engine_instance(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let config = state.config_store.get();

    // Find the instance to remove hooks before deleting
    if let Some(inst) = config.engine_instances.iter().find(|i| i.id == id) {
        let root = crate::agents::claude_code::expand_tilde(&inst.config_root);
        let adapter = crate::agents::claude_code::ClaudeCodeAdapter::with_config_root(root, inst.label.clone());
        if let Err(e) = adapter.remove_hooks() {
            log::warn!("Failed to remove hooks for engine instance {}: {}", id, e);
        }
    }

    let mut config = config;
    config.engine_instances.retain(|i| i.id != id);
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
