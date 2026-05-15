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
        "trae" | "traecli" => basic_skill_paths(&h, ".trae/skills"),
        "traecn" | "trae-cn" => basic_skill_paths(&h, ".trae-cn/skills"),
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
        "openclaw" => basic_skill_paths(&h, ".openclaw/skills"),
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
        "antigravity",
        "trae",
        "traecli",
        "traecn",
        "qwen",
        "kimi",
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
    PathBuf::from(path)
}
