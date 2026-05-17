use super::AppState;
use crate::hooks::conversation_parser::{discover_codex_session_file, discover_session_file};
use crate::hooks::server::RawHookEvent;
use crate::hooks::session_store::{SessionPhase, SessionState};
use crate::network_monitor::{NetworkMonitorStatus, NetworkRequestDetail, NetworkRequestSummary};
use serde::Serialize;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tauri::State;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSessionSummary {
    pub id: String,
    pub agent_type: String,
    pub engine_label: Option<String>,
    pub project: String,
    pub cwd: String,
    pub terminal: String,
    pub phase: String,
    pub started_at: i64,
    pub duration: i64,
    pub token_total: u64,
    pub last_tool_name: Option<String>,
    pub last_tool_target: Option<String>,
    pub last_tool_status: Option<String>,
    pub waiting_user: bool,
    pub pending_kind: Option<String>,
    pub subagent_count: usize,
    pub active_tool_count: usize,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSessionDetail {
    pub session: SessionState,
    pub timeline: Vec<MonitorTimelineItem>,
    pub raw_events: Vec<RawHookEvent>,
    pub transcript_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorTimelineItem {
    pub id: String,
    pub timestamp_ms: u64,
    pub kind: String,
    pub title: String,
    pub detail: Option<String>,
    pub status: Option<String>,
    pub tool_name: Option<String>,
    pub raw_event_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeWrapperStatus {
    pub installed: bool,
    pub shim_path: String,
    pub path_hint_installed: bool,
    pub shell_config_path: String,
}

#[tauri::command]
pub async fn get_monitor_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<MonitorSessionSummary>, String> {
    let mut sessions: Vec<MonitorSessionSummary> = state
        .session_store
        .get_all_sessions()
        .into_iter()
        .map(session_summary)
        .collect();

    sessions.sort_by(|a, b| {
        b.waiting_user
            .cmp(&a.waiting_user)
            .then_with(|| b.started_at.cmp(&a.started_at))
    });
    Ok(sessions)
}

#[tauri::command]
pub async fn get_monitor_session_detail(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<MonitorSessionDetail, String> {
    let session = state
        .session_store
        .get_session(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;
    let raw_events = state.hook_server.raw_events_for_session(&session_id);
    let timeline = build_timeline(&session, &raw_events);
    let transcript_path = monitor_transcript_path(&session, &raw_events);

    Ok(MonitorSessionDetail {
        session,
        timeline,
        raw_events,
        transcript_path,
    })
}

#[tauri::command]
pub async fn get_monitor_timeline(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<MonitorTimelineItem>, String> {
    let session = state
        .session_store
        .get_session(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;
    let raw_events = state.hook_server.raw_events_for_session(&session_id);
    Ok(build_timeline(&session, &raw_events))
}

#[tauri::command]
pub async fn get_network_monitor_status(
    state: State<'_, AppState>,
) -> Result<NetworkMonitorStatus, String> {
    Ok(state.network_monitor.status())
}

#[tauri::command]
pub async fn set_network_monitor_enabled(
    state: State<'_, AppState>,
    enabled: bool,
    upstream_base_url: Option<String>,
) -> Result<NetworkMonitorStatus, String> {
    state
        .network_monitor
        .set_enabled(enabled, upstream_base_url)
        .await
}

#[tauri::command]
pub async fn get_network_monitor_requests(
    state: State<'_, AppState>,
) -> Result<Vec<NetworkRequestSummary>, String> {
    Ok(state.network_monitor.requests())
}

#[tauri::command]
pub async fn get_network_monitor_request_detail(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<Option<NetworkRequestDetail>, String> {
    Ok(state.network_monitor.request_detail(&request_id))
}

#[tauri::command]
pub async fn get_claude_wrapper_status() -> Result<ClaudeWrapperStatus, String> {
    Ok(claude_wrapper_status())
}

#[tauri::command]
pub async fn install_claude_wrapper() -> Result<ClaudeWrapperStatus, String> {
    let path = claude_wrapper_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create wrapper dir: {e}"))?;
    }
    fs::write(&path, claude_wrapper_script())
        .map_err(|e| format!("Failed to write Claude wrapper: {e}"))?;
    let mut perms = fs::metadata(&path)
        .map_err(|e| format!("Failed to stat Claude wrapper: {e}"))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms)
        .map_err(|e| format!("Failed to chmod Claude wrapper: {e}"))?;
    ensure_shell_path_hook()?;
    Ok(claude_wrapper_status())
}

#[tauri::command]
pub async fn remove_claude_wrapper() -> Result<ClaudeWrapperStatus, String> {
    let path = claude_wrapper_path()?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to remove Claude wrapper: {e}"))?;
    }
    remove_shell_path_hook()?;
    Ok(claude_wrapper_status())
}

const WRAPPER_PATH_HOOK_START: &str = "# >>> AgentBro Claude Inspector >>>";
const WRAPPER_PATH_HOOK_END: &str = "# <<< AgentBro Claude Inspector <<<";

fn claude_wrapper_status() -> ClaudeWrapperStatus {
    let shim_path = claude_wrapper_path().unwrap_or_else(|_| PathBuf::from(""));
    let shell_config_path = shell_config_path();
    let shell_content = fs::read_to_string(&shell_config_path).unwrap_or_default();
    ClaudeWrapperStatus {
        installed: shim_path.exists(),
        shim_path: shim_path.display().to_string(),
        path_hint_installed: shell_content.contains(WRAPPER_PATH_HOOK_START),
        shell_config_path: shell_config_path.display().to_string(),
    }
}

fn claude_wrapper_path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|home| home.join(".agentbro").join("bin").join("claude"))
        .ok_or_else(|| "Unable to resolve home directory".to_string())
}

fn shell_config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    let shell = std::env::var("SHELL").unwrap_or_default();
    if shell.contains("bash") {
        let bash_profile = home.join(".bash_profile");
        if cfg!(target_os = "macos") && bash_profile.exists() {
            bash_profile
        } else {
            home.join(".bashrc")
        }
    } else {
        home.join(".zshrc")
    }
}

fn ensure_shell_path_hook() -> Result<(), String> {
    let config_path = shell_config_path();
    let mut content = fs::read_to_string(&config_path).unwrap_or_default();
    if content.contains(WRAPPER_PATH_HOOK_START) {
        return Ok(());
    }
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!(
        "\n{WRAPPER_PATH_HOOK_START}\nexport PATH=\"$HOME/.agentbro/bin:$PATH\"\n{WRAPPER_PATH_HOOK_END}\n"
    ));
    fs::write(&config_path, content).map_err(|e| {
        format!(
            "Failed to update shell config {}: {e}",
            config_path.display()
        )
    })
}

fn remove_shell_path_hook() -> Result<(), String> {
    let config_path = shell_config_path();
    let content = match fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(_) => return Ok(()),
    };
    if !content.contains(WRAPPER_PATH_HOOK_START) {
        return Ok(());
    }
    let mut next = String::new();
    let mut skipping = false;
    for line in content.lines() {
        if line == WRAPPER_PATH_HOOK_START {
            skipping = true;
            continue;
        }
        if line == WRAPPER_PATH_HOOK_END {
            skipping = false;
            continue;
        }
        if !skipping {
            next.push_str(line);
            next.push('\n');
        }
    }
    fs::write(&config_path, next).map_err(|e| {
        format!(
            "Failed to update shell config {}: {e}",
            config_path.display()
        )
    })
}

fn claude_wrapper_script() -> &'static str {
    r#"#!/bin/sh
# AgentBro Claude Inspector shim. Generated by AgentBro.

SELF_PATH="$0"
REAL_CLAUDE=""

for candidate in "$HOME/.claude/local/claude" /opt/homebrew/bin/claude /usr/local/bin/claude "$HOME/.local/bin/claude"; do
  if [ -x "$candidate" ] && [ "$candidate" != "$SELF_PATH" ]; then
    REAL_CLAUDE="$candidate"
    break
  fi
done

if [ -z "$REAL_CLAUDE" ]; then
  for candidate in $(which -a claude 2>/dev/null); do
    if [ "$candidate" != "$SELF_PATH" ] && [ "$candidate" != "$HOME/.agentbro/bin/claude" ]; then
      REAL_CLAUDE="$candidate"
      break
    fi
  done
fi

if [ -z "$REAL_CLAUDE" ]; then
  echo "AgentBro: real claude executable not found" >&2
  exit 127
fi

if ! command -v python3 >/dev/null 2>&1; then
  exec "$REAL_CLAUDE" "$@"
fi

AGENTBRO_ROUTE_BASE_URL="$(python3 - "$PWD" <<'PY'
import base64, json, os, sys
from pathlib import Path
from urllib import request as urlrequest

cwd = Path(sys.argv[1])
state_path = Path.home() / ".agentbro" / "network-monitor.json"
default = "https://api.anthropic.com"

def read_json(path):
    try:
        return json.loads(path.read_text())
    except Exception:
        return None

state = read_json(state_path) or {}
proxy = state.get("proxyUrl") if state.get("enabled") else None
if not proxy:
    print("")
    raise SystemExit

try:
    probe = urlrequest.Request(proxy.rstrip("/") + "/__agentbro_health", method="GET")
    urlrequest.urlopen(probe, timeout=0.25).close()
except Exception:
    print("")
    raise SystemExit

def is_agentbro_url(url):
    return isinstance(url, str) and "/__agentbro_route/" in url

def env_base(path):
    data = read_json(path)
    env = data.get("env") if isinstance(data, dict) else None
    value = env.get("ANTHROPIC_BASE_URL") if isinstance(env, dict) else None
    return value if isinstance(value, str) and value.strip() else None

upstream = os.environ.get("ANTHROPIC_BASE_URL")
if not upstream or is_agentbro_url(upstream):
    for candidate in (
        cwd / ".claude" / "settings.local.json",
        cwd / ".claude" / "settings.json",
        Path.home() / ".claude" / "settings.json",
    ):
        upstream = env_base(candidate)
        if upstream and not is_agentbro_url(upstream):
            break
if not upstream or is_agentbro_url(upstream):
    upstream = default

encoded = base64.urlsafe_b64encode(upstream.rstrip("/").encode()).decode().rstrip("=")
route = proxy.rstrip("/") + "/__agentbro_route/" + encoded
print(route)
PY
)"

if [ -n "$AGENTBRO_ROUTE_BASE_URL" ]; then
  ANTHROPIC_BASE_URL="$AGENTBRO_ROUTE_BASE_URL" exec "$REAL_CLAUDE" "$@"
fi

exec "$REAL_CLAUDE" "$@"
"#
}

fn session_summary(session: SessionState) -> MonitorSessionSummary {
    let pending_kind = pending_kind(&session);
    let token_total = session.tokens.input
        + session.tokens.output
        + session.tokens.cache_read
        + session.tokens.cache_create;

    MonitorSessionSummary {
        id: session.id,
        agent_type: session.agent_type,
        engine_label: session.engine_label,
        project: session.project,
        cwd: session.cwd,
        terminal: session.terminal,
        phase: phase_label(&session.phase).to_string(),
        started_at: session.started_at,
        duration: session.duration,
        token_total,
        last_tool_name: session.last_tool_name,
        last_tool_target: session.last_tool_target,
        last_tool_status: session.last_tool_status,
        waiting_user: pending_kind.is_some(),
        pending_kind,
        subagent_count: session.subagents.len(),
        active_tool_count: session
            .active_tools
            .iter()
            .filter(|tool| tool.status == "running")
            .count(),
        title: session.session_title,
    }
}

fn pending_kind(session: &SessionState) -> Option<String> {
    if session.pending_permission.is_some() {
        return Some("permission".to_string());
    }
    if session.pending_question.is_some() {
        return Some("question".to_string());
    }
    if session.pending_plan.is_some() {
        return Some("plan".to_string());
    }
    None
}

fn phase_label(phase: &SessionPhase) -> &'static str {
    match phase {
        SessionPhase::Idle => "idle",
        SessionPhase::Processing => "processing",
        SessionPhase::WaitingApproval => "waiting_approval",
        SessionPhase::WaitingInput => "waiting_input",
        SessionPhase::Compacting => "compacting",
        SessionPhase::Done => "done",
        SessionPhase::Error => "error",
        SessionPhase::Interrupted => "interrupted",
    }
}

fn build_timeline(session: &SessionState, raw_events: &[RawHookEvent]) -> Vec<MonitorTimelineItem> {
    let mut items = Vec::new();

    items.push(MonitorTimelineItem {
        id: format!("session:{}:start", session.id),
        timestamp_ms: seconds_to_ms(session.started_at),
        kind: "session".to_string(),
        title: "Session created".to_string(),
        detail: Some(format!("{} · {}", session.agent_type, session.cwd)),
        status: Some(phase_label(&session.phase).to_string()),
        tool_name: None,
        raw_event_seq: None,
    });

    for tool in &session.active_tools {
        let timestamp_ms = tool
            .completed_at
            .map(seconds_to_ms)
            .unwrap_or_else(|| seconds_to_ms(tool.started_at));
        items.push(MonitorTimelineItem {
            id: format!("tool:{}:{}", session.id, tool.tool_use_id),
            timestamp_ms,
            kind: "tool".to_string(),
            title: tool.tool_name.clone(),
            detail: tool_detail(tool),
            status: Some(tool.status.clone()),
            tool_name: Some(tool.tool_name.clone()),
            raw_event_seq: None,
        });
    }

    if let Some(tool_name) = &session.last_tool_name {
        let already_tracked = session
            .active_tools
            .iter()
            .any(|tool| &tool.tool_name == tool_name);
        if !already_tracked {
            items.push(MonitorTimelineItem {
                id: format!("tool:{}:last", session.id),
                timestamp_ms: seconds_to_ms(session.started_at + session.duration),
                kind: "tool".to_string(),
                title: tool_name.clone(),
                detail: session.last_tool_target.clone(),
                status: session.last_tool_status.clone(),
                tool_name: Some(tool_name.clone()),
                raw_event_seq: None,
            });
        }
    }

    for subagent in &session.subagents {
        items.push(MonitorTimelineItem {
            id: format!("subagent:{}:{}:start", session.id, subagent.agent_id),
            timestamp_ms: seconds_to_ms(subagent.started_at),
            kind: "subagent".to_string(),
            title: subagent
                .agent_type
                .clone()
                .unwrap_or_else(|| "Subagent".to_string()),
            detail: Some(subagent.description.clone()),
            status: Some(subagent.status.clone()),
            tool_name: None,
            raw_event_seq: None,
        });
        if let Some(completed_at) = subagent.completed_at {
            items.push(MonitorTimelineItem {
                id: format!("subagent:{}:{}:stop", session.id, subagent.agent_id),
                timestamp_ms: seconds_to_ms(completed_at),
                kind: "subagent".to_string(),
                title: "Subagent completed".to_string(),
                detail: subagent.last_assistant_message.clone(),
                status: Some(subagent.status.clone()),
                tool_name: None,
                raw_event_seq: None,
            });
        }
    }

    if let Some(permission) = &session.pending_permission {
        items.push(MonitorTimelineItem {
            id: format!("pending:{}:permission", session.id),
            timestamp_ms: seconds_to_ms(session.started_at + session.duration),
            kind: "approval".to_string(),
            title: format!("Permission: {}", permission.tool_name),
            detail: Some(permission.tool_input.clone()),
            status: Some("pending".to_string()),
            tool_name: Some(permission.tool_name.clone()),
            raw_event_seq: None,
        });
    }

    if let Some(question) = &session.pending_question {
        items.push(MonitorTimelineItem {
            id: format!("pending:{}:question", session.id),
            timestamp_ms: seconds_to_ms(session.started_at + session.duration),
            kind: "question".to_string(),
            title: "Question waiting".to_string(),
            detail: Some(question.question.clone()),
            status: Some("pending".to_string()),
            tool_name: None,
            raw_event_seq: None,
        });
    }

    if let Some(plan) = &session.pending_plan {
        items.push(MonitorTimelineItem {
            id: format!("pending:{}:plan", session.id),
            timestamp_ms: seconds_to_ms(session.started_at + session.duration),
            kind: "plan".to_string(),
            title: plan.title.clone(),
            detail: Some(plan.content.clone()),
            status: Some("pending".to_string()),
            tool_name: None,
            raw_event_seq: None,
        });
    }

    for event in raw_events {
        let tool_name = raw_tool_name(&event.raw);
        items.push(MonitorTimelineItem {
            id: format!("raw:{}:{}", session.id, event.seq),
            timestamp_ms: event.timestamp_ms,
            kind: if tool_name.is_some() {
                "hook_tool"
            } else {
                "hook"
            }
            .to_string(),
            title: event.event_name.clone(),
            detail: raw_event_detail(&event.raw),
            status: raw_status(&event.raw),
            tool_name,
            raw_event_seq: Some(event.seq),
        });
    }

    items.sort_by(|a, b| {
        b.timestamp_ms
            .cmp(&a.timestamp_ms)
            .then_with(|| b.id.cmp(&a.id))
    });
    items
}

fn tool_detail(tool: &crate::hooks::session_store::ToolResult) -> Option<String> {
    if let Some(error) = &tool.error {
        return Some(error.clone());
    }
    match tool.completed_at {
        Some(done_at) => Some(format!("{}s", (done_at - tool.started_at).max(0))),
        None => Some("running".to_string()),
    }
}

fn raw_tool_name(raw: &serde_json::Value) -> Option<String> {
    raw.get("tool_name")
        .or_else(|| raw.get("toolName"))
        .or_else(|| raw.get("tool"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn raw_status(raw: &serde_json::Value) -> Option<String> {
    raw.get("status")
        .or_else(|| raw.get("tool_status"))
        .or_else(|| raw.get("toolStatus"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn raw_event_detail(raw: &serde_json::Value) -> Option<String> {
    raw_tool_name(raw)
        .or_else(|| {
            raw.get("message")
                .or_else(|| raw.get("description"))
                .or_else(|| raw.get("prompt"))
                .and_then(|value| value.as_str())
                .map(truncate_detail)
        })
        .or_else(|| raw_status(raw))
}

fn monitor_transcript_path(session: &SessionState, raw_events: &[RawHookEvent]) -> Option<String> {
    raw_events
        .iter()
        .rev()
        .find_map(|event| {
            event
                .raw
                .get("transcript_path")
                .or_else(|| event.raw.get("transcriptPath"))
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .map(ToString::to_string)
        })
        .or_else(|| {
            if session.agent_type == "codex" {
                discover_codex_session_file(&session.id)
            } else {
                discover_session_file(&session.id, &session.cwd)
            }
            .map(|path| path.display().to_string())
        })
}

fn truncate_detail(value: &str) -> String {
    let trimmed = value.trim();
    let mut chars = trimmed.chars();
    let preview: String = chars.by_ref().take(180).collect();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

fn seconds_to_ms(seconds: i64) -> u64 {
    seconds.max(0) as u64 * 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_wrapper_preserves_hooks_by_avoiding_settings_override() {
        let script = claude_wrapper_script();

        assert!(
            !script.contains("--settings"),
            "wrapper must not pass generated --settings because that can bypass Claude Code hooks"
        );
        assert!(script.contains("AGENTBRO_ROUTE_BASE_URL"));
        assert!(script.contains(
            "ANTHROPIC_BASE_URL=\"$AGENTBRO_ROUTE_BASE_URL\" exec \"$REAL_CLAUDE\" \"$@\""
        ));
    }
}
