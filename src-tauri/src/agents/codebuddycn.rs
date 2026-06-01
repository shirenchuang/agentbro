// CodeBuddyCNAdapter — Agent adapter for CodeBuddy CN (Chinese variant)

use super::profiles;
use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

pub struct CodeBuddyCNAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl CodeBuddyCNAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = preferred_config_root(&home);
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
        std::path::Path::new("/Applications/CodyBuddyCN.app").exists()
            || std::path::Path::new("/Applications/CodeBuddy CN.app").exists()
    }

    fn settings_path(&self) -> PathBuf {
        self.config_root.join("settings.json")
    }
}

impl AgentAdapter for CodeBuddyCNAdapter {
    fn name(&self) -> &str {
        "codebuddycn"
    }
    fn display_name(&self) -> &str {
        "CodyBuddyCN"
    }
    fn icon(&self) -> &str {
        "codebuddy"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::install_at(&profiles::codebuddycn_profile(), &self.settings_path())?;
        log::info!("CodeBuddy CN hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall_at(&profiles::codebuddycn_profile(), &self.settings_path())?;
        log::info!("CodeBuddy CN hooks removed");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }

    fn detect_status_now(&self) -> AdapterStatus {
        if Self::is_installed() {
            AdapterStatus::Available
        } else {
            AdapterStatus::Unavailable
        }
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
        let agent = raw.get("agent").and_then(|v| v.as_str()).unwrap_or("");
        if !agent.is_empty() && agent != "codebuddycn" && agent != "codybuddycn" {
            return Err("not a CodeBuddy CN event".into());
        }
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
                agent_type: "codebuddycn".to_string(),
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
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        vec![
            self.settings_path(),
            home.join(".codybuddycn").join("settings.json"),
            home.join(".codebuddycn").join("settings.json"),
            home.join(".codebuddy-cn").join("settings.json"),
            home.join(".codebuddy").join("settings.json"),
        ]
    }
}

fn preferred_config_root(home: &std::path::Path) -> PathBuf {
    for name in [
        ".codybuddycn",
        ".codebuddycn",
        ".codebuddy-cn",
        ".codebuddy",
    ] {
        let path = home.join(name);
        if path.exists() {
            return path;
        }
    }
    home.join(".codybuddycn")
}
