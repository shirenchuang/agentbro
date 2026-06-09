// Remote attach connector — consumes the remote daemon stream over SSH and
// forwards each hook payload into the local AgentBro hook server.

use super::installer::RemoteInstaller;
use super::manager::RemoteHost;
use serde_json::Value;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(not(unix))]
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixStream;
use tokio::process::{Child, ChildStdin};

pub struct RemoteAttach {
    process: Arc<Mutex<Option<Child>>>,
}

impl RemoteAttach {
    pub fn new() -> Self {
        Self {
            process: Arc::new(Mutex::new(None)),
        }
    }

    pub fn connect(&self, host: &RemoteHost, local_socket_path: &str) -> Result<(), String> {
        self.disconnect_inner();

        let remote_command = RemoteInstaller::remote_agent_attach_command(host);
        let args = build_ssh_attach_args(host, &remote_command);
        let mut command = tokio::process::Command::new("ssh");
        command
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(ref sock) = host.auth_socket {
            if !sock.is_empty() {
                command.env("SSH_AUTH_SOCK", sock);
            }
        }

        let mut child = command
            .spawn()
            .map_err(|err| format!("ssh attach spawn failed: {err}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "ssh attach stdout unavailable".to_string())?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "ssh attach stdin unavailable".to_string())?;
        if let Some(stderr) = child.stderr.take() {
            tauri::async_runtime::spawn(read_attach_stderr(host.id.clone(), stderr));
        }

        let host_id = host.id.clone();
        let local_socket = local_socket_path.to_string();
        let stdin = Arc::new(tokio::sync::Mutex::new(stdin));
        tauri::async_runtime::spawn(async move {
            if let Err(err) = read_attach_stdout(host_id.clone(), stdout, stdin, local_socket).await
            {
                log::warn!("Remote attach stream for {} stopped: {}", host_id, err);
            }
        });

        *self.process.lock().unwrap() = Some(child);
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        let mut guard = self.process.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) | Err(_) => false,
            }
        } else {
            false
        }
    }

    pub fn disconnect(&self) {
        self.disconnect_inner();
    }

    fn disconnect_inner(&self) {
        let mut guard = self.process.lock().unwrap();
        if let Some(mut child) = guard.take() {
            let _ = child.start_kill();
        }
    }
}

impl Default for RemoteAttach {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RemoteAttach {
    fn drop(&mut self) {
        self.disconnect_inner();
    }
}

fn build_ssh_attach_args(host: &RemoteHost, remote_command: &str) -> Vec<String> {
    let mut args = vec![
        "-T".to_string(),
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=15".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=2".to_string(),
    ];

    if let Some(port) = host.port {
        args.push("-p".to_string());
        args.push(port.to_string());
    }

    if let Some(ref identity) = host.identity_file {
        let trimmed = identity.trim();
        if !trimmed.is_empty() {
            args.push("-i".to_string());
            args.push(super::path::expand_tilde(trimmed));
        }
    }

    args.push(host.ssh_target.clone());
    args.push(remote_command.to_string());
    args
}

async fn read_attach_stdout(
    host_id: String,
    stdout: tokio::process::ChildStdout,
    stdin: Arc<tokio::sync::Mutex<ChildStdin>>,
    local_socket_path: String,
) -> Result<(), String> {
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .await
            .map_err(|err| err.to_string())?;
        if bytes == 0 {
            return Ok(());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let message: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(err) => {
                log::warn!(
                    "Remote attach message decode failed for {}: {}",
                    host_id,
                    err
                );
                continue;
            }
        };
        let message_type = message
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if message_type == "hello" {
            log::info!(
                "Remote agent attached for {} ({})",
                host_id,
                message
                    .get("hostname")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            );
            continue;
        }
        if message_type != "hook_event" {
            log::debug!("Ignoring remote attach message type {}", message_type);
            continue;
        }

        let request_id = message
            .get("requestId")
            .or_else(|| message.get("requestID"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let expects_response = message
            .get("expectsResponse")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let Some(payload) = message.get("payload") else {
            continue;
        };

        match forward_to_local_hook_server(&local_socket_path, payload, expects_response).await {
            Ok(Some(response)) => {
                send_attach_response(&stdin, &request_id, Some(response), None).await?;
            }
            Ok(None) => {}
            Err(err) => {
                log::warn!(
                    "Remote attach local forward failed for {}: {}",
                    host_id,
                    err
                );
                if expects_response {
                    send_attach_response(&stdin, &request_id, None, Some(err)).await?;
                }
            }
        }
    }
}

async fn read_attach_stderr(host_id: String, stderr: tokio::process::ChildStderr) {
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    log::warn!("Remote attach stderr for {}: {}", host_id, trimmed);
                }
            }
            Err(err) => {
                log::warn!("Remote attach stderr read failed for {}: {}", host_id, err);
                break;
            }
        }
    }
}

async fn forward_to_local_hook_server(
    local_socket_path: &str,
    payload: &Value,
    expects_response: bool,
) -> Result<Option<Value>, String> {
    #[cfg(unix)]
    let mut stream = UnixStream::connect(local_socket_path)
        .await
        .map_err(|err| format!("connect local hook socket: {err}"))?;
    #[cfg(not(unix))]
    let mut stream = {
        let _ = local_socket_path;
        TcpStream::connect(crate::hook_endpoint::current().tcp_addr())
            .await
            .map_err(|err| format!("connect local hook TCP endpoint: {err}"))?
    };
    let body = serde_json::to_string(payload).map_err(|err| err.to_string())?;
    stream
        .write_all(body.as_bytes())
        .await
        .map_err(|err| err.to_string())?;
    stream
        .write_all(b"\n")
        .await
        .map_err(|err| err.to_string())?;
    stream.flush().await.map_err(|err| err.to_string())?;

    if !expects_response {
        return Ok(None);
    }

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader
        .read_line(&mut response)
        .await
        .map_err(|err| err.to_string())?;
    let response = response.trim();
    if response.is_empty() {
        return Ok(None);
    }
    serde_json::from_str(response)
        .map(Some)
        .map_err(|err| format!("decode local hook response: {err}"))
}

async fn send_attach_response(
    stdin: &Arc<tokio::sync::Mutex<ChildStdin>>,
    request_id: &str,
    payload: Option<Value>,
    error: Option<String>,
) -> Result<(), String> {
    if request_id.is_empty() {
        return Ok(());
    }

    let mut message = serde_json::Map::new();
    message.insert(
        "type".to_string(),
        Value::String("hook_response".to_string()),
    );
    message.insert(
        "requestId".to_string(),
        Value::String(request_id.to_string()),
    );
    if let Some(payload) = payload {
        message.insert("payload".to_string(), payload);
    }
    if let Some(error) = error {
        message.insert("error".to_string(), Value::String(error));
    }

    let body = serde_json::to_string(&Value::Object(message)).map_err(|err| err.to_string())?;
    let mut writer = stdin.lock().await;
    writer
        .write_all(body.as_bytes())
        .await
        .map_err(|err| err.to_string())?;
    writer
        .write_all(b"\n")
        .await
        .map_err(|err| err.to_string())?;
    writer.flush().await.map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::{build_ssh_attach_args, RemoteHost};

    #[test]
    fn attach_args_run_remote_command_without_reverse_forward() {
        let host = RemoteHost {
            id: "remote-1".to_string(),
            name: "Dev".to_string(),
            ssh_target: "dev.example.com".to_string(),
            port: Some(2222),
            identity_file: Some("~/.ssh/id_ed25519".to_string()),
            auth_socket: None,
            remote_socket_path: "/tmp/agentbro.sock".to_string(),
            auto_connect: true,
        };

        let args = build_ssh_attach_args(&host, "python3 \"$HOME/.agentbro/remote-agent.py\"");

        assert!(args.contains(&"-T".to_string()));
        assert!(!args.contains(&"-N".to_string()));
        assert!(!args.contains(&"-R".to_string()));
        assert_eq!(
            args.last().map(String::as_str),
            Some("python3 \"$HOME/.agentbro/remote-agent.py\"")
        );
    }
}
