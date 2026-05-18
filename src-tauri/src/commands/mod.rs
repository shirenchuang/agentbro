// Tauri IPC Commands — Bridge between frontend and Rust backend

pub mod buddy;
pub mod monitor;
pub mod persistence;

use crate::agents::{AdapterInfo, AgentAdapter};
use crate::config::CustomHookTemplate;
use crate::config::{AppConfig, ConfigStore};
use crate::hook_endpoint;
use crate::hooks::conversation_parser::{
    all_projects_dirs, discover_codex_session_file, discover_session_file_in_dirs,
    extract_subagents_from_transcript, ParsedMessage, TranscriptSubagentInfo,
};
use crate::hooks::diagnostics::DiagnosticRingBuffer;
use crate::hooks::file_watcher::ConversationWatcher;
use crate::hooks::server::HookServer;
use crate::hooks::session_store::{SessionState, SessionStore, SubagentInfo};
use crate::license::{LicenseManager, LicenseStatus};
use crate::network_monitor::NetworkMonitor;
use crate::platform::display_controller::DisplayController;
use crate::remote::RemoteManager;
use crate::sound::SoundEngine;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
    pub network_monitor: Arc<NetworkMonitor>,
    #[allow(dead_code)]
    pub tray_icon: tauri::tray::TrayIcon,
}

// ── Session Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn get_sessions(state: State<'_, AppState>) -> Result<Vec<SessionState>, String> {
    let sessions = state.session_store.get_all_sessions();
    for session in &sessions {
        hydrate_subagents_for_session(&state.session_store, session);
    }
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
        match send_message_to_codex_desktop(&session, &message) {
            Ok(()) => return Ok(()),
            Err(err) if session.tty.is_some() => {
                log::warn!("Codex Desktop send failed, falling back to TTY: {}", err);
            }
            Err(err) => return Err(err),
        }
    }

    let tty = resolve_session_tty(&session).ok_or_else(|| "Session has no TTY".to_string())?;
    if session.tty.as_deref() != Some(tty.as_str()) {
        let resolved_tty = tty.clone();
        state.session_store.update_session(&session_id, |s| {
            s.tty = Some(resolved_tty);
        });
    }

    crate::agents::claude_code::send_message_to_terminal(
        &tty,
        &message,
        &session.terminal,
        session.pid,
        session.term_bundle_id.as_deref(),
    )
    .map_err(|e| e.to_string())
}

fn is_codex_desktop_session(session: &SessionState) -> bool {
    session.agent_type == "codex"
        && session.tty.is_none()
        && !session.terminal.starts_with("/dev/")
        && (session.terminal.is_empty() || session.terminal.to_ascii_lowercase().contains("codex"))
}

fn resolve_session_tty(session: &SessionState) -> Option<String> {
    session
        .tty
        .as_deref()
        .filter(|tty| !tty.trim().is_empty())
        .map(normalize_tty_path)
        .or_else(|| {
            session
                .terminal
                .starts_with("/dev/tty")
                .then(|| session.terminal.clone())
        })
        .or_else(|| {
            let pid = session.pid?;
            let tree = crate::terminal::process_tree::build_tree();
            crate::terminal::process_tree::get_tty(pid, &tree)
                .as_deref()
                .map(normalize_tty_path)
        })
}

fn normalize_tty_path(tty: &str) -> String {
    if tty.starts_with("/dev/") {
        tty.to_string()
    } else {
        format!("/dev/{}", tty)
    }
}

fn open_codex_desktop_session(session_id: &str) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("Codex Desktop session jumping is only supported on macOS".to_string());
    }

    let opened_thread = std::process::Command::new("/usr/bin/open")
        .arg(format!("codex://threads/{}", session_id))
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if opened_thread {
        return Ok(());
    }

    let opened_app = std::process::Command::new("/usr/bin/open")
        .args(["-a", "Codex"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if opened_app {
        Ok(())
    } else {
        Err("Failed to activate Codex Desktop".to_string())
    }
}

fn send_message_to_codex_desktop(session: &SessionState, message: &str) -> Result<(), String> {
    send_message_to_codex_background(&session.id, message, &session.cwd)
}

fn send_message_to_codex_background(
    session_id: &str,
    message: &str,
    cwd: &str,
) -> Result<(), String> {
    let codex = resolve_codex_binary()
        .ok_or_else(|| "Could not find codex CLI for background send".to_string())?;

    let mut command = std::process::Command::new(&codex);
    command
        .args(codex_exec_resume_args(session_id))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if !cwd.trim().is_empty() && Path::new(cwd).is_dir() {
        command.current_dir(cwd);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start codex background send: {}", e))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Failed to open codex stdin".to_string())?;
    stdin
        .write_all(message.as_bytes())
        .map_err(|e| format!("Failed to write message to codex: {}", e))?;
    drop(stdin);

    let session_id = session_id.to_string();
    std::thread::spawn(move || match child.wait() {
        Ok(status) if status.success() => {
            log::debug!("Codex background send completed for {}", session_id);
        }
        Ok(status) => {
            log::warn!(
                "Codex background send exited with status {} for {}",
                status,
                session_id
            );
        }
        Err(err) => {
            log::warn!(
                "Codex background send wait failed for {}: {}",
                session_id,
                err
            );
        }
    });

    Ok(())
}

fn codex_exec_resume_args(session_id: &str) -> Vec<String> {
    vec![
        "exec".to_string(),
        "resume".to_string(),
        "--skip-git-repo-check".to_string(),
        session_id.to_string(),
        "-".to_string(),
    ]
}

fn resolve_codex_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CODEX_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    if let Some(path) = std::process::Command::new("which")
        .arg("codex")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|path| PathBuf::from(path.trim()))
        .filter(|path| path.is_file())
    {
        return Some(path);
    }

    codex_binary_candidates()
        .into_iter()
        .find(|path| path.is_file())
}

fn codex_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/opt/homebrew/bin/codex"),
        PathBuf::from("/usr/local/bin/codex"),
        PathBuf::from("/usr/bin/codex"),
    ];

    if let Some(home) = dirs::home_dir() {
        candidates.extend([
            home.join(".npm-global/bin/codex"),
            home.join(".local/bin/codex"),
            home.join(".bun/bin/codex"),
            home.join(".yarn/bin/codex"),
            home.join(".volta/bin/codex"),
        ]);

        let nvm_versions = home.join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(nvm_versions) {
            let mut nvm_candidates = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path().join("bin/codex"))
                .collect::<Vec<_>>();
            nvm_candidates.sort();
            nvm_candidates.reverse();
            candidates.extend(nvm_candidates);
        }
    }

    candidates
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

// ── Hook Lifecycle Simulation ─────────────────────────────────────

#[tauri::command]
pub async fn simulate_hook_event(
    event_name: String,
    tool_name: Option<String>,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;

    fn canonical_agent_id(agent: &str) -> &str {
        match agent {
            "gemini-cli" => "gemini",
            "traecli" | "trae-cli" => "traecli",
            "codybuddycn" => "codebuddycn",
            other => other,
        }
    }

    fn canonical_event_name(event: &str) -> &str {
        match event {
            "session_start" => "SessionStart",
            "session_end" => "SessionEnd",
            "user_prompt_submit" => "UserPromptSubmit",
            "pre_tool_use" => "PreToolUse",
            "post_tool_use" => "PostToolUse",
            "post_tool_use_failure" => "PostToolUseFailure",
            "permission_request" => "PermissionRequest",
            "permission_denied" => "PermissionDenied",
            "stop" => "Stop",
            "stop_failure" => "StopFailure",
            "SubagentsStop" => "SubagentStop",
            "SubagentEnd" => "SubagentStop",
            other => other,
        }
    }

    fn session_start_event_name(event: &str) -> &'static str {
        if event.contains('_') {
            "session_start"
        } else {
            "SessionStart"
        }
    }

    async fn send_payload(payload: serde_json::Value) -> Result<(), String> {
        let line = format!("{payload}\n");
        let bytes = line.as_bytes();
        let endpoint = hook_endpoint::current();

        if let Ok(mut stream) = tokio::net::UnixStream::connect(&endpoint.socket_path).await {
            stream.write_all(bytes).await.map_err(|e| e.to_string())
        } else {
            let mut stream = tokio::net::TcpStream::connect(endpoint.tcp_addr())
                .await
                .map_err(|e| e.to_string())?;
            stream.write_all(bytes).await.map_err(|e| e.to_string())
        }
    }

    let sid = format!("simulate-{}", uuid::Uuid::new_v4());
    let agent = canonical_agent_id(
        tool_name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("claude-code"),
    );
    let cwd = format!("/Users/demo/{}-hook-test", agent);
    let canonical_event = canonical_event_name(&event_name);
    let start_event = session_start_event_name(&event_name);
    let session_start = serde_json::json!({
        "agent": agent,
        "event": start_event,
        "session_id": sid,
        "cwd": cwd,
        "tty": "AgentBro Hook Tester",
        "terminal": "AgentBro Hook Tester",
    });

    let processing_payload = |event: &str, message: &str| {
        serde_json::json!({
            "agent": agent,
            "event": event,
            "session_id": sid,
            "cwd": cwd,
            "tty": "AgentBro Hook Tester",
            "terminal": "AgentBro Hook Tester",
            "prompt": message,
            "description": message,
        })
    };

    let tool_payload = |event: &str, message: &str, status_text: &str| {
        serde_json::json!({
            "agent": agent,
            "event": event,
            "session_id": sid,
            "cwd": cwd,
            "tty": "AgentBro Hook Tester",
            "terminal": "AgentBro Hook Tester",
            "description": message,
            "status": status_text,
            "tool": "Bash",
            "tool_name": "Bash",
            "tool_input": {
                "command": format!("echo '{}'", message.replace('\'', "'\\''"))
            },
            "toolInput": {
                "command": format!("echo '{}'", message.replace('\'', "'\\''"))
            },
        })
    };

    let permission_payload = |event: &str, message: &str| {
        serde_json::json!({
            "agent": agent,
            "event": event,
            "session_id": sid,
            "cwd": cwd,
            "tty": "AgentBro Hook Tester",
            "terminal": "AgentBro Hook Tester",
            "description": message,
            "tool": "Bash",
            "tool_name": "Bash",
            "tool_input": {
                "command": "cat ~/.ssh/id_rsa # AgentBro PermissionRequest 测试"
            },
            "toolInput": {
                "command": "cat ~/.ssh/id_rsa # AgentBro PermissionRequest 测试"
            },
            "diff": format!("[{}]\n\n{}", event, message),
        })
    };

    let notification_payload = |event: &str, message: &str| {
        serde_json::json!({
            "agent": agent,
            "event": event,
            "session_id": sid,
            "cwd": cwd,
            "tty": "AgentBro Hook Tester",
            "terminal": "AgentBro Hook Tester",
            "message": message,
        })
    };

    let completion_payload = |event: &str, message: &str| {
        serde_json::json!({
            "agent": agent,
            "event": event,
            "session_id": sid,
            "cwd": cwd,
            "tty": "AgentBro Hook Tester",
            "terminal": "AgentBro Hook Tester",
            "summary": message,
            "message": message,
            "last_assistant_message": message,
        })
    };

    let error_payload = |event: &str, message: &str| {
        serde_json::json!({
            "agent": agent,
            "event": event,
            "session_id": sid,
            "cwd": cwd,
            "tty": "AgentBro Hook Tester",
            "terminal": "AgentBro Hook Tester",
            "error": message,
            "message": message,
        })
    };

    let subagent_payload = |event: &str, message: &str, status_text: &str| {
        serde_json::json!({
            "agent": agent,
            "event": event,
            "session_id": sid,
            "cwd": cwd,
            "tty": "AgentBro Hook Tester",
            "terminal": "AgentBro Hook Tester",
            "description": message,
            "message": message,
            "last_assistant_message": message,
            "agent_id": "subagent-demo-001",
            "agent_status": status_text,
            "agent_type": "research",
        })
    };

    let test_message = format!(
        "正在测试 {} 的 {} 事件：这是 AgentBro 生成的模拟 Hook payload。",
        agent, event_name
    );

    let mut payloads = match canonical_event {
        "SessionStart" => vec![serde_json::json!({
            "agent": agent,
            "event": event_name,
            "session_id": sid,
            "cwd": format!("/Users/demo/{}-SessionStart-Hook", agent),
            "tty": "AgentBro Hook Tester",
            "terminal": "AgentBro Hook Tester",
        })],
        "SessionEnd" => vec![
            session_start.clone(),
            notification_payload(
                if event_name.contains('_') {
                    "notification"
                } else {
                    "Notification"
                },
                &format!("正在测试 {}：模拟会话即将结束。", event_name),
            ),
            processing_payload(&event_name, &test_message),
        ],
        "UserPromptSubmit" => vec![
            session_start.clone(),
            processing_payload(
                &event_name,
                &format!("正在测试 {}：模拟用户刚刚提交了一条新需求。", event_name),
            ),
        ],
        "PreToolUse" => vec![
            session_start.clone(),
            tool_payload(&event_name, &test_message, "running"),
        ],
        "PostToolUse" => vec![
            session_start.clone(),
            tool_payload(&event_name, &test_message, "success"),
        ],
        "PostToolUseFailure" | "PermissionDenied" => vec![
            session_start.clone(),
            tool_payload(&event_name, &test_message, "error"),
        ],
        "Notification" => vec![
            session_start.clone(),
            notification_payload(
                &event_name,
                &format!("正在测试 {}：这是一条模拟通知消息。", event_name),
            ),
        ],
        "Stop" => vec![
            session_start.clone(),
            completion_payload(
                &event_name,
                &format!("正在测试 {}：模拟任务已完成。", event_name),
            ),
        ],
        "StopFailure" => vec![
            session_start.clone(),
            error_payload(
                &event_name,
                &format!("正在测试 {}：模拟任务失败。", event_name),
            ),
        ],
        "PreCompact" | "PostCompact" => vec![
            session_start.clone(),
            processing_payload(
                &event_name,
                &format!("正在测试 {}：模拟上下文压缩阶段。", event_name),
            ),
        ],
        "PermissionRequest" => vec![
            session_start.clone(),
            permission_payload(
                &event_name,
                &format!(
                    "正在测试 {}：模拟一次需要用户确认的 Bash 操作。",
                    event_name
                ),
            ),
        ],
        "SubagentStop" => vec![
            session_start.clone(),
            subagent_payload(
                "SubagentStart",
                &format!("正在测试 {}：子 Agent 正在分析代码依赖关系。", event_name),
                "running",
            ),
            subagent_payload(&event_name, &test_message, "completed"),
        ],
        "SubagentStart" => vec![
            session_start.clone(),
            subagent_payload(&event_name, &test_message, "running"),
        ],
        other => return Err(format!("No simulation payload for event: {other}")),
    };
    if payloads.is_empty() {
        payloads.push(session_start);
    }

    let payload_count = payloads.len();
    for (index, payload) in payloads.into_iter().enumerate() {
        send_payload(payload).await?;
        if index + 1 < payload_count {
            tokio::time::sleep(std::time::Duration::from_millis(180)).await;
        }
    }
    Ok(())
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

    if is_codex_desktop_session(&session) {
        match open_codex_desktop_session(&session.id) {
            Ok(()) => return Ok(()),
            Err(err) if session.pid.is_some() || session.tty.is_some() => {
                log::warn!(
                    "Codex Desktop jump failed, falling back to terminal: {}",
                    err
                );
            }
            Err(err) => return Err(err),
        }
    }

    let pid = session.pid.unwrap_or(0);
    if pid == 0 && session.tty.as_deref().unwrap_or("").is_empty() {
        if session.terminal.trim().is_empty() || !can_fallback_to_terminal_app(&session.terminal) {
            return Err("Session has no terminal metadata to jump to".to_string());
        }
        return jump_to_terminal_fallback(&session.terminal, &session.cwd);
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
        wezterm_pane: session.wezterm_pane.or(terminal_env.wezterm_pane),
        zellij_pane_id: session.zellij_pane_id.or(terminal_env.zellij_pane_id),
        zellij_session_name: session
            .zellij_session_name
            .or(terminal_env.zellij_session_name),
        cmux_surface_id: session.cmux_surface_id.or(terminal_env.cmux_surface_id),
        cmux_workspace_id: session.cmux_workspace_id.or(terminal_env.cmux_workspace_id),
        tmux_pane,
        tmux_env: terminal_env.tmux,
        cwd: Some(session.cwd.clone()).filter(|cwd| !cwd.is_empty()),
        tty_path: session.tty.clone(),
        terminal_app: Some(session.terminal.clone()).filter(|terminal| !terminal.is_empty()),
        term_bundle_id: session.term_bundle_id.or(terminal_env.cf_bundle_identifier),
        agent_type: Some(session.agent_type.clone()),
    };

    match crate::terminal::jump::jump_to_terminal_with_context(&jump_context) {
        crate::terminal::jump::JumpResult::Success => Ok(()),
        crate::terminal::jump::JumpResult::SessionNotFound => Err("Session not found".to_string()),
        crate::terminal::jump::JumpResult::TerminalNotFound => {
            log::warn!(
                "Terminal not found in process tree for session {}. Falling back to app activation.",
                session_id
            );
            jump_to_terminal_fallback(&session.terminal, &session.cwd)
        }
        crate::terminal::jump::JumpResult::Failed(msg) => {
            log::warn!(
                "Precise terminal jump failed for session {}: {}. Falling back to app activation.",
                session_id,
                msg
            );
            jump_to_terminal_fallback(&session.terminal, &session.cwd)
        }
    }
}

fn jump_to_terminal_fallback(terminal: &str, cwd: &str) -> Result<(), String> {
    match jump_to_terminal_app_fallback(terminal) {
        Ok(()) => Ok(()),
        Err(app_err) => {
            log::warn!(
                "Terminal app fallback failed for {:?}: {}. Trying cwd fallback.",
                terminal,
                app_err
            );
            open_terminal_at_cwd(terminal, cwd).map_err(|cwd_err| {
                if cwd.trim().is_empty() {
                    app_err
                } else {
                    format!("{}; cwd fallback failed: {}", app_err, cwd_err)
                }
            })
        }
    }
}

fn jump_to_terminal_app_fallback(terminal: &str) -> Result<(), String> {
    if terminal.trim().is_empty() {
        return Err("Session has no PID, TTY, or terminal app".to_string());
    }
    if !can_fallback_to_terminal_app(terminal) {
        return Err(format!(
            "Session target {:?} is not a recognized terminal app",
            terminal
        ));
    }

    match crate::terminal::jump::jump_to_terminal_app(terminal) {
        crate::terminal::jump::JumpResult::Success => Ok(()),
        crate::terminal::jump::JumpResult::SessionNotFound => Err("Session not found".to_string()),
        crate::terminal::jump::JumpResult::TerminalNotFound => {
            Err("Terminal not found in process tree".to_string())
        }
        crate::terminal::jump::JumpResult::Failed(msg) => Err(format!("Jump failed: {}", msg)),
    }
}

fn can_fallback_to_terminal_app(terminal: &str) -> bool {
    crate::terminal::registry::is_terminal(terminal)
}

fn open_terminal_at_cwd(terminal: &str, cwd: &str) -> Result<(), String> {
    let cwd = cwd.trim();
    if cwd.is_empty() {
        return Err("Session has no working directory".to_string());
    }
    let path = std::path::Path::new(cwd);
    if !path.is_dir() {
        return Err(format!("Working directory {:?} does not exist", cwd));
    }

    #[cfg(target_os = "macos")]
    {
        let app = fallback_terminal_app_name(terminal);
        let output = std::process::Command::new("/usr/bin/open")
            .args(["-a", app, cwd])
            .output()
            .map_err(|e| format!("Failed to open terminal: {}", e))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("Opening a terminal at the session cwd is only supported on macOS".to_string())
    }
}

fn fallback_terminal_app_name(terminal: &str) -> &'static str {
    let lower = terminal.to_ascii_lowercase();
    if lower.contains("iterm") {
        "iTerm"
    } else if lower.contains("ghostty") {
        "Ghostty"
    } else if lower.contains("wezterm") || lower.contains("wez") {
        "WezTerm"
    } else if lower.contains("kitty") {
        "kitty"
    } else {
        "Terminal"
    }
}

fn osascript_ok(script: &str) -> bool {
    std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn find_binary(name: &str) -> Option<String> {
    let home = dirs::home_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    [
        format!("/opt/homebrew/bin/{name}"),
        format!("/usr/local/bin/{name}"),
        format!("/usr/bin/{name}"),
        format!("{home}/.local/bin/{name}"),
        format!("/Applications/cmux.app/Contents/Resources/bin/{name}"),
        format!("{home}/Applications/cmux.app/Contents/Resources/bin/{name}"),
        format!("/Applications/Kaku.app/Contents/MacOS/{name}"),
        format!("{home}/Applications/Kaku.app/Contents/MacOS/{name}"),
    ]
    .into_iter()
    .find(|path| std::path::Path::new(path).exists())
    .or_else(|| {
        std::process::Command::new("which")
            .arg(name)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
    })
}

fn agent_launch_command(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "claude-code" | "claude" => Some("claude"),
        "codex" => Some("codex"),
        "gemini" | "gemini-cli" => Some("gemini"),
        "cursor-cli" => Some("cursor-agent"),
        "copilot" => Some("gh copilot suggest"),
        "traecli" => Some("traecli"),
        "qoder-cli" => Some("qoder"),
        "qwen" => Some("qwen"),
        "kimi" => Some("kimi"),
        "opencode" => Some("opencode"),
        _ => None,
    }
}

fn launch_in_terminal(terminal: &str, cwd: &str, command: &str) -> Result<(), String> {
    let shell_command = format!("cd {} && {}", shell_quote(cwd), command);
    let lower = terminal.to_ascii_lowercase();

    let script = if lower.contains("iterm") {
        format!(
            r#"tell application "iTerm2"
    activate
    create window with default profile command "{}"
end tell"#,
            applescript_escape(&shell_command)
        )
    } else {
        format!(
            r#"tell application "Terminal"
    activate
    do script "{}"
end tell"#,
            applescript_escape(&shell_command)
        )
    };

    let output = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("Failed to run osascript: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn applescript_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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

    Ok(
        crate::terminal::suppression::is_terminal_focused_with_session(
            pid,
            session.term_bundle_id.as_deref(),
            session.wezterm_pane.as_deref(),
            session.zellij_pane_id.as_deref(),
            session.cmux_surface_id.as_deref(),
            session.tty.as_deref(),
        ),
    )
}

#[tauri::command]
pub async fn is_frontmost_app_fullscreen() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let script = r#"
tell application "System Events"
    set frontApp to first application process whose frontmost is true
    try
        set frontWindow to first window of frontApp
        set fullScreenValue to value of attribute "AXFullScreen" of frontWindow
        if fullScreenValue is true then return "true"
    end try
end tell
return "false"
"#;
        let output = std::process::Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&output.stdout).trim() == "true")
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(false)
    }
}

#[tauri::command]
pub async fn list_custom_hook_templates(
    state: State<'_, AppState>,
) -> Result<Vec<CustomHookTemplate>, String> {
    if !state.config_store.get().custom_hook_templates_enabled {
        return Err("Custom hook templates are disabled in settings".to_string());
    }
    Ok(state.config_store.get().custom_hook_templates)
}

#[tauri::command]
pub async fn upsert_custom_hook_template(
    state: State<'_, AppState>,
    template: CustomHookTemplate,
) -> Result<Vec<CustomHookTemplate>, String> {
    if !state.config_store.get().custom_hook_templates_enabled {
        return Err("Custom hook templates are disabled in settings".to_string());
    }
    let mut config = state.config_store.get();
    if let Some(existing) = config
        .custom_hook_templates
        .iter_mut()
        .find(|item| item.id == template.id)
    {
        *existing = template;
    } else {
        config.custom_hook_templates.push(template);
    }
    let templates = config.custom_hook_templates.clone();
    state.config_store.update(config)?;
    Ok(templates)
}

#[tauri::command]
pub async fn remove_custom_hook_template(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<CustomHookTemplate>, String> {
    if !state.config_store.get().custom_hook_templates_enabled {
        return Err("Custom hook templates are disabled in settings".to_string());
    }
    let mut config = state.config_store.get();
    config.custom_hook_templates.retain(|item| item.id != id);
    let templates = config.custom_hook_templates.clone();
    state.config_store.update(config)?;
    Ok(templates)
}

#[tauri::command]
pub async fn install_custom_hook_template(
    state: State<'_, AppState>,
    template: CustomHookTemplate,
) -> Result<(), String> {
    if !state.config_store.get().custom_hook_templates_enabled {
        return Err("Custom hook templates are disabled in settings".to_string());
    }
    let events: Vec<&str> = template.events.iter().map(String::as_str).collect();
    let path = crate::agents::claude_code::expand_tilde(&template.config_path);
    let command = if template.command.trim().is_empty() {
        format!(
            "{} --source {}",
            crate::agents::hook_manager::bridge_binary_path().display(),
            template.agent
        )
    } else {
        template.command
    };

    match template.format.as_str() {
        "json" => {
            let mut settings = crate::agents::hook_manager::read_json_config(&path);
            crate::agents::hook_manager::inject_hooks_json(&mut settings, &events, &command);
            crate::agents::hook_manager::write_json_config(&path, &settings)
                .map_err(|e| e.to_string())
        }
        "yaml" | "yml" => crate::agents::hook_manager::inject_hooks_yaml(&path, &command, &events)
            .map_err(|e| e.to_string()),
        "toml" => crate::agents::hook_manager::inject_hooks_toml(&path, &command, &events)
            .map_err(|e| e.to_string()),
        other => Err(format!("Unsupported hook template format: {other}")),
    }
}

#[tauri::command]
pub async fn remove_custom_hook_template_hooks(
    state: State<'_, AppState>,
    template: CustomHookTemplate,
) -> Result<(), String> {
    if !state.config_store.get().custom_hook_templates_enabled {
        return Err("Custom hook templates are disabled in settings".to_string());
    }
    let path = crate::agents::claude_code::expand_tilde(&template.config_path);
    match template.format.as_str() {
        "json" => {
            let mut settings = crate::agents::hook_manager::read_json_config(&path);
            crate::agents::hook_manager::remove_hooks_json(&mut settings);
            crate::agents::hook_manager::write_json_config(&path, &settings)
                .map_err(|e| e.to_string())
        }
        "yaml" | "yml" => {
            crate::agents::hook_manager::remove_hooks_yaml(&path).map_err(|e| e.to_string())
        }
        "toml" => crate::agents::hook_manager::remove_hooks_toml(&path).map_err(|e| e.to_string()),
        other => Err(format!("Unsupported hook template format: {other}")),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookDoctorCheck {
    pub id: String,
    pub label: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookDoctorReport {
    pub generated_at: i64,
    pub checks: Vec<HookDoctorCheck>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchAgentSessionRequest {
    pub agent_id: String,
    pub cwd: String,
    #[serde(default)]
    pub terminal: String,
    #[serde(default)]
    pub extra_args: String,
}

#[tauri::command]
pub async fn run_hook_doctor(state: State<'_, AppState>) -> Result<HookDoctorReport, String> {
    if !state.config_store.get().hook_doctor_enabled {
        return Err("Hook Doctor is disabled in settings".to_string());
    }

    let mut checks = Vec::new();
    let bridge = crate::agents::hook_manager::bridge_binary_path();
    checks.push(HookDoctorCheck {
        id: "bridge-binary".to_string(),
        label: "Bridge binary".to_string(),
        status: if bridge.exists() { "ok" } else { "error" }.to_string(),
        detail: bridge.display().to_string(),
    });

    checks.push(HookDoctorCheck {
        id: "hook-server".to_string(),
        label: "Hook server socket".to_string(),
        status: if std::path::Path::new(&hook_endpoint::current().socket_path).exists() {
            "ok"
        } else {
            "warn"
        }
        .to_string(),
        detail: hook_endpoint::current().socket_path,
    });

    let adapters = state
        .adapters
        .iter()
        .filter(|adapter| adapter.hooks_installed())
        .count();
    checks.push(HookDoctorCheck {
        id: "installed-hooks".to_string(),
        label: "Installed hooks".to_string(),
        status: if adapters > 0 { "ok" } else { "warn" }.to_string(),
        detail: format!("{adapters} adapter configs contain AgentBro hooks"),
    });

    checks.push(HookDoctorCheck {
        id: "automation-permission".to_string(),
        label: "macOS automation".to_string(),
        status: if osascript_ok(r#"tell application "System Events" to get name of first application process whose frontmost is true"#) {
            "ok"
        } else {
            "warn"
        }
        .to_string(),
        detail: "Required for terminal focus and fullscreen detection".to_string(),
    });

    for binary in [
        "tmux", "zellij", "cmux", "wezterm", "kaku", "kitten", "sqlite3",
    ] {
        checks.push(HookDoctorCheck {
            id: format!("binary-{binary}"),
            label: format!("{binary} binary"),
            status: if find_binary(binary).is_some() {
                "ok"
            } else {
                "warn"
            }
            .to_string(),
            detail: find_binary(binary).unwrap_or_else(|| "not found in common paths".to_string()),
        });
    }

    checks.push(HookDoctorCheck {
        id: "custom-templates".to_string(),
        label: "Custom hook templates".to_string(),
        status: "ok".to_string(),
        detail: format!(
            "{} templates configured",
            state.config_store.get().custom_hook_templates.len()
        ),
    });

    Ok(HookDoctorReport {
        generated_at: chrono::Utc::now().timestamp(),
        checks,
    })
}

#[tauri::command]
pub async fn launch_agent_session(
    state: State<'_, AppState>,
    request: LaunchAgentSessionRequest,
) -> Result<(), String> {
    if !state.config_store.get().session_launcher_enabled {
        return Err("Session Launcher is disabled in settings".to_string());
    }

    let cwd = request.cwd.trim();
    if cwd.is_empty() {
        return Err("Working directory is required".to_string());
    }
    if !std::path::Path::new(cwd).is_dir() {
        return Err(format!("Working directory does not exist: {cwd}"));
    }

    let base = agent_launch_command(&request.agent_id)
        .ok_or_else(|| format!("Unsupported launch agent: {}", request.agent_id))?;
    let mut command = if request.extra_args.trim().is_empty() {
        base.to_string()
    } else {
        format!("{} {}", base, request.extra_args.trim())
    };
    if matches!(request.agent_id.as_str(), "claude-code" | "claude") {
        if let Some(proxy_url) = state.network_monitor.proxy_url() {
            command = format!("ANTHROPIC_BASE_URL={} {}", shell_quote(&proxy_url), command);
        }
    }
    launch_in_terminal(
        if request.terminal.trim().is_empty() {
            "Terminal"
        } else {
            request.terminal.trim()
        },
        cwd,
        &command,
    )
}

// ── Config Commands ───────────────────────────────────────────────

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let mut config = state.config_store.get();
    config.launch_at_login = get_launch_at_login_state();
    Ok(config)
}

#[tauri::command]
pub async fn update_config(state: State<'_, AppState>, config: AppConfig) -> Result<(), String> {
    state.config_store.update(config)
}

#[tauri::command]
pub async fn set_launch_at_login(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    set_launch_at_login_state(enabled)?;
    let mut config = state.config_store.get();
    config.launch_at_login = enabled;
    state.config_store.update(config)
}

#[cfg(target_os = "macos")]
fn launch_agent_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Unable to resolve home directory".to_string())?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join("com.agentbro.desktop.login.plist"))
}

#[cfg(target_os = "macos")]
fn app_launch_target() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    for ancestor in exe.ancestors() {
        if ancestor.extension().and_then(|ext| ext.to_str()) == Some("app") {
            return Ok(ancestor.to_path_buf());
        }
    }
    Ok(exe)
}

#[cfg(target_os = "macos")]
fn escape_plist(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn set_launch_at_login_state(enabled: bool) -> Result<(), String> {
    let plist_path = launch_agent_path()?;
    if enabled {
        if let Some(parent) = plist_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let target = app_launch_target()?;
        let target = escape_plist(&target.to_string_lossy());
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.agentbro.desktop.login</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/bin/open</string>
    <string>{target}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#
        );
        std::fs::write(plist_path, plist).map_err(|e| e.to_string())?;
    } else if plist_path.exists() {
        let domain = format!("gui/{}", unsafe { libc::getuid() });
        let _ = Command::new("launchctl")
            .arg("bootout")
            .arg(domain)
            .arg(&plist_path)
            .output();
        std::fs::remove_file(plist_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn set_launch_at_login_state(_enabled: bool) -> Result<(), String> {
    Err("Launch at login is only supported on macOS for now".to_string())
}

#[cfg(target_os = "macos")]
fn get_launch_at_login_state() -> bool {
    launch_agent_path()
        .map(|path| path.exists())
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn get_launch_at_login_state() -> bool {
    false
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
    if config.island_surface_mode != island_surface_mode {
        config.island_pet_window_origin = None;
    }
    config.island_surface_mode = island_surface_mode;
    config.island_pet_scale = island_pet_scale.clamp(50, 120);
    state.config_store.update(config)
}

#[tauri::command]
pub async fn set_advanced_tool_flags(
    state: State<'_, AppState>,
    hook_doctor_enabled: bool,
    session_launcher_enabled: bool,
    custom_hook_templates_enabled: bool,
) -> Result<(), String> {
    let mut config = state.config_store.get();
    config.hook_doctor_enabled = hook_doctor_enabled;
    config.session_launcher_enabled = session_launcher_enabled;
    config.custom_hook_templates_enabled = custom_hook_templates_enabled;
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

    // Try to discover the JSONL file for this session.
    let file_path = if session.as_ref().is_some_and(|s| s.agent_type == "codex") {
        discover_codex_session_file(&session_id)
            .or_else(|| discover_session_file_in_dirs(&session_id, &cwd, &projects_dirs))
    } else {
        discover_session_file_in_dirs(&session_id, &cwd, &projects_dirs)
    }
    .ok_or_else(|| format!("No JSONL file found for session {}", session_id))?;

    hydrate_subagents_from_file(&state.session_store, &session_id, &file_path);

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

fn hydrate_subagents_for_session(store: &SessionStore, session: &SessionState) {
    if session
        .subagents
        .iter()
        .any(|subagent| subagent.agent_transcript_path.is_some())
    {
        return;
    }
    if let Some(path) = discover_transcript_for_session(session) {
        hydrate_subagents_from_file(store, &session.id, &path);
    }
}

fn discover_transcript_for_session(session: &SessionState) -> Option<PathBuf> {
    let mut projects_dirs = all_projects_dirs();
    if let Some(root) = session
        .engine_config_root
        .as_ref()
        .filter(|root| !root.is_empty())
    {
        let custom_projects = crate::agents::claude_code::expand_tilde(root).join("projects");
        if custom_projects.is_dir() && !projects_dirs.iter().any(|d| d == &custom_projects) {
            projects_dirs.push(custom_projects);
        }
    }

    if session.agent_type == "codex" {
        discover_codex_session_file(&session.id)
            .or_else(|| discover_session_file_in_dirs(&session.id, &session.cwd, &projects_dirs))
    } else {
        discover_session_file_in_dirs(&session.id, &session.cwd, &projects_dirs)
    }
}

fn hydrate_subagents_from_file(store: &SessionStore, session_id: &str, file_path: &Path) {
    let recovered = extract_subagents_from_transcript(file_path);
    if recovered.is_empty() {
        return;
    }

    store.update_session(session_id, |session| {
        for subagent in recovered {
            merge_subagent(session, subagent);
        }
        session.subagents.sort_by(|a, b| {
            a.started_at
                .cmp(&b.started_at)
                .then_with(|| a.agent_id.cmp(&b.agent_id))
        });
    });
}

fn merge_subagent(session: &mut SessionState, recovered: TranscriptSubagentInfo) {
    let incoming = SubagentInfo {
        agent_id: recovered.agent_id,
        agent_type: recovered.agent_type,
        description: recovered.description,
        transcript_path: recovered.transcript_path,
        agent_transcript_path: recovered.agent_transcript_path,
        last_assistant_message: recovered.last_assistant_message,
        started_at: recovered.started_at,
        completed_at: recovered.completed_at,
        status: recovered.status,
        tools: recovered.tools,
    };

    if let Some(existing) = session
        .subagents
        .iter_mut()
        .find(|item| item.agent_id == incoming.agent_id)
    {
        *existing = incoming;
    } else {
        session.subagents.push(incoming);
    }
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
    let requested_path = crate::agents::claude_code::expand_tilde(transcript_path);
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
    let instance_snapshot = instance.clone();
    instance.enabled = enabled;
    state.config_store.update(config)?;

    let root = crate::agents::claude_code::expand_tilde(&instance_snapshot.config_root);
    let adapter = crate::agents::claude_code::ClaudeCodeAdapter::with_config_root(
        root,
        instance_snapshot.label,
    );
    let result = if enabled {
        adapter.install_hooks()
    } else {
        adapter.remove_hooks()
    };
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn verify_engine_path(path: String) -> Result<bool, String> {
    let expanded = crate::agents::claude_code::expand_tilde(&path);
    Ok(expanded.is_dir())
}

#[cfg(test)]
mod tests {
    use super::{
        can_fallback_to_terminal_app, codex_exec_resume_args, fallback_terminal_app_name,
        is_codex_desktop_session, parse_subagent_chat_history_for_session, resolve_session_tty,
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
    fn codex_desktop_send_uses_background_exec_resume_args() {
        assert_eq!(
            codex_exec_resume_args("session-123"),
            vec![
                "exec",
                "resume",
                "--skip-git-repo-check",
                "session-123",
                "-"
            ]
        );
    }

    #[test]
    fn codex_desktop_detection_uses_missing_tty_or_codex_terminal() {
        // Desktop: no tty, terminal is empty or contains "codex"
        assert!(is_codex_desktop_session(&session("codex", "", None)));
        assert!(is_codex_desktop_session(&session("codex", "Codex", None)));
        assert!(!is_codex_desktop_session(&session(
            "codex", "AgentBro", None
        )));
        // CLI: terminal is a tty path — not desktop even if tty field is None
        assert!(!is_codex_desktop_session(&session(
            "codex",
            "/dev/ttys001",
            None
        )));
        // CLI: has explicit tty field set
        assert!(!is_codex_desktop_session(&session(
            "codex",
            "iTerm2",
            Some("/dev/ttys001")
        )));
        // Wrong agent type
        assert!(!is_codex_desktop_session(&session(
            "claude-code",
            "Codex",
            None
        )));
    }

    #[test]
    fn terminal_app_fallback_rejects_agent_app_labels() {
        assert!(can_fallback_to_terminal_app("iTerm2"));
        assert!(can_fallback_to_terminal_app("Terminal"));
        assert!(!can_fallback_to_terminal_app("Claude"));
        assert!(!can_fallback_to_terminal_app("AntCC"));
    }

    #[test]
    fn cwd_fallback_uses_real_terminal_app_names() {
        assert_eq!(fallback_terminal_app_name("iTerm·tmux"), "iTerm");
        assert_eq!(fallback_terminal_app_name("Ghostty"), "Ghostty");
        assert_eq!(fallback_terminal_app_name("AntCC"), "Terminal");
        assert_eq!(fallback_terminal_app_name(""), "Terminal");
    }

    #[test]
    fn send_message_resolves_tty_from_stored_tty_or_terminal_path() {
        assert_eq!(
            resolve_session_tty(&session("claude-code", "iTerm2", Some("ttys001"))),
            Some("/dev/ttys001".to_string())
        );
        assert_eq!(
            resolve_session_tty(&session("claude-code", "/dev/ttys002", None)),
            Some("/dev/ttys002".to_string())
        );
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
