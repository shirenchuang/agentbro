use super::{
    registry, InstallMode, MarketplaceItem, MarketplaceMcpConfig, MarketplaceSource, TargetConfig,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const SKILLS_SH_DEFAULT_QUERY: &str = "skill";
const SKILLS_SH_SEARCH_LIMIT: usize = 80;
const SKILLS_SH_LEADERBOARD_LIMIT: usize = 120;
const SKILLS_SH_CACHE_TTL: Duration = Duration::from_secs(300);
const SKILLS_SH_SEARCH_CACHE_TTL: Duration = Duration::from_secs(120);
const SKILLS_SH_USER_AGENT: &str = "AgentBro";

static SKILLS_SH_CACHE: OnceLock<Mutex<HashMap<String, SkillsShCacheEntry>>> = OnceLock::new();

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
    pub source: Option<String>,
    pub install_count: Option<u64>,
    pub download_url: String,
    pub web_url: Option<String>,
    pub is_installed: bool,
    pub synced_at: String,
    pub cache_updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillsShSkill {
    #[serde(default)]
    id: String,
    #[serde(alias = "skill_id", alias = "skillId")]
    skill_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    installs: u64,
}

#[derive(Debug, Clone)]
struct SkillsShCacheEntry {
    stored_at: Instant,
    items: Vec<MarketplaceSkill>,
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
    let mut registries = vec![
        SkillRegistry {
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
        },
        SkillRegistry {
            id: "skills-sh".to_string(),
            name: "skills.sh".to_string(),
            source_type: "skills.sh".to_string(),
            url: "https://skills.sh".to_string(),
            is_builtin: true,
            is_enabled: true,
            last_synced: None,
            last_attempted_sync: None,
            last_sync_status: "live".to_string(),
            last_sync_error: None,
            cache_updated_at: None,
            cache_expires_at: None,
            etag: None,
            last_modified: None,
            created_at: now.clone(),
        },
    ];
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
    marketplace_skills(Some(registry_id), None, Some("alltime"))
}

pub fn search_marketplace_skills(
    registry_id: Option<String>,
    query: Option<String>,
    board: Option<String>,
) -> Result<Vec<MarketplaceSkill>, String> {
    marketplace_skills(registry_id.as_deref(), query.as_deref(), board.as_deref())
}

pub async fn search_marketplace_skills_async(
    registry_id: Option<String>,
    query: Option<String>,
    board: Option<String>,
) -> Result<Vec<MarketplaceSkill>, String> {
    let wants_skills_sh = registry_id
        .as_deref()
        .map(|id| matches!(id, "skills-sh" | "skillssh" | "skills.sh"))
        .unwrap_or(true);
    let wants_builtin_items = registry_id
        .as_deref()
        .map(|id| !matches!(id, "skills-sh" | "skillssh" | "skills.sh"))
        .unwrap_or(true);

    if wants_skills_sh && !wants_builtin_items {
        return skills_sh_marketplace_async(query.as_deref(), board.as_deref()).await;
    }

    let mut items = if wants_builtin_items {
        marketplace_skills(registry_id.as_deref(), query.as_deref(), board.as_deref())?
    } else {
        Vec::new()
    };
    if wants_skills_sh {
        match skills_sh_marketplace_async(query.as_deref(), board.as_deref()).await {
            Ok(mut skills) => items.append(&mut skills),
            Err(error) => log::warn!("Failed to fetch skills.sh marketplace: {}", error),
        }
    }
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.id.clone()));
    Ok(items)
}

pub fn install_marketplace_skill(skill_id: &str) -> Result<(), String> {
    if let Some(source) = skills_sh_install_source(skill_id) {
        let targets = vec![TargetConfig {
            agent: "central".to_string(),
            install_mode: InstallMode::Direct,
        }];
        for installed_id in
            super::installer::install_skill(&source, &targets, &InstallMode::Direct)?
        {
            registry::add_source(&installed_id, &format!("skills.sh:{skill_id}"))?;
        }
        return Ok(());
    }

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
    board: Option<&str>,
) -> Result<Vec<MarketplaceSkill>, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let normalized_query = query
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());
    let normalized_board = normalize_skills_sh_board(board);
    let installed_sources = registry::load().sources;
    let wants_skills_sh = registry_id
        .map(|id| matches!(id, "skills-sh" | "skillssh" | "skills.sh"))
        .unwrap_or(true);
    let wants_builtin_items = registry_id
        .map(|id| !matches!(id, "skills-sh" | "skillssh" | "skills.sh"))
        .unwrap_or(true);
    let mut items = Vec::new();

    if wants_builtin_items {
        items.extend(
            list_items()?
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
                    let source_label = item.author.clone();
                    let web_url = marketplace_item_web_url(&item);
                    MarketplaceSkill {
                        id: item.id,
                        registry_id,
                        name: item.name,
                        description: Some(item.description),
                        source: Some(source_label),
                        install_count: None,
                        download_url: source,
                        web_url,
                        is_installed,
                        synced_at: now.clone(),
                        cache_updated_at: Some(now.clone()),
                    }
                })
                .collect::<Vec<_>>(),
        );
    }

    if wants_skills_sh {
        let result = if let Some(query) = normalized_query.as_deref() {
            fetch_skills_sh_search(query, SKILLS_SH_SEARCH_LIMIT)
        } else {
            fetch_skills_sh_board_search(normalized_board, SKILLS_SH_LEADERBOARD_LIMIT)
        };
        match result {
            Ok(skills) => items.extend(
                skills
                    .into_iter()
                    .map(|skill| skills_sh_marketplace_skill(skill, &installed_sources, &now)),
            ),
            Err(error) => log::warn!("Failed to fetch skills.sh marketplace: {}", error),
        }
    }

    if normalized_query.is_some() || normalized_board == "alltime" || wants_builtin_items {
        items.sort_by(|a, b| {
            b.is_installed
                .cmp(&a.is_installed)
                .then_with(|| {
                    b.install_count
                        .unwrap_or(0)
                        .cmp(&a.install_count.unwrap_or(0))
                })
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
    }
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.id.clone()));
    Ok(items)
}

async fn skills_sh_marketplace_async(
    query: Option<&str>,
    board: Option<&str>,
) -> Result<Vec<MarketplaceSkill>, String> {
    let normalized_query = query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let normalized_board = normalize_skills_sh_board(board);
    let cache_key = normalized_query
        .as_ref()
        .map(|query| {
            format!(
                "skills-sh:search:{}:{SKILLS_SH_SEARCH_LIMIT}",
                query.to_lowercase()
            )
        })
        .unwrap_or_else(|| format!("skills-sh:leaderboard:{normalized_board}"));
    let ttl = if normalized_query.is_some() {
        SKILLS_SH_SEARCH_CACHE_TTL
    } else {
        SKILLS_SH_CACHE_TTL
    };

    if let Some(items) = read_skills_sh_cache(&cache_key, ttl) {
        return Ok(items);
    }

    let skills = if let Some(query) = normalized_query.as_deref() {
        fetch_skills_sh_search_async(query, SKILLS_SH_SEARCH_LIMIT).await?
    } else {
        fetch_skills_sh_leaderboard_async(normalized_board, SKILLS_SH_LEADERBOARD_LIMIT).await?
    };
    let now = chrono::Utc::now().to_rfc3339();
    let installed_sources = registry::load().sources;
    let mut items = skills
        .into_iter()
        .map(|skill| skills_sh_marketplace_skill(skill, &installed_sources, &now))
        .collect::<Vec<_>>();
    items.sort_by(|a, b| {
        b.is_installed
            .cmp(&a.is_installed)
            .then_with(|| {
                b.install_count
                    .unwrap_or(0)
                    .cmp(&a.install_count.unwrap_or(0))
            })
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    write_skills_sh_cache(cache_key, items.clone());
    Ok(items)
}

fn read_skills_sh_cache(key: &str, ttl: Duration) -> Option<Vec<MarketplaceSkill>> {
    let cache = SKILLS_SH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let guard = cache.lock().ok()?;
    let entry = guard.get(key)?;
    if entry.stored_at.elapsed() <= ttl {
        Some(entry.items.clone())
    } else {
        None
    }
}

fn write_skills_sh_cache(key: String, items: Vec<MarketplaceSkill>) {
    let cache = SKILLS_SH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            key,
            SkillsShCacheEntry {
                stored_at: Instant::now(),
                items,
            },
        );
    }
}

fn skills_sh_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(SKILLS_SH_USER_AGENT)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to build skills.sh client: {e}"))
}

async fn fetch_skills_sh_leaderboard_async(
    board: &str,
    limit: usize,
) -> Result<Vec<SkillsShSkill>, String> {
    let html = skills_sh_client()?
        .get(skills_sh_board_url(board))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch skills.sh: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Failed to read skills.sh response: {e}"))?;
    let mut skills = parse_skills_sh_html(&html)?;
    skills.truncate(limit.clamp(1, 300));
    Ok(skills)
}

async fn fetch_skills_sh_search_async(
    query: &str,
    limit: usize,
) -> Result<Vec<SkillsShSkill>, String> {
    let limit = limit.clamp(1, 300).to_string();
    let value = skills_sh_client()?
        .get("https://skills.sh/api/search")
        .query(&[("q", query), ("limit", limit.as_str())])
        .send()
        .await
        .map_err(|e| format!("Failed to search skills.sh: {e}"))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse skills.sh search response: {e}"))?;
    Ok(parse_skills_sh_json_array(&value))
}

fn fetch_skills_sh_board_search(board: &str, limit: usize) -> Result<Vec<SkillsShSkill>, String> {
    let query = match board {
        "trending" => "trending",
        "hot" => "popular",
        _ => SKILLS_SH_DEFAULT_QUERY,
    };
    fetch_skills_sh_search(query, limit)
}

fn fetch_skills_sh_search(query: &str, limit: usize) -> Result<Vec<SkillsShSkill>, String> {
    let query = query.trim();
    let query = if query.is_empty() {
        SKILLS_SH_DEFAULT_QUERY
    } else {
        query
    };
    let limit = limit.clamp(1, 300).to_string();
    let output = Command::new("curl")
        .args([
            "-L",
            "--fail",
            "--silent",
            "--show-error",
            "--compressed",
            "--connect-timeout",
            "8",
            "--max-time",
            "15",
            "--get",
            "https://skills.sh/api/search",
        ])
        .arg("--data-urlencode")
        .arg(format!("q={query}"))
        .arg("--data-urlencode")
        .arg(format!("limit={limit}"))
        .output()
        .map_err(|e| format!("Failed to run curl: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse skills.sh response: {e}"))?;
    Ok(parse_skills_sh_json_array(&value))
}

fn parse_skills_sh_json_array(value: &serde_json::Value) -> Vec<SkillsShSkill> {
    let array = value
        .as_array()
        .cloned()
        .or_else(|| value.get("skills").and_then(|v| v.as_array()).cloned())
        .or_else(|| {
            value
                .pointer("/props/pageProps/initialSkills")
                .and_then(|v| v.as_array())
                .cloned()
        })
        .or_else(|| {
            value
                .pointer("/props/pageProps/skills")
                .and_then(|v| v.as_array())
                .cloned()
        })
        .or_else(|| {
            value
                .pointer("/props/pageProps/items")
                .and_then(|v| v.as_array())
                .cloned()
        })
        .unwrap_or_default();

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in array {
        let Ok(mut skill) = serde_json::from_value::<SkillsShSkill>(item) else {
            continue;
        };
        skill.source = skill.source.trim().to_string();
        skill.skill_id = skill.skill_id.trim().to_string();
        if skill.source.is_empty() || skill.skill_id.is_empty() {
            continue;
        }
        if skill.id.trim().is_empty() {
            skill.id = format!("{}/{}", skill.source, skill.skill_id);
        }
        if seen.insert(format!("{}/{}", skill.source, skill.skill_id)) {
            out.push(skill);
        }
    }
    out
}

fn parse_skills_sh_html(html: &str) -> Result<Vec<SkillsShSkill>, String> {
    if let Some(skills) = parse_skills_sh_next_data(html) {
        if !skills.is_empty() {
            return Ok(skills);
        }
    }

    let skills = parse_skills_sh_embedded_objects(html)?;
    if skills.is_empty() {
        log::warn!("Could not find skills in skills.sh HTML");
    }
    Ok(skills)
}

fn parse_skills_sh_next_data(html: &str) -> Option<Vec<SkillsShSkill>> {
    let marker = r#"<script id="__NEXT_DATA__" type="application/json">"#;
    let start = html.find(marker)? + marker.len();
    let end = html[start..].find("</script>")? + start;
    let value = serde_json::from_str::<serde_json::Value>(&html[start..end]).ok()?;
    Some(parse_skills_sh_json_array(&value))
}

fn parse_skills_sh_embedded_objects(html: &str) -> Result<Vec<SkillsShSkill>, String> {
    let pattern = Regex::new(
        r#"(?:\\)?\"source(?:\\)?\":(?:\\)?\"(?P<source>[^"\\]+)(?:\\)?\",(?:[^{}]|\\.)*?(?:(?:\\)?\"skillId(?:\\)?\"|(?:\\)?\"skill_id(?:\\)?\"):(?:\\)?\"(?P<skill_id>[^"\\]+)(?:\\)?\",(?:[^{}]|\\.)*?(?:\\)?\"name(?:\\)?\":(?:\\)?\"(?P<name>[^"\\]*)(?:\\)?\",(?:[^{}]|\\.)*?(?:\\)?\"installs(?:\\)?\":(?P<installs>\d+)"#,
    )
    .map_err(|e| e.to_string())?;
    let fallback_pattern = Regex::new(
        r#"\{"source":"(?P<source>[^"]+)","skill_id":"(?P<skill_id>[^"]+)"(?:,"name":"(?P<name>[^"]*)")?(?:.*?"installs":(?P<installs>\d+))?\}"#,
    )
    .map_err(|e| e.to_string())?;

    let mut skills = parse_skills_sh_with_regex(html, &pattern);
    if skills.is_empty() {
        skills = parse_skills_sh_with_regex(html, &fallback_pattern);
    }
    Ok(skills)
}

fn parse_skills_sh_with_regex(html: &str, pattern: &Regex) -> Vec<SkillsShSkill> {
    let mut seen = HashSet::new();
    let mut skills = Vec::new();
    for caps in pattern.captures_iter(html) {
        let Some(source) = caps.name("source") else {
            continue;
        };
        let Some(skill_id) = caps.name("skill_id") else {
            continue;
        };
        let source = source.as_str().replace(r#"\""#, "\"");
        let skill_id = skill_id.as_str().replace(r#"\""#, "\"");
        if source.trim().is_empty() || skill_id.trim().is_empty() {
            continue;
        }
        let dedupe_key = format!("{source}/{skill_id}");
        if !seen.insert(dedupe_key.clone()) {
            continue;
        }
        let name = caps
            .name("name")
            .map(|value| value.as_str().replace(r#"\""#, "\""))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| skill_id.clone());
        let installs = caps
            .name("installs")
            .and_then(|value| value.as_str().parse::<u64>().ok())
            .unwrap_or_default();
        skills.push(SkillsShSkill {
            id: dedupe_key,
            skill_id,
            name,
            source,
            installs,
        });
    }
    skills
}

fn normalize_skills_sh_board(board: Option<&str>) -> &'static str {
    match board.unwrap_or("alltime").trim().to_lowercase().as_str() {
        "trending" => "trending",
        "hot" => "hot",
        _ => "alltime",
    }
}

fn skills_sh_board_url(board: &str) -> &'static str {
    match board {
        "trending" => "https://skills.sh/trending",
        "hot" => "https://skills.sh/hot",
        _ => "https://skills.sh/",
    }
}

fn skills_sh_marketplace_skill(
    skill: SkillsShSkill,
    installed_sources: &HashMap<String, registry::SkillSourceEntry>,
    now: &str,
) -> MarketplaceSkill {
    let id = if skill.id.trim().is_empty() {
        format!("skillssh:{}@{}", skill.source, skill.skill_id)
    } else {
        format!("skillssh:{}", skill.id.replace('/', "@"))
    };
    let download_url = format!("skillssh:{}/{}", skill.source, skill.skill_id);
    let is_installed = installed_sources.values().any(|entry| {
        entry.origin == download_url
            || entry.origin == format!("github:{}/{}", skill.source, skill.skill_id)
            || entry.origin == format!("skills.sh:{id}")
    });
    let web_url = format!("https://skills.sh/{}/{}", skill.source, skill.skill_id);
    MarketplaceSkill {
        id,
        registry_id: "skills-sh".to_string(),
        name: if skill.name.trim().is_empty() {
            skill.skill_id
        } else {
            skill.name
        },
        description: Some(format!("来自 {} 的在线 Skill", skill.source)),
        source: Some(skill.source),
        install_count: Some(skill.installs),
        download_url,
        web_url: Some(web_url),
        is_installed,
        synced_at: now.to_string(),
        cache_updated_at: Some(now.to_string()),
    }
}

fn skills_sh_install_source(skill_id: &str) -> Option<String> {
    let rest = skill_id.strip_prefix("skillssh:")?;
    let normalized = rest.replace('@', "/");
    let mut parts = normalized.split('/').filter(|part| !part.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?;
    let subpath = parts.collect::<Vec<_>>().join("/");
    if subpath.is_empty() {
        None
    } else {
        Some(format!("skillssh:{owner}/{repo}/{subpath}"))
    }
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

fn marketplace_item_web_url(item: &MarketplaceItem) -> Option<String> {
    if item.source_type == "github" {
        Some(format!(
            "https://github.com/{}",
            [Some(item.source.as_str()), item.sub_path.as_deref()]
                .into_iter()
                .flatten()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join("/")
        ))
    } else if item.source.starts_with("http://") || item.source.starts_with("https://") {
        Some(item.source.clone())
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_skills_sh_rsc_payload() {
        let html = r#"
        <script>self.__next_f.push([1,"[{\"source\":\"anthropics/skills\",\"skillId\":\"frontend-design\",\"name\":\"frontend-design\",\"installs\":541500},{\"source\":\"vercel-labs/agent-skills\",\"skillId\":\"web-design-guidelines\",\"name\":\"web-design-guidelines\",\"installs\":388900}]"])</script>
        "#;

        let skills = parse_skills_sh_html(html).expect("RSC payload should parse");
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].id, "anthropics/skills/frontend-design");
        assert_eq!(skills[0].installs, 541500);
    }

    #[test]
    fn parses_skills_sh_search_response() {
        let value = serde_json::json!({
            "skills": [
                {
                    "id": "vercel-labs/skills/find-skills",
                    "skillId": "find-skills",
                    "name": "find-skills",
                    "source": "vercel-labs/skills",
                    "installs": 2007850
                }
            ]
        });

        let skills = parse_skills_sh_json_array(&value);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].skill_id, "find-skills");
        assert_eq!(skills[0].source, "vercel-labs/skills");
    }

    #[tokio::test]
    #[ignore]
    async fn live_skills_sh_marketplace_returns_results() {
        let skills = search_marketplace_skills_async(
            Some("skills-sh".to_string()),
            None,
            Some("alltime".to_string()),
        )
        .await
        .expect("live skills.sh marketplace should load");
        assert!(!skills.is_empty());
        assert!(skills
            .iter()
            .any(|skill| skill.install_count.unwrap_or(0) > 0));
    }
}
