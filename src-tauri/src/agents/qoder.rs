// QoderAdapter — Agent adapter for Qoder AI coding assistant

use super::{profiles, AdapterStatus, AgentAdapter, AgentEvent};
use crate::terminal::process_tree;
use std::path::{Path, PathBuf};

fn qoder_bundle_source(bundle_id: &str) -> Option<&'static str> {
    let bundle_id = bundle_id.to_ascii_lowercase();
    if bundle_id == "com.qoder.app" || bundle_id.starts_with("com.qoder.app.helper") {
        Some("qoder-desktop")
    } else if bundle_id == "com.qoder.ide" || bundle_id.starts_with("com.qoder.ide.helper") {
        Some("qoder")
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
pub(super) fn qoder_app_source(app: &Path) -> Option<&'static str> {
    let output = std::process::Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    qoder_bundle_source(std::str::from_utf8(&output.stdout).ok()?.trim())
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn windows_qoder_app_source(executable: &Path) -> Option<&'static str> {
    let name = executable.file_name()?.to_str()?;
    if name.eq_ignore_ascii_case("Qoder IDE.exe") {
        return Some("qoder");
    }
    if !name.eq_ignore_ascii_case("Qoder.exe") {
        return None;
    }
    let resources = executable.parent()?.join("resources");
    if resources.join("app/product.json").is_file() {
        Some("qoder")
    } else if resources.join("app.asar").is_file() {
        Some("qoder-desktop")
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn windows_process_image(pid: u32) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut path = vec![0u16; 32768];
    let mut len = path.len() as u32;
    let result = unsafe {
        let result = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut len);
        CloseHandle(process);
        result
    };
    (result != 0).then(|| PathBuf::from(std::ffi::OsString::from_wide(&path[..len as usize])))
}

fn qoder_process_source(process: &process_tree::ProcessInfo) -> Option<&'static str> {
    let path = Path::new(&process.command);
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    if !name.starts_with("qoder") {
        return None;
    }
    #[cfg(target_os = "macos")]
    if let Some(bundle) = path
        .ancestors()
        .find(|path| path.extension().is_some_and(|ext| ext == "app"))
    {
        return qoder_app_source(bundle);
    }
    #[cfg(target_os = "windows")]
    if name == "qoder.exe" || name == "qoder ide.exe" {
        return windows_process_image(process.pid)
            .and_then(|path| windows_qoder_app_source(&path).or(Some("qoder-cli")));
    }
    matches!(name.as_str(), "qoder" | "qodercli" | "qodercli.exe").then_some("qoder-cli")
}

pub fn qoder_hook_source(mut pid: u32, bundle_id: Option<&str>) -> Option<&'static str> {
    let tree = process_tree::build_tree();
    for _ in 0..20 {
        let Some(process) = tree.get(&pid) else {
            break;
        };
        if let Some(source) = qoder_process_source(process) {
            return Some(source);
        }
        if process.ppid == pid {
            break;
        }
        pid = process.ppid;
    }
    bundle_id.and_then(qoder_bundle_source)
}

pub struct QoderAdapter {
    source: &'static str,
    config_root: PathBuf,
    status: AdapterStatus,
}

impl QoderAdapter {
    pub fn new() -> Self {
        Self::with_source("qoder")
    }

    pub fn desktop() -> Self {
        Self::with_source("qoder-desktop")
    }

    fn with_source(source: &'static str) -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".qoder");
        let status = super::programs::detected_status_for_agent_program(source);
        Self {
            source,
            config_root,
            status,
        }
    }

    fn profile(&self) -> profiles::AgentIntegrationProfile {
        if self.source == "qoder-desktop" {
            profiles::qoder_desktop_profile()
        } else {
            profiles::qoder_profile()
        }
    }

    fn settings_path(&self) -> PathBuf {
        self.config_root.join("settings.json")
    }
}

impl AgentAdapter for QoderAdapter {
    fn name(&self) -> &str {
        self.source
    }
    fn display_name(&self) -> &str {
        if self.source == "qoder-desktop" {
            "Qoder Desktop"
        } else {
            "Qoder IDE"
        }
    }
    fn icon(&self) -> &str {
        "qoder"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::install_at(&self.profile(), &self.settings_path())?;
        log::info!("Qoder hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall_at(&self.profile(), &self.settings_path())?;
        log::info!("Qoder hooks removed");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }

    fn detect_status_now(&self) -> AdapterStatus {
        super::programs::detected_status_for_agent_program(self.source)
    }

    fn parse_event(
        &self,
        raw: &serde_json::Value,
    ) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        Ok(Self::parse_hook_event(self.source, raw))
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![self.settings_path()]
    }

    fn hooks_installed(&self) -> bool {
        profiles::is_installed_at(&self.profile(), &self.settings_path())
    }
}

impl QoderAdapter {
    pub(super) fn parse_hook_event(source: &str, raw: &serde_json::Value) -> AgentEvent {
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
            "session_start" | "SessionStart" => AgentEvent::SessionStart {
                session_id,
                project,
                cwd,
                terminal,
                agent_type: source.to_string(),
            },
            "session_end" | "SessionEnd" => AgentEvent::SessionEnd { session_id },
            "user_prompt_submit" | "UserPromptSubmit" => AgentEvent::Processing {
                session_id,
                description: user_prompt_description(raw),
            },
            "pre_tool_use" | "PreToolUse" => AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "running".to_string(),
            },
            "post_tool_use" | "PostToolUse" => AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "success".to_string(),
            },
            "post_tool_use_failure"
            | "PostToolUseFailure"
            | "permission_denied"
            | "PermissionDenied" => AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target: None,
                status: "error".to_string(),
            },
            "permission_request" | "PermissionRequest" => AgentEvent::PermissionRequest {
                session_id,
                tool_name,
                diff: None,
                options: None,
            },
            "notification" | "Notification" => AgentEvent::Notification {
                session_id,
                message: string_field(raw, &["message", "notification"])
                    .unwrap_or("")
                    .to_string(),
                status: string_field(raw, &["status"]).map(ToString::to_string),
            },
            "stop" | "Stop" => AgentEvent::AssistantResponseComplete {
                session_id,
                text: string_field(raw, &["summary", "last_assistant_message", "message"])
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("Task completed")
                    .to_string(),
            },
            "StopFailure" | "stop_failure" => AgentEvent::Error {
                session_id,
                message: string_field(raw, &["error_message", "error", "message"])
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("Task failed")
                    .to_string(),
            },
            "PreCompact" | "PostCompact" | "pre_compact" | "post_compact" => {
                AgentEvent::Processing {
                    session_id,
                    description: string_field(raw, &["message", "description"])
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or("Compacting context")
                        .to_string(),
                }
            }
            "SubagentStart" | "subagent_start" => AgentEvent::SubagentStart {
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
            },
            "SubagentStop" | "subagent_stop" => AgentEvent::SubagentStop {
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
            },
            _ => AgentEvent::Processing {
                session_id,
                description: format!("Event: {}", event),
            },
        }
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
            source: "qoder",
            config_root: PathBuf::from("/tmp/qoder-test"),
            status: AdapterStatus::Available,
        }
    }

    #[test]
    fn desktop_hooks_keep_the_desktop_session_identity() {
        let mut desktop = adapter();
        desktop.source = "qoder-desktop";
        let raw = json!({
            "hook_event_name": "SessionStart",
            "session_id": "qoder-task",
            "cwd": "/workspace/qoder"
        });
        assert!(matches!(
            desktop.parse_event(&raw).unwrap(),
            AgentEvent::SessionStart { session_id, agent_type, .. }
                if session_id == "qoder-task" && agent_type == "qoder-desktop"
        ));
        assert_eq!(
            qoder_bundle_source("com.qoder.app.helper.Renderer"),
            Some("qoder-desktop")
        );
        assert_eq!(qoder_bundle_source("com.qoder.ide.helper"), Some("qoder"));
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
