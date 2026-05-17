// OpenCodeAdapter — Agent adapter for OpenCode AI

use super::profiles;
use super::{AdapterStatus, AgentAdapter, AgentEvent, QuestionItem, QuestionOption};
use std::path::PathBuf;

pub struct OpenCodeAdapter {
    status: AdapterStatus,
}

impl OpenCodeAdapter {
    pub fn new() -> Self {
        let status = if Self::is_installed() {
            AdapterStatus::Available
        } else {
            AdapterStatus::Unavailable
        };
        Self { status }
    }

    fn is_installed() -> bool {
        std::process::Command::new("which")
            .arg("opencode")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn plugin_path(&self) -> PathBuf {
        profiles::configuration_url(&profiles::opencode_profile())
    }
}

impl AgentAdapter for OpenCodeAdapter {
    fn name(&self) -> &str {
        "opencode"
    }
    fn display_name(&self) -> &str {
        "OpenCode"
    }
    fn icon(&self) -> &str {
        "opencode"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::install(&profiles::opencode_profile())?;
        log::info!("OpenCode plugin hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall(&profiles::opencode_profile())?;
        log::info!("OpenCode plugin hooks removed");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
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
            "SessionStart" => Ok(AgentEvent::SessionStart {
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
                agent_type: "opencode".to_string(),
            }),
            "SessionEnd" | "Stop" => Ok(AgentEvent::SessionEnd { session_id }),
            "PreToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: raw
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                tool_input: raw
                    .get("tool_input")
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                tool_target: None,
                status: "running".to_string(),
            }),
            "PostToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: raw
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                tool_input: raw
                    .get("tool_input")
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                tool_target: None,
                status: "success".to_string(),
            }),
            "AskQuestion" => {
                let questions = raw
                    .get("questions")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| QuestionItem {
                                question: item
                                    .get("question")
                                    .and_then(|value| value.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                header: item
                                    .get("header")
                                    .and_then(|value| value.as_str())
                                    .map(|value| value.to_string()),
                                options: item
                                    .get("options")
                                    .and_then(|value| value.as_array())
                                    .map(|options| {
                                        options
                                            .iter()
                                            .filter_map(|option| {
                                                option.get("label").and_then(|label| {
                                                    label.as_str().map(|label| QuestionOption {
                                                        label: label.to_string(),
                                                        description: option
                                                            .get("description")
                                                            .and_then(|value| value.as_str())
                                                            .map(|value| value.to_string()),
                                                    })
                                                })
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                multi_select: item
                                    .get("multiSelect")
                                    .or_else(|| item.get("multiple"))
                                    .and_then(|value| value.as_bool())
                                    .unwrap_or(false),
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Ok(AgentEvent::AskQuestion {
                    session_id,
                    question: raw
                        .get("question")
                        .and_then(|value| value.as_str())
                        .unwrap_or("OpenCode needs input")
                        .to_string(),
                    options: raw
                        .get("options")
                        .and_then(|value| value.as_array())
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|value| value.as_str().map(|value| value.to_string()))
                                .collect()
                        })
                        .unwrap_or_default(),
                    descriptions: raw
                        .get("descriptions")
                        .and_then(|value| value.as_array())
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|value| value.as_str().map(|value| value.to_string()))
                                .collect()
                        })
                        .unwrap_or_default(),
                    header: raw
                        .get("header")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string()),
                    multi_select: raw
                        .get("multi_select")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false),
                    questions,
                })
            }
            "PermissionRequest" => Ok(AgentEvent::PermissionRequest {
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
        vec![self.plugin_path()]
    }

    fn hooks_installed(&self) -> bool {
        profiles::is_installed(&profiles::opencode_profile())
    }
}
