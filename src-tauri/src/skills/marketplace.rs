use super::{
    registry, InstallMode, MarketplaceItem, MarketplaceMcpConfig, MarketplaceSource, TargetConfig,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRegistry {
    pub id: String,
    pub name: String,
    pub source_type: String,
    pub url: String,
    pub is_builtin: bool,
    pub is_enabled: bool,
    pub last_synced: Option<String>,
    pub last_attempted_sync: Option<String>,
    pub last_sync_status: String,
    pub last_sync_error: Option<String>,
    pub cache_updated_at: Option<String>,
    pub cache_expires_at: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceSkill {
    pub id: String,
    pub registry_id: String,
    pub name: String,
    pub description: Option<String>,
    pub download_url: String,
    pub is_installed: bool,
    pub synced_at: String,
    pub cache_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncRegistryOptions {
    pub force_refresh: bool,
}

pub fn list_items() -> Result<Vec<MarketplaceItem>, String> {
    let mut items = default_items();
    for source in registry::list_marketplace_sources()
        .into_iter()
        .filter(|source| source.enabled)
    {
        let content = read_manifest(&source.url)?;
        let mut remote_items = parse_manifest(&content)
            .map_err(|error| format!("Failed to parse marketplace {}: {error}", source.name))?;
        items.append(&mut remote_items);
    }
    dedupe_items(items)
}

pub fn list_registries() -> Vec<SkillRegistry> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut registries = vec![SkillRegistry {
        id: "builtin".to_string(),
        name: "Built-in Marketplace".to_string(),
        source_type: "builtin".to_string(),
        url: "agentbro://builtin-marketplace".to_string(),
        is_builtin: true,
        is_enabled: true,
        last_synced: Some(now.clone()),
        last_attempted_sync: Some(now.clone()),
        last_sync_status: "success".to_string(),
        last_sync_error: None,
        cache_updated_at: Some(now.clone()),
        cache_expires_at: None,
        etag: None,
        last_modified: None,
        created_at: now.clone(),
    }];
    registries.extend(
        registry::list_marketplace_sources()
            .into_iter()
            .map(|source| source_to_registry(source, &now)),
    );
    registries
}

pub fn add_registry(
    name: String,
    source_type: String,
    url: String,
) -> Result<SkillRegistry, String> {
    if name.trim().is_empty() {
        return Err("Marketplace registry name cannot be empty".to_string());
    }
    if url.trim().is_empty() {
        return Err("Marketplace registry URL cannot be empty".to_string());
    }
    let id = registry_id_from_name(&name);
    let source = MarketplaceSource {
        id: id.clone(),
        name: name.trim().to_string(),
        url: url.trim().to_string(),
        enabled: true,
    };
    registry::upsert_marketplace_source(source.clone())?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut item = source_to_registry(source, &now);
    item.source_type = source_type;
    Ok(item)
}

pub fn remove_registry(id: &str) -> Result<(), String> {
    if id == "builtin" {
        return Err("Cannot remove built-in marketplace registry".to_string());
    }
    registry::remove_marketplace_source(id)
}

pub fn sync_registry(
    registry_id: &str,
    _options: SyncRegistryOptions,
) -> Result<Vec<MarketplaceSkill>, String> {
    marketplace_skills(Some(registry_id), None)
}

pub fn search_marketplace_skills(
    registry_id: Option<String>,
    query: Option<String>,
) -> Result<Vec<MarketplaceSkill>, String> {
    marketplace_skills(registry_id.as_deref(), query.as_deref())
}

pub fn install_marketplace_skill(skill_id: &str) -> Result<(), String> {
    let item = list_items()?
        .into_iter()
        .find(|item| item.id == skill_id)
        .ok_or_else(|| format!("Marketplace skill not found: {skill_id}"))?;
    match item.category.as_str() {
        "skill" => {
            let source = install_source_for_item(&item);
            let targets = vec![TargetConfig {
                agent: "central".to_string(),
                install_mode: InstallMode::Direct,
            }];
            for installed_id in
                super::installer::install_skill(&source, &targets, &InstallMode::Direct)?
            {
                registry::add_source(&installed_id, &source)?;
            }
            Ok(())
        }
        "plugin" => Err("Marketplace plugin installs require choosing a target Agent".to_string()),
        "mcp" => Err("Marketplace MCP installs require choosing a target Agent".to_string()),
        other => Err(format!("Unsupported marketplace item category: {other}")),
    }
}

fn marketplace_skills(
    registry_id: Option<&str>,
    query: Option<&str>,
) -> Result<Vec<MarketplaceSkill>, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let normalized_query = query
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());
    let installed_sources = registry::load().sources;
    let items = list_items()?
        .into_iter()
        .filter(|item| {
            registry_id
                .filter(|id| *id != "builtin")
                .map(|id| item.id.starts_with(id) || item.author.eq_ignore_ascii_case(id))
                .unwrap_or(true)
        })
        .filter(|item| {
            normalized_query
                .as_ref()
                .map(|query| {
                    item.name.to_lowercase().contains(query)
                        || item.description.to_lowercase().contains(query)
                        || item.author.to_lowercase().contains(query)
                })
                .unwrap_or(true)
        })
        .map(|item| {
            let source = install_source_for_item(&item);
            let is_installed = installed_sources
                .values()
                .any(|entry| entry.origin == source || entry.origin == item.source);
            let registry_id = registry_id_for_item(&item);
            MarketplaceSkill {
                id: item.id,
                registry_id,
                name: item.name,
                description: Some(item.description),
                download_url: source,
                is_installed,
                synced_at: now.clone(),
                cache_updated_at: Some(now.clone()),
            }
        })
        .collect();
    Ok(items)
}

fn source_to_registry(source: MarketplaceSource, now: &str) -> SkillRegistry {
    SkillRegistry {
        id: source.id,
        name: source.name,
        source_type: "json".to_string(),
        url: source.url,
        is_builtin: false,
        is_enabled: source.enabled,
        last_synced: None,
        last_attempted_sync: None,
        last_sync_status: "never".to_string(),
        last_sync_error: None,
        cache_updated_at: None,
        cache_expires_at: None,
        etag: None,
        last_modified: None,
        created_at: now.to_string(),
    }
}

fn registry_id_from_name(name: &str) -> String {
    let id = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if id.is_empty() {
        format!("registry-{}", uuid::Uuid::new_v4())
    } else {
        id
    }
}

fn registry_id_for_item(item: &MarketplaceItem) -> String {
    if item.id.starts_with("official:") || item.id.starts_with("official-repo:") {
        return "official".to_string();
    }
    "builtin".to_string()
}

pub fn install_source_for_item(item: &MarketplaceItem) -> String {
    if item.source_type == "github" {
        format!(
            "github:{}",
            [Some(item.source.as_str()), item.sub_path.as_deref()]
                .into_iter()
                .flatten()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join("/")
        )
    } else {
        item.source.clone()
    }
}

fn read_manifest(url: &str) -> Result<String, String> {
    let expanded = expand_user_path(url);
    if expanded.exists() {
        return fs::read_to_string(expanded).map_err(|e| e.to_string());
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        let output = Command::new("curl")
            .args(["-L", "--fail", url])
            .output()
            .map_err(|e| format!("Failed to run curl: {e}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        return String::from_utf8(output.stdout).map_err(|e| e.to_string());
    }
    Err(format!("Marketplace manifest not found: {url}"))
}

fn parse_manifest(content: &str) -> Result<Vec<MarketplaceItem>, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(content)?;
    if value.is_array() {
        return serde_json::from_value(value);
    }
    if let Some(items) = value.get("items") {
        return serde_json::from_value(items.clone());
    }
    serde_json::from_value(value)
}

fn dedupe_items(items: Vec<MarketplaceItem>) -> Result<Vec<MarketplaceItem>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for item in items {
        if item.id.trim().is_empty() {
            return Err("Marketplace item id cannot be empty".to_string());
        }
        if item.name.trim().is_empty() {
            return Err(format!("Marketplace item {} name cannot be empty", item.id));
        }
        if seen.insert(item.id.clone()) {
            result.push(item);
        }
    }
    Ok(result)
}

fn default_items() -> Vec<MarketplaceItem> {
    vec![
        market_item(
            "anthropic-skills",
            "Anthropic Skills",
            "官方示例 Skills 集合，适合作为 Claude Code / Codex 的基础能力包。",
            "skill",
            "github",
            "anthropics/skills",
            "Anthropic",
            "#5856d6",
            None,
        ),
        market_item(
            "openai-skills",
            "OpenAI Skills",
            "面向 Codex 工作流的 Skills 集合，可按子目录导入。",
            "skill",
            "github",
            "openai/skills",
            "OpenAI",
            "#34c759",
            None,
        ),
        market_item(
            "github-mcp",
            "GitHub MCP",
            "Issues、PR、Actions 和仓库 API 集成。",
            "mcp",
            "github",
            "github/github-mcp-server",
            "GitHub",
            "#1d1d1f",
            Some((
                "docker",
                vec!["run", "-i", "--rm", "ghcr.io/github/github-mcp-server"],
            )),
        ),
        market_item(
            "playwright-mcp",
            "Playwright MCP",
            "浏览器自动化、截图、页面检查和端到端验证。",
            "mcp",
            "github",
            "microsoft/playwright-mcp",
            "Microsoft",
            "#007aff",
            Some(("npx", vec!["-y", "@playwright/mcp@latest"])),
        ),
        market_item(
            "context7-mcp",
            "Context7 MCP",
            "面向代码助手的最新文档检索 MCP 能力。",
            "mcp",
            "github",
            "upstash/context7",
            "Upstash",
            "#ff9500",
            Some(("npx", vec!["-y", "@upstash/context7-mcp"])),
        ),
        market_item(
            "notion-mcp",
            "Notion MCP",
            "Notion 页面、数据库和知识库读写能力。",
            "mcp",
            "github",
            "makenotion/notion-mcp-server",
            "Notion",
            "#8e8e93",
            Some(("npx", vec!["-y", "@notionhq/notion-mcp-server"])),
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn market_item(
    id: &str,
    name: &str,
    description: &str,
    category: &str,
    source_type: &str,
    source: &str,
    author: &str,
    accent: &str,
    mcp: Option<(&str, Vec<&str>)>,
) -> MarketplaceItem {
    MarketplaceItem {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        category: category.to_string(),
        source_type: source_type.to_string(),
        source: source.to_string(),
        sub_path: None,
        author: author.to_string(),
        accent: accent.to_string(),
        mcp: mcp.map(|(command, args)| MarketplaceMcpConfig {
            command: command.to_string(),
            args: args.into_iter().map(ToString::to_string).collect(),
            env: std::collections::HashMap::new(),
        }),
        plugin: None,
    }
}

fn expand_user_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}
