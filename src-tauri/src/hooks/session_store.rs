// SessionStore — Thread-safe session state management
// Central source of truth for all agent sessions

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Session phase — mirrors frontend SessionPhase type
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    #[default]
    Ready,
    Idle,
    Processing,
    WaitingApproval,
    WaitingInput,
    Compacting,
    Done,
    Error,
    Interrupted,
}

impl SessionPhase {
    /// Check whether transitioning from `self` to `next` is valid.
    pub fn can_transition_to(&self, next: &SessionPhase) -> bool {
        use SessionPhase::*;
        matches!(
            (self, next),
            (Ready, Processing)
                | (Ready, WaitingApproval)
                | (Ready, Compacting)
                | (Ready, Idle)
                | (Ready, Done)
                | (Idle, Ready)
                | (Idle, Processing)
                | (Idle, WaitingApproval)
                | (Idle, Compacting)
                | (Processing, WaitingApproval)
                | (Processing, WaitingInput)
                | (Processing, Done)
                | (Processing, Ready)
                | (Processing, Error)
                | (Processing, Interrupted)
                | (Processing, Compacting)
                | (Processing, Idle)
                | (WaitingApproval, Ready)
                | (WaitingApproval, Processing)
                | (WaitingApproval, Idle)
                | (WaitingInput, Ready)
                | (WaitingInput, Processing)
                | (WaitingInput, Idle)
                | (Compacting, Ready)
                | (Compacting, Processing)
                | (Compacting, Idle)
                | (Done, Ready)
                | (Done, Idle)
                | (Done, Processing)
                | (Error, Ready)
                | (Error, Idle)
                | (Error, Processing)
                | (Interrupted, Ready)
                | (Interrupted, Idle)
                | (Interrupted, Processing)
                | (_, Ready)
                | (_, Idle) // Reset to Idle is always valid
        )
    }

    /// Returns true when this phase requires user attention.
    pub fn needs_attention(&self) -> bool {
        matches!(
            self,
            Self::WaitingApproval | Self::WaitingInput | Self::Error
        )
    }

    /// Returns true when the session is actively doing something.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Processing | Self::WaitingApproval | Self::WaitingInput | Self::Compacting
        )
    }
}

/// Token usage accumulator
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
}

/// API rate-limit status for compact island display.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitInfo {
    pub five_hour_usage: f64,
    pub five_hour_remaining: String,
    pub seven_day_usage: f64,
    pub seven_day_remaining: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub windows: Vec<UsageRateWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRateWindow {
    pub id: String,
    pub title: String,
    pub used_percent: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_minutes: Option<i64>,
}

impl RateLimitInfo {
    pub fn legacy(
        five_hour_usage: f64,
        five_hour_remaining: String,
        seven_day_usage: f64,
        seven_day_remaining: String,
    ) -> Self {
        Self {
            five_hour_usage,
            five_hour_remaining,
            seven_day_usage,
            seven_day_remaining,
            provider: None,
            provider_label: None,
            source: None,
            updated_at: None,
            windows: Vec::new(),
        }
    }
}

/// Context-window usage reported by an agent statusline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextWindowInfo {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub context_window_size: u64,
    pub used_percentage: Option<f64>,
}

/// Pending permission request details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPermission {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    pub tool_name: String,
    pub tool_input: String,
    pub diff: Option<String>,
    pub options: Option<Vec<String>>,
}

/// Pending question details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingQuestion {
    pub question: String,
    pub options: Vec<String>,
    pub descriptions: Vec<String>,
    pub header: Option<String>,
    #[serde(rename = "multiSelect")]
    pub multi_select: bool,
    #[serde(default)]
    pub questions: Vec<QuestionItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub question: String,
    #[serde(default)]
    pub header: Option<String>,
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multi_select: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Pending plan approval details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPlan {
    pub title: String,
    pub content: String,
    pub permissions: Vec<String>,
}

/// Information about an active subagent (nested agent spawned via Task tool)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentInfo {
    pub agent_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    pub description: String,
    #[serde(default)]
    pub transcript_path: Option<String>,
    #[serde(default)]
    pub agent_transcript_path: Option<String>,
    #[serde(default)]
    pub last_assistant_message: Option<String>,
    pub started_at: i64,
    #[serde(default)]
    pub completed_at: Option<i64>,
    pub status: String,     // "running", "completed", "error"
    pub tools: Vec<String>, // last few tool names used by this subagent
}

#[derive(Debug, Clone)]
pub struct SubagentStopUpdate {
    pub status: String,
    pub name: Option<String>,
    pub agent_type: Option<String>,
    pub transcript_path: Option<String>,
    pub agent_transcript_path: Option<String>,
    pub last_assistant_message: Option<String>,
}

/// Aggregated task item from TaskCreate/TaskUpdate tool calls
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInfo {
    pub id: String,
    pub name: String,
    pub status: String, // "pending", "in_progress", "completed"
}

/// A completed tool result cached for display
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub tool_use_id: String,
    pub tool_name: String,
    pub status: String, // "running", "success", "error"
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub error: Option<String>,
    pub tool_input: Option<String>,
}

/// State of a single agent session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub id: String,
    pub agent_type: String,
    pub engine_label: Option<String>,
    pub engine_config_root: Option<String>,
    pub codex_app_server_thread_id: Option<String>,
    pub project: String,
    pub cwd: String,
    pub terminal: String,
    pub phase: SessionPhase,
    pub started_at: i64,
    pub duration: i64,
    pub tokens: TokenUsage,
    pub rate_limits: Option<RateLimitInfo>,
    pub status_line_text: Option<String>,
    pub context_window: Option<ContextWindowInfo>,
    pub last_main_agent_at: Option<i64>,
    pub cache_ttl_ms: Option<i64>,
    pub pending_permission: Option<PendingPermission>,
    pub pending_question: Option<PendingQuestion>,
    pub pending_plan: Option<PendingPlan>,
    pub last_tool_name: Option<String>,
    pub last_tool_target: Option<String>,
    pub last_tool_status: Option<String>,
    pub description: Option<String>,
    pub session_title: Option<String>,
    pub remote_host_id: Option<String>,
    pub remote_host_name: Option<String>,
    pub pid: Option<u32>,
    pub tty: Option<String>,
    pub term_program: Option<String>,
    pub term_bundle_id: Option<String>,
    pub wezterm_pane: Option<String>,
    pub zellij_pane_id: Option<String>,
    pub zellij_session_name: Option<String>,
    pub cmux_surface_id: Option<String>,
    pub cmux_workspace_id: Option<String>,
    pub subagents: Vec<SubagentInfo>,
    pub active_tools: Vec<ToolResult>,
    pub tasks: Vec<TaskInfo>,
    pub last_user_message: Option<String>,
    pub last_response: Option<String>,
    pub last_thought: Option<String>,
    #[serde(default)]
    pub is_yolo_mode: bool,
    pub model: Option<String>,
}

impl SessionState {
    pub fn new(
        id: String,
        agent_type: String,
        project: String,
        cwd: String,
        terminal: String,
    ) -> Self {
        Self {
            id,
            agent_type,
            engine_label: None,
            engine_config_root: None,
            codex_app_server_thread_id: None,
            project,
            cwd,
            terminal,
            phase: SessionPhase::Ready,
            started_at: Utc::now().timestamp(),
            duration: 0,
            tokens: TokenUsage::default(),
            rate_limits: None,
            status_line_text: None,
            context_window: None,
            last_main_agent_at: None,
            cache_ttl_ms: None,
            pending_permission: None,
            pending_question: None,
            pending_plan: None,
            last_tool_name: None,
            last_tool_target: None,
            last_tool_status: None,
            description: None,
            session_title: None,
            remote_host_id: None,
            remote_host_name: None,
            pid: None,
            tty: None,
            term_program: None,
            term_bundle_id: None,
            wezterm_pane: None,
            zellij_pane_id: None,
            zellij_session_name: None,
            cmux_surface_id: None,
            cmux_workspace_id: None,
            subagents: Vec::new(),
            active_tools: Vec::new(),
            tasks: Vec::new(),
            last_user_message: None,
            last_response: None,
            last_thought: None,
            is_yolo_mode: false,
            model: None,
        }
    }

    /// Update duration based on current time
    pub fn update_duration(&mut self) {
        self.duration = Utc::now().timestamp() - self.started_at;
    }

    pub fn has_unfinished_tasks(&self) -> bool {
        self.tasks.iter().any(|task| task.status != "completed")
    }
}

fn reconcile_pending_for_phase(session: &mut SessionState) {
    match &session.phase {
        SessionPhase::WaitingApproval => {
            session.pending_question = None;
        }
        SessionPhase::WaitingInput => {
            session.pending_permission = None;
            session.pending_plan = None;
        }
        _ => {
            session.pending_permission = None;
            session.pending_question = None;
            session.pending_plan = None;
        }
    }
}

/// Event payload emitted to the frontend via Tauri events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdatePayload {
    pub sessions: Vec<SessionState>,
    /// When true, the event that triggered this update was suppressed
    /// because the agent's terminal is currently focused.
    #[serde(default)]
    pub suppressed: bool,
}

/// Thread-safe session store
#[derive(Clone)]
pub struct SessionStore {
    sessions: Arc<DashMap<String, SessionState>>,
    app_handle: Option<AppHandle>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            app_handle: None,
        }
    }

    /// Set the app handle for emitting Tauri events
    pub fn set_app_handle(&mut self, handle: AppHandle) {
        self.app_handle = Some(handle);
    }

    /// Get or create a session (atomic via DashMap entry API to avoid TOCTOU)
    pub fn get_or_create_session(
        &self,
        session_id: &str,
        agent_type: &str,
        project: &str,
        cwd: &str,
        terminal: &str,
    ) -> SessionState {
        let entry = self
            .sessions
            .entry(session_id.to_string())
            .or_insert_with(|| {
                SessionState::new(
                    session_id.to_string(),
                    agent_type.to_string(),
                    project.to_string(),
                    cwd.to_string(),
                    terminal.to_string(),
                )
            });
        let session = entry.value().clone();
        drop(entry);
        self.emit_update();
        session
    }

    /// Update a session's phase with state-machine validation.
    /// Logs a warning on invalid transitions but still applies the change
    /// to avoid blocking the pipeline.
    pub fn update_phase(&self, session_id: &str, phase: SessionPhase) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            if !session.phase.can_transition_to(&phase) {
                log::warn!(
                    "[session {}] Invalid phase transition: {:?} -> {:?}",
                    session_id,
                    session.phase,
                    phase
                );
            }
            session.phase = phase;
            reconcile_pending_for_phase(&mut session);
            session.update_duration();
        }
        self.emit_update();
    }

    /// Update a session with a full state replacement
    pub fn update_session(&self, session_id: &str, updater: impl FnOnce(&mut SessionState)) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            updater(&mut session);
            reconcile_pending_for_phase(&mut session);
            session.update_duration();
        }
        self.emit_update();
    }

    /// Set pending permission on a session
    pub fn set_pending_permission(&self, session_id: &str, permission: Option<PendingPermission>) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.pending_permission = permission;
            if session.pending_permission.is_some() {
                session.phase = SessionPhase::WaitingApproval;
            }
            session.update_duration();
        }
        self.emit_update();
    }

    /// Set pending question on a session
    pub fn set_pending_question(&self, session_id: &str, question: Option<PendingQuestion>) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.pending_question = question;
            if session.pending_question.is_some() {
                session.phase = SessionPhase::WaitingInput;
            }
            session.update_duration();
        }
        self.emit_update();
    }

    /// Set pending plan approval on a session
    pub fn set_pending_plan(&self, session_id: &str, plan: Option<PendingPlan>) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.pending_plan = plan;
            if session.pending_plan.is_some() {
                session.phase = SessionPhase::WaitingApproval;
            }
            session.update_duration();
        }
        self.emit_update();
    }

    /// Accumulate token usage for a session
    pub fn add_tokens(
        &self,
        session_id: &str,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_create: u64,
    ) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.tokens.input += input;
            session.tokens.output += output;
            session.tokens.cache_read += cache_read;
            session.tokens.cache_create += cache_create;
        }
        self.emit_update();
    }

    /// Set API rate-limit information for a session.
    pub fn set_rate_limits(
        &self,
        session_id: &str,
        rate_limits: RateLimitInfo,
        status_line_text: Option<String>,
    ) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.rate_limits = Some(rate_limits);
            session.status_line_text = status_line_text;
            session.update_duration();
        }
        self.emit_update();
    }

    /// Set statusline context and optional cache TTL metadata.
    pub fn set_statusline_metadata(
        &self,
        session_id: &str,
        context_window: Option<ContextWindowInfo>,
        status_line_text: Option<String>,
        last_main_agent_at: Option<i64>,
        cache_ttl_ms: Option<i64>,
    ) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            if context_window.is_some() {
                session.context_window = context_window;
            }
            if status_line_text.is_some() {
                session.status_line_text = status_line_text;
            }
            if last_main_agent_at.is_some() {
                session.last_main_agent_at = last_main_agent_at;
            }
            if cache_ttl_ms.is_some() {
                session.cache_ttl_ms = cache_ttl_ms;
            }
            session.update_duration();
        }
        self.emit_update();
    }

    /// Remove a session (on session end)
    pub fn remove_session(&self, session_id: &str) {
        self.sessions.remove(session_id);
        self.emit_update();
    }

    /// Add or update a subagent for a session
    pub fn add_subagent(
        &self,
        session_id: &str,
        agent_id: &str,
        name: Option<String>,
        description: &str,
        agent_type: Option<String>,
        transcript_path: Option<String>,
    ) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            // Check if subagent already exists (update it)
            if let Some(existing) = session
                .subagents
                .iter_mut()
                .find(|s| s.agent_id == agent_id)
            {
                existing.status = "running".to_string();
                existing.completed_at = None;
                if name.is_some() {
                    existing.name = name;
                }
                if !description.is_empty() {
                    existing.description = description.to_string();
                }
                if agent_type.is_some() {
                    existing.agent_type = agent_type;
                }
                if transcript_path.is_some() {
                    existing.transcript_path = transcript_path;
                }
            } else {
                session.subagents.push(SubagentInfo {
                    agent_id: agent_id.to_string(),
                    name,
                    agent_type,
                    description: description.to_string(),
                    transcript_path,
                    agent_transcript_path: None,
                    last_assistant_message: None,
                    started_at: Utc::now().timestamp(),
                    completed_at: None,
                    status: "running".to_string(),
                    tools: Vec::new(),
                });
            }
            session.update_duration();
        }
        self.emit_update();
    }

    /// Stop a subagent (mark as completed, keep in list for frontend display)
    pub fn stop_subagent(&self, session_id: &str, agent_id: &str, update: SubagentStopUpdate) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            if let Some(existing) = session
                .subagents
                .iter_mut()
                .find(|s| s.agent_id == agent_id)
            {
                existing.status = update.status;
                existing.completed_at = Some(Utc::now().timestamp());
                if update.name.is_some() {
                    existing.name = update.name;
                }
                if update.agent_type.is_some() {
                    existing.agent_type = update.agent_type;
                }
                if update.transcript_path.is_some() {
                    existing.transcript_path = update.transcript_path;
                }
                if update.agent_transcript_path.is_some() {
                    existing.agent_transcript_path = update.agent_transcript_path;
                }
                if update.last_assistant_message.is_some() {
                    existing.last_assistant_message = update.last_assistant_message;
                }
            }
            session.update_duration();
        }
        self.emit_update();
    }

    /// Add or update a task for a session
    pub fn upsert_task(&self, session_id: &str, task_id: &str, name: &str, status: &str) {
        if task_id.is_empty() || name.is_empty() {
            return;
        }

        if let Some(mut session) = self.sessions.get_mut(session_id) {
            if let Some(existing) = session.tasks.iter_mut().find(|task| task.id == task_id) {
                existing.name = name.to_string();
                existing.status = status.to_string();
            } else {
                session.tasks.push(TaskInfo {
                    id: task_id.to_string(),
                    name: name.to_string(),
                    status: status.to_string(),
                });
            }
            session.update_duration();
        }
        self.emit_update();
    }

    /// Update the latest user prompt preview for response overlays.
    pub fn set_last_user_message(&self, session_id: &str, message: Option<String>) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.last_user_message = message;
            session.update_duration();
        }
        self.emit_update();
    }

    /// Track a tool starting (PreToolUse)
    pub fn start_tool(
        &self,
        session_id: &str,
        tool_use_id: &str,
        tool_name: &str,
        tool_input: Option<String>,
    ) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            // Don't add duplicate entries
            if !session
                .active_tools
                .iter()
                .any(|t| t.tool_use_id == tool_use_id)
            {
                session.active_tools.push(ToolResult {
                    tool_use_id: tool_use_id.to_string(),
                    tool_name: tool_name.to_string(),
                    status: "running".to_string(),
                    started_at: Utc::now().timestamp(),
                    completed_at: None,
                    error: None,
                    tool_input,
                });
                // Keep only last 20 tools
                if session.active_tools.len() > 20 {
                    session.active_tools.remove(0);
                }
            } else if let Some(input) = tool_input {
                if let Some(existing) = session
                    .active_tools
                    .iter_mut()
                    .find(|t| t.tool_use_id == tool_use_id)
                {
                    existing.tool_input = Some(input);
                }
            }
            // Also track tool against active subagent
            if let Some(subagent) = session.subagents.last_mut() {
                if !subagent.tools.contains(&tool_name.to_string()) {
                    subagent.tools.push(tool_name.to_string());
                    // Keep only last 5 tool names per subagent
                    if subagent.tools.len() > 5 {
                        subagent.tools.remove(0);
                    }
                }
            }
            session.update_duration();
        }
        self.emit_update();
    }

    /// Mark a tool as completed (PostToolUse / PostToolUseFailure)
    pub fn complete_tool(
        &self,
        session_id: &str,
        tool_use_id: &str,
        success: bool,
        error: Option<String>,
    ) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            if let Some(tool) = session
                .active_tools
                .iter_mut()
                .find(|t| t.tool_use_id == tool_use_id)
            {
                tool.status = if success {
                    "success".to_string()
                } else {
                    "error".to_string()
                };
                tool.completed_at = Some(Utc::now().timestamp());
                tool.error = error;
            }
            session.update_duration();
        }
        self.emit_update();
    }

    /// Get all sessions as a vector
    pub fn get_all_sessions(&self) -> Vec<SessionState> {
        self.prune_dead_process_sessions();
        self.sessions
            .iter()
            .map(|entry| {
                let mut s = entry.value().clone();
                s.update_duration();
                s
            })
            .collect()
    }

    fn prune_dead_process_sessions(&self) {
        let tree = crate::terminal::process_tree::build_tree();
        let mut ended_ids = Vec::new();
        let mut stale_ids = Vec::new();

        for entry in self.sessions.iter() {
            let session = entry.value();
            let Some(pid) = session.pid else {
                continue;
            };
            if tree.contains_key(&pid) || session.phase.needs_attention() {
                continue;
            }
            if session.phase == SessionPhase::Done {
                stale_ids.push(session.id.clone());
            } else {
                ended_ids.push(session.id.clone());
            }
        }

        for id in &ended_ids {
            log::info!(
                "Marking session {} as ended because PID is no longer alive",
                id
            );
            if let Some(mut session) = self.sessions.get_mut(id) {
                session.phase = SessionPhase::Done;
                session.description = Some("Session ended".to_string());
                session.last_response = Some("Session ended".to_string());
                session.pending_permission = None;
                session.pending_question = None;
                session.pending_plan = None;
                session.update_duration();
            }
        }

        for id in &stale_ids {
            log::info!(
                "Removing stale session {} because PID is no longer alive",
                id
            );
            self.sessions.remove(id);
        }

        if !stale_ids.is_empty() || !ended_ids.is_empty() {
            self.emit_update();
        }
    }

    /// Get a specific session
    pub fn get_session(&self, session_id: &str) -> Option<SessionState> {
        self.sessions.get(session_id).map(|s| {
            let mut session = s.value().clone();
            session.update_duration();
            session
        })
    }

    /// Emit session update event to the frontend
    fn emit_update(&self) {
        self.emit_update_with_suppression(false);
    }

    /// Emit session update event with an optional suppression flag.
    /// When `suppressed` is true the frontend should update state but NOT
    /// auto-expand the panel (because the user is looking at the terminal).
    pub fn emit_update_suppressed(&self, suppressed: bool) {
        self.emit_update_with_suppression(suppressed);
    }

    fn emit_update_with_suppression(&self, suppressed: bool) {
        if let Some(ref handle) = self.app_handle {
            let sessions = self.get_all_sessions();
            let payload = SessionUpdatePayload {
                sessions,
                suppressed,
            };
            if let Err(e) = handle.emit("session-update", &payload) {
                log::error!("Failed to emit session-update: {}", e);
            }
        }
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_store() -> SessionStore {
        let store = SessionStore::new();
        store.get_or_create_session("s1", "claude-code", "agentbro", "/tmp/agentbro", "iTerm");
        store
    }

    #[test]
    fn upsert_task_adds_and_updates_session_tasks() {
        let store = seeded_store();

        store.upsert_task("s1", "task-1", "Port island overlay", "pending");
        store.upsert_task("s1", "task-1", "Port island overlay", "completed");

        let session = store.get_session("s1").expect("session should exist");
        assert_eq!(session.tasks.len(), 1);
        assert_eq!(session.tasks[0].id, "task-1");
        assert_eq!(session.tasks[0].name, "Port island overlay");
        assert_eq!(session.tasks[0].status, "completed");
    }

    #[test]
    fn session_serializes_subagents_tools_and_tasks_for_frontend() {
        let store = seeded_store();

        store.add_subagent(
            "s1",
            "agent-1",
            Some("island-audit".to_string()),
            "Inspect AgentBro island",
            Some("research".to_string()),
            Some("/tmp/main.jsonl".to_string()),
        );
        store.stop_subagent(
            "s1",
            "agent-1",
            SubagentStopUpdate {
                status: "completed".to_string(),
                name: None,
                agent_type: None,
                transcript_path: None,
                agent_transcript_path: Some("/tmp/agent.jsonl".to_string()),
                last_assistant_message: Some("Subagent completed its audit".to_string()),
            },
        );
        store.start_tool(
            "s1",
            "tool-1",
            "Read",
            Some("{\"file_path\":\"src/lib.rs\"}".to_string()),
        );
        store.complete_tool("s1", "tool-1", true, None);
        store.upsert_task("s1", "task-1", "Verify parity", "in_progress");

        let session = store.get_session("s1").expect("session should exist");
        let json = serde_json::to_value(session).expect("session should serialize");

        assert_eq!(json["subagents"][0]["agentId"], "agent-1");
        assert_eq!(json["subagents"][0]["name"], "island-audit");
        assert_eq!(json["subagents"][0]["agentType"], "research");
        assert_eq!(json["subagents"][0]["transcriptPath"], "/tmp/main.jsonl");
        assert_eq!(
            json["subagents"][0]["agentTranscriptPath"],
            "/tmp/agent.jsonl"
        );
        assert_eq!(
            json["subagents"][0]["lastAssistantMessage"],
            "Subagent completed its audit"
        );
        assert!(json["subagents"][0]["completedAt"].is_i64());
        assert_eq!(json["subagents"][0]["tools"][0], "Read");
        assert_eq!(json["activeTools"][0]["toolUseId"], "tool-1");
        assert_eq!(json["activeTools"][0]["status"], "success");
        assert_eq!(
            json["activeTools"][0]["toolInput"],
            "{\"file_path\":\"src/lib.rs\"}"
        );
        assert_eq!(json["tasks"][0]["id"], "task-1");
        assert_eq!(json["tasks"][0]["status"], "in_progress");
    }

    #[test]
    fn session_serializes_last_user_message_for_frontend() {
        let store = seeded_store();

        store.set_last_user_message("s1", Some("Continue the migration".to_string()));

        let session = store.get_session("s1").expect("session should exist");
        let json = serde_json::to_value(session).expect("session should serialize");

        assert_eq!(json["lastUserMessage"], "Continue the migration");
    }

    #[test]
    fn session_detects_unfinished_tasks() {
        let mut session = SessionState::new(
            "s1".to_string(),
            "claude-code".to_string(),
            "agentbro".to_string(),
            "/tmp/agentbro".to_string(),
            "iTerm".to_string(),
        );

        assert!(!session.has_unfinished_tasks());

        session.tasks.push(TaskInfo {
            id: "task-1".to_string(),
            name: "Verify parity".to_string(),
            status: "in_progress".to_string(),
        });
        assert!(session.has_unfinished_tasks());

        session.tasks[0].status = "completed".to_string();
        assert!(!session.has_unfinished_tasks());
    }

    #[test]
    fn get_all_sessions_prunes_dead_pid_sessions() {
        let store = seeded_store();
        store.update_session("s1", |session| {
            session.pid = Some(u32::MAX);
            session.phase = SessionPhase::Done;
        });

        assert!(store.get_all_sessions().is_empty());
        assert!(store.get_session("s1").is_none());
    }

    #[test]
    fn update_session_clears_pending_permission_when_activity_resumes() {
        let store = seeded_store();
        store.set_pending_permission(
            "s1",
            Some(PendingPermission {
                tool_use_id: None,
                tool_name: "Bash".to_string(),
                tool_input: "{\"command\":\"pnpm test\"}".to_string(),
                diff: None,
                options: None,
            }),
        );

        store.update_session("s1", |session| {
            session.phase = SessionPhase::Processing;
            session.last_tool_name = Some("Bash".to_string());
            session.last_tool_status = Some("running".to_string());
        });

        let session = store.get_session("s1").expect("session should exist");
        assert_eq!(session.phase, SessionPhase::Processing);
        assert!(session.pending_permission.is_none());
    }

    #[test]
    fn get_all_sessions_marks_dead_active_sessions_as_ended_once() {
        let store = seeded_store();
        store.update_session("s1", |session| {
            session.pid = Some(u32::MAX);
            session.phase = SessionPhase::Processing;
            session.description = Some("Hi! How can I help you today?".to_string());
            session.last_response = Some("Hi! How can I help you today?".to_string());
        });

        let sessions = store.get_all_sessions();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].phase, SessionPhase::Done);
        assert_eq!(sessions[0].description.as_deref(), Some("Session ended"));
        assert_eq!(sessions[0].last_response.as_deref(), Some("Session ended"));
    }

    #[test]
    fn get_all_sessions_keeps_attention_sessions_even_when_pid_is_dead() {
        let store = seeded_store();
        store.update_session("s1", |session| {
            session.pid = Some(u32::MAX);
            session.phase = SessionPhase::WaitingInput;
        });

        assert_eq!(store.get_all_sessions().len(), 1);
        assert!(store.get_session("s1").is_some());
    }
}
