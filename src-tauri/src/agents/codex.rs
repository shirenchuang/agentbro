// CodexAdapter — Agent adapter for OpenAI Codex CLI

use super::{profiles, AdapterStatus, AgentAdapter, AgentEvent, QuestionItem, QuestionOption};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[cfg(test)]
const AGENTBRO_MARKER: &str = "agentbro";
const CODEX_PERMISSION_TIMEOUT_SECONDS: u64 = 21_600;

const HOOK_EVENTS: &[(&str, &str, u64)] = &[
    ("SessionStart", "session_start", 5),
    ("UserPromptSubmit", "user_prompt_submit", 5),
    ("PreToolUse", "pre_tool_use", 5),
    ("PostToolUse", "post_tool_use", 5),
    (
        "PermissionRequest",
        "permission_request",
        CODEX_PERMISSION_TIMEOUT_SECONDS,
    ),
    ("Stop", "stop", 5),
];

pub struct CodexAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl CodexAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".codex");
        let status = if Self::is_installed() {
            AdapterStatus::Available
        } else {
            AdapterStatus::Unavailable
        };
        Self {
            config_root,
            status,
        }
    }

    fn is_installed() -> bool {
        std::process::Command::new("which")
            .arg("codex")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn hooks_path(&self) -> PathBuf {
        self.config_root.join("hooks.json")
    }

    fn config_toml_path(&self) -> PathBuf {
        self.config_root.join("config.toml")
    }

    fn hook_command() -> Result<String, Box<dyn std::error::Error>> {
        profiles::managed_bridge_command(&profiles::codex_profile())
    }

    #[cfg(test)]
    fn inject_hooks_json(settings: &mut serde_json::Value, hook_command: &str) {
        if !settings.get("hooks").is_some_and(|hooks| hooks.is_object()) {
            settings["hooks"] = serde_json::json!({});
        }
        let hooks = settings["hooks"].as_object_mut().expect("hooks is object");

        for (event, _, timeout) in HOOK_EVENTS {
            let entry = hooks
                .entry(event.to_string())
                .or_insert_with(|| serde_json::json!([]));
            if !entry.is_array() {
                *entry = serde_json::json!([]);
            }
            let groups = entry.as_array_mut().expect("event hooks is array");
            groups.retain(|group| !group_contains_agentbro(group));
            groups.push(serde_json::json!({
                "hooks": [{
                    "type": "command",
                    "command": hook_command,
                    "timeout": timeout
                }]
            }));
        }
    }

    fn trust_codex_hooks(
        hooks_path: &Path,
        config_toml_path: &Path,
        settings: &serde_json::Value,
        hook_command: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let source_path = hooks_path
            .canonicalize()
            .unwrap_or_else(|_| hooks_path.to_path_buf());
        let source_path = source_path.to_string_lossy().to_string();
        let mut states = BTreeMap::new();

        let Some(hooks) = settings.get("hooks").and_then(|hooks| hooks.as_object()) else {
            return Ok(());
        };

        for (event_name, event_key, event_timeout) in HOOK_EVENTS {
            let Some(groups) = hooks.get(*event_name).and_then(|value| value.as_array()) else {
                continue;
            };

            for (group_index, group) in groups.iter().enumerate() {
                let Some(handlers) = group.get("hooks").and_then(|value| value.as_array()) else {
                    continue;
                };
                for (handler_index, handler) in handlers.iter().enumerate() {
                    if handler.get("type").and_then(|value| value.as_str()) != Some("command") {
                        continue;
                    }
                    if handler.get("command").and_then(|value| value.as_str()) != Some(hook_command)
                    {
                        continue;
                    }

                    let timeout = handler
                        .get("timeout")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(*event_timeout)
                        .max(1);
                    let mut normalized_group = serde_json::json!({
                        "hooks": [{
                            "type": "command",
                            "command": hook_command,
                            "timeout": timeout,
                            "async": handler
                                .get("async")
                                .and_then(|value| value.as_bool())
                                .unwrap_or(false)
                        }]
                    });
                    if let Some(matcher) = group.get("matcher").and_then(|value| value.as_str()) {
                        normalized_group["matcher"] =
                            serde_json::Value::String(matcher.to_string());
                    }

                    let identity = serde_json::json!({
                        "event_name": event_key,
                        "hooks": normalized_group["hooks"],
                        "matcher": normalized_group.get("matcher"),
                    });
                    let identity = if normalized_group.get("matcher").is_some() {
                        identity
                    } else {
                        serde_json::json!({
                            "event_name": event_key,
                            "hooks": normalized_group["hooks"],
                        })
                    };
                    let trusted_hash = codex_hook_hash(&identity);
                    let key = format!("{source_path}:{event_key}:{group_index}:{handler_index}");
                    states.insert(key, trusted_hash);
                }
            }
        }

        if states.is_empty() {
            return Ok(());
        }
        upsert_codex_trust_state(config_toml_path, &states)?;
        Ok(())
    }
}

impl AgentAdapter for CodexAdapter {
    fn name(&self) -> &str {
        "codex"
    }
    fn display_name(&self) -> &str {
        "OpenAI Codex"
    }
    fn icon(&self) -> &str {
        "codex"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let hooks_path = self.hooks_path();
        let hook_command = Self::hook_command()?;
        let settings = profiles::install_nested_json_hooks_at(
            &profiles::codex_profile(),
            &hooks_path,
            &hook_command,
        )?;
        Self::trust_codex_hooks(
            &hooks_path,
            &self.config_toml_path(),
            &settings,
            &hook_command,
        )?;
        log::info!("Codex hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall_at(&profiles::codex_profile(), &self.hooks_path())?;
        log::info!("Codex hooks removed");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }

    fn parse_event(
        &self,
        raw: &serde_json::Value,
    ) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        let agent = raw.get("agent").and_then(|v| v.as_str()).unwrap_or("");
        if !agent.is_empty() && agent != "codex" {
            return Err("not a codex event".into());
        }

        let session_id = raw
            .get("session_id")
            .or_else(|| raw.get("sessionId"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let event = normalize_event_name(raw.get("event").and_then(|v| v.as_str()).unwrap_or(""));
        let status = raw.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let cwd = raw
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let project = cwd.rsplit('/').next().unwrap_or(&cwd).to_string();
        let terminal = raw
            .get("tty")
            .or_else(|| raw.get("terminal"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let raw_tool_name = raw
            .get("tool")
            .or_else(|| raw.get("tool_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tool_input_value = raw.get("tool_input").or_else(|| raw.get("toolInput"));
        let tool_input = raw
            .get("tool_input")
            .or_else(|| raw.get("toolInput"))
            .map(|v| v.to_string())
            .unwrap_or_default();
        let tool_name = normalize_codex_tool_name(&raw_tool_name);
        let tool_target = extract_codex_tool_target(&tool_name, tool_input_value);

        match event.as_str() {
            "SessionStart" => Ok(AgentEvent::SessionStart {
                session_id,
                project,
                cwd,
                terminal,
                agent_type: "codex".to_string(),
            }),
            "SessionEnd" => Ok(AgentEvent::SessionEnd { session_id }),
            "UserPromptSubmit" => Ok(AgentEvent::Processing {
                session_id,
                description: "Processing user input".to_string(),
            }),
            "PreToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target,
                status: "running".to_string(),
            }),
            "PostToolUse" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target,
                status: codex_post_tool_status(raw),
            }),
            "PostToolUseFailure" | "PermissionDenied" => Ok(AgentEvent::ToolUse {
                session_id,
                tool_name,
                tool_input,
                tool_target,
                status: "error".to_string(),
            }),
            "PermissionRequest" => Ok(AgentEvent::PermissionRequest {
                session_id,
                tool_name,
                diff: None,
                options: None,
            }),
            "AskQuestion" => parse_question_event(raw, session_id),
            "PlanApproval" => parse_plan_event(raw, session_id),
            "Stop" => Ok(AgentEvent::AssistantResponseComplete {
                session_id,
                text: raw
                    .get("summary")
                    .or_else(|| raw.get("last_assistant_message"))
                    .or_else(|| raw.get("lastAssistantMessage"))
                    .or_else(|| raw.get("message"))
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or("Task completed")
                    .to_string(),
            }),
            "StopFailure" => Ok(AgentEvent::Error {
                session_id,
                message: raw
                    .get("error")
                    .or_else(|| raw.get("message"))
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or("Task failed")
                    .to_string(),
            }),
            "Notification" => Ok(AgentEvent::Notification {
                session_id,
                message: raw
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                status: if status.is_empty() {
                    None
                } else {
                    Some(status.to_string())
                },
            }),
            _ => Ok(AgentEvent::Processing {
                session_id,
                description: format!("Event: {}", event),
            }),
        }
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![self.hooks_path(), self.config_toml_path()]
    }
}

#[cfg(test)]
fn group_contains_agentbro(group: &serde_json::Value) -> bool {
    if group
        .get("command")
        .and_then(|command| command.as_str())
        .is_some_and(|command| command.contains(AGENTBRO_MARKER))
    {
        return true;
    }
    group
        .get("hooks")
        .and_then(|hooks| hooks.as_array())
        .is_some_and(|hooks| {
            hooks.iter().any(|hook| {
                hook.get("command")
                    .and_then(|command| command.as_str())
                    .is_some_and(|command| command.contains(AGENTBRO_MARKER))
            })
        })
}

fn normalize_event_name(event: &str) -> String {
    match event {
        "session_start" => "SessionStart",
        "session_end" => "SessionEnd",
        "user_prompt_submit" => "UserPromptSubmit",
        "pre_tool_use" => "PreToolUse",
        "post_tool_use" => "PostToolUse",
        "post_tool_use_failure" => "PostToolUseFailure",
        "permission_request" => "PermissionRequest",
        "permission_denied" => "PermissionDenied",
        "stop" => "Stop",
        "stop_failure" => "StopFailure",
        other => other,
    }
    .to_string()
}

fn normalize_codex_tool_name(tool_name: &str) -> String {
    let trimmed = tool_name.trim();
    if trimmed.starts_with("mcp__") {
        return trimmed.to_string();
    }
    let normalized = trimmed
        .trim_start_matches("functions.")
        .replace(['-', ' '], "_")
        .to_lowercase();

    match normalized.as_str() {
        "read" | "read_file" | "readfile" | "open_file" | "view_file" | "sed" | "cat" => {
            "Read".to_string()
        }
        "write" | "write_file" | "writefile" | "create_file" => "Write".to_string(),
        "edit" | "edit_file" | "editfile" | "apply_patch" | "patch" | "str_replace"
        | "update_file" | "notebook_edit" => "Edit".to_string(),
        "grep" | "rg" | "search" | "find" | "glob" | "list_files" | "ls" => "Grep".to_string(),
        "bash" | "shell" | "exec" | "exec_command" | "terminal" | "run_command" | "command" => {
            "Bash".to_string()
        }
        "web_search" | "search_query" => "WebSearch".to_string(),
        "web_fetch" | "open" | "fetch" => "WebFetch".to_string(),
        "update_plan" | "todo_write" => "TodoWrite".to_string(),
        "spawn_agent" | "send_input" | "wait_agent" => "Agent".to_string(),
        _ if trimmed.is_empty() => "Tool".to_string(),
        _ => trimmed.to_string(),
    }
}

fn extract_codex_tool_target(
    normalized_tool_name: &str,
    tool_input: Option<&serde_json::Value>,
) -> Option<String> {
    let input = parse_embedded_json(tool_input);

    if matches!(normalized_tool_name, "Edit" | "Write") {
        if let Some(patch) = string_field(&input, &["patch", "command", "input", "diff"]) {
            if let Some(target) = patch_target_with_stats(&patch) {
                return Some(target);
            }
        }
    }

    match normalized_tool_name {
        "Read" | "Edit" | "Write" | "Grep" | "Glob" | "NotebookEdit" => {
            let path = string_field(
                &input,
                &[
                    "file_path",
                    "filePath",
                    "path",
                    "filepath",
                    "filename",
                    "target",
                    "relative_path",
                ],
            )?;
            Some(display_path_name(&path))
        }
        "Bash" => string_field(&input, &["command", "cmd"])
            .map(|command| truncate_display_text(command.trim(), 50)),
        "WebSearch" | "WebFetch" => string_field(&input, &["query", "url"])
            .map(|value| truncate_display_text(value.trim(), 50)),
        "TodoWrite" => Some("tasks".to_string()),
        "Agent" => string_field(&input, &["message", "prompt", "task", "description"])
            .map(|value| truncate_display_text(value.trim(), 50)),
        _ => string_field(&input, &["target", "path", "file_path"])
            .map(|value| truncate_display_text(value.trim(), 50)),
    }
}

fn parse_embedded_json(tool_input: Option<&serde_json::Value>) -> serde_json::Value {
    let Some(value) = tool_input else {
        return serde_json::Value::Null;
    };
    if let Some(text) = value.as_str() {
        serde_json::from_str(text).unwrap_or_else(|_| serde_json::Value::String(text.to_string()))
    } else {
        value.clone()
    }
}

fn string_field(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            for key in keys {
                if let Some(text) = map
                    .get(*key)
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                {
                    return Some(text.to_string());
                }
            }
            for nested_key in ["args", "arguments", "params", "input"] {
                if let Some(nested) = map.get(nested_key) {
                    if let Some(value) = string_field(nested, keys) {
                        return Some(value);
                    }
                }
            }
            None
        }
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    }
}

fn patch_target_with_stats(patch: &str) -> Option<String> {
    let mut path: Option<String> = None;
    let mut additions = 0usize;
    let mut deletions = 0usize;

    for line in patch.lines() {
        if path.is_none() {
            path = line
                .strip_prefix("*** Update File: ")
                .or_else(|| line.strip_prefix("*** Add File: "))
                .or_else(|| line.strip_prefix("*** Delete File: "))
                .map(|value| value.trim().to_string())
                .or_else(|| parse_diff_path_marker(line));
        }

        if line.starts_with('+') && !line.starts_with("+++") {
            additions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }
    }

    let path = path?;
    let name = display_path_name(&path);
    if additions == 0 && deletions == 0 {
        Some(name)
    } else {
        Some(format!("{name} +{additions} -{deletions}"))
    }
}

fn parse_diff_path_marker(line: &str) -> Option<String> {
    let value = line
        .strip_prefix("+++ ")
        .or_else(|| line.strip_prefix("--- "))?
        .trim();
    if value == "/dev/null" {
        return None;
    }
    Some(
        value
            .trim_start_matches("a/")
            .trim_start_matches("b/")
            .to_string(),
    )
}

fn display_path_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn truncate_display_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let take_chars = max_chars.saturating_sub(3);
    format!("{}...", text.chars().take(take_chars).collect::<String>())
}

fn codex_post_tool_status(raw: &serde_json::Value) -> String {
    if raw.get("tool_error").is_some()
        || raw.get("denial_reason").is_some()
        || raw
            .get("tool_response")
            .or_else(|| raw.get("toolResponse"))
            .is_some_and(tool_response_looks_failed)
    {
        "error".to_string()
    } else {
        "success".to_string()
    }
}

fn tool_response_looks_failed(response: &serde_json::Value) -> bool {
    response
        .get("exit_code")
        .or_else(|| response.get("exitCode"))
        .and_then(|value| value.as_i64())
        .is_some_and(|code| code != 0)
        || response
            .get("isError")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        || response
            .get("status")
            .and_then(|value| value.as_str())
            .is_some_and(|status| matches!(status, "error" | "failed" | "failure"))
        || response.get("error").is_some()
}

fn parse_question_event(
    raw: &serde_json::Value,
    session_id: String,
) -> Result<AgentEvent, Box<dyn std::error::Error>> {
    let question = raw
        .get("question")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let options = raw
        .get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let descriptions = raw
        .get("descriptions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let header = raw
        .get("header")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let multi_select = raw
        .get("multi_select")
        .or_else(|| raw.get("multiSelect"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let questions = raw
        .get("questions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|question| {
                    let text = question.get("question")?.as_str()?.to_string();
                    let options = question
                        .get("options")
                        .and_then(|v| v.as_array())
                        .map(|options| {
                            options
                                .iter()
                                .filter_map(|option| {
                                    let label = option.get("label")?.as_str()?.to_string();
                                    let description = option
                                        .get("description")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                    Some(QuestionOption { label, description })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let header = question
                        .get("header")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let multi_select = question
                        .get("multiSelect")
                        .or_else(|| question.get("multi_select"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    Some(QuestionItem {
                        question: text,
                        header,
                        options,
                        multi_select,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(AgentEvent::AskQuestion {
        session_id,
        question,
        options,
        descriptions,
        header,
        multi_select,
        questions,
    })
}

fn parse_plan_event(
    raw: &serde_json::Value,
    session_id: String,
) -> Result<AgentEvent, Box<dyn std::error::Error>> {
    let title = raw
        .get("plan_title")
        .or_else(|| raw.get("planTitle"))
        .and_then(|v| v.as_str())
        .unwrap_or("Plan")
        .to_string();
    let content = raw
        .get("plan_content")
        .or_else(|| raw.get("planContent"))
        .or_else(|| raw.get("plan"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let permissions = raw
        .get("requested_permissions")
        .or_else(|| raw.get("allowedPrompts"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|item| {
                    if let Some(s) = item.as_str() {
                        s.to_string()
                    } else if let (Some(tool), Some(prompt)) = (
                        item.get("tool").and_then(|v| v.as_str()),
                        item.get("prompt").and_then(|v| v.as_str()),
                    ) {
                        format!("{tool}: {prompt}")
                    } else {
                        item.to_string()
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(AgentEvent::PlanApproval {
        session_id,
        title,
        content,
        permissions,
    })
}

fn codex_hook_hash(value: &serde_json::Value) -> String {
    let serialized = canonical_json(value);
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::String(value) => serde_json::to_string(value).unwrap_or_default(),
        serde_json::Value::Array(values) => {
            let inner = values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{inner}]")
        }
        serde_json::Value::Object(values) => {
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort();
            let inner = keys
                .into_iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_default(),
                        canonical_json(&values[key])
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{inner}}}")
        }
    }
}

fn upsert_codex_trust_state(
    config_path: &Path,
    states: &BTreeMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let existing = std::fs::read_to_string(config_path).unwrap_or_default();
    let mut content = ensure_codex_hooks_feature(&existing);
    let headers = states
        .keys()
        .map(|key| format!("[hooks.state.\"{}\"]", toml_basic_string(key)))
        .collect::<BTreeSet<_>>();
    content = remove_toml_tables(&content, &headers)
        .trim_end()
        .to_string();

    for (key, trusted_hash) in states {
        content.push_str("\n\n");
        content.push_str(&format!(
            "[hooks.state.\"{}\"]\ntrusted_hash = \"{}\"",
            toml_basic_string(key),
            toml_basic_string(trusted_hash)
        ));
    }
    content.push('\n');
    std::fs::write(config_path, content)?;
    Ok(())
}

fn toml_basic_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn remove_toml_tables(content: &str, table_headers: &BTreeSet<String>) -> String {
    let mut result = Vec::new();
    let mut skipping = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            skipping = table_headers.contains(trimmed);
        }
        if !skipping {
            result.push(line);
        }
    }
    result.join("\n").replace("\n\n\n", "\n\n")
}

fn ensure_codex_hooks_feature(content: &str) -> String {
    let mut lines = content
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    let Some(features_index) = lines.iter().position(|line| line.trim() == "[features]") else {
        let prefix = content.trim_end();
        return format!(
            "{prefix}{}[features]\nhooks = true\n",
            if prefix.is_empty() { "" } else { "\n\n" }
        );
    };

    let next_section_index = lines
        .iter()
        .enumerate()
        .skip(features_index + 1)
        .find_map(|(index, line)| {
            let trimmed = line.trim();
            (trimmed.starts_with('[') && trimmed.ends_with(']')).then_some(index)
        })
        .unwrap_or(lines.len());

    let hook_line_index = lines
        .iter()
        .enumerate()
        .skip(features_index + 1)
        .take(next_section_index.saturating_sub(features_index + 1))
        .find_map(|(index, line)| line.trim_start().starts_with("hooks").then_some(index));

    if let Some(index) = hook_line_index {
        lines[index] = "hooks = true".to_string();
    } else {
        lines.insert(next_section_index, "hooks = true".to_string());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> CodexAdapter {
        CodexAdapter {
            config_root: PathBuf::from("/tmp/codex-test"),
            status: AdapterStatus::Available,
        }
    }

    #[test]
    fn injects_codex_nested_hooks_json_format() {
        let mut settings = serde_json::json!({
            "hooks": {
                "SessionStart": [
                    { "hooks": [{ "type": "command", "command": "/bin/other" }] },
                    { "hooks": [{ "type": "command", "command": "/Users/me/.agentbro/bin/agentbro-bridge --source codex" }] }
                ]
            }
        });

        CodexAdapter::inject_hooks_json(
            &mut settings,
            "/Users/me/.agentbro/bin/agentbro-bridge --source codex",
        );

        let session_start = settings["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start.len(), 2);
        assert_eq!(
            session_start[1]["hooks"][0]["command"],
            "/Users/me/.agentbro/bin/agentbro-bridge --source codex"
        );
        assert_eq!(
            settings["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"],
            CODEX_PERMISSION_TIMEOUT_SECONDS
        );
        assert!(settings["hooks"]["UserPromptSubmit"].is_array());
        assert!(settings["hooks"].get("SessionEnd").is_none());
        assert!(settings["hooks"].get("PostToolUseFailure").is_none());
        assert!(settings["hooks"].get("StopFailure").is_none());
    }

    #[test]
    fn enables_codex_hooks_feature_without_dropping_existing_config() {
        let content = r#"model = "gpt-5"

[features]
goals = true

[projects."/tmp/example"]
trust_level = "trusted"
"#;

        let updated = ensure_codex_hooks_feature(content);

        assert!(updated.contains("[features]"));
        assert!(updated.contains("goals = true"));
        assert!(updated.contains("hooks = true"));
        assert!(updated.contains("[projects.\"/tmp/example\"]"));
    }

    #[test]
    fn parses_codex_pascal_session_start_as_codex_session() {
        let event = adapter()
            .parse_event(&serde_json::json!({
                "agent": "codex",
                "event": "SessionStart",
                "session_id": "s1",
                "cwd": "/tmp/agent-island",
                "tty": "/dev/ttys001"
            }))
            .unwrap();

        match event {
            AgentEvent::SessionStart {
                agent_type,
                project,
                ..
            } => {
                assert_eq!(agent_type, "codex");
                assert_eq!(project, "agent-island");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parses_codex_tool_lifecycle() {
        let event = adapter()
            .parse_event(&serde_json::json!({
                "agent": "codex",
                "event": "PreToolUse",
                "session_id": "s1",
                "tool": "Bash",
                "tool_input": { "command": "pnpm test" }
            }))
            .unwrap();

        match event {
            AgentEvent::ToolUse {
                tool_name,
                tool_target,
                status,
                ..
            } => {
                assert_eq!(tool_name, "Bash");
                assert_eq!(tool_target.as_deref(), Some("pnpm test"));
                assert_eq!(status, "running");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn marks_codex_failed_post_tool_use_from_tool_response() {
        let event = adapter()
            .parse_event(&serde_json::json!({
                "agent": "codex",
                "event": "PostToolUse",
                "session_id": "s1",
                "tool_name": "Bash",
                "tool_input": { "command": "pnpm test" },
                "tool_response": { "exit_code": 1, "stderr": "failed" }
            }))
            .unwrap();

        match event {
            AgentEvent::ToolUse { status, .. } => {
                assert_eq!(status, "error");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn normalizes_codex_file_tools_with_targets() {
        let event = adapter()
            .parse_event(&serde_json::json!({
                "agent": "codex",
                "event": "PreToolUse",
                "session_id": "s1",
                "tool_name": "read_file",
                "tool_input": { "path": "src/components/notch/HoverList.tsx" }
            }))
            .unwrap();

        match event {
            AgentEvent::ToolUse {
                tool_name,
                tool_target,
                status,
                ..
            } => {
                assert_eq!(tool_name, "Read");
                assert_eq!(tool_target.as_deref(), Some("HoverList.tsx"));
                assert_eq!(status, "running");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn extracts_codex_patch_target_and_change_counts() {
        let event = adapter()
            .parse_event(&serde_json::json!({
                "agent": "codex",
                "event": "PreToolUse",
                "session_id": "s1",
                "tool_name": "apply_patch",
                "tool_input": {
                    "command": "*** Begin Patch\n*** Update File: src/components/overlay/OverlayFeedbackPanel.tsx\n@@\n-old\n+new\n+again\n*** End Patch\n"
                }
            }))
            .unwrap();

        match event {
            AgentEvent::ToolUse {
                tool_name,
                tool_target,
                ..
            } => {
                assert_eq!(tool_name, "Edit");
                assert_eq!(
                    tool_target.as_deref(),
                    Some("OverlayFeedbackPanel.tsx +2 -1")
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parses_codex_stop_last_assistant_message() {
        let event = adapter()
            .parse_event(&serde_json::json!({
                "agent": "codex",
                "event": "Stop",
                "session_id": "s1",
                "last_assistant_message": "Fixed the layout."
            }))
            .unwrap();

        match event {
            AgentEvent::AssistantResponseComplete { text, .. } => {
                assert_eq!(text, "Fixed the layout.");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
