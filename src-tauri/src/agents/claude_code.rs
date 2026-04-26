// ClaudeCodeAdapter — Agent adapter for Claude Code CLI
// Handles hook installation, event parsing, and communication via tmux

use std::path::PathBuf;
use super::{AdapterStatus, AgentAdapter, AgentEvent};

/// Bridge binary name
const BRIDGE_BINARY_NAME: &str = "agent-island-bridge";

/// Claude Code adapter implementation
pub struct ClaudeCodeAdapter {
    status: AdapterStatus,
}

impl ClaudeCodeAdapter {
    pub fn new() -> Self {
        // Check if Claude Code is available
        let status = if Self::is_claude_code_installed() {
            AdapterStatus::Available
        } else {
            AdapterStatus::Unavailable
        };

        Self { status }
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
    /// Uses ~/.agent-island/bin/ to avoid spaces in path
    fn bridge_binary_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| std::env::temp_dir());
        home.join(".agent-island").join("bin").join(BRIDGE_BINARY_NAME)
    }

    /// Get the Claude Code settings.json path
    fn claude_settings_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| std::env::temp_dir());
        home.join(".claude").join("settings.json")
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

    /// Install the bridge binary to ~/.agent-island/bin/
    /// Copies the compiled binary from the app bundle directory.
    fn install_bridge() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let dest = Self::bridge_binary_path();
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let source = Self::find_source_bridge()
            .ok_or("Bridge binary not found next to main executable")?;

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

    /// Build the hook command string — just the path to the compiled binary
    fn hook_command() -> Result<String, Box<dyn std::error::Error>> {
        let bridge_path = Self::install_bridge()?;
        Ok(bridge_path.display().to_string())
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
        ]
    }

    /// Hook events that need a timeout (permission-related)
    fn events_with_timeout() -> Vec<&'static str> {
        vec![
            "PermissionRequest",
        ]
    }

    /// Remove old Python hook artifacts (migration from Python to Rust bridge)
    fn cleanup_old_python_hook() {
        let home = dirs::home_dir().unwrap_or_else(|| std::env::temp_dir());
        let old_script = home.join(".agent-island").join("agent-island-hook.py");
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
        "Claude Code"
    }

    fn icon(&self) -> &str {
        "claude"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Clean up old Python hook if present
        Self::cleanup_old_python_hook();

        let settings_path = Self::claude_settings_path();
        let hook_command = Self::hook_command()?;

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

        let hooks = settings.get_mut("hooks")
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
                // If it's an array, check if we already have an agent-island entry
                if let Some(arr) = existing.as_array_mut() {
                    // Remove old Python hook entries and old bridge entries
                    arr.retain(|group| {
                        let has_agent_island = |cmd: Option<&str>| -> bool {
                            cmd.map(|c| c.contains("agent-island")).unwrap_or(false)
                        };

                        // Check nested format
                        let nested_match = group.get("hooks")
                            .and_then(|h| h.as_array())
                            .map(|hooks_arr| hooks_arr.iter().any(|h| {
                                has_agent_island(h.get("command").and_then(|c| c.as_str()))
                            }))
                            .unwrap_or(false);

                        // Check flat format
                        let flat_match = has_agent_island(
                            group.get("command").and_then(|c| c.as_str())
                        );

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
        let settings_path = Self::claude_settings_path();

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
                            let has_agent_island = |cmd: Option<&str>| -> bool {
                                cmd.map(|c| c.contains("agent-island")).unwrap_or(false)
                            };

                            // Check nested format
                            let nested_match = group.get("hooks")
                                .and_then(|h| h.as_array())
                                .map(|hooks_arr| hooks_arr.iter().any(|h| {
                                    has_agent_island(h.get("command").and_then(|c| c.as_str()))
                                }))
                                .unwrap_or(false);

                            // Check flat format
                            let flat_match = has_agent_island(
                                group.get("command").and_then(|c| c.as_str())
                            );

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

    fn parse_event(&self, raw: &serde_json::Value) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        // Check if this is a claude-code event
        let agent = raw.get("agent").and_then(|v| v.as_str()).unwrap_or("claude-code");
        if agent != "claude-code" {
            return Err("Not a Claude Code event".into());
        }

        let session_id = raw.get("session_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing session_id")?
            .to_string();

        let event = raw.get("event")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let status = raw.get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let cwd = raw.get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let tool_name = raw.get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let tool_input = raw.get("tool_input")
            .map(|v| v.to_string())
            .unwrap_or_default();

        // Parse tool_input as JSON for extracting tool target
        let tool_input_json: serde_json::Value = tool_input
            .parse()
            .unwrap_or(serde_json::Value::Null);

        let tool_target = extract_tool_target(&tool_name, &tool_input_json);

        let tty = raw.get("tty")
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
                status: "denied".to_string(),
            }),
            "PermissionRequest" => Ok(AgentEvent::PermissionRequest {
                session_id,
                tool_name,
                diff: None,
                options: None,
            }),
            "SubagentStart" => {
                let agent_id = raw.get("agent_id")
                    .and_then(|v| v.as_str())
                    .or_else(|| raw.get("tool_use_id").and_then(|v| v.as_str()))
                    .unwrap_or("unknown")
                    .to_string();
                let description = raw.get("description")
                    .and_then(|v| v.as_str())
                    .or_else(|| raw.get("message").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();
                Ok(AgentEvent::SubagentStart {
                    session_id,
                    agent_id,
                    description,
                })
            }
            "SubagentStop" => {
                let agent_id = raw.get("agent_id")
                    .and_then(|v| v.as_str())
                    .or_else(|| raw.get("tool_use_id").and_then(|v| v.as_str()))
                    .unwrap_or("unknown")
                    .to_string();
                let stop_status = raw.get("agent_status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("completed")
                    .to_string();
                Ok(AgentEvent::SubagentStop {
                    session_id,
                    agent_id,
                    status: stop_status,
                })
            }
            "Stop" | "StopFailure" | "Notification" if status == "waiting_for_input" => {
                Ok(AgentEvent::Processing {
                    session_id,
                    description: "Waiting for input".to_string(),
                })
            }
            "PreCompact" | "PostCompact" => Ok(AgentEvent::Processing {
                session_id,
                description: "Compacting context".to_string(),
            }),
            _ => Ok(AgentEvent::Processing {
                session_id,
                description: format!("Event: {}", event),
            }),
        }
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![Self::claude_settings_path()]
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

/// Extract the target (file path or command) from tool input for display purposes.
fn extract_tool_target(tool_name: &str, tool_input: &serde_json::Value) -> Option<String> {
    match tool_name {
        "Read" | "Edit" | "Write" | "Glob" | "Grep" | "GlobSearch" => {
            tool_input
                .get("file_path")
                .or_else(|| tool_input.get("path"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        }
        "Bash" | "shell" => {
            tool_input
                .get("command")
                .and_then(|v| v.as_str())
                .map(|s| {
                    let cmd = s.trim();
                    if cmd.len() > 50 {
                        format!("{}...", &cmd[..47])
                    } else {
                        cmd.to_string()
                    }
                })
        }
        "TaskCreate" | "TaskUpdate" | "TaskList" => {
            tool_input
                .get("subject")
                .or_else(|| tool_input.get("name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        }
        "WebSearch" | "mcp__web_browser__browse" => {
            tool_input
                .get("query")
                .and_then(|v| v.as_str())
                .map(|s| {
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
        let settings_path = Self::claude_settings_path();
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
            let has_our_hook = hooks.get(*event)
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().any(|group| {
                    group.get("hooks")
                        .and_then(|h| h.as_array())
                        .map(|hooks_arr| hooks_arr.iter().any(|h| {
                            h.get("command")
                                .and_then(|c| c.as_str())
                                .map(|c| c.contains("agent-island-bridge"))
                                .unwrap_or(false)
                        }))
                        .unwrap_or(false)
                }))
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

/// Send a message to a Claude Code session via tmux send-keys
pub fn send_message_to_terminal(tty: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Try to find the tmux pane for this tty
    let output = std::process::Command::new("tmux")
        .args(["list-panes", "-a", "-F", "#{pane_tty} #{pane_id}"])
        .output()?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    let pane_id = output_str
        .lines()
        .find(|line| line.starts_with(tty))
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| format!("No tmux pane found for tty {}", tty))?;

    // Send keys to the tmux pane using -l (literal) mode to prevent injection
    std::process::Command::new("tmux")
        .args(["send-keys", "-l", "-t", pane_id, &format!("{}\n", message)])
        .output()?;

    Ok(())
}
