// Webhook forwarder — DingTalk (HmacSHA256) and Feishu (timestamp+secret) signing + delivery
//
// HTTP delivery uses `curl` (available on all macOS systems) to avoid needing reqwest.
// HMAC-SHA256 signing uses python3's built-in hmac module.
// No external crate dependencies required.

use std::time::{SystemTime, UNIX_EPOCH};
use super::templates::{self, NotificationEvent};

/// Webhook platform type
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WebhookPlatform {
    DingTalk,
    Feishu,
}

/// Webhook configuration (stored in app settings)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WebhookConfig {
    pub id: String,
    pub name: String,
    pub platform: WebhookPlatform,
    /// Webhook URL from DingTalk/Feishu bot config
    pub url: String,
    /// Optional signing secret
    pub secret: Option<String>,
    /// Agent sources that trigger this webhook (empty = all)
    pub sources: Vec<String>,
    pub enabled: bool,
}

/// Result of a webhook delivery attempt
#[derive(Debug)]
pub enum WebhookResult {
    Success,
    Skipped,
    Failed(String),
}

pub struct WebhookForwarder;

impl WebhookForwarder {
    /// Send a notification event to all matching enabled webhooks
    pub async fn send(
        configs: &[WebhookConfig],
        event: &NotificationEvent,
        source: &str,
        session_id: &str,
    ) -> Vec<(String, WebhookResult)> {
        let mut results = Vec::new();

        for cfg in configs {
            if !cfg.enabled {
                results.push((cfg.id.clone(), WebhookResult::Skipped));
                continue;
            }
            if !cfg.sources.is_empty() && !cfg.sources.iter().any(|s| s == source) {
                results.push((cfg.id.clone(), WebhookResult::Skipped));
                continue;
            }

            let result = match cfg.platform {
                WebhookPlatform::DingTalk => send_dingtalk(cfg, event, source, session_id),
                WebhookPlatform::Feishu => send_feishu(cfg, event, source, session_id),
            };
            results.push((cfg.id.clone(), result));
        }

        results
    }
}

// ─── DingTalk ─────────────────────────────────────────────────────────────────

fn send_dingtalk(
    cfg: &WebhookConfig,
    event: &NotificationEvent,
    source: &str,
    session_id: &str,
) -> WebhookResult {
    let body = templates::dingtalk_markdown(event, source, session_id);
    let url = build_dingtalk_url(&cfg.url, cfg.secret.as_deref());
    post_json_curl(&url, &body)
}

fn build_dingtalk_url(base_url: &str, secret: Option<&str>) -> String {
    let Some(secret) = secret else {
        return base_url.to_string();
    };
    if secret.is_empty() {
        return base_url.to_string();
    }

    let timestamp = now_millis();
    let sign_string = format!("{}\n{}", timestamp, secret);
    let signature = hmac_sha256_base64_via_python(secret, &sign_string);
    let encoded = url_percent_encode(&signature);

    if base_url.contains('?') {
        format!("{}&timestamp={}&sign={}", base_url, timestamp, encoded)
    } else {
        format!("{}?timestamp={}&sign={}", base_url, timestamp, encoded)
    }
}

// ─── Feishu ───────────────────────────────────────────────────────────────────

fn send_feishu(
    cfg: &WebhookConfig,
    event: &NotificationEvent,
    source: &str,
    session_id: &str,
) -> WebhookResult {
    let timestamp = now_secs();
    let mut body = templates::feishu_interactive(event, source, session_id);

    if let Some(secret) = cfg.secret.as_deref() {
        if !secret.is_empty() {
            // Feishu signs: timestamp + "\n" + secret as the HMAC key, data = ""
            let sign_input = format!("{}\n{}", timestamp, secret);
            let signature = hmac_sha256_base64_via_python(&sign_input, "");
            body["timestamp"] = serde_json::Value::String(timestamp.to_string());
            body["sign"] = serde_json::Value::String(signature);
        }
    }

    post_json_curl(&cfg.url, &body)
}

// ─── HTTP via curl ────────────────────────────────────────────────────────────

fn post_json_curl(url: &str, body: &serde_json::Value) -> WebhookResult {
    let json_str = match serde_json::to_string(body) {
        Ok(s) => s,
        Err(e) => return WebhookResult::Failed(format!("JSON serialization failed: {}", e)),
    };

    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "-o", "/dev/null",
            "-w", "%{http_code}",
            "-X", "POST",
            "-H", "Content-Type: application/json",
            "--max-time", "10",
            "--data-raw", &json_str,
            url,
        ])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let code = String::from_utf8_lossy(&o.stdout);
            let code_str = code.trim();
            // HTTP 2xx = success
            if code_str.starts_with('2') {
                WebhookResult::Success
            } else {
                WebhookResult::Failed(format!("HTTP {}", code_str))
            }
        }
        Ok(o) => WebhookResult::Failed(format!(
            "curl failed: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => WebhookResult::Failed(format!("curl exec failed: {}", e)),
    }
}

// ─── HMAC-SHA256 via python3 ──────────────────────────────────────────────────

fn hmac_sha256_base64_via_python(key: &str, data: &str) -> String {
    // Use python3's built-in hmac + base64 — always available on macOS
    let script = format!(
        r#"import hmac, hashlib, base64, sys
key = {key:?}.encode()
msg = {data:?}.encode()
sig = hmac.new(key, msg, hashlib.sha256).digest()
print(base64.b64encode(sig).decode(), end='')
"#,
        key = key,
        data = data
    );

    let output = std::process::Command::new("python3")
        .args(["-c", &script])
        .output();

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => String::new(),
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn url_percent_encode(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                vec![c]
            }
            '+' => vec!['%', '2', 'B'],
            '/' => vec!['%', '2', 'F'],
            '=' => vec!['%', '3', 'D'],
            _ => {
                let encoded = format!("%{:02X}", c as u8);
                encoded.chars().collect()
            }
        })
        .collect()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
