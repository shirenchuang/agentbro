use super::{in_project_scope, portable_messages, PortableMessage, SessionSummary};
use crate::hooks::conversation_parser::ConversationParser;
use reqwest::Url;
use rusqlite::{Connection, OpenFlags};
use std::collections::HashSet;
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(super) struct AntigravityConversation {
    pub summary: SessionSummary,
    logs: PathBuf,
    step_count: u64,
}

pub(super) fn catalog(
    home: &Path,
    scope: Option<&[PathBuf]>,
) -> Result<Vec<AntigravityConversation>, String> {
    let desktop = home.join(".gemini/antigravity");
    let database = desktop.join("conversation_summaries.db");
    if !database.try_exists().map_err(|err| err.to_string())? {
        return Ok(Vec::new());
    }
    read_catalog(&desktop, &database, scope)
        .map_err(|err| format!("Could not read Antigravity conversations: {err}"))
}

fn read_catalog(
    desktop: &Path,
    database: &Path,
    scope: Option<&[PathBuf]>,
) -> rusqlite::Result<Vec<AntigravityConversation>> {
    let conn = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let has_index: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'conversations_fts')",
        [],
        |row| row.get(0),
    )?;
    let mut archived = HashSet::new();
    if has_index {
        let mut index = conn.prepare(
            "SELECT conversation_id FROM conversations_fts
             WHERE chunk_index = -1 AND CAST(archived AS INTEGER) = 1",
        )?;
        for id in index.query_map([], |row| row.get::<_, String>(0))? {
            archived.insert(id?);
        }
    }
    let mut summaries = conn.prepare(
        "SELECT conversation_id, title, workspace_uris, last_modified_time, step_count
         FROM conversation_summaries WHERE COALESCE(parent_conversation_id, '') = ''
         ORDER BY last_modified_time DESC, conversation_id",
    )?;
    let mut rows = summaries.query([])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        let raw_id: String = row.get(0)?;
        let Ok(id) = Uuid::parse_str(&raw_id) else {
            continue;
        };
        let workspaces: String = row.get(2)?;
        let Some(cwd) = workspace_paths(&workspaces)
            .into_iter()
            .find(|path| scope.is_none_or(|roots| in_project_scope(path, roots)))
        else {
            continue;
        };
        let updated: String = row.get(3)?;
        let updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
            .ok()
            .map(|date| date.to_utc().to_rfc3339());
        let step_count: u64 = row.get(4)?;
        let id = id.to_string();
        let logs = desktop
            .join("brain")
            .join(&id)
            .join(".system_generated/logs");
        result.push(AntigravityConversation {
            summary: SessionSummary {
                id,
                title: row.get(1)?,
                cwd: cwd.to_string_lossy().into_owned(),
                archived: archived.contains(&raw_id),
                updated_at,
                directory_exists: cwd.is_dir(),
            },
            logs,
            step_count,
        });
    }
    Ok(result)
}

fn workspace_paths(raw: &str) -> Vec<PathBuf> {
    let raw = raw.trim();
    let paths = serde_json::from_str::<Vec<String>>(raw).unwrap_or_else(|_| vec![raw.to_owned()]);
    paths
        .into_iter()
        .filter_map(|item| {
            if let Ok(uri) = Url::parse(&item) {
                if uri.scheme() == "file" {
                    return uri.to_file_path().ok();
                }
            }
            let path = PathBuf::from(item);
            path.is_absolute().then_some(path)
        })
        .collect()
}

pub(super) fn messages(
    conversation: &AntigravityConversation,
) -> io::Result<(Vec<PortableMessage>, bool)> {
    let transcript = ["transcript_full.jsonl", "transcript.jsonl"]
        .into_iter()
        .map(|name| conversation.logs.join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Antigravity transcript is missing")
        })?;
    let mut records = Vec::new();
    let mut observed = 0;
    let mut partial = false;
    for line in BufReader::new(fs::File::open(transcript)?).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<serde_json::Value>(&line) else {
            partial = true;
            continue;
        };
        let Some(step) = record["step_index"].as_u64() else {
            partial = true;
            continue;
        };
        observed = observed.max(step.saturating_add(1));
        if let Some(kind) = record["type"].as_str() {
            if let Some(message) = ConversationParser::parse_antigravity_message(kind, &record) {
                records.push((step, message));
            }
        }
    }
    records.sort_by_key(|(step, _)| *step);
    records.dedup_by_key(|(step, _)| *step);
    Ok((
        portable_messages(records.into_iter().map(|(_, message)| message).collect()),
        partial || observed < conversation.step_count,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hooks::conversation_parser::ChatRole, skills::installer::TempInstallDir};
    use serde_json::json;

    #[test]
    fn reads_scoped_desktop_history_and_archived_metadata_without_writing() {
        let scratch = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let home = scratch.path();
        let desktop = home.join(".gemini/antigravity");
        let project = home.join("Target 项目");
        let removed = project.join("removed");
        fs::create_dir_all(&desktop).unwrap();
        fs::create_dir_all(&project).unwrap();
        let project_uri = Url::from_directory_path(&project).unwrap();
        let missing_uri = Url::from_directory_path(&removed).unwrap();
        let db = desktop.join("conversation_summaries.db");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE conversation_summaries (conversation_id, title, workspace_uris,
               last_modified_time, step_count, parent_conversation_id);
             CREATE VIRTUAL TABLE conversations_fts USING fts5(conversation_id,
               chunk_index UNINDEXED, archived UNINDEXED);",
        )
        .unwrap();
        let primary = Uuid::new_v4().to_string();
        let archived = Uuid::new_v4().to_string();
        for (id, title, workspaces, parent) in [
            (
                &primary,
                "Main conversation",
                json!(["ssh://example/project", project_uri.as_str()]),
                "",
            ),
            (
                &archived,
                "Archived conversation",
                json!([missing_uri.as_str()]),
                "",
            ),
            (
                &Uuid::new_v4().to_string(),
                "Subagent",
                json!([project_uri.as_str()]),
                primary.as_str(),
            ),
            (
                &Uuid::new_v4().to_string(),
                "Outside project",
                json!([Url::from_directory_path(home).unwrap().as_str()]),
                "",
            ),
        ] {
            conn.execute(
                "INSERT INTO conversation_summaries VALUES (?1, ?2, ?3, ?4, 4, ?5)",
                rusqlite::params![
                    id,
                    title,
                    workspaces.to_string(),
                    "2026-11-26 08:08:20.269555+00:00",
                    parent
                ],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO conversations_fts VALUES (?1, -1, 1)",
            [&archived],
        )
        .unwrap();
        drop(conn);
        let before = fs::read(&db).unwrap();
        let list = catalog(home, Some(&[project.canonicalize().unwrap()])).unwrap();
        assert_eq!(list.len(), 2);
        let main = list.iter().find(|row| row.summary.id == primary).unwrap();
        let old = list.iter().find(|row| row.summary.id == archived).unwrap();
        assert_eq!(main.summary.title, "Main conversation");
        assert_eq!(
            main.summary.updated_at.as_deref(),
            Some("2026-11-26T08:08:20.269555+00:00")
        );
        assert!(old.summary.archived);
        assert!(!old.summary.directory_exists);
        assert!(messages(old).is_err());

        fs::create_dir_all(&main.logs).unwrap();
        let transcript = main.logs.join("transcript_full.jsonl");
        let mut contents = String::new();
        for (step, kind, source, text) in [
            (2, "PLANNER_RESPONSE", "MODEL", "Previous answer"),
            (0, "USER_INPUT", "USER_EXPLICIT", "Previous question"),
            (1, "RUN_COMMAND", "MODEL", "tool details"),
        ] {
            contents.push_str(
                &json!({"step_index":step,"type":kind,"source":source,"content":text}).to_string(),
            );
            contents.push('\n');
        }
        fs::write(&transcript, &contents).unwrap();
        let (history, partial) = messages(main).unwrap();
        assert!(partial);
        assert_eq!(history.len(), 2);
        assert!(history[0].role == ChatRole::User);
        assert!(history[1].role == ChatRole::Assistant);
        assert_eq!(history[0].text, "Previous question");
        assert_eq!(history[1].text, "Previous answer");
        assert_eq!(fs::read(&db).unwrap(), before);
        assert_eq!(fs::read_to_string(transcript).unwrap(), contents);
    }
}
