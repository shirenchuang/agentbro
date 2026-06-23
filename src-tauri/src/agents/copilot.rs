// CopilotAdapter — Agent adapter for GitHub Copilot CLI

use super::{profiles, AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

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
        if let Some(gh_path) = super::executable::find_binary("gh") {
            return std::process::Command::new(gh_path)
                .args(["copilot", "--version"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
        }
        false
    }

    fn settings_path(&self) -> PathBuf {
        self.config_root.join("hooks.json")
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
        profiles::install_at(&profiles::copilot_profile(), &self.settings_path())?;
        log::info!("Copilot hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall_at(&profiles::copilot_profile(), &self.settings_path())?;
        log::info!("Copilot hooks removed");
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
            .or_else(|| raw.get("toolArgs"))
            .map(|v| v.to_string())
            .unwrap_or_default();
        match event {
            "session_start" | "sessionStart" | "SessionStart" => Ok(AgentEvent::SessionStart {
                session_id,
                project: super::project_name_from_path(&cwd),
                cwd,
                terminal: string_field(raw, &["tty"]).unwrap_or("").to_string(),
                agent_type: "copilot".to_string(),
            }),
            "session_end" | "sessionEnd" | "SessionEnd" => {
                Ok(AgentEvent::SessionEnd { session_id })
            }
            "user_prompt_submit" | "userPromptSubmitted" | "UserPromptSubmit" => {
                Ok(AgentEvent::Processing {
                    session_id,
                    description: string_field(raw, &["prompt", "user_prompt", "message"])
                        .filter(|value| !value.trim().is_empty())
                        .map(|value| {
                            format!("Prompt: {}", value.chars().take(80).collect::<String>())
                        })
                        .unwrap_or_else(|| "User prompt submitted".to_string()),
                })
            }
            "pre_tool_use" | "preToolUse" | "PreToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "running".to_string(),
            }),
            "post_tool_use" | "postToolUse" | "PostToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "success".to_string(),
            }),
            "postToolUseFailure" | "PostToolUseFailure" | "post_tool_use_failure" => {
                if tool_name != "Tool" {
                    Ok(AgentEvent::ToolUse {
                        session_id,
                        tool_name,
                        tool_input,
                        tool_target: None,
                        status: "error".to_string(),
                    })
                } else {
                    Ok(AgentEvent::Error {
                        session_id,
                        message: string_field(raw, &["error", "message"])
                            .unwrap_or("Copilot reported an error")
                            .to_string(),
                    })
                }
            }
            "permissionRequest" | "PermissionRequest" | "permission_request" => {
                Ok(AgentEvent::PermissionRequest {
                    session_id,
                    tool_name,
                    diff: raw
                        .get("diff")
                        .or_else(|| raw.get("preview"))
                        .and_then(|value| value.as_str())
                        .map(ToString::to_string),
                    options: None,
                })
            }
            "notification" | "Notification" => Ok(AgentEvent::Notification {
                session_id,
                message: string_field(raw, &["message", "notification", "title"])
                    .unwrap_or("Copilot notification")
                    .to_string(),
                status: string_field(raw, &["notification_type", "notificationType", "type"])
                    .map(ToString::to_string),
            }),
            "agentStop" | "Stop" => Ok(AgentEvent::AssistantResponseComplete {
                session_id,
                text: string_field(
                    raw,
                    &[
                        "last_assistant_message",
                        "lastAssistantMessage",
                        "summary",
                        "message",
                    ],
                )
                .unwrap_or("Copilot turn completed")
                .to_string(),
            }),
            "preCompact" | "PreCompact" => Ok(AgentEvent::Processing {
                session_id,
                description: "Compacting context".to_string(),
            }),
            "subagentStart" | "SubagentStart" => Ok(AgentEvent::SubagentStart {
                session_id,
                agent_id: string_field(raw, &["agent_id", "agentId", "agentName"])
                    .unwrap_or("unknown")
                    .to_string(),
                name: string_field(
                    raw,
                    &["agent_display_name", "agentDisplayName", "agentName"],
                )
                .map(ToString::to_string),
                description: string_field(
                    raw,
                    &["agent_description", "agentDescription", "description"],
                )
                .unwrap_or("")
                .to_string(),
                agent_type: Some("copilot".to_string()),
                transcript_path: string_field(raw, &["transcript_path", "transcriptPath"])
                    .map(ToString::to_string),
            }),
            "subagentStop" | "SubagentStop" => Ok(AgentEvent::SubagentStop {
                session_id,
                agent_id: string_field(raw, &["agent_id", "agentId", "agentName"])
                    .unwrap_or("unknown")
                    .to_string(),
                status: string_field(raw, &["agent_status", "agentStatus", "stopReason"])
                    .unwrap_or("completed")
                    .to_string(),
                name: string_field(
                    raw,
                    &["agent_display_name", "agentDisplayName", "agentName"],
                )
                .map(ToString::to_string),
                agent_type: Some("copilot".to_string()),
                transcript_path: string_field(raw, &["transcript_path", "transcriptPath"])
                    .map(ToString::to_string),
                agent_transcript_path: string_field(
                    raw,
                    &["agent_transcript_path", "agentTranscriptPath"],
                )
                .map(ToString::to_string),
                last_assistant_message: string_field(
                    raw,
                    &["last_assistant_message", "lastAssistantMessage", "message"],
                )
                .map(ToString::to_string),
            }),
            "errorOccurred" | "ErrorOccurred" => Ok(AgentEvent::Error {
                session_id,
                message: raw
                    .get("error")
                    .and_then(|value| {
                        value
                            .as_str()
                            .or_else(|| value.get("message").and_then(|message| message.as_str()))
                    })
                    .or_else(|| string_field(raw, &["message"]))
                    .unwrap_or("Copilot reported an error")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> CopilotAdapter {
        CopilotAdapter {
            config_root: PathBuf::new(),
            status: AdapterStatus::Unavailable,
        }
    }

    #[test]
    fn parses_permission_request_as_interactive_event() {
        let event = adapter()
            .parse_event(&serde_json::json!({
                "event": "permissionRequest",
                "sessionId": "s1",
                "cwd": "/tmp/demo",
                "toolName": "Bash",
                "toolArgs": { "command": "rm -rf dist" }
            }))
            .expect("parse event");

        assert!(matches!(
            event,
            AgentEvent::PermissionRequest { session_id, tool_name, .. }
                if session_id == "s1" && tool_name == "Bash"
        ));
    }

    #[test]
    fn parses_post_tool_use_failure_with_camel_args() {
        let event = adapter()
            .parse_event(&serde_json::json!({
                "event": "postToolUseFailure",
                "sessionId": "s2",
                "cwd": "/tmp/demo",
                "toolName": "Bash",
                "toolArgs": { "command": "false" },
                "error": "failed"
            }))
            .expect("parse event");

        assert!(matches!(
            event,
            AgentEvent::ToolUse { session_id, tool_name, tool_input, status, .. }
                if session_id == "s2"
                    && tool_name == "Bash"
                    && tool_input.contains("false")
                    && status == "error"
        ));
    }
}
