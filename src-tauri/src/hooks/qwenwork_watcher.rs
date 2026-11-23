use super::session_store::{SessionPhase, SessionState, SessionStore};
use rusqlite::{Connection, OpenFlags};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const AGENT: &str = "qwenwork-cn";
const NAME: &str = "千问办公";
const DATABASE: &str = "host-agent-responses.db";
const FLASH_DATABASE: &str = "qwenwork.db";
const STALE_MS: i64 = 30 * 60 * 1000;
const DONE_MS: i64 = 20 * 1000;

pub fn start(store: Arc<SessionStore>) {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let root = home.join(".qwenworkcn");
    tauri::async_runtime::spawn(async move {
        let mut visible = HashSet::new();
        let mut cached = HashMap::new();
        loop {
            let root = root.clone();
            let now = chrono::Utc::now().timestamp_millis();
            if let Ok(observed) =
                tokio::task::spawn_blocking(move || read_recent_turns(&root, now)).await
            {
                reconcile(&store, &mut visible, &mut cached, observed, now);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

#[derive(Debug)]
struct Turn {
    session_id: String,
    title: Option<String>,
    phase: SessionPhase,
    description: &'static str,
    updated: i64,
}

fn read_recent_turns(root: &Path, now: i64) -> HashMap<PathBuf, Option<Vec<Turn>>> {
    database_paths(root)
        .into_iter()
        .map(|path| {
            let turns = if path.ends_with(FLASH_DATABASE) {
                read_flash_database(&path, now)
            } else {
                read_database(&path, now)
            };
            (path, turns.ok())
        })
        .collect()
}

fn database_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![
        root.join(DATABASE),
        root.join(".harness/flash-cli/qwenwork")
            .join(FLASH_DATABASE),
    ];
    for (directory, suffix) in [("identities", "qwapp"), ("qwapp-identities", "")] {
        let Ok(entries) = std::fs::read_dir(root.join(directory)) else {
            continue;
        };
        for entry in entries.flatten().take(32) {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                paths.push(entry.path().join(suffix).join(DATABASE));
            }
        }
    }
    paths.retain(|path| {
        path.symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_file())
    });
    paths
}

fn read_database(path: &Path, now: i64) -> rusqlite::Result<Vec<Turn>> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_millis(100))?;
    let mut query = connection.prepare(
        "SELECT session_id, status, updated_at FROM (
            SELECT session_id, status, updated_at,
                   status IN ('queued', 'in_progress', 'waiting_interaction', 'cancelling') AS active,
                   ROW_NUMBER() OVER (
                       PARTITION BY session_id
                       ORDER BY CASE status
                           WHEN 'waiting_interaction' THEN 0
                           WHEN 'in_progress' THEN 0
                           WHEN 'cancelling' THEN 0
                           ELSE 1
                       END, ordinal DESC
                   ) AS rank
            FROM desk_turns
            WHERE updated_at >= ?1
              AND typeof(session_id) = 'text' AND typeof(status) = 'text'
              AND typeof(updated_at) = 'integer' AND typeof(ordinal) = 'integer'
        ) WHERE rank = 1
            AND (active OR (status IN ('completed', 'incomplete', 'failed', 'cancelled') AND updated_at >= ?2))
        ORDER BY active DESC, updated_at DESC LIMIT 64",
    )?;
    let mut rows = query.query([now - STALE_MS, now - DONE_MS])?;
    let mut turns = Vec::new();
    while let Some(row) = rows.next()? {
        let (Ok(session_id), Ok(status), Ok(updated)) = (
            row.get::<_, String>(0),
            row.get::<_, String>(1),
            row.get::<_, i64>(2),
        ) else {
            continue;
        };
        let (phase, description) = match status.as_str() {
            "queued" => (SessionPhase::Ready, "任务已排队"),
            "in_progress" => (SessionPhase::Processing, "任务进行中"),
            "waiting_interaction" => (SessionPhase::WaitingInput, "请在千问办公中处理待确认事项"),
            "cancelling" => (SessionPhase::Processing, "正在取消任务"),
            "completed" => (SessionPhase::Done, "任务已完成"),
            "incomplete" | "failed" => (SessionPhase::Error, "任务未完成"),
            "cancelled" => (SessionPhase::Interrupted, "任务已取消"),
            _ => continue,
        };
        if session_id.is_empty() || session_id.len() > 256 {
            continue;
        }
        turns.push(Turn {
            session_id: format!("{AGENT}:{session_id}"),
            title: None,
            phase,
            description,
            updated,
        });
    }
    Ok(turns)
}

fn read_flash_database(path: &Path, now: i64) -> rusqlite::Result<Vec<Turn>> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_millis(100))?;
    // CASE guards extraction even if SQLite reorders WHERE predicates. Flag 10 accepts
    // JSON5 text and fully validated JSONB, the formats handled by the bundled extractor.
    let mut query = connection.prepare(
        "WITH latest_turns AS (
            SELECT s.id, substr(s.title, 1, 120) AS title, s.time_updated AS session_updated,
                   COALESCE(prompt.time_updated, prompt.time_created) AS prompt_updated,
                   COALESCE(answer.time_updated, prompt.time_updated, prompt.time_created) AS message_updated,
                   json_extract(answer.data, '$.time.completed') AS completed_at,
                   json_extract(answer.data, '$.finish') AS finish_reason,
                   json_extract(answer.data, '$.error.name') AS error_name
            FROM session s
            JOIN message prompt ON prompt.id = (
                SELECT m.id FROM message m
                WHERE m.session_id = s.id
                  AND CASE WHEN json_valid(m.data, 10) THEN json_extract(m.data, '$.role') = 'user' END
                ORDER BY m.time_created DESC, m.id DESC LIMIT 1
            )
            LEFT JOIN message answer ON answer.id = (
                SELECT m.id FROM message m
                WHERE m.session_id = s.id
                  AND CASE WHEN json_valid(m.data, 10) THEN
                      json_extract(m.data, '$.role') = 'assistant'
                      AND json_extract(m.data, '$.parentID') = prompt.id
                  END
                ORDER BY m.time_created DESC, m.id DESC LIMIT 1
            )
            WHERE typeof(s.time_updated) = 'integer' AND s.time_archived IS NULL AND s.parent_id IS NULL
        ), classified AS (
            SELECT *, MAX(session_updated, prompt_updated, message_updated) AS activity_at,
            CASE
                WHEN error_name IS NOT NULL THEN message_updated
                WHEN finish_reason NOT IN ('tool-calls', 'unknown') THEN completed_at
            END AS terminal_at
            FROM latest_turns
        )
        SELECT id, title, activity_at, message_updated, completed_at, finish_reason, error_name
        FROM classified
        WHERE activity_at >= ?1 AND (terminal_at IS NULL OR terminal_at >= ?2)
        ORDER BY terminal_at IS NOT NULL, COALESCE(terminal_at, activity_at) DESC LIMIT 64",
    )?;
    let mut rows = query.query([now - STALE_MS, now - DONE_MS])?;
    let mut turns = Vec::new();
    while let Some(row) = rows.next()? {
        let (Ok(session_id), Ok(activity_at), Ok(message_updated)) = (
            row.get::<_, String>(0),
            row.get::<_, i64>(2),
            row.get::<_, i64>(3),
        ) else {
            continue;
        };
        if session_id.is_empty() || session_id.len() > 256 {
            continue;
        }
        let title = row
            .get::<_, Option<String>>(1)
            .ok()
            .flatten()
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());
        let (Ok(completed), Ok(finish), Ok(error)) = (
            row.get::<_, Option<i64>>(4),
            row.get::<_, Option<String>>(5),
            row.get::<_, Option<String>>(6),
        ) else {
            continue;
        };
        let (phase, description, updated) = match (completed, finish.as_deref(), error.as_deref()) {
            (_, _, Some("MessageAbortedError")) => {
                (SessionPhase::Interrupted, "任务已取消", message_updated)
            }
            (_, _, Some(_)) => (SessionPhase::Error, "任务未完成", message_updated),
            (Some(at), Some(reason), None) if !matches!(reason, "tool-calls" | "unknown") => {
                (SessionPhase::Done, "任务已完成", at)
            }
            _ => (SessionPhase::Processing, "任务进行中", activity_at),
        };
        turns.push(Turn {
            session_id: format!("{AGENT}:flash:{session_id}"),
            title,
            phase,
            description,
            updated,
        });
    }
    Ok(turns)
}

fn reconcile(
    store: &SessionStore,
    visible: &mut HashSet<String>,
    cached: &mut HashMap<PathBuf, Vec<Turn>>,
    observed: HashMap<PathBuf, Option<Vec<Turn>>>,
    now: i64,
) {
    cached.retain(|path, _| observed.contains_key(path));
    for (path, turns) in observed {
        if let Some(turns) = turns {
            cached.insert(path, turns);
        }
    }

    let mut latest = HashMap::<&str, &Turn>::new();
    for turns in cached.values_mut() {
        turns.retain(|turn| now.saturating_sub(turn.updated) <= STALE_MS);
        for turn in turns.iter() {
            if latest
                .get(turn.session_id.as_str())
                .is_none_or(|old| old.updated < turn.updated)
            {
                latest.insert(&turn.session_id, turn);
            }
        }
    }
    let mut next = HashSet::new();
    let mut updates = Vec::new();
    for (id, turn) in latest {
        if matches!(
            turn.phase,
            SessionPhase::Done | SessionPhase::Error | SessionPhase::Interrupted
        ) && now.saturating_sub(turn.updated) > DONE_MS
        {
            continue;
        }
        next.insert(id.to_string());
        let existing = store.get_session(id);
        if existing.as_ref().is_some_and(|session| {
            session.phase == turn.phase
                && session.description.as_deref() == Some(turn.description)
                && session.session_title == turn.title
        }) {
            continue;
        }
        let mut observed = SessionState::new(
            id.to_string(),
            AGENT.to_string(),
            NAME.to_string(),
            String::new(),
            NAME.to_string(),
        );
        observed.term_bundle_id =
            cfg!(target_os = "macos").then(|| "cn.qwenwork.desktop.mac".to_string());
        observed.session_title = turn.title.clone();
        observed.phase = turn.phase.clone();
        observed.description = Some(turn.description.to_string());
        updates.push(observed);
    }
    store.apply_observed_sessions(updates, visible.difference(&next).map(String::as_str));
    *visible = next;
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = "CREATE TABLE desk_turns (
        session_id TEXT NOT NULL, status TEXT NOT NULL,
        ordinal INTEGER NOT NULL, updated_at INTEGER NOT NULL
    );";
    const FLASH_SCHEMA: &str = "CREATE TABLE session (
        id TEXT PRIMARY KEY, title TEXT, time_updated INTEGER,
        time_archived INTEGER, parent_id TEXT
    );
    CREATE TABLE message (
        id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER,
        time_updated INTEGER, data TEXT
    );";

    #[test]
    fn flash_uses_prompt_and_reply_activity_when_session_metadata_is_stale() {
        let directory =
            std::env::temp_dir().join(format!("agentbro-qwenwork-{}", uuid::Uuid::new_v4()));
        let flash_root = directory.join(".harness/flash-cli/qwenwork");
        std::fs::create_dir_all(&flash_root).unwrap();
        let database = Connection::open(flash_root.join(FLASH_DATABASE)).unwrap();
        database.execute_batch(FLASH_SCHEMA).unwrap();
        let now = STALE_MS + 10_000;
        database
            .execute_batch(&format!(
                r#"INSERT INTO session VALUES
                    ('new-prompt', 'Resumed conversation', 1000, NULL, NULL),
                    ('live-reply', 'Long-running reply', 1000, NULL, NULL),
                    ('archived', 'Archived conversation', 1000, 2000, NULL),
                    ('child', 'Child conversation', 1000, NULL, 'new-prompt');
                   INSERT INTO message VALUES
                    ('np-user', 'new-prompt', {now}, {now}, '{{"role":"user"}}'),
                    ('lr-user', 'live-reply', 1000, 1000, '{{"role":"user"}}'),
                    ('lr-assistant', 'live-reply', 1001, {now},
                     '{{"role":"assistant","parentID":"lr-user","finish":"tool-calls"}}'),
                    ('archived-user', 'archived', {now}, {now}, '{{"role":"user"}}'),
                    ('child-user', 'child', {now}, {now}, '{{"role":"user"}}');"#,
            ))
            .unwrap();
        let store = SessionStore::new();
        let mut visible = HashSet::new();
        let mut cached = HashMap::new();
        let mut observe = |at| {
            let observed = read_recent_turns(&directory, at);
            reconcile(&store, &mut visible, &mut cached, observed, at);
        };

        observe(now);
        let phase = |id: &str| {
            store
                .get_session(&format!("qwenwork-cn:flash:{id}"))
                .map(|session| session.phase)
        };
        assert_eq!(phase("new-prompt"), Some(SessionPhase::Processing));
        assert_eq!(phase("live-reply"), Some(SessionPhase::Processing));
        assert_eq!(phase("archived"), None);
        assert_eq!(phase("child"), None);

        database
            .execute(
                "UPDATE message SET data = json_object(
                    'role','assistant','parentID','lr-user',
                    'time',json_object('completed',?1),'finish','stop'
                ) WHERE id = 'lr-assistant'",
                [now],
            )
            .unwrap();
        observe(now);
        assert_eq!(phase("live-reply"), Some(SessionPhase::Done));
        observe(now + DONE_MS + 1);
        assert_eq!(phase("live-reply"), None);
        assert_eq!(phase("new-prompt"), Some(SessionPhase::Processing));
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn flash_tasks_follow_the_latest_user_message_through_tool_steps() {
        let directory =
            std::env::temp_dir().join(format!("agentbro-qwenwork-{}", uuid::Uuid::new_v4()));
        let flash_root = directory.join(".harness/flash-cli/qwenwork");
        std::fs::create_dir_all(&flash_root).unwrap();
        let database = Connection::open(flash_root.join(FLASH_DATABASE)).unwrap();
        database.execute_batch(FLASH_SCHEMA).unwrap();
        database
            .execute_batch(
                r#"INSERT INTO session VALUES ('s', '实际任务标题', 3000, NULL, NULL);
                   INSERT INTO message VALUES
                     ('u1', 's', 1000, 3000, '{"role":"user"}'),
                     ('a1', 's', 1001, 2000,
                      '{"role":"assistant","parentID":"u1","time":{"completed":2000},"finish":"stop"}'),
                     ('u2', 's', 3000, 3000, '{"role":"user"}');"#,
            )
            .unwrap();
        let store = SessionStore::new();
        let mut visible = HashSet::new();
        let mut cached = HashMap::new();
        let mut observe = |now| {
            let observed = read_recent_turns(&directory, now);
            reconcile(&store, &mut visible, &mut cached, observed, now);
            store.get_session("qwenwork-cn:flash:s")
        };
        let session = observe(3000).unwrap();
        assert_eq!(session.phase, SessionPhase::Processing);
        assert_eq!(session.session_title.as_deref(), Some("实际任务标题"));
        database
            .execute_batch(
                r#"INSERT INTO message VALUES
                   ('a2', 's', 3001, 4000,
                    '{"role":"assistant","parentID":"u2","time":{"completed":4000},"finish":"tool-calls"}'),
                   ('partial', 's', 3999, 4000, '{"role":"assistant"');"#,
            )
            .unwrap();
        assert_eq!(observe(4000).unwrap().phase, SessionPhase::Processing);
        database
            .execute_batch(
                r#"INSERT INTO session VALUES ('bad', '无效时间', X'FF', NULL, NULL);
                   INSERT INTO message VALUES
                   ('bad-prompt', 'bad', 5000, 5000, '{"role":"user"}'),
                   ('a3', 's', 4001, 5000,
                    '{role:"assistant",parentID:"u2",time:{completed:5000},finish:"stop"}');"#,
            )
            .unwrap();
        assert_eq!(observe(5000).unwrap().phase, SessionPhase::Done);
        assert!(observe(5000 + DONE_MS + 1).is_none());
        assert!(store.get_session("qwenwork-cn:flash:bad").is_none());
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn keeps_waiting_and_running_tasks_visible_amid_completed_history() {
        let directory =
            std::env::temp_dir().join(format!("agentbro-qwenwork-{}", uuid::Uuid::new_v4()));
        let flash_root = directory.join(".harness/flash-cli/qwenwork");
        std::fs::create_dir_all(&flash_root).unwrap();
        let desk_path = directory.join(DATABASE);
        let flash_path = flash_root.join(FLASH_DATABASE);
        let desk = Connection::open(&desk_path).unwrap();
        let flash = Connection::open(&flash_path).unwrap();
        desk.execute_batch(SCHEMA).unwrap();
        flash.execute_batch(FLASH_SCHEMA).unwrap();
        desk.execute_batch(
            "INSERT INTO desk_turns VALUES ('live-desk', 'waiting_interaction', 1, 1000);
             WITH RECURSIVE items(n) AS (VALUES(1) UNION ALL SELECT n + 1 FROM items WHERE n < 70)
             INSERT INTO desk_turns SELECT 'done-' || n, 'completed', 1, 115000 FROM items;",
        )
        .unwrap();
        flash
            .execute_batch(
                r#"WITH RECURSIVE items(n) AS (VALUES(1) UNION ALL SELECT n + 1 FROM items WHERE n < 70)
                   INSERT INTO session SELECT 'done-' || n, 'Done task', 115000, NULL, NULL FROM items;
                   INSERT INTO message
                     SELECT 'prompt-' || id, id, 114999, 114999, '{"role":"user"}' FROM session;
                   INSERT INTO message
                     SELECT 'answer-' || id, id, 115000, 115000,
                            json_object('role','assistant','parentID','prompt-' || id,
                                        'time',json_object('completed',115000),'finish','stop')
                     FROM session;
                   INSERT INTO session VALUES ('live-flash', 'Running task', 1000, NULL, NULL);
                   INSERT INTO message VALUES ('live-prompt', 'live-flash', 1000, 1000, '{"role":"user"}');"#,
            )
            .unwrap();
        let store = SessionStore::new();
        store.get_or_create_session("other-agent", "codex", "project", "", "terminal");
        let initial_updates = store.update_request_count();
        let mut visible = HashSet::new();
        let mut cached = HashMap::new();
        for (now, expected_rows, update_count) in [
            (120000, 64, 1),
            (120000, 64, 1),
            (115000 + DONE_MS + 1, 1, 2),
        ] {
            let observed = read_recent_turns(&directory, now);
            for path in [&desk_path, &flash_path] {
                assert_eq!(observed[path].as_ref().unwrap().len(), expected_rows);
            }
            reconcile(&store, &mut visible, &mut cached, observed, now);
            assert_eq!(store.update_request_count() - initial_updates, update_count);
            assert_eq!(
                store
                    .get_session("qwenwork-cn:live-desk")
                    .map(|session| session.phase),
                Some(SessionPhase::WaitingInput)
            );
            assert_eq!(
                store
                    .get_session("qwenwork-cn:flash:live-flash")
                    .map(|session| session.phase),
                Some(SessionPhase::Processing)
            );
        }
        assert_eq!(visible.len(), 2);
        assert!(store.get_session("other-agent").is_some());
        drop(desk);
        drop(flash);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn mirrors_qwenwork_turn_phases_without_owning_approvals() {
        let directory =
            std::env::temp_dir().join(format!("agentbro-qwenwork-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let database = Connection::open(directory.join(DATABASE)).unwrap();
        database.execute_batch(SCHEMA).unwrap();
        database
            .execute_batch(
                "INSERT INTO desk_turns VALUES
               ('a', 'in_progress', 1, 1000),
               ('a', 'queued', 2, 1000),
               ('a', 'in_progress', 3, 'invalid'),
               (X'FF', 'in_progress', 1, 2000),
               ('b', 'completed', 1, 1000);",
            )
            .unwrap();
        let store = SessionStore::new();
        let mut visible = HashSet::new();
        let mut cached = HashMap::new();
        let mut observe = |now| {
            let observed = read_recent_turns(&directory, now);
            reconcile(&store, &mut visible, &mut cached, observed, now);
        };
        observe(1000);
        let id = "qwenwork-cn:a";
        assert_eq!(
            store.get_session(id).unwrap().phase,
            SessionPhase::Processing
        );
        assert_eq!(
            store.get_session("qwenwork-cn:b").unwrap().phase,
            SessionPhase::Done
        );

        database
            .execute(
                "UPDATE desk_turns SET status = 'waiting_interaction', updated_at = 2000
             WHERE session_id = 'a' AND ordinal = 1",
                [],
            )
            .unwrap();
        observe(2000);
        let waiting = store.get_session(id).unwrap();
        assert_eq!(waiting.phase, SessionPhase::WaitingInput);
        assert_eq!(
            waiting.term_bundle_id.as_deref(),
            cfg!(target_os = "macos").then_some("cn.qwenwork.desktop.mac")
        );
        assert!(waiting.pending_permission.is_none());
        assert!(waiting.pending_question.is_none());

        observe(STALE_MS + 2001);
        assert!(store.get_session(id).is_none());
        assert!(visible.is_empty());
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn isolates_unreadable_databases_and_reconciles_recovery_and_removal() {
        let directory =
            std::env::temp_dir().join(format!("agentbro-qwenwork-{}", uuid::Uuid::new_v4()));
        let account = directory.join("identities/secondary/qwapp");
        std::fs::create_dir_all(&account).unwrap();
        let default_db = Connection::open(directory.join(DATABASE)).unwrap();
        default_db.execute_batch(SCHEMA).unwrap();
        default_db
            .execute(
                "INSERT INTO desk_turns VALUES ('a', 'in_progress', 1, 1)",
                [],
            )
            .unwrap();
        let account_path = account.join(DATABASE);
        let account_db = Connection::open(&account_path).unwrap();

        let store = SessionStore::new();
        let mut visible = HashSet::new();
        let mut cached = HashMap::new();
        let mut observe = |now| {
            let observed = read_recent_turns(&directory, now);
            reconcile(&store, &mut visible, &mut cached, observed, now);
        };
        let phase = |id: &str| store.get_session(id).map(|session| session.phase);
        let (a, b) = ("qwenwork-cn:a", "qwenwork-cn:b");

        observe(1);
        assert_eq!(phase(a), Some(SessionPhase::Processing));
        assert_eq!(phase(b), None);

        account_db.execute_batch(SCHEMA).unwrap();
        account_db
            .execute(
                "INSERT INTO desk_turns VALUES ('b', 'in_progress', 1, 2)",
                [],
            )
            .unwrap();
        observe(2);
        assert_eq!(phase(b), Some(SessionPhase::Processing));

        account_db.execute("DROP TABLE desk_turns", []).unwrap();
        default_db
            .execute(
                "UPDATE desk_turns SET status = 'completed', updated_at = 3",
                [],
            )
            .unwrap();
        observe(3);
        assert_eq!(phase(a), Some(SessionPhase::Done));
        assert_eq!(phase(b), Some(SessionPhase::Processing));

        observe(STALE_MS + 3);
        assert_eq!(phase(b), None);
        account_db.execute_batch(SCHEMA).unwrap();
        account_db
            .execute(
                "INSERT INTO desk_turns VALUES ('b', 'waiting_interaction', 2, ?1)",
                [STALE_MS + 4],
            )
            .unwrap();
        observe(STALE_MS + 4);
        assert_eq!(phase(b), Some(SessionPhase::WaitingInput));

        drop(account_db);
        std::fs::remove_file(account_path).unwrap();
        observe(STALE_MS + 5);
        assert_eq!(phase(b), None);
        drop(default_db);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
