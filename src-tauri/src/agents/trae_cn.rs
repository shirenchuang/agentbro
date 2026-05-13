// TraeCNAdapter — Agent adapter for Trae CN (Chinese variant, YAML config)

use super::hook_manager;
use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

pub struct TraeCNAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl TraeCNAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| std::env::temp_dir());
        let config_root = home.join(".trae-cn");
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
            .arg("traecn")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
            || std::path::Path::new("/Applications/Trae.app").exists()
    }

    fn config_path(&self) -> PathBuf {
        self.config_root.join("config.yaml")
    }
}

impl AgentAdapter for TraeCNAdapter {
    fn name(&self) -> &str {
        "traecn"
    }
    fn display_name(&self) -> &str {
        "Trae CN"
    }
    fn icon(&self) -> &str {
        "trae"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = self.config_path();
        let hook_command = hook_manager::bridge_binary_path().display().to_string();
        let events = &["pre_tool_use", "post_tool_use", "session_start"];
        hook_manager::inject_hooks_yaml(&config_path, &hook_command, events)?;
        log::info!("Trae CN hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = self.config_path();
        hook_manager::remove_hooks_yaml(&config_path)?;
        log::info!("Trae CN hooks removed");
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
                agent_type: "traecn".to_string(),
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
