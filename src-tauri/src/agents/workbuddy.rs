// WorkBuddyAdapter — Agent adapter for WorkBuddy

use super::profiles;
use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

pub struct WorkBuddyAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl WorkBuddyAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".workbuddy");
        let status = Self::detect_status();
        Self {
            config_root,
            status,
        }
    }

    fn detect_status() -> AdapterStatus {
        super::programs::detected_status_for_agent_program("workbuddy")
    }

    fn hooks_path(&self) -> PathBuf {
        self.config_root.join("hooks.json")
    }
}

impl AgentAdapter for WorkBuddyAdapter {
    fn name(&self) -> &str {
        "workbuddy"
    }
    fn display_name(&self) -> &str {
        "WorkBuddy"
    }
    fn icon(&self) -> &str {
        "workbuddy"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::install_at(&profiles::workbuddy_profile(), &self.hooks_path())?;
        log::info!("WorkBuddy hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall_at(&profiles::workbuddy_profile(), &self.hooks_path())?;
        log::info!("WorkBuddy hooks removed");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }

    fn detect_status_now(&self) -> AdapterStatus {
        Self::detect_status()
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
                agent_type: "workbuddy".to_string(),
            }),
            "session_end" => Ok(AgentEvent::SessionEnd { session_id }),
            "Stop" => Ok(AgentEvent::AssistantResponseComplete {
                session_id,
                text: raw
                    .get("summary")
                    .or_else(|| raw.get("last_assistant_message"))
                    .or_else(|| raw.get("message"))
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or("Task completed")
                    .to_string(),
            }),
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
