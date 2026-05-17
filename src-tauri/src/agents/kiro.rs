// KiroAdapter — Agent adapter for Kiro CLI (JSON agent file format)

use super::{profiles, AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

pub struct KiroAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl KiroAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".kiro");
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
            .arg("kiro")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn agent_file_path(&self) -> PathBuf {
        self.config_root.join("agents").join("agentbro.json")
    }
}

impl AgentAdapter for KiroAdapter {
    fn name(&self) -> &str {
        "kiro"
    }
    fn display_name(&self) -> &str {
        "Kiro"
    }
    fn icon(&self) -> &str {
        "kiro"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.agent_file_path();
        profiles::install_at(&profiles::kiro_profile(), &path)?;
        log::info!("Kiro hooks installed at {:?}", path);
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall_at(&profiles::kiro_profile(), &self.agent_file_path())?;
        log::info!("Kiro hooks removed");
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
                agent_type: "kiro".to_string(),
            }),
            "session_end" | "Stop" => Ok(AgentEvent::SessionEnd { session_id }),
            _ => Ok(AgentEvent::Processing {
                session_id,
                description: format!("Event: {}", event),
            }),
        }
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![self.agent_file_path()]
    }
}
