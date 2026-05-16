// CursorAdapter — Agent adapter for Cursor IDE

use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

const BRIDGE_BINARY_NAME: &str = "agentbro-bridge";

pub struct CursorAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl CursorAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".cursor");
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
        // Check for cursor CLI or app
        if std::process::Command::new("which")
            .arg("cursor")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return true;
        }
        // Check for macOS app bundle
        std::path::Path::new("/Applications/Cursor.app").exists()
    }

    fn bridge_binary_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        home.join(".agentbro").join("bin").join(BRIDGE_BINARY_NAME)
    }

    fn settings_path(&self) -> PathBuf {
        self.config_root.join("settings.json")
    }

    fn inject_hooks_json(settings: &mut serde_json::Value, hook_command: &str) {
        if settings.get("agentIslandHooks").is_none() {
            settings["agentIslandHooks"] = serde_json::json!({
                "command": hook_command,
                "enabled": true,
            });
        } else {
            settings["agentIslandHooks"]["command"] = serde_json::json!(hook_command);
            settings["agentIslandHooks"]["enabled"] = serde_json::json!(true);
        }
    }

    fn remove_hooks_json(settings: &mut serde_json::Value) {
        if let Some(obj) = settings.as_object_mut() {
            obj.remove("agentIslandHooks");
        }
    }
}

impl AgentAdapter for CursorAdapter {
    fn name(&self) -> &str {
        "cursor"
    }
    fn display_name(&self) -> &str {
        "Cursor"
    }
    fn icon(&self) -> &str {
        "cursor"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let settings_path = self.settings_path();
        let hook_command = Self::bridge_binary_path().display().to_string();

        let mut settings: serde_json::Value = if settings_path.exists() {
            let content = std::fs::read_to_string(&settings_path)?;
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        Self::inject_hooks_json(&mut settings, &hook_command);

        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        log::info!("Cursor hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let settings_path = self.settings_path();
        if !settings_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&settings_path)?;
        let mut settings: serde_json::Value = serde_json::from_str(&content)?;
        let had_hooks = settings.get("agentIslandHooks").is_some();
        Self::remove_hooks_json(&mut settings);
        if had_hooks {
            std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        }
        log::info!("Cursor hooks removed");
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
                terminal: raw
                    .get("tty")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                agent_type: "cursor".to_string(),
            }),
            "session_end" | "Stop" => Ok(AgentEvent::SessionEnd { session_id }),
            _ => Ok(AgentEvent::Processing {
                session_id,
                description: format!("Event: {}", event),
            }),
        }
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![self.settings_path()]
    }
}

impl CursorAdapter {
    pub fn is_cursor_installed() -> bool {
        Self::is_installed()
    }

    pub fn has_agentbro_hooks(&self) -> bool {
        let settings_path = self.settings_path();
        if !settings_path.exists() {
            return false;
        }
        std::fs::read_to_string(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .map(|v| {
                v.get("agentIslandHooks")
                    .and_then(|h| h.get("enabled"))
                    .and_then(|e| e.as_bool())
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }
}
