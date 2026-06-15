use axum::body::{to_bytes, Body, Bytes};
use axum::extract::State;
use axum::http::header::{ACCEPT_ENCODING, CONTENT_LENGTH, HOST, TRANSFER_ENCODING};
use axum::http::{HeaderMap, Method, Request, Response, StatusCode, Uri};
use axum::routing::any;
use axum::Router;
use base64::Engine;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

const DEFAULT_UPSTREAM_BASE_URL: &str = "https://api.anthropic.com";
const ROUTE_PREFIX: &str = "/__agentbro_route/";
const MAX_REQUEST_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_CAPTURED_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_CAPTURED_REQUESTS: usize = 200;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkMonitorStatus {
    pub enabled: bool,
    pub proxy_url: Option<String>,
    pub upstream_base_url: String,
    pub request_count: usize,
    pub active_request_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkUsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub total_tokens: u64,
    pub cache_hit_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkRequestSummary {
    pub id: String,
    pub timestamp_ms: u64,
    pub provider: String,
    pub method: String,
    pub url: String,
    pub upstream_url: String,
    pub session_id: Option<String>,
    pub project: Option<String>,
    pub model: Option<String>,
    pub status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub request_bytes: usize,
    pub response_bytes: usize,
    pub is_stream: bool,
    pub main_agent: bool,
    pub request_type: String,
    pub request_sub_type: Option<String>,
    pub message_count: usize,
    pub tool_count: usize,
    pub system_preview: Option<String>,
    pub usage: Option<Value>,
    pub usage_summary: Option<NetworkUsageSummary>,
    pub error: Option<String>,
    pub in_progress: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkRequestDetail {
    pub summary: NetworkRequestSummary,
    pub request_headers: Value,
    pub request_body: Value,
    pub response_headers: Value,
    pub response_body: Option<String>,
    pub response_body_truncated: bool,
    pub stream_event_count: u64,
}

#[derive(Debug, Clone)]
struct NetworkRequestEntry {
    summary: NetworkRequestSummary,
    request_headers: Value,
    request_body: Value,
    response_headers: Value,
    response_body: Option<String>,
    response_body_truncated: bool,
    stream_event_count: u64,
}

#[derive(Default)]
struct NetworkMonitorInner {
    enabled: bool,
    port: Option<u16>,
    upstream_base_url: String,
    shutdown_tx: Option<oneshot::Sender<()>>,
    requests: VecDeque<NetworkRequestEntry>,
    active_request_count: usize,
}

#[derive(Default)]
struct ResponseCapture {
    chunks: Vec<u8>,
    total_bytes: usize,
    truncated: bool,
    event_count: u64,
}

pub struct NetworkMonitor {
    inner: Mutex<NetworkMonitorInner>,
    client: reqwest::Client,
}

impl NetworkMonitor {
    pub fn new() -> Self {
        write_monitor_state(false, None);
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build network monitor HTTP client");
        Self {
            inner: Mutex::new(NetworkMonitorInner {
                upstream_base_url: DEFAULT_UPSTREAM_BASE_URL.to_string(),
                ..NetworkMonitorInner::default()
            }),
            client,
        }
    }

    pub fn status(&self) -> NetworkMonitorStatus {
        let inner = self.inner.lock().expect("network monitor lock poisoned");
        Self::status_from_inner(&inner)
    }

    pub async fn set_enabled(
        self: &Arc<Self>,
        enabled: bool,
        upstream_base_url: Option<String>,
    ) -> Result<NetworkMonitorStatus, String> {
        if enabled {
            self.start(upstream_base_url).await
        } else {
            self.stop()
        }
    }

    pub fn requests(&self) -> Vec<NetworkRequestSummary> {
        let inner = self.inner.lock().expect("network monitor lock poisoned");
        inner
            .requests
            .iter()
            .rev()
            .map(|entry| entry.summary.clone())
            .collect()
    }

    pub fn request_detail(&self, request_id: &str) -> Option<NetworkRequestDetail> {
        let inner = self.inner.lock().expect("network monitor lock poisoned");
        inner
            .requests
            .iter()
            .find(|entry| entry.summary.id == request_id)
            .map(|entry| NetworkRequestDetail {
                summary: entry.summary.clone(),
                request_headers: entry.request_headers.clone(),
                request_body: entry.request_body.clone(),
                response_headers: entry.response_headers.clone(),
                response_body: entry.response_body.clone(),
                response_body_truncated: entry.response_body_truncated,
                stream_event_count: entry.stream_event_count,
            })
    }

    pub fn proxy_url(&self) -> Option<String> {
        let inner = self.inner.lock().expect("network monitor lock poisoned");
        inner.port.map(|port| format!("http://127.0.0.1:{port}"))
    }

    async fn start(
        self: &Arc<Self>,
        upstream_base_url: Option<String>,
    ) -> Result<NetworkMonitorStatus, String> {
        let upstream = normalize_upstream_base_url(upstream_base_url)?;
        if let Some(status) = self.update_upstream_if_running(&upstream) {
            return Ok(status);
        }

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Failed to bind network monitor proxy: {e}"))?;
        let addr = listener
            .local_addr()
            .map_err(|e| format!("Failed to read network monitor proxy address: {e}"))?;
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        {
            let mut inner = self.inner.lock().expect("network monitor lock poisoned");
            inner.enabled = true;
            inner.port = Some(addr.port());
            inner.upstream_base_url = upstream;
            inner.shutdown_tx = Some(shutdown_tx);
            inner.active_request_count = 0;
            inner.requests.clear();
        }
        write_monitor_state(true, Some(addr.port()));

        let monitor = self.clone();
        let app = Router::new()
            .fallback(any(proxy_handler))
            .with_state(monitor.clone());

        tokio::spawn(async move {
            let server = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });
            if let Err(err) = server.await {
                log::warn!("Network monitor proxy stopped with error: {err}");
            }
            monitor.mark_stopped(addr);
        });

        Ok(self.status())
    }

    fn update_upstream_if_running(&self, upstream: &str) -> Option<NetworkMonitorStatus> {
        let mut inner = self.inner.lock().expect("network monitor lock poisoned");
        if !inner.enabled {
            return None;
        }
        inner.upstream_base_url = upstream.to_string();
        Some(Self::status_from_inner(&inner))
    }

    fn stop(&self) -> Result<NetworkMonitorStatus, String> {
        let shutdown = {
            let mut inner = self.inner.lock().expect("network monitor lock poisoned");
            inner.enabled = false;
            inner.port = None;
            inner.active_request_count = 0;
            inner.shutdown_tx.take()
        };
        if let Some(tx) = shutdown {
            let _ = tx.send(());
        }
        write_monitor_state(false, None);
        Ok(self.status())
    }

    fn mark_stopped(&self, addr: SocketAddr) {
        let mut inner = self.inner.lock().expect("network monitor lock poisoned");
        if inner.port == Some(addr.port()) {
            inner.enabled = false;
            inner.port = None;
            inner.shutdown_tx = None;
            inner.active_request_count = 0;
            write_monitor_state(false, None);
        }
    }

    async fn proxy_request(
        self: Arc<Self>,
        method: Method,
        uri: Uri,
        headers: HeaderMap,
        body: Body,
    ) -> Response<Body> {
        if uri.path() == "/__agentbro_health" {
            return text_response(StatusCode::NO_CONTENT, "");
        }

        let status = self.status();
        if !status.enabled {
            return text_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "AgentBro network monitor is off",
            );
        }

        let route = match resolve_route(&status.upstream_base_url, &uri) {
            Ok(route) => route,
            Err(err) => return text_response(StatusCode::BAD_REQUEST, &err),
        };

        let upstream_url = match join_upstream_url(&route.upstream_base_url, &route.path_and_query)
        {
            Ok(url) => url,
            Err(err) => return text_response(StatusCode::BAD_REQUEST, &err),
        };

        let started_at = Instant::now();
        let request_bytes = match to_bytes(body, MAX_REQUEST_BODY_BYTES).await {
            Ok(bytes) => bytes,
            Err(err) => {
                return text_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    &format!("Failed to read request body: {err}"),
                )
            }
        };

        let request_body = parse_json_or_text(&request_bytes);
        let request_id = self.record_started(
            method.clone(),
            uri.clone(),
            upstream_url.clone(),
            headers.clone(),
            request_body.clone(),
            request_bytes.len(),
        );

        let mut forward_headers = headers;
        forward_headers.remove(HOST);
        forward_headers.remove(CONTENT_LENGTH);
        forward_headers.remove(ACCEPT_ENCODING);

        let response = self
            .client
            .request(method, &upstream_url)
            .headers(forward_headers)
            .body(request_bytes)
            .send()
            .await;

        let upstream_response = match response {
            Ok(response) => response,
            Err(err) => {
                let elapsed = started_at.elapsed().as_millis() as u64;
                self.record_error(&request_id, elapsed, err.to_string());
                return text_response(
                    StatusCode::BAD_GATEWAY,
                    "AgentBro proxy upstream request failed",
                );
            }
        };

        let response_status = upstream_response.status();
        let response_headers = upstream_response.headers().clone();
        let safe_response_headers = redact_headers(&response_headers);
        let capture = Arc::new(Mutex::new(ResponseCapture::default()));
        let capture_for_stream = capture.clone();
        let monitor_for_stream = self.clone();
        let request_id_for_stream = request_id.clone();
        let mut upstream_stream = upstream_response.bytes_stream();

        let stream = async_stream::stream! {
            while let Some(item) = upstream_stream.next().await {
                match item {
                    Ok(bytes) => {
                        capture_response_chunk(&capture_for_stream, &bytes);
                        yield Ok::<Bytes, std::io::Error>(bytes);
                    }
                    Err(err) => {
                        let elapsed = started_at.elapsed().as_millis() as u64;
                        let message = err.to_string();
                        monitor_for_stream.record_error(&request_id_for_stream, elapsed, message.clone());
                        yield Err(std::io::Error::other(message));
                        return;
                    }
                }
            }

            let elapsed = started_at.elapsed().as_millis() as u64;
            let capture = capture_for_stream.lock().expect("response capture lock poisoned");
            monitor_for_stream.record_completed(
                &request_id_for_stream,
                response_status.as_u16(),
                elapsed,
                safe_response_headers,
                &capture,
            );
        };

        let mut builder = Response::builder().status(response_status);
        for (name, value) in response_headers.iter() {
            if name == TRANSFER_ENCODING || name == CONTENT_LENGTH {
                continue;
            }
            builder = builder.header(name.clone(), value.clone());
        }
        builder.body(Body::from_stream(stream)).unwrap_or_else(|_| {
            text_response(StatusCode::BAD_GATEWAY, "Failed to build proxy response")
        })
    }

    fn record_started(
        &self,
        method: Method,
        uri: Uri,
        upstream_url: String,
        headers: HeaderMap,
        request_body: Value,
        request_bytes: usize,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let metadata = request_metadata(&request_body, &uri);
        let summary = NetworkRequestSummary {
            id: id.clone(),
            timestamp_ms: now,
            provider: "anthropic".to_string(),
            method: method.to_string(),
            url: uri.to_string(),
            upstream_url,
            session_id: headers
                .get("x-agentbro-session-id")
                .and_then(|value| value.to_str().ok())
                .map(ToString::to_string),
            project: headers
                .get("x-agentbro-project")
                .and_then(|value| value.to_str().ok())
                .map(ToString::to_string),
            model: metadata.model,
            status: None,
            duration_ms: None,
            request_bytes,
            response_bytes: 0,
            is_stream: metadata.is_stream,
            main_agent: metadata.main_agent,
            request_type: metadata.request_type,
            request_sub_type: metadata.request_sub_type,
            message_count: metadata.message_count,
            tool_count: metadata.tool_count,
            system_preview: metadata.system_preview,
            usage: None,
            usage_summary: None,
            error: None,
            in_progress: true,
        };

        let entry = NetworkRequestEntry {
            summary,
            request_headers: redact_headers(&headers),
            request_body,
            response_headers: json!({}),
            response_body: None,
            response_body_truncated: false,
            stream_event_count: 0,
        };

        let mut inner = self.inner.lock().expect("network monitor lock poisoned");
        inner.active_request_count += 1;
        inner.requests.push_back(entry);
        while inner.requests.len() > MAX_CAPTURED_REQUESTS {
            inner.requests.pop_front();
        }
        id
    }

    fn record_completed(
        &self,
        request_id: &str,
        status: u16,
        duration_ms: u64,
        response_headers: Value,
        capture: &ResponseCapture,
    ) {
        let body_text = String::from_utf8_lossy(&capture.chunks).to_string();
        let usage = extract_usage(&body_text);
        let usage_summary = usage.as_ref().map(summarize_usage);
        let mut inner = self.inner.lock().expect("network monitor lock poisoned");
        if inner.active_request_count > 0 {
            inner.active_request_count -= 1;
        }
        if let Some(entry) = inner
            .requests
            .iter_mut()
            .find(|entry| entry.summary.id == request_id)
        {
            entry.summary.status = Some(status);
            entry.summary.duration_ms = Some(duration_ms);
            entry.summary.response_bytes = capture.total_bytes;
            entry.summary.usage = usage;
            entry.summary.usage_summary = usage_summary;
            entry.summary.in_progress = false;
            entry.response_headers = response_headers;
            entry.response_body = Some(body_text);
            entry.response_body_truncated = capture.truncated;
            entry.stream_event_count = capture.event_count;
        }
    }

    fn record_error(&self, request_id: &str, duration_ms: u64, error: String) {
        let mut inner = self.inner.lock().expect("network monitor lock poisoned");
        if inner.active_request_count > 0 {
            inner.active_request_count -= 1;
        }
        if let Some(entry) = inner
            .requests
            .iter_mut()
            .find(|entry| entry.summary.id == request_id)
        {
            entry.summary.duration_ms = Some(duration_ms);
            entry.summary.error = Some(error);
            entry.summary.in_progress = false;
        }
    }

    fn status_from_inner(inner: &NetworkMonitorInner) -> NetworkMonitorStatus {
        NetworkMonitorStatus {
            enabled: inner.enabled,
            proxy_url: inner.port.map(|port| format!("http://127.0.0.1:{port}")),
            upstream_base_url: if inner.upstream_base_url.is_empty() {
                DEFAULT_UPSTREAM_BASE_URL.to_string()
            } else {
                inner.upstream_base_url.clone()
            },
            request_count: inner.requests.len(),
            active_request_count: inner.active_request_count,
        }
    }
}

impl Default for NetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}

async fn proxy_handler(
    State(monitor): State<Arc<NetworkMonitor>>,
    request: Request<Body>,
) -> Response<Body> {
    let (parts, body) = request.into_parts();
    monitor
        .proxy_request(parts.method, parts.uri, parts.headers, body)
        .await
}

struct RouteTarget {
    upstream_base_url: String,
    path_and_query: String,
}

fn normalize_upstream_base_url(value: Option<String>) -> Result<String, String> {
    let value = value
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_UPSTREAM_BASE_URL.to_string());
    let url = reqwest::Url::parse(&value).map_err(|e| format!("Invalid upstream URL: {e}"))?;
    match url.scheme() {
        "http" | "https" => Ok(value),
        scheme => Err(format!("Unsupported upstream URL scheme: {scheme}")),
    }
}

fn resolve_route(default_upstream_base_url: &str, uri: &Uri) -> Result<RouteTarget, String> {
    let path_and_query = uri
        .path_and_query()
        .map(|part| part.as_str())
        .unwrap_or("/");
    if !path_and_query.starts_with(ROUTE_PREFIX) {
        return Ok(RouteTarget {
            upstream_base_url: default_upstream_base_url.to_string(),
            path_and_query: path_and_query.to_string(),
        });
    }

    let rest = &path_and_query[ROUTE_PREFIX.len()..];
    let Some((encoded_upstream, remaining)) = rest.split_once('/') else {
        return Err("Invalid AgentBro route URL".to_string());
    };
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded_upstream)
        .map_err(|_| "Invalid AgentBro route upstream encoding".to_string())?;
    let upstream_base_url = String::from_utf8(decoded)
        .map_err(|_| "Invalid AgentBro route upstream text".to_string())?;
    let upstream_base_url = normalize_upstream_base_url(Some(upstream_base_url))?;
    Ok(RouteTarget {
        upstream_base_url,
        path_and_query: format!("/{remaining}"),
    })
}

fn join_upstream_url(upstream_base_url: &str, path_and_query: &str) -> Result<String, String> {
    let url = format!(
        "{}{}",
        upstream_base_url.trim_end_matches('/'),
        if path_and_query.starts_with('/') {
            path_and_query.to_string()
        } else {
            format!("/{path_and_query}")
        }
    );
    reqwest::Url::parse(&url)
        .map(|url| url.to_string())
        .map_err(|e| format!("Invalid proxied URL: {e}"))
}

fn monitor_state_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        let new_path = home.join(".agentbro").join("network").join("monitor-state.json");
        let old_path = home.join(".agentbro").join("network-monitor.json");
        crate::data_dir::migrate_file(&old_path, &new_path);
        new_path
    })
}

fn write_monitor_state(enabled: bool, port: Option<u16>) {
    let Some(path) = monitor_state_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let proxy_url = port.map(|port| format!("http://127.0.0.1:{port}"));
    let payload = json!({
        "enabled": enabled,
        "proxyUrl": proxy_url,
        "updatedAt": now_ms(),
    });
    if let Ok(text) = serde_json::to_string_pretty(&payload) {
        let _ = fs::write(path, text);
    }
}

fn parse_json_or_text(bytes: &Bytes) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|_| {
        let text = String::from_utf8_lossy(bytes);
        json!({ "_raw": text })
    })
}

fn redact_headers(headers: &HeaderMap) -> Value {
    let mut object = Map::new();
    for (name, value) in headers.iter() {
        let key = name.as_str().to_ascii_lowercase();
        let value = value
            .to_str()
            .map(ToString::to_string)
            .unwrap_or_else(|_| "<binary>".to_string());
        object.insert(
            key.clone(),
            Value::String(redact_header_value(&key, &value)),
        );
    }
    Value::Object(object)
}

fn redact_header_value(key: &str, value: &str) -> String {
    let sensitive = key == "authorization"
        || key == "x-api-key"
        || key == "cookie"
        || key == "set-cookie"
        || key.contains("token")
        || key.contains("secret")
        || key.contains("key");
    if !sensitive {
        return value.to_string();
    }
    if value.len() <= 12 {
        "****".to_string()
    } else {
        format!("{}****{}", &value[..6], &value[value.len() - 4..])
    }
}

struct RequestMetadata {
    model: Option<String>,
    is_stream: bool,
    main_agent: bool,
    request_type: String,
    request_sub_type: Option<String>,
    message_count: usize,
    tool_count: usize,
    system_preview: Option<String>,
}

fn request_metadata(body: &Value, uri: &Uri) -> RequestMetadata {
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let is_stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let message_count = body
        .get("messages")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let tool_count = body
        .get("tools")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let system_text = system_text(body);
    let system_preview = system_text.as_ref().map(|text| truncate_chars(text, 280));
    let (request_type, request_sub_type, main_agent) =
        classify_request(body, uri, system_text.as_deref(), message_count, tool_count);

    RequestMetadata {
        model,
        is_stream,
        main_agent,
        request_type,
        request_sub_type,
        message_count,
        tool_count,
        system_preview,
    }
}

fn classify_request(
    body: &Value,
    uri: &Uri,
    system_text: Option<&str>,
    message_count: usize,
    tool_count: usize,
) -> (String, Option<String>, bool) {
    let path = uri.path();
    if path.ends_with("/v1/messages/count_tokens") || is_count_request(body) {
        return ("Count".to_string(), None, false);
    }

    let sys = system_text.unwrap_or("");
    if let Some(sub_type) = subagent_sub_type(sys, body) {
        return ("SubAgent".to_string(), Some(sub_type), false);
    }

    let main_agent = is_main_agent_request(body, sys, message_count, tool_count);
    if main_agent {
        if let Some(sub_type) = synthetic_sub_type(body) {
            return ("Synthetic".to_string(), Some(sub_type), true);
        }
        return ("MainAgent".to_string(), None, true);
    }

    if is_preflight_request(body, sys, tool_count, message_count) {
        if latest_user_text(body)
            .map(|text| {
                text.trim_start()
                    .starts_with("Implement the following plan:")
            })
            .unwrap_or(false)
        {
            return ("Plan".to_string(), Some("Prompt".to_string()), false);
        }
        return ("Preflight".to_string(), None, false);
    }

    ("Unknown".to_string(), None, false)
}

fn is_main_agent_request(
    body: &Value,
    sys: &str,
    _message_count: usize,
    tool_count: usize,
) -> bool {
    if body.get("system").is_none() || body.get("tools").and_then(Value::as_array).is_none() {
        return false;
    }
    if !sys.contains("Claude Code") {
        return false;
    }
    if is_subagent_system(sys) {
        return false;
    }

    let tools = body.get("tools").and_then(Value::as_array);
    if has_tool_named(tools, "ToolSearch") {
        let first_text = body
            .get("messages")
            .and_then(Value::as_array)
            .and_then(|messages| messages.first())
            .and_then(message_text)
            .unwrap_or_default();
        if first_text.contains("<available-deferred-tools>") {
            return true;
        }
    }

    tool_count > 5
        && has_tool_named(tools, "Edit")
        && has_tool_named(tools, "Bash")
        && (has_tool_named(tools, "Task") || has_tool_named(tools, "Agent"))
}

fn subagent_sub_type(sys: &str, body: &Value) -> Option<String> {
    if sys.contains("Extract any file paths")
        || sys.contains("process Bash commands")
        || contains_ci(sys, "command execution specialist")
    {
        return Some("Bash".to_string());
    }
    if contains_ci(sys, "file search specialist") {
        return Some("Search".to_string());
    }
    if contains_ci(sys, "planning specialist") {
        return Some("Plan".to_string());
    }
    if contains_ci(sys, "general-purpose agent") {
        return Some("General".to_string());
    }

    latest_user_text(body).and_then(|text| {
        if text.lines().any(|line| line.starts_with("Command:")) {
            Some("Bash".to_string())
        } else {
            None
        }
    })
}

fn is_subagent_system(sys: &str) -> bool {
    contains_ci(sys, "command execution specialist")
        || contains_ci(sys, "file search specialist")
        || contains_ci(sys, "planning specialist")
        || contains_ci(sys, "general-purpose agent")
}

fn is_count_request(body: &Value) -> bool {
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return false;
    };
    if messages.len() != 1 {
        return false;
    }
    messages
        .first()
        .and_then(message_text)
        .map(|text| text.trim() == "count")
        .unwrap_or(false)
}

fn is_preflight_request(body: &Value, sys: &str, tool_count: usize, message_count: usize) -> bool {
    if tool_count > 0 || message_count != 1 || !sys.contains("Claude Code") {
        return false;
    }
    let Some(text) = latest_user_text(body) else {
        return false;
    };
    let trimmed = text.trim();
    !trimmed.is_empty()
        && trimmed != "count"
        && !trimmed.starts_with("Command:")
        && !trimmed.starts_with("<policy_spec>")
        && !trimmed.starts_with("<task-notification>")
        && !sys.contains("process Bash commands")
        && !sys.contains("Extract any file paths")
}

fn synthetic_sub_type(body: &Value) -> Option<String> {
    let text = latest_user_text(body)?;
    let trimmed = text.trim_start();
    let patterns = [
        ("Recap", "Your task is to create a detailed summary"),
        ("Title", "Generate a short title"),
        (
            "Compact",
            "Your task is to create a summary of the conversation",
        ),
        (
            "Topic",
            "Analyze if this message indicates a new conversation topic",
        ),
        ("Summary", "Provide a concise summary"),
    ];
    patterns
        .iter()
        .find(|(_, prefix)| trimmed.starts_with(prefix))
        .map(|(name, _)| (*name).to_string())
}

fn latest_user_text(body: &Value) -> Option<String> {
    body.get("messages")
        .and_then(Value::as_array)?
        .iter()
        .rev()
        .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        .and_then(message_text)
}

fn message_text(message: &Value) -> Option<String> {
    let content = message.get("content")?;
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    if let Some(blocks) = content.as_array() {
        let parts: Vec<String> = blocks
            .iter()
            .filter_map(|block| {
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
            })
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    } else {
        None
    }
}

fn has_tool_named(tools: Option<&Vec<Value>>, name: &str) -> bool {
    tools
        .map(|tools| {
            tools
                .iter()
                .any(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
        })
        .unwrap_or(false)
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn system_text(body: &Value) -> Option<String> {
    let system = body.get("system")?;
    if let Some(text) = system.as_str() {
        return Some(text.to_string());
    }
    if let Some(items) = system.as_array() {
        let parts: Vec<String> = items
            .iter()
            .filter_map(|item| {
                item.as_str().map(ToString::to_string).or_else(|| {
                    item.get("text")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
            })
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n\n"))
        }
    } else {
        None
    }
}

fn capture_response_chunk(capture: &Arc<Mutex<ResponseCapture>>, bytes: &Bytes) {
    let mut capture = capture.lock().expect("response capture lock poisoned");
    capture.total_bytes += bytes.len();
    capture.event_count += bytes
        .windows(7)
        .filter(|window| *window == b"event: ")
        .count() as u64;
    if capture.chunks.len() < MAX_CAPTURED_RESPONSE_BYTES {
        let remaining = MAX_CAPTURED_RESPONSE_BYTES - capture.chunks.len();
        let take = remaining.min(bytes.len());
        capture.chunks.extend_from_slice(&bytes[..take]);
        if take < bytes.len() {
            capture.truncated = true;
        }
    } else {
        capture.truncated = true;
    }
}

fn extract_usage(body_text: &str) -> Option<Value> {
    if let Ok(json) = serde_json::from_str::<Value>(body_text) {
        return json.get("usage").cloned();
    }

    let mut latest_usage = None;
    for line in body_text.lines() {
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(json) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(usage) = json.get("usage") {
            latest_usage = Some(usage.clone());
        }
        if let Some(usage) = json.get("message").and_then(|message| message.get("usage")) {
            latest_usage = Some(usage.clone());
        }
    }
    latest_usage
}

fn summarize_usage(usage: &Value) -> NetworkUsageSummary {
    let input_tokens = usage_u64(usage, "input_tokens");
    let output_tokens = usage_u64(usage, "output_tokens");
    let cache_creation_input_tokens = usage_u64(usage, "cache_creation_input_tokens");
    let cache_read_input_tokens = usage_u64(usage, "cache_read_input_tokens");
    let total_tokens =
        input_tokens + output_tokens + cache_creation_input_tokens + cache_read_input_tokens;
    let cache_total = cache_creation_input_tokens + cache_read_input_tokens;
    let cache_hit_rate = if cache_total > 0 {
        Some((cache_read_input_tokens as f64 / cache_total as f64) * 100.0)
    } else {
        None
    };

    NetworkUsageSummary {
        input_tokens,
        output_tokens,
        cache_creation_input_tokens,
        cache_read_input_tokens,
        total_tokens,
        cache_hit_rate,
    }
}

fn usage_u64(usage: &Value, key: &str) -> u64 {
    usage.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut result = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= limit {
            result.push('…');
            break;
        }
        result.push(ch);
    }
    result
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn text_response(status: StatusCode, text: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Body::from(text.to_string()))
        .unwrap_or_else(|_| Response::new(Body::from(text.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_encoded_route_to_original_upstream_and_path() {
        let encoded =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("https://www.coreapi.cc/");
        let uri: Uri = format!("{ROUTE_PREFIX}{encoded}/v1/messages?beta=true")
            .parse()
            .unwrap();

        let route = resolve_route(DEFAULT_UPSTREAM_BASE_URL, &uri).unwrap();

        assert_eq!(route.upstream_base_url, "https://www.coreapi.cc");
        assert_eq!(route.path_and_query, "/v1/messages?beta=true");
        assert_eq!(
            join_upstream_url(&route.upstream_base_url, &route.path_and_query).unwrap(),
            "https://www.coreapi.cc/v1/messages?beta=true"
        );
    }

    #[test]
    fn classifies_count_token_request_from_path() {
        let body = json!({
            "model": "claude-sonnet-4",
            "messages": [{"role": "user", "content": "hello"}]
        });
        let uri: Uri = "/v1/messages/count_tokens".parse().unwrap();

        let metadata = request_metadata(&body, &uri);

        assert_eq!(metadata.request_type, "Count");
        assert!(!metadata.main_agent);
    }

    #[test]
    fn extracts_latest_stream_usage_summary() {
        let body = "\
event: message_start\n\
data: {\"message\":{\"usage\":{\"input_tokens\":10,\"output_tokens\":0,\"cache_creation_input_tokens\":2,\"cache_read_input_tokens\":3}}}\n\n\
event: message_delta\n\
data: {\"usage\":{\"input_tokens\":10,\"output_tokens\":7,\"cache_creation_input_tokens\":2,\"cache_read_input_tokens\":6}}\n\n";

        let usage = extract_usage(body).unwrap();
        let summary = summarize_usage(&usage);

        assert_eq!(summary.input_tokens, 10);
        assert_eq!(summary.output_tokens, 7);
        assert_eq!(summary.cache_creation_input_tokens, 2);
        assert_eq!(summary.cache_read_input_tokens, 6);
        assert_eq!(summary.total_tokens, 25);
        assert_eq!(summary.cache_hit_rate, Some(75.0));
    }
}
