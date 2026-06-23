// ClineAdapter — VS Code extension hooks from ~/Documents/Cline/Hooks/<EventName>

use super::{profiles, AdapterStatus, AgentAdapter, AgentEvent};
use std::path::{Path, PathBuf};

pub struct ClineAdapter {
    hooks_dir: PathBuf,
    status: AdapterStatus,
}

impl ClineAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let hooks_dir = home.join("Documents").join("Cline").join("Hooks");
        let status = if Self::hooks_supported_on_platform() && Self::is_installed(&hooks_dir) {
            AdapterStatus::Installed
        } else {
            AdapterStatus::Unavailable
        };
        Self { hooks_dir, status }
    }

    fn hooks_supported_on_platform() -> bool {
        !cfg!(target_os = "windows")
    }

    fn is_installed(hooks_dir: &Path) -> bool {
        if !Self::hooks_supported_on_platform() {
            return false;
        }
        hooks_dir.exists()
            || dirs::home_dir()
                .map(|home| {
                    home.join("Documents").join("Cline").exists()
                        || has_cline_extension(&home.join(".vscode").join("extensions"))
                        || has_cline_extension(&home.join(".cursor").join("extensions"))
                })
                .unwrap_or(false)
    }
}

impl AgentAdapter for ClineAdapter {
    fn name(&self) -> &str {
        "cline"
    }

    fn display_name(&self) -> &str {
        "Cline"
    }

    fn icon(&self) -> &str {
        "cline"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !Self::hooks_supported_on_platform() {
            return Err(
                "Cline Hooks are not available on Windows yet; Cline currently supports hooks on macOS/Linux only."
                    .into(),
            );
        }
        profiles::install_at(&profiles::cline_profile(), &self.hooks_dir)
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !Self::hooks_supported_on_platform() {
            return Ok(());
        }
        profiles::uninstall_at(&profiles::cline_profile(), &self.hooks_dir)
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }

    fn detect_status_now(&self) -> AdapterStatus {
        if Self::is_installed(&self.hooks_dir) {
            AdapterStatus::Installed
        } else {
            AdapterStatus::Unavailable
        }
    }

    fn parse_event(
        &self,
        raw: &serde_json::Value,
    ) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        let session_id = string_field(raw, &["session_id", "sessionId", "task_id", "taskId", "id"])
            .unwrap_or("unknown")
            .to_string();
        let event = string_field(raw, &["event", "hook_event_name", "hookEventName"]).unwrap_or("");
        let event_payload = cline_event_payload(raw, event);
        let cwd = string_field(raw, &["cwd"])
            .or_else(|| first_string_array_field(raw, "workspaceRoots"))
            .unwrap_or("")
            .to_string();
        let project = super::project_name_from_path(&cwd);
        let tool_name =
            string_field_with_payload(raw, event_payload, &["tool", "tool_name", "toolName"])
                .unwrap_or("Tool")
                .to_string();
        let tool_input = raw
            .get("tool_input")
            .or_else(|| raw.get("toolInput"))
            .or_else(|| event_payload.and_then(|payload| payload.get("parameters")))
            .or_else(|| event_payload.and_then(|payload| payload.get("toolArgs")))
            .or_else(|| event_payload.and_then(|payload| payload.get("input")))
            .map(|value| value.to_string())
            .unwrap_or_default();

        match event {
            "TaskStart" | "TaskResume" => Ok(AgentEvent::SessionStart {
                session_id,
                project,
                cwd,
                terminal: string_field(raw, &["tty"]).unwrap_or("").to_string(),
                agent_type: "cline".to_string(),
            }),
            "UserPromptSubmit" => Ok(AgentEvent::Processing {
                session_id,
                description: string_field_with_payload(
                    raw,
                    event_payload,
                    &["prompt", "user_prompt", "userPrompt", "message"],
                )
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!("Prompt: {}", value.chars().take(80).collect::<String>()))
                .unwrap_or_else(|| "User prompt submitted".to_string()),
            }),
            "PreToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "running".to_string(),
            }),
            "PostToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: if event_payload
                    .and_then(|payload| payload.get("success"))
                    .and_then(|value| value.as_bool())
                    .is_some_and(|success| !success)
                {
                    "error"
                } else {
                    "success"
                }
                .to_string(),
            }),
            "TaskCancel" => Ok(AgentEvent::Interrupt { session_id }),
            "TaskComplete" => Ok(AgentEvent::AssistantResponseComplete {
                session_id,
                text: string_field_with_payload(
                    raw,
                    event_payload,
                    &["summary", "message", "last_assistant_message", "task"],
                )
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("Task completed")
                .to_string(),
            }),
            "PreCompact" => Ok(AgentEvent::Processing {
                session_id,
                description: "Compacting context".to_string(),
            }),
            _ => Ok(AgentEvent::Processing {
                session_id,
                description: format!("Event: {event}"),
            }),
        }
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![self.hooks_dir.clone()]
    }

    fn hooks_installed(&self) -> bool {
        if !Self::hooks_supported_on_platform() {
            return false;
        }
        profiles::install_health(&profiles::cline_profile(), &self.hooks_dir).is_present()
    }
}

fn string_field<'a>(raw: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| raw.get(key).and_then(|value| value.as_str()))
}

fn string_field_with_payload<'a>(
    raw: &'a serde_json::Value,
    payload: Option<&'a serde_json::Value>,
    keys: &[&str],
) -> Option<&'a str> {
    string_field(raw, keys).or_else(|| payload.and_then(|value| string_field(value, keys)))
}

fn first_string_array_field<'a>(raw: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    raw.get(key)
        .and_then(|value| value.as_array())
        .and_then(|values| values.iter().find_map(|value| value.as_str()))
}

fn cline_event_payload<'a>(
    raw: &'a serde_json::Value,
    event: &str,
) -> Option<&'a serde_json::Value> {
    let key = match event {
        "UserPromptSubmit" => "userPromptSubmit",
        "PreToolUse" => "preToolUse",
        "PostToolUse" => "postToolUse",
        "TaskStart" => "taskStart",
        "TaskResume" => "taskResume",
        "TaskCancel" => "taskCancel",
        "TaskComplete" => "taskComplete",
        "PreCompact" => "preCompact",
        _ => return None,
    };
    raw.get(key)
}

fn has_cline_extension(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        name.starts_with("saoudrizwan.claude-dev") || name.starts_with("cline.cline")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> ClineAdapter {
        ClineAdapter {
            hooks_dir: PathBuf::new(),
            status: AdapterStatus::Unavailable,
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_reports_hooks_unavailable() {
        let adapter = adapter();
        assert_eq!(adapter.detect_status_now(), AdapterStatus::Unavailable);
        assert!(!adapter.hooks_installed());
        assert!(adapter.install_hooks().is_err());
    }

    #[test]
    fn parses_nested_prompt_and_workspace_root() {
        let event = adapter()
            .parse_event(&serde_json::json!({
                "hookName": "UserPromptSubmit",
                "event": "UserPromptSubmit",
                "taskId": "task-1",
                "workspaceRoots": ["/tmp/demo"],
                "userPromptSubmit": { "prompt": "Ship it" }
            }))
            .expect("parse event");

        assert!(matches!(
            event,
            AgentEvent::Processing { session_id, description }
                if session_id == "task-1" && description.contains("Ship it")
        ));
    }

    #[test]
    fn parses_nested_tool_payload_and_failure_status() {
        let event = adapter()
            .parse_event(&serde_json::json!({
                "event": "PostToolUse",
                "taskId": "task-2",
                "workspaceRoots": ["/tmp/demo"],
                "postToolUse": {
                    "tool": "read_file",
                    "parameters": { "path": "src/main.rs" },
                    "success": false,
                    "result": "not found"
                }
            }))
            .expect("parse event");

        assert!(matches!(
            event,
            AgentEvent::ToolUse { session_id, tool_name, tool_input, status, .. }
                if session_id == "task-2"
                    && tool_name == "read_file"
                    && tool_input.contains("src/main.rs")
                    && status == "error"
        ));
    }
}
