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
        let status = Self::detect_status();
        Self {
            config_root,
            status,
        }
    }

    fn detect_status() -> AdapterStatus {
        super::programs::detected_status_for_agent_program("kiro")
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

    fn detect_status_now(&self) -> AdapterStatus {
        Self::detect_status()
    }

    fn parse_event(
        &self,
        raw: &serde_json::Value,
    ) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        let session_id = string_field(raw, &["session_id", "sessionId"])
            .unwrap_or("unknown")
            .to_string();
        let event = string_field(raw, &["event", "hook_event_name", "hookEventName"]).unwrap_or("");
        let cwd = string_field(raw, &["cwd"]).unwrap_or("").to_string();
        let tool_name = string_field(raw, &["tool", "tool_name", "toolName"])
            .unwrap_or("Tool")
            .to_string();
        let tool_input = raw
            .get("tool_input")
            .or_else(|| raw.get("toolInput"))
            .map(|v| v.to_string())
            .unwrap_or_default();
        match event {
            "agentSpawn" | "session_start" | "SessionStart" => Ok(AgentEvent::SessionStart {
                session_id,
                project: super::project_name_from_path(&cwd),
                cwd,
                terminal: string_field(raw, &["tty"]).unwrap_or("").to_string(),
                agent_type: "kiro".to_string(),
            }),
            "userPromptSubmit" | "UserPromptSubmit" => Ok(AgentEvent::Processing {
                session_id,
                description: string_field(raw, &["prompt", "user_prompt", "message"])
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| format!("Prompt: {}", value.chars().take(80).collect::<String>()))
                    .unwrap_or_else(|| "User prompt submitted".to_string()),
            }),
            "preToolUse" | "PreToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "running".to_string(),
            }),
            "postToolUse" | "PostToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "success".to_string(),
            }),
            "session_end" | "SessionEnd" => Ok(AgentEvent::SessionEnd { session_id }),
            "stop" | "Stop" => Ok(AgentEvent::AssistantResponseComplete {
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
        vec![self.agent_file_path()]
    }
}

fn string_field<'a>(raw: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| raw.get(key).and_then(|value| value.as_str()))
}
