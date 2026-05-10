use std::fs;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use super::{SkillPack, SyncConfig};
use super::agent_paths;

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
    meta.sources.insert(skill_id.to_string(), SkillSourceEntry {
        origin: origin.to_string(),
        installed_via: "island".to_string(),
    });
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
    if let Some(existing) = meta.packs.iter_mut().find(|p| p.id == pack.id) {
        *existing = pack;
    }
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
