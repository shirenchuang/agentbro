use super::agent_paths;
use super::{
    CollectionExport, DiscoveredSkill, MarketplaceSource, ScanRoot, SkillCollection, SkillPack,
    SyncConfig,
};
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
    pub collections: Vec<SkillCollection>,
    #[serde(default)]
    pub scan_roots: Vec<ScanRoot>,
    #[serde(default)]
    pub discovered_skills: Vec<DiscoveredSkill>,
    #[serde(default)]
    pub discovered_scanned_at: Option<String>,
    #[serde(default)]
    pub sync: Option<SyncConfig>,
    #[serde(default)]
    pub custom_agents: Vec<CustomAgentEntry>,
    #[serde(default)]
    pub marketplace_sources: Vec<MarketplaceSource>,
    #[serde(default)]
    pub explanations: HashMap<String, SkillExplanationEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillExplanationEntry {
    pub skill_id: String,
    pub lang: String,
    pub model: String,
    pub text: String,
    pub cached_at: String,
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

pub fn get_skill_explanation(skill_id: &str, lang: &str) -> Option<SkillExplanationEntry> {
    load()
        .explanations
        .get(&explanation_key(skill_id, lang))
        .cloned()
}

pub fn cache_skill_explanation(entry: SkillExplanationEntry) -> Result<(), String> {
    let mut meta = load();
    meta.explanations
        .insert(explanation_key(&entry.skill_id, &entry.lang), entry);
    save(&meta)
}

fn explanation_key(skill_id: &str, lang: &str) -> String {
    format!("{}::{}", skill_id, lang)
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

pub fn list_collections() -> Vec<SkillCollection> {
    let mut collections = load().collections;
    collections.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    collections
}

pub fn upsert_collection(mut collection: SkillCollection) -> Result<SkillCollection, String> {
    if collection.name.trim().is_empty() {
        return Err("Collection name cannot be empty".to_string());
    }
    collection.name = collection.name.trim().to_string();
    collection.description = collection.description.trim().to_string();
    collection.skills.sort();
    collection.skills.dedup();

    let now = chrono::Utc::now().to_rfc3339();
    let mut meta = load();
    if let Some(existing) = meta
        .collections
        .iter_mut()
        .find(|item| item.id == collection.id)
    {
        if collection.created_at.trim().is_empty() {
            collection.created_at = existing.created_at.clone();
        }
        collection.updated_at = now;
        *existing = collection.clone();
    } else {
        if collection.id.trim().is_empty() {
            collection.id = format!("collection-{}", uuid::Uuid::new_v4());
        }
        if collection.created_at.trim().is_empty() {
            collection.created_at = now.clone();
        }
        if collection.updated_at.trim().is_empty() {
            collection.updated_at = now;
        }
        meta.collections.push(collection.clone());
    }
    save(&meta)?;
    Ok(collection)
}

pub fn delete_collection(id: &str) -> Result<(), String> {
    let mut meta = load();
    let before = meta.collections.len();
    meta.collections.retain(|collection| collection.id != id);
    if before == meta.collections.len() {
        return Err(format!("Collection not found: {id}"));
    }
    save(&meta)
}

pub fn export_collection(id: &str) -> Result<String, String> {
    let collection = load()
        .collections
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| format!("Collection not found: {id}"))?;
    serde_json::to_string_pretty(&CollectionExport {
        schema_version: 1,
        collection,
    })
    .map_err(|e| e.to_string())
}

pub fn import_collection(json: &str) -> Result<SkillCollection, String> {
    let parsed: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let collection_value = parsed
        .get("collection")
        .cloned()
        .unwrap_or_else(|| parsed.clone());
    let mut collection: SkillCollection =
        serde_json::from_value(collection_value).map_err(|e| e.to_string())?;
    if collection.id.trim().is_empty() {
        collection.id = format!("collection-{}", uuid::Uuid::new_v4());
    }
    let existing_ids = load()
        .collections
        .into_iter()
        .map(|item| item.id)
        .collect::<std::collections::HashSet<_>>();
    if existing_ids.contains(&collection.id) {
        collection.id = format!(
            "{}-imported-{}",
            collection.id,
            chrono::Utc::now().timestamp()
        );
        collection.name = format!("{} (Imported)", collection.name);
    }
    upsert_collection(collection)
}

pub fn list_scan_roots() -> Vec<ScanRoot> {
    let roots = load().scan_roots;
    if roots.is_empty() {
        default_scan_roots()
    } else {
        roots
    }
}

pub fn set_scan_roots(mut roots: Vec<ScanRoot>) -> Result<(), String> {
    for root in &mut roots {
        root.path = normalize_path(&root.path);
        if root.label.trim().is_empty() {
            root.label = label_for_path(&root.path);
        }
    }
    roots.sort_by(|a, b| a.path.cmp(&b.path));
    roots.dedup_by(|a, b| a.path == b.path);
    let mut meta = load();
    meta.scan_roots = roots;
    save(&meta)
}

pub fn set_scan_root_enabled(path: &str, enabled: bool) -> Result<(), String> {
    let normalized = normalize_path(path);
    let mut roots = list_scan_roots();
    let root = roots
        .iter_mut()
        .find(|root| root.path == normalized)
        .ok_or_else(|| format!("Scan root not found: {path}"))?;
    root.enabled = enabled;
    set_scan_roots(roots)
}

pub fn cache_discovered_skills(
    skills: Vec<DiscoveredSkill>,
) -> Result<Vec<DiscoveredSkill>, String> {
    let mut meta = load();
    meta.discovered_skills = skills;
    meta.discovered_scanned_at = Some(chrono::Utc::now().to_rfc3339());
    let cached = meta.discovered_skills.clone();
    save(&meta)?;
    Ok(cached)
}

pub fn list_discovered_skills() -> Vec<DiscoveredSkill> {
    load().discovered_skills
}

pub fn clear_discovered_skills() -> Result<(), String> {
    let mut meta = load();
    meta.discovered_skills.clear();
    meta.discovered_scanned_at = None;
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

fn default_scan_roots() -> Vec<ScanRoot> {
    ["~/code", "~/projects", "~/workspace", "~/Documents"]
        .iter()
        .map(|path| ScanRoot {
            path: normalize_path(path),
            enabled: true,
            label: label_for_path(path),
        })
        .collect()
}

fn label_for_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(path)
        .to_string()
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    Path::new(path).to_path_buf()
}
