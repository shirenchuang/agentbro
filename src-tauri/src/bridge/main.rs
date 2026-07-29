//! AgentBro Hook Bridge
//!
//! A lightweight compiled binary that Claude Code hooks call.
//! Reads JSON from stdin, forwards events to AgentBro via Unix socket or TCP.
//! For PermissionRequest events, waits for a response and outputs it.

use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

const TIMEOUT_SECONDS: u64 = 21_600;

/// Polymorphic stream: Unix socket or TCP
enum Stream {
    #[cfg(unix)]
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.read(buf),
            Stream::Tcp(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.write(buf),
            Stream::Tcp(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.flush(),
            Stream::Tcp(s) => s.flush(),
        }
    }
}

impl Stream {
    fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Stream::Unix(s) => s.set_read_timeout(dur),
            Stream::Tcp(s) => s.set_read_timeout(dur),
        }
    }
}

fn normalize_tty(raw: &str) -> Option<String> {
    let tty = raw.trim();
    if tty.is_empty() || tty == "??" || tty == "?" || tty == "-" {
        return None;
    }
    Some(if tty.starts_with("/dev/") {
        tty.to_string()
    } else {
        format!("/dev/{}", tty)
    })
}

#[cfg(test)]
fn ask_user_question_hook_output(updated_input: serde_json::Value) -> serde_json::Value {
    ask_user_question_hook_output_for_source("claude-code", updated_input)
}

fn ask_user_question_hook_output_for_source(
    source: &str,
    updated_input: serde_json::Value,
) -> serde_json::Value {
    if is_zcode_source(source) {
        return serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "updatedInput": updated_input
            }
        });
    }

    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "decision": {
                "behavior": "allow",
                "updatedInput": updated_input.clone()
            },
            "updatedInput": updated_input
        }
    })
}

fn plan_title_from_content(plan_content: &str) -> String {
    plan_content
        .lines()
        .find_map(|line| {
            let title = line.trim().trim_start_matches('#').trim();
            if title.is_empty() {
                None
            } else {
                Some(title.to_string())
            }
        })
        .unwrap_or_else(|| "Plan".to_string())
}

#[cfg(test)]
fn plan_hook_output(
    tool_input: serde_json::Value,
    mode: &str,
    message: Option<&str>,
) -> serde_json::Value {
    plan_hook_output_for_source("claude-code", tool_input, mode, message)
}

fn plan_hook_output_for_source(
    source: &str,
    tool_input: serde_json::Value,
    mode: &str,
    message: Option<&str>,
) -> serde_json::Value {
    if mode == "feedback" {
        let msg = message
            .filter(|msg| !msg.is_empty())
            .unwrap_or("User requested changes");
        return serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": {
                    "behavior": "deny",
                    "message": msg
                }
            }
        });
    }

    let mut decision = serde_json::json!({
        "behavior": "allow",
        "updatedInput": tool_input
    });
    if !is_zcode_source(source) && matches!(mode, "acceptEdits" | "bypassPermissions") {
        if let Some(obj) = decision.as_object_mut() {
            obj.insert(
                "updatedPermissions".into(),
                serde_json::json!([
                    { "type": "setMode", "mode": mode, "destination": "session" }
                ]),
            );
        }
    }

    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": decision
        }
    })
}

fn is_codex_source(source: &str) -> bool {
    source == "codex" || source == "openai.codex"
}

fn is_zcode_source(source: &str) -> bool {
    source == "zcode"
}

fn is_gemini_source(source: &str) -> bool {
    source == "gemini"
}

fn is_antigravity_source(source: &str) -> bool {
    source == "antigravity"
}

fn permission_hook_output_gemini(decision: &str, reason: &str) -> serde_json::Value {
    match decision {
        "allow" => serde_json::json!({
            "decision": { "behavior": "allow" }
        }),
        "auto" => serde_json::json!({
            "decision": { "behavior": "allow" }
        }),
        "deny" => {
            let msg = if reason.is_empty() {
                "Denied by user via AgentBro"
            } else {
                reason
            };
            serde_json::json!({
                "decision": { "behavior": "deny", "message": msg }
            })
        }
        _ => serde_json::json!({
            "decision": { "behavior": "allow" }
        }),
    }
}

fn permission_hook_output_antigravity(decision: &str, reason: &str) -> serde_json::Value {
    match decision {
        "allow" | "auto" => serde_json::json!({ "decision": "allow" }),
        "deny" => serde_json::json!({
            "decision": "deny",
            "reason": if reason.is_empty() {
                "Denied by user via AgentBro"
            } else {
                reason
            }
        }),
        _ => serde_json::json!({
            "decision": "ask",
            "reason": "AgentBro could not resolve this permission request"
        }),
    }
}

fn antigravity_hook_output(event: &str) -> serde_json::Value {
    if event == "Stop" {
        serde_json::json!({ "decision": "stop" })
    } else {
        serde_json::json!({})
    }
}

fn permission_hook_output(
    source: &str,
    decision: &str,
    reason: &str,
    always: bool,
    permission_suggestions: serde_json::Value,
) -> Option<serde_json::Value> {
    match decision {
        "allow" => {
            let mut decision = serde_json::json!({ "behavior": "allow" });
            if !is_codex_source(source)
                && !is_zcode_source(source)
                && always
                && permission_suggestions
                    .as_array()
                    .map(|arr| !arr.is_empty())
                    .unwrap_or(false)
            {
                if let Some(obj) = decision.as_object_mut() {
                    obj.insert("updatedPermissions".into(), permission_suggestions);
                }
            }
            Some(permission_request_output(decision))
        }
        "auto" => {
            if is_codex_source(source) || is_zcode_source(source) {
                Some(permission_request_output(
                    serde_json::json!({ "behavior": "allow" }),
                ))
            } else {
                Some(permission_request_output(serde_json::json!({
                    "behavior": "allow",
                    "updatedPermissions": [
                        { "type": "setMode", "mode": "bypassPermissions", "destination": "session" }
                    ]
                })))
            }
        }
        "deny" => {
            let msg = if reason.is_empty() {
                "Denied by user via AgentBro"
            } else {
                reason
            };
            Some(permission_request_output(serde_json::json!({
                "behavior": "deny",
                "message": msg
            })))
        }
        _ => None,
    }
}

fn permission_request_output(decision: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": decision
        }
    })
}

fn question_multi_select(question: &serde_json::Value) -> bool {
    question
        .get("multiSelect")
        .or_else(|| question.get("multi_select"))
        .or_else(|| question.get("multiple"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn question_option_label(option: &serde_json::Value) -> Option<&str> {
    option
        .as_str()
        .or_else(|| option.get("label").and_then(|v| v.as_str()))
}

fn question_option_description(option: &serde_json::Value) -> Option<&str> {
    option.get("description").and_then(|v| v.as_str())
}

fn populate_ask_user_question_state(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    tool_input: &serde_json::Value,
) {
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
                .filter_map(|opt| question_option_label(opt).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let descriptions: Vec<String> = first_q
        .and_then(|q| q.get("options"))
        .and_then(|o| o.as_array())
        .map(|arr| {
            arr.iter()
                .map(|opt| question_option_description(opt).unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default();

    let header = first_q
        .and_then(|q| q.get("header"))
        .and_then(|h| h.as_str())
        .map(|s| s.to_string());

    let multi_select = first_q.map(question_multi_select).unwrap_or(false);
    let normalized_questions: Vec<serde_json::Value> = questions
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|q| {
                    serde_json::json!({
                        "question": q.get("question").and_then(|v| v.as_str()).unwrap_or(""),
                        "header": q.get("header").and_then(|v| v.as_str()),
                        "options": q.get("options").cloned().unwrap_or_else(|| serde_json::json!([])),
                        "multiSelect": question_multi_select(q),
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
}

fn ask_user_question_updated_input(
    tool_input: &serde_json::Value,
    answer: &str,
) -> serde_json::Value {
    let mut updated_input = tool_input.clone();
    let mut answers = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(answer)
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

    if let Some(questions) = updated_input.get("questions").and_then(|qs| qs.as_array()) {
        for question in questions {
            let Some(header) = question.get("header").and_then(|h| h.as_str()) else {
                continue;
            };
            if header.is_empty() {
                continue;
            }
            let Some(text) = question.get("question").and_then(|q| q.as_str()) else {
                continue;
            };
            if answers.contains_key(text) && !answers.contains_key(header) {
                if let Some(value) = answers.get(text).cloned() {
                    answers.insert(header.to_string(), value);
                }
            } else if answers.contains_key(header) && !answers.contains_key(text) {
                if let Some(value) = answers.get(header).cloned() {
                    answers.insert(text.to_string(), value);
                }
            }
        }
    }

    if let Some(input) = updated_input.as_object_mut() {
        input.insert("answers".into(), serde_json::Value::Object(answers));
    }
    updated_input
}

#[cfg(unix)]
fn ps_tty_and_ppid(pid: u32) -> Option<(Option<String>, Option<u32>)> {
    if let Ok(output) = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "tty=,ppid="])
        .output()
    {
        let output = String::from_utf8_lossy(&output.stdout);
        let mut parts = output.split_whitespace();
        let tty = parts.next().and_then(normalize_tty);
        let ppid = parts.next().and_then(|value| value.parse::<u32>().ok());
        if tty.is_some() || ppid.is_some() {
            return Some((tty, ppid));
        }
    }
    None
}

#[cfg(not(unix))]
fn ps_tty_and_ppid(_pid: u32) -> Option<(Option<String>, Option<u32>)> {
    None
}

/// Get the TTY of the invoking agent process.
fn get_tty() -> Option<String> {
    let mut pid = std::process::id();
    for _ in 0..8 {
        let Some((tty, ppid)) = ps_tty_and_ppid(pid) else {
            break;
        };
        if tty.is_some() {
            return tty;
        }
        let Some(parent) = ppid else {
            break;
        };
        if parent <= 1 || parent == pid {
            break;
        }
        pid = parent;
    }

    if let Ok(tty) = std::env::var("TTY") {
        if let Some(normalized) = normalize_tty(&tty) {
            return Some(normalized);
        }
    }

    None
}

fn parent_process_id() -> u32 {
    #[cfg(unix)]
    {
        std::os::unix::process::parent_id()
    }
    #[cfg(all(not(unix), target_os = "windows"))]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };

        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return 0;
            }

            let current_pid = std::process::id();
            let mut entry = PROCESSENTRY32W::default();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut parent_pid = 0;

            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    if entry.th32ProcessID == current_pid {
                        parent_pid = entry.th32ParentProcessID;
                        break;
                    }
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }

            let _ = CloseHandle(snapshot);
            parent_pid
        }
    }
    #[cfg(all(not(unix), not(target_os = "windows")))]
    {
        0
    }
}

/// Connect to AgentBro: try Unix socket first, fall back to TCP
fn connect() -> Option<Stream> {
    let endpoint = agentbro_lib::hook_endpoint::current();
    #[cfg(unix)]
    {
        if let Ok(s) = UnixStream::connect(&endpoint.socket_path) {
            return Some(Stream::Unix(s));
        }
    }
    if let Ok(s) = TcpStream::connect(endpoint.tcp_addr()) {
        return Some(Stream::Tcp(s));
    }
    None
}

fn send_and_maybe_receive(
    state: &serde_json::Value,
    wait_response: bool,
) -> Option<serde_json::Value> {
    let Some(mut stream) = connect() else {
        record_invocation(state, false);
        return None;
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(TIMEOUT_SECONDS)))
        .ok();
    let payload = format!("{}\n", state);
    if stream.write_all(payload.as_bytes()).is_err() {
        record_invocation(state, false);
        return None;
    }
    record_invocation(state, true);

    if wait_response {
        let mut reader = BufReader::new(&mut stream);
        let mut response = String::new();
        if reader.read_line(&mut response).is_ok() && !response.is_empty() {
            return serde_json::from_str(&response).ok();
        }
    }
    None
}

fn invocation_log_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    let new_path = home
        .join(".agentbro")
        .join("hooks")
        .join("invocations.jsonl");
    // Migrate from old flat location
    let old_path = home.join(".agentbro").join("hook-invocations.jsonl");
    if old_path.exists() && !new_path.exists() {
        if let Some(parent) = new_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::rename(&old_path, &new_path);
    }
    new_path
}

fn invocation_log_line(state: &serde_json::Value, forwarded: bool) -> String {
    let source = state
        .get("agent")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let event = state
        .get("event")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let session_id = state
        .get("session_id")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    serde_json::json!({
        "ts": chrono::Utc::now().timestamp_millis(),
        "source": source,
        "event": event,
        "session_id": session_id,
        "forwarded": forwarded,
    })
    .to_string()
}

fn record_invocation(state: &serde_json::Value, forwarded: bool) {
    let path = invocation_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{}", invocation_log_line(state, forwarded));
    }
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

fn tool_response_error(response: &serde_json::Value) -> Option<String> {
    let failed = response
        .get("exit_code")
        .or_else(|| response.get("exitCode"))
        .and_then(|value| value.as_i64())
        .is_some_and(|code| code != 0)
        || response
            .get("isError")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        || response
            .get("status")
            .and_then(|value| value.as_str())
            .is_some_and(|status| matches!(status, "error" | "failed" | "failure"))
        || response.get("error").is_some();

    if !failed {
        return None;
    }

    response
        .get("stderr")
        .or_else(|| response.get("error"))
        .or_else(|| response.get("message"))
        .or_else(|| response.get("stdout"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(500).collect::<String>())
        .or_else(|| {
            response
                .get("exit_code")
                .or_else(|| response.get("exitCode"))
                .and_then(|value| value.as_i64())
                .map(|code| format!("Tool exited with status {code}"))
        })
        .or_else(|| Some("Tool result reported an error".to_string()))
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

fn string_field_with_payload<'a>(
    data: &'a serde_json::Value,
    payload: Option<&'a serde_json::Value>,
    keys: &[&str],
) -> Option<&'a str> {
    string_field(data, keys).or_else(|| payload.and_then(|value| string_field(value, keys)))
}

fn value_field_with_payload<'a>(
    data: &'a serde_json::Value,
    payload: Option<&'a serde_json::Value>,
    keys: &[&str],
) -> Option<&'a serde_json::Value> {
    keys.iter()
        .find_map(|key| data.get(key))
        .or_else(|| payload.and_then(|value| keys.iter().find_map(|key| value.get(key))))
}

fn first_string_array_field<'a>(data: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    data.get(key)
        .and_then(|value| value.as_array())
        .and_then(|values| values.iter().find_map(|value| value.as_str()))
}

fn cline_payload_key(event: &str) -> Option<&'static str> {
    match event {
        "UserPromptSubmit" => Some("userPromptSubmit"),
        "PreToolUse" => Some("preToolUse"),
        "PostToolUse" => Some("postToolUse"),
        "TaskStart" => Some("taskStart"),
        "TaskResume" => Some("taskResume"),
        "TaskCancel" => Some("taskCancel"),
        "TaskComplete" => Some("taskComplete"),
        "PreCompact" => Some("preCompact"),
        _ => None,
    }
}

fn cline_event_payload<'a>(
    source: &str,
    event: &str,
    data: &'a serde_json::Value,
) -> Option<&'a serde_json::Value> {
    if source != "cline" {
        return None;
    }
    cline_payload_key(event).and_then(|key| data.get(key))
}

fn normalize_hook_event(event: &str) -> &str {
    match event {
        "session_start" => "SessionStart",
        "sessionStart" => "SessionStart",
        "session_end" => "SessionEnd",
        "sessionEnd" => "SessionEnd",
        "user_prompt_submit" => "UserPromptSubmit",
        "userPromptSubmit" | "userPromptSubmitted" => "UserPromptSubmit",
        "pre_tool_use" => "PreToolUse",
        "preToolUse" | "BeforeTool" => "PreToolUse",
        "post_tool_use" => "PostToolUse",
        "postToolUse" | "AfterTool" => "PostToolUse",
        "post_tool_use_failure" => "PostToolUseFailure",
        "postToolUseFailure" | "errorOccurred" => "PostToolUseFailure",
        "permission_request" => "PermissionRequest",
        "permissionRequest" => "PermissionRequest",
        "permission_result" => "PermissionResult",
        "permissionResult" => "PermissionResult",
        "permission_denied" => "PermissionDenied",
        "permissionDenied" => "PermissionDenied",
        "notification" => "Notification",
        "stop" => "Stop",
        "agentStop" => "Stop",
        "ErrorOccurred" => "PostToolUseFailure",
        "pre_compact" => "PreCompact",
        "preCompact" => "PreCompact",
        "post_compact" => "PostCompact",
        "postCompact" => "PostCompact",
        "subagent_start" => "SubagentStart",
        "subagentStart" => "SubagentStart",
        "subagent_stop" => "SubagentStop",
        "subagentStop" => "SubagentStop",
        "agentSpawn" => "SessionStart",
        "stop_failure" => "StopFailure",
        "interrupt" => "Interrupt",
        other => other,
    }
}

fn hook_input_data(input: &str, forced_event: Option<&str>) -> Option<serde_json::Value> {
    match serde_json::from_str(input) {
        Ok(value) => Some(value),
        Err(_) if forced_event.is_some() => Some(serde_json::json!({})),
        Err(_) => None,
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

    let Some(data) = hook_input_data(&input, forced_event.as_deref()) else {
        return;
    };

    let session_id = string_field(
        &data,
        &[
            "session_id",
            "sessionId",
            "conversationId",
            "task_id",
            "taskId",
            "id",
        ],
    )
    .unwrap_or("unknown");
    let hook_event = forced_event
        .as_deref()
        .or_else(|| string_field(&data, &["hook_event_name", "event", "hookType"]))
        .map(normalize_hook_event)
        .unwrap_or("");
    let event_payload = cline_event_payload(&source, hook_event, &data);
    let cwd = string_field(&data, &["cwd"])
        .or_else(|| first_string_array_field(&data, "workspaceRoots"))
        .or_else(|| first_string_array_field(&data, "workspacePaths"))
        .unwrap_or("");
    let tool_call = data.get("toolCall");
    let tool_input = data
        .get("tool_input")
        .or_else(|| data.get("toolInput"))
        .or_else(|| data.get("toolArgs"))
        .or_else(|| data.get("arguments"))
        .or_else(|| data.get("args"))
        .or_else(|| event_payload.and_then(|payload| payload.get("parameters")))
        .or_else(|| event_payload.and_then(|payload| payload.get("toolArgs")))
        .or_else(|| event_payload.and_then(|payload| payload.get("input")))
        .or_else(|| event_payload.and_then(|payload| payload.get("arguments")))
        .or_else(|| event_payload.and_then(|payload| payload.get("args")))
        .or_else(|| tool_call.and_then(|call| call.get("args")))
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let tool_name = string_field_with_payload(
        &data,
        event_payload,
        &["tool_name", "toolName", "tool", "name"],
    )
    .or_else(|| {
        tool_call
            .and_then(|call| call.get("name"))
            .and_then(|value| value.as_str())
    })
    .unwrap_or("");
    let claude_pid = parent_process_id();
    let tty = get_tty();
    let engine_label =
        arg_value("--engine-label").or_else(|| std::env::var("AGENTBRO_ENGINE_LABEL").ok());
    let engine_config_root =
        arg_value("--config-root").or_else(|| std::env::var("AGENTBRO_CONFIG_ROOT").ok());
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
    copy_optional_field(obj, &data, "model", &["model"]);
    copy_optional_field(obj, &data, "turn_id", &["turn_id", "turnId"]);
    copy_optional_field(
        obj,
        &data,
        "permission_mode",
        &["permission_mode", "permissionMode"],
    );
    copy_optional_field(obj, &data, "start_source", &["source"]);
    copy_optional_field(
        obj,
        &data,
        "artifact_directory_path",
        &["artifactDirectoryPath"],
    );
    copy_optional_field(obj, &data, "step_idx", &["stepIdx"]);
    copy_optional_field(obj, &data, "invocation_num", &["invocationNum"]);
    copy_optional_field(obj, &data, "initial_num_steps", &["initialNumSteps"]);
    copy_optional_field(obj, &data, "execution_num", &["executionNum"]);
    copy_optional_field(obj, &data, "termination_reason", &["terminationReason"]);
    copy_optional_field(obj, &data, "fully_idle", &["fullyIdle"]);
    copy_optional_field(obj, &data, "error", &["error"]);
    if is_antigravity_source(&source) {
        if let Some(step_idx) = data.get("stepIdx").and_then(|value| value.as_u64()) {
            obj.insert(
                "tool_use_id".into(),
                format!("antigravity-step-{step_idx}").into(),
            );
        }
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
            if let Some(prompt) = value_field_with_payload(
                &data,
                event_payload,
                &["user_prompt", "userPrompt", "prompt", "message"],
            ) {
                obj.insert("prompt".into(), prompt.clone());
            }
        }
        "PreToolUse" => {
            // AskUserQuestion: intercept at PreToolUse to provide updatedInput with answers.
            // updatedInput is only supported in PreToolUse hooks (not PermissionRequest).
            if tool_name == "AskUserQuestion"
                && !is_codex_source(&source)
                && !is_antigravity_source(&source)
            {
                populate_ask_user_question_state(obj, &tool_input);

                if let Some(resp) = send_and_maybe_receive(&state, true) {
                    let answer = resp["answer"].as_str().unwrap_or("");
                    let updated_input = ask_user_question_updated_input(&tool_input, answer);
                    let output = ask_user_question_hook_output_for_source(&source, updated_input);
                    println!("{}", output);
                }
                return;
            }

            // Gemini: BeforeTool acts as the permission gate (no separate PermissionRequest event).
            // Block and wait for user approval via AgentBro UI.
            if is_gemini_source(&source) {
                obj.insert("status".into(), "waiting_for_approval".into());
                if !tool_name.is_empty() {
                    obj.insert("tool".into(), tool_name.into());
                } else if let Some(t) = data.get("tool_name").or_else(|| data.get("tool")) {
                    obj.insert("tool".into(), t.clone());
                }
                obj.insert("tool_input".into(), tool_input);
                if let Some(id) = data
                    .get("tool_use_id")
                    .or_else(|| data.get("toolUseId"))
                    .or_else(|| data.get("tool_call_id"))
                {
                    obj.insert("tool_use_id".into(), id.clone());
                }

                if let Some(resp) = send_and_maybe_receive(&state, true) {
                    let decision = resp["decision"].as_str().unwrap_or("allow");
                    let reason = resp["reason"].as_str().unwrap_or("");
                    let output = permission_hook_output_gemini(decision, reason);
                    println!("{}", output);
                }
                return;
            }

            if is_antigravity_source(&source) {
                obj.insert("status".into(), "waiting_for_approval".into());
                if !tool_name.is_empty() {
                    obj.insert("tool".into(), tool_name.into());
                }
                obj.insert("tool_input".into(), tool_input);
                let output = if let Some(resp) = send_and_maybe_receive(&state, true) {
                    permission_hook_output_antigravity(
                        resp["decision"].as_str().unwrap_or("ask"),
                        resp["reason"].as_str().unwrap_or(""),
                    )
                } else {
                    permission_hook_output_antigravity("ask", "")
                };
                println!("{}", output);
                return;
            }

            obj.insert("status".into(), "running_tool".into());
            if !tool_name.is_empty() {
                obj.insert("tool".into(), tool_name.into());
            } else if let Some(t) = data.get("tool_name").or_else(|| data.get("tool")) {
                obj.insert("tool".into(), t.clone());
            }
            obj.insert("tool_input".into(), tool_input);
            if let Some(id) = data
                .get("tool_use_id")
                .or_else(|| data.get("toolUseId"))
                .or_else(|| data.get("tool_call_id"))
            {
                obj.insert("tool_use_id".into(), id.clone());
            }
        }
        "PostToolUse" => {
            obj.insert("status".into(), "processing".into());
            if !tool_name.is_empty() {
                obj.insert("tool".into(), tool_name.into());
            } else if let Some(t) = data.get("tool_name").or_else(|| data.get("tool")) {
                obj.insert("tool".into(), t.clone());
            }
            obj.insert("tool_input".into(), tool_input);
            if let Some(id) = data
                .get("tool_use_id")
                .or_else(|| data.get("toolUseId"))
                .or_else(|| data.get("tool_call_id"))
            {
                obj.insert("tool_use_id".into(), id.clone());
            }
            if let Some(response) = value_field_with_payload(
                &data,
                event_payload,
                &[
                    "tool_response",
                    "toolResponse",
                    "tool_result",
                    "toolResult",
                    "toolResultPreview",
                    "result",
                ],
            ) {
                obj.insert("tool_response".into(), response.clone());
                if let Some(error) = tool_response_error(response) {
                    obj.insert("tool_error".into(), error.into());
                }
            }
            if event_payload
                .and_then(|payload| payload.get("success"))
                .and_then(|value| value.as_bool())
                .is_some_and(|success| !success)
            {
                obj.insert("tool_error".into(), "Tool reported failure".into());
            }
        }
        "PostToolUseFailure" => {
            obj.insert("status".into(), "processing".into());
            if !tool_name.is_empty() {
                obj.insert("tool".into(), tool_name.into());
            } else if let Some(t) = data.get("tool_name").or_else(|| data.get("tool")) {
                obj.insert("tool".into(), t.clone());
            }
            obj.insert("tool_input".into(), tool_input);
            if let Some(e) =
                value_field_with_payload(&data, event_payload, &["error", "message", "result"])
            {
                obj.insert("tool_error".into(), e.clone());
            }
            if let Some(id) = data
                .get("tool_use_id")
                .or_else(|| data.get("toolUseId"))
                .or_else(|| data.get("tool_call_id"))
            {
                obj.insert("tool_use_id".into(), id.clone());
            }
        }
        "PermissionDenied" => {
            obj.insert("status".into(), "processing".into());
            if !tool_name.is_empty() {
                obj.insert("tool".into(), tool_name.into());
            } else if let Some(t) = data.get("tool_name").or_else(|| data.get("tool")) {
                obj.insert("tool".into(), t.clone());
            }
            obj.insert("tool_input".into(), tool_input);
            if let Some(r) = data.get("reason").or_else(|| data.get("message")) {
                obj.insert("denial_reason".into(), r.clone());
            }
        }
        "PermissionRequest" => {
            if source == "kimi" {
                obj.insert("status".into(), "waiting_for_approval".into());
                if !tool_name.is_empty() {
                    obj.insert("tool".into(), tool_name.into());
                }
                obj.insert("tool_input".into(), tool_input);
                copy_optional_field(
                    obj,
                    &data,
                    "tool_use_id",
                    &["tool_call_id", "tool_use_id", "toolUseId"],
                );
                copy_optional_field(obj, &data, "action", &["action"]);
                copy_optional_field(obj, &data, "display", &["display"]);
                send_and_maybe_receive(&state, false);
                return;
            }

            if tool_name == "AskUserQuestion" {
                // OpenCode asks questions through PermissionRequest and consumes
                // decision.updatedInput.answers from the bridge response. Claude Code
                // questions are handled earlier in PreToolUse, where updatedInput is
                // applied by the hook runtime.
                if (source == "opencode" || data.get("_opencode_request_id").is_some())
                    && !is_codex_source(&source)
                {
                    populate_ask_user_question_state(obj, &tool_input);
                    if let Some(resp) = send_and_maybe_receive(&state, true) {
                        let answer = resp["answer"].as_str().unwrap_or("");
                        let updated_input = ask_user_question_updated_input(&tool_input, answer);
                        let output =
                            ask_user_question_hook_output_for_source(&source, updated_input);
                        println!("{}", output);
                    }
                }
                return;
            }

            // ExitPlanMode: route as a plan approval card with Manual / Accept Edits / Auto.
            if tool_name == "ExitPlanMode" && !is_codex_source(&source) {
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
                    .unwrap_or_else(|| plan_title_from_content(&plan_content));
                let requested_permissions = tool_input
                    .get("allowedPrompts")
                    .cloned()
                    .unwrap_or(serde_json::json!([]));

                obj.insert("event".into(), "PlanApproval".into());
                obj.insert("status".into(), "waiting_for_approval".into());
                obj.insert("tool".into(), tool_name.into());
                obj.insert("tool_input".into(), tool_input.clone());
                obj.insert("plan_title".into(), plan_title.into());
                obj.insert("plan_content".into(), plan_content.into());
                obj.insert("requested_permissions".into(), requested_permissions);

                if let Some(resp) = send_and_maybe_receive(&state, true) {
                    let mode = resp["mode"].as_str().unwrap_or("manual");
                    let output = plan_hook_output_for_source(
                        &source,
                        tool_input,
                        mode,
                        resp["message"].as_str(),
                    );
                    println!("{}", output);
                }
                return;
            }

            // Regular permission request
            obj.insert("status".into(), "waiting_for_approval".into());
            if !tool_name.is_empty() {
                obj.insert("tool".into(), tool_name.into());
            } else if let Some(t) = data.get("tool_name").or_else(|| data.get("tool")) {
                obj.insert("tool".into(), t.clone());
            }
            obj.insert("tool_input".into(), tool_input);
            if let Some(id) = data
                .get("tool_use_id")
                .or_else(|| data.get("toolUseId"))
                .or_else(|| data.get("tool_call_id"))
            {
                obj.insert("tool_use_id".into(), id.clone());
            }

            if let Some(resp) = send_and_maybe_receive(&state, true) {
                let decision = resp["decision"].as_str().unwrap_or("ask");
                let reason = resp["reason"].as_str().unwrap_or("");
                let always = resp["always"].as_bool().unwrap_or(false);

                let permission_suggestions = data
                    .get("permission_suggestions")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!([]));
                if let Some(output) = permission_hook_output(
                    &source,
                    decision,
                    reason,
                    always,
                    permission_suggestions,
                ) {
                    println!("{}", output);
                }
            }
            return;
        }
        "PermissionResult" => {
            obj.insert("status".into(), "processing".into());
            if !tool_name.is_empty() {
                obj.insert("tool".into(), tool_name.into());
            }
            obj.insert("tool_input".into(), tool_input);
            copy_optional_field(
                obj,
                &data,
                "tool_use_id",
                &["tool_call_id", "tool_use_id", "toolUseId"],
            );
            copy_optional_field(obj, &data, "decision", &["decision", "permission_decision"]);
            copy_optional_field(obj, &data, "selected_label", &["selected_label"]);
            copy_optional_field(obj, &data, "action", &["action"]);
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
            copy_optional_field(obj, &data, "title", &["title"]);
            copy_optional_field(obj, &data, "body", &["body", "message"]);
            copy_optional_field(obj, &data, "severity", &["severity"]);
            copy_optional_field(obj, &data, "source_kind", &["source_kind"]);
            copy_optional_field(obj, &data, "source_id", &["source_id"]);
        }
        "Stop" => {
            obj.insert("status".into(), "waiting_for_input".into());
            copy_optional_field(
                obj,
                &data,
                "summary",
                &[
                    "last_assistant_message",
                    "lastAssistantMessage",
                    "summary",
                    "responseText",
                    "responsePreview",
                    "message",
                    "result",
                ],
            );
        }
        "TaskComplete" => {
            obj.insert("status".into(), "waiting_for_input".into());
            if let Some(summary) = value_field_with_payload(
                &data,
                event_payload,
                &[
                    "last_assistant_message",
                    "lastAssistantMessage",
                    "summary",
                    "message",
                    "task",
                ],
            ) {
                obj.insert("summary".into(), summary.clone());
            }
        }
        "TaskStart" | "TaskResume" => {
            obj.insert("status".into(), "processing".into());
        }
        "TaskCancel" => {
            obj.insert("status".into(), "interrupted".into());
        }
        "StopFailure" => {
            obj.insert("status".into(), "waiting_for_input".into());
            if let Some(e) = data
                .get("error_message")
                .or_else(|| data.get("error"))
                .or_else(|| data.get("message"))
            {
                obj.insert("stop_error".into(), e.clone());
                obj.insert("error_message".into(), e.clone());
            }
            copy_optional_field(obj, &data, "error_type", &["error_type"]);
        }
        "SessionStart" => {
            obj.insert("status".into(), "waiting_for_input".into());
        }
        "SessionEnd" => {
            obj.insert("status".into(), "ended".into());
        }
        "BeforeAgent" => {
            obj.insert("status".into(), "processing".into());
            copy_optional_field(obj, &data, "prompt", &["prompt", "message", "description"]);
        }
        "AfterAgent" => {
            obj.insert("status".into(), "response_received".into());
            copy_optional_field(
                obj,
                &data,
                "prompt_response",
                &[
                    "prompt_response",
                    "summary",
                    "last_assistant_message",
                    "message",
                ],
            );
        }
        "PreCompact" => {
            obj.insert("status".into(), "compacting".into());
            copy_optional_field(obj, &data, "trigger", &["trigger"]);
        }
        "PostCompact" => {
            obj.insert("status".into(), "processing".into());
            copy_optional_field(obj, &data, "trigger", &["trigger"]);
        }
        "SubagentStart" => {
            obj.insert("status".into(), "processing".into());
            let agent_id = data
                .get("agent_id")
                .or_else(|| data.get("agentId"))
                .or_else(|| data.get("agent_name"))
                .or_else(|| data.get("tool_use_id"))
                .or_else(|| data.get("toolUseId"))
                .cloned()
                .unwrap_or_else(|| "unknown".into());
            obj.insert("agent_id".into(), agent_id.clone());
            obj.insert("agent_name".into(), agent_id);
            let desc = data
                .get("description")
                .or_else(|| data.get("prompt"))
                .or_else(|| data.get("message"))
                .cloned()
                .unwrap_or_else(|| "".into());
            obj.insert("description".into(), desc);
            copy_optional_field(obj, &data, "prompt", &["prompt"]);
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
                .or_else(|| data.get("agent_name"))
                .or_else(|| data.get("tool_use_id"))
                .or_else(|| data.get("toolUseId"))
                .cloned()
                .unwrap_or_else(|| "unknown".into());
            obj.insert("agent_id".into(), agent_id.clone());
            obj.insert("agent_name".into(), agent_id);
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
                &[
                    "response",
                    "last_assistant_message",
                    "lastAssistantMessage",
                    "message",
                ],
            );
        }
        "Interrupt" => {
            obj.insert("status".into(), "interrupted".into());
            copy_optional_field(obj, &data, "reason", &["reason"]);
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
    if is_antigravity_source(&source) {
        println!("{}", antigravity_hook_output(hook_event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_current_kimi_permission_events() {
        assert_eq!(
            normalize_hook_event("permission_request"),
            "PermissionRequest"
        );
        assert_eq!(
            normalize_hook_event("permission_result"),
            "PermissionResult"
        );
        assert_eq!(normalize_hook_event("interrupt"), "Interrupt");
    }

    #[test]
    fn ask_user_question_output_keeps_updated_input_on_decision() {
        let updated_input = serde_json::json!({
            "questions": [
                {
                    "question": "Which view?",
                    "options": [{ "label": "Overlay" }]
                }
            ],
            "answers": { "Which view?": "Overlay" }
        });

        let output = ask_user_question_hook_output(updated_input.clone());

        assert_eq!(
            output["hookSpecificOutput"]["decision"]["updatedInput"],
            updated_input
        );
        assert_eq!(
            output["hookSpecificOutput"]["updatedInput"]["answers"]["Which view?"],
            "Overlay"
        );
        assert_eq!(output["hookSpecificOutput"]["permissionDecision"], "allow");
    }

    #[test]
    fn zcode_question_output_matches_strict_pre_tool_schema() {
        let output = ask_user_question_hook_output_for_source(
            "zcode",
            serde_json::json!({ "answers": { "Which view?": "Overlay" } }),
        );
        let hook_output = output["hookSpecificOutput"]
            .as_object()
            .expect("hook output object");

        assert_eq!(hook_output["hookEventName"], "PreToolUse");
        assert_eq!(hook_output["permissionDecision"], "allow");
        assert!(hook_output.contains_key("updatedInput"));
        assert!(!hook_output.contains_key("decision"));
    }

    #[test]
    fn ask_user_question_state_accepts_multiple_alias_and_string_options() {
        let tool_input = serde_json::json!({
            "questions": [
                {
                    "header": "Deploy",
                    "question": "Which targets?",
                    "options": ["Preview", "Production"],
                    "multiple": true
                }
            ]
        });
        let mut obj = serde_json::Map::new();

        populate_ask_user_question_state(&mut obj, &tool_input);

        assert_eq!(obj["question"], "[Deploy] Which targets?");
        assert_eq!(obj["options"], serde_json::json!(["Preview", "Production"]));
        assert_eq!(obj["multi_select"], true);
        assert_eq!(obj["questions"][0]["multiSelect"], true);
    }

    #[test]
    fn ask_user_question_updated_input_adds_header_answer_alias() {
        let tool_input = serde_json::json!({
            "questions": [
                {
                    "header": "Deploy",
                    "question": "Which targets?",
                    "options": [{ "label": "Preview" }],
                    "multiSelect": true
                }
            ]
        });

        let updated_input = ask_user_question_updated_input(
            &tool_input,
            r#"{"Which targets?":"Preview, Production"}"#,
        );

        assert_eq!(
            updated_input["answers"]["Which targets?"],
            "Preview, Production"
        );
        assert_eq!(updated_input["answers"]["Deploy"], "Preview, Production");
    }

    #[test]
    fn plan_hook_output_maps_manual_accept_and_bypass_modes() {
        let tool_input = serde_json::json!({
            "plan": "# Ship migration\n\nContext\n\n1. Update schema",
            "allowedPrompts": ["Edit: src/schema.ts", "Bash: pnpm test"]
        });

        let manual = plan_hook_output(tool_input.clone(), "manual", None);
        assert_eq!(
            manual["hookSpecificOutput"]["decision"]["behavior"],
            "allow"
        );
        assert_eq!(
            manual["hookSpecificOutput"]["decision"]["updatedInput"],
            tool_input
        );
        assert!(manual["hookSpecificOutput"]["decision"]
            .get("updatedPermissions")
            .is_none());

        let accept = plan_hook_output(tool_input.clone(), "acceptEdits", None);
        assert_eq!(
            accept["hookSpecificOutput"]["decision"]["updatedPermissions"][0],
            serde_json::json!({
                "type": "setMode",
                "mode": "acceptEdits",
                "destination": "session"
            })
        );

        let bypass = plan_hook_output(tool_input, "bypassPermissions", None);
        assert_eq!(
            bypass["hookSpecificOutput"]["decision"]["updatedPermissions"][0],
            serde_json::json!({
                "type": "setMode",
                "mode": "bypassPermissions",
                "destination": "session"
            })
        );
    }

    #[test]
    fn plan_hook_output_maps_feedback_to_denial_message() {
        let output = plan_hook_output(
            serde_json::json!({ "plan": "1. Update schema" }),
            "feedback",
            Some("Revise the rollout order"),
        );

        assert_eq!(
            output["hookSpecificOutput"]["decision"],
            serde_json::json!({
                "behavior": "deny",
                "message": "Revise the rollout order"
            })
        );
    }

    #[test]
    fn zcode_plan_output_omits_unsupported_permission_updates() {
        let output = plan_hook_output_for_source(
            "zcode",
            serde_json::json!({ "plan": "1. Update schema" }),
            "bypassPermissions",
            None,
        );
        let decision = &output["hookSpecificOutput"]["decision"];

        assert_eq!(decision["behavior"], "allow");
        assert!(decision.get("updatedInput").is_some());
        assert!(decision.get("updatedPermissions").is_none());
    }

    #[test]
    fn codex_permission_output_never_returns_reserved_permission_fields() {
        let output = permission_hook_output(
            "codex",
            "auto",
            "",
            true,
            serde_json::json!([{ "type": "setMode", "mode": "bypassPermissions" }]),
        )
        .expect("permission output");

        let decision = &output["hookSpecificOutput"]["decision"];
        assert_eq!(decision["behavior"], "allow");
        assert!(decision.get("updatedInput").is_none());
        assert!(decision.get("updatedPermissions").is_none());
        assert!(decision.get("interrupt").is_none());
    }

    #[test]
    fn zcode_permission_output_matches_strict_permission_schema() {
        let output = permission_hook_output(
            "zcode",
            "auto",
            "",
            true,
            serde_json::json!([{ "type": "setMode", "mode": "bypassPermissions" }]),
        )
        .expect("permission output");
        let decision = &output["hookSpecificOutput"]["decision"];

        assert_eq!(decision, &serde_json::json!({ "behavior": "allow" }));
    }

    #[test]
    fn non_codex_permission_output_can_keep_persistent_allow() {
        let output = permission_hook_output(
            "claude-code",
            "allow",
            "",
            true,
            serde_json::json!([{ "type": "setMode", "mode": "acceptEdits" }]),
        )
        .expect("permission output");

        assert_eq!(
            output["hookSpecificOutput"]["decision"]["updatedPermissions"][0]["mode"],
            "acceptEdits"
        );
    }

    #[test]
    fn antigravity_permission_output_uses_official_decisions() {
        assert_eq!(
            permission_hook_output_antigravity("allow", ""),
            serde_json::json!({ "decision": "allow" })
        );
        assert_eq!(
            permission_hook_output_antigravity("deny", "Not approved"),
            serde_json::json!({
                "decision": "deny",
                "reason": "Not approved"
            })
        );
        assert_eq!(
            permission_hook_output_antigravity("ask", "")["decision"],
            "ask"
        );
    }

    #[test]
    fn antigravity_non_permission_hooks_always_return_json() {
        assert_eq!(
            antigravity_hook_output("PostToolUse"),
            serde_json::json!({})
        );
        assert_eq!(
            antigravity_hook_output("PreInvocation"),
            serde_json::json!({})
        );
        assert_eq!(
            antigravity_hook_output("PostInvocation"),
            serde_json::json!({})
        );
        assert_eq!(
            antigravity_hook_output("Stop"),
            serde_json::json!({ "decision": "stop" })
        );
    }

    #[test]
    fn antigravity_documented_fields_are_available_to_normalization() {
        let data = serde_json::json!({
            "conversationId": "conversation-1",
            "workspacePaths": ["/workspace/project"],
            "toolCall": {
                "name": "run_command",
                "args": {
                    "CommandLine": "pnpm test"
                }
            }
        });

        assert_eq!(
            string_field(&data, &["session_id", "conversationId"]),
            Some("conversation-1")
        );
        assert_eq!(
            first_string_array_field(&data, "workspacePaths"),
            Some("/workspace/project")
        );
        assert_eq!(data["toolCall"]["name"], "run_command");
        assert_eq!(data["toolCall"]["args"]["CommandLine"], "pnpm test");
    }

    #[test]
    fn tool_response_error_extracts_nonzero_stderr() {
        let error = tool_response_error(&serde_json::json!({
            "exit_code": 2,
            "stderr": "command failed"
        }));

        assert_eq!(error.as_deref(), Some("command failed"));
    }

    #[test]
    fn plan_title_from_content_uses_first_non_empty_markdown_line() {
        assert_eq!(
            plan_title_from_content("\n\n## Database migration\n\n1. Update schema"),
            "Database migration"
        );
        assert_eq!(plan_title_from_content("\n\n"), "Plan");
    }

    #[test]
    fn invocation_log_line_omits_prompt_and_cwd() {
        let state = serde_json::json!({
            "agent": "qoder",
            "event": "UserPromptSubmit",
            "session_id": "solo-session",
            "cwd": "/Users/me/secret-project",
            "prompt": "sensitive prompt"
        });

        let line = invocation_log_line(&state, true);
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert_eq!(parsed["source"], "qoder");
        assert_eq!(parsed["event"], "UserPromptSubmit");
        assert_eq!(parsed["session_id"], "solo-session");
        assert_eq!(parsed["forwarded"], true);
        assert!(!line.contains("secret-project"));
        assert!(!line.contains("sensitive prompt"));
    }

    #[test]
    fn forced_event_allows_empty_input() {
        let data = hook_input_data("", Some("UserPromptSubmit")).unwrap();

        assert_eq!(data, serde_json::json!({}));
    }

    #[test]
    fn invalid_input_without_forced_event_is_ignored() {
        assert!(hook_input_data("", None).is_none());
        assert!(hook_input_data("not json", None).is_none());
    }
}
