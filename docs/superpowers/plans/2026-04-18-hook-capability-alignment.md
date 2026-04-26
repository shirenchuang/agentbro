# Hook Capability Alignment with Vibe Island Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add missing hook handlers to match Vibe Island's capability - specifically beforeShellExecution, afterShellExecution, beforeMCPExecution, afterMCPExecution, afterAgentResponse, and afterAgentThought.

**Architecture:** Extend the existing hook system in src-tauri/src/bridge/main.rs to handle new event types, add corresponding AgentEvent variants in src-tauri/src/agents/mod.rs, and update src-tauri/src/hooks/server.rs to process these new event types.

**Tech Stack:** Rust (Tauri bridge), Claude Code hooks, Unix Socket/TCP communication

---

## File Structure

```
src-tauri/src/
├── bridge/main.rs          # Modify: Add new hook event handlers
├── agents/mod.rs           # Modify: Add new AgentEvent variants
├── hooks/server.rs         # Modify: Add processing logic for new events
└── hooks/session_store.rs  # Modify: Add state tracking for shell/MCP/agent events
```

---

## Task 1: Add New AgentEvent Variants

**Files:**
- Modify: `src-tauri/src/agents/mod.rs:16-76`

- [ ] **Step 1: Add new AgentEvent variants for shell, MCP, and agent response hooks**

Add these variants to the `AgentEvent` enum after `SubagentStop`:

```rust
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
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/agents/mod.rs
git commit -m "feat: add AgentEvent variants for shell/MCP/agent hooks"
```

---

## Task 2: Update Bridge to Handle New Hook Events

**Files:**
- Modify: `src-tauri/src/bridge/main.rs:101-261`

- [ ] **Step 1: Add new hook event handlers in main() match statement**

Find the `_ =>` default case (around line 254) and add cases before it:

```rust
        "beforeShellExecution" => {
            obj.insert("status".into(), "shell_starting".into());
            if let Some(cmd) = data.get("tool_input").and_then(|v| v.get("command")) {
                obj.insert("command".into(), cmd.clone());
            }
        }
        "afterShellExecution" => {
            obj.insert("status".into(), "shell_completed".into());
            if let Some(cmd) = data.get("tool_input").and_then(|v| v.get("command")) {
                obj.insert("command".into(), cmd.clone());
            }
            if let Some(result) = data.get("tool_result") {
                obj.insert("stdout".into(), result.get("stdout").cloned().unwrap_or_default());
                obj.insert("stderr".into(), result.get("stderr").cloned().unwrap_or_default());
                obj.insert("exit_code".into(), result.get("exit_code").cloned().unwrap_or_default());
            }
        }
        "beforeMCPExecution" => {
            obj.insert("status".into(), "mcp_starting".into());
            if let Some(tool) = data.get("tool_name") {
                obj.insert("mcp_tool".into(), tool.clone());
            }
            obj.insert("mcp_arguments".into(), tool_input);
        }
        "afterMCPExecution" => {
            obj.insert("status".into(), "mcp_completed".into());
            if let Some(tool) = data.get("tool_name") {
                obj.insert("mcp_tool".into(), tool.clone());
            }
            if let Some(result) = data.get("tool_result") {
                obj.insert("mcp_result".into(), result.clone());
            }
        }
        "afterAgentResponse" => {
            obj.insert("status".into(), "response_received".into());
            if let Some(text) = data.get("text").or_else(|| data.get("content")) {
                obj.insert("response_content".into(), text.clone());
            }
            if let Some(content_type) = data.get("content_type") {
                obj.insert("content_type".into(), content_type.clone());
            }
        }
        "afterAgentThought" => {
            obj.insert("status".into(), "thought_processed".into());
            if let Some(thought) = data.get("thought").or_else(|| data.get("reasoning")) {
                obj.insert("thought_content".into(), thought.clone());
            }
        }
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/bridge/main.rs
git commit -m "feat: add hook handlers for shell/MCP/agent events"
```

---

## Task 3: Update Hook Server to Process New Events

**Files:**
- Modify: `src-tauri/src/hooks/server.rs:350-482`

- [ ] **Step 1: Add adapter parsing for new event types in parse_with_adapters**

Update the `parse_with_adapters` function to handle the new event types. The function already iterates through adapters, but we need to ensure the claude_code adapter can parse these new events.

- [ ] **Step 2: Add process_event cases for new AgentEvent variants**

Add cases to `process_event` function for the new event types:

```rust
            AgentEvent::ShellExecutionStart { session_id, command, cwd } => {
                log::debug!("Shell starting: {} in {}", command, cwd);
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Processing;
                    s.last_tool_name = Some(format!("shell:{}", command));
                    s.last_tool_status = Some("running".to_string());
                });
            }
            AgentEvent::ShellExecutionEnd { session_id, command, exit_code, stdout, stderr, duration_ms } => {
                log::debug!("Shell completed: {} (exit={:?}, {}ms)", command, exit_code, duration_ms);
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Processing;
                    s.last_tool_status = Some(if exit_code == Some(0) { "success".to_string() } else { "error".to_string() });
                });
                // Play completion sound
                if exit_code == Some(0) {
                    Self::play_sound_for_session(sound, store, session_id, SoundEvent::TaskComplete);
                }
            }
            AgentEvent::MCPExecutionStart { session_id, server_name, tool_name, arguments } => {
                log::debug!("MCP {}:{} starting", server_name, tool_name);
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Processing;
                    s.last_tool_name = Some(format!("mcp:{}:{}", server_name, tool_name));
                    s.last_tool_status = Some("running".to_string());
                });
            }
            AgentEvent::MCPExecutionEnd { session_id, server_name, tool_name, result, error, duration_ms } => {
                log::debug!("MCP {}:{} completed ({}ms)", server_name, tool_name, duration_ms);
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Processing;
                    s.last_tool_status = Some(if error.is_none() { "success".to_string() } else { "error".to_string() });
                });
            }
            AgentEvent::AgentResponse { session_id, content, content_type } => {
                log::debug!("Agent response received: {} bytes, type={}", content.len(), content_type);
                store.update_session(session_id, |s| {
                    s.phase = SessionPhase::Processing;
                    s.last_response = Some(content.clone());
                });
            }
            AgentEvent::AgentThought { session_id, thought } => {
                log::debug!("Agent thought: {} chars", thought.len());
                store.update_session(session_id, |s| {
                    s.last_thought = Some(thought.clone());
                });
            }
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/hooks/server.rs
git commit -m "feat: process shell/MCP/agent hook events in server"
```

---

## Task 4: Update Session Store to Track New State

**Files:**
- Modify: `src-tauri/src/hooks/session_store.rs`

- [ ] **Step 1: Add new fields to Session struct**

Find the Session struct and add:
```rust
pub last_response: Option<String>,
pub last_thought: Option<String>,
pub shell_history: Vec<ShellEntry>,
pub mcp_history: Vec<MCPEntry>,
```

Where:
```rust
#[derive(Debug, Clone)]
pub struct ShellEntry {
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub duration_ms: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct MCPEntry {
    pub server_name: String,
    pub tool_name: String,
    pub arguments: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/hooks/session_store.rs
git commit -m "feat: add state tracking for shell/MCP/agent events"
```

---

## Task 5: Add Settings.json Hook Configuration

**Files:**
- Modify: `~/.claude/settings.json` (or create new hook entries)

- [ ] **Step 1: Verify new hooks are registered**

The current settings.json already has hooks for agent-island. We need to ensure the new hook types are also registered. Add to the hooks configuration:

```json
"beforeShellExecution": [
  {
    "hooks": [
      {
        "command": "/Users/shirenchuang/.agent-island/bin/agent-island-bridge",
        "type": "command"
      }
    ],
    "matcher": "*"
  }
],
"afterShellExecution": [
  {
    "hooks": [
      {
        "command": "/Users/shirenchuang/.agent-island/bin/agent-island-bridge",
        "type": "command"
      }
    ],
    "matcher": "*"
  }
],
"beforeMCPExecution": [
  {
    "hooks": [
      {
        "command": "/Users/shirenchuang/.agent-island/bin/agent-island-bridge",
        "type": "command"
      }
    ],
    "matcher": "mcp__.*"
  }
],
"afterMCPExecution": [
  {
    "hooks": [
      {
        "command": "/Users/shirenchuang/.agent-island/bin/agent-island-bridge",
        "type": "command"
      }
    ],
    "matcher": "mcp__.*"
  }
],
"afterAgentResponse": [
  {
    "hooks": [
      {
        "command": "/Users/shirenchuang/.agent-island/bin/agent-island-bridge",
        "type": "command"
      }
    ],
    "matcher": "*"
  }
],
"afterAgentThought": [
  {
    "hooks": [
      {
        "command": "/Users/shirenchuang/.agent-island/bin/agent-island-bridge",
        "type": "command"
      }
    ],
    "matcher": "*"
  }
],
```

- [ ] **Step 2: Commit**

```bash
git add -A  # if any config files changed
git commit -m "feat: register new hook types in settings"
```

---

## Task 6: Build and Test

**Files:**
- Test: Full application build and manual testing

- [ ] **Step 1: Build the Tauri application**

```bash
cd src-tauri
cargo build --release
```

- [ ] **Step 2: Start the application and trigger new hooks**

Test by:
1. Starting a Claude Code session
2. Running a shell command (triggers beforeShellExecution/afterShellExecution)
3. Using an MCP tool (triggers beforeMCPExecution/afterMCPExecution)
4. Getting an agent response (triggers afterAgentResponse)

- [ ] **Step 3: Verify UI reflects new event states**

Check that:
- Shell execution shows in the panel
- MCP tool calls are displayed
- Agent responses are captured

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: complete hook capability alignment with Vibe Island"
```

---

## Summary

This plan adds 6 new hook types to match Vibe Island's capabilities:

| Hook | Purpose | Complexity |
|------|---------|------------|
| beforeShellExecution | Monitor shell command start | Low |
| afterShellExecution | Capture shell results | Medium |
| beforeMCPExecution | Monitor MCP tool start | Low |
| afterMCPExecution | Capture MCP tool results | Medium |
| afterAgentResponse | Capture AI responses | High |
| afterAgentThought | Capture AI reasoning | High |

**Estimated total tasks:** 15-20 steps across 6 tasks