// GeminiAdapter — Agent adapter for Google Gemini CLI

use super::{profiles, AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

pub struct GeminiAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl GeminiAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".gemini");
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
        super::executable::command_exists("gemini")
    }

    fn settings_path(&self) -> PathBuf {
        self.config_root.join("settings.json")
    }
}

impl AgentAdapter for GeminiAdapter {
    fn name(&self) -> &str {
        "gemini"
    }
    fn display_name(&self) -> &str {
        "Google Gemini"
    }
    fn icon(&self) -> &str {
        "gemini"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::install_at(&profiles::gemini_profile(), &self.settings_path())?;
        log::info!("Gemini hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall_at(&profiles::gemini_profile(), &self.settings_path())?;
        log::info!("Gemini hooks removed");
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
            "SessionStart" | "session_start" => Ok(AgentEvent::SessionStart {
                session_id,
                project: cwd.rsplit('/').next().unwrap_or("").to_string(),
                cwd,
                terminal: string_field(raw, &["tty"]).unwrap_or("").to_string(),
                agent_type: "gemini".to_string(),
            }),
            "SessionEnd" | "session_end" => Ok(AgentEvent::SessionEnd { session_id }),
            "Stop" => {
                let summary = string_field(raw, &["summary", "message", "last_assistant_message"])
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or("Task completed")
                    .to_string();
                Ok(AgentEvent::AssistantResponseComplete {
                    session_id,
                    text: summary,
                })
            }
            "BeforeTool" | "PreToolUse" | "pre_tool_use" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "running".to_string(),
            }),
            "AfterTool" | "PostToolUse" | "post_tool_use" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "success".to_string(),
            }),
            "BeforeAgent" => Ok(AgentEvent::Processing {
                session_id,
                description: string_field(raw, &["message", "description"])
                    .unwrap_or("Agent started")
                    .to_string(),
            }),
            "AfterAgent" => Ok(AgentEvent::AssistantResponseComplete {
                session_id,
                text: string_field(raw, &["summary", "last_assistant_message", "message"])
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("Agent completed")
                    .to_string(),
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

fn string_field<'a>(raw: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| raw.get(key).and_then(|value| value.as_str()))
}
