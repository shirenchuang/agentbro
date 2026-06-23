// Webhook forwarder — DingTalk (HmacSHA256) and Feishu (timestamp+secret) signing + delivery
//
// HTTP delivery uses `curl` to avoid needing reqwest.

use super::templates::{self, NotificationEvent};
use base64::{engine::general_purpose, Engine as _};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Webhook platform type
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WebhookPlatform {
    DingTalk,
    Feishu,
}

/// Webhook configuration (stored in app settings)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// Event keys that trigger this webhook (empty = all)
    #[serde(default)]
    pub events: Vec<String>,
    pub enabled: bool,
    /// Delay interactive notifications until the session remains unresolved.
    #[serde(default)]
    pub delay_enabled: bool,
    /// Delay duration in minutes for interactive notifications.
    #[serde(default = "default_delay_minutes")]
    pub delay_minutes: u32,
}

fn default_delay_minutes() -> u32 {
    1
}

impl WebhookConfig {
    pub fn matches(&self, event_key: &str, source: &str) -> bool {
        self.enabled
            && (self.sources.is_empty() || self.sources.iter().any(|s| s == source))
            && (self.events.is_empty() || self.events.iter().any(|e| e == event_key))
    }
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
        language: &str,
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
            let event_key = templates::event_key(event);
            if event_key != "custom"
                && !cfg.events.is_empty()
                && !cfg.events.iter().any(|e| e == event_key)
            {
                results.push((cfg.id.clone(), WebhookResult::Skipped));
                continue;
            }

            let result = match cfg.platform {
                WebhookPlatform::DingTalk => {
                    send_dingtalk(cfg, event, source, session_id, language)
                }
                WebhookPlatform::Feishu => send_feishu(cfg, event, source, session_id, language),
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
    language: &str,
) -> WebhookResult {
    let body = templates::dingtalk_markdown(event, source, session_id, language);
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
    let signature = hmac_sha256_base64(secret, &sign_string);
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
    language: &str,
) -> WebhookResult {
    let timestamp = now_secs();
    let mut body = templates::feishu_interactive(event, source, session_id, language);

    if let Some(secret) = cfg.secret.as_deref() {
        if !secret.is_empty() {
            // Feishu signs: timestamp + "\n" + secret as the HMAC key, data = ""
            let sign_input = format!("{}\n{}", timestamp, secret);
            let signature = hmac_sha256_base64(&sign_input, "");
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

    let output = std::process::Command::new(crate::agents::executable::command_path("curl"))
        .args([
            "-s",
            "-o",
            curl_discard_target(),
            "-w",
            "%{http_code}",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "--max-time",
            "10",
            "--data-raw",
            &json_str,
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

// ─── HMAC-SHA256 ──────────────────────────────────────────────────────────────

fn curl_discard_target() -> &'static str {
    if cfg!(target_os = "windows") {
        "NUL"
    } else {
        "/dev/null"
    }
}

fn hmac_sha256_base64(key: &str, data: &str) -> String {
    // HMAC-SHA256 using Rust primitives keeps webhook signing platform neutral.
    const BLOCK_SIZE: usize = 64;

    let mut key_bytes = key.as_bytes().to_vec();
    if key_bytes.len() > BLOCK_SIZE {
        key_bytes = Sha256::digest(&key_bytes).to_vec();
    }
    key_bytes.resize(BLOCK_SIZE, 0);

    let mut inner_pad = [0x36_u8; BLOCK_SIZE];
    let mut outer_pad = [0x5c_u8; BLOCK_SIZE];
    for (index, key_byte) in key_bytes.iter().enumerate() {
        inner_pad[index] ^= key_byte;
        outer_pad[index] ^= key_byte;
    }

    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(data.as_bytes());
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_hash);

    general_purpose::STANDARD.encode(outer.finalize())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_sha256_base64_matches_standard_vector() {
        assert_eq!(
            hmac_sha256_base64("Jefe", "what do ya want for nothing?"),
            "W9zBRr9gdU5qBCQmCJV1x1oAPwidJzmDnexYuWTsOEM="
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn curl_discard_target_uses_windows_null_device() {
        assert_eq!(curl_discard_target(), "NUL");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn curl_discard_target_uses_unix_null_device() {
        assert_eq!(curl_discard_target(), "/dev/null");
    }
}
