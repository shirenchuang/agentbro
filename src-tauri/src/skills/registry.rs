use super::agent_paths;
use super::{MarketplaceSource, SkillPack, SyncConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillSourceEntry {
    pub origin: String,
    pub installed_via: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(default)]
    pub sources: HashMap<String, SkillSourceEntry>,
    #[serde(default)]
    pub packs: Vec<SkillPack>,
    #[serde(default)]
    pub sync: Option<SyncConfig>,
    #[serde(default)]
    pub custom_agents: Vec<CustomAgentEntry>,
    #[serde(default)]
    pub marketplace_sources: Vec<MarketplaceSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomAgentEntry {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub global_skills_dir: String,
    pub icon_name: Option<String>,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomAgentConfig {
    pub id: Option<String>,
    pub display_name: String,
    pub category: Option<String>,
    pub global_skills_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCustomAgentConfig {
    pub display_name: String,
    pub category: Option<String>,
    pub global_skills_dir: String,
}

pub fn load() -> Metadata {
    let path = agent_paths::agentbro_metadata_path();
    fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

pub fn save(metadata: &Metadata) -> Result<(), String> {
    let path = agent_paths::agentbro_metadata_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(metadata).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())
}

pub fn add_source(skill_id: &str, origin: &str) -> Result<(), String> {
    let mut meta = load();
    meta.sources.insert(
        skill_id.to_string(),
        SkillSourceEntry {
            origin: origin.to_string(),
            installed_via: "island".to_string(),
        },
    );
    save(&meta)
}

pub fn remove_source(skill_id: &str) -> Result<(), String> {
    let mut meta = load();
    meta.sources.remove(skill_id);
    save(&meta)
}

pub fn list_packs() -> Vec<SkillPack> {
    load().packs
}

pub fn create_pack(pack: SkillPack) -> Result<(), String> {
    let mut meta = load();
    meta.packs.push(pack);
    save(&meta)
}

pub fn update_pack(pack: SkillPack) -> Result<(), String> {
    let mut meta = load();
    let existing = meta
        .packs
        .iter_mut()
        .find(|p| p.id == pack.id)
        .ok_or_else(|| format!("Pack not found: {}", pack.id))?;
    *existing = pack;
    save(&meta)
}

pub fn delete_pack(id: &str) -> Result<(), String> {
    let mut meta = load();
    meta.packs.retain(|p| p.id != id);
    save(&meta)
}

pub fn get_sync_config() -> Option<SyncConfig> {
    load().sync
}

pub fn set_sync_config(config: SyncConfig) -> Result<(), String> {
    let mut meta = load();
    meta.sync = Some(config);
    save(&meta)
}

pub fn list_marketplace_sources() -> Vec<MarketplaceSource> {
    load().marketplace_sources
}

pub fn upsert_marketplace_source(source: MarketplaceSource) -> Result<(), String> {
    if source.id.trim().is_empty() {
        return Err("Marketplace source id cannot be empty".to_string());
    }
    if source.url.trim().is_empty() {
        return Err("Marketplace source url cannot be empty".to_string());
    }
    let mut meta = load();
    if let Some(existing) = meta
        .marketplace_sources
        .iter_mut()
        .find(|entry| entry.id == source.id)
    {
        *existing = source;
    } else {
        meta.marketplace_sources.push(source);
    }
    save(&meta)
}

pub fn remove_marketplace_source(id: &str) -> Result<(), String> {
    let mut meta = load();
    let before = meta.marketplace_sources.len();
    meta.marketplace_sources.retain(|entry| entry.id != id);
    if before == meta.marketplace_sources.len() {
        return Err(format!("Marketplace source not found: {id}"));
    }
    save(&meta)
}

pub fn list_custom_agents() -> Vec<CustomAgentEntry> {
    load().custom_agents
}

pub fn add_custom_agent(config: CustomAgentConfig) -> Result<CustomAgentEntry, String> {
    if config.display_name.trim().is_empty() {
        return Err("Agent display name cannot be empty".to_string());
    }
    if config.global_skills_dir.trim().is_empty() {
        return Err("Agent skills directory cannot be empty".to_string());
    }

    let mut meta = load();
    let id = config
        .id
        .as_deref()
        .map(normalize_agent_id)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("custom-{}", normalize_agent_id(&config.display_name)));

    if id == "custom-" {
        return Err("Agent ID cannot be empty".to_string());
    }
    if agent_paths::known_agent_ids().contains(&id.as_str())
        || meta.custom_agents.iter().any(|agent| agent.id == id)
    {
        return Err(format!("Agent already exists: {id}"));
    }

    let entry = CustomAgentEntry {
        id,
        display_name: config.display_name.trim().to_string(),
        category: config.category.unwrap_or_else(|| "custom".to_string()),
        global_skills_dir: normalize_path(&config.global_skills_dir),
        icon_name: None,
        is_enabled: true,
    };
    meta.custom_agents.push(entry.clone());
    save(&meta)?;
    Ok(entry)
}

pub fn update_custom_agent(
    agent_id: &str,
    config: UpdateCustomAgentConfig,
) -> Result<CustomAgentEntry, String> {
    if config.display_name.trim().is_empty() {
        return Err("Agent display name cannot be empty".to_string());
    }
    if config.global_skills_dir.trim().is_empty() {
        return Err("Agent skills directory cannot be empty".to_string());
    }

    let mut meta = load();
    let entry = meta
        .custom_agents
        .iter_mut()
        .find(|agent| agent.id == agent_id)
        .ok_or_else(|| format!("Custom agent not found: {agent_id}"))?;

    entry.display_name = config.display_name.trim().to_string();
    entry.category = config.category.unwrap_or_else(|| "custom".to_string());
    entry.global_skills_dir = normalize_path(&config.global_skills_dir);
    let updated = entry.clone();
    save(&meta)?;
    Ok(updated)
}

pub fn remove_custom_agent(agent_id: &str) -> Result<(), String> {
    let mut meta = load();
    let before = meta.custom_agents.len();
    meta.custom_agents.retain(|agent| agent.id != agent_id);
    if meta.custom_agents.len() == before {
        return Err(format!("Custom agent not found: {agent_id}"));
    }
    save(&meta)
}

fn normalize_agent_id(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with("~/") {
        return expand_home(trimmed).display().to_string();
    }
    trimmed.to_string()
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    Path::new(path).to_path_buf()
}
