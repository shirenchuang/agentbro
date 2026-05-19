use serde_json::Value;

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

fn write_opencode_config(_provider: &SwitchProvider) -> anyhow::Result<()> {
    // OpenCode config varies by version; stub for now
    Ok(())
}

fn write_hermes_config(_provider: &SwitchProvider) -> anyhow::Result<()> {
    // Hermes config varies by version; stub for now
    Ok(())
}
