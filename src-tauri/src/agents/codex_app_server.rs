use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader as TokioBufReader};
use tokio::net::TcpStream;
use tokio::process::Child;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// Holds the spawned `codex app-server --listen ws://...` child process plus
/// the connected WebSocket client. Dropping this kills the child via
/// [`Child::start_kill`] on shutdown helper paths.
pub struct CodexAppServerConnection {
    pub child: Child,
    pub socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    pub listen_port: u16,
}

/// Spawn `codex app-server --listen ws://127.0.0.1:0` and connect to it once
/// the server prints its listening URL on stderr.
///
/// The codex 0.x CLI emits `listening on: ws://127.0.0.1:<port>` on stderr
/// shortly after binding. We parse the port out of that line, then open a
/// WebSocket client to the same address. We always use port 0 so the OS picks
/// a free port — this sidesteps collisions with another AgentBro instance,
/// Ping Island, or Codex.app itself binding the same number.
pub async fn spawn_and_connect_app_server(
    binary: &Path,
    startup_timeout: Duration,
) -> Result<CodexAppServerConnection, String> {
    let mut command = crate::platform::process::background_tokio_command(binary);
    command
        .arg("app-server")
        .arg("--listen")
        .arg("ws://127.0.0.1:0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(target_os = "windows")]
    if let Some(path) = crate::agents::executable::augmented_path_env() {
        command.env("PATH", path);
    }
    let mut child = command
        .spawn()
        .map_err(|err| format!("Failed to start codex app-server: {err}"))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture codex app-server stderr".to_string())?;

    let port = tokio::time::timeout(startup_timeout, parse_listen_port(stderr))
        .await
        .map_err(|_| "Timed out waiting for codex app-server to listen".to_string())?
        .inspect_err(|_| {
            // Kill the child so it doesn't outlive a failed startup.
            // start_kill is best-effort; ignore its error.
            let _ = child.start_kill();
        })?;

    let url = format!("ws://127.0.0.1:{port}");
    let (socket, _response) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|err| {
            let _ = child.start_kill();
            format!("Failed to connect to codex app-server at {url}: {err}")
        })?;

    Ok(CodexAppServerConnection {
        child,
        socket,
        listen_port: port,
    })
}

async fn parse_listen_port(stderr: tokio::process::ChildStderr) -> Result<u16, String> {
    let mut reader = TokioBufReader::new(stderr).lines();
    while let Some(line) = reader
        .next_line()
        .await
        .map_err(|err| format!("Failed to read codex app-server stderr: {err}"))?
    {
        if let Some(port) = parse_listen_port_line(&line) {
            // Continue draining stderr in the background so the codex child
            // doesn't deadlock on a full pipe — but don't block startup on it.
            tokio::spawn(async move {
                let mut tail = reader;
                while let Ok(Some(extra)) = tail.next_line().await {
                    log::debug!("codex app-server stderr: {extra}");
                }
            });
            return Ok(port);
        }
        log::debug!("codex app-server stderr (pre-listen): {line}");
    }
    Err("codex app-server exited before printing a listen URL".to_string())
}

/// Returns the port parsed out of a `listening on: ws://host:port` line.
///
/// Codex prints `  listening on: ws://127.0.0.1:59158` after binding. The
/// leading whitespace and prefix are tolerated; anything else returns None.
pub fn parse_listen_port_line(line: &str) -> Option<u16> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("listening on:")?.trim_start();
    let after_scheme = rest
        .strip_prefix("ws://")
        .or_else(|| rest.strip_prefix("wss://"))?;
    let (_host, port_str) = after_scheme.rsplit_once(':')?;
    let port_str = port_str.split('/').next().unwrap_or(port_str);
    port_str.parse::<u16>().ok()
}

/// Compute the next exponential backoff delay, capped. Used by the background
/// monitor when the codex CLI is present but the WebSocket connection keeps
/// failing.
pub fn next_backoff(previous: Duration) -> Duration {
    const FLOOR: Duration = Duration::from_secs(5);
    const CEILING: Duration = Duration::from_secs(120);
    if previous < FLOOR {
        return FLOOR;
    }
    let doubled = previous.checked_mul(2).unwrap_or(CEILING).min(CEILING);
    doubled.max(FLOOR)
}

/// Convenience: serialize a JSON value and frame it as a WebSocket text message.
pub fn json_message(payload: &serde_json::Value) -> Result<Message, String> {
    serde_json::to_string(payload)
        .map(Message::Text)
        .map_err(|err| format!("Failed to serialize codex app-server message: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_listen_line() {
        assert_eq!(
            parse_listen_port_line("  listening on: ws://127.0.0.1:59158"),
            Some(59158)
        );
    }

    #[test]
    fn parses_listen_line_with_path() {
        assert_eq!(
            parse_listen_port_line("listening on: ws://127.0.0.1:41241/control"),
            Some(41241)
        );
    }

    #[test]
    fn rejects_other_lines() {
        assert_eq!(
            parse_listen_port_line("codex app-server (WebSockets)"),
            None
        );
        assert_eq!(
            parse_listen_port_line("  readyz: http://127.0.0.1:59158/readyz"),
            None
        );
        assert_eq!(parse_listen_port_line(""), None);
    }

    #[test]
    fn backoff_floor_and_doubling() {
        assert_eq!(next_backoff(Duration::ZERO), Duration::from_secs(5));
        assert_eq!(
            next_backoff(Duration::from_secs(5)),
            Duration::from_secs(10)
        );
        assert_eq!(
            next_backoff(Duration::from_secs(10)),
            Duration::from_secs(20)
        );
    }

    #[test]
    fn backoff_caps_at_120s() {
        assert_eq!(
            next_backoff(Duration::from_secs(60)),
            Duration::from_secs(120)
        );
        assert_eq!(
            next_backoff(Duration::from_secs(120)),
            Duration::from_secs(120)
        );
        assert_eq!(
            next_backoff(Duration::from_secs(3_600)),
            Duration::from_secs(120)
        );
    }
}
