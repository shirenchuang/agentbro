// ToolProcessor — Tracks active tools per session and caches recent results
// Used for correlating PreToolUse → PostToolUse and PermissionRequest → tool_use_id

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::session_store::SessionStore;

/// Information about a currently-active tool invocation
#[derive(Debug, Clone)]
pub struct ActiveTool {
    pub tool_use_id: String,
    pub tool_name: String,
    pub session_id: String,
    pub started_at: i64,
}

/// The ToolProcessor maintains a map of in-flight tools and delegates
/// lifecycle updates to the SessionStore.
pub struct ToolProcessor {
    /// tool_use_id → ActiveTool for currently-running tools
    active: Arc<RwLock<HashMap<String, ActiveTool>>>,
    /// session store reference
    store: Arc<SessionStore>,
}

impl ToolProcessor {
    pub fn new(store: Arc<SessionStore>) -> Self {
        Self {
            active: Arc::new(RwLock::new(HashMap::new())),
            store,
        }
    }

    /// Called on PreToolUse — register a new active tool
    pub fn on_pre_tool_use(&self, session_id: &str, tool_use_id: &str, tool_name: &str) {
        if tool_use_id.is_empty() {
            return;
        }

        let tool = ActiveTool {
            tool_use_id: tool_use_id.to_string(),
            tool_name: tool_name.to_string(),
            session_id: session_id.to_string(),
            started_at: chrono::Utc::now().timestamp(),
        };

        {
            let mut active = self.active.write().unwrap();
            active.insert(tool_use_id.to_string(), tool);
        }

        self.store.start_tool(session_id, tool_use_id, tool_name);
    }

    /// Called on PostToolUse — mark tool as successfully completed
    pub fn on_post_tool_use(&self, session_id: &str, tool_use_id: &str) {
        if tool_use_id.is_empty() {
            return;
        }

        {
            let mut active = self.active.write().unwrap();
            active.remove(tool_use_id);
        }

        self.store.complete_tool(session_id, tool_use_id, true, None);
    }

    /// Called on PostToolUseFailure — mark tool as failed
    pub fn on_post_tool_use_failure(&self, session_id: &str, tool_use_id: &str, error: Option<String>) {
        if tool_use_id.is_empty() {
            return;
        }

        {
            let mut active = self.active.write().unwrap();
            active.remove(tool_use_id);
        }

        self.store.complete_tool(session_id, tool_use_id, false, error);
    }

    /// Look up the tool_use_id for a PermissionRequest correlation.
    /// Returns the most recently started tool for this session if tool_use_id
    /// is not explicitly provided in the permission event.
    pub fn find_active_tool_for_session(&self, session_id: &str) -> Option<ActiveTool> {
        let active = self.active.read().unwrap();
        active
            .values()
            .filter(|t| t.session_id == session_id)
            .max_by_key(|t| t.started_at)
            .cloned()
    }

    /// Remove all active tools for a session (on session end)
    pub fn clear_session(&self, session_id: &str) {
        let mut active = self.active.write().unwrap();
        active.retain(|_, t| t.session_id != session_id);
    }

    /// Get count of active tools (for diagnostics)
    pub fn active_count(&self) -> usize {
        self.active.read().unwrap().len()
    }
}
