// Remote hook installer — uploads and configures agent hooks on a remote host

use super::manager::RemoteHost;

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
        let socket = &host.remote_socket_path;
        let host_id = &host.id;
        let host_name = &host.name;

        let script = format!(
            r#"
import json, pathlib, os

home = pathlib.Path.home()
hook_cmd = (
    f"AGENTBRO_SOCKET={socket:?} "
    f"AGENTBRO_HOST_ID={host_id:?} "
    f"AGENTBRO_HOST_NAME={host_name:?} "
    "python3 ~/.agentbro/remote-hook.py"
)

# Claude Code hooks
claude_settings = home / ".claude" / "settings.json"
if claude_settings.exists():
    try:
        data = json.loads(claude_settings.read_text())
        hooks = data.setdefault("hooks", {{}})
        for event in ["PostToolUse", "Stop", "Notification"]:
            entries = hooks.setdefault(event, [])
            if not any("remote-hook.py" in str(e) for e in entries):
                entries.append({{"matcher": "", "hooks": [{{"type": "command", "command": hook_cmd}}]}})
        claude_settings.write_text(json.dumps(data, indent=2))
        print("Claude hooks configured")
    except Exception as e:
        print(f"Claude config error: {{e}}")
"#,
            socket = socket,
            host_id = host_id,
            host_name = host_name,
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

    fn hook_script_content(host: &RemoteHost) -> String {
        let socket = &host.remote_socket_path;
        format!(
            r#"#!/usr/bin/env python3
import json, os, socket, sys

def main():
    payload = sys.stdin.read()
    sock_path = os.environ.get("AGENTBRO_SOCKET", "{socket}")
    host_id = os.environ.get("AGENTBRO_HOST_ID", "")
    host_name = os.environ.get("AGENTBRO_HOST_NAME", "")

    try:
        data = json.loads(payload)
    except Exception:
        data = {{"raw": payload}}

    data["_remote_host_id"] = host_id
    data["_remote_host_name"] = host_name

    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(5)
            s.connect(sock_path)
            s.sendall(json.dumps(data).encode() + b"\n")
    except Exception as e:
        print(f"Remote hook error: {{e}}", file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    main()
"#,
            socket = socket
        )
    }
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
