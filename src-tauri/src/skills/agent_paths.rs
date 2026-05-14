use std::path::PathBuf;

pub struct SkillPaths {
    pub skill_dirs: Vec<PathBuf>,
    pub mcp_config: Option<PathBuf>,
    pub settings_file: Option<PathBuf>,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| std::env::temp_dir())
}

pub fn paths_for_agent(agent: &str) -> SkillPaths {
    let h = home();
    match agent {
        "claude-code" => SkillPaths {
            skill_dirs: vec![h.join(".claude").join("skills")],
            mcp_config: Some(h.join(".claude").join("settings.json")),
            settings_file: Some(h.join(".claude").join("settings.json")),
        },
        "codex" => SkillPaths {
            skill_dirs: vec![
                h.join(".codex").join("skills"),
                h.join(".agents").join("skills"),
                h.join(".codex").join("agents"),
            ],
            mcp_config: Some(h.join(".codex").join("config.json")),
            settings_file: Some(h.join(".codex").join("config.json")),
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
        "antigravity" | "cline" | "deep-agents" | "dexto" | "firebender" | "warp" => SkillPaths {
            skill_dirs: vec![h.join(".agents").join("skills")],
            mcp_config: None,
            settings_file: None,
        },
        "opencode" => basic_skill_paths(&h, ".opencode/skills"),
        "qoder" | "qoder-cli" => basic_skill_paths(&h, ".qoder/skills"),
        "qwen" => basic_skill_paths(&h, ".qwen/skills"),
        "kimi" | "kimi-code-cli" => basic_skill_paths(&h, ".agents/skills"),
        "droid" | "factory-droid" => basic_skill_paths(&h, ".factory/skills"),
        "stepfun" => basic_skill_paths(&h, ".stepfun/skills"),
        "codebuddy" | "codebuddycn" => basic_skill_paths(&h, ".codebuddy/skills"),
        "trae" => basic_skill_paths(&h, ".trae/skills"),
        "traecn" | "trae-cn" => basic_skill_paths(&h, ".trae-cn/skills"),
        "workbuddy" => basic_skill_paths(&h, ".workbuddy/skills-marketplace/skills"),
        "copilot" => basic_skill_paths(&h, ".copilot/skills"),
        "kiro" => basic_skill_paths(&h, ".kiro/skills"),
        "pi" => basic_skill_paths(&h, ".pi/agent/skills"),
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

fn basic_skill_paths(home: &PathBuf, relative: &str) -> SkillPaths {
    SkillPaths {
        skill_dirs: vec![home.join(relative)],
        mcp_config: None,
        settings_file: None,
    }
}

pub fn known_agent_ids() -> &'static [&'static str] {
    &[
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
        "antigravity",
        "trae",
        "traecn",
        "qwen",
        "kimi",
        "droid",
        "stepfun",
        "codebuddy",
        "codebuddycn",
        "workbuddy",
        "kiro",
        "pi",
    ]
}

pub fn agentbro_skills_dir() -> PathBuf {
    home().join(".agentbro").join("skills")
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
    PathBuf::from(path)
}
