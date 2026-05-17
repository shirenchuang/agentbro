//! AgentBro Hook Bridge
//!
//! A lightweight compiled binary that Claude Code hooks call.
//! Reads JSON from stdin, forwards events to AgentBro via Unix socket or TCP.
//! For PermissionRequest events, waits for a response and outputs it.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::time::Duration;

const SOCKET_PATH: &str = "/tmp/agentbro.sock";
const TCP_ADDR: &str = "127.0.0.1:17892";
const TIMEOUT_SECONDS: u64 = 21_600;

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

/// Connect to AgentBro: try Unix socket first, fall back to TCP
fn connect() -> Option<Stream> {
    if let Ok(s) = UnixStream::connect(SOCKET_PATH) {
        return Some(Stream::Unix(s));
    }
    if let Ok(s) = TcpStream::connect(TCP_ADDR) {
        return Some(Stream::Tcp(s));
    }
    None
}

fn send_and_maybe_receive(
    state: &serde_json::Value,
    wait_response: bool,
) -> Option<serde_json::Value> {
    let mut stream = connect()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(TIMEOUT_SECONDS)))
        .ok();
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

fn copy_optional_field(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    data: &serde_json::Value,
    target_key: &str,
    source_keys: &[&str],
) {
    if let Some(value) = source_keys.iter().find_map(|key| data.get(key)) {
        obj.insert(target_key.into(), value.clone());
    }
}

fn arg_value(flag: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next();
        }
    }
    None
}

fn string_field<'a>(data: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| data.get(key).and_then(|value| value.as_str()))
}

fn normalize_hook_event(event: &str) -> &str {
    match event {
        "session_start" => "SessionStart",
        "session_end" => "SessionEnd",
        "user_prompt_submit" => "UserPromptSubmit",
        "pre_tool_use" => "PreToolUse",
        "post_tool_use" => "PostToolUse",
        "post_tool_use_failure" => "PostToolUseFailure",
        "permission_request" => "PermissionRequest",
        "permission_denied" => "PermissionDenied",
        "stop" => "Stop",
        "stop_failure" => "StopFailure",
        other => other,
    }
}

fn main() {
    let source = arg_value("--source")
        .or_else(|| std::env::var("AGENTBRO_AGENT").ok())
        .unwrap_or_else(|| "claude-code".to_string());
    let forced_event = arg_value("--event");

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

    let session_id = string_field(&data, &["session_id", "sessionId"]).unwrap_or("unknown");
    let hook_event = forced_event
        .as_deref()
        .or_else(|| string_field(&data, &["hook_event_name", "event", "hookType"]))
        .map(normalize_hook_event)
        .unwrap_or("");
    let cwd = string_field(&data, &["cwd"]).unwrap_or("");
    let tool_input = data
        .get("tool_input")
        .or_else(|| data.get("toolInput"))
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let claude_pid = std::os::unix::process::parent_id();
    let tty = get_tty();
    let engine_label = std::env::var("AGENTBRO_ENGINE_LABEL").ok();
    let engine_config_root = std::env::var("AGENTBRO_CONFIG_ROOT").ok();
    let term_program = std::env::var("TERM_PROGRAM").ok();
    let term_bundle_id = std::env::var("__CFBundleIdentifier").ok();
    let wezterm_pane = std::env::var("WEZTERM_PANE").ok();
    let zellij = std::env::var("ZELLIJ").ok();
    let zellij_pane_id = std::env::var("ZELLIJ_PANE_ID").ok();
    let zellij_session_name = std::env::var("ZELLIJ_SESSION_NAME").ok();
    let cmux_surface_id = std::env::var("CMUX_SURFACE_ID").ok();
    let cmux_workspace_id = std::env::var("CMUX_WORKSPACE_ID").ok();

    // Build event state matching what the Python script produced
    let mut state = serde_json::json!({
        "agent": source,
        "session_id": session_id,
        "cwd": cwd,
        "event": hook_event,
        "pid": claude_pid,
        "tty": tty,
    });

    let obj = state.as_object_mut().unwrap();
    if let Some(label) = engine_label {
        obj.insert("engine_label".into(), label.into());
    }
    if let Some(root) = engine_config_root {
        obj.insert("engine_config_root".into(), root.into());
    }
    if let Some(program) = term_program {
        obj.insert("_term_program".into(), program.into());
    }
    if let Some(bundle_id) = term_bundle_id {
        obj.insert("_term_bundle_id".into(), bundle_id.into());
    }
    if let Some(pane) = wezterm_pane {
        obj.insert("_wezterm_pane".into(), pane.into());
    }
    if let Some(value) = zellij {
        obj.insert("_zellij".into(), value.into());
    }
    if let Some(pane_id) = zellij_pane_id {
        obj.insert("_zellij_pane_id".into(), pane_id.into());
    }
    if let Some(session_name) = zellij_session_name {
        obj.insert("_zellij_session_name".into(), session_name.into());
    }
    if let Some(surface_id) = cmux_surface_id {
        obj.insert("_cmux_surface_id".into(), surface_id.into());
    }
    if let Some(workspace_id) = cmux_workspace_id {
        obj.insert("_cmux_workspace_id".into(), workspace_id.into());
    }
    if let Some(transcript_path) = data
        .get("transcript_path")
        .or_else(|| data.get("transcriptPath"))
    {
        obj.insert("transcript_path".into(), transcript_path.clone());
    }

    if data.get("rate_limits").is_some() || data.get("context_window").is_some() {
        obj.insert("event".into(), "StatusLineUpdate".into());
        obj.insert("status".into(), "processing".into());

        if let Some(rl) = data.get("rate_limits") {
            obj.insert(
                "rateLimits".into(),
                serde_json::json!({
                    "fiveHour": {
                        "usedPercentage": rl
                            .get("five_hour")
                            .and_then(|v| v.get("used_percentage"))
                            .cloned()
                            .unwrap_or(serde_json::json!(0)),
                        "resetsAt": rl
                            .get("five_hour")
                            .and_then(|v| v.get("resets_at"))
                            .cloned()
                            .unwrap_or(serde_json::json!(0))
                    },
                    "sevenDay": {
                        "usedPercentage": rl
                            .get("seven_day")
                            .and_then(|v| v.get("used_percentage"))
                            .cloned()
                            .unwrap_or(serde_json::json!(0)),
                        "resetsAt": rl
                            .get("seven_day")
                            .and_then(|v| v.get("resets_at"))
                            .cloned()
                            .unwrap_or(serde_json::json!(0))
                    }
                }),
            );
        }
        if let Some(cw) = data.get("context_window") {
            obj.insert(
                "contextWindow".into(),
                serde_json::json!({
                    "totalInputTokens": cw
                        .get("total_input_tokens")
                        .cloned()
                        .unwrap_or(serde_json::json!(0)),
                    "totalOutputTokens": cw
                        .get("total_output_tokens")
                        .cloned()
                        .unwrap_or(serde_json::json!(0)),
                    "contextWindowSize": cw
                        .get("context_window_size")
                        .cloned()
                        .unwrap_or(serde_json::json!(0)),
                    "usedPercentage": cw.get("used_percentage").cloned().unwrap_or(serde_json::Value::Null)
                }),
            );
        }
        if let Some(text) = data
            .get("status_line_text")
            .or_else(|| data.get("statusLineText"))
        {
            obj.insert("statusLineText".into(), text.clone());
        }
        send_and_maybe_receive(&state, false);
        return;
    }

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
            if let Some(t) = data.get("tool_name").or_else(|| data.get("tool")) {
                obj.insert("tool".into(), t.clone());
            }
            obj.insert("tool_input".into(), tool_input);
            if let Some(id) = data.get("tool_use_id").or_else(|| data.get("toolUseId")) {
                obj.insert("tool_use_id".into(), id.clone());
            }
        }
        "PostToolUse" => {
            obj.insert("status".into(), "processing".into());
            if let Some(t) = data.get("tool_name").or_else(|| data.get("tool")) {
                obj.insert("tool".into(), t.clone());
            }
            obj.insert("tool_input".into(), tool_input);
            if let Some(id) = data.get("tool_use_id").or_else(|| data.get("toolUseId")) {
                obj.insert("tool_use_id".into(), id.clone());
            }
        }
        "PostToolUseFailure" => {
            obj.insert("status".into(), "processing".into());
            if let Some(t) = data.get("tool_name").or_else(|| data.get("tool")) {
                obj.insert("tool".into(), t.clone());
            }
            obj.insert("tool_input".into(), tool_input);
            if let Some(e) = data.get("error").or_else(|| data.get("message")) {
                obj.insert("tool_error".into(), e.clone());
            }
            if let Some(id) = data.get("tool_use_id").or_else(|| data.get("toolUseId")) {
                obj.insert("tool_use_id".into(), id.clone());
            }
        }
        "PermissionDenied" => {
            obj.insert("status".into(), "processing".into());
            if let Some(t) = data.get("tool_name").or_else(|| data.get("tool")) {
                obj.insert("tool".into(), t.clone());
            }
            obj.insert("tool_input".into(), tool_input);
            if let Some(r) = data.get("reason").or_else(|| data.get("message")) {
                obj.insert("denial_reason".into(), r.clone());
            }
        }
        "PermissionRequest" => {
            let tool_name_str = data
                .get("tool_name")
                .or_else(|| data.get("tool"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // AskUserQuestion: route as interactive question card instead of permission dialog
            if tool_name_str == "AskUserQuestion" {
                let questions = tool_input
                    .get("questions")
                    .cloned()
                    .unwrap_or(serde_json::json!([]));
                let first_q = questions.as_array().and_then(|arr| arr.first());

                let question_text = first_q
                    .map(|q| {
                        let header = q.get("header").and_then(|h| h.as_str()).unwrap_or("");
                        let text = q.get("question").and_then(|t| t.as_str()).unwrap_or("");
                        if header.is_empty() {
                            text.to_string()
                        } else {
                            format!("[{}] {}", header, text)
                        }
                    })
                    .unwrap_or_default();

                let options: Vec<String> = first_q
                    .and_then(|q| q.get("options"))
                    .and_then(|o| o.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|opt| {
                                opt.get("label")
                                    .and_then(|l| l.as_str())
                                    .map(|s| s.to_string())
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let descriptions: Vec<String> = first_q
                    .and_then(|q| q.get("options"))
                    .and_then(|o| o.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|opt| {
                                opt.get("description")
                                    .and_then(|d| d.as_str())
                                    .map(|s| s.to_string())
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let header = first_q
                    .and_then(|q| q.get("header"))
                    .and_then(|h| h.as_str())
                    .map(|s| s.to_string());

                let multi_select = first_q
                    .and_then(|q| q.get("multiSelect"))
                    .and_then(|m| m.as_bool())
                    .unwrap_or(false);
                let normalized_questions: Vec<serde_json::Value> = questions
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .map(|q| {
                                serde_json::json!({
                                    "question": q.get("question").and_then(|v| v.as_str()).unwrap_or(""),
                                    "header": q.get("header").and_then(|v| v.as_str()),
                                    "options": q.get("options").cloned().unwrap_or_else(|| serde_json::json!([])),
                                    "multiSelect": q.get("multiSelect").and_then(|v| v.as_bool()).unwrap_or(false),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                obj.insert("event".into(), "AskQuestion".into());
                obj.insert("status".into(), "waiting_for_input".into());
                obj.insert("question".into(), question_text.into());
                obj.insert("options".into(), serde_json::json!(options));
                obj.insert("descriptions".into(), serde_json::json!(descriptions));
                obj.insert("questions".into(), serde_json::json!(normalized_questions));
                if let Some(ref h) = header {
                    obj.insert("header".into(), h.clone().into());
                }
                obj.insert("multi_select".into(), multi_select.into());
                obj.insert("tool_input".into(), tool_input.clone());

                if let Some(resp) = send_and_maybe_receive(&state, true) {
                    let answer = resp["answer"].as_str().unwrap_or("");
                    // Build updatedInput with the user's answer
                    let mut updated_input = tool_input.clone();
                    let answers =
                        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(answer)
                            .unwrap_or_else(|_| {
                                let mut answers = serde_json::Map::new();
                                let first_question = updated_input
                                    .get("questions")
                                    .and_then(|qs| qs.as_array())
                                    .and_then(|arr| arr.first())
                                    .and_then(|first| first.get("question"))
                                    .and_then(|q| q.as_str())
                                    .unwrap_or("");
                                answers.insert(
                                    first_question.to_string(),
                                    serde_json::Value::String(answer.to_string()),
                                );
                                answers
                            });
                    if let Some(input) = updated_input.as_object_mut() {
                        input.insert("answers".into(), serde_json::Value::Object(answers));
                    }

                    let output = serde_json::json!({
                        "hookSpecificOutput": {
                            "hookEventName": "PermissionRequest",
                            "decision": { "behavior": "allow" },
                            "updatedInput": updated_input
                        }
                    });
                    println!("{}", output);
                }
                return;
            }

            // ExitPlanMode: route as a plan approval card with Manual / Accept Edits / Auto.
            if tool_name_str == "ExitPlanMode" {
                let plan_content = tool_input
                    .get("plan")
                    .or_else(|| tool_input.get("planContent"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let plan_title = tool_input
                    .get("planTitle")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        plan_content
                            .lines()
                            .find(|line| !line.trim().is_empty())
                            .map(|line| line.trim().trim_start_matches('#').trim().to_string())
                            .filter(|line| !line.is_empty())
                            .unwrap_or_else(|| "Plan".to_string())
                    });
                let requested_permissions = tool_input
                    .get("allowedPrompts")
                    .cloned()
                    .unwrap_or(serde_json::json!([]));

                obj.insert("event".into(), "PlanApproval".into());
                obj.insert("status".into(), "waiting_for_approval".into());
                obj.insert("tool".into(), tool_name_str.into());
                obj.insert("tool_input".into(), tool_input.clone());
                obj.insert("plan_title".into(), plan_title.into());
                obj.insert("plan_content".into(), plan_content.into());
                obj.insert("requested_permissions".into(), requested_permissions);

                if let Some(resp) = send_and_maybe_receive(&state, true) {
                    let mode = resp["mode"].as_str().unwrap_or("manual");
                    if mode == "feedback" {
                        let msg = resp["message"]
                            .as_str()
                            .filter(|msg| !msg.is_empty())
                            .unwrap_or("User requested changes");
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
                    } else {
                        let updated_permissions = match mode {
                            "acceptEdits" | "bypassPermissions" => serde_json::json!([
                                { "type": "setMode", "mode": mode, "destination": "session" }
                            ]),
                            _ => serde_json::json!([]),
                        };
                        let mut decision = serde_json::json!({
                            "behavior": "allow",
                            "updatedInput": tool_input
                        });
                        if let Some(obj) = decision.as_object_mut() {
                            if updated_permissions
                                .as_array()
                                .map(|arr| !arr.is_empty())
                                .unwrap_or(false)
                            {
                                obj.insert("updatedPermissions".into(), updated_permissions);
                            }
                        }
                        let output = serde_json::json!({
                            "hookSpecificOutput": {
                                "hookEventName": "PermissionRequest",
                                "decision": decision
                            }
                        });
                        println!("{}", output);
                    }
                }
                return;
            }

            // Regular permission request
            obj.insert("status".into(), "waiting_for_approval".into());
            if let Some(t) = data.get("tool_name").or_else(|| data.get("tool")) {
                obj.insert("tool".into(), t.clone());
            }
            obj.insert("tool_input".into(), tool_input);

            if let Some(resp) = send_and_maybe_receive(&state, true) {
                let decision = resp["decision"].as_str().unwrap_or("ask");
                let reason = resp["reason"].as_str().unwrap_or("");
                let always = resp["always"].as_bool().unwrap_or(false);

                if decision == "allow" {
                    let permission_suggestions = data
                        .get("permission_suggestions")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([]));
                    let mut decision = serde_json::json!({ "behavior": "allow" });
                    if always
                        && permission_suggestions
                            .as_array()
                            .map(|arr| !arr.is_empty())
                            .unwrap_or(false)
                    {
                        if let Some(obj) = decision.as_object_mut() {
                            obj.insert("updatedPermissions".into(), permission_suggestions);
                        }
                    }
                    let output = serde_json::json!({
                        "hookSpecificOutput": {
                            "hookEventName": "PermissionRequest",
                            "decision": decision
                        }
                    });
                    println!("{}", output);
                } else if decision == "auto" {
                    let output = serde_json::json!({
                        "hookSpecificOutput": {
                            "hookEventName": "PermissionRequest",
                            "decision": {
                                "behavior": "allow",
                                "updatedPermissions": [
                                    { "type": "setMode", "mode": "bypassPermissions", "destination": "session" }
                                ]
                            }
                        }
                    });
                    println!("{}", output);
                } else if decision == "deny" {
                    let msg = if reason.is_empty() {
                        "Denied by user via AgentBro"
                    } else {
                        reason
                    };
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
            if let Some(msg) = data.get("message") {
                obj.insert("message".into(), msg.clone());
            }
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
            let agent_id = data
                .get("agent_id")
                .or_else(|| data.get("agentId"))
                .or_else(|| data.get("tool_use_id"))
                .or_else(|| data.get("toolUseId"))
                .cloned()
                .unwrap_or_else(|| "unknown".into());
            obj.insert("agent_id".into(), agent_id);
            let desc = data
                .get("description")
                .or_else(|| data.get("message"))
                .cloned()
                .unwrap_or_else(|| "".into());
            obj.insert("description".into(), desc);
            copy_optional_field(
                obj,
                &data,
                "agent_type",
                &["agent_type", "agentType", "type"],
            );
            copy_optional_field(
                obj,
                &data,
                "transcript_path",
                &["transcript_path", "transcriptPath"],
            );
        }
        "SubagentStop" => {
            obj.insert("status".into(), "processing".into());
            let agent_id = data
                .get("agent_id")
                .or_else(|| data.get("agentId"))
                .or_else(|| data.get("tool_use_id"))
                .or_else(|| data.get("toolUseId"))
                .cloned()
                .unwrap_or_else(|| "unknown".into());
            obj.insert("agent_id".into(), agent_id);
            let agent_status = data
                .get("agent_status")
                .or_else(|| data.get("agentStatus"))
                .cloned()
                .unwrap_or_else(|| "completed".into());
            obj.insert("agent_status".into(), agent_status);
            copy_optional_field(
                obj,
                &data,
                "agent_type",
                &["agent_type", "agentType", "type"],
            );
            copy_optional_field(
                obj,
                &data,
                "transcript_path",
                &["transcript_path", "transcriptPath"],
            );
            copy_optional_field(
                obj,
                &data,
                "agent_transcript_path",
                &["agent_transcript_path", "agentTranscriptPath"],
            );
            copy_optional_field(
                obj,
                &data,
                "last_assistant_message",
                &["last_assistant_message", "lastAssistantMessage", "message"],
            );
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
                obj.insert(
                    "stdout".into(),
                    result.get("stdout").cloned().unwrap_or_default(),
                );
                obj.insert(
                    "stderr".into(),
                    result.get("stderr").cloned().unwrap_or_default(),
                );
                obj.insert(
                    "exit_code".into(),
                    result.get("exit_code").cloned().unwrap_or_default(),
                );
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
