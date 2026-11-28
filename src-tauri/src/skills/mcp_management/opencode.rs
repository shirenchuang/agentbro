use super::{read_only_capabilities, McpInventory, McpServerDraft, McpServerEntry, McpTransport};
use regex::{Captures, Regex};
use reqwest::Url;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub(crate) fn list_opencode_mcp(home: &Path) -> Result<McpInventory, String> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    read_inventory(&config_home.join("opencode"))
}

fn read_inventory(directory: &Path) -> Result<McpInventory, String> {
    let mut servers = BTreeMap::new();
    let mut revision = Sha256::new();
    let mut config_path = directory.join("opencode.json");
    for filename in ["opencode.json", "opencode.jsonc"] {
        let path = directory.join(filename);
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("Failed to read {}: {error}", path.display())),
        };
        let parsed = parse_jsonc(&content)
            .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?;
        let root = parsed.as_object().ok_or_else(|| {
            format!(
                "OpenCode configuration must be an object: {}",
                path.display()
            )
        })?;
        if let Some(mcp) = root.get("mcp") {
            let mcp = mcp.as_object().ok_or_else(|| {
                format!(
                    "OpenCode MCP configuration must be an object: {}",
                    path.display()
                )
            })?;
            for (name, value) in mcp {
                if let Some(entry) = server_entry(name, value, &path, false) {
                    servers.insert(name.clone(), entry);
                }
            }
            if let Some(native) = mcp.get("servers").and_then(Value::as_object) {
                for (name, value) in native {
                    if let Some(entry) = server_entry(name, value, &path, true) {
                        servers.insert(name.clone(), entry);
                    }
                }
            }
        }
        revision.update(filename.as_bytes());
        revision.update([0]);
        revision.update(content.as_bytes());
        config_path = path;
    }
    Ok(McpInventory {
        agent_id: "opencode".into(),
        config_path: Some(config_path.display().to_string()),
        revision: format!("sha256:{:x}", revision.finalize()),
        capabilities: read_only_capabilities(),
        servers: servers.into_values().collect(),
    })
}

fn server_entry(name: &str, value: &Value, path: &Path, native: bool) -> Option<McpServerEntry> {
    let (transport, command, url, valid) = match value.get("type")?.as_str()? {
        "local" => {
            let args = value.get("command").and_then(Value::as_array);
            let command = args
                .and_then(|args| args.first())
                .and_then(Value::as_str)
                .filter(|command| !command.trim().is_empty())
                .map(str::to_owned);
            let valid =
                command.is_some() && args.is_some_and(|args| args.iter().all(Value::is_string));
            (McpTransport::Stdio, command, None, valid)
        }
        "remote" => {
            let raw_url = value.get("url").and_then(Value::as_str).unwrap_or_default();
            let url = Url::parse(raw_url)
                .ok()
                .filter(|url| matches!(url.scheme(), "http" | "https") && url.has_host())
                .map(|url| url.origin().ascii_serialization());
            static URL_REFERENCE: OnceLock<Regex> = OnceLock::new();
            let reference = URL_REFERENCE.get_or_init(|| {
                Regex::new(r"\{(?:env|file):[^}]+\}").expect("static OpenCode reference regex")
            });
            let valid = url.is_some() || reference.is_match(raw_url);
            (McpTransport::Http, None, url, valid)
        }
        _ => return None,
    };
    let enabled = if native {
        value.get("disabled").and_then(Value::as_bool) != Some(true)
    } else {
        value.get("enabled").and_then(Value::as_bool) != Some(false)
    };
    Some(McpServerEntry {
        draft: McpServerDraft {
            resolved_env_headers: Vec::new(),
            name: name.into(),
            transport,
            command,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            url,
            headers: Vec::new(),
        },
        enabled,
        disabled_by_agentbro: false,
        valid,
        message: if valid {
            "Configured in OpenCode".into()
        } else {
            "An OpenCode MCP command or HTTP URL is required".into()
        },
        warnings: Vec::new(),
        config_path: path.display().to_string(),
        editable: false,
        source_kind: "configured".into(),
    })
}

fn parse_jsonc(content: &str) -> Result<Value, serde_json::Error> {
    static COMMENTS: OnceLock<Regex> = OnceLock::new();
    static TRAILING_COMMAS: OnceLock<Regex> = OnceLock::new();
    let comments = COMMENTS.get_or_init(|| {
        Regex::new(r#"(?s)("(?:\\.|[^"\\])*")|//[^\r\n]*|/\*.*?\*/"#).expect("static JSONC regex")
    });
    let json = comments.replace_all(
        content.trim_start_matches('\u{feff}'),
        |captures: &Captures| {
            captures.get(1).map_or_else(
                || {
                    captures[0]
                        .chars()
                        .map(|ch| if matches!(ch, '\r' | '\n') { ch } else { ' ' })
                        .collect()
                },
                |quoted| quoted.as_str().to_string(),
            )
        },
    );
    let trailing_commas = TRAILING_COMMAS.get_or_init(|| {
        Regex::new(r#""(?:\\.|[^"\\])*"|,(\s*[}\]])"#).expect("static JSONC regex")
    });
    let json = trailing_commas.replace_all(&json, |captures: &Captures| {
        if let (Some(token), Some(closing)) = (captures.get(0), captures.get(1)) {
            if json[..token.start()]
                .trim_end()
                .ends_with(|ch| !matches!(ch, '{' | '[' | ':' | ','))
            {
                return closing.as_str().to_string();
            }
        }
        captures[0].to_string()
    });
    if json.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    serde_json::from_str(&json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::installer::TempInstallDir;

    #[test]
    fn opencode_inventory_reads_legacy_and_native_user_config_without_credentials() {
        let directory = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let legacy_path = directory.path().join("opencode.json");
        let native_path = directory.path().join("opencode.jsonc");
        let legacy = r#"{
            "mcp": {
                "local": {
                    "type": "local",
                    "command": ["C:\\Program Files\\Agent Fixture\\mcp.exe", "SECRET_ARGUMENT"],
                    "environment": {"Authorization": "SECRET_ENV"},
                    "enabled": false
                },
                "override": {"type": "local", "command": ["old"]}
            }
        }"#;
        let native = r#"{
            /* JSONC comment */
            "mcp": {
                "override": {"type": "local", "command": ["old-same-file"]},
                "servers": {
                    "override": {
                        "type": "remote",
                        "url": "https://user:SECRET_USER@example.invalid/SECRET_PATH?key=SECRET_QUERY",
                        "headers": {"Authorization": "SECRET_HEADER",},
                        "disabled": true,
                    },
                },
            },
        }"#;
        fs::write(&legacy_path, legacy).unwrap();
        fs::write(&native_path, native).unwrap();

        let inventory = read_inventory(directory.path()).unwrap();
        assert!(!inventory.capabilities.editable);
        assert_eq!(inventory.config_path.as_deref(), native_path.to_str());
        assert_eq!(inventory.servers.len(), 2);
        let local = &inventory.servers[0];
        assert_eq!(local.draft.name, "local");
        assert_eq!(local.draft.transport, McpTransport::Stdio);
        assert_eq!(
            local.draft.command.as_deref(),
            Some(r"C:\Program Files\Agent Fixture\mcp.exe")
        );
        assert_eq!(Path::new(&local.config_path), legacy_path);
        assert!(!local.enabled);
        assert!(local.valid);
        let remote = &inventory.servers[1];
        assert_eq!(remote.draft.name, "override");
        assert_eq!(remote.draft.transport, McpTransport::Http);
        assert_eq!(remote.draft.url.as_deref(), Some("https://example.invalid"));
        assert_eq!(Path::new(&remote.config_path), native_path);
        assert!(!remote.enabled);
        assert!(remote.valid);
        assert!(!serde_json::to_string(&inventory)
            .unwrap()
            .contains("SECRET_"));
        assert_eq!(fs::read_to_string(legacy_path).unwrap(), legacy);
        assert_eq!(fs::read_to_string(native_path).unwrap(), native);
    }

    #[test]
    fn opencode_inventory_reports_invalid_jsonc_without_rewriting_it() {
        let directory = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = directory.path().join("opencode.jsonc");
        let content = "{ /* comment */ \"mcp\": {\"servers\": [,]}}";
        fs::write(&path, content).unwrap();
        let error = read_inventory(directory.path()).unwrap_err();
        assert!(error.contains("opencode.jsonc"), "{error}");
        assert_eq!(fs::read_to_string(path).unwrap(), content);
    }
}
