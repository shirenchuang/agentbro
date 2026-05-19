use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::app_type::SwitchAppType;
use super::db::SwitchDatabase;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchProvider {
    pub id: String,
    pub app_type: String,
    pub name: String,
    pub settings_config: Value,
    pub website_url: Option<String>,
    pub category: Option<String>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub meta: Value,
    pub is_current: bool,
    pub in_failover_queue: bool,
    pub created_at: Option<i64>,
    pub sort_index: Option<i32>,
    pub notes: Option<String>,
}

impl SwitchProvider {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let settings_str: String = row.get("settings_config")?;
        let meta_str: String = row.get("meta")?;
        Ok(Self {
            id: row.get("id")?,
            app_type: row.get("app_type")?,
            name: row.get("name")?,
            settings_config: serde_json::from_str(&settings_str)
                .unwrap_or(Value::Object(Default::default())),
            website_url: row.get("website_url")?,
            category: row.get("category")?,
            icon: row.get("icon")?,
            icon_color: row.get("icon_color")?,
            meta: serde_json::from_str(&meta_str).unwrap_or(Value::Object(Default::default())),
            is_current: row.get("is_current")?,
            in_failover_queue: row.get("in_failover_queue")?,
            created_at: row.get("created_at")?,
            sort_index: row.get("sort_index")?,
            notes: row.get("notes")?,
        })
    }
}

pub fn list_providers(
    db: &SwitchDatabase,
    app_type: &SwitchAppType,
) -> anyhow::Result<Vec<SwitchProvider>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT * FROM providers WHERE app_type = ?1 ORDER BY sort_index ASC, created_at ASC",
        )?;
        let rows = stmt
            .query_map(params![app_type.as_str()], SwitchProvider::from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn get_provider(
    db: &SwitchDatabase,
    app_type: &SwitchAppType,
    id: &str,
) -> anyhow::Result<Option<SwitchProvider>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT * FROM providers WHERE id = ?1 AND app_type = ?2")?;
        let mut rows = stmt.query_map(params![id, app_type.as_str()], SwitchProvider::from_row)?;
        match rows.next() {
            Some(Ok(p)) => Ok(Some(p)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    })
}

pub fn create_provider(db: &SwitchDatabase, provider: &SwitchProvider) -> anyhow::Result<()> {
    db.with_conn(|conn| {
        insert_provider(conn, provider)?;
        Ok(())
    })
}

pub fn insert_provider(conn: &Connection, p: &SwitchProvider) -> anyhow::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO providers (id, app_type, name, settings_config, website_url, category, icon, icon_color, meta, is_current, in_failover_queue, created_at, sort_index, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            p.id,
            p.app_type,
            p.name,
            serde_json::to_string(&p.settings_config)?,
            p.website_url,
            p.category,
            p.icon,
            p.icon_color,
            serde_json::to_string(&p.meta)?,
            p.is_current,
            p.in_failover_queue,
            p.created_at,
            p.sort_index,
            p.notes,
        ],
    )?;
    Ok(())
}

pub fn update_provider(db: &SwitchDatabase, provider: &SwitchProvider) -> anyhow::Result<()> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE providers SET name=?3, settings_config=?4, website_url=?5, category=?6, icon=?7, icon_color=?8, meta=?9, is_current=?10, in_failover_queue=?11, sort_index=?12, notes=?13
             WHERE id=?1 AND app_type=?2",
            params![
                provider.id,
                provider.app_type,
                provider.name,
                serde_json::to_string(&provider.settings_config)?,
                provider.website_url,
                provider.category,
                provider.icon,
                provider.icon_color,
                serde_json::to_string(&provider.meta)?,
                provider.is_current,
                provider.in_failover_queue,
                provider.sort_index,
                provider.notes,
            ],
        )?;
        Ok(())
    })
}

pub fn delete_provider(
    db: &SwitchDatabase,
    app_type: &SwitchAppType,
    id: &str,
) -> anyhow::Result<()> {
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM providers WHERE id = ?1 AND app_type = ?2",
            params![id, app_type.as_str()],
        )?;
        Ok(())
    })
}

pub fn set_current(db: &SwitchDatabase, app_type: &SwitchAppType, id: &str) -> anyhow::Result<()> {
    db.with_conn(|conn| {
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE providers SET is_current = 0 WHERE app_type = ?1",
            params![app_type.as_str()],
        )?;
        let affected = tx.execute(
            "UPDATE providers SET is_current = 1 WHERE id = ?1 AND app_type = ?2",
            params![id, app_type.as_str()],
        )?;
        if affected == 0 {
            anyhow::bail!("provider not found: {id}");
        }
        tx.commit()?;
        Ok(())
    })
}

pub fn duplicate_provider(
    db: &SwitchDatabase,
    app_type: &SwitchAppType,
    id: &str,
) -> anyhow::Result<SwitchProvider> {
    let original = get_provider(db, app_type, id)?
        .ok_or_else(|| anyhow::anyhow!("provider not found: {id}"))?;
    let mut dup = original;
    dup.id = uuid::Uuid::new_v4().to_string();
    dup.name = format!("{} (副本)", dup.name);
    dup.is_current = false;
    dup.created_at = Some(chrono::Utc::now().timestamp());
    create_provider(db, &dup)?;
    Ok(dup)
}

pub fn get_current(
    db: &SwitchDatabase,
    app_type: &SwitchAppType,
) -> anyhow::Result<Option<SwitchProvider>> {
    db.with_conn(|conn| {
        let mut stmt =
            conn.prepare("SELECT * FROM providers WHERE app_type = ?1 AND is_current = 1 LIMIT 1")?;
        let mut rows = stmt.query_map(params![app_type.as_str()], SwitchProvider::from_row)?;
        match rows.next() {
            Some(Ok(p)) => Ok(Some(p)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    })
}
