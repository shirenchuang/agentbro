// HermesAdapter — Agent adapter for Hermes AI agent

use super::hook_manager;
use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

pub struct HermesAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl HermesAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".hermes");
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
            .arg("hermes")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn hooks_path(&self) -> PathBuf {
        self.config_root.join("hooks.json")
    }
}

impl AgentAdapter for HermesAdapter {
    fn name(&self) -> &str {
        "hermes"
    }
    fn display_name(&self) -> &str {
        "Hermes"
    }
    fn icon(&self) -> &str {
        "hermes"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.hooks_path();
        let hook_command = hook_manager::bridge_binary_path().display().to_string();
        let mut settings = hook_manager::read_json_config(&path);
        let events = &["PreToolUse", "PostToolUse", "Notification", "Stop"];
        hook_manager::inject_hooks_json(&mut settings, events, &hook_command);
        hook_manager::write_json_config(&path, &settings)?;
        log::info!("Hermes hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.hooks_path();
        if !path.exists() {
            return Ok(());
        }
        let mut settings = hook_manager::read_json_config(&path);
        hook_manager::remove_hooks_json(&mut settings);
        hook_manager::write_json_config(&path, &settings)?;
        log::info!("Hermes hooks removed");
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
            "session_start" | "SessionStart" => Ok(AgentEvent::SessionStart {
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
                agent_type: "hermes".to_string(),
            }),
            "session_end" | "Stop" => Ok(AgentEvent::SessionEnd { session_id }),
            _ => Ok(AgentEvent::Processing {
                session_id,
                description: format!("Event: {}", event),
            }),
        }
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![self.hooks_path()]
    }
}
