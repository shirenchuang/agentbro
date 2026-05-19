// buddy.rs — Read Claude Buddy data from ~/.claude.json

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

use super::AppState;
use crate::config::BuddyDeviceConfig;
use crate::hooks::session_store::SessionStore;

fn claude_json_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude.json"))
}

/// Read the buddy section from ~/.claude.json and return it as a JSON string.
/// Returns a default stub if the file doesn't exist or has no buddy section.
#[tauri::command]
pub fn read_buddy_data() -> Result<String, String> {
    let path = claude_json_path().ok_or("Cannot determine home directory")?;

    if !path.exists() {
        return Ok(default_buddy_json());
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    // Extract buddy sub-object if present
    if let Some(buddy) = parsed.get("buddy") {
        return serde_json::to_string(buddy).map_err(|e| e.to_string());
    }

    Ok(default_buddy_json())
}

fn default_buddy_json() -> String {
    r#"{
  "species": "cat",
  "name": "Claude",
  "level": 1,
  "xp": 0,
  "xpMax": 100,
  "happiness": 50,
  "energy": 50,
  "interactions": 0
}"#
    .to_string()
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuddyDeviceSession {
    pub id: String,
    pub agent_type: String,
    pub source_slot: u8,
    pub project: String,
    pub phase: String,
    pub status_code: u8,
    pub needs_attention: bool,
    pub last_tool_name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuddyDeviceSnapshot {
    pub protocol_version: u32,
    pub sessions: Vec<BuddyDeviceSession>,
    pub focus_action: String,
}

/// Compact JSON protocol payload for Apple Watch / ESP32 companion devices.
#[tauri::command]
pub fn buddy_device_snapshot(state: State<'_, AppState>) -> Result<BuddyDeviceSnapshot, String> {
    let sessions = state
        .session_store
        .get_all_sessions()
        .into_iter()
        .map(|session| BuddyDeviceSession {
            id: session.id,
            source_slot: buddy_source_slot(&session.agent_type),
            agent_type: session.agent_type,
            project: session.project,
            phase: serde_json::to_value(&session.phase)
                .ok()
                .and_then(|value| value.as_str().map(ToString::to_string))
                .unwrap_or_else(|| "idle".to_string()),
            status_code: buddy_status_code(&session.phase, session.last_tool_status.as_deref()),
            needs_attention: session.phase.needs_attention(),
            last_tool_name: session.last_tool_name,
        })
        .collect();

    Ok(BuddyDeviceSnapshot {
        protocol_version: 1,
        sessions,
        focus_action: "buddy_reverse_focus".to_string(),
    })
}

#[tauri::command]
pub fn get_buddy_device_config(state: State<'_, AppState>) -> Result<BuddyDeviceConfig, String> {
    Ok(state.config_store.get().buddy_device)
}

#[tauri::command]
pub fn set_buddy_device_config(
    state: State<'_, AppState>,
    config: BuddyDeviceConfig,
) -> Result<BuddyDeviceConfig, String> {
    let mut app_config = state.config_store.get();
    app_config.buddy_device = config.clone();
    state.config_store.update(app_config)?;
    Ok(config)
}

/// Reverse-focus entrypoint used by external Buddy devices.
#[tauri::command]
pub async fn buddy_reverse_focus(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    super::jump_to_terminal(state, session_id).await
}

pub fn start_buddy_device_server(config: BuddyDeviceConfig, store: Arc<SessionStore>) {
    if !config.enabled || config.transport != "http" {
        return;
    }

    std::thread::Builder::new()
        .name("buddy-device-http".to_string())
        .spawn(move || {
            let addr = format!("127.0.0.1:{}", config.port);
            let listener = match TcpListener::bind(&addr) {
                Ok(listener) => listener,
                Err(err) => {
                    log::warn!("Buddy device HTTP bridge failed to bind {addr}: {err}");
                    return;
                }
            };
            log::info!("Buddy device HTTP bridge listening on {addr}");

            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => handle_buddy_http_stream(stream, &config, &store),
                    Err(err) => log::warn!("Buddy device HTTP bridge accept error: {err}"),
                }
            }
        })
        .ok();
}

fn handle_buddy_http_stream(
    mut stream: TcpStream,
    config: &BuddyDeviceConfig,
    store: &Arc<SessionStore>,
) {
    let mut buffer = [0_u8; 4096];
    let Ok(size) = stream.read(&mut buffer) else {
        return;
    };
    let request = String::from_utf8_lossy(&buffer[..size]);
    let first_line = request.lines().next().unwrap_or("");
    let path = first_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();

    if !config.shared_secret.is_empty()
        && !path.contains(&format!("secret={}", config.shared_secret))
    {
        write_http_response(&mut stream, 403, "text/plain", "forbidden");
        return;
    }

    if path.starts_with("/snapshot") {
        let snapshot = snapshot_from_store(store);
        match serde_json::to_string(&snapshot) {
            Ok(body) => write_http_response(&mut stream, 200, "application/json", &body),
            Err(err) => write_http_response(&mut stream, 500, "text/plain", &err.to_string()),
        }
        return;
    }

    if let Some(session_id) = path
        .strip_prefix("/focus/")
        .and_then(|value| value.split('?').next())
    {
        match jump_session_from_store(store, session_id) {
            Ok(()) => write_http_response(&mut stream, 200, "application/json", r#"{"ok":true}"#),
            Err(err) => write_http_response(&mut stream, 404, "text/plain", &err),
        }
        return;
    }

    write_http_response(
        &mut stream,
        404,
        "text/plain",
        "use /snapshot or /focus/<session_id>",
    );
}

fn write_http_response(stream: &mut TcpStream, status: u16, content_type: &str, body: &str) {
    let reason = match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

fn snapshot_from_store(store: &Arc<SessionStore>) -> BuddyDeviceSnapshot {
    let sessions = store
        .get_all_sessions()
        .into_iter()
        .map(|session| BuddyDeviceSession {
            id: session.id,
            source_slot: buddy_source_slot(&session.agent_type),
            agent_type: session.agent_type,
            project: session.project,
            phase: serde_json::to_value(&session.phase)
                .ok()
                .and_then(|value| value.as_str().map(ToString::to_string))
                .unwrap_or_else(|| "idle".to_string()),
            status_code: buddy_status_code(&session.phase, session.last_tool_status.as_deref()),
            needs_attention: session.phase.needs_attention(),
            last_tool_name: session.last_tool_name,
        })
        .collect();

    BuddyDeviceSnapshot {
        protocol_version: 1,
        sessions,
        focus_action: "GET /focus/<session_id>".to_string(),
    }
}

fn buddy_source_slot(agent_type: &str) -> u8 {
    match agent_type {
        "claude-code" | "claude" => 0,
        "codex" => 1,
        "gemini" | "gemini-cli" => 2,
        "cursor" | "cursor-cli" => 3,
        "copilot" => 4,
        "trae" | "traecn" | "traecli" => 5,
        "qoder" | "qoder-cli" => 6,
        "droid" | "factory" | "factory-droid" => 7,
        "codebuddy" | "codebuddycn" | "codybuddycn" => 8,
        "stepfun" => 9,
        "opencode" => 10,
        "qwen" => 11,
        "antigravity" => 12,
        "workbuddy" => 13,
        "hermes" => 14,
        "kimi" => 15,
        "deepseek" => 16,
        _ => 255,
    }
}

fn buddy_status_code(
    phase: &crate::hooks::session_store::SessionPhase,
    last_tool_status: Option<&str>,
) -> u8 {
    use crate::hooks::session_store::SessionPhase;
    match phase {
        SessionPhase::WaitingApproval => 3,
        SessionPhase::WaitingInput => 4,
        SessionPhase::Processing | SessionPhase::Compacting => {
            if matches!(last_tool_status, Some("running")) {
                2
            } else {
                1
            }
        }
        _ => 0,
    }
}

fn jump_session_from_store(store: &Arc<SessionStore>, session_id: &str) -> Result<(), String> {
    let session = store
        .get_session(session_id)
        .ok_or_else(|| format!("Session {session_id} not found"))?;
    let pid = session.pid.unwrap_or(0);
    if pid == 0 {
        if session.terminal.trim().is_empty()
            || !crate::terminal::registry::is_terminal(&session.terminal)
        {
            return Err("Session has no terminal metadata to jump to".to_string());
        }
        return crate::terminal::jump::jump_to_terminal_app(&session.terminal).into_result();
    }

    let tree = crate::terminal::process_tree::build_tree();
    let terminal_env = crate::terminal::process_tree::read_terminal_env(pid, &tree);
    let context = crate::terminal::jump::JumpContext {
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
        tmux_pane: crate::terminal::tmux::find_pane_for_pid(pid).map(|pane| pane.target_string()),
        tmux_env: terminal_env.tmux,
        cwd: Some(session.cwd).filter(|cwd| !cwd.is_empty()),
        tty_path: session.tty,
        terminal_app: Some(session.terminal).filter(|terminal| !terminal.is_empty()),
        term_bundle_id: session.term_bundle_id.or(terminal_env.cf_bundle_identifier),
        agent_type: Some(session.agent_type),
    };
    crate::terminal::jump::jump_to_terminal_with_context(&context).into_result()
}

trait JumpResultExt {
    fn into_result(self) -> Result<(), String>;
}

impl JumpResultExt for crate::terminal::jump::JumpResult {
    fn into_result(self) -> Result<(), String> {
        match self {
            crate::terminal::jump::JumpResult::Success => Ok(()),
            crate::terminal::jump::JumpResult::SessionNotFound => {
                Err("Session not found".to_string())
            }
            crate::terminal::jump::JumpResult::TerminalNotFound => {
                Err("Terminal not found".to_string())
            }
            crate::terminal::jump::JumpResult::Failed(msg) => Err(msg),
        }
    }
}
