use serde::{Deserialize, Serialize};
use std::time::Instant;

use super::app_type::SwitchAppType;
use super::db::SwitchDatabase;
use super::providers;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedTestResult {
    pub provider_id: String,
    pub latency_ms: u64,
    pub success: bool,
    pub error: Option<String>,
    pub status_code: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub provider_id: String,
    pub provider_name: String,
    pub is_current: bool,
    pub has_api_key: bool,
    pub base_url: String,
    pub last_test: Option<SpeedTestResult>,
}

pub fn get_provider_health(
    db: &SwitchDatabase,
    app_type: &SwitchAppType,
) -> anyhow::Result<Vec<ProviderHealth>> {
    let providers = providers::list_providers(db, app_type)?;
    let result = providers
        .iter()
        .map(|p| {
            let sc = p.settings_config.as_object();
            let api_key = sc
                .and_then(|o| o.get("primaryApiKey"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let base_url = sc
                .and_then(|o| o.get("baseUrl"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            ProviderHealth {
                provider_id: p.id.clone(),
                provider_name: p.name.clone(),
                is_current: p.is_current,
                has_api_key: !api_key.is_empty(),
                base_url: base_url.to_string(),
                last_test: None,
            }
        })
        .collect();
    Ok(result)
}

pub async fn speed_test(
    db: &SwitchDatabase,
    app_type: &SwitchAppType,
    provider_id: &str,
) -> anyhow::Result<SpeedTestResult> {
    let provider = providers::get_provider(db, app_type, provider_id)?
        .ok_or_else(|| anyhow::anyhow!("provider not found"))?;

    let sc = provider.settings_config.as_object();
    let api_key = sc
        .and_then(|o| o.get("primaryApiKey"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let base_url = sc
        .and_then(|o| o.get("baseUrl"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if base_url.is_empty() {
        return Ok(SpeedTestResult {
            provider_id: provider_id.to_string(),
            latency_ms: 0,
            success: false,
            error: Some("No base URL configured".into()),
            status_code: None,
        });
    }

    let url = if base_url.contains("anthropic.com") {
        format!("{}/v1/messages", base_url.trim_end_matches('/'))
    } else {
        format!("{}/models", base_url.trim_end_matches('/'))
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let start = Instant::now();
    let mut req = client.get(&url);
    if !api_key.is_empty() {
        if base_url.contains("anthropic.com") {
            req = req
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01");
        } else {
            req = req.header("Authorization", format!("Bearer {api_key}"));
        }
    }

    match req.send().await {
        Ok(resp) => {
            let latency = start.elapsed().as_millis() as u64;
            let status = resp.status().as_u16();
            Ok(SpeedTestResult {
                provider_id: provider_id.to_string(),
                latency_ms: latency,
                success: status < 500,
                error: if status >= 400 {
                    Some(format!("HTTP {status}"))
                } else {
                    None
                },
                status_code: Some(status),
            })
        }
        Err(e) => {
            let latency = start.elapsed().as_millis() as u64;
            Ok(SpeedTestResult {
                provider_id: provider_id.to_string(),
                latency_ms: latency,
                success: false,
                error: Some(e.to_string()),
                status_code: None,
            })
        }
    }
}
