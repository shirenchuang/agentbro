// SessionStore — Thread-safe session state management
// Central source of truth for all agent sessions

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Session phase — mirrors frontend SessionPhase type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
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
            (Idle, Processing)
                | (Idle, WaitingApproval)
                | (Idle, Compacting)
                | (Processing, WaitingApproval)
                | (Processing, WaitingInput)
                | (Processing, Done)
                | (Processing, Error)
                | (Processing, Interrupted)
                | (Processing, Compacting)
                | (Processing, Idle)
                | (WaitingApproval, Processing)
                | (WaitingApproval, Idle)
                | (WaitingInput, Processing)
                | (Compacting, Processing)
                | (Done, Idle)
                | (Done, Processing)
                | (Error, Idle)
                | (Error, Processing)
                | (Interrupted, Idle)
                | (Interrupted, Processing)
                | (_, Idle) // Reset to Idle is always valid
        )
    }

    /// Returns true when this phase requires user attention.
    pub fn needs_attention(&self) -> bool {
        matches!(self, Self::WaitingApproval | Self::WaitingInput | Self::Error)
    }

    /// Returns true when the session is actively doing something.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Processing | Self::WaitingApproval | Self::WaitingInput | Self::Compacting
        )
    }
}

impl Default for SessionPhase {
    fn default() -> Self {
        Self::Idle
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

/// Pending permission request details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPermission {
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
}

/// Information about an active subagent (nested agent spawned via Task tool)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentInfo {
    pub agent_id: String,
    pub description: String,
    pub started_at: i64,
    pub status: String, // "running", "completed", "error"
    pub tools: Vec<String>, // last few tool names used by this subagent
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
}

/// State of a single agent session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub id: String,
    pub agent_type: String,
    pub project: String,
    pub cwd: String,
    pub terminal: String,
    pub phase: SessionPhase,
    pub started_at: i64,
    pub duration: i64,
    pub tokens: TokenUsage,
    pub pending_permission: Option<PendingPermission>,
    pub pending_question: Option<PendingQuestion>,
    pub last_tool_name: Option<String>,
    pub last_tool_target: Option<String>,
    pub last_tool_status: Option<String>,
    pub description: Option<String>,
    pub session_title: Option<String>,
    pub pid: Option<u32>,
    pub tty: Option<String>,
    pub subagents: Vec<SubagentInfo>,
    pub active_tools: Vec<ToolResult>,
    pub last_response: Option<String>,
    pub last_thought: Option<String>,
}

impl SessionState {
    pub fn new(id: String, agent_type: String, project: String, cwd: String, terminal: String) -> Self {
        Self {
            id,
            agent_type,
            project,
            cwd,
            terminal,
            phase: SessionPhase::Idle,
            started_at: Utc::now().timestamp(),
            duration: 0,
            tokens: TokenUsage::default(),
            pending_permission: None,
            pending_question: None,
            last_tool_name: None,
            last_tool_target: None,
            last_tool_status: None,
            description: None,
            session_title: None,
            pid: None,
            tty: None,
            subagents: Vec::new(),
            active_tools: Vec::new(),
            last_response: None,
            last_thought: None,
        }
    }

    /// Update duration based on current time
    pub fn update_duration(&mut self) {
        self.duration = Utc::now().timestamp() - self.started_at;
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
        let entry = self.sessions.entry(session_id.to_string()).or_insert_with(|| {
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
            session.update_duration();
        }
        self.emit_update();
    }

    /// Update a session with a full state replacement
    pub fn update_session(&self, session_id: &str, updater: impl FnOnce(&mut SessionState)) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            updater(&mut session);
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

    /// Accumulate token usage for a session
    pub fn add_tokens(&self, session_id: &str, input: u64, output: u64, cache_read: u64, cache_create: u64) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.tokens.input += input;
            session.tokens.output += output;
            session.tokens.cache_read += cache_read;
            session.tokens.cache_create += cache_create;
        }
        self.emit_update();
    }

    /// Remove a session (on session end)
    pub fn remove_session(&self, session_id: &str) {
        self.sessions.remove(session_id);
        self.emit_update();
    }

    /// Add or update a subagent for a session
    pub fn add_subagent(&self, session_id: &str, agent_id: &str, description: &str) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            // Check if subagent already exists (update it)
            if let Some(existing) = session.subagents.iter_mut().find(|s| s.agent_id == agent_id) {
                existing.status = "running".to_string();
                if !description.is_empty() {
                    existing.description = description.to_string();
                }
            } else {
                session.subagents.push(SubagentInfo {
                    agent_id: agent_id.to_string(),
                    description: description.to_string(),
                    started_at: Utc::now().timestamp(),
                    status: "running".to_string(),
                    tools: Vec::new(),
                });
            }
            session.update_duration();
        }
        self.emit_update();
    }

    /// Stop a subagent (mark as completed, keep in list for frontend display)
    pub fn stop_subagent(&self, session_id: &str, agent_id: &str, status: &str) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            if let Some(existing) = session.subagents.iter_mut().find(|s| s.agent_id == agent_id) {
                existing.status = status.to_string();
            }
            session.update_duration();
        }
        self.emit_update();
    }

    /// Track a tool starting (PreToolUse)
    pub fn start_tool(&self, session_id: &str, tool_use_id: &str, tool_name: &str) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            // Don't add duplicate entries
            if !session.active_tools.iter().any(|t| t.tool_use_id == tool_use_id) {
                session.active_tools.push(ToolResult {
                    tool_use_id: tool_use_id.to_string(),
                    tool_name: tool_name.to_string(),
                    status: "running".to_string(),
                    started_at: Utc::now().timestamp(),
                    completed_at: None,
                    error: None,
                });
                // Keep only last 20 tools
                if session.active_tools.len() > 20 {
                    session.active_tools.remove(0);
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
    pub fn complete_tool(&self, session_id: &str, tool_use_id: &str, success: bool, error: Option<String>) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            if let Some(tool) = session.active_tools.iter_mut().find(|t| t.tool_use_id == tool_use_id) {
                tool.status = if success { "success".to_string() } else { "error".to_string() };
                tool.completed_at = Some(Utc::now().timestamp());
                tool.error = error;
            }
            session.update_duration();
        }
        self.emit_update();
    }

    /// Get all sessions as a vector
    pub fn get_all_sessions(&self) -> Vec<SessionState> {
        self.sessions
            .iter()
            .map(|entry| {
                let mut s = entry.value().clone();
                s.update_duration();
                s
            })
            .collect()
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
            let payload = SessionUpdatePayload { sessions, suppressed };
            if let Err(e) = handle.emit("session-update", &payload) {
                log::error!("Failed to emit session-update: {}", e);
            }
        }
    }
}
