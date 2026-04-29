// KimiAdapter — Agent adapter for Moonshot Kimi (TOML config format)
// Uses sentinel-based TOML injection to avoid a toml dependency.

use std::path::PathBuf;
use super::{AdapterStatus, AgentAdapter, AgentEvent};

const BRIDGE_BINARY_NAME: &str = "agent-island-bridge";
const AGENT_ISLAND_MARKER: &str = "agent-island";
const TOML_BLOCK_START: &str = "# [AGENT-ISLAND-START]";
const TOML_BLOCK_END: &str = "# [AGENT-ISLAND-END]";

pub struct KimiAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl KimiAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| std::env::temp_dir());
        let config_root = home.join(".kimi");
        let status = if Self::is_installed() {
            AdapterStatus::Available
        } else {
            AdapterStatus::Unavailable
        };
        Self { config_root, status }
    }

    fn is_installed() -> bool {
        std::process::Command::new("which")
            .arg("kimi")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn bridge_binary_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| std::env::temp_dir());
        home.join(".agent-island").join("bin").join(BRIDGE_BINARY_NAME)
    }

    fn config_path(&self) -> PathBuf {
        self.config_root.join("config.toml")
    }

    fn build_toml_block(hook_command: &str) -> String {
        let cmd = hook_command.replace('"', "\\\"");
        format!(
            "{start}\n[[hooks]]\nevent = \"pre_tool_use\"\ncommand = \"{cmd}\"\n\n[[hooks]]\nevent = \"post_tool_use\"\ncommand = \"{cmd}\"\n\n[[hooks]]\nevent = \"session_start\"\ncommand = \"{cmd}\"\n{end}",
            start = TOML_BLOCK_START,
            cmd = cmd,
            end = TOML_BLOCK_END,
        )
    }

    fn strip_our_block(content: &str) -> String {
        let mut result = String::new();
        let mut inside_block = false;
        for line in content.lines() {
            if line.trim() == TOML_BLOCK_START {
                inside_block = true;
                continue;
            }
            if line.trim() == TOML_BLOCK_END {
                inside_block = false;
                continue;
            }
            if !inside_block {
                result.push_str(line);
                result.push('\n');
            }
        }
        result
    }

    fn has_our_block(content: &str) -> bool {
        content.contains(AGENT_ISLAND_MARKER)
    }
}

impl AgentAdapter for KimiAdapter {
    fn name(&self) -> &str { "kimi" }
    fn display_name(&self) -> &str { "Kimi" }
    fn icon(&self) -> &str { "kimi" }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = self.config_path();
        let hook_command = Self::bridge_binary_path().display().to_string();

        let existing = if config_path.exists() {
            std::fs::read_to_string(&config_path)?
        } else {
            String::new()
        };

        let stripped = Self::strip_our_block(&existing);
        let new_block = Self::build_toml_block(&hook_command);
        let new_content = if stripped.trim().is_empty() {
            new_block
        } else {
            format!("{}\n{}", stripped.trim_end(), new_block)
        };

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&config_path, new_content)?;
        log::info!("Kimi hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = self.config_path();
        if !config_path.exists() { return Ok(()); }

        let content = std::fs::read_to_string(&config_path)?;
        if !Self::has_our_block(&content) { return Ok(()); }

        let stripped = Self::strip_our_block(&content);
        std::fs::write(&config_path, stripped)?;
        log::info!("Kimi hooks removed");
        Ok(())
    }

    fn status(&self) -> AdapterStatus { self.status.clone() }

    fn parse_event(&self, raw: &serde_json::Value) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        let session_id = raw.get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let event = raw.get("event").and_then(|v| v.as_str()).unwrap_or("");
        match event {
            "session_start" => Ok(AgentEvent::SessionStart {
                session_id,
                project: raw.get("cwd").and_then(|v| v.as_str())
                    .and_then(|p| p.rsplit('/').next()).unwrap_or("").to_string(),
                cwd: raw.get("cwd").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                terminal: "".to_string(),
                agent_type: "kimi".to_string(),
            }),
            "session_end" => Ok(AgentEvent::SessionEnd { session_id }),
            _ => Ok(AgentEvent::Processing {
                session_id,
                description: format!("Event: {}", event),
            }),
        }
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![self.config_path()]
    }
}
