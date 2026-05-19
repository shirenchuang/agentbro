// ConversationParser — Incremental JSONL conversation file parser
// Reads Claude Code conversation history from JSONL files,
// tracking file offset to only parse new lines on subsequent calls.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

// ── Parsed message types ────────────────────────────────────────

/// Role of a chat message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
}

/// A block of content within a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageBlock {
    /// Plain text content
    Text { text: String },
    /// Tool invocation by the assistant
    ToolUse {
        id: String,
        name: String,
        input: HashMap<String, String>,
    },
    /// Result of a tool invocation (from a user-role line)
    ToolResult {
        tool_use_id: String,
        content: Option<String>,
        is_error: bool,
    },
    /// Extended thinking block
    Thinking { thinking: String },
    /// Image block from multimodal user messages.
    Image { source: String },
    /// The conversation was interrupted
    Interrupted,
}

/// A single parsed chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedMessage {
    pub id: String,
    pub role: ChatRole,
    pub timestamp: Option<String>,
    pub blocks: Vec<MessageBlock>,
}

/// Result of an incremental parse
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementalParseResult {
    pub new_messages: Vec<ParsedMessage>,
    pub all_messages: Vec<ParsedMessage>,
    pub clear_detected: bool,
    /// Byte offset in the file after this parse (use for next streaming call)
    pub byte_offset: u64,
    /// Number of raw JSONL lines read in this batch
    pub lines_read: usize,
}

/// Cache TTL metadata inferred from the latest main-agent assistant JSONL entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheTtlInfo {
    pub timestamp_ms: i64,
    pub ttl_ms: i64,
}

/// Subagent metadata recovered from Claude Code JSONL transcripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptSubagentInfo {
    pub agent_id: String,
    pub launch_tool_use_id: Option<String>,
    pub name: Option<String>,
    pub agent_type: Option<String>,
    pub description: String,
    pub transcript_path: Option<String>,
    pub agent_transcript_path: Option<String>,
    pub last_assistant_message: Option<String>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub status: String,
    pub tools: Vec<String>,
}

// ── Parser ──────────────────────────────────────────────────────

/// Incremental JSONL conversation parser.
///
/// Tracks file offset so that repeated calls only parse newly-appended lines.
/// Handles malformed lines gracefully by skipping them with a warning log.
pub struct ConversationParser {
    file_path: PathBuf,
    last_offset: u64,
    messages: Vec<ParsedMessage>,
    seen_tool_ids: HashSet<String>,
}

impl ConversationParser {
    /// Create a new parser for the given JSONL file path.
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            last_offset: 0,
            messages: Vec::new(),
            seen_tool_ids: HashSet::new(),
        }
    }

    /// Return the file path this parser is tracking.
    pub fn file_path(&self) -> &PathBuf {
        &self.file_path
    }

    /// Current byte offset (position after the last successful parse).
    pub fn last_byte_offset(&self) -> u64 {
        self.last_offset
    }

    /// Parse from an explicit byte offset (for external streaming callers).
    /// Updates internal offset to `start_offset` before parsing new lines.
    pub fn parse_from_offset(
        &mut self,
        start_offset: u64,
    ) -> Result<IncrementalParseResult, std::io::Error> {
        self.last_offset = start_offset;
        self.parse_incremental()
    }

    /// Parse only new lines appended since the last call.
    /// Returns an `IncrementalParseResult` containing new messages,
    /// the full message list, and whether a `/clear` was detected.
    pub fn parse_incremental(&mut self) -> Result<IncrementalParseResult, std::io::Error> {
        let file = match File::open(&self.file_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(IncrementalParseResult {
                    new_messages: vec![],
                    all_messages: self.messages.clone(),
                    clear_detected: false,
                    byte_offset: self.last_offset,
                    lines_read: 0,
                });
            }
            Err(e) => return Err(e),
        };

        let file_size = file.metadata()?.len();

        // If file shrank (e.g. was recreated), reset state
        if file_size < self.last_offset {
            self.last_offset = 0;
            self.messages.clear();
            self.seen_tool_ids.clear();
        }

        // Nothing new
        if file_size == self.last_offset {
            return Ok(IncrementalParseResult {
                new_messages: vec![],
                all_messages: self.messages.clone(),
                clear_detected: false,
                byte_offset: self.last_offset,
                lines_read: 0,
            });
        }

        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(self.last_offset))?;

        let mut new_messages = Vec::new();
        let mut clear_detected = false;
        let mut lines_read: usize = 0;

        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            let bytes_read = reader.read_line(&mut line_buf)?;
            if bytes_read == 0 {
                break;
            }
            lines_read += 1;

            let line = line_buf.trim();
            if line.is_empty() {
                continue;
            }

            // Detect /clear command
            if line.contains("<command-name>/clear</command-name>") {
                self.messages.clear();
                self.seen_tool_ids.clear();
                clear_detected = true;
                new_messages.clear();
                continue;
            }

            // Parse the JSON line
            let json: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!(
                        "Skipping malformed JSONL line in {}: {}",
                        self.file_path.display(),
                        e
                    );
                    continue;
                }
            };

            if let Some(msg) = self.parse_line(&json) {
                new_messages.push(msg.clone());
                self.messages.push(msg);
            }
        }

        self.last_offset = file_size;

        Ok(IncrementalParseResult {
            new_messages,
            all_messages: self.messages.clone(),
            clear_detected,
            byte_offset: file_size,
            lines_read,
        })
    }

    /// Parse the entire file from scratch (ignores previous offset).
    /// Useful for initial load of a conversation.
    pub fn parse_full(&mut self) -> Result<Vec<ParsedMessage>, std::io::Error> {
        // Reset state
        self.last_offset = 0;
        self.messages.clear();
        self.seen_tool_ids.clear();

        let result = self.parse_incremental()?;
        Ok(result.all_messages)
    }

    /// Reset parser state (useful when conversation is cleared or reloaded).
    pub fn reset(&mut self) {
        self.last_offset = 0;
        self.messages.clear();
        self.seen_tool_ids.clear();
    }

    /// Get current messages without re-parsing.
    pub fn messages(&self) -> &[ParsedMessage] {
        &self.messages
    }

    // ── Internal parsing ────────────────────────────────────────

    /// Parse a single JSONL object into a `ParsedMessage`, if applicable.
    fn parse_line(&mut self, json: &serde_json::Value) -> Option<ParsedMessage> {
        let msg_type = json.get("type")?.as_str()?;

        if msg_type == "response_item" {
            return self.parse_codex_response_item(json);
        }

        // Only parse user and assistant message lines
        if msg_type != "user" && msg_type != "assistant" {
            return None;
        }

        // Skip meta messages (system injections, command wrappers, etc.)
        if json
            .get("isMeta")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return None;
        }

        let uuid = json
            .get("uuid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let timestamp = json
            .get("timestamp")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let message = json.get("message")?;
        let role = if msg_type == "user" {
            ChatRole::User
        } else {
            ChatRole::Assistant
        };

        let blocks = self.parse_content(message)?;

        if blocks.is_empty() {
            return None;
        }

        Some(ParsedMessage {
            id: uuid,
            role,
            timestamp,
            blocks,
        })
    }

    fn parse_codex_response_item(&mut self, json: &serde_json::Value) -> Option<ParsedMessage> {
        let payload = json.get("payload")?;
        let item_type = payload.get("type")?.as_str()?;
        let timestamp = json
            .get("timestamp")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        match item_type {
            "message" => {
                let role = match payload.get("role").and_then(|v| v.as_str())? {
                    "user" => ChatRole::User,
                    "assistant" => ChatRole::Assistant,
                    _ => return None,
                };
                let blocks = Self::parse_codex_message_content(payload.get("content")?)?;
                if blocks.is_empty() {
                    return None;
                }
                Some(ParsedMessage {
                    id: payload
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    role,
                    timestamp,
                    blocks,
                })
            }
            "function_call" => {
                let call_id = payload
                    .get("call_id")
                    .or_else(|| payload.get("callId"))
                    .or_else(|| payload.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = payload
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let input = Self::parse_codex_function_arguments(payload.get("arguments"));
                Some(ParsedMessage {
                    id: call_id.clone(),
                    role: ChatRole::Assistant,
                    timestamp,
                    blocks: vec![MessageBlock::ToolUse {
                        id: call_id,
                        name,
                        input,
                    }],
                })
            }
            "function_call_output" => {
                let call_id = payload
                    .get("call_id")
                    .or_else(|| payload.get("callId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let content = payload.get("output").map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                });
                let is_error = payload
                    .get("is_error")
                    .or_else(|| payload.get("isError"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                Some(ParsedMessage {
                    id: call_id.clone(),
                    role: ChatRole::User,
                    timestamp,
                    blocks: vec![MessageBlock::ToolResult {
                        tool_use_id: call_id,
                        content,
                        is_error,
                    }],
                })
            }
            "reasoning" => {
                let summary = payload.get("summary")?.as_array()?;
                let thinking = summary
                    .iter()
                    .filter(|item| {
                        item.get("type").and_then(|v| v.as_str()) == Some("summary_text")
                    })
                    .filter_map(|item| item.get("text").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                if thinking.is_empty() {
                    return None;
                }
                Some(ParsedMessage {
                    id: payload
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    role: ChatRole::Assistant,
                    timestamp,
                    blocks: vec![MessageBlock::Thinking { thinking }],
                })
            }
            _ => None,
        }
    }

    fn parse_codex_message_content(content: &serde_json::Value) -> Option<Vec<MessageBlock>> {
        let mut blocks = Vec::new();
        let arr = content.as_array()?;

        for block in arr {
            let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match block_type {
                "input_text" | "output_text" | "text" => {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        if text.starts_with("<environment_context>") {
                            continue;
                        }
                        blocks.push(MessageBlock::Text {
                            text: text.to_string(),
                        });
                    }
                }
                "input_image" | "image_url" | "image" => {
                    let source = block
                        .get("image_url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            block
                                .get("image_url")
                                .and_then(|v| v.get("url"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .or_else(|| {
                            block
                                .get("url")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .or_else(|| Self::parse_image_source(block));
                    if let Some(source) = source {
                        blocks.push(MessageBlock::Image { source });
                    }
                }
                _ => {}
            }
        }

        Some(blocks)
    }

    fn parse_codex_function_arguments(
        arguments: Option<&serde_json::Value>,
    ) -> HashMap<String, String> {
        let mut input = HashMap::new();
        match arguments {
            Some(serde_json::Value::String(raw)) => {
                match serde_json::from_str::<serde_json::Value>(raw) {
                    Ok(serde_json::Value::Object(map)) => Self::flatten_input(Some(&map)),
                    Ok(other) => {
                        input.insert("arguments".to_string(), other.to_string());
                        input
                    }
                    Err(_) => {
                        input.insert("arguments".to_string(), raw.clone());
                        input
                    }
                }
            }
            Some(serde_json::Value::Object(map)) => Self::flatten_input(Some(map)),
            Some(other) => {
                input.insert("arguments".to_string(), other.to_string());
                input
            }
            None => input,
        }
    }

    /// Parse the `message.content` field into a list of `MessageBlock`s.
    fn parse_content(&mut self, message: &serde_json::Value) -> Option<Vec<MessageBlock>> {
        let content = message.get("content")?;
        let mut blocks = Vec::new();

        if let Some(text) = content.as_str() {
            // Simple string content
            if text.starts_with("<command-name>")
                || text.starts_with("<local-command")
                || text.starts_with("Caveat:")
                || text.starts_with("<command-message>")
            {
                // Skip internal command messages
                return None;
            }
            if text.starts_with("[Request interrupted by user") {
                blocks.push(MessageBlock::Interrupted);
            } else {
                blocks.push(MessageBlock::Text {
                    text: text.to_string(),
                });
            }
        } else if let Some(arr) = content.as_array() {
            for block in arr {
                let block_type = match block.get("type").and_then(|v| v.as_str()) {
                    Some(t) => t,
                    None => continue,
                };

                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                            if text.starts_with("[Request interrupted by user") {
                                blocks.push(MessageBlock::Interrupted);
                            } else {
                                blocks.push(MessageBlock::Text {
                                    text: text.to_string(),
                                });
                            }
                        }
                    }
                    "tool_use" => {
                        let id = match block.get("id").and_then(|v| v.as_str()) {
                            Some(id) => id.to_string(),
                            None => continue,
                        };

                        // Deduplicate tool uses (Claude Code may repeat them)
                        if self.seen_tool_ids.contains(&id) {
                            continue;
                        }
                        self.seen_tool_ids.insert(id.clone());

                        let name = block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown")
                            .to_string();

                        let input =
                            Self::flatten_input(block.get("input").and_then(|v| v.as_object()));

                        blocks.push(MessageBlock::ToolUse { id, name, input });
                    }
                    "tool_result" => {
                        let tool_use_id = match block.get("tool_use_id").and_then(|v| v.as_str()) {
                            Some(id) => id.to_string(),
                            None => continue,
                        };

                        let content_str = block
                            .get("content")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        let is_error = block
                            .get("is_error")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);

                        blocks.push(MessageBlock::ToolResult {
                            tool_use_id,
                            content: content_str,
                            is_error,
                        });
                    }
                    "thinking" => {
                        if let Some(thinking) = block.get("thinking").and_then(|v| v.as_str()) {
                            blocks.push(MessageBlock::Thinking {
                                thinking: thinking.to_string(),
                            });
                        }
                    }
                    "image" => {
                        if let Some(source) = Self::parse_image_source(block) {
                            blocks.push(MessageBlock::Image { source });
                        }
                    }
                    "image_url" => {
                        if let Some(source) = block
                            .get("image_url")
                            .and_then(|v| v.get("url"))
                            .and_then(|v| v.as_str())
                            .or_else(|| block.get("url").and_then(|v| v.as_str()))
                        {
                            blocks.push(MessageBlock::Image {
                                source: source.to_string(),
                            });
                        }
                    }
                    _ => {
                        // Skip unknown block types gracefully
                    }
                }
            }
        }

        Some(blocks)
    }

    fn parse_image_source(block: &serde_json::Value) -> Option<String> {
        let source = block.get("source")?;
        if let Some(url) = source.get("url").and_then(|v| v.as_str()) {
            return Some(url.to_string());
        }
        if source.get("type").and_then(|v| v.as_str()) == Some("base64") {
            let media_type = source
                .get("media_type")
                .or_else(|| source.get("mediaType"))
                .and_then(|v| v.as_str())
                .unwrap_or("image/png");
            let data = source.get("data").and_then(|v| v.as_str())?;
            return Some(format!("data:{};base64,{}", media_type, data));
        }
        None
    }

    /// Flatten a JSON object into a HashMap<String, String> for tool inputs.
    fn flatten_input(
        input: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> HashMap<String, String> {
        let mut map = HashMap::new();
        if let Some(obj) = input {
            for (key, value) in obj {
                let str_val = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => value.to_string(),
                };
                map.insert(key.clone(), str_val);
            }
        }
        map
    }
}

// ── Session file discovery ──────────────────────────────────────

/// Discover the JSONL file path for a given session.
///
/// Claude Code stores conversations at:
///   `~/.claude/projects/<project-dir-hash>/<session-id>.jsonl`
///
/// The project directory hash is the cwd with `/` replaced by `-` and `.` replaced by `-`.
pub fn discover_session_file(session_id: &str, cwd: &str) -> Option<PathBuf> {
    discover_session_file_in_dirs(session_id, cwd, &all_projects_dirs())
}

/// Search for a session JSONL across multiple project directories.
pub fn discover_session_file_in_dirs(
    session_id: &str,
    cwd: &str,
    projects_dirs: &[PathBuf],
) -> Option<PathBuf> {
    let project_dir_name = cwd.replace(['/', '.'], "-");

    for projects_dir in projects_dirs {
        let session_file = projects_dir
            .join(&project_dir_name)
            .join(format!("{}.jsonl", session_id));

        if session_file.exists() {
            return Some(session_file);
        }

        // Fallback: search all project subdirectories for this session ID
        if let Ok(entries) = std::fs::read_dir(projects_dir) {
            for entry in entries.flatten() {
                let candidate = entry.path().join(format!("{}.jsonl", session_id));
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

/// Discover a Codex JSONL rollout file for a session id.
///
/// Codex stores transcripts under:
///   `~/.codex/sessions/<yyyy>/<mm>/<dd>/rollout-...-<session-id>.jsonl`
/// and may later move them to `~/.codex/archived_sessions`.
pub fn discover_codex_session_file(session_id: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let codex_root = home.join(".codex");
    let roots = [
        codex_root.join("sessions"),
        codex_root.join("archived_sessions"),
    ];

    for root in roots {
        if let Some(path) = find_codex_session_file_in_dir(&root, session_id) {
            return Some(path);
        }
    }

    None
}

fn find_codex_session_file_in_dir(root: &Path, session_id: &str) -> Option<PathBuf> {
    if !root.is_dir() {
        return None;
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            let Some(file_name) = path.file_name().and_then(|v| v.to_str()) else {
                continue;
            };
            if file_name.ends_with(".jsonl")
                && file_name.starts_with("rollout-")
                && file_name.contains(session_id)
            {
                return Some(path);
            }
        }
    }

    None
}

/// Extract the latest main-agent assistant text from a Claude/Codex JSONL transcript.
pub fn extract_latest_assistant_text(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut latest = None;

    for line in reader.lines() {
        let Ok(line) = line else {
            continue;
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(text) = assistant_text_from_json(&json) {
            latest = Some(text);
        }
    }

    latest
}

#[derive(Debug, Clone)]
struct PendingSubagentTool {
    tool_use_id: String,
    name: Option<String>,
    description: String,
    prompt: String,
    agent_type: Option<String>,
    started_at: i64,
}

/// Recover Claude Code subagent activity from a main session transcript.
///
/// Claude Code records subagent launches as assistant `tool_use` blocks named
/// `Agent` (or `Task` in some builds), then records completion metadata on the
/// matching user `tool_result` line under `toolUseResult`. The sidechain
/// transcript lives beside the main file at:
/// `<session-id>/subagents/agent-<agent-id>.jsonl`.
pub fn extract_subagents_from_transcript(path: &Path) -> Vec<TranscriptSubagentInfo> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };
    let reader = BufReader::new(file);
    let main_transcript_path = path.to_string_lossy().to_string();
    let session_id = path.file_stem().and_then(|v| v.to_str()).unwrap_or("");
    let mut pending: HashMap<String, PendingSubagentTool> = HashMap::new();
    let mut subagents: Vec<TranscriptSubagentInfo> = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        for tool in subagent_tool_uses_from_json(&json) {
            pending.insert(tool.tool_use_id.clone(), tool);
        }

        if let Some(mut subagent) =
            completed_subagent_from_json(&json, path, session_id, &main_transcript_path, &pending)
        {
            pending.remove(&subagent.agent_id);
            if let Some(tool_use_id) = tool_result_id_from_json(&json) {
                pending.remove(&tool_use_id);
            }
            enrich_subagent_from_sidechain(path, session_id, &mut subagent);
            upsert_transcript_subagent(&mut subagents, subagent);
        }
    }

    for tool in pending.into_values() {
        upsert_transcript_subagent(
            &mut subagents,
            TranscriptSubagentInfo {
                agent_id: tool.tool_use_id,
                launch_tool_use_id: None,
                name: tool.name,
                agent_type: tool.agent_type,
                description: choose_subagent_description(&tool.description, &tool.prompt),
                transcript_path: Some(main_transcript_path.clone()),
                agent_transcript_path: None,
                last_assistant_message: None,
                started_at: tool.started_at,
                completed_at: None,
                status: "running".to_string(),
                tools: Vec::new(),
            },
        );
    }

    subagents.sort_by(|a, b| {
        a.started_at
            .cmp(&b.started_at)
            .then(a.agent_id.cmp(&b.agent_id))
    });
    subagents
}

fn subagent_tool_uses_from_json(json: &serde_json::Value) -> Vec<PendingSubagentTool> {
    if json.get("type").and_then(|v| v.as_str()) != Some("assistant") {
        return Vec::new();
    }
    if json
        .get("isSidechain")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Vec::new();
    }

    let timestamp = timestamp_seconds(json);
    json.get("message")
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_array())
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(|v| v.as_str()) == Some("tool_use"))
        .filter(|block| {
            matches!(
                block.get("name").and_then(|v| v.as_str()),
                Some("Agent" | "Task")
            )
        })
        .filter_map(|block| {
            let tool_use_id = block.get("id").and_then(|v| v.as_str())?.to_string();
            let input = block.get("input").and_then(|v| v.as_object());
            let description = input
                .and_then(|obj| obj.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let prompt = input
                .and_then(|obj| obj.get("prompt"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let agent_type = input
                .and_then(|obj| {
                    obj.get("agent_type")
                        .or_else(|| obj.get("agentType"))
                        .or_else(|| obj.get("subagent_type"))
                        .or_else(|| obj.get("subagentType"))
                })
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let name = input
                .and_then(|obj| {
                    obj.get("name")
                        .or_else(|| obj.get("agent_name"))
                        .or_else(|| obj.get("agentName"))
                })
                .and_then(|v| v.as_str())
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty());

            Some(PendingSubagentTool {
                tool_use_id,
                name,
                description,
                prompt,
                agent_type,
                started_at: timestamp,
            })
        })
        .collect()
}

fn completed_subagent_from_json(
    json: &serde_json::Value,
    main_path: &Path,
    session_id: &str,
    main_transcript_path: &str,
    pending: &HashMap<String, PendingSubagentTool>,
) -> Option<TranscriptSubagentInfo> {
    if json.get("type").and_then(|v| v.as_str()) != Some("user") {
        return None;
    }

    let result = json.get("toolUseResult")?;
    let agent_id = result.get("agentId").and_then(|v| v.as_str())?.to_string();
    let tool_use_id = tool_result_id_from_json(json);
    let pending_tool = tool_use_id.as_ref().and_then(|id| pending.get(id));
    let raw_status = result
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("completed");
    let status = if raw_status.eq_ignore_ascii_case("completed")
        || raw_status.eq_ignore_ascii_case("success")
    {
        "completed"
    } else {
        "error"
    }
    .to_string();
    let prompt = result.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
    let description = pending_tool
        .map(|tool| choose_subagent_description(&tool.description, &tool.prompt))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| choose_subagent_description("", prompt));
    let started_at = pending_tool
        .map(|tool| tool.started_at)
        .unwrap_or_else(|| timestamp_seconds(json));
    let last_assistant_message = tool_use_result_text(result);
    let agent_type = result
        .get("agentType")
        .or_else(|| result.get("agent_type"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .or_else(|| pending_tool.and_then(|tool| tool.agent_type.clone()));
    let name = result
        .get("name")
        .or_else(|| result.get("agentName"))
        .or_else(|| result.get("agent_name"))
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| pending_tool.and_then(|tool| tool.name.clone()));

    Some(TranscriptSubagentInfo {
        agent_id: agent_id.clone(),
        launch_tool_use_id: tool_use_id,
        name,
        agent_type,
        description,
        transcript_path: Some(main_transcript_path.to_string()),
        agent_transcript_path: sidechain_transcript_path(main_path, session_id, &agent_id)
            .filter(|path| path.exists())
            .map(|path| path.to_string_lossy().to_string()),
        last_assistant_message,
        started_at,
        completed_at: Some(timestamp_seconds(json)),
        status,
        tools: Vec::new(),
    })
}

fn enrich_subagent_from_sidechain(
    main_path: &Path,
    session_id: &str,
    subagent: &mut TranscriptSubagentInfo,
) {
    let Some(agent_path) = sidechain_transcript_path(main_path, session_id, &subagent.agent_id)
    else {
        return;
    };

    if subagent.agent_transcript_path.is_none() && agent_path.exists() {
        subagent.agent_transcript_path = Some(agent_path.to_string_lossy().to_string());
    }

    if let Some(meta) = read_subagent_meta(&agent_path) {
        if subagent.name.is_none() {
            subagent.name = meta.name;
        }
        if subagent.agent_type.is_none() {
            subagent.agent_type = meta.agent_type;
        }
        if subagent.description.is_empty() {
            subagent.description = meta.description.unwrap_or_default();
        }
    }

    if agent_path.exists() {
        let (first_ts, last_ts, latest_assistant, tools) = scan_sidechain_transcript(&agent_path);
        if subagent.started_at == 0 {
            if let Some(ts) = first_ts {
                subagent.started_at = ts;
            }
        }
        if subagent.completed_at.is_none() {
            subagent.completed_at = last_ts;
        }
        if subagent.last_assistant_message.is_none() {
            subagent.last_assistant_message = latest_assistant;
        }
        if subagent.tools.is_empty() {
            subagent.tools = tools;
        }
    }
}

struct SubagentMeta {
    name: Option<String>,
    agent_type: Option<String>,
    description: Option<String>,
}

fn read_subagent_meta(agent_path: &Path) -> Option<SubagentMeta> {
    let file_name = agent_path.file_name()?.to_str()?;
    let meta_path = agent_path.with_file_name(file_name.replace(".jsonl", ".meta.json"));
    let raw = std::fs::read_to_string(meta_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some(SubagentMeta {
        name: json
            .get("name")
            .or_else(|| json.get("agentName"))
            .or_else(|| json.get("agent_name"))
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        agent_type: json
            .get("agentType")
            .or_else(|| json.get("agent_type"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        description: json
            .get("description")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
    })
}

fn scan_sidechain_transcript(
    path: &Path,
) -> (Option<i64>, Option<i64>, Option<String>, Vec<String>) {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return (None, None, None, Vec::new()),
    };
    let reader = BufReader::new(file);
    let mut first_ts = None;
    let mut last_ts = None;
    let mut latest_assistant = None;
    let mut tools = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let ts = timestamp_seconds(&json);
        if ts > 0 {
            first_ts.get_or_insert(ts);
            last_ts = Some(ts);
        }

        if json.get("type").and_then(|v| v.as_str()) == Some("assistant") {
            if let Some(text) = json
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(text_from_content)
            {
                latest_assistant = Some(text);
            }
            if let Some(content) = json
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(|content| content.as_array())
            {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                        if let Some(name) = block.get("name").and_then(|v| v.as_str()) {
                            if !tools.iter().any(|tool| tool == name) {
                                tools.push(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    (first_ts, last_ts, latest_assistant, tools)
}

fn sidechain_transcript_path(
    main_path: &Path,
    session_id: &str,
    agent_id: &str,
) -> Option<PathBuf> {
    if session_id.is_empty() || agent_id.is_empty() {
        return None;
    }
    Some(
        main_path
            .parent()?
            .join(session_id)
            .join("subagents")
            .join(format!("agent-{}.jsonl", agent_id)),
    )
}

fn choose_subagent_description(description: &str, prompt: &str) -> String {
    let description = description.trim();
    if !description.is_empty() {
        return description.to_string();
    }
    prompt.lines().next().unwrap_or(prompt).trim().to_string()
}

fn tool_result_id_from_json(json: &serde_json::Value) -> Option<String> {
    json.get("message")
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_array())
        .and_then(|blocks| {
            blocks.iter().find_map(|block| {
                if block.get("type").and_then(|v| v.as_str()) != Some("tool_result") {
                    return None;
                }
                block
                    .get("tool_use_id")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string())
            })
        })
}

fn tool_use_result_text(result: &serde_json::Value) -> Option<String> {
    result
        .get("content")
        .and_then(|content| content.as_array())
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(|v| v.as_str()))
                .filter(|text| !text.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|text| !text.trim().is_empty())
}

fn timestamp_seconds(json: &serde_json::Value) -> i64 {
    json.get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(|timestamp| chrono::DateTime::parse_from_rfc3339(timestamp).ok())
        .map(|timestamp| timestamp.timestamp())
        .unwrap_or(0)
}

fn upsert_transcript_subagent(
    subagents: &mut Vec<TranscriptSubagentInfo>,
    subagent: TranscriptSubagentInfo,
) {
    if let Some(existing) = subagents
        .iter_mut()
        .find(|item| item.agent_id == subagent.agent_id)
    {
        *existing = subagent;
    } else {
        subagents.push(subagent);
    }
}

fn assistant_text_from_json(json: &serde_json::Value) -> Option<String> {
    match json.get("type").and_then(|v| v.as_str())? {
        "assistant" => {
            if json
                .get("isSidechain")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || json.get("agentId").is_some()
            {
                return None;
            }
            let message = json.get("message")?;
            if message
                .get("role")
                .and_then(|v| v.as_str())
                .is_some_and(|role| role != "assistant")
            {
                return None;
            }
            text_from_content(message.get("content")?)
        }
        "response_item" => {
            let payload = json.get("payload")?;
            if payload.get("type").and_then(|v| v.as_str()) != Some("message")
                || payload.get("role").and_then(|v| v.as_str()) != Some("assistant")
            {
                return None;
            }
            text_from_content(payload.get("content")?)
        }
        _ => None,
    }
}

fn user_text_from_json(json: &serde_json::Value) -> Option<String> {
    match json.get("type").and_then(|v| v.as_str())? {
        "user" => {
            if json
                .get("isMeta")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return None;
            }
            let message = json.get("message")?;
            if message
                .get("role")
                .and_then(|v| v.as_str())
                .is_some_and(|role| role != "user")
            {
                return None;
            }
            let text = extract_text_from_content(message.get("content")?);
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        "response_item" => {
            let payload = json.get("payload")?;
            if payload.get("type").and_then(|v| v.as_str()) != Some("message")
                || payload.get("role").and_then(|v| v.as_str()) != Some("user")
            {
                return None;
            }
            let text = extract_text_from_content(payload.get("content")?);
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        _ => None,
    }
}

fn text_from_content(content: &serde_json::Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return useful_text_block(text).map(|text| text.to_string());
    }

    let texts = content
        .as_array()?
        .iter()
        .filter_map(|block| {
            let block_type = block.get("type").and_then(|v| v.as_str())?;
            if !matches!(block_type, "text" | "output_text" | "input_text") {
                return None;
            }
            block
                .get("text")
                .and_then(|v| v.as_str())
                .and_then(useful_text_block)
                .map(|text| text.to_string())
        })
        .collect::<Vec<_>>();

    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n\n"))
    }
}

fn useful_text_block(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("<command-name>")
        || trimmed.starts_with("<local-command")
        || trimmed.starts_with("<command-message>")
        || trimmed.starts_with("<environment_context>")
        || trimmed.starts_with("[Request interrupted by user")
        || trimmed.starts_with("Caveat:")
    {
        None
    } else {
        Some(trimmed)
    }
}

/// Read the tail of a Claude Code JSONL transcript and infer cache TTL.
///
/// Claude records cache-creation usage on assistant messages. Evolab uses the
/// latest non-sidechain, non-subagent assistant entry to show whether the next
/// main-agent request can still reuse the prompt cache.
pub fn extract_cache_ttl_info(file_path: &Path) -> Option<CacheTtlInfo> {
    const TAIL_BYTES: u64 = 20 * 1024;

    let mut file = File::open(file_path).ok()?;
    let file_size = file.metadata().ok()?.len();
    let start = file_size.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;

    let mut buf = String::new();
    file.read_to_string(&mut buf).ok()?;

    for line in buf.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let entry: serde_json::Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };

        if entry.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        if entry
            .get("isSidechain")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        if entry.get("agentId").is_some() {
            continue;
        }

        let Some(cache_creation) = entry
            .get("message")
            .and_then(|v| v.get("usage"))
            .and_then(|v| v.get("cache_creation"))
        else {
            continue;
        };

        let timestamp = entry.get("timestamp").and_then(|v| v.as_str())?;
        let timestamp_ms = chrono::DateTime::parse_from_rfc3339(timestamp)
            .ok()?
            .timestamp_millis();
        let has_one_hour_cache = cache_creation
            .get("ephemeral_1h_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            > 0;

        return Some(CacheTtlInfo {
            timestamp_ms,
            ttl_ms: if has_one_hour_cache {
                3_600_000
            } else {
                300_000
            },
        });
    }

    None
}

/// Get the default Claude projects directory (~/.claude/projects).
pub fn claude_projects_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let dir = home.join(".claude").join("projects");
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

fn known_claude_engine_projects_dirs() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    [home
        .join(".codefuse")
        .join("engine")
        .join("cc")
        .join("projects")]
    .into_iter()
    .filter(|dir| dir.is_dir())
    .collect()
}

/// Collect all known projects directories (default + custom engine instances).
/// Used by file watcher and session discovery.
pub fn all_projects_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = claude_projects_dir() {
        dirs.push(d);
    }
    for dir in known_claude_engine_projects_dirs() {
        if !dirs.iter().any(|existing| existing == &dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// Collect projects directories from a set of config roots.
pub fn projects_dirs_from_roots(config_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root in config_roots {
        let dir = root.join("projects");
        if dir.is_dir() {
            dirs.push(dir);
        }
    }
    dirs
}

// ── Startup session discovery ─────────────────────────────────

/// A session discovered by scanning JSONL files at startup.
#[derive(Debug, Clone)]
pub struct DiscoveredSession {
    pub session_id: String,
    pub cwd: String,
    pub project: String,
    pub session_title: Option<String>,
    pub projects_dir: PathBuf,
    pub modified_at: i64,
}

/// Scan projects directories for recently-active JSONL files.
///
/// Returns sessions whose JSONL file was modified within `max_age`.
/// For each file, reads the first ~30 lines to extract session metadata
/// and the first user message as a title.
pub fn discover_active_sessions(max_age: Duration) -> Vec<DiscoveredSession> {
    discover_active_sessions_in_dirs(max_age, &all_projects_dirs())
}

/// Scan specific projects directories for recently-active sessions.
pub fn discover_active_sessions_in_dirs(
    max_age: Duration,
    projects_dirs: &[PathBuf],
) -> Vec<DiscoveredSession> {
    let cutoff = std::time::SystemTime::now()
        .checked_sub(max_age)
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    let mut results = Vec::new();

    for projects_dir in projects_dirs {
        let project_entries = match std::fs::read_dir(projects_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for project_entry in project_entries.flatten() {
            let project_path = project_entry.path();
            if !project_path.is_dir() {
                continue;
            }

            let jsonl_entries = match std::fs::read_dir(&project_path) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for file_entry in jsonl_entries.flatten() {
                let file_path = file_entry.path();

                // Only .jsonl files
                if file_path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }

                // Skip subagent files
                if file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with("agent-"))
                    .unwrap_or(false)
                {
                    continue;
                }

                // Check modification time
                let metadata = match std::fs::metadata(&file_path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let modified = match metadata.modified() {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                if modified < cutoff {
                    continue;
                }

                // Parse the first ~30 lines to extract metadata
                if let Some(mut session) = parse_session_header(&file_path) {
                    session.projects_dir = projects_dir.clone();
                    session.modified_at = modified
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .map(|duration| duration.as_secs() as i64)
                        .unwrap_or_else(|_| chrono::Utc::now().timestamp());
                    results.push(session);
                }
            }
        }
    }

    results
}

/// Parse the first ~30 lines of a JSONL file to extract session metadata.
fn parse_session_header(file_path: &std::path::Path) -> Option<DiscoveredSession> {
    let file = File::open(file_path).ok()?;
    let reader = BufReader::new(file);

    let mut session_id: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut first_user_text: Option<String> = None;

    // Also try to extract session_id from filename
    let filename_id = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());

    for (i, line) in reader.lines().enumerate() {
        if i >= 30 {
            break;
        }
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let json: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Extract sessionId
        if session_id.is_none() {
            if let Some(sid) = json.get("sessionId").and_then(|v| v.as_str()) {
                session_id = Some(sid.to_string());
            }
        }

        // Extract cwd
        if cwd.is_none() {
            if let Some(c) = json.get("cwd").and_then(|v| v.as_str()) {
                cwd = Some(c.to_string());
            }
        }

        // Extract first user message text as title
        if first_user_text.is_none() {
            if let Some(text) = user_text_from_json(&json) {
                first_user_text = Some(text);
            }
        }

        // Stop early if we have everything
        if session_id.is_some() && cwd.is_some() && first_user_text.is_some() {
            break;
        }
    }

    let sid = session_id.or(filename_id)?;
    let cwd_str = cwd.unwrap_or_default();
    let project = cwd_str.rsplit('/').next().unwrap_or(&cwd_str).to_string();

    Some(DiscoveredSession {
        session_id: sid,
        cwd: cwd_str,
        project,
        session_title: first_user_text,
        projects_dir: PathBuf::new(),
        modified_at: chrono::Utc::now().timestamp(),
    })
}

/// Extract plain text from a message content field (string or array of blocks).
/// Truncates to ~80 chars for use as a session title.
fn extract_text_from_content(content: &serde_json::Value) -> String {
    let raw = if let Some(text) = content.as_str() {
        // Simple string content — skip internal commands
        if text.starts_with("<command-name>")
            || text.starts_with("<local-command")
            || text.starts_with("Caveat:")
        {
            return String::new();
        }
        text.to_string()
    } else if let Some(arr) = content.as_array() {
        // Array of blocks — find the first text block
        let mut found = String::new();
        for block in arr {
            if matches!(
                block.get("type").and_then(|v| v.as_str()),
                Some("text" | "input_text" | "output_text")
            ) {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    if !text.starts_with("<command-name>") && !text.starts_with("[Image") {
                        found = text.to_string();
                        break;
                    }
                }
            }
        }
        found
    } else {
        return String::new();
    };

    // Clean up: remove [Image #N] prefixes, trim, and truncate
    let cleaned = raw.lines().next().unwrap_or(&raw).trim().to_string();

    if cleaned.len() > 80 {
        // Find a valid char boundary at or before byte 77
        let mut end = 77;
        while !cleaned.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &cleaned[..end])
    } else {
        cleaned
    }
}

/// Extract session title from a JSONL file by finding the first user message.
/// Lightweight version that only looks for the title, skipping other metadata.
pub fn extract_session_title(file_path: &std::path::Path) -> Option<String> {
    let file = File::open(file_path).ok()?;
    let reader = BufReader::new(file);

    for (i, line) in reader.lines().enumerate() {
        if i >= 50 {
            break;
        }
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let json: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(text) = user_text_from_json(&json) {
            return Some(text);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp_jsonl(name: &str, content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "agentbro-{name}-{}-{}.jsonl",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::write(&path, content).expect("write temp jsonl");
        path
    }

    #[test]
    fn extracts_latest_main_assistant_text() {
        let path = write_temp_jsonl(
            "latest-assistant",
            r#"{"type":"assistant","isSidechain":true,"message":{"role":"assistant","content":[{"type":"text","text":"sidechain reply"}]}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hi! How can I help you today?"}]}}
"#,
        );

        assert_eq!(
            extract_latest_assistant_text(&path).as_deref(),
            Some("Hi! How can I help you today?")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn extracts_codex_response_item_user_as_session_title() {
        let path = write_temp_jsonl(
            "codex-title",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Fix Codex session list layout"}]}}
{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done"}]}}
"#,
        );

        assert_eq!(
            extract_session_title(&path).as_deref(),
            Some("Fix Codex session list layout")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_flatten_input_strings() {
        let mut map = serde_json::Map::new();
        map.insert("command".into(), serde_json::json!("ls -la"));
        map.insert("timeout".into(), serde_json::json!(30));
        map.insert("verbose".into(), serde_json::json!(true));

        let result = ConversationParser::flatten_input(Some(&map));
        assert_eq!(result.get("command").unwrap(), "ls -la");
        assert_eq!(result.get("timeout").unwrap(), "30");
        assert_eq!(result.get("verbose").unwrap(), "true");
    }

    #[test]
    fn test_parse_user_text_message() {
        let line = serde_json::json!({
            "type": "user",
            "uuid": "test-uuid-1",
            "timestamp": "2026-01-01T00:00:00.000Z",
            "message": {
                "role": "user",
                "content": "Hello, world!"
            }
        });

        let mut parser = ConversationParser::new(PathBuf::from("/tmp/test.jsonl"));
        let msg = parser.parse_line(&line).unwrap();
        assert_eq!(msg.role, ChatRole::User);
        assert_eq!(msg.id, "test-uuid-1");
        assert_eq!(msg.blocks.len(), 1);
        match &msg.blocks[0] {
            MessageBlock::Text { text } => assert_eq!(text, "Hello, world!"),
            _ => panic!("Expected Text block"),
        }
    }

    #[test]
    fn test_parse_codex_user_and_assistant_messages() {
        let user_line = serde_json::json!({
            "timestamp": "2026-01-01T00:00:00.000Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": "hello codex" }]
            }
        });
        let assistant_line = serde_json::json!({
            "timestamp": "2026-01-01T00:00:01.000Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "hello back" }]
            }
        });

        let mut parser = ConversationParser::new(PathBuf::from("/tmp/test.jsonl"));
        let user = parser.parse_line(&user_line).unwrap();
        let assistant = parser.parse_line(&assistant_line).unwrap();

        assert_eq!(user.role, ChatRole::User);
        assert_eq!(assistant.role, ChatRole::Assistant);
        match &user.blocks[0] {
            MessageBlock::Text { text } => assert_eq!(text, "hello codex"),
            _ => panic!("Expected Text block"),
        }
        match &assistant.blocks[0] {
            MessageBlock::Text { text } => assert_eq!(text, "hello back"),
            _ => panic!("Expected Text block"),
        }
    }

    #[test]
    fn test_parse_codex_skips_environment_context() {
        let line = serde_json::json!({
            "timestamp": "2026-01-01T00:00:00.000Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{
                    "type": "input_text",
                    "text": "<environment_context>\n  <cwd>/tmp</cwd>\n</environment_context>"
                }]
            }
        });

        let mut parser = ConversationParser::new(PathBuf::from("/tmp/test.jsonl"));
        assert!(parser.parse_line(&line).is_none());
    }

    #[test]
    fn test_parse_codex_tool_call_and_output() {
        let call_line = serde_json::json!({
            "timestamp": "2026-01-01T00:00:00.000Z",
            "type": "response_item",
            "payload": {
                "type": "function_call",
                "name": "exec_command",
                "call_id": "call_123",
                "arguments": "{\"cmd\":\"ls\"}"
            }
        });
        let output_line = serde_json::json!({
            "timestamp": "2026-01-01T00:00:01.000Z",
            "type": "response_item",
            "payload": {
                "type": "function_call_output",
                "call_id": "call_123",
                "output": "file.txt\n"
            }
        });

        let mut parser = ConversationParser::new(PathBuf::from("/tmp/test.jsonl"));
        let call = parser.parse_line(&call_line).unwrap();
        let output = parser.parse_line(&output_line).unwrap();

        assert_eq!(call.role, ChatRole::Assistant);
        match &call.blocks[0] {
            MessageBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "call_123");
                assert_eq!(name, "exec_command");
                assert_eq!(input.get("cmd").unwrap(), "ls");
            }
            _ => panic!("Expected ToolUse block"),
        }
        assert_eq!(output.role, ChatRole::User);
        match &output.blocks[0] {
            MessageBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                assert_eq!(tool_use_id, "call_123");
                assert_eq!(content.as_deref(), Some("file.txt\n"));
                assert!(!is_error);
            }
            _ => panic!("Expected ToolResult block"),
        }
    }

    #[test]
    fn test_find_codex_session_file_in_dir() {
        let session_id = "019d1a08-a24d-7ef0-a7ed-c3a84a84704a";
        let root = std::env::temp_dir().join(format!(
            "agentbro-codex-sessions-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let day_dir = root.join("2026").join("03").join("23");
        std::fs::create_dir_all(&day_dir).expect("create codex session dir");
        let file_path = day_dir.join(format!("rollout-2026-03-23T17-31-06-{session_id}.jsonl"));
        std::fs::write(&file_path, "").expect("write codex session");

        assert_eq!(
            find_codex_session_file_in_dir(&root, session_id),
            Some(file_path.clone())
        );

        let _ = std::fs::remove_file(file_path);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn test_skip_meta_messages() {
        let line = serde_json::json!({
            "type": "user",
            "uuid": "test-meta",
            "isMeta": true,
            "message": {
                "role": "user",
                "content": "system injection"
            }
        });

        let mut parser = ConversationParser::new(PathBuf::from("/tmp/test.jsonl"));
        assert!(parser.parse_line(&line).is_none());
    }

    #[test]
    fn test_skip_command_messages() {
        let line = serde_json::json!({
            "type": "user",
            "uuid": "test-cmd",
            "message": {
                "role": "user",
                "content": "<command-name>/clear</command-name>"
            }
        });

        let mut parser = ConversationParser::new(PathBuf::from("/tmp/test.jsonl"));
        assert!(parser.parse_line(&line).is_none());
    }

    #[test]
    fn test_parse_assistant_with_tool_use() {
        let line = serde_json::json!({
            "type": "assistant",
            "uuid": "test-tool",
            "timestamp": "2026-01-01T00:00:01.000Z",
            "message": {
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "Let me check that."},
                    {
                        "type": "tool_use",
                        "id": "tool-123",
                        "name": "Bash",
                        "input": {"command": "ls -la"}
                    }
                ]
            }
        });

        let mut parser = ConversationParser::new(PathBuf::from("/tmp/test.jsonl"));
        let msg = parser.parse_line(&line).unwrap();
        assert_eq!(msg.role, ChatRole::Assistant);
        assert_eq!(msg.blocks.len(), 2);

        match &msg.blocks[1] {
            MessageBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "tool-123");
                assert_eq!(name, "Bash");
                assert_eq!(input.get("command").unwrap(), "ls -la");
            }
            _ => panic!("Expected ToolUse block"),
        }
    }

    #[test]
    fn test_extract_cache_ttl_info_uses_latest_main_agent_cache_creation() {
        let path = write_temp_jsonl(
            "cache-ttl-main",
            r#"{"type":"assistant","timestamp":"2026-04-23T08:00:00.000Z","message":{"usage":{"cache_creation":{"ephemeral_5m_input_tokens":128}}}}
{"type":"assistant","timestamp":"2026-04-23T08:10:00.000Z","isSidechain":true,"message":{"usage":{"cache_creation":{"ephemeral_1h_input_tokens":64}}}}
{"type":"assistant","timestamp":"2026-04-23T08:22:54.251Z","message":{"usage":{"cache_creation":{"ephemeral_1h_input_tokens":256}}}}
"#,
        );

        let info = extract_cache_ttl_info(&path).expect("cache TTL info");
        assert_eq!(info.timestamp_ms, 1776932574251);
        assert_eq!(info.ttl_ms, 3_600_000);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn extracts_subagents_from_claude_agent_tool_results() {
        let root = std::env::temp_dir().join(format!(
            "agentbro-subagents-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let session_id = "session-abc";
        let subagent_dir = root.join(session_id).join("subagents");
        std::fs::create_dir_all(&subagent_dir).expect("create subagent dir");
        let main_path = root.join(format!("{session_id}.jsonl"));
        let agent_path = subagent_dir.join("agent-a123.jsonl");
        let meta_path = subagent_dir.join("agent-a123.meta.json");

        std::fs::write(
            &main_path,
            r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-05-18T13:37:30.255Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"Agent","input":{"description":"计算 1+1 (agent 1)","prompt":"请计算 1+1 等于几","name":"calc-a"}}]}}
{"type":"user","timestamp":"2026-05-18T13:37:34.729Z","message":{"role":"user","content":[{"tool_use_id":"tool-1","type":"tool_result","content":[{"type":"text","text":"2"}]}]},"toolUseResult":{"status":"completed","prompt":"请计算 1+1 等于几","agentId":"a123","agentType":"general-purpose","content":[{"type":"text","text":"2"}],"totalToolUseCount":0}}
"#,
        )
        .expect("write main transcript");
        std::fs::write(
            &agent_path,
            r#"{"isSidechain":true,"agentId":"a123","type":"user","timestamp":"2026-05-18T13:37:30.730Z","message":{"role":"user","content":"请计算 1+1 等于几"}}
{"isSidechain":true,"agentId":"a123","type":"assistant","timestamp":"2026-05-18T13:37:33.604Z","message":{"role":"assistant","content":[{"type":"text","text":"1+1 = 2"}]}}
"#,
        )
        .expect("write sidechain transcript");
        std::fs::write(
            &meta_path,
            r#"{"agentType":"general-purpose","description":"计算 1+1 (agent 1)","name":"calc-a"}"#,
        )
        .expect("write meta");

        let subagents = extract_subagents_from_transcript(&main_path);
        assert_eq!(subagents.len(), 1);
        assert_eq!(subagents[0].agent_id, "a123");
        assert_eq!(subagents[0].name.as_deref(), Some("calc-a"));
        assert_eq!(subagents[0].agent_type.as_deref(), Some("general-purpose"));
        assert_eq!(subagents[0].description, "计算 1+1 (agent 1)");
        assert_eq!(subagents[0].status, "completed");
        assert_eq!(subagents[0].last_assistant_message.as_deref(), Some("2"));
        assert_eq!(
            subagents[0].agent_transcript_path.as_deref(),
            Some(agent_path.to_string_lossy().as_ref())
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn test_extract_cache_ttl_info_defaults_to_five_minutes() {
        let path = write_temp_jsonl(
            "cache-ttl-5m",
            r#"{"type":"assistant","timestamp":"2026-04-23T08:00:00.000Z","agentId":"agent-1","message":{"usage":{"cache_creation":{"ephemeral_1h_input_tokens":64}}}}
{"type":"assistant","timestamp":"2026-04-23T08:22:54.251Z","message":{"usage":{"cache_creation":{"ephemeral_5m_input_tokens":128}}}}
"#,
        );

        let info = extract_cache_ttl_info(&path).expect("cache TTL info");
        assert_eq!(info.timestamp_ms, 1776932574251);
        assert_eq!(info.ttl_ms, 300_000);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_tool_use_deduplication() {
        let line = serde_json::json!({
            "type": "assistant",
            "uuid": "test-dedup",
            "message": {
                "role": "assistant",
                "content": [
                    {
                        "type": "tool_use",
                        "id": "tool-dup",
                        "name": "Read",
                        "input": {"file_path": "/tmp/a.txt"}
                    }
                ]
            }
        });

        let mut parser = ConversationParser::new(PathBuf::from("/tmp/test.jsonl"));
        let msg1 = parser.parse_line(&line);
        assert!(msg1.is_some());

        // Same tool_use id again — should be skipped, resulting in empty blocks
        let msg2 = parser.parse_line(&line);
        assert!(msg2.is_none()); // No blocks => None
    }

    #[test]
    fn test_parse_tool_result() {
        let line = serde_json::json!({
            "type": "user",
            "uuid": "test-result",
            "message": {
                "role": "user",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "tool-123",
                        "content": "file contents here",
                        "is_error": false
                    }
                ]
            }
        });

        let mut parser = ConversationParser::new(PathBuf::from("/tmp/test.jsonl"));
        let msg = parser.parse_line(&line).unwrap();
        assert_eq!(msg.role, ChatRole::User);
        match &msg.blocks[0] {
            MessageBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                assert_eq!(tool_use_id, "tool-123");
                assert_eq!(content.as_deref(), Some("file contents here"));
                assert!(!is_error);
            }
            _ => panic!("Expected ToolResult block"),
        }
    }

    #[test]
    fn test_parse_image_block() {
        let line = serde_json::json!({
            "type": "user",
            "uuid": "test-image",
            "message": {
                "role": "user",
                "content": [
                    {"type": "text", "text": "What is in this image?"},
                    {
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": "image/png",
                            "data": "abc123"
                        }
                    }
                ]
            }
        });

        let mut parser = ConversationParser::new(PathBuf::from("/tmp/test.jsonl"));
        let msg = parser.parse_line(&line).unwrap();
        assert_eq!(msg.blocks.len(), 2);
        match &msg.blocks[1] {
            MessageBlock::Image { source } => {
                assert_eq!(source, "data:image/png;base64,abc123");
            }
            _ => panic!("Expected Image block"),
        }
    }
}
