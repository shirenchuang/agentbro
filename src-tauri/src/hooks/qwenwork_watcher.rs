use super::session_store::{SessionPhase, SessionStore};
use rusqlite::{Connection, OpenFlags};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const AGENT: &str = "qwenwork-cn";
const NAME: &str = "千问办公";
const DATABASE: &str = "host-agent-responses.db";
const STALE_MS: i64 = 30 * 60 * 1000;
const DONE_MS: i64 = 20 * 1000;

pub fn start(store: Arc<SessionStore>) {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let root = home.join(".qwenworkcn");
    tauri::async_runtime::spawn(async move {
        let mut visible = HashSet::new();
        loop {
            let root = root.clone();
            let now = chrono::Utc::now().timestamp_millis();
            if let Ok(Some(turns)) =
                tokio::task::spawn_blocking(move || read_recent_turns(&root, now)).await
            {
                reconcile(&store, &mut visible, turns, now);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

#[derive(Debug)]
struct Turn {
    session_id: String,
    phase: SessionPhase,
    description: &'static str,
    updated: i64,
}

fn read_recent_turns(root: &Path, now: i64) -> Option<Vec<Turn>> {
    let mut turns = Vec::new();
    for database in database_paths(root) {
        let records = read_database(&database, now - STALE_MS).ok()?;
        turns.extend(records);
    }
    Some(turns)
}

fn database_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![root.join(DATABASE)];
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

fn read_database(path: &Path, since: i64) -> rusqlite::Result<Vec<Turn>> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_millis(100))?;
    let mut query = connection.prepare(
        "SELECT session_id, status, updated_at FROM (
            SELECT session_id, status, updated_at,
                   ROW_NUMBER() OVER (
                       PARTITION BY session_id
                       ORDER BY CASE status
                           WHEN 'waiting_interaction' THEN 0
                           WHEN 'in_progress' THEN 0
                           WHEN 'cancelling' THEN 0
                           ELSE 1
                       END, ordinal DESC
                   ) AS rank
            FROM desk_turns WHERE updated_at >= ?1
        ) WHERE rank = 1 ORDER BY updated_at DESC LIMIT 64",
    )?;
    let rows = query.query_map([since], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    let mut turns = Vec::new();
    for row in rows {
        let (session_id, status, updated) = row?;
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
            phase,
            description,
            updated,
        });
    }
    Ok(turns)
}

fn reconcile(store: &SessionStore, visible: &mut HashSet<String>, turns: Vec<Turn>, now: i64) {
    let mut latest = HashMap::<String, Turn>::new();
    for turn in turns {
        if latest
            .get(&turn.session_id)
            .is_none_or(|old| old.updated < turn.updated)
        {
            latest.insert(turn.session_id.clone(), turn);
        }
    }
    let mut next = HashSet::new();
    for (id, turn) in latest {
        if matches!(
            turn.phase,
            SessionPhase::Done | SessionPhase::Error | SessionPhase::Interrupted
        ) && now.saturating_sub(turn.updated) > DONE_MS
        {
            continue;
        }
        next.insert(id.clone());
        let existing = store.get_session(&id);
        if existing.as_ref().is_some_and(|session| {
            session.phase == turn.phase && session.description.as_deref() == Some(turn.description)
        }) {
            continue;
        }
        if existing.is_none() {
            store.get_or_create_session(&id, AGENT, NAME, "", NAME);
        }
        store.update_session(&id, |session| {
            session.term_bundle_id = Some("cn.qwenwork.desktop.mac".to_string());
            session.phase = turn.phase;
            session.description = Some(turn.description.to_string());
        });
    }
    for id in visible.difference(&next) {
        store.remove_session(id);
    }
    *visible = next;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_qwenwork_turn_phases_without_owning_approvals() {
        let directory =
            std::env::temp_dir().join(format!("agentbro-qwenwork-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let database = Connection::open(directory.join(DATABASE)).unwrap();
        database
            .execute_batch(
                "CREATE TABLE desk_turns (
                session_id TEXT NOT NULL, status TEXT NOT NULL,
                ordinal INTEGER NOT NULL, updated_at INTEGER NOT NULL
             );
             INSERT INTO desk_turns VALUES
               ('a', 'in_progress', 1, 1000),
               ('a', 'queued', 2, 1000),
               ('b', 'completed', 1, 1000);",
            )
            .unwrap();
        let store = SessionStore::new();
        let mut visible = HashSet::new();
        let observe = |visible: &mut HashSet<String>, now| {
            let turns = read_recent_turns(&directory, now).unwrap();
            reconcile(&store, visible, turns, now);
        };
        observe(&mut visible, 1000);
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
        observe(&mut visible, 2000);
        let waiting = store.get_session(id).unwrap();
        assert_eq!(waiting.phase, SessionPhase::WaitingInput);
        assert!(waiting.pending_permission.is_none());
        assert!(waiting.pending_question.is_none());

        observe(&mut visible, STALE_MS + 2001);
        assert!(store.get_session(id).is_none());
        assert!(visible.is_empty());
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
