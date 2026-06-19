use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

use super::schema;

pub struct SwitchDatabase {
    conn: Mutex<Connection>,
}

impl SwitchDatabase {
    pub fn open() -> anyhow::Result<Self> {
        let db_dir = Self::db_dir()?;
        std::fs::create_dir_all(&db_dir)?;
        let db_path = db_dir.join("switch.db");
        // Migrate from old flat location
        let old_db = crate::data_dir::agentbro_home().join("switch.db");
        crate::data_dir::migrate_sqlite(&old_db, &db_path);
        let conn = Connection::open(&db_path)?;
        Self::init_connection(&conn, true)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init_connection(&conn, false)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn init_connection(conn: &Connection, persistent: bool) -> anyhow::Result<()> {
        if persistent {
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        }
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        schema::init_tables(conn)?;
        Ok(())
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

    fn db_dir() -> anyhow::Result<PathBuf> {
        dirs::home_dir()
            .map(|home| home.join(".agentbro").join("switch"))
            .ok_or_else(|| anyhow::anyhow!("home dir not found"))
    }
}
