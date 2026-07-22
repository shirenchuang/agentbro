use super::profiles;
use super::{AdapterStatus, AgentAdapter, AgentEvent, QuestionItem, QuestionOption};
use serde_json::Value;
use std::path::PathBuf;

pub struct ZcodeAdapter {
    status: AdapterStatus,
}

impl ZcodeAdapter {
    pub fn new() -> Self {
        Self {
            status: Self::detect_status(),
        }
    }

    fn detect_status() -> AdapterStatus {
        super::programs::detected_status_for_agent_program("zcode")
    }

    fn config_path(&self) -> PathBuf {
        profiles::configuration_url(&profiles::zcode_profile())
    }
}

impl AgentAdapter for ZcodeAdapter {
    fn name(&self) -> &str {
        "zcode"
    }

    fn display_name(&self) -> &str {
        "ZCode"
    }

    fn icon(&self) -> &str {
        "zcode"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::install(&profiles::zcode_profile())?;
        log::info!("ZCode hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall(&profiles::zcode_profile())?;
        log::info!("ZCode hooks removed");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }

    fn detect_status_now(&self) -> AdapterStatus {
        Self::detect_status()
    }

    fn parse_event(&self, raw: &Value) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        let session_id = string_field(raw, &["session_id", "sessionId"])
            .unwrap_or("unknown")
            .to_string();
        let event = string_field(raw, &["event", "hook_event_name", "hookEventName"]).unwrap_or("");
        let cwd = string_field(raw, &["cwd"]).unwrap_or("").to_string();
        let tool_name = || {
            string_field(raw, &["tool", "tool_name", "toolName"])
                .unwrap_or("Tool")
                .to_string()
        };
        let tool_input = || {
            raw.get("tool_input")
                .or_else(|| raw.get("toolInput"))
                .map(json_value_text)
                .unwrap_or_default()
        };

        match event {
            "SessionStart" => Ok(AgentEvent::SessionStart {
                session_id,
                project: super::project_name_from_path(&cwd),
                cwd,
                terminal: string_field(raw, &["tty"]).unwrap_or("").to_string(),
                agent_type: "zcode".to_string(),
            }),
            "UserPromptSubmit" => Ok(AgentEvent::Processing {
                session_id,
                description: "Processing ZCode request".to_string(),
            }),
            "PreToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: tool_name(),
                tool_input: tool_input(),
                tool_target: None,
                status: "running".to_string(),
            }),
            "PermissionRequest" => Ok(AgentEvent::PermissionRequest {
                session_id,
                tool_name: tool_name(),
                diff: None,
                options: None,
            }),
            "AskQuestion" => parse_question_event(raw, session_id),
            "PlanApproval" => parse_plan_event(raw, session_id),
            "PostToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: tool_name(),
                tool_input: tool_input(),
                tool_target: None,
                status: if raw.get("tool_error").is_some() {
                    "error".to_string()
                } else {
                    "success".to_string()
                },
            }),
            "PostToolUseFailure" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: tool_name(),
                tool_input: tool_input(),
                tool_target: None,
                status: "error".to_string(),
            }),
            "Stop" => Ok(AgentEvent::AssistantResponseComplete {
                session_id,
                text: string_field(
                    raw,
                    &[
                        "summary",
                        "responseText",
                        "responsePreview",
                        "last_assistant_message",
                        "message",
                    ],
                )
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("ZCode turn completed")
                .to_string(),
            }),
            other => Ok(AgentEvent::Processing {
                session_id,
                description: format!("ZCode event: {other}"),
            }),
        }
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![self.config_path()]
    }

    fn hooks_installed(&self) -> bool {
        profiles::is_installed(&profiles::zcode_profile())
    }
}

fn string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
}

fn json_value_text(value: &Value) -> String {
    value
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn parse_question_event(
    raw: &Value,
    session_id: String,
) -> Result<AgentEvent, Box<dyn std::error::Error>> {
    let question = string_field(raw, &["question"]).unwrap_or("").to_string();
    let options = string_array(raw.get("options"));
    let descriptions = string_array(raw.get("descriptions"));
    let header = string_field(raw, &["header"]).map(ToString::to_string);
    let multi_select = raw
        .get("multi_select")
        .or_else(|| raw.get("multiSelect"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let questions = raw
        .get("questions")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(QuestionItem {
                        question: item.get("question")?.as_str()?.to_string(),
                        header: item
                            .get("header")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        options: item
                            .get("options")
                            .and_then(Value::as_array)
                            .map(|options| {
                                options
                                    .iter()
                                    .filter_map(|option| {
                                        Some(QuestionOption {
                                            label: option.get("label")?.as_str()?.to_string(),
                                            description: option
                                                .get("description")
                                                .and_then(Value::as_str)
                                                .map(ToString::to_string),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        multi_select: item
                            .get("multiSelect")
                            .or_else(|| item.get("multi_select"))
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(AgentEvent::AskQuestion {
        session_id,
        question,
        options,
        descriptions,
        header,
        multi_select,
        questions,
    })
}

fn parse_plan_event(
    raw: &Value,
    session_id: String,
) -> Result<AgentEvent, Box<dyn std::error::Error>> {
    let title = string_field(raw, &["plan_title", "planTitle"])
        .unwrap_or("Plan")
        .to_string();
    let content = string_field(raw, &["plan_content", "planContent", "plan"])
        .unwrap_or("")
        .to_string();
    let permissions = raw
        .get("requested_permissions")
        .or_else(|| raw.get("allowedPrompts"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.as_str().map(ToString::to_string).unwrap_or_else(|| {
                        match (
                            item.get("tool").and_then(Value::as_str),
                            item.get("prompt").and_then(Value::as_str),
                        ) {
                            (Some(tool), Some(prompt)) => format!("{tool}: {prompt}"),
                            _ => item.to_string(),
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(AgentEvent::PlanApproval {
        session_id,
        title,
        content,
        permissions,
    })
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn adapter() -> ZcodeAdapter {
        ZcodeAdapter {
            status: AdapterStatus::Unavailable,
        }
    }

    #[test]
    fn parses_zcode_session_start() {
        let event = adapter()
            .parse_event(&json!({
                "agent": "zcode",
                "event": "SessionStart",
                "session_id": "sess_123",
                "cwd": "/Users/me/code/demo"
            }))
            .expect("parse event");

        match event {
            AgentEvent::SessionStart {
                session_id,
                project,
                agent_type,
                ..
            } => {
                assert_eq!(session_id, "sess_123");
                assert_eq!(project, "demo");
                assert_eq!(agent_type, "zcode");
            }
            other => panic!("expected SessionStart, got {other:?}"),
        }
    }

    #[test]
    fn parses_zcode_tool_lifecycle() {
        let running = adapter()
            .parse_event(&json!({
                "event": "PreToolUse",
                "session_id": "sess_123",
                "tool": "Bash",
                "tool_input": {"command": "pnpm test"}
            }))
            .expect("parse event");
        let failed = adapter()
            .parse_event(&json!({
                "event": "PostToolUseFailure",
                "session_id": "sess_123",
                "tool": "Bash",
                "tool_input": {"command": "pnpm test"}
            }))
            .expect("parse event");

        assert!(matches!(
            running,
            AgentEvent::ToolUse { status, .. } if status == "running"
        ));
        assert!(matches!(
            failed,
            AgentEvent::ToolUse { status, .. } if status == "error"
        ));
    }

    #[test]
    fn parses_zcode_permission_request() {
        let event = adapter()
            .parse_event(&json!({
                "event": "PermissionRequest",
                "session_id": "sess_123",
                "tool": "Edit"
            }))
            .expect("parse event");

        assert!(matches!(
            event,
            AgentEvent::PermissionRequest { tool_name, .. } if tool_name == "Edit"
        ));
    }

    #[test]
    fn uses_zcode_response_as_completion_text() {
        let event = adapter()
            .parse_event(&json!({
                "event": "Stop",
                "session_id": "sess_123",
                "summary": "Implemented ZCode support"
            }))
            .expect("parse event");

        assert!(matches!(
            event,
            AgentEvent::AssistantResponseComplete { text, .. }
                if text == "Implemented ZCode support"
        ));
    }

    #[test]
    fn parses_bridge_question_event() {
        let event = adapter()
            .parse_event(&json!({
                "event": "AskQuestion",
                "session_id": "sess_123",
                "question": "Which view?",
                "options": ["Overlay", "Settings"],
                "descriptions": ["Floating island", "Configuration page"],
                "header": "View",
                "questions": [{
                    "question": "Which view?",
                    "header": "View",
                    "options": [{"label": "Overlay", "description": "Floating island"}],
                    "multiSelect": false
                }]
            }))
            .expect("parse question");

        assert!(matches!(
            event,
            AgentEvent::AskQuestion { question, options, questions, .. }
                if question == "Which view?"
                    && options == vec!["Overlay", "Settings"]
                    && questions.len() == 1
        ));
    }

    #[test]
    fn parses_bridge_plan_event() {
        let event = adapter()
            .parse_event(&json!({
                "event": "PlanApproval",
                "session_id": "sess_123",
                "plan_title": "ZCode integration",
                "plan_content": "1. Enable hooks",
                "requested_permissions": [
                    {"tool": "Bash", "prompt": "run tests"}
                ]
            }))
            .expect("parse plan");

        assert!(matches!(
            event,
            AgentEvent::PlanApproval { title, content, permissions, .. }
                if title == "ZCode integration"
                    && content == "1. Enable hooks"
                    && permissions == vec!["Bash: run tests"]
        ));
    }
}
