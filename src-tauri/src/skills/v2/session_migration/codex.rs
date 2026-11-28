use super::{HandoffBundle, MigratedConversation, PortableSession};
use crate::agents::executable;
use crate::hooks::conversation_parser::ChatRole;
use crate::platform::process::background_tokio_command;
use crate::skills::installer::TempInstallDir;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use uuid::Uuid;

static IMPORTS: Mutex<()> = Mutex::const_new(());
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

pub(super) async fn import(bundle: HandoffBundle) -> Result<Vec<MigratedConversation>, String> {
    let sources: Vec<_> = bundle
        .conversations
        .iter()
        .filter(|session| !session.messages.is_empty())
        .collect();
    if sources.is_empty() {
        return Err("There are no readable conversations to migrate.".to_string());
    }
    let _guard = IMPORTS.lock().await;
    let binary = executable::find_codex_cli_binary()
        .ok_or("Codex was not found. You can still save a handoff archive.")?;
    let mut codex = Client::connect(&binary).await?;
    let expected: HashSet<_> = sources
        .iter()
        .map(|record| provenance_id(&record.summary.id, &bundle.target_directory))
        .collect();
    let mut existing = codex
        .imported_threads(&bundle.target_directory, &expected)
        .await?;
    let stage = TempInstallDir::create_in(&std::env::temp_dir())?;
    let mut migrated = Vec::new();
    for session in sources {
        let source_id = provenance_id(&session.summary.id, &bundle.target_directory);
        let (target_id, reused) = if let Some(thread_id) = existing.remove(&source_id) {
            (thread_id, true)
        } else {
            let path = stage.path().join(format!(
                "rollout-{}-{source_id}.jsonl",
                chrono::Local::now().format("%Y-%m-%dT%H-%M-%S")
            ));
            let target = Path::new(&bundle.target_directory);
            write_rollout(&path, source_id, session, target).map_err(|error| error.to_string())?;
            let fork = codex
                .request(
                    "thread/fork",
                    json!({
                        "threadId": source_id,
                        "path": path,
                        "cwd": target,
                        "ephemeral": false,
                        "excludeTurns": true,
                    }),
                )
                .await?;
            let id = thread_id(&fork["thread"])?;
            if !session.summary.title.trim().is_empty() {
                if let Err(error) = codex
                    .request(
                        "thread/name/set",
                        json!({
                            "threadId": id,
                            "name": session.summary.title,
                        }),
                    )
                    .await
                {
                    log::debug!("Could not set the imported Codex thread title: {error}");
                }
            }
            (id, false)
        };
        migrated.push(MigratedConversation {
            source_id: session.summary.id.clone(),
            target_id,
            reused,
        });
    }
    Ok(migrated)
}

fn provenance_id(id: &str, target: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(b"AgentBro portability v1\0zcode\0codex\0");
    hasher.update(id.as_bytes());
    hasher.update(b"\0");
    hasher.update(target.as_bytes());
    let digest = hasher.finalize();
    let mut result = [0_u8; 16];
    result.copy_from_slice(&digest[..16]);
    result[6] = (result[6] & 0x0f) | 0x80;
    result[8] = (result[8] & 0x3f) | 0x80;
    Uuid::from_bytes(result)
}

fn thread_id(thread: &Value) -> Result<String, String> {
    let id = thread["id"]
        .as_str()
        .ok_or("Codex did not return a thread ID.")?;
    Uuid::parse_str(id)
        .map(|uuid| uuid.to_string())
        .map_err(|_| "Codex returned an invalid thread ID.".to_string())
}

fn write_rollout(
    path: &Path,
    source: Uuid,
    session: &PortableSession,
    target: &Path,
) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = BufWriter::new(options.open(path)?);
    let generated_at = chrono::Utc::now().to_rfc3339();
    let first_timestamp = session
        .messages
        .iter()
        .find_map(|message| message.timestamp.as_deref())
        .unwrap_or(&generated_at);
    entry(
        &mut file,
        first_timestamp,
        "session_meta",
        json!({
            "id": source,
            "session_id": source,
            "timestamp": first_timestamp,
            "cwd": target,
            "originator": "AgentBro",
            "cli_version": env!("CARGO_PKG_VERSION"),
            "source": "vscode",
            "history_mode": "legacy",
        }),
    )?;
    let mut turn = None;
    let mut last_answer = None;
    let mut last_timestamp = first_timestamp;
    for message in &session.messages {
        let timestamp = message.timestamp.as_deref().unwrap_or(last_timestamp);
        if message.role == ChatRole::User {
            if let Some(previous) = turn.take() {
                complete_turn(&mut file, last_timestamp, previous, last_answer.take())?;
            }
        }
        if turn.is_none() {
            let id = Uuid::new_v4();
            entry(
                &mut file,
                timestamp,
                "event_msg",
                json!({
                    "type": "task_started",
                    "turn_id": id,
                    "started_at": epoch_seconds(timestamp),
                }),
            )?;
            turn = Some(id);
        }
        let (role, block_type, display) = match message.role {
            ChatRole::User => (
                "user",
                "input_text",
                json!({
                    "type": "user_message", "message": message.text, "images": null,
                    "local_images": [], "text_elements": [],
                }),
            ),
            ChatRole::Assistant => {
                last_answer = Some(message.text.as_str());
                (
                    "assistant",
                    "output_text",
                    json!({
                        "type": "agent_message", "message": message.text, "phase": "final_answer"
                    }),
                )
            }
        };
        // Codex uses response items for model context and events for native turn history.
        entry(
            &mut file,
            timestamp,
            "response_item",
            json!({
                "type": "message", "role": role, "content": [{"type": block_type, "text": message.text}]
            }),
        )?;
        entry(&mut file, timestamp, "event_msg", display)?;
        last_timestamp = timestamp;
    }
    if let Some(turn) = turn {
        complete_turn(&mut file, last_timestamp, turn, last_answer)?;
    }
    file.flush()
}

fn complete_turn(
    file: &mut impl Write,
    timestamp: &str,
    turn: Uuid,
    last_answer: Option<&str>,
) -> io::Result<()> {
    entry(
        file,
        timestamp,
        "event_msg",
        json!({
            "type": "task_complete", "turn_id": turn, "last_agent_message": last_answer,
            "completed_at": epoch_seconds(timestamp),
        }),
    )
}

fn epoch_seconds(timestamp: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|value| value.timestamp())
}

fn entry(
    file: &mut impl Write,
    timestamp: &str,
    item_type: &str,
    payload: Value,
) -> io::Result<()> {
    serde_json::to_writer(
        &mut *file,
        &json!({
            "timestamp": timestamp, "type": item_type, "payload": payload,
        }),
    )?;
    file.write_all(b"\n")
}

struct Client {
    _child: Child,
    input: ChildStdin,
    output: Lines<BufReader<ChildStdout>>,
    seq: u64,
}

impl Client {
    async fn connect(binary: &Path) -> Result<Self, String> {
        let mut command = background_tokio_command(binary);
        command
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if let Some(path) = executable::augmented_path_env() {
            command.env("PATH", path);
        }
        let mut child = command
            .spawn()
            .map_err(|err| format!("Could not start Codex App Server: {err}"))?;
        let input = child.stdin.take().ok_or("Could not open Codex input.")?;
        let output = child.stdout.take().ok_or("Could not open Codex output.")?;
        let mut client = Self {
            _child: child,
            input,
            output: BufReader::new(output).lines(),
            seq: 0,
        };
        client
            .request(
                "initialize",
                json!({
                    "clientInfo": {"name": "AgentBro", "version": env!("CARGO_PKG_VERSION")},
                    "capabilities": {"experimentalApi": true},
                }),
            )
            .await?;
        client.send(json!({"method": "initialized"})).await?;
        Ok(client)
    }

    async fn send(&mut self, message: Value) -> Result<(), String> {
        let mut line = serde_json::to_vec(&message).map_err(|err| err.to_string())?;
        line.push(b'\n');
        self.input
            .write_all(&line)
            .await
            .map_err(|err| err.to_string())
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.seq += 1;
        let id = self.seq;
        tokio::time::timeout(REQUEST_TIMEOUT, async {
            self.send(json!({"id": id, "method": method, "params": params}))
                .await?;
            while let Some(line) = self
                .output
                .next_line()
                .await
                .map_err(|err| err.to_string())?
            {
                let Ok(message) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if message["id"].as_u64() != Some(id) {
                    continue;
                }
                if let Some(error) = message["error"]["message"].as_str() {
                    return Err(format!("Codex {method}: {error}"));
                }
                return message
                    .get("result")
                    .cloned()
                    .ok_or_else(|| format!("Codex returned no result for {method}."));
            }
            Err("Codex App Server stopped responding.".to_string())
        })
        .await
        .map_err(|_| format!("Codex {method} timed out."))?
    }

    async fn imported_threads(
        &mut self,
        directory: &str,
        sources: &HashSet<Uuid>,
    ) -> Result<HashMap<Uuid, String>, String> {
        let mut imports = HashMap::new();
        for archived in [false, true] {
            let mut cursor = None;
            loop {
                let listing = self
                    .request(
                        "thread/list",
                        json!({
                            "cwd": directory, "archived": archived, "limit": 100, "cursor": cursor,
                        }),
                    )
                    .await?;
                let rows = listing["data"]
                    .as_array()
                    .ok_or("Codex returned an invalid thread list.")?;
                for row in rows {
                    let id = thread_id(row)?;
                    let native = self.request("thread/read", json!({"threadId": id})).await?;
                    let matches = native["thread"]["forkedFromId"]
                        .as_str()
                        .and_then(|value| Uuid::parse_str(value).ok())
                        .filter(|value| sources.contains(value));
                    if let Some(source) = matches {
                        imports.entry(source).or_insert(id);
                    }
                }
                if imports.len() == sources.len() {
                    return Ok(imports);
                }
                let next = listing["nextCursor"].as_str().map(str::to_owned);
                if next.is_none() {
                    break;
                }
                if next == cursor {
                    return Err("Codex returned a repeated history cursor.".to_string());
                }
                cursor = next;
            }
        }
        Ok(imports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::v2::session_migration::{PortableMessage, SessionSummary};

    #[test]
    fn writes_native_model_context_and_display_history_for_every_reviewed_message() {
        let dir = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = dir.path().join("preview.jsonl");
        let target = dir.path().join("target");
        let source = provenance_id("sess_portable", target.to_str().unwrap());
        assert_eq!(
            source,
            provenance_id("sess_portable", target.to_str().unwrap())
        );
        assert_ne!(
            source,
            provenance_id("sess_portable", dir.path().to_str().unwrap())
        );

        let session = PortableSession {
            summary: SessionSummary {
                id: "sess_portable".to_string(),
                title: "Work in progress".to_string(),
                cwd: dir.path().display().to_string(),
                archived: false,
                updated_at: None,
                directory_exists: true,
            },
            messages: [
                ChatRole::User,
                ChatRole::Assistant,
                ChatRole::User,
                ChatRole::Assistant,
            ]
            .into_iter()
            .enumerate()
            .map(|(index, role)| PortableMessage {
                id: format!("message-{index}"),
                role,
                timestamp: Some("2026-11-28T08:00:00+00:00".to_string()),
                text: format!("内容 {index}"),
            })
            .collect(),
            read_issue: None,
            git: None,
        };
        write_rollout(&path, source, &session, &target).unwrap();
        let stored = fs::read_to_string(path).unwrap();
        let records: Vec<Value> = stored
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(records[0]["payload"]["id"], source.to_string());
        assert_eq!(records[0]["payload"]["cwd"], target.to_str().unwrap());
        let model: Vec<_> = records
            .iter()
            .filter(|row| row["type"] == "response_item")
            .map(|item| item["payload"]["role"].as_str().unwrap())
            .collect();
        assert_eq!(model, ["user", "assistant", "user", "assistant"]);
        let event_types: Vec<_> = records
            .iter()
            .filter(|row| row["type"] == "event_msg")
            .map(|item| item["payload"]["type"].as_str().unwrap())
            .collect();
        assert_eq!(
            event_types,
            [
                "task_started",
                "user_message",
                "agent_message",
                "task_complete",
                "task_started",
                "user_message",
                "agent_message",
                "task_complete"
            ]
        );
        assert_eq!(stored.matches("内容").count(), 10);
    }
}
