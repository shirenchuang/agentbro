// ClaudeCodeAdapter — Agent adapter for Claude Code CLI
// Handles hook installation, event parsing, and communication via tmux

use super::{AdapterStatus, AgentAdapter, AgentEvent, QuestionItem, QuestionOption};
use std::path::PathBuf;

/// Bridge binary name
const BRIDGE_BINARY_NAME: &str = "agentbro-bridge";

/// Claude Code adapter implementation
pub struct ClaudeCodeAdapter {
    config_root: PathBuf,
    label: String,
    status: AdapterStatus,
}

impl ClaudeCodeAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".claude");
        let status = if Self::is_claude_code_installed() {
            AdapterStatus::Available
        } else {
            AdapterStatus::Unavailable
        };

        Self {
            config_root,
            label: "Claude Code".into(),
            status,
        }
    }

    /// Create an adapter for a custom engine instance path
    pub fn with_config_root(config_root: PathBuf, label: String) -> Self {
        let status = if Self::is_claude_code_installed() {
            AdapterStatus::Available
        } else {
            AdapterStatus::Unavailable
        };

        Self {
            config_root,
            label,
            status,
        }
    }

    /// Get the config root path for this instance
    pub fn config_root(&self) -> &PathBuf {
        &self.config_root
    }

    /// Get the projects directory for this instance (for conversation watching)
    pub fn projects_dir(&self) -> Option<PathBuf> {
        let dir = self.config_root.join("projects");
        if dir.is_dir() {
            Some(dir)
        } else {
            None
        }
    }

    /// Check if Claude Code CLI is installed
    fn is_claude_code_installed() -> bool {
        std::process::Command::new("which")
            .arg("claude")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Get the path where the bridge binary should be installed
    /// Uses ~/.agentbro/bin/ to avoid spaces in path (shared across all instances)
    fn bridge_binary_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        home.join(".agentbro").join("bin").join(BRIDGE_BINARY_NAME)
    }

    /// Get the settings.json path for this instance
    fn settings_path(&self) -> PathBuf {
        self.config_root.join("settings.json")
    }

    /// Find the source bridge binary next to the current executable
    fn find_source_bridge() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let exe_dir = exe.parent()?;
        let bridge = exe_dir.join(BRIDGE_BINARY_NAME);
        if bridge.exists() {
            Some(bridge)
        } else {
            None
        }
    }

    /// Install the bridge binary to ~/.agentbro/bin/
    /// Copies the compiled binary from the app bundle directory.
    fn install_bridge() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let dest = Self::bridge_binary_path();
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let source =
            Self::find_source_bridge().ok_or("Bridge binary not found next to main executable")?;

        std::fs::copy(&source, &dest)?;

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
        }

        log::info!("Bridge binary installed to {}", dest.display());
        Ok(dest)
    }

    /// Build the hook command string and stamp instance metadata into the bridge env.
    fn hook_command(&self) -> Result<String, Box<dyn std::error::Error>> {
        let bridge_path = Self::install_bridge()?;
        Ok(format!(
            "/usr/bin/env AGENTBRO_ENGINE_LABEL={} AGENTBRO_CONFIG_ROOT={} {}",
            shell_quote(&self.label),
            shell_quote(&self.config_root.display().to_string()),
            shell_quote(&bridge_path.display().to_string()),
        ))
    }

    /// All hook event names that Claude Code supports
    fn hook_events() -> Vec<&'static str> {
        vec![
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "PermissionRequest",
            "PermissionDenied",
            "Notification",
            "Stop",
            "StopFailure",
            "SubagentStart",
            "SubagentStop",
            "SessionStart",
            "SessionEnd",
            "PreCompact",
            "PostCompact",
            "beforeShellExecution",
            "afterShellExecution",
            "beforeMCPExecution",
            "afterMCPExecution",
            "afterAgentResponse",
            "afterAgentThought",
        ]
    }

    /// Hook events that need a matcher pattern
    fn events_with_matcher() -> Vec<&'static str> {
        vec![
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "PermissionRequest",
            "PermissionDenied",
            "Notification",
            "beforeShellExecution",
            "afterShellExecution",
            "beforeMCPExecution",
            "afterMCPExecution",
            "afterAgentResponse",
            "afterAgentThought",
        ]
    }

    /// Hook events that need a timeout (permission-related)
    fn events_with_timeout() -> Vec<&'static str> {
        vec!["PermissionRequest"]
    }

    /// Remove old Python hook artifacts (migration from Python to Rust bridge)
    fn cleanup_old_python_hook() {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let old_script = home.join(".agentbro").join("agentbro-hook.py");
        if old_script.exists() {
            if let Err(e) = std::fs::remove_file(&old_script) {
                log::warn!("Failed to remove old Python hook script: {}", e);
            } else {
                log::info!("Removed old Python hook script: {}", old_script.display());
            }
        }
    }
}

impl AgentAdapter for ClaudeCodeAdapter {
    fn name(&self) -> &str {
        "claude-code"
    }

    fn display_name(&self) -> &str {
        &self.label
    }

    fn icon(&self) -> &str {
        "claude"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Clean up old Python hook if present
        Self::cleanup_old_python_hook();

        let settings_path = self.settings_path();
        let hook_command = self.hook_command()?;

        // Read existing settings or create new
        let mut settings: serde_json::Value = if settings_path.exists() {
            let content = std::fs::read_to_string(&settings_path)?;
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        // Ensure hooks object exists
        if settings.get("hooks").is_none() {
            settings["hooks"] = serde_json::json!({});
        }

        let hooks = settings
            .get_mut("hooks")
            .and_then(|h| h.as_object_mut())
            .ok_or("Failed to access hooks object")?;

        let events_with_matcher = Self::events_with_matcher();
        let events_with_timeout = Self::events_with_timeout();

        // Add hook entries for each event using Claude Code's nested format:
        // { "hooks": { "EventName": [ { "matcher": "...", "hooks": [ { "type": "command", "command": "..." } ] } ] } }
        for event_name in Self::hook_events() {
            let use_matcher = events_with_matcher.contains(&event_name);
            let use_timeout = events_with_timeout.contains(&event_name);

            // Build the inner hook object
            let mut inner_hook = serde_json::json!({
                "type": "command",
                "command": hook_command
            });
            if use_timeout {
                inner_hook["timeout"] = serde_json::json!(86400);
            }

            // Build the outer hook group (with matcher if needed)
            let mut hook_group = serde_json::json!({
                "hooks": [inner_hook]
            });
            if use_matcher {
                hook_group["matcher"] = serde_json::json!("*");
            }

            if let Some(existing) = hooks.get_mut(event_name) {
                // If it's an array, check if we already have an agentbro entry
                if let Some(arr) = existing.as_array_mut() {
                    // Remove old Python hook entries and old bridge entries.
                    // Older builds used ~/.agent-island/bin/agent-island-bridge;
                    // leaving those around emits duplicate unlabeled events, so
                    // custom engines like AntCC can appear as plain Claude.
                    arr.retain(|group| {
                        let has_agentbro = |cmd: Option<&str>| -> bool {
                            cmd.map(|c| {
                                c.contains("agentbro")
                                    || c.contains("agent-island")
                                    || c.contains("vibe-island")
                            })
                            .unwrap_or(false)
                        };

                        // Check nested format
                        let nested_match = group
                            .get("hooks")
                            .and_then(|h| h.as_array())
                            .map(|hooks_arr| {
                                hooks_arr.iter().any(|h| {
                                    has_agentbro(h.get("command").and_then(|c| c.as_str()))
                                })
                            })
                            .unwrap_or(false);

                        // Check flat format
                        let flat_match =
                            has_agentbro(group.get("command").and_then(|c| c.as_str()));

                        !nested_match && !flat_match
                    });
                    arr.push(hook_group);
                } else {
                    // Convert single entry to array
                    let old = existing.clone();
                    *existing = serde_json::json!([old, hook_group]);
                }
            } else {
                hooks.insert(event_name.to_string(), serde_json::json!([hook_group]));
            }
        }

        // Write settings back
        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&settings)?;
        std::fs::write(&settings_path, content)?;

        log::info!("Claude Code hooks installed successfully");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let settings_path = self.settings_path();

        if !settings_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&settings_path)?;
        let mut settings: serde_json::Value = serde_json::from_str(&content)?;

        // Collect event names to remove (can't mutate while iterating)
        let mut events_to_remove = Vec::new();

        if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
            for event_name in Self::hook_events() {
                if let Some(existing) = hooks.get_mut(event_name) {
                    if let Some(arr) = existing.as_array_mut() {
                        arr.retain(|group| {
                            let has_agentbro = |cmd: Option<&str>| -> bool {
                                cmd.map(|c| c.contains("agentbro")).unwrap_or(false)
                            };

                            // Check nested format
                            let nested_match = group
                                .get("hooks")
                                .and_then(|h| h.as_array())
                                .map(|hooks_arr| {
                                    hooks_arr.iter().any(|h| {
                                        has_agentbro(h.get("command").and_then(|c| c.as_str()))
                                    })
                                })
                                .unwrap_or(false);

                            // Check flat format
                            let flat_match =
                                has_agentbro(group.get("command").and_then(|c| c.as_str()));

                            !nested_match && !flat_match
                        });
                        if arr.is_empty() {
                            events_to_remove.push(event_name.to_string());
                        }
                    }
                }
            }

            for event_name in events_to_remove {
                hooks.remove(&event_name);
            }
        }

        let content = serde_json::to_string_pretty(&settings)?;
        std::fs::write(&settings_path, content)?;

        // Remove the bridge binary
        let bridge_path = Self::bridge_binary_path();
        if bridge_path.exists() {
            let _ = std::fs::remove_file(&bridge_path);
        }

        // Also clean up old Python hook if present
        Self::cleanup_old_python_hook();

        log::info!("Claude Code hooks removed successfully");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }

    fn parse_event(
        &self,
        raw: &serde_json::Value,
    ) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        // Check if this is a claude-code event
        let agent = raw
            .get("agent")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-code");
        if agent != "claude-code" {
            return Err("Not a Claude Code event".into());
        }

        let session_id = raw
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing session_id")?
            .to_string();

        let event = raw.get("event").and_then(|v| v.as_str()).unwrap_or("");

        let status = raw.get("status").and_then(|v| v.as_str()).unwrap_or("");

        let cwd = raw
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let tool_name = raw
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let tool_input = raw
            .get("tool_input")
            .map(|v| v.to_string())
            .unwrap_or_default();

        // Parse tool_input as JSON for extracting tool target
        let tool_input_json: serde_json::Value =
            tool_input.parse().unwrap_or(serde_json::Value::Null);

        let tool_target = extract_tool_target(&tool_name, &tool_input_json);

        let tty = raw
            .get("tty")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Extract project name from cwd
        let project = cwd.rsplit('/').next().unwrap_or(&cwd).to_string();

        match event {
            "SessionStart" => Ok(AgentEvent::SessionStart {
                session_id,
                project,
                cwd,
                terminal: tty,
                agent_type: "claude-code".to_string(),
            }),
            "SessionEnd" => Ok(AgentEvent::SessionEnd { session_id }),
            "UserPromptSubmit" => Ok(AgentEvent::Processing {
                session_id,
                description: "Processing user input".to_string(),
            }),
            "PreToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: tool_target.clone(),
                status: "running".to_string(),
            }),
            "PostToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: tool_target.clone(),
                status: "success".to_string(),
            }),
            "PostToolUseFailure" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: tool_target.clone(),
                status: "error".to_string(),
            }),
            "PermissionDenied" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: tool_target.clone(),
                status: "error".to_string(),
            }),
            "PermissionRequest" => Ok(AgentEvent::PermissionRequest {
                session_id,
                tool_name,
                diff: None,
                options: None,
            }),
            "AskQuestion" => {
                let question = raw
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let options = raw
                    .get("options")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let descriptions = raw
                    .get("descriptions")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let header = raw
                    .get("header")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let multi_select = raw
                    .get("multi_select")
                    .or_else(|| raw.get("multiSelect"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let questions = raw
                    .get("questions")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|question| {
                                let text = question.get("question")?.as_str()?.to_string();
                                let options = question
                                    .get("options")
                                    .and_then(|v| v.as_array())
                                    .map(|opts| {
                                        opts.iter()
                                            .filter_map(|opt| {
                                                Some(QuestionOption {
                                                    label: opt
                                                        .get("label")
                                                        .and_then(|v| v.as_str())?
                                                        .to_string(),
                                                    description: opt
                                                        .get("description")
                                                        .and_then(|v| v.as_str())
                                                        .map(|s| s.to_string()),
                                                })
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                Some(QuestionItem {
                                    question: text,
                                    header: question
                                        .get("header")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                    options,
                                    multi_select: question
                                        .get("multiSelect")
                                        .and_then(|v| v.as_bool())
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
            "PlanApproval" => {
                let title = raw
                    .get("plan_title")
                    .or_else(|| raw.get("planTitle"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Plan")
                    .to_string();
                let content = raw
                    .get("plan_content")
                    .or_else(|| raw.get("planContent"))
                    .or_else(|| raw.get("plan"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let permissions = raw
                    .get("requested_permissions")
                    .or_else(|| raw.get("allowedPrompts"))
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .map(|item| {
                                if let Some(s) = item.as_str() {
                                    s.to_string()
                                } else if let (Some(tool), Some(prompt)) = (
                                    item.get("tool").and_then(|v| v.as_str()),
                                    item.get("prompt").and_then(|v| v.as_str()),
                                ) {
                                    format!("{tool}: {prompt}")
                                } else {
                                    item.to_string()
                                }
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
            "RateLimitsUpdate" | "StatusLineUpdate" => {
                let rate_limits = raw
                    .get("rateLimits")
                    .or_else(|| raw.get("rate_limits"))
                    .ok_or_else(|| "missing rate limits".to_string())?;
                let five_hour = rate_limits
                    .get("fiveHour")
                    .or_else(|| rate_limits.get("five_hour"))
                    .ok_or_else(|| "missing five-hour rate limit".to_string())?;
                let seven_day = rate_limits
                    .get("sevenDay")
                    .or_else(|| rate_limits.get("seven_day"))
                    .ok_or_else(|| "missing seven-day rate limit".to_string())?;
                let five_hour_usage = percentage_value(five_hour);
                let seven_day_usage = percentage_value(seven_day);
                let five_hour_remaining = remaining_label(five_hour);
                let seven_day_remaining = remaining_label(seven_day);
                let status_line_text = raw
                    .get("statusLineText")
                    .or_else(|| raw.get("status_line_text"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let context_window = raw
                    .get("contextWindow")
                    .or_else(|| raw.get("context_window"));
                let total_input_tokens = context_window.and_then(|v| {
                    v.get("totalInputTokens")
                        .or_else(|| v.get("total_input_tokens"))
                        .and_then(|n| n.as_u64())
                });
                let total_output_tokens = context_window.and_then(|v| {
                    v.get("totalOutputTokens")
                        .or_else(|| v.get("total_output_tokens"))
                        .and_then(|n| n.as_u64())
                });
                let context_window_size = context_window.and_then(|v| {
                    v.get("contextWindowSize")
                        .or_else(|| v.get("context_window_size"))
                        .and_then(|n| n.as_u64())
                });
                let context_used_percentage = context_window.and_then(|v| {
                    v.get("usedPercentage")
                        .or_else(|| v.get("used_percentage"))
                        .and_then(|n| n.as_f64())
                });
                let last_main_agent_at = raw
                    .get("lastMainAgentAt")
                    .or_else(|| raw.get("last_main_agent_at"))
                    .and_then(|v| v.as_i64());
                let cache_ttl_ms = raw
                    .get("cacheTTLMs")
                    .or_else(|| raw.get("cache_ttl_ms"))
                    .and_then(|v| v.as_i64());
                Ok(AgentEvent::RateLimitUpdate {
                    session_id,
                    five_hour_usage,
                    five_hour_remaining,
                    seven_day_usage,
                    seven_day_remaining,
                    status_line_text,
                    total_input_tokens,
                    total_output_tokens,
                    context_window_size,
                    context_used_percentage,
                    last_main_agent_at,
                    cache_ttl_ms,
                })
            }
            "SubagentStart" => {
                let agent_id = raw
                    .get("agent_id")
                    .and_then(|v| v.as_str())
                    .or_else(|| raw.get("agentId").and_then(|v| v.as_str()))
                    .or_else(|| raw.get("tool_use_id").and_then(|v| v.as_str()))
                    .or_else(|| raw.get("toolUseId").and_then(|v| v.as_str()))
                    .unwrap_or("unknown")
                    .to_string();
                let description = raw
                    .get("description")
                    .and_then(|v| v.as_str())
                    .or_else(|| raw.get("message").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();
                Ok(AgentEvent::SubagentStart {
                    session_id,
                    agent_id,
                    description,
                    agent_type: optional_string(raw, &["agent_type", "agentType", "type"]),
                    transcript_path: optional_string(raw, &["transcript_path", "transcriptPath"]),
                })
            }
            "SubagentStop" => {
                let agent_id = raw
                    .get("agent_id")
                    .and_then(|v| v.as_str())
                    .or_else(|| raw.get("agentId").and_then(|v| v.as_str()))
                    .or_else(|| raw.get("tool_use_id").and_then(|v| v.as_str()))
                    .or_else(|| raw.get("toolUseId").and_then(|v| v.as_str()))
                    .unwrap_or("unknown")
                    .to_string();
                let stop_status = raw
                    .get("agent_status")
                    .or_else(|| raw.get("agentStatus"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("completed")
                    .to_string();
                Ok(AgentEvent::SubagentStop {
                    session_id,
                    agent_id,
                    status: stop_status,
                    agent_type: optional_string(raw, &["agent_type", "agentType", "type"]),
                    transcript_path: optional_string(raw, &["transcript_path", "transcriptPath"]),
                    agent_transcript_path: optional_string(
                        raw,
                        &["agent_transcript_path", "agentTranscriptPath"],
                    ),
                    last_assistant_message: optional_string(
                        raw,
                        &["last_assistant_message", "lastAssistantMessage", "message"],
                    ),
                })
            }
            "Stop" => {
                let summary = raw
                    .get("summary")
                    .or_else(|| raw.get("message"))
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or("Task completed")
                    .to_string();
                Ok(AgentEvent::AssistantResponseComplete {
                    session_id,
                    text: summary,
                })
            }
            "StopFailure" => {
                let message = raw
                    .get("error")
                    .or_else(|| raw.get("message"))
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or("Task failed")
                    .to_string();
                Ok(AgentEvent::Error {
                    session_id,
                    message,
                })
            }
            "Notification" => Ok(AgentEvent::Notification {
                session_id,
                message: raw
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                status: if status.is_empty() {
                    None
                } else {
                    Some(status.to_string())
                },
            }),
            "PreCompact" | "PostCompact" => Ok(AgentEvent::Processing {
                session_id,
                description: "Compacting context".to_string(),
            }),
            "beforeShellExecution" => {
                let command = raw
                    .get("command")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        raw.get("tool_input")
                            .and_then(|v| v.get("command"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("")
                    .to_string();
                Ok(AgentEvent::ShellExecutionStart {
                    session_id,
                    command,
                    cwd: cwd.clone(),
                })
            }
            "afterShellExecution" => {
                let command = raw
                    .get("command")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        raw.get("tool_input")
                            .and_then(|v| v.get("command"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("")
                    .to_string();
                let exit_code = raw
                    .get("exit_code")
                    .and_then(|v| v.as_i64())
                    .or_else(|| {
                        raw.get("tool_result")
                            .and_then(|r| r.get("exit_code"))
                            .and_then(|v| v.as_i64())
                    })
                    .map(|v| v as i32);
                let stdout = raw
                    .get("stdout")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        raw.get("tool_result")
                            .and_then(|r| r.get("stdout"))
                            .and_then(|v| v.as_str())
                    })
                    .map(|s| s.to_string());
                let stderr = raw
                    .get("stderr")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        raw.get("tool_result")
                            .and_then(|r| r.get("stderr"))
                            .and_then(|v| v.as_str())
                    })
                    .map(|s| s.to_string());
                let duration_ms = raw.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                Ok(AgentEvent::ShellExecutionEnd {
                    session_id,
                    command,
                    exit_code,
                    stdout,
                    stderr,
                    duration_ms,
                })
            }
            "beforeMCPExecution" => {
                let server_name = raw
                    .get("mcp_server")
                    .or_else(|| raw.get("server_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mcp_tool_name = raw
                    .get("mcp_tool")
                    .or_else(|| raw.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments = raw
                    .get("mcp_arguments")
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                Ok(AgentEvent::MCPExecutionStart {
                    session_id,
                    server_name,
                    tool_name: mcp_tool_name,
                    arguments,
                })
            }
            "afterMCPExecution" => {
                let server_name = raw
                    .get("mcp_server")
                    .or_else(|| raw.get("server_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mcp_tool_name = raw
                    .get("mcp_tool")
                    .or_else(|| raw.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let result = raw.get("mcp_result").map(|v| v.to_string());
                let error = raw
                    .get("mcp_error")
                    .or_else(|| raw.get("error"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let duration_ms = raw.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                Ok(AgentEvent::MCPExecutionEnd {
                    session_id,
                    server_name,
                    tool_name: mcp_tool_name,
                    result,
                    error,
                    duration_ms,
                })
            }
            "afterAgentResponse" => {
                let content = raw
                    .get("response_content")
                    .or_else(|| raw.get("text"))
                    .or_else(|| raw.get("content"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let ct = raw
                    .get("content_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("text")
                    .to_string();
                Ok(AgentEvent::AgentResponse {
                    session_id,
                    content,
                    content_type: ct,
                })
            }
            "afterAgentThought" => {
                let thought = raw
                    .get("thought_content")
                    .or_else(|| raw.get("thought"))
                    .or_else(|| raw.get("reasoning"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Ok(AgentEvent::AgentThought {
                    session_id,
                    thought,
                })
            }
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

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn percentage_value(value: &serde_json::Value) -> f64 {
    value
        .get("usedPercentage")
        .or_else(|| value.get("used_percentage"))
        .or_else(|| value.get("usage"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

fn remaining_label(value: &serde_json::Value) -> String {
    if let Some(label) = value
        .get("remaining")
        .or_else(|| value.get("remainingLabel"))
        .or_else(|| value.get("remaining_label"))
        .and_then(|v| v.as_str())
    {
        return label.to_string();
    }

    let Some(resets_at) = value
        .get("resetsAt")
        .or_else(|| value.get("resets_at"))
        .and_then(|v| v.as_f64())
    else {
        return "--".to_string();
    };

    let now_ms = chrono::Utc::now().timestamp_millis() as f64;
    let reset_ms = if resets_at < 10_000_000_000.0 {
        resets_at * 1000.0
    } else {
        resets_at
    };
    let remaining_secs = ((reset_ms - now_ms) / 1000.0).max(0.0) as i64;
    let hours = remaining_secs / 3600;
    let minutes = (remaining_secs % 3600) / 60;
    if hours >= 24 {
        let days = hours / 24;
        let rem_hours = hours % 24;
        format!("{}d{}h", days, rem_hours)
    } else if hours > 0 {
        format!("{}h{}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

/// Result of verifying hook installation integrity
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookVerificationResult {
    /// Everything is correctly installed
    Ok,
    /// Bridge binary is missing or outdated and needs reinstall
    NeedsReinstall,
    /// settings.json is missing or corrupted / missing our hook entries
    SettingsCorrupted,
}

/// Read the first non-empty string value from a list of possible hook payload keys.
fn optional_string(raw: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        raw.get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
    })
}

/// Extract the target (file path or command) from tool input for display purposes.
fn extract_tool_target(tool_name: &str, tool_input: &serde_json::Value) -> Option<String> {
    match tool_name {
        "Read" | "Edit" | "Write" | "Glob" | "Grep" | "GlobSearch" => tool_input
            .get("file_path")
            .or_else(|| tool_input.get("path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "Bash" | "shell" => tool_input.get("command").and_then(|v| v.as_str()).map(|s| {
            let cmd = s.trim();
            if cmd.len() > 50 {
                format!("{}...", &cmd[..47])
            } else {
                cmd.to_string()
            }
        }),
        "TaskCreate" | "TaskUpdate" | "TaskList" => tool_input
            .get("subject")
            .or_else(|| tool_input.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "WebSearch" | "mcp__web_browser__browse" => {
            tool_input.get("query").and_then(|v| v.as_str()).map(|s| {
                let q = s.trim();
                if q.len() > 50 {
                    format!("{}...", &q[..47])
                } else {
                    q.to_string()
                }
            })
        }
        _ => None,
    }
}

impl ClaudeCodeAdapter {
    /// Verify that hooks are correctly installed and up to date.
    /// Checks:
    ///  1. The bridge binary exists on disk
    ///  2. settings.json contains our hook entries pointing to the bridge binary
    pub fn verify_hooks(&self) -> HookVerificationResult {
        // 1. Check bridge binary exists
        let bridge_path = Self::bridge_binary_path();
        if !bridge_path.exists() {
            return HookVerificationResult::NeedsReinstall;
        }

        // 2. Check settings.json has our hook entries
        let settings_path = self.settings_path();
        let settings_content = match std::fs::read_to_string(&settings_path) {
            Ok(c) => c,
            Err(_) => return HookVerificationResult::SettingsCorrupted,
        };
        let settings: serde_json::Value = match serde_json::from_str(&settings_content) {
            Ok(v) => v,
            Err(_) => return HookVerificationResult::SettingsCorrupted,
        };

        let hooks = match settings.get("hooks").and_then(|h| h.as_object()) {
            Some(h) => h,
            None => return HookVerificationResult::SettingsCorrupted,
        };

        // Spot-check a few critical events
        for event in &["PermissionRequest", "PreToolUse", "SessionStart"] {
            let has_our_hook = hooks
                .get(*event)
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter().any(|group| {
                        group
                            .get("hooks")
                            .and_then(|h| h.as_array())
                            .map(|hooks_arr| {
                                hooks_arr.iter().any(|h| {
                                    h.get("command")
                                        .and_then(|c| c.as_str())
                                        .map(|c| c.contains("agentbro-bridge"))
                                        .unwrap_or(false)
                                })
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);

            if !has_our_hook {
                return HookVerificationResult::SettingsCorrupted;
            }
        }

        HookVerificationResult::Ok
    }

    /// Check if the deployed bridge binary differs from the bundled version.
    /// If so, overwrite with the new version (app was updated).
    /// Returns true if the binary was updated.
    pub fn update_hook_script_if_needed() -> bool {
        let dest = Self::bridge_binary_path();
        let source = match Self::find_source_bridge() {
            Some(s) => s,
            None => return false,
        };

        // Compare file sizes as a quick check for differences
        let dest_meta = match std::fs::metadata(&dest) {
            Ok(m) => m,
            Err(_) => return false, // If dest doesn't exist, install_hooks will handle it
        };
        let source_meta = match std::fs::metadata(&source) {
            Ok(m) => m,
            Err(_) => return false,
        };

        if dest_meta.len() != source_meta.len() {
            match Self::install_bridge() {
                Ok(_) => {
                    log::info!("Bridge binary updated to latest version");
                    return true;
                }
                Err(e) => {
                    log::warn!("Failed to update bridge binary: {}", e);
                }
            }
        }
        false
    }
}

/// Expand `~` prefix to the user's home directory
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        home.join(rest)
    } else if path == "~" {
        dirs::home_dir().unwrap_or_else(std::env::temp_dir)
    } else {
        PathBuf::from(path)
    }
}

/// Send a message to a Claude Code session via tmux or direct TTY write
pub fn send_message_to_terminal(
    tty: &str,
    message: &str,
    terminal: &str,
    pid: Option<u32>,
    term_bundle_id: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Try tmux first
    if let Ok(output) = std::process::Command::new("tmux")
        .args(["list-panes", "-a", "-F", "#{pane_tty} #{pane_id}"])
        .output()
    {
        let output_str = String::from_utf8_lossy(&output.stdout);
        if let Some(pane_id) = output_str
            .lines()
            .find(|line| line.starts_with(tty))
            .and_then(|line| line.split_whitespace().nth(1))
        {
            send_tmux_text_with_enter(pane_id, message)?;
            return Ok(());
        }
    }

    let terminal_app = resolve_terminal_app_label(terminal, term_bundle_id, pid);
    if terminal_app.eq_ignore_ascii_case("iterm2") || terminal_app.eq_ignore_ascii_case("iterm") {
        run_osascript(&build_iterm_write_script(tty, message))?;
        return Ok(());
    }

    if terminal_app.eq_ignore_ascii_case("terminal") {
        run_osascript(&build_terminal_write_script(tty, message))?;
        return Ok(());
    }

    if let Some(pid) = pid {
        let jump_context = crate::terminal::jump::JumpContext {
            pid,
            iterm_session_id: None,
            kitty_window_id: None,
            wezterm_pane: None,
            zellij_pane_id: None,
            zellij_session_name: None,
            cmux_surface_id: None,
            cmux_workspace_id: None,
            tmux_pane: None,
            tmux_env: None,
            cwd: None,
            tty_path: Some(tty.to_string()),
            terminal_app: Some(terminal_app.clone()),
            term_bundle_id: term_bundle_id.map(ToString::to_string),
            agent_type: Some("claude-code".to_string()),
        };
        let _ = crate::terminal::jump::jump_to_terminal_with_context(&jump_context);
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    run_osascript(&build_system_events_type_script(message))
}

fn send_tmux_text_with_enter(
    pane_id: &str,
    message: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let text_status = std::process::Command::new("tmux")
        .args(["send-keys", "-l", "-t", pane_id, message])
        .status()?;
    if !text_status.success() {
        return Err(format!("Failed to send message to tmux pane {}", pane_id).into());
    }

    let enter_status = std::process::Command::new("tmux")
        .args(["send-keys", "-t", pane_id, "Enter"])
        .status()?;
    if !enter_status.success() {
        return Err(format!("Failed to send Enter to tmux pane {}", pane_id).into());
    }

    Ok(())
}

fn resolve_terminal_app_label(
    terminal: &str,
    term_bundle_id: Option<&str>,
    pid: Option<u32>,
) -> String {
    let bundle = term_bundle_id.unwrap_or("").to_ascii_lowercase();
    if bundle.contains("iterm") {
        return "iTerm2".to_string();
    }
    if bundle.contains("terminal") {
        return "Terminal".to_string();
    }
    if bundle.contains("ghostty") {
        return "Ghostty".to_string();
    }
    if bundle.contains("kitty") {
        return "kitty".to_string();
    }

    let lower = terminal.to_ascii_lowercase();
    if lower.contains("iterm") {
        return "iTerm2".to_string();
    }
    if lower.contains("terminal") && !lower.contains("/dev/") {
        return "Terminal".to_string();
    }
    if lower.contains("ghostty") {
        return "Ghostty".to_string();
    }
    if lower.contains("kitty") {
        return "kitty".to_string();
    }

    if let Some(pid) = pid {
        let tree = crate::terminal::process_tree::build_tree();
        if let Some(app) = crate::terminal::process_tree::find_terminal_app_name(pid, &tree) {
            return app;
        }
    }

    "Terminal".to_string()
}

fn build_iterm_write_script(tty: &str, message: &str) -> String {
    let tty_literal = apple_script_string_literal(tty);
    let message_literal = apple_script_string_literal(message);
    format!(
        r#"tell application "iTerm2"
  repeat with w in windows
    repeat with t in tabs of w
      repeat with s in sessions of t
        if tty of s contains {tty_literal} then
          tell s to write text {message_literal}
          return
        end if
      end repeat
    end repeat
  end repeat
  tell current session of current tab of current window to write text {message_literal}
end tell"#
    )
}

fn build_terminal_write_script(tty: &str, message: &str) -> String {
    let tty_literal = apple_script_string_literal(tty);
    let message_literal = apple_script_string_literal(message);
    format!(
        r#"tell application "Terminal"
  repeat with w in windows
    repeat with t in tabs of w
      if tty of t is {tty_literal} then
        do script {message_literal} in t
        return
      end if
    end repeat
  end repeat
  do script {message_literal}
end tell"#
    )
}

fn build_system_events_type_script(message: &str) -> String {
    let message_literal = apple_script_string_literal(message);
    format!(
        r#"tell application "System Events"
  keystroke {message_literal}
  key code 36
end tell"#
    )
}

fn run_osascript(script: &str) -> Result<(), Box<dyn std::error::Error>> {
    let output = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_string()
            .into())
    }
}

fn apple_script_string_literal(value: &str) -> String {
    let parts: Vec<String> = value
        .split('\n')
        .map(|part| format!("\"{}\"", part.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    parts.join(" & linefeed & ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_stop_as_assistant_response_complete() {
        let adapter = ClaudeCodeAdapter::new();
        let event = adapter
            .parse_event(&json!({
                "event": "Stop",
                "session_id": "session-1",
                "cwd": "/tmp/project",
                "summary": "All checks passed"
            }))
            .expect("Stop event should parse");

        match event {
            AgentEvent::AssistantResponseComplete { session_id, text } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(text, "All checks passed");
            }
            other => panic!("expected AssistantResponseComplete, got {other:?}"),
        }
    }

    #[test]
    fn parses_notification_with_message_and_status() {
        let adapter = ClaudeCodeAdapter::new();
        let event = adapter
            .parse_event(&json!({
                "event": "Notification",
                "session_id": "session-1",
                "cwd": "/tmp/project",
                "message": "Task done",
                "status": "waiting_for_input"
            }))
            .expect("Notification event should parse");

        match event {
            AgentEvent::Notification {
                session_id,
                message,
                status,
            } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(message, "Task done");
                assert_eq!(status.as_deref(), Some("waiting_for_input"));
            }
            other => panic!("expected Notification, got {other:?}"),
        }
    }

    #[test]
    fn terminal_message_scripts_target_tty_and_submit() {
        let iterm_script = build_iterm_write_script("/dev/ttys001", "ok");
        assert!(iterm_script.contains("tty of s contains \"/dev/ttys001\""));
        assert!(iterm_script.contains("write text \"ok\""));

        let terminal_script = build_terminal_write_script("/dev/ttys001", "ok");
        assert!(terminal_script.contains("tty of t is \"/dev/ttys001\""));
        assert!(terminal_script.contains("do script \"ok\" in t"));
    }

    #[test]
    fn apple_script_string_literal_escapes_quotes_and_newlines() {
        assert_eq!(
            apple_script_string_literal("say \"hi\"\nnext"),
            "\"say \\\"hi\\\"\" & linefeed & \"next\""
        );
    }

    #[test]
    fn parses_subagent_start_with_rich_metadata() {
        let adapter = ClaudeCodeAdapter::new();
        let event = adapter
            .parse_event(&json!({
                "event": "SubagentStart",
                "session_id": "session-1",
                "cwd": "/tmp/project",
                "agentId": "agent-1",
                "description": "Research checkout flow",
                "agentType": "research",
                "transcriptPath": "/tmp/main.jsonl"
            }))
            .expect("SubagentStart event should parse");

        match event {
            AgentEvent::SubagentStart {
                session_id,
                agent_id,
                description,
                agent_type,
                transcript_path,
            } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(agent_id, "agent-1");
                assert_eq!(description, "Research checkout flow");
                assert_eq!(agent_type.as_deref(), Some("research"));
                assert_eq!(transcript_path.as_deref(), Some("/tmp/main.jsonl"));
            }
            other => panic!("expected SubagentStart, got {other:?}"),
        }
    }

    #[test]
    fn parses_subagent_stop_with_rich_metadata() {
        let adapter = ClaudeCodeAdapter::new();
        let event = adapter
            .parse_event(&json!({
                "event": "SubagentStop",
                "session_id": "session-1",
                "cwd": "/tmp/project",
                "agent_id": "agent-1",
                "agent_status": "completed",
                "agent_type": "research",
                "transcript_path": "/tmp/main.jsonl",
                "agent_transcript_path": "/tmp/agent.jsonl",
                "last_assistant_message": "Finished checkout flow audit"
            }))
            .expect("SubagentStop event should parse");

        match event {
            AgentEvent::SubagentStop {
                session_id,
                agent_id,
                status,
                agent_type,
                transcript_path,
                agent_transcript_path,
                last_assistant_message,
            } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(agent_id, "agent-1");
                assert_eq!(status, "completed");
                assert_eq!(agent_type.as_deref(), Some("research"));
                assert_eq!(transcript_path.as_deref(), Some("/tmp/main.jsonl"));
                assert_eq!(agent_transcript_path.as_deref(), Some("/tmp/agent.jsonl"));
                assert_eq!(
                    last_assistant_message.as_deref(),
                    Some("Finished checkout flow audit")
                );
            }
            other => panic!("expected SubagentStop, got {other:?}"),
        }
    }

    #[test]
    fn parses_stop_failure_as_error() {
        let adapter = ClaudeCodeAdapter::new();
        let event = adapter
            .parse_event(&json!({
                "event": "StopFailure",
                "session_id": "session-1",
                "cwd": "/tmp/project",
                "error": "API request failed"
            }))
            .expect("StopFailure event should parse");

        match event {
            AgentEvent::Error {
                session_id,
                message,
            } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(message, "API request failed");
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn parses_status_line_rate_limits() {
        let adapter = ClaudeCodeAdapter::new();
        let event = adapter
            .parse_event(&json!({
                "event": "StatusLineUpdate",
                "session_id": "session-1",
                "cwd": "/tmp/project",
                "statusLineText": "5h 42%",
                "rateLimits": {
                    "fiveHour": { "usedPercentage": 42, "remaining": "1h20m" },
                    "sevenDay": { "usedPercentage": 7, "remaining": "6d3h" }
                },
                "contextWindow": {
                    "totalInputTokens": 12000,
                    "totalOutputTokens": 3000,
                    "contextWindowSize": 200000,
                    "usedPercentage": 7.5
                },
                "lastMainAgentAt": 1778640000000_i64,
                "cacheTTLMs": 300000_i64
            }))
            .expect("StatusLineUpdate event should parse");

        match event {
            AgentEvent::RateLimitUpdate {
                session_id,
                five_hour_usage,
                five_hour_remaining,
                seven_day_usage,
                seven_day_remaining,
                status_line_text,
                total_input_tokens,
                total_output_tokens,
                context_window_size,
                context_used_percentage,
                last_main_agent_at,
                cache_ttl_ms,
            } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(five_hour_usage, 42.0);
                assert_eq!(five_hour_remaining, "1h20m");
                assert_eq!(seven_day_usage, 7.0);
                assert_eq!(seven_day_remaining, "6d3h");
                assert_eq!(status_line_text.as_deref(), Some("5h 42%"));
                assert_eq!(total_input_tokens, Some(12000));
                assert_eq!(total_output_tokens, Some(3000));
                assert_eq!(context_window_size, Some(200000));
                assert_eq!(context_used_percentage, Some(7.5));
                assert_eq!(last_main_agent_at, Some(1778640000000));
                assert_eq!(cache_ttl_ms, Some(300000));
            }
            other => panic!("expected RateLimitUpdate, got {other:?}"),
        }
    }

    #[test]
    fn parses_plan_approval() {
        let adapter = ClaudeCodeAdapter::new();
        let event = adapter
            .parse_event(&json!({
                "event": "PlanApproval",
                "session_id": "session-1",
                "cwd": "/tmp/project",
                "plan_title": "Implement checkout",
                "plan_content": "1. Edit API\n2. Run tests",
                "requested_permissions": [
                    { "tool": "Edit", "prompt": "Update checkout.ts" },
                    "Bash: npm test"
                ]
            }))
            .expect("PlanApproval event should parse");

        match event {
            AgentEvent::PlanApproval {
                session_id,
                title,
                content,
                permissions,
            } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(title, "Implement checkout");
                assert_eq!(content, "1. Edit API\n2. Run tests");
                assert_eq!(
                    permissions,
                    vec![
                        "Edit: Update checkout.ts".to_string(),
                        "Bash: npm test".to_string()
                    ]
                );
            }
            other => panic!("expected PlanApproval, got {other:?}"),
        }
    }

    #[test]
    fn parses_ask_question_with_questions() {
        let adapter = ClaudeCodeAdapter::new();
        let event = adapter
            .parse_event(&json!({
                "event": "AskQuestion",
                "session_id": "session-1",
                "cwd": "/tmp/project",
                "question": "[Deploy] Choose options",
                "options": ["Preview", "Ship"],
                "descriptions": ["Open staging", "Release now"],
                "header": "Deploy",
                "multi_select": true,
                "questions": [
                    {
                        "header": "Deploy",
                        "question": "Which target?",
                        "options": [
                            { "label": "Preview", "description": "Open staging" },
                            { "label": "Ship", "description": "Release now" }
                        ],
                        "multiSelect": true
                    },
                    {
                        "question": "Notify channel?",
                        "options": [
                            { "label": "Yes" },
                            { "label": "No" }
                        ]
                    }
                ]
            }))
            .expect("AskQuestion event should parse");

        match event {
            AgentEvent::AskQuestion {
                session_id,
                question,
                options,
                descriptions,
                header,
                multi_select,
                questions,
            } => {
                assert_eq!(session_id, "session-1");
                assert_eq!(question, "[Deploy] Choose options");
                assert_eq!(options, vec!["Preview".to_string(), "Ship".to_string()]);
                assert_eq!(
                    descriptions,
                    vec!["Open staging".to_string(), "Release now".to_string()]
                );
                assert_eq!(header.as_deref(), Some("Deploy"));
                assert!(multi_select);
                assert_eq!(questions.len(), 2);
                assert_eq!(questions[0].header.as_deref(), Some("Deploy"));
                assert_eq!(questions[0].question, "Which target?");
                assert!(questions[0].multi_select);
                assert_eq!(questions[0].options[0].label, "Preview");
                assert_eq!(
                    questions[0].options[0].description.as_deref(),
                    Some("Open staging")
                );
                assert_eq!(questions[1].question, "Notify channel?");
                assert!(!questions[1].multi_select);
            }
            other => panic!("expected AskQuestion, got {other:?}"),
        }
    }
}
