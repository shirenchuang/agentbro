use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub model_id: String,
    pub display_name: String,
    pub provider: String,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_read_per_mtok: Option<f64>,
    pub cache_write_per_mtok: Option<f64>,
}

pub fn default_pricing() -> Vec<ModelPricing> {
    vec![
        ModelPricing {
            model_id: "claude-opus-4-20250514".into(),
            display_name: "Claude Opus 4".into(),
            provider: "anthropic".into(),
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
            cache_read_per_mtok: Some(1.5),
            cache_write_per_mtok: Some(18.75),
        },
        ModelPricing {
            model_id: "claude-sonnet-4-20250514".into(),
            display_name: "Claude Sonnet 4".into(),
            provider: "anthropic".into(),
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.3),
            cache_write_per_mtok: Some(3.75),
        },
        ModelPricing {
            model_id: "claude-3-5-sonnet-20241022".into(),
            display_name: "Claude 3.5 Sonnet".into(),
            provider: "anthropic".into(),
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.3),
            cache_write_per_mtok: Some(3.75),
        },
        ModelPricing {
            model_id: "claude-3-5-haiku-20241022".into(),
            display_name: "Claude 3.5 Haiku".into(),
            provider: "anthropic".into(),
            input_per_mtok: 0.8,
            output_per_mtok: 4.0,
            cache_read_per_mtok: Some(0.08),
            cache_write_per_mtok: Some(1.0),
        },
        ModelPricing {
            model_id: "gpt-4o".into(),
            display_name: "GPT-4o".into(),
            provider: "openai".into(),
            input_per_mtok: 2.5,
            output_per_mtok: 10.0,
            cache_read_per_mtok: Some(1.25),
            cache_write_per_mtok: None,
        },
        ModelPricing {
            model_id: "gpt-4.1".into(),
            display_name: "GPT-4.1".into(),
            provider: "openai".into(),
            input_per_mtok: 2.0,
            output_per_mtok: 8.0,
            cache_read_per_mtok: Some(0.5),
            cache_write_per_mtok: None,
        },
        ModelPricing {
            model_id: "o3-mini".into(),
            display_name: "o3-mini".into(),
            provider: "openai".into(),
            input_per_mtok: 1.1,
            output_per_mtok: 4.4,
            cache_read_per_mtok: Some(0.55),
            cache_write_per_mtok: None,
        },
        ModelPricing {
            model_id: "gemini-2.5-pro".into(),
            display_name: "Gemini 2.5 Pro".into(),
            provider: "google".into(),
            input_per_mtok: 1.25,
            output_per_mtok: 10.0,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        },
        ModelPricing {
            model_id: "deepseek-chat".into(),
            display_name: "DeepSeek V3".into(),
            provider: "deepseek".into(),
            input_per_mtok: 0.27,
            output_per_mtok: 1.1,
            cache_read_per_mtok: Some(0.07),
            cache_write_per_mtok: None,
        },
        ModelPricing {
            model_id: "deepseek-reasoner".into(),
            display_name: "DeepSeek R1".into(),
            provider: "deepseek".into(),
            input_per_mtok: 0.55,
            output_per_mtok: 2.19,
            cache_read_per_mtok: Some(0.14),
            cache_write_per_mtok: None,
        },
    ]
}

pub fn find_pricing(model_id: &str) -> Option<ModelPricing> {
    default_pricing()
        .into_iter()
        .find(|p| p.model_id == model_id || model_id.starts_with(&p.model_id))
}

pub fn estimate_cost(model_id: &str, input_tokens: u64, output_tokens: u64) -> Option<f64> {
    let pricing = find_pricing(model_id)?;
    let cost = (input_tokens as f64 / 1_000_000.0) * pricing.input_per_mtok
        + (output_tokens as f64 / 1_000_000.0) * pricing.output_per_mtok;
    Some(cost)
}
