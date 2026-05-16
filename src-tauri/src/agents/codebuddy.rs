// CodeBuddyAdapter — Agent adapter for Tencent CodeBuddy

use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

const BRIDGE_BINARY_NAME: &str = "agentbro-bridge";
const AGENTBRO_MARKER: &str = "agentbro";

pub struct CodeBuddyAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl CodeBuddyAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".codebuddy");
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
            .arg("codebuddy")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn bridge_binary_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        home.join(".agentbro").join("bin").join(BRIDGE_BINARY_NAME)
    }

    fn settings_path(&self) -> PathBuf {
        self.config_root.join("settings.json")
    }

    fn inject_hooks(settings: &mut serde_json::Value, hook_command: &str) {
        if settings.get("hooks").is_none() {
            settings["hooks"] = serde_json::json!({});
        }
        let hooks = settings["hooks"].as_object_mut().unwrap();
        for event in &["PreToolUse", "PostToolUse", "SessionStart", "SessionEnd"] {
            let entry = hooks
                .entry(event.to_string())
                .or_insert_with(|| serde_json::json!([]));
            if let Some(arr) = entry.as_array_mut() {
                arr.retain(|e| {
                    !e.get("command")
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains(AGENTBRO_MARKER))
                        .unwrap_or(false)
                });
                arr.push(serde_json::json!({"type": "command", "command": hook_command}));
            }
        }
    }

    fn remove_hooks(settings: &mut serde_json::Value) {
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

impl AgentAdapter for CodeBuddyAdapter {
    fn name(&self) -> &str {
        "codebuddy"
    }
    fn display_name(&self) -> &str {
        "CodeBuddy"
    }
    fn icon(&self) -> &str {
        "codebuddy"
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

        Self::inject_hooks(&mut settings, &hook_command);

        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        log::info!("CodeBuddy hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let settings_path = self.settings_path();
        if !settings_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&settings_path)?;
        let mut settings: serde_json::Value = serde_json::from_str(&content)?;
        Self::remove_hooks(&mut settings);
        std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        log::info!("CodeBuddy hooks removed");
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
            "SessionStart" => Ok(AgentEvent::SessionStart {
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
                agent_type: "codebuddy".to_string(),
            }),
            "SessionEnd" => Ok(AgentEvent::SessionEnd { session_id }),
            "PreToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: raw
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                tool_input: raw
                    .get("tool_input")
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                tool_target: None,
                status: "running".to_string(),
            }),
            "PostToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: raw
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                tool_input: raw
                    .get("tool_input")
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                tool_target: None,
                status: "success".to_string(),
            }),
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
