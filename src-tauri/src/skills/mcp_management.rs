use regex::Regex;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::Notify;

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MCP_PROTOCOL_FALLBACKS: [&str; 2] = ["2025-06-18", "2024-11-05"];
const MCP_TEST_TIMEOUT: Duration = Duration::from_secs(15);
const MCP_INSPECTION_TIMEOUT: Duration = Duration::from_secs(20);
const MCP_INSPECTION_STEP_TIMEOUT: Duration = Duration::from_secs(5);
const MCP_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
const MCP_OPERATION_MAX_RESULT_BYTES: usize = 2 * 1024 * 1024;
const MCP_INSPECTION_MAX_PAGES: usize = 10;
const MCP_INSPECTION_MAX_ITEMS: usize = 500;
const REDACTED_VALUE: &str = "••••••••";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    Stdio,
    Http,
    Sse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigValue {
    pub key: String,
    pub value: Option<String>,
    pub secret: bool,
    pub configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDraft {
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<McpConfigValue>,
    pub cwd: Option<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub headers: Vec<McpConfigValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerEntry {
    #[serde(flatten)]
    pub draft: McpServerDraft,
    pub enabled: bool,
    pub disabled_by_agentbro: bool,
    pub valid: bool,
    pub message: String,
    pub warnings: Vec<String>,
    pub config_path: String,
    pub editable: bool,
    pub source_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCapabilities {
    pub editable: bool,
    pub supports_stdio: bool,
    pub supports_http: bool,
    pub supports_sse: bool,
    pub supports_native_toggle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInventory {
    pub agent_id: String,
    pub config_path: Option<String>,
    pub revision: String,
    pub capabilities: McpCapabilities,
    pub servers: Vec<McpServerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpValidationResultV2 {
    pub valid: bool,
    pub message: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConnectionTestResult {
    pub success: bool,
    pub category: String,
    pub message: String,
    pub latency_ms: u64,
    pub protocol_version: Option<String>,
    pub server_name: Option<String>,
    pub server_version: Option<String>,
    pub tool_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInspectionCapabilities {
    pub tools: bool,
    pub resources: bool,
    pub prompts: bool,
    pub logging: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInspectionToolAnnotations {
    pub read_only: Option<bool>,
    pub destructive: Option<bool>,
    pub idempotent: Option<bool>,
    pub open_world: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInspectionInput {
    pub name: String,
    pub value_type: String,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInspectionTool {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub inputs: Vec<McpInspectionInput>,
    pub input_schema: Value,
    pub output_schema: Option<Value>,
    pub annotations: McpInspectionToolAnnotations,
    pub has_annotations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInspectionResource {
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInspectionPromptArgument {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInspectionPrompt {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub arguments: Vec<McpInspectionPromptArgument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInspectionStep {
    pub phase: String,
    pub status: String,
    pub duration_ms: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInspectionReport {
    pub inspection_id: String,
    pub status: String,
    pub category: String,
    pub summary: String,
    pub inspected_at_ms: u64,
    pub duration_ms: u64,
    pub protocol_version: Option<String>,
    pub server_name: Option<String>,
    pub server_version: Option<String>,
    pub transport: McpTransport,
    pub capabilities: McpInspectionCapabilities,
    pub tools: Vec<McpInspectionTool>,
    pub resources: Vec<McpInspectionResource>,
    pub prompts: Vec<McpInspectionPrompt>,
    pub steps: Vec<McpInspectionStep>,
    pub warnings: Vec<String>,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOperationResult {
    pub operation_id: String,
    pub kind: String,
    pub name: String,
    pub category: String,
    pub duration_ms: u64,
    pub result: Value,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct RawServer {
    draft: McpServerDraft,
    enabled: bool,
    raw: Value,
}

#[derive(Debug, Clone)]
struct AgentConfig {
    agent_id: String,
    path: PathBuf,
    kind: ConfigKind,
    capabilities: McpCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigKind {
    Claude,
    Codex,
    Gemini,
    Cursor,
    Kimi,
    Zcode,
}

pub fn list_mcp_servers(agent_id: &str) -> Result<McpInventory, String> {
    let Some(config) = config_for_agent(agent_id) else {
        return Ok(McpInventory {
            agent_id: agent_id.to_string(),
            config_path: None,
            revision: revision_for(agent_id, None)?,
            capabilities: read_only_capabilities(),
            servers: Vec::new(),
        });
    };
    let mut raw = read_active_servers(&config)?;
    if uses_disabled_store(config.kind) {
        for (name, stored) in read_disabled_servers(agent_id)? {
            raw.entry(name).or_insert(stored);
        }
    }
    let revision = revision_for(agent_id, Some(&config.path))?;
    let mut servers = raw
        .into_iter()
        .map(|(name, mut server)| {
            server.draft.name = name;
            let validation =
                validate_mcp_server_draft_inner(&config, &server.draft, None, &HashSet::new());
            let disabled_by_agentbro = uses_disabled_store(config.kind) && !server.enabled;
            McpServerEntry {
                draft: redact_draft(server.draft),
                enabled: server.enabled,
                disabled_by_agentbro,
                valid: validation.valid,
                message: validation.message,
                warnings: validation.warnings,
                config_path: config.path.display().to_string(),
                editable: config.capabilities.editable,
                source_kind: "configured".to_string(),
            }
        })
        .collect::<Vec<_>>();
    servers.sort_by_key(|server| server.draft.name.to_lowercase());
    Ok(McpInventory {
        agent_id: agent_id.to_string(),
        config_path: Some(config.path.display().to_string()),
        revision,
        capabilities: config.capabilities,
        servers,
    })
}

pub fn validate_mcp_server_draft(
    agent_id: &str,
    draft: &McpServerDraft,
    original_name: Option<&str>,
) -> Result<McpValidationResultV2, String> {
    let config = config_for_agent(agent_id)
        .ok_or_else(|| format!("Agent {agent_id} does not support editable MCP configuration"))?;
    let existing = read_active_servers(&config)?
        .keys()
        .cloned()
        .chain(read_disabled_servers(agent_id)?.into_keys())
        .collect::<HashSet<_>>();
    Ok(validate_mcp_server_draft_inner(
        &config,
        draft,
        original_name,
        &existing,
    ))
}

pub fn save_mcp_server(
    agent_id: &str,
    original_name: Option<&str>,
    revision: &str,
    draft: &McpServerDraft,
) -> Result<McpInventory, String> {
    let config = config_for_agent(agent_id)
        .ok_or_else(|| format!("Agent {agent_id} does not support editable MCP configuration"))?;
    ensure_revision(agent_id, Some(&config.path), revision)?;
    let active = read_active_servers(&config)?;
    let disabled = read_disabled_servers(agent_id)?;
    let existing = active
        .keys()
        .cloned()
        .chain(disabled.keys().cloned())
        .collect::<HashSet<_>>();
    let validation = validate_mcp_server_draft_inner(&config, draft, original_name, &existing);
    if !validation.valid {
        return Err(validation.message);
    }

    let original = original_name.unwrap_or(&draft.name);
    let previous = active
        .get(original)
        .or_else(|| disabled.get(original))
        .cloned();
    let resolved = resolve_secret_placeholders(draft, previous.as_ref().map(|item| &item.draft))?;
    let was_disabled = disabled.contains_key(original);

    if was_disabled && uses_disabled_store(config.kind) {
        let mut next = disabled;
        next.remove(original);
        next.insert(
            resolved.name.clone(),
            RawServer {
                draft: resolved.clone(),
                enabled: false,
                raw: raw_value_for(&config, &resolved, false, previous.as_ref().map(|p| &p.raw)),
            },
        );
        write_disabled_servers(agent_id, &next)?;
    } else {
        write_active_server(
            &config,
            original,
            &resolved,
            previous.as_ref().map(|item| &item.raw),
            previous.as_ref().map(|item| item.enabled).unwrap_or(true),
        )?;
    }
    list_mcp_servers(agent_id)
}

pub fn set_mcp_server_enabled(
    agent_id: &str,
    server_name: &str,
    revision: &str,
    enabled: bool,
) -> Result<McpInventory, String> {
    let config = config_for_agent(agent_id)
        .ok_or_else(|| format!("Agent {agent_id} does not support editable MCP configuration"))?;
    ensure_revision(agent_id, Some(&config.path), revision)?;
    if uses_disabled_store(config.kind) {
        set_sidecar_enabled(&config, server_name, enabled)?;
    } else {
        set_native_enabled(&config, server_name, enabled)?;
    }
    list_mcp_servers(agent_id)
}

pub fn delete_mcp_server(
    agent_id: &str,
    server_name: &str,
    revision: &str,
) -> Result<McpInventory, String> {
    let config = config_for_agent(agent_id)
        .ok_or_else(|| format!("Agent {agent_id} does not support editable MCP configuration"))?;
    ensure_revision(agent_id, Some(&config.path), revision)?;
    let mut disabled = read_disabled_servers(agent_id)?;
    if disabled.remove(server_name).is_some() {
        write_disabled_servers(agent_id, &disabled)?;
    } else {
        remove_active_server(&config, server_name)?;
    }
    list_mcp_servers(agent_id)
}

fn config_for_agent(agent_id: &str) -> Option<AgentConfig> {
    let home = super::v2::fsutil::home();
    let config = match agent_id {
        "claude-code" => AgentConfig {
            agent_id: agent_id.to_string(),
            path: home.join(".claude.json"),
            kind: ConfigKind::Claude,
            capabilities: capabilities(true, true, true, false),
        },
        "codex" => AgentConfig {
            agent_id: agent_id.to_string(),
            path: home.join(".codex").join("config.toml"),
            kind: ConfigKind::Codex,
            capabilities: capabilities(true, true, false, true),
        },
        "gemini" | "gemini-cli" => AgentConfig {
            agent_id: agent_id.to_string(),
            path: home.join(".gemini").join("settings.json"),
            kind: ConfigKind::Gemini,
            capabilities: capabilities(true, true, true, true),
        },
        "cursor" | "cursor-cli" => AgentConfig {
            agent_id: agent_id.to_string(),
            path: home.join(".cursor").join("mcp.json"),
            kind: ConfigKind::Cursor,
            capabilities: capabilities(true, true, true, false),
        },
        "kimi" | "kimi-code-cli" => AgentConfig {
            agent_id: agent_id.to_string(),
            path: super::agent_paths::kimi_code_home().join("mcp.json"),
            kind: ConfigKind::Kimi,
            capabilities: capabilities(true, true, true, true),
        },
        "zcode" => AgentConfig {
            agent_id: agent_id.to_string(),
            path: home.join(".zcode").join("cli").join("config.json"),
            kind: ConfigKind::Zcode,
            capabilities: capabilities(true, true, true, true),
        },
        _ => return None,
    };
    Some(config)
}

fn capabilities(
    supports_stdio: bool,
    supports_http: bool,
    supports_sse: bool,
    supports_native_toggle: bool,
) -> McpCapabilities {
    McpCapabilities {
        editable: true,
        supports_stdio,
        supports_http,
        supports_sse,
        supports_native_toggle,
    }
}

fn read_only_capabilities() -> McpCapabilities {
    McpCapabilities {
        editable: false,
        supports_stdio: false,
        supports_http: false,
        supports_sse: false,
        supports_native_toggle: false,
    }
}

fn read_active_servers(config: &AgentConfig) -> Result<BTreeMap<String, RawServer>, String> {
    if config.kind == ConfigKind::Codex {
        let content = read_text_or_empty(&config.path)?;
        return Ok(parse_codex_servers(&content));
    }
    let root = read_json_or_empty(&config.path)?;
    let servers = json_servers(&root, config.kind)
        .cloned()
        .unwrap_or_default();
    let excluded = if config.kind == ConfigKind::Gemini {
        root.pointer("/mcp/excluded")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default()
    } else {
        HashSet::new()
    };
    Ok(servers
        .into_iter()
        .filter_map(|(name, raw)| {
            normalize_json_server(config.kind, &name, &raw, excluded.contains(&name))
                .map(|server| (name, server))
        })
        .collect())
}

fn json_servers(root: &Value, kind: ConfigKind) -> Option<&Map<String, Value>> {
    if kind == ConfigKind::Zcode {
        root.pointer("/mcp/servers").and_then(Value::as_object)
    } else {
        root.get("mcpServers").and_then(Value::as_object)
    }
}

fn json_servers_mut(root: &mut Value, kind: ConfigKind) -> Result<&mut Map<String, Value>, String> {
    if !root.is_object() {
        return Err("MCP config root must be an object".to_string());
    }
    if kind == ConfigKind::Zcode {
        let object = root.as_object_mut().expect("checked object");
        let mcp = object
            .entry("mcp")
            .or_insert_with(|| Value::Object(Map::new()));
        let mcp = mcp
            .as_object_mut()
            .ok_or("ZCode mcp config must be an object")?;
        let servers = mcp
            .entry("servers")
            .or_insert_with(|| Value::Object(Map::new()));
        return servers
            .as_object_mut()
            .ok_or_else(|| "ZCode mcp.servers must be an object".to_string());
    }
    root.as_object_mut()
        .expect("checked object")
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "mcpServers must be an object".to_string())
}

fn normalize_json_server(
    kind: ConfigKind,
    name: &str,
    raw: &Value,
    gemini_excluded: bool,
) -> Option<RawServer> {
    let object = raw.as_object()?;
    let command = object
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_string);
    let explicit_type = object
        .get("type")
        .or_else(|| object.get("transport"))
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    let (transport, url) = if let Some(command) = command.as_ref().filter(|value| !value.is_empty())
    {
        let _ = command;
        (McpTransport::Stdio, None)
    } else if kind == ConfigKind::Gemini {
        if let Some(url) = object.get("httpUrl").and_then(Value::as_str) {
            (McpTransport::Http, Some(url.to_string()))
        } else {
            let url = object.get("url").and_then(Value::as_str)?.to_string();
            let transport = if explicit_type.as_deref() == Some("sse") {
                McpTransport::Sse
            } else {
                McpTransport::Sse
            };
            (transport, Some(url))
        }
    } else {
        let url = object.get("url").and_then(Value::as_str)?.to_string();
        let transport = if matches!(explicit_type.as_deref(), Some("sse")) {
            McpTransport::Sse
        } else {
            McpTransport::Http
        };
        (transport, Some(url))
    };
    let enabled = match kind {
        ConfigKind::Gemini => !gemini_excluded,
        ConfigKind::Kimi => object
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        ConfigKind::Zcode => object
            .get("enable")
            .or_else(|| object.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(true),
        _ => !object
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    let draft = McpServerDraft {
        name: name.to_string(),
        transport,
        command,
        args: string_array(object.get("args")),
        env: config_values(object.get("env")),
        cwd: object
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::to_string),
        url,
        headers: config_values(object.get("headers").or_else(|| object.get("http_headers"))),
    };
    Some(RawServer {
        draft,
        enabled,
        raw: raw.clone(),
    })
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn config_values(value: Option<&Value>) -> Vec<McpConfigValue> {
    value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| McpConfigValue {
                        key: key.clone(),
                        value: Some(value.to_string()),
                        secret: is_sensitive_key(key),
                        configured: true,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn raw_value_for(
    config: &AgentConfig,
    draft: &McpServerDraft,
    enabled: bool,
    existing: Option<&Value>,
) -> Value {
    let mut object = existing
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for key in [
        "command",
        "args",
        "env",
        "cwd",
        "url",
        "httpUrl",
        "headers",
        "http_headers",
        "type",
        "transport",
    ] {
        object.remove(key);
    }
    match draft.transport {
        McpTransport::Stdio => {
            object.insert(
                "command".to_string(),
                Value::String(draft.command.clone().unwrap_or_default()),
            );
            object.insert(
                "args".to_string(),
                Value::Array(draft.args.iter().cloned().map(Value::String).collect()),
            );
            if config.kind == ConfigKind::Claude || config.kind == ConfigKind::Zcode {
                object.insert("type".to_string(), Value::String("stdio".to_string()));
            }
            if let Some(cwd) = nonempty(draft.cwd.as_deref()) {
                object.insert("cwd".to_string(), Value::String(cwd.to_string()));
            }
            insert_config_values(&mut object, "env", &draft.env);
        }
        McpTransport::Http | McpTransport::Sse => {
            let url = draft.url.clone().unwrap_or_default();
            if config.kind == ConfigKind::Gemini && draft.transport == McpTransport::Http {
                object.insert("httpUrl".to_string(), Value::String(url));
            } else {
                object.insert("url".to_string(), Value::String(url));
            }
            match config.kind {
                ConfigKind::Claude | ConfigKind::Cursor | ConfigKind::Zcode => {
                    object.insert(
                        "type".to_string(),
                        Value::String(if draft.transport == McpTransport::Sse {
                            "sse".to_string()
                        } else {
                            "http".to_string()
                        }),
                    );
                }
                ConfigKind::Kimi if draft.transport == McpTransport::Sse => {
                    object.insert("transport".to_string(), Value::String("sse".to_string()));
                }
                _ => {}
            }
            insert_config_values(&mut object, "headers", &draft.headers);
        }
    }
    match config.kind {
        ConfigKind::Kimi => {
            object.insert("enabled".to_string(), Value::Bool(enabled));
        }
        ConfigKind::Zcode => {
            object.insert("enable".to_string(), Value::Bool(enabled));
        }
        _ => {}
    }
    Value::Object(object)
}

fn insert_config_values(object: &mut Map<String, Value>, key: &str, values: &[McpConfigValue]) {
    if values.is_empty() {
        return;
    }
    object.insert(
        key.to_string(),
        Value::Object(
            values
                .iter()
                .filter_map(|item| {
                    item.value
                        .as_ref()
                        .map(|value| (item.key.clone(), Value::String(value.clone())))
                })
                .collect(),
        ),
    );
}

fn redact_draft(mut draft: McpServerDraft) -> McpServerDraft {
    for value in draft.env.iter_mut().chain(draft.headers.iter_mut()) {
        value.secret = is_sensitive_key(&value.key);
        if value.secret && value.configured {
            value.value = None;
        }
    }
    draft
}

fn resolve_secret_placeholders(
    draft: &McpServerDraft,
    previous: Option<&McpServerDraft>,
) -> Result<McpServerDraft, String> {
    let mut out = draft.clone();
    out.env = resolve_config_values(&draft.env, previous.map(|item| item.env.as_slice()))?;
    out.headers =
        resolve_config_values(&draft.headers, previous.map(|item| item.headers.as_slice()))?;
    Ok(out)
}

fn resolve_config_values(
    values: &[McpConfigValue],
    previous: Option<&[McpConfigValue]>,
) -> Result<Vec<McpConfigValue>, String> {
    let old = previous
        .unwrap_or_default()
        .iter()
        .filter_map(|item| item.value.as_ref().map(|value| (&item.key, value)))
        .collect::<HashMap<_, _>>();
    values
        .iter()
        .map(|item| {
            let value = if let Some(value) = item.value.as_ref() {
                if value == REDACTED_VALUE {
                    old.get(&item.key)
                        .map(|value| (*value).clone())
                        .ok_or_else(|| format!("No existing sensitive value for {}", item.key))?
                } else {
                    value.clone()
                }
            } else if item.configured {
                old.get(&item.key)
                    .map(|value| (*value).clone())
                    .ok_or_else(|| format!("No existing sensitive value for {}", item.key))?
            } else {
                String::new()
            };
            Ok(McpConfigValue {
                key: item.key.clone(),
                value: Some(value),
                secret: is_sensitive_key(&item.key),
                configured: true,
            })
        })
        .collect()
}

fn is_sensitive_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    upper == "AUTHORIZATION"
        || ["TOKEN", "KEY", "SECRET", "PASSWORD", "PASSWD"]
            .iter()
            .any(|needle| upper.contains(needle))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn validate_mcp_server_draft_inner(
    config: &AgentConfig,
    draft: &McpServerDraft,
    original_name: Option<&str>,
    existing_names: &HashSet<String>,
) -> McpValidationResultV2 {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let name = draft.name.trim();
    if name.is_empty() {
        errors.push("MCP server name cannot be empty".to_string());
    } else {
        let pattern = if config.kind == ConfigKind::Claude {
            r"^[A-Za-z0-9_-]+$"
        } else {
            r"^[A-Za-z0-9_.-]+$"
        };
        if !Regex::new(pattern)
            .expect("static MCP name regex")
            .is_match(name)
        {
            errors.push(if config.kind == ConfigKind::Claude {
                "Claude Code MCP names may contain only letters, numbers, hyphens, and underscores"
                    .to_string()
            } else {
                "MCP names may contain only letters, numbers, dots, hyphens, and underscores"
                    .to_string()
            });
        }
    }
    if existing_names.contains(name) && original_name != Some(name) {
        errors.push(format!("An MCP server named {name} already exists"));
    }
    let supported = match draft.transport {
        McpTransport::Stdio => config.capabilities.supports_stdio,
        McpTransport::Http => config.capabilities.supports_http,
        McpTransport::Sse => config.capabilities.supports_sse,
    };
    if !supported {
        errors.push(format!(
            "{} does not support {} MCP servers",
            display_agent(&config.agent_id),
            transport_label(draft.transport)
        ));
    }
    match draft.transport {
        McpTransport::Stdio => {
            let command = nonempty(draft.command.as_deref());
            if command.is_none() {
                errors.push("A command is required for stdio MCP servers".to_string());
            } else if !command_available(command.expect("checked command")) {
                warnings.push(format!(
                    "Command not found in AgentBro's PATH: {}",
                    command.expect("checked command")
                ));
            }
        }
        McpTransport::Http | McpTransport::Sse => {
            let Some(url) = nonempty(draft.url.as_deref()) else {
                errors.push("A URL is required for remote MCP servers".to_string());
                return validation_result(errors, warnings);
            };
            match Url::parse(url) {
                Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => {
                    if parsed.scheme() == "http" && !is_local_host(parsed.host_str()) {
                        warnings.push("This remote MCP server uses unencrypted HTTP".to_string());
                    }
                }
                _ => errors.push("MCP URL must be a valid HTTP or HTTPS URL".to_string()),
            }
        }
    }
    validate_config_value_keys("environment variable", &draft.env, &mut errors);
    validate_config_value_keys("header", &draft.headers, &mut errors);
    validation_result(errors, warnings)
}

fn validation_result(errors: Vec<String>, warnings: Vec<String>) -> McpValidationResultV2 {
    McpValidationResultV2 {
        valid: errors.is_empty(),
        message: if errors.is_empty() {
            if warnings.is_empty() {
                "MCP configuration is valid".to_string()
            } else {
                warnings.join("; ")
            }
        } else {
            errors.join("; ")
        },
        warnings,
    }
}

fn validate_config_value_keys(label: &str, values: &[McpConfigValue], errors: &mut Vec<String>) {
    let mut names = HashSet::new();
    for value in values {
        let key = value.key.trim();
        if key.is_empty() {
            errors.push(format!("{label} name cannot be empty"));
        } else if !names.insert(key.to_ascii_lowercase()) {
            errors.push(format!("Duplicate {label}: {key}"));
        }
    }
}

fn command_available(command: &str) -> bool {
    if command.starts_with('~') {
        return super::v2::fsutil::expand_tilde(command).is_file();
    }
    let path = Path::new(command);
    if path.components().count() > 1 || path.is_absolute() {
        return path.is_file();
    }
    crate::agents::executable::command_exists(command)
}

fn is_local_host(host: Option<&str>) -> bool {
    matches!(
        host.map(str::to_ascii_lowercase).as_deref(),
        Some("localhost") | Some("127.0.0.1") | Some("::1")
    )
}

fn display_agent(agent_id: &str) -> &str {
    match agent_id {
        "claude-code" => "Claude Code",
        "codex" => "Codex",
        "gemini" | "gemini-cli" => "Gemini CLI",
        "cursor" | "cursor-cli" => "Cursor",
        "kimi" | "kimi-code-cli" => "Kimi Code",
        "zcode" => "ZCode",
        other => other,
    }
}

fn transport_label(transport: McpTransport) -> &'static str {
    match transport {
        McpTransport::Stdio => "stdio",
        McpTransport::Http => "HTTP",
        McpTransport::Sse => "SSE",
    }
}

fn uses_disabled_store(kind: ConfigKind) -> bool {
    matches!(kind, ConfigKind::Claude | ConfigKind::Cursor)
}

fn disabled_store_path() -> PathBuf {
    super::v2::fsutil::agentbro_home()
        .join("mcp")
        .join("disabled.json")
}

fn read_disabled_root() -> Result<Value, String> {
    read_json_or_empty(&disabled_store_path())
}

fn read_disabled_servers(agent_id: &str) -> Result<BTreeMap<String, RawServer>, String> {
    let root = read_disabled_root()?;
    let Some(config) = config_for_agent(agent_id) else {
        return Ok(BTreeMap::new());
    };
    Ok(root
        .get(agent_id)
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|servers| servers.iter())
        .filter_map(|(name, raw)| {
            normalize_json_server(config.kind, name, raw, false).map(|mut server| {
                server.enabled = false;
                (name.clone(), server)
            })
        })
        .collect())
}

fn write_disabled_servers(
    agent_id: &str,
    servers: &BTreeMap<String, RawServer>,
) -> Result<(), String> {
    let path = disabled_store_path();
    let mut root = read_disabled_root()?;
    let object = root
        .as_object_mut()
        .ok_or("AgentBro disabled MCP store must be an object")?;
    if servers.is_empty() {
        object.remove(agent_id);
    } else {
        object.insert(
            agent_id.to_string(),
            Value::Object(
                servers
                    .iter()
                    .map(|(name, server)| (name.clone(), server.raw.clone()))
                    .collect(),
            ),
        );
    }
    safe_write_json(&path, &root, "agentbro-disabled")?;
    Ok(())
}

fn set_sidecar_enabled(
    config: &AgentConfig,
    server_name: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut disabled = read_disabled_servers(&config.agent_id)?;
    let mut root = read_json_or_empty(&config.path)?;
    if enabled {
        let stored = disabled
            .remove(server_name)
            .ok_or_else(|| format!("Disabled MCP server not found: {server_name}"))?;
        let servers = json_servers_mut(&mut root, config.kind)?;
        if servers.contains_key(server_name) {
            return Err(format!(
                "Cannot restore {server_name}: an active MCP server now uses that name"
            ));
        }
        servers.insert(server_name.to_string(), stored.raw);
        safe_write_json(&config.path, &root, &config.agent_id)?;
        if let Err(error) = write_disabled_servers(&config.agent_id, &disabled) {
            let mut rollback_root = read_json_or_empty(&config.path)?;
            json_servers_mut(&mut rollback_root, config.kind)?.remove(server_name);
            let _ = safe_write_json(&config.path, &rollback_root, &config.agent_id);
            return Err(error);
        }
    } else {
        let raw = json_servers_mut(&mut root, config.kind)?
            .remove(server_name)
            .ok_or_else(|| format!("MCP server not found: {server_name}"))?;
        let stored = normalize_json_server(config.kind, server_name, &raw, false)
            .ok_or_else(|| format!("Invalid MCP server configuration: {server_name}"))?;
        disabled.insert(
            server_name.to_string(),
            RawServer {
                draft: stored.draft,
                enabled: false,
                raw: raw.clone(),
            },
        );
        write_disabled_servers(&config.agent_id, &disabled)?;
        if let Err(error) = safe_write_json(&config.path, &root, &config.agent_id) {
            disabled.remove(server_name);
            let _ = write_disabled_servers(&config.agent_id, &disabled);
            return Err(error);
        }
    }
    Ok(())
}

fn set_native_enabled(
    config: &AgentConfig,
    server_name: &str,
    enabled: bool,
) -> Result<(), String> {
    if config.kind == ConfigKind::Codex {
        let content = read_text_or_empty(&config.path)?;
        if !parse_codex_servers(&content).contains_key(server_name) {
            return Err(format!("MCP server not found: {server_name}"));
        }
        let next = set_codex_enabled(&content, server_name, enabled)?;
        return safe_write_toml(&config.path, &next, &config.agent_id);
    }
    let mut root = read_json_or_empty(&config.path)?;
    match config.kind {
        ConfigKind::Gemini => {
            let root_object = root
                .as_object_mut()
                .ok_or("Gemini settings root must be an object")?;
            let mcp = root_object
                .entry("mcp")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .ok_or("Gemini mcp setting must be an object")?;
            let excluded = mcp
                .entry("excluded")
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .ok_or("Gemini mcp.excluded must be an array")?;
            let name_value = Value::String(server_name.to_string());
            if enabled {
                excluded.retain(|value| value != &name_value);
            } else if !excluded.contains(&name_value) {
                excluded.push(name_value);
            }
        }
        ConfigKind::Kimi => {
            let server = json_servers_mut(&mut root, config.kind)?
                .get_mut(server_name)
                .and_then(Value::as_object_mut)
                .ok_or_else(|| format!("MCP server not found: {server_name}"))?;
            server.insert("enabled".to_string(), Value::Bool(enabled));
        }
        ConfigKind::Zcode => {
            let server = json_servers_mut(&mut root, config.kind)?
                .get_mut(server_name)
                .and_then(Value::as_object_mut)
                .ok_or_else(|| format!("MCP server not found: {server_name}"))?;
            server.insert("enable".to_string(), Value::Bool(enabled));
        }
        _ => return Err("This Agent does not support native MCP enablement".to_string()),
    }
    safe_write_json(&config.path, &root, &config.agent_id)
}

fn write_active_server(
    config: &AgentConfig,
    original_name: &str,
    draft: &McpServerDraft,
    previous_raw: Option<&Value>,
    enabled: bool,
) -> Result<(), String> {
    if config.kind == ConfigKind::Codex {
        let content = read_text_or_empty(&config.path)?;
        let next = upsert_codex_server(&content, original_name, draft, enabled)?;
        return safe_write_toml(&config.path, &next, &config.agent_id);
    }
    let mut root = read_json_or_empty(&config.path)?;
    let servers = json_servers_mut(&mut root, config.kind)?;
    if original_name != draft.name {
        servers.remove(original_name);
    }
    servers.insert(
        draft.name.clone(),
        raw_value_for(config, draft, enabled, previous_raw),
    );
    safe_write_json(&config.path, &root, &config.agent_id)
}

fn remove_active_server(config: &AgentConfig, server_name: &str) -> Result<(), String> {
    if config.kind == ConfigKind::Codex {
        let content = read_text_or_empty(&config.path)?;
        let (next, removed) = remove_codex_server_sections(&content, server_name);
        if !removed {
            return Err(format!("MCP server not found: {server_name}"));
        }
        return safe_write_toml(&config.path, &next, &config.agent_id);
    }
    let mut root = read_json_or_empty(&config.path)?;
    if json_servers_mut(&mut root, config.kind)?
        .remove(server_name)
        .is_none()
    {
        return Err(format!("MCP server not found: {server_name}"));
    }
    if config.kind == ConfigKind::Gemini {
        if let Some(excluded) = root
            .pointer_mut("/mcp/excluded")
            .and_then(Value::as_array_mut)
        {
            excluded.retain(|value| value.as_str() != Some(server_name));
        }
    }
    safe_write_json(&config.path, &root, &config.agent_id)
}

fn read_text_or_empty(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("Failed to read {}: {error}", path.display())),
    }
}

fn read_json_or_empty(path: &Path) -> Result<Value, String> {
    let content = read_text_or_empty(path)?;
    if content.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    serde_json::from_str(&content)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))
}

fn safe_write_json(path: &Path, root: &Value, backup_group: &str) -> Result<(), String> {
    let content = serde_json::to_string_pretty(root).map_err(|error| error.to_string())? + "\n";
    safe_write(path, content.as_bytes(), backup_group, |written| {
        serde_json::from_slice::<Value>(written)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
}

fn safe_write_toml(path: &Path, content: &str, backup_group: &str) -> Result<(), String> {
    safe_write(path, content.as_bytes(), backup_group, |written| {
        let content = std::str::from_utf8(written).map_err(|error| error.to_string())?;
        validate_toml_shape(content)
    })
}

fn safe_write(
    path: &Path,
    content: &[u8],
    backup_group: &str,
    validate: impl Fn(&[u8]) -> Result<(), String>,
) -> Result<(), String> {
    validate(content)?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("Invalid MCP config path: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
    let original = fs::read(path).ok();
    let backup = if let Some(bytes) = original.as_ref() {
        Some(write_backup(path, bytes, backup_group)?)
    } else {
        None
    };
    let temp = parent.join(format!(
        ".{}.agentbro-{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("mcp"),
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        write_private_file(&temp, content)?;
        fs::rename(&temp, path)
            .map_err(|error| format!("Failed to replace {}: {error}", path.display()))?;
        let written = fs::read(path)
            .map_err(|error| format!("Failed to verify {}: {error}", path.display()))?;
        validate(&written)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        if let Some(bytes) = original {
            let _ = write_private_file(path, &bytes);
        } else {
            let _ = fs::remove_file(path);
        }
        return Err(error);
    }
    let _ = backup;
    Ok(())
}

fn write_backup(path: &Path, content: &[u8], group: &str) -> Result<PathBuf, String> {
    let dir = super::v2::fsutil::agentbro_home()
        .join("mcp")
        .join("backups")
        .join(sanitize_component(group));
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Failed to create MCP backup directory: {error}"))?;
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("mcp-config");
    let backup = dir.join(format!("{timestamp}-{filename}"));
    write_private_file(&backup, content)?;
    Ok(backup)
}

fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn write_private_file(path: &Path, content: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))?;
    file.write_all(content)
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("Failed to sync {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("Failed to secure {}: {error}", path.display()))?;
    }
    Ok(())
}

fn revision_for(agent_id: &str, config_path: Option<&Path>) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(agent_id.as_bytes());
    if let Some(path) = config_path {
        match fs::read(path) {
            Ok(content) => hasher.update(content),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Failed to read {}: {error}", path.display())),
        }
    }
    let disabled = disabled_store_path();
    match fs::read(&disabled) {
        Ok(content) => hasher.update(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Failed to read {}: {error}", disabled.display())),
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn ensure_revision(
    agent_id: &str,
    config_path: Option<&Path>,
    expected: &str,
) -> Result<(), String> {
    let current = revision_for(agent_id, config_path)?;
    if current != expected {
        return Err(
            "MCP configuration changed outside AgentBro. Reload before saving.".to_string(),
        );
    }
    Ok(())
}

fn parse_codex_servers(content: &str) -> BTreeMap<String, RawServer> {
    let mut drafts = BTreeMap::<String, McpServerDraft>::new();
    let mut enabled = HashMap::<String, bool>::new();
    let mut current_path = Vec::<String>::new();
    for line in logical_toml_lines(content) {
        if let Some(path) = parse_toml_table_path(&line) {
            current_path = path;
            if current_path.first().map(String::as_str) == Some("mcp_servers")
                && current_path.len() >= 2
            {
                let name = current_path[1].clone();
                drafts
                    .entry(name.clone())
                    .or_insert_with(|| McpServerDraft {
                        name,
                        transport: McpTransport::Stdio,
                        command: None,
                        args: Vec::new(),
                        env: Vec::new(),
                        cwd: None,
                        url: None,
                        headers: Vec::new(),
                    });
            }
            continue;
        }
        if current_path.first().map(String::as_str) != Some("mcp_servers") || current_path.len() < 2
        {
            continue;
        }
        let name = &current_path[1];
        let Some((raw_key, raw_value)) = split_toml_assignment(&line) else {
            continue;
        };
        let key = unquote_toml_key(raw_key.trim());
        let value = raw_value.trim();
        let Some(draft) = drafts.get_mut(name) else {
            continue;
        };
        if current_path.len() == 2 {
            match key.as_str() {
                "command" => draft.command = parse_toml_string(value),
                "args" => draft.args = parse_toml_string_array(value),
                "url" => {
                    draft.url = parse_toml_string(value);
                    draft.transport = McpTransport::Http;
                }
                "cwd" => draft.cwd = parse_toml_string(value),
                "enabled" => {
                    if let Some(value) = parse_toml_bool(value) {
                        enabled.insert(name.clone(), value);
                    }
                }
                "http_headers" => {
                    draft.headers = parse_toml_inline_table(value);
                }
                _ => {}
            }
        } else if current_path.len() == 3 && current_path[2] == "env" {
            if let Some(value) = parse_toml_string(value) {
                draft.env.push(McpConfigValue {
                    secret: is_sensitive_key(&key),
                    key,
                    value: Some(value),
                    configured: true,
                });
            }
        } else if current_path.len() == 3 && current_path[2] == "http_headers" {
            if let Some(value) = parse_toml_string(value) {
                draft.headers.push(McpConfigValue {
                    secret: is_sensitive_key(&key),
                    key,
                    value: Some(value),
                    configured: true,
                });
            }
        }
    }
    drafts
        .into_iter()
        .map(|(name, draft)| {
            (
                name.clone(),
                RawServer {
                    draft,
                    enabled: enabled.get(&name).copied().unwrap_or(true),
                    raw: Value::Null,
                },
            )
        })
        .collect()
}

fn upsert_codex_server(
    content: &str,
    original_name: &str,
    draft: &McpServerDraft,
    enabled: bool,
) -> Result<String, String> {
    if draft.transport == McpTransport::Sse {
        return Err("Codex does not support legacy SSE MCP servers".to_string());
    }
    let preserved = collect_codex_preserved(content, original_name, &draft.name);
    let (mut base, _) = remove_codex_server_sections(content, original_name);
    if original_name != draft.name {
        let (without_collision, removed_collision) =
            remove_codex_server_sections(&base, &draft.name);
        if removed_collision {
            return Err(format!("An MCP server named {} already exists", draft.name));
        }
        base = without_collision;
    }
    if !base.is_empty() && !base.ends_with('\n') {
        base.push('\n');
    }
    if !base.trim().is_empty() {
        base.push('\n');
    }
    base.push_str(&render_codex_server(draft, enabled, &preserved));
    Ok(base)
}

fn set_codex_enabled(content: &str, server_name: &str, enabled: bool) -> Result<String, String> {
    let server = parse_codex_servers(content)
        .remove(server_name)
        .ok_or_else(|| format!("MCP server not found: {server_name}"))?;
    upsert_codex_server(content, server_name, &server.draft, enabled)
}

#[derive(Default)]
struct CodexPreserved {
    base_lines: Vec<String>,
    nested_sections: Vec<Vec<String>>,
}

fn collect_codex_preserved(content: &str, server_name: &str, next_name: &str) -> CodexPreserved {
    let lines = content.lines().collect::<Vec<_>>();
    let mut preserved = CodexPreserved::default();
    let mut index = 0usize;
    while index < lines.len() {
        let Some(path) = parse_toml_table_path(&strip_toml_comment(lines[index])) else {
            index += 1;
            continue;
        };
        let start = index;
        index += 1;
        while index < lines.len()
            && parse_toml_table_path(&strip_toml_comment(lines[index])).is_none()
        {
            index += 1;
        }
        if path.first().map(String::as_str) != Some("mcp_servers")
            || path.get(1).map(String::as_str) != Some(server_name)
        {
            continue;
        }
        if path.len() == 2 {
            let mut line_index = start + 1;
            while line_index < index {
                let cleaned = strip_toml_comment(lines[line_index]);
                if let Some((key, value)) = split_toml_assignment(&cleaned) {
                    let key = unquote_toml_key(key.trim());
                    let owned = matches!(
                        key.as_str(),
                        "command" | "args" | "url" | "cwd" | "enabled" | "http_headers"
                    );
                    if owned {
                        let mut depth = toml_bracket_delta(value);
                        line_index += 1;
                        while depth > 0 && line_index < index {
                            depth += toml_bracket_delta(&strip_toml_comment(lines[line_index]));
                            line_index += 1;
                        }
                        continue;
                    }
                }
                preserved.base_lines.push(lines[line_index].to_string());
                line_index += 1;
            }
        } else if !matches!(
            path.get(2).map(String::as_str),
            Some("env" | "http_headers")
        ) {
            let mut section = lines[start..index]
                .iter()
                .map(|line| (*line).to_string())
                .collect::<Vec<_>>();
            let mut renamed = path;
            renamed[1] = next_name.to_string();
            section[0] = render_toml_table_path(&renamed);
            preserved.nested_sections.push(section);
        }
    }
    preserved
}

fn render_codex_server(
    draft: &McpServerDraft,
    enabled: bool,
    preserved: &CodexPreserved,
) -> String {
    let name = quote_toml_key(&draft.name);
    let mut lines = vec![format!("[mcp_servers.{name}]")];
    match draft.transport {
        McpTransport::Stdio => {
            lines.push(format!(
                "command = {}",
                quote_toml_string(draft.command.as_deref().unwrap_or_default())
            ));
            lines.push(format!(
                "args = [{}]",
                draft
                    .args
                    .iter()
                    .map(|value| quote_toml_string(value))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if let Some(cwd) = nonempty(draft.cwd.as_deref()) {
                lines.push(format!("cwd = {}", quote_toml_string(cwd)));
            }
        }
        McpTransport::Http => {
            lines.push(format!(
                "url = {}",
                quote_toml_string(draft.url.as_deref().unwrap_or_default())
            ));
        }
        McpTransport::Sse => {}
    }
    lines.push(format!("enabled = {enabled}"));
    lines.extend(
        preserved
            .base_lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .cloned(),
    );
    if draft.transport == McpTransport::Stdio && !draft.env.is_empty() {
        lines.push(String::new());
        lines.push(format!("[mcp_servers.{name}.env]"));
        for item in &draft.env {
            if let Some(value) = item.value.as_ref() {
                lines.push(format!(
                    "{} = {}",
                    quote_toml_key(&item.key),
                    quote_toml_string(value)
                ));
            }
        }
    }
    if draft.transport == McpTransport::Http && !draft.headers.is_empty() {
        lines.push(String::new());
        lines.push(format!("[mcp_servers.{name}.http_headers]"));
        for item in &draft.headers {
            if let Some(value) = item.value.as_ref() {
                lines.push(format!(
                    "{} = {}",
                    quote_toml_key(&item.key),
                    quote_toml_string(value)
                ));
            }
        }
    }
    for section in &preserved.nested_sections {
        lines.push(String::new());
        lines.extend(section.iter().cloned());
    }
    lines.push(String::new());
    lines.join("\n")
}

fn remove_codex_server_sections(content: &str, server_name: &str) -> (String, bool) {
    let mut kept = Vec::<String>::new();
    let mut skip = false;
    let mut removed = false;
    for line in content.lines() {
        if let Some(path) = parse_toml_table_path(&strip_toml_comment(line)) {
            skip = path.first().map(String::as_str) == Some("mcp_servers")
                && path.get(1).map(String::as_str) == Some(server_name);
            removed |= skip;
        }
        if !skip {
            kept.push(line.to_string());
        }
    }
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    let mut next = kept.join("\n");
    if !next.is_empty() {
        next.push('\n');
    }
    (next, removed)
}

pub(crate) fn validate_toml_shape(content: &str) -> Result<(), String> {
    let mut brackets = 0isize;
    for line in content.lines() {
        let clean = strip_toml_comment(line);
        if clean.trim_start().starts_with('[')
            && toml_bracket_delta(&clean) == 0
            && parse_toml_table_path(&clean).is_none()
            && !clean.trim_start().starts_with("[[")
        {
            return Err(format!("Invalid TOML table header: {}", clean.trim()));
        }
        brackets += toml_bracket_delta(&clean);
        if brackets < 0 {
            return Err("Unbalanced TOML brackets".to_string());
        }
    }
    if brackets != 0 {
        return Err("Unbalanced TOML brackets".to_string());
    }
    Ok(())
}

fn logical_toml_lines(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0isize;
    for raw in content.lines() {
        let clean = strip_toml_comment(raw);
        let trimmed = clean.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(trimmed);
        depth += toml_bracket_delta(trimmed);
        if depth <= 0 {
            out.push(std::mem::take(&mut current));
            depth = 0;
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn parse_toml_table_path(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') || trimmed.starts_with("[[") {
        return None;
    }
    split_toml_path(&trimmed[1..trimmed.len().saturating_sub(1)])
}

fn split_toml_path(value: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for ch in value.chars() {
        if quoted {
            if escaped {
                current.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            } else {
                current.push(ch);
            }
        } else if ch == '"' {
            quoted = true;
        } else if ch == '.' {
            if current.trim().is_empty() {
                return None;
            }
            parts.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    if quoted || current.trim().is_empty() {
        return None;
    }
    parts.push(current.trim().to_string());
    Some(parts)
}

fn split_toml_assignment(line: &str) -> Option<(&str, &str)> {
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else if ch == '"' {
            quoted = true;
        } else if ch == '=' {
            return Some((&line[..index], &line[index + 1..]));
        }
    }
    None
}

fn strip_toml_comment(line: &str) -> String {
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else if ch == '"' {
            quoted = true;
        } else if ch == '#' {
            return line[..index].to_string();
        }
    }
    line.to_string()
}

fn toml_bracket_delta(line: &str) -> isize {
    let mut depth = 0;
    let mut quoted = false;
    let mut escaped = false;
    for ch in line.chars() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else if ch == '"' {
            quoted = true;
        } else if ch == '[' {
            depth += 1;
        } else if ch == ']' {
            depth -= 1;
        }
    }
    depth
}

fn parse_toml_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_toml_string(value: &str) -> Option<String> {
    let inner = value.trim().strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for ch in inner.chars() {
        if escaped {
            out.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

fn parse_toml_string_array(value: &str) -> Vec<String> {
    let Some(inner) = value
        .trim()
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    else {
        return Vec::new();
    };
    split_toml_comma_values(inner)
        .into_iter()
        .filter_map(parse_toml_string)
        .collect()
}

fn parse_toml_inline_table(value: &str) -> Vec<McpConfigValue> {
    let Some(inner) = value
        .trim()
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    else {
        return Vec::new();
    };
    split_toml_comma_values(inner)
        .into_iter()
        .filter_map(|item| {
            let (key, value) = split_toml_assignment(item)?;
            let key = unquote_toml_key(key.trim());
            Some(McpConfigValue {
                secret: is_sensitive_key(&key),
                key,
                value: parse_toml_string(value.trim()),
                configured: true,
            })
        })
        .collect()
}

fn split_toml_comma_values(value: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in value.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else if ch == '"' {
            quoted = true;
        } else if ch == ',' {
            out.push(value[start..index].trim());
            start = index + 1;
        }
    }
    out.push(value[start..].trim());
    out
}

fn unquote_toml_key(value: &str) -> String {
    parse_toml_string(value).unwrap_or_else(|| value.trim().to_string())
}

fn quote_toml_key(value: &str) -> String {
    if Regex::new(r"^[A-Za-z0-9_-]+$")
        .expect("static TOML key regex")
        .is_match(value)
    {
        value.to_string()
    } else {
        quote_toml_string(value)
    }
}

fn quote_toml_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
}

fn render_toml_table_path(path: &[String]) -> String {
    format!(
        "[{}]",
        path.iter()
            .map(|part| quote_toml_key(part))
            .collect::<Vec<_>>()
            .join(".")
    )
}

#[derive(Debug)]
struct HandshakeInfo {
    protocol_version: Option<String>,
    server_name: Option<String>,
    server_version: Option<String>,
    tool_count: Option<usize>,
}

#[derive(Debug)]
struct TestFailure {
    category: &'static str,
    message: String,
}

#[derive(Debug)]
struct InspectionDiscovery {
    items: Vec<Value>,
    truncated: bool,
}

enum InspectionSession {
    Stdio {
        stdin: tokio::process::ChildStdin,
        lines: tokio::io::Lines<BufReader<ChildStdout>>,
        child: Child,
        stderr_task: Option<tokio::task::JoinHandle<String>>,
        process_group: ProcessGroupCleanup,
        next_id: i64,
    },
    Streamable {
        client: Client,
        draft: McpServerDraft,
        url: String,
        session_id: Option<String>,
        next_id: i64,
    },
    LegacySse {
        client: Client,
        draft: McpServerDraft,
        endpoint: String,
        stream: reqwest::Response,
        buffer: String,
        next_id: i64,
    },
}

struct ProcessGroupCleanup {
    process_id: Option<u32>,
}

impl ProcessGroupCleanup {
    fn new(process_id: Option<u32>) -> Self {
        Self { process_id }
    }

    fn disarm(&mut self) {
        self.process_id = None;
    }
}

impl Drop for ProcessGroupCleanup {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(process_id) = self.process_id {
            unsafe {
                libc::kill(-(process_id as i32), libc::SIGKILL);
            }
        }
    }
}

static MCP_INSPECTION_CANCELLATIONS: OnceLock<Mutex<HashMap<String, Arc<Notify>>>> =
    OnceLock::new();

fn inspection_cancellations() -> &'static Mutex<HashMap<String, Arc<Notify>>> {
    MCP_INSPECTION_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn cancel_mcp_inspection(inspection_id: &str) -> Result<(), String> {
    let signal = inspection_cancellations()
        .lock()
        .map_err(|_| "MCP inspection cancellation state is unavailable".to_string())?
        .get(inspection_id)
        .cloned();
    if let Some(signal) = signal {
        signal.notify_one();
    }
    Ok(())
}

pub fn cancel_mcp_operation(operation_id: &str) -> Result<(), String> {
    cancel_mcp_inspection(operation_id)
}

pub async fn call_mcp_tool(
    agent_id: &str,
    server_name: &str,
    operation_id: &str,
    tool_name: &str,
    arguments: Value,
) -> Result<McpOperationResult, String> {
    if tool_name.trim().is_empty() {
        return Err("MCP tool name is required".to_string());
    }
    if !arguments.is_object() {
        return Err("MCP tool arguments must be a JSON object".to_string());
    }
    run_mcp_operation(
        agent_id,
        server_name,
        operation_id,
        "tool",
        tool_name,
        "tools/call",
        serde_json::json!({
            "name": tool_name,
            "arguments": arguments,
        }),
    )
    .await
}

pub async fn get_mcp_prompt(
    agent_id: &str,
    server_name: &str,
    operation_id: &str,
    prompt_name: &str,
    arguments: Value,
) -> Result<McpOperationResult, String> {
    if prompt_name.trim().is_empty() {
        return Err("MCP prompt name is required".to_string());
    }
    if !arguments.is_object() {
        return Err("MCP prompt arguments must be a JSON object".to_string());
    }
    run_mcp_operation(
        agent_id,
        server_name,
        operation_id,
        "prompt",
        prompt_name,
        "prompts/get",
        serde_json::json!({
            "name": prompt_name,
            "arguments": arguments,
        }),
    )
    .await
}

async fn run_mcp_operation(
    agent_id: &str,
    server_name: &str,
    operation_id: &str,
    kind: &str,
    name: &str,
    method: &str,
    params: Value,
) -> Result<McpOperationResult, String> {
    let config = config_for_agent(agent_id)
        .ok_or_else(|| format!("Agent {agent_id} does not support MCP operations"))?;
    let active = read_active_servers(&config)?;
    let disabled = read_disabled_servers(agent_id)?;
    let server = active
        .get(server_name)
        .or_else(|| disabled.get(server_name))
        .ok_or_else(|| format!("MCP server not found: {server_name}"))?;
    let validation =
        validate_mcp_server_draft_inner(&config, &server.draft, Some(server_name), &HashSet::new());
    if !validation.valid {
        return Err(redact_message(&validation.message, &server.draft));
    }

    let signal = Arc::new(Notify::new());
    {
        let mut cancellations = inspection_cancellations()
            .lock()
            .map_err(|_| "MCP operation cancellation state is unavailable".to_string())?;
        if let Some(previous) = cancellations.insert(operation_id.to_string(), signal.clone()) {
            previous.notify_one();
        }
    }

    let started = Instant::now();
    let result = tokio::select! {
        _ = signal.notified() => Err("MCP operation cancelled".to_string()),
        result = tokio::time::timeout(
            MCP_OPERATION_TIMEOUT,
            perform_mcp_operation(
                operation_id,
                kind,
                name,
                method,
                params,
                &server.draft,
                validation.warnings,
            ),
        ) => match result {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(error)) => Err(redact_message(&error.message, &server.draft)),
            Err(_) => Err(format!(
                "MCP operation timed out after {} seconds",
                MCP_OPERATION_TIMEOUT.as_secs()
            )),
        },
    };

    if let Ok(mut cancellations) = inspection_cancellations().lock() {
        cancellations.remove(operation_id);
    }
    result.map(|mut result| {
        result.duration_ms = elapsed_ms(started);
        result
    })
}

async fn perform_mcp_operation(
    operation_id: &str,
    kind: &str,
    name: &str,
    method: &str,
    params: Value,
    draft: &McpServerDraft,
    mut warnings: Vec<String>,
) -> Result<McpOperationResult, TestFailure> {
    let started = Instant::now();
    let mut session = open_inspection_session(draft).await?;
    let initialize = match initialize_inspection_session(&mut session).await {
        Ok(value) => value,
        Err(error) if error.category == "legacy_sse_required" => {
            session.shutdown().await?;
            session = open_legacy_inspection_session(draft).await?;
            warnings.push("Streamable HTTP was unavailable; operation used legacy SSE".to_string());
            initialize_inspection_session(&mut session).await?
        }
        Err(error) => return Err(error),
    };
    handshake_info(&initialize)?;
    session.notify_initialized().await?;

    let response = session.request(method, params).await;
    if let Err(error) = session.shutdown().await {
        warnings.push(redact_message(&error.message, draft));
    }
    let response = response?;
    let result = response.get("result").cloned().ok_or_else(|| TestFailure {
        category: "protocol_error",
        message: format!("{method} response is missing a result"),
    })?;
    let result_size = serde_json::to_vec(&result)
        .map_err(|error| TestFailure {
            category: "protocol_error",
            message: format!("Failed to encode MCP operation result: {error}"),
        })?
        .len();
    if result_size > MCP_OPERATION_MAX_RESULT_BYTES {
        return Err(TestFailure {
            category: "response_too_large",
            message: format!(
                "MCP operation result exceeded the {} MB limit",
                MCP_OPERATION_MAX_RESULT_BYTES / 1024 / 1024
            ),
        });
    }
    let category = if kind == "tool"
        && result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        "tool_error"
    } else {
        "success"
    };

    Ok(McpOperationResult {
        operation_id: operation_id.to_string(),
        kind: kind.to_string(),
        name: name.to_string(),
        category: category.to_string(),
        duration_ms: elapsed_ms(started),
        result,
        warnings,
    })
}

pub async fn inspect_mcp_server(
    agent_id: &str,
    server_name: &str,
    inspection_id: &str,
) -> Result<McpInspectionReport, String> {
    let config = config_for_agent(agent_id)
        .ok_or_else(|| format!("Agent {agent_id} does not support MCP inspection"))?;
    let active = read_active_servers(&config)?;
    let disabled = read_disabled_servers(agent_id)?;
    let server = active
        .get(server_name)
        .or_else(|| disabled.get(server_name))
        .ok_or_else(|| format!("MCP server not found: {server_name}"))?;
    let validation =
        validate_mcp_server_draft_inner(&config, &server.draft, Some(server_name), &HashSet::new());
    if !validation.valid {
        return Ok(failed_inspection_report(
            inspection_id,
            &server.draft,
            "invalid_config",
            validation.message,
            validation.warnings,
            0,
        ));
    }

    let signal = Arc::new(Notify::new());
    {
        let mut cancellations = inspection_cancellations()
            .lock()
            .map_err(|_| "MCP inspection cancellation state is unavailable".to_string())?;
        if let Some(previous) = cancellations.insert(inspection_id.to_string(), signal.clone()) {
            previous.notify_one();
        }
    }

    let started = Instant::now();
    let result = tokio::select! {
        _ = signal.notified() => cancelled_inspection_report(
            inspection_id,
            &server.draft,
            elapsed_ms(started),
        ),
        result = tokio::time::timeout(
            MCP_INSPECTION_TIMEOUT,
            perform_mcp_inspection(
                inspection_id,
                &server.draft,
                validation.warnings,
            ),
        ) => match result {
            Ok(report) => report,
            Err(_) => failed_inspection_report(
                inspection_id,
                &server.draft,
                "timeout",
                "MCP inspection timed out after 20 seconds".to_string(),
                Vec::new(),
                elapsed_ms(started),
            ),
        },
    };

    if let Ok(mut cancellations) = inspection_cancellations().lock() {
        cancellations.remove(inspection_id);
    }
    Ok(result)
}

async fn perform_mcp_inspection(
    inspection_id: &str,
    draft: &McpServerDraft,
    mut warnings: Vec<String>,
) -> McpInspectionReport {
    let started = Instant::now();
    let mut report = inspection_report_base(inspection_id, draft);
    report.warnings.append(&mut warnings);

    let phase_started = Instant::now();
    let mut session =
        match tokio::time::timeout(MCP_INSPECTION_STEP_TIMEOUT, open_inspection_session(draft))
            .await
        {
            Ok(Ok(session)) => {
                report.steps.push(inspection_step(
                    "connect",
                    "success",
                    phase_started,
                    match draft.transport {
                        McpTransport::Stdio => "MCP process started",
                        McpTransport::Http => "HTTP inspection session prepared",
                        McpTransport::Sse => "SSE connection established",
                    },
                ));
                session
            }
            Ok(Err(error)) => {
                return complete_failed_inspection(
                    report,
                    draft,
                    "connect",
                    error,
                    phase_started,
                    started,
                );
            }
            Err(_) => {
                return complete_failed_inspection(
                    report,
                    draft,
                    "connect",
                    step_timeout_failure("Connection"),
                    phase_started,
                    started,
                );
            }
        };

    let phase_started = Instant::now();
    let initialize = match tokio::time::timeout(
        MCP_INSPECTION_STEP_TIMEOUT,
        initialize_inspection_session(&mut session),
    )
    .await
    {
        Ok(Ok(value)) => value,
        Ok(Err(error)) if error.category == "legacy_sse_required" => {
            if let Err(shutdown_error) = session.shutdown().await {
                report
                    .warnings
                    .push(redact_message(&shutdown_error.message, draft));
            }
            let fallback_started = Instant::now();
            session = match tokio::time::timeout(
                MCP_INSPECTION_STEP_TIMEOUT,
                open_legacy_inspection_session(draft),
            )
            .await
            {
                Ok(Ok(session)) => session,
                Ok(Err(error)) => {
                    return complete_failed_inspection(
                        report,
                        draft,
                        "initialize",
                        error,
                        fallback_started,
                        started,
                    );
                }
                Err(_) => {
                    return complete_failed_inspection(
                        report,
                        draft,
                        "initialize",
                        step_timeout_failure("Legacy SSE connection"),
                        fallback_started,
                        started,
                    );
                }
            };
            report
                .warnings
                .push("Streamable HTTP was unavailable; inspection used legacy SSE".to_string());
            match tokio::time::timeout(
                MCP_INSPECTION_STEP_TIMEOUT,
                initialize_inspection_session(&mut session),
            )
            .await
            {
                Ok(Ok(value)) => value,
                Ok(Err(error)) => {
                    return complete_failed_inspection(
                        report,
                        draft,
                        "initialize",
                        error,
                        phase_started,
                        started,
                    );
                }
                Err(_) => {
                    return complete_failed_inspection(
                        report,
                        draft,
                        "initialize",
                        step_timeout_failure("Initialization"),
                        phase_started,
                        started,
                    );
                }
            }
        }
        Ok(Err(error)) => {
            return complete_failed_inspection(
                report,
                draft,
                "initialize",
                error,
                phase_started,
                started,
            );
        }
        Err(_) => {
            return complete_failed_inspection(
                report,
                draft,
                "initialize",
                step_timeout_failure("Initialization"),
                phase_started,
                started,
            );
        }
    };

    let info = match handshake_info(&initialize) {
        Ok(info) => info,
        Err(error) => {
            return complete_failed_inspection(
                report,
                draft,
                "initialize",
                error,
                phase_started,
                started,
            );
        }
    };
    let result = initialize.get("result").unwrap_or(&Value::Null);
    report.protocol_version = info.protocol_version;
    report.server_name = info.server_name;
    report.server_version = info.server_version;
    report.capabilities = McpInspectionCapabilities {
        tools: result.pointer("/capabilities/tools").is_some(),
        resources: result.pointer("/capabilities/resources").is_some(),
        prompts: result.pointer("/capabilities/prompts").is_some(),
        logging: result.pointer("/capabilities/logging").is_some(),
    };
    report.steps.push(inspection_step(
        "initialize",
        "success",
        phase_started,
        "Protocol negotiation completed",
    ));

    let phase_started = Instant::now();
    match tokio::time::timeout(MCP_INSPECTION_STEP_TIMEOUT, session.notify_initialized()).await {
        Ok(Ok(())) => report.steps.push(inspection_step(
            "initialized",
            "success",
            phase_started,
            "Initialized notification sent",
        )),
        Ok(Err(error)) => {
            return complete_failed_inspection(
                report,
                draft,
                "initialized",
                error,
                phase_started,
                started,
            );
        }
        Err(_) => {
            return complete_failed_inspection(
                report,
                draft,
                "initialized",
                step_timeout_failure("Initialized notification"),
                phase_started,
                started,
            );
        }
    }

    let mut partial = false;
    if report.capabilities.tools {
        let phase_started = Instant::now();
        match tokio::time::timeout(
            MCP_INSPECTION_STEP_TIMEOUT,
            discover_inspection_items(&mut session, "tools/list", "tools"),
        )
        .await
        {
            Ok(Ok(discovery)) => {
                report.tools = discovery
                    .items
                    .iter()
                    .filter_map(normalize_inspection_tool)
                    .collect();
                if discovery.truncated {
                    partial = true;
                    report.warnings.push(format!(
                        "Tools were limited to {MCP_INSPECTION_MAX_ITEMS} items"
                    ));
                }
                let missing_annotations = report
                    .tools
                    .iter()
                    .filter(|tool| !tool.has_annotations)
                    .count();
                if missing_annotations > 0 {
                    report.warnings.push(format!(
                        "{missing_annotations} tools do not declare risk annotations"
                    ));
                }
                report.steps.push(inspection_step(
                    "tools",
                    "success",
                    phase_started,
                    format!("Discovered {} tools", report.tools.len()),
                ));
            }
            Ok(Err(error)) => {
                partial = true;
                push_discovery_failure(&mut report, draft, "tools", error, phase_started);
            }
            Err(_) => {
                partial = true;
                push_discovery_failure(
                    &mut report,
                    draft,
                    "tools",
                    step_timeout_failure("Tools discovery"),
                    phase_started,
                );
            }
        }
    } else {
        report.steps.push(skipped_inspection_step(
            "tools",
            "Server does not declare Tools",
        ));
    }

    if report.capabilities.resources {
        let phase_started = Instant::now();
        match tokio::time::timeout(
            MCP_INSPECTION_STEP_TIMEOUT,
            discover_inspection_items(&mut session, "resources/list", "resources"),
        )
        .await
        {
            Ok(Ok(discovery)) => {
                report.resources = discovery
                    .items
                    .iter()
                    .filter_map(normalize_inspection_resource)
                    .collect();
                if discovery.truncated {
                    partial = true;
                    report.warnings.push(format!(
                        "Resources were limited to {MCP_INSPECTION_MAX_ITEMS} items"
                    ));
                }
                report.steps.push(inspection_step(
                    "resources",
                    "success",
                    phase_started,
                    format!("Discovered {} resources", report.resources.len()),
                ));
            }
            Ok(Err(error)) => {
                partial = true;
                push_discovery_failure(&mut report, draft, "resources", error, phase_started);
            }
            Err(_) => {
                partial = true;
                push_discovery_failure(
                    &mut report,
                    draft,
                    "resources",
                    step_timeout_failure("Resources discovery"),
                    phase_started,
                );
            }
        }
    } else {
        report.steps.push(skipped_inspection_step(
            "resources",
            "Server does not declare Resources",
        ));
    }

    if report.capabilities.prompts {
        let phase_started = Instant::now();
        match tokio::time::timeout(
            MCP_INSPECTION_STEP_TIMEOUT,
            discover_inspection_items(&mut session, "prompts/list", "prompts"),
        )
        .await
        {
            Ok(Ok(discovery)) => {
                report.prompts = discovery
                    .items
                    .iter()
                    .filter_map(normalize_inspection_prompt)
                    .collect();
                if discovery.truncated {
                    partial = true;
                    report.warnings.push(format!(
                        "Prompts were limited to {MCP_INSPECTION_MAX_ITEMS} items"
                    ));
                }
                report.steps.push(inspection_step(
                    "prompts",
                    "success",
                    phase_started,
                    format!("Discovered {} prompts", report.prompts.len()),
                ));
            }
            Ok(Err(error)) => {
                partial = true;
                push_discovery_failure(&mut report, draft, "prompts", error, phase_started);
            }
            Err(_) => {
                partial = true;
                push_discovery_failure(
                    &mut report,
                    draft,
                    "prompts",
                    step_timeout_failure("Prompts discovery"),
                    phase_started,
                );
            }
        }
    } else {
        report.steps.push(skipped_inspection_step(
            "prompts",
            "Server does not declare Prompts",
        ));
    }

    let phase_started = Instant::now();
    match tokio::time::timeout(MCP_INSPECTION_STEP_TIMEOUT, session.shutdown()).await {
        Ok(Ok(())) => report.steps.push(inspection_step(
            "shutdown",
            "success",
            phase_started,
            "Inspection session closed",
        )),
        Ok(Err(error)) => {
            partial = true;
            push_discovery_failure(&mut report, draft, "shutdown", error, phase_started);
        }
        Err(_) => {
            partial = true;
            push_discovery_failure(
                &mut report,
                draft,
                "shutdown",
                step_timeout_failure("Shutdown"),
                phase_started,
            );
        }
    }

    report.duration_ms = elapsed_ms(started);
    report.status = if partial {
        "partial".to_string()
    } else {
        "connected".to_string()
    };
    report.category = report.status.clone();
    report.summary = if partial {
        format!(
            "Connected with incomplete discovery · {} tools · {} resources · {} prompts",
            report.tools.len(),
            report.resources.len(),
            report.prompts.len(),
        )
    } else {
        format!(
            "Connected · {} tools · {} resources · {} prompts",
            report.tools.len(),
            report.resources.len(),
            report.prompts.len(),
        )
    };
    report
}

async fn open_inspection_session(draft: &McpServerDraft) -> Result<InspectionSession, TestFailure> {
    match draft.transport {
        McpTransport::Stdio => open_stdio_inspection_session(draft).await,
        McpTransport::Http => Ok(InspectionSession::Streamable {
            client: mcp_http_client()?,
            draft: draft.clone(),
            url: draft.url.clone().unwrap_or_default(),
            session_id: None,
            next_id: 1,
        }),
        McpTransport::Sse => open_legacy_inspection_session(draft).await,
    }
}

async fn open_stdio_inspection_session(
    draft: &McpServerDraft,
) -> Result<InspectionSession, TestFailure> {
    let command = draft.command.as_deref().unwrap_or_default();
    if !command_available(command) {
        return Err(TestFailure {
            category: "command_not_found",
            message: format!("Command not found: {command}"),
        });
    }
    let command_path = if command.starts_with('~') {
        super::v2::fsutil::expand_tilde(command)
    } else {
        crate::agents::executable::command_path(command)
    };
    let mut process = Command::new(command_path);
    process
        .args(&draft.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(path) = crate::agents::executable::augmented_path_env() {
        process.env("PATH", path);
    }
    if let Some(cwd) = nonempty(draft.cwd.as_deref()) {
        process.current_dir(super::v2::fsutil::expand_tilde(cwd));
    }
    for item in &draft.env {
        if let Some(value) = item.value.as_ref() {
            process.env(&item.key, value);
        }
    }
    #[cfg(unix)]
    process.process_group(0);
    let mut child = process.spawn().map_err(|error| TestFailure {
        category: "startup_failed",
        message: format!("Failed to start {command}: {error}"),
    })?;
    let process_group = ProcessGroupCleanup::new(child.id());
    let stdin = child.stdin.take().ok_or_else(|| TestFailure {
        category: "startup_failed",
        message: "MCP process stdin is unavailable".to_string(),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| TestFailure {
        category: "startup_failed",
        message: "MCP process stdout is unavailable".to_string(),
    })?;
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(async move {
            let mut output = Vec::new();
            let _ = stderr.take(8192).read_to_end(&mut output).await;
            String::from_utf8_lossy(&output).to_string()
        })
    });
    Ok(InspectionSession::Stdio {
        stdin,
        lines: BufReader::new(stdout).lines(),
        child,
        stderr_task,
        process_group,
        next_id: 1,
    })
}

async fn open_legacy_inspection_session(
    draft: &McpServerDraft,
) -> Result<InspectionSession, TestFailure> {
    let client = mcp_http_client()?;
    let url = draft.url.as_deref().unwrap_or_default();
    let mut request = client.get(url).header("Accept", "text/event-stream");
    request = apply_http_headers(request, draft)?;
    let mut stream = request.send().await.map_err(network_failure)?;
    if matches!(
        stream.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return Err(TestFailure {
            category: "authentication_required",
            message: format!("Authentication required ({})", stream.status()),
        });
    }
    if !stream.status().is_success() {
        return Err(TestFailure {
            category: "network_error",
            message: format!("SSE endpoint returned HTTP {}", stream.status()),
        });
    }
    let mut buffer = String::new();
    let (event, endpoint) = next_sse_event(&mut stream, &mut buffer).await?;
    if event.as_deref() != Some("endpoint") {
        return Err(TestFailure {
            category: "protocol_incompatible",
            message: "Legacy SSE server did not provide an endpoint event".to_string(),
        });
    }
    let endpoint = Url::parse(url)
        .and_then(|base| base.join(endpoint.trim()))
        .map_err(|error| TestFailure {
            category: "invalid_config",
            message: format!("Invalid SSE message endpoint: {error}"),
        })?;
    Ok(InspectionSession::LegacySse {
        client,
        draft: draft.clone(),
        endpoint: endpoint.to_string(),
        stream,
        buffer,
        next_id: 1,
    })
}

async fn initialize_inspection_session(
    session: &mut InspectionSession,
) -> Result<Value, TestFailure> {
    let mut last_error = None;
    for protocol_version in std::iter::once(MCP_PROTOCOL_VERSION).chain(MCP_PROTOCOL_FALLBACKS) {
        match session.initialize(protocol_version).await {
            Ok(response) => return Ok(response),
            Err(error) if error.category == "protocol_incompatible" => {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or(TestFailure {
        category: "protocol_incompatible",
        message: "MCP server rejected all supported protocol versions".to_string(),
    }))
}

impl InspectionSession {
    async fn initialize(&mut self, protocol_version: &str) -> Result<Value, TestFailure> {
        let id = self.next_request_id();
        self.rpc(&initialize_request(id, protocol_version), Some(id))
            .await
    }

    async fn notify_initialized(&mut self) -> Result<(), TestFailure> {
        self.rpc(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }),
            None,
        )
        .await
        .map(|_| ())
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value, TestFailure> {
        let id = self.next_request_id();
        self.rpc(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params
            }),
            Some(id),
        )
        .await
    }

    fn next_request_id(&mut self) -> i64 {
        let next_id = match self {
            InspectionSession::Stdio { next_id, .. }
            | InspectionSession::Streamable { next_id, .. }
            | InspectionSession::LegacySse { next_id, .. } => next_id,
        };
        let id = *next_id;
        *next_id += 1;
        id
    }

    async fn rpc(&mut self, body: &Value, id: Option<i64>) -> Result<Value, TestFailure> {
        match self {
            InspectionSession::Stdio { stdin, lines, .. } => {
                write_stdio_message(stdin, body).await?;
                if let Some(id) = id {
                    read_stdio_response(lines, id).await
                } else {
                    Ok(Value::Null)
                }
            }
            InspectionSession::Streamable {
                client,
                draft,
                url,
                session_id,
                ..
            } => {
                let (response, next_session) = send_streamable_rpc(
                    client,
                    draft,
                    url,
                    body,
                    session_id.as_deref(),
                    id.is_none(),
                )
                .await?;
                *session_id = next_session;
                Ok(response)
            }
            InspectionSession::LegacySse {
                client,
                draft,
                endpoint,
                stream,
                buffer,
                ..
            } => {
                post_legacy_sse(client, draft, endpoint, body).await?;
                if let Some(id) = id {
                    next_sse_rpc_response(stream, buffer, id).await
                } else {
                    Ok(Value::Null)
                }
            }
        }
    }

    async fn shutdown(self) -> Result<(), TestFailure> {
        match self {
            InspectionSession::Stdio {
                stdin,
                mut child,
                stderr_task,
                mut process_group,
                ..
            } => {
                drop(stdin);
                terminate_child(&mut child).await;
                process_group.disarm();
                if let Some(task) = stderr_task {
                    let _ = tokio::time::timeout(Duration::from_millis(250), task).await;
                }
                Ok(())
            }
            InspectionSession::Streamable {
                client,
                draft,
                url,
                session_id,
                ..
            } => {
                if let Some(session_id) = session_id {
                    let mut request = client.delete(url).header("Mcp-Session-Id", session_id);
                    request = apply_http_headers(request, &draft)?;
                    let _ = request.send().await;
                }
                Ok(())
            }
            InspectionSession::LegacySse { .. } => Ok(()),
        }
    }
}

async fn discover_inspection_items(
    session: &mut InspectionSession,
    method: &str,
    result_key: &str,
) -> Result<InspectionDiscovery, TestFailure> {
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    for page in 0..MCP_INSPECTION_MAX_PAGES {
        let params = cursor
            .as_ref()
            .map(|cursor| serde_json::json!({ "cursor": cursor }))
            .unwrap_or_else(|| serde_json::json!({}));
        let response = session.request(method, params).await?;
        let page_items = response
            .pointer(&format!("/result/{result_key}"))
            .and_then(Value::as_array)
            .ok_or_else(|| TestFailure {
                category: "protocol_error",
                message: format!("{method} response is missing the {result_key} array"),
            })?;
        for item in page_items {
            if items.len() == MCP_INSPECTION_MAX_ITEMS {
                return Ok(InspectionDiscovery {
                    items,
                    truncated: true,
                });
            }
            items.push(item.clone());
        }
        cursor = response
            .pointer("/result/nextCursor")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if cursor.is_none() {
            return Ok(InspectionDiscovery {
                items,
                truncated: false,
            });
        }
        if page + 1 == MCP_INSPECTION_MAX_PAGES {
            return Ok(InspectionDiscovery {
                items,
                truncated: true,
            });
        }
    }
    Ok(InspectionDiscovery {
        items,
        truncated: false,
    })
}

fn normalize_inspection_tool(value: &Value) -> Option<McpInspectionTool> {
    let name = value.get("name")?.as_str()?.to_string();
    let required = value
        .pointer("/inputSchema/required")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    let inputs = value
        .pointer("/inputSchema/properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .iter()
                .take(100)
                .map(|(property_name, property)| McpInspectionInput {
                    name: property_name.clone(),
                    value_type: schema_type_label(property),
                    description: property
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    required: required.contains(property_name.as_str()),
                })
                .collect()
        })
        .unwrap_or_default();
    let annotations = value.get("annotations").and_then(Value::as_object);
    Some(McpInspectionTool {
        name,
        title: value
            .get("title")
            .or_else(|| value.pointer("/annotations/title"))
            .and_then(Value::as_str)
            .map(str::to_string),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        inputs,
        input_schema: value
            .get("inputSchema")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({ "type": "object" })),
        output_schema: value.get("outputSchema").cloned(),
        annotations: McpInspectionToolAnnotations {
            read_only: annotations
                .and_then(|value| value.get("readOnlyHint"))
                .and_then(Value::as_bool),
            destructive: annotations
                .and_then(|value| value.get("destructiveHint"))
                .and_then(Value::as_bool),
            idempotent: annotations
                .and_then(|value| value.get("idempotentHint"))
                .and_then(Value::as_bool),
            open_world: annotations
                .and_then(|value| value.get("openWorldHint"))
                .and_then(Value::as_bool),
        },
        has_annotations: annotations.is_some(),
    })
}

fn normalize_inspection_resource(value: &Value) -> Option<McpInspectionResource> {
    Some(McpInspectionResource {
        uri: value.get("uri")?.as_str()?.to_string(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Unnamed resource")
            .to_string(),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        mime_type: value
            .get("mimeType")
            .and_then(Value::as_str)
            .map(str::to_string),
        size: value.get("size").and_then(Value::as_u64),
    })
}

fn normalize_inspection_prompt(value: &Value) -> Option<McpInspectionPrompt> {
    Some(McpInspectionPrompt {
        name: value.get("name")?.as_str()?.to_string(),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        arguments: value
            .get("arguments")
            .and_then(Value::as_array)
            .map(|arguments| {
                arguments
                    .iter()
                    .filter_map(|argument| {
                        Some(McpInspectionPromptArgument {
                            name: argument.get("name")?.as_str()?.to_string(),
                            description: argument
                                .get("description")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            required: argument
                                .get("required")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn schema_type_label(value: &Value) -> String {
    match value.get("type") {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" | "),
        _ if value.get("enum").is_some() => "enum".to_string(),
        _ => "unknown".to_string(),
    }
}

fn inspection_report_base(inspection_id: &str, draft: &McpServerDraft) -> McpInspectionReport {
    McpInspectionReport {
        inspection_id: inspection_id.to_string(),
        status: "inspecting".to_string(),
        category: "inspecting".to_string(),
        summary: "Inspecting MCP server".to_string(),
        inspected_at_ms: unix_time_ms(),
        duration_ms: 0,
        protocol_version: None,
        server_name: None,
        server_version: None,
        transport: draft.transport,
        capabilities: McpInspectionCapabilities {
            tools: false,
            resources: false,
            prompts: false,
            logging: false,
        },
        tools: Vec::new(),
        resources: Vec::new(),
        prompts: Vec::new(),
        steps: Vec::new(),
        warnings: Vec::new(),
        suggestions: Vec::new(),
    }
}

fn failed_inspection_report(
    inspection_id: &str,
    draft: &McpServerDraft,
    category: &str,
    message: String,
    warnings: Vec<String>,
    duration_ms: u64,
) -> McpInspectionReport {
    let mut report = inspection_report_base(inspection_id, draft);
    report.status = "failed".to_string();
    report.category = category.to_string();
    report.summary = redact_message(&message, draft);
    report.duration_ms = duration_ms;
    report.warnings = warnings;
    report.suggestions = suggestions_for_failure(category);
    report.steps.push(McpInspectionStep {
        phase: "inspection".to_string(),
        status: "failed".to_string(),
        duration_ms,
        message: report.summary.clone(),
    });
    report
}

fn cancelled_inspection_report(
    inspection_id: &str,
    draft: &McpServerDraft,
    duration_ms: u64,
) -> McpInspectionReport {
    let mut report = inspection_report_base(inspection_id, draft);
    report.status = "cancelled".to_string();
    report.category = "cancelled".to_string();
    report.summary = "Inspection cancelled".to_string();
    report.duration_ms = duration_ms;
    report.steps.push(McpInspectionStep {
        phase: "inspection".to_string(),
        status: "cancelled".to_string(),
        duration_ms,
        message: report.summary.clone(),
    });
    report
}

fn complete_failed_inspection(
    mut report: McpInspectionReport,
    draft: &McpServerDraft,
    phase: &str,
    error: TestFailure,
    phase_started: Instant,
    inspection_started: Instant,
) -> McpInspectionReport {
    report.status = "failed".to_string();
    report.category = error.category.to_string();
    report.summary = redact_message(&error.message, draft);
    report.duration_ms = elapsed_ms(inspection_started);
    report.suggestions = suggestions_for_failure(error.category);
    report.steps.push(inspection_step(
        phase,
        "failed",
        phase_started,
        report.summary.clone(),
    ));
    report
}

fn push_discovery_failure(
    report: &mut McpInspectionReport,
    draft: &McpServerDraft,
    phase: &str,
    error: TestFailure,
    phase_started: Instant,
) {
    let message = redact_message(&error.message, draft);
    report.warnings.push(message.clone());
    for suggestion in suggestions_for_failure(error.category) {
        if !report.suggestions.contains(&suggestion) {
            report.suggestions.push(suggestion);
        }
    }
    report
        .steps
        .push(inspection_step(phase, "failed", phase_started, message));
}

fn inspection_step(
    phase: &str,
    status: &str,
    started: Instant,
    message: impl Into<String>,
) -> McpInspectionStep {
    McpInspectionStep {
        phase: phase.to_string(),
        status: status.to_string(),
        duration_ms: elapsed_ms(started),
        message: message.into(),
    }
}

fn skipped_inspection_step(phase: &str, message: &str) -> McpInspectionStep {
    McpInspectionStep {
        phase: phase.to_string(),
        status: "skipped".to_string(),
        duration_ms: 0,
        message: message.to_string(),
    }
}

fn step_timeout_failure(label: &str) -> TestFailure {
    TestFailure {
        category: "timeout",
        message: format!("{label} timed out after 5 seconds"),
    }
}

fn suggestions_for_failure(category: &str) -> Vec<String> {
    let suggestions = match category {
        "command_not_found" => {
            &["Verify the command path and install the required package runtime"][..]
        }
        "startup_failed" => &[
            "Check command arguments, working directory, executable permissions, and server logs",
        ],
        "timeout" => &[
            "Verify that the server does not wait for interactive input and writes MCP messages to stdout",
        ],
        "authentication_required" => &[
            "Authenticate through the target Agent or configure the required authorization header",
        ],
        "network_error" => &["Check the URL, proxy, DNS, TLS, and local firewall"],
        "protocol_incompatible" | "protocol_error" => {
            &["Update the MCP server or the target Agent and inspect again"]
        }
        "invalid_config" => &["Edit the MCP configuration and correct the reported fields"],
        _ => &["Inspect the MCP server's own logs and try again"],
    };
    suggestions
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

pub async fn test_mcp_server_connection(
    agent_id: &str,
    server_name: &str,
) -> Result<McpConnectionTestResult, String> {
    let config = config_for_agent(agent_id)
        .ok_or_else(|| format!("Agent {agent_id} does not support MCP connection testing"))?;
    let active = read_active_servers(&config)?;
    let disabled = read_disabled_servers(agent_id)?;
    let server = active
        .get(server_name)
        .or_else(|| disabled.get(server_name))
        .ok_or_else(|| format!("MCP server not found: {server_name}"))?;
    let validation =
        validate_mcp_server_draft_inner(&config, &server.draft, Some(server_name), &HashSet::new());
    if !validation.valid {
        return Ok(McpConnectionTestResult {
            success: false,
            category: "invalid_config".to_string(),
            message: validation.message,
            latency_ms: 0,
            protocol_version: None,
            server_name: None,
            server_version: None,
            tool_count: None,
        });
    }
    let started = Instant::now();
    let test = async {
        match server.draft.transport {
            McpTransport::Stdio => test_stdio_connection(&server.draft).await,
            McpTransport::Http => test_streamable_http_connection(&server.draft).await,
            McpTransport::Sse => test_legacy_sse_connection(&server.draft).await,
        }
    };
    let result = tokio::time::timeout(MCP_TEST_TIMEOUT, test).await;
    let latency_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    match result {
        Ok(Ok(info)) => Ok(McpConnectionTestResult {
            success: true,
            category: "connected".to_string(),
            message: match info.tool_count {
                Some(count) => format!("Connected · {count} tools discovered"),
                None => "Connected · server does not declare tools".to_string(),
            },
            latency_ms,
            protocol_version: info.protocol_version,
            server_name: info.server_name,
            server_version: info.server_version,
            tool_count: info.tool_count,
        }),
        Ok(Err(error)) => Ok(McpConnectionTestResult {
            success: false,
            category: error.category.to_string(),
            message: redact_message(&error.message, &server.draft),
            latency_ms,
            protocol_version: None,
            server_name: None,
            server_version: None,
            tool_count: None,
        }),
        Err(_) => Ok(McpConnectionTestResult {
            success: false,
            category: "timeout".to_string(),
            message: "MCP connection test timed out after 15 seconds".to_string(),
            latency_ms,
            protocol_version: None,
            server_name: None,
            server_version: None,
            tool_count: None,
        }),
    }
}

async fn test_stdio_connection(draft: &McpServerDraft) -> Result<HandshakeInfo, TestFailure> {
    let command = draft.command.as_deref().unwrap_or_default();
    if !command_available(command) {
        return Err(TestFailure {
            category: "command_not_found",
            message: format!("Command not found: {command}"),
        });
    }
    let command_path = if command.starts_with('~') {
        super::v2::fsutil::expand_tilde(command)
    } else {
        crate::agents::executable::command_path(command)
    };
    let mut process = Command::new(command_path);
    process
        .args(&draft.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(path) = crate::agents::executable::augmented_path_env() {
        process.env("PATH", path);
    }
    if let Some(cwd) = nonempty(draft.cwd.as_deref()) {
        process.current_dir(super::v2::fsutil::expand_tilde(cwd));
    }
    for item in &draft.env {
        if let Some(value) = item.value.as_ref() {
            process.env(&item.key, value);
        }
    }
    #[cfg(unix)]
    process.process_group(0);
    let mut child = process.spawn().map_err(|error| TestFailure {
        category: "startup_failed",
        message: format!("Failed to start {command}: {error}"),
    })?;
    let mut stdin = child.stdin.take().ok_or_else(|| TestFailure {
        category: "startup_failed",
        message: "MCP process stdin is unavailable".to_string(),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| TestFailure {
        category: "startup_failed",
        message: "MCP process stdout is unavailable".to_string(),
    })?;
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(async move {
            let mut output = Vec::new();
            let _ = stderr.take(8192).read_to_end(&mut output).await;
            String::from_utf8_lossy(&output).to_string()
        })
    });
    let mut lines = BufReader::new(stdout).lines();
    let result = async {
        let mut initialize = None;
        let mut last_error = None;
        for protocol_version in std::iter::once(MCP_PROTOCOL_VERSION).chain(MCP_PROTOCOL_FALLBACKS)
        {
            write_stdio_message(&mut stdin, &initialize_request(1, protocol_version)).await?;
            match read_stdio_response(&mut lines, 1).await {
                Ok(response) => {
                    initialize = Some(response);
                    break;
                }
                Err(error) if error.category == "protocol_incompatible" => {
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        let initialize = initialize.ok_or_else(|| {
            last_error.unwrap_or(TestFailure {
                category: "protocol_incompatible",
                message: "MCP server rejected all supported protocol versions".to_string(),
            })
        })?;
        let info = handshake_info(&initialize)?;
        write_stdio_message(
            &mut stdin,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }),
        )
        .await?;
        let tools = initialize.pointer("/result/capabilities/tools").is_some();
        let tool_count = if tools {
            write_stdio_message(
                &mut stdin,
                &serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/list",
                    "params": {}
                }),
            )
            .await?;
            let response = read_stdio_response(&mut lines, 2).await?;
            Some(tool_count_from_response(&response)?)
        } else {
            None
        };
        Ok(HandshakeInfo { tool_count, ..info })
    }
    .await;
    drop(stdin);
    terminate_child(&mut child).await;
    let stderr = if let Some(task) = stderr_task {
        tokio::time::timeout(Duration::from_millis(250), task)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default()
    } else {
        String::new()
    };
    result.map_err(|mut error: TestFailure| {
        if !stderr.trim().is_empty() && error.message.len() < 1200 {
            error.message = format!("{} · {}", error.message, stderr.trim());
        }
        error
    })
}

async fn write_stdio_message(
    stdin: &mut tokio::process::ChildStdin,
    value: &Value,
) -> Result<(), TestFailure> {
    let mut bytes = serde_json::to_vec(value).map_err(|error| TestFailure {
        category: "protocol_error",
        message: error.to_string(),
    })?;
    bytes.push(b'\n');
    stdin.write_all(&bytes).await.map_err(|error| TestFailure {
        category: "startup_failed",
        message: format!("Failed to write to MCP process: {error}"),
    })?;
    stdin.flush().await.map_err(|error| TestFailure {
        category: "startup_failed",
        message: format!("Failed to flush MCP process input: {error}"),
    })
}

async fn read_stdio_response(
    lines: &mut tokio::io::Lines<BufReader<ChildStdout>>,
    id: i64,
) -> Result<Value, TestFailure> {
    loop {
        let line = lines.next_line().await.map_err(|error| TestFailure {
            category: "protocol_error",
            message: format!("Failed to read MCP response: {error}"),
        })?;
        let Some(line) = line else {
            return Err(TestFailure {
                category: "startup_failed",
                message: "MCP process exited before completing the handshake".to_string(),
            });
        };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("id").and_then(Value::as_i64) == Some(id) {
            return rpc_result(value);
        }
    }
}

async fn terminate_child(child: &mut Child) {
    if child.try_wait().ok().flatten().is_some() {
        return;
    }
    #[cfg(unix)]
    if let Some(id) = child.id() {
        unsafe {
            libc::kill(-(id as i32), libc::SIGTERM);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.start_kill();
    }
    if tokio::time::timeout(Duration::from_millis(500), child.wait())
        .await
        .is_err()
    {
        #[cfg(unix)]
        if let Some(id) = child.id() {
            unsafe {
                libc::kill(-(id as i32), libc::SIGKILL);
            }
        }
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}

async fn test_streamable_http_connection(
    draft: &McpServerDraft,
) -> Result<HandshakeInfo, TestFailure> {
    let client = mcp_http_client()?;
    let url = draft.url.as_deref().unwrap_or_default();
    let mut initialized = None;
    let mut last_error = None;
    for protocol_version in std::iter::once(MCP_PROTOCOL_VERSION).chain(MCP_PROTOCOL_FALLBACKS) {
        match send_streamable_rpc(
            &client,
            draft,
            url,
            &initialize_request(1, protocol_version),
            None,
            false,
        )
        .await
        {
            Ok(value) => {
                initialized = Some(value);
                break;
            }
            Err(error) if error.category == "legacy_sse_required" => {
                return test_legacy_sse_connection(draft).await;
            }
            Err(error) if error.category == "protocol_incompatible" => {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    let (initialize, session_id) = initialized.ok_or_else(|| {
        last_error.unwrap_or(TestFailure {
            category: "protocol_incompatible",
            message: "MCP server rejected all supported protocol versions".to_string(),
        })
    })?;
    let info = handshake_info(&initialize)?;
    send_streamable_rpc(
        &client,
        draft,
        url,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
        session_id.as_deref(),
        true,
    )
    .await?;
    let tool_count = if initialize.pointer("/result/capabilities/tools").is_some() {
        let (response, _) = send_streamable_rpc(
            &client,
            draft,
            url,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {}
            }),
            session_id.as_deref(),
            false,
        )
        .await?;
        Some(tool_count_from_response(&response)?)
    } else {
        None
    };
    if let Some(session_id) = session_id {
        let mut request = client.delete(url).header("Mcp-Session-Id", session_id);
        request = apply_http_headers(request, draft)?;
        let _ = request.send().await;
    }
    Ok(HandshakeInfo { tool_count, ..info })
}

fn mcp_http_client() -> Result<Client, TestFailure> {
    Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(MCP_TEST_TIMEOUT)
        .user_agent("AgentBro-MCP-Test/1")
        .build()
        .map_err(|error| TestFailure {
            category: "network_error",
            message: format!("Failed to create MCP HTTP client: {error}"),
        })
}

async fn send_streamable_rpc(
    client: &Client,
    draft: &McpServerDraft,
    url: &str,
    body: &Value,
    session_id: Option<&str>,
    notification: bool,
) -> Result<(Value, Option<String>), TestFailure> {
    let mut request = client
        .post(url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json");
    if let Some(session_id) = session_id {
        request = request
            .header("Mcp-Session-Id", session_id)
            .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION);
    }
    request = apply_http_headers(request, draft)?;
    let response = request.json(body).send().await.map_err(network_failure)?;
    if matches!(
        response.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return Err(TestFailure {
            category: "authentication_required",
            message: format!("Authentication required ({})", response.status()),
        });
    }
    if session_id.is_none()
        && matches!(
            response.status(),
            StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
        )
    {
        return Err(TestFailure {
            category: "legacy_sse_required",
            message: "Server requires the legacy HTTP+SSE transport".to_string(),
        });
    }
    if !response.status().is_success() {
        return Err(TestFailure {
            category: "network_error",
            message: format!("MCP server returned HTTP {}", response.status()),
        });
    }
    let next_session = response
        .headers()
        .get("Mcp-Session-Id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .or_else(|| session_id.map(str::to_string));
    if notification {
        return Ok((Value::Null, next_session));
    }
    let text = response.text().await.map_err(network_failure)?;
    let value = parse_rpc_response_text(&text)?;
    Ok((rpc_result(value)?, next_session))
}

fn apply_http_headers(
    mut request: reqwest::RequestBuilder,
    draft: &McpServerDraft,
) -> Result<reqwest::RequestBuilder, TestFailure> {
    for item in &draft.headers {
        let Some(value) = item.value.as_ref() else {
            continue;
        };
        let name =
            reqwest::header::HeaderName::from_bytes(item.key.as_bytes()).map_err(|error| {
                TestFailure {
                    category: "invalid_config",
                    message: format!("Invalid HTTP header {}: {error}", item.key),
                }
            })?;
        let value = reqwest::header::HeaderValue::from_str(value).map_err(|error| TestFailure {
            category: "invalid_config",
            message: format!("Invalid HTTP header value for {}: {error}", item.key),
        })?;
        request = request.header(name, value);
    }
    Ok(request)
}

async fn test_legacy_sse_connection(draft: &McpServerDraft) -> Result<HandshakeInfo, TestFailure> {
    let client = mcp_http_client()?;
    let url = draft.url.as_deref().unwrap_or_default();
    let mut request = client.get(url).header("Accept", "text/event-stream");
    request = apply_http_headers(request, draft)?;
    let mut stream = request.send().await.map_err(network_failure)?;
    if matches!(
        stream.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return Err(TestFailure {
            category: "authentication_required",
            message: format!("Authentication required ({})", stream.status()),
        });
    }
    if !stream.status().is_success() {
        return Err(TestFailure {
            category: "network_error",
            message: format!("SSE endpoint returned HTTP {}", stream.status()),
        });
    }
    let mut buffer = String::new();
    let (event, endpoint) = next_sse_event(&mut stream, &mut buffer).await?;
    if event.as_deref() != Some("endpoint") {
        return Err(TestFailure {
            category: "protocol_incompatible",
            message: "Legacy SSE server did not provide an endpoint event".to_string(),
        });
    }
    let endpoint = Url::parse(url)
        .and_then(|base| base.join(endpoint.trim()))
        .map_err(|error| TestFailure {
            category: "invalid_config",
            message: format!("Invalid SSE message endpoint: {error}"),
        })?;
    let mut initialize = None;
    let mut last_error = None;
    for protocol_version in std::iter::once(MCP_PROTOCOL_VERSION).chain(MCP_PROTOCOL_FALLBACKS) {
        post_legacy_sse(
            &client,
            draft,
            endpoint.as_str(),
            &initialize_request(1, protocol_version),
        )
        .await?;
        match next_sse_rpc_response(&mut stream, &mut buffer, 1).await {
            Ok(response) => {
                initialize = Some(response);
                break;
            }
            Err(error) if error.category == "protocol_incompatible" => {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    let initialize = initialize.ok_or_else(|| {
        last_error.unwrap_or(TestFailure {
            category: "protocol_incompatible",
            message: "MCP server rejected all supported protocol versions".to_string(),
        })
    })?;
    let info = handshake_info(&initialize)?;
    post_legacy_sse(
        &client,
        draft,
        endpoint.as_str(),
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    )
    .await?;
    let tool_count = if initialize.pointer("/result/capabilities/tools").is_some() {
        post_legacy_sse(
            &client,
            draft,
            endpoint.as_str(),
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {}
            }),
        )
        .await?;
        let response = next_sse_rpc_response(&mut stream, &mut buffer, 2).await?;
        Some(tool_count_from_response(&response)?)
    } else {
        None
    };
    Ok(HandshakeInfo { tool_count, ..info })
}

async fn post_legacy_sse(
    client: &Client,
    draft: &McpServerDraft,
    endpoint: &str,
    body: &Value,
) -> Result<(), TestFailure> {
    let mut request = client
        .post(endpoint)
        .header("Content-Type", "application/json");
    request = apply_http_headers(request, draft)?;
    let response = request.json(body).send().await.map_err(network_failure)?;
    if matches!(
        response.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return Err(TestFailure {
            category: "authentication_required",
            message: format!("Authentication required ({})", response.status()),
        });
    }
    if !response.status().is_success() {
        return Err(TestFailure {
            category: "network_error",
            message: format!("SSE message endpoint returned HTTP {}", response.status()),
        });
    }
    Ok(())
}

async fn next_sse_rpc_response(
    response: &mut reqwest::Response,
    buffer: &mut String,
    id: i64,
) -> Result<Value, TestFailure> {
    loop {
        let (_, data) = next_sse_event(response, buffer).await?;
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        if value.get("id").and_then(Value::as_i64) == Some(id) {
            return rpc_result(value);
        }
    }
}

async fn next_sse_event(
    response: &mut reqwest::Response,
    buffer: &mut String,
) -> Result<(Option<String>, String), TestFailure> {
    loop {
        if let Some(index) = buffer.find("\n\n") {
            let event = buffer[..index].to_string();
            buffer.drain(..index + 2);
            let mut event_name = None;
            let mut data = Vec::new();
            for line in event.lines() {
                if let Some(value) = line.strip_prefix("event:") {
                    event_name = Some(value.trim().to_string());
                } else if let Some(value) = line.strip_prefix("data:") {
                    data.push(value.trim_start().to_string());
                }
            }
            if !data.is_empty() {
                return Ok((event_name, data.join("\n")));
            }
            continue;
        }
        let Some(chunk) = response.chunk().await.map_err(network_failure)? else {
            return Err(TestFailure {
                category: "startup_failed",
                message: "SSE connection closed before the handshake completed".to_string(),
            });
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk).replace("\r\n", "\n"));
        if buffer.len() > 256 * 1024 {
            return Err(TestFailure {
                category: "protocol_error",
                message: "SSE handshake exceeded the response limit".to_string(),
            });
        }
    }
}

fn initialize_request(id: i64, protocol_version: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": protocol_version,
            "capabilities": {},
            "clientInfo": {
                "name": "AgentBro MCP Test",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

fn parse_rpc_response_text(text: &str) -> Result<Value, TestFailure> {
    if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
        return Ok(value);
    }
    for event in text.replace("\r\n", "\n").split("\n\n") {
        let data = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if let Ok(value) = serde_json::from_str::<Value>(&data) {
            return Ok(value);
        }
    }
    Err(TestFailure {
        category: "protocol_error",
        message: "MCP server returned an invalid JSON-RPC response".to_string(),
    })
}

fn rpc_result(value: Value) -> Result<Value, TestFailure> {
    if let Some(error) = value.get("error") {
        return Err(TestFailure {
            category: "protocol_incompatible",
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("MCP server returned a protocol error")
                .to_string(),
        });
    }
    Ok(value)
}

fn handshake_info(response: &Value) -> Result<HandshakeInfo, TestFailure> {
    let result = response.get("result").ok_or_else(|| TestFailure {
        category: "protocol_error",
        message: "Initialize response is missing result".to_string(),
    })?;
    Ok(HandshakeInfo {
        protocol_version: result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .map(str::to_string),
        server_name: result
            .pointer("/serverInfo/name")
            .and_then(Value::as_str)
            .map(str::to_string),
        server_version: result
            .pointer("/serverInfo/version")
            .and_then(Value::as_str)
            .map(str::to_string),
        tool_count: None,
    })
}

fn tool_count_from_response(response: &Value) -> Result<usize, TestFailure> {
    response
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .map(Vec::len)
        .ok_or_else(|| TestFailure {
            category: "protocol_error",
            message: "tools/list response is missing the tools array".to_string(),
        })
}

fn network_failure(error: reqwest::Error) -> TestFailure {
    TestFailure {
        category: if error.is_timeout() {
            "timeout"
        } else {
            "network_error"
        },
        message: error.to_string(),
    }
}

fn redact_message(message: &str, draft: &McpServerDraft) -> String {
    let mut redacted = message.to_string();
    for value in draft.env.iter().chain(draft.headers.iter()) {
        if let Some(secret) = value.value.as_deref().filter(|value| !value.is_empty()) {
            redacted = redacted.replace(secret, "***");
        }
    }
    if redacted.len() > 1600 {
        redacted.truncate(1600);
        redacted.push('…');
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Json, Router};

    fn stdio_draft(name: &str, command: &str) -> McpServerDraft {
        McpServerDraft {
            name: name.to_string(),
            transport: McpTransport::Stdio,
            command: Some(command.to_string()),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            url: None,
            headers: Vec::new(),
        }
    }

    fn config(kind: ConfigKind) -> AgentConfig {
        AgentConfig {
            agent_id: "test".to_string(),
            path: PathBuf::from("/tmp/agentbro-mcp-test"),
            kind,
            capabilities: capabilities(true, true, true, true),
        }
    }

    #[test]
    fn json_adapter_preserves_unknown_fields() {
        let draft = McpServerDraft {
            name: "remote".to_string(),
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            url: Some("https://example.com/mcp".to_string()),
            headers: vec![McpConfigValue {
                key: "Authorization".to_string(),
                value: Some("Bearer secret".to_string()),
                secret: true,
                configured: true,
            }],
        };
        let existing = serde_json::json!({
            "command": "old",
            "unknown": { "keep": true },
            "enable": false
        });

        let raw = raw_value_for(&config(ConfigKind::Zcode), &draft, true, Some(&existing));

        assert_eq!(raw.pointer("/unknown/keep"), Some(&Value::Bool(true)));
        assert_eq!(raw.get("command"), None);
        assert_eq!(raw.get("type").and_then(Value::as_str), Some("http"));
        assert_eq!(raw.get("enable").and_then(Value::as_bool), Some(true));
        assert_eq!(
            raw.pointer("/headers/Authorization")
                .and_then(Value::as_str),
            Some("Bearer secret")
        );
    }

    #[test]
    fn json_adapters_follow_each_agent_transport_shape() {
        let cases = [
            (
                ConfigKind::Claude,
                serde_json::json!({
                    "type": "http",
                    "url": "https://example.com/mcp",
                    "headers": { "Authorization": "Bearer secret" }
                }),
                McpTransport::Http,
                true,
            ),
            (
                ConfigKind::Gemini,
                serde_json::json!({ "httpUrl": "https://example.com/mcp" }),
                McpTransport::Http,
                true,
            ),
            (
                ConfigKind::Gemini,
                serde_json::json!({ "url": "https://example.com/sse" }),
                McpTransport::Sse,
                true,
            ),
            (
                ConfigKind::Cursor,
                serde_json::json!({ "type": "sse", "url": "https://example.com/sse" }),
                McpTransport::Sse,
                true,
            ),
            (
                ConfigKind::Kimi,
                serde_json::json!({
                    "url": "https://example.com/mcp",
                    "enabled": false
                }),
                McpTransport::Http,
                false,
            ),
            (
                ConfigKind::Zcode,
                serde_json::json!({
                    "type": "stdio",
                    "command": "node",
                    "args": ["server.js"],
                    "enable": false
                }),
                McpTransport::Stdio,
                false,
            ),
        ];

        for (kind, raw, transport, enabled) in cases {
            let normalized = normalize_json_server(kind, "demo", &raw, false).unwrap();
            assert_eq!(normalized.draft.transport, transport);
            assert_eq!(normalized.enabled, enabled);
        }
    }

    #[test]
    fn secret_values_are_redacted_and_can_be_preserved() {
        let previous = McpServerDraft {
            env: vec![McpConfigValue {
                key: "API_TOKEN".to_string(),
                value: Some("top-secret".to_string()),
                secret: true,
                configured: true,
            }],
            ..stdio_draft("demo", "node")
        };
        let redacted = redact_draft(previous.clone());
        assert_eq!(redacted.env[0].value, None);
        assert!(redacted.env[0].configured);

        let resolved = resolve_secret_placeholders(&redacted, Some(&previous)).unwrap();
        assert_eq!(resolved.env[0].value.as_deref(), Some("top-secret"));
        assert_eq!(
            redact_message("request failed for top-secret", &previous),
            "request failed for ***"
        );
    }

    #[test]
    fn codex_upsert_preserves_unrelated_and_unknown_toml() {
        let content = r#"# keep this comment
model = "gpt-5"

[mcp_servers.old]
command = "old-command"
args = ["--old"]
enabled = true
startup_timeout_sec = 20

[mcp_servers.old.env]
API_TOKEN = "old-secret"

[mcp_servers.old.custom]
keep = "yes"

[profiles.work]
model = "gpt-5"
"#;
        let draft = McpServerDraft {
            args: vec!["server.js".to_string()],
            env: vec![McpConfigValue {
                key: "API_TOKEN".to_string(),
                value: Some("new-secret".to_string()),
                secret: true,
                configured: true,
            }],
            ..stdio_draft("renamed", "node")
        };

        let next = upsert_codex_server(content, "old", &draft, false).unwrap();
        let parsed = parse_codex_servers(&next);
        let server = parsed.get("renamed").unwrap();

        assert!(next.contains("# keep this comment"));
        assert!(next.contains("[profiles.work]"));
        assert!(next.contains("startup_timeout_sec = 20"));
        assert!(next.contains("[mcp_servers.renamed.custom]"));
        assert!(!next.contains("[mcp_servers.old]"));
        assert_eq!(server.draft.command.as_deref(), Some("node"));
        assert_eq!(server.draft.args, vec!["server.js"]);
        assert!(!server.enabled);
    }

    #[test]
    fn codex_upsert_rejects_a_rename_collision() {
        let content = r#"[mcp_servers.one]
command = "one"

[mcp_servers.two]
command = "two"
"#;
        let draft = stdio_draft("two", "replacement");

        let error = upsert_codex_server(content, "one", &draft, true).unwrap_err();

        assert!(error.contains("already exists"));
    }

    #[test]
    fn validation_rejects_duplicates_and_unsupported_transport() {
        let mut codex = config(ConfigKind::Codex);
        codex.capabilities = capabilities(true, true, false, true);
        let mut names = HashSet::new();
        names.insert("existing".to_string());
        let draft = McpServerDraft {
            name: "existing".to_string(),
            transport: McpTransport::Sse,
            command: None,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            url: Some("https://example.com/sse".to_string()),
            headers: Vec::new(),
        };

        let result = validate_mcp_server_draft_inner(&codex, &draft, None, &names);

        assert!(!result.valid);
        assert!(result.message.contains("already exists"));
        assert!(result.message.contains("does not support SSE"));
    }

    #[test]
    fn revision_detects_external_changes() {
        let path = std::env::temp_dir().join(format!("agentbro-mcp-{}.json", uuid::Uuid::new_v4()));
        fs::write(&path, b"one").unwrap();
        let revision = revision_for("revision-test", Some(&path)).unwrap();
        fs::write(&path, b"two").unwrap();

        let error = ensure_revision("revision-test", Some(&path), &revision).unwrap_err();

        assert!(error.contains("changed outside AgentBro"));
        let _ = fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn safe_write_uses_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("agentbro-mcp-{}", uuid::Uuid::new_v4()));
        let path = dir.join("config.json");
        safe_write_json(&path, &serde_json::json!({ "mcpServers": {} }), "test").unwrap();

        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stdio_test_falls_back_then_performs_tools_list() {
        let mut draft = stdio_draft("fake", "/bin/sh");
        draft.args = vec![
            "-c".to_string(),
            r#"while IFS= read -r line; do
case "$line" in
  *'"protocolVersion":"2024-11-05"'*) printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"fake-mcp","version":"1.0.0"}}}' ;;
  *'"method":"initialize"'*) printf '%s\n' '{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"unsupported protocol version"}}' ;;
  *'"id":2'*) printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"one"},{"name":"two"}]}}' ;;
esac
done"#
                .to_string(),
        ];

        let result = test_stdio_connection(&draft).await.unwrap();

        assert_eq!(result.protocol_version.as_deref(), Some("2024-11-05"));
        assert_eq!(result.server_name.as_deref(), Some("fake-mcp"));
        assert_eq!(result.tool_count, Some(2));
    }

    async fn fake_http_mcp(Json(body): Json<Value>) -> Json<Value> {
        let id = body.get("id").cloned().unwrap_or(Value::Null);
        let result = match body.get("method").and_then(Value::as_str) {
            Some("initialize") => serde_json::json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "fake-http", "version": "1.0.0" }
            }),
            Some("tools/list") => serde_json::json!({
                "tools": [{ "name": "one" }, { "name": "two" }, { "name": "three" }]
            }),
            _ => Value::Null,
        };
        Json(serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    async fn fake_inspector_mcp(Json(body): Json<Value>) -> Json<Value> {
        let id = body.get("id").cloned().unwrap_or(Value::Null);
        let method = body.get("method").and_then(Value::as_str);
        assert_ne!(method, Some("tools/call"));
        assert_ne!(method, Some("resources/read"));
        assert_ne!(method, Some("prompts/get"));
        let result = match method {
            Some("initialize") => serde_json::json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {},
                    "resources": {},
                    "prompts": {}
                },
                "serverInfo": { "name": "fake-inspector", "version": "2.0.0" }
            }),
            Some("tools/list") if body.pointer("/params/cursor").is_none() => serde_json::json!({
                "tools": [{
                    "name": "search",
                    "title": "Search",
                    "description": "Search connected data",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": {
                                "type": "string",
                                "description": "Search query"
                            }
                        },
                        "required": ["query"]
                    },
                    "annotations": {
                        "readOnlyHint": true,
                        "destructiveHint": false,
                        "idempotentHint": true,
                        "openWorldHint": false
                    }
                }],
                "nextCursor": "tools-page-2"
            }),
            Some("tools/list") => serde_json::json!({
                "tools": [{
                    "name": "publish",
                    "inputSchema": { "type": "object", "properties": {} }
                }]
            }),
            Some("resources/list") => serde_json::json!({
                "resources": [{
                    "uri": "demo://knowledge",
                    "name": "Knowledge",
                    "mimeType": "application/json"
                }]
            }),
            Some("prompts/list") => serde_json::json!({
                "prompts": [{
                    "name": "summarize",
                    "description": "Summarize data",
                    "arguments": [{
                        "name": "topic",
                        "required": true
                    }]
                }]
            }),
            _ => Value::Null,
        };
        Json(serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    async fn fake_partial_inspector_mcp(Json(body): Json<Value>) -> Json<Value> {
        let id = body.get("id").cloned().unwrap_or(Value::Null);
        match body.get("method").and_then(Value::as_str) {
            Some("initialize") => Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": { "tools": {}, "resources": {} },
                    "serverInfo": { "name": "fake-partial", "version": "1.0.0" }
                }
            })),
            Some("tools/list") => Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": [] }
            })),
            Some("resources/list") => Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32603, "message": "resource backend unavailable" }
            })),
            _ => Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": Value::Null
            })),
        }
    }

    async fn fake_operation_mcp(Json(body): Json<Value>) -> Json<Value> {
        let id = body.get("id").cloned().unwrap_or(Value::Null);
        let result = match body.get("method").and_then(Value::as_str) {
            Some("initialize") => serde_json::json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {},
                    "prompts": {}
                },
                "serverInfo": { "name": "fake-operation", "version": "1.0.0" }
            }),
            Some("tools/call") => serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": body.pointer("/params/arguments/query")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                }],
                "structuredContent": {
                    "received": body.pointer("/params/arguments/query")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                },
                "isError": false
            }),
            Some("prompts/get") => serde_json::json!({
                "description": "Generated preview",
                "messages": [{
                    "role": "user",
                    "content": {
                        "type": "text",
                        "text": body.pointer("/params/arguments/topic")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                    }
                }]
            }),
            _ => Value::Null,
        };
        Json(serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    #[tokio::test]
    async fn streamable_http_test_initializes_and_lists_tools() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/", post(fake_http_mcp)))
                .await
                .unwrap();
        });
        let draft = McpServerDraft {
            name: "remote".to_string(),
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            url: Some(format!("http://{address}/")),
            headers: Vec::new(),
        };

        let result = test_streamable_http_connection(&draft).await.unwrap();

        assert_eq!(result.server_name.as_deref(), Some("fake-http"));
        assert_eq!(result.tool_count, Some(3));
        task.abort();
    }

    #[tokio::test]
    async fn capability_inspector_discovers_metadata_without_invoking_features() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/", post(fake_inspector_mcp)))
                .await
                .unwrap();
        });
        let draft = McpServerDraft {
            name: "inspector".to_string(),
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            url: Some(format!("http://{address}/")),
            headers: Vec::new(),
        };

        let report = perform_mcp_inspection("inspection-1", &draft, Vec::new()).await;

        assert_eq!(report.status, "connected");
        assert_eq!(report.server_name.as_deref(), Some("fake-inspector"));
        assert_eq!(report.tools.len(), 2);
        assert_eq!(report.resources.len(), 1);
        assert_eq!(report.prompts.len(), 1);
        assert_eq!(report.tools[0].inputs[0].name, "query");
        assert_eq!(
            report.tools[0]
                .input_schema
                .pointer("/properties/query/type")
                .and_then(Value::as_str),
            Some("string")
        );
        assert_eq!(report.tools[0].annotations.read_only, Some(true));
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("risk annotations")));
        task.abort();
    }

    #[tokio::test]
    async fn one_shot_operations_call_tools_and_preview_prompts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/", post(fake_operation_mcp)))
                .await
                .unwrap();
        });
        let draft = McpServerDraft {
            name: "operations".to_string(),
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            url: Some(format!("http://{address}/")),
            headers: Vec::new(),
        };

        let tool = perform_mcp_operation(
            "tool-operation",
            "tool",
            "echo",
            "tools/call",
            serde_json::json!({
                "name": "echo",
                "arguments": { "query": "hello" }
            }),
            &draft,
            Vec::new(),
        )
        .await
        .unwrap();
        assert_eq!(tool.category, "success");
        assert_eq!(
            tool.result
                .pointer("/structuredContent/received")
                .and_then(Value::as_str),
            Some("hello")
        );

        let prompt = perform_mcp_operation(
            "prompt-operation",
            "prompt",
            "summarize",
            "prompts/get",
            serde_json::json!({
                "name": "summarize",
                "arguments": { "topic": "Blender" }
            }),
            &draft,
            Vec::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            prompt
                .result
                .pointer("/messages/0/content/text")
                .and_then(Value::as_str),
            Some("Blender")
        );
        task.abort();
    }

    #[tokio::test]
    async fn capability_inspector_keeps_partial_results() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/", post(fake_partial_inspector_mcp)),
            )
            .await
            .unwrap();
        });
        let draft = McpServerDraft {
            name: "partial".to_string(),
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            url: Some(format!("http://{address}/")),
            headers: Vec::new(),
        };

        let report = perform_mcp_inspection("inspection-2", &draft, Vec::new()).await;

        assert_eq!(report.status, "partial");
        assert!(report.capabilities.tools);
        assert!(report.capabilities.resources);
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("resource backend unavailable")));
        assert!(report
            .steps
            .iter()
            .any(|step| step.phase == "resources" && step.status == "failed"));
        task.abort();
    }

    #[tokio::test]
    async fn streamable_http_reports_authentication_required() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/", post(|| async { axum::http::StatusCode::UNAUTHORIZED })),
            )
            .await
            .unwrap();
        });
        let draft = McpServerDraft {
            name: "remote".to_string(),
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            url: Some(format!("http://{address}/")),
            headers: Vec::new(),
        };

        let error = test_streamable_http_connection(&draft).await.unwrap_err();

        assert_eq!(error.category, "authentication_required");
        task.abort();
    }
}
