// TraeCliAdapter — Agent adapter for Trae CLI (separate from Trae app)

use super::hook_manager;
use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::{Path, PathBuf};

pub struct TraeCliAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl TraeCliAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".trae");
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
        which("traecli").is_some()
            || dirs::home_dir()
                .map(|home| home.join(".trae").join("traecli.yaml").exists())
                .unwrap_or(false)
            || Path::new("/Applications/Trae.app").exists()
    }

    fn config_path(&self) -> PathBuf {
        self.config_root.join("traecli.yaml")
    }
}

impl AgentAdapter for TraeCliAdapter {
    fn name(&self) -> &str {
        "traecli"
    }

    fn display_name(&self) -> &str {
        "TraeCli"
    }

    fn icon(&self) -> &str {
        "trae"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let hook_command = format!(
            "{} --source traecli",
            hook_manager::bridge_binary_path().display()
        );
        let events = &[
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "Stop",
            "SessionStart",
            "SessionEnd",
        ];
        hook_manager::inject_hooks_yaml(&self.config_path(), &hook_command, events)?;
        log::info!("TraeCli hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        hook_manager::remove_hooks_yaml(&self.config_path())?;
        log::info!("TraeCli hooks removed");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }

    fn parse_event(
        &self,
        raw: &serde_json::Value,
    ) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        let agent = raw.get("agent").and_then(|v| v.as_str()).unwrap_or("");
        if !agent.is_empty() && agent != self.name() {
            return Err("not a traecli event".into());
        }

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
                agent_type: "traecli".to_string(),
            }),
            "session_end" | "SessionEnd" | "Stop" => Ok(AgentEvent::SessionEnd { session_id }),
            "PreToolUse" | "pre_tool_use" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: raw
                    .get("tool")
                    .or_else(|| raw.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool")
                    .to_string(),
                tool_input: raw
                    .get("tool_input")
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                tool_target: None,
                status: "running".to_string(),
            }),
            "PostToolUse" | "post_tool_use" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: raw
                    .get("tool")
                    .or_else(|| raw.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool")
                    .to_string(),
                tool_input: raw
                    .get("tool_input")
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                tool_target: None,
                status: "success".to_string(),
            }),
            "PermissionRequest" | "permission_request" => Ok(AgentEvent::PermissionRequest {
                session_id,
                tool_name: raw
                    .get("tool")
                    .or_else(|| raw.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool")
                    .to_string(),
                diff: raw
                    .get("diff")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string),
                options: None,
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
}

fn which(binary: &str) -> Option<String> {
    std::process::Command::new("which")
        .arg(binary)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
}
