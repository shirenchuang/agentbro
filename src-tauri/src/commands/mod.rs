// Tauri IPC Commands — Bridge between frontend and Rust backend

pub mod buddy;
pub mod monitor;
pub mod persistence;

use crate::agents::{AdapterInfo, AgentAdapter};
use crate::config::{AppConfig, ConfigStore};
use crate::hook_endpoint;
use crate::hooks::conversation_parser::{
    all_projects_dirs, discover_codex_session_file, discover_session_file_in_dirs,
    extract_subagents_from_transcript, ParsedMessage, TranscriptSubagentInfo,
};
use crate::hooks::diagnostics::DiagnosticRingBuffer;
use crate::hooks::file_watcher::ConversationWatcher;
use crate::hooks::server::HookServer;
use crate::hooks::session_store::{
    PendingQuestion, RateLimitInfo, SessionState, SessionStore, SubagentInfo, UsageRateWindow,
};
use crate::license::{LicenseManager, LicenseStatus};
use crate::network_monitor::NetworkMonitor;
use crate::platform::display_controller::DisplayController;
use crate::remote::RemoteManager;
use crate::sound::SoundEngine;
use crate::switch::db::SwitchDatabase;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader as StdBufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::State;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader};
use tokio::process::{ChildStdin, ChildStdout, Command as TokioCommand};

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
    pub switch_db: Arc<SwitchDatabase>,
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

#[tauri::command]
pub async fn authorize_usage_provider(provider: String) -> Result<(), String> {
    let (binary, args): (&str, &[&str]) = match provider.as_str() {
        "codex" => ("codex", &["login"]),
        "claude-code" | "claude" => ("claude", &["login"]),
        "gemini" | "gemini-cli" => ("gemini", &["auth"]),
        "copilot" => ("gh", &["auth", "login"]),
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
    [
        ("z-ai", "Z.ai", "Z.ai", "api/key", None, false),
        ("kimi", "Kimi", "Kimi", "web/token", Some("~/.kimi"), false),
        ("gemini-cli", "Gemini CLI", "Gemini", "api/oauth", Some("~/.gemini"), find_binary("gemini").is_some()),
        ("copilot", "GitHub Copilot", "Copilot", "api/device-flow", None, find_binary("gh").is_some()),
        ("cursor", "Cursor", "Cursor", "web/cookies", Some("~/.cursor"), false),
        ("cursor-cli", "Cursor CLI", "Cursor", "web/cookies", Some("~/.cursor"), false),
        ("deepseek", "DeepSeek", "DeepSeek", "api/key", Some("~/.deepseek"), false),
        ("opencode", "OpenCode", "OpenCode", "web/cookies", Some("~/.opencode"), false),
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
    .collect()
}

fn catalog_unsupported_agent_usage_providers(enabled: bool) -> Vec<UsageProviderStatus> {
    [
        ("trae", "Trae"),
        ("traecli", "Trae CLI"),
        ("traecn", "Trae CN"),
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
    let terminal = session.terminal.trim();

    session.agent_type == "codex"
        && session
            .tty
            .as_deref()
            .map_or(true, |tty| tty.trim().is_empty())
        && !terminal.starts_with("/dev/")
        && (terminal.is_empty() || terminal.to_ascii_lowercase().contains("codex"))
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

    if let Some(session) = state.session_store.get_session(&session_id) {
        if let Some(question) = session.pending_question.clone() {
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
    let formatted_answers = answers
        .into_iter()
        .map(|(key, values)| (key, serde_json::json!({ "answers": values })))
        .collect::<serde_json::Map<_, _>>();
    serde_json::json!({ "answers": formatted_answers }).to_string()
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
        detail: "Required for terminal focus".to_string(),
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

    Ok(HookDoctorReport {
        generated_at: chrono::Utc::now().timestamp(),
        checks,
    })
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
        can_fallback_to_terminal_app, codex_answers_for_pending_question, codex_exec_resume_args,
        codex_request_user_input_output, fallback_terminal_app_name, is_codex_desktop_session,
        parse_subagent_chat_history_for_session, resolve_session_tty,
    };
    use crate::hooks::session_store::{
        PendingQuestion, QuestionItem, QuestionOption, SessionState, SubagentInfo,
    };
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
        // Desktop background send is only safe when there is no foreground TTY.
        assert!(is_codex_desktop_session(&session("codex", "", None)));
        assert!(is_codex_desktop_session(&session("codex", "Codex", None)));
        assert!(!is_codex_desktop_session(&session(
            "codex",
            "Codex",
            Some("/dev/ttys001")
        )));
        let mut bundle_session = session("codex", "iTerm2", Some("/dev/ttys001"));
        bundle_session.term_bundle_id = Some("com.openai.codex".to_string());
        assert!(!is_codex_desktop_session(&bundle_session));
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
}
