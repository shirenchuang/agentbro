// QwenAdapter — Agent adapter for Alibaba Qwen AI coding assistant

use std::path::PathBuf;
use super::{AdapterStatus, AgentAdapter, AgentEvent};

const BRIDGE_BINARY_NAME: &str = "agent-island-bridge";
const AGENT_ISLAND_MARKER: &str = "agent-island";

pub struct QwenAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl QwenAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| std::env::temp_dir());
        let config_root = home.join(".qwen");
        let status = if Self::is_installed() {
            AdapterStatus::Available
        } else {
            AdapterStatus::Unavailable
        };
        Self { config_root, status }
    }

    fn is_installed() -> bool {
        // Check for qwen-coder or qwen CLI
        for cmd in &["qwen-coder", "qwen"] {
            if std::process::Command::new("which")
                .arg(cmd)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }

    fn bridge_binary_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| std::env::temp_dir());
        home.join(".agent-island").join("bin").join(BRIDGE_BINARY_NAME)
    }

    fn settings_path(&self) -> PathBuf {
        self.config_root.join("settings.json")
    }

    fn inject_hooks(settings: &mut serde_json::Value, hook_command: &str) {
        if settings.get("hooks").is_none() {
            settings["hooks"] = serde_json::json!({});
        }
        let hooks = settings["hooks"].as_object_mut().unwrap();
        for event in &["pre_tool_use", "post_tool_use", "session_start", "session_end"] {
            let entry = hooks.entry(event.to_string()).or_insert_with(|| serde_json::json!([]));
            if let Some(arr) = entry.as_array_mut() {
                arr.retain(|e| !e.get("command").and_then(|c| c.as_str())
                    .map(|c| c.contains(AGENT_ISLAND_MARKER)).unwrap_or(false));
                arr.push(serde_json::json!({"command": hook_command}));
            }
        }
    }

    fn remove_hooks(settings: &mut serde_json::Value) {
        if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
            for (_, v) in hooks.iter_mut() {
                if let Some(arr) = v.as_array_mut() {
                    arr.retain(|e| !e.get("command").and_then(|c| c.as_str())
                        .map(|c| c.contains(AGENT_ISLAND_MARKER)).unwrap_or(false));
                }
            }
        }
    }
}

impl AgentAdapter for QwenAdapter {
    fn name(&self) -> &str { "qwen" }
    fn display_name(&self) -> &str { "Qwen Coder" }
    fn icon(&self) -> &str { "qwen" }

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
        log::info!("Qwen hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let settings_path = self.settings_path();
        if !settings_path.exists() { return Ok(()); }

        let content = std::fs::read_to_string(&settings_path)?;
        let mut settings: serde_json::Value = serde_json::from_str(&content)?;
        Self::remove_hooks(&mut settings);
        std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        log::info!("Qwen hooks removed");
        Ok(())
    }

    fn status(&self) -> AdapterStatus { self.status.clone() }

    fn parse_event(&self, raw: &serde_json::Value) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        let session_id = raw.get("session_id")
            .and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let event = raw.get("event").and_then(|v| v.as_str()).unwrap_or("");
        match event {
            "session_start" => Ok(AgentEvent::SessionStart {
                session_id,
                project: raw.get("cwd").and_then(|v| v.as_str())
                    .and_then(|p| p.rsplit('/').next()).unwrap_or("").to_string(),
                cwd: raw.get("cwd").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                terminal: "".to_string(),
                agent_type: "qwen".to_string(),
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
