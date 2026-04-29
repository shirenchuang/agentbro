//! Agent Island Hook Bridge
//!
//! A lightweight compiled binary that Claude Code hooks call.
//! Reads JSON from stdin, forwards events to Agent Island via Unix socket or TCP.
//! For PermissionRequest events, waits for a response and outputs it.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::time::Duration;

const SOCKET_PATH: &str = "/tmp/agent-island.sock";
const TCP_ADDR: &str = "127.0.0.1:17892";
const TIMEOUT_SECONDS: u64 = 300;

/// Polymorphic stream: Unix socket or TCP
enum Stream {
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Unix(s) => s.read(buf),
            Stream::Tcp(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Unix(s) => s.write(buf),
            Stream::Tcp(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Unix(s) => s.flush(),
            Stream::Tcp(s) => s.flush(),
        }
    }
}

impl Stream {
    fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        match self {
            Stream::Unix(s) => s.set_read_timeout(dur),
            Stream::Tcp(s) => s.set_read_timeout(dur),
        }
    }
}

/// Get the TTY of the parent Claude process
fn get_tty() -> Option<String> {
    let ppid = std::os::unix::process::parent_id();
    if let Ok(output) = std::process::Command::new("ps")
        .args(["-p", &ppid.to_string(), "-o", "tty="])
        .output()
    {
        let tty = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !tty.is_empty() && tty != "??" && tty != "-" {
            return Some(if tty.starts_with("/dev/") {
                tty
            } else {
                format!("/dev/{}", tty)
            });
        }
    }
    None
}

/// Connect to Agent Island: try Unix socket first, fall back to TCP
fn connect() -> Option<Stream> {
    if let Ok(s) = UnixStream::connect(SOCKET_PATH) {
        return Some(Stream::Unix(s));
    }
    if let Ok(s) = TcpStream::connect(TCP_ADDR) {
        return Some(Stream::Tcp(s));
    }
    None
}

fn send_and_maybe_receive(state: &serde_json::Value, wait_response: bool) -> Option<serde_json::Value> {
    let mut stream = connect()?;
    stream.set_read_timeout(Some(Duration::from_secs(TIMEOUT_SECONDS))).ok();
    let payload = format!("{}\n", state);
    stream.write_all(payload.as_bytes()).ok()?;

    if wait_response {
        let mut reader = BufReader::new(&mut stream);
        let mut response = String::new();
        if reader.read_line(&mut response).is_ok() && !response.is_empty() {
            return serde_json::from_str(&response).ok();
        }
    }
    None
}

fn main() {
    // Read all stdin
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return;
    }

    // Parse stdin JSON
    let data: serde_json::Value = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(_) => return,
    };

    let session_id = data["session_id"].as_str().unwrap_or("unknown");
    let hook_event = data["hook_event_name"].as_str().unwrap_or("");
    let cwd = data["cwd"].as_str().unwrap_or("");
    let tool_input = data.get("tool_input").cloned().unwrap_or(serde_json::json!({}));
    let claude_pid = std::os::unix::process::parent_id();
    let tty = get_tty();

    // Build event state matching what the Python script produced
    let mut state = serde_json::json!({
        "agent": "claude-code",
        "session_id": session_id,
        "cwd": cwd,
        "event": hook_event,
        "pid": claude_pid,
        "tty": tty,
    });

    let obj = state.as_object_mut().unwrap();

    match hook_event {
        "UserPromptSubmit" => {
            obj.insert("status".into(), "processing".into());
            // Forward user prompt text for session title extraction
            if let Some(prompt) = data.get("user_prompt").or_else(|| data.get("prompt")) {
                obj.insert("prompt".into(), prompt.clone());
            }
        }
        "PreToolUse" => {
            obj.insert("status".into(), "running_tool".into());
            if let Some(t) = data.get("tool_name") { obj.insert("tool".into(), t.clone()); }
            obj.insert("tool_input".into(), tool_input);
            if let Some(id) = data.get("tool_use_id") { obj.insert("tool_use_id".into(), id.clone()); }
        }
        "PostToolUse" => {
            obj.insert("status".into(), "processing".into());
            if let Some(t) = data.get("tool_name") { obj.insert("tool".into(), t.clone()); }
            obj.insert("tool_input".into(), tool_input);
            if let Some(id) = data.get("tool_use_id") { obj.insert("tool_use_id".into(), id.clone()); }
        }
        "PostToolUseFailure" => {
            obj.insert("status".into(), "processing".into());
            if let Some(t) = data.get("tool_name") { obj.insert("tool".into(), t.clone()); }
            obj.insert("tool_input".into(), tool_input);
            if let Some(e) = data.get("error").or_else(|| data.get("message")) {
                obj.insert("tool_error".into(), e.clone());
            }
            if let Some(id) = data.get("tool_use_id") { obj.insert("tool_use_id".into(), id.clone()); }
        }
        "PermissionDenied" => {
            obj.insert("status".into(), "processing".into());
            if let Some(t) = data.get("tool_name") { obj.insert("tool".into(), t.clone()); }
            obj.insert("tool_input".into(), tool_input);
            if let Some(r) = data.get("reason").or_else(|| data.get("message")) {
                obj.insert("denial_reason".into(), r.clone());
            }
        }
        "PermissionRequest" => {
            obj.insert("status".into(), "waiting_for_approval".into());
            if let Some(t) = data.get("tool_name") { obj.insert("tool".into(), t.clone()); }
            obj.insert("tool_input".into(), tool_input);

            if let Some(resp) = send_and_maybe_receive(&state, true) {
                let decision = resp["decision"].as_str().unwrap_or("ask");
                let reason = resp["reason"].as_str().unwrap_or("");

                if decision == "allow" {
                    let output = serde_json::json!({
                        "hookSpecificOutput": {
                            "hookEventName": "PermissionRequest",
                            "decision": { "behavior": "allow" }
                        }
                    });
                    println!("{}", output);
                } else if decision == "deny" {
                    let msg = if reason.is_empty() { "Denied by user via Agent Island" } else { reason };
                    let output = serde_json::json!({
                        "hookSpecificOutput": {
                            "hookEventName": "PermissionRequest",
                            "decision": {
                                "behavior": "deny",
                                "message": msg
                            }
                        }
                    });
                    println!("{}", output);
                }
            }
            return;
        }
        "Notification" => {
            let notification_type = data["notification_type"].as_str().unwrap_or("");
            if notification_type == "permission_prompt" {
                return;
            } else if notification_type == "idle_prompt" {
                obj.insert("status".into(), "waiting_for_input".into());
            } else {
                obj.insert("status".into(), "notification".into());
            }
            obj.insert("notification_type".into(), notification_type.into());
            if let Some(msg) = data.get("message") { obj.insert("message".into(), msg.clone()); }
        }
        "Stop" => {
            obj.insert("status".into(), "waiting_for_input".into());
        }
        "StopFailure" => {
            obj.insert("status".into(), "waiting_for_input".into());
            if let Some(e) = data.get("error").or_else(|| data.get("message")) {
                obj.insert("stop_error".into(), e.clone());
            }
        }
        "SessionStart" => {
            obj.insert("status".into(), "waiting_for_input".into());
        }
        "SessionEnd" => {
            obj.insert("status".into(), "ended".into());
        }
        "PreCompact" => {
            obj.insert("status".into(), "compacting".into());
        }
        "PostCompact" => {
            obj.insert("status".into(), "processing".into());
        }
        "SubagentStart" => {
            obj.insert("status".into(), "processing".into());
            let agent_id = data.get("agent_id").or_else(|| data.get("tool_use_id"))
                .cloned().unwrap_or_else(|| "unknown".into());
            obj.insert("agent_id".into(), agent_id);
            let desc = data.get("description").or_else(|| data.get("message"))
                .cloned().unwrap_or_else(|| "".into());
            obj.insert("description".into(), desc);
        }
        "SubagentStop" => {
            obj.insert("status".into(), "processing".into());
            let agent_id = data.get("agent_id").or_else(|| data.get("tool_use_id"))
                .cloned().unwrap_or_else(|| "unknown".into());
            obj.insert("agent_id".into(), agent_id);
            let agent_status = data.get("agent_status")
                .cloned().unwrap_or_else(|| "completed".into());
            obj.insert("agent_status".into(), agent_status);
        }
        "beforeShellExecution" => {
            obj.insert("status".into(), "shell_starting".into());
            if let Some(cmd) = data.get("tool_input").and_then(|v| v.get("command")) {
                obj.insert("command".into(), cmd.clone());
            }
            if let Some(cwd_val) = data.get("cwd") {
                obj.insert("shell_cwd".into(), cwd_val.clone());
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
            if let Some(dur) = data.get("duration_ms") {
                obj.insert("duration_ms".into(), dur.clone());
            }
        }
        "beforeMCPExecution" => {
            obj.insert("status".into(), "mcp_starting".into());
            if let Some(tool) = data.get("tool_name") {
                obj.insert("mcp_tool".into(), tool.clone());
            }
            if let Some(server) = data.get("server_name") {
                obj.insert("mcp_server".into(), server.clone());
            }
            obj.insert("mcp_arguments".into(), tool_input.clone());
        }
        "afterMCPExecution" => {
            obj.insert("status".into(), "mcp_completed".into());
            if let Some(tool) = data.get("tool_name") {
                obj.insert("mcp_tool".into(), tool.clone());
            }
            if let Some(server) = data.get("server_name") {
                obj.insert("mcp_server".into(), server.clone());
            }
            if let Some(result) = data.get("tool_result") {
                obj.insert("mcp_result".into(), result.clone());
            }
            if let Some(err) = data.get("error") {
                obj.insert("mcp_error".into(), err.clone());
            }
            if let Some(dur) = data.get("duration_ms") {
                obj.insert("duration_ms".into(), dur.clone());
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
        _ => {
            obj.insert("status".into(), "unknown".into());
        }
    }

    // Send event (non-PermissionRequest path)
    send_and_maybe_receive(&state, false);
}
