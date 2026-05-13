use super::agent_paths;
use super::{AgentSkillState, InstallMode, ScannedSkill, SkillSource, SkillType};
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

    results
}

pub fn scan_all() -> std::collections::HashMap<String, Vec<ScannedSkill>> {
    let agents = ["claude-code", "codex", "gemini-cli", "cursor", "hermes"];
    let mut map = std::collections::HashMap::new();
    for agent in &agents {
        let skills = scan_agent(agent);
        if !skills.is_empty() {
            map.insert(agent.to_string(), skills);
        }
    }
    map
}

fn scan_directory(dir: &Path, agent: &str, results: &mut Vec<ScannedSkill>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

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

                results.push(ScannedSkill {
                    id: skill_name.clone(),
                    name: skill_name,
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
                        install_mode: if is_symlink {
                            InstallMode::Symlink
                        } else {
                            InstallMode::Direct
                        },
                        enabled: true,
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
                name: skill_name,
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
                    install_mode: InstallMode::Direct,
                    enabled: true,
                }],
                frontmatter: fm,
            });
        }
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

    let servers = json
        .get("mcpServers")
        .or_else(|| json.get("mcp_servers"))
        .and_then(|v| v.as_object());

    let servers = match servers {
        Some(s) => s,
        None => return,
    };

    let meta = fs::metadata(config_path).ok();
    let modified_at = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    for (name, value) in servers {
        let command = value.get("command").and_then(|v| v.as_str()).unwrap_or("");
        let desc = if command.is_empty() {
            format!("MCP server: {}", name)
        } else {
            let args = value
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            format!("{} {}", command, args)
        };

        let id = format!("mcp:{}", name);
        if results.iter().any(|s| s.id == id) {
            continue;
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
                install_mode: InstallMode::Direct,
                enabled: true,
            }],
            frontmatter: std::collections::HashMap::new(),
        });
    }
}

fn find_index_file(dir: &Path) -> Option<PathBuf> {
    for name in &["index.md", "README.md", "main.md"] {
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

    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return std::collections::HashMap::new();
    }

    let fm_text = parts[1];
    let mut map = std::collections::HashMap::new();

    for line in fm_text.lines() {
        let line = line.trim();
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim().to_string();
            let val = line[colon_pos + 1..]
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            if !key.is_empty() && !val.is_empty() {
                map.insert(key, val);
            }
        }
    }

    map
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
