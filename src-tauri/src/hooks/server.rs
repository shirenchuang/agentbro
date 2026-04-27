// HookServer — Async TCP + Unix socket server for agent hook events
// Accepts JSON-line protocol from hook scripts, routes to adapters,
// and keeps connections alive for permission request/response flow.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::{Mutex, oneshot};

use super::session_store::{
    PendingPermission, PendingQuestion, SessionPhase, SessionStore,
};
use crate::agents::{AgentEvent, AgentAdapter};
use crate::hooks::conversation_parser::{discover_session_file, extract_session_title};
use crate::sound::{SoundEngine, SoundEvent};
use crate::terminal::suppression;

/// Socket path for Unix domain socket
pub const UNIX_SOCKET_PATH: &str = "/tmp/agent-island.sock";

/// TCP port for hook connections
pub const TCP_PORT: u16 = 17892;

/// Permission response sent back to hook script
#[derive(Debug, Clone, serde::Serialize)]
pub struct PermissionResponse {
    pub decision: String,
    pub reason: Option<String>,
}

/// A pending permission waiting for UI response
pub(crate) struct PendingPermissionEntry {
    pub(crate) tx: oneshot::Sender<PermissionResponse>,
}

/// The HookServer manages incoming connections from agent hook scripts
pub struct HookServer {
    /// Pending permission requests: session_id -> sender
    pending_permissions: Arc<Mutex<HashMap<String, PendingPermissionEntry>>>,
    /// Session store reference
    session_store: Arc<SessionStore>,
    /// Registered adapters (shared with AppState to avoid duplication)
    adapters: Arc<Vec<Arc<dyn AgentAdapter>>>,
    /// Optional sound engine for playing event sounds
    sound_engine: Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
    /// App handle for sending notifications
    app_handle: Arc<std::sync::Mutex<Option<tauri::AppHandle>>>,
}

impl HookServer {
    pub fn new(session_store: Arc<SessionStore>, adapters: Arc<Vec<Arc<dyn AgentAdapter>>>) -> Self {
        Self {
            pending_permissions: Arc::new(Mutex::new(HashMap::new())),
            session_store,
            adapters,
            sound_engine: Arc::new(std::sync::Mutex::new(None)),
            app_handle: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Set the app handle (called after construction)
    pub fn set_app_handle(&self, handle: tauri::AppHandle) {
        if let Ok(mut h) = self.app_handle.lock() {
            *h = Some(handle);
        }
    }

    /// Set the sound engine (called after construction, once the engine is ready)
    pub fn set_sound_engine(&self, engine: Arc<SoundEngine>) {
        if let Ok(mut se) = self.sound_engine.lock() {
            *se = Some(engine);
        }
    }

    /// Play a sound event if the engine is available
    fn play_sound(sound_engine: &Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>, event: SoundEvent) {
        if let Ok(guard) = sound_engine.lock() {
            if let Some(ref engine) = *guard {
                engine.play(event);
            }
        }
    }

    /// Play a sound event, skipping if the session looks like a probe
    fn play_sound_for_session(
        sound_engine: &Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
        store: &SessionStore,
        session_id: &str,
        event: SoundEvent,
    ) {
        if let Ok(guard) = sound_engine.lock() {
            if let Some(ref engine) = *guard {
                if engine.is_probe_filter_enabled() {
                    if let Some(session) = store.get_session(session_id) {
                        if Self::is_probe_session(&session.project) {
                            return;
                        }
                    }
                }
                engine.play(event);
            }
        }
    }

    /// Check if a session project name looks like an automated probe
    fn is_probe_session(project: &str) -> bool {
        let lower = project.to_lowercase();
        ["health-check", "health_check", "probe", "ping", "codexbar", "healthcheck"]
            .iter()
            .any(|p| lower.contains(p))
    }

    /// Start listening on both Unix socket and TCP
    pub async fn start(&self) -> anyhow::Result<()> {
        let pending = self.pending_permissions.clone();
        let store = self.session_store.clone();
        let adapters = self.adapters.clone();
        let sound = self.sound_engine.clone();
        let app = self.app_handle.clone();

        // Start Unix socket listener
        let pending_unix = pending.clone();
        let store_unix = store.clone();
        let adapters_unix = adapters.clone();
        let sound_unix = sound.clone();
        let app_unix = app.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::listen_unix(pending_unix, store_unix, adapters_unix, sound_unix, app_unix).await {
                log::error!("Unix socket listener error: {}", e);
            }
        });

        // Start TCP listener
        let pending_tcp = pending.clone();
        let store_tcp = store.clone();
        let adapters_tcp = adapters.clone();
        let sound_tcp = sound.clone();
        let app_tcp = app.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::listen_tcp(pending_tcp, store_tcp, adapters_tcp, sound_tcp, app_tcp).await {
                log::error!("TCP listener error: {}", e);
            }
        });

        log::info!("HookServer started on {} and 127.0.0.1:{}", UNIX_SOCKET_PATH, TCP_PORT);
        Ok(())
    }

    /// Listen on Unix domain socket
    async fn listen_unix(
        pending: Arc<Mutex<HashMap<String, PendingPermissionEntry>>>,
        store: Arc<SessionStore>,
        adapters: Arc<Vec<Arc<dyn AgentAdapter>>>,
        sound: Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
        app: Arc<std::sync::Mutex<Option<tauri::AppHandle>>>,
    ) -> anyhow::Result<()> {
        // Remove stale socket file
        let socket_path = Path::new(UNIX_SOCKET_PATH);
        if socket_path.exists() {
            std::fs::remove_file(socket_path)?;
        }

        let listener = UnixListener::bind(UNIX_SOCKET_PATH)?;

        // Set socket permissions to owner-only
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(UNIX_SOCKET_PATH, std::fs::Permissions::from_mode(0o600))?;
        }

        log::info!("Listening on Unix socket: {}", UNIX_SOCKET_PATH);

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let pending = pending.clone();
                    let store = store.clone();
                    let adapters = adapters.clone();
                    let sound = sound.clone();
                    let app = app.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, pending, store, adapters, sound, app).await {
                            log::debug!("Unix connection handler error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    log::error!("Unix accept error: {}", e);
                }
            }
        }
    }

    /// Listen on TCP socket
    async fn listen_tcp(
        pending: Arc<Mutex<HashMap<String, PendingPermissionEntry>>>,
        store: Arc<SessionStore>,
        adapters: Arc<Vec<Arc<dyn AgentAdapter>>>,
        sound: Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
        app: Arc<std::sync::Mutex<Option<tauri::AppHandle>>>,
    ) -> anyhow::Result<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", TCP_PORT)).await?;
        log::info!("Listening on TCP: 127.0.0.1:{}", TCP_PORT);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    log::debug!("TCP connection from: {}", addr);
                    let pending = pending.clone();
                    let store = store.clone();
                    let adapters = adapters.clone();
                    let sound = sound.clone();
                    let app = app.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, pending, store, adapters, sound, app).await {
                            log::debug!("TCP connection handler error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    log::error!("TCP accept error: {}", e);
                }
            }
        }
    }

    /// Handle a single connection (works with both Unix and TCP streams via AsyncRead+AsyncWrite)
    async fn handle_connection<S>(
        stream: S,
        pending: Arc<Mutex<HashMap<String, PendingPermissionEntry>>>,
        store: Arc<SessionStore>,
        adapters: Arc<Vec<Arc<dyn AgentAdapter>>>,
        sound: Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
        app: Arc<std::sync::Mutex<Option<tauri::AppHandle>>>,
    ) -> anyhow::Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();

        // Read a single JSON line
        let bytes_read = buf_reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            // Also try reading without newline delimiter (Python script may send without trailing newline)
            return Ok(());
        }

        let line = line.trim();
        if line.is_empty() {
            return Ok(());
        }

        // Parse the raw JSON
        let raw: serde_json::Value = serde_json::from_str(line)?;

        log::debug!("Received hook event: {}", raw.get("event").and_then(|v| v.as_str()).unwrap_or("unknown"));

        // Try to find a matching adapter and parse the event
        let event = Self::parse_with_adapters(&adapters, &raw);

        match event {
            Some(AgentEvent::PermissionRequest { ref session_id, ref tool_name, ref diff, ref options }) => {
                // Check smart suppression: is the agent's terminal focused?
                let is_suppressed = Self::check_suppression(&store, session_id);

                // Only play sound if not suppressed
                if !is_suppressed {
                    Self::play_sound(&sound, SoundEvent::NeedsApproval);
                }

                // Set pending permission on session
                store.set_pending_permission(
                    session_id,
                    Some(PendingPermission {
                        tool_name: tool_name.clone(),
                        tool_input: raw.get("tool_input")
                            .map(|v| v.to_string())
                            .unwrap_or_default(),
                        diff: diff.clone(),
                        options: options.clone(),
                    }),
                );

                // Re-emit with suppression flag so frontend knows not to auto-expand
                if is_suppressed {
                    store.emit_update_suppressed(true);
                    if let Ok(guard) = app.lock() {
                        if let Some(ref handle) = *guard {
                            crate::platform::notifications::send_permission_notification(handle, tool_name);
                        }
                    }
                }

                // Create a oneshot channel for the permission response
                let (tx, rx) = oneshot::channel();
                {
                    let mut pending_map = pending.lock().await;
                    pending_map.insert(session_id.clone(), PendingPermissionEntry {
                        tx,
                    });
                }

                // Wait for the UI to respond (with 5 minute timeout)
                let response = tokio::time::timeout(
                    std::time::Duration::from_secs(300),
                    rx,
                ).await;

                match response {
                    Ok(Ok(resp)) => {
                        // Send response back to the hook script
                        let json = serde_json::to_string(&resp)?;
                        writer.write_all(json.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;

                        // Play confirmation sound
                        Self::play_sound(&sound, SoundEvent::TaskConfirmation);

                        // Clear pending permission
                        store.set_pending_permission(session_id, None);
                        store.update_phase(session_id, SessionPhase::Processing);
                    }
                    _ => {
                        // Timeout or channel closed — let Claude Code handle it normally
                        let mut pending_map = pending.lock().await;
                        pending_map.remove(session_id);
                        store.set_pending_permission(session_id, None);
                        log::warn!("Permission request timed out for session {}", session_id);
                    }
                }
            }
            Some(AgentEvent::AskQuestion { ref session_id, ref question, ref options }) => {
                // Check smart suppression: is the agent's terminal focused?
                let is_suppressed = Self::check_suppression(&store, session_id);

                // Only play sound if not suppressed
                if !is_suppressed {
                    Self::play_sound(&sound, SoundEvent::NeedsApproval);
                }

                store.set_pending_question(
                    session_id,
                    Some(PendingQuestion {
                        question: question.clone(),
                        options: options.clone(),
                    }),
                );

                // Re-emit with suppression flag so frontend knows not to auto-expand
                if is_suppressed {
                    store.emit_update_suppressed(true);
                    if let Ok(guard) = app.lock() {
                        if let Some(ref handle) = *guard {
                            crate::platform::notifications::send_question_notification(handle, question);
                        }
                    }
                }
            }
            Some(ref agent_event) => {
                // Process non-permission events (with sound)
                Self::process_event(&store, agent_event, &raw, &sound);
            }
            None => {
                // No adapter matched — try generic processing (with sound)
                Self::process_raw_event(&store, &raw, &sound);
            }
        }

        Ok(())
    }

    /// Try to parse with registered adapters
    fn parse_with_adapters(
        adapters: &[Arc<dyn AgentAdapter>],
        raw: &serde_json::Value,
    ) -> Option<AgentEvent> {
        // Default to claude-code adapter for now
        for adapter in adapters.iter() {
            if let Ok(event) = adapter.parse_event(raw) {
                return Some(event);
            }
        }
        None
    }

    /// Process a parsed agent event and update session store
    fn process_event(
        store: &SessionStore,
        event: &AgentEvent,
        _raw: &serde_json::Value,
        sound: &Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
    ) {
        match event {
            AgentEvent::SessionStart { session_id, project, cwd, terminal, agent_type } => {
                store.get_or_create_session(session_id, agent_type, project, cwd, terminal);
                store.update_phase(session_id, SessionPhase::Idle);

                // Try to extract session title from the JSONL conversation file
                if let Some(file_path) = discover_session_file(session_id, cwd) {
                    if let Some(title) = extract_session_title(&file_path) {
                        store.update_session(session_id, |s| {
                            s.session_title = Some(title);
                        });
                    }
                }

                Self::play_sound_for_session(sound, store, session_id, SoundEvent::SessionStart);
            }
            AgentEvent::SessionEnd { session_id } => {
                store.remove_session(session_id);
            }
            AgentEvent::Processing { session_id, description } => {
                // If this is a UserPromptSubmit and session has no title yet,
                // try to extract the user prompt as session title
                let event_name = _raw.get("event").and_then(|v| v.as_str()).unwrap_or("");
                if event_name == "UserPromptSubmit" {
                    let has_title = store.get_session(session_id)
                        .map(|s| s.session_title.is_some())
                        .unwrap_or(false);
                    if !has_title {
                        // Try to get prompt from the raw event data
                        let prompt = _raw.get("prompt")
                            .or_else(|| _raw.get("user_prompt"))
                            .and_then(|v| v.as_str())
                            .map(|s| {
                                let first_line = s.lines().next().unwrap_or(s).trim();
                                if first_line.len() > 80 {
                                    format!("{}...", &first_line[..77])
                                } else {
                                    first_line.to_string()
                                }
                            });
                        if let Some(title) = prompt {
                            if !title.is_empty() {
                                store.update_session(session_id, |s| {
                                    s.session_title = Some(title);
                                });
                            }
                        }
                    }
                }
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Processing;
                    s.description = Some(description.clone());
                });
            }
            AgentEvent::ToolUse { session_id, tool_name, tool_target, status, .. } => {
                // Extract tool_use_id from raw event for tool tracking
                let tool_use_id = _raw.get("tool_use_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                store.update_session(session_id, |s| {
                    s.last_tool_name = Some(tool_name.clone());
                    s.last_tool_target = tool_target.clone();
                    s.last_tool_status = Some(status.clone());
                    s.phase = SessionPhase::Processing;
                });

                // Track tool lifecycle
                if !tool_use_id.is_empty() {
                    match status.as_str() {
                        "running" => {
                            store.start_tool(session_id, &tool_use_id, tool_name);
                        }
                        "success" => {
                            store.complete_tool(session_id, &tool_use_id, true, None);
                        }
                        "error" => {
                            let error_msg = _raw.get("tool_error")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            store.complete_tool(session_id, &tool_use_id, false, error_msg);
                        }
                        _ => {}
                    }
                }
            }
            AgentEvent::TaskComplete { session_id, .. } => {
                store.update_phase(session_id, SessionPhase::Done);
                Self::play_sound_for_session(sound, store, session_id, SoundEvent::TaskComplete);
            }
            AgentEvent::Error { session_id, .. } => {
                store.update_phase(session_id, SessionPhase::Error);
                Self::play_sound_for_session(sound, store, session_id, SoundEvent::TaskError);
            }
            AgentEvent::Interrupt { session_id } => {
                store.update_phase(session_id, SessionPhase::Interrupted);
            }
            AgentEvent::TokenUsage { session_id, input, output, cache_read, cache_create } => {
                store.add_tokens(session_id, *input, *output, *cache_read, *cache_create);
            }
            AgentEvent::SubagentStart { session_id, agent_id, description } => {
                log::info!("Subagent started: {} ({}) for session {}", agent_id, description, session_id);
                store.add_subagent(session_id, agent_id, description);
            }
            AgentEvent::SubagentStop { session_id, agent_id, status } => {
                log::info!("Subagent stopped: {} (status={}) for session {}", agent_id, status, session_id);
                store.stop_subagent(session_id, agent_id, status);
            }
            // PermissionRequest and AskQuestion are handled in handle_connection
            _ => {}
        }
    }

    /// Process raw event when no adapter matched (generic fallback)
    fn process_raw_event(
        store: &SessionStore,
        raw: &serde_json::Value,
        sound: &Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
    ) {
        let session_id = match raw.get("session_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return,
        };
        let _event_name = raw.get("event").and_then(|v| v.as_str()).unwrap_or("");
        let status = raw.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let cwd = raw.get("cwd").and_then(|v| v.as_str()).unwrap_or("");

        // Extract project name from cwd
        let project = cwd.rsplit('/').next().unwrap_or(cwd);

        // Ensure session exists
        store.get_or_create_session(session_id, "claude-code", project, cwd, "");

        // Extract session title from UserPromptSubmit if no title yet
        if _event_name == "UserPromptSubmit" {
            let has_title = store.get_session(session_id)
                .map(|s| s.session_title.is_some())
                .unwrap_or(false);
            if !has_title {
                let prompt = raw.get("prompt")
                    .or_else(|| raw.get("user_prompt"))
                    .and_then(|v| v.as_str())
                    .map(|s| {
                        let first_line = s.lines().next().unwrap_or(s).trim();
                        if first_line.len() > 80 {
                            format!("{}...", &first_line[..77])
                        } else {
                            first_line.to_string()
                        }
                    });
                if let Some(title) = prompt {
                    if !title.is_empty() {
                        store.update_session(session_id, |s| {
                            s.session_title = Some(title);
                        });
                    }
                }
            }
        }

        // Update pid/tty if present
        if let Some(pid) = raw.get("pid").and_then(|v| v.as_u64()) {
            store.update_session(session_id, |s| {
                s.pid = Some(pid as u32);
            });
        }
        if let Some(tty) = raw.get("tty").and_then(|v| v.as_str()) {
            store.update_session(session_id, |s| {
                s.tty = Some(tty.to_string());
            });
        }

        // Map status to phase
        let phase = match status {
            "processing" | "running_tool" | "starting" => SessionPhase::Processing,
            "waiting_for_input" => SessionPhase::Idle,
            "waiting_for_approval" => {
                Self::play_sound(sound, SoundEvent::NeedsApproval);
                SessionPhase::WaitingApproval
            }
            "compacting" => {
                Self::play_sound(sound, SoundEvent::ContextLimit);
                SessionPhase::Compacting
            }
            "ended" => {
                store.remove_session(session_id);
                return;
            }
            _ => SessionPhase::Processing,
        };

        // Update tool info if present
        if let Some(tool) = raw.get("tool").and_then(|v| v.as_str()) {
            store.update_session(session_id, |s| {
                s.last_tool_name = Some(tool.to_string());
                s.last_tool_target = None; // Tool target not available in this event format
                s.last_tool_status = Some(status.to_string());
            });
        }

        store.update_phase(session_id, phase);
    }

    /// Check if a session's terminal is focused (smart suppression).
    /// Returns true if the user is already looking at the terminal, meaning
    /// the panel should NOT auto-expand.
    fn check_suppression(store: &SessionStore, session_id: &str) -> bool {
        let pid = store
            .get_session(session_id)
            .and_then(|s| s.pid)
            .unwrap_or(0);
        if pid == 0 {
            return false;
        }
        suppression::is_terminal_focused(pid)
    }

    /// Respond to a pending permission request from the UI
    pub async fn respond_permission(
        &self,
        session_id: &str,
        allowed: bool,
    ) -> anyhow::Result<()> {
        let mut pending_map = self.pending_permissions.lock().await;
        if let Some(entry) = pending_map.remove(session_id) {
            let response = PermissionResponse {
                decision: if allowed { "allow".to_string() } else { "deny".to_string() },
                reason: if allowed { None } else { Some("Denied by user via Agent Island".to_string()) },
            };
            // Ignore send error — receiver may have already dropped
            let _ = entry.tx.send(response);
            Ok(())
        } else {
            anyhow::bail!("No pending permission for session {}", session_id)
        }
    }

    pub async fn respond_auto_approve(
        &self,
        session_id: &str,
    ) -> anyhow::Result<()> {
        let mut pending_map = self.pending_permissions.lock().await;
        if let Some(entry) = pending_map.remove(session_id) {
            let response = PermissionResponse {
                decision: "auto".to_string(),
                reason: None,
            };
            let _ = entry.tx.send(response);
            Ok(())
        } else {
            anyhow::bail!("No pending permission for session {}", session_id)
        }
    }

}

impl Drop for HookServer {
    fn drop(&mut self) {
        // Clean up Unix socket file
        let _ = std::fs::remove_file(UNIX_SOCKET_PATH);
    }
}
