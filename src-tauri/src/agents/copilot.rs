// CopilotAdapter — Agent adapter for GitHub Copilot CLI

use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

const BRIDGE_BINARY_NAME: &str = "agentbro-bridge";
const AGENTBRO_MARKER: &str = "agentbro";

pub struct CopilotAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl CopilotAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".config").join("github-copilot");
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
        // Check for gh CLI with copilot extension
        if std::process::Command::new("which")
            .arg("gh")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return std::process::Command::new("gh")
                .args(["copilot", "--version"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
        }
        false
    }

    fn bridge_binary_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        home.join(".agentbro").join("bin").join(BRIDGE_BINARY_NAME)
    }

    fn settings_path(&self) -> PathBuf {
        self.config_root.join("hooks.json")
    }

    fn inject_hooks_json(settings: &mut serde_json::Value, hook_command: &str) {
        let hook_entry = serde_json::json!([{"command": hook_command}]);
        if settings.get("hooks").is_none() {
            settings["hooks"] = serde_json::json!({});
        }
        let hooks = settings["hooks"].as_object_mut().unwrap();
        for event in &[
            "pre_tool_use",
            "post_tool_use",
            "session_start",
            "session_end",
        ] {
            let existing = hooks
                .entry(event.to_string())
                .or_insert_with(|| serde_json::json!([]));
            if let Some(arr) = existing.as_array_mut() {
                arr.retain(|e| {
                    !e.get("command")
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains(AGENTBRO_MARKER))
                        .unwrap_or(false)
                });
                arr.push(serde_json::json!({"command": hook_command}));
            }
        }
        let _ = hook_entry;
    }

    fn remove_hooks_json(settings: &mut serde_json::Value) {
        if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
            for (_, v) in hooks.iter_mut() {
                if let Some(arr) = v.as_array_mut() {
                    arr.retain(|e| {
                        !e.get("command")
                            .and_then(|c| c.as_str())
                            .map(|c| c.contains(AGENTBRO_MARKER))
                            .unwrap_or(false)
                    });
                }
            }
        }
    }
}

impl AgentAdapter for CopilotAdapter {
    fn name(&self) -> &str {
        "copilot"
    }
    fn display_name(&self) -> &str {
        "GitHub Copilot"
    }
    fn icon(&self) -> &str {
        "copilot"
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
        log::info!("Copilot hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let settings_path = self.settings_path();
        if !settings_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&settings_path)?;
        let mut settings: serde_json::Value = serde_json::from_str(&content)?;
        Self::remove_hooks_json(&mut settings);
        std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        log::info!("Copilot hooks removed");
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
                agent_type: "copilot".to_string(),
            }),
            "session_end" => Ok(AgentEvent::SessionEnd { session_id }),
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
