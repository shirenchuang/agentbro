use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

use super::schema;

pub struct SwitchDatabase {
    conn: Mutex<Connection>,
}

impl SwitchDatabase {
    pub fn open() -> anyhow::Result<Self> {
        let db_dir = Self::db_dir();
        std::fs::create_dir_all(&db_dir)?;
        let db_path = db_dir.join("switch.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        schema::init_tables(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn with_conn<F, R>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&Connection) -> anyhow::Result<R>,
    {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        f(&conn)
    }

    pub fn clear_all_data(&self) -> anyhow::Result<()> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "DELETE FROM usage_logs;
                 DELETE FROM provider_endpoints;
                 DELETE FROM providers;
                 DELETE FROM prompts;
                 DELETE FROM mcp_servers;
                 DELETE FROM settings;
                 DELETE FROM skills;",
            )?;
            Ok(())
        })
    }

    fn db_dir() -> PathBuf {
        dirs::home_dir()
            .expect("home dir not found")
            .join(".agentbro")
    }
}
