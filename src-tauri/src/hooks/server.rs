// HookServer — Async TCP + Unix socket server for agent hook events
// Accepts JSON-line protocol from hook scripts, routes to adapters,
// and keeps connections alive for permission request/response flow.

use std::collections::{BTreeMap, HashMap, VecDeque};
#[cfg(unix)]
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{oneshot, Mutex};

use super::session_store::{
    AgentRunStatus, ContextWindowInfo, PendingPermission, PendingPlan, PendingQuestion,
    QuestionItem as PendingQuestionItem, QuestionOption as PendingQuestionOption, RateLimitInfo,
    SessionPhase, SessionStore, SubagentInfo, SubagentStopUpdate,
};
use crate::agents::{AgentAdapter, AgentEvent};
use crate::config::{AppConfig, ConfigStore};
use crate::hook_endpoint;
use crate::hooks::conversation_parser::{
    discover_codex_session_file, discover_session_file, extract_cache_ttl_info,
    extract_latest_assistant_text, extract_pending_codex_user_input, extract_session_title,
    extract_subagents_from_transcript, CodexPendingUserInput, TranscriptSubagentInfo,
};
use crate::sound::{SoundEngine, SoundEvent};
use crate::terminal::suppression;
use crate::webhook::{self, templates::NotificationEvent};

const RAW_EVENT_BUFFER_PER_SESSION: usize = 200;
const SESSION_END_CLEANUP_SECS: u64 = 5;
const DONE_SESSION_HISTORY_CLEANUP_SECS: u64 = 300;
const DEFAULT_INTERACTION_RESPONSE_TIMEOUT_SECS: u64 = 300;
const HUMAN_INTERACTION_RESPONSE_TIMEOUT_SECS: u64 = 21_600;
const RECENT_TOOL_CACHE_TTL_MS: u64 = 2 * 60 * 1000;
const RECENT_TOOL_CACHE_LIMIT: usize = 200;
const WEBHOOK_INTERACTION_EVENTS: [&str; 3] =
    ["waiting_approval", "waiting_input", "plan_approval"];

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

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawHookEventSummary {
    pub seq: u64,
    pub timestamp_ms: u64,
    pub session_id: String,
    pub agent: Option<String>,
    pub event_name: String,
    pub cwd: Option<String>,
    pub payload_keys: Vec<String>,
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

    fn recent_summaries(&self, limit: usize) -> Vec<RawHookEventSummary> {
        let mut events = self
            .by_session
            .values()
            .flat_map(|events| events.iter())
            .collect::<Vec<_>>();
        events.sort_by_key(|event| event.seq);
        let start = events.len().saturating_sub(limit);
        events[start..]
            .iter()
            .map(|event| RawHookEventSummary::from_event(event))
            .collect()
    }
}

impl RawHookEventSummary {
    fn from_event(event: &RawHookEvent) -> Self {
        let payload_keys = event
            .raw
            .as_object()
            .map(|object| object.keys().cloned().collect())
            .unwrap_or_default();
        Self {
            seq: event.seq,
            timestamp_ms: event.timestamp_ms,
            session_id: event.session_id.clone(),
            agent: event.agent.clone(),
            event_name: event.event_name.clone(),
            cwd: event
                .raw
                .get("cwd")
                .and_then(|value| value.as_str())
                .map(ToString::to_string),
            payload_keys,
        }
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
    pub(crate) session_id: String,
    pub(crate) received_at_ms: u64,
    pub(crate) tx: oneshot::Sender<PermissionReply>,
}

pub(crate) struct PermissionReply {
    pub(crate) response: PermissionResponse,
    pub(crate) ack: oneshot::Sender<Result<(), String>>,
}

#[derive(Debug, Clone)]
struct RecentToolInvocation {
    session_id: String,
    tool_name: String,
    tool_use_id: String,
    signature: String,
    seen_at_ms: u64,
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
    /// Pending permission requests: tool/request key -> waiting hook connections
    pending_permissions: Arc<Mutex<HashMap<String, Vec<PendingPermissionEntry>>>>,
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
    /// App config store for webhook forwarding.
    config_store: Arc<std::sync::Mutex<Option<ConfigStore>>>,
    /// IPC endpoint owned by this server instance.
    endpoint: hook_endpoint::HookEndpoint,
    socket_owned: Arc<AtomicBool>,
    /// Recent PreToolUse cache for PermissionRequest correlation when Codex omits tool_use_id.
    recent_tools: Arc<Mutex<VecDeque<RecentToolInvocation>>>,
}

#[derive(Clone)]
struct HookConnectionContext {
    pending: Arc<Mutex<HashMap<String, Vec<PendingPermissionEntry>>>>,
    pending_q: Arc<Mutex<HashMap<String, PendingQuestionEntry>>>,
    pending_plan: Arc<Mutex<HashMap<String, PendingPlanEntry>>>,
    store: Arc<SessionStore>,
    adapters: Arc<Vec<Arc<dyn AgentAdapter>>>,
    sound: Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
    app: Arc<std::sync::Mutex<Option<tauri::AppHandle>>>,
    raw_events: Arc<std::sync::Mutex<RawHookEventStore>>,
    config_store: Arc<std::sync::Mutex<Option<ConfigStore>>>,
    recent_tools: Arc<Mutex<VecDeque<RecentToolInvocation>>>,
}

#[derive(Clone)]
enum PendingWebhookCheck {
    Permission {
        tool_use_id: Option<String>,
        tool_name: String,
    },
    Question {
        question: String,
    },
    Plan {
        title: String,
        content: String,
    },
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
            config_store: Arc::new(std::sync::Mutex::new(None)),
            endpoint: hook_endpoint::current(),
            socket_owned: Arc::new(AtomicBool::new(false)),
            recent_tools: Arc::new(Mutex::new(VecDeque::new())),
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

    /// Set the config store (called after construction)
    pub fn set_config_store(&self, config_store: ConfigStore) {
        if let Ok(mut store) = self.config_store.lock() {
            *store = Some(config_store);
        }
    }

    fn interaction_response_timeout(raw: &serde_json::Value) -> Duration {
        let agent = raw
            .get("agent")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let seconds = if matches!(
            agent,
            "codex" | "openai.codex" | "claude-code" | "claude" | "opencode"
        ) {
            HUMAN_INTERACTION_RESPONSE_TIMEOUT_SECS
        } else {
            DEFAULT_INTERACTION_RESPONSE_TIMEOUT_SECS
        };
        Duration::from_secs(seconds)
    }

    async fn record_recent_tool_invocation(
        recent_tools: &Arc<Mutex<VecDeque<RecentToolInvocation>>>,
        event: Option<&AgentEvent>,
        raw: &serde_json::Value,
    ) {
        let Some(AgentEvent::ToolUse {
            session_id,
            tool_name,
            status,
            ..
        }) = event
        else {
            return;
        };
        if status != "running" {
            return;
        }
        let Some(tool_use_id) = Self::raw_tool_use_id(raw) else {
            return;
        };

        let now = current_time_ms();
        let mut tools = recent_tools.lock().await;
        Self::prune_recent_tools(&mut tools, now);
        tools.push_back(RecentToolInvocation {
            session_id: session_id.clone(),
            tool_name: tool_name.clone(),
            tool_use_id,
            signature: Self::tool_input_signature_from_raw(raw),
            seen_at_ms: now,
        });
        while tools.len() > RECENT_TOOL_CACHE_LIMIT {
            tools.pop_front();
        }
    }

    async fn resolve_permission_tool_use_id(
        recent_tools: &Arc<Mutex<VecDeque<RecentToolInvocation>>>,
        session_id: &str,
        tool_name: &str,
        raw: &serde_json::Value,
    ) -> Option<String> {
        if let Some(tool_use_id) = Self::raw_tool_use_id(raw) {
            return Some(tool_use_id);
        }

        let signature = Self::tool_input_signature_from_raw(raw);
        let now = current_time_ms();
        let mut tools = recent_tools.lock().await;
        Self::prune_recent_tools(&mut tools, now);

        if !signature.is_empty() {
            if let Some(tool) = tools.iter().rev().find(|tool| {
                tool.session_id == session_id
                    && tool.tool_name == tool_name
                    && tool.signature == signature
            }) {
                return Some(tool.tool_use_id.clone());
            }
        }

        tools
            .iter()
            .rev()
            .find(|tool| tool.session_id == session_id && tool.tool_name == tool_name)
            .map(|tool| tool.tool_use_id.clone())
    }

    fn prune_recent_tools(tools: &mut VecDeque<RecentToolInvocation>, now_ms: u64) {
        while tools
            .front()
            .is_some_and(|tool| now_ms.saturating_sub(tool.seen_at_ms) > RECENT_TOOL_CACHE_TTL_MS)
        {
            tools.pop_front();
        }
    }

    fn raw_tool_use_id(raw: &serde_json::Value) -> Option<String> {
        raw.get("tool_use_id")
            .or_else(|| raw.get("toolUseId"))
            .or_else(|| raw.get("toolUseID"))
            .or_else(|| raw.get("toolCallId"))
            .or_else(|| raw.get("tool_call_id"))
            .or_else(|| raw.get("callId"))
            .or_else(|| raw.get("call_id"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.to_string())
    }

    fn tool_input_signature_from_raw(raw: &serde_json::Value) -> String {
        Self::tool_input_signature(raw.get("tool_input").or_else(|| raw.get("toolInput")))
    }

    fn tool_input_signature(input: Option<&serde_json::Value>) -> String {
        let Some(input) = input else {
            return String::new();
        };

        if let Some(obj) = input.as_object() {
            for key in ["command", "file_path", "filePath", "path"] {
                if let Some(value) = obj.get(key).and_then(|value| value.as_str()) {
                    if !value.trim().is_empty() {
                        return format!("{}:{}", key, value);
                    }
                }
            }
        }

        Self::canonical_json_for_signature(input)
    }

    fn clear_pending_permission_after_tool_resolution(
        store: &SessionStore,
        session_id: &str,
        raw: &serde_json::Value,
    ) {
        let Some(session) = store.get_session(session_id) else {
            return;
        };
        let Some(pending) = session.pending_permission else {
            return;
        };

        let raw_tool_use_id = Self::raw_tool_use_id(raw);
        let same_tool_use_id = raw_tool_use_id
            .as_deref()
            .is_some_and(|id| pending.tool_use_id.as_deref() == Some(id));
        if same_tool_use_id || Self::is_permission_resolution_event(raw) {
            store.set_pending_permission(session_id, None);
        }
    }

    fn is_permission_resolution_event(raw: &serde_json::Value) -> bool {
        let event = Self::raw_event_name(raw).to_ascii_lowercase();
        event == "permissiondenied"
            || event.ends_with(".approved")
            || event.ends_with(".denied")
            || event.ends_with(".rejected")
    }

    fn canonical_json_for_signature(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Object(map) => {
                let mut entries = BTreeMap::new();
                for (key, value) in map {
                    if key == "description" {
                        continue;
                    }
                    entries.insert(key.clone(), Self::canonical_json_for_signature(value));
                }
                let parts = entries
                    .into_iter()
                    .map(|(key, value)| {
                        let key = serde_json::to_string(&key).unwrap_or_else(|_| "\"\"".into());
                        format!("{}:{}", key, value)
                    })
                    .collect::<Vec<_>>();
                format!("{{{}}}", parts.join(","))
            }
            serde_json::Value::Array(values) => {
                let parts = values
                    .iter()
                    .map(Self::canonical_json_for_signature)
                    .collect::<Vec<_>>();
                format!("[{}]", parts.join(","))
            }
            _ => serde_json::to_string(value).unwrap_or_default(),
        }
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

    fn current_config_store(
        config_store: &Arc<std::sync::Mutex<Option<ConfigStore>>>,
    ) -> Option<ConfigStore> {
        config_store.lock().ok().and_then(|store| store.clone())
    }

    fn source_for_webhook(
        store: &SessionStore,
        raw: &serde_json::Value,
        session_id: &str,
        fallback: Option<&str>,
    ) -> String {
        store
            .get_session(session_id)
            .map(|session| session.agent_type)
            .filter(|source| !source.trim().is_empty())
            .or_else(|| {
                raw.get("agent")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string)
            })
            .or_else(|| fallback.map(ToString::to_string))
            .unwrap_or_else(|| "agent".to_string())
    }

    fn permission_input_value_from_raw(raw: &serde_json::Value) -> Option<serde_json::Value> {
        if let Some(input) = raw.get("tool_input").or_else(|| raw.get("toolInput")) {
            return Some(input.clone());
        }

        let mut input = serde_json::Map::new();
        if let Some(command) = raw
            .get("commandPreview")
            .or_else(|| raw.pointer("/messageParams/command"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            input.insert(
                "command".to_string(),
                serde_json::Value::String(command.to_string()),
            );
        }
        if let Some(paths) = raw.pointer("/messageParams/paths").cloned() {
            input.insert("paths".to_string(), paths);
        }
        if let Some(message) = raw
            .get("message")
            .or_else(|| raw.get("messageKey"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            input.insert(
                "message".to_string(),
                serde_json::Value::String(message.to_string()),
            );
        }
        if let Some(category) = raw.get("category").and_then(|value| value.as_str()) {
            input.insert(
                "category".to_string(),
                serde_json::Value::String(category.to_string()),
            );
        }
        if let Some(event_type) = raw.get("eventType").and_then(|value| value.as_str()) {
            input.insert(
                "eventType".to_string(),
                serde_json::Value::String(event_type.to_string()),
            );
        }

        (!input.is_empty()).then_some(serde_json::Value::Object(input))
    }

    fn normalized_permission_input_from_raw(raw: &serde_json::Value) -> Option<serde_json::Value> {
        let input = Self::permission_input_value_from_raw(raw)?;
        if let Some(text) = input.as_str() {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
                return Some(parsed);
            }
        }
        Some(input)
    }

    fn permission_tool_input_from_raw(raw: &serde_json::Value) -> String {
        Self::permission_input_value_from_raw(raw)
            .map(|value| value.to_string())
            .unwrap_or_default()
    }

    fn approval_detail_for_webhook(tool_name: &str, raw: &serde_json::Value) -> Option<String> {
        let parsed_input = Self::normalized_permission_input_from_raw(raw)?;

        let mut lines = vec![format!("> {}", tool_name)];
        let command = parsed_input
            .get("command")
            .or_else(|| parsed_input.get("cmd"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let description = parsed_input
            .get("description")
            .or_else(|| parsed_input.get("desc"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if let Some(command) = command {
            lines.push(String::new());
            lines.push("```bash".to_string());
            lines.push(command.chars().take(1200).collect());
            lines.push("```".to_string());
        }
        if let Some(description) = description {
            lines.push(String::new());
            lines.push(format!("> {}", description));
        }
        if command.is_none() && description.is_none() {
            let detail = serde_json::to_string_pretty(&parsed_input)
                .or_else(|_| serde_json::to_string(&parsed_input))
                .unwrap_or_default();
            if detail.trim().is_empty() {
                return None;
            }
            lines.push(String::new());
            lines.push("```json".to_string());
            lines.push(detail.chars().take(1600).collect());
            lines.push("```".to_string());
        }

        Some(lines.join("\n"))
    }

    fn dispatch_webhook_event(
        config_store: &Arc<std::sync::Mutex<Option<ConfigStore>>>,
        store: &Arc<SessionStore>,
        event: NotificationEvent,
        source: String,
        session_id: String,
        pending_check: Option<PendingWebhookCheck>,
    ) {
        let Some(config_store) = Self::current_config_store(config_store) else {
            return;
        };
        let event_key = webhook::templates::event_key(&event);
        let app_config = config_store.get();
        let language = app_config.language.clone();
        let configs = app_config.webhook_configs;
        let mut immediate = Vec::new();

        for cfg in configs {
            if !cfg.matches(event_key, &source) {
                continue;
            }

            if let Some(check) = pending_check
                .clone()
                .filter(|_| WEBHOOK_INTERACTION_EVENTS.contains(&event_key) && cfg.delay_enabled)
            {
                Self::schedule_delayed_webhook(
                    config_store.clone(),
                    store.clone(),
                    cfg.id.clone(),
                    event_key.to_string(),
                    event.clone(),
                    source.clone(),
                    session_id.clone(),
                    check,
                    cfg.delay_minutes.max(1),
                );
            } else {
                immediate.push(cfg);
            }
        }

        if immediate.is_empty() {
            return;
        }

        tokio::spawn(async move {
            let results = webhook::WebhookForwarder::send(
                &immediate,
                &event,
                &source,
                &session_id,
                &language,
            )
            .await;
            for (id, result) in results {
                match result {
                    webhook::WebhookResult::Success => {
                        log::info!("Webhook {} delivered for session {}", id, session_id)
                    }
                    webhook::WebhookResult::Skipped => {}
                    webhook::WebhookResult::Failed(message) => {
                        log::warn!(
                            "Webhook {} failed for session {}: {}",
                            id,
                            session_id,
                            message
                        )
                    }
                }
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn schedule_delayed_webhook(
        config_store: ConfigStore,
        store: Arc<SessionStore>,
        webhook_id: String,
        event_key: String,
        event: NotificationEvent,
        source: String,
        session_id: String,
        pending_check: PendingWebhookCheck,
        delay_minutes: u32,
    ) {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(u64::from(delay_minutes) * 60)).await;

            if !Self::is_interaction_still_pending(&store, &session_id, &pending_check) {
                log::info!(
                    "Delayed webhook {} skipped; session {} was handled",
                    webhook_id,
                    session_id
                );
                return;
            }

            let app_config = config_store.get();
            let language = app_config.language;
            let cfg = app_config.webhook_configs.into_iter().find(|cfg| {
                cfg.id == webhook_id && cfg.delay_enabled && cfg.matches(&event_key, &source)
            });

            let Some(cfg) = cfg else {
                return;
            };

            let results =
                webhook::WebhookForwarder::send(&[cfg], &event, &source, &session_id, &language)
                    .await;
            for (id, result) in results {
                match result {
                    webhook::WebhookResult::Success => log::info!(
                        "Delayed webhook {} delivered for session {}",
                        id,
                        session_id
                    ),
                    webhook::WebhookResult::Skipped => {}
                    webhook::WebhookResult::Failed(message) => log::warn!(
                        "Delayed webhook {} failed for session {}: {}",
                        id,
                        session_id,
                        message
                    ),
                }
            }
        });
    }

    fn is_interaction_still_pending(
        store: &SessionStore,
        session_id: &str,
        pending_check: &PendingWebhookCheck,
    ) -> bool {
        let Some(session) = store.get_session(session_id) else {
            return false;
        };

        match pending_check {
            PendingWebhookCheck::Permission {
                tool_use_id,
                tool_name,
            } => session.pending_permission.as_ref().is_some_and(|pending| {
                if let Some(tool_use_id) = tool_use_id {
                    pending.tool_use_id.as_ref() == Some(tool_use_id)
                } else {
                    pending.tool_name == *tool_name
                }
            }),
            PendingWebhookCheck::Question { question } => session
                .pending_question
                .as_ref()
                .is_some_and(|pending| pending.question == *question),
            PendingWebhookCheck::Plan { title, content } => session
                .pending_plan
                .as_ref()
                .is_some_and(|pending| pending.title == *title && pending.content == *content),
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
            "heartbeat",
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

        let context = HookConnectionContext {
            pending: self.pending_permissions.clone(),
            pending_q: self.pending_questions.clone(),
            pending_plan: self.pending_plans.clone(),
            store: self.session_store.clone(),
            adapters: self.adapters.clone(),
            sound: self.sound_engine.clone(),
            app: self.app_handle.clone(),
            raw_events: self.raw_events.clone(),
            config_store: self.config_store.clone(),
            recent_tools: self.recent_tools.clone(),
        };

        #[cfg(unix)]
        {
            let unix_listener = Self::bind_unix(&endpoint.socket_path).await?;
            self.socket_owned.store(true, Ordering::SeqCst);
            let unix_context = context.clone();
            tokio::spawn(async move { Self::accept_unix(unix_listener, unix_context).await });
        }

        log::info!("Listening on TCP: {}", tcp_addr);
        let tcp_context = context.clone();
        tokio::spawn(async move { Self::accept_tcp(tcp_listener, tcp_context).await });

        #[cfg(unix)]
        log::info!(
            "HookServer started on {} and {}",
            endpoint.socket_path,
            tcp_addr
        );
        #[cfg(not(unix))]
        log::info!("HookServer started on {}", tcp_addr);
        Ok(())
    }

    #[cfg(unix)]
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

    #[cfg(unix)]
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
        let config_store = context.config_store;
        let recent_tools = context.recent_tools;
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

        // Try to find a matching adapter and parse the event
        let event = Self::parse_with_adapters(&adapters, &raw);
        if Self::should_silence_event(&config_store, &raw, event.as_ref()) {
            log::debug!(
                "Silenced hook event {} for cwd {}",
                Self::raw_event_name(&raw),
                raw.get("cwd")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
            );
            return Ok(());
        }
        Self::record_raw_event(&raw_events, &raw, event.as_ref());
        if let Some(ref agent_event) = event {
            Self::ensure_session_for_event(&store, agent_event, &raw);
        }
        Self::record_recent_tool_invocation(&recent_tools, event.as_ref(), &raw).await;

        match event {
            Some(AgentEvent::PermissionRequest {
                ref session_id,
                ref tool_name,
                ref diff,
                ref options,
            }) => {
                if Self::route_interaction_to_terminal_if_idle(
                    &config_store,
                    &store,
                    &raw,
                    session_id,
                    "permission",
                ) {
                    return Ok(());
                }

                // Check smart suppression: is the agent's terminal focused?
                let is_suppressed = Self::check_suppression(&store, session_id);

                // Only play sound if not suppressed
                if !is_suppressed {
                    Self::play_sound(&sound, SoundEvent::TaskConfirmation);
                }

                let resolved_tool_use_id = Self::resolve_permission_tool_use_id(
                    &recent_tools,
                    session_id,
                    tool_name,
                    &raw,
                )
                .await
                .unwrap_or_else(|| format!("permission:{}:{}", session_id, current_time_ms()));

                // Set pending permission on session
                store.set_pending_permission(
                    session_id,
                    Some(PendingPermission {
                        tool_use_id: Some(resolved_tool_use_id.clone()),
                        tool_name: tool_name.clone(),
                        tool_input: Self::permission_tool_input_from_raw(&raw),
                        diff: diff.clone(),
                        options: options.clone(),
                    }),
                );
                Self::update_session_metadata_from_raw(&store, &raw);
                Self::dispatch_webhook_event(
                    &config_store,
                    &store,
                    NotificationEvent::WaitingApproval {
                        tool_name: tool_name.clone(),
                        detail: Self::approval_detail_for_webhook(tool_name, &raw),
                    },
                    Self::source_for_webhook(&store, &raw, session_id, None),
                    session_id.clone(),
                    Some(PendingWebhookCheck::Permission {
                        tool_use_id: Some(resolved_tool_use_id.clone()),
                        tool_name: tool_name.clone(),
                    }),
                );

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
                    pending_map
                        .entry(resolved_tool_use_id.clone())
                        .or_default()
                        .push(PendingPermissionEntry {
                            session_id: session_id.clone(),
                            received_at_ms: current_time_ms(),
                            tx,
                        });
                }

                // Wait for the UI to respond. Codex uses a longer hook timeout for AgentBro
                // approvals; other agents keep the existing five-minute fallback window.
                let response =
                    tokio::time::timeout(Self::interaction_response_timeout(&raw), rx).await;

                match response {
                    Ok(Ok(reply)) => {
                        // Send response back to the hook script
                        let write_result = match serde_json::to_string(&reply.response) {
                            Ok(json) => {
                                let result = async {
                                    writer.write_all(json.as_bytes()).await?;
                                    writer.write_all(b"\n").await?;
                                    writer.flush().await
                                }
                                .await
                                .map_err(|err| err.to_string());
                                result
                            }
                            Err(err) => Err(err.to_string()),
                        };
                        let _ = reply.ack.send(write_result.clone());

                        if write_result.is_ok() {
                            // Play confirmation sound
                            Self::play_sound(&sound, SoundEvent::TaskConfirmation);

                            // Clear pending permission
                            store.set_pending_permission(session_id, None);
                            store.update_phase(session_id, SessionPhase::Processing);
                        } else if let Err(err) = write_result {
                            log::warn!(
                                "Permission response write failed for session {} tool {}: {}",
                                session_id,
                                resolved_tool_use_id,
                                err
                            );
                        }
                    }
                    _ => {
                        // Timeout or channel closed — let Claude Code handle it normally
                        let mut pending_map = pending.lock().await;
                        pending_map.remove(&resolved_tool_use_id);
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
                if Self::route_interaction_to_terminal_if_idle(
                    &config_store,
                    &store,
                    &raw,
                    session_id,
                    "question",
                ) {
                    return Ok(());
                }

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
                                id: None,
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
                        tool_use_id: None,
                        source: None,
                        response_mode: None,
                    }),
                );
                Self::update_session_metadata_from_raw(&store, &raw);
                Self::dispatch_webhook_event(
                    &config_store,
                    &store,
                    NotificationEvent::WaitingInput {
                        question: question.clone(),
                    },
                    Self::source_for_webhook(&store, &raw, session_id, None),
                    session_id.clone(),
                    Some(PendingWebhookCheck::Question {
                        question: question.clone(),
                    }),
                );

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
                if Self::route_interaction_to_terminal_if_idle(
                    &config_store,
                    &store,
                    &raw,
                    session_id,
                    "plan approval",
                ) {
                    return Ok(());
                }

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
                Self::dispatch_webhook_event(
                    &config_store,
                    &store,
                    NotificationEvent::PlanApproval {
                        title: title.clone(),
                    },
                    Self::source_for_webhook(&store, &raw, session_id, None),
                    session_id.clone(),
                    Some(PendingWebhookCheck::Plan {
                        title: title.clone(),
                        content: content.clone(),
                    }),
                );

                if is_suppressed {
                    store.emit_update_suppressed(true);
                    if let Ok(guard) = app.lock() {
                        if let Some(ref handle) = *guard {
                            crate::platform::notifications::send_plan_notification(handle, title);
                        }
                    }
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
                Self::process_event(agent_event, &raw, &store, &sound, &app, &config_store);
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

    pub fn recent_raw_event_summaries(&self, limit: usize) -> Vec<RawHookEventSummary> {
        self.raw_events
            .lock()
            .map(|events| events.recent_summaries(limit))
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
            .or_else(|| raw.get("eventType"))
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

    fn should_silence_event(
        config_store: &Arc<std::sync::Mutex<Option<ConfigStore>>>,
        raw: &serde_json::Value,
        event: Option<&AgentEvent>,
    ) -> bool {
        if Self::is_blocking_event(raw, event) {
            return false;
        }
        let Some(config_store) = Self::current_config_store(config_store) else {
            return false;
        };
        Self::config_silences_raw_event(&config_store.get(), raw)
    }

    fn is_blocking_event(raw: &serde_json::Value, event: Option<&AgentEvent>) -> bool {
        if matches!(
            event,
            Some(
                AgentEvent::PermissionRequest { .. }
                    | AgentEvent::AskQuestion { .. }
                    | AgentEvent::PlanApproval { .. }
                    | AgentEvent::Error { .. }
            )
        ) {
            return true;
        }
        matches!(
            Self::raw_event_name(raw),
            "PermissionRequest" | "permission_request" | "PlanApproval" | "AskQuestion"
        ) || matches!(
            raw.get("status").and_then(|value| value.as_str()),
            Some("waiting_for_approval" | "waiting_for_input")
        )
    }

    fn config_silences_raw_event(config: &AppConfig, raw: &serde_json::Value) -> bool {
        let cwd = Self::normalized_silence_text(raw.get("cwd").and_then(|value| value.as_str()));
        if !cwd.is_empty()
            && config
                .excluded_hook_cwd_substrings
                .split(['\n', ','])
                .map(|item| Self::normalized_silence_text(Some(item)))
                .filter(|pattern| !pattern.is_empty())
                .any(|pattern| cwd.contains(&pattern))
        {
            return true;
        }

        let prompt = Self::normalized_silence_text(
            raw.get("prompt")
                .or_else(|| raw.get("user_prompt"))
                .or_else(|| raw.get("message"))
                .and_then(|value| value.as_str()),
        );
        config.session_silence_rules.iter().any(|rule| {
            if !rule.enabled {
                return false;
            }
            let pattern = Self::normalized_silence_text(Some(rule.pattern.as_str()));
            if pattern.is_empty() {
                return false;
            }
            match rule.kind.as_str() {
                "cwd" => !cwd.is_empty() && cwd.contains(&pattern),
                "prompt" => !prompt.is_empty() && prompt.starts_with(&pattern),
                _ => false,
            }
        })
    }

    fn normalized_silence_text(value: Option<&str>) -> String {
        value
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    fn raw_event_name(raw: &serde_json::Value) -> &str {
        raw.get("event")
            .or_else(|| raw.get("hook_event_name"))
            .or_else(|| raw.get("hookEventName"))
            .or_else(|| raw.get("eventType"))
            .or_else(|| raw.get("type"))
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
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
        event: &AgentEvent,
        _raw: &serde_json::Value,
        store: &Arc<SessionStore>,
        sound: &Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
        app: &Arc<std::sync::Mutex<Option<tauri::AppHandle>>>,
        config_store: &Arc<std::sync::Mutex<Option<ConfigStore>>>,
    ) {
        Self::update_session_metadata_from_raw(store, _raw);

        let done_cleanup_secs = config_store
            .lock()
            .ok()
            .and_then(|guard| {
                guard
                    .as_ref()
                    .map(|c| c.get().idle_timeout_minutes as u64 * 60)
            })
            .unwrap_or(DONE_SESSION_HISTORY_CLEANUP_SECS);

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
                Self::dispatch_webhook_event(
                    config_store,
                    store,
                    NotificationEvent::SessionStart,
                    agent_type.clone(),
                    session_id.clone(),
                    None,
                );
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
                    Self::play_sound_for_session(
                        sound,
                        store,
                        session_id,
                        SoundEvent::ContextLimit,
                    );
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
                if event_name == "UserPromptSubmit" || event_name == "BeforeAgent" {
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

                    let should_replace_title = store
                        .get_session(session_id)
                        .map(|s| Self::should_replace_session_title(s.session_title.as_deref()))
                        .unwrap_or(true);
                    if should_replace_title {
                        // Try to get prompt from the raw event data
                        if let Some(title) = Self::extract_user_prompt_title(_raw, 80) {
                            store.update_session(session_id, |s| {
                                s.session_title = Some(title);
                            });
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
                if status != "running" {
                    Self::clear_pending_permission_after_tool_resolution(store, session_id, _raw);
                }

                if Self::is_subagent_tool(tool_name)
                    && Self::is_codex_session(store, session_id, _raw)
                {
                    Self::refresh_subagents_from_transcript(store, session_id, _raw);
                }

                if Self::is_subagent_tool(tool_name) && !tool_use_id.is_empty() {
                    match status.as_str() {
                        "running" => {
                            let metadata =
                                Self::subagent_metadata_from_tool_input(_raw, tool_input);
                            let transcript_path =
                                Self::transcript_path_for_session(store, session_id, _raw)
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
                            let metadata =
                                Self::subagent_metadata_from_tool_input(_raw, tool_input);
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
                if is_suppressed || Self::is_remote_hook_event(_raw) {
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
                Self::dispatch_webhook_event(
                    config_store,
                    store,
                    NotificationEvent::Completion {
                        summary: summary.clone(),
                    },
                    Self::source_for_webhook(store, _raw, session_id, None),
                    session_id.clone(),
                    None,
                );
                Self::schedule_done_session_cleanup(store, session_id, done_cleanup_secs);
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
                    if s.phase == SessionPhase::Ready {
                        s.run_state.status = AgentRunStatus::Completed;
                        s.run_state.phase = Some("done".to_string());
                        s.run_state.current_action = Some(truncated.clone());
                        s.run_state.updated_at = chrono::Utc::now().to_rfc3339();
                    }
                });
                Self::refresh_cache_ttl_from_transcript(store, session_id, _raw);
                Self::refresh_subagents_from_transcript(store, session_id, _raw);
                let is_suppressed = Self::check_suppression(store, session_id);
                if is_suppressed || Self::is_remote_hook_event(_raw) {
                    if let Ok(guard) = app.lock() {
                        if let Some(ref handle) = *guard {
                            crate::platform::notifications::send_completion_notification(
                                handle, &truncated,
                            );
                        }
                    }
                }
                Self::play_sound_for_session(sound, store, session_id, SoundEvent::TaskComplete);
                Self::dispatch_webhook_event(
                    config_store,
                    store,
                    NotificationEvent::Completion {
                        summary: truncated.clone(),
                    },
                    Self::source_for_webhook(store, _raw, session_id, None),
                    session_id.clone(),
                    None,
                );
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
                Self::dispatch_webhook_event(
                    config_store,
                    store,
                    NotificationEvent::Error {
                        message: message.clone(),
                    },
                    Self::source_for_webhook(store, _raw, session_id, None),
                    session_id.clone(),
                    None,
                );
                Self::schedule_done_session_cleanup(store, session_id, done_cleanup_secs);
            }
            AgentEvent::Interrupt { session_id } => {
                store.update_phase(session_id, SessionPhase::Interrupted);
                Self::schedule_done_session_cleanup(store, session_id, done_cleanup_secs);
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
                Self::process_notification(
                    store,
                    session_id,
                    message,
                    status,
                    sound,
                    _raw,
                    done_cleanup_secs,
                );
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
                // Ensure parent session exists (may not exist yet due to event ordering)
                store.get_or_create_session(session_id, "opencode", "", "", "");
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
        let project = crate::agents::project_name_from_path(cwd);

        // Ensure session exists
        store.get_or_create_session(session_id, "claude-code", &project, cwd, "");
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

            let should_replace_title = store
                .get_session(session_id)
                .map(|s| Self::should_replace_session_title(s.session_title.as_deref()))
                .unwrap_or(true);
            if should_replace_title {
                if let Some(title) = Self::extract_user_prompt_title(raw, 80) {
                    store.update_session(session_id, |s| {
                        s.session_title = Some(title);
                    });
                }
            }
        }

        // Update local pid/tty if present. Remote hooks report the agent PID from the
        // remote machine, which cannot be validated against the local process tree.
        if !Self::is_remote_hook_event(raw) {
            if let Some(pid) = raw.get("pid").and_then(|v| v.as_u64()) {
                store.update_session(session_id, |s| {
                    s.pid = Some(pid as u32);
                });
            }
        }
        if let Some(tty) = raw.get("tty").and_then(|v| v.as_str()) {
            store.update_session(session_id, |s| {
                s.tty = Some(tty.to_string());
            });
        }

        if status == "waiting_for_input"
            && Self::try_set_codex_rollout_pending_question(store, session_id, sound, raw)
        {
            return;
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
        done_cleanup_secs: u64,
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
            Self::schedule_done_session_cleanup(store, session_id, done_cleanup_secs);
            return;
        }

        if status.as_deref() == Some("waiting_for_input") {
            if Self::try_set_codex_rollout_pending_question(store, session_id, sound, raw) {
                return;
            }
            store.update_session(session_id, |s| {
                s.phase = SessionPhase::Ready;
                s.description = Some("Waiting for input".to_string());
            });
        }
    }

    fn try_set_codex_rollout_pending_question(
        store: &SessionStore,
        session_id: &str,
        sound: &Arc<std::sync::Mutex<Option<Arc<SoundEngine>>>>,
        raw: &serde_json::Value,
    ) -> bool {
        let is_codex = raw
            .get("agent")
            .and_then(|value| value.as_str())
            .is_some_and(|agent| agent == "codex" || agent == "openai.codex")
            || store
                .get_session(session_id)
                .is_some_and(|session| session.agent_type == "codex");
        if !is_codex {
            return false;
        }

        let Some(path) = Self::transcript_path_for_session(store, session_id, raw)
            .or_else(|| discover_codex_session_file(session_id))
        else {
            return false;
        };
        let Some(pending) = extract_pending_codex_user_input(&path) else {
            return false;
        };

        let already_showing = store
            .get_session(session_id)
            .and_then(|session| session.pending_question)
            .and_then(|question| question.tool_use_id)
            .is_some_and(|tool_use_id| tool_use_id == pending.call_id);

        if !already_showing && !Self::check_suppression(store, session_id) {
            Self::play_sound(sound, SoundEvent::TaskConfirmation);
        }

        store.set_pending_question(
            session_id,
            Some(Self::pending_question_from_codex_user_input(pending)),
        );
        true
    }

    fn pending_question_from_codex_user_input(pending: CodexPendingUserInput) -> PendingQuestion {
        PendingQuestion {
            question: pending.question,
            options: pending.options,
            descriptions: pending.descriptions,
            header: pending.header,
            multi_select: pending.multi_select,
            questions: pending
                .questions
                .into_iter()
                .map(|question| PendingQuestionItem {
                    id: question.id,
                    question: question.question,
                    header: question.header,
                    options: question
                        .options
                        .into_iter()
                        .map(|option| PendingQuestionOption {
                            label: option.label,
                            description: option.description,
                        })
                        .collect(),
                    multi_select: question.multi_select,
                })
                .collect(),
            tool_use_id: Some(pending.call_id),
            source: Some("codex_rollout_request_user_input".to_string()),
            response_mode: Some("external_only".to_string()),
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
        let project = if cwd.trim().is_empty() {
            "Unknown".to_string()
        } else {
            crate::agents::project_name_from_path(cwd)
        };
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

        store.get_or_create_session(session_id, agent_type, &project, cwd, terminal);
    }

    fn schedule_done_session_cleanup(store: &SessionStore, session_id: &str, delay_secs: u64) {
        let store_for_cleanup = store.clone();
        let session_id_for_cleanup = session_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
            let should_remove = store_for_cleanup
                .get_session(&session_id_for_cleanup)
                .map(|session| {
                    (session.phase == SessionPhase::Done
                        || session.phase == SessionPhase::Error
                        || session.phase == SessionPhase::Interrupted)
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

    fn extract_user_prompt_title(raw: &serde_json::Value, max_chars: usize) -> Option<String> {
        raw.get("prompt")
            .or_else(|| raw.get("user_prompt"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.lines().find(|line| !line.trim().is_empty()))
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

    fn should_replace_session_title(title: Option<&str>) -> bool {
        title.map(Self::is_internal_codex_title).unwrap_or(true)
    }

    fn is_internal_codex_title(title: &str) -> bool {
        let normalized = title
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        normalized.starts_with("<environment_context>")
            || normalized.starts_with(
                "you are a helpful assistant. you will be presented with a user prompt",
            )
            || normalized.starts_with("you are codex, a coding agent")
            || normalized.starts_with("you are an ai assistant accessed via an api")
    }

    fn is_compacting_context_text(text: &str) -> bool {
        let normalized = text
            .trim()
            .trim_end_matches(['.', '!', '。', '！'])
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
            .trim_end_matches(['.', '!', '。', '！'])
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
        if raw.is_some_and(Self::is_remote_hook_event) {
            return Self::useful_completion_text(Some(incoming))
                .unwrap_or_else(|| fallback.to_string());
        }

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
        let normalized = tool_name
            .trim()
            .trim_start_matches("functions.")
            .trim_start_matches("multi_agent_v1.")
            .replace(['-', ' '], "_")
            .to_lowercase();
        matches!(
            normalized.as_str(),
            "agent"
                | "task"
                | "spawn_agent"
                | "send_input"
                | "wait_agent"
                | "close_agent"
                | "resume_agent"
        )
    }

    fn is_codex_session(store: &SessionStore, session_id: &str, raw: &serde_json::Value) -> bool {
        raw.get("agent")
            .and_then(|value| value.as_str())
            .is_some_and(|agent| matches!(agent, "codex" | "openai.codex"))
            || store
                .get_session(session_id)
                .is_some_and(|session| session.agent_type == "codex")
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

        let pid = if Self::is_remote_hook_event(raw) {
            None
        } else {
            raw.get("pid").and_then(|v| v.as_u64()).map(|v| v as u32)
        };
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
        let remote_host_id = optional_nonempty_string(raw, "_remote_host_id");
        let remote_host_name = optional_nonempty_string(raw, "_remote_host_name");
        let model = optional_nonempty_string(raw, "model");

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
            && remote_host_id.is_none()
            && remote_host_name.is_none()
            && model.is_none()
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
            if let Some(value) = remote_host_id {
                s.remote_host_id = Some(value.to_string());
            }
            if let Some(value) = remote_host_name {
                s.remote_host_name = Some(value.to_string());
            }
            if let Some(value) = model {
                s.model = Some(value.to_string());
            }
        });
    }

    fn is_remote_hook_event(raw: &serde_json::Value) -> bool {
        raw.get("_remote_host_id").is_some() || raw.get("_remote_host_name").is_some()
    }

    fn route_interaction_to_terminal_if_idle(
        config_store: &Arc<std::sync::Mutex<Option<ConfigStore>>>,
        store: &Arc<SessionStore>,
        raw: &serde_json::Value,
        session_id: &str,
        interaction_kind: &str,
    ) -> bool {
        let Some(idle_seconds) = Self::idle_interaction_route_seconds(config_store, raw) else {
            return false;
        };

        Self::update_session_metadata_from_raw(store, raw);
        store.emit_update_suppressed(true);
        log::info!(
            "Routing {} interaction for session {} back to terminal after {}s of user idle time",
            interaction_kind,
            session_id,
            idle_seconds
        );
        true
    }

    fn idle_interaction_route_seconds(
        config_store: &Arc<std::sync::Mutex<Option<ConfigStore>>>,
        raw: &serde_json::Value,
    ) -> Option<u64> {
        let config_store = Self::current_config_store(config_store)?;
        let config = config_store.get();
        let idle_seconds = crate::platform::idle::user_idle_seconds()?;
        if Self::idle_interaction_should_route(&config, raw, Some(idle_seconds)) {
            Some(idle_seconds)
        } else {
            None
        }
    }

    fn idle_interaction_should_route(
        config: &AppConfig,
        raw: &serde_json::Value,
        idle_seconds: Option<u64>,
    ) -> bool {
        if !config.idle_interaction_routing_enabled
            || config.idle_interaction_routing_minutes == 0
            || Self::is_remote_hook_event(raw)
        {
            return false;
        }

        let Some(idle_seconds) = idle_seconds else {
            return false;
        };
        let threshold = u64::from(config.idle_interaction_routing_minutes) * 60;
        idle_seconds >= threshold
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

        if let Some(existing) = session.subagents.iter_mut().find(|subagent| {
            subagent.agent_id == incoming.agent_id
                || launch_tool_use_id
                    .as_deref()
                    .is_some_and(|tool_use_id| subagent.agent_id == tool_use_id)
        }) {
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
        self.send_pending_permission_response(session_id, response)
            .await
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
        let response = PermissionResponse {
            decision: "auto".to_string(),
            reason: None,
            always: None,
        };
        self.send_pending_permission_response(session_id, response)
            .await
    }

    async fn send_pending_permission_response(
        &self,
        session_id: &str,
        response: PermissionResponse,
    ) -> anyhow::Result<()> {
        let entries = {
            let mut pending_map = self.pending_permissions.lock().await;
            let Some(key) = Self::latest_pending_permission_key(&pending_map, session_id) else {
                anyhow::bail!("No pending permission for session {}", session_id);
            };
            pending_map.remove(&key).unwrap_or_default()
        };

        let mut ack_receivers = Vec::new();
        let mut send_failures = 0usize;
        for entry in entries {
            let (ack_tx, ack_rx) = oneshot::channel();
            let reply = PermissionReply {
                response: response.clone(),
                ack: ack_tx,
            };
            if entry.tx.send(reply).is_ok() {
                ack_receivers.push(ack_rx);
            } else {
                send_failures += 1;
            }
        }

        if ack_receivers.is_empty() {
            anyhow::bail!(
                "No active permission hook receivers for session {} ({} send failures)",
                session_id,
                send_failures
            );
        }

        let mut successes = 0usize;
        let mut failures = Vec::new();
        for ack in ack_receivers {
            match tokio::time::timeout(Duration::from_secs(5), ack).await {
                Ok(Ok(Ok(()))) => successes += 1,
                Ok(Ok(Err(err))) => failures.push(err),
                Ok(Err(_)) => {
                    failures.push("permission hook receiver dropped before write ack".into())
                }
                Err(_) => failures.push("permission hook write ack timed out".into()),
            }
        }

        if successes > 0 {
            if !failures.is_empty() {
                log::warn!(
                    "Permission response for session {} had {} successful hook write(s) and {} failure(s): {}",
                    session_id,
                    successes,
                    failures.len(),
                    failures.join("; ")
                );
            }
            Ok(())
        } else {
            anyhow::bail!(
                "Permission response failed to reach hook for session {}: {}",
                session_id,
                failures.join("; ")
            )
        }
    }

    fn latest_pending_permission_key(
        pending_map: &HashMap<String, Vec<PendingPermissionEntry>>,
        session_id: &str,
    ) -> Option<String> {
        pending_map
            .iter()
            .filter_map(|(key, entries)| {
                entries
                    .iter()
                    .filter(|entry| entry.session_id == session_id)
                    .map(|entry| (key, entry.received_at_ms))
                    .max_by_key(|(_, received_at_ms)| *received_at_ms)
            })
            .max_by_key(|(_, received_at_ms)| *received_at_ms)
            .map(|(key, _)| key.clone())
    }
}

fn canonical_agent_id(agent: &str) -> &str {
    match agent {
        "codybuddycn" => "codebuddycn",
        "claude" => "claude-code",
        "openai.codex" => "codex",
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
        let store = Arc::new(SessionStore::new());
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
    fn ensure_session_uses_basename_for_windows_cwd() {
        let store = Arc::new(SessionStore::new());
        let raw = serde_json::json!({
            "agent": "codex",
            "session_id": "codex-windows-session",
            "cwd": r"C:\Users\admin\Documents\github\agentbro",
            "tty": ""
        });
        let event = AgentEvent::Processing {
            session_id: "codex-windows-session".to_string(),
            description: "Processing user input".to_string(),
        };

        HookServer::ensure_session_for_event(&store, &event, &raw);

        let session = store
            .get_session("codex-windows-session")
            .expect("session should be created");
        assert_eq!(session.project, "agentbro");
        assert_eq!(session.cwd, r"C:\Users\admin\Documents\github\agentbro");
    }

    #[test]
    fn canonical_agent_id_maps_codex_aliases() {
        assert_eq!(canonical_agent_id("openai.codex"), "codex");
        assert_eq!(canonical_agent_id("codex"), "codex");
    }

    #[test]
    fn parse_with_adapters_routes_openai_codex_to_codex_adapter() {
        let adapters: Vec<Arc<dyn AgentAdapter>> =
            vec![Arc::new(crate::agents::codex::CodexAdapter::new())];
        let raw = serde_json::json!({
            "agent": "openai.codex",
            "event": "SessionStart",
            "session_id": "codex-alias-session",
            "cwd": "/tmp/agentbro"
        });

        let event = HookServer::parse_with_adapters(&adapters, &raw).expect("codex alias parses");

        match event {
            AgentEvent::SessionStart {
                session_id,
                agent_type,
                ..
            } => {
                assert_eq!(session_id, "codex-alias-session");
                assert_eq!(agent_type, "codex");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn user_prompt_submit_replaces_codex_environment_context_title() {
        let store = Arc::new(SessionStore::new());
        let raw = serde_json::json!({
            "agent": "codex",
            "session_id": "codex-title-session",
            "cwd": "/tmp/agentbro",
            "event": "UserPromptSubmit",
            "prompt": "Build AgentBro landing page"
        });
        let event = AgentEvent::Processing {
            session_id: "codex-title-session".to_string(),
            description: "Processing user input".to_string(),
        };
        let sound = Arc::new(std::sync::Mutex::new(None));
        let app = Arc::new(std::sync::Mutex::new(None));
        let config_store = Arc::new(std::sync::Mutex::new(None));

        HookServer::ensure_session_for_event(&store, &event, &raw);
        store.update_session("codex-title-session", |session| {
            session.session_title = Some(
                "<environment_context>\n  <cwd>/tmp/agentbro</cwd>\n</environment_context>"
                    .to_string(),
            );
        });
        HookServer::process_event(&event, &raw, &store, &sound, &app, &config_store);

        let session = store
            .get_session("codex-title-session")
            .expect("session should exist");
        assert_eq!(
            session.session_title.as_deref(),
            Some("Build AgentBro landing page")
        );
        assert_eq!(
            session.last_user_message.as_deref(),
            Some("Build AgentBro landing page")
        );
    }

    #[test]
    fn remote_hook_pid_is_not_stored_as_local_process() {
        let store = Arc::new(SessionStore::new());
        let raw = serde_json::json!({
            "agent": "claude-code",
            "session_id": "remote-mid-session",
            "cwd": "/tmp/remote-project",
            "event": "UserPromptSubmit",
            "status": "processing",
            "prompt": "remote prompt",
            "pid": u32::MAX,
            "_remote_host_id": "host-1",
            "_remote_host_name": "Remote"
        });
        let event = AgentEvent::Processing {
            session_id: "remote-mid-session".to_string(),
            description: "Processing user input: remote prompt".to_string(),
        };
        let sound = Arc::new(std::sync::Mutex::new(None));
        let app = Arc::new(std::sync::Mutex::new(None));
        let config_store = Arc::new(std::sync::Mutex::new(None));

        HookServer::ensure_session_for_event(&store, &event, &raw);
        HookServer::process_event(&event, &raw, &store, &sound, &app, &config_store);

        let sessions = store.get_all_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "remote-mid-session");
        assert_eq!(sessions[0].pid, None);
        assert_eq!(sessions[0].remote_host_id.as_deref(), Some("host-1"));
        assert_eq!(sessions[0].remote_host_name.as_deref(), Some("Remote"));
        assert_eq!(sessions[0].phase, SessionPhase::Processing);
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
        let raw = serde_json::json!({ "agent": "gemini" });

        assert_eq!(
            HookServer::interaction_response_timeout(&raw),
            Duration::from_secs(DEFAULT_INTERACTION_RESPONSE_TIMEOUT_SECS)
        );
    }

    #[test]
    fn opencode_interaction_timeout_uses_long_window() {
        let raw = serde_json::json!({ "agent": "opencode" });

        assert_eq!(
            HookServer::interaction_response_timeout(&raw),
            Duration::from_secs(HUMAN_INTERACTION_RESPONSE_TIMEOUT_SECS)
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

    #[tokio::test]
    async fn permission_request_resolves_recent_pre_tool_use_id_without_explicit_id() {
        let recent_tools = Arc::new(Mutex::new(VecDeque::new()));
        let pre_raw = serde_json::json!({
            "event": "PreToolUse",
            "session_id": "codex-s1",
            "tool_name": "Bash",
            "tool_use_id": "call-date-1",
            "tool_input": { "command": "date" }
        });
        let pre_event = AgentEvent::ToolUse {
            session_id: "codex-s1".to_string(),
            tool_name: "Bash".to_string(),
            tool_input: "{\"command\":\"date\"}".to_string(),
            tool_target: None,
            status: "running".to_string(),
        };

        HookServer::record_recent_tool_invocation(&recent_tools, Some(&pre_event), &pre_raw).await;

        let permission_raw = serde_json::json!({
            "event": "PermissionRequest",
            "session_id": "codex-s1",
            "tool_name": "Bash",
            "tool_input": {
                "command": "date",
                "description": "Show the current time."
            }
        });

        assert_eq!(
            HookServer::resolve_permission_tool_use_id(
                &recent_tools,
                "codex-s1",
                "Bash",
                &permission_raw
            )
            .await
            .as_deref(),
            Some("call-date-1")
        );
    }

    #[tokio::test]
    async fn handle_connection_sets_pending_permission_for_windows_hook_event() {
        let store = Arc::new(SessionStore::new());
        let adapters: Vec<Arc<dyn AgentAdapter>> = vec![Arc::new(
            crate::agents::claude_code::ClaudeCodeAdapter::new(),
        )];
        let context = HookConnectionContext {
            pending: Arc::new(Mutex::new(HashMap::new())),
            pending_q: Arc::new(Mutex::new(HashMap::new())),
            pending_plan: Arc::new(Mutex::new(HashMap::new())),
            store: store.clone(),
            adapters: Arc::new(adapters),
            sound: Arc::new(std::sync::Mutex::new(None)),
            app: Arc::new(std::sync::Mutex::new(None)),
            raw_events: Arc::new(std::sync::Mutex::new(RawHookEventStore::new())),
            config_store: Arc::new(std::sync::Mutex::new(None)),
            recent_tools: Arc::new(Mutex::new(VecDeque::new())),
        };
        let (mut client, server) = tokio::io::duplex(4096);
        let task = tokio::spawn(async move {
            let _ = HookServer::handle_connection(server, context).await;
        });

        client
            .write_all(
                br#"{"agent":"claude-code","event":"PermissionRequest","session_id":"win-hook","cwd":"C:\\Users\\admin\\Documents\\github\\agentbro","tool_name":"Bash","tool_input":{"command":"echo AgentBro Windows smoke"}}"#,
            )
            .await
            .unwrap();
        client.write_all(b"\n").await.unwrap();
        client.flush().await.unwrap();

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if store
                    .get_session("win-hook")
                    .and_then(|session| session.pending_permission)
                    .is_some()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("pending permission should be set");

        let session = store.get_session("win-hook").expect("session should exist");
        assert_eq!(session.project, "agentbro");
        assert_eq!(session.phase, SessionPhase::WaitingApproval);
        let pending = session
            .pending_permission
            .expect("permission should stay pending while hook waits");
        assert_eq!(pending.tool_name, "Bash");
        assert!(pending.tool_input.contains("AgentBro Windows smoke"));

        task.abort();
    }

    #[tokio::test]
    async fn handle_connection_sets_pending_permission_for_workbuddy_safety_event() {
        let store = Arc::new(SessionStore::new());
        let adapters: Vec<Arc<dyn AgentAdapter>> =
            vec![Arc::new(crate::agents::workbuddy::WorkBuddyAdapter::new())];
        let context = HookConnectionContext {
            pending: Arc::new(Mutex::new(HashMap::new())),
            pending_q: Arc::new(Mutex::new(HashMap::new())),
            pending_plan: Arc::new(Mutex::new(HashMap::new())),
            store: store.clone(),
            adapters: Arc::new(adapters),
            sound: Arc::new(std::sync::Mutex::new(None)),
            app: Arc::new(std::sync::Mutex::new(None)),
            raw_events: Arc::new(std::sync::Mutex::new(RawHookEventStore::new())),
            config_store: Arc::new(std::sync::Mutex::new(None)),
            recent_tools: Arc::new(Mutex::new(VecDeque::new())),
        };
        let (mut client, server) = tokio::io::duplex(4096);
        let task = tokio::spawn(async move {
            let _ = HookServer::handle_connection(server, context).await;
        });

        client
            .write_all(
                br#"{"agent":"workbuddy","sessionId":"workbuddy-safety","cwd":"/Users/me/project","toolCallId":"toolu_1","category":"file-safety","eventType":"file-safety.needs-approval","decision":"info","messageKey":"securityCenter.audit.fileSafety.needsApproval.write","messageParams":{"paths":"/Users/me/Desktop/out"}}"#,
            )
            .await
            .unwrap();
        client.write_all(b"\n").await.unwrap();
        client.flush().await.unwrap();

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if store
                    .get_session("workbuddy-safety")
                    .and_then(|session| session.pending_permission)
                    .is_some()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("workbuddy safety approval should be set");

        let session = store
            .get_session("workbuddy-safety")
            .expect("session should exist");
        assert_eq!(session.agent_type, "workbuddy");
        assert_eq!(session.phase, SessionPhase::WaitingApproval);
        let pending = session
            .pending_permission
            .expect("permission should stay pending while hook waits");
        assert_eq!(pending.tool_name, "File Safety");
        assert_eq!(pending.tool_use_id.as_deref(), Some("toolu_1"));
        assert!(pending.tool_input.contains("/Users/me/Desktop/out"));

        task.abort();
    }

    #[test]
    fn permission_tool_input_synthesizes_workbuddy_audit_context() {
        let raw = serde_json::json!({
            "agent": "workbuddy",
            "sessionId": "s1",
            "toolCallId": "toolu_1",
            "category": "command-safety",
            "eventType": "command-safety.needs-approval",
            "commandPreview": "pwd && mkdir -p ~/Desktop/out",
            "messageParams": {
                "command": "pwd && mkdir -p ~/Desktop/out"
            }
        });

        let input = HookServer::permission_tool_input_from_raw(&raw);
        assert!(input.contains("pwd && mkdir -p"));
        assert!(input.contains("command-safety"));
        assert_eq!(
            HookServer::raw_tool_use_id(&raw).as_deref(),
            Some("toolu_1")
        );
    }

    #[test]
    fn workbuddy_permission_resolution_clears_matching_pending_permission() {
        let store = Arc::new(SessionStore::new());
        store.get_or_create_session("s1", "workbuddy", "project", "/tmp/project", "");
        store.set_pending_permission(
            "s1",
            Some(PendingPermission {
                tool_use_id: Some("toolu_1".to_string()),
                tool_name: "File Safety".to_string(),
                tool_input: "{}".to_string(),
                diff: None,
                options: None,
            }),
        );
        let sound = Arc::new(std::sync::Mutex::new(None));
        let app = Arc::new(std::sync::Mutex::new(None));
        let config_store = Arc::new(std::sync::Mutex::new(None));
        let raw = serde_json::json!({
            "agent": "workbuddy",
            "sessionId": "s1",
            "toolCallId": "toolu_1",
            "category": "file-safety",
            "eventType": "file-safety.approved",
            "decision": "approved"
        });
        let event = AgentEvent::ToolUse {
            session_id: "s1".to_string(),
            tool_name: "File Safety".to_string(),
            tool_input: "{}".to_string(),
            tool_target: None,
            status: "success".to_string(),
        };

        HookServer::process_event(&event, &raw, &store, &sound, &app, &config_store);

        let session = store.get_session("s1").expect("session should exist");
        assert!(session.pending_permission.is_none());
        assert_eq!(session.phase, SessionPhase::Processing);
    }

    #[test]
    fn permission_signature_ignores_human_description() {
        let pre = serde_json::json!({ "tool_input": { "command": "pwd" } });
        let permission = serde_json::json!({
            "tool_input": {
                "description": "Show the current directory.",
                "command": "pwd"
            }
        });

        assert_eq!(
            HookServer::tool_input_signature_from_raw(&pre),
            HookServer::tool_input_signature_from_raw(&permission)
        );
    }

    #[test]
    fn silence_config_matches_cwd_but_blocking_event_is_protected() {
        let mut config = AppConfig::default();
        config.excluded_hook_cwd_substrings = "/tmp/my-project".to_string();
        let raw = serde_json::json!({
            "event": "PermissionRequest",
            "status": "waiting_for_approval",
            "cwd": "/tmp/my-project",
            "session_id": "s1"
        });
        let event = AgentEvent::PermissionRequest {
            session_id: "s1".to_string(),
            tool_name: "Bash".to_string(),
            diff: None,
            options: None,
        };

        assert!(HookServer::config_silences_raw_event(&config, &raw));
        assert!(HookServer::is_blocking_event(&raw, Some(&event)));
    }

    #[test]
    fn idle_interaction_routing_requires_enabled_local_idle_session() {
        let raw = serde_json::json!({ "session_id": "s1", "cwd": "/tmp/project" });
        let mut config = AppConfig::default();
        config.idle_interaction_routing_minutes = 5;

        assert!(!HookServer::idle_interaction_should_route(
            &config,
            &raw,
            Some(600)
        ));

        config.idle_interaction_routing_enabled = true;
        assert!(!HookServer::idle_interaction_should_route(
            &config,
            &raw,
            Some(299)
        ));
        assert!(HookServer::idle_interaction_should_route(
            &config,
            &raw,
            Some(300)
        ));
    }

    #[test]
    fn idle_interaction_routing_skips_remote_sessions() {
        let raw = serde_json::json!({
            "session_id": "remote",
            "_remote_host_id": "host-1"
        });
        let mut config = AppConfig::default();
        config.idle_interaction_routing_enabled = true;
        config.idle_interaction_routing_minutes = 1;

        assert!(!HookServer::idle_interaction_should_route(
            &config,
            &raw,
            Some(3600)
        ));
    }

    #[test]
    fn latest_pending_permission_key_prefers_newest_entry_for_session() {
        let mut pending = HashMap::new();
        let (old_tx, _old_rx) = oneshot::channel();
        let (new_tx, _new_rx) = oneshot::channel();
        pending.insert(
            "old-tool".to_string(),
            vec![PendingPermissionEntry {
                session_id: "s1".to_string(),
                received_at_ms: 10,
                tx: old_tx,
            }],
        );
        pending.insert(
            "new-tool".to_string(),
            vec![PendingPermissionEntry {
                session_id: "s1".to_string(),
                received_at_ms: 20,
                tx: new_tx,
            }],
        );

        assert_eq!(
            HookServer::latest_pending_permission_key(&pending, "s1").as_deref(),
            Some("new-tool")
        );
    }

    #[test]
    fn completion_summary_ignores_processing_prompt_preview() {
        let store = Arc::new(SessionStore::new());
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
        let store = Arc::new(SessionStore::new());
        store.get_or_create_session(
            "compact-session",
            "claude-code",
            "project",
            "/tmp/project",
            "iTerm",
        );
        let sound = Arc::new(std::sync::Mutex::new(None));
        let app = Arc::new(std::sync::Mutex::new(None));
        let config_store = Arc::new(std::sync::Mutex::new(None));

        let pre = AgentEvent::Processing {
            session_id: "compact-session".to_string(),
            description: "Compacting context".to_string(),
        };
        HookServer::process_event(
            &pre,
            &serde_json::json!({ "event": "PreCompact" }),
            &store,
            &sound,
            &app,
            &config_store,
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
            &post,
            &serde_json::json!({ "event": "PostCompact" }),
            &store,
            &sound,
            &app,
            &config_store,
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
