use std::path::{Path, PathBuf};

pub struct SkillPaths {
    pub skill_dirs: Vec<PathBuf>,
    pub mcp_config: Option<PathBuf>,
    pub settings_file: Option<PathBuf>,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(std::env::temp_dir)
}

pub fn paths_for_agent(agent: &str) -> SkillPaths {
    let h = home();
    match agent {
        "central" | "agentbro" => SkillPaths {
            skill_dirs: central_skill_dirs(),
            mcp_config: None,
            settings_file: None,
        },
        "claude-code" => SkillPaths {
            skill_dirs: vec![h.join(".claude").join("skills")],
            mcp_config: Some(h.join(".claude").join("settings.json")),
            settings_file: Some(h.join(".claude").join("settings.json")),
        },
        "codex" => SkillPaths {
            skill_dirs: vec![
                h.join(".codex").join("skills"),
                h.join(".agents").join("skills"),
            ],
            mcp_config: Some(h.join(".codex").join("config.toml")),
            settings_file: Some(h.join(".codex").join("config.toml")),
        },
        "gemini" | "gemini-cli" => SkillPaths {
            skill_dirs: vec![h.join(".gemini").join("skills")],
            mcp_config: Some(h.join(".gemini").join("settings.json")),
            settings_file: Some(h.join(".gemini").join("settings.json")),
        },
        "cursor" | "cursor-cli" => SkillPaths {
            skill_dirs: vec![
                h.join(".cursor").join("skills"),
                h.join(".cursor").join("rules"),
            ],
            mcp_config: Some(h.join(".cursor").join("mcp.json")),
            settings_file: Some(h.join(".cursor").join("mcp.json")),
        },
        "agents" | "antigravity" | "cline" | "deep-agents" | "dexto" | "firebender" | "warp" => {
            SkillPaths {
                skill_dirs: vec![h.join(".agents").join("skills")],
                mcp_config: None,
                settings_file: None,
            }
        }
        "opencode" => basic_skill_paths(&h, ".opencode/skills"),
        "qoder" | "qoder-cli" => basic_skill_paths(&h, ".qoder/skills"),
        "qwen" => basic_skill_paths(&h, ".qwen/skills"),
        "kimi" | "kimi-code-cli" => basic_skill_paths(&h, ".kimi/skills"),
        "deepseek" => basic_skill_paths(&h, ".deepseek/skills"),
        "droid" | "factory-droid" => basic_skill_paths(&h, ".factory/skills"),
        "stepfun" => basic_skill_paths(&h, ".stepfun/skills"),
        "codebuddy" => basic_skill_paths(&h, ".codebuddy/skills"),
        "codebuddycn" | "codybuddycn" => SkillPaths {
            skill_dirs: vec![
                h.join(".codybuddycn").join("skills"),
                h.join(".codebuddycn").join("skills"),
                h.join(".codebuddy").join("skills"),
            ],
            mcp_config: None,
            settings_file: None,
        },
        "workbuddy" => basic_skill_paths(&h, ".workbuddy/skills-marketplace/skills"),
        "copilot" => basic_skill_paths(&h, ".copilot/skills"),
        "kiro" => basic_skill_paths(&h, ".kiro/skills"),
        "pi" => basic_skill_paths(&h, ".pi/agent/skills"),
        "junie" => basic_skill_paths(&h, ".junie/skills"),
        "windsurf" => SkillPaths {
            skill_dirs: vec![
                h.join(".windsurf").join("skills"),
                h.join(".codeium").join("windsurf").join("skills"),
            ],
            mcp_config: None,
            settings_file: None,
        },
        "augment" => basic_skill_paths(&h, ".augment/skills"),
        "kilocode" => basic_skill_paths(&h, ".kilocode/skills"),
        "ob1" => basic_skill_paths(&h, ".ob1/skills"),
        "amp" => basic_skill_paths(&h, ".amp/skills"),
        "aider" => basic_skill_paths(&h, ".aider/skills"),
        "openclaw" => SkillPaths {
            skill_dirs: openclaw_skill_dirs(&h),
            mcp_config: None,
            settings_file: Some(h.join(".openclaw").join("openclaw.json")),
        },
        "qclaw" => basic_skill_paths(&h, ".qclaw/skills"),
        "easyclaw" => basic_skill_paths(&h, ".easyclaw/skills"),
        "easyclaw-v2" => basic_skill_paths(&h, ".easyclaw-20260322-01/skills"),
        "autoclaw" => basic_skill_paths(&h, ".openclaw-autoclaw/skills"),
        "hermes" => SkillPaths {
            skill_dirs: vec![h.join(".hermes").join("skills")],
            mcp_config: None,
            settings_file: None,
        },
        _ => SkillPaths {
            skill_dirs: custom_agent_skill_dir(agent).into_iter().collect(),
            mcp_config: None,
            settings_file: None,
        },
    }
}

fn basic_skill_paths(home: &Path, relative: &str) -> SkillPaths {
    SkillPaths {
        skill_dirs: vec![home.join(relative)],
        mcp_config: None,
        settings_file: None,
    }
}

fn openclaw_skill_dirs(home: &Path) -> Vec<PathBuf> {
    let workspace = openclaw_workspace_dir(home);
    let mut dirs = vec![
        workspace.join("skills"),
        workspace.join(".agents").join("skills"),
        home.join(".agents").join("skills"),
        home.join(".openclaw").join("skills"),
        home.join(".openclaw").join("plugin-skills"),
    ];
    dirs.extend(openclaw_bundled_skill_dirs());
    dedupe_paths(dirs)
}

fn openclaw_workspace_dir(home: &Path) -> PathBuf {
    let config_path = home.join(".openclaw").join("openclaw.json");
    let workspace = std::fs::read_to_string(config_path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|json| {
            json.pointer("/agents/defaults/workspace")
                .and_then(|value| value.as_str())
                .map(|value| expand_home_with_base(home, value))
        });
    workspace.unwrap_or_else(|| home.join(".openclaw").join("workspace"))
}

fn openclaw_bundled_skill_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe) = find_in_path("openclaw") {
        if let Ok(real) = exe.canonicalize() {
            if let Some(parent) = real.parent() {
                dirs.push(parent.join("skills"));
            }
        }
    }
    dirs.push(PathBuf::from(
        "/opt/homebrew/lib/node_modules/openclaw/skills",
    ));
    dirs.push(PathBuf::from("/usr/local/lib/node_modules/openclaw/skills"));
    dirs
}

fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
}

fn expand_home_with_base(home: &Path, value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/") {
        home.join(rest)
    } else if cfg!(target_os = "windows") {
        value
            .strip_prefix("~\\")
            .map(|rest| home.join(rest))
            .unwrap_or_else(|| PathBuf::from(value))
    } else {
        PathBuf::from(value)
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::BTreeSet::new();
    paths
        .into_iter()
        .filter(|path| {
            let key = path.display().to_string();
            seen.insert(key)
        })
        .collect()
}

pub fn known_agent_ids() -> &'static [&'static str] {
    &[
        "central",
        "claude-code",
        "codex",
        "gemini",
        "gemini-cli",
        "cursor",
        "cursor-cli",
        "hermes",
        "opencode",
        "qoder",
        "qoder-cli",
        "copilot",
        "agents",
        "antigravity",
        "qwen",
        "kimi",
        "deepseek",
        "droid",
        "stepfun",
        "codebuddy",
        "codebuddycn",
        "codybuddycn",
        "workbuddy",
        "kiro",
        "pi",
        "factory-droid",
        "junie",
        "windsurf",
        "augment",
        "kilocode",
        "ob1",
        "amp",
        "aider",
        "openclaw",
        "qclaw",
        "easyclaw",
        "easyclaw-v2",
        "autoclaw",
    ]
}

pub fn agentbro_skills_dir() -> PathBuf {
    central_skills_dir()
}

pub fn central_skills_dir() -> PathBuf {
    home().join(".agents").join("skills")
}

pub fn legacy_agentbro_skills_dir() -> PathBuf {
    home().join(".agentbro").join("skills")
}

pub fn plugin_cache_dir(agent: &str) -> Option<PathBuf> {
    let h = home();
    match agent {
        "claude-code" => Some(h.join(".claude").join("plugins").join("cache")),
        "codex" => Some(h.join(".codex").join("plugins").join("cache")),
        _ => None,
    }
}

pub fn central_skill_dirs() -> Vec<PathBuf> {
    vec![central_skills_dir(), legacy_agentbro_skills_dir()]
}

pub fn agentbro_metadata_path() -> PathBuf {
    home().join(".agentbro").join("metadata.json")
}

fn custom_agent_skill_dir(agent: &str) -> Option<PathBuf> {
    let content = std::fs::read_to_string(agentbro_metadata_path()).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let custom_agents = json
        .get("customAgents")
        .or_else(|| json.get("custom_agents"))?
        .as_array()?;

    custom_agents.iter().find_map(|entry| {
        let id = entry.get("id")?.as_str()?;
        if id != agent {
            return None;
        }
        let path = entry
            .get("globalSkillsDir")
            .or_else(|| entry.get("global_skills_dir"))?
            .as_str()?;
        Some(expand_home(path))
    })
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
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn expands_windows_home_paths_with_base() {
        let home = PathBuf::from(r"C:\Users\agentbro");
        assert_eq!(
            expand_home_with_base(&home, r"~\.openclaw\workspace"),
            home.join(r".openclaw\workspace")
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn expands_custom_agent_windows_home_paths() {
        let expanded = expand_home(r"~\.agentbro\custom-skills");
        assert!(expanded.ends_with(r".agentbro\custom-skills"));
        assert!(!expanded.display().to_string().starts_with('~'));
    }
}
