// Remote hook installer — uploads and configures agent hooks on a remote host

use super::manager::RemoteHost;
use crate::agents::profiles::{self, HookEntryTemplate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub struct InstallResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteProbeReport {
    pub ok: bool,
    pub summary: String,
    pub checks: Vec<RemoteProbeCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteProbeCheck {
    pub id: String,
    pub label: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCodexThreadSnapshot {
    pub id: String,
    pub cwd: String,
    pub title: Option<String>,
    pub preview: Option<String>,
    pub rollout_path: Option<String>,
    pub source: Option<String>,
    pub thread_source: Option<String>,
    pub updated_at_ms: i64,
}

pub struct RemoteInstaller;

/// IDs of agents that can be installed remotely via SSH
pub const REMOTE_INSTALLABLE_AGENTS: &[&str] = &[
    "claude-code",
    "codex",
    "gemini",
    "copilot",
    "cursor-cli",
    "kiro",
];

impl RemoteInstaller {
    /// Probe a remote host for platform, tool, and AgentBro hook readiness.
    pub async fn probe_host(host: &RemoteHost) -> RemoteProbeReport {
        let command = remote_probe_command(&host.remote_socket_path);
        let result = run_ssh(host, &command, 20).await;
        if result.exit_code != 0 {
            let detail = if result.stderr.trim().is_empty() {
                "SSH probe failed".to_string()
            } else {
                result.stderr.trim().chars().take(500).collect()
            };
            return RemoteProbeReport {
                ok: false,
                summary: detail.clone(),
                checks: vec![RemoteProbeCheck {
                    id: "ssh".to_string(),
                    label: "SSH".to_string(),
                    status: "error".to_string(),
                    detail,
                }],
            };
        }

        let values = parse_probe_key_values(&result.stdout);
        let can_check_installed_hooks = values
            .get("cmd_python3")
            .is_some_and(|value| !value.is_empty())
            && values
                .get("hook_script")
                .is_some_and(|value| value != "missing");
        let installed_agents = if can_check_installed_hooks {
            Self::check_installed_agents(host).await
        } else {
            Vec::new()
        };
        let checks = build_probe_checks(&values, &installed_agents);
        let errors = checks
            .iter()
            .filter(|check| check.status == "error")
            .count();
        let warnings = checks.iter().filter(|check| check.status == "warn").count();
        let summary = if errors > 0 {
            format!("{errors} blocking issue(s), {warnings} warning(s)")
        } else if warnings > 0 {
            format!("{warnings} warning(s)")
        } else {
            "Remote host is ready".to_string()
        };

        RemoteProbeReport {
            ok: errors == 0,
            summary,
            checks,
        }
    }

    /// Install hooks for a specific agent on the remote host via SSH
    pub async fn install_hooks_for_agent(host: &RemoteHost, agent_id: &str) -> InstallResult {
        let profile = match profiles::profile_for_agent(agent_id) {
            Some(p) => p,
            None => {
                return InstallResult {
                    ok: false,
                    message: format!("Unknown agent: {}", agent_id),
                }
            }
        };

        let upload = Self::upload_hook_script(host).await;
        if !upload.ok {
            return upload;
        }

        let configure = Self::configure_hooks(host, &profile).await;
        if !configure.ok {
            return configure;
        }

        InstallResult {
            ok: true,
            message: format!("{} hooks installed", profile.source),
        }
    }

    /// Install agent hooks on the remote host via SSH (legacy, defaults to claude-code)
    pub async fn install_hooks(host: &RemoteHost) -> InstallResult {
        Self::install_hooks_for_agent(host, "claude-code").await
    }

    /// Remove hooks for a specific agent from the remote host via SSH
    pub async fn uninstall_hooks_for_agent(host: &RemoteHost, agent_id: &str) -> InstallResult {
        let profile = match profiles::profile_for_agent(agent_id) {
            Some(p) => p,
            None => {
                return InstallResult {
                    ok: false,
                    message: format!("Unknown agent: {}", agent_id),
                }
            }
        };

        let remove = Self::remove_configured_hooks(host, &profile).await;
        if !remove.ok {
            return remove;
        }

        InstallResult {
            ok: true,
            message: format!("{} hooks removed", profile.source),
        }
    }

    /// Remove AgentBro-managed agent hooks from the remote host via SSH (legacy)
    pub async fn uninstall_hooks(host: &RemoteHost) -> InstallResult {
        Self::uninstall_hooks_for_agent(host, "claude-code").await
    }

    /// Check which agents have hooks installed on the remote host.
    /// Returns a list of agent source names that are detected.
    pub async fn check_installed_agents(host: &RemoteHost) -> Vec<String> {
        let mut installed = Vec::new();
        for agent_id in REMOTE_INSTALLABLE_AGENTS {
            let profile = match profiles::profile_for_agent(agent_id) {
                Some(p) => p,
                None => continue,
            };
            if Self::check_hooks_for_profile(host, &profile).await {
                installed.push(agent_id.to_string());
            }
        }
        installed
    }

    /// Read recent Codex app-server thread activity from the remote
    /// `~/.codex/state_*.sqlite` database through the existing SSH management
    /// path.
    pub async fn read_recent_codex_threads(
        host: &RemoteHost,
        updated_since_ms: i64,
        limit: usize,
    ) -> Result<Vec<RemoteCodexThreadSnapshot>, String> {
        let script = remote_codex_state_script(updated_since_ms, limit.clamp(1, 50));
        let encoded = base64_encode(script.as_bytes());
        let cmd = format!(
            "\"${{SHELL:-/bin/bash}}\" -lc \"echo '{}' | base64 -d | python3\"",
            encoded
        );
        let result = run_ssh(host, &cmd, 25).await;
        if result.exit_code != 0 {
            let detail = if result.stderr.trim().is_empty() {
                result.stdout.trim()
            } else {
                result.stderr.trim()
            };
            return Err(detail.chars().take(500).collect());
        }
        parse_remote_codex_threads_stdout(&result.stdout)
    }

    /// Ensure the lightweight remote daemon is installed and listening without
    /// requiring a platform-specific remote sidecar binary.
    pub async fn ensure_remote_agent_running(host: &RemoteHost) -> Result<(), String> {
        let upload = Self::upload_remote_agent_script(host).await;
        if !upload.ok {
            return Err(upload.message);
        }

        let command = remote_agent_service_command(host);
        let result = run_ssh(host, &command, 20).await;
        if result.exit_code == 0 {
            Ok(())
        } else {
            let detail = if result.stderr.trim().is_empty() {
                result.stdout.trim()
            } else {
                result.stderr.trim()
            };
            Err(detail.chars().take(500).collect())
        }
    }

    /// Stop the remote daemon/attach processes and remove daemon sockets.
    pub async fn stop_remote_agent(host: &RemoteHost) {
        let command = remote_agent_stop_command(host);
        let _ = run_ssh(host, &command, 8).await;
    }

    pub fn remote_agent_attach_command(host: &RemoteHost) -> String {
        format!(
            "python3 \"$HOME/.agentbro/remote/agent.py\" --mode attach --control-socket {}",
            crate::agents::hook_manager::shell_quote(&remote_agent_control_socket_path(host))
        )
    }

    async fn check_hooks_for_profile(
        host: &RemoteHost,
        profile: &profiles::AgentIntegrationProfile,
    ) -> bool {
        let config_path_parts_json = profile_config_path_parts_json(profile);

        let script = format!(
            r#"
import json, pathlib
home = pathlib.Path.home()
config_path = home
for part in json.loads(r'''{config_path_parts_json}'''):
    config_path = config_path / part
hook_script = home / ".agentbro" / "remote" / "hook.py"
if not hook_script.exists():
    print("no_script")
elif not config_path.exists():
    print("no_config")
else:
    try:
        data = json.loads(config_path.read_text() or "{{}}")
        hooks = data.get("hooks", {{}})
        found = any(
            "remote-hook.py" in json.dumps(entry, sort_keys=True) or "remote/hook.py" in json.dumps(entry, sort_keys=True)
            for entries in hooks.values()
            if isinstance(entries, list)
            for entry in entries
        )
        print("installed" if found else "no_hook_entry")
    except Exception as e:
        print(f"error:{{e}}")
"#,
            config_path_parts_json = config_path_parts_json,
        );

        let encoded = base64_encode(script.as_bytes());
        let cmd = format!(
            "\"${{SHELL:-/bin/bash}}\" -lc \"echo '{}' | base64 -d | python3\"",
            encoded
        );
        let result = run_ssh(host, &cmd, 15).await;
        result.exit_code == 0 && result.stdout.trim() == "installed"
    }

    async fn upload_remote_agent_script(host: &RemoteHost) -> InstallResult {
        let script = remote_agent_script_content();
        let encoded = base64_encode(script.as_bytes());

        let python = format!(
            r#"import base64, os, pathlib
target = pathlib.Path.home() / ".agentbro" / "remote" / "agent.py"
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
                    "Remote agent upload failed: {}",
                    result.stderr.chars().take(200).collect::<String>()
                ),
            }
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
target = pathlib.Path.home() / ".agentbro" / "remote" / "hook.py"
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

    async fn configure_hooks(
        host: &RemoteHost,
        profile: &profiles::AgentIntegrationProfile,
    ) -> InstallResult {
        let socket = &host.remote_socket_path;
        let host_id = &host.id;
        let host_name = &host.name;
        let source = profile.source;
        let config_path_parts_json = profile_config_path_parts_json(profile);
        let event_specs_json = remote_event_specs_json(profile);
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
    "python3 ~/.agentbro/remote/hook.py",
])

def is_agentbro_entry(value):
    try:
        text = json.dumps(value, sort_keys=True)
    except Exception:
        text = str(value)
    return (
        "remote-hook.py" in text
        or "remote/hook.py" in text
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

    async fn remove_configured_hooks(
        host: &RemoteHost,
        profile: &profiles::AgentIntegrationProfile,
    ) -> InstallResult {
        let source = profile.source;
        let config_path_parts_json = profile_config_path_parts_json(profile);
        let source_json = json_string(source);

        let script = format!(
            r#"
import json, pathlib

home = pathlib.Path.home()
config_path = home
for part in json.loads(r'''{config_path_parts_json}'''):
    config_path = config_path / part
source = json.loads(r'''{source_json}''')
hook_script = home / ".agentbro" / "remote" / "hook.py"

def is_agentbro_entry(value):
    try:
        text = json.dumps(value, sort_keys=True)
    except Exception:
        text = str(value)
    return (
        "remote-hook.py" in text
        or "remote/hook.py" in text
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
import json, os, socket, sys, time

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
            time.sleep(0.1)
    except Exception as e:
        print(f"Remote hook error: {{e}}", file=sys.stderr)
        sys.exit(1)
    return None

def _text_from_transcript_value(value):
    if value is None:
        return None
    if isinstance(value, str):
        text = value.strip()
        return text or None
    if isinstance(value, list):
        parts = []
        for item in value:
            text = _text_from_transcript_value(item)
            if text:
                parts.append(text)
        return "\n".join(parts).strip() or None
    if isinstance(value, dict):
        if value.get("type") == "text":
            text = _stable_text(value.get("text"))
            if text:
                return text
        for key in ("text", "content", "message", "summary"):
            text = _text_from_transcript_value(value.get(key))
            if text:
                return text
    return None

def _safe_state_name(session_id):
    text = _stable_text(session_id) or "unknown"
    return "".join(ch if ch.isalnum() or ch in ("-", "_", ".") else "_" for ch in text)[:160]

def _session_state_path(session_id):
    root = os.path.expanduser("~/.agentbro/remote/state")
    try:
        os.makedirs(root, exist_ok=True)
    except Exception:
        return None
    return os.path.join(root, _safe_state_name(session_id) + ".json")

def _transcript_line_count(path):
    path = _stable_text(path)
    if not path:
        return 0
    try:
        with open(path, "r", encoding="utf-8") as handle:
            return sum(1 for _ in handle)
    except Exception:
        return 0

def _remember_prompt_boundary(session_id, transcript_path):
    state_path = _session_state_path(session_id)
    if not state_path:
        return
    try:
        with open(state_path, "w", encoding="utf-8") as handle:
            json.dump({{"line_count": _transcript_line_count(transcript_path)}}, handle)
    except Exception:
        pass

def _prompt_boundary_line(session_id):
    state_path = _session_state_path(session_id)
    if not state_path:
        return 0
    try:
        with open(state_path, "r", encoding="utf-8") as handle:
            data = json.load(handle)
        return int(data.get("line_count") or 0)
    except Exception:
        return 0

def _latest_assistant_from_transcript(path, after_line=0):
    path = _stable_text(path)
    if not path:
        return None
    try:
        with open(path, "r", encoding="utf-8") as handle:
            lines = list(enumerate(handle.readlines(), start=1))[-240:]
    except Exception:
        return None
    for line_no, line in reversed(lines):
        if line_no <= after_line:
            continue
        try:
            item = json.loads(line)
        except Exception:
            continue
        if not isinstance(item, dict):
            continue
        message = item.get("message") if isinstance(item.get("message"), dict) else item
        item_type = _stable_text(item.get("type"))
        role = _stable_text(message.get("role") if isinstance(message, dict) else None)
        if item_type != "assistant" and role != "assistant":
            continue
        text = _text_from_transcript_value(message.get("content") if isinstance(message, dict) else None)
        if text:
            return text
    return None

def _latest_assistant_after_prompt(session_id, transcript_path):
    after_line = _prompt_boundary_line(session_id)
    if not after_line:
        return None
    for _ in range(8):
        text = _latest_assistant_from_transcript(transcript_path, after_line)
        if text:
            return text
        time.sleep(0.25)
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
        _remember_prompt_boundary(session_id, transcript_path)
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
    elif hook_event == "Stop":
        state["status"] = "waiting_for_input"
        summary = _stable_text(_first(data, "summary", "message")) or _latest_assistant_after_prompt(session_id, transcript_path)
        if summary:
            state["summary"] = summary
    elif hook_event == "SessionStart":
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

fn remote_agent_control_socket_path(host: &RemoteHost) -> String {
    format!("{}.control", host.remote_socket_path)
}

fn remote_agent_service_pattern(host: &RemoteHost) -> String {
    format!(
        ".agentbro/remote/agent.py --mode service --hook-socket {}",
        host.remote_socket_path
    )
}

fn remote_agent_attach_pattern(host: &RemoteHost) -> String {
    format!(
        ".agentbro/remote/agent.py --mode attach --control-socket {}",
        remote_agent_control_socket_path(host)
    )
}

fn remote_agent_service_command(host: &RemoteHost) -> String {
    let quote = crate::agents::hook_manager::shell_quote;
    let hook_socket = quote(&host.remote_socket_path);
    let control_socket_path = remote_agent_control_socket_path(host);
    let control_socket = quote(&control_socket_path);
    let service_pattern = quote(&remote_agent_service_pattern(host));
    let host_id = quote(&host.id);
    let host_name = quote(&host.name);

    format!(
        r#"
hook_socket={hook_socket}
control_socket={control_socket}
install_root="${{HOME:-.}}/.agentbro/remote"
agent_script="$install_root/agent.py"
mkdir -p "$install_root" "$install_root/run" "$install_root/logs" "$(dirname "$hook_socket")" "$(dirname "$control_socket")"
if [ -S "$hook_socket" ] && [ -S "$control_socket" ]; then
  exit 0
fi
pkill -f {service_pattern} >/dev/null 2>&1 || true
rm -f "$hook_socket" "$control_socket"
nohup python3 "$agent_script" --mode service --hook-socket "$hook_socket" --control-socket "$control_socket" --host-id {host_id} --host-name {host_name} > "$install_root/logs/remote-agent.log" 2>&1 &
sleep 1
if [ -S "$hook_socket" ] && [ -S "$control_socket" ]; then
  exit 0
fi
echo "AgentBro remote agent failed to start" >&2
tail -n 40 "$install_root/logs/remote-agent.log" >&2 2>/dev/null || true
exit 1
"#
    )
}

fn remote_agent_stop_command(host: &RemoteHost) -> String {
    let quote = crate::agents::hook_manager::shell_quote;
    let hook_socket = quote(&host.remote_socket_path);
    let control_socket_path = remote_agent_control_socket_path(host);
    let control_socket = quote(&control_socket_path);
    let service_pattern = quote(&remote_agent_service_pattern(host));
    let attach_pattern = quote(&remote_agent_attach_pattern(host));

    format!(
        r#"
pkill -f {attach_pattern} >/dev/null 2>&1 || true
pkill -f {service_pattern} >/dev/null 2>&1 || true
sleep 1
rm -f {hook_socket} {control_socket}
"#
    )
}

fn remote_agent_script_content() -> String {
    r#"#!/usr/bin/env python3
import argparse
import json
import os
import socket
import sys
import threading
import time
import uuid

TIMEOUT_SECONDS = 21600


def read_line(sock):
    chunks = []
    while True:
        chunk = sock.recv(4096)
        if not chunk:
            break
        chunks.append(chunk)
        if b"\n" in chunk:
            break
    if not chunks:
        return ""
    return b"".join(chunks).split(b"\n", 1)[0].decode("utf-8", errors="replace")


def write_json_line(sock, value):
    data = json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + b"\n"
    sock.sendall(data)


def expects_response(payload):
    event = str(payload.get("event") or "")
    status = str(payload.get("status") or "")
    return event in ("PermissionRequest", "AskQuestion", "PlanApproval") or status in (
        "waiting_for_approval",
        "waiting_for_input",
        "waiting_for_plan",
    )


def make_server(path):
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass
    parent = os.path.dirname(path)
    if parent:
        os.makedirs(parent, exist_ok=True)
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(path)
    try:
        os.chmod(path, 0o700)
    except OSError:
        pass
    server.listen(32)
    return server


class RemoteAgentService:
    def __init__(self, hook_socket, control_socket, host_id="", host_name=""):
        self.hook_socket = hook_socket
        self.control_socket = control_socket
        self.host_id = host_id
        self.host_name = host_name
        self.lock = threading.RLock()
        self.control = None
        self.pending = {}
        self.queue = []

    def run(self):
        hook_server = make_server(self.hook_socket)
        control_server = make_server(self.control_socket)
        threading.Thread(target=self.accept_loop, args=(hook_server, self.handle_hook), daemon=True).start()
        threading.Thread(target=self.accept_loop, args=(control_server, self.handle_control), daemon=True).start()
        while True:
            time.sleep(3600)

    def accept_loop(self, server, handler):
        while True:
            try:
                conn, _ = server.accept()
            except OSError:
                continue
            threading.Thread(target=handler, args=(conn,), daemon=True).start()

    def handle_hook(self, conn):
        try:
            conn.settimeout(TIMEOUT_SECONDS)
            line = read_line(conn)
            if not line:
                conn.close()
                return
            try:
                payload = json.loads(line)
            except Exception:
                conn.close()
                return
            if not isinstance(payload, dict):
                conn.close()
                return
            if self.host_id and not payload.get("_remote_host_id"):
                payload["_remote_host_id"] = self.host_id
            if self.host_name and not payload.get("_remote_host_name"):
                payload["_remote_host_name"] = self.host_name

            request_id = str(uuid.uuid4())
            wants_response = expects_response(payload)
            if wants_response:
                with self.lock:
                    if self.control is None:
                        conn.close()
                        return
                    self.pending[request_id] = conn

            self.enqueue(
                {
                    "type": "hook_event",
                    "requestId": request_id,
                    "expectsResponse": wants_response,
                    "payload": payload,
                }
            )

            if not wants_response:
                conn.close()
        except Exception as exc:
            print(f"remote hook handling failed: {exc}", file=sys.stderr)
            try:
                conn.close()
            except Exception:
                pass

    def handle_control(self, conn):
        with self.lock:
            if self.control is not None:
                try:
                    self.control.close()
                except Exception:
                    pass
            self.control = conn
            self.fail_open_pending_locked()
            self.send_locked(
                {
                    "type": "hello",
                    "version": "agentbro-python-remote-agent",
                    "hostname": socket.gethostname(),
                }
            )
            self.flush_locked()

        buffer = b""
        try:
            while True:
                chunk = conn.recv(4096)
                if not chunk:
                    break
                buffer += chunk
                while b"\n" in buffer:
                    line, buffer = buffer.split(b"\n", 1)
                    if line.strip():
                        self.handle_control_line(line)
        finally:
            with self.lock:
                if self.control is conn:
                    self.control = None
                    self.fail_open_pending_locked()
            try:
                conn.close()
            except Exception:
                pass

    def handle_control_line(self, line):
        try:
            message = json.loads(line.decode("utf-8", errors="replace"))
        except Exception:
            return
        if not isinstance(message, dict) or message.get("type") != "hook_response":
            return
        request_id = message.get("requestId") or message.get("requestID")
        if not request_id:
            return
        with self.lock:
            conn = self.pending.pop(str(request_id), None)
        if conn is None:
            return
        try:
            payload = message.get("payload")
            if payload is not None:
                write_json_line(conn, payload)
        finally:
            try:
                conn.close()
            except Exception:
                pass

    def enqueue(self, message):
        data = json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + b"\n"
        with self.lock:
            if self.control is None:
                self.queue.append(data)
                if len(self.queue) > 128:
                    self.queue = self.queue[-128:]
                return
            try:
                self.control.sendall(data)
            except Exception:
                self.queue.append(data)
                self.control = None

    def send_locked(self, message):
        if self.control is None:
            return
        data = json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + b"\n"
        self.control.sendall(data)

    def flush_locked(self):
        if self.control is None:
            return
        while self.queue:
            data = self.queue.pop(0)
            self.control.sendall(data)

    def fail_open_pending_locked(self):
        pending = list(self.pending.values())
        self.pending.clear()
        for conn in pending:
            try:
                conn.close()
            except Exception:
                pass


def relay(src, dst, close_write=False):
    try:
        while True:
            data = src.recv(4096) if hasattr(src, "recv") else src.read(4096)
            if not data:
                break
            if hasattr(dst, "sendall"):
                dst.sendall(data)
            else:
                dst.write(data)
                dst.flush()
    finally:
        if close_write and hasattr(dst, "shutdown"):
            try:
                dst.shutdown(socket.SHUT_WR)
            except Exception:
                pass


def run_attach(control_socket):
    conn = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    conn.connect(control_socket)
    stdin = sys.stdin.buffer
    stdout = sys.stdout.buffer
    writer = threading.Thread(target=relay, args=(stdin, conn, True), daemon=True)
    writer.start()
    relay(conn, stdout)
    writer.join(timeout=1)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", required=True, choices=["service", "attach"])
    parser.add_argument("--hook-socket")
    parser.add_argument("--control-socket", required=True)
    parser.add_argument("--host-id", default="")
    parser.add_argument("--host-name", default="")
    args = parser.parse_args()

    if args.mode == "service":
        if not args.hook_socket:
            parser.error("--hook-socket is required for service mode")
        RemoteAgentService(args.hook_socket, args.control_socket, args.host_id, args.host_name).run()
    else:
        run_attach(args.control_socket)


if __name__ == "__main__":
    main()
"#
    .to_string()
}

fn remote_probe_command(remote_socket_path: &str) -> String {
    let socket_path = crate::agents::hook_manager::shell_quote(remote_socket_path);
    let script = format!(
        r#"
socket_path={socket_path}
control_path="${{socket_path}}.control"
printf 'os=%s\n' "$(uname -s 2>/dev/null || true)"
printf 'arch=%s\n' "$(uname -m 2>/dev/null || true)"
printf 'home=%s\n' "${{HOME:-}}"
printf 'shell=%s\n' "${{SHELL:-}}"
for item in \
  python3:python3 \
  tmux:tmux \
  claude_code:claude \
  codex:codex \
  gemini:gemini \
  qwen:qwen \
  kimi:kimi \
  opencode:opencode \
  cursor_cli:cursor-agent \
  copilot:gh \
  qoder:qoder \
  codebuddy:codebuddy
do
  key="${{item%%:*}}"
  cmd="${{item#*:}}"
  path="$(command -v "$cmd" 2>/dev/null || true)"
  printf 'cmd_%s=%s\n' "$key" "$path"
done
if [ -S "$socket_path" ]; then
  printf 'remote_socket=socket\n'
elif [ -e "$socket_path" ]; then
  printf 'remote_socket=present\n'
else
  printf 'remote_socket=missing\n'
fi
if [ -S "$control_path" ]; then
  printf 'remote_agent_control=socket\n'
elif [ -e "$control_path" ]; then
  printf 'remote_agent_control=present\n'
else
  printf 'remote_agent_control=missing\n'
fi
agent_script="${{HOME:-}}/.agentbro/remote/agent.py"
if [ -x "$agent_script" ]; then
  printf 'remote_agent_script=executable\n'
elif [ -f "$agent_script" ]; then
  printf 'remote_agent_script=present\n'
else
  printf 'remote_agent_script=missing\n'
fi
if command -v pgrep >/dev/null 2>&1 && pgrep -f ".agentbro/remote/agent.py --mode service --hook-socket $socket_path" >/dev/null 2>&1; then
  printf 'remote_agent_service=running\n'
else
  printf 'remote_agent_service=stopped\n'
fi
hook_script="${{HOME:-}}/.agentbro/remote/hook.py"
if [ -x "$hook_script" ]; then
  printf 'hook_script=executable\n'
elif [ -f "$hook_script" ]; then
  printf 'hook_script=present\n'
else
  printf 'hook_script=missing\n'
fi
if command -v python3 >/dev/null 2>&1; then
  python3 - <<'PY' 2>/dev/null || printf 'codex_state=unreadable\n'
import glob, os, pathlib, re, sqlite3
paths = []
for path in glob.glob(str(pathlib.Path.home() / ".codex" / "state_*.sqlite")):
    name = os.path.basename(path)
    match = re.match(r"^state_(\d+)\.sqlite$", name)
    if match:
        paths.append((int(match.group(1)), path))
if not paths:
    print("codex_state=missing")
else:
    path = max(paths, key=lambda item: item[0])[1]
    conn = sqlite3.connect("file:" + path + "?mode=ro", uri=True)
    try:
        columns = {{row[1] for row in conn.execute("PRAGMA table_info(threads)")}}
        print("codex_state=present" if {{"id", "cwd"}}.issubset(columns) else "codex_state=unreadable")
    finally:
        conn.close()
PY
else
  printf 'codex_state=python_missing\n'
fi
"#
    );
    format!(
        "sh -lc {}",
        crate::agents::hook_manager::shell_quote(&script)
    )
}

fn remote_codex_state_script(updated_since_ms: i64, limit: usize) -> String {
    format!(
        r#"
import glob
import json
import os
import pathlib
import re
import sqlite3
import sys

UPDATED_SINCE_MS = {updated_since_ms}
LIMIT = {limit}

def newest_state_database(codex_home):
    candidates = []
    for path in glob.glob(str(codex_home / "state_*.sqlite")):
        name = os.path.basename(path)
        match = re.match(r"^state_(\d+)\.sqlite$", name)
        if match:
            candidates.append((int(match.group(1)), path))
    if not candidates:
        return None
    return max(candidates, key=lambda item: item[0])[1]

def nullable_text(name, columns):
    return "NULLIF({{0}}, '')".format(name) if name in columns else "NULL"

def coalesce_text(names, columns):
    return "COALESCE({{0}})".format(", ".join(nullable_text(name, columns) for name in names))

def updated_expression(columns):
    candidates = []
    if "updated_at_ms" in columns:
        candidates.append("updated_at_ms")
    if "updated_at" in columns:
        candidates.append("updated_at * 1000")
    if "created_at_ms" in columns:
        candidates.append("created_at_ms")
    if "created_at" in columns:
        candidates.append("created_at * 1000")
    if not candidates:
        return None
    return "COALESCE({{0}})".format(", ".join(candidates))

def main():
    codex_home = pathlib.Path.home() / ".codex"
    database_path = newest_state_database(codex_home)
    if not database_path:
        print("[]")
        return

    uri = "file:" + database_path + "?mode=ro"
    conn = sqlite3.connect(uri, uri=True)
    try:
        columns = {{row[1] for row in conn.execute("PRAGMA table_info(threads)")}}
        updated = updated_expression(columns)
        if "id" not in columns or "cwd" not in columns or not updated:
            print("[]")
            return

        title = coalesce_text(["title", "first_user_message"], columns)
        preview = nullable_text("preview", columns)
        rollout_path = nullable_text("rollout_path", columns)
        source = nullable_text("source", columns)
        thread_source = nullable_text("thread_source", columns)
        archived_predicate = "AND COALESCE(archived, 0) = 0" if "archived" in columns else ""
        query = f"""
        SELECT
          id,
          cwd,
          {{title}} AS title,
          {{preview}} AS preview,
          {{rollout_path}} AS rolloutPath,
          {{source}} AS source,
          {{thread_source}} AS threadSource,
          {{updated}} AS updatedAtMs
        FROM threads
        WHERE cwd != ''
          {{archived_predicate}}
          AND COALESCE({{preview}}, {{title}}) IS NOT NULL
          AND {{updated}} >= ?
        ORDER BY updatedAtMs DESC
        LIMIT ?
        """
        rows = []
        for row in conn.execute(query, (UPDATED_SINCE_MS, LIMIT)):
            rows.append({{
                "id": row[0],
                "cwd": row[1],
                "title": row[2],
                "preview": row[3],
                "rolloutPath": row[4],
                "source": row[5],
                "threadSource": row[6],
                "updatedAtMs": int(row[7] or 0),
            }})
        print(json.dumps(rows, ensure_ascii=False))
    finally:
        conn.close()

try:
    main()
except Exception as exc:
    print(json.dumps({{"error": str(exc)}}), file=sys.stderr)
    sys.exit(1)
"#
    )
}

fn parse_remote_codex_threads_stdout(
    stdout: &str,
) -> Result<Vec<RemoteCodexThreadSnapshot>, String> {
    let payload = remote_codex_threads_json_payload(stdout)?;
    let value: serde_json::Value = serde_json::from_str(payload)
        .map_err(|err| format!("Invalid remote Codex state JSON: {err}"))?;
    let Some(items) = value.as_array() else {
        return Err("Remote Codex state probe returned a non-array payload".to_string());
    };
    let mut threads = Vec::new();
    for item in items {
        let Ok(thread) = serde_json::from_value::<RemoteCodexThreadSnapshot>(item.clone()) else {
            continue;
        };
        if thread.id.trim().is_empty() || thread.cwd.trim().is_empty() {
            continue;
        }
        threads.push(thread);
    }
    Ok(threads)
}

fn remote_codex_threads_json_payload(stdout: &str) -> Result<&str, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        return Ok("[]");
    }
    if trimmed.starts_with('[') {
        return Ok(trimmed);
    }
    if let Some(line) = trimmed
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| line.starts_with('['))
    {
        return Ok(line);
    }
    if trimmed.starts_with('{') {
        return Ok(trimmed);
    }
    Err("Remote Codex state probe returned no JSON payload".to_string())
}

fn parse_probe_key_values(stdout: &str) -> BTreeMap<String, String> {
    stdout
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn build_probe_checks(
    values: &BTreeMap<String, String>,
    installed_agents: &[String],
) -> Vec<RemoteProbeCheck> {
    let mut checks = vec![RemoteProbeCheck {
        id: "ssh".to_string(),
        label: "SSH".to_string(),
        status: "ok".to_string(),
        detail: "Connected".to_string(),
    }];
    checks.push(required_value_check(values, "os", "Operating system"));
    checks.push(required_value_check(values, "arch", "Architecture"));
    checks.push(required_value_check(values, "home", "Home directory"));
    checks.push(optional_value_check(values, "shell", "Login shell"));
    checks.push(command_check(values, "python3", "Python 3", true));
    checks.push(command_check(values, "tmux", "tmux", false));

    let agent_commands = [
        ("claude_code", "Claude Code"),
        ("codex", "Codex"),
        ("gemini", "Gemini CLI"),
        ("qwen", "Qwen"),
        ("kimi", "Kimi"),
        ("opencode", "OpenCode"),
        ("cursor_cli", "Cursor CLI"),
        ("copilot", "GitHub Copilot CLI"),
        ("qoder", "Qoder"),
        ("codebuddy", "CodeBuddy"),
    ];
    let available_agents = agent_commands
        .iter()
        .filter_map(|(id, label)| {
            let key = format!("cmd_{id}");
            values
                .get(&key)
                .filter(|value| !value.is_empty())
                .map(|_| *label)
        })
        .collect::<Vec<_>>();
    checks.push(RemoteProbeCheck {
        id: "agent_commands".to_string(),
        label: "Agent CLIs".to_string(),
        status: if available_agents.is_empty() {
            "warn".to_string()
        } else {
            "ok".to_string()
        },
        detail: if available_agents.is_empty() {
            "No supported agent CLI found on PATH".to_string()
        } else {
            available_agents.join(", ")
        },
    });

    let socket_state = values
        .get("remote_socket")
        .map(String::as_str)
        .unwrap_or("missing");
    checks.push(RemoteProbeCheck {
        id: "remote_socket".to_string(),
        label: "Remote socket".to_string(),
        status: if socket_state == "socket" {
            "ok"
        } else {
            "warn"
        }
        .to_string(),
        detail: match socket_state {
            "socket" => "Reverse tunnel socket is present".to_string(),
            "present" => "Path exists but is not a Unix socket".to_string(),
            _ => "Reverse tunnel socket is missing".to_string(),
        },
    });

    let agent_script = values
        .get("remote_agent_script")
        .map(String::as_str)
        .unwrap_or("missing");
    let agent_control = values
        .get("remote_agent_control")
        .map(String::as_str)
        .unwrap_or("missing");
    let agent_service = values
        .get("remote_agent_service")
        .map(String::as_str)
        .unwrap_or("stopped");
    let agent_ready =
        agent_script == "executable" && agent_control == "socket" && agent_service == "running";
    checks.push(RemoteProbeCheck {
        id: "remote_agent".to_string(),
        label: "Remote agent".to_string(),
        status: if agent_ready { "ok" } else { "warn" }.to_string(),
        detail: if agent_ready {
            "Attach daemon is running".to_string()
        } else {
            format!("script: {agent_script}, control: {agent_control}, service: {agent_service}")
        },
    });

    let hook_script = values
        .get("hook_script")
        .map(String::as_str)
        .unwrap_or("missing");
    checks.push(RemoteProbeCheck {
        id: "hook_script".to_string(),
        label: "Remote hook script".to_string(),
        status: match hook_script {
            "executable" => "ok",
            "present" => "warn",
            _ => "warn",
        }
        .to_string(),
        detail: match hook_script {
            "executable" => "~/.agentbro/remote/hook.py is executable".to_string(),
            "present" => "~/.agentbro/remote/hook.py exists but is not executable".to_string(),
            _ => "~/.agentbro/remote/hook.py is not installed".to_string(),
        },
    });

    let codex_state = values
        .get("codex_state")
        .map(String::as_str)
        .unwrap_or("missing");
    checks.push(RemoteProbeCheck {
        id: "codex_state".to_string(),
        label: "Codex state database".to_string(),
        status: if codex_state == "present" {
            "ok"
        } else {
            "warn"
        }
        .to_string(),
        detail: match codex_state {
            "present" => "~/.codex/state_*.sqlite is readable".to_string(),
            "python_missing" => "Python 3 is required to inspect remote Codex state".to_string(),
            "unreadable" => "~/.codex/state_*.sqlite exists but could not be read".to_string(),
            _ => "No remote Codex state database detected".to_string(),
        },
    });

    checks.push(RemoteProbeCheck {
        id: "installed_hooks".to_string(),
        label: "Installed hooks".to_string(),
        status: if installed_agents.is_empty() {
            "warn".to_string()
        } else {
            "ok".to_string()
        },
        detail: if installed_agents.is_empty() {
            "No AgentBro-managed hooks detected".to_string()
        } else {
            installed_agents.join(", ")
        },
    });

    checks
}

fn required_value_check(
    values: &BTreeMap<String, String>,
    key: &str,
    label: &str,
) -> RemoteProbeCheck {
    let value = values.get(key).cloned().unwrap_or_default();
    RemoteProbeCheck {
        id: key.to_string(),
        label: label.to_string(),
        status: if value.is_empty() { "error" } else { "ok" }.to_string(),
        detail: if value.is_empty() {
            "Not detected".to_string()
        } else {
            value
        },
    }
}

fn optional_value_check(
    values: &BTreeMap<String, String>,
    key: &str,
    label: &str,
) -> RemoteProbeCheck {
    let value = values.get(key).cloned().unwrap_or_default();
    RemoteProbeCheck {
        id: key.to_string(),
        label: label.to_string(),
        status: if value.is_empty() { "warn" } else { "ok" }.to_string(),
        detail: if value.is_empty() {
            "Not detected".to_string()
        } else {
            value
        },
    }
}

fn command_check(
    values: &BTreeMap<String, String>,
    id: &str,
    label: &str,
    required: bool,
) -> RemoteProbeCheck {
    let key = format!("cmd_{id}");
    let value = values.get(&key).cloned().unwrap_or_default();
    RemoteProbeCheck {
        id: key,
        label: label.to_string(),
        status: if value.is_empty() {
            if required {
                "error"
            } else {
                "warn"
            }
        } else {
            "ok"
        }
        .to_string(),
        detail: if value.is_empty() {
            "Not found on PATH".to_string()
        } else {
            value
        },
    }
}

pub(super) struct SshResult {
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) exit_code: i32,
}

pub(super) async fn run_ssh(host: &RemoteHost, command: &str, timeout_secs: u64) -> SshResult {
    use std::process::Stdio;
    use tokio::time::Duration;

    let args = ssh_command_args(host, command);

    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        tokio::process::Command::new(crate::agents::executable::command_path("ssh"))
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

pub(super) async fn run_ssh_with_input(
    host: &RemoteHost,
    command: &str,
    input: Vec<u8>,
    timeout_secs: u64,
) -> SshResult {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tokio::time::Duration;

    let args = ssh_command_args(host, command);
    let mut child =
        match tokio::process::Command::new(crate::agents::executable::command_path("ssh"))
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                return SshResult {
                    stdout: String::new(),
                    stderr: format!("SSH error: {error}"),
                    exit_code: -1,
                }
            }
        };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(error) = stdin.write_all(&input).await {
            let _ = child.kill().await;
            return SshResult {
                stdout: String::new(),
                stderr: format!("SSH upload error: {error}"),
                exit_code: -1,
            };
        }
    }

    match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await {
        Ok(Ok(output)) => SshResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        },
        Ok(Err(error)) => SshResult {
            stdout: String::new(),
            stderr: format!("SSH error: {error}"),
            exit_code: -1,
        },
        Err(_) => SshResult {
            stdout: String::new(),
            stderr: format!("SSH timed out after {timeout_secs}s"),
            exit_code: -1,
        },
    }
}

fn ssh_command_args(host: &RemoteHost, command: &str) -> Vec<String> {
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
    args
}

pub(super) fn base64_encode(data: &[u8]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> RemoteHost {
        RemoteHost {
            id: "remote-1".to_string(),
            name: "GPU Box".to_string(),
            ssh_target: "alice@example.com".to_string(),
            port: Some(2222),
            identity_file: Some("~/.ssh/id_ed25519".to_string()),
            auth_socket: None,
            remote_socket_path: "/tmp/agentbro-remote.sock".to_string(),
            auto_connect: true,
        }
    }

    #[test]
    fn parse_probe_key_values_keeps_values_after_first_equals() {
        let values = parse_probe_key_values("os=Darwin\nhome=/Users/me\nshell=/bin/zsh\n");

        assert_eq!(values.get("os").map(String::as_str), Some("Darwin"));
        assert_eq!(values.get("home").map(String::as_str), Some("/Users/me"));
        assert_eq!(values.get("shell").map(String::as_str), Some("/bin/zsh"));
    }

    #[test]
    fn probe_checks_report_missing_required_python_as_error() {
        let values = parse_probe_key_values(
            "os=Linux\narch=arm64\nhome=/home/me\nshell=/bin/bash\ncmd_python3=\nremote_socket=missing\nhook_script=missing\n",
        );
        let checks = build_probe_checks(&values, &[]);

        let python = checks
            .iter()
            .find(|check| check.id == "cmd_python3")
            .expect("python check");
        let socket = checks
            .iter()
            .find(|check| check.id == "remote_socket")
            .expect("socket check");

        assert_eq!(python.status, "error");
        assert_eq!(socket.status, "warn");
    }

    #[test]
    fn probe_checks_report_installed_hooks() {
        let values = parse_probe_key_values(
            "os=Darwin\narch=arm64\nhome=/Users/me\nshell=/bin/zsh\ncmd_python3=/usr/bin/python3\ncmd_tmux=/opt/homebrew/bin/tmux\ncmd_codex=/opt/homebrew/bin/codex\nremote_socket=socket\nhook_script=executable\ncodex_state=present\n",
        );
        let checks = build_probe_checks(&values, &["codex".to_string()]);

        let hooks = checks
            .iter()
            .find(|check| check.id == "installed_hooks")
            .expect("installed hooks check");
        let agents = checks
            .iter()
            .find(|check| check.id == "agent_commands")
            .expect("agent commands check");

        assert_eq!(hooks.status, "ok");
        assert_eq!(hooks.detail, "codex");
        assert_eq!(agents.status, "ok");
        assert!(agents.detail.contains("Codex"));
        let codex_state = checks
            .iter()
            .find(|check| check.id == "codex_state")
            .expect("codex state check");
        assert_eq!(codex_state.status, "ok");
    }

    #[test]
    fn remote_agent_paths_and_commands_match_attach_topology() {
        let host = host();

        assert_eq!(
            remote_agent_control_socket_path(&host),
            "/tmp/agentbro-remote.sock.control"
        );

        let ensure = remote_agent_service_command(&host);
        assert!(ensure.contains("remote/agent.py"));
        assert!(ensure.contains("--mode service"));
        assert!(ensure.contains("--hook-socket \"$hook_socket\""));
        assert!(ensure.contains("--control-socket \"$control_socket\""));
        assert!(ensure.contains("nohup python3 \"$agent_script\""));
        assert!(ensure.contains("AgentBro remote agent failed to start"));

        let attach = RemoteInstaller::remote_agent_attach_command(&host);
        assert!(attach.contains("--mode attach"));
        assert!(attach.contains("--control-socket /tmp/agentbro-remote.sock.control"));

        let stop = remote_agent_stop_command(&host);
        assert!(stop.contains("--mode attach --control-socket /tmp/agentbro-remote.sock.control"));
        assert!(stop.contains("--mode service --hook-socket /tmp/agentbro-remote.sock"));
        assert!(stop.contains("rm -f /tmp/agentbro-remote.sock /tmp/agentbro-remote.sock.control"));
    }

    #[test]
    fn probe_checks_report_running_remote_agent() {
        let values = parse_probe_key_values(
            "os=Linux\narch=x86_64\nhome=/home/me\nshell=/bin/bash\ncmd_python3=/usr/bin/python3\nremote_socket=socket\nremote_agent_control=socket\nremote_agent_script=executable\nremote_agent_service=running\nhook_script=executable\ncodex_state=missing\n",
        );
        let checks = build_probe_checks(&values, &[]);

        let remote_agent = checks
            .iter()
            .find(|check| check.id == "remote_agent")
            .expect("remote agent check");

        assert_eq!(remote_agent.status, "ok");
        assert_eq!(remote_agent.detail, "Attach daemon is running");
    }

    #[test]
    fn parses_remote_codex_thread_snapshots() {
        let stdout = r#"
[
  {
    "id": "thread-1",
    "cwd": "/work/project",
    "title": "Build the island",
    "preview": "Implemented the island",
    "rolloutPath": "/home/me/.codex/sessions/rollout.jsonl",
    "source": "codex",
    "threadSource": "app-server",
    "updatedAtMs": 1780070000123
  },
  { "id": "", "cwd": "/ignored", "updatedAtMs": 1 }
]
"#;
        let threads = parse_remote_codex_threads_stdout(stdout).expect("parse threads");

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "thread-1");
        assert_eq!(threads[0].cwd, "/work/project");
        assert_eq!(
            threads[0].preview.as_deref(),
            Some("Implemented the island")
        );
        assert_eq!(threads[0].thread_source.as_deref(), Some("app-server"));
        assert_eq!(threads[0].updated_at_ms, 1780070000123);
    }

    #[test]
    fn remote_codex_thread_parser_skips_stdout_noise() {
        let stdout = "Last login: Sat May 30\nwarning: shell banner\n[{\"id\":\"t\",\"cwd\":\"/repo\",\"updatedAtMs\":2}]\n";
        let threads = parse_remote_codex_threads_stdout(stdout).expect("parse noisy stdout");

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "t");
    }

    #[test]
    fn remote_codex_thread_parser_rejects_non_array_payload() {
        let err = parse_remote_codex_threads_stdout(r#"{"error":"boom"}"#)
            .expect_err("non-array payload should fail");

        assert!(err.contains("non-array"));
    }
}
