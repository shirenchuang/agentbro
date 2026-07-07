// WorkBuddyAdapter — Agent adapter for WorkBuddy

use super::profiles;
use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

pub struct WorkBuddyAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl WorkBuddyAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".workbuddy");
        let status = Self::detect_status();
        Self {
            config_root,
            status,
        }
    }

    fn detect_status() -> AdapterStatus {
        super::programs::detected_status_for_agent_program("workbuddy")
    }

    fn settings_path(&self) -> PathBuf {
        self.config_root.join("settings.json")
    }
}

impl AgentAdapter for WorkBuddyAdapter {
    fn name(&self) -> &str {
        "workbuddy"
    }
    fn display_name(&self) -> &str {
        "WorkBuddy"
    }
    fn icon(&self) -> &str {
        "workbuddy"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::install_at(&profiles::workbuddy_profile(), &self.settings_path())?;
        log::info!("WorkBuddy hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall_at(&profiles::workbuddy_profile(), &self.settings_path())?;
        log::info!("WorkBuddy hooks removed");
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
            &[
                "event",
                "hook_event_name",
                "hookEventName",
                "eventType",
                "type",
            ],
        )
        .unwrap_or("");
        let cwd = string_field(raw, &["cwd"]).unwrap_or("").to_string();
        let base_tool_name = string_field(raw, &["tool_name", "toolName", "tool"])
            .unwrap_or("Tool")
            .to_string();
        let tool_name = permission_tool_name(raw, event, &base_tool_name);
        let tool_input = tool_input_json(raw).unwrap_or_default();

        if is_permission_request(raw, event) {
            return Ok(AgentEvent::PermissionRequest {
                session_id,
                tool_name,
                diff: None,
                options: None,
            });
        }

        match event {
            "session_start" | "SessionStart" => Ok(AgentEvent::SessionStart {
                session_id,
                project: super::project_name_from_path(&cwd),
                cwd,
                terminal: string_field(raw, &["tty"]).unwrap_or("").to_string(),
                agent_type: "workbuddy".to_string(),
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
            "PermissionDenied" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "error".to_string(),
            }),
            "notification" | "Notification" => Ok(AgentEvent::Notification {
                session_id,
                message: string_field(raw, &["message", "notification"])
                    .unwrap_or("")
                    .to_string(),
                status: string_field(raw, &["status", "notification_type", "notificationType"])
                    .map(ToString::to_string),
            }),
            "stop" | "Stop" => Ok(AgentEvent::AssistantResponseComplete {
                session_id,
                text: string_field(raw, &["summary", "last_assistant_message", "message"])
                    .filter(|v| !v.trim().is_empty())
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
                status: string_field(raw, &["agent_status", "agentStatus", "status"])
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
            _ if is_permission_resolution(event) => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: permission_resolution_status(raw, event).to_string(),
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

fn is_permission_request(raw: &serde_json::Value, event: &str) -> bool {
    let normalized = event.to_ascii_lowercase();
    if matches!(event, "permission_request" | "PermissionRequest")
        || normalized.ends_with(".needs-approval")
        || normalized.contains("needsapproval")
    {
        return true;
    }

    matches!(
        string_field(raw, &["status", "notification_type", "notificationType"]),
        Some("permission_prompt" | "waiting_for_approval")
    )
}

fn is_permission_resolution(event: &str) -> bool {
    let normalized = event.to_ascii_lowercase();
    normalized.ends_with(".approved")
        || normalized.ends_with(".denied")
        || normalized.ends_with(".rejected")
}

fn permission_resolution_status<'a>(raw: &'a serde_json::Value, event: &'a str) -> &'a str {
    let decision = string_field(raw, &["decision"]).unwrap_or_default();
    let normalized = event.to_ascii_lowercase();
    if matches!(decision, "denied" | "rejected") || normalized.ends_with(".denied") {
        "error"
    } else {
        "success"
    }
}

fn permission_tool_name(raw: &serde_json::Value, event: &str, fallback: &str) -> String {
    let category = string_field(raw, &["category"]).unwrap_or_default();
    let normalized_event = event.to_ascii_lowercase();
    match category {
        "command-safety" => "Bash".to_string(),
        "file-safety" => "File Safety".to_string(),
        _ if normalized_event.starts_with("command-safety.") => "Bash".to_string(),
        _ if normalized_event.starts_with("file-safety.") => "File Safety".to_string(),
        _ => fallback.to_string(),
    }
}

fn tool_input_json(raw: &serde_json::Value) -> Option<String> {
    if let Some(input) = raw.get("tool_input").or_else(|| raw.get("toolInput")) {
        return Some(input.to_string());
    }

    let mut input = serde_json::Map::new();
    if let Some(command) = string_field(raw, &["commandPreview"]).or_else(|| {
        raw.pointer("/messageParams/command")
            .and_then(|value| value.as_str())
    }) {
        input.insert(
            "command".to_string(),
            serde_json::Value::String(command.to_string()),
        );
    }
    if let Some(paths) = raw.pointer("/messageParams/paths").cloned() {
        input.insert("paths".to_string(), paths);
    }
    if let Some(message) = string_field(raw, &["message", "messageKey"]) {
        input.insert(
            "message".to_string(),
            serde_json::Value::String(message.to_string()),
        );
    }
    if let Some(category) = string_field(raw, &["category"]) {
        input.insert(
            "category".to_string(),
            serde_json::Value::String(category.to_string()),
        );
    }
    if let Some(event_type) = string_field(raw, &["eventType"]) {
        input.insert(
            "eventType".to_string(),
            serde_json::Value::String(event_type.to_string()),
        );
    }

    (!input.is_empty()).then(|| serde_json::Value::Object(input).to_string())
}

fn user_prompt_description(raw: &serde_json::Value) -> String {
    string_field(raw, &["prompt", "user_prompt", "userPrompt", "message"])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let preview: String = value.chars().take(80).collect();
            if value.chars().count() > 80 {
                format!("Prompt: {preview}...")
            } else {
                format!("Prompt: {preview}")
            }
        })
        .unwrap_or_else(|| "User prompt submitted".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn adapter() -> WorkBuddyAdapter {
        WorkBuddyAdapter {
            config_root: PathBuf::from("/tmp/workbuddy-test"),
            status: AdapterStatus::Available,
        }
    }

    #[test]
    fn parses_workbuddy_hook_event_name_session_start() {
        let event = adapter()
            .parse_event(&json!({
                "session_id": "session-1",
                "hook_event_name": "SessionStart",
                "cwd": "/Users/me/project",
                "tty": "/dev/ttys002"
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
                assert_eq!(session_id, "session-1");
                assert_eq!(project, "project");
                assert_eq!(cwd, "/Users/me/project");
                assert_eq!(terminal, "/dev/ttys002");
                assert_eq!(agent_type, "workbuddy");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parses_workbuddy_tool_events() {
        let event = adapter()
            .parse_event(&json!({
                "session_id": "session-1",
                "hook_event_name": "PostToolUse",
                "tool_name": "Write",
                "tool_input": { "file_path": "/tmp/a.txt" }
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
                assert_eq!(session_id, "session-1");
                assert_eq!(tool_name, "Write");
                assert!(tool_input.contains("/tmp/a.txt"));
                assert_eq!(status, "success");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parses_workbuddy_file_safety_needs_approval() {
        let event = adapter()
            .parse_event(&json!({
                "agent": "workbuddy",
                "sessionId": "session-1",
                "toolCallId": "toolu_1",
                "category": "file-safety",
                "eventType": "file-safety.needs-approval",
                "decision": "info",
                "messageKey": "securityCenter.audit.fileSafety.needsApproval.write",
                "messageParams": {
                    "paths": "/Users/me/Desktop/out"
                }
            }))
            .unwrap();

        match event {
            AgentEvent::PermissionRequest {
                session_id,
                tool_name,
                diff,
                options,
            } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(tool_name, "File Safety");
                assert!(diff.is_none());
                assert!(options.is_none());
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parses_workbuddy_command_safety_needs_approval() {
        let event = adapter()
            .parse_event(&json!({
                "sessionId": "session-1",
                "toolCallId": "toolu_1",
                "category": "command-safety",
                "eventType": "command-safety.needs-approval",
                "commandPreview": "rm -rf /tmp/demo",
                "messageParams": {
                    "command": "rm -rf /tmp/demo"
                }
            }))
            .unwrap();

        match event {
            AgentEvent::PermissionRequest {
                session_id,
                tool_name,
                ..
            } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(tool_name, "Bash");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parses_workbuddy_notification_permission_prompt() {
        let event = adapter()
            .parse_event(&json!({
                "session_id": "session-1",
                "hook_event_name": "Notification",
                "notification_type": "permission_prompt",
                "message": "检测到工作空间外部文件修改",
                "tool_name": "Bash",
                "tool_input": {
                    "command": "mkdir -p ~/Desktop/out"
                }
            }))
            .unwrap();

        match event {
            AgentEvent::PermissionRequest {
                session_id,
                tool_name,
                ..
            } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(tool_name, "Bash");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parses_workbuddy_permission_resolution_as_tool_use() {
        let event = adapter()
            .parse_event(&json!({
                "sessionId": "session-1",
                "toolCallId": "toolu_1",
                "category": "file-safety",
                "eventType": "file-safety.approved",
                "decision": "approved",
                "messageParams": {
                    "paths": "/Users/me/Desktop/out"
                }
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
                assert_eq!(session_id, "session-1");
                assert_eq!(tool_name, "File Safety");
                assert!(tool_input.contains("/Users/me/Desktop/out"));
                assert_eq!(status, "success");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
