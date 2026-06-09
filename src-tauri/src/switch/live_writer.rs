use serde_json::{Map, Value};

use super::app_type::SwitchAppType;
use super::providers::SwitchProvider;

pub fn write_provider_to_agent_config(provider: &SwitchProvider) -> anyhow::Result<()> {
    let app_type: SwitchAppType = provider
        .app_type
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;
    match app_type {
        SwitchAppType::Claude => write_claude_config(provider),
        SwitchAppType::Codex => write_codex_config(provider),
        SwitchAppType::Gemini => write_gemini_config(provider),
        SwitchAppType::OpenCode => write_opencode_config(provider),
        SwitchAppType::Hermes => write_hermes_config(provider),
    }
}

fn home() -> anyhow::Result<std::path::PathBuf> {
    dirs::home_dir().ok_or_else(|| anyhow::anyhow!("home directory not found"))
}

fn read_json_file(path: &std::path::Path) -> Value {
    if path.exists() {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Object(Default::default()))
    } else {
        Value::Object(Default::default())
    }
}

fn write_json_file(path: &std::path::Path, value: &Value) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

fn write_claude_config(provider: &SwitchProvider) -> anyhow::Result<()> {
    let config_path = home()?.join(".claude").join("settings.json");
    let mut config = read_json_file(&config_path);

    if let Some(obj) = config.as_object_mut() {
        if let Some(sc) = provider.settings_config.as_object() {
            if let Some(env) = sc.get("env") {
                obj.insert("env".into(), env.clone());
            }
            if let Some(api_key) = sc.get("primaryApiKey") {
                obj.insert("primaryApiKey".into(), api_key.clone());
            }
            if let Some(base_url) = sc.get("baseUrl") {
                if let Some(env_obj) = obj
                    .entry("env")
                    .or_insert_with(|| Value::Object(Default::default()))
                    .as_object_mut()
                {
                    env_obj.insert("ANTHROPIC_BASE_URL".into(), base_url.clone());
                }
            }
        }
    }

    write_json_file(&config_path, &config)
}

fn write_codex_config(provider: &SwitchProvider) -> anyhow::Result<()> {
    let codex_dir = home()?.join(".codex");
    std::fs::create_dir_all(&codex_dir)?;

    if let Some(sc) = provider.settings_config.as_object() {
        // Write auth.json
        if sc.contains_key("primaryApiKey") || sc.contains_key("apiKey") {
            let auth_path = codex_dir.join("auth.json");
            let mut auth = read_json_file(&auth_path);
            if let Some(obj) = auth.as_object_mut() {
                if let Some(key) = sc.get("primaryApiKey").or(sc.get("apiKey")) {
                    obj.insert("api_key".into(), key.clone());
                }
            }
            write_json_file(&auth_path, &auth)?;
        }

        // Write env to config.toml-compatible env vars
        if let Some(base_url) = sc.get("baseUrl") {
            let config_path = codex_dir.join("config.toml");
            let mut content = if config_path.exists() {
                std::fs::read_to_string(&config_path)?
            } else {
                String::new()
            };
            let base_url_str = base_url.as_str().unwrap_or("");
            if !base_url_str.is_empty() {
                if content.contains("OPENAI_BASE_URL") {
                    let lines: Vec<String> = content
                        .lines()
                        .map(|l| {
                            if l.trim_start().starts_with("OPENAI_BASE_URL") {
                                format!("OPENAI_BASE_URL = \"{}\"", base_url_str)
                            } else {
                                l.to_string()
                            }
                        })
                        .collect();
                    content = lines.join("\n");
                } else {
                    if !content.is_empty() && !content.ends_with('\n') {
                        content.push('\n');
                    }
                    content.push_str(&format!("OPENAI_BASE_URL = \"{}\"\n", base_url_str));
                }
                std::fs::write(&config_path, content)?;
            }
        }
    }
    Ok(())
}

fn write_gemini_config(provider: &SwitchProvider) -> anyhow::Result<()> {
    if let Some(sc) = provider.settings_config.as_object() {
        if let Some(key) = sc.get("primaryApiKey").or(sc.get("apiKey")) {
            if let Some(key_str) = key.as_str() {
                let gemini_dir = home()?.join(".gemini");
                std::fs::create_dir_all(&gemini_dir)?;
                let settings_path = gemini_dir.join("settings.json");
                let mut config = read_json_file(&settings_path);
                if let Some(obj) = config.as_object_mut() {
                    obj.insert("apiKey".into(), Value::String(key_str.to_string()));
                }
                write_json_file(&settings_path, &config)?;
            }
        }
    }
    Ok(())
}

fn write_opencode_config(provider: &SwitchProvider) -> anyhow::Result<()> {
    let config_path = home()?
        .join(".config")
        .join("opencode")
        .join("opencode.json");
    write_opencode_config_at(provider, &config_path)
}

fn write_opencode_config_at(
    provider: &SwitchProvider,
    config_path: &std::path::Path,
) -> anyhow::Result<()> {
    let mut config = read_json_file(config_path);
    if !config.is_object() {
        config = Value::Object(Map::new());
    }

    let sc = provider.settings_config.as_object();
    let env = sc
        .and_then(|settings| settings.get("env"))
        .and_then(Value::as_object);
    let meta = provider.meta.as_object();
    let provider_id = opencode_provider_id(provider, sc, meta);
    let provider_name = non_empty_string(
        sc.and_then(|settings| settings.get("displayName"))
            .or_else(|| meta.and_then(|settings| settings.get("displayName"))),
    )
    .map(str::to_string)
    .unwrap_or_else(|| provider.name.clone());

    let api_key = first_string_value(&[
        sc.and_then(|settings| settings.get("primaryApiKey")),
        sc.and_then(|settings| settings.get("apiKey")),
        env.and_then(|settings| settings.get("ANTHROPIC_AUTH_TOKEN")),
        env.and_then(|settings| settings.get("ANTHROPIC_API_KEY")),
        env.and_then(|settings| settings.get("OPENAI_API_KEY")),
    ]);
    let base_url = first_string_value(&[
        sc.and_then(|settings| settings.get("baseUrl")),
        sc.and_then(|settings| settings.get("baseURL")),
        env.and_then(|settings| settings.get("ANTHROPIC_BASE_URL")),
        env.and_then(|settings| settings.get("OPENAI_BASE_URL")),
        env.and_then(|settings| settings.get("OPENAI_API_BASE")),
    ]);
    let default_model = first_string_value(&[
        sc.and_then(|settings| settings.get("model")),
        sc.and_then(|settings| settings.get("defaultModel")),
        env.and_then(|settings| settings.get("ANTHROPIC_MODEL")),
        env.and_then(|settings| settings.get("ANTHROPIC_DEFAULT_SONNET_MODEL")),
        env.and_then(|settings| settings.get("ANTHROPIC_DEFAULT_OPUS_MODEL")),
    ]);
    let small_model = first_string_value(&[
        sc.and_then(|settings| settings.get("smallModel")),
        sc.and_then(|settings| settings.get("small_model")),
        env.and_then(|settings| settings.get("ANTHROPIC_DEFAULT_HAIKU_MODEL")),
    ]);

    let Some(root) = config.as_object_mut() else {
        anyhow::bail!("OpenCode config root is not a JSON object");
    };
    {
        let provider_root = root
            .entry("provider")
            .or_insert_with(|| Value::Object(Map::new()));
        if !provider_root.is_object() {
            *provider_root = Value::Object(Map::new());
        }
        let provider_map = provider_root
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("OpenCode provider config is not a JSON object"))?;

        let entry = provider_map
            .entry(provider_id.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        let provider_entry = entry
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("OpenCode provider entry is not a JSON object"))?;

        provider_entry
            .entry("npm")
            .or_insert_with(|| Value::String("@ai-sdk/openai-compatible".to_string()));
        provider_entry.insert("name".into(), Value::String(provider_name));

        if api_key.is_some() || base_url.is_some() {
            let options = provider_entry
                .entry("options")
                .or_insert_with(|| Value::Object(Map::new()));
            if !options.is_object() {
                *options = Value::Object(Map::new());
            }
            let options = options.as_object_mut().ok_or_else(|| {
                anyhow::anyhow!("OpenCode provider options are not a JSON object")
            })?;
            if let Some(value) = api_key {
                options.insert("apiKey".into(), Value::String(value));
            }
            if let Some(value) = base_url {
                options.insert("baseURL".into(), Value::String(value));
            }
        }

        let model_keys = [default_model.as_deref(), small_model.as_deref()]
            .into_iter()
            .flatten()
            .filter_map(|value| opencode_model_key(value, &provider_id))
            .collect::<Vec<_>>();
        if !model_keys.is_empty() {
            let models = provider_entry
                .entry("models")
                .or_insert_with(|| Value::Object(Map::new()));
            if !models.is_object() {
                *models = Value::Object(Map::new());
            }
            let models = models
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("OpenCode provider models are not a JSON object"))?;
            for model in model_keys {
                models
                    .entry(model.clone())
                    .or_insert_with(|| serde_json::json!({ "name": model }));
            }
        }
    }

    if let Some(value) = default_model.and_then(|value| opencode_model_ref(&value, &provider_id)) {
        root.insert("model".into(), Value::String(value));
    }
    if let Some(value) = small_model.and_then(|value| opencode_model_ref(&value, &provider_id)) {
        root.insert("small_model".into(), Value::String(value));
    }

    write_json_file(config_path, &config)
}

fn write_hermes_config(_provider: &SwitchProvider) -> anyhow::Result<()> {
    // Hermes config varies by version; stub for now
    Ok(())
}

fn opencode_provider_id(
    provider: &SwitchProvider,
    sc: Option<&Map<String, Value>>,
    meta: Option<&Map<String, Value>>,
) -> String {
    let raw = first_string_value(&[
        sc.and_then(|settings| settings.get("opencodeProviderId")),
        sc.and_then(|settings| settings.get("providerId")),
        sc.and_then(|settings| settings.get("provider")),
        meta.and_then(|settings| settings.get("opencodeProviderId")),
        meta.and_then(|settings| settings.get("providerId")),
    ])
    .unwrap_or_else(|| provider.id.clone());
    sanitize_opencode_id(&raw)
}

fn first_string_value(values: &[Option<&Value>]) -> Option<String> {
    values
        .iter()
        .find_map(|value| non_empty_string(*value).map(str::to_string))
}

fn non_empty_string(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn sanitize_opencode_id(value: &str) -> String {
    let sanitized = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "agentbro-provider".to_string()
    } else {
        sanitized
    }
}

fn opencode_model_key(model: &str, provider_id: &str) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return None;
    }
    let prefix = format!("{}/", provider_id);
    let key = if let Some(rest) = trimmed.strip_prefix(&prefix) {
        rest
    } else {
        trimmed
            .rsplit_once('/')
            .map(|(_, model)| model)
            .unwrap_or(trimmed)
    };
    let key = key.trim();
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

fn opencode_model_ref(model: &str, provider_id: &str) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('/') {
        Some(trimmed.to_string())
    } else {
        Some(format!("{}/{}", provider_id, trimmed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opencode_provider(settings_config: Value, meta: Value) -> SwitchProvider {
        SwitchProvider {
            id: "VolcEngine AgentPlan".to_string(),
            app_type: "opencode".to_string(),
            name: "VolcEngine AgentPlan".to_string(),
            settings_config,
            website_url: None,
            category: None,
            icon: None,
            icon_color: None,
            meta,
            is_current: false,
            in_failover_queue: false,
            created_at: None,
            sort_index: None,
            notes: None,
        }
    }

    #[test]
    fn opencode_writer_preserves_plugin_and_writes_provider() {
        let dir = std::env::temp_dir().join(format!("agentbro-opencode-{}", uuid::Uuid::new_v4()));
        let config_path = dir.join("opencode.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &config_path,
            serde_json::json!({
                "plugin": ["file://C:\\Users\\me\\.config\\opencode\\plugins\\agentbro.js"]
            })
            .to_string(),
        )
        .unwrap();

        let provider = opencode_provider(
            serde_json::json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://ark.cn-beijing.volces.com/api/coding",
                    "ANTHROPIC_AUTH_TOKEN": "sk-test",
                    "ANTHROPIC_MODEL": "ark-code-latest",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "ark-code-lite"
                }
            }),
            Value::Object(Map::new()),
        );

        write_opencode_config_at(&provider, &config_path).unwrap();

        let config: Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(
            config["plugin"][0],
            "file://C:\\Users\\me\\.config\\opencode\\plugins\\agentbro.js"
        );
        assert_eq!(config["model"], "volcengine-agentplan/ark-code-latest");
        assert_eq!(config["small_model"], "volcengine-agentplan/ark-code-lite");
        assert_eq!(
            config["provider"]["volcengine-agentplan"]["npm"],
            "@ai-sdk/openai-compatible"
        );
        assert_eq!(
            config["provider"]["volcengine-agentplan"]["options"]["baseURL"],
            "https://ark.cn-beijing.volces.com/api/coding"
        );
        assert_eq!(
            config["provider"]["volcengine-agentplan"]["options"]["apiKey"],
            "sk-test"
        );
        assert!(
            config["provider"]["volcengine-agentplan"]["models"]["ark-code-latest"].is_object()
        );
        assert!(config["provider"]["volcengine-agentplan"]["models"]["ark-code-lite"].is_object());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn opencode_model_refs_keep_already_qualified_models() {
        assert_eq!(
            opencode_model_ref("openai/gpt-4.1", "agentbro-openai"),
            Some("openai/gpt-4.1".to_string())
        );
        assert_eq!(
            opencode_model_key("openai/gpt-4.1", "agentbro-openai"),
            Some("gpt-4.1".to_string())
        );
    }
}
