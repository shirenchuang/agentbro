// SSH tunnel — reverse-forward local Unix socket to remote host
// Uses `ssh -N -R <remote_socket>:<local_socket>` with keepalive options.

use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

/// SSH tunnel connection status
#[derive(Debug, Clone, PartialEq)]
pub enum TunnelStatus {
    Disconnected,
    Connecting,
    Connected,
    Failed(String),
}

/// Manages a single SSH reverse tunnel process
pub struct SshTunnel {
    process: Arc<Mutex<Option<Child>>>,
    generation: Arc<Mutex<u64>>,
}

impl SshTunnel {
    pub fn new() -> Self {
        Self {
            process: Arc::new(Mutex::new(None)),
            generation: Arc::new(Mutex::new(0)),
        }
    }

    /// Start the SSH tunnel. Returns Ok(()) immediately if the process launches;
    /// actual connection state is polled via `is_running()`.
    pub fn connect(
        &self,
        host: &super::manager::RemoteHost,
        local_socket_path: &str,
    ) -> Result<(), String> {
        self.disconnect_inner();

        let args = build_ssh_args(host, local_socket_path);
        let mut cmd = Command::new("ssh");
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        // Merge optional SSH_AUTH_SOCK
        if let Some(ref sock) = host.auth_socket {
            if !sock.is_empty() {
                cmd.env("SSH_AUTH_SOCK", sock);
            }
        }

        let child = cmd
            .spawn()
            .map_err(|e| format!("ssh spawn failed: {}", e))?;

        let mut gen = self.generation.lock().unwrap();
        *gen += 1;

        *self.process.lock().unwrap() = Some(child);

        Ok(())
    }

    /// Check if the underlying ssh process is still running
    pub fn is_running(&self) -> bool {
        let mut guard = self.process.lock().unwrap();
        if let Some(ref mut child) = *guard {
            match child.try_wait() {
                Ok(None) => true, // still running
                Ok(Some(_)) | Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Terminate the tunnel process
    pub fn disconnect(&self) {
        self.disconnect_inner();
    }

    fn disconnect_inner(&self) {
        let mut guard = self.process.lock().unwrap();
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Default for SshTunnel {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        self.disconnect_inner();
    }
}

fn build_ssh_args(host: &super::manager::RemoteHost, local_socket_path: &str) -> Vec<String> {
    let mut args = vec![
        "-N".to_string(),
        "-T".to_string(),
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=15".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=2".to_string(),
        "-o".to_string(),
        "StreamLocalBindUnlink=yes".to_string(),
        "-o".to_string(),
        "StreamLocalBindMask=0000".to_string(),
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

    // Reverse forward: remote socket -> local socket
    args.push("-R".to_string());
    args.push(format!("{}:{}", host.remote_socket_path, local_socket_path));
    args.push(host.ssh_target.clone());

    args
}

#[cfg(test)]
mod tests {
    use super::build_ssh_args;
    use crate::remote::manager::RemoteHost;

    #[test]
    fn expands_tilde_identity_file_for_ssh_command_args() {
        let host = RemoteHost {
            id: "h1".to_string(),
            name: "Dev".to_string(),
            ssh_target: "dev.example.com".to_string(),
            port: Some(2222),
            identity_file: Some("~/.ssh/id_ed25519".to_string()),
            auth_socket: None,
            remote_socket_path: "/tmp/agentbro.sock".to_string(),
            auto_connect: false,
        };

        let args = build_ssh_args(&host, "/tmp/local.sock");
        let identity_index = args.iter().position(|arg| arg == "-i").expect("-i arg");

        assert!(args[identity_index + 1].ends_with("/.ssh/id_ed25519"));
        assert!(!args[identity_index + 1].starts_with('~'));
    }
}
