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
            skill_dirs: vec![h.join(".codex").join("agents")],
            mcp_config: Some(h.join(".codex").join("config.json")),
            settings_file: Some(h.join(".codex").join("config.json")),
        },
        "gemini-cli" => SkillPaths {
            skill_dirs: vec![h.join(".gemini").join("skills")],
            mcp_config: Some(h.join(".gemini").join("settings.json")),
            settings_file: Some(h.join(".gemini").join("settings.json")),
        },
        "cursor" | "cursor-cli" => SkillPaths {
            skill_dirs: vec![h.join(".cursor").join("rules")],
            mcp_config: Some(h.join(".cursor").join("mcp.json")),
            settings_file: Some(h.join(".cursor").join("mcp.json")),
        },
        "hermes" => SkillPaths {
            skill_dirs: vec![h.join(".hermes").join("skills")],
            mcp_config: None,
            settings_file: None,
        },
        _ => SkillPaths {
            skill_dirs: vec![],
            mcp_config: None,
            settings_file: None,
        },
    }
}

pub fn agentbro_skills_dir() -> PathBuf {
    home().join(".agentbro").join("skills")
}

pub fn agentbro_metadata_path() -> PathBuf {
    home().join(".agentbro").join("metadata.json")
}
