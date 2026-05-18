// Agent Adapter Trait & Registry
pub mod antigravity;
pub mod claude_code;
pub mod codebuddy;
pub mod codebuddycn;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod cursor_cli;
pub mod detection;
pub mod droid;
pub mod gemini;
pub mod hermes;
pub mod hook_manager;
pub mod kimi;
pub mod kiro;
pub mod opencode;
pub mod pi;
pub mod profiles;
pub mod programs;
pub mod qoder;
pub mod qoder_cli;
pub mod qwen;
pub mod stepfun;
pub mod trae;
pub mod trae_cli;
pub mod trae_cn;
pub mod traits;
pub mod workbuddy;

pub use traits::AgentAdapter;

use serde::{Deserialize, Serialize};

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
        descriptions: Vec<String>,
        header: Option<String>,
        multi_select: bool,
        questions: Vec<QuestionItem>,
    },
    PlanApproval {
        session_id: String,
        title: String,
        content: String,
        permissions: Vec<String>,
    },
    TaskComplete {
        session_id: String,
        summary: String,
    },
    AssistantResponseComplete {
        session_id: String,
        text: String,
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
    RateLimitUpdate {
        session_id: String,
        five_hour_usage: f64,
        five_hour_remaining: String,
        seven_day_usage: f64,
        seven_day_remaining: String,
        status_line_text: Option<String>,
        total_input_tokens: Option<u64>,
        total_output_tokens: Option<u64>,
        context_window_size: Option<u64>,
        context_used_percentage: Option<f64>,
        last_main_agent_at: Option<i64>,
        cache_ttl_ms: Option<i64>,
    },
    Notification {
        session_id: String,
        message: String,
        status: Option<String>,
    },
    SubagentStart {
        session_id: String,
        agent_id: String,
        name: Option<String>,
        description: String,
        agent_type: Option<String>,
        transcript_path: Option<String>,
    },
    SubagentStop {
        session_id: String,
        agent_id: String,
        status: String,
        name: Option<String>,
        agent_type: Option<String>,
        transcript_path: Option<String>,
        agent_transcript_path: Option<String>,
        last_assistant_message: Option<String>,
    },
    // Shell execution hooks
    ShellExecutionStart {
        session_id: String,
        command: String,
        cwd: String,
    },
    ShellExecutionEnd {
        session_id: String,
        command: String,
        exit_code: Option<i32>,
        stdout: Option<String>,
        stderr: Option<String>,
        duration_ms: u64,
    },
    // MCP execution hooks
    MCPExecutionStart {
        session_id: String,
        server_name: String,
        tool_name: String,
        arguments: String,
    },
    MCPExecutionEnd {
        session_id: String,
        server_name: String,
        tool_name: String,
        result: Option<String>,
        error: Option<String>,
        duration_ms: u64,
    },
    // Agent response hooks
    AgentResponse {
        session_id: String,
        content: String,
        content_type: String,
    },
    AgentThought {
        session_id: String,
        thought: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionItem {
    pub question: String,
    pub header: Option<String>,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
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

/// Build the default list of all supported adapters
pub fn all_adapters() -> Vec<Box<dyn AgentAdapter>> {
    vec![
        Box::new(claude_code::ClaudeCodeAdapter::new()),
        Box::new(codex::CodexAdapter::new()),
        Box::new(gemini::GeminiAdapter::new()),
        Box::new(cursor::CursorAdapter::new()),
        Box::new(cursor_cli::CursorCliAdapter::new()),
        Box::new(copilot::CopilotAdapter::new()),
        Box::new(trae::TraeAdapter::new()),
        Box::new(trae_cli::TraeCliAdapter::new()),
        Box::new(trae_cn::TraeCNAdapter::new()),
        Box::new(qoder::QoderAdapter::new()),
        Box::new(qoder_cli::QoderCliAdapter::new()),
        Box::new(codebuddy::CodeBuddyAdapter::new()),
        Box::new(codebuddycn::CodeBuddyCNAdapter::new()),
        Box::new(qwen::QwenAdapter::new()),
        Box::new(kimi::KimiAdapter::new()),
        Box::new(opencode::OpenCodeAdapter::new()),
        Box::new(droid::DroidAdapter::new()),
        Box::new(stepfun::StepFunAdapter::new()),
        Box::new(antigravity::AntiGravityAdapter::new()),
        Box::new(workbuddy::WorkBuddyAdapter::new()),
        Box::new(hermes::HermesAdapter::new()),
        Box::new(pi::PiAdapter::new()),
        Box::new(kiro::KiroAdapter::new()),
    ]
}

macro_rules! impl_default_adapter {
    ($($adapter:path),+ $(,)?) => {
        $(
            impl Default for $adapter {
                fn default() -> Self {
                    Self::new()
                }
            }
        )+
    };
}

impl_default_adapter!(
    antigravity::AntiGravityAdapter,
    claude_code::ClaudeCodeAdapter,
    codebuddy::CodeBuddyAdapter,
    codebuddycn::CodeBuddyCNAdapter,
    codex::CodexAdapter,
    copilot::CopilotAdapter,
    cursor::CursorAdapter,
    cursor_cli::CursorCliAdapter,
    droid::DroidAdapter,
    gemini::GeminiAdapter,
    hermes::HermesAdapter,
    kimi::KimiAdapter,
    kiro::KiroAdapter,
    opencode::OpenCodeAdapter,
    pi::PiAdapter,
    qoder::QoderAdapter,
    qoder_cli::QoderCliAdapter,
    qwen::QwenAdapter,
    stepfun::StepFunAdapter,
    trae::TraeAdapter,
    trae_cn::TraeCNAdapter,
    trae_cli::TraeCliAdapter,
    workbuddy::WorkBuddyAdapter,
);
