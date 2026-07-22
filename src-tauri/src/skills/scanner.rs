use super::{agent_paths, zcode_config};
use super::{
    AgentSkillState, DiscoveredSkill, InstallMode, McpServerConfig, ObsidianVault, ScannedSkill,
    SkillSource, SkillType,
};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub fn scan_agent(agent: &str) -> Vec<ScannedSkill> {
    let paths = agent_paths::paths_for_agent(agent);
    let mut results = Vec::new();

    for skill_dir in &paths.skill_dirs {
        if !skill_dir.is_dir() {
            continue;
        }
        scan_directory(skill_dir, agent, &mut results);
    }

    if let Some(ref mcp_path) = paths.mcp_config {
        scan_mcp_config(mcp_path, agent, &mut results);
    }

    if matches!(agent, "claude-code" | "codex" | "zcode") {
        scan_plugins(agent, &mut results);
    }

    results
}

pub fn scan_all() -> std::collections::HashMap<String, Vec<ScannedSkill>> {
    let mut map = std::collections::HashMap::new();
    for agent in agent_paths::known_agent_ids() {
        let skills = scan_agent(agent);
        if !skills.is_empty() {
            map.insert((*agent).to_string(), skills);
        }
    }
    for agent in super::registry::list_custom_agents() {
        if !agent.is_enabled {
            continue;
        }
        let skills = scan_agent(&agent.id);
        if !skills.is_empty() {
            map.insert(agent.id, skills);
        }
    }
    map
}

pub fn discover_project_skills(roots: &[String]) -> Vec<DiscoveredSkill> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        let path = expand_user_path(root);
        if path.is_dir() {
            discover_in_dir(&path, &path, 0, &mut seen, &mut results);
        }
        if results.len() >= 400 {
            break;
        }
    }
    results
}

pub fn discover_project_skills_from_scan_roots() -> Vec<DiscoveredSkill> {
    let roots = super::registry::list_scan_roots()
        .into_iter()
        .filter(|root| root.enabled)
        .map(|root| root.path)
        .collect::<Vec<_>>();
    discover_project_skills(&roots)
}

pub fn get_obsidian_vaults() -> Vec<ObsidianVault> {
    let mut vaults = Vec::new();
    let mut seen = HashSet::new();
    let roots = super::registry::list_scan_roots()
        .into_iter()
        .filter(|root| root.enabled)
        .map(|root| expand_user_path(&root.path))
        .collect::<Vec<_>>();

    for root in roots {
        if root.is_dir() {
            discover_obsidian_vaults_in_dir(&root, 0, &mut seen, &mut vaults);
        }
        if vaults.len() >= 100 {
            break;
        }
    }
    vaults.sort_by_key(|a| a.name.to_lowercase());
    vaults
}

pub fn get_obsidian_vault_skills(vault_path: &str) -> Vec<DiscoveredSkill> {
    let path = expand_user_path(vault_path);
    if !path.is_dir() || !path.join(".obsidian").is_dir() {
        return Vec::new();
    }
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    discover_in_dir(&path, &path, 0, &mut seen, &mut results);
    results
}

fn discover_obsidian_vaults_in_dir(
    dir: &Path,
    depth: usize,
    seen: &mut HashSet<String>,
    vaults: &mut Vec<ObsidianVault>,
) {
    if depth > 6 || vaults.len() >= 100 {
        return;
    }
    if dir.join(".obsidian").is_dir() {
        let key = dir.display().to_string();
        if seen.insert(key.clone()) {
            let skills = get_obsidian_vault_skills(&key);
            vaults.push(ObsidianVault {
                id: stable_id(&key),
                name: dir
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                path: key,
                skill_count: skills.len(),
            });
        }
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(
            name.as_str(),
            "node_modules" | ".git" | "target" | "dist" | "build" | ".next" | ".turbo"
        ) {
            continue;
        }
        discover_obsidian_vaults_in_dir(&path, depth + 1, seen, vaults);
    }
}

fn stable_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn discover_in_dir(
    root: &Path,
    dir: &Path,
    depth: usize,
    seen: &mut HashSet<String>,
    results: &mut Vec<DiscoveredSkill>,
) {
    if depth > 8 || results.len() >= 400 {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !path.is_dir() {
            continue;
        }
        if matches!(
            name.as_str(),
            "node_modules" | ".git" | "target" | "dist" | "build" | ".next" | ".turbo"
        ) {
            continue;
        }

        let skill_file = path.join("SKILL.md");
        if skill_file.exists() && is_project_skill_dir(&path) {
            let key = path.display().to_string();
            if seen.insert(key) {
                results.push(discovered_skill(root, &path, &skill_file));
            }
        }
        discover_in_dir(root, &path, depth + 1, seen, results);
        if results.len() >= 400 {
            break;
        }
    }
}

fn is_project_skill_dir(path: &Path) -> bool {
    let text = path.display().to_string();
    text.contains("/.skills/")
        || text.ends_with("/.skills")
        || text.contains("/.agents/skills/")
        || text.ends_with("/.agents/skills")
        || text.contains("/.claude/skills/")
        || text.ends_with("/.claude/skills")
}

fn discovered_skill(root: &Path, dir_path: &Path, skill_file: &Path) -> DiscoveredSkill {
    let fm = parse_frontmatter(skill_file);
    let name = fm.get("name").cloned().unwrap_or_else(|| {
        dir_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    });
    let project_path = project_path_for(root, dir_path);
    let project_name = project_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    DiscoveredSkill {
        id: format!("{}:{}", project_path.display(), name),
        name,
        description: fm.get("description").cloned().unwrap_or_default(),
        file_path: skill_file.display().to_string(),
        dir_path: dir_path.display().to_string(),
        project_path: project_path.display().to_string(),
        project_name,
        source_kind: discover_source_kind(dir_path),
    }
}

fn project_path_for(root: &Path, dir_path: &Path) -> PathBuf {
    let components = dir_path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let value = component.as_os_str().to_string_lossy();
        if matches!(value.as_ref(), ".skills" | ".agents" | ".claude") {
            let mut project = PathBuf::new();
            for part in &components[..index] {
                project.push(part.as_os_str());
            }
            return project;
        }
    }
    root.to_path_buf()
}

fn discover_source_kind(dir_path: &Path) -> String {
    let text = dir_path.display().to_string();
    let prefix = if is_obsidian_vault_skill(dir_path) {
        "obsidian/"
    } else {
        ""
    };
    if text.contains("/.agents/skills") {
        format!("{prefix}.agents/skills")
    } else if text.contains("/.claude/skills") {
        format!("{prefix}.claude/skills")
    } else {
        format!("{prefix}.skills")
    }
}

fn is_obsidian_vault_skill(dir_path: &Path) -> bool {
    for ancestor in dir_path.ancestors() {
        if ancestor.join(".obsidian").is_dir() {
            return true;
        }
        let text = ancestor.display().to_string();
        if text.ends_with(".obsidian") {
            return true;
        }
    }
    false
}

fn scan_directory(dir: &Path, agent: &str, results: &mut Vec<ScannedSkill>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let disabled_skills = disabled_skill_ids(agent);

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let index = find_index_file(&path);
            if let Some(index_path) = index {
                let fm = parse_frontmatter(&index_path);
                let skill_name = fm.get("name").cloned().unwrap_or_else(|| {
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                });
                let desc = fm.get("description").cloned().unwrap_or_default();
                let meta = fs::metadata(&path).ok();
                let is_symlink = entry
                    .path()
                    .symlink_metadata()
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false);
                let link_target = if is_symlink {
                    symlink_target(&path)
                } else {
                    None
                };

                results.push(ScannedSkill {
                    id: skill_name.clone(),
                    name: skill_name.clone(),
                    description: desc,
                    skill_type: SkillType::Skill,
                    icon: None,
                    source: SkillSource::Local,
                    origin_url: None,
                    has_update: false,
                    file_path: path.display().to_string(),
                    file_size: dir_size(&path),
                    modified_at: meta
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                    agents: vec![AgentSkillState {
                        agent: agent.to_string(),
                        install_path: path.display().to_string(),
                        link_target,
                        install_mode: if is_symlink {
                            InstallMode::Symlink
                        } else {
                            InstallMode::Direct
                        },
                        enabled: skill_enabled(agent, &path, &skill_name, &disabled_skills),
                    }],
                    frontmatter: fm,
                });
            }
        } else if path.extension().map(|e| e == "md").unwrap_or(false) {
            let fm = parse_frontmatter(&path);
            let skill_name = fm.get("name").cloned().unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            });
            let desc = fm.get("description").cloned().unwrap_or_default();
            let meta = fs::metadata(&path).ok();

            results.push(ScannedSkill {
                id: skill_name.clone(),
                name: skill_name.clone(),
                description: desc,
                skill_type: SkillType::Skill,
                icon: None,
                source: SkillSource::Local,
                origin_url: None,
                has_update: false,
                file_path: path.display().to_string(),
                file_size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified_at: meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                agents: vec![AgentSkillState {
                    agent: agent.to_string(),
                    install_path: path.display().to_string(),
                    link_target: None,
                    install_mode: InstallMode::Direct,
                    enabled: skill_enabled(agent, &path, &skill_name, &disabled_skills),
                }],
                frontmatter: fm,
            });
        }
    }
}

fn disabled_skill_ids(agent: &str) -> HashSet<String> {
    let Some(settings_path) = agent_paths::paths_for_agent(agent).settings_file else {
        return HashSet::new();
    };
    let Ok(content) = fs::read_to_string(&settings_path) else {
        return HashSet::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return HashSet::new();
    };
    if agent == "zcode" {
        return zcode_config::disabled_skill_paths(&json);
    }
    json.get("disabledSkills")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn skill_enabled(agent: &str, path: &Path, skill_name: &str, disabled: &HashSet<String>) -> bool {
    if agent == "zcode" {
        !disabled.contains(path.to_string_lossy().as_ref())
    } else {
        !disabled.contains(skill_name)
    }
}

fn scan_mcp_config(config_path: &Path, agent: &str, results: &mut Vec<ScannedSkill>) {
    let content = match fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let servers = if agent == "zcode" {
        zcode_config::mcp_servers(&json)
    } else {
        json.get("mcpServers")
            .or_else(|| json.get("mcp_servers"))
            .and_then(|v| v.as_object())
    };

    let servers = match servers {
        Some(s) => s,
        None => return,
    };
    let disabled: HashSet<String> = json
        .get("disabledMcpServers")
        .or_else(|| json.get("disabled_mcp_servers"))
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default();

    let meta = fs::metadata(config_path).ok();
    let modified_at = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    for (name, value) in servers {
        let command = value.get("command").and_then(|v| v.as_str()).unwrap_or("");
        let args_vec: Vec<String> = value
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let desc = if command.is_empty() {
            format!("MCP server: {}", name)
        } else {
            format!("{} {}", command, args_vec.join(" "))
        };

        let id = format!("mcp:{}", name);
        if results.iter().any(|s| s.id == id) {
            continue;
        }

        let mut frontmatter = std::collections::HashMap::new();
        frontmatter.insert("command".to_string(), command.to_string());
        if !args_vec.is_empty() {
            frontmatter.insert("args".to_string(), args_vec.join(" "));
        }
        if let Some(env) = value.get("env").and_then(|v| v.as_object()) {
            let mut keys: Vec<&str> = env.keys().map(|key| key.as_str()).collect();
            keys.sort();
            if !keys.is_empty() {
                frontmatter.insert("envKeys".to_string(), keys.join(", "));
            }
        }

        results.push(ScannedSkill {
            id,
            name: name.clone(),
            description: desc,
            skill_type: SkillType::Mcp,
            icon: None,
            source: SkillSource::Local,
            origin_url: None,
            has_update: false,
            file_path: config_path.display().to_string(),
            file_size: 0,
            modified_at,
            agents: vec![AgentSkillState {
                agent: agent.to_string(),
                install_path: config_path.display().to_string(),
                link_target: None,
                install_mode: InstallMode::Direct,
                enabled: if agent == "zcode" {
                    value.get("enabled").and_then(|value| value.as_bool()) != Some(false)
                } else {
                    !disabled.contains(name)
                },
            }],
            frontmatter,
        });
    }
}

pub fn read_mcp_server_config(agent: &str, server_name: &str) -> Option<McpServerConfig> {
    let config_path = agent_paths::paths_for_agent(agent).mcp_config?;
    let content = fs::read_to_string(config_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let server = if agent == "zcode" {
        zcode_config::mcp_servers(&json)?.get(server_name)?
    } else {
        json.get("mcpServers")
            .or_else(|| json.get("mcp_servers"))?
            .get(server_name)?
    };
    let command = server.get("command")?.as_str()?.to_string();
    let args = server
        .get("args")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default();
    let env = server
        .get("env")
        .and_then(|value| value.as_object())
        .map(|items| {
            items
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(McpServerConfig {
        name: server_name.to_string(),
        command,
        args,
        env,
    })
}

fn scan_plugins(agent: &str, results: &mut Vec<ScannedSkill>) {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let roots = match agent {
        "claude-code" => vec![
            home.join(".claude").join("plugins").join("marketplaces"),
            home.join(".claude").join("plugins").join("cache"),
        ],
        "codex" => vec![
            home.join(".codex").join("plugins").join("marketplaces"),
            home.join(".codex").join("plugins").join("cache"),
        ],
        "zcode" => vec![home.join(".zcode/cli/plugins/cache")],
        _ => Vec::new(),
    };
    for root in roots {
        scan_plugin_root(&root, agent, results);
    }
}

fn scan_plugin_root(root: &Path, agent: &str, results: &mut Vec<ScannedSkill>) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(
            name.as_str(),
            "node_modules" | ".git" | "target" | "dist" | "build"
        ) {
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        scan_plugin_candidate(&path, agent, results);
        scan_plugin_root(&path, agent, results);
    }
}

fn scan_plugin_candidate(path: &Path, agent: &str, results: &mut Vec<ScannedSkill>) {
    let manifest_path = path
        .join(".claude-plugin")
        .join("plugin.json")
        .exists()
        .then(|| path.join(".claude-plugin").join("plugin.json"))
        .or_else(|| {
            path.join(".codex-plugin")
                .join("plugin.json")
                .exists()
                .then(|| path.join(".codex-plugin").join("plugin.json"))
        })
        .or_else(|| {
            path.join(".zcode-plugin")
                .join("plugin.json")
                .exists()
                .then(|| path.join(".zcode-plugin").join("plugin.json"))
        });

    if manifest_path.is_none() {
        return;
    }

    let manifest = manifest_path
        .as_ref()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok());
    let name = manifest
        .as_ref()
        .and_then(|json| {
            json.pointer("/interface/displayName")
                .or_else(|| json.get("displayName"))
                .or_else(|| json.get("name"))
                .and_then(|v| v.as_str())
        })
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
    let plugin_id = manifest
        .as_ref()
        .and_then(|json| json.get("name").and_then(|v| v.as_str()))
        .map(ToString::to_string)
        .unwrap_or_else(|| name.clone());
    let description = manifest
        .as_ref()
        .and_then(|json| {
            json.pointer("/interface/shortDescription")
                .or_else(|| json.get("description"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string();
    let version = manifest
        .as_ref()
        .and_then(|json| json.get("version").and_then(|v| v.as_str()));
    let mut frontmatter = std::collections::HashMap::new();
    frontmatter.insert("pluginId".to_string(), plugin_id.clone());
    if let Some(version) = version {
        frontmatter.insert("version".to_string(), version.to_string());
    }

    let plugin_config_key = plugin_config_key(path, agent, &plugin_id);
    let id = format!(
        "plugin:{}",
        if agent == "zcode" {
            &plugin_config_key
        } else {
            &plugin_id
        }
    );
    if results.iter().any(|skill| skill.id == id) {
        return;
    }

    let meta = fs::metadata(path).ok();
    results.push(ScannedSkill {
        id,
        name,
        description,
        skill_type: SkillType::Plugin,
        icon: None,
        source: SkillSource::Local,
        origin_url: manifest
            .as_ref()
            .and_then(|json| {
                json.get("repository")
                    .or_else(|| json.get("homepage"))
                    .and_then(|v| v.as_str())
            })
            .map(ToString::to_string),
        has_update: false,
        file_path: path.display().to_string(),
        file_size: dir_size(path),
        modified_at: meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0),
        agents: vec![AgentSkillState {
            agent: agent.to_string(),
            install_path: path.display().to_string(),
            link_target: None,
            install_mode: InstallMode::Direct,
            enabled: plugin_enabled(agent, &plugin_config_key, &plugin_id),
        }],
        frontmatter,
    });
}

fn plugin_config_key(path: &Path, agent: &str, plugin_id: &str) -> String {
    if !matches!(agent, "codex" | "zcode") {
        return plugin_id.to_string();
    }
    let mut components = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string());
    while let Some(component) = components.next() {
        if component == "cache" {
            if let Some(source) = components.next() {
                return format!("{plugin_id}@{source}");
            }
        }
    }
    plugin_id.to_string()
}

fn plugin_enabled(agent: &str, plugin_key: &str, plugin_id: &str) -> bool {
    let Some(settings_path) = agent_paths::paths_for_agent(agent).settings_file else {
        return true;
    };
    let Ok(content) = fs::read_to_string(&settings_path) else {
        return true;
    };
    if settings_path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
        let enabled_plugins = crate::skills::codex_config::parse_plugin_enabled_config(&content);
        return enabled_plugins
            .get(plugin_key)
            .or_else(|| enabled_plugins.get(plugin_id))
            .copied()
            .unwrap_or(true);
    }
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return true;
    };
    let enabled_plugins = if agent == "zcode" {
        zcode_config::enabled_plugins(&json)
    } else {
        json.get("enabledPlugins")
            .and_then(|value| value.as_object())
    };
    enabled_plugins
        .and_then(|plugins| plugins.get(plugin_key).or_else(|| plugins.get(plugin_id)))
        .and_then(|value| value.as_bool())
        .unwrap_or_else(|| {
            if agent == "zcode" {
                dirs::home_dir()
                    .map(|home| {
                        home.join(".zcode/cli/plugins/data")
                            .join(plugin_key)
                            .exists()
                    })
                    .unwrap_or(false)
            } else {
                true
            }
        })
}

fn symlink_target(path: &Path) -> Option<String> {
    let target = fs::read_link(path).ok()?;
    let absolute = if target.is_absolute() {
        target
    } else {
        path.parent()
            .map(|parent| parent.join(&target))
            .unwrap_or(target)
    };
    Some(absolute.display().to_string())
}

fn find_index_file(dir: &Path) -> Option<PathBuf> {
    for name in &["SKILL.md", "index.md", "README.md", "main.md"] {
        let p = dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
        .map(|e| e.path())
}

fn parse_frontmatter(path: &Path) -> std::collections::HashMap<String, String> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return std::collections::HashMap::new(),
    };

    if !content.starts_with("---") {
        return std::collections::HashMap::new();
    }

    let parts: Vec<&str> = content.split("---").collect();
    if parts.len() < 3 {
        return std::collections::HashMap::new();
    }

    let fm_text = parts[1];
    parse_frontmatter_text(fm_text)
}

fn parse_frontmatter_text(fm_text: &str) -> std::collections::HashMap<String, String> {
    crate::skills::frontmatter::parse_section(fm_text)
        .into_iter()
        .collect()
}

fn expand_user_path(path: &str) -> PathBuf {
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
    PathBuf::from(path)
}

fn dir_size(path: &Path) -> u64 {
    fs::read_dir(path)
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .map(|e| {
                    let p = e.path();
                    if p.is_file() {
                        fs::metadata(&p).map(|m| m.len()).unwrap_or(0)
                    } else if p.is_dir() {
                        dir_size(&p)
                    } else {
                        0
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

pub fn read_file_tree(skill_path: &str) -> super::FileTreeNode {
    let path = PathBuf::from(skill_path);
    build_tree(&path)
}

fn build_tree(path: &Path) -> super::FileTreeNode {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    if path.is_file() {
        return super::FileTreeNode {
            name,
            node_type: "file".to_string(),
            path: path.display().to_string(),
            children: None,
        };
    }

    let children: Vec<super::FileTreeNode> = fs::read_dir(path)
        .ok()
        .map(|entries| {
            let mut nodes: Vec<_> = entries
                .flatten()
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .map(|e| build_tree(&e.path()))
                .collect();
            nodes.sort_by(|a, b| {
                let a_dir = a.node_type == "dir";
                let b_dir = b.node_type == "dir";
                b_dir.cmp(&a_dir).then(a.name.cmp(&b.name))
            });
            nodes
        })
        .unwrap_or_default();

    super::FileTreeNode {
        name,
        node_type: "dir".to_string(),
        path: path.display().to_string(),
        children: Some(children),
    }
}
