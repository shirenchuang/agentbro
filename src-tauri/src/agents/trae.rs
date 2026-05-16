// TraeAdapter — Agent adapter for Trae AI (YAML config format)
// Uses sentinel-based YAML injection to avoid a serde_yaml dependency.

use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

const BRIDGE_BINARY_NAME: &str = "agentbro-bridge";
const AGENTBRO_MARKER: &str = "agentbro";
const YAML_BLOCK_START: &str = "# [AGENTBRO-START]";
const YAML_BLOCK_END: &str = "# [AGENTBRO-END]";

pub struct TraeAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl TraeAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".trae");
        let status = if Self::is_installed() {
            AdapterStatus::Available
        } else {
            AdapterStatus::Unavailable
        };
        Self {
            config_root,
            status,
        }
    }

    fn is_installed() -> bool {
        std::process::Command::new("which")
            .arg("trae")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn bridge_binary_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        home.join(".agentbro").join("bin").join(BRIDGE_BINARY_NAME)
    }

    fn config_path(&self) -> PathBuf {
        self.config_root.join("config.yaml")
    }

    fn build_yaml_block(hook_command: &str) -> String {
        format!(
            "{start}\nhooks:\n  pre_tool_use:\n    - command: \"{cmd}\"\n  post_tool_use:\n    - command: \"{cmd}\"\n  session_start:\n    - command: \"{cmd}\"\n{end}",
            start = YAML_BLOCK_START,
            cmd = hook_command.replace('"', "\\\""),
            end = YAML_BLOCK_END,
        )
    }

    fn strip_our_block(content: &str) -> String {
        let mut result = String::new();
        let mut inside_block = false;
        for line in content.lines() {
            if line.trim() == YAML_BLOCK_START {
                inside_block = true;
                continue;
            }
            if line.trim() == YAML_BLOCK_END {
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
        content.contains(AGENTBRO_MARKER)
    }
}

impl AgentAdapter for TraeAdapter {
    fn name(&self) -> &str {
        "trae"
    }
    fn display_name(&self) -> &str {
        "Trae"
    }
    fn icon(&self) -> &str {
        "trae"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = self.config_path();
        let hook_command = Self::bridge_binary_path().display().to_string();

        let existing = if config_path.exists() {
            std::fs::read_to_string(&config_path)?
        } else {
            String::new()
        };

        // Remove any previous block, then append new one
        let stripped = Self::strip_our_block(&existing);
        let new_block = Self::build_yaml_block(&hook_command);
        let new_content = if stripped.trim().is_empty() {
            new_block
        } else {
            format!("{}\n{}", stripped.trim_end(), new_block)
        };

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&config_path, new_content)?;
        log::info!("Trae hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = self.config_path();
        if !config_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&config_path)?;
        if !Self::has_our_block(&content) {
            return Ok(());
        }

        let stripped = Self::strip_our_block(&content);
        std::fs::write(&config_path, stripped)?;
        log::info!("Trae hooks removed");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }

    fn parse_event(
        &self,
        raw: &serde_json::Value,
    ) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        let session_id = raw
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let event = raw.get("event").and_then(|v| v.as_str()).unwrap_or("");
        match event {
            "session_start" => Ok(AgentEvent::SessionStart {
                session_id,
                project: raw
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .and_then(|p| p.rsplit('/').next())
                    .unwrap_or("")
                    .to_string(),
                cwd: raw
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                terminal: "".to_string(),
                agent_type: "trae".to_string(),
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
