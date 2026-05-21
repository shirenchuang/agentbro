// Remote hook installer — uploads and configures agent hooks on a remote host

use super::manager::RemoteHost;
use crate::agents::profiles::{self, HookEntryTemplate};
use serde::Serialize;

pub struct InstallResult {
    pub ok: bool,
    pub message: String,
}

pub struct RemoteInstaller;

impl RemoteInstaller {
    /// Install agent hooks on the remote host via SSH
    pub async fn install_hooks(host: &RemoteHost) -> InstallResult {
        // Upload the hook script
        let upload = Self::upload_hook_script(host).await;
        if !upload.ok {
            return upload;
        }

        // Configure hooks for all supported tools
        let configure = Self::configure_hooks(host).await;
        if !configure.ok {
            return configure;
        }

        InstallResult {
            ok: true,
            message: "Remote hooks installed successfully".to_string(),
        }
    }

    /// Remove AgentBro-managed agent hooks from the remote host via SSH
    pub async fn uninstall_hooks(host: &RemoteHost) -> InstallResult {
        let remove = Self::remove_configured_hooks(host).await;
        if !remove.ok {
            return remove;
        }

        InstallResult {
            ok: true,
            message: "Remote hooks uninstalled successfully".to_string(),
        }
    }

    /// Remove the reverse-forwarded Unix socket on the remote side (cleanup before reconnect)
    pub async fn cleanup_remote_socket(host: &RemoteHost) {
        let cmd = format!("rm -f '{}'", host.remote_socket_path.replace('\'', "\\'"));
        let _ = run_ssh(host, &cmd, 8).await;
    }

    async fn upload_hook_script(host: &RemoteHost) -> InstallResult {
        // Inline a minimal Python hook that forwards events to the remote socket
        let hook_script = Self::hook_script_content(host);
        let encoded = base64_encode(hook_script.as_bytes());

        let python = format!(
            r#"import base64, os, pathlib
target = pathlib.Path.home() / ".agentbro" / "remote-hook.py"
target.parent.mkdir(parents=True, exist_ok=True)
target.write_bytes(base64.b64decode('{}'))
os.chmod(target, 0o755)
print(target)
"#,
            encoded
        );

        let cmd = format!("python3 - <<'PYEOF'\n{}\nPYEOF", python);
        let result = run_ssh(host, &cmd, 25).await;

        if result.exit_code == 0 {
            InstallResult {
                ok: true,
                message: result.stdout.trim().to_string(),
            }
        } else {
            InstallResult {
                ok: false,
                message: format!(
                    "Upload failed: {}",
                    result.stderr.chars().take(200).collect::<String>()
                ),
            }
        }
    }

    async fn configure_hooks(host: &RemoteHost) -> InstallResult {
        let profile = profiles::claude_code_profile();
        let socket = &host.remote_socket_path;
        let host_id = &host.id;
        let host_name = &host.name;
        let source = profile.source;
        let config_path_parts_json = profile_config_path_parts_json(&profile);
        let event_specs_json = remote_event_specs_json(&profile);
        let socket_json = json_string(socket);
        let host_id_json = json_string(host_id);
        let host_name_json = json_string(host_name);
        let source_json = json_string(source);

        let script = format!(
            r#"
import json, pathlib, os, shlex

home = pathlib.Path.home()
config_path = home
for part in json.loads(r'''{config_path_parts_json}'''):
    config_path = config_path / part
event_specs = json.loads(r'''{event_specs_json}''')
socket_path = json.loads(r'''{socket_json}''')
host_id = json.loads(r'''{host_id_json}''')
host_name = json.loads(r'''{host_name_json}''')
source = json.loads(r'''{source_json}''')
hook_cmd = " ".join([
    "AGENTBRO_SOCKET=" + shlex.quote(socket_path),
    "AGENTBRO_HOST_ID=" + shlex.quote(host_id),
    "AGENTBRO_HOST_NAME=" + shlex.quote(host_name),
    "AGENTBRO_AGENT=" + shlex.quote(source),
    "python3 ~/.agentbro/remote-hook.py",
])

def is_agentbro_entry(value):
    try:
        text = json.dumps(value, sort_keys=True)
    except Exception:
        text = str(value)
    return (
        "remote-hook.py" in text
        or "agentbro-bridge" in text
        or "AgentBro managed integration" in text
    )

try:
    if config_path.exists():
        data = json.loads(config_path.read_text() or "{{}}")
    else:
        data = {{}}
    hooks = data.setdefault("hooks", {{}})
    for event_name, value in list(hooks.items()):
        if not isinstance(value, list):
            continue
        filtered = [entry for entry in value if not is_agentbro_entry(entry)]
        if filtered:
            hooks[event_name] = filtered
        else:
            hooks.pop(event_name, None)

    for event in event_specs:
        inner = {{"type": "command", "command": hook_cmd}}
        if event.get("timeout") is not None:
            inner["timeout"] = event["timeout"]
        entry = {{"hooks": [inner]}}
        if event.get("matcher") is not None:
            entry["matcher"] = event["matcher"]
        hooks.setdefault(event["name"], []).append(entry)

    if hooks:
        data["hooks"] = hooks
    else:
        data.pop("hooks", None)
    config_path.parent.mkdir(parents=True, exist_ok=True)
    config_path.write_text(json.dumps(data, indent=2))
    print(f"{{source}} hooks configured: {{len(event_specs)}} events")
except Exception as e:
    print(f"{{source}} config error: {{e}}")
"#,
            config_path_parts_json = config_path_parts_json,
            event_specs_json = event_specs_json,
            socket_json = socket_json,
            host_id_json = host_id_json,
            host_name_json = host_name_json,
            source_json = source_json,
        );

        let encoded = base64_encode(script.as_bytes());
        let cmd = format!(
            "\"${{SHELL:-/bin/bash}}\" -lc \"echo '{}' | base64 -d | python3\"",
            encoded
        );
        let result = run_ssh(host, &cmd, 30).await;

        if result.exit_code == 0 {
            InstallResult {
                ok: true,
                message: result.stdout.trim().to_string(),
            }
        } else {
            InstallResult {
                ok: false,
                message: format!(
                    "Configure failed: {}",
                    result.stderr.chars().take(200).collect::<String>()
                ),
            }
        }
    }

    async fn remove_configured_hooks(host: &RemoteHost) -> InstallResult {
        let profile = profiles::claude_code_profile();
        let source = profile.source;
        let config_path_parts_json = profile_config_path_parts_json(&profile);
        let source_json = json_string(source);

        let script = format!(
            r#"
import json, pathlib

home = pathlib.Path.home()
config_path = home
for part in json.loads(r'''{config_path_parts_json}'''):
    config_path = config_path / part
source = json.loads(r'''{source_json}''')
hook_script = home / ".agentbro" / "remote-hook.py"

def is_agentbro_entry(value):
    try:
        text = json.dumps(value, sort_keys=True)
    except Exception:
        text = str(value)
    return (
        "remote-hook.py" in text
        or "agentbro-bridge" in text
        or "AgentBro managed integration" in text
    )

try:
    removed = 0
    if config_path.exists():
        data = json.loads(config_path.read_text() or "{{}}")
        hooks = data.get("hooks")
        if isinstance(hooks, dict):
            for event_name, value in list(hooks.items()):
                if not isinstance(value, list):
                    continue
                filtered = [entry for entry in value if not is_agentbro_entry(entry)]
                removed += len(value) - len(filtered)
                if filtered:
                    hooks[event_name] = filtered
                else:
                    hooks.pop(event_name, None)
            if hooks:
                data["hooks"] = hooks
            else:
                data.pop("hooks", None)
            config_path.write_text(json.dumps(data, indent=2))
    if hook_script.exists():
        hook_script.unlink()
    print(f"{{source}} hooks removed: {{removed}}")
except Exception as e:
    print(f"{{source}} uninstall error: {{e}}")
    raise
"#,
            config_path_parts_json = config_path_parts_json,
            source_json = source_json,
        );

        let encoded = base64_encode(script.as_bytes());
        let cmd = format!(
            "\"${{SHELL:-/bin/bash}}\" -lc \"echo '{}' | base64 -d | python3\"",
            encoded
        );
        let result = run_ssh(host, &cmd, 30).await;

        if result.exit_code == 0 {
            InstallResult {
                ok: true,
                message: result.stdout.trim().to_string(),
            }
        } else {
            InstallResult {
                ok: false,
                message: format!(
                    "Uninstall failed: {}",
                    result.stderr.chars().take(200).collect::<String>()
                ),
            }
        }
    }

    fn hook_script_content(host: &RemoteHost) -> String {
        let socket = &host.remote_socket_path;
        format!(
            r#"#!/usr/bin/env python3
import json, os, socket, sys

TIMEOUT_SECONDS = 300

def _first(data, *keys):
    for key in keys:
        if isinstance(data, dict) and key in data:
            return data[key]
    return None

def _stable_text(value):
    if value is None:
        return None
    if isinstance(value, str):
        value = value.strip()
        return value or None
    try:
        return json.dumps(value, ensure_ascii=False)
    except Exception:
        return None

def _normalize_hook_event(event):
    return {{
        "session_start": "SessionStart",
        "session_end": "SessionEnd",
        "user_prompt_submit": "UserPromptSubmit",
        "pre_tool_use": "PreToolUse",
        "post_tool_use": "PostToolUse",
        "post_tool_use_failure": "PostToolUseFailure",
        "permission_request": "PermissionRequest",
        "permission_denied": "PermissionDenied",
        "stop": "Stop",
        "stop_failure": "StopFailure",
    }}.get(event, event or "")

def _detect_tty():
    for fd in (0, 1, 2):
        try:
            value = os.ttyname(fd)
        except OSError:
            continue
        if value:
            return value
    return os.environ.get("TTY")

def _read_response(sock):
    chunks = []
    while True:
        chunk = sock.recv(4096)
        if not chunk:
            break
        chunks.append(chunk)
        if b"\n" in chunk:
            break
    text = b"".join(chunks).decode(errors="replace").strip()
    if not text:
        return None
    try:
        return json.loads(text)
    except Exception:
        return None

def _send_and_maybe_receive(state, wait_response=False):
    sock_path = os.environ.get("AGENTBRO_SOCKET", "{socket}")
    state["_remote_host_id"] = os.environ.get("AGENTBRO_HOST_ID", "")
    state["_remote_host_name"] = os.environ.get("AGENTBRO_HOST_NAME", "")
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(TIMEOUT_SECONDS + 5 if wait_response else 5)
            s.connect(sock_path)
            s.sendall(json.dumps(state, ensure_ascii=False).encode() + b"\n")
            if wait_response:
                return _read_response(s)
    except Exception as e:
        print(f"Remote hook error: {{e}}", file=sys.stderr)
        sys.exit(1)
    return None

def _normalized_question_payload(tool_input):
    questions = tool_input.get("questions") if isinstance(tool_input, dict) else None
    if not isinstance(questions, list):
        questions = []
    first = questions[0] if questions and isinstance(questions[0], dict) else {{}}
    header = _stable_text(first.get("header"))
    text = _stable_text(first.get("question")) or ""
    question_text = f"[{{header}}] {{text}}" if header else text
    options = first.get("options") if isinstance(first.get("options"), list) else []
    def _question_multi_select(item):
        return bool(item.get("multiSelect") or item.get("multi_select") or item.get("multiple"))
    def _option_label(opt):
        if isinstance(opt, str):
            return opt
        if isinstance(opt, dict):
            return _stable_text(opt.get("label"))
        return None
    def _option_description(opt):
        if isinstance(opt, dict) and isinstance(opt.get("description"), str):
            return opt.get("description")
        return ""
    return {{
        "question": question_text,
        "options": [_option_label(opt) for opt in options if _option_label(opt)],
        "descriptions": [_option_description(opt) for opt in options],
        "header": header,
        "multi_select": _question_multi_select(first),
        "questions": [
            {{
                "question": _stable_text(item.get("question")) or "",
                "header": _stable_text(item.get("header")),
                "options": item.get("options") if isinstance(item.get("options"), list) else [],
                "multiSelect": _question_multi_select(item),
            }}
            for item in questions
            if isinstance(item, dict)
        ],
    }}

def _print_json(value):
    print(json.dumps(value, ensure_ascii=False))

def _permission_output_from_response(response, data, tool_input):
    if not response:
        return
    decision = response.get("decision", "ask")
    reason = response.get("reason") or ""
    always = bool(response.get("always"))
    if decision == "allow":
        permission_suggestions = data.get("permission_suggestions") or []
        body = {{"behavior": "allow"}}
        if always and permission_suggestions:
            body["updatedPermissions"] = permission_suggestions
        _print_json({{"hookSpecificOutput": {{"hookEventName": "PermissionRequest", "decision": body}}}})
    elif decision == "auto":
        _print_json({{"hookSpecificOutput": {{"hookEventName": "PermissionRequest", "decision": {{"behavior": "allow", "updatedPermissions": [{{"type": "setMode", "mode": "bypassPermissions", "destination": "session"}}]}}}}}})
    elif decision == "deny":
        _print_json({{"hookSpecificOutput": {{"hookEventName": "PermissionRequest", "decision": {{"behavior": "deny", "message": reason or "Denied by user via AgentBro"}}}}}})

def main():
    payload = sys.stdin.read()

    try:
        data = json.loads(payload)
    except Exception:
        data = {{"raw": payload}}

    if not isinstance(data, dict):
        return

    source = os.environ.get("AGENTBRO_AGENT", "claude-code")
    hook_event = _normalize_hook_event(_stable_text(_first(data, "hook_event_name", "event", "hookType")))
    session_id = _stable_text(_first(data, "session_id", "sessionId")) or "unknown"
    cwd = _stable_text(data.get("cwd")) or ""
    tool_input = _first(data, "tool_input", "toolInput")
    if not isinstance(tool_input, (dict, list)):
        tool_input = {{}}

    state = {{
        "agent": source,
        "session_id": session_id,
        "cwd": cwd,
        "event": hook_event,
        "pid": os.getppid(),
        "tty": _detect_tty(),
    }}

    transcript_path = _first(data, "transcript_path", "transcriptPath")
    if transcript_path is not None:
        state["transcript_path"] = transcript_path

    if data.get("rate_limits") is not None or data.get("context_window") is not None:
        state["event"] = "StatusLineUpdate"
        state["status"] = "processing"
        if data.get("rate_limits") is not None:
            state["rateLimits"] = data["rate_limits"]
        if data.get("context_window") is not None:
            state["contextWindow"] = data["context_window"]
        if _first(data, "status_line_text", "statusLineText") is not None:
            state["statusLineText"] = _first(data, "status_line_text", "statusLineText")
        _send_and_maybe_receive(state)
        return

    if hook_event == "UserPromptSubmit":
        state["status"] = "processing"
        prompt = _first(data, "user_prompt", "prompt")
        if prompt is not None:
            state["prompt"] = prompt
    elif hook_event == "PreToolUse":
        state["status"] = "running_tool"
        if _first(data, "tool_name", "tool") is not None:
            state["tool"] = _first(data, "tool_name", "tool")
        state["tool_input"] = tool_input
        if _first(data, "tool_use_id", "toolUseId") is not None:
            state["tool_use_id"] = _first(data, "tool_use_id", "toolUseId")
    elif hook_event in ("PostToolUse", "PostToolUseFailure", "PermissionDenied"):
        state["status"] = "processing"
        if _first(data, "tool_name", "tool") is not None:
            state["tool"] = _first(data, "tool_name", "tool")
        state["tool_input"] = tool_input
        if hook_event == "PostToolUseFailure" and _first(data, "error", "message") is not None:
            state["tool_error"] = _first(data, "error", "message")
        if _first(data, "tool_use_id", "toolUseId") is not None:
            state["tool_use_id"] = _first(data, "tool_use_id", "toolUseId")
    elif hook_event == "PermissionRequest":
        tool_name = _stable_text(_first(data, "tool_name", "tool")) or ""
        if tool_name == "AskUserQuestion":
            state["event"] = "AskQuestion"
            state["status"] = "waiting_for_input"
            state["tool_input"] = tool_input
            state.update(_normalized_question_payload(tool_input if isinstance(tool_input, dict) else {{}}))
            response = _send_and_maybe_receive(state, True)
            if response:
                updated_input = dict(tool_input) if isinstance(tool_input, dict) else {{}}
                answer = response.get("answer") or ""
                try:
                    answers = json.loads(answer)
                    if not isinstance(answers, dict):
                        raise ValueError()
                except Exception:
                    questions = updated_input.get("questions") if isinstance(updated_input.get("questions"), list) else []
                    first = questions[0] if questions and isinstance(questions[0], dict) else {{}}
                    answers = {{_stable_text(first.get("question")) or "": answer}}
                questions = updated_input.get("questions") if isinstance(updated_input.get("questions"), list) else []
                for question in questions:
                    if not isinstance(question, dict):
                        continue
                    header = _stable_text(question.get("header"))
                    text = _stable_text(question.get("question"))
                    if not header or not text:
                        continue
                    if text in answers and header not in answers:
                        answers[header] = answers[text]
                    elif header in answers and text not in answers:
                        answers[text] = answers[header]
                updated_input["answers"] = answers
                _print_json({{"hookSpecificOutput": {{"hookEventName": "PermissionRequest", "permissionDecision": "allow", "decision": {{"behavior": "allow", "updatedInput": updated_input}}, "updatedInput": updated_input}}}})
            return

        if tool_name == "ExitPlanMode":
            plan_content = _stable_text(tool_input.get("plan") if isinstance(tool_input, dict) else None) or _stable_text(tool_input.get("planContent") if isinstance(tool_input, dict) else None) or ""
            plan_title = _stable_text(tool_input.get("planTitle") if isinstance(tool_input, dict) else None) or next((line.strip().lstrip(chr(35)).strip() for line in plan_content.splitlines() if line.strip()), "Plan")
            state.update({{
                "event": "PlanApproval",
                "status": "waiting_for_approval",
                "tool": tool_name,
                "tool_input": tool_input,
                "plan_title": plan_title,
                "plan_content": plan_content,
                "requested_permissions": tool_input.get("allowedPrompts", []) if isinstance(tool_input, dict) else [],
            }})
            response = _send_and_maybe_receive(state, True)
            if response:
                mode = response.get("mode") or "manual"
                if mode == "feedback":
                    _print_json({{"hookSpecificOutput": {{"hookEventName": "PermissionRequest", "decision": {{"behavior": "deny", "message": response.get("message") or "User requested changes"}}}}}})
                else:
                    decision = {{"behavior": "allow", "updatedInput": tool_input}}
                    if mode in ("acceptEdits", "bypassPermissions"):
                        decision["updatedPermissions"] = [{{"type": "setMode", "mode": mode, "destination": "session"}}]
                    _print_json({{"hookSpecificOutput": {{"hookEventName": "PermissionRequest", "decision": decision}}}})
            return

        state["status"] = "waiting_for_approval"
        if tool_name:
            state["tool"] = tool_name
        state["tool_input"] = tool_input
        _permission_output_from_response(_send_and_maybe_receive(state, True), data, tool_input)
        return
    elif hook_event == "Notification":
        notification_type = _stable_text(data.get("notification_type")) or ""
        if notification_type == "permission_prompt":
            return
        state["status"] = "waiting_for_input" if notification_type == "idle_prompt" else "notification"
        state["notification_type"] = notification_type
        if data.get("message") is not None:
            state["message"] = data["message"]
    elif hook_event in ("Stop", "SessionStart"):
        state["status"] = "waiting_for_input"
    elif hook_event == "StopFailure":
        state["status"] = "waiting_for_input"
        if _first(data, "error", "message") is not None:
            state["stop_error"] = _first(data, "error", "message")
    elif hook_event == "SessionEnd":
        state["status"] = "ended"
    elif hook_event == "PreCompact":
        state["status"] = "compacting"
    elif hook_event == "PostCompact":
        state["status"] = "processing"
    else:
        state["status"] = "unknown"

    _send_and_maybe_receive(state)

if __name__ == "__main__":
    main()
"#,
            socket = socket
        )
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteHookEventSpec {
    name: &'static str,
    matcher: Option<&'static str>,
    timeout: Option<u64>,
}

fn remote_event_specs_json(profile: &profiles::AgentIntegrationProfile) -> String {
    let specs = profiles::effective_event_descriptors(profile)
        .iter()
        .map(|event| RemoteHookEventSpec {
            name: event.name,
            matcher: match event.template {
                HookEntryTemplate::Plain => None,
                HookEntryTemplate::Matcher(matcher) => Some(matcher),
            },
            timeout: event.timeout,
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&specs).unwrap_or_else(|_| "[]".to_string())
}

fn profile_config_path_parts_json(profile: &profiles::AgentIntegrationProfile) -> String {
    let parts = profile
        .configuration_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    serde_json::to_string(&parts).unwrap_or_else(|_| "[]".to_string())
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

struct SshResult {
    stdout: String,
    stderr: String,
    exit_code: i32,
}

async fn run_ssh(host: &RemoteHost, command: &str, timeout_secs: u64) -> SshResult {
    use std::process::Stdio;
    use tokio::time::Duration;

    let mut args = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
    ];

    if let Some(port) = host.port {
        args.push("-p".to_string());
        args.push(port.to_string());
    }
    if let Some(ref identity) = host.identity_file {
        let t = identity.trim();
        if !t.is_empty() {
            args.push("-i".to_string());
            args.push(super::path::expand_tilde(t));
        }
    }
    args.push(host.ssh_target.clone());
    args.push(command.to_string());

    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        tokio::process::Command::new("ssh")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => SshResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        },
        Ok(Err(e)) => SshResult {
            stdout: String::new(),
            stderr: format!("SSH error: {}", e),
            exit_code: -1,
        },
        Err(_) => SshResult {
            stdout: String::new(),
            stderr: format!("SSH timed out after {}s", timeout_secs),
            exit_code: -1,
        },
    }
}

fn base64_encode(data: &[u8]) -> String {
    // Standard base64 without padding newlines
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 {
            chunk[1] as usize
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            chunk[2] as usize
        } else {
            0
        };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[(n >> 18) & 63] as char);
        out.push(CHARS[(n >> 12) & 63] as char);
        out.push(if chunk.len() > 1 {
            CHARS[(n >> 6) & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[n & 63] as char
        } else {
            '='
        });
    }
    out
}
