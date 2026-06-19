//! SQLite storage for Skill Manager v2 — schema, migrations, and typed row
//! access. The connection lives in a `Mutex<Connection>` held by `Service`.

use crate::skills::v2::fsutil;
use crate::skills::v2::models::{Snapshot, SCHEMA_VERSION};
use rusqlite::{params, Connection, OptionalExtension};
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
        crate::data_dir::migrate_sqlite(&old_db, path);
        // Detect incompatible legacy schema and nuke the file if needed.
        if path.exists() {
            if let Ok(probe) = Connection::open(path) {
                let is_legacy: bool = probe
                    .prepare("SELECT skill_type FROM skills LIMIT 0")
                    .is_err();
                drop(probe);
                if is_legacy {
                    let _ = std::fs::remove_file(path);
                    let _ = std::fs::remove_file(path.with_extension("db-wal"));
                    let _ = std::fs::remove_file(path.with_extension("db-shm"));
                }
            }
        }
        let conn = Connection::open(path).map_err(|e| format!("open db: {}", e))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| format!("pragma: {}", e))?;
        for sql in MIGRATIONS {
            conn.execute_batch(sql)
                .map_err(|e| format!("migration: {}", e))?;
        }
        // record applied version (idempotent)
        let now = now_iso();
        conn.execute(
            "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
            params![SCHEMA_VERSION, now],
        )
        .map_err(|e| format!("record migration: {}", e))?;
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
    let table_rows = |table: &str| -> Result<Vec<serde_json::Value>, String> {
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM {table}"))
            .map_err(|e| e.to_string())?;
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
        skills: table_rows("skills")?,
        sources: table_rows("skill_sources")?,
        agents: table_rows("agents")?,
        targets: table_rows("skill_targets")?,
        claims: table_rows("skill_target_claims")?,
        packs: table_rows("skill_packs")?,
        projects: table_rows("projects")?,
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
