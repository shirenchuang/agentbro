use super::{
    codex, portable_messages, HandoffBundle, MigratedConversation, PortableMessage,
    PortableSession, ReturnContext,
};
use crate::agents::programs::installed_app_path_for_agent;
use crate::hooks::conversation_parser::ChatRole;
use crate::hooks::zcode_history;
use crate::platform::process::background_tokio_command;
use crate::skills::installer::TempInstallDir;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::Mutex;

static IMPORTS: Mutex<()> = Mutex::const_new(());
const RESTORE_SCRIPT: &str = "glm/packages/restore-legacy-sessions-plugin/skills/restore-legacy-sessions/scripts/restore-conversation.mjs";
const RESTORE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
struct Stores {
    cli: PathBuf,
    tasks: PathBuf,
}

impl Stores {
    fn live(home: &Path) -> Self {
        Self {
            cli: zcode_history::database_path(home),
            tasks: home.join(".zcode/v2/tasks-index.sqlite"),
        }
    }

    fn stage(path: &Path) -> Self {
        Self {
            cli: path.join("history.sqlite"),
            tasks: path.join("index.sqlite"),
        }
    }
}

struct RestoreTool {
    executable: PathBuf,
    script: PathBuf,
}

impl RestoreTool {
    fn installed() -> Result<Self, String> {
        let application = installed_app_path_for_agent("zcode")
            .map(PathBuf::from)
            .ok_or("ZCode Desktop was not found. You can still save a handoff archive.")?;
        let (executable, resources) = if cfg!(target_os = "macos") {
            (
                application.join("Contents/MacOS/ZCode"),
                application.join("Contents/Resources"),
            )
        } else if cfg!(target_os = "windows") {
            let resources = application
                .parent()
                .ok_or("ZCode's installation directory was not found.")?
                .join("resources");
            (application, resources)
        } else {
            return Err("Native ZCode migration is available on macOS and Windows.".to_string());
        };
        let script = resources.join(RESTORE_SCRIPT);
        if !executable.is_file() || !script.is_file() {
            return Err("The installed ZCode does not contain the conversation restore tool. You can still save a handoff archive.".into());
        }
        Ok(Self { executable, script })
    }

    async fn restore(&self, snapshot: &Path, stage: &Stores) -> Result<(), String> {
        let mut child = background_tokio_command(&self.executable);
        child
            .env("ELECTRON_RUN_AS_NODE", "1")
            .env("ELECTRON_NO_ASAR", "1")
            .env_remove("NODE_OPTIONS")
            .env_remove("NODE_PATH")
            .kill_on_drop(true)
            .arg(&self.script)
            .arg("--snapshot")
            .arg(snapshot)
            .arg("--cli-db")
            .arg(&stage.cli)
            .arg("--task-index")
            .arg(&stage.tasks);
        let output = tokio::time::timeout(RESTORE_TIMEOUT, child.output())
            .await
            .map_err(|_| "ZCode's conversation restore tool timed out.".to_string())?
            .map_err(|error| format!("Could not run ZCode's conversation restore tool: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "ZCode's conversation restore tool could not prepare the history ({})",
                output.status
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
struct ImportTarget {
    id: String,
    trace: String,
    directory: String,
}

fn target_for(session_id: &str, directory: &str) -> ImportTarget {
    let mut digest = Sha256::new();
    digest.update(b"AgentBro portability v1\0codex\0zcode\0");
    digest.update(session_id);
    digest.update(b"\0");
    digest.update(directory);
    let hash = format!("{:x}", digest.finalize());
    let id = format!("sess_agentbro_{}", &hash[..32]);
    ImportTarget {
        trace: format!("agentbro:codex:{hash}"),
        id,
        directory: directory.to_owned(),
    }
}

pub(super) fn return_context(
    home: &Path,
    parent: &str,
    codex_directory: &str,
    messages: &[PortableMessage],
) -> Option<ReturnContext> {
    let parent = uuid::Uuid::parse_str(parent).ok()?;
    let canonical = super::project_path(Path::new(codex_directory))?;
    let directory = canonical.to_str()?;
    let database = zcode_history::open_database(&zcode_history::database_path(home)).ok()?;
    let mut statement = database
        .prepare("SELECT id FROM session WHERE parent_id IS NULL AND time_deleted IS NULL")
        .ok()?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .ok()?;
    let original_session_id = ids
        .filter_map(Result::ok)
        .find(|id| codex::provenance_id(id, directory) == parent)?;
    let original =
        portable_messages(zcode_history::query_messages(&database, &original_session_id).ok()?);
    let matching_messages = original
        .iter()
        .zip(messages)
        .take_while(|(left, right)| left.role == right.role && left.text == right.text)
        .count();
    (matching_messages > 0).then_some(ReturnContext {
        original_session_id,
        matching_messages,
    })
}

pub(super) async fn import(
    home: PathBuf,
    bundle: HandoffBundle,
) -> Result<Vec<MigratedConversation>, String> {
    let restore = RestoreTool::installed()?;
    let live = Stores::live(&home);
    let _guard = IMPORTS.lock().await;
    let stage_dir = TempInstallDir::create_in(&std::env::temp_dir())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(stage_dir.path(), fs::Permissions::from_mode(0o700))
            .map_err(|err| err.to_string())?;
    }
    let mut result = Vec::new();
    for session in bundle
        .conversations
        .iter()
        .filter(|session| !session.messages.is_empty())
    {
        let target = target_for(&session.summary.id, &bundle.target_directory);
        let stage = stage_dir.path().join(&target.id);
        let (live_paths, pending, stage_path) = (live.clone(), target.clone(), stage.clone());
        let payload = snapshot(session, &target);
        let expected = session.messages.len();
        let needs_restore = tokio::task::spawn_blocking(move || {
            prepare(&live_paths, &pending, &stage_path, payload).map_err(|err| err.to_string())
        })
        .await
        .map_err(|err| err.to_string())??;
        if needs_restore {
            let temp = Stores::stage(&stage);
            restore
                .restore(&stage.join("conversation.json"), &temp)
                .await?;
            let (live_paths, pending) = (live.clone(), target.clone());
            tokio::task::spawn_blocking(move || apply(&live_paths, &temp, &pending, expected))
                .await
                .map_err(|err| err.to_string())??;
        }
        result.push(MigratedConversation {
            source_id: session.summary.id.clone(),
            target_id: target.id,
            reused: !needs_restore,
        });
    }
    if result.is_empty() {
        return Err("There are no readable conversations to migrate.".to_string());
    }
    Ok(result)
}

fn snapshot(source: &PortableSession, target: &ImportTarget) -> Value {
    let default_time = chrono::Utc::now().timestamp_millis();
    let timestamp = |message: &PortableMessage| {
        message
            .timestamp
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|date| date.timestamp_millis())
            .unwrap_or(default_time)
    };
    let messages: Vec<_> = source
        .messages
        .iter()
        .map(|message| {
            let role = match message.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
            };
            json!({"role": role, "content": message.text, "timestamp": timestamp(message)})
        })
        .collect();
    json!({
        "meta": {
            "taskId": target.id,
            "title": source.summary.title,
            "workspacePath": target.directory,
            "traceId": target.trace,
            "provider": "codex",
            "status": "completed",
            "createdAt": source.messages.first().map(timestamp).unwrap_or(default_time),
            "updatedAt": source.messages.last().map(timestamp).unwrap_or(default_time),
            "agentbroHandoff": {"source": "codex", "sessionId": source.summary.id, "returnContext": source.return_context},
        },
        "messages": messages,
    })
}

fn existing_session(connection: &Connection, target: &ImportTarget) -> Result<bool, String> {
    let stored: Option<(String, Option<String>, Option<i64>)> = connection
        .query_row(
            "SELECT directory, trace_id, time_deleted FROM session WHERE id = ?1",
            [&target.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|err| err.to_string())?;
    match stored {
        Some((directory, trace, None))
            if directory == target.directory && trace.as_deref() == Some(&target.trace) =>
        {
            Ok(true)
        }
        Some(_) => Err("A different or deleted ZCode conversation already has this ID.".into()),
        None => Ok(false),
    }
}

fn existing_task(
    connection: &Connection,
    table: &str,
    target: &ImportTarget,
) -> Result<bool, String> {
    let query =
        format!("SELECT workspace_path, meta_json, deleted FROM {table} WHERE task_id = ?1");
    let mut stmt = connection.prepare(&query).map_err(|err| err.to_string())?;
    let mut rows = stmt.query([&target.id]).map_err(|err| err.to_string())?;
    let Some(row) = rows.next().map_err(|err| err.to_string())? else {
        return Ok(false);
    };
    let path: String = row.get(0).map_err(|err| err.to_string())?;
    let raw: String = row.get(1).map_err(|err| err.to_string())?;
    let deleted: i64 = row.get(2).map_err(|err| err.to_string())?;
    let metadata: Value = serde_json::from_str(&raw).map_err(|err| err.to_string())?;
    if path != target.directory
        || metadata["traceId"].as_str() != Some(&target.trace)
        || deleted != 0
        || rows.next().map_err(|err| err.to_string())?.is_some()
    {
        return Err("A different or deleted ZCode task already has this ID.".into());
    }
    Ok(true)
}

fn prepare(
    live: &Stores,
    target: &ImportTarget,
    folder: &Path,
    mut payload: Value,
) -> Result<bool, String> {
    let cli = zcode_history::open_database(&live.cli)
        .map_err(|_| "Open ZCode once to initialize its conversation database.".to_string())?;
    let tasks = zcode_history::open_database(&live.tasks)
        .map_err(|_| "Open ZCode once to initialize its task index.".to_string())?;
    if existing_session(&cli, target)? && existing_task(&tasks, "tasks", target)? {
        return Ok(false);
    }
    if let Some(identity) = tasks
        .query_row(
            "SELECT workspace_identity FROM tasks WHERE workspace_path = ?1 AND host_id = 'local'
            AND deleted = 0 AND workspace_identity IS NOT NULL ORDER BY updated_at DESC LIMIT 1",
            [&target.directory],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|err| err.to_string())?
    {
        payload["meta"]["workspaceIdentity"] = json!(identity);
    }

    fs::create_dir(folder).map_err(|err| err.to_string())?;
    let stage = Stores::stage(folder);
    stage_schema(&cli, &stage.cli, &["session", "message", "part"])?;
    stage_schema(&tasks, &stage.tasks, &["tasks"])?;
    let bytes = serde_json::to_vec(&payload).map_err(|err| err.to_string())?;
    fs::write(folder.join("conversation.json"), bytes).map_err(|err| err.to_string())?;
    Ok(true)
}

fn stage_schema(source: &Connection, path: &Path, tables: &[&str]) -> Result<(), String> {
    let stage = Connection::open(path).map_err(|err| err.to_string())?;
    for table in tables {
        let sql: String = source
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|err| err.to_string())?;
        stage.execute(&sql, []).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn apply(
    live: &Stores,
    staged: &Stores,
    target: &ImportTarget,
    expected_messages: usize,
) -> Result<(), String> {
    let mut connection = Connection::open_with_flags(&live.cli, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|err| err.to_string())?;
    connection
        .busy_timeout(Duration::from_secs(2))
        .map_err(|err| err.to_string())?;
    for (path, alias) in [
        (&live.tasks, "task_index"),
        (&staged.cli, "staged_history"),
        (&staged.tasks, "staged_index"),
    ] {
        let path = path
            .to_str()
            .ok_or("The database path is not valid UTF-8.")?;
        connection
            .execute(&format!("ATTACH DATABASE ?1 AS {alias}"), [path])
            .map_err(|err| err.to_string())?;
    }
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|err| err.to_string())?;
    if !existing_session(&tx, target)? {
        let actual: usize = tx
            .query_row(
                "SELECT count(*) FROM staged_history.message WHERE session_id = ?1",
                [&target.id],
                |row| row.get(0),
            )
            .map_err(|err| err.to_string())?;
        if actual != expected_messages {
            return Err("ZCode's restore tool did not prepare all reviewed messages.".to_string());
        }
        let workspace_id = tx
            .query_row(
                "SELECT workspace_id FROM main.session WHERE directory = ?1
                   AND time_deleted IS NULL AND workspace_id IS NOT NULL
                 ORDER BY time_updated DESC LIMIT 1",
                [&target.directory],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|err| err.to_string())?
            .unwrap_or_else(|| target.directory.clone());
        tx.execute(
            "UPDATE staged_history.session SET workspace_id = ?1 WHERE id = ?2",
            params![workspace_id, target.id],
        )
        .map_err(|err| err.to_string())?;
        let count = tx.execute("INSERT INTO main.session SELECT * FROM staged_history.session WHERE id = ?1 AND trace_id = ?2", params![target.id, target.trace]).map_err(|err| err.to_string())?;
        if count != 1 {
            return Err(
                "ZCode's restore tool did not prepare the selected conversation.".to_string(),
            );
        }
        tx.execute(
            "UPDATE staged_history.message AS message SET sequence = ordered.ordinal
              FROM (SELECT id, row_number() OVER (ORDER BY rowid) AS ordinal
                      FROM staged_history.message WHERE session_id = ?1) AS ordered
              WHERE message.id = ordered.id",
            [&target.id],
        )
        .map_err(|err| err.to_string())?;
        tx.execute(
            "UPDATE staged_history.part AS part SET sequence = ordered.ordinal
              FROM (SELECT id, row_number() OVER (ORDER BY rowid) AS ordinal
                      FROM staged_history.part WHERE session_id = ?1) AS ordered
              WHERE part.id = ordered.id",
            [&target.id],
        )
        .map_err(|err| err.to_string())?;
        for query in [
            "INSERT INTO main.message SELECT * FROM staged_history.message WHERE session_id = ?1",
            "INSERT INTO main.part SELECT * FROM staged_history.part WHERE session_id = ?1",
        ] {
            tx.execute(query, [&target.id])
                .map_err(|err| err.to_string())?;
        }
    }
    if !existing_task(&tx, "task_index.tasks", target)? {
        if !existing_task(&tx, "staged_index.tasks", target)? {
            return Err("ZCode's restore tool did not prepare the selected task.".to_string());
        }
        let project_id: Option<String> = tx.query_row(
            "SELECT p.project_id FROM task_index.projects p JOIN task_index.project_roots r
                ON p.project_id = r.project_id WHERE p.host_id = 'local' AND p.deleted = 0 AND r.root_path = ?1 LIMIT 1",
            [&target.directory], |row| row.get(0),
        ).optional().map_err(|err| err.to_string())?;
        tx.execute(
            "UPDATE staged_index.tasks SET host_id = 'local', root_path = ?1, project_id = ?2 WHERE task_id = ?3",
            params![target.directory, project_id, target.id],
        ).map_err(|err| err.to_string())?;
        tx.execute(
            "INSERT INTO task_index.tasks SELECT * FROM staged_index.tasks WHERE task_id = ?1",
            [&target.id],
        )
        .map_err(|err| err.to_string())?;
    }
    tx.commit().map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schemas(stores: &Stores) {
        let cli = Connection::open(&stores.cli).unwrap();
        cli.execute_batch(
            "CREATE TABLE session(id TEXT PRIMARY KEY, directory TEXT, trace_id TEXT, time_deleted INTEGER, workspace_id TEXT, time_updated INTEGER);
             CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, sequence INTEGER, data TEXT);
             CREATE TABLE part(id TEXT PRIMARY KEY, session_id TEXT, message_id TEXT, sequence INTEGER, data TEXT);",
        ).unwrap();
        let tasks = Connection::open(&stores.tasks).unwrap();
        tasks.execute_batch(
            "CREATE TABLE tasks(
                task_id TEXT PRIMARY KEY, workspace_path TEXT, meta_json TEXT, deleted INTEGER,
                workspace_identity TEXT, host_id TEXT, root_path TEXT, project_id TEXT, updated_at INTEGER
             );
             CREATE TABLE projects(project_id TEXT PRIMARY KEY, host_id TEXT, deleted INTEGER);
             CREATE TABLE project_roots(project_id TEXT, root_path TEXT);",
        ).unwrap();
    }

    #[test]
    fn recognizes_the_matching_prefix_after_a_zcode_conversation_continues_in_codex() {
        let test = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let home = test.path();
        let project = home.join("归还项目");
        let db_path = zcode_history::database_path(home);
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            r#"CREATE TABLE session(id TEXT PRIMARY KEY, parent_id TEXT, time_deleted INTEGER);
             CREATE TABLE message(id TEXT, session_id TEXT, time_created INTEGER, sequence INTEGER, data TEXT);
             CREATE TABLE part(id TEXT, session_id TEXT, message_id TEXT, time_created INTEGER, sequence INTEGER, data TEXT);
             INSERT INTO session VALUES ('original', NULL, NULL);
             INSERT INTO message VALUES ('m0', 'original', 1000, 0, '{"role":"user"}');
             INSERT INTO part VALUES ('p0', 'original', 'm0', 1000, 0, '{"type":"text","text":"original"}');"#,
        ).unwrap();
        let canonical = project.canonicalize().unwrap();
        let parent = codex::provenance_id("original", canonical.to_str().unwrap());
        let conversations = ["original", "continued in Codex"]
            .into_iter()
            .map(|text| PortableMessage {
                id: text.to_owned(),
                role: ChatRole::User,
                text: text.to_owned(),
                timestamp: None,
            })
            .collect::<Vec<_>>();
        let context = return_context(
            home,
            &parent.to_string(),
            project.to_str().unwrap(),
            &conversations,
        )
        .unwrap();
        assert_eq!(context.original_session_id, "original");
        assert_eq!(context.matching_messages, 1);
    }

    #[test]
    fn imports_both_native_stores_and_repairs_a_missing_index_without_touching_history() {
        let dir = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let live_folder = dir.path().join("live");
        let staged_folder = dir.path().join("staged");
        fs::create_dir(&live_folder).unwrap();
        fs::create_dir(&staged_folder).unwrap();
        let live = Stores::stage(&live_folder);
        let staged = Stores::stage(&staged_folder);
        schemas(&live);
        schemas(&staged);
        let target = target_for("source-1", dir.path().to_str().unwrap());
        let cli = Connection::open(&staged.cli).unwrap();
        cli.execute(
            "INSERT INTO session VALUES (?1, ?2, ?3, NULL, 'temporary', 1)",
            params![target.id, target.directory, target.trace],
        )
        .unwrap();
        cli.execute(
            "INSERT INTO message VALUES ('imported', ?1, NULL, 'historical')",
            [&target.id],
        )
        .unwrap();
        cli.execute(
            "INSERT INTO part VALUES ('part', ?1, 'imported', NULL, 'text')",
            [&target.id],
        )
        .unwrap();
        let tasks = Connection::open(&staged.tasks).unwrap();
        assert!(apply(&live, &staged, &target, 1).is_err());
        let current_cli = Connection::open(&live.cli).unwrap();
        assert!(!existing_session(&current_cli, &target).unwrap());

        tasks
            .execute(
                "INSERT INTO tasks VALUES (?1, ?2, ?3, 0, NULL, NULL, NULL, NULL, 1)",
                params![
                    target.id,
                    target.directory,
                    json!({"traceId": target.trace}).to_string()
                ],
            )
            .unwrap();
        let current_index = Connection::open(&live.tasks).unwrap();
        current_index
            .execute(
                "INSERT INTO projects VALUES ('reviewed-project', 'local', 0)",
                [],
            )
            .unwrap();
        current_index
            .execute(
                "INSERT INTO project_roots VALUES ('reviewed-project', ?1)",
                [&target.directory],
            )
            .unwrap();
        apply(&live, &staged, &target, 1).unwrap();
        assert!(existing_session(&current_cli, &target).unwrap());
        assert_eq!(
            current_cli
                .query_row(
                    "SELECT workspace_id FROM session WHERE id = ?1",
                    [&target.id],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            target.directory
        );
        assert_eq!(
            current_cli
                .query_row(
                    "SELECT sequence FROM message WHERE id = 'imported'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            current_index
                .query_row(
                    "SELECT project_id FROM tasks WHERE task_id = ?1",
                    [&target.id],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "reviewed-project"
        );
        current_cli
            .execute(
                "INSERT INTO message VALUES ('follow-up', ?1, 2, 'new answer')",
                [&target.id],
            )
            .unwrap();
        current_index
            .execute("DELETE FROM tasks WHERE task_id = ?1", [&target.id])
            .unwrap();
        apply(&live, &staged, &target, 1).unwrap();
        apply(&live, &staged, &target, 1).unwrap();
        assert!(existing_task(&current_index, "tasks", &target).unwrap());
        assert_eq!(
            current_cli
                .query_row(
                    "SELECT data FROM message WHERE id = 'follow-up'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "new answer"
        );
        assert_eq!(
            current_cli
                .query_row("SELECT count(*) FROM message", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
}
