use super::conversation_parser::{ChatRole, MessageBlock, ParsedMessage};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub async fn read_local_messages(session_id: &str) -> Vec<ParsedMessage> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let path = database_path(&home);
    let session_id = session_id.to_owned();
    match tokio::task::spawn_blocking(move || read_messages(&path, &session_id)).await {
        Ok(Ok(messages)) => messages,
        Ok(Err(error)) => {
            log::debug!("Could not read saved ZCode conversation: {error}");
            Vec::new()
        }
        Err(error) => {
            log::debug!("ZCode conversation reader did not finish: {error}");
            Vec::new()
        }
    }
}

pub(crate) fn database_path(home: &Path) -> PathBuf {
    home.join(".zcode/cli/db/db.sqlite")
}

pub(crate) fn open_database(database: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_millis(150))?;
    Ok(conn)
}

pub(crate) fn read_messages(
    database: &Path,
    session_id: &str,
) -> rusqlite::Result<Vec<ParsedMessage>> {
    query_messages(&open_database(database)?, session_id)
}

pub(crate) fn query_messages(
    conn: &Connection,
    session_id: &str,
) -> rusqlite::Result<Vec<ParsedMessage>> {
    let mut statement = conn.prepare(
        "SELECT m.id, m.time_created,
                CASE WHEN json_valid(m.data) THEN json_extract(m.data, '$.role') END AS role,
                CASE WHEN json_valid(p.data) THEN
                    CASE WHEN json_extract(p.data, '$.type') = 'text'
                        AND json_type(p.data, '$.text') = 'text'
                        AND coalesce(json_extract(p.data, '$.synthetic'), 0) = 0
                        AND coalesce(json_extract(p.data, '$.ignored'), 0) = 0
                    THEN json_extract(p.data, '$.text') END
                END AS text
         FROM session AS s
         JOIN message AS m ON m.session_id = s.id
         JOIN part AS p ON p.session_id = m.session_id AND p.message_id = m.id
         WHERE s.id = ?1 AND s.time_deleted IS NULL
           AND role IN ('user', 'assistant') AND text IS NOT NULL
         ORDER BY m.sequence, m.time_created, m.id, p.sequence, p.time_created, p.id",
    )?;
    let mut rows = statement.query([session_id])?;
    let mut messages: Vec<ParsedMessage> = Vec::new();
    while let Some(row) = rows.next()? {
        let text: String = row.get(3)?;
        if text.trim().is_empty() {
            continue;
        }
        let id: String = row.get(0)?;
        let block = MessageBlock::Text { text };
        if let Some(message) = messages.last_mut().filter(|message| message.id == id) {
            message.blocks.push(block);
            continue;
        }
        let created_at: Option<i64> = row.get(1)?;
        let role = match row.get_ref(2)?.as_str()? {
            "user" => ChatRole::User,
            "assistant" => ChatRole::Assistant,
            _ => continue,
        };
        messages.push(ParsedMessage {
            id,
            role,
            timestamp: created_at
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|date| date.to_rfc3339()),
            blocks: vec![block],
        });
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::installer::TempInstallDir;
    use serde_json::json;

    #[test]
    fn reads_committed_zcode_messages_in_session_order_from_wal() {
        let fixture = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let database = fixture.path().join("ZCode 会话.sqlite");
        let writer = Connection::open(&database).unwrap();
        writer
            .execute_batch(
                r#"PRAGMA journal_mode = WAL;
                 CREATE TABLE session(id TEXT PRIMARY KEY, time_deleted INTEGER);
                 CREATE TABLE message(id TEXT, session_id TEXT, time_created INTEGER, sequence INTEGER, data TEXT);
                 CREATE TABLE part(id TEXT, message_id TEXT, session_id TEXT, time_created INTEGER, sequence INTEGER, data TEXT);
                 INSERT INTO session VALUES ('sess_open', NULL), ('sess_other', NULL), ('sess_deleted', 2);
                 INSERT INTO message VALUES
                    ('assistant', 'sess_open', 1000, 2, '{"role":"assistant"}'),
                    ('user', 'sess_open', 2000, 1, '{"role":"user"}'),
                    ('bad-message', 'sess_open', 3000, 3, 'malformed'),
                    ('user', 'sess_deleted', 2000, 1, '{"role":"user"}');"#,
            )
            .unwrap();

        let parts = [
            (
                "sess_open",
                "assistant",
                2,
                json!({"type":"text", "text":"回答后半段"}),
            ),
            (
                "sess_open",
                "assistant",
                1,
                json!({"type":"text", "text":"回答前半段"}),
            ),
            (
                "sess_open",
                "assistant",
                3,
                json!({"type":"tool", "state":{"output":"tool output"}}),
            ),
            (
                "sess_open",
                "assistant",
                4,
                json!({"type":"reasoning", "text":"internal reasoning"}),
            ),
            (
                "sess_open",
                "user",
                1,
                json!({"type":"text", "text":"原始问题"}),
            ),
            (
                "sess_open",
                "user",
                2,
                json!({"type":"text", "text":"synthetic text", "synthetic":true}),
            ),
            (
                "sess_open",
                "user",
                3,
                json!({"type":"text", "text":"ignored text", "ignored":true}),
            ),
            (
                "sess_open",
                "user",
                4,
                json!({"type":"file", "url":"data:attachment"}),
            ),
            (
                "sess_other",
                "user",
                0,
                json!({"type":"text", "text":"unrelated conversation"}),
            ),
            (
                "sess_deleted",
                "user",
                0,
                json!({"type":"text", "text":"deleted conversation"}),
            ),
            (
                "sess_open",
                "bad-message",
                0,
                json!({"type":"text", "text":"invalid message"}),
            ),
        ];
        for (index, (session, message, sequence, part)) in parts.into_iter().enumerate() {
            writer
                .execute(
                    "INSERT INTO part VALUES (?1, ?2, ?3, 1000, ?4, ?5)",
                    (
                        index.to_string(),
                        message,
                        session,
                        sequence,
                        part.to_string(),
                    ),
                )
                .unwrap();
        }
        writer
            .execute(
                "INSERT INTO part VALUES ('bad-part', 'user', 'sess_open', 1000, 4, 'malformed')",
                [],
            )
            .unwrap();

        let messages = read_messages(&database, "sess_open").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(
            (messages[0].id.as_str(), &messages[0].role),
            ("user", &ChatRole::User)
        );
        assert_eq!(
            messages[0].timestamp.as_deref(),
            Some("1970-01-01T00:00:02+00:00")
        );
        assert!(
            matches!(&messages[0].blocks[..], [MessageBlock::Text { text }] if text == "原始问题")
        );
        assert_eq!(
            (messages[1].id.as_str(), &messages[1].role),
            ("assistant", &ChatRole::Assistant)
        );
        assert!(matches!(&messages[1].blocks[..], [
            MessageBlock::Text { text: first }, MessageBlock::Text { text: second }
        ] if first == "回答前半段" && second == "回答后半段"));
        assert!(read_messages(&database, "sess_deleted").unwrap().is_empty());
        assert!(read_messages(&database, "sess_missing").unwrap().is_empty());
    }

    #[test]
    fn missing_zcode_database_is_never_created_by_the_reader() {
        let fixture = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let database = fixture.path().join("db.sqlite");
        assert!(read_messages(&database, "sess_missing").is_err());
        assert!(!database.exists());
    }
}
