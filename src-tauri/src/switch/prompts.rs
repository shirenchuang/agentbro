use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::app_type::SwitchAppType;
use super::db::SwitchDatabase;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchPrompt {
    pub id: String,
    pub app_type: String,
    pub name: String,
    pub content: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub sort_index: Option<i32>,
}

impl SwitchPrompt {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            app_type: row.get("app_type")?,
            name: row.get("name")?,
            content: row.get("content")?,
            description: row.get("description")?,
            enabled: row.get("enabled")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            sort_index: row.get("sort_index")?,
        })
    }
}

pub fn list_prompts(
    db: &SwitchDatabase,
    app_type: &SwitchAppType,
) -> anyhow::Result<Vec<SwitchPrompt>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT * FROM prompts WHERE app_type = ?1 ORDER BY sort_index ASC, created_at ASC",
        )?;
        let rows = stmt
            .query_map(params![app_type.as_str()], SwitchPrompt::from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn create_prompt(db: &SwitchDatabase, prompt: &SwitchPrompt) -> anyhow::Result<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO prompts (id, app_type, name, content, description, enabled, created_at, updated_at, sort_index)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                prompt.id,
                prompt.app_type,
                prompt.name,
                prompt.content,
                prompt.description,
                prompt.enabled,
                prompt.created_at,
                prompt.updated_at,
                prompt.sort_index,
            ],
        )?;
        Ok(())
    })
}

pub fn update_prompt(db: &SwitchDatabase, prompt: &SwitchPrompt) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE prompts SET name=?3, content=?4, description=?5, enabled=?6, updated_at=?7, sort_index=?8
             WHERE id=?1 AND app_type=?2",
            params![
                prompt.id,
                prompt.app_type,
                prompt.name,
                prompt.content,
                prompt.description,
                prompt.enabled,
                now,
                prompt.sort_index,
            ],
        )?;
        Ok(())
    })
}

pub fn delete_prompt(
    db: &SwitchDatabase,
    id: &str,
    app_type: &SwitchAppType,
) -> anyhow::Result<()> {
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM prompts WHERE id = ?1 AND app_type = ?2",
            params![id, app_type.as_str()],
        )?;
        Ok(())
    })
}

pub fn toggle_prompt(
    db: &SwitchDatabase,
    id: &str,
    app_type: &SwitchAppType,
) -> anyhow::Result<()> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE prompts SET enabled = NOT enabled, updated_at = ?3 WHERE id = ?1 AND app_type = ?2",
            params![id, app_type.as_str(), chrono::Utc::now().timestamp()],
        )?;
        Ok(())
    })
}

fn home() -> anyhow::Result<std::path::PathBuf> {
    dirs::home_dir().ok_or_else(|| anyhow::anyhow!("home directory not found"))
}

pub fn apply_prompts(db: &SwitchDatabase, app_type: &SwitchAppType) -> anyhow::Result<()> {
    let prompts = list_prompts(db, app_type)?;
    let enabled: Vec<&SwitchPrompt> = prompts.iter().filter(|p| p.enabled).collect();

    match app_type {
        SwitchAppType::Claude => apply_claude_prompts(&enabled),
        _ => Ok(()),
    }
}

fn apply_claude_prompts(prompts: &[&SwitchPrompt]) -> anyhow::Result<()> {
    let claude_md_path = home()?.join(".claude").join("CLAUDE.md");
    if prompts.is_empty() {
        return Ok(());
    }

    let marker_start = "<!-- AgentBro Switch Prompts Start -->";
    let marker_end = "<!-- AgentBro Switch Prompts End -->";

    let existing = if claude_md_path.exists() {
        std::fs::read_to_string(&claude_md_path)?
    } else {
        String::new()
    };

    let before = if let Some(idx) = existing.find(marker_start) {
        &existing[..idx]
    } else {
        &existing
    };

    let after = if let Some(idx) = existing.find(marker_end) {
        &existing[idx + marker_end.len()..]
    } else {
        ""
    };

    let mut managed = String::new();
    managed.push_str(marker_start);
    managed.push('\n');
    for p in prompts {
        managed.push_str(&format!("\n## {}\n\n{}\n", p.name, p.content));
    }
    managed.push_str(marker_end);

    let result = format!(
        "{}{}{}",
        before.trim_end(),
        if before.trim_end().is_empty() {
            ""
        } else {
            "\n\n"
        },
        format!("{}{}", managed, after)
    );

    if let Some(parent) = claude_md_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&claude_md_path, result.trim())?;
    Ok(())
}
