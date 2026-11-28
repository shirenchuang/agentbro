use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tauri::{AppHandle, Emitter};

use super::conversation_parser::{
    ChatRole, ConversationParser, IncrementalParseResult, MessageBlock, ParsedMessage,
};
use super::file_watcher::{ConversationUpdatePayload, CONVERSATION_UPDATE_EVENT};
use super::session_store::{SessionPhase, SessionStore};

const ACTIVE_WINDOW_SECS: i64 = 4 * 60 * 60;
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
struct ClaudeDesktopSessionMetadata {
    local_session_id: String,
    cli_session_id: String,
    cwd: String,
    title: Option<String>,
    is_archived: bool,
    created_at: i64,
    last_activity_at: i64,
}

#[derive(Debug, Clone)]
struct WatchedDesktopSession {
    metadata_path: PathBuf,
    audit_path: PathBuf,
    cli_session_id: String,
    file_size: u64,
    result_scan_offset: u64,
}

#[derive(Default)]
struct WatchState {
    known_local_session_ids: HashSet<String>,
    sessions: HashMap<String, WatchedDesktopSession>,
    parsers: HashMap<String, ConversationParser>,
}

pub fn start(session_store: Arc<SessionStore>, app_handle: AppHandle) {
    let Some(root) = sessions_root() else {
        log::info!("Claude Desktop watcher disabled: home directory unavailable");
        return;
    };

    tauri::async_runtime::spawn(async move {
        run_loop(root, session_store, app_handle).await;
    });
}

pub fn find_audit_file_for_cli_session(session_id: &str) -> Option<PathBuf> {
    find_session_audit(session_id, metadata_files(&sessions_root()?))
}

fn find_session_audit(
    session_id: &str,
    metadata_paths: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    metadata_paths.into_iter().find_map(|metadata_path| {
        let metadata = read_metadata(&metadata_path)?;
        if metadata.cli_session_id != session_id {
            return None;
        }
        let audit_path = audit_path_for_metadata(&metadata_path)?;
        audit_path.is_file().then_some(audit_path)
    })
}

async fn run_loop(root: PathBuf, session_store: Arc<SessionStore>, app_handle: AppHandle) {
    let mut state = WatchState::default();
    loop {
        scan_for_sessions(&root, &mut state, &session_store);
        poll_sessions(&mut state, &session_store, &app_handle);
        tokio::time::sleep(DISCOVERY_INTERVAL).await;
    }
}

fn scan_for_sessions(root: &Path, state: &mut WatchState, session_store: &SessionStore) {
    if !root.is_dir() {
        return;
    }

    for metadata_path in metadata_files(root) {
        let Some(local_session_id) = metadata_path.file_stem().and_then(|value| value.to_str())
        else {
            continue;
        };
        if state.known_local_session_ids.contains(local_session_id) {
            continue;
        }

        let Some(metadata) = read_metadata(&metadata_path) else {
            continue;
        };
        if metadata.is_archived || is_stale(&metadata) {
            state
                .known_local_session_ids
                .insert(metadata.local_session_id);
            continue;
        }

        let Some(audit_path) = audit_path_for_metadata(&metadata_path) else {
            continue;
        };
        if !audit_path.is_file() {
            continue;
        }

        register_session(state, session_store, metadata_path, audit_path, metadata);
    }
}

fn register_session(
    state: &mut WatchState,
    session_store: &SessionStore,
    metadata_path: PathBuf,
    audit_path: PathBuf,
    metadata: ClaudeDesktopSessionMetadata,
) {
    let mut parser = ConversationParser::new(audit_path.clone());
    let initial = parser.parse_incremental().ok();
    let file_size = initial
        .as_ref()
        .map(|result| result.byte_offset)
        .unwrap_or_else(|| file_size(&audit_path));
    let project = project_name(&metadata.cwd);

    session_store.get_or_create_session(
        &metadata.cli_session_id,
        "claude-code",
        &project,
        &metadata.cwd,
        "Claude Desktop",
    );
    session_store.update_session(&metadata.cli_session_id, |session| {
        session.engine_label = Some("Claude Desktop".to_string());
        session.term_bundle_id = Some("com.anthropic.claudefordesktop".to_string());
        session.session_title = metadata.title.clone();
        session.started_at = metadata.created_at;
        session.duration = Utc::now().timestamp().saturating_sub(metadata.created_at);
    });

    state
        .known_local_session_ids
        .insert(metadata.local_session_id.clone());
    state
        .parsers
        .insert(metadata.local_session_id.clone(), parser);
    state.sessions.insert(
        metadata.local_session_id,
        WatchedDesktopSession {
            metadata_path,
            audit_path,
            cli_session_id: metadata.cli_session_id,
            file_size,
            result_scan_offset: file_size,
        },
    );
}

fn poll_sessions(state: &mut WatchState, session_store: &SessionStore, app_handle: &AppHandle) {
    let keys = state.sessions.keys().cloned().collect::<Vec<_>>();
    let mut finished = Vec::new();

    for local_session_id in keys {
        let Some(session) = state.sessions.get_mut(&local_session_id) else {
            continue;
        };

        if read_metadata(&session.metadata_path).is_some_and(|metadata| metadata.is_archived) {
            session_store.update_session(&session.cli_session_id, |stored| {
                stored.phase = SessionPhase::Done;
                stored.last_response = Some("Claude Desktop session archived".to_string());
            });
            finished.push(local_session_id.clone());
            continue;
        }

        let current_size = file_size(&session.audit_path);
        if current_size == session.file_size {
            continue;
        }

        if current_size < session.file_size {
            if let Some(parser) = state.parsers.get_mut(&local_session_id) {
                parser.reset();
            }
            session.result_scan_offset = 0;
        }

        let completed = scan_for_result_entry(
            &session.audit_path,
            session.result_scan_offset,
            current_size,
        );
        session.result_scan_offset = current_size;

        let parsed = state
            .parsers
            .entry(local_session_id.clone())
            .or_insert_with(|| ConversationParser::new(session.audit_path.clone()))
            .parse_incremental()
            .ok();
        session.file_size = current_size;

        if let Some(result) = parsed {
            apply_parse_result(session_store, &session.cli_session_id, &result);
            emit_conversation_update(app_handle, &session.cli_session_id, result);
        }

        if let Some(result) = completed {
            apply_result_entry(session_store, &session.cli_session_id, result);
        }
    }

    for local_session_id in finished {
        state.sessions.remove(&local_session_id);
        state.parsers.remove(&local_session_id);
    }
}

fn apply_parse_result(
    session_store: &SessionStore,
    session_id: &str,
    result: &IncrementalParseResult,
) {
    if result.clear_detected {
        session_store.update_session(session_id, |session| {
            session.last_response = None;
            session.last_user_message = None;
        });
    }

    if result.new_messages.is_empty() {
        return;
    }

    let latest_user = latest_text_for_role(&result.new_messages, ChatRole::User);
    session_store.update_session(session_id, |session| {
        session.phase = SessionPhase::Processing;
        if let Some(text) = latest_user {
            session.last_user_message = Some(text);
        }
    });
}

fn emit_conversation_update(
    app_handle: &AppHandle,
    session_id: &str,
    result: IncrementalParseResult,
) {
    if result.new_messages.is_empty() && !result.clear_detected {
        return;
    }

    let payload = ConversationUpdatePayload {
        session_id: session_id.to_string(),
        result,
    };
    if let Err(err) = app_handle.emit(CONVERSATION_UPDATE_EVENT, &payload) {
        log::debug!(
            "Failed to emit Claude Desktop conversation update for {}: {}",
            session_id,
            err
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResultEntry {
    is_error: bool,
    num_turns: i64,
    result: Option<String>,
}

fn apply_result_entry(session_store: &SessionStore, session_id: &str, result: ResultEntry) {
    session_store.update_session(session_id, |session| {
        session.phase = if result.is_error {
            SessionPhase::Error
        } else {
            SessionPhase::Done
        };
        session.last_response = result
            .result
            .filter(|text| !text.trim().is_empty())
            .or_else(|| {
                Some(format!(
                    "Claude Desktop turn completed ({} turn(s))",
                    result.num_turns
                ))
            });
    });
}

fn latest_text_for_role(messages: &[ParsedMessage], role: ChatRole) -> Option<String> {
    messages.iter().rev().find_map(|message| {
        if message.role != role {
            return None;
        }
        message.blocks.iter().find_map(|block| match block {
            MessageBlock::Text { text } => {
                let trimmed = text.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }
            _ => None,
        })
    })
}

fn scan_for_result_entry(path: &Path, from: u64, to: u64) -> Option<ResultEntry> {
    if to <= from {
        return None;
    }

    let mut file = File::open(path).ok()?;
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut data = vec![0; (to - from) as usize];
    file.read_exact(&mut data).ok()?;
    let text = String::from_utf8_lossy(&data);

    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        if value.get("type").and_then(|value| value.as_str()) != Some("result") {
            continue;
        }
        return Some(ResultEntry {
            is_error: value
                .get("is_error")
                .or_else(|| value.get("isError"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            num_turns: value
                .get("num_turns")
                .or_else(|| value.get("numTurns"))
                .and_then(|value| value.as_i64())
                .unwrap_or(1),
            result: value
                .get("result")
                .or_else(|| value.get("message"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string),
        });
    }

    None
}

fn sessions_root() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join("Library")
            .join("Application Support")
            .join("Claude")
            .join("local-agent-mode-sessions")
    })
}

fn metadata_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(org_dirs) = fs::read_dir(root) else {
        return files;
    };

    for org_dir in org_dirs
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
    {
        let Ok(user_dirs) = fs::read_dir(org_dir) else {
            continue;
        };
        for user_dir in user_dirs
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
        {
            let Ok(entries) = fs::read_dir(user_dir) else {
                continue;
            };
            for path in entries.flatten().map(|entry| entry.path()) {
                let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                if name.starts_with("local_") && name.ends_with(".json") {
                    files.push(path);
                }
            }
        }
    }

    files
}

fn read_metadata(path: &Path) -> Option<ClaudeDesktopSessionMetadata> {
    let data = fs::read_to_string(path).ok()?;
    parse_metadata_json(&data)
}

fn parse_metadata_json(data: &str) -> Option<ClaudeDesktopSessionMetadata> {
    let json: serde_json::Value = serde_json::from_str(data).ok()?;
    let cli_session_id = json.get("cliSessionId")?.as_str()?.to_string();
    let local_session_id = json.get("sessionId")?.as_str()?.to_string();
    let cwd = json
        .get("cwd")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .or_else(|| dirs::home_dir().map(|path| path.display().to_string()))
        .unwrap_or_default();
    let created_at = millis_to_seconds(json.get("createdAt").and_then(|value| value.as_i64()));
    let last_activity_at = millis_to_seconds(
        json.get("lastActivityAt")
            .and_then(|value| value.as_i64())
            .or_else(|| json.get("createdAt").and_then(|value| value.as_i64())),
    );

    Some(ClaudeDesktopSessionMetadata {
        local_session_id,
        cli_session_id,
        cwd,
        title: json
            .get("title")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        is_archived: json
            .get("isArchived")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        created_at,
        last_activity_at,
    })
}

fn millis_to_seconds(value: Option<i64>) -> i64 {
    value
        .filter(|value| *value > 0)
        .map(|value| value / 1000)
        .unwrap_or_else(|| Utc::now().timestamp())
}

fn audit_path_for_metadata(metadata_path: &Path) -> Option<PathBuf> {
    let parent = metadata_path.parent()?;
    let stem = metadata_path.file_stem()?;
    Some(parent.join(stem).join("audit.jsonl"))
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn project_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("Claude Desktop")
        .to_string()
}

fn is_stale(metadata: &ClaudeDesktopSessionMetadata) -> bool {
    Utc::now()
        .timestamp()
        .saturating_sub(metadata.last_activity_at)
        > ACTIVE_WINDOW_SECS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_desktop_metadata() {
        let metadata = parse_metadata_json(
            r#"{
              "sessionId": "local_abc",
              "cliSessionId": "cli-123",
              "cwd": "/tmp/agentbro",
              "title": "Ship watcher",
              "isArchived": false,
              "createdAt": 10000,
              "lastActivityAt": 12000
            }"#,
        )
        .expect("metadata");

        assert_eq!(metadata.local_session_id, "local_abc");
        assert_eq!(metadata.cli_session_id, "cli-123");
        assert_eq!(metadata.cwd, "/tmp/agentbro");
        assert_eq!(metadata.title.as_deref(), Some("Ship watcher"));
        assert_eq!(metadata.created_at, 10);
        assert_eq!(metadata.last_activity_at, 12);
    }

    #[test]
    fn scans_result_entry() {
        let path = std::env::temp_dir().join(format!(
            "agentbro-claude-desktop-result-{}.jsonl",
            uuid::Uuid::new_v4()
        ));
        fs::write(
            &path,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}
{"type":"result","is_error":false,"num_turns":3,"result":"Done"}
"#,
        )
        .expect("write fixture");

        let size = file_size(&path);
        let result = scan_for_result_entry(&path, 0, size).expect("result entry");
        let _ = fs::remove_file(&path);

        assert!(!result.is_error);
        assert_eq!(result.num_turns, 3);
        assert_eq!(result.result.as_deref(), Some("Done"));
    }

    #[test]
    fn finds_claude_desktop_audit_after_unreadable_metadata_and_missing_audit() {
        let root =
            std::env::temp_dir().join(format!("agentbro-claude-audit-{}", uuid::Uuid::new_v4()));
        let invalid = root.join("local_invalid.json");
        let missing_audit = root.join("local_missing.json");
        let valid = root.join("local_valid.json");
        let audit = audit_path_for_metadata(&valid).unwrap();
        fs::create_dir_all(audit.parent().unwrap()).unwrap();
        fs::write(&invalid, "{").unwrap();
        fs::write(
            &missing_audit,
            r#"{"sessionId":"local_missing","cliSessionId":"session-1"}"#,
        )
        .unwrap();
        fs::write(
            &valid,
            r#"{"sessionId":"local_valid","cliSessionId":"session-1"}"#,
        )
        .unwrap();
        fs::write(&audit, "").unwrap();
        assert_eq!(
            find_session_audit("session-1", [invalid, missing_audit, valid]),
            Some(audit)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn metadata_path_resolves_audit_path() {
        let path = PathBuf::from("/tmp/root/org/user/local_abc.json");
        assert_eq!(
            audit_path_for_metadata(&path),
            Some(PathBuf::from("/tmp/root/org/user/local_abc/audit.jsonl"))
        );
    }
}
