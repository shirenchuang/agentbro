// Agent Adapter Trait & Registry
pub mod claude_code;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AdapterStatus {
    Active,
    Installed,
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    SessionStart {
        session_id: String,
        project: String,
        cwd: String,
        terminal: String,
        agent_type: String,
    },
    SessionEnd {
        session_id: String,
    },
    Processing {
        session_id: String,
        description: String,
    },
    ToolUse {
        session_id: String,
        tool_name: String,
        tool_input: String,
        tool_target: Option<String>,
        status: String,
    },
    PermissionRequest {
        session_id: String,
        tool_name: String,
        diff: Option<String>,
        options: Option<Vec<String>>,
    },
    AskQuestion {
        session_id: String,
        question: String,
        options: Vec<String>,
    },
    TaskComplete {
        session_id: String,
        summary: String,
    },
    Error {
        session_id: String,
        message: String,
    },
    Interrupt {
        session_id: String,
    },
    TokenUsage {
        session_id: String,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_create: u64,
    },
    SubagentStart {
        session_id: String,
        agent_id: String,
        description: String,
    },
    SubagentStop {
        session_id: String,
        agent_id: String,
        status: String,
    },
}

pub trait AgentAdapter: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn display_name(&self) -> &str;
    fn icon(&self) -> &str;
    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn status(&self) -> AdapterStatus;
    fn parse_event(&self, raw: &serde_json::Value) -> Result<AgentEvent, Box<dyn std::error::Error>>;
    fn hook_config_paths(&self) -> Vec<PathBuf>;
}

/// Adapter info returned to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterInfo {
    pub name: String,
    pub display_name: String,
    pub icon: String,
    pub status: AdapterStatus,
}
