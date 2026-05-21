// Webhook forwarding — DingTalk and Feishu (Lark) notification delivery
// DingTalk: msgtype=markdown with HmacSHA256 request signing
// Feishu: msg_type=interactive with timestamp+secret signing
//
// External dependency required: reqwest (async HTTP client with TLS + JSON support)
// Add to Cargo.toml: reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
// Also needed: hmac = "0.12", sha2 = "0.10", base64 = "0.22"

pub mod forwarder;
pub mod templates;

pub use forwarder::{WebhookConfig, WebhookForwarder, WebhookPlatform, WebhookResult};
