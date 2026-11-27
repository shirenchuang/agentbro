use super::profiles;
use super::{AdapterStatus, AgentAdapter, AgentEvent};
use serde_json::Value;
use std::path::PathBuf;

pub struct AntiGravityAdapter {
    agent_id: &'static str,
    config_root: PathBuf,
    legacy_settings_path: PathBuf,
    status: AdapterStatus,
}

impl AntiGravityAdapter {
    pub fn new() -> Self {
        Self::with_agent_id("antigravity")
    }

    #[cfg(target_os = "macos")]
    pub fn gemini_desktop() -> Self {
        Self::with_agent_id(super::gemini_desktop::ID)
    }

    fn with_agent_id(agent_id: &'static str) -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let status = Self::detect_status(agent_id);
        Self {
            agent_id,
            config_root: home.join(".gemini").join("config"),
            legacy_settings_path: home.join(".antigravity").join("settings.json"),
            status,
        }
    }

    fn detect_status(agent_id: &str) -> AdapterStatus {
        #[cfg(target_os = "macos")]
        if agent_id == super::gemini_desktop::ID {
            return if super::gemini_desktop::has_bundled_runtime() {
                AdapterStatus::Available
            } else {
                AdapterStatus::Unavailable
            };
        }
        super::programs::detected_status_for_agent_program(agent_id)
    }

    fn hooks_path(&self) -> PathBuf {
        self.config_root.join("hooks.json")
    }
}

impl AgentAdapter for AntiGravityAdapter {
    fn name(&self) -> &str {
        self.agent_id
    }

    fn display_name(&self) -> &str {
        if self.agent_id == super::gemini_desktop::ID {
            "Gemini Desktop"
        } else {
            "Antigravity"
        }
    }

    fn icon(&self) -> &str {
        if self.agent_id == super::gemini_desktop::ID {
            "gemini"
        } else {
            "antigravity"
        }
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::install_at(&profiles::antigravity_profile(), &self.hooks_path())?;
        profiles::remove_legacy_antigravity_hooks_at(&self.legacy_settings_path)?;
        log::info!("Antigravity desktop and agy CLI hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall_at(&profiles::antigravity_profile(), &self.hooks_path())?;
        profiles::remove_legacy_antigravity_hooks_at(&self.legacy_settings_path)?;
        log::info!("Antigravity desktop and agy CLI hooks removed");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }

    fn detect_status_now(&self) -> AdapterStatus {
        Self::detect_status(self.agent_id)
    }

    fn parse_event(&self, raw: &Value) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        let session_id = string_field(raw, &["session_id", "sessionId", "conversationId"])
            .unwrap_or("unknown")
            .to_string();
        let event = string_field(raw, &["event", "hook_event_name", "hookEventName"]).unwrap_or("");
        let cwd = string_field(raw, &["cwd"])
            .or_else(|| first_string_array_field(raw, "workspacePaths"))
            .unwrap_or("")
            .to_string();
        let tool_call = raw.get("toolCall");
        let tool_name = string_field(raw, &["tool", "tool_name", "toolName"])
            .or_else(|| {
                tool_call
                    .and_then(|call| call.get("name"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("Tool")
            .to_string();
        let tool_input = raw
            .get("tool_input")
            .or_else(|| raw.get("toolInput"))
            .or_else(|| tool_call.and_then(|call| call.get("args")))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        match event {
            "PostToolUse" => {
                let error = string_field(raw, &["tool_error", "error"])
                    .filter(|value| !value.trim().is_empty());
                Ok(AgentEvent::ToolUse {
                    session_id,
                    tool_name,
                    tool_input: tool_input.to_string(),
                    tool_target: tool_target(&tool_input),
                    status: if error.is_some() {
                        "failure".to_string()
                    } else {
                        "success".to_string()
                    },
                })
            }
            "PreInvocation" => Ok(AgentEvent::Processing {
                session_id,
                description: format!("{} is thinking", self.display_name()),
            }),
            "PostInvocation" => Ok(AgentEvent::Processing {
                session_id,
                description: format!("{} is processing the next step", self.display_name()),
            }),
            "Stop" => {
                let error = string_field(raw, &["error"]).filter(|value| !value.trim().is_empty());
                let termination_reason =
                    string_field(raw, &["termination_reason", "terminationReason"])
                        .unwrap_or("completed");
                if let Some(message) = error {
                    Ok(AgentEvent::Error {
                        session_id,
                        message: message.to_string(),
                    })
                } else if termination_reason.eq_ignore_ascii_case("error") {
                    Ok(AgentEvent::Error {
                        session_id,
                        message: format!("{} stopped with an error", self.display_name()),
                    })
                } else if self.agent_id == super::gemini_desktop::ID
                    && raw
                        .get("fully_idle")
                        .or_else(|| raw.get("fullyIdle"))
                        .and_then(Value::as_bool)
                        == Some(false)
                {
                    Ok(AgentEvent::Processing {
                        session_id,
                        description: "Gemini is finishing background tasks".to_string(),
                    })
                } else {
                    Ok(AgentEvent::TaskComplete {
                        session_id,
                        summary: if self.agent_id == super::gemini_desktop::ID {
                            "Gemini coding turn completed".to_string()
                        } else {
                            format!("Antigravity completed ({termination_reason})")
                        },
                    })
                }
            }
            "SessionStart" | "session_start" => Ok(AgentEvent::SessionStart {
                session_id,
                project: super::project_name_from_path(&cwd),
                cwd,
                terminal: String::new(),
                agent_type: self.agent_id.to_string(),
            }),
            "SessionEnd" | "session_end" => Ok(AgentEvent::SessionEnd { session_id }),
            _ => Ok(AgentEvent::Processing {
                session_id,
                description: format!("Antigravity event: {event}"),
            }),
        }
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![self.hooks_path()]
    }
}

fn string_field<'a>(raw: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| raw.get(*key).and_then(Value::as_str))
}

fn first_string_array_field<'a>(raw: &'a Value, key: &str) -> Option<&'a str> {
    raw.get(key)
        .and_then(Value::as_array)
        .and_then(|values| values.iter().find_map(Value::as_str))
}

fn tool_target(input: &Value) -> Option<String> {
    [
        "TargetFile",
        "AbsolutePath",
        "DirectoryPath",
        "SearchPath",
        "Cwd",
        "Url",
    ]
    .iter()
    .find_map(|key| input.get(*key).and_then(Value::as_str))
    .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_documented_stop_payload() {
        let adapter = AntiGravityAdapter::new();
        let event = adapter
            .parse_event(&serde_json::json!({
                "event": "Stop",
                "conversationId": "conversation-2",
                "terminationReason": "model_stop",
                "fullyIdle": true
            }))
            .expect("parse Stop");

        assert!(matches!(
            event,
            AgentEvent::TaskComplete {
                session_id,
                summary,
            } if session_id == "conversation-2" && summary.contains("model_stop")
        ));
    }

    #[test]
    fn post_tool_failure_uses_documented_error_field() {
        let adapter = AntiGravityAdapter::new();
        let event = adapter
            .parse_event(&serde_json::json!({
                "event": "PostToolUse",
                "conversationId": "conversation-3",
                "error": "exit status 1"
            }))
            .expect("parse PostToolUse");

        assert!(matches!(
            event,
            AgentEvent::ToolUse { status, .. } if status == "failure"
        ));
    }
}
