// DeepSeekAdapter — Agent adapter for DeepSeek CLI (TOML config format)

use super::profiles;
use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

pub struct DeepSeekAdapter {
    status: AdapterStatus,
}

impl DeepSeekAdapter {
    pub fn new() -> Self {
        let status = Self::detect_status();
        Self { status }
    }

    fn detect_status() -> AdapterStatus {
        super::programs::detected_status_for_agent_program("deepseek")
    }

    fn config_path(&self) -> PathBuf {
        profiles::configuration_url(&profiles::deepseek_profile())
    }
}

impl AgentAdapter for DeepSeekAdapter {
    fn name(&self) -> &str {
        "deepseek"
    }
    fn display_name(&self) -> &str {
        "DeepSeek"
    }
    fn icon(&self) -> &str {
        "deepseek"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::install(&profiles::deepseek_profile())?;
        log::info!("DeepSeek hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall(&profiles::deepseek_profile())?;
        log::info!("DeepSeek hooks removed");
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
                agent_type: "deepseek".to_string(),
            }),
            "session_end" | "SessionEnd" => Ok(AgentEvent::SessionEnd { session_id }),
            "stop" | "Stop" => Ok(AgentEvent::TaskComplete {
                session_id,
                summary: raw
                    .get("last_assistant_message")
                    .or_else(|| raw.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("DeepSeek turn completed")
                    .to_string(),
            }),
            "pre_tool_use" | "PreToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: raw
                    .get("tool")
                    .or_else(|| raw.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Tool")
                    .to_string(),
                tool_input: raw
                    .get("tool_input")
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                tool_target: None,
                status: "running".to_string(),
            }),
            "post_tool_use" | "PostToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: raw
                    .get("tool")
                    .or_else(|| raw.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Tool")
                    .to_string(),
                tool_input: raw
                    .get("tool_input")
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                tool_target: None,
                status: "success".to_string(),
            }),
            "permission_request" | "PermissionRequest" => Ok(AgentEvent::PermissionRequest {
                session_id,
                tool_name: raw
                    .get("tool")
                    .or_else(|| raw.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Permission")
                    .to_string(),
                diff: None,
                options: Some(vec!["Allow".to_string(), "Deny".to_string()]),
            }),
            _ => Ok(AgentEvent::Processing {
                session_id,
                description: format!("Event: {}", event),
            }),
        }
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![self.config_path()]
    }

    fn hooks_installed(&self) -> bool {
        profiles::is_installed(&profiles::deepseek_profile())
    }
}
