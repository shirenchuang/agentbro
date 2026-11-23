use super::agent_paths;
use super::{
    CollectionExport, DiscoveredSkill, MarketplaceSource, ScanRoot, SkillCollection, SkillPack,
    SyncConfig,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::ops::{Deref, DerefMut};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    #[serde(default)]
    pub config_dir: Option<String>,
    #[serde(default)]
    pub settings_file: Option<String>,
    #[serde(default)]
    pub mcp_config: Option<String>,
    #[serde(default)]
    pub plugin_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomAgentConfig {
    pub id: Option<String>,
    pub display_name: String,
    pub category: Option<String>,
    pub global_skills_dir: String,
    #[serde(default)]
    pub icon_name: Option<String>,
    #[serde(default)]
    pub config_dir: Option<String>,
    #[serde(default)]
    pub settings_file: Option<String>,
    #[serde(default)]
    pub mcp_config: Option<String>,
    #[serde(default)]
    pub plugin_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCustomAgentConfig {
    pub display_name: String,
    pub category: Option<String>,
    pub global_skills_dir: String,
    #[serde(default)]
    pub icon_name: Option<String>,
    #[serde(default)]
    pub config_dir: Option<String>,
    #[serde(default)]
    pub settings_file: Option<String>,
    #[serde(default)]
    pub mcp_config: Option<String>,
    #[serde(default)]
    pub plugin_dir: Option<String>,
}

pub fn load() -> Metadata {
    read_metadata().unwrap_or_default()
}

pub(crate) fn read_metadata() -> Result<Metadata, String> {
    read_at(&agent_paths::agentbro_metadata_path())
}

fn read_at(path: &Path) -> Result<Metadata, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "Invalid AgentBro metadata at {} (line {}, column {})",
                path.display(),
                error.line(),
                error.column(),
            )
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Metadata::default()),
        Err(error) => Err(format!(
            "Read AgentBro metadata {}: {error}",
            path.display()
        )),
    }
}

pub(crate) fn edit() -> Result<MetadataEdit, String> {
    MetadataEdit::open(agent_paths::agentbro_metadata_path())
}

pub(crate) struct MetadataEdit {
    path: PathBuf,
    metadata: Metadata,
    _lock: fs::File,
}

impl MetadataEdit {
    fn open(path: PathBuf) -> Result<Self, String> {
        let path = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_symlink() => path.canonicalize().map_err(|error| {
                format!("Resolve AgentBro metadata {}: {error}", path.display())
            })?,
            _ => path,
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        // Keep the lock on a sibling whose identity survives replacing the JSON file.
        let mut lock_path = path.as_os_str().to_os_string();
        lock_path.push(".lock");
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path)
            .and_then(|file| {
                file.lock()?;
                Ok(file)
            })
            .map_err(|error| format!("Lock AgentBro metadata {}: {error}", path.display()))?;
        let metadata = read_at(&path)?;
        Ok(Self {
            path,
            metadata,
            _lock: lock,
        })
    }

    pub(crate) fn save(self) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(&self.metadata).map_err(|error| error.to_string())?;
        replace_metadata(&self.path, &json)
            .map_err(|error| format!("Save AgentBro metadata {}: {error}", self.path.display()))
    }
}

impl Deref for MetadataEdit {
    type Target = Metadata;

    fn deref(&self) -> &Self::Target {
        &self.metadata
    }
}

impl DerefMut for MetadataEdit {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.metadata
    }
}

fn replace_metadata(path: &Path, json: &[u8]) -> io::Result<()> {
    let previous_permissions = match fs::metadata(path) {
        Ok(previous) => Some(previous.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if previous_permissions
        .as_ref()
        .is_some_and(fs::Permissions::readonly)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "metadata file is read-only",
        ));
    }
    let staging = path.with_file_name(format!(".agentbro-metadata-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&staging)?;
    let result = (|| {
        file.write_all(json)?;
        if let Some(permissions) = previous_permissions {
            file.set_permissions(permissions)?;
        }
        file.sync_all()?;
        drop(file);
        fs::rename(&staging, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(staging);
    }
    result
}

pub fn add_source(skill_id: &str, origin: &str) -> Result<(), String> {
    let mut meta = edit()?;
    meta.sources.insert(
        skill_id.to_string(),
        SkillSourceEntry {
            origin: origin.to_string(),
            installed_via: "island".to_string(),
        },
    );
    meta.save()
}

pub fn remove_source(skill_id: &str) -> Result<(), String> {
    let mut meta = edit()?;
    meta.sources.remove(skill_id);
    meta.save()
}

pub fn get_skill_explanation(skill_id: &str, lang: &str) -> Option<SkillExplanationEntry> {
    load()
        .explanations
        .get(&explanation_key(skill_id, lang))
        .cloned()
}

pub fn cache_skill_explanation(entry: SkillExplanationEntry) -> Result<(), String> {
    let mut meta = edit()?;
    meta.explanations
        .insert(explanation_key(&entry.skill_id, &entry.lang), entry);
    meta.save()
}

fn explanation_key(skill_id: &str, lang: &str) -> String {
    format!("{}::{}", skill_id, lang)
}

pub fn list_packs() -> Vec<SkillPack> {
    load().packs
}

pub fn create_pack(pack: SkillPack) -> Result<(), String> {
    let mut meta = edit()?;
    meta.packs.push(pack);
    meta.save()
}

pub fn update_pack(pack: SkillPack) -> Result<(), String> {
    let mut meta = edit()?;
    let existing = meta
        .packs
        .iter_mut()
        .find(|p| p.id == pack.id)
        .ok_or_else(|| format!("Pack not found: {}", pack.id))?;
    *existing = pack;
    meta.save()
}

pub fn delete_pack(id: &str) -> Result<(), String> {
    let mut meta = edit()?;
    meta.packs.retain(|p| p.id != id);
    meta.save()
}

pub fn list_collections() -> Vec<SkillCollection> {
    let mut collections = load().collections;
    collections.sort_by_key(|a| a.name.to_lowercase());
    collections
}

pub fn upsert_collection(collection: SkillCollection) -> Result<SkillCollection, String> {
    upsert_collection_in(edit()?, collection)
}

fn upsert_collection_in(
    mut meta: MetadataEdit,
    mut collection: SkillCollection,
) -> Result<SkillCollection, String> {
    if collection.name.trim().is_empty() {
        return Err("Collection name cannot be empty".to_string());
    }
    collection.name = collection.name.trim().to_string();
    collection.description = collection.description.trim().to_string();
    collection.skills.sort();
    collection.skills.dedup();

    let now = chrono::Utc::now().to_rfc3339();
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
    meta.save()?;
    Ok(collection)
}

pub fn delete_collection(id: &str) -> Result<(), String> {
    let mut meta = edit()?;
    let before = meta.collections.len();
    meta.collections.retain(|collection| collection.id != id);
    if before == meta.collections.len() {
        return Err(format!("Collection not found: {id}"));
    }
    meta.save()
}

pub fn export_collection(id: &str) -> Result<String, String> {
    let collection = read_metadata()?
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
    let meta = edit()?;
    if meta.collections.iter().any(|item| item.id == collection.id) {
        collection.id = format!("{}-imported-{}", collection.id, uuid::Uuid::new_v4());
        collection.name = format!("{} (Imported)", collection.name);
    }
    upsert_collection_in(meta, collection)
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
    normalize_scan_roots(&mut roots);
    let mut meta = edit()?;
    meta.scan_roots = roots;
    meta.save()
}

fn normalize_scan_roots(roots: &mut Vec<ScanRoot>) {
    for root in roots.iter_mut() {
        root.path = normalize_path(&root.path);
        if root.label.trim().is_empty() {
            root.label = label_for_path(&root.path);
        }
    }
    roots.sort_by(|a, b| a.path.cmp(&b.path));
    roots.dedup_by(|a, b| a.path == b.path);
}

pub fn set_scan_root_enabled(path: &str, enabled: bool) -> Result<(), String> {
    let normalized = normalize_path(path);
    let mut meta = edit()?;
    if meta.scan_roots.is_empty() {
        meta.scan_roots = default_scan_roots();
    }
    let root = meta
        .scan_roots
        .iter_mut()
        .find(|root| root.path == normalized)
        .ok_or_else(|| format!("Scan root not found: {path}"))?;
    root.enabled = enabled;
    normalize_scan_roots(&mut meta.scan_roots);
    meta.save()
}

pub fn cache_discovered_skills(
    skills: Vec<DiscoveredSkill>,
) -> Result<Vec<DiscoveredSkill>, String> {
    let mut meta = edit()?;
    meta.discovered_skills = skills;
    meta.discovered_scanned_at = Some(chrono::Utc::now().to_rfc3339());
    let cached = meta.discovered_skills.clone();
    meta.save()?;
    Ok(cached)
}

pub fn list_discovered_skills() -> Vec<DiscoveredSkill> {
    load().discovered_skills
}

pub fn clear_discovered_skills() -> Result<(), String> {
    let mut meta = edit()?;
    meta.discovered_skills.clear();
    meta.discovered_scanned_at = None;
    meta.save()
}

pub fn get_sync_config() -> Option<SyncConfig> {
    load().sync
}

pub fn set_sync_config(config: SyncConfig) -> Result<(), String> {
    let mut meta = edit()?;
    meta.sync = Some(config);
    meta.save()
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
    let mut meta = edit()?;
    if let Some(existing) = meta
        .marketplace_sources
        .iter_mut()
        .find(|entry| entry.id == source.id)
    {
        *existing = source;
    } else {
        meta.marketplace_sources.push(source);
    }
    meta.save()
}

pub fn remove_marketplace_source(id: &str) -> Result<(), String> {
    let mut meta = edit()?;
    let before = meta.marketplace_sources.len();
    meta.marketplace_sources.retain(|entry| entry.id != id);
    if before == meta.marketplace_sources.len() {
        return Err(format!("Marketplace source not found: {id}"));
    }
    meta.save()
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

    let mut meta = edit()?;
    let id = resolve_custom_agent_id(
        config.id.as_deref(),
        &config.display_name,
        &meta.custom_agents,
    )?;

    let entry = CustomAgentEntry {
        id,
        display_name: config.display_name.trim().to_string(),
        category: config.category.unwrap_or_else(|| "custom".to_string()),
        global_skills_dir: normalize_path(&config.global_skills_dir),
        icon_name: config.icon_name.filter(|value| !value.trim().is_empty()),
        is_enabled: true,
        config_dir: config.config_dir.map(|value| normalize_path(&value)),
        settings_file: config.settings_file.map(|value| normalize_path(&value)),
        mcp_config: config.mcp_config.map(|value| normalize_path(&value)),
        plugin_dir: config.plugin_dir.map(|value| normalize_path(&value)),
    };
    meta.custom_agents.push(entry.clone());
    meta.save()?;
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

    let mut meta = edit()?;
    let entry = meta
        .custom_agents
        .iter_mut()
        .find(|agent| agent.id == agent_id)
        .ok_or_else(|| format!("Custom agent not found: {agent_id}"))?;

    entry.display_name = config.display_name.trim().to_string();
    entry.category = config.category.unwrap_or_else(|| "custom".to_string());
    entry.global_skills_dir = normalize_path(&config.global_skills_dir);
    entry.icon_name = config.icon_name.filter(|value| !value.trim().is_empty());
    entry.config_dir = config.config_dir.map(|value| normalize_path(&value));
    entry.settings_file = config.settings_file.map(|value| normalize_path(&value));
    entry.mcp_config = config.mcp_config.map(|value| normalize_path(&value));
    entry.plugin_dir = config.plugin_dir.map(|value| normalize_path(&value));
    let updated = entry.clone();
    meta.save()?;
    Ok(updated)
}

pub fn remove_custom_agent(agent_id: &str) -> Result<(), String> {
    let mut meta = edit()?;
    let before = meta.custom_agents.len();
    meta.custom_agents.retain(|agent| agent.id != agent_id);
    if meta.custom_agents.len() == before {
        return Err(format!("Custom agent not found: {agent_id}"));
    }
    meta.save()
}

fn resolve_custom_agent_id(
    explicit_id: Option<&str>,
    display_name: &str,
    existing_agents: &[CustomAgentEntry],
) -> Result<String, String> {
    let display_name = display_name.trim();
    let explicit_id = explicit_id
        .map(normalize_agent_id)
        .filter(|value| !value.is_empty());
    let generated_unicode_id = explicit_id.is_none() && !display_name.is_ascii();
    let id = explicit_id.unwrap_or_else(|| {
        if generated_unicode_id {
            format!("custom-unicode-{:x}", Sha256::digest(display_name))
        } else {
            format!("custom-{}", normalize_agent_id(display_name))
        }
    });
    if id == "custom-" {
        return Err("Agent ID cannot be empty".to_string());
    }
    if agent_paths::known_agent_ids().contains(&id.as_str()) {
        return Err(format!("Agent already exists: {id}"));
    }
    if let Some(agent) = existing_agents.iter().find(|agent| {
        agent.id == id || (generated_unicode_id && agent.display_name == display_name)
    }) {
        return Err(format!("Agent already exists: {}", agent.id));
    }
    Ok(id)
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
    #[cfg(target_os = "windows")]
    if trimmed.starts_with("~\\") {
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
    #[cfg(target_os = "windows")]
    if let Some(rest) = path.strip_prefix("~\\") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    Path::new(path).to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_metadata_edits_preserve_both_updates() {
        let temporary =
            super::super::installer::TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = temporary.path().join("metadata.json");
        let mut first = MetadataEdit::open(path.clone()).unwrap();
        first.sources.insert(
            "fixture-skill".to_string(),
            SkillSourceEntry {
                origin: "fixture".to_string(),
                installed_via: "island".to_string(),
            },
        );

        let lock = fs::OpenOptions::new()
            .write(true)
            .open(temporary.path().join("metadata.json.lock"))
            .unwrap();
        assert!(matches!(lock.try_lock(), Err(fs::TryLockError::WouldBlock)));

        std::thread::scope(|scope| {
            let second = scope.spawn(|| {
                let mut second = MetadataEdit::open(path.clone()).unwrap();
                assert!(second.sources.contains_key("fixture-skill"));
                second.discovered_scanned_at = Some("second edit".to_string());
                second.save().unwrap();
            });
            first.save().unwrap();
            second.join().unwrap();
        });

        let persisted = read_at(&path).unwrap();
        assert!(persisted.sources.contains_key("fixture-skill"));
        assert_eq!(
            persisted.discovered_scanned_at.as_deref(),
            Some("second edit")
        );
    }

    #[test]
    fn readers_keep_the_complete_original_metadata_after_replace() {
        use std::io::Read;

        let temporary =
            super::super::installer::TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = temporary.path().join("metadata.json");
        let original = br#"{"discoveredScannedAt":"original"}"#;
        fs::write(&path, original).unwrap();
        let mut reader = fs::File::open(&path).unwrap();
        let mut edit = MetadataEdit::open(path.clone()).unwrap();
        edit.discovered_scanned_at = Some("updated".to_string());
        edit.save().unwrap();

        let mut observed = Vec::new();
        reader.read_to_end(&mut observed).unwrap();
        assert_eq!(observed, original);
        assert_eq!(
            read_at(&path).unwrap().discovered_scanned_at.as_deref(),
            Some("updated")
        );
        assert!(fs::read_dir(temporary.path()).unwrap().all(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_none_or(|extension| extension != "tmp")
        }));
    }

    #[test]
    fn unreadable_metadata_is_preserved_and_errors_omit_its_contents() {
        let temporary =
            super::super::installer::TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = temporary.path().join("metadata.json");
        let invalid = br#"{"sync":"fixture-private-content"}"#;
        fs::write(&path, invalid).unwrap();
        let error = MetadataEdit::open(path.clone()).err().unwrap();
        assert!(error.contains("Invalid AgentBro metadata"));
        assert!(!error.contains("fixture-private-content"));
        assert_eq!(fs::read(path).unwrap(), invalid);
    }

    #[test]
    fn custom_agent_names_keep_legacy_ids_and_distinguish_unicode_names() {
        let expected =
            "custom-unicode-1b18e79ddcc903d4cdd035e65afe757205a1b412f9e0ac2bd6103993d816edfa";
        assert_eq!(
            resolve_custom_agent_id(None, " 研发助手 ", &[]).unwrap(),
            expected
        );
        assert_ne!(
            resolve_custom_agent_id(None, "研发 Claude", &[]).unwrap(),
            resolve_custom_agent_id(None, "运维 Claude", &[]).unwrap(),
        );
        assert_eq!(
            resolve_custom_agent_id(None, "Company Claude", &[]).unwrap(),
            "custom-company-claude",
        );
        assert_eq!(
            resolve_custom_agent_id(Some(" Team_Claude "), "研发助手", &[]).unwrap(),
            "team-claude",
        );
        assert_eq!(
            resolve_custom_agent_id(Some(expected), "研发助手", &[]).unwrap(),
            expected,
        );
        assert_eq!(
            resolve_custom_agent_id(Some("Codex"), "研发助手", &[]).unwrap_err(),
            "Agent already exists: codex",
        );
        assert_eq!(
            resolve_custom_agent_id(None, "!", &[]).unwrap_err(),
            "Agent ID cannot be empty",
        );

        let legacy: CustomAgentEntry = serde_json::from_value(serde_json::json!({
            "id": "custom-claude",
            "displayName": "研发 Claude",
            "category": "claude-compatible",
            "globalSkillsDir": "/example/skills",
            "isEnabled": true,
        }))
        .unwrap();
        assert_eq!(
            resolve_custom_agent_id(None, "研发 Claude", &[legacy]).unwrap_err(),
            "Agent already exists: custom-claude",
        );
    }

    #[test]
    fn normalize_path_expands_posix_home_prefix() {
        let Some(home) = dirs::home_dir() else {
            return;
        };

        assert_eq!(
            normalize_path("~/AgentBro/skills"),
            home.join("AgentBro/skills").display().to_string()
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn normalize_path_expands_windows_home_prefix() {
        let Some(home) = dirs::home_dir() else {
            return;
        };

        assert_eq!(
            normalize_path(r"~\AgentBro\skills"),
            home.join(r"AgentBro\skills").display().to_string()
        );
    }
}
