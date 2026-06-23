// QoderAdapter — Agent adapter for Qoder AI coding assistant

use super::{profiles, AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

pub struct QoderAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl QoderAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".qoder");
        let status = Self::detect_status();
        Self {
            config_root,
            status,
        }
    }

    fn detect_status() -> AdapterStatus {
        super::programs::detected_status_for_agent_program("qoder")
    }

    fn settings_path(&self) -> PathBuf {
        self.config_root.join("settings.json")
    }
}

impl AgentAdapter for QoderAdapter {
    fn name(&self) -> &str {
        "qoder"
    }
    fn display_name(&self) -> &str {
        "Qoder"
    }
    fn icon(&self) -> &str {
        "qoder"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::install_at(&profiles::qoder_profile(), &self.settings_path())?;
        log::info!("Qoder hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall_at(&profiles::qoder_profile(), &self.settings_path())?;
        log::info!("Qoder hooks removed");
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
        let event = string_field(
            raw,
            &["event", "hook_event_name", "hookEventName", "hookType"],
        )
        .unwrap_or("");
        let cwd = string_field(raw, &["cwd"]).unwrap_or("").to_string();
        let project = super::project_name_from_path(&cwd);
        let terminal = string_field(raw, &["tty"]).unwrap_or("").to_string();
        let tool_name = string_field(raw, &["tool", "tool_name", "toolName"])
            .unwrap_or("Tool")
            .to_string();
        let tool_input = raw
            .get("tool_input")
            .or_else(|| raw.get("toolInput"))
            .map(|v| v.to_string())
            .unwrap_or_default();

        match event {
            "session_start" | "SessionStart" => Ok(AgentEvent::SessionStart {
                session_id,
                project,
                cwd,
                terminal,
                agent_type: "qoder".to_string(),
            }),
            "session_end" | "SessionEnd" => Ok(AgentEvent::SessionEnd { session_id }),
            "user_prompt_submit" | "UserPromptSubmit" => Ok(AgentEvent::Processing {
                session_id,
                description: user_prompt_description(raw),
            }),
            "pre_tool_use" | "PreToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "running".to_string(),
            }),
            "post_tool_use" | "PostToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "success".to_string(),
            }),
            "post_tool_use_failure" | "PostToolUseFailure" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "error".to_string(),
            }),
            "permission_request" | "PermissionRequest" => Ok(AgentEvent::PermissionRequest {
                session_id,
                tool_name,
                diff: None,
                options: None,
            }),
            "notification" | "Notification" => Ok(AgentEvent::Notification {
                session_id,
                message: string_field(raw, &["message", "notification"])
                    .unwrap_or("")
                    .to_string(),
                status: string_field(raw, &["status"]).map(ToString::to_string),
            }),
            "stop" | "Stop" => Ok(AgentEvent::AssistantResponseComplete {
                session_id,
                text: string_field(raw, &["summary", "last_assistant_message", "message"])
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("Task completed")
                    .to_string(),
            }),
            "PreCompact" | "PostCompact" | "pre_compact" | "post_compact" => {
                Ok(AgentEvent::Processing {
                    session_id,
                    description: string_field(raw, &["message", "description"])
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or("Compacting context")
                        .to_string(),
                })
            }
            "SubagentStart" | "subagent_start" => Ok(AgentEvent::SubagentStart {
                session_id,
                agent_id: string_field(raw, &["agent_id", "agentId", "tool_use_id", "toolUseId"])
                    .unwrap_or("unknown")
                    .to_string(),
                name: string_field(raw, &["name", "agent_name", "agentName"])
                    .map(ToString::to_string),
                description: string_field(raw, &["description", "message"])
                    .unwrap_or("")
                    .to_string(),
                agent_type: string_field(raw, &["agent_type", "agentType", "type"])
                    .map(ToString::to_string),
                transcript_path: string_field(raw, &["transcript_path", "transcriptPath"])
                    .map(ToString::to_string),
            }),
            "SubagentStop" | "subagent_stop" => Ok(AgentEvent::SubagentStop {
                session_id,
                agent_id: string_field(raw, &["agent_id", "agentId", "tool_use_id", "toolUseId"])
                    .unwrap_or("unknown")
                    .to_string(),
                status: string_field(raw, &["agent_status", "agentStatus"])
                    .unwrap_or("completed")
                    .to_string(),
                name: string_field(raw, &["name", "agent_name", "agentName"])
                    .map(ToString::to_string),
                agent_type: string_field(raw, &["agent_type", "agentType", "type"])
                    .map(ToString::to_string),
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

fn user_prompt_description(raw: &serde_json::Value) -> String {
    string_field(raw, &["description"])
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            string_field(raw, &["prompt", "user_prompt", "userPrompt"])
                .filter(|value| !value.trim().is_empty())
                .map(|prompt| {
                    let first_line = prompt.lines().next().unwrap_or(prompt).trim();
                    let preview: String = first_line.chars().take(80).collect();
                    let suffix = if first_line.chars().count() > 80 {
                        "..."
                    } else {
                        ""
                    };
                    format!("Processing user input: {preview}{suffix}")
                })
        })
        .unwrap_or_else(|| "Processing user input".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn adapter() -> QoderAdapter {
        QoderAdapter {
            config_root: PathBuf::from("/tmp/qoder-test"),
            status: AdapterStatus::Available,
        }
    }

    #[test]
    fn parses_qoder_app_session_start() {
        let event = adapter()
            .parse_event(&json!({
                "sessionId": "015da017-e2f7-4894-ab30-e9b3adc7668c",
                "event": "SessionStart",
                "cwd": "/Users/me/project",
                "tty": "/dev/ttys001"
            }))
            .unwrap();

        match event {
            AgentEvent::SessionStart {
                session_id,
                project,
                cwd,
                terminal,
                agent_type,
            } => {
                assert_eq!(session_id, "015da017-e2f7-4894-ab30-e9b3adc7668c");
                assert_eq!(project, "project");
                assert_eq!(cwd, "/Users/me/project");
                assert_eq!(terminal, "/dev/ttys001");
                assert_eq!(agent_type, "qoder");
            }
            other => panic!("expected SessionStart, got {other:?}"),
        }
    }

    #[test]
    fn parses_qoder_stop_as_response_complete() {
        let event = adapter()
            .parse_event(&json!({
                "session_id": "qoder-session",
                "event": "Stop",
                "last_assistant_message": "done"
            }))
            .unwrap();

        match event {
            AgentEvent::AssistantResponseComplete { session_id, text } => {
                assert_eq!(session_id, "qoder-session");
                assert_eq!(text, "done");
            }
            other => panic!("expected AssistantResponseComplete, got {other:?}"),
        }
    }

    #[test]
    fn parses_qoder_tool_failure_status() {
        let event = adapter()
            .parse_event(&json!({
                "session_id": "qoder-session",
                "event": "PostToolUseFailure",
                "tool_name": "Edit",
                "tool_input": {"file_path": "/tmp/a.rs"}
            }))
            .unwrap();

        match event {
            AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                status,
                ..
            } => {
                assert_eq!(session_id, "qoder-session");
                assert_eq!(tool_name, "Edit");
                assert_eq!(tool_input, "{\"file_path\":\"/tmp/a.rs\"}");
                assert_eq!(status, "error");
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }
}
