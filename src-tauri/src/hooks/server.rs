// HookServer — Async TCP + Unix socket server for agent hook events
// Accepts JSON-line protocol from hook scripts, routes to adapters,
// and keeps connections alive for permission request/response flow.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UnixListener, UnixStream};
use tokio::sync::{oneshot, Mutex};

use super::session_store::{
    ContextWindowInfo, PendingPermission, PendingPlan, PendingQuestion,
    QuestionItem as PendingQuestionItem, QuestionOption as PendingQuestionOption, RateLimitInfo,
    SessionPhase, SessionStore, SubagentInfo, SubagentStopUpdate,
};
use crate::agents::{AgentAdapter, AgentEvent};
use crate::hook_endpoint;
use crate::hooks::conversation_parser::{
    discover_codex_session_file, discover_session_file, extract_cache_ttl_info,
    extract_latest_assistant_text, extract_session_title, extract_subagents_from_transcript,
    TranscriptSubagentInfo,
};
use crate::sound::{SoundEngine, SoundEvent};
use crate::terminal::suppression;

const RAW_EVENT_BUFFER_PER_SESSION: usize = 200;
const SESSION_END_CLEANUP_SECS: u64 = 5;
const DONE_SESSION_HISTORY_CLEANUP_SECS: u64 = 120 * 60;
const DEFAULT_INTERACTION_RESPONSE_TIMEOUT_SECS: u64 = 300;
const HUMAN_INTERACTION_RESPONSE_TIMEOUT_SECS: u64 = 21_600;

/// Raw hook event snapshot retained for Agent monitor diagnostics.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawHookEvent {
    pub seq: u64,
    pub timestamp_ms: u64,
    pub session_id: String,
    pub agent: Option<String>,
    pub event_name: String,
    pub raw: serde_json::Value,
}

struct SubagentToolMetadata {
    name: Option<String>,
    description: String,
    agent_type: Option<String>,
}

struct RawHookEventStore {
    next_seq: u64,
    by_session: HashMap<String, VecDeque<RawHookEvent>>,
}

impl RawHookEventStore {
    fn new() -> Self {
        Self {
            next_seq: 0,
            by_session: HashMap::new(),
        }
    }

    fn push(&mut self, mut event: RawHookEvent) {
        event.seq = self.next_seq;
        self.next_seq += 1;

        let events = self
            .by_session
            .entry(event.session_id.clone())
            .or_insert_with(|| VecDeque::with_capacity(RAW_EVENT_BUFFER_PER_SESSION));
        if events.len() == RAW_EVENT_BUFFER_PER_SESSION {
            events.pop_front();
        }
        events.push_back(event);
    }

    fn session_events(&self, session_id: &str) -> Vec<RawHookEvent> {
        self.by_session
            .get(session_id)
            .map(|events| events.iter().cloned().collect())
            .unwrap_or_default()
    }
}

/// Permission response sent back to hook script
#[derive(Debug, Clone, serde::Serialize)]
pub struct PermissionResponse {
    pub decision: String,
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always: Option<bool>,
}

/// A pending permission waiting for UI response
pub(crate) struct PendingPermissionEntry {
    pub(crate) tx: oneshot::Sender<PermissionResponse>,
}

/// Question response sent back to hook script
#[derive(Debug, Clone, serde::Serialize)]
pub struct QuestionResponse {
    pub answer: String,
}

/// A pending question waiting for UI response
pub(crate) struct PendingQuestionEntry {
    pub(crate) tx: oneshot::Sender<QuestionResponse>,
}

/// Plan response sent back to hook script
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanResponse {
    pub mode: String,
    pub message: Option<String>,
}

/// A pending plan waiting for UI response
pub(crate) struct PendingPlanEntry {
    pub(crate) tx: oneshot::Sender<PlanResponse>,
}

/// The HookServer manages incoming connections from agent hook scripts
pub struct HookServer {
    /// Pending permission requests: session_id -> sender
    pending_permissions: Arc<Mutex<HashMap<String, PendingPermissionEntry>>>,
    /// Pending question requests: session_id -> sender
    pending_questions: Arc<Mutex<HashMap<String, PendingQuestionEntry>>>,
    /// Pending plan approvals: session_id -> sender
    pending_plans: Arc<Mutex<HashMap<String, PendingPlanEntry>>>,
    /// Session store reference
    session_store: Arc<SessionStore>,
    /// Registered adapters (shared with AppState to avoid duplication)
    adapters: Arc<Vec<Arc<dyn AgentAdapter>>>,
    /// Optional sound engine for playing event sounds
    sound_engine: Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
    /// App handle for sending notifications
    app_handle: Arc<std::sync::Mutex<Option<tauri::AppHandle>>>,
    /// Recent raw hook events grouped by session for monitor diagnostics.
    raw_events: Arc<std::sync::Mutex<RawHookEventStore>>,
    /// IPC endpoint owned by this server instance.
    endpoint: hook_endpoint::HookEndpoint,
    socket_owned: Arc<AtomicBool>,
}

#[derive(Clone)]
struct HookConnectionContext {
    pending: Arc<Mutex<HashMap<String, PendingPermissionEntry>>>,
    pending_q: Arc<Mutex<HashMap<String, PendingQuestionEntry>>>,
    pending_plan: Arc<Mutex<HashMap<String, PendingPlanEntry>>>,
    store: Arc<SessionStore>,
    adapters: Arc<Vec<Arc<dyn AgentAdapter>>>,
    sound: Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
    app: Arc<std::sync::Mutex<Option<tauri::AppHandle>>>,
    raw_events: Arc<std::sync::Mutex<RawHookEventStore>>,
}

impl HookServer {
    pub fn new(
        session_store: Arc<SessionStore>,
        adapters: Arc<Vec<Arc<dyn AgentAdapter>>>,
    ) -> Self {
        Self {
            pending_permissions: Arc::new(Mutex::new(HashMap::new())),
            pending_questions: Arc::new(Mutex::new(HashMap::new())),
            pending_plans: Arc::new(Mutex::new(HashMap::new())),
            session_store,
            adapters,
            sound_engine: Arc::new(std::sync::Mutex::new(None)),
            app_handle: Arc::new(std::sync::Mutex::new(None)),
            raw_events: Arc::new(std::sync::Mutex::new(RawHookEventStore::new())),
            endpoint: hook_endpoint::current(),
            socket_owned: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn backoff_after_accept_error(protocol: &str, error: &std::io::Error) {
        if error.raw_os_error() == Some(24) {
            log::error!(
                "{} accept error: {}. Backing off before retrying.",
                protocol,
                error
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
        } else {
            log::error!("{} accept error: {}", protocol, error);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Set the app handle (called after construction)
    pub fn set_app_handle(&self, handle: tauri::AppHandle) {
        if let Ok(mut h) = self.app_handle.lock() {
            *h = Some(handle);
        }
    }

    fn interaction_response_timeout(raw: &serde_json::Value) -> Duration {
        let agent = raw
            .get("agent")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let seconds = if matches!(agent, "codex" | "openai.codex" | "claude-code" | "claude") {
            HUMAN_INTERACTION_RESPONSE_TIMEOUT_SECS
        } else {
            DEFAULT_INTERACTION_RESPONSE_TIMEOUT_SECS
        };
        Duration::from_secs(seconds)
    }

    /// Set the sound engine (called after construction, once the engine is ready)
    pub fn set_sound_engine(&self, engine: Arc<SoundEngine>) {
        if let Ok(mut se) = self.sound_engine.lock() {
            *se = Some(engine);
        }
    }

    /// Play a sound event if the engine is available
    fn play_sound(
        sound_engine: &Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
        event: SoundEvent,
    ) {
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

    /// Detect if Cursor has YOLO mode enabled by reading its settings.json
    fn detect_cursor_yolo_mode() -> bool {
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return false,
        };
        let settings_path = format!(
            "{}/Library/Application Support/Cursor/User/settings.json",
            home
        );
        let content = match std::fs::read_to_string(&settings_path) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return false,
        };
        json.get("cursor.general.yoloMode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Check if a session project name looks like an automated probe
    fn is_probe_session(project: &str) -> bool {
        let lower = project.to_lowercase();
        [
            "health-check",
            "health_check",
            "probe",
            "ping",
            "healthcheck",
        ]
        .iter()
        .any(|p| lower.contains(p))
    }

    /// Start listening on both Unix socket and TCP
    pub async fn start(&self) -> anyhow::Result<()> {
        let endpoint = self.endpoint.clone();
        let tcp_addr = endpoint.tcp_addr();
        let tcp_listener = TcpListener::bind(&tcp_addr).await?;
        let unix_listener = Self::bind_unix(&endpoint.socket_path).await?;
        self.socket_owned.store(true, Ordering::SeqCst);

        let context = HookConnectionContext {
            pending: self.pending_permissions.clone(),
            pending_q: self.pending_questions.clone(),
            pending_plan: self.pending_plans.clone(),
            store: self.session_store.clone(),
            adapters: self.adapters.clone(),
            sound: self.sound_engine.clone(),
            app: self.app_handle.clone(),
            raw_events: self.raw_events.clone(),
        };

        // Start Unix socket listener
        let unix_context = context.clone();
        tokio::spawn(async move { Self::accept_unix(unix_listener, unix_context).await });

        log::info!("Listening on TCP: {}", tcp_addr);
        let tcp_context = context.clone();
        tokio::spawn(async move { Self::accept_tcp(tcp_listener, tcp_context).await });

        log::info!(
            "HookServer started on {} and {}",
            endpoint.socket_path,
            tcp_addr
        );
        Ok(())
    }

    async fn bind_unix(socket_path: &str) -> anyhow::Result<UnixListener> {
        let socket_path = Path::new(socket_path);
        if socket_path.exists() {
            match UnixStream::connect(socket_path).await {
                Ok(_) => anyhow::bail!("Unix socket already in use: {}", socket_path.display()),
                Err(_) => std::fs::remove_file(socket_path)?,
            }
        }

        let listener = UnixListener::bind(socket_path)?;

        // Set socket permissions to owner-only
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
        }

        log::info!("Listening on Unix socket: {}", socket_path.display());
        Ok(listener)
    }

    async fn accept_unix(listener: UnixListener, context: HookConnectionContext) {
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let context = context.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, context).await {
                            log::debug!("Unix connection handler error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    Self::backoff_after_accept_error("Unix", &e).await;
                }
            }
        }
    }

    /// Accept connections on an already-bound TCP listener
    async fn accept_tcp(listener: TcpListener, context: HookConnectionContext) {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    log::debug!("TCP connection from: {}", addr);
                    let context = context.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, context).await {
                            log::debug!("TCP connection handler error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    Self::backoff_after_accept_error("TCP", &e).await;
                }
            }
        }
    }

    /// Handle a single connection (works with both Unix and TCP streams via AsyncRead+AsyncWrite)
    async fn handle_connection<S>(stream: S, context: HookConnectionContext) -> anyhow::Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let pending = context.pending;
        let pending_q = context.pending_q;
        let pending_plan = context.pending_plan;
        let store = context.store;
        let adapters = context.adapters;
        let sound = context.sound;
        let app = context.app;
        let raw_events = context.raw_events;
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

        log::debug!(
            "Received hook event: {}",
            raw.get("event")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
        );

        // Try to find a matching adapter and parse the event
        let event = Self::parse_with_adapters(&adapters, &raw);
        Self::record_raw_event(&raw_events, &raw, event.as_ref());
        if let Some(ref agent_event) = event {
            Self::ensure_session_for_event(&store, agent_event, &raw);
        }

        match event {
            Some(AgentEvent::PermissionRequest {
                ref session_id,
                ref tool_name,
                ref diff,
                ref options,
            }) => {
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
                        tool_input: raw
                            .get("tool_input")
                            .map(|v| v.to_string())
                            .unwrap_or_default(),
                        diff: diff.clone(),
                        options: options.clone(),
                    }),
                );
                Self::update_session_metadata_from_raw(&store, &raw);

                // Re-emit with suppression flag so frontend knows not to auto-expand
                if is_suppressed {
                    store.emit_update_suppressed(true);
                    if let Ok(guard) = app.lock() {
                        if let Some(ref handle) = *guard {
                            crate::platform::notifications::send_permission_notification(
                                handle, tool_name,
                            );
                        }
                    }
                }

                // Create a oneshot channel for the permission response
                let (tx, rx) = oneshot::channel();
                {
                    let mut pending_map = pending.lock().await;
                    pending_map.insert(session_id.clone(), PendingPermissionEntry { tx });
                }

                // Wait for the UI to respond. Codex uses a longer hook timeout for AgentBro
                // approvals; other agents keep the existing five-minute fallback window.
                let response =
                    tokio::time::timeout(Self::interaction_response_timeout(&raw), rx).await;

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
            Some(AgentEvent::AskQuestion {
                ref session_id,
                ref question,
                ref options,
                ref descriptions,
                ref header,
                multi_select,
                ref questions,
            }) => {
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
                        descriptions: descriptions.clone(),
                        header: header.clone(),
                        multi_select,
                        questions: questions
                            .iter()
                            .map(|q| PendingQuestionItem {
                                question: q.question.clone(),
                                header: q.header.clone(),
                                options: q
                                    .options
                                    .iter()
                                    .map(|opt| PendingQuestionOption {
                                        label: opt.label.clone(),
                                        description: opt.description.clone(),
                                    })
                                    .collect(),
                                multi_select: q.multi_select,
                            })
                            .collect(),
                    }),
                );
                Self::update_session_metadata_from_raw(&store, &raw);

                // Re-emit with suppression flag so frontend knows not to auto-expand
                if is_suppressed {
                    store.emit_update_suppressed(true);
                    if let Ok(guard) = app.lock() {
                        if let Some(ref handle) = *guard {
                            crate::platform::notifications::send_question_notification(
                                handle, question,
                            );
                        }
                    }
                }

                // Create a oneshot channel for the question response
                let (tx, rx) = oneshot::channel();
                {
                    let mut pending_map = pending_q.lock().await;
                    pending_map.insert(session_id.clone(), PendingQuestionEntry { tx });
                }

                let response =
                    tokio::time::timeout(Self::interaction_response_timeout(&raw), rx).await;

                match response {
                    Ok(Ok(resp)) => {
                        let json = serde_json::to_string(&resp)?;
                        writer.write_all(json.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;

                        Self::play_sound(&sound, SoundEvent::TaskConfirmation);

                        store.set_pending_question(session_id, None);
                        store.update_phase(session_id, SessionPhase::Processing);
                    }
                    _ => {
                        let mut pending_map = pending_q.lock().await;
                        pending_map.remove(session_id);
                        store.set_pending_question(session_id, None);
                        log::warn!("Question request timed out for session {}", session_id);
                    }
                }
            }
            Some(AgentEvent::PlanApproval {
                ref session_id,
                ref title,
                ref content,
                ref permissions,
            }) => {
                let is_suppressed = Self::check_suppression(&store, session_id);

                if !is_suppressed {
                    Self::play_sound(&sound, SoundEvent::PlanApproval);
                }

                store.set_pending_plan(
                    session_id,
                    Some(PendingPlan {
                        title: title.clone(),
                        content: content.clone(),
                        permissions: permissions.clone(),
                    }),
                );
                Self::update_session_metadata_from_raw(&store, &raw);

                if is_suppressed {
                    store.emit_update_suppressed(true);
                }

                let (tx, rx) = oneshot::channel();
                {
                    let mut pending_map = pending_plan.lock().await;
                    pending_map.insert(session_id.clone(), PendingPlanEntry { tx });
                }

                let response =
                    tokio::time::timeout(Self::interaction_response_timeout(&raw), rx).await;

                match response {
                    Ok(Ok(resp)) => {
                        let json = serde_json::to_string(&resp)?;
                        writer.write_all(json.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;

                        Self::play_sound(&sound, SoundEvent::TaskConfirmation);
                        store.set_pending_plan(session_id, None);
                        store.update_phase(session_id, SessionPhase::Processing);
                    }
                    _ => {
                        let mut pending_map = pending_plan.lock().await;
                        pending_map.remove(session_id);
                        store.set_pending_plan(session_id, None);
                        log::warn!("Plan approval timed out for session {}", session_id);
                    }
                }
            }
            Some(ref agent_event) => {
                // Process non-permission events (with sound)
                Self::process_event(&store, agent_event, &raw, &sound, &app);
            }
            None => {
                // No adapter matched — try generic processing (with sound)
                Self::process_raw_event(&store, &raw, &sound);
            }
        }

        Ok(())
    }

    /// Return recent raw hook events for a session, oldest first.
    pub fn raw_events_for_session(&self, session_id: &str) -> Vec<RawHookEvent> {
        self.raw_events
            .lock()
            .map(|events| events.session_events(session_id))
            .unwrap_or_default()
    }

    fn record_raw_event(
        raw_events: &Arc<std::sync::Mutex<RawHookEventStore>>,
        raw: &serde_json::Value,
        event: Option<&AgentEvent>,
    ) {
        let Some(session_id) = Self::session_id_from_raw_or_event(raw, event) else {
            return;
        };
        if session_id.trim().is_empty() || session_id == "unknown" {
            return;
        }

        let agent = raw
            .get("agent")
            .and_then(|value| value.as_str())
            .map(|value| canonical_agent_id(value).to_string());
        let event_name = raw
            .get("event")
            .or_else(|| raw.get("hook_event_name"))
            .or_else(|| raw.get("hookEventName"))
            .or_else(|| raw.get("type"))
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| Self::event_kind_label(event))
            .to_string();

        let hook_event = RawHookEvent {
            seq: 0,
            timestamp_ms: current_time_ms(),
            session_id,
            agent,
            event_name,
            raw: raw.clone(),
        };

        if let Ok(mut events) = raw_events.lock() {
            events.push(hook_event);
        }
    }

    fn session_id_from_raw_or_event(
        raw: &serde_json::Value,
        event: Option<&AgentEvent>,
    ) -> Option<String> {
        raw.get("session_id")
            .or_else(|| raw.get("sessionId"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
            .or_else(|| Self::session_id_from_event(event).map(ToString::to_string))
    }

    fn session_id_from_event(event: Option<&AgentEvent>) -> Option<&str> {
        let event = event?;
        Some(match event {
            AgentEvent::SessionStart { session_id, .. }
            | AgentEvent::SessionEnd { session_id }
            | AgentEvent::Processing { session_id, .. }
            | AgentEvent::ToolUse { session_id, .. }
            | AgentEvent::PermissionRequest { session_id, .. }
            | AgentEvent::AskQuestion { session_id, .. }
            | AgentEvent::PlanApproval { session_id, .. }
            | AgentEvent::TaskComplete { session_id, .. }
            | AgentEvent::AssistantResponseComplete { session_id, .. }
            | AgentEvent::Error { session_id, .. }
            | AgentEvent::Interrupt { session_id }
            | AgentEvent::TokenUsage { session_id, .. }
            | AgentEvent::RateLimitUpdate { session_id, .. }
            | AgentEvent::Notification { session_id, .. }
            | AgentEvent::SubagentStart { session_id, .. }
            | AgentEvent::SubagentStop { session_id, .. }
            | AgentEvent::ShellExecutionStart { session_id, .. }
            | AgentEvent::ShellExecutionEnd { session_id, .. }
            | AgentEvent::MCPExecutionStart { session_id, .. }
            | AgentEvent::MCPExecutionEnd { session_id, .. }
            | AgentEvent::AgentResponse { session_id, .. }
            | AgentEvent::AgentThought { session_id, .. } => session_id.as_str(),
        })
    }

    fn event_kind_label(event: Option<&AgentEvent>) -> &'static str {
        match event {
            Some(AgentEvent::SessionStart { .. }) => "session_start",
            Some(AgentEvent::SessionEnd { .. }) => "session_end",
            Some(AgentEvent::Processing { .. }) => "processing",
            Some(AgentEvent::ToolUse { .. }) => "tool_use",
            Some(AgentEvent::PermissionRequest { .. }) => "permission_request",
            Some(AgentEvent::AskQuestion { .. }) => "ask_question",
            Some(AgentEvent::PlanApproval { .. }) => "plan_approval",
            Some(AgentEvent::TaskComplete { .. }) => "task_complete",
            Some(AgentEvent::AssistantResponseComplete { .. }) => "assistant_response_complete",
            Some(AgentEvent::Error { .. }) => "error",
            Some(AgentEvent::Interrupt { .. }) => "interrupt",
            Some(AgentEvent::TokenUsage { .. }) => "token_usage",
            Some(AgentEvent::RateLimitUpdate { .. }) => "rate_limit_update",
            Some(AgentEvent::Notification { .. }) => "notification",
            Some(AgentEvent::SubagentStart { .. }) => "subagent_start",
            Some(AgentEvent::SubagentStop { .. }) => "subagent_stop",
            Some(AgentEvent::ShellExecutionStart { .. }) => "shell_execution_start",
            Some(AgentEvent::ShellExecutionEnd { .. }) => "shell_execution_end",
            Some(AgentEvent::MCPExecutionStart { .. }) => "mcp_execution_start",
            Some(AgentEvent::MCPExecutionEnd { .. }) => "mcp_execution_end",
            Some(AgentEvent::AgentResponse { .. }) => "agent_response",
            Some(AgentEvent::AgentThought { .. }) => "agent_thought",
            None => "unknown",
        }
    }

    /// Try to parse with registered adapters
    fn parse_with_adapters(
        adapters: &[Arc<dyn AgentAdapter>],
        raw: &serde_json::Value,
    ) -> Option<AgentEvent> {
        if let Some(agent) = raw.get("agent").and_then(|v| v.as_str()) {
            let agent = canonical_agent_id(agent);
            if let Some(adapter) = adapters.iter().find(|adapter| adapter.name() == agent) {
                if let Ok(event) = adapter.parse_event(raw) {
                    return Some(event);
                }
            }
        }

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
        app: &Arc<std::sync::Mutex<Option<tauri::AppHandle>>>,
    ) {
        Self::update_session_metadata_from_raw(store, _raw);
        match event {
            AgentEvent::SessionStart {
                session_id,
                project,
                cwd,
                terminal,
                agent_type,
            } => {
                store.get_or_create_session(session_id, agent_type, project, cwd, terminal);
                store.update_phase(session_id, SessionPhase::Ready);

                // Try to extract session title from the JSONL conversation file
                if let Some(file_path) = discover_session_file(session_id, cwd).or_else(|| {
                    if agent_type == "codex" {
                        discover_codex_session_file(session_id)
                    } else {
                        None
                    }
                }) {
                    if let Some(title) = extract_session_title(&file_path) {
                        store.update_session(session_id, |s| {
                            s.session_title = Some(title);
                        });
                    }
                }

                if let Some(prompt) = Self::extract_user_prompt_preview(_raw, 100) {
                    store.update_session(session_id, |s| {
                        if s.session_title.is_none() {
                            s.session_title = Some(prompt.clone());
                        }
                        if s.last_user_message.is_none() {
                            s.last_user_message = Some(prompt);
                        }
                    });
                }

                // Detect YOLO mode for Cursor sessions
                if agent_type == "cursor" || agent_type == "cursor-cli" {
                    let is_yolo = Self::detect_cursor_yolo_mode();
                    if is_yolo {
                        store.update_session(session_id, |s| {
                            s.is_yolo_mode = true;
                        });
                    }
                }

                Self::play_sound_for_session(sound, store, session_id, SoundEvent::SessionStart);
            }
            AgentEvent::SessionEnd { session_id } => {
                let summary = "Session ended".to_string();
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Done;
                    s.description = Some(summary.clone());
                    s.last_response = Some(summary.clone());
                });
                Self::play_sound_for_session(sound, store, session_id, SoundEvent::TaskComplete);
                Self::schedule_done_session_cleanup(store, session_id, SESSION_END_CLEANUP_SECS);
            }
            AgentEvent::Processing {
                session_id,
                description,
            } => {
                // If this is a UserPromptSubmit and session has no title yet,
                // try to extract the user prompt as session title
                let event_name = _raw.get("event").and_then(|v| v.as_str()).unwrap_or("");
                if event_name == "PreCompact" {
                    store.update_session(session_id, |s| {
                        s.phase = SessionPhase::Compacting;
                        s.description = Some("Compacting context".to_string());
                        s.last_tool_name = Some("Compacting".to_string());
                        s.last_tool_target = Some("context".to_string());
                        s.last_tool_status = Some("running".to_string());
                    });
                    Self::play_sound_for_session(sound, store, session_id, SoundEvent::ContextLimit);
                    return;
                }
                if event_name == "PostCompact" {
                    store.update_session(session_id, |s| {
                        s.phase = SessionPhase::Processing;
                        if s.description
                            .as_deref()
                            .map(Self::is_compacting_context_text)
                            .unwrap_or(false)
                        {
                            s.description = None;
                        }
                        if s.last_tool_name
                            .as_deref()
                            .map(Self::is_compacting_context_text)
                            .unwrap_or(false)
                        {
                            s.last_tool_name = None;
                            s.last_tool_target = None;
                            s.last_tool_status = None;
                        }
                    });
                    return;
                }
                if event_name == "UserPromptSubmit" {
                    if let Some(prompt) = Self::extract_user_prompt_preview(_raw, 100) {
                        store.set_last_user_message(session_id, Some(prompt));
                    }
                    store.update_session(session_id, |s| {
                        s.last_response = None;
                        s.last_thought = None;
                        s.last_tool_name = None;
                        s.last_tool_target = None;
                        s.last_tool_status = None;
                        s.description = None;
                    });

                    let has_title = store
                        .get_session(session_id)
                        .map(|s| s.session_title.is_some())
                        .unwrap_or(false);
                    if !has_title {
                        // Try to get prompt from the raw event data
                        let prompt = _raw
                            .get("prompt")
                            .or_else(|| _raw.get("user_prompt"))
                            .and_then(|v| v.as_str())
                            .map(|s| {
                                let first_line = s.lines().next().unwrap_or(s).trim();
                                Self::truncate_preview(first_line, 80)
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
            AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target,
                status,
            } => {
                // Extract tool_use_id from raw event for tool tracking
                let tool_use_id = _raw
                    .get("tool_use_id")
                    .or_else(|| _raw.get("toolUseId"))
                    .or_else(|| _raw.get("toolUseID"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                log::info!(
                    "[ToolUse] session={} tool={} status={}",
                    session_id,
                    tool_name,
                    status
                );
                store.update_session(session_id, |s| {
                    s.last_tool_name = Some(tool_name.clone());
                    s.last_tool_target = tool_target.clone();
                    s.last_tool_status = Some(status.clone());
                    s.phase = SessionPhase::Processing;
                });

                // Track TaskCreate/TaskUpdate tool payloads so the island task
                // summary survives backend-driven session-update refreshes.
                if matches!(tool_name.as_str(), "TaskCreate" | "TaskUpdate") {
                    let parsed_input = serde_json::from_str::<serde_json::Value>(tool_input)
                        .ok()
                        .or_else(|| _raw.get("tool_input").cloned())
                        .unwrap_or(serde_json::Value::Null);
                    let field = |name: &str| {
                        parsed_input
                            .get(name)
                            .and_then(|value| value.as_str())
                            .map(|value| value.to_string())
                    };

                    let subject = field("subject")
                        .or_else(|| field("name"))
                        .or_else(|| field("title"));
                    let task_id = field("taskId")
                        .or_else(|| field("task_id"))
                        .or_else(|| field("id"))
                        .or_else(|| subject.clone());
                    let task_status = field("status").unwrap_or_else(|| {
                        if tool_name == "TaskCreate" {
                            "pending".to_string()
                        } else {
                            "in_progress".to_string()
                        }
                    });

                    if let (Some(task_id), Some(subject)) = (task_id, subject) {
                        store.upsert_task(session_id, &task_id, &subject, &task_status);
                    }
                }

                // Track tool lifecycle
                if !tool_use_id.is_empty() {
                    match status.as_str() {
                        "running" => {
                            store.start_tool(
                                session_id,
                                &tool_use_id,
                                tool_name,
                                Some(tool_input.clone()),
                            );
                        }
                        "success" => {
                            store.complete_tool(session_id, &tool_use_id, true, None);
                        }
                        "error" => {
                            let error_msg = _raw
                                .get("tool_error")
                                .or_else(|| _raw.get("denial_reason"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            store.complete_tool(session_id, &tool_use_id, false, error_msg);
                        }
                        _ => {}
                    }
                }

                if Self::is_subagent_tool(tool_name) && !tool_use_id.is_empty() {
                    match status.as_str() {
                        "running" => {
                            let metadata = Self::subagent_metadata_from_tool_input(_raw, tool_input);
                            let transcript_path = Self::transcript_path_for_session(
                                store,
                                session_id,
                                _raw,
                            )
                            .map(|path| path.to_string_lossy().to_string());
                            store.add_subagent(
                                session_id,
                                &tool_use_id,
                                metadata.name,
                                &metadata.description,
                                metadata.agent_type,
                                transcript_path,
                            );
                            Self::refresh_subagents_from_transcript(store, session_id, _raw);
                        }
                        "success" => {
                            Self::refresh_subagents_from_transcript(store, session_id, _raw);
                        }
                        "error" => {
                            let metadata = Self::subagent_metadata_from_tool_input(_raw, tool_input);
                            store.stop_subagent(
                                session_id,
                                &tool_use_id,
                                SubagentStopUpdate {
                                    status: "error".to_string(),
                                    name: metadata.name,
                                    agent_type: metadata.agent_type,
                                    transcript_path: None,
                                    agent_transcript_path: None,
                                    last_assistant_message: None,
                                },
                            );
                        }
                        _ => {}
                    }
                }
            }
            AgentEvent::TaskComplete {
                session_id,
                summary,
            } => {
                let summary = Self::resolve_completion_summary(
                    store,
                    session_id,
                    summary,
                    "Task completed",
                    Some(_raw),
                );
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Done;
                    s.description = Some(summary.clone());
                    s.last_response = Some(summary.clone());
                });
                Self::refresh_cache_ttl_from_transcript(store, session_id, _raw);
                Self::refresh_subagents_from_transcript(store, session_id, _raw);
                let is_suppressed = Self::check_suppression(store, session_id);
                if is_suppressed {
                    if let Ok(guard) = app.lock() {
                        if let Some(ref handle) = *guard {
                            let summary = store
                                .get_session(session_id)
                                .and_then(|s| s.session_title.clone())
                                .unwrap_or_else(|| "Task completed".to_string());
                            crate::platform::notifications::send_completion_notification(
                                handle, &summary,
                            );
                        }
                    }
                }
                Self::play_sound_for_session(sound, store, session_id, SoundEvent::TaskComplete);
                Self::schedule_done_session_cleanup(
                    store,
                    session_id,
                    DONE_SESSION_HISTORY_CLEANUP_SECS,
                );
            }
            AgentEvent::AssistantResponseComplete { session_id, text } => {
                let truncated = Self::resolve_completion_summary(
                    store,
                    session_id,
                    text,
                    "Task completed",
                    Some(_raw),
                );
                store.update_session(session_id, |s| {
                    s.phase = if s.has_unfinished_tasks() {
                        SessionPhase::WaitingInput
                    } else {
                        SessionPhase::Ready
                    };
                    s.description = Some(truncated.clone());
                    s.last_response = Some(truncated.clone());
                    s.last_tool_name = None;
                    s.last_tool_target = None;
                    s.last_tool_status = None;
                    for subagent in &mut s.subagents {
                        if subagent.status == "running" {
                            subagent.status = "completed".to_string();
                        }
                    }
                });
                Self::refresh_cache_ttl_from_transcript(store, session_id, _raw);
                Self::refresh_subagents_from_transcript(store, session_id, _raw);
                let is_suppressed = Self::check_suppression(store, session_id);
                if is_suppressed {
                    if let Ok(guard) = app.lock() {
                        if let Some(ref handle) = *guard {
                            crate::platform::notifications::send_completion_notification(
                                handle, &truncated,
                            );
                        }
                    }
                }
                Self::play_sound_for_session(sound, store, session_id, SoundEvent::TaskComplete);
            }
            AgentEvent::Error {
                session_id,
                message,
            } => {
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Error;
                    s.description = Some(message.clone());
                    s.last_response = None;
                });
                Self::play_sound_for_session(sound, store, session_id, SoundEvent::TaskError);
            }
            AgentEvent::Interrupt { session_id } => {
                store.update_phase(session_id, SessionPhase::Interrupted);
            }
            AgentEvent::TokenUsage {
                session_id,
                input,
                output,
                cache_read,
                cache_create,
            } => {
                store.add_tokens(session_id, *input, *output, *cache_read, *cache_create);
            }
            AgentEvent::RateLimitUpdate {
                session_id,
                five_hour_usage,
                five_hour_remaining,
                seven_day_usage,
                seven_day_remaining,
                status_line_text,
                total_input_tokens,
                total_output_tokens,
                context_window_size,
                context_used_percentage,
                last_main_agent_at,
                cache_ttl_ms,
            } => {
                store.set_rate_limits(
                    session_id,
                    RateLimitInfo::legacy(
                        *five_hour_usage,
                        five_hour_remaining.clone(),
                        *seven_day_usage,
                        seven_day_remaining.clone(),
                    ),
                    status_line_text.clone(),
                );
                let context_window =
                    match (total_input_tokens, total_output_tokens, context_window_size) {
                        (Some(input), Some(output), Some(size)) => Some(ContextWindowInfo {
                            total_input_tokens: *input,
                            total_output_tokens: *output,
                            context_window_size: *size,
                            used_percentage: *context_used_percentage,
                        }),
                        _ => None,
                    };
                store.set_statusline_metadata(
                    session_id,
                    context_window,
                    status_line_text.clone(),
                    *last_main_agent_at,
                    *cache_ttl_ms,
                );
            }
            AgentEvent::Notification {
                session_id,
                message,
                status,
            } => {
                Self::process_notification(store, session_id, message, status, sound, _raw);
            }
            AgentEvent::SubagentStart {
                session_id,
                agent_id,
                name,
                description,
                agent_type,
                transcript_path,
            } => {
                log::info!(
                    "Subagent started: {} ({}) for session {}",
                    agent_id,
                    description,
                    session_id
                );
                store.add_subagent(
                    session_id,
                    agent_id,
                    name.clone(),
                    description,
                    agent_type.clone(),
                    transcript_path.clone(),
                );
            }
            AgentEvent::SubagentStop {
                session_id,
                agent_id,
                status,
                name,
                agent_type,
                transcript_path,
                agent_transcript_path,
                last_assistant_message,
            } => {
                log::info!(
                    "Subagent stopped: {} (status={}) for session {}",
                    agent_id,
                    status,
                    session_id
                );
                store.stop_subagent(
                    session_id,
                    agent_id,
                    SubagentStopUpdate {
                        status: status.clone(),
                        name: name.clone(),
                        agent_type: agent_type.clone(),
                        transcript_path: transcript_path.clone(),
                        agent_transcript_path: agent_transcript_path.clone(),
                        last_assistant_message: last_assistant_message.clone(),
                    },
                );
            }
            AgentEvent::ShellExecutionStart {
                session_id,
                command,
                cwd,
            } => {
                log::debug!("Shell starting: {} in {}", command, cwd);
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Processing;
                    s.last_tool_name = Some(format!("shell:{}", command));
                    s.last_tool_status = Some("running".to_string());
                });
            }
            AgentEvent::ShellExecutionEnd {
                session_id,
                command,
                exit_code,
                duration_ms,
                ..
            } => {
                log::debug!(
                    "Shell completed: {} (exit={:?}, {}ms)",
                    command,
                    exit_code,
                    duration_ms
                );
                let is_error = matches!(exit_code, Some(c) if *c != 0);
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Processing;
                    s.last_tool_status = Some(if is_error {
                        "error".to_string()
                    } else {
                        "success".to_string()
                    });
                });
                if is_error {
                    Self::play_sound_for_session(sound, store, session_id, SoundEvent::TaskError);
                }
            }
            AgentEvent::MCPExecutionStart {
                session_id,
                server_name,
                tool_name,
                ..
            } => {
                log::debug!("MCP {}:{} starting", server_name, tool_name);
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Processing;
                    s.last_tool_name = Some(format!("mcp:{}:{}", server_name, tool_name));
                    s.last_tool_status = Some("running".to_string());
                });
            }
            AgentEvent::MCPExecutionEnd {
                session_id,
                server_name,
                tool_name,
                error,
                duration_ms,
                ..
            } => {
                log::debug!(
                    "MCP {}:{} completed ({}ms)",
                    server_name,
                    tool_name,
                    duration_ms
                );
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Processing;
                    s.last_tool_status = Some(if error.is_none() {
                        "success".to_string()
                    } else {
                        "error".to_string()
                    });
                });
            }
            AgentEvent::AgentResponse {
                session_id,
                content,
                content_type,
            } => {
                log::debug!(
                    "Agent response received: {} bytes, type={}",
                    content.len(),
                    content_type
                );
                let truncated = if content.len() > 2000 {
                    let mut end = 1997;
                    while !content.is_char_boundary(end) {
                        end -= 1;
                    }
                    format!("{}...", &content[..end])
                } else {
                    content.clone()
                };
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Processing;
                    s.last_response = Some(truncated);
                });
            }
            AgentEvent::AgentThought {
                session_id,
                thought,
            } => {
                log::debug!("Agent thought: {} chars", thought.len());
                let truncated = if thought.len() > 2000 {
                    let mut end = 1997;
                    while !thought.is_char_boundary(end) {
                        end -= 1;
                    }
                    format!("{}...", &thought[..end])
                } else {
                    thought.clone()
                };
                store.update_session(session_id, |s| {
                    s.last_thought = Some(truncated);
                });
            }
            // PermissionRequest and AskQuestion are handled in handle_connection
            _ => {}
        }

        Self::update_session_metadata_from_raw(store, _raw);
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
        Self::update_session_metadata_from_raw(store, raw);

        // Extract session title from UserPromptSubmit if no title yet
        if _event_name == "UserPromptSubmit" {
            if let Some(prompt) = Self::extract_user_prompt_preview(raw, 100) {
                store.set_last_user_message(session_id, Some(prompt));
            }
            store.update_session(session_id, |s| {
                s.last_response = None;
                s.last_thought = None;
                s.last_tool_name = None;
                s.last_tool_target = None;
                s.last_tool_status = None;
                s.description = None;
            });

            let has_title = store
                .get_session(session_id)
                .map(|s| s.session_title.is_some())
                .unwrap_or(false);
            if !has_title {
                let prompt = raw
                    .get("prompt")
                    .or_else(|| raw.get("user_prompt"))
                    .and_then(|v| v.as_str())
                    .map(|s| {
                        let first_line = s.lines().next().unwrap_or(s).trim();
                        Self::truncate_preview(first_line, 80)
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
            "waiting_for_input" => SessionPhase::Ready,
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

    fn process_notification(
        store: &SessionStore,
        session_id: &str,
        message: &str,
        status: &Option<String>,
        sound: &Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
        raw: &serde_json::Value,
    ) {
        let lower = message.to_lowercase();
        let notification_type = raw
            .get("notification_type")
            .or_else(|| raw.get("notificationType"))
            .and_then(|v| v.as_str());

        if notification_type == Some("assistant_message") {
            if let Some(text) = Self::useful_completion_text(Some(message)) {
                store.update_session(session_id, |s| {
                    s.description = Some(text.clone());
                    s.last_response = Some(text);
                    s.last_tool_name = None;
                    s.last_tool_target = None;
                    s.last_tool_status = None;
                });
            }
            return;
        }

        if lower.contains("clear") || lower.contains("compact") {
            store.update_session(session_id, |s| {
                s.last_user_message = None;
                s.last_response = None;
                s.last_thought = None;
                s.last_tool_name = None;
                s.last_tool_target = None;
                s.last_tool_status = None;
                s.description = None;
            });
            return;
        }

        if Self::looks_like_error_message(&lower) {
            let text = Self::truncate_preview(message, 200);
            store.update_session(session_id, |s| {
                s.phase = SessionPhase::Error;
                s.description = Some(text.clone());
                s.last_response = None;
            });
            Self::play_sound_for_session(sound, store, session_id, SoundEvent::TaskError);
            return;
        }

        if lower.contains("complete") || lower.contains("done") {
            let summary = Self::resolve_completion_summary(
                store,
                session_id,
                message.trim(),
                "Task complete",
                Some(raw),
            );
            store.update_session(session_id, |s| {
                s.phase = SessionPhase::Done;
                s.description = Some(summary.clone());
                s.last_response = Some(summary.clone());
            });
            Self::play_sound_for_session(sound, store, session_id, SoundEvent::TaskComplete);
            Self::schedule_done_session_cleanup(
                store,
                session_id,
                DONE_SESSION_HISTORY_CLEANUP_SECS,
            );
            return;
        }

        if status.as_deref() == Some("waiting_for_input") {
            store.update_session(session_id, |s| {
                s.phase = SessionPhase::Ready;
                s.description = Some("Waiting for input".to_string());
            });
        }
    }

    fn ensure_session_for_event(store: &SessionStore, event: &AgentEvent, raw: &serde_json::Value) {
        let session_id = match event {
            AgentEvent::SessionStart { session_id, .. }
            | AgentEvent::SessionEnd { session_id }
            | AgentEvent::Processing { session_id, .. }
            | AgentEvent::ToolUse { session_id, .. }
            | AgentEvent::PermissionRequest { session_id, .. }
            | AgentEvent::AskQuestion { session_id, .. }
            | AgentEvent::PlanApproval { session_id, .. }
            | AgentEvent::TaskComplete { session_id, .. }
            | AgentEvent::AssistantResponseComplete { session_id, .. }
            | AgentEvent::Error { session_id, .. }
            | AgentEvent::Interrupt { session_id }
            | AgentEvent::TokenUsage { session_id, .. }
            | AgentEvent::RateLimitUpdate { session_id, .. }
            | AgentEvent::Notification { session_id, .. }
            | AgentEvent::SubagentStart { session_id, .. }
            | AgentEvent::SubagentStop { session_id, .. }
            | AgentEvent::ShellExecutionStart { session_id, .. }
            | AgentEvent::ShellExecutionEnd { session_id, .. }
            | AgentEvent::MCPExecutionStart { session_id, .. }
            | AgentEvent::MCPExecutionEnd { session_id, .. }
            | AgentEvent::AgentResponse { session_id, .. }
            | AgentEvent::AgentThought { session_id, .. } => session_id,
        };

        if session_id.is_empty()
            || session_id == "unknown"
            || store.get_session(session_id).is_some()
        {
            return;
        }

        let cwd = raw
            .get("cwd")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let project = cwd
            .rsplit('/')
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or("Unknown");
        let terminal = raw
            .get("tty")
            .or_else(|| raw.get("terminal"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let agent_type = match event {
            AgentEvent::SessionStart { agent_type, .. } => agent_type.as_str(),
            _ => raw
                .get("agent")
                .and_then(|value| value.as_str())
                .unwrap_or("claude-code"),
        };

        store.get_or_create_session(session_id, agent_type, project, cwd, terminal);
    }

    fn schedule_done_session_cleanup(store: &SessionStore, session_id: &str, delay_secs: u64) {
        let store_for_cleanup = store.clone();
        let session_id_for_cleanup = session_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
            let should_remove = store_for_cleanup
                .get_session(&session_id_for_cleanup)
                .map(|session| {
                    session.phase == SessionPhase::Done
                        && session.pending_permission.is_none()
                        && session.pending_question.is_none()
                        && session.pending_plan.is_none()
                })
                .unwrap_or(false);
            if should_remove {
                store_for_cleanup.remove_session(&session_id_for_cleanup);
            }
        });
    }

    fn looks_like_error_message(lower: &str) -> bool {
        lower.starts_with("api error")
            || lower.starts_with("error:")
            || lower.starts_with("error -")
            || (lower.contains("quota") && lower.contains("exceeded"))
    }

    fn extract_user_prompt_preview(raw: &serde_json::Value, max_chars: usize) -> Option<String> {
        raw.get("prompt")
            .or_else(|| raw.get("user_prompt"))
            .or_else(|| raw.get("message"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| Self::truncate_preview(s, max_chars))
    }

    fn truncate_preview(text: &str, max_chars: usize) -> String {
        let mut chars = text.chars();
        let truncated: String = chars.by_ref().take(max_chars).collect();
        if chars.next().is_some() {
            format!("{truncated}...")
        } else {
            truncated
        }
    }

    fn is_compacting_context_text(text: &str) -> bool {
        let normalized = text
            .trim()
            .trim_end_matches(|ch: char| matches!(ch, '.' | '!' | '。' | '！'))
            .to_lowercase();
        normalized == "compacting"
            || normalized == "compacting context"
            || normalized.starts_with("compacting context")
            || normalized == "compacting conversation"
            || normalized.starts_with("compacting conversation")
    }

    fn is_generic_completion_text(text: &str) -> bool {
        let normalized = text
            .trim()
            .trim_end_matches(|ch: char| matches!(ch, '.' | '!' | '。' | '！'))
            .to_lowercase();
        normalized.is_empty()
            || normalized == "done"
            || normalized == "task complete"
            || normalized == "task completed"
            || normalized == "session ended"
            || normalized == "processing user input"
            || normalized.starts_with("processing user input:")
            || normalized == "compacting context"
            || normalized.starts_with("compacting context")
            || normalized == "compacting conversation"
            || normalized.starts_with("compacting conversation")
            || normalized == "waiting for input"
    }

    fn useful_completion_text(text: Option<&str>) -> Option<String> {
        let trimmed = text?.trim();
        if trimmed.is_empty() || Self::is_generic_completion_text(trimmed) {
            None
        } else {
            Some(Self::truncate_preview(trimmed, 2000))
        }
    }

    fn resolve_completion_summary(
        store: &SessionStore,
        session_id: &str,
        incoming: &str,
        fallback: &str,
        raw: Option<&serde_json::Value>,
    ) -> String {
        Self::useful_completion_text(Some(incoming))
            .or_else(|| {
                store
                    .get_session(session_id)
                    .and_then(|s| Self::useful_completion_text(s.last_response.as_deref()))
            })
            .or_else(|| Self::latest_assistant_text_from_transcript(store, session_id, raw))
            .or_else(|| {
                store
                    .get_session(session_id)
                    .and_then(|s| Self::useful_completion_text(s.description.as_deref()))
            })
            .unwrap_or_else(|| fallback.to_string())
    }

    fn latest_assistant_text_from_transcript(
        store: &SessionStore,
        session_id: &str,
        raw: Option<&serde_json::Value>,
    ) -> Option<String> {
        let path = Self::transcript_path_for_session(store, session_id, raw?)?;
        Self::useful_completion_text(extract_latest_assistant_text(&path).as_deref())
    }

    fn is_subagent_tool(tool_name: &str) -> bool {
        matches!(tool_name, "Agent" | "Task")
    }

    fn subagent_metadata_from_tool_input(
        raw: &serde_json::Value,
        tool_input: &str,
    ) -> SubagentToolMetadata {
        let input = serde_json::from_str::<serde_json::Value>(tool_input)
            .ok()
            .or_else(|| raw.get("tool_input").cloned())
            .unwrap_or(serde_json::Value::Null);

        let field = |keys: &[&str]| -> Option<String> {
            keys.iter()
                .find_map(|key| input.get(*key).and_then(|value| value.as_str()))
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };

        let name = field(&["name", "agent_name", "agentName"]);
        let prompt = field(&["prompt"]);
        let description = field(&["description"])
            .or_else(|| prompt.clone())
            .unwrap_or_else(|| "Subagent".to_string());
        let agent_type = field(&["agent_type", "agentType", "subagent_type", "subagentType"]);

        SubagentToolMetadata {
            name,
            description,
            agent_type,
        }
    }

    fn transcript_path_for_session(
        store: &SessionStore,
        session_id: &str,
        raw: &serde_json::Value,
    ) -> Option<std::path::PathBuf> {
        raw.get("transcript_path")
            .or_else(|| raw.get("transcriptPath"))
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| {
                let session = store.get_session(session_id)?;
                discover_session_file(session_id, &session.cwd).or_else(|| {
                    if session.agent_type == "codex"
                        || raw
                            .get("agent")
                            .and_then(|value| value.as_str())
                            .is_some_and(|agent| agent == "codex")
                    {
                        discover_codex_session_file(session_id)
                    } else {
                        None
                    }
                })
            })
    }

    fn update_session_metadata_from_raw(store: &SessionStore, raw: &serde_json::Value) {
        let Some(session_id) = raw.get("session_id").and_then(|v| v.as_str()) else {
            return;
        };

        let pid = raw.get("pid").and_then(|v| v.as_u64()).map(|v| v as u32);
        let tty = raw
            .get("tty")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty());
        let engine_label = raw
            .get("engine_label")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty());
        let engine_config_root = raw
            .get("engine_config_root")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty());
        let term_program = optional_nonempty_string(raw, "_term_program")
            .or_else(|| optional_nonempty_string(raw, "_term_app"));
        let term_bundle_id = optional_nonempty_string(raw, "_term_bundle_id");
        let wezterm_pane = optional_nonempty_string(raw, "_wezterm_pane");
        let zellij_pane_id = optional_nonempty_string(raw, "_zellij_pane_id");
        let zellij_session_name = optional_nonempty_string(raw, "_zellij_session_name");
        let cmux_surface_id = optional_nonempty_string(raw, "_cmux_surface_id");
        let cmux_workspace_id = optional_nonempty_string(raw, "_cmux_workspace_id");

        if pid.is_none()
            && tty.is_none()
            && engine_label.is_none()
            && engine_config_root.is_none()
            && term_program.is_none()
            && term_bundle_id.is_none()
            && wezterm_pane.is_none()
            && zellij_pane_id.is_none()
            && zellij_session_name.is_none()
            && cmux_surface_id.is_none()
            && cmux_workspace_id.is_none()
        {
            return;
        }

        store.update_session(session_id, |s| {
            if let Some(pid) = pid {
                s.pid = Some(pid);
            }
            if let Some(tty) = tty {
                s.tty = Some(tty.to_string());
            }
            if let Some(label) = engine_label {
                s.engine_label = Some(label.to_string());
            }
            if let Some(root) = engine_config_root {
                s.engine_config_root = Some(root.to_string());
            }
            if let Some(value) = term_program {
                s.term_program = Some(value.to_string());
            }
            if let Some(value) = term_bundle_id {
                s.term_bundle_id = Some(value.to_string());
            }
            if let Some(value) = wezterm_pane {
                s.wezterm_pane = Some(value.to_string());
            }
            if let Some(value) = zellij_pane_id {
                s.zellij_pane_id = Some(value.to_string());
            }
            if let Some(value) = zellij_session_name {
                s.zellij_session_name = Some(value.to_string());
            }
            if let Some(value) = cmux_surface_id {
                s.cmux_surface_id = Some(value.to_string());
            }
            if let Some(value) = cmux_workspace_id {
                s.cmux_workspace_id = Some(value.to_string());
            }
        });
    }

    fn refresh_cache_ttl_from_transcript(
        store: &SessionStore,
        session_id: &str,
        raw: &serde_json::Value,
    ) {
        let transcript_path = raw
            .get("transcript_path")
            .or_else(|| raw.get("transcriptPath"))
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| {
                let session = store.get_session(session_id)?;
                discover_session_file(session_id, &session.cwd)
            });

        let Some(path) = transcript_path else {
            return;
        };
        let Some(info) = extract_cache_ttl_info(&path) else {
            return;
        };

        store.set_statusline_metadata(
            session_id,
            None,
            None,
            Some(info.timestamp_ms),
            Some(info.ttl_ms),
        );
    }

    fn refresh_subagents_from_transcript(
        store: &SessionStore,
        session_id: &str,
        raw: &serde_json::Value,
    ) {
        let Some(path) = Self::transcript_path_for_session(store, session_id, raw) else {
            return;
        };
        let recovered = extract_subagents_from_transcript(&path);
        if recovered.is_empty() {
            return;
        }

        store.update_session(session_id, |session| {
            for subagent in recovered {
                Self::merge_recovered_subagent(session, subagent);
            }
            session.subagents.sort_by(|a, b| {
                a.started_at
                    .cmp(&b.started_at)
                    .then_with(|| a.agent_id.cmp(&b.agent_id))
            });
        });
    }

    fn merge_recovered_subagent(
        session: &mut super::session_store::SessionState,
        recovered: TranscriptSubagentInfo,
    ) {
        let launch_tool_use_id = recovered.launch_tool_use_id.clone();
        let incoming = SubagentInfo {
            agent_id: recovered.agent_id,
            name: recovered.name,
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
            .find(|subagent| {
                subagent.agent_id == incoming.agent_id
                    || launch_tool_use_id
                        .as_deref()
                        .is_some_and(|tool_use_id| subagent.agent_id == tool_use_id)
            })
        {
            *existing = incoming;
        } else {
            session.subagents.push(incoming);
        }
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
        always: bool,
    ) -> anyhow::Result<()> {
        let mut pending_map = self.pending_permissions.lock().await;
        if let Some(entry) = pending_map.remove(session_id) {
            let response = PermissionResponse {
                decision: if allowed {
                    "allow".to_string()
                } else {
                    "deny".to_string()
                },
                reason: if allowed {
                    None
                } else {
                    Some("Denied by user via AgentBro".to_string())
                },
                always: if allowed && always { Some(true) } else { None },
            };
            // Ignore send error — receiver may have already dropped
            let _ = entry.tx.send(response);
            Ok(())
        } else {
            anyhow::bail!("No pending permission for session {}", session_id)
        }
    }

    /// Respond to a pending question from the UI
    pub async fn respond_question(&self, session_id: &str, answer: String) -> anyhow::Result<()> {
        let mut pending_map = self.pending_questions.lock().await;
        if let Some(entry) = pending_map.remove(session_id) {
            let response = QuestionResponse { answer };
            let _ = entry.tx.send(response);
            Ok(())
        } else {
            anyhow::bail!("No pending question for session {}", session_id)
        }
    }

    /// Respond to a pending plan approval from the UI
    pub async fn respond_plan(
        &self,
        session_id: &str,
        mode: String,
        message: Option<String>,
    ) -> anyhow::Result<()> {
        let mut pending_map = self.pending_plans.lock().await;
        if let Some(entry) = pending_map.remove(session_id) {
            let response = PlanResponse { mode, message };
            let _ = entry.tx.send(response);
            Ok(())
        } else {
            anyhow::bail!("No pending plan for session {}", session_id)
        }
    }

    pub async fn respond_auto_approve(&self, session_id: &str) -> anyhow::Result<()> {
        let mut pending_map = self.pending_permissions.lock().await;
        if let Some(entry) = pending_map.remove(session_id) {
            let response = PermissionResponse {
                decision: "auto".to_string(),
                reason: None,
                always: None,
            };
            let _ = entry.tx.send(response);
            Ok(())
        } else {
            anyhow::bail!("No pending permission for session {}", session_id)
        }
    }
}

fn canonical_agent_id(agent: &str) -> &str {
    match agent {
        "codybuddycn" => "codebuddycn",
        "claude" => "claude-code",
        other => other,
    }
}

fn optional_nonempty_string<'a>(raw: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    raw.get(key)
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl Drop for HookServer {
    fn drop(&mut self) {
        if self.socket_owned.swap(false, Ordering::SeqCst) {
            let _ = std::fs::remove_file(&self.endpoint.socket_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_session_creates_codex_session_without_session_start() {
        let store = SessionStore::new();
        let raw = serde_json::json!({
            "agent": "codex",
            "session_id": "codex-mid-session",
            "cwd": "/tmp/my-project",
            "tty": "/dev/ttys001"
        });
        let event = AgentEvent::Processing {
            session_id: "codex-mid-session".to_string(),
            description: "Processing user input".to_string(),
        };

        HookServer::ensure_session_for_event(&store, &event, &raw);

        let session = store
            .get_session("codex-mid-session")
            .expect("session should be created");
        assert_eq!(session.agent_type, "codex");
        assert_eq!(session.project, "my-project");
        assert_eq!(session.cwd, "/tmp/my-project");
        assert_eq!(session.terminal, "/dev/ttys001");
    }

    #[test]
    fn codex_interaction_timeout_matches_hook_bridge_window() {
        let raw = serde_json::json!({ "agent": "codex" });

        assert_eq!(
            HookServer::interaction_response_timeout(&raw),
            Duration::from_secs(HUMAN_INTERACTION_RESPONSE_TIMEOUT_SECS)
        );
    }

    #[test]
    fn non_codex_interaction_timeout_keeps_existing_window() {
        let raw = serde_json::json!({ "agent": "opencode" });

        assert_eq!(
            HookServer::interaction_response_timeout(&raw),
            Duration::from_secs(DEFAULT_INTERACTION_RESPONSE_TIMEOUT_SECS)
        );
    }

    #[test]
    fn claude_code_interaction_timeout_matches_hook_bridge_window() {
        let raw = serde_json::json!({ "agent": "claude-code" });

        assert_eq!(
            HookServer::interaction_response_timeout(&raw),
            Duration::from_secs(HUMAN_INTERACTION_RESPONSE_TIMEOUT_SECS)
        );
    }

    #[test]
    fn completion_summary_ignores_processing_prompt_preview() {
        let store = SessionStore::new();
        let summary = HookServer::resolve_completion_summary(
            &store,
            "missing-session",
            "Processing user input: hi",
            "Task completed",
            None,
        );

        assert_eq!(summary, "Task completed");
    }

    #[test]
    fn pre_and_post_compact_events_drive_compacting_phase() {
        let store = SessionStore::new();
        store.get_or_create_session(
            "compact-session",
            "claude-code",
            "project",
            "/tmp/project",
            "iTerm",
        );
        let sound = Arc::new(std::sync::Mutex::new(None));
        let app = Arc::new(std::sync::Mutex::new(None));

        let pre = AgentEvent::Processing {
            session_id: "compact-session".to_string(),
            description: "Compacting context".to_string(),
        };
        HookServer::process_event(
            &store,
            &pre,
            &serde_json::json!({ "event": "PreCompact" }),
            &sound,
            &app,
        );

        let session = store
            .get_session("compact-session")
            .expect("session should exist");
        assert_eq!(session.phase, SessionPhase::Compacting);
        assert_eq!(session.description.as_deref(), Some("Compacting context"));
        assert_eq!(session.last_tool_name.as_deref(), Some("Compacting"));

        let post = AgentEvent::Processing {
            session_id: "compact-session".to_string(),
            description: "Compacting context".to_string(),
        };
        HookServer::process_event(
            &store,
            &post,
            &serde_json::json!({ "event": "PostCompact" }),
            &sound,
            &app,
        );

        let session = store
            .get_session("compact-session")
            .expect("session should exist");
        assert_eq!(session.phase, SessionPhase::Processing);
        assert_eq!(session.description, None);
        assert_eq!(session.last_tool_name, None);
        assert_eq!(session.last_tool_target, None);
        assert_eq!(session.last_tool_status, None);
    }

    #[test]
    fn ensure_session_creates_session_for_first_permission_request() {
        let store = SessionStore::new();
        let raw = serde_json::json!({
            "agent": "codex",
            "session_id": "codex-approval",
            "cwd": "/tmp/my-project"
        });
        let event = AgentEvent::PermissionRequest {
            session_id: "codex-approval".to_string(),
            tool_name: "Bash".to_string(),
            diff: None,
            options: None,
        };

        HookServer::ensure_session_for_event(&store, &event, &raw);

        let session = store
            .get_session("codex-approval")
            .expect("permission request should create a session");
        assert_eq!(session.agent_type, "codex");
        assert_eq!(session.project, "my-project");
    }

    #[test]
    fn parses_running_agent_tool_metadata_for_subagent_rows() {
        let raw = serde_json::json!({
            "tool_input": {
                "name": "calc-a",
                "description": "计算 1+1",
                "prompt": "请计算 1+1 等于几",
                "agentType": "general-purpose"
            }
        });

        let metadata = HookServer::subagent_metadata_from_tool_input(&raw, "");

        assert_eq!(metadata.name.as_deref(), Some("calc-a"));
        assert_eq!(metadata.description, "计算 1+1");
        assert_eq!(metadata.agent_type.as_deref(), Some("general-purpose"));
    }

    #[test]
    fn recovered_subagent_replaces_running_tool_placeholder() {
        let mut session = crate::hooks::session_store::SessionState::new(
            "s1".to_string(),
            "claude-code".to_string(),
            "agentbro".to_string(),
            "/tmp/agentbro".to_string(),
            "iTerm".to_string(),
        );
        session.subagents.push(SubagentInfo {
            agent_id: "toolu-1".to_string(),
            name: Some("calc-a".to_string()),
            agent_type: Some("general-purpose".to_string()),
            description: "计算 1+1".to_string(),
            transcript_path: Some("/tmp/main.jsonl".to_string()),
            agent_transcript_path: None,
            last_assistant_message: None,
            started_at: 10,
            completed_at: None,
            status: "running".to_string(),
            tools: Vec::new(),
        });

        HookServer::merge_recovered_subagent(
            &mut session,
            TranscriptSubagentInfo {
                agent_id: "agent-a123".to_string(),
                launch_tool_use_id: Some("toolu-1".to_string()),
                name: Some("calc-a".to_string()),
                agent_type: Some("general-purpose".to_string()),
                description: "计算 1+1".to_string(),
                transcript_path: Some("/tmp/main.jsonl".to_string()),
                agent_transcript_path: Some("/tmp/agent-a123.jsonl".to_string()),
                last_assistant_message: Some("2".to_string()),
                started_at: 10,
                completed_at: Some(20),
                status: "completed".to_string(),
                tools: Vec::new(),
            },
        );

        assert_eq!(session.subagents.len(), 1);
        assert_eq!(session.subagents[0].agent_id, "agent-a123");
        assert_eq!(session.subagents[0].status, "completed");
        assert_eq!(
            session.subagents[0].agent_transcript_path.as_deref(),
            Some("/tmp/agent-a123.jsonl")
        );
    }
}
