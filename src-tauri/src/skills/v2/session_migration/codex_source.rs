use super::{in_project_scope, PortableMessage, SessionSummary};
use crate::hooks::conversation_parser::ChatRole;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

pub(super) struct CodexRollout {
    pub summary: SessionSummary,
    pub forked_from: Option<String>,
    path: PathBuf,
}

pub(super) fn catalog(home: &Path, scope: &[PathBuf]) -> Result<Vec<CodexRollout>, String> {
    let configured = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    catalog_at(&configured.unwrap_or_else(|| home.join(".codex")), scope)
}

fn catalog_at(home: &Path, scope: &[PathBuf]) -> Result<Vec<CodexRollout>, String> {
    let titles = local_titles(home);
    let mut sessions = Vec::new();
    let mut seen = HashSet::new();
    for (folder, archived) in [("sessions", false), ("archived_sessions", true)] {
        let mut directories = vec![home.join(folder)];
        while let Some(directory) = directories.pop() {
            let entries = match fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("Could not read Codex history: {error}")),
            };
            for entry in entries.flatten() {
                let Ok(ty) = entry.file_type() else {
                    continue;
                };
                let path = entry.path();
                if ty.is_dir() {
                    directories.push(path);
                    continue;
                }
                if !ty.is_file()
                    || !entry.file_name().to_str().is_some_and(|name| {
                        name.starts_with("rollout-") && name.ends_with(".jsonl")
                    })
                {
                    continue;
                }
                let Ok(mut reader) = fs::File::open(&path).map(BufReader::new) else {
                    continue;
                };
                let Ok(header) = session_header(&mut reader) else {
                    continue;
                };
                let Some(cwd) = header["cwd"].as_str() else {
                    continue;
                };
                let Some(id) = header["id"]
                    .as_str()
                    .and_then(|value| uuid::Uuid::parse_str(value).ok())
                else {
                    continue;
                };
                let id = id.to_string();
                if !in_project_scope(Path::new(cwd), scope) || !seen.insert(id.clone()) {
                    continue;
                }
                let metadata = titles.get(&id);
                let updated_at = metadata.and_then(|details| details.1.clone()).or_else(|| {
                    entry
                        .metadata()
                        .ok()
                        .and_then(|info| info.modified().ok())
                        .map(|modified| {
                            chrono::DateTime::<chrono::Utc>::from(modified).to_rfc3339()
                        })
                });
                sessions.push(CodexRollout {
                    summary: SessionSummary {
                        title: metadata.map_or_else(String::new, |details| details.0.clone()),
                        id,
                        cwd: cwd.to_owned(),
                        archived,
                        updated_at,
                        directory_exists: Path::new(cwd).is_dir(),
                    },
                    forked_from: header["forked_from_id"].as_str().map(str::to_owned),
                    path,
                });
            }
        }
    }
    sessions.sort_by(|left, right| {
        right
            .summary
            .updated_at
            .cmp(&left.summary.updated_at)
            .then_with(|| left.summary.id.cmp(&right.summary.id))
    });
    Ok(sessions)
}

fn local_titles(home: &Path) -> HashMap<String, (String, Option<String>)> {
    let database = fs::read_dir(home)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let version = name
                .to_str()?
                .strip_prefix("state_")?
                .strip_suffix(".sqlite")?
                .parse::<u32>()
                .ok()?;
            entry
                .file_type()
                .ok()?
                .is_file()
                .then_some((version, entry.path()))
        })
        .max_by_key(|(version, _)| *version);
    let Some((_, database)) = database else {
        return HashMap::new();
    };
    let Ok(connection) = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return HashMap::new();
    };
    let Ok(mut query) = connection.prepare("SELECT id, title, updated_at FROM threads") else {
        return HashMap::new();
    };
    let Ok(results) = query.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let updated = row
            .get::<_, Option<i64>>(2)?
            .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
            .map(|date| date.to_rfc3339());
        Ok((id, (title, updated)))
    }) else {
        return HashMap::new();
    };
    results.filter_map(Result::ok).collect()
}

fn session_header(reader: &mut impl BufRead) -> io::Result<Value> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let record: Value = serde_json::from_str(&line)?;
    if record["type"] != "session_meta" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Missing Codex session metadata",
        ));
    }
    Ok(record["payload"].clone())
}

pub(super) fn messages(rollout: &CodexRollout) -> io::Result<(Vec<PortableMessage>, bool)> {
    let mut file = BufReader::new(fs::File::open(&rollout.path)?);
    let header = session_header(&mut file)?;
    if header["id"] != rollout.summary.id || header["cwd"] != rollout.summary.cwd {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The Codex rollout changed",
        ));
    }
    let mut result = Vec::new();
    let mut user_turns = Vec::new();
    let mut partial = false;
    for (index, line) in file.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            partial = true;
            continue;
        };
        let payload = &record["payload"];
        if record["type"] == "event_msg" && payload["type"] == "thread_rolled_back" {
            if let Some(count) = payload["num_turns"].as_u64().filter(|count| *count > 0) {
                let first_removed = user_turns
                    .len()
                    .saturating_sub(usize::try_from(count).unwrap_or(usize::MAX));
                if let Some(boundary) = user_turns.get(first_removed).copied() {
                    result.truncate(boundary);
                    user_turns.truncate(first_removed);
                }
            }
            continue;
        }
        if record["type"] != "response_item" || payload["type"] != "message" {
            continue;
        }
        let role = match payload["role"].as_str() {
            Some("user") => ChatRole::User,
            Some("assistant") => ChatRole::Assistant,
            _ => continue,
        };
        let Some(blocks) = payload["content"].as_array() else {
            partial = true;
            continue;
        };
        let text = blocks
            .iter()
            .filter(|part| {
                matches!(
                    part["type"].as_str(),
                    Some("input_text" | "output_text" | "text")
                )
            })
            .filter_map(|part| part["text"].as_str())
            .filter(|text| !text.trim().is_empty() && !synthetic_message(text))
            .collect::<Vec<_>>()
            .join("\n\n");
        if blocks.iter().any(|part| {
            matches!(
                part["type"].as_str(),
                Some("input_image" | "image_url" | "image" | "input_file")
            )
        }) {
            partial = true;
        }
        if text.is_empty() {
            continue;
        }
        if role == ChatRole::User {
            user_turns.push(result.len());
        }
        result.push(PortableMessage {
            id: format!("rollout-{index}"),
            role,
            text,
            timestamp: record["timestamp"].as_str().map(str::to_owned),
        });
    }
    Ok((result, partial))
}

fn synthetic_message(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with("<environment_context>")
        || (text.starts_with("# AGENTS.md instructions for ") && text.contains("<INSTRUCTIONS>"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::installer::TempInstallDir;
    use serde_json::json;
    use std::io::Write;

    #[test]
    fn archived_codex_rollout_preserves_roles_while_omitting_reverted_turns_and_internal_text() {
        let dir = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let project = dir.path().join("项目");
        let archived = dir.path().join("archived_sessions");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&archived).unwrap();
        let id = uuid::Uuid::new_v4();
        let parent = uuid::Uuid::new_v4();
        let mut out = fs::File::create(archived.join(format!("rollout-{id}.jsonl"))).unwrap();
        writeln!(
            out,
            "{}",
            json!({"type":"session_meta","payload":{"id":id,"cwd":project,"forked_from_id":parent}})
        )
        .unwrap();
        for (role, text) in [
            (
                "user",
                "<environment_context>internal</environment_context>",
            ),
            ("user", "First user message"),
            ("assistant", "First answer"),
            ("user", "Reverted user message"),
            ("assistant", "Reverted answer"),
        ] {
            writeln!(out, "{}", json!({"type":"response_item","payload":{"type":"message","role":role,"content":[{"type":"text","text":text}]}})).unwrap();
        }
        writeln!(
            out,
            "{}",
            json!({"type":"event_msg","payload":{"type":"thread_rolled_back","num_turns":1}})
        )
        .unwrap();
        writeln!(out, "damaged record").unwrap();
        drop(out);
        let scope = vec![project.canonicalize().unwrap()];
        let listing = catalog_at(dir.path(), &scope).unwrap();
        assert!(listing[0].summary.archived);
        assert_eq!(listing[0].forked_from, Some(parent.to_string()));
        let (messages, partial) = messages(&listing[0]).unwrap();
        assert!(partial);
        assert_eq!(
            messages
                .iter()
                .map(|message| (&message.role, message.text.as_str()))
                .collect::<Vec<_>>(),
            [
                (&ChatRole::User, "First user message"),
                (&ChatRole::Assistant, "First answer")
            ]
        );
    }
}
