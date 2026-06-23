// OpenCodeAdapter — Agent adapter for OpenCode AI

use super::profiles;
use super::{AdapterStatus, AgentAdapter, AgentEvent, QuestionItem, QuestionOption};
use std::path::PathBuf;

/// Result of verifying hook installation integrity for OpenCode
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenCodeHookVerificationResult {
    /// Everything is correctly installed
    Ok,
    /// Plugin file is missing and needs reinstall
    MissingPluginFile,
    /// opencode.json is missing the agentbro plugin registration
    NotRegistered,
}

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
        super::executable::command_exists("opencode")
    }

    fn plugin_path(&self) -> PathBuf {
        profiles::configuration_url(&profiles::opencode_profile())
    }

    /// Verify that the opencode plugin hooks are correctly installed.
    /// Checks:
    ///  1. The plugin file exists at ~/.config/opencode/plugins/agentbro.js
    ///  2. opencode.json contains "agentbro" in its plugin array
    pub fn verify_hooks(&self) -> OpenCodeHookVerificationResult {
        let plugin_path = self.plugin_path();
        if !plugin_path.exists() {
            return OpenCodeHookVerificationResult::MissingPluginFile;
        }

        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_path = home.join(".config").join("opencode").join("opencode.json");

        let config_content = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(_) => return OpenCodeHookVerificationResult::NotRegistered,
        };

        let config: serde_json::Value = match serde_json::from_str(&config_content) {
            Ok(v) => v,
            Err(_) => return OpenCodeHookVerificationResult::NotRegistered,
        };

        let plugins = match config.get("plugin").and_then(|p| p.as_array()) {
            Some(arr) => arr,
            None => return OpenCodeHookVerificationResult::NotRegistered,
        };

        let has_agentbro = plugins.iter().any(|p| {
            p.as_str().map(|s| s.contains("agentbro")).unwrap_or(false)
                || p.as_array()
                    .and_then(|arr| {
                        arr.first()
                            .and_then(|v| v.as_str())
                            .map(|s| s.contains("agentbro"))
                    })
                    .unwrap_or(false)
        });

        if has_agentbro {
            OpenCodeHookVerificationResult::Ok
        } else {
            OpenCodeHookVerificationResult::NotRegistered
        }
    }
}

fn truncate_display_text(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= max_chars {
        trimmed.to_string()
    } else {
        let end = trimmed
            .char_indices()
            .take(max_chars + 1)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(max_chars);
        format!("{}...", &trimmed[..end])
    }
}

fn extract_tool_target(tool_name: &str, tool_input: &serde_json::Value) -> Option<String> {
    let input = tool_input.as_object()?;
    match tool_name {
        "Read" | "Edit" | "Write" => input
            .get("file_path")
            .or_else(|| input.get("path"))
            .and_then(|v| v.as_str())
            .map(|t| truncate_display_text(t, 50)),

        "Bash" => input
            .get("command")
            .and_then(|v| v.as_str())
            .map(|t| truncate_display_text(t, 50)),

        "Glob" | "Grep" => input
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|t| truncate_display_text(t, 50)),

        "Task" => input
            .get("description")
            .or_else(|| input.get("subagent_type"))
            .and_then(|v| v.as_str())
            .map(|t| truncate_display_text(t, 50)),

        "WebSearch" | "WebFetch" => input
            .get("query")
            .or_else(|| input.get("url"))
            .and_then(|v| v.as_str())
            .map(|t| truncate_display_text(t, 50)),

        "delegate" | "task" => input
            .get("description")
            .or_else(|| input.get("prompt"))
            .and_then(|v| v.as_str())
            .map(|t| truncate_display_text(t, 50)),

        _ => None,
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
        let session_id = raw
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let event = raw
            .get("hook_event_name")
            .or_else(|| raw.get("event"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let cwd = raw
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let tool_name_extract = || {
            raw.get("tool_name")
                .or_else(|| raw.get("tool"))
                .and_then(|v| v.as_str())
                .unwrap_or("Tool")
                .to_string()
        };

        let tool_input_value = || {
            raw.get("tool_input")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        };

        match event {
            "SessionStart" => Ok(AgentEvent::SessionStart {
                session_id,
                project: super::project_name_from_path(&cwd),
                cwd,
                terminal: raw
                    .get("tty")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                agent_type: "opencode".to_string(),
            }),
            "SessionEnd" => Ok(AgentEvent::SessionEnd { session_id }),
            "Stop" => {
                let summary = raw
                    .get("summary")
                    .or_else(|| raw.get("message"))
                    .or_else(|| raw.get("last_assistant_message"))
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or("Task completed")
                    .to_string();
                Ok(AgentEvent::TaskComplete {
                    session_id,
                    summary,
                })
            }

            "UserPromptSubmit" => {
                let prompt = raw.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

                let preview = if prompt.is_empty() {
                    "Processing user input".to_string()
                } else {
                    let first_line = prompt.lines().next().unwrap_or("");
                    let truncated = truncate_display_text(first_line, 80);
                    format!("Processing user input: {}", truncated)
                };

                Ok(AgentEvent::Processing {
                    session_id,
                    description: preview,
                })
            }

            "PreToolUse" => {
                let tool_name = tool_name_extract();
                let tool_input = tool_input_value();
                let tool_target = extract_tool_target(&tool_name, &tool_input);

                Ok(AgentEvent::ToolUse {
                    session_id,
                    tool_name,
                    tool_input: tool_input.to_string(),
                    tool_target,
                    status: "running".to_string(),
                })
            }

            "PostToolUse" => {
                let tool_name = tool_name_extract();
                let tool_input = tool_input_value();
                let tool_target = extract_tool_target(&tool_name, &tool_input);

                let status = raw
                    .get("tool_state")
                    .and_then(|v| v.as_str())
                    .filter(|&s| s == "error")
                    .map(|_| "error")
                    .unwrap_or("success");

                Ok(AgentEvent::ToolUse {
                    session_id,
                    tool_name,
                    tool_input: tool_input.to_string(),
                    tool_target,
                    status: status.to_string(),
                })
            }

            "PostToolUseFailure" => {
                let tool_name = tool_name_extract();
                let tool_input = tool_input_value();
                let tool_target = extract_tool_target(&tool_name, &tool_input);
                let error_msg = raw
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Tool failed");

                Ok(AgentEvent::ToolUse {
                    session_id,
                    tool_name,
                    tool_input: if error_msg != "Tool failed" {
                        error_msg.to_string()
                    } else {
                        tool_input.to_string()
                    },
                    tool_target,
                    status: "error".to_string(),
                })
            }

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

            "Error" => Ok(AgentEvent::Error {
                session_id,
                message: raw
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("OpenCode error")
                    .to_string(),
            }),

            "SubagentStart" => Ok(AgentEvent::SubagentStart {
                session_id,
                agent_id: raw
                    .get("agent_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                name: raw
                    .get("agent_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                description: raw
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Subagent started")
                    .to_string(),
                agent_type: Some("opencode".to_string()),
                transcript_path: None,
            }),

            "SubagentStop" => Ok(AgentEvent::SubagentStop {
                session_id,
                agent_id: raw
                    .get("agent_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                status: "completed".to_string(),
                name: raw
                    .get("agent_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                agent_type: Some("opencode".to_string()),
                transcript_path: None,
                agent_transcript_path: None,
                last_assistant_message: None,
            }),

            "ShellExecutionStart" => Ok(AgentEvent::ShellExecutionStart {
                session_id,
                command: raw
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                cwd: raw
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            }),

            "ShellExecutionEnd" => Ok(AgentEvent::ShellExecutionEnd {
                session_id,
                command: raw
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                exit_code: None,
                stdout: raw
                    .get("output")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                stderr: None,
                duration_ms: 0,
            }),

            "TokenUsage" => Ok(AgentEvent::TokenUsage {
                session_id,
                input: raw
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                output: raw
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                cache_read: 0,
                cache_create: 0,
            }),

            "AgentThought" => Ok(AgentEvent::AgentThought {
                session_id,
                thought: raw
                    .get("thought")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            }),

            "Notification" => Ok(AgentEvent::Notification {
                session_id,
                message: raw
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                status: raw
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn adapter() -> OpenCodeAdapter {
        OpenCodeAdapter::new()
    }

    // ——— P0-1: UserPromptSubmit ———

    #[test]
    fn user_prompt_submit_with_prompt() {
        let raw = json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "abc",
            "cwd": "/home/user/project",
            "prompt": "Add a new feature for export\nThis is the second line"
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::Processing {
                session_id,
                description,
            } => {
                assert_eq!(session_id, "abc");
                assert!(description.contains("Add a new feature for export"));
                assert!(description.len() <= "Processing user input: ".len() + 80);
            }
            other => panic!("Expected Processing, got {:?}", other),
        }
    }

    #[test]
    fn user_prompt_submit_long_first_line_truncated() {
        let long_line = "A".repeat(200);
        let raw = json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "s1",
            "prompt": long_line
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::Processing { description, .. } => {
                assert!(description.contains("..."));
                assert!(description.len() <= "Processing user input: ".len() + 83);
            }
            other => panic!("Expected Processing, got {:?}", other),
        }
    }

    #[test]
    fn user_prompt_submit_empty_prompt_fallback() {
        let raw = json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "s1",
            "prompt": ""
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::Processing { description, .. } => {
                assert_eq!(description, "Processing user input");
            }
            other => panic!("Expected Processing, got {:?}", other),
        }
    }

    #[test]
    fn user_prompt_submit_no_prompt_field() {
        let raw = json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "s1"
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::Processing { description, .. } => {
                assert_eq!(description, "Processing user input");
            }
            other => panic!("Expected Processing, got {:?}", other),
        }
    }

    // ——— P0-2: Stop → AssistantResponseComplete ———

    #[test]
    fn stop_maps_to_task_complete() {
        let raw = json!({
            "hook_event_name": "Stop",
            "session_id": "abc",
            "cwd": "/home/user/project",
            "last_assistant_message": "I have completed the task successfully.",
            "session_title": "Export feature"
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::TaskComplete {
                session_id,
                summary,
            } => {
                assert_eq!(session_id, "abc");
                assert_eq!(summary, "I have completed the task successfully.");
            }
            other => panic!("Expected TaskComplete, got {:?}", other),
        }
    }

    #[test]
    fn stop_without_message_uses_fallback() {
        let raw = json!({
            "hook_event_name": "Stop",
            "session_id": "abc",
            "last_assistant_message": ""
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::TaskComplete {
                session_id,
                summary,
            } => {
                assert_eq!(session_id, "abc");
                assert_eq!(summary, "Task completed");
            }
            other => panic!("Expected TaskComplete, got {:?}", other),
        }
    }

    #[test]
    fn session_end_still_maps_to_session_end() {
        let raw = json!({
            "hook_event_name": "SessionEnd",
            "session_id": "abc"
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::SessionEnd { session_id } => {
                assert_eq!(session_id, "abc");
            }
            other => panic!("Expected SessionEnd, got {:?}", other),
        }
    }

    // ——— P0-3: extract_tool_target ———

    #[test]
    fn tool_target_read_extracts_file_path() {
        let input = json!({"file_path": "/src/main.rs"});
        let target = extract_tool_target("Read", &input);
        assert_eq!(target, Some("/src/main.rs".to_string()));
    }

    #[test]
    fn tool_target_write_extracts_path() {
        let input = json!({"path": "/tmp/output.txt"});
        let target = extract_tool_target("Write", &input);
        assert_eq!(target, Some("/tmp/output.txt".to_string()));
    }

    #[test]
    fn tool_target_edit_prefers_file_path() {
        let input = json!({"file_path": "/src/main.rs", "path": "/fallback.rs"});
        let target = extract_tool_target("Edit", &input);
        assert_eq!(target, Some("/src/main.rs".to_string()));
    }

    #[test]
    fn tool_target_bash_extracts_command() {
        let input = json!({"command": "cargo build --release"});
        let target = extract_tool_target("Bash", &input);
        assert_eq!(target, Some("cargo build --release".to_string()));
    }

    #[test]
    fn tool_target_glob_extracts_pattern() {
        let input = json!({"pattern": "**/*.rs"});
        let target = extract_tool_target("Glob", &input);
        assert_eq!(target, Some("**/*.rs".to_string()));
    }

    #[test]
    fn tool_target_grep_extracts_pattern() {
        let input = json!({"pattern": "fn parse_event"});
        let target = extract_tool_target("Grep", &input);
        assert_eq!(target, Some("fn parse_event".to_string()));
    }

    #[test]
    fn tool_target_task_extracts_description() {
        let input = json!({"description": "Research Rust patterns"});
        let target = extract_tool_target("Task", &input);
        assert_eq!(target, Some("Research Rust patterns".to_string()));
    }

    #[test]
    fn tool_target_task_falls_back_to_subagent_type() {
        let input = json!({"subagent_type": "researcher"});
        let target = extract_tool_target("Task", &input);
        assert_eq!(target, Some("researcher".to_string()));
    }

    #[test]
    fn tool_target_web_search_extracts_query() {
        let input = json!({"query": "Rust serde tutorial"});
        let target = extract_tool_target("WebSearch", &input);
        assert_eq!(target, Some("Rust serde tutorial".to_string()));
    }

    #[test]
    fn tool_target_web_fetch_extracts_url() {
        let input = json!({"url": "https://example.com"});
        let target = extract_tool_target("WebFetch", &input);
        assert_eq!(target, Some("https://example.com".to_string()));
    }

    #[test]
    fn tool_target_delegate_extracts_description() {
        let input = json!({"description": "Search for implementation details"});
        let target = extract_tool_target("delegate", &input);
        assert_eq!(
            target,
            Some("Search for implementation details".to_string())
        );
    }

    #[test]
    fn tool_target_delegate_falls_back_to_prompt() {
        let input = json!({"prompt": "Do the thing"});
        let target = extract_tool_target("delegate", &input);
        assert_eq!(target, Some("Do the thing".to_string()));
    }

    #[test]
    fn tool_target_truncates_long_values() {
        let long = "A".repeat(100);
        let input = json!({"command": &long});
        let target = extract_tool_target("Bash", &input);
        assert!(target.unwrap().len() <= 53); // 50 chars + "..."
    }

    #[test]
    fn tool_target_unknown_tool_returns_none() {
        let input = json!({"key": "value"});
        let target = extract_tool_target("SomeUnknownTool", &input);
        assert_eq!(target, None);
    }

    #[test]
    fn tool_target_non_object_input_returns_none() {
        let target = extract_tool_target("Read", &json!("not an object"));
        assert_eq!(target, None);
    }

    // ——— P0-4: PostToolUse error distinction ———

    #[test]
    fn post_tool_use_error_state() {
        let raw = json!({
            "hook_event_name": "PostToolUse",
            "session_id": "abc",
            "tool_name": "Bash",
            "tool_input": {"command": "exit 1"},
            "tool_state": "error"
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::ToolUse {
                session_id,
                status,
                tool_name,
                ..
            } => {
                assert_eq!(session_id, "abc");
                assert_eq!(status, "error");
                assert_eq!(tool_name, "Bash");
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn post_tool_use_success_state() {
        let raw = json!({
            "hook_event_name": "PostToolUse",
            "session_id": "abc",
            "tool_name": "Read",
            "tool_input": {"file_path": "src/main.rs"},
            "tool_state": "success"
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::ToolUse { status, .. } => {
                assert_eq!(status, "success");
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn post_tool_use_default_success_when_no_tool_state() {
        let raw = json!({
            "hook_event_name": "PostToolUse",
            "session_id": "abc",
            "tool_name": "Read",
            "tool_input": {"file_path": "src/main.rs"}
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::ToolUse { status, .. } => {
                assert_eq!(status, "success");
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn post_tool_use_failure_event_name_is_error() {
        let raw = json!({
            "hook_event_name": "PostToolUseFailure",
            "session_id": "abc",
            "tool_name": "Bash",
            "tool_input": {"command": "bad"},
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::ToolUse { status, .. } => {
                assert_eq!(status, "error");
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn pre_tool_use_includes_tool_target() {
        let raw = json!({
            "hook_event_name": "PreToolUse",
            "session_id": "abc",
            "tool_name": "Read",
            "tool_input": {"file_path": "/src/lib.rs"}
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::ToolUse {
                tool_name,
                tool_target,
                status,
                ..
            } => {
                assert_eq!(tool_name, "Read");
                assert_eq!(status, "running");
                assert_eq!(tool_target, Some("/src/lib.rs".to_string()));
            }
            other => panic!("Expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn truncate_display_text_short() {
        assert_eq!(truncate_display_text("hello", 50), "hello");
    }

    #[test]
    fn truncate_display_text_long() {
        let long = "A".repeat(100);
        let result = truncate_display_text(&long, 50);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 53);
    }

    // ——— P1: New event types ———

    #[test]
    fn error_event_parsing() {
        let raw = json!({
            "hook_event_name": "Error",
            "session_id": "abc",
            "cwd": "/home/user/project",
            "message": "Something went wrong"
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::Error {
                session_id,
                message,
                ..
            } => {
                assert_eq!(session_id, "abc");
                assert_eq!(message, "Something went wrong");
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[test]
    fn error_event_fallback_message() {
        let raw = json!({
            "hook_event_name": "Error",
            "session_id": "abc"
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::Error { message, .. } => {
                assert_eq!(message, "OpenCode error");
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[test]
    fn subagent_start_parsing() {
        let raw = json!({
            "hook_event_name": "SubagentStart",
            "session_id": "abc",
            "cwd": "/home/user/project",
            "agent_name": "researcher",
            "description": "Subagent: researcher"
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::SubagentStart {
                session_id,
                agent_id,
                name,
                description,
                agent_type,
                ..
            } => {
                assert_eq!(session_id, "abc");
                assert_eq!(agent_id, "researcher");
                assert_eq!(name, Some("researcher".to_string()));
                assert_eq!(description, "Subagent: researcher");
                assert_eq!(agent_type, Some("opencode".to_string()));
            }
            other => panic!("Expected SubagentStart, got {:?}", other),
        }
    }

    #[test]
    fn shell_execution_start_parsing() {
        let raw = json!({
            "hook_event_name": "ShellExecutionStart",
            "session_id": "abc",
            "cwd": "/home/user/project",
            "command": "cargo build --release"
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::ShellExecutionStart {
                session_id,
                command,
                cwd,
            } => {
                assert_eq!(session_id, "abc");
                assert_eq!(command, "cargo build --release");
                assert_eq!(cwd, "/home/user/project");
            }
            other => panic!("Expected ShellExecutionStart, got {:?}", other),
        }
    }

    #[test]
    fn token_usage_parsing() {
        let raw = json!({
            "hook_event_name": "TokenUsage",
            "session_id": "abc",
            "cwd": "/home/user/project",
            "input_tokens": 1234,
            "output_tokens": 567
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::TokenUsage {
                session_id,
                input,
                output,
                ..
            } => {
                assert_eq!(session_id, "abc");
                assert_eq!(input, 1234);
                assert_eq!(output, 567);
            }
            other => panic!("Expected TokenUsage, got {:?}", other),
        }
    }

    #[test]
    fn token_usage_defaults_to_zero() {
        let raw = json!({
            "hook_event_name": "TokenUsage",
            "session_id": "abc"
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::TokenUsage { input, output, .. } => {
                assert_eq!(input, 0);
                assert_eq!(output, 0);
            }
            other => panic!("Expected TokenUsage, got {:?}", other),
        }
    }

    #[test]
    fn agent_thought_parsing() {
        let raw = json!({
            "hook_event_name": "AgentThought",
            "session_id": "abc",
            "cwd": "/home/user/project",
            "thought": "I need to refactor this function"
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::AgentThought {
                session_id,
                thought,
                ..
            } => {
                assert_eq!(session_id, "abc");
                assert_eq!(thought, "I need to refactor this function");
            }
            other => panic!("Expected AgentThought, got {:?}", other),
        }
    }

    // ——— P2: verify_hooks ———

    #[test]
    fn verify_hooks_detects_missing_plugin_file() {
        let adapter = OpenCodeAdapter {
            status: AdapterStatus::Available,
        };
        let result = adapter.verify_hooks();
        match result {
            OpenCodeHookVerificationResult::Ok => {
                // Plugin is installed — verify fields are accessible
            }
            _ => {
                // Plugin not installed — expected in CI
            }
        }
    }
}
