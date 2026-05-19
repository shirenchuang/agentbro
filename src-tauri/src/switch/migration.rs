use std::path::PathBuf;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::app_type::SwitchAppType;
use super::db::SwitchDatabase;
use super::providers::{insert_provider, SwitchProvider};

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportPreview {
    pub providers: usize,
    pub provider_endpoints: usize,
    pub mcp_servers: usize,
    pub prompts: usize,
    pub skills: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportResult {
    pub providers_imported: usize,
    pub provider_endpoints_imported: usize,
    pub mcp_servers_imported: usize,
    pub prompts_imported: usize,
    pub skills_imported: usize,
}

fn cc_switch_db_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("home directory not found"))?;
    Ok(home.join(".cc-switch").join("cc-switch.db"))
}

pub fn detect_cc_switch() -> bool {
    cc_switch_db_path().map(|p| p.exists()).unwrap_or(false)
}

pub fn import_preview() -> anyhow::Result<ImportPreview> {
    let path = cc_switch_db_path()?;
    if !path.exists() {
        anyhow::bail!("CC Switch database not found at {}", path.display());
    }
    let conn = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    Ok(ImportPreview {
        providers: safe_count(&conn, "providers"),
        provider_endpoints: safe_count(&conn, "provider_endpoints"),
        mcp_servers: safe_count(&conn, "mcp_servers"),
        prompts: safe_count(&conn, "prompts"),
        skills: safe_count(&conn, "skills"),
    })
}

fn safe_count(conn: &Connection, table: &str) -> usize {
    const ALLOWED: &[&str] = &[
        "providers",
        "provider_endpoints",
        "mcp_servers",
        "prompts",
        "skills",
    ];
    if !ALLOWED.contains(&table) {
        return 0;
    }
    let sql = format!("SELECT COUNT(*) FROM {table}");
    conn.query_row(&sql, [], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize
}

pub fn import_from_cc_switch(db: &SwitchDatabase) -> anyhow::Result<ImportResult> {
    let path = cc_switch_db_path()?;
    if !path.exists() {
        anyhow::bail!("CC Switch database not found at {}", path.display());
    }
    let src = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut result = ImportResult {
        providers_imported: 0,
        provider_endpoints_imported: 0,
        mcp_servers_imported: 0,
        prompts_imported: 0,
        skills_imported: 0,
    };

    result.providers_imported = import_providers(&src, db)?;
    result.provider_endpoints_imported = import_provider_endpoints(&src, db)?;
    result.mcp_servers_imported = import_mcp_servers(&src, db)?;
    result.prompts_imported = import_prompts(&src, db)?;
    result.skills_imported = import_skills(&src, db)?;

    Ok(result)
}

fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let rows = match stmt.query_map([], |row| row.get::<_, String>(1)) {
        Ok(r) => r,
        Err(_) => return false,
    };
    for name in rows.flatten() {
        if name == column {
            return true;
        }
    }
    false
}

fn import_providers(src: &Connection, db: &SwitchDatabase) -> anyhow::Result<usize> {
    let has_cost = has_column(src, "providers", "cost_multiplier");
    let has_type = has_column(src, "providers", "provider_type");
    let has_daily = has_column(src, "providers", "limit_daily_usd");
    let has_monthly = has_column(src, "providers", "limit_monthly_usd");

    let mut stmt = src.prepare(
        "SELECT id, app_type, name, settings_config, website_url, category, icon, icon_color, meta, is_current, in_failover_queue, created_at, sort_index, notes FROM providers",
    )?;

    let mut count = 0usize;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let raw_type: String = row.get("app_type")?;
        let mapped_type = match SwitchAppType::from_cc_switch(&raw_type) {
            Some(t) => t,
            None => continue,
        };

        let id: String = row.get("id")?;
        let cost_multiplier = if has_cost {
            opt_col_str(src, "providers", "cost_multiplier", &raw_type, &id)
        } else {
            None
        };
        let provider_type = if has_type {
            opt_col_str(src, "providers", "provider_type", &raw_type, &id)
        } else {
            None
        };
        let limit_daily = if has_daily {
            opt_col_str(src, "providers", "limit_daily_usd", &raw_type, &id)
        } else {
            None
        };
        let limit_monthly = if has_monthly {
            opt_col_str(src, "providers", "limit_monthly_usd", &raw_type, &id)
        } else {
            None
        };

        let sc: String = row.get("settings_config")?;
        let meta_str: String = row.get("meta")?;
        let mut meta: serde_json::Value = serde_json::from_str(&meta_str).unwrap_or_default();
        if let Some(obj) = meta.as_object_mut() {
            if let Some(v) = &cost_multiplier {
                obj.insert(
                    "cost_multiplier".into(),
                    serde_json::Value::String(v.clone()),
                );
            }
            if let Some(v) = &provider_type {
                obj.insert("provider_type".into(), serde_json::Value::String(v.clone()));
            }
            if let Some(v) = &limit_daily {
                obj.insert(
                    "limit_daily_usd".into(),
                    serde_json::Value::String(v.clone()),
                );
            }
            if let Some(v) = &limit_monthly {
                obj.insert(
                    "limit_monthly_usd".into(),
                    serde_json::Value::String(v.clone()),
                );
            }
        }

        let provider = SwitchProvider {
            id,
            app_type: mapped_type.as_str().to_string(),
            name: row.get("name")?,
            settings_config: serde_json::from_str(&sc).unwrap_or_default(),
            website_url: row.get("website_url")?,
            category: row.get("category")?,
            icon: row.get("icon")?,
            icon_color: row.get("icon_color")?,
            meta,
            is_current: row.get("is_current")?,
            in_failover_queue: row.get("in_failover_queue")?,
            created_at: row.get("created_at")?,
            sort_index: row.get("sort_index")?,
            notes: row.get("notes")?,
        };
        db.with_conn(|conn| {
            insert_provider(conn, &provider)?;
            Ok(())
        })?;
        count += 1;
    }
    Ok(count)
}

fn opt_col_str(
    conn: &Connection,
    table: &str,
    col: &str,
    app_type: &str,
    id: &str,
) -> Option<String> {
    let sql = format!("SELECT {col} FROM {table} WHERE id = ?1 AND app_type = ?2");
    conn.query_row(&sql, rusqlite::params![id, app_type], |row| row.get(0))
        .ok()
}

fn import_provider_endpoints(src: &Connection, db: &SwitchDatabase) -> anyhow::Result<usize> {
    if !has_column(src, "provider_endpoints", "id") {
        return Ok(0);
    }
    let mut stmt =
        src.prepare("SELECT provider_id, app_type, url, added_at FROM provider_endpoints")?;
    let mut count = 0usize;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let raw_type: String = row.get::<_, String>(1)?;
        let mapped = match SwitchAppType::from_cc_switch(&raw_type) {
            Some(t) => t.as_str().to_string(),
            None => continue,
        };
        let provider_id: String = row.get(0)?;
        let url: String = row.get(2)?;
        let added_at: Option<i64> = row.get(3)?;
        db.with_conn(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO provider_endpoints (provider_id, app_type, url, added_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![provider_id, mapped, url, added_at],
            )?;
            Ok(())
        })?;
        count += 1;
    }
    Ok(count)
}

fn import_mcp_servers(src: &Connection, db: &SwitchDatabase) -> anyhow::Result<usize> {
    let has_docs = has_column(src, "mcp_servers", "docs");
    let has_tags = has_column(src, "mcp_servers", "tags");

    let mut stmt = src.prepare(
        "SELECT id, name, server_config, description, homepage, enabled_claude, enabled_codex, enabled_gemini, enabled_opencode, enabled_hermes FROM mcp_servers",
    )?;

    let mut count = 0usize;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let docs: Option<String> = if has_docs {
            src.query_row("SELECT docs FROM mcp_servers WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .ok()
        } else {
            None
        };
        let tags: String = if has_tags {
            src.query_row("SELECT tags FROM mcp_servers WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .unwrap_or_else(|_| "[]".to_string())
        } else {
            "[]".to_string()
        };

        db.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO mcp_servers (id, name, server_config, description, homepage, docs, tags, enabled_claude, enabled_codex, enabled_gemini, enabled_opencode, enabled_hermes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params![
                    id,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    docs,
                    tags,
                    row.get::<_, bool>(5)?,
                    row.get::<_, bool>(6)?,
                    row.get::<_, bool>(7)?,
                    row.get::<_, bool>(8)?,
                    row.get::<_, bool>(9)?,
                ],
            )?;
            Ok(())
        })?;
        count += 1;
    }
    Ok(count)
}

fn import_prompts(src: &Connection, db: &SwitchDatabase) -> anyhow::Result<usize> {
    let has_updated = has_column(src, "prompts", "updated_at");

    let mut stmt = src.prepare(
        "SELECT id, app_type, name, content, description, enabled, created_at FROM prompts",
    )?;

    let mut count = 0usize;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let raw_type: String = row.get(1)?;
        let mapped = match SwitchAppType::from_cc_switch(&raw_type) {
            Some(t) => t.as_str().to_string(),
            None => continue,
        };
        let updated_at: Option<i64> = if has_updated {
            src.query_row(
                "SELECT updated_at FROM prompts WHERE id = ?1 AND app_type = ?2",
                rusqlite::params![&id, &raw_type],
                |r| r.get(0),
            )
            .ok()
        } else {
            None
        };

        db.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO prompts (id, app_type, name, content, description, enabled, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    id,
                    mapped,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    updated_at,
                ],
            )?;
            Ok(())
        })?;
        count += 1;
    }
    Ok(count)
}

fn import_skills(src: &Connection, db: &SwitchDatabase) -> anyhow::Result<usize> {
    let has_desc = has_column(src, "skills", "description");
    let has_branch = has_column(src, "skills", "repo_branch");
    let has_readme = has_column(src, "skills", "readme_url");
    let has_installed = has_column(src, "skills", "installed_at");
    let has_hash = has_column(src, "skills", "content_hash");
    let has_updated = has_column(src, "skills", "updated_at");

    let mut stmt = src.prepare(
        "SELECT id, name, directory, repo_owner, repo_name, enabled_claude, enabled_codex, enabled_gemini, enabled_opencode, enabled_hermes FROM skills",
    )?;

    let mut count = 0usize;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;

        let description: Option<String> = if has_desc {
            src.query_row("SELECT description FROM skills WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .ok()
            .flatten()
        } else {
            None
        };
        let repo_branch: Option<String> = if has_branch {
            src.query_row("SELECT repo_branch FROM skills WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .ok()
            .flatten()
        } else {
            None
        };
        let readme_url: Option<String> = if has_readme {
            src.query_row("SELECT readme_url FROM skills WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .ok()
            .flatten()
        } else {
            None
        };
        let installed_at: i64 = if has_installed {
            src.query_row(
                "SELECT installed_at FROM skills WHERE id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap_or(0)
        } else {
            0
        };
        let content_hash: Option<String> = if has_hash {
            src.query_row(
                "SELECT content_hash FROM skills WHERE id = ?1",
                [&id],
                |r| r.get(0),
            )
            .ok()
            .flatten()
        } else {
            None
        };
        let updated_at: i64 = if has_updated {
            src.query_row("SELECT updated_at FROM skills WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .unwrap_or(0)
        } else {
            0
        };

        db.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO skills (id, name, description, directory, repo_owner, repo_name, repo_branch, readme_url, enabled_claude, enabled_codex, enabled_gemini, enabled_opencode, enabled_hermes, installed_at, content_hash, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                rusqlite::params![
                    id,
                    row.get::<_, String>(1)?,
                    description,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    repo_branch,
                    readme_url,
                    row.get::<_, bool>(5)?,
                    row.get::<_, bool>(6)?,
                    row.get::<_, bool>(7)?,
                    row.get::<_, bool>(8)?,
                    row.get::<_, bool>(9)?,
                    installed_at,
                    content_hash,
                    updated_at,
                ],
            )?;
            Ok(())
        })?;
        count += 1;
    }
    Ok(count)
}
