#[cfg(any(unix, test))]
use base64::Engine as _;
#[cfg(unix)]
use std::io::{BufRead as _, Write as _};

#[cfg(unix)]
pub fn focus_block(block_id: &str, tab_id: &str, jwt: &str) -> Result<(), String> {
    let mut client = WaveRpcClient::connect(jwt)?;
    client.authenticate(jwt)?;
    client.call(
        "setblockfocus",
        serde_json::Value::String(block_id.to_string()),
        Some(format!("tab:{tab_id}")),
    )?;
    Ok(())
}

#[cfg(not(unix))]
pub fn focus_block(_block_id: &str, _tab_id: &str, _jwt: &str) -> Result<(), String> {
    Err("Wave terminal integration is only supported on Unix".to_string())
}

#[cfg(unix)]
pub fn send_input(block_id: &str, jwt: &str, input: &str) -> Result<(), String> {
    let mut client = WaveRpcClient::connect(jwt)?;
    client.authenticate(jwt)?;
    client.call(
        "controllerinput",
        serde_json::json!({
            "blockid": block_id,
            "inputdata64": base64::engine::general_purpose::STANDARD.encode(input.as_bytes()),
        }),
        None,
    )?;
    Ok(())
}

#[cfg(not(unix))]
pub fn send_input(_block_id: &str, _jwt: &str, _input: &str) -> Result<(), String> {
    Err("Wave terminal integration is only supported on Unix".to_string())
}

#[cfg(unix)]
struct WaveRpcClient {
    writer: std::os::unix::net::UnixStream,
    reader: std::io::BufReader<std::os::unix::net::UnixStream>,
}

#[cfg(unix)]
impl WaveRpcClient {
    fn connect(jwt: &str) -> Result<Self, String> {
        let sock = socket_from_jwt(jwt)?;
        let stream = std::os::unix::net::UnixStream::connect(sock)
            .map_err(|err| format!("connect Wave socket: {err}"))?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(2500)))
            .map_err(|err| err.to_string())?;
        stream
            .set_write_timeout(Some(std::time::Duration::from_millis(2500)))
            .map_err(|err| err.to_string())?;
        let writer = stream.try_clone().map_err(|err| err.to_string())?;
        Ok(Self {
            writer,
            reader: std::io::BufReader::new(stream),
        })
    }

    fn authenticate(&mut self, jwt: &str) -> Result<(), String> {
        self.call(
            "authenticate",
            serde_json::Value::String(jwt.to_string()),
            Some("$control".to_string()),
        )
    }

    fn call(
        &mut self,
        command: &str,
        data: serde_json::Value,
        route: Option<String>,
    ) -> Result<(), String> {
        let req_id = uuid::Uuid::new_v4().to_string();
        let mut request = serde_json::json!({
            "command": command,
            "reqid": req_id,
            "data": data,
            "timeout": 2000,
        });
        if let Some(route) = route {
            request["route"] = serde_json::Value::String(route);
        }

        serde_json::to_writer(&mut self.writer, &request).map_err(|err| err.to_string())?;
        self.writer
            .write_all(b"\n")
            .map_err(|err| err.to_string())?;
        self.writer.flush().map_err(|err| err.to_string())?;
        self.read_response(&req_id)
    }

    fn read_response(&mut self, req_id: &str) -> Result<(), String> {
        let mut line = String::new();
        loop {
            line.clear();
            if self
                .reader
                .read_line(&mut line)
                .map_err(|err| err.to_string())?
                == 0
            {
                return Err("Wave RPC closed before response".to_string());
            }
            let value: serde_json::Value =
                serde_json::from_str(line.trim()).map_err(|err| err.to_string())?;
            if value.get("resid").and_then(|resid| resid.as_str()) != Some(req_id) {
                continue;
            }
            if let Some(err) = value.get("error").and_then(|err| err.as_str()) {
                if !err.is_empty() {
                    return Err(format!("Wave RPC error: {err}"));
                }
            }
            return Ok(());
        }
    }
}

#[cfg(any(unix, test))]
fn socket_from_jwt(jwt: &str) -> Result<String, String> {
    let payload = jwt
        .split('.')
        .nth(1)
        .ok_or_else(|| "Wave JWT payload missing".to_string())?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|err| err.to_string())?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|err| err.to_string())?;
    value
        .get("sock")
        .and_then(|sock| sock.as_str())
        .filter(|sock| !sock.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| "Wave JWT sock claim missing".to_string())
}

#[cfg(test)]
mod tests {
    use super::socket_from_jwt;
    use base64::Engine as _;

    #[test]
    fn wave_socket_is_decoded_from_jwt_payload() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sock":"/tmp/wave.sock","blockid":"block-1"}"#);
        let jwt = format!("header.{payload}.signature");

        assert_eq!(socket_from_jwt(&jwt).unwrap(), "/tmp/wave.sock");
    }
}
