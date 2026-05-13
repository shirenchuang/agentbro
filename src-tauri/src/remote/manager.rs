// Remote manager — SSH tunnel lifecycle with exponential-backoff auto-reconnect

use super::installer::RemoteInstaller;
use super::ssh_tunnel::SshTunnel;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// SSH remote host configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteHost {
    pub id: String,
    pub name: String,
    /// SSH target: [user@]hostname
    #[serde(alias = "ssh_target")]
    pub ssh_target: String,
    pub port: Option<u16>,
    #[serde(alias = "identity_file")]
    pub identity_file: Option<String>,
    #[serde(alias = "auth_socket")]
    pub auth_socket: Option<String>,
    /// Remote Unix socket path for the reverse tunnel
    #[serde(alias = "remote_socket_path")]
    pub remote_socket_path: String,
    #[serde(alias = "auto_connect")]
    pub auto_connect: bool,
}

impl RemoteHost {
    pub fn display_address(&self) -> String {
        if let Some(port) = self.port {
            format!("{}:{}", self.ssh_target, port)
        } else {
            self.ssh_target.clone()
        }
    }
}

/// Connection status for a remote host
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Failed { message: String },
}

/// Exponential backoff delays (seconds) for auto-reconnect
const BACKOFF_DELAYS: &[u64] = &[5, 15, 45, 120, 300];
const MAX_RECONNECT_ATTEMPTS: u32 = 10;

fn backoff_delay(attempt: u32) -> Duration {
    let idx = (attempt as usize).min(BACKOFF_DELAYS.len() - 1);
    Duration::from_secs(BACKOFF_DELAYS[idx])
}

struct HostState {
    tunnel: SshTunnel,
    reconnect_attempts: u32,
    status: ConnectionStatus,
}

/// Manages all SSH remote host connections
pub struct RemoteManager {
    hosts: Mutex<Vec<RemoteHost>>,
    states: Mutex<HashMap<String, HostState>>,
    /// Path to the local Unix socket to reverse-forward
    local_socket_path: String,
    /// Status change callback (host_id, new_status)
    on_status_change: Arc<Mutex<Option<Box<dyn Fn(&str, ConnectionStatus) + Send + Sync>>>>,
}

impl RemoteManager {
    pub fn new(local_socket_path: String) -> Self {
        Self {
            hosts: Mutex::new(Vec::new()),
            states: Mutex::new(HashMap::new()),
            local_socket_path,
            on_status_change: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_status_callback<F>(&self, cb: F)
    where
        F: Fn(&str, ConnectionStatus) + Send + Sync + 'static,
    {
        *self.on_status_change.lock().unwrap() = Some(Box::new(cb));
    }

    pub fn add_host(&self, host: RemoteHost) {
        self.hosts.lock().unwrap().push(host);
    }

    pub fn remove_host(&self, id: &str) {
        self.disconnect(id);
        self.hosts.lock().unwrap().retain(|h| h.id != id);
        self.states.lock().unwrap().remove(id);
    }

    pub fn hosts(&self) -> Vec<RemoteHost> {
        self.hosts.lock().unwrap().clone()
    }

    pub fn status(&self, id: &str) -> ConnectionStatus {
        self.states
            .lock()
            .unwrap()
            .get(id)
            .map(|s| s.status.clone())
            .unwrap_or(ConnectionStatus::Disconnected)
    }

    /// Start auto-connect for all hosts marked `auto_connect`
    pub fn startup(&self) {
        let hosts: Vec<RemoteHost> = self
            .hosts
            .lock()
            .unwrap()
            .iter()
            .filter(|h| h.auto_connect)
            .cloned()
            .collect();

        for host in hosts {
            self.connect(&host.id);
        }
    }

    /// Disconnect all hosts
    pub fn shutdown(&self) {
        let ids: Vec<String> = self
            .hosts
            .lock()
            .unwrap()
            .iter()
            .map(|h| h.id.clone())
            .collect();
        for id in ids {
            self.disconnect(&id);
        }
    }

    /// User-initiated connect (resets backoff counter)
    pub fn connect(&self, id: &str) {
        {
            let mut states = self.states.lock().unwrap();
            if let Some(state) = states.get_mut(id) {
                state.reconnect_attempts = 0;
            }
        }
        self.connect_internal(id);
    }

    /// Disconnect and remove tunnel for a host
    pub fn disconnect(&self, id: &str) {
        if let Some(state) = self.states.lock().unwrap().get_mut(id) {
            state.tunnel.disconnect();
            state.reconnect_attempts = 0;
            state.status = ConnectionStatus::Disconnected;
        }
        self.emit_status(id, ConnectionStatus::Disconnected);
    }

    fn connect_internal(&self, id: &str) {
        let host = {
            let hosts = self.hosts.lock().unwrap();
            hosts.iter().find(|h| h.id == id).cloned()
        };

        let Some(host) = host else { return };

        if host.ssh_target.is_empty() {
            let status = ConnectionStatus::Failed {
                message: "invalid host".to_string(),
            };
            self.set_status(id, status.clone());
            self.emit_status(id, status);
            return;
        }

        self.set_status(id, ConnectionStatus::Connecting);
        self.emit_status(id, ConnectionStatus::Connecting);

        // Ensure state entry exists
        {
            let mut states = self.states.lock().unwrap();
            states.entry(id.to_string()).or_insert_with(|| HostState {
                tunnel: SshTunnel::new(),
                reconnect_attempts: 0,
                status: ConnectionStatus::Connecting,
            });
        }

        // Clean up remote socket, then start tunnel
        let local_path = self.local_socket_path.clone();
        let id_owned = id.to_string();
        let status_cb = Arc::clone(&self.on_status_change);

        tokio::spawn(async move {
            RemoteInstaller::cleanup_remote_socket(&host).await;

            // Start the tunnel synchronously (it's just a process spawn)
            let tunnel = SshTunnel::new();
            let connect_result = tunnel.connect(&host, &local_path);

            match connect_result {
                Err(e) => {
                    let status = ConnectionStatus::Failed { message: e };
                    if let Ok(cb) = status_cb.lock() {
                        if let Some(ref f) = *cb {
                            f(&id_owned, status);
                        }
                    }
                }
                Ok(()) => {
                    // Poll for 350ms to see if ssh stays running (mirrors Swift impl)
                    tokio::time::sleep(Duration::from_millis(350)).await;
                    let status = if tunnel.is_running() {
                        ConnectionStatus::Connected
                    } else {
                        ConnectionStatus::Failed {
                            message: "ssh exited immediately".to_string(),
                        }
                    };

                    if let Ok(cb) = status_cb.lock() {
                        if let Some(ref f) = *cb {
                            f(&id_owned, status.clone());
                        }
                    }

                    // If connected, install hooks in the background
                    if status == ConnectionStatus::Connected {
                        let install = RemoteInstaller::install_hooks(&host).await;
                        if !install.ok {
                            log::warn!(
                                "Remote hook install failed for {}: {}",
                                id_owned,
                                install.message
                            );
                        }
                    }
                }
            }
        });
    }

    /// Called when a tunnel drops; schedules reconnect with exponential backoff
    pub fn schedule_reconnect(&self, id: &str) {
        let attempt = {
            let mut states = self.states.lock().unwrap();
            let state = states.entry(id.to_string()).or_insert_with(|| HostState {
                tunnel: SshTunnel::new(),
                reconnect_attempts: 0,
                status: ConnectionStatus::Disconnected,
            });
            if state.reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
                return;
            }
            state.reconnect_attempts += 1;
            state.reconnect_attempts
        };

        let delay = backoff_delay(attempt);
        let id_owned = id.to_string();
        // Clone self as raw pointer workaround — use Arc<RemoteManager> in production
        // For now we just log the intent and let callers re-drive reconnect
        log::info!(
            "Scheduling reconnect for {} in {}s (attempt {})",
            id_owned,
            delay.as_secs(),
            attempt
        );

        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            log::info!("Reconnect timer fired for {}", id_owned);
            // Caller should listen to status callback and re-call connect()
        });
    }

    fn set_status(&self, id: &str, status: ConnectionStatus) {
        let mut states = self.states.lock().unwrap();
        if let Some(state) = states.get_mut(id) {
            state.status = status;
        }
    }

    fn emit_status(&self, id: &str, status: ConnectionStatus) {
        if let Ok(cb) = self.on_status_change.lock() {
            if let Some(ref f) = *cb {
                f(id, status);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteHost;

    fn host() -> RemoteHost {
        RemoteHost {
            id: "remote-1".to_string(),
            name: "Dev".to_string(),
            ssh_target: "alice@example.com".to_string(),
            port: Some(2222),
            identity_file: Some("~/.ssh/id_ed25519".to_string()),
            auth_socket: None,
            remote_socket_path: "/tmp/agentbro-remote.sock".to_string(),
            auto_connect: true,
        }
    }

    #[test]
    fn remote_host_serializes_for_frontend_with_camel_case_keys() {
        let value = serde_json::to_value(host()).expect("serialize remote host");

        assert_eq!(value["sshTarget"], "alice@example.com");
        assert_eq!(value["identityFile"], "~/.ssh/id_ed25519");
        assert_eq!(value["remoteSocketPath"], "/tmp/agentbro-remote.sock");
        assert_eq!(value["autoConnect"], true);
        assert!(value.get("ssh_target").is_none());
    }

    #[test]
    fn remote_host_accepts_legacy_snake_case_config_keys() {
        let value = serde_json::json!({
            "id": "remote-1",
            "name": "Dev",
            "ssh_target": "alice@example.com",
            "port": 2222,
            "identity_file": "~/.ssh/id_ed25519",
            "auth_socket": null,
            "remote_socket_path": "/tmp/agentbro-remote.sock",
            "auto_connect": true
        });

        let parsed: RemoteHost = serde_json::from_value(value).expect("parse legacy remote host");

        assert_eq!(parsed.ssh_target, "alice@example.com");
        assert_eq!(parsed.identity_file.as_deref(), Some("~/.ssh/id_ed25519"));
        assert_eq!(parsed.remote_socket_path, "/tmp/agentbro-remote.sock");
        assert!(parsed.auto_connect);
    }
}
