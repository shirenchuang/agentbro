// Tauri IPC Commands — Bridge between frontend and Rust backend

pub mod buddy;
pub mod monitor;
pub mod persistence;

use crate::agents::{AdapterInfo, AgentAdapter};
use crate::config::{AppConfig, ConfigStore};
use crate::energy::{self, EnergyMode};
use crate::hook_endpoint;
use crate::hooks::conversation_parser::{
    all_projects_dirs, discover_codex_session_file, discover_session_file_in_dirs,
    extract_subagents_from_transcript, ChatRole, MessageBlock, ParsedMessage,
    TranscriptSubagentInfo,
};
use crate::hooks::diagnostics::DiagnosticRingBuffer;
use crate::hooks::file_watcher::ConversationWatcher;
use crate::hooks::server::{HookServer, RawHookEvent};
use crate::hooks::session_store::{
    PendingQuestion, RateLimitInfo, SessionPhase, SessionState, SessionStore, SubagentInfo,
    UsageRateWindow,
};
use crate::network_monitor::NetworkMonitor;
use crate::platform::display_controller::DisplayController;
use crate::remote::{ConnectionStatus, RemoteHost, RemoteManager};
use crate::sound::SoundEngine;
use crate::switch::db::SwitchDatabase;
use crate::telemetry::TelemetryService;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader as StdBufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tauri::{Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, ChildStdin, ChildStdout, Command as TokioCommand};
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type CodexWsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type CodexWsSink = SplitSink<CodexWsStream, Message>;
type CodexWsSource = SplitStream<CodexWsStream>;

/// Shared app state accessible from Tauri commands
pub struct AppState {
    pub session_store: Arc<SessionStore>,
    pub hook_server: Arc<HookServer>,
    pub codex_app_server: Arc<CodexAppServerBridge>,
    pub config_store: ConfigStore,
    pub adapters: Vec<Arc<dyn AgentAdapter>>,
    pub sound_engine: Option<Arc<SoundEngine>>,
    /// Conversation file watcher — watches JSONL files for real-time chat updates.
    /// Wrapped in Mutex because RecommendedWatcher is not Sync on all platforms.
    pub conversation_watcher: Arc<Mutex<Option<ConversationWatcher>>>,
    pub display_controller: Arc<DisplayController>,
    pub remote_manager: Arc<RemoteManager>,
    pub diagnostic_buffer: Arc<DiagnosticRingBuffer>,
    pub network_monitor: Arc<NetworkMonitor>,
    pub switch_db: Arc<SwitchDatabase>,
    pub telemetry: Arc<TelemetryService>,
    #[allow(dead_code)]
    pub tray_icon: tauri::tray::TrayIcon,
}

#[derive(Clone)]
pub struct CodexAppServerBridge {
    tx: Arc<TokioMutex<Option<mpsc::UnboundedSender<CodexAppServerCommand>>>>,
}

/// Global handle to the live Codex app-server bridge. Set once during app
/// setup so utility paths (rate-limit fetch, future ad-hoc RPC calls) can
/// reuse the persistent WebSocket connection without threading the bridge
/// through every caller.
static CODEX_APP_SERVER_BRIDGE_HANDLE: OnceLock<Arc<CodexAppServerBridge>> = OnceLock::new();

pub fn register_codex_app_server_bridge(bridge: Arc<CodexAppServerBridge>) {
    let _ = CODEX_APP_SERVER_BRIDGE_HANDLE.set(bridge);
}

fn global_codex_app_server_bridge() -> Option<Arc<CodexAppServerBridge>> {
    CODEX_APP_SERVER_BRIDGE_HANDLE.get().cloned()
}

impl Default for CodexAppServerBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexAppServerBridge {
    pub fn new() -> Self {
        Self {
            tx: Arc::new(TokioMutex::new(None)),
        }
    }

    async fn attach(&self, tx: mpsc::UnboundedSender<CodexAppServerCommand>) {
        *self.tx.lock().await = Some(tx);
    }

    async fn detach(&self) {
        *self.tx.lock().await = None;
    }

    pub async fn respond_permission(
        &self,
        thread_id: &str,
        allowed: bool,
        always: bool,
    ) -> Result<bool, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let command = CodexAppServerCommand::Permission {
            thread_id: thread_id.to_string(),
            allowed,
            always,
            reply: reply_tx,
        };
        let Some(tx) = self.tx.lock().await.clone() else {
            return Ok(false);
        };
        if tx.send(command).is_err() {
            self.detach().await;
            return Ok(false);
        }
        reply_rx
            .await
            .map_err(|_| "Codex app-server monitor stopped before responding".to_string())?
    }

    pub async fn respond_question(
        &self,
        thread_id: &str,
        answers: BTreeMap<String, Vec<String>>,
    ) -> Result<bool, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let command = CodexAppServerCommand::Question {
            thread_id: thread_id.to_string(),
            answers,
            reply: reply_tx,
        };
        let Some(tx) = self.tx.lock().await.clone() else {
            return Ok(false);
        };
        if tx.send(command).is_err() {
            self.detach().await;
            return Ok(false);
        }
        reply_rx
            .await
            .map_err(|_| "Codex app-server monitor stopped before responding".to_string())?
    }

    /// Ask the live app-server for the latest account rate limits. Returns
    /// `Ok(None)` when the bridge isn't attached so the caller can fall back
    /// to a one-off stdio spawn.
    pub async fn fetch_rate_limits(&self) -> Result<Option<serde_json::Value>, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let command = CodexAppServerCommand::RateLimits { reply: reply_tx };
        let Some(tx) = self.tx.lock().await.clone() else {
            return Ok(None);
        };
        if tx.send(command).is_err() {
            self.detach().await;
            return Ok(None);
        }
        let value = reply_rx
            .await
            .map_err(|_| "Codex app-server monitor stopped before responding".to_string())??;
        Ok(Some(value))
    }

    /// Send a free-form user turn into a known Codex thread via JSON-RPC
    /// `turn/steer`. Returns `Ok(false)` when the bridge isn't attached so
    /// the caller can fall back to AppleScript-based message delivery.
    pub async fn send_user_turn(&self, thread_id: &str, text: &str) -> Result<bool, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let command = CodexAppServerCommand::SendUserTurn {
            thread_id: thread_id.to_string(),
            text: text.to_string(),
            reply: reply_tx,
        };
        let Some(tx) = self.tx.lock().await.clone() else {
            return Ok(false);
        };
        if tx.send(command).is_err() {
            self.detach().await;
            return Ok(false);
        }
        reply_rx
            .await
            .map_err(|_| "Codex app-server monitor stopped before responding".to_string())?
    }

    /// Returns true when an app-server monitor is currently connected.
    /// Cheap read used by the frontend gate to decide whether Codex.app
    /// sessions should expose a sendable composer.
    pub fn is_attached(&self) -> bool {
        self.tx
            .try_lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }
}

enum CodexAppServerCommand {
    Permission {
        thread_id: String,
        allowed: bool,
        always: bool,
        reply: oneshot::Sender<Result<bool, String>>,
    },
    Question {
        thread_id: String,
        answers: BTreeMap<String, Vec<String>>,
        reply: oneshot::Sender<Result<bool, String>>,
    },
    RateLimits {
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    SendUserTurn {
        thread_id: String,
        text: String,
        reply: oneshot::Sender<Result<bool, String>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CodexAppServerPendingKind {
    CommandApproval,
    FileApproval,
    PermissionsApproval,
    UserInput,
}

#[derive(Debug, Clone)]
struct CodexAppServerPendingRequest {
    request_id: serde_json::Value,
    kind: CodexAppServerPendingKind,
    requested_permissions: Option<serde_json::Value>,
}

enum CodexAppServerOutgoingRequest {
    ThreadList,
    RateLimits {
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    SendUserTurn {
        reply: oneshot::Sender<Result<bool, String>>,
    },
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
pub async fn get_usage_rate_limits(
    state: State<'_, AppState>,
) -> Result<Option<RateLimitInfo>, String> {
    if !state.config_store.get().usage_query_enabled {
        return Ok(None);
    }
    Ok(load_latest_usage_rate_limits().await)
}

#[tauri::command]
pub async fn get_usage_snapshots(state: State<'_, AppState>) -> Result<Vec<RateLimitInfo>, String> {
    if !state.config_store.get().usage_query_enabled {
        return Ok(Vec::new());
    }
    Ok(load_usage_snapshots().await)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStateFlags {
    /// True when an active Codex app-server WebSocket bridge is attached
    /// (i.e. background sync is running and connected). Frontend uses this
    /// to decide whether Codex.app sessions expose a sendable composer.
    pub codex_app_server_live: bool,
}

#[tauri::command]
pub fn get_app_state_flags(state: State<'_, AppState>) -> AppStateFlags {
    AppStateFlags {
        codex_app_server_live: state.codex_app_server.is_attached(),
    }
}

pub fn start_codex_app_server_background_sync(
    config_store: ConfigStore,
    session_store: Arc<SessionStore>,
    bridge: Arc<CodexAppServerBridge>,
) {
    tauri::async_runtime::spawn(async move {
        if resolve_codex_binary().is_none() {
            log::info!("Codex CLI not found; app-server monitor disabled");
            return;
        }

        let mut backoff = Duration::ZERO;
        let mut last_error: Option<String> = None;
        loop {
            let config = config_store.get();
            if !config.codex_app_server_sync_enabled {
                bridge.detach().await;
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            }

            match run_codex_app_server_monitor_once(
                config_store.clone(),
                session_store.clone(),
                bridge.clone(),
            )
            .await
            {
                Ok(()) => {
                    if last_error.is_some() {
                        log::info!("Codex app-server background sync recovered");
                    }
                    last_error = None;
                    backoff = Duration::ZERO;
                }
                Err(err) => {
                    if last_error.as_deref() != Some(err.as_str()) {
                        log::warn!("Codex app-server monitor failed: {}", err);
                        last_error = Some(err.clone());
                    }
                    bridge.detach().await;
                    backoff = crate::agents::codex_app_server::next_backoff(backoff);
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    });
}

fn codex_app_server_refresh_interval_seconds(
    store: &SessionStore,
    configured_seconds: u32,
) -> (EnergyMode, u64) {
    let mode = energy::mode_for_sessions(&store.get_all_sessions());
    let interval = energy::interval_seconds(mode, configured_seconds, 15, 60, 300);
    (mode, interval)
}

pub fn start_remote_codex_state_sync(
    config_store: ConfigStore,
    store: Arc<SessionStore>,
    remote_manager: Arc<RemoteManager>,
) {
    tauri::async_runtime::spawn(async move {
        let mut delivered: HashMap<String, i64> = HashMap::new();
        let mut last_energy_mode: Option<EnergyMode> = None;

        loop {
            let config = config_store.get();
            if !config.codex_app_server_sync_enabled {
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            }

            let (energy_mode, interval) = codex_app_server_refresh_interval_seconds(
                &store,
                config.codex_app_server_sync_interval_seconds,
            );
            if last_energy_mode != Some(energy_mode) {
                log::debug!(
                    "Remote Codex state sync energy mode: {:?}, interval={}s",
                    energy_mode,
                    interval
                );
                last_energy_mode = Some(energy_mode);
            }

            let cutoff_ms = chrono::Utc::now().timestamp_millis() - 15 * 60 * 1000;
            for host in remote_manager.hosts() {
                if remote_manager.status(&host.id) != ConnectionStatus::Connected {
                    continue;
                }

                match crate::remote::installer::RemoteInstaller::read_recent_codex_threads(
                    &host, cutoff_ms, 12,
                )
                .await
                {
                    Ok(threads) => {
                        for thread in threads {
                            let key = format!("{}:{}", host.id, thread.id);
                            if thread.updated_at_ms <= delivered.get(&key).copied().unwrap_or(0) {
                                continue;
                            }
                            delivered.insert(key, thread.updated_at_ms);
                            sync_remote_codex_thread_to_store(&store, &host, &thread);
                        }
                    }
                    Err(err) => {
                        log::debug!(
                            "Remote Codex state sync skipped host {}: {}",
                            host.name,
                            err
                        );
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageProviderStatus {
    provider: String,
    label: String,
    enabled: bool,
    available: bool,
    catalog_supported: bool,
    implementation_status: String,
    source: Option<String>,
    detail: String,
    auth_status: String,
    auth_path: Option<String>,
    can_authorize: bool,
}

#[tauri::command]
pub async fn list_usage_providers(
    state: State<'_, AppState>,
    live: Option<bool>,
) -> Result<Vec<UsageProviderStatus>, String> {
    let enabled = state.config_store.get().usage_query_enabled;
    let allow_live = live.unwrap_or(true);
    let mut providers = vec![
        codex_usage_provider_status(enabled, allow_live).await,
        claude_usage_provider_status(enabled),
    ];
    providers.extend(catalog_supported_agent_usage_providers(enabled));
    providers.extend(catalog_unsupported_agent_usage_providers(enabled));
    Ok(providers)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAppServerThreadSummary {
    id: String,
    name: Option<String>,
    preview: Option<String>,
    cwd: Option<String>,
    status: Option<String>,
    phase: String,
    updated_at: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAppServerSyncReport {
    total: usize,
    synced: usize,
    read: usize,
    errors: Vec<String>,
    threads: Vec<CodexAppServerThreadSummary>,
}

#[tauri::command]
pub async fn authorize_usage_provider(provider: String) -> Result<(), String> {
    let (binary, args): (&str, &[&str]) = match provider.as_str() {
        "codex" => ("codex", &["login"]),
        "claude-code" | "claude" => ("claude", &["login"]),
        "gemini" | "gemini-cli" => ("gemini", &["auth"]),
        "copilot" => ("gh", &["auth", "login"]),
        "opencode" => ("opencode", &["providers"]),
        "kiro" => ("kiro-cli", &["login"]),
        _ => return Err(format!("Unsupported usage provider: {provider}")),
    };

    let Some(binary_path) = find_binary(binary) else {
        return Err(format!("{} CLI not found in PATH.", binary));
    };
    let command = std::iter::once(shell_quote(&binary_path))
        .chain(args.iter().map(|arg| shell_quote(arg)))
        .collect::<Vec<_>>()
        .join(" ");
    let cwd = dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .to_string_lossy()
        .to_string();
    launch_in_terminal("Terminal", &cwd, &command)
}

#[derive(Clone)]
struct UsageRateLimitSnapshot {
    rate_limits: RateLimitInfo,
    captured_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Default)]
struct CodexUsageLiveCache {
    fetched_at: Option<Instant>,
    snapshot: Option<UsageRateLimitSnapshot>,
}

const CODEX_USAGE_LIVE_CACHE_TTL: Duration = Duration::from_secs(300);
const CODEX_USAGE_LIVE_FAILURE_TTL: Duration = Duration::from_secs(60);

async fn load_usage_snapshots() -> Vec<RateLimitInfo> {
    [
        load_codex_usage_rate_limits(true).await,
        load_claude_usage_rate_limits(),
    ]
    .into_iter()
    .flatten()
    .map(|snapshot| snapshot.rate_limits)
    .collect()
}

async fn codex_usage_provider_status(enabled: bool, allow_live: bool) -> UsageProviderStatus {
    let auth_path = dirs::home_dir().map(|home| home.join(".codex").join("auth.json"));
    let has_auth = auth_path.as_ref().is_some_and(|path| path.exists());
    let snapshot = load_codex_usage_rate_limits(allow_live).await;
    let source = snapshot
        .as_ref()
        .and_then(|item| item.rate_limits.source.clone());
    let updated = snapshot
        .as_ref()
        .and_then(|item| item.captured_at)
        .map(|date| format!(" updated {}", date.format("%H:%M:%S")))
        .unwrap_or_default();
    UsageProviderStatus {
        provider: "codex".to_string(),
        label: "Codex".to_string(),
        enabled,
        available: snapshot.is_some(),
        catalog_supported: true,
        implementation_status: "active".to_string(),
        source,
        detail: if snapshot.is_some() {
            format!("Codex account rate limits found.{updated}")
        } else if has_auth {
            "Codex auth found, waiting for account quota data.".to_string()
        } else {
            "No Codex auth or account quota data found.".to_string()
        },
        auth_status: if has_auth { "authorized" } else { "missing" }.to_string(),
        auth_path: auth_path.map(|path| path.display().to_string()),
        can_authorize: find_binary("codex").is_some(),
    }
}

fn claude_usage_provider_status(enabled: bool) -> UsageProviderStatus {
    let auth_path = dirs::home_dir().map(|home| home.join(".claude").join(".credentials.json"));
    let has_auth = auth_path.as_ref().is_some_and(|path| path.exists());
    let temp_path = Path::new("/tmp/island-rate-limits.json");
    let has_temp = temp_path.exists();
    let snapshot = load_claude_usage_rate_limits();
    let source = snapshot
        .as_ref()
        .and_then(|item| item.rate_limits.source.clone());
    UsageProviderStatus {
        provider: "claude-code".to_string(),
        label: "Claude Code".to_string(),
        enabled,
        available: snapshot.is_some(),
        catalog_supported: true,
        implementation_status: "active".to_string(),
        source,
        detail: if snapshot.is_some() {
            "Claude island/statusline rate limits found.".to_string()
        } else if has_temp {
            "Claude rate-limit file exists but could not be parsed.".to_string()
        } else if has_auth {
            "Claude credentials found, waiting for statusline rate-limit data.".to_string()
        } else {
            "No Claude credentials or rate-limit statusline data found.".to_string()
        },
        auth_status: if has_auth { "authorized" } else { "missing" }.to_string(),
        auth_path: auth_path.map(|path| path.display().to_string()),
        can_authorize: find_binary("claude").is_some(),
    }
}

fn catalog_supported_agent_usage_providers(enabled: bool) -> Vec<UsageProviderStatus> {
    let mut providers = [
        ("z-ai", "Z.ai", "Z.ai", "api/key", None, false),
        ("kimi", "Kimi", "Kimi", "web/token", Some("~/.kimi"), false),
        ("gemini-cli", "Gemini CLI", "Gemini", "api/oauth", Some("~/.gemini"), find_binary("gemini").is_some()),
        ("copilot", "GitHub Copilot", "Copilot", "api/device-flow", None, find_binary("gh").is_some()),
        ("cursor", "Cursor", "Cursor", "web/cookies", Some("~/.cursor"), false),
        ("cursor-cli", "Cursor CLI", "Cursor", "web/cookies", Some("~/.cursor"), false),
        ("deepseek", "DeepSeek", "DeepSeek", "api/key", Some("~/.deepseek"), false),
        ("droid", "Factory / Droid", "Droid/Factory", "web/local-storage", Some("~/.factory"), false),
        ("stepfun", "StepFun", "StepFun", "web/token", None, false),
        ("antigravity", "Antigravity", "Antigravity", "local-probe", None, false),
        ("kiro", "Kiro", "Kiro", "cli", Some("~/.kiro"), find_binary("kiro-cli").is_some()),
    ]
    .into_iter()
    .map(|(provider, label, source_name, source, auth_path, can_authorize)| {
        known_provider_status(
            enabled,
            provider,
            label,
            true,
            "available",
            source,
            &format!("{source_name} has a known usage strategy; AgentBro usage reader is not wired yet."),
            "unknown",
            auth_path,
            can_authorize,
        )
    })
    .collect::<Vec<_>>();
    providers.push(opencode_usage_provider_status(enabled));
    providers
}

fn opencode_usage_provider_status(enabled: bool) -> UsageProviderStatus {
    let home = dirs::home_dir();
    let config_dir = home
        .as_ref()
        .map(|home| home.join(".config").join("opencode"));
    let auth_path = home.as_ref().map(|home| {
        home.join(".local")
            .join("share")
            .join("opencode")
            .join("auth.json")
    });
    let has_config = config_dir.as_ref().is_some_and(|path| path.exists());
    let has_auth = auth_path.as_ref().is_some_and(|path| path.exists());
    let display_path = if has_auth { auth_path } else { config_dir };
    UsageProviderStatus {
        provider: "opencode".to_string(),
        label: "OpenCode".to_string(),
        enabled,
        available: false,
        catalog_supported: true,
        implementation_status: "available".to_string(),
        source: Some("cli/config".to_string()),
        detail: if has_auth {
            "OpenCode auth found; AgentBro usage reader is not wired yet.".to_string()
        } else if has_config {
            "OpenCode config found; run OpenCode provider authorization if usage data is needed."
                .to_string()
        } else {
            "OpenCode config directory was not found.".to_string()
        },
        auth_status: if has_auth { "unknown" } else { "missing" }.to_string(),
        auth_path: display_path.map(|path| path.display().to_string()),
        can_authorize: !has_auth && find_binary("opencode").is_some(),
    }
}

fn catalog_unsupported_agent_usage_providers(enabled: bool) -> Vec<UsageProviderStatus> {
    [
        ("qoder", "Qoder"),
        ("qoder-cli", "Qoder CLI"),
        ("codebuddy", "CodeBuddy"),
        ("codebuddycn", "CodeBuddy CN"),
        ("qwen", "Qwen"),
        ("deepseek", "DeepSeek"),
        ("workbuddy", "WorkBuddy"),
        ("hermes", "Hermes"),
        ("pi", "Pi"),
    ]
    .into_iter()
    .map(|(provider, label)| {
        known_provider_status(
            enabled,
            provider,
            label,
            false,
            "unsupported",
            None,
            "No usage reader is available for this Agent yet.",
            "unknown",
            None,
            false,
        )
    })
    .collect()
}

#[allow(clippy::too_many_arguments)]
fn known_provider_status(
    enabled: bool,
    provider: &str,
    label: &str,
    catalog_supported: bool,
    implementation_status: &str,
    source: impl Into<Option<&'static str>>,
    detail: &str,
    auth_status: &str,
    auth_path: impl Into<Option<&'static str>>,
    can_authorize: bool,
) -> UsageProviderStatus {
    let auth_path = auth_path
        .into()
        .and_then(|path| expand_home_path(path).map(|path| path.display().to_string()));
    UsageProviderStatus {
        provider: provider.to_string(),
        label: label.to_string(),
        enabled,
        available: false,
        catalog_supported,
        implementation_status: implementation_status.to_string(),
        source: source.into().map(str::to_string),
        detail: detail.to_string(),
        auth_status: auth_status.to_string(),
        auth_path,
        can_authorize,
    }
}

fn expand_home_path(path: &str) -> Option<PathBuf> {
    if path == "~" {
        return dirs::home_dir();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs::home_dir().map(|home| home.join(rest));
    }
    Some(PathBuf::from(path))
}

async fn load_latest_usage_rate_limits() -> Option<RateLimitInfo> {
    let codex = load_codex_usage_rate_limits(true).await;
    let claude = load_claude_usage_rate_limits();

    match (codex, claude) {
        (Some(codex), Some(claude)) => {
            let codex_time = codex
                .captured_at
                .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC);
            let claude_time = claude
                .captured_at
                .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC);
            Some(if codex_time >= claude_time {
                codex.rate_limits
            } else {
                claude.rate_limits
            })
        }
        (Some(codex), None) => Some(codex.rate_limits),
        (None, Some(claude)) => Some(claude.rate_limits),
        (None, None) => None,
    }
}

fn load_claude_usage_rate_limits() -> Option<UsageRateLimitSnapshot> {
    let path = Path::new("/tmp/island-rate-limits.json");
    let content = fs::read_to_string(path).ok()?;
    let payload: serde_json::Value = serde_json::from_str(&content).ok()?;
    let metadata_time = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(chrono::DateTime::<chrono::Utc>::from);

    let five_hour = payload
        .get("five_hour")
        .or_else(|| payload.get("fiveHour"))?;
    let seven_day = payload
        .get("seven_day")
        .or_else(|| payload.get("sevenDay"))?;
    let captured_at = metadata_time;

    Some(UsageRateLimitSnapshot {
        rate_limits: provider_rate_limits(
            "claude-code",
            "Claude",
            "claude-island",
            captured_at,
            five_hour,
            seven_day,
        )?,
        captured_at,
    })
}

async fn load_codex_usage_rate_limits(allow_live: bool) -> Option<UsageRateLimitSnapshot> {
    if allow_live {
        load_codex_usage_rate_limits_live_cached()
            .await
            .or_else(load_codex_usage_rate_limits_from_jsonl)
    } else {
        load_codex_usage_rate_limits_from_jsonl()
    }
}

async fn load_codex_usage_rate_limits_live_cached() -> Option<UsageRateLimitSnapshot> {
    static CACHE: OnceLock<tokio::sync::Mutex<CodexUsageLiveCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| tokio::sync::Mutex::new(CodexUsageLiveCache::default()));
    let mut guard = cache.lock().await;

    if let Some(fetched_at) = guard.fetched_at {
        let ttl = if guard.snapshot.is_some() {
            CODEX_USAGE_LIVE_CACHE_TTL
        } else {
            CODEX_USAGE_LIVE_FAILURE_TTL
        };
        if fetched_at.elapsed() < ttl {
            return guard.snapshot.clone();
        }
    }

    let snapshot = load_codex_usage_rate_limits_live_uncached().await;
    guard.fetched_at = Some(Instant::now());
    guard.snapshot = snapshot.clone();
    snapshot
}

async fn load_codex_usage_rate_limits_live_uncached() -> Option<UsageRateLimitSnapshot> {
    // Prefer the persistent app-server WebSocket bridge when it's attached —
    // sidesteps a redundant stdio spawn on every rate-limit poll.
    if let Some(bridge) = global_codex_app_server_bridge() {
        match bridge.fetch_rate_limits().await {
            Ok(Some(response)) => {
                if let Some(snapshot) = codex_usage_snapshot_from_rpc_message(&response) {
                    return Some(snapshot);
                }
                // bridge responded but payload wasn't parseable — fall through
                // to stdio fallback rather than returning None silently.
            }
            Ok(None) => {
                // bridge not attached; fall through
            }
            Err(err) => {
                log::debug!("Codex app-server bridge rate-limit fetch failed: {err}");
            }
        }
    }

    let binary = find_binary("codex")?;
    let mut child = TokioCommand::new(binary)
        .args(["-s", "read-only", "-a", "untrusted", "app-server"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let mut stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let mut lines = TokioBufReader::new(stdout).lines();

    let result = tokio::time::timeout(Duration::from_secs(8), async {
        write_json_rpc(
            &mut stdin,
            serde_json::json!({
                "id": 1,
                "method": "initialize",
                "params": {
                    "clientInfo": {
                        "name": "AgentBro",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }),
        )
        .await?;
        read_json_rpc_response(&mut lines, 1).await?;

        write_json_rpc(
            &mut stdin,
            serde_json::json!({
                "method": "initialized",
                "params": {}
            }),
        )
        .await?;

        write_json_rpc(
            &mut stdin,
            serde_json::json!({
                "id": 2,
                "method": "account/rateLimits/read",
                "params": {}
            }),
        )
        .await?;
        let response = read_json_rpc_response(&mut lines, 2).await?;
        codex_usage_snapshot_from_rpc_message(&response)
    })
    .await
    .ok()
    .flatten();

    if child.try_wait().ok().flatten().is_none() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }

    result
}

async fn write_json_rpc(stdin: &mut ChildStdin, payload: serde_json::Value) -> Option<()> {
    let mut line = serde_json::to_vec(&payload).ok()?;
    line.push(b'\n');
    stdin.write_all(&line).await.ok()?;
    stdin.flush().await.ok()?;
    Some(())
}

async fn read_json_rpc_response(
    lines: &mut tokio::io::Lines<TokioBufReader<ChildStdout>>,
    expected_id: i64,
) -> Option<serde_json::Value> {
    while let Some(line) = lines.next_line().await.ok()? {
        let value: serde_json::Value = serde_json::from_str(&line).ok()?;
        if value.get("id").and_then(|id| id.as_i64()) != Some(expected_id) {
            continue;
        }
        if value.get("error").is_some() {
            return None;
        }
        return Some(value);
    }
    None
}

async fn shutdown_codex_app_server_child(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}

async fn write_ws_json(sink: &mut CodexWsSink, payload: serde_json::Value) -> Result<(), String> {
    let message = crate::agents::codex_app_server::json_message(&payload)?;
    sink.send(message)
        .await
        .map_err(|err| format!("Failed to write to codex app-server WebSocket: {err}"))
}

async fn read_ws_until_id(
    stream: &mut CodexWsSource,
    expected_id: i64,
) -> Result<serde_json::Value, String> {
    while let Some(message) = stream.next().await {
        let message = message.map_err(|err| format!("codex app-server WebSocket error: {err}"))?;
        let text = match message {
            Message::Text(text) => text,
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                continue
            }
            Message::Close(_) => {
                return Err("codex app-server closed the WebSocket".to_string());
            }
        };
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|err| format!("Invalid codex app-server JSON: {err}"))?;
        if value.get("id").and_then(|id| id.as_i64()) != Some(expected_id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(|message| message.as_str())
                .unwrap_or("Codex app-server request failed");
            return Err(message.to_string());
        }
        return Ok(value);
    }
    Err("codex app-server closed before responding".to_string())
}

async fn initialize_codex_app_server_ws(
    sink: &mut CodexWsSink,
    stream: &mut CodexWsSource,
) -> Result<(), String> {
    write_ws_json(
        sink,
        serde_json::json!({
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "AgentBro",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }),
    )
    .await?;
    read_ws_until_id(stream, 1).await?;

    write_ws_json(
        sink,
        serde_json::json!({
            "method": "initialized",
            "params": {}
        }),
    )
    .await
}

async fn run_codex_app_server_monitor_once(
    config_store: ConfigStore,
    store: Arc<SessionStore>,
    bridge: Arc<CodexAppServerBridge>,
) -> Result<(), String> {
    let binary = resolve_codex_binary()
        .ok_or_else(|| "Could not find codex CLI for app-server".to_string())?;
    let mut connection = crate::agents::codex_app_server::spawn_and_connect_app_server(
        &binary,
        Duration::from_secs(10),
    )
    .await?;
    log::info!(
        "Codex app-server listening on ws://127.0.0.1:{}",
        connection.listen_port
    );
    let (mut sink, mut stream) = connection.socket.split();
    let result = async {
        initialize_codex_app_server_ws(&mut sink, &mut stream).await?;

        let (tx, mut rx) = mpsc::unbounded_channel();
        bridge.attach(tx).await;

        let mut next_request_id = 2_i64;
        let mut outgoing: HashMap<i64, CodexAppServerOutgoingRequest> = HashMap::new();
        let mut pending_requests: HashMap<String, CodexAppServerPendingRequest> = HashMap::new();
        let mut last_energy_mode: Option<EnergyMode> = None;

        send_codex_app_server_thread_list_request(&mut sink, &mut outgoing, &mut next_request_id)
            .await?;

        loop {
            if !config_store.get().codex_app_server_sync_enabled {
                break Ok(());
            }

            let (energy_mode, interval) = codex_app_server_refresh_interval_seconds(
                &store,
                config_store.get().codex_app_server_sync_interval_seconds,
            );
            if last_energy_mode != Some(energy_mode) {
                log::debug!(
                    "Codex app-server energy mode: {:?}, thread/list interval={}s",
                    energy_mode,
                    interval
                );
                last_energy_mode = Some(energy_mode);
            }

            tokio::select! {
                incoming = stream.next() => {
                    let message = incoming
                        .ok_or_else(|| "Codex app-server WebSocket stream ended".to_string())?
                        .map_err(|err| format!("codex app-server WebSocket error: {err}"))?;
                    let text = match message {
                        Message::Text(text) => text,
                        Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
                        Message::Close(_) => break Err("codex app-server closed the WebSocket".to_string()),
                    };
                    let value: serde_json::Value = serde_json::from_str(&text)
                        .map_err(|err| format!("Invalid codex app-server JSON message: {err}"))?;
                    handle_codex_app_server_message(
                        &store,
                        &mut pending_requests,
                        &mut outgoing,
                        &value,
                    )
                    .await?;
                }
                Some(command) = rx.recv() => {
                    handle_codex_app_server_command(
                        &store,
                        &mut sink,
                        &mut pending_requests,
                        &mut outgoing,
                        &mut next_request_id,
                        command,
                    )
                    .await;
                }
                _ = tokio::time::sleep(Duration::from_secs(interval)) => {
                    send_codex_app_server_thread_list_request(
                        &mut sink,
                        &mut outgoing,
                        &mut next_request_id,
                    )
                    .await?;
                }
            }
        }
    }
    .await;

    bridge.detach().await;
    let _ = sink.close().await;
    shutdown_codex_app_server_child(&mut connection.child).await;
    result
}

async fn send_codex_app_server_thread_list_request(
    sink: &mut CodexWsSink,
    outgoing: &mut HashMap<i64, CodexAppServerOutgoingRequest>,
    next_request_id: &mut i64,
) -> Result<(), String> {
    let request_id = *next_request_id;
    *next_request_id += 1;
    write_ws_json(
        sink,
        serde_json::json!({
            "id": request_id,
            "method": "thread/list",
            "params": {
                "archived": false,
                "limit": 30,
                "sortKey": "updated_at"
            }
        }),
    )
    .await?;
    outgoing.insert(request_id, CodexAppServerOutgoingRequest::ThreadList);
    Ok(())
}

async fn handle_codex_app_server_message(
    store: &SessionStore,
    pending_requests: &mut HashMap<String, CodexAppServerPendingRequest>,
    outgoing: &mut HashMap<i64, CodexAppServerOutgoingRequest>,
    message: &serde_json::Value,
) -> Result<(), String> {
    if let Some(method) = message.get("method").and_then(|value| value.as_str()) {
        let params = message
            .get("params")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(request_id) = message.get("id").cloned() {
            handle_codex_app_server_request(store, pending_requests, request_id, method, &params)
                .await?;
        } else {
            handle_codex_app_server_notification(store, pending_requests, method, &params).await?;
        }
        return Ok(());
    }

    let Some(id) = message.get("id").and_then(|value| value.as_i64()) else {
        return Ok(());
    };
    let Some(kind) = outgoing.remove(&id) else {
        return Ok(());
    };
    if let Some(error) = message.get("error") {
        let err_message = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Codex app-server request failed")
            .to_string();
        log::warn!("Codex app-server request {} failed: {}", id, err_message);
        match kind {
            CodexAppServerOutgoingRequest::RateLimits { reply } => {
                let _ = reply.send(Err(err_message));
            }
            CodexAppServerOutgoingRequest::SendUserTurn { reply } => {
                let _ = reply.send(Err(err_message));
            }
            CodexAppServerOutgoingRequest::ThreadList => {}
        }
        return Ok(());
    }
    match kind {
        CodexAppServerOutgoingRequest::ThreadList => {
            if let Some(threads) = codex_thread_list_from_response(message) {
                for thread in threads {
                    sync_codex_app_server_thread_to_store(store, &thread);
                }
            }
        }
        CodexAppServerOutgoingRequest::RateLimits { reply } => {
            let _ = reply.send(Ok(message.clone()));
        }
        CodexAppServerOutgoingRequest::SendUserTurn { reply } => {
            let _ = reply.send(Ok(true));
        }
    }
    Ok(())
}

async fn handle_codex_app_server_notification(
    store: &SessionStore,
    pending_requests: &HashMap<String, CodexAppServerPendingRequest>,
    method: &str,
    params: &serde_json::Value,
) -> Result<(), String> {
    match method {
        "thread/status/changed" => {
            let Some(thread_id) = codex_string(params, "threadId") else {
                return Ok(());
            };
            if store.get_session(&thread_id).is_none() && !pending_requests.contains_key(&thread_id)
            {
                return Ok(());
            }
            let phase = codex_phase_from_status(
                params.get("status"),
                pending_requests
                    .get(&thread_id)
                    .map(|pending| &pending.kind),
            );
            store.get_or_create_session(&thread_id, "codex", "Codex", "/", "Codex");
            store.update_session(&thread_id, |session| {
                session.agent_type = "codex".to_string();
                session.engine_label = Some("Codex App".to_string());
                session.terminal = "Codex".to_string();
                session.term_bundle_id = Some("com.openai.codex".to_string());
                session.phase = phase;
            });
        }
        "thread/started" => {
            if let Some(thread) = params.get("thread") {
                sync_codex_app_server_thread_to_store(store, thread);
            }
        }
        "thread/name/updated" => {
            let Some(thread_id) = codex_string(params, "threadId") else {
                return Ok(());
            };
            let name = codex_string(params, "threadName");
            store.update_session(&thread_id, |session| {
                if let Some(name) = name.clone() {
                    session.project = name.clone();
                    session.session_title = Some(name);
                }
            });
        }
        "thread/archived" => {
            let Some(thread_id) = codex_string(params, "threadId") else {
                return Ok(());
            };
            store.update_session(&thread_id, |session| {
                session.phase = SessionPhase::Done;
                session.description = Some("Session ended".to_string());
                session.last_response = Some("Session ended".to_string());
            });
        }
        _ => {}
    }
    Ok(())
}

async fn handle_codex_app_server_request(
    store: &SessionStore,
    pending_requests: &mut HashMap<String, CodexAppServerPendingRequest>,
    request_id: serde_json::Value,
    method: &str,
    params: &serde_json::Value,
) -> Result<(), String> {
    match method {
        "item/commandExecution/requestApproval" => {
            let thread_id = codex_string(params, "threadId")
                .or_else(|| codex_string(params, "conversationId"))
                .unwrap_or_default();
            if thread_id.is_empty() {
                return Ok(());
            }
            let command = params
                .get("command")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let reason = codex_string(params, "reason");
            let cwd = codex_string(params, "cwd");
            let preview = if command.is_empty() {
                reason
                    .clone()
                    .unwrap_or_else(|| "Codex wants to run a terminal command.".to_string())
            } else {
                command.clone()
            };
            pending_requests.insert(
                thread_id.clone(),
                CodexAppServerPendingRequest {
                    request_id: request_id.clone(),
                    kind: CodexAppServerPendingKind::CommandApproval,
                    requested_permissions: None,
                },
            );
            upsert_codex_app_server_pending_permission(
                store,
                &thread_id,
                cwd.as_deref(),
                &preview,
                request_id_to_string(&request_id),
                "exec_command",
                &preview,
            );
        }
        "item/fileChange/requestApproval" => {
            let Some(thread_id) = codex_string(params, "threadId") else {
                return Ok(());
            };
            let preview = codex_string(params, "reason")
                .or_else(|| codex_string(params, "grantRoot"))
                .unwrap_or_else(|| "Codex wants to modify files in this workspace.".to_string());
            pending_requests.insert(
                thread_id.clone(),
                CodexAppServerPendingRequest {
                    request_id: request_id.clone(),
                    kind: CodexAppServerPendingKind::FileApproval,
                    requested_permissions: None,
                },
            );
            upsert_codex_app_server_pending_permission(
                store,
                &thread_id,
                None,
                &preview,
                request_id_to_string(&request_id),
                "file_change",
                &preview,
            );
        }
        "item/permissions/requestApproval" => {
            let Some(thread_id) = codex_string(params, "threadId") else {
                return Ok(());
            };
            let permissions = params
                .get("permissions")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let preview = codex_string(params, "reason")
                .unwrap_or_else(|| codex_app_server_permission_summary(&permissions));
            pending_requests.insert(
                thread_id.clone(),
                CodexAppServerPendingRequest {
                    request_id: request_id.clone(),
                    kind: CodexAppServerPendingKind::PermissionsApproval,
                    requested_permissions: Some(permissions.clone()),
                },
            );
            upsert_codex_app_server_pending_permission(
                store,
                &thread_id,
                None,
                &preview,
                request_id_to_string(&request_id),
                "permissions_request",
                &preview,
            );
        }
        "item/tool/requestUserInput" => {
            let Some(thread_id) = codex_string(params, "threadId") else {
                return Ok(());
            };
            let pending_question = codex_app_server_pending_question(params, &request_id);
            let preview = pending_question.question.clone();
            pending_requests.insert(
                thread_id.clone(),
                CodexAppServerPendingRequest {
                    request_id,
                    kind: CodexAppServerPendingKind::UserInput,
                    requested_permissions: None,
                },
            );
            upsert_codex_app_server_pending_question(store, &thread_id, &preview, pending_question);
        }
        _ => {}
    }
    Ok(())
}

async fn handle_codex_app_server_command(
    store: &SessionStore,
    sink: &mut CodexWsSink,
    pending_requests: &mut HashMap<String, CodexAppServerPendingRequest>,
    outgoing: &mut HashMap<i64, CodexAppServerOutgoingRequest>,
    next_request_id: &mut i64,
    command: CodexAppServerCommand,
) {
    match command {
        CodexAppServerCommand::Permission {
            thread_id,
            allowed,
            always,
            reply,
        } => {
            let result = match pending_requests.remove(&thread_id) {
                Some(pending)
                    if matches!(
                        pending.kind,
                        CodexAppServerPendingKind::CommandApproval
                            | CodexAppServerPendingKind::FileApproval
                            | CodexAppServerPendingKind::PermissionsApproval
                    ) =>
                {
                    let response = codex_app_server_permission_response(&pending, allowed, always);
                    match write_ws_json(
                        sink,
                        serde_json::json!({
                            "id": pending.request_id,
                            "result": response
                        }),
                    )
                    .await
                    {
                        Ok(()) => {
                            clear_codex_app_server_interaction(store, &thread_id);
                            Ok(true)
                        }
                        Err(err) => Err(err),
                    }
                }
                Some(pending) => {
                    pending_requests.insert(thread_id.clone(), pending);
                    Ok(false)
                }
                None => Ok(false),
            };
            let _ = reply.send(result);
        }
        CodexAppServerCommand::Question {
            thread_id,
            answers,
            reply,
        } => {
            let result = match pending_requests.remove(&thread_id) {
                Some(pending) if pending.kind == CodexAppServerPendingKind::UserInput => {
                    match write_ws_json(
                        sink,
                        serde_json::json!({
                            "id": pending.request_id,
                            "result": codex_request_user_input_payload(answers)
                        }),
                    )
                    .await
                    {
                        Ok(()) => {
                            clear_codex_app_server_interaction(store, &thread_id);
                            Ok(true)
                        }
                        Err(err) => Err(err),
                    }
                }
                Some(pending) => {
                    pending_requests.insert(thread_id.clone(), pending);
                    Ok(false)
                }
                None => Ok(false),
            };
            let _ = reply.send(result);
        }
        CodexAppServerCommand::RateLimits { reply } => {
            let request_id = *next_request_id;
            *next_request_id += 1;
            let write_result = write_ws_json(
                sink,
                serde_json::json!({
                    "id": request_id,
                    "method": "account/rateLimits/read",
                    "params": {}
                }),
            )
            .await;
            match write_result {
                Ok(()) => {
                    outgoing.insert(
                        request_id,
                        CodexAppServerOutgoingRequest::RateLimits { reply },
                    );
                }
                Err(err) => {
                    let _ = reply.send(Err(err));
                }
            }
        }
        CodexAppServerCommand::SendUserTurn {
            thread_id,
            text,
            reply,
        } => {
            let request_id = *next_request_id;
            *next_request_id += 1;
            let payload = codex_turn_steer_payload(request_id, &thread_id, &text);
            match write_ws_json(sink, payload).await {
                Ok(()) => {
                    outgoing.insert(
                        request_id,
                        CodexAppServerOutgoingRequest::SendUserTurn { reply },
                    );
                }
                Err(err) => {
                    let _ = reply.send(Err(err));
                }
            }
        }
    }
}

/// Build the `turn/steer` JSON-RPC payload that injects a fresh user turn
/// into an existing Codex thread. Extracted so unit tests can pin the wire
/// format independent of the WebSocket I/O.
fn codex_turn_steer_payload(request_id: i64, thread_id: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "id": request_id,
        "method": "turn/steer",
        "params": {
            "threadId": thread_id,
            "expectedTurnId": "",
            "input": [
                { "type": "text", "text": text }
            ]
        }
    })
}

fn upsert_codex_app_server_pending_permission(
    store: &SessionStore,
    thread_id: &str,
    cwd: Option<&str>,
    preview: &str,
    tool_use_id: String,
    tool_name: &str,
    tool_input: &str,
) {
    let cwd = cwd.unwrap_or("/");
    store.get_or_create_session(thread_id, "codex", "Codex", cwd, "Codex");
    store.update_session(thread_id, |session| {
        session.agent_type = "codex".to_string();
        session.engine_label = Some("Codex App".to_string());
        session.codex_app_server_thread_id = Some(thread_id.to_string());
        session.project = if session.project.trim().is_empty() {
            "Codex".to_string()
        } else {
            session.project.clone()
        };
        session.cwd = cwd.to_string();
        session.terminal = "Codex".to_string();
        session.term_bundle_id = Some("com.openai.codex".to_string());
        session.phase = SessionPhase::WaitingApproval;
        session.description = Some(preview.to_string());
        session.pending_permission = Some(crate::hooks::session_store::PendingPermission {
            tool_use_id: Some(tool_use_id),
            tool_name: tool_name.to_string(),
            tool_input: tool_input.to_string(),
            diff: None,
            options: None,
        });
        session.pending_question = None;
        session.pending_plan = None;
    });
}

fn upsert_codex_app_server_pending_question(
    store: &SessionStore,
    thread_id: &str,
    preview: &str,
    pending_question: PendingQuestion,
) {
    store.get_or_create_session(thread_id, "codex", "Codex", "/", "Codex");
    store.update_session(thread_id, |session| {
        session.agent_type = "codex".to_string();
        session.engine_label = Some("Codex App".to_string());
        session.codex_app_server_thread_id = Some(thread_id.to_string());
        session.terminal = "Codex".to_string();
        session.term_bundle_id = Some("com.openai.codex".to_string());
        session.phase = SessionPhase::WaitingInput;
        session.description = Some(preview.to_string());
        session.pending_question = Some(pending_question);
        session.pending_permission = None;
        session.pending_plan = None;
    });
}

fn clear_codex_app_server_interaction(store: &SessionStore, thread_id: &str) {
    store.update_session(thread_id, |session| {
        session.pending_permission = None;
        session.pending_question = None;
        session.pending_plan = None;
        session.phase = SessionPhase::Processing;
    });
}

fn codex_app_server_permission_response(
    pending: &CodexAppServerPendingRequest,
    allowed: bool,
    always: bool,
) -> serde_json::Value {
    match pending.kind {
        CodexAppServerPendingKind::PermissionsApproval => serde_json::json!({
            "permissions": if allowed {
                pending.requested_permissions.clone().unwrap_or_else(|| serde_json::json!({}))
            } else {
                serde_json::json!({})
            },
            "scope": if allowed && always { "session" } else { "turn" }
        }),
        _ => serde_json::json!({
            "decision": if allowed {
                if always { "acceptForSession" } else { "accept" }
            } else {
                "decline"
            }
        }),
    }
}

fn codex_app_server_permission_summary(permissions: &serde_json::Value) -> String {
    let Some(object) = permissions.as_object() else {
        return "Codex requested extra permissions.".to_string();
    };
    if object.is_empty() {
        return "Codex requested extra permissions.".to_string();
    }
    object
        .keys()
        .map(|key| format!("Codex requested {key} permission."))
        .collect::<Vec<_>>()
        .join("\n")
}

fn codex_app_server_pending_question(
    params: &serde_json::Value,
    request_id: &serde_json::Value,
) -> PendingQuestion {
    let questions = params
        .get("questions")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .map(codex_app_server_question_item)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let first = questions.first();
    let question = first
        .map(|item| item.question.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Codex needs your input.".to_string());
    let options = first
        .map(|item| {
            item.options
                .iter()
                .map(|option| option.label.clone())
                .collect()
        })
        .unwrap_or_default();
    let descriptions = first
        .map(|item| {
            item.options
                .iter()
                .map(|option| option.description.clone().unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    PendingQuestion {
        question,
        options,
        descriptions,
        header: first.and_then(|item| item.header.clone()),
        multi_select: first.map(|item| item.multi_select).unwrap_or(false),
        questions,
        tool_use_id: Some(request_id_to_string(request_id)),
        source: Some("codex_app_server_request_user_input".to_string()),
        response_mode: Some("app_server".to_string()),
    }
}

fn codex_app_server_question_item(
    value: &serde_json::Value,
) -> crate::hooks::session_store::QuestionItem {
    let question = codex_string(value, "question").unwrap_or_default();
    let id = codex_string(value, "id");
    let header = codex_string(value, "header");
    let multi_select = value
        .get("isMultiple")
        .or_else(|| value.get("allowsMultiple"))
        .or_else(|| value.get("multiSelect"))
        .or_else(|| value.get("multiple"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let options = value
        .get("options")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .enumerate()
                .map(
                    |(index, option)| crate::hooks::session_store::QuestionOption {
                        label: codex_string(option, "label")
                            .unwrap_or_else(|| format!("Option {}", index + 1)),
                        description: codex_string(option, "description"),
                    },
                )
                .collect()
        })
        .unwrap_or_default();
    crate::hooks::session_store::QuestionItem {
        id,
        question,
        header,
        options,
        multi_select,
    }
}

fn request_id_to_string(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|value| value.to_string()))
        .unwrap_or_else(|| value.to_string())
}

fn codex_thread_list_from_response(response: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    let result = response.get("result")?;
    let threads = result
        .get("data")
        .or_else(|| result.get("threads"))
        .or_else(|| result.get("items"))?
        .as_array()?;
    Some(threads.clone())
}

fn sync_codex_app_server_thread_to_store(
    store: &SessionStore,
    thread: &serde_json::Value,
) -> Option<CodexAppServerThreadSummary> {
    let thread_id = codex_string(thread, "id")?;
    let name = codex_string(thread, "name");
    let preview = codex_string(thread, "preview");
    let cwd = codex_string(thread, "cwd")
        .or_else(|| codex_string(thread, "path"))
        .unwrap_or_else(|| "/".to_string());
    let phase = codex_phase_from_thread(thread);
    let status = thread
        .get("status")
        .and_then(|status| status.get("type"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let updated_at = codex_timestamp(thread.get("updatedAt").or_else(|| thread.get("updated_at")));
    let created_at = codex_timestamp(thread.get("createdAt").or_else(|| thread.get("created_at")));
    let (last_user_message, last_response) = codex_latest_messages_from_thread(thread);
    let project = name
        .clone()
        .or_else(|| preview.clone())
        .or_else(|| {
            Path::new(&cwd)
                .file_name()
                .and_then(|value| value.to_str())
                .map(str::to_string)
        })
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Codex".to_string());

    store.get_or_create_session(&thread_id, "codex", &project, &cwd, "Codex");
    store.update_session(&thread_id, |session| {
        session.agent_type = "codex".to_string();
        session.engine_label = Some("Codex App".to_string());
        session.codex_app_server_thread_id = Some(thread_id.clone());
        session.project = project.clone();
        session.cwd = cwd.clone();
        session.terminal = "Codex".to_string();
        session.term_bundle_id = Some("com.openai.codex".to_string());
        session.phase = if session.pending_permission.is_some() || session.pending_plan.is_some() {
            SessionPhase::WaitingApproval
        } else if session.pending_question.is_some() {
            SessionPhase::WaitingInput
        } else {
            phase.clone()
        };
        session.session_title = name.clone().or_else(|| preview.clone());
        session.description = preview.clone();
        if let Some(created_at) = created_at {
            session.started_at = created_at;
        }
        if let Some(updated_at) = updated_at {
            session.last_main_agent_at = Some(updated_at);
        }
        if last_user_message.is_some() {
            session.last_user_message = last_user_message.clone();
        }
        if last_response.is_some() {
            session.last_response = last_response.clone();
        }
    });

    Some(CodexAppServerThreadSummary {
        id: thread_id,
        name,
        preview,
        cwd: Some(cwd),
        status,
        phase: format!("{:?}", phase),
        updated_at,
    })
}

fn sync_remote_codex_thread_to_store(
    store: &SessionStore,
    host: &RemoteHost,
    thread: &crate::remote::installer::RemoteCodexThreadSnapshot,
) -> Option<CodexAppServerThreadSummary> {
    let thread_id = thread.id.trim();
    let cwd = thread.cwd.trim();
    if thread_id.is_empty() || cwd.is_empty() {
        return None;
    }

    let preview = thread
        .preview
        .clone()
        .or_else(|| thread.title.clone())
        .filter(|value| !value.trim().is_empty());
    let name = thread
        .title
        .clone()
        .or_else(|| preview.clone())
        .filter(|value| !value.trim().is_empty());
    let project = name
        .clone()
        .or_else(|| {
            Path::new(cwd)
                .file_name()
                .and_then(|value| value.to_str())
                .map(str::to_string)
        })
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Codex".to_string());
    let updated_at = (thread.updated_at_ms > 0).then_some(thread.updated_at_ms / 1000);
    let status = Some(
        thread
            .thread_source
            .clone()
            .or_else(|| thread.source.clone())
            .unwrap_or_else(|| "remote-state".to_string()),
    );

    store.get_or_create_session(thread_id, "codex", &project, cwd, &host.name);
    store.update_session(thread_id, |session| {
        session.agent_type = "codex".to_string();
        session.engine_label = Some(format!("Codex App · {}", host.name));
        session.codex_app_server_thread_id = None;
        session.project = project.clone();
        session.cwd = cwd.to_string();
        session.terminal = host.name.clone();
        session.term_bundle_id = None;
        session.pid = None;
        session.tty = None;
        session.remote_host_id = Some(host.id.clone());
        session.remote_host_name = Some(host.name.clone());
        session.phase = if session.pending_permission.is_some() || session.pending_plan.is_some() {
            SessionPhase::WaitingApproval
        } else if session.pending_question.is_some() {
            SessionPhase::WaitingInput
        } else {
            SessionPhase::Processing
        };
        session.session_title = name.clone();
        session.description = preview.clone();
        if let Some(updated_at) = updated_at {
            session.last_main_agent_at = Some(updated_at);
        }
        if let Some(title) = name
            .as_deref()
            .filter(|title| preview.as_deref() != Some(*title))
        {
            session.last_user_message = Some(title.to_string());
        }
        if let Some(preview) = preview.clone() {
            session.last_response = Some(preview);
        }
    });

    Some(CodexAppServerThreadSummary {
        id: thread_id.to_string(),
        name,
        preview,
        cwd: Some(cwd.to_string()),
        status,
        phase: format!("{:?}", SessionPhase::Processing),
        updated_at,
    })
}

fn codex_phase_from_thread(thread: &serde_json::Value) -> SessionPhase {
    codex_phase_from_status(thread.get("status"), None)
}

fn codex_phase_from_status(
    status: Option<&serde_json::Value>,
    pending_kind: Option<&CodexAppServerPendingKind>,
) -> SessionPhase {
    if matches!(
        pending_kind,
        Some(
            CodexAppServerPendingKind::CommandApproval
                | CodexAppServerPendingKind::FileApproval
                | CodexAppServerPendingKind::PermissionsApproval
        )
    ) {
        return SessionPhase::WaitingApproval;
    }
    if pending_kind == Some(&CodexAppServerPendingKind::UserInput) {
        return SessionPhase::WaitingInput;
    }

    let status_type = status
        .and_then(|status| status.get("type"))
        .and_then(|value| value.as_str());
    match status_type {
        Some("active") | Some("running") | Some("processing") => {
            let flags = status
                .and_then(|status| status.get("activeFlags"))
                .or_else(|| status.and_then(|status| status.get("active_flags")))
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if flags.contains(&"waitingOnApproval") {
                SessionPhase::WaitingApproval
            } else if flags.contains(&"waitingOnUserInput") {
                SessionPhase::WaitingInput
            } else {
                SessionPhase::Processing
            }
        }
        Some("error") | Some("failed") => SessionPhase::Error,
        _ => SessionPhase::Idle,
    }
}

fn codex_latest_messages_from_thread(
    thread: &serde_json::Value,
) -> (Option<String>, Option<String>) {
    let mut latest_user = None;
    let mut latest_agent = None;
    let Some(turns) = thread.get("turns").and_then(|value| value.as_array()) else {
        return (latest_user, latest_agent);
    };

    for item in turns
        .iter()
        .filter_map(|turn| turn.get("items").and_then(|items| items.as_array()))
        .flatten()
    {
        match item.get("type").and_then(|value| value.as_str()) {
            Some("userMessage") => {
                if let Some(text) = codex_user_message_text(item) {
                    latest_user = Some(text);
                }
            }
            Some("agentMessage") => {
                if let Some(text) = codex_string(item, "text") {
                    latest_agent = Some(text);
                }
            }
            _ => {}
        }
    }

    (latest_user, latest_agent)
}

fn codex_user_message_text(item: &serde_json::Value) -> Option<String> {
    if let Some(text) = codex_string(item, "text") {
        return Some(text);
    }
    let content = item.get("content")?.as_array()?;
    let text = content
        .iter()
        .filter_map(|part| {
            part.as_str()
                .map(str::to_string)
                .or_else(|| codex_string(part, "text"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn codex_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn codex_timestamp(value: Option<&serde_json::Value>) -> Option<i64> {
    match value? {
        serde_json::Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value.round() as i64)),
        serde_json::Value::String(value) => chrono::DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|value| value.timestamp())
            .or_else(|| value.parse::<i64>().ok()),
        _ => None,
    }
}

fn codex_usage_snapshot_from_rpc_message(
    message: &serde_json::Value,
) -> Option<UsageRateLimitSnapshot> {
    let rate_limits = message
        .get("result")?
        .get("rateLimits")
        .or_else(|| message.get("result")?.get("rate_limits"))?;
    let (five_hour, seven_day) = codex_window_pair(rate_limits)?;
    let captured_at = Some(chrono::Utc::now());

    Some(UsageRateLimitSnapshot {
        rate_limits: provider_rate_limits(
            "codex",
            "Codex",
            "codex-cli",
            captured_at,
            five_hour,
            seven_day,
        )?,
        captured_at,
    })
}

fn load_codex_usage_rate_limits_from_jsonl() -> Option<UsageRateLimitSnapshot> {
    let root = dirs::home_dir()?.join(".codex").join("sessions");
    let mut candidates = Vec::new();
    collect_codex_rollout_files(&root, &mut candidates);
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));

    let mut best: Option<UsageRateLimitSnapshot> = None;
    for (path, modified_at) in candidates.into_iter().take(40) {
        let Some(snapshot) = load_codex_usage_rate_limits_from_file(&path, modified_at) else {
            continue;
        };
        if !usage_snapshot_within_age(&snapshot, chrono::Duration::hours(6)) {
            continue;
        }
        let snapshot_time = snapshot
            .captured_at
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC);
        let best_time = best
            .as_ref()
            .and_then(|candidate| candidate.captured_at)
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC);
        if snapshot_time >= best_time {
            best = Some(snapshot);
        }
    }
    best
}

fn collect_codex_rollout_files(
    root: &Path,
    candidates: &mut Vec<(PathBuf, std::time::SystemTime)>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };

        if metadata.is_dir() {
            collect_codex_rollout_files(&path, candidates);
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if file_name.starts_with("rollout-")
            && path.extension().and_then(|value| value.to_str()) == Some("jsonl")
        {
            candidates.push((
                path,
                metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            ));
        }
    }
}

fn load_codex_usage_rate_limits_from_file(
    path: &Path,
    modified_at: std::time::SystemTime,
) -> Option<UsageRateLimitSnapshot> {
    let file = fs::File::open(path).ok()?;
    let fallback_time = chrono::DateTime::<chrono::Utc>::from(modified_at);
    let mut latest: Option<UsageRateLimitSnapshot> = None;

    for line in StdBufReader::new(file).lines().map_while(Result::ok) {
        let Some(snapshot) = codex_usage_snapshot_from_line(&line, fallback_time) else {
            continue;
        };
        latest = Some(snapshot);
    }

    latest
}

fn codex_usage_snapshot_from_line(
    line: &str,
    fallback_time: chrono::DateTime<chrono::Utc>,
) -> Option<UsageRateLimitSnapshot> {
    let object: serde_json::Value = serde_json::from_str(line).ok()?;
    if object.get("type").and_then(|value| value.as_str()) != Some("event_msg") {
        return None;
    }

    let payload = object.get("payload")?;
    if payload.get("type").and_then(|value| value.as_str()) != Some("token_count") {
        return None;
    }

    let rate_limits = payload.get("rate_limits")?;
    let (five_hour, seven_day) = codex_window_pair(rate_limits)?;

    Some(UsageRateLimitSnapshot {
        rate_limits: provider_rate_limits(
            "codex",
            "Codex",
            "codex-jsonl",
            object
                .get("timestamp")
                .and_then(date_from_value)
                .or(Some(fallback_time)),
            five_hour,
            seven_day,
        )?,
        captured_at: object
            .get("timestamp")
            .and_then(date_from_value)
            .or(Some(fallback_time)),
    })
}

fn codex_window_pair(
    rate_limits: &serde_json::Value,
) -> Option<(&serde_json::Value, &serde_json::Value)> {
    let primary = rate_limits.get("primary")?;
    let secondary = rate_limits.get("secondary")?;
    let primary_minutes = positive_window_minutes(primary)?;
    let secondary_minutes = positive_window_minutes(secondary)?;
    let primary_is_long = primary_minutes >= 1_440;
    let secondary_is_long = secondary_minutes >= 1_440;

    match (primary_is_long, secondary_is_long) {
        (false, true) => Some((primary, secondary)),
        (true, false) => Some((secondary, primary)),
        _ => None,
    }
}

fn usage_snapshot_within_age(snapshot: &UsageRateLimitSnapshot, max_age: chrono::Duration) -> bool {
    let Some(captured_at) = snapshot.captured_at else {
        return false;
    };
    let age = chrono::Utc::now().signed_duration_since(captured_at);
    age >= -chrono::Duration::minutes(5) && age <= max_age
}

fn provider_rate_limits(
    provider: &str,
    provider_label: &str,
    source: &str,
    captured_at: Option<chrono::DateTime<chrono::Utc>>,
    five_hour: &serde_json::Value,
    seven_day: &serde_json::Value,
) -> Option<RateLimitInfo> {
    let five_hour_usage = used_percentage(five_hour)?;
    let seven_day_usage = used_percentage(seven_day)?;
    Some(RateLimitInfo {
        five_hour_usage,
        five_hour_remaining: remaining_label(five_hour),
        seven_day_usage,
        seven_day_remaining: remaining_label(seven_day),
        provider: Some(provider.to_string()),
        provider_label: Some(provider_label.to_string()),
        source: Some(source.to_string()),
        updated_at: captured_at.map(|date| date.timestamp_millis()),
        windows: vec![
            usage_window("five_hour", "5h", five_hour, Some(300))?,
            usage_window("seven_day", "7d", seven_day, Some(10_080))?,
        ],
    })
}

fn usage_window(
    id: &str,
    title: &str,
    value: &serde_json::Value,
    fallback_minutes: Option<i64>,
) -> Option<UsageRateWindow> {
    let used_percent = used_percentage(value)?;
    Some(UsageRateWindow {
        id: id.to_string(),
        title: title.to_string(),
        used_percent,
        remaining_percent: Some((100.0 - used_percent).clamp(0.0, 100.0)),
        remaining_label: Some(remaining_label(value)).filter(|text| !text.is_empty()),
        resets_at: reset_at_iso(value),
        window_minutes: window_minutes(value).or(fallback_minutes),
    })
}

fn usage_percentage(value: &serde_json::Value) -> Option<f64> {
    number_field(value, "used_percentage")
        .or_else(|| number_field(value, "usedPercentage"))
        .or_else(|| number_field(value, "utilization"))
}

fn used_percentage(value: &serde_json::Value) -> Option<f64> {
    usage_percentage(value)
        .or_else(|| number_field(value, "used_percent"))
        .or_else(|| number_field(value, "usedPercent"))
}

fn window_minutes(value: &serde_json::Value) -> Option<i64> {
    value
        .get("window_minutes")
        .or_else(|| value.get("windowMinutes"))
        .or_else(|| value.get("windowDurationMins"))
        .and_then(number_from_value)
        .map(|value| value as i64)
        .or_else(|| {
            value
                .get("limit_window_seconds")
                .or_else(|| value.get("limitWindowSeconds"))
                .and_then(number_from_value)
                .map(|value| (value / 60.0).round() as i64)
        })
        .filter(|minutes| *minutes > 0)
}

fn positive_window_minutes(value: &serde_json::Value) -> Option<i64> {
    window_minutes(value).filter(|minutes| *minutes > 0)
}

fn reset_at_iso(value: &serde_json::Value) -> Option<String> {
    value
        .get("resets_at")
        .or_else(|| value.get("resetsAt"))
        .or_else(|| value.get("reset_at"))
        .or_else(|| value.get("resetAt"))
        .and_then(date_from_value)
        .map(|date| date.to_rfc3339())
}

fn number_field(value: &serde_json::Value, key: &str) -> Option<f64> {
    value.get(key).and_then(number_from_value)
}

fn number_from_value(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
}

fn remaining_label(value: &serde_json::Value) -> String {
    if let Some(text) = value
        .get("remaining")
        .or_else(|| value.get("remainingLabel"))
        .and_then(|value| value.as_str())
        .filter(|text| !text.trim().is_empty())
    {
        return text.to_string();
    }

    value
        .get("resets_at")
        .or_else(|| value.get("resetsAt"))
        .or_else(|| value.get("reset_at"))
        .or_else(|| value.get("resetAt"))
        .and_then(date_from_value)
        .and_then(|date| format_remaining_duration(date, chrono::Utc::now()))
        .unwrap_or_default()
}

fn date_from_value(value: &serde_json::Value) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Some(seconds) = number_from_value(value) {
        return chrono::DateTime::<chrono::Utc>::from_timestamp(seconds as i64, 0);
    }

    let text = value.as_str()?.trim();
    if let Ok(seconds) = text.parse::<f64>() {
        return chrono::DateTime::<chrono::Utc>::from_timestamp(seconds as i64, 0);
    }

    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|date| date.with_timezone(&chrono::Utc))
}

fn format_remaining_duration(
    reset_at: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    let seconds = reset_at.signed_duration_since(now).num_seconds();
    if seconds <= 0 {
        return None;
    }

    let total_minutes = seconds / 60;
    let days = total_minutes / 1_440;
    let hours = (total_minutes % 1_440) / 60;
    let minutes = total_minutes % 60;

    if days > 0 && hours > 0 {
        Some(format!("{days}d{hours}h"))
    } else if days > 0 {
        Some(format!("{days}d"))
    } else if hours > 0 && minutes > 0 {
        Some(format!("{hours}h{minutes}m"))
    } else if hours > 0 {
        Some(format!("{hours}h"))
    } else if minutes > 0 {
        Some(format!("{minutes}m"))
    } else {
        Some("<1m".to_string())
    }
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

    match state
        .codex_app_server
        .respond_permission(&session_id, allowed, always)
        .await
    {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(err) => log::warn!(
            "Codex app-server permission response failed for {}: {}",
            session_id,
            err
        ),
    }

    // Try hook socket first
    let hook_result = state
        .hook_server
        .respond_permission(&session_id, allowed, always)
        .await;

    match hook_result {
        Ok(()) => Ok(()),
        Err(e) => {
            #[cfg(target_os = "windows")]
            {
                return Err(format!(
                    "Hook response failed on Windows: {e}. Make sure the AgentBro hook TCP bridge is running, then retry from the island."
                ));
            }
            #[cfg(not(target_os = "windows"))]
            {
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
                    crate::terminal::approval::reject(&tmux_target, None)
                        .map_err(|e| e.to_string())?;
                }

                // Clear pending permission since we handled it via tmux
                state
                    .session_store
                    .set_pending_permission(&session_id, None);
                Ok(())
            }
        }
    }
}

#[tauri::command]
pub async fn send_message(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    message: String,
    activate_before_send: Option<bool>,
) -> Result<(), String> {
    log::info!("Send message: session={}, msg={}", session_id, message);
    release_notch_keyboard_focus(&app);

    let session = state
        .session_store
        .get_session(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    if is_codex_desktop_session(&session) {
        #[cfg(target_os = "windows")]
        {
            let _ = activate_before_send;
            let _ = message;
            return Err(codex_desktop_windows_message_error(None));
        }

        #[cfg(not(target_os = "windows"))]
        {
            // When the persistent app-server bridge is live we can route the
            // user turn straight through JSON-RPC, no clipboard/AppleScript
            // round-trip, no app activation. macOS can still fall back to
            // AppleScript when the bridge is unavailable.
            let codex_thread_id = session
                .codex_app_server_thread_id
                .as_deref()
                .unwrap_or(session_id.as_str());
            let mut app_server_error = None;
            match state
                .codex_app_server
                .send_user_turn(codex_thread_id, &message)
                .await
            {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    log::debug!(
                        "Codex app-server bridge not attached for session {} thread {}",
                        session_id,
                        codex_thread_id
                    );
                }
                Err(err) => {
                    log::warn!(
                        "Codex app-server turn/steer failed for session {} thread {}: {}",
                        session_id,
                        codex_thread_id,
                        err
                    );
                    app_server_error = Some(err);
                }
            }

            #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
            {
                let _ = activate_before_send;
                let _ = message;
                return Err(
                "Codex Desktop message sending is only supported via app-server on this platform."
                    .to_string(),
            );
            }

            #[cfg(target_os = "macos")]
            if activate_before_send.unwrap_or(true) {
                return send_message_to_codex_desktop(&session, &message);
            }

            #[cfg(target_os = "macos")]
            return send_message_to_codex_desktop_without_activation(&session, &message);
        }
    }

    if is_qoder_app_session(&session) {
        return send_message_to_qoder_app(&session, &message, activate_before_send.unwrap_or(true));
    }

    if let Some(err) = app_host_message_unsupported_error(&session) {
        return Err(err);
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

#[cfg(target_os = "windows")]
fn codex_desktop_windows_message_error(app_server_error: Option<&str>) -> String {
    let trimmed_error = app_server_error
        .map(str::trim)
        .filter(|error| !error.is_empty());
    let detail = trimmed_error
        .map(str::trim)
        .map(|error| format!(" Last app-server error: {error}"))
        .unwrap_or_default();
    format!(
        "Codex Desktop free-form replies are not supported on Windows yet. Codex Desktop keeps its app-server bridge private to the desktop app, so AgentBro cannot safely inject a new user turn. Open Codex Desktop and reply there for now.{detail}"
    )
}

fn is_codex_desktop_session(session: &SessionState) -> bool {
    let terminal = session.terminal.trim();

    if session.agent_type != "codex" {
        return false;
    }

    if session
        .term_bundle_id
        .as_deref()
        .is_some_and(is_codex_app_bundle)
    {
        return true;
    }

    if let Some(meta) = read_codex_session_meta(&session.id) {
        let originator = meta.originator.unwrap_or_default().to_ascii_lowercase();
        if originator.contains("desktop") {
            return true;
        }
        if originator.contains("tui") || originator.contains("cli") {
            return false;
        }

        let source = meta.source.unwrap_or_default().to_ascii_lowercase();
        if source == "cli" {
            return false;
        }
        if source == "vscode" || source == "desktop" {
            return true;
        }
    }

    let missing_tty = session
        .tty
        .as_deref()
        .is_none_or(|tty| tty.trim().is_empty());
    missing_tty
        && !terminal.starts_with("/dev/")
        && (terminal.is_empty() || terminal.to_ascii_lowercase().contains("codex"))
}

fn is_codex_app_bundle(bundle_id: &str) -> bool {
    bundle_id.to_ascii_lowercase().contains("openai.codex")
}

fn is_qoder_app_bundle(bundle_id: &str) -> bool {
    let lower = bundle_id.to_ascii_lowercase();
    lower == "com.qoder.ide" || lower == "com.qoder.ide.helper"
}

fn is_qoder_app_session(session: &SessionState) -> bool {
    session.agent_type == "qoder"
        && (session
            .term_bundle_id
            .as_deref()
            .is_some_and(is_qoder_app_bundle)
            || session.terminal.to_ascii_lowercase().contains("qoder"))
}

fn native_app_bundle_matches_session(session: &SessionState, bundle_id: &str) -> bool {
    let lower = bundle_id.to_ascii_lowercase();
    matches!(
        (session.agent_type.as_str(), lower.as_str()),
        ("codex", "com.openai.codex")
            | ("cursor", "com.todesktop.230313mzl4w4u92")
            | ("cursor-cli", "com.todesktop.230313mzl4w4u92")
            | ("qoder", "com.qoder.ide")
            | ("qoder-cli", "com.qoder.ide")
            | ("droid", "com.factory.app")
            | ("codebuddy", "com.tencent.codebuddy")
            | ("codebuddycn", "com.tencent.codebuddy.cn")
            | ("codybuddycn", "com.tencent.codebuddy.cn")
            | ("stepfun", "com.stepfun.app")
            | ("opencode", "ai.opencode.desktop")
            | ("workbuddy", "com.workbuddy.workbuddy")
    )
}

fn is_known_ide_or_agent_host_bundle(bundle_id: &str) -> bool {
    let lower = bundle_id.to_ascii_lowercase();
    lower.contains("vscode")
        || lower.contains("vscodium")
        || lower.contains("todesktop.230313mzl4w4u92")
        || lower.contains("cursor")
        || lower.contains("windsurf")
        || lower.contains("codeium")
        || lower.contains("zed")
        || lower.contains("jetbrains")
        || lower.contains("xcode")
        || lower == "com.apple.dt.xcode"
        || lower.contains("panic.nova")
        || lower.contains("android.studio")
        || lower.contains("antigravity")
        || lower == "com.qoder.ide"
        || lower == "com.qoder.ide.helper"
        || lower == "com.factory.app"
        || lower == "com.tencent.codebuddy"
        || lower == "com.tencent.codebuddy.cn"
        || lower == "com.stepfun.app"
        || lower == "ai.opencode.desktop"
        || lower == "com.workbuddy.workbuddy"
}

fn is_ide_terminal_session(session: &SessionState) -> bool {
    let Some(bundle_id) = session
        .term_bundle_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };

    if crate::terminal::registry::is_terminal_bundle(bundle_id)
        && !is_known_ide_or_agent_host_bundle(bundle_id)
    {
        return false;
    }

    is_known_ide_or_agent_host_bundle(bundle_id)
        && !native_app_bundle_matches_session(session, bundle_id)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CodexSessionMeta {
    originator: Option<String>,
    source: Option<String>,
}

fn read_codex_session_meta(session_id: &str) -> Option<CodexSessionMeta> {
    let path = discover_codex_session_file(session_id)?;
    read_codex_session_meta_from_path(&path)
}

fn read_codex_session_meta_from_path(path: &Path) -> Option<CodexSessionMeta> {
    let file = fs::File::open(path).ok()?;
    let reader = StdBufReader::new(file);

    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(payload) = codex_session_meta_payload(&entry) else {
            continue;
        };

        return Some(CodexSessionMeta {
            originator: payload
                .get("originator")
                .or_else(|| entry.get("originator"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string),
            source: payload
                .get("source")
                .or_else(|| entry.get("source"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string),
        });
    }

    None
}

fn codex_session_meta_payload(entry: &serde_json::Value) -> Option<&serde_json::Value> {
    if entry.get("type").and_then(|value| value.as_str()) == Some("session_meta") {
        return entry.get("payload").or(Some(entry));
    }

    let payload = entry.get("payload")?;
    if payload.get("type").and_then(|value| value.as_str()) == Some("session_meta") {
        return payload.get("payload").or(Some(payload));
    }

    None
}

fn app_host_bundle_id(session: &SessionState) -> Option<&str> {
    let bundle_id = session.term_bundle_id.as_deref()?.trim();
    if bundle_id.is_empty() || crate::terminal::registry::is_terminal_bundle(bundle_id) {
        return None;
    }
    if is_known_ide_or_agent_host_bundle(bundle_id)
        && !native_app_bundle_matches_session(session, bundle_id)
    {
        return None;
    }
    Some(bundle_id)
}

fn app_host_display_name(session: &SessionState) -> &'static str {
    if session
        .term_bundle_id
        .as_deref()
        .is_some_and(is_codex_app_bundle)
    {
        "Codex App"
    } else if session
        .term_bundle_id
        .as_deref()
        .is_some_and(is_qoder_app_bundle)
    {
        "Qoder App"
    } else {
        "App-hosted"
    }
}

fn app_host_message_unsupported_error(session: &SessionState) -> Option<String> {
    if app_host_bundle_id(session).is_some()
        && !is_codex_desktop_session(session)
        && !is_qoder_app_session(session)
    {
        return Some(format!(
            "{} sessions do not support AgentBro message injection yet. Open the app to continue.",
            app_host_display_name(session)
        ));
    }

    None
}

fn open_app_host_session(session: &SessionState) -> Result<(), String> {
    if is_codex_desktop_session(session) {
        return open_codex_desktop_session(session);
    }

    let bundle_id = app_host_bundle_id(session)
        .ok_or_else(|| "Session has no app bundle metadata to jump to".to_string())?;
    open_bundle_id(bundle_id)
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

fn open_codex_desktop_session(session: &SessionState) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return open_codex_desktop_session_macos(session);
    }

    #[cfg(target_os = "windows")]
    {
        return open_codex_desktop_session_windows(session);
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let _ = session;
        Err("Codex Desktop session jumping is only supported on macOS and Windows".to_string())
    }
}

#[cfg(target_os = "macos")]
fn open_codex_desktop_session_macos(session: &SessionState) -> Result<(), String> {
    let opened_thread = is_uuid_like(&session.id)
        && std::process::Command::new("/usr/bin/open")
            .arg(format!("codex://threads/{}", session.id))
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

    if opened_thread {
        let _ = activate_codex_desktop_app(session.pid);
        return Ok(());
    }

    if activate_codex_desktop_app(session.pid) {
        Ok(())
    } else {
        Err("Failed to activate Codex Desktop".to_string())
    }
}

#[cfg(target_os = "windows")]
fn open_codex_desktop_session_windows(_session: &SessionState) -> Result<(), String> {
    let mut errors = Vec::new();

    for app_id in crate::agents::executable::codex_desktop_app_user_model_ids() {
        match open_windows_app_user_model_id(&app_id) {
            Ok(()) => return Ok(()),
            Err(err) => errors.push(format!("{app_id}: {err}")),
        }
    }

    for path in crate::agents::executable::codex_desktop_app_candidates()
        .into_iter()
        .filter(|path| path.exists())
    {
        let target = path.to_string_lossy().to_string();
        match open_windows_shell_target(&target) {
            Ok(()) => return Ok(()),
            Err(err) => errors.push(format!("{target}: {err}")),
        }
    }

    Err(if errors.is_empty() {
        "Codex Desktop was not found. Install or launch Codex Desktop, then try again.".to_string()
    } else {
        format!("Failed to open Codex Desktop: {}", errors.join("; "))
    })
}

#[cfg(target_os = "windows")]
fn open_windows_app_user_model_id(app_id: &str) -> Result<(), String> {
    let app_id = clean_windows_app_user_model_id(app_id)
        .ok_or_else(|| format!("Invalid Windows app id: {app_id}"))?;
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
$shell = New-Object -ComObject Shell.Application
$folder = $shell.Namespace('shell:AppsFolder')
if ($null -eq $folder) {{ throw 'AppsFolder is unavailable' }}
$item = $folder.ParseName({})
if ($null -eq $item) {{ throw 'App is not installed' }}
$item.InvokeVerb('open')
"#,
        powershell_string_literal(&app_id)
    );

    match run_windows_powershell(&script) {
        Ok(()) => Ok(()),
        Err(primary) => {
            let target = format!("shell:AppsFolder\\{app_id}");
            open_windows_explorer_target(&target)
                .map_err(|fallback| format!("{primary}; explorer fallback failed: {fallback}"))
        }
    }
}

#[cfg(target_os = "windows")]
fn clean_windows_app_user_model_id(value: &str) -> Option<String> {
    let trimmed = value
        .split(['?', '&', '#'])
        .next()
        .unwrap_or(value)
        .trim()
        .trim_matches('"')
        .trim_matches('\'');
    if trimmed.is_empty()
        || trimmed.contains('\\')
        || trimmed.contains('/')
        || !trimmed.contains('!')
    {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(target_os = "windows")]
fn run_windows_powershell(script: &str) -> Result<(), String> {
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|err| format!("Failed to run PowerShell app activation: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Err(if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("powershell exited with status {}", output.status)
        })
    }
}

#[cfg(target_os = "windows")]
fn powershell_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(target_os = "windows")]
fn open_windows_explorer_target(target: &str) -> Result<(), String> {
    let output = std::process::Command::new("explorer.exe")
        .arg(target)
        .output()
        .map_err(|err| format!("Failed to open {target}: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Err(if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("explorer exited with status {}", output.status)
        })
    }
}

#[cfg(target_os = "windows")]
fn open_windows_shell_target(target: &str) -> Result<(), String> {
    let output = std::process::Command::new("cmd")
        .args(["/C", "start", "", target])
        .output()
        .map_err(|err| format!("Failed to open {target}: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Err(if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("cmd start exited with status {}", output.status)
        })
    }
}

#[cfg(target_os = "macos")]
fn activate_codex_desktop_app(pid: Option<u32>) -> bool {
    if std::process::Command::new("/usr/bin/open")
        .args(["-a", "Codex"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
    {
        return true;
    }

    if let Some(pid) = pid {
        let script = format!(
            r#"tell application "System Events"
  set matchingProcesses to application processes whose unix id is {pid}
  if (count of matchingProcesses) > 0 then
    set frontmost of item 1 of matchingProcesses to true
    return "ok"
  end if
end tell"#
        );
        if osascript_ok(&script) {
            return true;
        }
    }

    osascript_ok(
        r#"tell application "System Events"
  set matchingProcesses to application processes whose name is "Codex"
  if (count of matchingProcesses) is 0 then
    set matchingProcesses to application processes whose name contains "Codex"
  end if
  if (count of matchingProcesses) > 0 then
    set frontmost of item 1 of matchingProcesses to true
    return "ok"
  end if
end tell"#,
    ) || osascript_ok(r#"tell application "Codex" to activate"#)
}

#[cfg(target_os = "macos")]
fn send_message_to_codex_desktop(session: &SessionState, message: &str) -> Result<(), String> {
    open_codex_desktop_session(session)?;
    std::thread::sleep(Duration::from_millis(300));

    let script = codex_desktop_send_message_script(message);
    let output = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
        .map_err(|err| format!("Failed to run Codex Desktop send script: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "Failed to send message to Codex Desktop".to_string()
        } else {
            stderr
        })
    }
}

#[cfg(any(target_os = "macos", test))]
fn codex_desktop_send_message_script(message: &str) -> String {
    let message_literal = applescript_string_literal(message);
    format!(
        r#"set previousClipboard to missing value
try
  set previousClipboard to the clipboard
end try
set the clipboard to {message_literal}
set sendError to missing value
try
  tell application "System Events"
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
  end tell
on error errMsg
  set sendError to errMsg
end try
if previousClipboard is not missing value then
  delay 0.2
  set the clipboard to previousClipboard
end if
if sendError is not missing value then
  error sendError
end if"#
    )
}

fn send_message_to_qoder_app(
    session: &SessionState,
    message: &str,
    activate_before_send: bool,
) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("Qoder App message sending is only supported on macOS".to_string());
    }
    if !activate_before_send {
        return Err(
            "Qoder App message sending requires Jump Before Send so the editor can receive focus."
                .to_string(),
        );
    }

    if !activate_qoder_app(session.pid) {
        return Err("Failed to activate Qoder App".to_string());
    }
    std::thread::sleep(Duration::from_millis(300));

    let script = qoder_app_send_message_script(message, session.pid);
    let output = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
        .map_err(|err| format!("Failed to run Qoder App send script: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "Failed to send message to Qoder App".to_string()
        } else {
            stderr
        })
    }
}

fn activate_qoder_app(pid: Option<u32>) -> bool {
    if std::process::Command::new("/usr/bin/open")
        .args(["-b", "com.qoder.ide"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
    {
        return true;
    }

    if let Some(pid) = pid {
        let script = format!(
            r#"tell application "System Events"
  set matchingProcesses to (application processes whose unix id is {pid})
  if (count of matchingProcesses) > 0 then
    set qoderProcess to item 1 of matchingProcesses
    try
      if bundle identifier of qoderProcess is "com.qoder.ide" then
        set frontmost of qoderProcess to true
        return "ok"
      end if
    end try
  end if
end tell"#
        );
        if osascript_ok(&script) {
            return true;
        }
    }

    osascript_ok(
        r#"tell application "System Events"
  set matchingProcesses to (application processes whose bundle identifier is "com.qoder.ide")
  if (count of matchingProcesses) > 0 then
    set frontmost of item 1 of matchingProcesses to true
    return "ok"
  end if
end tell"#,
    )
}

fn qoder_app_send_message_script(message: &str, pid: Option<u32>) -> String {
    let message_literal = applescript_string_literal(message);
    let pid_lookup = pid
        .map(|pid| {
            format!(
                r#"set matchingProcesses to (application processes whose unix id is {pid})
  if (count of matchingProcesses) > 0 then
    set candidateProcess to item 1 of matchingProcesses
    try
      if bundle identifier of candidateProcess is "com.qoder.ide" then
        set qoderProcess to candidateProcess
      end if
    end try
  end if"#
            )
        })
        .unwrap_or_default();

    format!(
        r#"set previousClipboard to missing value
try
  set previousClipboard to the clipboard
end try
set the clipboard to {message_literal}
set sendError to missing value
try
  tell application "System Events"
    set qoderProcess to missing value
    {pid_lookup}
    if qoderProcess is missing value then
      set matchingProcesses to (application processes whose bundle identifier is "com.qoder.ide")
      if (count of matchingProcesses) > 0 then
        set qoderProcess to item 1 of matchingProcesses
      end if
    end if
    if qoderProcess is missing value then error "Qoder App is not running"
    set frontmost of qoderProcess to true
    delay 0.15
    if (count of windows of qoderProcess) is 0 then error "Qoder App has no open windows"

    set targetWindow to front window of qoderProcess
    set inputFields to (entire contents of targetWindow) whose (role is "AXTextArea" or role is "AXTextField")
    if (count of inputFields) is 0 then error "Could not find Qoder message input"
    set inputField to item (count of inputFields) of inputFields
    try
      set focused of inputField to true
    end try
    delay 0.05
    keystroke "a" using command down
    keystroke "v" using command down
    delay 0.35

    set didSend to false
    set sendButtons to (entire contents of targetWindow) whose role is "AXButton" and (name is "Send message" or description is "Send message" or name is "Send" or description is "Send" or name is "发送" or description is "发送")
    repeat with sendButton in sendButtons
      try
        if enabled of sendButton is not false then
          click sendButton
          set didSend to true
          exit repeat
        end if
      end try
    end repeat
    if didSend is false then
      key code 36
    end if
  end tell
on error errMsg
  set sendError to errMsg
end try
if previousClipboard is not missing value then
  delay 0.2
  set the clipboard to previousClipboard
end if
if sendError is not missing value then
  error sendError
end if"#
    )
}

#[cfg(target_os = "macos")]
fn send_message_to_codex_desktop_without_activation(
    session: &SessionState,
    message: &str,
) -> Result<(), String> {
    open_codex_desktop_session_in_background(session)?;
    std::thread::sleep(Duration::from_millis(500));

    let script = codex_desktop_send_message_without_activation_script(message, session.pid);
    let output = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
        .map_err(|err| format!("Failed to run Codex Desktop background send script: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "Failed to send message to Codex Desktop without activating it. Turn on Jump Before Send or switch to the Codex App thread and try again.".to_string()
        } else {
            stderr
        })
    }
}

#[cfg(target_os = "macos")]
fn open_codex_desktop_session_in_background(session: &SessionState) -> Result<(), String> {
    if !is_uuid_like(&session.id) {
        return Err("Codex App background sending requires a thread UUID. Turn on Jump Before Send to continue.".to_string());
    }

    let output = std::process::Command::new("/usr/bin/open")
        .args(["-g", &format!("codex://threads/{}", session.id)])
        .output()
        .map_err(|err| format!("Failed to open Codex thread in the background: {err}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "Failed to open Codex thread in the background".to_string()
        } else {
            stderr
        })
    }
}

#[cfg(any(target_os = "macos", test))]
fn codex_desktop_send_message_without_activation_script(message: &str, pid: Option<u32>) -> String {
    let message_literal = applescript_string_literal(message);
    let pid_lookup = pid
        .map(|pid| {
            format!(
                r#"set matchingProcesses to application processes whose unix id is {pid}
    if (count of matchingProcesses) > 0 then
      set codexProcess to item 1 of matchingProcesses
    end if"#
            )
        })
        .unwrap_or_default();

    format!(
        r#"set messageText to {message_literal}
tell application "System Events"
  set codexProcess to missing value
  {pid_lookup}
  if codexProcess is missing value then
    set matchingProcesses to application processes whose name is "Codex"
    if (count of matchingProcesses) is 0 then
      set matchingProcesses to application processes whose name contains "Codex"
    end if
    if (count of matchingProcesses) > 0 then
      set codexProcess to item 1 of matchingProcesses
    end if
  end if
  if codexProcess is missing value then error "Codex App is not running"
  if (count of windows of codexProcess) is 0 then error "Codex App has no open windows"

  set targetWindow to front window of codexProcess
  set inputFields to (entire contents of targetWindow) whose (role is "AXTextArea" or role is "AXTextField")
  set didInput to false
  repeat with inputIndex from (count of inputFields) to 1 by -1
    set inputField to item inputIndex of inputFields
    try
      set value of inputField to messageText
      set didInput to true
      exit repeat
    end try
  end repeat
  if didInput is false then error "Could not find Codex message input"

  set sendButtons to (entire contents of targetWindow) whose role is "AXButton" and (name is "Send" or description is "Send" or name is "发送" or description is "发送")
  if (count of sendButtons) > 0 then
    perform action "AXPress" of item 1 of sendButtons
  else
    error "Could not find Codex send button"
  end if
end tell"#
    )
}

#[cfg(any(target_os = "macos", test))]
fn is_uuid_like(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().copied().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn open_bundle_id(bundle_id: &str) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("App bundle jumping is only supported on macOS".to_string());
    }

    let output = std::process::Command::new("/usr/bin/open")
        .args(["-b", bundle_id])
        .output()
        .map_err(|e| format!("Failed to activate app bundle {bundle_id}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("Failed to activate app bundle {bundle_id}")
        } else {
            stderr
        })
    }
}

fn resolve_codex_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CODEX_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    if let Some(path) = crate::agents::executable::find_codex_cli_binary() {
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

    if let Some(session) = state.session_store.get_session(&session_id) {
        if let Some(question) = session.pending_question.clone() {
            if is_codex_app_server_question(&question) {
                let answers = codex_answers_for_pending_question(&question, &answer);
                match state
                    .codex_app_server
                    .respond_question(&session_id, answers)
                    .await
                {
                    Ok(true) => return Ok(()),
                    Ok(false) => {}
                    Err(err) => log::warn!(
                        "Codex app-server question response failed for {}: {}",
                        session_id,
                        err
                    ),
                }
            }
            if is_codex_rollout_question(&question) {
                let call_id = question
                    .tool_use_id
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "Codex question is missing call_id".to_string())?;
                let answers = codex_answers_for_pending_question(&question, &answer);
                submit_codex_request_user_input_output(&session_id, call_id, answers).await?;
                state.session_store.set_pending_question(&session_id, None);
                state.session_store.update_phase(
                    &session_id,
                    crate::hooks::session_store::SessionPhase::Processing,
                );
                return Ok(());
            }
        }
    }

    state
        .hook_server
        .respond_question(&session_id, answer)
        .await
        .map_err(|e| e.to_string())
}

fn is_codex_rollout_question(question: &PendingQuestion) -> bool {
    question.source.as_deref() == Some("codex_rollout_request_user_input")
        && question.response_mode.as_deref() == Some("external_only")
}

fn is_codex_app_server_question(question: &PendingQuestion) -> bool {
    question.source.as_deref() == Some("codex_app_server_request_user_input")
        && question.response_mode.as_deref() == Some("app_server")
}

fn codex_answers_for_pending_question(
    question: &PendingQuestion,
    answer: &str,
) -> BTreeMap<String, Vec<String>> {
    let mut answers = BTreeMap::new();

    if let Ok(serde_json::Value::Object(object)) = serde_json::from_str::<serde_json::Value>(answer)
    {
        for item in &question.questions {
            let answer_id = codex_question_answer_id(item.id.as_deref(), &item.question);
            let raw = object
                .get(&item.question)
                .or_else(|| object.get(answer_id.as_str()));
            if let Some(raw) = raw {
                let values = codex_answer_values_from_json(raw, item.multi_select);
                if !values.is_empty() {
                    answers.insert(answer_id, values);
                }
            }
        }
        if !answers.is_empty() {
            return answers;
        }
    }

    let answer_id = question
        .questions
        .first()
        .map(|item| codex_question_answer_id(item.id.as_deref(), &item.question))
        .unwrap_or_else(|| question.question.clone());
    let multi_select = question
        .questions
        .first()
        .map(|item| item.multi_select)
        .unwrap_or(question.multi_select);
    answers.insert(
        answer_id,
        codex_answer_values_from_text(answer, multi_select),
    );
    answers
}

fn codex_question_answer_id(id: Option<&str>, question: &str) -> String {
    id.filter(|value| !value.trim().is_empty())
        .unwrap_or(question)
        .to_string()
}

fn codex_answer_values_from_json(value: &serde_json::Value, multi_select: bool) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str())
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        serde_json::Value::String(value) => codex_answer_values_from_text(value, multi_select),
        other => codex_answer_values_from_text(&other.to_string(), multi_select),
    }
}

fn codex_answer_values_from_text(value: &str, multi_select: bool) -> Vec<String> {
    if multi_select {
        value
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    } else {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            Vec::new()
        } else {
            vec![trimmed.to_string()]
        }
    }
}

async fn submit_codex_request_user_input_output(
    thread_id: &str,
    call_id: &str,
    answers: BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let binary = resolve_codex_binary()
        .ok_or_else(|| "Could not find codex CLI for app-server".to_string())?;
    let output = codex_request_user_input_output(answers);

    let mut child = TokioCommand::new(binary)
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("Failed to start codex app-server: {}", err))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Failed to open codex app-server stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to open codex app-server stdout".to_string())?;
    let mut lines = TokioBufReader::new(stdout).lines();

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        write_json_rpc(
            &mut stdin,
            serde_json::json!({
                "id": 1,
                "method": "initialize",
                "params": {
                    "clientInfo": {
                        "name": "AgentBro",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }),
        )
        .await
        .ok_or_else(|| "Failed to initialize codex app-server".to_string())?;
        read_json_rpc_response(&mut lines, 1)
            .await
            .ok_or_else(|| "Codex app-server initialize failed".to_string())?;

        write_json_rpc(
            &mut stdin,
            serde_json::json!({
                "method": "initialized",
                "params": {}
            }),
        )
        .await
        .ok_or_else(|| "Failed to send codex app-server initialized".to_string())?;

        write_json_rpc(
            &mut stdin,
            serde_json::json!({
                "id": 2,
                "method": "thread/inject_items",
                "params": {
                    "threadId": thread_id,
                    "items": [
                        {
                            "type": "function_call_output",
                            "call_id": call_id,
                            "output": output
                        }
                    ]
                }
            }),
        )
        .await
        .ok_or_else(|| "Failed to submit Codex question answer".to_string())?;
        read_json_rpc_response(&mut lines, 2)
            .await
            .ok_or_else(|| "Codex app-server rejected question answer".to_string())?;

        Ok::<(), String>(())
    })
    .await
    .map_err(|_| "Timed out submitting Codex question answer".to_string())
    .and_then(|inner| inner);

    if child.try_wait().ok().flatten().is_none() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }

    result
}

fn codex_request_user_input_output(answers: BTreeMap<String, Vec<String>>) -> String {
    codex_request_user_input_payload(answers).to_string()
}

fn codex_request_user_input_payload(answers: BTreeMap<String, Vec<String>>) -> serde_json::Value {
    let formatted_answers = answers
        .into_iter()
        .map(|(key, values)| (key, serde_json::json!({ "answers": values })))
        .collect::<serde_json::Map<_, _>>();
    serde_json::json!({ "answers": formatted_answers })
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
            #[cfg(target_os = "windows")]
            {
                return Err(format!(
                    "Auto-approve failed on Windows: {e}. Make sure the AgentBro hook TCP bridge is running, then retry."
                ));
            }

            #[cfg(not(target_os = "windows"))]
            {
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

                crate::terminal::approval::approve_always(&tmux_target)
                    .map_err(|e| e.to_string())?;

                state
                    .session_store
                    .set_pending_permission(&session_id, None);
                Ok(())
            }
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

        #[cfg(unix)]
        {
            if let Ok(mut stream) = tokio::net::UnixStream::connect(&endpoint.socket_path).await {
                return stream.write_all(bytes).await.map_err(|e| e.to_string());
            }
        }

        let mut stream = tokio::net::TcpStream::connect(endpoint.tcp_addr())
            .await
            .map_err(|e| e.to_string())?;
        stream.write_all(bytes).await.map_err(|e| e.to_string())
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

/// Process-wide lock to serialize jump_to_terminal executions.
/// A single jump may fork-exec `pgrep`, `lsof`, `osascript`, and `open` and
/// run AppleScript loops over every iTerm/Ghostty window — running multiple
/// concurrently (e.g. from a user rage-clicking a stale "jump" button) can
/// stack into a system-wide stall. We serialize and silently drop overlap.
static JUMP_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[tauri::command]
pub async fn jump_to_terminal(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let lock = JUMP_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = match lock.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            log::info!(
                "Jump already in progress; ignoring duplicate click for session={}",
                session_id
            );
            return Ok(());
        }
    };

    log::info!("Jump to terminal: session={}", session_id);
    release_notch_keyboard_focus(&app);

    let session = state
        .session_store
        .get_session(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    if is_codex_desktop_session(&session) {
        return open_app_host_session(&session);
    }
    if is_ide_terminal_session(&session) {
        let bundle_id = session
            .term_bundle_id
            .as_deref()
            .ok_or_else(|| "IDE terminal session has no app bundle metadata".to_string())?;
        return match crate::terminal::jump::jump_to_ide_window(
            bundle_id,
            Some(session.cwd.as_str()),
        ) {
            crate::terminal::jump::JumpResult::Success => Ok(()),
            crate::terminal::jump::JumpResult::SessionNotFound => {
                Err("Session not found".to_string())
            }
            crate::terminal::jump::JumpResult::TerminalNotFound => {
                Err("IDE host app not found".to_string())
            }
            crate::terminal::jump::JumpResult::Failed(msg) => Err(msg),
        };
    }
    if app_host_bundle_id(&session).is_some() {
        return open_app_host_session(&session);
    }

    let pid = session.pid.unwrap_or(0);
    let resolved_tty = resolve_session_tty(&session);
    if let Some(tty) = &resolved_tty {
        if session.tty.as_deref() != Some(tty.as_str()) {
            let resolved_tty = tty.clone();
            state.session_store.update_session(&session_id, |s| {
                s.tty = Some(resolved_tty);
            });
        }
    }

    let has_jump_metadata = pid != 0
        || resolved_tty.is_some()
        || !session.terminal.trim().is_empty()
        || session
            .term_program
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || session
            .term_bundle_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || session
            .wezterm_pane
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || session
            .zellij_pane_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || session
            .cmux_surface_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
    if !has_jump_metadata {
        return Err("Session has no terminal metadata to jump to".to_string());
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
        wezterm_pane: session.wezterm_pane.clone().or(terminal_env.wezterm_pane),
        waveterm_block_id: terminal_env.waveterm_block_id,
        waveterm_tab_id: terminal_env.waveterm_tab_id,
        waveterm_jwt: terminal_env.waveterm_jwt,
        zellij_pane_id: session
            .zellij_pane_id
            .clone()
            .or(terminal_env.zellij_pane_id),
        zellij_session_name: session
            .zellij_session_name
            .clone()
            .or(terminal_env.zellij_session_name),
        cmux_surface_id: session
            .cmux_surface_id
            .clone()
            .or(terminal_env.cmux_surface_id),
        cmux_workspace_id: session
            .cmux_workspace_id
            .clone()
            .or(terminal_env.cmux_workspace_id),
        tmux_pane,
        tmux_env: terminal_env.tmux,
        cwd: Some(session.cwd.clone()).filter(|cwd| !cwd.is_empty()),
        tty_path: resolved_tty,
        terminal_app: Some(session.terminal.clone()).filter(|terminal| !terminal.is_empty()),
        term_program: session.term_program.clone().or(terminal_env.term_program),
        term_bundle_id: session
            .term_bundle_id
            .clone()
            .or(terminal_env.cf_bundle_identifier),
        agent_type: Some(session.agent_type.clone()),
    };

    let fallback_terminal = terminal_hint_for_fallback(&session);
    match crate::terminal::jump::jump_to_terminal_with_context(&jump_context) {
        crate::terminal::jump::JumpResult::Success => Ok(()),
        crate::terminal::jump::JumpResult::SessionNotFound => Err("Session not found".to_string()),
        crate::terminal::jump::JumpResult::TerminalNotFound => {
            log::warn!(
                "Terminal not found in process tree for session {}. Falling back to app activation.",
                session_id
            );
            jump_to_terminal_fallback(&fallback_terminal, &session.cwd)
        }
        crate::terminal::jump::JumpResult::Failed(msg) => {
            log::warn!(
                "Precise terminal jump failed for session {}: {}. Falling back to app activation.",
                session_id,
                msg
            );
            jump_to_terminal_fallback(&fallback_terminal, &session.cwd)
        }
    }
}

fn release_notch_keyboard_focus(app: &tauri::AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window("notch") else {
            return;
        };

        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::NSWindow;
            if let Ok(ptr) = window.ns_window() {
                unsafe {
                    let ns_window = ptr as *const NSWindow;
                    (*ns_window).resignKeyWindow();
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = window;
        }
    });
}

fn terminal_hint_for_fallback(session: &SessionState) -> String {
    if !session.terminal.trim().is_empty() {
        return session.terminal.clone();
    }

    if let Some(program) = session
        .term_program
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return fallback_terminal_app_name(program).to_string();
    }

    if let Some(bundle_id) = session
        .term_bundle_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return fallback_terminal_app_name(bundle_id).to_string();
    }

    String::new()
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
    #[cfg(not(target_os = "macos"))]
    let _ = terminal;

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
    } else if lower.contains("wave") {
        "Wave"
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
    if let Some(path) = crate::agents::executable::find_binary(name) {
        return Some(path.display().to_string());
    }

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
        #[cfg(not(target_os = "windows"))]
        {
            std::process::Command::new("which")
                .arg(name)
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|path| path.trim().to_string())
                .filter(|path| !path.is_empty())
        }
        #[cfg(target_os = "windows")]
        {
            None
        }
    })
}

fn launch_in_terminal(terminal: &str, cwd: &str, command: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let _ = terminal;
        return launch_in_windows_terminal(cwd, command);
    }

    #[cfg(target_os = "macos")]
    {
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

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let _ = terminal;
        let _ = cwd;
        let _ = command;
        Err("Opening an authorization terminal is not supported on this platform yet.".to_string())
    }
}

#[cfg(target_os = "windows")]
fn launch_in_windows_terminal(cwd: &str, command: &str) -> Result<(), String> {
    if std::process::Command::new("wt.exe")
        .args(["-d", cwd, "cmd", "/K", command])
        .spawn()
        .is_ok()
    {
        return Ok(());
    }

    std::process::Command::new("cmd")
        .args(["/C", "start", "", "/D", cwd, "cmd", "/K", command])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open Windows terminal: {e}"))
}

#[cfg(target_os = "windows")]
fn shell_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(not(target_os = "windows"))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn applescript_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn applescript_string_literal(value: &str) -> String {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let parts = normalized
        .split('\n')
        .map(|part| format!("\"{}\"", applescript_escape(part)))
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "\"\"".to_string()
    } else {
        parts.join(" & linefeed & ")
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

#[tauri::command]
pub async fn run_hook_doctor(state: State<'_, AppState>) -> Result<HookDoctorReport, String> {
    let mut checks = Vec::new();
    let bridge = crate::agents::hook_manager::bridge_binary_path();
    checks.push(HookDoctorCheck {
        id: "bridge-binary".to_string(),
        label: "Bridge binary".to_string(),
        status: if bridge.exists() { "ok" } else { "error" }.to_string(),
        detail: bridge.display().to_string(),
    });
    checks.push(HookDoctorCheck {
        id: "bridge-current".to_string(),
        label: "Bridge version".to_string(),
        status: if crate::agents::hook_manager::bridge_binary_is_current() {
            "ok"
        } else {
            "warn"
        }
        .to_string(),
        detail: "Installed hook bridge matches bundled bridge".to_string(),
    });

    let endpoint = hook_endpoint::current();
    #[cfg(unix)]
    {
        let socket_status = tokio::time::timeout(
            Duration::from_millis(300),
            tokio::net::UnixStream::connect(&endpoint.socket_path),
        )
        .await
        .is_ok_and(|result| result.is_ok());
        checks.push(HookDoctorCheck {
            id: "hook-server".to_string(),
            label: "Hook server socket".to_string(),
            status: if socket_status { "ok" } else { "warn" }.to_string(),
            detail: endpoint.socket_path.clone(),
        });
    }

    let tcp_status = tokio::time::timeout(
        Duration::from_millis(300),
        tokio::net::TcpStream::connect(endpoint.tcp_addr()),
    )
    .await
    .is_ok_and(|result| result.is_ok());
    checks.push(HookDoctorCheck {
        id: "hook-server-tcp".to_string(),
        label: "Hook server TCP".to_string(),
        status: if tcp_status { "ok" } else { "warn" }.to_string(),
        detail: endpoint.tcp_addr(),
    });

    let installed_hook_names = state
        .adapters
        .iter()
        .filter(|adapter| adapter.hooks_installed())
        .map(|adapter| adapter.display_name().to_string())
        .collect::<Vec<_>>();
    checks.push(HookDoctorCheck {
        id: "installed-hooks".to_string(),
        label: "Installed hooks".to_string(),
        status: if installed_hook_names.is_empty() {
            "warn"
        } else {
            "ok"
        }
        .to_string(),
        detail: if installed_hook_names.is_empty() {
            "No adapter configs contain AgentBro hooks".to_string()
        } else {
            format!(
                "{} adapter configs contain AgentBro hooks: {}",
                installed_hook_names.len(),
                installed_hook_names.join(", ")
            )
        },
    });

    let bridge_invocations = recent_bridge_invocations(50);
    checks.push(HookDoctorCheck {
        id: "bridge-invocations".to_string(),
        label: "Bridge invocation trace".to_string(),
        status: if bridge_invocations.is_empty() {
            "info"
        } else {
            "ok"
        }
        .to_string(),
        detail: bridge_invocations
            .last()
            .cloned()
            .unwrap_or_else(|| "No bridge invocations recorded yet".to_string()),
    });

    let mut present_profiles = 0usize;
    let mut unhealthy_profiles = Vec::new();
    for adapter in &state.adapters {
        let Some(profile) = crate::agents::profiles::profile_for_agent(adapter.name()) else {
            continue;
        };
        let health = crate::agents::profiles::install_health_for_profile(&profile);
        if !health.is_present() {
            continue;
        }
        present_profiles += 1;
        checks.push(HookDoctorCheck {
            id: format!("hook-profile-{}", profile.id),
            label: format!("{} hook profile", adapter.display_name()),
            status: doctor_status_for_hook_health(health).to_string(),
            detail: format!(
                "{} ({})",
                health.as_status_str(),
                profile.configuration_path
            ),
        });
        if health != crate::agents::profiles::HookInstallHealth::Installed {
            unhealthy_profiles.push(format!("{}={}", profile.id, health.as_status_str()));
        }
    }
    checks.push(HookDoctorCheck {
        id: "hook-profile-health".to_string(),
        label: "Hook event coverage".to_string(),
        status: if unhealthy_profiles.is_empty() {
            if present_profiles > 0 {
                "ok"
            } else {
                "warn"
            }
        } else {
            "warn"
        }
        .to_string(),
        detail: if unhealthy_profiles.is_empty() {
            format!("{present_profiles} installed profiles checked")
        } else {
            unhealthy_profiles.join(", ")
        },
    });

    append_codex_doctor_checks(&mut checks);

    #[cfg(target_os = "macos")]
    checks.push(HookDoctorCheck {
        id: "automation-permission".to_string(),
        label: "macOS automation".to_string(),
        status: if osascript_ok(r#"tell application "System Events" to get name of first application process whose frontmost is true"#) {
            "ok"
        } else {
            "warn"
        }
        .to_string(),
        detail: "Required for terminal focus".to_string(),
    });

    #[cfg(target_os = "windows")]
    checks.push(HookDoctorCheck {
        id: "platform-integration".to_string(),
        label: "Windows hook transport".to_string(),
        status: "info".to_string(),
        detail: "Windows uses TCP hook delivery and CLI config files; macOS automation permission is not required.".to_string(),
    });

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    checks.push(HookDoctorCheck {
        id: "platform-integration".to_string(),
        label: "Platform hook transport".to_string(),
        status: "info".to_string(),
        detail: "This platform uses TCP hook delivery and CLI config files; macOS automation permission is not required.".to_string(),
    });

    #[cfg(target_os = "macos")]
    let required_binaries: &[&str] = &["tmux", "sqlite3"];
    #[cfg(not(target_os = "macos"))]
    let required_binaries: &[&str] = &[];

    for binary in required_binaries {
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

    // Check optional terminal multiplexers — only report found ones as OK,
    // and add a single info-level note if none are found
    let optional_terminals = ["zellij", "cmux", "wezterm", "kaku", "kitten"];
    let mut found_any_terminal = false;

    for binary in optional_terminals {
        if let Some(path) = find_binary(binary) {
            found_any_terminal = true;
            checks.push(HookDoctorCheck {
                id: format!("binary-{binary}"),
                label: format!("{binary} binary"),
                status: "ok".to_string(),
                detail: path,
            });
        }
    }

    if !found_any_terminal {
        checks.push(HookDoctorCheck {
            id: "optional-terminals".to_string(),
            label: "Optional terminal multiplexers".to_string(),
            status: "info".to_string(),
            detail: "No optional terminals found (zellij, cmux, wezterm, kaku, kitten). Only install if you use them.".to_string(),
        });
    }

    if let Some(warning) = check_bare_mode() {
        checks.push(HookDoctorCheck {
            id: "claude-bare-mode".to_string(),
            label: "Claude Code bare mode".to_string(),
            status: "error".to_string(),
            detail: warning,
        });
    } else {
        checks.push(HookDoctorCheck {
            id: "claude-bare-mode".to_string(),
            label: "Claude Code bare mode".to_string(),
            status: "ok".to_string(),
            detail: "CLAUDE_CODE_SIMPLE is not set".to_string(),
        });
    }

    {
        let home = dirs::home_dir();
        let trust_path = home
            .as_ref()
            .map(|h| h.join(".gemini").join("trustedFolders.json"));
        let cwd = std::env::current_dir().ok();
        let mut trust_ok = true;
        let mut trust_detail = "Gemini folder trust: no working directory".to_string();

        if let (Some(trust_path), Some(cwd)) = (&trust_path, &cwd) {
            if let Ok(content) = std::fs::read_to_string(trust_path) {
                if let Ok(trust) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(obj) = trust.as_object() {
                        for (_path, level) in obj {
                            if level.as_str() == Some("TRUST_PARENT") {
                                let parent = std::path::PathBuf::from(_path);
                                if cwd.starts_with(&parent) {
                                    trust_detail =
                                        format!("{} is trusted via {}", cwd.display(), _path);
                                    trust_ok = true;
                                    break;
                                }
                                trust_ok = false;
                                trust_detail = format!(
                                    "{} is NOT in any trusted folder. Add it via: gemini trust",
                                    cwd.display()
                                );
                            } else if level.as_str() == Some("TRUST_FOLDER") {
                                if _path == &cwd.display().to_string() {
                                    trust_detail = format!("{} is trusted", cwd.display());
                                    trust_ok = true;
                                    break;
                                }
                                trust_ok = false;
                                trust_detail = format!(
                                    "{} is NOT in any trusted folder. Add it via: gemini trust",
                                    cwd.display()
                                );
                            }
                        }
                    }
                }
            }
        }

        checks.push(HookDoctorCheck {
            id: "gemini-folder-trust".to_string(),
            label: "Gemini folder trust".to_string(),
            status: if trust_ok { "ok" } else { "warn" }.to_string(),
            detail: trust_detail,
        });
    }

    Ok(HookDoctorReport {
        generated_at: chrono::Utc::now().timestamp(),
        checks,
    })
}

fn append_codex_doctor_checks(checks: &mut Vec<HookDoctorCheck>) {
    let probe = crate::agents::codex::probe_app_server_readiness();
    for check in probe.checks {
        if matches!(check.id.as_str(), "server-port" | "live-sync") {
            continue;
        }
        checks.push(HookDoctorCheck {
            id: format!("codex-{}", check.id),
            label: format!("Codex {}", check.label),
            status: check.status,
            detail: check.detail,
        });
    }
}

fn doctor_status_for_hook_health(
    health: crate::agents::profiles::HookInstallHealth,
) -> &'static str {
    match health {
        crate::agents::profiles::HookInstallHealth::Installed => "ok",
        crate::agents::profiles::HookInstallHealth::SettingsCorrupted
        | crate::agents::profiles::HookInstallHealth::Error => "error",
        crate::agents::profiles::HookInstallHealth::NotInstalled
        | crate::agents::profiles::HookInstallHealth::NeedsReinstall => "warn",
    }
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
    let previous = state.config_store.get();
    state.config_store.update(config.clone())?;
    if previous.analytics_enabled != config.analytics_enabled {
        state.telemetry.handle_consent_changed(&config).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_language(state: State<'_, AppState>, language: String) -> Result<(), String> {
    let language = match language.as_str() {
        "en" | "zh" | "ja" | "ko" | "tr" => language,
        other => return Err(format!("Unsupported language: {}", other)),
    };
    let mut config = state.config_store.get();
    config.language = language;
    state.config_store.update(config)
}

#[tauri::command]
pub async fn set_analytics_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mut config = state.config_store.get();
    config.analytics_enabled = enabled;
    config.analytics_consent_prompt_completed = true;
    state.config_store.update(config.clone())?;
    state.telemetry.handle_consent_changed(&config).await;
    Ok(())
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
const APP_BUNDLE_IDENTIFIER: &str = "com.agentbro.desktop";

#[cfg(target_os = "macos")]
fn set_launch_at_login_state(enabled: bool) -> Result<(), String> {
    let plist_path = launch_agent_path()?;
    if enabled {
        if let Some(parent) = plist_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
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
    <string>-b</string>
    <string>{APP_BUNDLE_IDENTIFIER}</string>
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
        let _ = std::process::Command::new("launchctl")
            .arg("bootout")
            .arg(domain)
            .arg(&plist_path)
            .output();
        std::fs::remove_file(plist_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
const WINDOWS_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(target_os = "windows")]
const WINDOWS_RUN_VALUE: &str = "AgentBro";

#[cfg(target_os = "windows")]
fn set_launch_at_login_state(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = std::env::current_exe()
            .map_err(|err| format!("Unable to resolve current executable: {err}"))?;
        let command = format!("\"{}\"", exe.display());
        let output = std::process::Command::new("reg")
            .args([
                "add",
                WINDOWS_RUN_KEY,
                "/v",
                WINDOWS_RUN_VALUE,
                "/t",
                "REG_SZ",
                "/d",
                &command,
                "/f",
            ])
            .output()
            .map_err(|err| format!("Failed to update Windows startup registry: {err}"))?;
        return reg_output_result(output, "Failed to enable Windows launch at login");
    }

    let output = std::process::Command::new("reg")
        .args(["delete", WINDOWS_RUN_KEY, "/v", WINDOWS_RUN_VALUE, "/f"])
        .output()
        .map_err(|err| format!("Failed to update Windows startup registry: {err}"))?;
    if output.status.success() || !get_launch_at_login_state() {
        Ok(())
    } else {
        reg_output_result(output, "Failed to disable Windows launch at login")
    }
}

#[cfg(target_os = "windows")]
fn reg_output_result(output: std::process::Output, fallback: &str) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Err(if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        fallback.to_string()
    })
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn set_launch_at_login_state(_enabled: bool) -> Result<(), String> {
    Err("Launch at login is not supported on this platform yet".to_string())
}

#[cfg(target_os = "macos")]
fn get_launch_at_login_state() -> bool {
    launch_agent_path()
        .map(|path| path.exists())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn get_launch_at_login_state() -> bool {
    std::process::Command::new("reg")
        .args(["query", WINDOWS_RUN_KEY, "/v", WINDOWS_RUN_VALUE])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
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
    app: tauri::AppHandle,
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
    let mode_changed = config.island_surface_mode != island_surface_mode;
    config.island_surface_mode = island_surface_mode;
    config.island_pet_scale = island_pet_scale.clamp(10, 120);
    state.config_store.update(config.clone())?;

    if mode_changed {
        let handle = app.clone();
        let saved_origin = config.island_pet_window_origin.clone();
        let is_pet_mode = config.island_surface_mode == "pet";
        app.run_on_main_thread(move || {
            crate::sync_pet_window_visibility_inner(&handle, is_pet_mode, saved_origin.as_ref());
        })
        .map_err(|e| e.to_string())?;
    } else if config.island_surface_mode == "pet" {
        crate::configure_pet_window_for_spaces(&app);
    }
    Ok(())
}

#[tauri::command]
pub async fn set_active_pet_id(
    state: State<'_, AppState>,
    pet_id: Option<String>,
) -> Result<(), String> {
    let mut config = state.config_store.get();
    config.island_active_pet_id = pet_id.filter(|s| !s.is_empty());
    state.config_store.update(config)
}

#[tauri::command]
pub async fn set_agent_default_pet(
    state: State<'_, AppState>,
    agent: String,
    pet_id: Option<String>,
) -> Result<(), String> {
    let mut config = state.config_store.get();
    match pet_id.filter(|s| !s.is_empty()) {
        Some(pid) => {
            config.island_agent_pet_map.insert(agent, pid);
        }
        None => {
            config.island_agent_pet_map.remove(&agent);
        }
    }
    state.config_store.update(config)
}

// ── Hook Management Commands ──────────────────────────────────────

pub fn check_bare_mode() -> Option<String> {
    if std::env::var("CLAUDE_CODE_SIMPLE").ok().as_deref() == Some("1") {
        return Some("CLAUDE_CODE_SIMPLE=1 is set in the process environment.".to_string());
    }
    if let Some(val) = crate::agents::executable::login_shell_var("CLAUDE_CODE_SIMPLE") {
        if val == "1" {
            let shell = std::env::var("SHELL")
                .ok()
                .unwrap_or_else(|| "/bin/zsh".to_string());
            return Some(format!(
                "CLAUDE_CODE_SIMPLE=1 is set in {} config (bare mode skips all hooks).",
                shell.rsplit('/').next().unwrap_or("shell")
            ));
        }
    }
    None
}

pub fn ensure_gemini_folder_trust() {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return,
    };
    let trust_path = home.join(".gemini").join("trustedFolders.json");
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(_) => return,
    };
    let cwd_str = cwd.display().to_string();

    let mut trust: serde_json::Value = match std::fs::read_to_string(&trust_path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    };

    if let Some(obj) = trust.as_object() {
        for (_path, level) in obj {
            if level.as_str() == Some("TRUST_PARENT") {
                let parent = std::path::PathBuf::from(_path);
                if cwd.starts_with(&parent) {
                    return;
                }
            }
            if level.as_str() == Some("TRUST_FOLDER") && _path == &cwd_str {
                return;
            }
        }
    }

    if let Some(obj) = trust.as_object_mut() {
        obj.insert(cwd_str.clone(), serde_json::json!("TRUST_PARENT"));
    }
    if let Some(parent) = trust_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(formatted) = serde_json::to_string_pretty(&trust) {
        let _ = std::fs::write(&trust_path, formatted);
        log::info!("Added {} to Gemini trusted folders", cwd_str);
    }
}

#[tauri::command]
pub async fn install_hooks(state: State<'_, AppState>, agent: String) -> Result<(), String> {
    log::info!("Installing hooks for agent: {}", agent);
    let adapter = state
        .adapters
        .iter()
        .find(|a| a.name() == agent)
        .ok_or_else(|| format!("Unknown agent: {}", agent))?;
    if matches!(
        adapter.detect_status_now(),
        crate::agents::AdapterStatus::Unavailable
    ) {
        return Err(format!(
            "{} CLI not found. Searched process PATH, login shell PATH, \
             and common directories (homebrew, nvm, volta, mise, cargo). \
             Confirm it is installed and try restarting AgentBro.",
            adapter.display_name()
        ));
    }
    adapter.install_hooks().map_err(|e| e.to_string())?;

    if agent == "claude-code" {
        if let Some(warning) = check_bare_mode() {
            log::warn!("Claude Code bare mode detected: {}", warning);
        }
    }
    if agent == "gemini" {
        ensure_gemini_folder_trust();
    }

    if let Err(e) = state.config_store.mark_agent_enabled(&agent) {
        log::warn!(
            "Failed to persist enabled-agent intent for {}: {}",
            agent,
            e
        );
    }
    let config = state.config_store.get();
    state.telemetry.record_hook_install(&config, &agent).await;
    Ok(())
}

#[tauri::command]
pub async fn remove_hooks(state: State<'_, AppState>, agent: String) -> Result<(), String> {
    log::info!("Removing hooks for agent: {}", agent);
    let adapter = state
        .adapters
        .iter()
        .find(|a| a.name() == agent)
        .ok_or_else(|| format!("Unknown agent: {}", agent))?;
    adapter.remove_hooks().map_err(|e| e.to_string())?;
    if let Err(e) = state.config_store.mark_agent_disabled(&agent) {
        log::warn!("Failed to clear enabled-agent intent for {}: {}", agent, e);
    }
    let config = state.config_store.get();
    state.telemetry.record_hook_uninstall(&config, &agent).await;
    Ok(())
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
    parse_session_messages_for_command(&state, &session_id).map(|either| match either {
        SessionMessagesResult::Local(messages) => messages,
        SessionMessagesResult::Remote(messages) => messages,
    })
}

/// Paginated slice of a session's chat history. Used by the frontend to load
/// only the tail of a transcript on first open (typical 50 messages instead
/// of the full file, which can be 100MB+ for long Codex sessions).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistorySlice {
    pub messages: Vec<ParsedMessage>,
    pub has_more: bool,
    pub first_message_id: Option<String>,
    pub total_count: usize,
    pub transcript_path: Option<String>,
}

const DEFAULT_TAIL_LIMIT: usize = 50;
const MAX_TAIL_LIMIT: usize = 500;

#[tauri::command]
pub async fn get_chat_history_tail(
    state: State<'_, AppState>,
    session_id: String,
    limit: Option<usize>,
    before_id: Option<String>,
) -> Result<ChatHistorySlice, String> {
    let limit = limit.unwrap_or(DEFAULT_TAIL_LIMIT).clamp(1, MAX_TAIL_LIMIT);
    let (messages, transcript_path) = match parse_session_messages_for_command(&state, &session_id)?
    {
        SessionMessagesResult::Local(messages) => {
            let path = resolve_transcript_path_for_session(&state, &session_id);
            (messages, path)
        }
        SessionMessagesResult::Remote(messages) => (messages, None),
    };

    let total_count = messages.len();
    let end = match before_id.as_deref() {
        Some(id) => messages
            .iter()
            .position(|m| m.id == id)
            .unwrap_or(total_count),
        None => total_count,
    };
    let start = end.saturating_sub(limit);

    let slice = messages[start..end].to_vec();
    let first_message_id = slice.first().map(|m| m.id.clone());

    Ok(ChatHistorySlice {
        messages: slice,
        has_more: start > 0,
        first_message_id,
        total_count,
        transcript_path: transcript_path.map(|p| p.to_string_lossy().into_owned()),
    })
}

enum SessionMessagesResult {
    Local(Vec<ParsedMessage>),
    Remote(Vec<ParsedMessage>),
}

fn resolve_transcript_path_for_session(
    state: &State<'_, AppState>,
    session_id: &str,
) -> Option<PathBuf> {
    let session = state.session_store.get_session(session_id)?;
    let cwd = session.cwd.clone();
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
    if session.engine_label.as_deref() == Some("Claude Desktop") {
        crate::hooks::claude_desktop_watcher::find_audit_file_for_cli_session(session_id)
    } else if session.agent_type == "codex" {
        discover_codex_session_file(session_id)
            .or_else(|| discover_session_file_in_dirs(session_id, &cwd, &projects_dirs))
    } else {
        discover_session_file_in_dirs(session_id, &cwd, &projects_dirs)
    }
}

fn parse_session_messages_for_command(
    state: &State<'_, AppState>,
    session_id: &str,
) -> Result<SessionMessagesResult, String> {
    let session = state.session_store.get_session(session_id);
    let file_path = resolve_transcript_path_for_session(state, session_id);

    let Some(file_path) = file_path else {
        // No JSONL file — build chat history from raw hook events.
        // This covers agents like OpenCode that don't write transcript files.
        let raw_events = state.hook_server.raw_events_for_session(session_id);
        if !raw_events.is_empty() {
            let fallback_session = session.unwrap_or_else(|| {
                crate::hooks::session_store::SessionState::new(
                    session_id.to_string(),
                    "unknown".to_string(),
                    String::new(),
                    String::new(),
                    String::new(),
                )
            });
            return Ok(SessionMessagesResult::Remote(remote_session_chat_history(
                &fallback_session,
                raw_events,
            )));
        }
        if let Some(ref session) = session {
            if session.remote_host_id.is_some() || session.remote_host_name.is_some() {
                return Ok(SessionMessagesResult::Remote(remote_session_chat_history(
                    session,
                    state.hook_server.raw_events_for_session(session_id),
                )));
            }
        }
        return Err(format!("No JSONL file found for session {}", session_id));
    };

    hydrate_subagents_from_file(&state.session_store, session_id, &file_path);

    if let Ok(watcher_guard) = state.conversation_watcher.lock() {
        if let Some(ref watcher) = *watcher_guard {
            if let Some(result) = watcher.parse_session_full(session_id, file_path.clone()) {
                return Ok(SessionMessagesResult::Local(result.all_messages));
            }
        }
    }

    let mut parser = crate::hooks::conversation_parser::ConversationParser::new(file_path);
    parser
        .parse_full()
        .map(SessionMessagesResult::Local)
        .map_err(|e| format!("Failed to parse conversation: {}", e))
}

fn remote_session_chat_history(
    session: &SessionState,
    raw_events: Vec<RawHookEvent>,
) -> Vec<ParsedMessage> {
    let mut messages = Vec::new();

    for event in raw_events {
        let raw = event.raw;
        let timestamp =
            chrono::DateTime::<chrono::Utc>::from_timestamp_millis(event.timestamp_ms as i64)
                .map(|date| date.to_rfc3339());

        match event.event_name.as_str() {
            "UserPromptSubmit" => {
                if let Some(text) = first_nonempty_string(&raw, &["prompt", "user_prompt"]) {
                    messages.push(parsed_text_message(
                        format!("remote-user-{}", event.seq),
                        ChatRole::User,
                        timestamp,
                        text,
                    ));
                }
            }
            "PreToolUse" => {
                let name = first_nonempty_string(&raw, &["tool", "tool_name"])
                    .unwrap_or("Tool")
                    .to_string();
                let id = first_nonempty_string(&raw, &["tool_use_id", "toolUseId"])
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("remote-tool-{}", event.seq));
                messages.push(ParsedMessage {
                    id: format!("remote-tool-use-{}", event.seq),
                    role: ChatRole::Assistant,
                    timestamp,
                    blocks: vec![MessageBlock::ToolUse {
                        id,
                        name,
                        input: remote_tool_input_map(raw.get("tool_input")),
                    }],
                });
            }
            "PostToolUse" | "PostToolUseFailure" | "PermissionDenied" => {
                let id = first_nonempty_string(&raw, &["tool_use_id", "toolUseId"])
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("remote-tool-{}", event.seq));
                let content = first_nonempty_string(
                    &raw,
                    &[
                        "tool_response",
                        "toolResponse",
                        "result",
                        "output",
                        "message",
                        "tool_error",
                    ],
                )
                .map(ToString::to_string);
                messages.push(ParsedMessage {
                    id: format!("remote-tool-result-{}", event.seq),
                    role: ChatRole::User,
                    timestamp,
                    blocks: vec![MessageBlock::ToolResult {
                        tool_use_id: id,
                        content,
                        is_error: event.event_name != "PostToolUse",
                    }],
                });
            }
            "PermissionRequest" => {
                let name = first_nonempty_string(&raw, &["tool", "tool_name"])
                    .unwrap_or("Tool")
                    .to_string();
                let input = remote_tool_input_map(raw.get("tool_input"));
                messages.push(ParsedMessage {
                    id: format!("remote-perm-{}", event.seq),
                    role: ChatRole::Assistant,
                    timestamp,
                    blocks: vec![MessageBlock::ToolUse {
                        id: format!("remote-perm-{}", event.seq),
                        name,
                        input,
                    }],
                });
            }
            "Notification" => {
                if let Some(text) = first_nonempty_string(&raw, &["message"]) {
                    messages.push(parsed_text_message(
                        format!("remote-notification-{}", event.seq),
                        ChatRole::Assistant,
                        timestamp,
                        text,
                    ));
                }
            }
            "Stop" | "StopFailure" => {
                if let Some(text) = first_nonempty_string(&raw, &["summary", "message", "error"]) {
                    messages.push(parsed_text_message(
                        format!("remote-stop-{}", event.seq),
                        ChatRole::Assistant,
                        timestamp,
                        text,
                    ));
                }
            }
            "BeforeAgent" => {
                if let Some(text) = first_nonempty_string(&raw, &["prompt", "message"]) {
                    messages.push(parsed_text_message(
                        format!("remote-user-{}", event.seq),
                        ChatRole::User,
                        timestamp,
                        text,
                    ));
                }
            }
            "AfterAgent" => {
                if let Some(text) = first_nonempty_string(
                    &raw,
                    &[
                        "prompt_response",
                        "summary",
                        "last_assistant_message",
                        "message",
                    ],
                ) {
                    messages.push(parsed_text_message(
                        format!("remote-assistant-{}", event.seq),
                        ChatRole::Assistant,
                        timestamp,
                        text,
                    ));
                }
            }
            _ => {}
        }
    }

    let has_user = messages
        .iter()
        .any(|message| matches!(message.role, ChatRole::User));
    let has_assistant = messages
        .iter()
        .any(|message| matches!(message.role, ChatRole::Assistant));

    if !has_user {
        if let Some(text) = session
            .last_user_message
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        {
            messages.push(parsed_text_message(
                format!("remote-user-{}", session.id),
                ChatRole::User,
                None,
                text,
            ));
        }
    }
    if !has_assistant {
        if let Some(text) = session
            .last_response
            .as_deref()
            .or(session.description.as_deref())
            .filter(|text| !text.trim().is_empty())
        {
            messages.push(parsed_text_message(
                format!("remote-assistant-{}", session.id),
                ChatRole::Assistant,
                None,
                text,
            ));
        }
    }

    messages
}

fn parsed_text_message(
    id: String,
    role: ChatRole,
    timestamp: Option<String>,
    text: &str,
) -> ParsedMessage {
    ParsedMessage {
        id,
        role,
        timestamp,
        blocks: vec![MessageBlock::Text {
            text: text.to_string(),
        }],
    }
}

fn first_nonempty_string<'a>(raw: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| raw.get(*key).and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn remote_tool_input_map(value: Option<&serde_json::Value>) -> HashMap<String, String> {
    let mut input = HashMap::new();
    if let Some(serde_json::Value::Object(map)) = value {
        for (key, value) in map {
            let text = value
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_else(|| value.to_string());
            input.insert(key.clone(), text);
        }
    }
    input
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

    if let Some(existing) = session.subagents.iter_mut().find(|item| {
        item.agent_id == incoming.agent_id
            || launch_tool_use_id
                .as_deref()
                .is_some_and(|tool_use_id| item.agent_id == tool_use_id)
    }) {
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
    let (transcript_path, requested_agent_id) = split_subagent_history_request(transcript_path);
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

    if let Some(subagent) = session.subagents.iter().find(|subagent| {
        requested_agent_id
            .as_deref()
            .map(|agent_id| subagent.agent_id == agent_id)
            .unwrap_or(true)
            && subagent.agent_transcript_path.is_none()
            && subagent
                .transcript_path
                .as_ref()
                .map(|path| crate::agents::claude_code::expand_tilde(path))
                .and_then(|path| path.canonicalize().ok())
                .map(|path| path == requested_path)
                .unwrap_or(false)
    }) {
        return Ok(synthetic_subagent_chat_history(subagent));
    }

    if requested_agent_id.is_some() {
        return Err("Subagent transcript is not registered on this session".to_string());
    }

    let mut parser = crate::hooks::conversation_parser::ConversationParser::new(requested_path);
    parser
        .parse_full()
        .map_err(|e| format!("Failed to parse subagent conversation: {}", e))
}

fn split_subagent_history_request(transcript_path: &str) -> (&str, Option<String>) {
    if let Some((path, agent_id)) = transcript_path.split_once("#agentbro-subagent=") {
        (path, Some(agent_id.to_string()))
    } else {
        (transcript_path, None)
    }
}

fn synthetic_subagent_chat_history(subagent: &SubagentInfo) -> Vec<ParsedMessage> {
    let mut messages = Vec::new();
    if !subagent.description.trim().is_empty() {
        messages.push(ParsedMessage {
            id: format!("{}-prompt", subagent.agent_id),
            role: ChatRole::User,
            timestamp: timestamp_to_rfc3339(subagent.started_at),
            blocks: vec![MessageBlock::Text {
                text: subagent.description.clone(),
            }],
        });
    }
    if let Some(text) = subagent
        .last_assistant_message
        .as_ref()
        .map(|text| text.trim())
        .filter(|text| !text.is_empty())
    {
        messages.push(ParsedMessage {
            id: format!("{}-response", subagent.agent_id),
            role: ChatRole::Assistant,
            timestamp: subagent.completed_at.and_then(timestamp_to_rfc3339),
            blocks: vec![MessageBlock::Text {
                text: text.to_string(),
            }],
        });
    }
    messages
}

fn timestamp_to_rfc3339(timestamp: i64) -> Option<String> {
    chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0).map(|dt| dt.to_rfc3339())
}

// ── Diagnostics Commands ────────────────────────────────────────

/// Redact user home directory paths across macOS, Linux, and Windows.
fn redact_paths(text: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let full = home.to_string_lossy().to_string();
        let redacted = "<HOME>";
        let mut result = text.replace(&full, redacted);
        #[cfg(target_os = "windows")]
        {
            result = result.replace(&full.replace('\\', "/"), redacted);
        }
        if let Some(user_name) = home.file_name().and_then(|value| value.to_str()) {
            result = result
                .replace(&format!("/Users/{user_name}"), redacted)
                .replace(&format!("C:\\Users\\{user_name}"), redacted)
                .replace(&format!("C:/Users/{user_name}"), redacted);
        }
        return result;
    }
    text.to_string()
}

fn redact_remote_urls(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = match (rest.find("http://"), rest.find("https://")) {
        (Some(http), Some(https)) => Some(http.min(https)),
        (Some(http), None) => Some(http),
        (None, Some(https)) => Some(https),
        (None, None) => None,
    } {
        output.push_str(&rest[..start]);
        let url_rest = &rest[start..];
        let end = url_rest
            .char_indices()
            .find_map(|(idx, ch)| {
                if ch.is_whitespace() || matches!(ch, '"' | '\'' | ')' | ']' | '}' | '<' | '`') {
                    Some(idx)
                } else {
                    None
                }
            })
            .unwrap_or(url_rest.len());
        let url = &url_rest[..end];
        if url.starts_with("http://localhost")
            || url.starts_with("http://127.")
            || url.starts_with("http://[::1]")
        {
            output.push_str(url);
        } else {
            output.push_str("[REDACTED_URL]");
        }
        rest = &url_rest[end..];
    }
    output.push_str(rest);
    output
}

fn redact_sensitive_hook_config(text: &str) -> String {
    let text = redact_remote_urls(&redact_paths(text));
    // The privacy notice promises env var *values* are never included. A
    // line-by-line keyword blocklist can't guarantee that (a custom secret in
    // an env var with an innocuous name would slip through), so when the file
    // parses as JSON we structurally strip every value under an `env` object,
    // keeping the key names for diagnostic value.
    let text = match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(mut value) => {
            redact_env_values(&mut value);
            serde_json::to_string_pretty(&value).unwrap_or(text)
        }
        Err(_) => text,
    };
    text.lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            let sensitive = [
                "api_key",
                "apikey",
                "authorization",
                "bearer ",
                "password",
                "secret",
                "token",
                "webhook",
            ]
            .iter()
            .any(|needle| lower.contains(needle));
            if sensitive {
                let indent = line
                    .chars()
                    .take_while(|ch| ch.is_whitespace())
                    .collect::<String>();
                format!("{indent}[REDACTED sensitive hook config line]")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Recursively replace every string value inside any `env` object with
/// `[REDACTED]`, leaving the variable names intact. Numeric/bool env values are
/// also redacted since the notice promises no env *values* are included.
fn redact_env_values(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key.eq_ignore_ascii_case("env") {
                    if let serde_json::Value::Object(env_map) = child {
                        for env_val in env_map.values_mut() {
                            if !env_val.is_null() {
                                *env_val = serde_json::Value::String("[REDACTED]".to_string());
                            }
                        }
                        continue;
                    }
                }
                redact_env_values(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_env_values(item);
            }
        }
        _ => {}
    }
}

/// Generate or retrieve an anonymous install ID stored in the config directory.
fn get_or_create_install_id() -> String {
    let id_path = dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("agentbro")
        .join("install_id");
    if let Ok(id) = std::fs::read_to_string(&id_path) {
        let trimmed = id.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = std::fs::create_dir_all(id_path.parent().unwrap());
    let _ = std::fs::write(&id_path, &id);
    id
}

/// Build sanitized config JSON — redacts secrets, SSH targets, webhook URLs.
fn sanitized_config_json(config: &AppConfig) -> serde_json::Value {
    let mut val = serde_json::to_value(config).unwrap_or_default();

    // Redact webhook configs
    if let Some(webhooks) = val.get_mut("webhookConfigs").and_then(|v| v.as_array_mut()) {
        for wh in webhooks.iter_mut() {
            if let Some(url) = wh.get_mut("url") {
                *url = serde_json::Value::String("[REDACTED]".to_string());
            }
            if let Some(secret) = wh.get_mut("secret") {
                if !secret.is_null() {
                    *secret = serde_json::Value::String("[REDACTED]".to_string());
                }
            }
        }
    }

    // Redact remote hosts
    if let Some(hosts) = val.get_mut("remoteHosts").and_then(|v| v.as_array_mut()) {
        for host in hosts.iter_mut() {
            if let Some(ssh_target) = host.get_mut("sshTarget") {
                *ssh_target = serde_json::Value::String("[REDACTED]".to_string());
            }
            if let Some(identity_file) = host.get_mut("identityFile") {
                if identity_file.is_string() {
                    *identity_file = serde_json::Value::String("[REDACTED]".to_string());
                }
            }
            if let Some(auth_socket) = host.get_mut("authSocket") {
                if auth_socket.is_string() {
                    *auth_socket = serde_json::Value::String("[REDACTED]".to_string());
                }
            }
            if let Some(remote_socket_path) = host.get_mut("remoteSocketPath") {
                let s = remote_socket_path.as_str().unwrap_or("");
                *remote_socket_path = serde_json::Value::String(redact_paths(s));
            }
        }
    }

    // Redact buddy device shared secret
    if let Some(secret) = val.pointer_mut("/buddyDevice/sharedSecret") {
        if secret.is_string() && !secret.as_str().unwrap_or("").is_empty() {
            *secret = serde_json::Value::String("[REDACTED]".to_string());
        }
    }

    // Redact custom hooks install paths
    if let Some(installs) = val
        .get_mut("customHookInstalls")
        .and_then(|v| v.as_array_mut())
    {
        for inst in installs.iter_mut() {
            if let Some(dir) = inst.get_mut("installDirectory") {
                let s = dir.as_str().unwrap_or("");
                *dir = serde_json::Value::String(redact_paths(s));
            }
        }
    }

    // Redact engine instance config roots
    if let Some(instances) = val
        .get_mut("engineInstances")
        .and_then(|v| v.as_array_mut())
    {
        for inst in instances.iter_mut() {
            if let Some(root) = inst.get_mut("configRoot") {
                let s = root.as_str().unwrap_or("");
                *root = serde_json::Value::String(redact_paths(s));
            }
        }
    }

    // Redact custom sounds data URLs (may contain embedded file paths)
    if let Some(sounds) = val.get_mut("customSounds").and_then(|v| v.as_array_mut()) {
        for sound in sounds.iter_mut() {
            if let Some(data_url) = sound.get_mut("dataUrl") {
                if data_url.is_string() {
                    *data_url = serde_json::Value::String("[REDACTED]".to_string());
                }
            }
            if let Some(path) = sound.get_mut("path") {
                let s = path.as_str().unwrap_or("");
                *path = serde_json::Value::String(redact_paths(s));
            }
        }
    }

    if let Some(value) = val.get_mut("excludedHookCwdSubstrings") {
        let s = value.as_str().unwrap_or("");
        *value = serde_json::Value::String(redact_paths(s));
    }
    if let Some(rules) = val
        .get_mut("sessionSilenceRules")
        .and_then(|v| v.as_array_mut())
    {
        for rule in rules.iter_mut() {
            let kind = rule
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if let Some(pattern) = rule.get_mut("pattern") {
                let s = pattern.as_str().unwrap_or("");
                *pattern = serde_json::Value::String(if kind == "cwd" {
                    redact_paths(s)
                } else {
                    "[REDACTED]".to_string()
                });
            }
        }
    }

    val
}

/// Collect hook config file contents from all adapters, with path redaction.
fn collect_hooks_sections(adapters: &[Arc<dyn AgentAdapter>]) -> Vec<String> {
    let mut sections = Vec::new();
    for adapter in adapters {
        let paths = adapter.hook_config_paths();
        if paths.is_empty() {
            continue;
        }
        let mut block = format!(
            "### {}\n\n> ID: `{}`\n\n",
            adapter.display_name(),
            adapter.name()
        );
        for path in &paths {
            let display_path = redact_paths(&path.to_string_lossy());
            if path.is_dir() {
                for name in ["plugin.yaml", "__init__.py", "settings.json", "hooks.json"] {
                    let candidate = path.join(name);
                    if candidate.exists() {
                        let cp = redact_paths(&candidate.to_string_lossy());
                        match std::fs::read_to_string(&candidate) {
                            Ok(content) => {
                                block.push_str(&format!(
                                    "**{}**\n```json\n{}\n```\n\n",
                                    cp,
                                    redact_sensitive_hook_config(&content)
                                ));
                            }
                            Err(e) => {
                                block.push_str(&format!("**{}** — _read error: {}_\n\n", cp, e));
                            }
                        }
                    }
                }
            } else if path.exists() {
                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        block.push_str(&format!(
                            "**{}**\n```json\n{}\n```\n\n",
                            display_path,
                            redact_sensitive_hook_config(&content)
                        ));
                    }
                    Err(e) => {
                        block.push_str(&format!("**{}** — _read error: {}_\n\n", display_path, e));
                    }
                }
            } else {
                block.push_str(&format!("**{}** — _not found_\n\n", display_path));
            }
        }
        sections.push(block);
    }
    sections
}

/// Read recent log files from tauri-plugin-log's log directory.
fn collect_log_files() -> Vec<(String, Vec<u8>)> {
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("agentbro")
        .join("logs");
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&log_dir) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));
        for entry in entries.into_iter().rev().take(3) {
            let path = entry.path();
            if path.is_file() {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                match std::fs::read(&path) {
                    Ok(data) => {
                        let redacted = redact_paths(&String::from_utf8_lossy(&data));
                        files.push((format!("logs/{}", name), redacted.into_bytes()));
                    }
                    Err(_) => continue,
                }
            }
        }
    }
    files
}

fn bridge_invocations_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".agentbro")
        .join("hook-invocations.jsonl")
}

fn recent_bridge_invocations(limit: usize) -> Vec<String> {
    let path = bridge_invocations_path();
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut lines = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if lines.len() > limit {
        lines.drain(0..lines.len() - limit);
    }
    lines
}

/// Collect crash reports matching AgentBro from the system DiagnosticReports dir.
fn collect_crash_reports() -> Vec<(String, Vec<u8>)> {
    let crash_dir = PathBuf::from("/Library/Logs/DiagnosticReports");
    let user_crash_dir = dirs::home_dir()
        .map(|h| h.join("Library/Logs/DiagnosticReports"))
        .unwrap_or_else(|| PathBuf::from("/tmp/none"));
    let mut files = Vec::new();
    for dir in &[crash_dir, user_crash_dir] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name.contains("AgentBro") || name.contains("agentbro") {
                    if let Ok(data) = std::fs::read(&path) {
                        let redacted = redact_paths(&String::from_utf8_lossy(&data));
                        files.push((format!("crashes/{}", name), redacted.into_bytes()));
                    }
                }
            }
        }
    }
    files
}

#[tauri::command]
pub async fn export_diagnostics(
    state: State<'_, AppState>,
    target_path: String,
) -> Result<(), String> {
    let config = state.config_store.get();
    let sessions = state.session_store.get_all_sessions();
    let diagnostic_events = state.diagnostic_buffer.all();
    let raw_event_summaries = state.hook_server.recent_raw_event_summaries(500);
    let bridge_invocations = recent_bridge_invocations(500);
    let install_id = get_or_create_install_id();
    let now = chrono::Local::now();
    let timestamp = now.format("%Y-%m-%d %H:%M:%S %Z").to_string();

    // ── Build Markdown report ──
    let mut md = String::new();

    // Header
    md.push_str("# AgentBro Diagnostic Report\n\n");
    md.push_str("| Field | Value |\n|---|---|\n");
    md.push_str(&format!("| Generated | {} |\n", timestamp));
    md.push_str(&format!("| Version | {} |\n", env!("CARGO_PKG_VERSION")));
    md.push_str(&format!("| Install ID | {} |\n", install_id));
    md.push_str(&format!(
        "| Platform | {} / {} |\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    md.push_str("\n---\n\n");

    // Privacy notice
    md.push_str("> **Privacy:** Home paths are masked as `<HOME>`. Webhook URLs, secrets, SSH targets, and credentials are replaced with `[REDACTED]`. Session content and environment variable values are never included.\n\n---\n\n");

    // Adapter status table
    md.push_str("## Supported Agents\n\n");
    md.push_str("| Agent | ID | Status | Hooks |\n|---|---|---|---|\n");
    for adapter in state.adapters.iter() {
        let status_str = match adapter.status() {
            crate::agents::AdapterStatus::Active => "✅ Active",
            crate::agents::AdapterStatus::Installed => "📦 Installed",
            crate::agents::AdapterStatus::Available => "⚡ Available",
            crate::agents::AdapterStatus::Unavailable => "— Unavailable",
        };
        let hooks_str = if adapter.hooks_installed() {
            "✓"
        } else {
            "—"
        };
        md.push_str(&format!(
            "| {} | `{}` | {} | {} |\n",
            adapter.display_name(),
            adapter.name(),
            status_str,
            hooks_str
        ));
    }
    md.push('\n');

    // CLI tool versions
    md.push_str("## CLI Tools\n\n");
    md.push_str("| Tool | Version |\n|---|---|\n");
    for tool in &["claude", "cursor", "codex", "aider", "gemini"] {
        let version = std::process::Command::new(tool)
            .arg("--version")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "—".to_string());
        md.push_str(&format!("| `{}` | {} |\n", tool, version));
    }
    md.push_str("\n---\n\n");

    // Sessions overview
    md.push_str("## Sessions\n\n");
    md.push_str(&format!("**Total:** {}\n\n", sessions.len()));
    let mut by_agent: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_phase: BTreeMap<String, usize> = BTreeMap::new();
    for s in &sessions {
        *by_agent.entry(s.agent_type.clone()).or_insert(0) += 1;
        *by_phase.entry(format!("{:?}", s.phase)).or_insert(0) += 1;
    }
    if !by_agent.is_empty() {
        md.push_str("| Agent | Count |\n|---|---|\n");
        for (agent, count) in &by_agent {
            md.push_str(&format!("| {} | {} |\n", agent, count));
        }
        md.push('\n');
    }
    if !by_phase.is_empty() {
        md.push_str("| Phase | Count |\n|---|---|\n");
        for (phase, count) in &by_phase {
            md.push_str(&format!("| {} | {} |\n", phase, count));
        }
        md.push('\n');
    }
    md.push_str("---\n\n");

    // Hook configurations
    md.push_str("## Hook Configurations\n\n");
    let hooks_sections = collect_hooks_sections(&state.adapters);
    if hooks_sections.is_empty() {
        md.push_str("_No hook configurations found._\n\n");
    } else {
        for section in &hooks_sections {
            md.push_str(section);
        }
    }
    md.push_str("---\n\n");

    // Raw hook event summaries
    md.push_str("## Recent Hook Events\n\n");
    if raw_event_summaries.is_empty() {
        md.push_str("_No raw hook events recorded._\n\n");
    } else {
        md.push_str(
            "| Seq | Agent | Event | Session | CWD | Payload keys |\n|---|---|---|---|---|---|\n",
        );
        for event in &raw_event_summaries {
            let cwd = event
                .cwd
                .as_deref()
                .map(redact_paths)
                .unwrap_or_else(|| "—".to_string());
            md.push_str(&format!(
                "| {} | {} | {} | `{}` | {} | {} |\n",
                event.seq,
                event.agent.as_deref().unwrap_or("—"),
                event.event_name,
                event.session_id,
                cwd,
                event.payload_keys.join(", ")
            ));
        }
        md.push('\n');
    }
    md.push_str("---\n\n");

    // Bridge invocation summaries
    md.push_str("## Recent Bridge Invocations\n\n");
    if bridge_invocations.is_empty() {
        md.push_str("_No bridge invocations recorded._\n\n");
    } else {
        md.push_str("```jsonl\n");
        for line in &bridge_invocations {
            md.push_str(&redact_paths(line));
            md.push('\n');
        }
        md.push_str("```\n\n");
    }
    md.push_str("---\n\n");

    // Diagnostic events
    md.push_str("## Recent Events\n\n");
    if diagnostic_events.is_empty() {
        md.push_str("_No diagnostic events recorded._\n\n");
    } else {
        md.push_str("```json\n");
        let events_json = serde_json::to_string_pretty(&diagnostic_events)
            .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
        md.push_str(&redact_paths(&events_json));
        md.push_str("\n```\n\n");
    }
    md.push_str("---\n\n");

    // Archive contents
    md.push_str("## Archive Contents\n\n");
    md.push_str("| File | Description |\n|---|---|\n");
    md.push_str("| `Diagnostic-Report.md` | This report |\n");
    md.push_str("| `config.json` | Sanitized app configuration (JSON) |\n");
    md.push_str(
        "| `recent-hook-events.json` | Recent hook event summaries without raw payload content |\n",
    );
    md.push_str(
        "| `bridge-invocations.jsonl` | Recent hook bridge invocations without prompt or payload content |\n",
    );
    md.push_str("| `logs/` | Recent application logs |\n");
    md.push_str("| `crashes/` | System crash reports (if any) |\n");

    // ── Sanitized config as standalone JSON ──
    let config_json = serde_json::to_string_pretty(&sanitized_config_json(&config))
        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));

    // ── Build the zip ──
    let file = std::fs::File::create(&target_path)
        .map_err(|e| format!("Failed to create zip file: {}", e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let write_text = |zip: &mut zip::ZipWriter<std::fs::File>,
                      name: &str,
                      content: &str|
     -> Result<(), String> {
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        zip.write_all(content.as_bytes()).map_err(|e| e.to_string())
    };

    write_text(&mut zip, "Diagnostic-Report.md", &md)?;
    write_text(&mut zip, "config.json", &redact_paths(&config_json))?;
    let hook_events_json = serde_json::to_string_pretty(&raw_event_summaries)
        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
    write_text(
        &mut zip,
        "recent-hook-events.json",
        &redact_paths(&hook_events_json),
    )?;
    write_text(
        &mut zip,
        "bridge-invocations.jsonl",
        &redact_paths(&bridge_invocations.join("\n")),
    )?;

    // Logs
    for (name, data) in collect_log_files() {
        zip.start_file(&name, options).map_err(|e| e.to_string())?;
        zip.write_all(&data).map_err(|e| e.to_string())?;
    }

    // Crash reports
    for (name, data) in collect_crash_reports() {
        zip.start_file(&name, options).map_err(|e| e.to_string())?;
        zip.write_all(&data).map_err(|e| e.to_string())?;
    }

    zip.finish()
        .map_err(|e| format!("Failed to finalize zip: {}", e))?;

    log::info!("Diagnostics exported to {}", redact_paths(&target_path));
    Ok(())
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
        app_host_message_unsupported_error, can_fallback_to_terminal_app,
        codex_answers_for_pending_question, codex_app_server_pending_question,
        codex_app_server_permission_response, codex_app_server_refresh_interval_seconds,
        codex_desktop_send_message_script, codex_desktop_send_message_without_activation_script,
        codex_phase_from_thread, codex_request_user_input_output, codex_turn_steer_payload,
        fallback_terminal_app_name, handle_codex_app_server_request, is_codex_desktop_session,
        is_ide_terminal_session, is_uuid_like, parse_subagent_chat_history_for_session,
        qoder_app_send_message_script, read_codex_session_meta_from_path,
        redact_sensitive_hook_config, remote_session_chat_history, resolve_session_tty,
        sync_codex_app_server_thread_to_store, sync_remote_codex_thread_to_store,
        terminal_hint_for_fallback, CodexAppServerPendingKind, CodexAppServerPendingRequest,
    };
    #[cfg(target_os = "windows")]
    use super::{clean_windows_app_user_model_id, powershell_string_literal};
    use crate::energy::EnergyMode;
    use crate::hooks::conversation_parser::{ChatRole, MessageBlock};
    use crate::hooks::server::RawHookEvent;
    use crate::hooks::session_store::{
        PendingQuestion, QuestionItem, QuestionOption, SessionPhase, SessionState, SessionStore,
        SubagentInfo,
    };
    use crate::remote::installer::RemoteCodexThreadSnapshot;
    use crate::remote::RemoteHost;
    use std::{collections::HashMap, fs};

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
    fn codex_app_server_status_maps_to_attention_phase() {
        let thread = serde_json::json!({
            "status": {
                "type": "active",
                "activeFlags": ["waitingOnUserInput"]
            }
        });

        assert_eq!(codex_phase_from_thread(&thread), SessionPhase::WaitingInput);
    }

    #[test]
    fn codex_app_server_thread_sync_updates_session_store() {
        let store = SessionStore::new();
        let thread = serde_json::json!({
            "id": "thread-1",
            "name": "Fix flaky tests",
            "preview": "Investigate test timing",
            "cwd": "/tmp/agentbro",
            "createdAt": 1_700_000_000,
            "updatedAt": 1_700_000_120,
            "status": { "type": "active", "activeFlags": [] },
            "turns": [{
                "items": [
                    {
                        "type": "userMessage",
                        "content": [{ "type": "input_text", "text": "Please fix tests" }]
                    },
                    {
                        "type": "agentMessage",
                        "text": "I found the timing issue.",
                        "phase": "final"
                    }
                ]
            }]
        });

        let summary = sync_codex_app_server_thread_to_store(&store, &thread).unwrap();
        let session = store.get_session("thread-1").unwrap();

        assert_eq!(summary.phase, "Processing");
        assert_eq!(session.agent_type, "codex");
        assert_eq!(session.engine_label.as_deref(), Some("Codex App"));
        assert_eq!(session.project, "Fix flaky tests");
        assert_eq!(session.cwd, "/tmp/agentbro");
        assert_eq!(session.phase, SessionPhase::Processing);
        assert_eq!(
            session.last_user_message.as_deref(),
            Some("Please fix tests")
        );
        assert_eq!(
            session.last_response.as_deref(),
            Some("I found the timing issue.")
        );
        assert_eq!(session.term_bundle_id.as_deref(), Some("com.openai.codex"));
    }

    #[test]
    fn remote_codex_state_sync_marks_session_as_remote_codex_app() {
        let store = SessionStore::new();
        let host = RemoteHost {
            id: "host-1".to_string(),
            name: "GPU Box".to_string(),
            ssh_target: "dev@gpu-box".to_string(),
            port: Some(22),
            identity_file: None,
            auth_socket: None,
            remote_socket_path: "/tmp/agentbro-remote.sock".to_string(),
            auto_connect: true,
        };
        let thread = RemoteCodexThreadSnapshot {
            id: "remote-thread-1".to_string(),
            cwd: "/srv/project".to_string(),
            title: Some("Ship remote Codex".to_string()),
            preview: Some("Remote Codex changed files.".to_string()),
            rollout_path: Some("/home/dev/.codex/sessions/rollout.jsonl".to_string()),
            source: Some("codex".to_string()),
            thread_source: Some("app-server".to_string()),
            updated_at_ms: 1_780_070_000_123,
        };

        let summary = sync_remote_codex_thread_to_store(&store, &host, &thread).unwrap();
        let session = store.get_session("remote-thread-1").unwrap();

        assert_eq!(summary.status.as_deref(), Some("app-server"));
        assert_eq!(session.agent_type, "codex");
        assert_eq!(session.engine_label.as_deref(), Some("Codex App · GPU Box"));
        assert_eq!(session.remote_host_id.as_deref(), Some("host-1"));
        assert_eq!(session.remote_host_name.as_deref(), Some("GPU Box"));
        assert_eq!(session.terminal, "GPU Box");
        assert_eq!(session.phase, SessionPhase::Processing);
        assert_eq!(session.last_main_agent_at, Some(1_780_070_000));
        assert_eq!(
            session.last_response.as_deref(),
            Some("Remote Codex changed files.")
        );
    }

    #[tokio::test]
    async fn codex_app_server_permission_request_creates_pending_session() {
        let store = SessionStore::new();
        let mut pending = HashMap::new();
        let params = serde_json::json!({
            "threadId": "thread-approval",
            "command": ["pnpm", "test"],
            "cwd": "/tmp/agentbro",
            "reason": "Run the test suite"
        });

        handle_codex_app_server_request(
            &store,
            &mut pending,
            serde_json::json!("request-1"),
            "item/commandExecution/requestApproval",
            &params,
        )
        .await
        .unwrap();

        let session = store.get_session("thread-approval").unwrap();
        assert_eq!(session.phase, SessionPhase::WaitingApproval);
        assert_eq!(session.cwd, "/tmp/agentbro");
        assert_eq!(session.engine_label.as_deref(), Some("Codex App"));
        assert_eq!(
            session
                .pending_permission
                .as_ref()
                .map(|p| p.tool_name.as_str()),
            Some("exec_command")
        );
        assert_eq!(
            pending.get("thread-approval").map(|p| &p.kind),
            Some(&CodexAppServerPendingKind::CommandApproval)
        );
    }

    #[test]
    fn codex_app_server_thread_sync_preserves_pending_interaction() {
        let store = SessionStore::new();
        store.get_or_create_session("thread-waiting", "codex", "Codex", "/", "Codex");
        store.set_pending_permission(
            "thread-waiting",
            Some(crate::hooks::session_store::PendingPermission {
                tool_use_id: Some("approval-1".to_string()),
                tool_name: "exec_command".to_string(),
                tool_input: "pnpm test".to_string(),
                diff: None,
                options: None,
            }),
        );
        let thread = serde_json::json!({
            "id": "thread-waiting",
            "name": "Waiting thread",
            "cwd": "/tmp/agentbro",
            "status": { "type": "active", "activeFlags": [] }
        });

        sync_codex_app_server_thread_to_store(&store, &thread).unwrap();
        let session = store.get_session("thread-waiting").unwrap();

        assert_eq!(session.phase, SessionPhase::WaitingApproval);
        assert!(session.pending_permission.is_some());
    }

    #[test]
    fn codex_app_server_energy_policy_slows_down_when_quiet() {
        let store = SessionStore::new();

        assert_eq!(
            codex_app_server_refresh_interval_seconds(&store, 15),
            (EnergyMode::QuietBackground, 300)
        );

        store.get_or_create_session("idle-thread", "codex", "Codex", "/", "Codex");
        store.update_phase("idle-thread", SessionPhase::Idle);
        assert_eq!(
            codex_app_server_refresh_interval_seconds(&store, 15),
            (EnergyMode::IdleVisible, 60)
        );

        store.update_phase("idle-thread", SessionPhase::Processing);
        assert_eq!(
            codex_app_server_refresh_interval_seconds(&store, 15),
            (EnergyMode::Active, 15)
        );
    }

    #[test]
    fn codex_app_server_question_request_maps_options_and_payload() {
        let params = serde_json::json!({
            "questions": [{
                "id": "target",
                "header": "Target",
                "question": "Which target?",
                "isMultiple": true,
                "options": [
                    { "label": "Preview", "description": "Dry run" },
                    { "label": "Ship" }
                ]
            }]
        });

        let pending = codex_app_server_pending_question(&params, &serde_json::json!("req-q"));
        assert_eq!(
            pending.source.as_deref(),
            Some("codex_app_server_request_user_input")
        );
        assert_eq!(pending.response_mode.as_deref(), Some("app_server"));
        assert_eq!(pending.questions[0].id.as_deref(), Some("target"));
        assert!(pending.questions[0].multi_select);
        assert_eq!(
            pending.options,
            vec!["Preview".to_string(), "Ship".to_string()]
        );

        let response = codex_app_server_permission_response(
            &CodexAppServerPendingRequest {
                request_id: serde_json::json!("perm-1"),
                kind: CodexAppServerPendingKind::PermissionsApproval,
                requested_permissions: Some(
                    serde_json::json!({ "network": { "domains": ["example.com"] } }),
                ),
            },
            true,
            true,
        );
        assert_eq!(response["scope"], "session");
        assert!(response["permissions"].get("network").is_some());
    }

    #[test]
    fn remote_session_chat_history_uses_raw_hook_events() {
        let mut session = session("claude-code", "", None);
        session.id = "remote-session".to_string();
        session.remote_host_id = Some("host-1".to_string());
        session.last_response = Some("Fallback response".to_string());

        let messages = remote_session_chat_history(
            &session,
            vec![
                RawHookEvent {
                    seq: 1,
                    timestamp_ms: 1_700_000_000_000,
                    session_id: "remote-session".to_string(),
                    agent: Some("claude-code".to_string()),
                    event_name: "UserPromptSubmit".to_string(),
                    raw: serde_json::json!({
                        "prompt": "你好"
                    }),
                },
                RawHookEvent {
                    seq: 2,
                    timestamp_ms: 1_700_000_001_000,
                    session_id: "remote-session".to_string(),
                    agent: Some("claude-code".to_string()),
                    event_name: "Stop".to_string(),
                    raw: serde_json::json!({
                        "summary": "你好！有什么我可以帮你的吗？"
                    }),
                },
            ],
        );

        assert_eq!(messages.len(), 2);
        assert!(matches!(
            messages[0].role,
            crate::hooks::conversation_parser::ChatRole::User
        ));
        assert!(matches!(
            messages[1].role,
            crate::hooks::conversation_parser::ChatRole::Assistant
        ));
        assert!(matches!(
            &messages[1].blocks[0],
            crate::hooks::conversation_parser::MessageBlock::Text { text } if text.contains("帮你")
        ));
    }

    #[test]
    fn remote_session_chat_history_adds_session_completion_when_stop_is_generic() {
        let mut session = session("claude-code", "", None);
        session.id = "remote-session".to_string();
        session.remote_host_id = Some("host-1".to_string());
        session.last_response = Some("Task completed".to_string());

        let messages = remote_session_chat_history(
            &session,
            vec![RawHookEvent {
                seq: 1,
                timestamp_ms: 1_700_000_000_000,
                session_id: "remote-session".to_string(),
                agent: Some("claude-code".to_string()),
                event_name: "UserPromptSubmit".to_string(),
                raw: serde_json::json!({
                    "prompt": "hi"
                }),
            }],
        );

        assert_eq!(messages.len(), 2);
        assert!(matches!(
            messages[0].role,
            crate::hooks::conversation_parser::ChatRole::User
        ));
        assert!(matches!(
            messages[1].role,
            crate::hooks::conversation_parser::ChatRole::Assistant
        ));
        assert!(matches!(
            &messages[1].blocks[0],
            crate::hooks::conversation_parser::MessageBlock::Text { text } if text == "Task completed"
        ));
    }

    #[test]
    fn diagnostics_hook_config_redacts_inline_secrets_and_remote_urls() {
        let redacted = redact_sensitive_hook_config(
            r#"{
  "command": "curl -H 'Authorization: Bearer abc123' https://hooks.example.com/path?token=abc",
  "safe": "http://localhost:17894"
}"#,
        );

        assert!(!redacted.contains("abc123"));
        assert!(!redacted.contains("hooks.example.com"));
        assert!(redacted.contains("[REDACTED sensitive hook config line]"));
        assert!(redacted.contains("http://localhost:17894"));
    }

    #[test]
    fn diagnostics_hook_config_redacts_https_before_later_http_url() {
        let redacted = redact_sensitive_hook_config(
            r#"{"remote":"https://example.com/callback?id=abc","local":"http://localhost:17894"}"#,
        );

        assert!(!redacted.contains("example.com"));
        assert!(redacted.contains("[REDACTED_URL]"));
        assert!(redacted.contains("http://localhost:17894"));
    }

    #[test]
    fn diagnostics_hook_config_redacts_all_env_values() {
        let redacted = redact_sensitive_hook_config(
            r#"{
  "env": {
    "ANTHROPIC_BASE_URL": "http://127.0.0.1:20128/v1",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "h-combo",
    "CUSTOM_CREDENTIAL": "super-sensitive-value"
  }
}"#,
        );

        // Key names stay (useful for diagnostics), every value is gone — even
        // ones with innocuous names that the keyword blocklist would miss.
        assert!(redacted.contains("ANTHROPIC_BASE_URL"));
        assert!(redacted.contains("CUSTOM_CREDENTIAL"));
        assert!(!redacted.contains("super-sensitive-value"));
        assert!(!redacted.contains("h-combo"));
        assert!(!redacted.contains("20128"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn codex_desktop_detection_uses_bundle_or_missing_tty() {
        assert!(is_codex_desktop_session(&session("codex", "", None)));
        assert!(is_codex_desktop_session(&session("codex", "Codex", None)));
        assert!(!is_codex_desktop_session(&session(
            "codex",
            "Codex",
            Some("/dev/ttys001")
        )));
        let mut bundle_session = session("codex", "iTerm2", Some("/dev/ttys001"));
        bundle_session.term_bundle_id = Some("com.openai.codex".to_string());
        assert!(is_codex_desktop_session(&bundle_session));
        assert!(!is_codex_desktop_session(&session(
            "codex", "AgentBro", None
        )));
        // CLI: a tty without Codex app metadata should stay on the terminal path.
        assert!(!is_codex_desktop_session(&session(
            "codex",
            "",
            Some("/dev/ttys001")
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

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_app_user_model_id_strips_notification_query() {
        assert_eq!(
            clean_windows_app_user_model_id("OpenAI.Codex_2p2nqsd0c76g0!App?type=click&tag=123")
                .as_deref(),
            Some("OpenAI.Codex_2p2nqsd0c76g0!App")
        );
        assert_eq!(
            clean_windows_app_user_model_id(r"C:\Program Files\WindowsApps\OpenAI.Codex\App"),
            None
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn powershell_string_literal_escapes_single_quotes() {
        assert_eq!(
            powershell_string_literal("OpenAI.Codex_abc!App"),
            "'OpenAI.Codex_abc!App'"
        );
        assert_eq!(powershell_string_literal("A'B"), "'A''B'");
    }

    #[test]
    fn codex_session_meta_reads_originator_and_source() {
        let nonce = uuid::Uuid::new_v4();
        let path = std::env::temp_dir().join(format!("agentbro-codex-meta-{nonce}.jsonl"));
        fs::write(
            &path,
            r#"{"type":"session_meta","payload":{"originator":"codex_cli_rs","source":"cli"}}
{"type":"event_msg","payload":{"type":"token_count"}}"#,
        )
        .expect("write codex meta");

        let meta = read_codex_session_meta_from_path(&path).expect("read meta");
        assert_eq!(meta.originator.as_deref(), Some("codex_cli_rs"));
        assert_eq!(meta.source.as_deref(), Some("cli"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn codex_desktop_message_send_uses_accessibility_script() {
        let script = codex_desktop_send_message_script("挺好 \"Codex\"\nnext");

        assert!(script.contains("keystroke \"v\" using command down"));
        assert!(script.contains("name is \"发送\""));
        assert!(script.contains("description is \"发送\""));
        assert!(
            script.contains("set the clipboard to \"挺好 \\\"Codex\\\"\" & linefeed & \"next\"")
        );
        assert!(script.contains("key code 36"));
    }

    #[test]
    fn codex_desktop_background_send_uses_app_accessibility_not_cli() {
        let script =
            codex_desktop_send_message_without_activation_script("挺好 \"Codex\"\nnext", Some(42));

        assert!(script.contains("application processes whose unix id is 42"));
        assert!(script.contains("role is \"AXTextArea\""));
        assert!(script.contains("set value of inputField to messageText"));
        assert!(script.contains("perform action \"AXPress\""));
        assert!(script.contains("name is \"发送\""));
        assert!(!script.contains("keystroke"));
        assert!(!script.contains("exec"));
        assert!(!script.contains("resume"));
    }

    #[test]
    fn codex_desktop_message_send_is_supported_app_host() {
        let mut session = session("codex", "", Some("/dev/ttys001"));
        session.term_bundle_id = Some("com.openai.codex".to_string());

        assert!(is_codex_desktop_session(&session));
        assert!(app_host_message_unsupported_error(&session).is_none());
    }

    #[test]
    fn app_host_message_send_blocks_non_terminal_app_bundles() {
        let mut app_session = session("claude-code", "", Some("/dev/ttys001"));
        app_session.term_bundle_id = Some("com.example.agenthost".to_string());
        assert!(app_host_message_unsupported_error(&app_session)
            .unwrap()
            .contains("App-hosted sessions do not support"));

        let mut terminal_session = session("claude-code", "iTerm2", Some("/dev/ttys001"));
        terminal_session.term_bundle_id = Some("com.googlecode.iterm2".to_string());
        assert!(app_host_message_unsupported_error(&terminal_session).is_none());
    }

    #[test]
    fn ide_host_bundles_are_terminal_sessions_when_source_does_not_match() {
        let mut claude_in_cursor = session("claude-code", "Cursor", Some("/dev/ttys001"));
        claude_in_cursor.term_bundle_id = Some("com.todesktop.230313mzl4w4u92".to_string());

        assert!(is_ide_terminal_session(&claude_in_cursor));
        assert!(app_host_message_unsupported_error(&claude_in_cursor).is_none());

        let mut cursor_app = session("cursor", "Cursor", None);
        cursor_app.term_bundle_id = Some("com.todesktop.230313mzl4w4u92".to_string());
        assert!(!is_ide_terminal_session(&cursor_app));

        let mut claude_in_qoder = session("claude-code", "Qoder", Some("/dev/ttys002"));
        claude_in_qoder.term_bundle_id = Some("com.qoder.ide".to_string());
        assert!(is_ide_terminal_session(&claude_in_qoder));
        assert!(app_host_message_unsupported_error(&claude_in_qoder).is_none());
    }

    #[test]
    fn qoder_app_message_send_is_supported_app_host() {
        let mut session = session("qoder", "Qoder", None);
        session.term_bundle_id = Some("com.qoder.ide".to_string());

        assert!(app_host_message_unsupported_error(&session).is_none());
    }

    #[test]
    fn qoder_app_send_uses_accessibility_script() {
        let script = qoder_app_send_message_script("继续 \"Qoder\"\nnext", Some(42));

        assert!(script.contains("application processes whose unix id is 42"));
        assert!(script.contains("bundle identifier of candidateProcess is \"com.qoder.ide\""));
        assert!(script.contains("role is \"AXTextArea\""));
        assert!(script.contains("keystroke \"v\" using command down"));
        assert!(script.contains("name is \"Send message\""));
        assert!(script.contains("description is \"Send message\""));
        assert!(
            script.contains("set the clipboard to \"继续 \\\"Qoder\\\"\" & linefeed & \"next\"")
        );
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
        assert_eq!(
            fallback_terminal_app_name("dev.commandline.waveterm"),
            "Wave"
        );
        assert_eq!(fallback_terminal_app_name("AntCC"), "Terminal");
        assert_eq!(fallback_terminal_app_name(""), "Terminal");
    }

    #[test]
    fn jump_fallback_uses_terminal_environment_hint_when_terminal_is_empty() {
        let mut env_session = session("claude-code", "", None);
        env_session.term_program = Some("iTerm.app".to_string());
        assert_eq!(terminal_hint_for_fallback(&env_session), "iTerm");

        let mut bundle_session = session("claude-code", "", None);
        bundle_session.term_bundle_id = Some("com.mitchellh.ghostty".to_string());
        assert_eq!(terminal_hint_for_fallback(&bundle_session), "Ghostty");

        let mut wave_session = session("claude-code", "", None);
        wave_session.term_bundle_id = Some("dev.commandline.waveterm".to_string());
        assert_eq!(terminal_hint_for_fallback(&wave_session), "Wave");
    }

    #[test]
    fn codex_thread_jump_requires_uuid_like_ids() {
        assert!(is_uuid_like("123e4567-e89b-12d3-a456-426614174000"));
        assert!(!is_uuid_like("not-a-thread-id"));
        assert!(!is_uuid_like("123e4567e89b12d3a456426614174000"));
    }

    #[test]
    fn codex_question_answer_payload_uses_question_ids() {
        let pending = PendingQuestion {
            question: "Pick one".to_string(),
            options: vec!["Preview".to_string(), "Ship".to_string()],
            descriptions: Vec::new(),
            header: None,
            multi_select: false,
            questions: vec![QuestionItem {
                id: Some("target".to_string()),
                question: "Pick one".to_string(),
                header: None,
                options: vec![
                    QuestionOption {
                        label: "Preview".to_string(),
                        description: None,
                    },
                    QuestionOption {
                        label: "Ship".to_string(),
                        description: None,
                    },
                ],
                multi_select: false,
            }],
            tool_use_id: Some("call_question_1".to_string()),
            source: Some("codex_rollout_request_user_input".to_string()),
            response_mode: Some("external_only".to_string()),
        };

        let answers = codex_answers_for_pending_question(&pending, "Ship");
        assert_eq!(answers.get("target"), Some(&vec!["Ship".to_string()]));
        assert_eq!(
            codex_request_user_input_output(answers),
            r#"{"answers":{"target":{"answers":["Ship"]}}}"#
        );
    }

    #[test]
    fn codex_question_answer_payload_maps_nested_json_answers() {
        let pending = PendingQuestion {
            question: "Questions".to_string(),
            options: Vec::new(),
            descriptions: Vec::new(),
            header: None,
            multi_select: false,
            questions: vec![
                QuestionItem {
                    id: Some("target".to_string()),
                    question: "Which target?".to_string(),
                    header: None,
                    options: Vec::new(),
                    multi_select: true,
                },
                QuestionItem {
                    id: Some("notify".to_string()),
                    question: "Notify?".to_string(),
                    header: None,
                    options: Vec::new(),
                    multi_select: false,
                },
            ],
            tool_use_id: Some("call_question_1".to_string()),
            source: Some("codex_rollout_request_user_input".to_string()),
            response_mode: Some("external_only".to_string()),
        };

        let answers = codex_answers_for_pending_question(
            &pending,
            r#"{"Which target?":"Preview, Ship","Notify?":"No"}"#,
        );
        assert_eq!(
            answers.get("target"),
            Some(&vec!["Preview".to_string(), "Ship".to_string()])
        );
        assert_eq!(answers.get("notify"), Some(&vec!["No".to_string()]));
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
            name: Some("audit-agent".to_string()),
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

    #[test]
    fn subagent_chat_history_synthesizes_codex_main_transcript_rows() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let transcript_path =
            std::env::temp_dir().join(format!("agentbro-codex-subagent-history-{nonce}.jsonl"));
        fs::write(&transcript_path, "").expect("write transcript");

        let transcript_path = transcript_path.to_string_lossy().to_string();
        let mut session = session("codex", "Codex App", None);
        session.subagents.push(SubagentInfo {
            agent_id: "019e73f1-5808-7a91-bfd4-2aadc13d2c77".to_string(),
            name: Some("Laplace".to_string()),
            agent_type: None,
            description: "请只计算这个表达式并返回最终结果：1+1。".to_string(),
            transcript_path: Some(transcript_path.clone()),
            agent_transcript_path: None,
            last_assistant_message: Some("2".to_string()),
            started_at: 1_780_061_657,
            completed_at: Some(1_780_061_664),
            status: "completed".to_string(),
            tools: Vec::new(),
        });
        session.subagents.push(SubagentInfo {
            agent_id: "019e73f1-5899-7342-9013-b3ffa5404cac".to_string(),
            name: Some("Newton".to_string()),
            agent_type: None,
            description: "请只计算这个表达式并返回最终结果：2+2。".to_string(),
            transcript_path: Some(transcript_path.clone()),
            agent_transcript_path: None,
            last_assistant_message: Some("4".to_string()),
            started_at: 1_780_061_657,
            completed_at: Some(1_780_061_668),
            status: "completed".to_string(),
            tools: Vec::new(),
        });

        let request_path =
            format!("{transcript_path}#agentbro-subagent=019e73f1-5899-7342-9013-b3ffa5404cac");
        let messages =
            parse_subagent_chat_history_for_session(&session, &request_path).expect("parse");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(messages[1].role, ChatRole::Assistant);
        match &messages[1].blocks[0] {
            MessageBlock::Text { text } => assert_eq!(text, "4"),
            other => panic!("expected text block, got {other:?}"),
        }

        let _ = fs::remove_file(transcript_path);
    }

    #[test]
    fn turn_steer_payload_ascii() {
        let payload = codex_turn_steer_payload(42, "thread-abc", "hello world");
        assert_eq!(payload["id"], 42);
        assert_eq!(payload["method"], "turn/steer");
        assert_eq!(payload["params"]["threadId"], "thread-abc");
        assert_eq!(payload["params"]["expectedTurnId"], "");
        assert_eq!(payload["params"]["input"][0]["type"], "text");
        assert_eq!(payload["params"]["input"][0]["text"], "hello world");
    }

    #[test]
    fn turn_steer_payload_unicode_and_multiline() {
        let text = "第一行\n第二行\n🚀 emoji";
        let payload = codex_turn_steer_payload(1, "t-1", text);
        assert_eq!(payload["params"]["input"][0]["text"], text);
    }
}
