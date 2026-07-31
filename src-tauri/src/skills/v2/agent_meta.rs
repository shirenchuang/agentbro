//! Agent display metadata + skill-dir resolution for Skill Manager v2.
//!
//! Reuses the existing `agent_paths` resolver for on-disk locations and layers
//! a stable display-name + icon-key mapping on top.

use crate::skills::{agent_paths, registry};
use std::path::{Path, PathBuf};

pub struct AgentMeta {
    pub id: &'static str,
    pub display_name: &'static str,
    pub icon_key: &'static str,
}

pub fn display_name(id: &str) -> String {
    table()
        .iter()
        .find(|m| m.id == id)
        .map(|m| m.display_name.to_string())
        .or_else(|| {
            registry::list_custom_agents()
                .into_iter()
                .find(|agent| agent.id == id && agent.is_enabled)
                .map(|agent| agent.display_name)
        })
        .unwrap_or_else(|| humanize(id))
}

pub fn icon_key(id: &str) -> String {
    table()
        .iter()
        .find(|m| m.id == id)
        .map(|m| m.icon_key.to_string())
        .or_else(|| {
            registry::list_custom_agents()
                .into_iter()
                .find(|agent| agent.id == id && agent.is_enabled)
                .and_then(|agent| agent.icon_name)
        })
        .unwrap_or_else(|| id.to_string())
}

/// Agents that can be managed targets, in canonical display order.
pub fn managed_agent_ids() -> Vec<String> {
    let mut ids = vec![
        "agents",
        "claude-code",
        "codex",
        "gemini",
        "antigravity",
        "cursor",
        "opencode",
        "openclaw",
        "qclaw",
        "easyclaw",
        "easyclaw-v2",
        "autoclaw",
        "copilot",
        "qwen",
        "kimi",
        "doubao",
        "deepseek",
        "workbuddy",
        "zcode",
        "windsurf",
        "augment",
        "kilocode",
        "aider",
        "amp",
        "kiro",
        "hermes",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect::<Vec<_>>();

    for agent in registry::list_custom_agents() {
        if agent.is_enabled && !ids.iter().any(|id| id == &agent.id) {
            ids.push(agent.id);
        }
    }

    ids
}

/// Real coding agents shown in Agent management views. The shared `.agents`
/// skill directory is scanned as an import source, but is not an agent.
pub fn visible_agent_ids() -> Vec<String> {
    managed_agent_ids()
        .into_iter()
        .filter(|id| id != "agents")
        .collect()
}

fn table() -> &'static [AgentMeta] {
    &[
        AgentMeta {
            id: "agents",
            display_name: ".agents",
            icon_key: "agents",
        },
        AgentMeta {
            id: "claude-code",
            display_name: "Claude Code",
            icon_key: "claude-code",
        },
        AgentMeta {
            id: "codex",
            display_name: "Codex",
            icon_key: "codex",
        },
        AgentMeta {
            id: "gemini",
            display_name: "Gemini CLI",
            icon_key: "gemini",
        },
        AgentMeta {
            id: "antigravity",
            display_name: "Antigravity",
            icon_key: "antigravity",
        },
        AgentMeta {
            id: "cursor",
            display_name: "Cursor",
            icon_key: "cursor",
        },
        AgentMeta {
            id: "opencode",
            display_name: "OpenCode",
            icon_key: "opencode",
        },
        AgentMeta {
            id: "openclaw",
            display_name: "OpenClaw",
            icon_key: "openclaw",
        },
        AgentMeta {
            id: "qclaw",
            display_name: "QClaw",
            icon_key: "qclaw",
        },
        AgentMeta {
            id: "easyclaw",
            display_name: "EasyClaw",
            icon_key: "easyclaw",
        },
        AgentMeta {
            id: "easyclaw-v2",
            display_name: "EasyClaw V2",
            icon_key: "easyclaw",
        },
        AgentMeta {
            id: "autoclaw",
            display_name: "AutoClaw",
            icon_key: "autoclaw",
        },
        AgentMeta {
            id: "copilot",
            display_name: "Copilot",
            icon_key: "copilot",
        },
        AgentMeta {
            id: "qwen",
            display_name: "Qwen",
            icon_key: "qwen",
        },
        AgentMeta {
            id: "kimi",
            display_name: "Kimi Code",
            icon_key: "kimi",
        },
        AgentMeta {
            id: "doubao",
            display_name: "Doubao",
            icon_key: "doubao",
        },
        AgentMeta {
            id: "deepseek",
            display_name: "DeepSeek",
            icon_key: "deepseek",
        },
        AgentMeta {
            id: "workbuddy",
            display_name: "WorkBuddy",
            icon_key: "workbuddy",
        },
        AgentMeta {
            id: "zcode",
            display_name: "ZCode",
            icon_key: "zcode",
        },
        AgentMeta {
            id: "windsurf",
            display_name: "Windsurf",
            icon_key: "windsurf",
        },
        AgentMeta {
            id: "augment",
            display_name: "Augment",
            icon_key: "augment",
        },
        AgentMeta {
            id: "kilocode",
            display_name: "Kilo Code",
            icon_key: "kilocode",
        },
        AgentMeta {
            id: "aider",
            display_name: "Aider",
            icon_key: "aider",
        },
        AgentMeta {
            id: "amp",
            display_name: "Amp",
            icon_key: "amp",
        },
        AgentMeta {
            id: "kiro",
            display_name: "Kiro",
            icon_key: "kiro",
        },
        AgentMeta {
            id: "hermes",
            display_name: "Hermes",
            icon_key: "hermes",
        },
    ]
}

fn humanize(id: &str) -> String {
    id.split(['-', '_'])
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolve the primary skills directory for an agent under a given home.
pub fn agent_skills_dir(home: &std::path::Path, agent: &str) -> Option<PathBuf> {
    // Use the existing resolver by temporarily relying on its home() helper.
    // agent_paths uses dirs::home_dir(), so we pass-through; for tests we set
    // HOME env. To support a custom home we replicate the minimal mapping here.
    let rel = match agent {
        "openclaw" => return Some(openclaw_workspace_dir(home).join("skills")),
        "agents" => ".agents/skills",
        "claude-code" => ".claude/skills",
        "codex" => ".codex/skills",
        "gemini" => ".gemini/skills",
        "antigravity" => ".gemini/config/skills",
        "cursor" => ".cursor/skills",
        "opencode" => ".opencode/skills",
        "qclaw" => ".qclaw/skills",
        "easyclaw" => ".easyclaw/skills",
        "easyclaw-v2" => ".easyclaw-20260322-01/skills",
        "autoclaw" => ".openclaw-autoclaw/skills",
        "copilot" => ".copilot/skills",
        "qwen" => ".qwen/skills",
        "kimi" => return Some(agent_paths::kimi_code_home_for(home).join("skills")),
        "doubao" => return Some(agent_paths::doubao_user_skills_dir_for(home)),
        "deepseek" => ".deepseek/skills",
        "workbuddy" => ".workbuddy/skills",
        "zcode" => ".zcode/skills",
        "windsurf" => ".windsurf/skills",
        "augment" => ".augment/skills",
        "kilocode" => ".kilocode/skills",
        "aider" => ".aider/skills",
        "amp" => ".amp/skills",
        "kiro" => ".kiro/skills",
        "hermes" => ".hermes/skills",
        _ => {
            return agent_paths::paths_for_agent(agent)
                .skill_dirs
                .first()
                .cloned()
        }
    };
    Some(home.join(rel))
}

/// Resolve every skill root OpenClaw can load from local filesystem sources,
/// ordered from highest precedence to lowest.
pub fn agent_skill_dirs(home: &Path, agent: &str) -> Vec<PathBuf> {
    if agent == "doubao" {
        return agent_paths::doubao_skill_dirs_for(home);
    }
    if agent != "openclaw" {
        if table().iter().all(|m| m.id != agent) {
            return agent_paths::paths_for_agent(agent).skill_dirs;
        }
        return agent_skills_dir(home, agent).into_iter().collect();
    }
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

pub fn agent_owned_skill_dirs(home: &Path, agent: &str) -> Vec<PathBuf> {
    if agent == "doubao" {
        return vec![agent_paths::doubao_user_skills_dir_for(home)];
    }
    if agent == "agents" {
        return vec![home.join(".agents").join("skills")];
    }
    if agent == "openclaw" {
        return vec![
            openclaw_workspace_dir(home).join("skills"),
            home.join(".openclaw").join("skills"),
            home.join(".openclaw").join("plugin-skills"),
        ];
    }
    let shared = home.join(".agents").join("skills");
    agent_skill_dirs(home, agent)
        .into_iter()
        .filter(|path| path != &shared)
        .collect()
}

pub fn agent_read_only_skill_dirs(home: &Path, agent: &str) -> Vec<PathBuf> {
    if agent == "doubao" {
        return agent_paths::doubao_builtin_skill_dirs_for(home);
    }
    Vec::new()
}

pub fn is_read_only_agent_skill_path(home: &Path, agent: &str, path: &Path) -> bool {
    agent_read_only_skill_dirs(home, agent)
        .into_iter()
        .any(|root| path.strip_prefix(root).is_ok())
}

/// Runtime status includes the program and Agent-owned residual capabilities;
/// fixture homes keep the broader path-based behavior used by service tests.
pub fn agent_installed(home: &std::path::Path, agent: &str) -> bool {
    #[cfg(not(target_os = "windows"))]
    if is_runtime_home(home) && crate::agents::programs::has_program_metadata(agent) {
        if agent_program_installed(agent) {
            return true;
        }
        if agent_owned_skill_dirs(home, agent)
            .into_iter()
            .any(|path| directory_has_valid_skill(&path, agent == "openclaw"))
        {
            return true;
        }
        let paths = agent_paths::paths_for_agent(agent);
        return [paths.settings_file, paths.mcp_config]
            .into_iter()
            .flatten()
            .any(|path| crate::agents::hook_manager::has_agentbro_hooks(&path));
    }

    for dir in agent_skill_dirs(home, agent) {
        if dir.exists() {
            return true;
        }
    }
    let paths = agent_paths::paths_for_agent(agent);
    if paths
        .settings_file
        .as_ref()
        .is_some_and(|path| path.exists())
        || paths.mcp_config.as_ref().is_some_and(|path| path.exists())
        || paths.skill_dirs.first().is_some_and(|path| path.exists())
    {
        return true;
    }

    #[cfg(target_os = "windows")]
    {
        // Keep the Skill Manager list cheap on Windows. Program probing walks
        // PATH and can block the settings window while every agent is listed.
        false
    }

    #[cfg(not(target_os = "windows"))]
    false
}

fn directory_has_valid_skill(path: &Path, recursive: bool) -> bool {
    directory_has_valid_skill_inner(path, recursive, 0)
}

fn directory_has_valid_skill_inner(path: &Path, recursive: bool, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    std::fs::read_dir(path).ok().is_some_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if crate::skills::v2::fsutil::is_ignored_entry(&name) || name.starts_with('.') {
                return false;
            }
            let child = entry.path();
            child.is_dir()
                && (crate::skills::v2::fsutil::is_skill_dir(&child)
                    || recursive && directory_has_valid_skill_inner(&child, true, depth + 1))
        })
    })
}

#[cfg(not(target_os = "windows"))]
fn agent_program_installed(agent: &str) -> bool {
    use crate::agents::AdapterStatus;

    matches!(
        crate::agents::programs::detected_status_for_agent_program(agent),
        AdapterStatus::Active | AdapterStatus::Installed | AdapterStatus::Available
    )
}

#[cfg(not(target_os = "windows"))]
fn is_runtime_home(home: &Path) -> bool {
    same_path(home, &crate::skills::v2::fsutil::home())
}

#[cfg(not(target_os = "windows"))]
fn same_path(a: &Path, b: &Path) -> bool {
    let a = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
    let b = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    a == b
}

#[cfg(all(target_os = "windows", test))]
fn same_path(a: &Path, b: &Path) -> bool {
    let a = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
    let b = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    a.to_string_lossy().to_ascii_lowercase() == b.to_string_lossy().to_ascii_lowercase()
}

fn openclaw_workspace_dir(home: &Path) -> PathBuf {
    let config_path = home.join(".openclaw").join("openclaw.json");
    let workspace = std::fs::read_to_string(config_path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|json| {
            json.pointer("/agents/defaults/workspace")
                .and_then(|value| value.as_str())
                .map(|value| expand_home(home, value))
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

fn expand_home(home: &Path, value: &str) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workbuddy_is_visible_and_uses_installed_skills_dir() {
        assert!(visible_agent_ids().iter().any(|id| id == "workbuddy"));
        assert_eq!(display_name("workbuddy"), "WorkBuddy");
        assert_eq!(icon_key("workbuddy"), "workbuddy");

        let home = Path::new("/Users/tester");
        assert_eq!(
            agent_skills_dir(home, "workbuddy"),
            Some(home.join(".workbuddy/skills"))
        );
    }

    #[test]
    fn owned_skill_dirs_include_shared_inventory_root() {
        let home = Path::new("/Users/tester");
        assert_eq!(
            agent_owned_skill_dirs(home, "agents"),
            vec![home.join(".agents/skills")]
        );
        assert_eq!(
            agent_owned_skill_dirs(home, "codex"),
            vec![home.join(".codex/skills")]
        );
        assert!(!agent_owned_skill_dirs(home, "openclaw")
            .iter()
            .any(|path| path == &home.join(".agents/skills")));
    }

    #[test]
    fn skill_manifest_alone_is_not_an_installed_capability() {
        let root = std::env::temp_dir().join(format!(
            "agentbro-agent-meta-skills-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".skills-manifest.json"), "{}").unwrap();

        assert!(!directory_has_valid_skill(&root, false));

        let skill = root.join("release-checklist");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "# Release checklist").unwrap();
        assert!(directory_has_valid_skill(&root, false));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zcode_is_visible_and_uses_official_skills_dir() {
        assert!(visible_agent_ids().iter().any(|id| id == "zcode"));
        assert_eq!(display_name("zcode"), "ZCode");
        assert_eq!(icon_key("zcode"), "zcode");

        let home = Path::new("/Users/tester");
        assert_eq!(
            agent_skills_dir(home, "zcode"),
            Some(home.join(".zcode/skills"))
        );
    }

    #[test]
    fn antigravity_is_visible_and_uses_official_skills_dir() {
        assert!(visible_agent_ids().iter().any(|id| id == "antigravity"));
        assert_eq!(display_name("antigravity"), "Antigravity");
        assert_eq!(icon_key("antigravity"), "antigravity");

        let home = Path::new("/Users/tester");
        assert_eq!(
            agent_skills_dir(home, "antigravity"),
            Some(home.join(".gemini/config/skills"))
        );
    }

    #[test]
    fn doubao_is_visible_and_separates_user_and_builtin_skills() {
        assert!(visible_agent_ids().iter().any(|id| id == "doubao"));
        assert_eq!(display_name("doubao"), "Doubao");
        assert_eq!(icon_key("doubao"), "doubao");

        let home = Path::new("/Users/tester");
        assert_eq!(
            agent_skills_dir(home, "doubao"),
            Some(home.join("Doubao/skills"))
        );
        assert_eq!(
            agent_owned_skill_dirs(home, "doubao"),
            vec![home.join("Doubao/skills")]
        );
        let builtin = home.join(
            "Library/Application Support/Doubao/Default/.doubao/agent_mode/workspace/.skills",
        );
        assert_eq!(
            agent_skill_dirs(home, "doubao"),
            vec![home.join("Doubao/skills"), builtin.clone()]
        );
        assert!(is_read_only_agent_skill_path(
            home,
            "doubao",
            &builtin.join("browser-task")
        ));
        assert!(!is_read_only_agent_skill_path(
            home,
            "doubao",
            &home.join("Doubao/skills/custom")
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_same_path_is_case_insensitive() {
        assert!(same_path(
            Path::new(r"C:\Users\AgentBro"),
            Path::new(r"c:\users\agentbro")
        ));
    }
}
