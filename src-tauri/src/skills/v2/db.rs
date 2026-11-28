//! SQLite storage for Skill Manager v2 — schema, migrations, and typed row
//! access. The connection lives in a `Mutex<Connection>` held by `Service`.

use crate::skills::v2::fsutil;
use crate::skills::v2::models::{Snapshot, SCHEMA_VERSION};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const MIGRATIONS: &[&str] = &[
    // v1 — initial schema
    r#"
    CREATE TABLE IF NOT EXISTS schema_migrations (
      version INTEGER PRIMARY KEY,
      applied_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS settings (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS skills (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      description TEXT NOT NULL DEFAULT '',
      skill_type TEXT NOT NULL DEFAULT 'skill',
      center_path TEXT NOT NULL,
      current_hash TEXT NOT NULL,
      frontmatter_json TEXT NOT NULL DEFAULT '{}',
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL,
      last_scanned_at TEXT
    );

    CREATE TABLE IF NOT EXISTS skill_sources (
      skill_id TEXT PRIMARY KEY REFERENCES skills(id) ON DELETE CASCADE,
      source_type TEXT NOT NULL,
      source_uri TEXT,
      source_ref TEXT,
      imported_from_agent TEXT,
      imported_from_path TEXT,
      installed_via TEXT NOT NULL,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS agents (
      id TEXT PRIMARY KEY,
      display_name TEXT NOT NULL,
      skills_dir TEXT,
      config_path TEXT,
      mcp_config_path TEXT,
      plugin_dir TEXT,
      version TEXT,
      latest_version TEXT,
      enabled INTEGER NOT NULL DEFAULT 1,
      last_scanned_at TEXT
    );

    CREATE TABLE IF NOT EXISTS skill_targets (
      id TEXT PRIMARY KEY,
      skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
      agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
      target_path TEXT NOT NULL,
      install_mode TEXT NOT NULL,
      actual_mode TEXT NOT NULL,
      source_hash TEXT NOT NULL,
      current_hash TEXT,
      status TEXT NOT NULL,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL,
      UNIQUE(skill_id, agent_id, target_path)
    );

    CREATE TABLE IF NOT EXISTS skill_target_claims (
      id TEXT PRIMARY KEY,
      target_id TEXT NOT NULL REFERENCES skill_targets(id) ON DELETE CASCADE,
      claim_type TEXT NOT NULL,
      pack_id TEXT,
      created_at TEXT NOT NULL,
      UNIQUE(target_id, claim_type, pack_id)
    );

    CREATE TABLE IF NOT EXISTS skill_packs (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      description TEXT NOT NULL DEFAULT '',
      tags_json TEXT NOT NULL DEFAULT '[]',
      revision INTEGER NOT NULL DEFAULT 1,
      last_sync_status TEXT NOT NULL DEFAULT 'synced',
      last_sync_error TEXT,
      last_synced_at TEXT,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS skill_pack_members (
      pack_id TEXT NOT NULL REFERENCES skill_packs(id) ON DELETE CASCADE,
      skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
      sort_order INTEGER NOT NULL DEFAULT 0,
      required INTEGER NOT NULL DEFAULT 1,
      PRIMARY KEY(pack_id, skill_id)
    );

    CREATE TABLE IF NOT EXISTS skill_pack_agent_syncs (
      pack_id TEXT NOT NULL REFERENCES skill_packs(id) ON DELETE CASCADE,
      agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
      synced_revision INTEGER NOT NULL DEFAULT 0,
      status TEXT NOT NULL DEFAULT 'synced',
      error TEXT,
      updated_at TEXT NOT NULL,
      PRIMARY KEY(pack_id, agent_id)
    );

    CREATE TABLE IF NOT EXISTS unmanaged_items (
      id TEXT PRIMARY KEY,
      item_type TEXT NOT NULL,
      agent_id TEXT,
      path TEXT NOT NULL,
      inferred_skill_id TEXT,
      hash TEXT,
      reason TEXT NOT NULL,
      first_seen_at TEXT NOT NULL,
      last_seen_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS diagnosis_issues (
      id TEXT PRIMARY KEY,
      issue_type TEXT NOT NULL,
      severity TEXT NOT NULL,
      entity_type TEXT NOT NULL,
      entity_id TEXT,
      title TEXT NOT NULL,
      detail TEXT NOT NULL,
      fix_kind TEXT NOT NULL,
      payload_json TEXT NOT NULL DEFAULT '{}',
      created_at TEXT NOT NULL,
      resolved_at TEXT
    );

    CREATE TABLE IF NOT EXISTS operations (
      id TEXT PRIMARY KEY,
      operation_type TEXT NOT NULL,
      status TEXT NOT NULL,
      preview_json TEXT NOT NULL DEFAULT '{}',
      result_json TEXT NOT NULL DEFAULT '{}',
      error TEXT,
      created_at TEXT NOT NULL,
      completed_at TEXT
    );

    CREATE TABLE IF NOT EXISTS projects (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      root_path TEXT NOT NULL UNIQUE,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL,
      last_scanned_at TEXT,
      pinned INTEGER NOT NULL DEFAULT 0
    );

    CREATE INDEX IF NOT EXISTS idx_skill_targets_agent ON skill_targets(agent_id);
    CREATE INDEX IF NOT EXISTS idx_claims_pack ON skill_target_claims(pack_id);
    CREATE INDEX IF NOT EXISTS idx_pack_agent_syncs_status ON skill_pack_agent_syncs(pack_id, status);
    CREATE INDEX IF NOT EXISTS idx_issues_type ON diagnosis_issues(issue_type, resolved_at);
    CREATE INDEX IF NOT EXISTS idx_projects_root_path ON projects(root_path);
    "#,
];

pub struct Db {
    pub conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create db dir {}: {}", parent.display(), e))?;
        }
        // Migrate from old flat location (~/.agentbro/skill-manager.db)
        let old_db = crate::data_dir::agentbro_home().join("skill-manager.db");
        crate::data_dir::migrate_sqlite(&old_db, path).map_err(|error| format!("{error:#}"))?;
        Self::open_at(path)
    }

    fn open_at(path: &Path) -> Result<Self, String> {
        let mut conn = Connection::open(path).map_err(|e| {
            format!(
                "Could not open Skill Manager database {}: {e}",
                path.display()
            )
        })?;
        initialize_schema(&mut conn).map_err(|e| {
            format!(
                "Could not initialize Skill Manager database {}: {e}",
                path.display()
            )
        })?;
        Ok(Db {
            conn: Mutex::new(conn),
        })
    }

    pub fn with_conn<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&Connection) -> Result<T, String>,
    {
        let conn = self.conn.lock().map_err(|e| format!("db lock: {}", e))?;
        f(&conn)
    }

    pub fn transaction<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T, String>,
    {
        let mut conn = self.conn.lock().map_err(|e| format!("db lock: {}", e))?;
        let tx = conn.transaction().map_err(|e| format!("begin tx: {}", e))?;
        let res = f(&tx)?;
        tx.commit().map_err(|e| format!("commit: {}", e))?;
        Ok(res)
    }

    pub fn applied_version(&self) -> Result<i64, String> {
        self.with_conn(|c| {
            c.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
        })
    }
}

fn initialize_schema(conn: &mut Connection) -> Result<(), String> {
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|e| e.to_string())?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    for sql in MIGRATIONS {
        tx.execute_batch(sql).map_err(|e| e.to_string())?;
    }
    tx.prepare(
        "SELECT id, name, description, center_path, current_hash, frontmatter_json,
                created_at, updated_at, last_scanned_at FROM skills LIMIT 0",
    )
    .map_err(|e| format!("Incompatible skills table: {e}"))?;
    ensure_column(
        &tx,
        "skills",
        "skill_type",
        "ALTER TABLE skills ADD COLUMN skill_type TEXT NOT NULL DEFAULT 'skill'",
    )?;
    ensure_skill_pack_sync_schema(&tx)?;
    tx.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
        params![SCHEMA_VERSION, now_iso()],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

fn ensure_skill_pack_sync_schema(conn: &Connection) -> Result<(), String> {
    ensure_column(
        conn,
        "skill_packs",
        "revision",
        "ALTER TABLE skill_packs ADD COLUMN revision INTEGER NOT NULL DEFAULT 1",
    )?;
    ensure_column(
        conn,
        "skill_packs",
        "last_sync_status",
        "ALTER TABLE skill_packs ADD COLUMN last_sync_status TEXT NOT NULL DEFAULT 'synced'",
    )?;
    ensure_column(
        conn,
        "skill_packs",
        "last_sync_error",
        "ALTER TABLE skill_packs ADD COLUMN last_sync_error TEXT",
    )?;
    ensure_column(
        conn,
        "skill_packs",
        "last_synced_at",
        "ALTER TABLE skill_packs ADD COLUMN last_synced_at TEXT",
    )?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS skill_pack_agent_syncs (
          pack_id TEXT NOT NULL REFERENCES skill_packs(id) ON DELETE CASCADE,
          agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
          synced_revision INTEGER NOT NULL DEFAULT 0,
          status TEXT NOT NULL DEFAULT 'synced',
          error TEXT,
          updated_at TEXT NOT NULL,
          PRIMARY KEY(pack_id, agent_id)
        );
        CREATE INDEX IF NOT EXISTS idx_pack_agent_syncs_status ON skill_pack_agent_syncs(pack_id, status);
        "#,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| e.to_string())?;
    for row in rows {
        if row.map_err(|e| e.to_string())? == column {
            return Ok(());
        }
    }
    conn.execute(alter_sql, []).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Settings (key/value in DB, with JSON file mirror) ─────────────

pub fn load_settings_json(conn: &Connection) -> serde_json::Value {
    let row: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'settings'",
            [],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    row.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

pub fn save_settings_json(conn: &Connection, value: &serde_json::Value) -> Result<(), String> {
    let s = serde_json::to_string(value).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO settings(key, value) VALUES('settings', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![s],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ── Snapshot export (raw rows) ────────────────────────────────────

pub fn export_snapshot(conn: &Connection, center_path: &str) -> Result<Snapshot, String> {
    let table_rows = |select: &str| -> Result<Vec<serde_json::Value>, String> {
        let mut stmt = conn.prepare(select).map_err(|e| e.to_string())?;
        let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt
            .query_map([], |r| {
                let mut obj = serde_json::Map::new();
                for (i, col) in cols.iter().enumerate() {
                    let val: serde_json::Value = match r.get_ref(i) {
                        Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
                        Ok(rusqlite::types::ValueRef::Integer(n)) => serde_json::json!(n),
                        Ok(rusqlite::types::ValueRef::Real(f)) => serde_json::json!(f),
                        Ok(rusqlite::types::ValueRef::Text(t)) => {
                            serde_json::Value::String(String::from_utf8_lossy(t).to_string())
                        }
                        Ok(rusqlite::types::ValueRef::Blob(b)) => {
                            serde_json::Value::String(hex::encode_simple(b))
                        }
                        Err(_) => serde_json::Value::Null,
                    };
                    obj.insert(col.clone(), val);
                }
                Ok(serde_json::Value::Object(obj))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    };

    Ok(Snapshot {
        schema_version: SCHEMA_VERSION,
        exported_at: now_iso(),
        center_path: center_path.to_string(),
        skills: table_rows("SELECT * FROM skills")?,
        sources: table_rows("SELECT * FROM skill_sources")?,
        agents: table_rows("SELECT * FROM agents")?,
        targets: table_rows("SELECT * FROM skill_targets")?,
        claims: table_rows("SELECT * FROM skill_target_claims")?,
        packs: table_rows("SELECT * FROM skill_packs")?,
        pack_members: table_rows(
            "SELECT * FROM skill_pack_members ORDER BY pack_id, sort_order, skill_id",
        )?,
        pack_agent_syncs: table_rows(
            "SELECT * FROM skill_pack_agent_syncs ORDER BY pack_id, agent_id",
        )?,
        settings: Some(
            serde_json::from_value(load_settings_json(conn)).map_err(|e| e.to_string())?,
        ),
        projects: table_rows("SELECT * FROM projects")?,
        diagnosis_summary: serde_json::json!({}),
    })
}

mod hex {
    pub fn encode_simple(b: &[u8]) -> String {
        let mut s = String::with_capacity(b.len() * 2);
        for byte in b {
            s.push_str(&format!("{:02x}", byte));
        }
        s
    }
}

// helper for migration: read legacy metadata.json
pub fn legacy_metadata_path() -> PathBuf {
    fsutil::home().join(".agentbro").join("metadata.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::installer::TempInstallDir;

    #[test]
    fn preserves_an_unreadable_database() {
        let dir = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = dir.path().join("skills.db");
        let original = b"unreadable skill database fixture";
        std::fs::write(&path, original).unwrap();

        assert!(Db::open_at(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn adds_the_skill_type_to_existing_rows_without_replacing_the_database() {
        let dir = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = dir.path().join("skills.db");
        let legacy = Connection::open(&path).unwrap();
        legacy.execute_batch(MIGRATIONS[0]).unwrap();
        legacy
            .execute_batch(
                "ALTER TABLE skills DROP COLUMN skill_type;
                 INSERT INTO skills (id, name, center_path, current_hash, created_at, updated_at)
                 VALUES ('review', 'Review', 'existing/review', 'hash', 'date', 'date');
                 INSERT INTO skill_sources (skill_id, source_type, installed_via, created_at, updated_at)
                 VALUES ('review', 'local_folder', 'manual', 'date', 'date');",
            )
            .unwrap();
        drop(legacy);

        let db = Db::open_at(&path).unwrap();
        let (name, skill_type, source): (String, String, String) = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT s.name, s.skill_type, src.source_type
                     FROM skills s JOIN skill_sources src ON s.id = src.skill_id
                     WHERE s.id = 'review'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(|error| error.to_string())
            })
            .unwrap();
        assert_eq!(
            (name.as_str(), skill_type.as_str(), source.as_str()),
            ("Review", "skill", "local_folder")
        );
        assert_eq!(db.applied_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn leaves_an_incompatible_schema_and_its_existing_rows_intact() {
        let dir = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = dir.path().join("skills.db");
        let existing = Connection::open(&path).unwrap();
        existing
            .execute_batch(
                "CREATE TABLE skills (id TEXT PRIMARY KEY);
                 INSERT INTO skills (id) VALUES ('retained');",
            )
            .unwrap();
        drop(existing);

        assert!(Db::open_at(&path).is_err());
        let existing = Connection::open(&path).unwrap();
        let id: String = existing
            .query_row("SELECT id FROM skills", [], |row| row.get(0))
            .unwrap();
        assert_eq!(id, "retained");
        let tables: i64 = existing
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 1);
    }
}
