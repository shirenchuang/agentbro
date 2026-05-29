pub const HOOK_SOCKET_ENV: &str = "AGENTBRO_HOOK_SOCKET";
pub const HOOK_PORT_ENV: &str = "AGENTBRO_HOOK_PORT";

const RELEASE_SOCKET_PATH: &str = "/tmp/agentbro-hook.sock";
const RELEASE_TCP_PORT: u16 = 17894;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookEndpoint {
    pub socket_path: String,
    pub tcp_port: u16,
}

impl HookEndpoint {
    pub fn tcp_addr(&self) -> String {
        format!("127.0.0.1:{}", self.tcp_port)
    }

    /// Path to the shared-secret file gating the TCP transport. Lives next to
    /// the socket so it inherits the same per-user directory context.
    pub fn token_path(&self) -> String {
        format!("{}.token", self.socket_path)
    }
}

/// Read the shared-secret token written by the running HookServer, if present.
/// Used by the bridge and the in-app sender to authenticate over TCP.
pub fn read_token() -> Option<String> {
    let token = std::fs::read_to_string(current().token_path()).ok()?;
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

pub fn current() -> HookEndpoint {
    HookEndpoint {
        socket_path: std::env::var(HOOK_SOCKET_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| default_socket_path().to_string()),
        tcp_port: std::env::var(HOOK_PORT_ENV)
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|port| *port != 0)
            .unwrap_or_else(default_tcp_port),
    }
}

pub fn default_socket_path() -> &'static str {
    RELEASE_SOCKET_PATH
}

pub fn default_tcp_port() -> u16 {
    RELEASE_TCP_PORT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_path_sits_next_to_socket() {
        let endpoint = HookEndpoint {
            socket_path: "/tmp/agentbro-hook.sock".to_string(),
            tcp_port: 17894,
        };
        assert_eq!(endpoint.token_path(), "/tmp/agentbro-hook.sock.token");
    }
}
