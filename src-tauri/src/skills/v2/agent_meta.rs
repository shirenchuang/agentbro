//! Agent display metadata + skill-dir resolution for Skill Manager v2.
//!
//! Reuses the existing `agent_paths` resolver for on-disk locations and layers
//! a stable display-name + icon-key mapping on top.

use crate::skills::agent_paths;
use std::path::PathBuf;

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
        "claude-code",
        "codex",
        "gemini",
        "cursor",
        "opencode",
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

fn table() -> &'static [AgentMeta] {
    &[
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
        "claude-code" => ".claude/skills",
        "codex" => ".codex/skills",
        "gemini" => ".gemini/skills",
        "cursor" => ".cursor/skills",
        "opencode" => ".opencode/skills",
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

/// Whether the agent's skills directory (or config) exists on disk — used to
/// decide if the agent is "installed".
pub fn agent_installed(home: &std::path::Path, agent: &str) -> bool {
    if let Some(dir) = agent_skills_dir(home, agent) {
        if dir.exists() {
            return true;
        }
    }
    // fall back to the agent's config dir presence
    matches!(agent_paths::paths_for_agent(agent).skill_dirs.first(), Some(d) if d.exists())
}
