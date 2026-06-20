//! Agent display metadata + skill-dir resolution for Skill Manager v2.
//!
//! Reuses the existing `agent_paths` resolver for on-disk locations and layers
//! a stable display-name + icon-key mapping on top.

use crate::skills::agent_paths;
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
        .unwrap_or_else(|| humanize(id))
}

pub fn icon_key(id: &str) -> String {
    table()
        .iter()
        .find(|m| m.id == id)
        .map(|m| m.icon_key.to_string())
        .unwrap_or_else(|| id.to_string())
}

/// Agents that can be managed targets, in canonical display order.
pub fn managed_agent_ids() -> Vec<&'static str> {
    vec![
        "agents",
        "claude-code",
        "codex",
        "gemini",
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
        "deepseek",
        "windsurf",
        "augment",
        "kilocode",
        "aider",
        "amp",
        "kiro",
        "hermes",
    ]
}

/// Real coding agents shown in Agent management views. The shared `.agents`
/// skill directory is scanned as an import source, but is not an agent.
pub fn visible_agent_ids() -> Vec<&'static str> {
    managed_agent_ids()
        .into_iter()
        .filter(|id| *id != "agents")
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
            display_name: "Kimi",
            icon_key: "kimi",
        },
        AgentMeta {
            id: "deepseek",
            display_name: "DeepSeek",
            icon_key: "deepseek",
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
        "cursor" => ".cursor/skills",
        "opencode" => ".opencode/skills",
        "qclaw" => ".qclaw/skills",
        "easyclaw" => ".easyclaw/skills",
        "easyclaw-v2" => ".easyclaw-20260322-01/skills",
        "autoclaw" => ".openclaw-autoclaw/skills",
        "copilot" => ".copilot/skills",
        "qwen" => ".qwen/skills",
        "kimi" => ".kimi/skills",
        "deepseek" => ".deepseek/skills",
        "windsurf" => ".windsurf/skills",
        "augment" => ".augment/skills",
        "kilocode" => ".kilocode/skills",
        "aider" => ".aider/skills",
        "amp" => ".amp/skills",
        "kiro" => ".kiro/skills",
        "hermes" => ".hermes/skills",
        _ => return None,
    };
    Some(home.join(rel))
}

/// Resolve every skill root OpenClaw can load from local filesystem sources,
/// ordered from highest precedence to lowest.
pub fn agent_skill_dirs(home: &Path, agent: &str) -> Vec<PathBuf> {
    if agent != "openclaw" {
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

/// Whether the agent's skills directory (or config) exists on disk — used to
/// decide if the agent is "installed".
pub fn agent_installed(home: &std::path::Path, agent: &str) -> bool {
    for dir in agent_skill_dirs(home, agent) {
        if dir.exists() {
            return true;
        }
    }
    // fall back to the agent's config dir presence
    matches!(agent_paths::paths_for_agent(agent).skill_dirs.first(), Some(d) if d.exists())
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
