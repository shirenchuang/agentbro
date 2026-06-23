// KimiAdapter — Agent adapter for Moonshot Kimi (TOML config format)

use super::profiles;
use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

pub struct KimiAdapter {
    status: AdapterStatus,
}

impl KimiAdapter {
    pub fn new() -> Self {
        let status = Self::detect_status();
        Self { status }
    }

    fn detect_status() -> AdapterStatus {
        super::programs::detected_status_for_agent_program("kimi")
    }

    fn config_path(&self) -> PathBuf {
        profiles::configuration_url(&profiles::kimi_profile())
    }
}

impl AgentAdapter for KimiAdapter {
    fn name(&self) -> &str {
        "kimi"
    }
    fn display_name(&self) -> &str {
        "Kimi"
    }
    fn icon(&self) -> &str {
        "kimi"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::install(&profiles::kimi_profile())?;
        log::info!("Kimi hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall(&profiles::kimi_profile())?;
        log::info!("Kimi hooks removed");
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
        let session_id = raw
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        // Kimi stdin JSON uses `hook_event_name` (PascalCase value) per the
        // official spec. Older builds may have used `event` — keep it as a
        // fallback so we don't regress local fixtures.
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
        let tool_name = || {
            raw.get("tool_name")
                .or_else(|| raw.get("tool"))
                .and_then(|v| v.as_str())
                .unwrap_or("Tool")
                .to_string()
        };
        let tool_input_str = || {
            raw.get("tool_input")
                .map(|v| v.to_string())
                .unwrap_or_default()
        };
        match event {
            "SessionStart" => Ok(AgentEvent::SessionStart {
                session_id,
                project: super::project_name_from_path(&cwd),
                cwd,
                terminal: String::new(),
                agent_type: "kimi".to_string(),
            }),
            "SessionEnd" => Ok(AgentEvent::SessionEnd { session_id }),
            "Stop" => Ok(AgentEvent::TaskComplete {
                session_id,
                summary: "Kimi turn completed".to_string(),
            }),
            "StopFailure" => Ok(AgentEvent::Error {
                session_id,
                message: raw
                    .get("error_message")
                    .or_else(|| raw.get("error"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Kimi turn failed")
                    .to_string(),
            }),
            "PreToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: tool_name(),
                tool_input: tool_input_str(),
                tool_target: None,
                status: "running".to_string(),
            }),
            "PostToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: tool_name(),
                tool_input: tool_input_str(),
                tool_target: None,
                status: "success".to_string(),
            }),
            "PostToolUseFailure" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name: tool_name(),
                tool_input: tool_input_str(),
                tool_target: None,
                status: "failure".to_string(),
            }),
            "UserPromptSubmit" => Ok(AgentEvent::Processing {
                session_id,
                description: "User prompt submitted".to_string(),
            }),
            "Notification" => Ok(AgentEvent::Notification {
                session_id,
                message: raw
                    .get("body")
                    .or_else(|| raw.get("title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Kimi notification")
                    .to_string(),
                status: raw
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            }),
            "SubagentStart" => Ok(AgentEvent::SubagentStart {
                session_id,
                agent_id: raw
                    .get("agent_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("kimi-subagent")
                    .to_string(),
                name: raw
                    .get("agent_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                description: raw
                    .get("agent_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("subagent")
                    .to_string(),
                agent_type: Some("kimi".to_string()),
                transcript_path: None,
            }),
            "SubagentStop" => Ok(AgentEvent::SubagentStop {
                session_id,
                agent_id: raw
                    .get("agent_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("kimi-subagent")
                    .to_string(),
                status: "completed".to_string(),
                name: raw
                    .get("agent_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                agent_type: Some("kimi".to_string()),
                transcript_path: None,
                agent_transcript_path: None,
                last_assistant_message: raw
                    .get("response")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            }),
            "PreCompact" => Ok(AgentEvent::Processing {
                session_id,
                description: format!(
                    "Compacting context ({})",
                    raw.get("trigger")
                        .and_then(|v| v.as_str())
                        .unwrap_or("auto")
                ),
            }),
            "PostCompact" => Ok(AgentEvent::Processing {
                session_id,
                description: "Context compacted".to_string(),
            }),
            other => Ok(AgentEvent::Processing {
                session_id,
                description: format!("Kimi event: {}", other),
            }),
        }
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![self.config_path()]
    }

    fn hooks_installed(&self) -> bool {
        profiles::is_installed(&profiles::kimi_profile())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn adapter() -> KimiAdapter {
        // Bypass detection — we only test parse_event.
        KimiAdapter {
            status: AdapterStatus::Unavailable,
        }
    }

    /// Per Kimi spec the stdin JSON uses `hook_event_name`, not `event`.
    /// Reading the wrong key was Bug #3 — every event silently fell through
    /// to the Processing fallback. Guard against regression.
    #[test]
    fn parse_event_reads_hook_event_name_field() {
        let raw = json!({
            "session_id": "abc",
            "cwd": "/tmp/project",
            "hook_event_name": "SessionStart",
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::SessionStart {
                session_id,
                cwd,
                agent_type,
                ..
            } => {
                assert_eq!(session_id, "abc");
                assert_eq!(cwd, "/tmp/project");
                assert_eq!(agent_type, "kimi");
            }
            other => panic!("expected SessionStart, got {:?}", other),
        }
    }

    #[test]
    fn parse_event_routes_pre_tool_use_to_tool_use_running() {
        let raw = json!({
            "session_id": "s",
            "hook_event_name": "PreToolUse",
            "tool_name": "Shell",
            "tool_input": {"command": "ls"},
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::ToolUse {
                tool_name, status, ..
            } => {
                assert_eq!(tool_name, "Shell");
                assert_eq!(status, "running");
            }
            other => panic!("expected ToolUse running, got {:?}", other),
        }
    }

    #[test]
    fn parse_event_routes_post_tool_use_failure_to_tool_use_failure() {
        let raw = json!({
            "session_id": "s",
            "hook_event_name": "PostToolUseFailure",
            "tool_name": "Shell",
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::ToolUse { status, .. } => assert_eq!(status, "failure"),
            other => panic!("expected ToolUse failure, got {:?}", other),
        }
    }

    #[test]
    fn parse_event_routes_stop_failure_to_error() {
        let raw = json!({
            "session_id": "s",
            "hook_event_name": "StopFailure",
            "error_message": "model timed out",
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::Error { message, .. } => assert_eq!(message, "model timed out"),
            other => panic!("expected Error, got {:?}", other),
        }
    }

    #[test]
    fn parse_event_routes_subagent_start_and_stop() {
        let start = adapter()
            .parse_event(&json!({
                "session_id": "s",
                "hook_event_name": "SubagentStart",
                "agent_name": "code-review",
            }))
            .expect("parse");
        match start {
            AgentEvent::SubagentStart { agent_id, .. } => assert_eq!(agent_id, "code-review"),
            other => panic!("expected SubagentStart, got {:?}", other),
        }

        let stop = adapter()
            .parse_event(&json!({
                "session_id": "s",
                "hook_event_name": "SubagentStop",
                "agent_name": "code-review",
                "response": "done",
            }))
            .expect("parse");
        match stop {
            AgentEvent::SubagentStop {
                agent_id,
                last_assistant_message,
                ..
            } => {
                assert_eq!(agent_id, "code-review");
                assert_eq!(last_assistant_message.as_deref(), Some("done"));
            }
            other => panic!("expected SubagentStop, got {:?}", other),
        }
    }

    #[test]
    fn parse_event_routes_notification() {
        let raw = json!({
            "session_id": "s",
            "hook_event_name": "Notification",
            "title": "Heads up",
            "body": "Long-running task",
            "severity": "info",
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        match evt {
            AgentEvent::Notification {
                message, status, ..
            } => {
                assert_eq!(message, "Long-running task");
                assert_eq!(status.as_deref(), Some("info"));
            }
            other => panic!("expected Notification, got {:?}", other),
        }
    }

    #[test]
    fn parse_event_routes_compact_events_to_processing() {
        let pre = adapter()
            .parse_event(&json!({
                "session_id": "s",
                "hook_event_name": "PreCompact",
                "trigger": "auto",
            }))
            .expect("parse");
        match pre {
            AgentEvent::Processing { description, .. } => {
                assert!(description.contains("Compacting"));
                assert!(description.contains("auto"));
            }
            other => panic!("expected Processing, got {:?}", other),
        }

        let post = adapter()
            .parse_event(&json!({
                "session_id": "s",
                "hook_event_name": "PostCompact",
            }))
            .expect("parse");
        match post {
            AgentEvent::Processing { description, .. } => {
                assert!(description.contains("compacted"))
            }
            other => panic!("expected Processing, got {:?}", other),
        }
    }

    /// Backward compatibility: tolerate stdin shaped like older fixtures that
    /// used `event` instead of `hook_event_name`. Both should route correctly.
    #[test]
    fn parse_event_falls_back_to_event_field_for_legacy_payloads() {
        let raw = json!({
            "session_id": "s",
            "event": "Stop",
        });
        let evt = adapter().parse_event(&raw).expect("parse");
        assert!(matches!(evt, AgentEvent::TaskComplete { .. }));
    }
}
