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
    /// Total messages parsed so far across the whole file (including any that
    /// were evicted from `all_messages` by the retention cap).
    #[serde(default)]
    pub total_count: usize,
    /// True when `all_messages` is a tail window because older messages were
    /// evicted to keep memory bounded.
    #[serde(default)]
    pub truncated: bool,
}

/// Cache TTL metadata inferred from the latest main-agent assistant JSONL entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheTtlInfo {
    pub timestamp_ms: i64,
    pub ttl_ms: i64,
}

/// Pending native Codex `request_user_input` recovered from rollout JSONL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexPendingUserInput {
    pub call_id: String,
    pub question: String,
    pub options: Vec<String>,
    pub descriptions: Vec<String>,
    pub header: Option<String>,
    pub multi_select: bool,
    pub questions: Vec<CodexUserInputQuestion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexUserInputQuestion {
    pub id: Option<String>,
    pub question: String,
    pub header: Option<String>,
    pub options: Vec<CodexUserInputOption>,
    pub multi_select: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexUserInputOption {
    pub label: String,
    pub description: Option<String>,
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

/// Upper bound on messages retained per session in the streaming buffer.
/// Long conversations would otherwise grow `messages` (and every
/// `all_messages.clone()` shipped to the frontend) without limit. Older
/// messages stay on disk and can be re-fetched via `parse_full`.
const MAX_RETAINED_MESSAGES: usize = 500;

/// Upper bound on `seen_tool_ids` so the dedup set can't grow forever in
/// very long sessions. Old IDs almost never collide with new ones.
const MAX_SEEN_TOOL_IDS: usize = 4_000;

/// Incremental JSONL conversation parser.
///
/// Tracks file offset so that repeated calls only parse newly-appended lines.
/// Handles malformed lines gracefully by skipping them with a warning log.
pub struct ConversationParser {
    file_path: PathBuf,
    last_offset: u64,
    messages: Vec<ParsedMessage>,
    /// Count of messages ever parsed (including any evicted from `messages`).
    total_parsed: usize,
    seen_tool_ids: HashSet<String>,
}

impl ConversationParser {
    /// Create a new parser for the given JSONL file path.
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            last_offset: 0,
            messages: Vec::new(),
            total_parsed: 0,
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
                return Ok(self.build_result(vec![], false, self.last_offset, 0));
            }
            Err(e) => return Err(e),
        };

        let file_size = file.metadata()?.len();

        // If file shrank (e.g. was recreated), reset state
        if file_size < self.last_offset {
            self.last_offset = 0;
            self.messages.clear();
            self.total_parsed = 0;
            self.seen_tool_ids.clear();
        }

        // Nothing new
        if file_size == self.last_offset {
            return Ok(self.build_result(vec![], false, self.last_offset, 0));
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
                self.total_parsed = 0;
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
                self.push_message(msg);
            }
        }

        self.last_offset = file_size;
        self.compact_seen_tool_ids();

        Ok(self.build_result(new_messages, clear_detected, file_size, lines_read))
    }

    /// Push a freshly-parsed message into the retention buffer, dropping the
    /// oldest entries if we are already at `MAX_RETAINED_MESSAGES`.
    fn push_message(&mut self, msg: ParsedMessage) {
        self.total_parsed = self.total_parsed.saturating_add(1);
        if self.messages.len() >= MAX_RETAINED_MESSAGES {
            let drop_count = self.messages.len() + 1 - MAX_RETAINED_MESSAGES;
            self.messages.drain(0..drop_count);
        }
        self.messages.push(msg);
    }

    /// Keep `seen_tool_ids` bounded. We trade a vanishing chance of
    /// re-emitting a very old duplicate tool_use for a guaranteed memory
    /// ceiling. Tool IDs are random per invocation so collisions across
    /// a reset are not a practical concern.
    fn compact_seen_tool_ids(&mut self) {
        if self.seen_tool_ids.len() > MAX_SEEN_TOOL_IDS {
            self.seen_tool_ids.clear();
        }
    }

    fn build_result(
        &self,
        new_messages: Vec<ParsedMessage>,
        clear_detected: bool,
        byte_offset: u64,
        lines_read: usize,
    ) -> IncrementalParseResult {
        IncrementalParseResult {
            new_messages,
            all_messages: self.messages.clone(),
            clear_detected,
            byte_offset,
            lines_read,
            total_count: self.total_parsed,
            truncated: self.total_parsed > self.messages.len(),
        }
    }

    /// Parse the entire file from scratch (ignores previous offset).
    /// Useful for initial load of a conversation. Returns the full message
    /// list — `parse_full` is intentionally not subject to the in-memory
    /// retention cap, so user-initiated "open this session" still surfaces
    /// complete history. Internal state is left primed with only the tail,
    /// so subsequent `parse_incremental` calls stay bounded.
    pub fn parse_full(&mut self) -> Result<Vec<ParsedMessage>, std::io::Error> {
        self.last_offset = 0;
        self.messages.clear();
        self.total_parsed = 0;
        self.seen_tool_ids.clear();

        let file = match File::open(&self.file_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let file_size = file.metadata()?.len();

        let mut reader = BufReader::new(file);
        let mut all = Vec::new();
        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            let bytes_read = reader.read_line(&mut line_buf)?;
            if bytes_read == 0 {
                break;
            }
            let line = line_buf.trim();
            if line.is_empty() {
                continue;
            }
            if line.contains("<command-name>/clear</command-name>") {
                all.clear();
                self.seen_tool_ids.clear();
                continue;
            }
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
                all.push(msg);
            }
        }

        self.last_offset = file_size;
        self.total_parsed = all.len();
        let tail_start = all.len().saturating_sub(MAX_RETAINED_MESSAGES);
        self.messages = all[tail_start..].to_vec();
        self.compact_seen_tool_ids();

        Ok(all)
    }

    /// Reset parser state (useful when conversation is cleared or reloaded).
    pub fn reset(&mut self) {
        self.last_offset = 0;
        self.messages.clear();
        self.total_parsed = 0;
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

/// Extract the latest unresolved native Codex `request_user_input` call.
pub fn extract_pending_codex_user_input(path: &Path) -> Option<CodexPendingUserInput> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut pending: Option<CodexPendingUserInput> = None;

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
        let Some(payload) = json.get("payload") else {
            continue;
        };
        let Some(payload_type) = payload.get("type").and_then(|value| value.as_str()) else {
            continue;
        };

        match payload_type {
            "function_call" => {
                let tool_name = payload
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                if normalize_tool_name(tool_name) != "requestuserinput" {
                    continue;
                }
                if let Some(next_pending) = parse_codex_user_input_call(payload) {
                    pending = Some(next_pending);
                }
            }
            "function_call_output" => {
                let call_id = payload
                    .get("call_id")
                    .or_else(|| payload.get("callId"))
                    .or_else(|| payload.get("id"))
                    .and_then(|value| value.as_str());
                if pending
                    .as_ref()
                    .is_some_and(|item| call_id == Some(item.call_id.as_str()))
                {
                    pending = None;
                }
            }
            _ => {}
        }
    }

    pending
}

fn parse_codex_user_input_call(payload: &serde_json::Value) -> Option<CodexPendingUserInput> {
    let call_id = payload
        .get("call_id")
        .or_else(|| payload.get("callId"))
        .or_else(|| payload.get("id"))
        .and_then(|value| value.as_str())?
        .to_string();
    let arguments = codex_arguments_value(payload.get("arguments"))?;
    let questions = parse_codex_user_input_questions(arguments.get("questions")?.as_array()?);
    if questions.is_empty() {
        return None;
    }

    let first = &questions[0];
    Some(CodexPendingUserInput {
        call_id,
        question: first.question.clone(),
        options: first
            .options
            .iter()
            .map(|option| option.label.clone())
            .collect(),
        descriptions: first
            .options
            .iter()
            .map(|option| option.description.clone().unwrap_or_default())
            .collect(),
        header: first.header.clone(),
        multi_select: first.multi_select,
        questions,
    })
}

fn codex_arguments_value(arguments: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    match arguments? {
        serde_json::Value::String(raw) => serde_json::from_str(raw).ok(),
        value @ serde_json::Value::Object(_) => Some(value.clone()),
        _ => None,
    }
}

fn parse_codex_user_input_questions(
    raw_questions: &[serde_json::Value],
) -> Vec<CodexUserInputQuestion> {
    raw_questions
        .iter()
        .filter_map(|question| {
            let prompt = string_field(question, &["question", "prompt", "label"])?;
            if prompt.trim().is_empty() {
                return None;
            }
            let options = question
                .get("options")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(parse_codex_user_input_option)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if options.is_empty() {
                return None;
            }
            Some(CodexUserInputQuestion {
                id: string_field(question, &["id"]),
                question: prompt,
                header: string_field(question, &["header"]),
                options,
                multi_select: bool_field(
                    question,
                    &[
                        "multiSelect",
                        "multi_select",
                        "isMultiple",
                        "allowsMultiple",
                        "multiple",
                    ],
                )
                .unwrap_or(false),
            })
        })
        .collect()
}

fn parse_codex_user_input_option(value: &serde_json::Value) -> Option<CodexUserInputOption> {
    if let Some(label) = value.as_str() {
        if label.trim().is_empty() {
            return None;
        }
        return Some(CodexUserInputOption {
            label: label.to_string(),
            description: None,
        });
    }

    let label = string_field(value, &["label", "title"])?;
    if label.trim().is_empty() {
        return None;
    }
    Some(CodexUserInputOption {
        label,
        description: string_field(value, &["description", "detail"]),
    })
}

fn normalize_tool_name(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn string_field(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(|inner| inner.as_str())
            .filter(|inner| !inner.trim().is_empty())
            .map(|inner| inner.to_string())
    })
}

fn bool_field(value: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|inner| inner.as_bool()))
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
    let mut pending_codex_spawns: HashMap<String, PendingSubagentTool> = HashMap::new();
    let mut codex_agents: HashMap<String, TranscriptSubagentInfo> = HashMap::new();
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
        if let Some(tool) = codex_spawn_agent_call_from_json(&json) {
            pending_codex_spawns.insert(tool.tool_use_id.clone(), tool);
        }
        if let Some(subagent) =
            codex_spawn_agent_output_from_json(&json, &main_transcript_path, &pending_codex_spawns)
        {
            codex_agents.insert(subagent.agent_id.clone(), subagent.clone());
            upsert_transcript_subagent(&mut subagents, subagent);
        }
        for completion in codex_subagent_completions_from_json(&json) {
            let started = codex_agents.get(&completion.agent_id).cloned().or_else(|| {
                pending_codex_spawns
                    .values()
                    .find(|tool| tool.tool_use_id == completion.agent_id)
                    .map(|tool| {
                        transcript_subagent_from_pending_codex_tool(
                            tool,
                            &main_transcript_path,
                            None,
                            None,
                        )
                    })
            });
            let mut subagent = started.unwrap_or_else(|| TranscriptSubagentInfo {
                agent_id: completion.agent_id.clone(),
                launch_tool_use_id: None,
                name: None,
                agent_type: None,
                description: "Subagent".to_string(),
                transcript_path: Some(main_transcript_path.clone()),
                agent_transcript_path: None,
                last_assistant_message: None,
                started_at: timestamp_seconds(&json),
                completed_at: None,
                status: "running".to_string(),
                tools: Vec::new(),
            });
            subagent.status = completion.status;
            subagent.last_assistant_message = completion.last_assistant_message;
            subagent.completed_at = Some(timestamp_seconds(&json));
            codex_agents.insert(subagent.agent_id.clone(), subagent.clone());
            upsert_transcript_subagent(&mut subagents, subagent);
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
    for tool in pending_codex_spawns.into_values() {
        if codex_agents.values().any(|subagent| {
            subagent.launch_tool_use_id.as_deref() == Some(tool.tool_use_id.as_str())
        }) {
            continue;
        }
        upsert_transcript_subagent(
            &mut subagents,
            transcript_subagent_from_pending_codex_tool(&tool, &main_transcript_path, None, None),
        );
    }

    subagents.sort_by(|a, b| {
        a.started_at
            .cmp(&b.started_at)
            .then(a.agent_id.cmp(&b.agent_id))
    });
    subagents
}

#[derive(Debug, Clone)]
struct CodexSubagentCompletion {
    agent_id: String,
    status: String,
    last_assistant_message: Option<String>,
}

fn codex_spawn_agent_call_from_json(json: &serde_json::Value) -> Option<PendingSubagentTool> {
    let payload = json.get("payload")?;
    if json.get("type").and_then(|v| v.as_str()) != Some("response_item")
        || payload.get("type").and_then(|v| v.as_str()) != Some("function_call")
        || payload.get("name").and_then(|v| v.as_str()) != Some("spawn_agent")
    {
        return None;
    }

    let call_id = payload
        .get("call_id")
        .or_else(|| payload.get("callId"))
        .or_else(|| payload.get("id"))
        .and_then(|v| v.as_str())?
        .to_string();
    let input = codex_arguments_value(payload.get("arguments")).unwrap_or(serde_json::Value::Null);
    let prompt = string_field(&input, &["message", "prompt", "task", "description"])
        .unwrap_or_else(|| "Subagent".to_string());

    Some(PendingSubagentTool {
        tool_use_id: call_id,
        name: string_field(&input, &["name", "agent_name", "agentName"]),
        description: string_field(&input, &["description"]).unwrap_or_default(),
        prompt,
        agent_type: string_field(
            &input,
            &["agent_type", "agentType", "subagent_type", "subagentType"],
        ),
        started_at: timestamp_seconds(json),
    })
}

fn codex_spawn_agent_output_from_json(
    json: &serde_json::Value,
    main_transcript_path: &str,
    pending: &HashMap<String, PendingSubagentTool>,
) -> Option<TranscriptSubagentInfo> {
    let payload = json.get("payload")?;
    if json.get("type").and_then(|v| v.as_str()) != Some("response_item")
        || payload.get("type").and_then(|v| v.as_str()) != Some("function_call_output")
    {
        return None;
    }
    let call_id = payload
        .get("call_id")
        .or_else(|| payload.get("callId"))
        .or_else(|| payload.get("id"))
        .and_then(|v| v.as_str())?;
    let pending_tool = pending.get(call_id)?;
    let output = codex_output_value(payload.get("output"))?;
    let agent_id = string_field(&output, &["agent_id", "agentId", "agent_path", "agentPath"])?;
    let name = string_field(&output, &["nickname", "name", "agent_name", "agentName"])
        .or_else(|| pending_tool.name.clone());

    Some(transcript_subagent_from_pending_codex_tool(
        pending_tool,
        main_transcript_path,
        Some(agent_id),
        name,
    ))
}

fn codex_subagent_completions_from_json(json: &serde_json::Value) -> Vec<CodexSubagentCompletion> {
    if let Some(payload) = json.get("payload") {
        if json.get("type").and_then(|v| v.as_str()) == Some("response_item")
            && payload.get("type").and_then(|v| v.as_str()) == Some("function_call_output")
        {
            if let Some(output) = codex_output_value(payload.get("output")) {
                if let Some(status) = output.get("status") {
                    return codex_status_map_to_completions(status);
                }
            }
        }
    }

    codex_raw_user_text_from_json(json)
        .and_then(|text| {
            let text = text.trim();
            let inner = text
                .strip_prefix("<subagent_notification>")?
                .strip_suffix("</subagent_notification>")?
                .trim();
            serde_json::from_str::<serde_json::Value>(inner).ok()
        })
        .and_then(|notification| {
            let agent_id = string_field(
                &notification,
                &["agent_path", "agentPath", "agent_id", "agentId"],
            )?;
            let status = notification.get("status")?;
            codex_status_entry_to_completion(&agent_id, status)
        })
        .into_iter()
        .collect()
}

fn codex_raw_user_text_from_json(json: &serde_json::Value) -> Option<String> {
    if json.get("type").and_then(|v| v.as_str()) != Some("response_item") {
        return None;
    }
    let payload = json.get("payload")?;
    if payload.get("type").and_then(|v| v.as_str()) != Some("message")
        || payload.get("role").and_then(|v| v.as_str()) != Some("user")
    {
        return None;
    }
    payload
        .get("content")?
        .as_array()?
        .iter()
        .find_map(|block| {
            if block.get("type").and_then(|v| v.as_str()) == Some("input_text") {
                block
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            } else {
                None
            }
        })
}

fn codex_status_map_to_completions(status: &serde_json::Value) -> Vec<CodexSubagentCompletion> {
    status
        .as_object()
        .into_iter()
        .flat_map(|map| map.iter())
        .filter_map(|(agent_id, value)| codex_status_entry_to_completion(agent_id, value))
        .collect()
}

fn codex_status_entry_to_completion(
    agent_id: &str,
    value: &serde_json::Value,
) -> Option<CodexSubagentCompletion> {
    if let Some(text) = value.get("completed").and_then(|v| v.as_str()) {
        return Some(CodexSubagentCompletion {
            agent_id: agent_id.to_string(),
            status: "completed".to_string(),
            last_assistant_message: Some(text.to_string()),
        });
    }
    if let Some(text) = value
        .get("failed")
        .or_else(|| value.get("error"))
        .and_then(|v| v.as_str())
    {
        return Some(CodexSubagentCompletion {
            agent_id: agent_id.to_string(),
            status: "error".to_string(),
            last_assistant_message: Some(text.to_string()),
        });
    }
    None
}

fn transcript_subagent_from_pending_codex_tool(
    tool: &PendingSubagentTool,
    main_transcript_path: &str,
    agent_id: Option<String>,
    name: Option<String>,
) -> TranscriptSubagentInfo {
    TranscriptSubagentInfo {
        agent_id: agent_id.unwrap_or_else(|| tool.tool_use_id.clone()),
        launch_tool_use_id: Some(tool.tool_use_id.clone()),
        name,
        agent_type: tool.agent_type.clone(),
        description: choose_subagent_description(&tool.description, &tool.prompt),
        transcript_path: Some(main_transcript_path.to_string()),
        agent_transcript_path: None,
        last_assistant_message: None,
        started_at: tool.started_at,
        completed_at: None,
        status: "running".to_string(),
        tools: Vec::new(),
    }
}

fn codex_output_value(output: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    match output? {
        serde_json::Value::String(raw) => serde_json::from_str(raw).ok(),
        value @ serde_json::Value::Object(_) => Some(value.clone()),
        _ => None,
    }
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
        || trimmed.starts_with("# AGENTS.md instructions")
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
/// Claude records cache-creation usage on assistant messages. AgentBro uses the
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
    let dir = crate::agents::claude_code::default_config_root().join("projects");
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
    let project = crate::agents::project_name_from_path(&cwd_str);

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
        match useful_text_block(text) {
            Some(text) => text.to_string(),
            None => return String::new(),
        }
    } else if let Some(arr) = content.as_array() {
        // Array of blocks — find the first useful text block.
        let mut found = String::new();
        for block in arr {
            if matches!(
                block.get("type").and_then(|v| v.as_str()),
                Some("text" | "input_text" | "output_text")
            ) {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    if let Some(text) = useful_text_block(text) {
                        if text.starts_with("[Image") {
                            continue;
                        }
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
    fn extract_session_title_skips_codex_environment_context() {
        let path = write_temp_jsonl(
            "codex-title-env-context",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>\n  <cwd>/tmp/project</cwd>\n</environment_context>"}]}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Build the real landing page"}]}}
"#,
        );

        assert_eq!(
            extract_session_title(&path).as_deref(),
            Some("Build the real landing page")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn extract_session_title_skips_codex_agent_instructions_context() {
        let path = write_temp_jsonl(
            "codex-title-agents-context",
            r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for /tmp/project\n\n<INSTRUCTIONS>\nProject rules\n</INSTRUCTIONS>\n<environment_context>\n  <cwd>/tmp/project</cwd>\n</environment_context>"}]}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"创建3个subagent 来计算 1+1  2+2  3+3"}]}}
"##,
        );

        assert_eq!(
            extract_session_title(&path).as_deref(),
            Some("创建3个subagent 来计算 1+1  2+2  3+3")
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
    fn extracts_pending_codex_request_user_input() {
        let path = write_temp_jsonl(
            "codex-request-user-input",
            r#"{"timestamp":"2026-01-01T00:00:00Z","type":"response_item","payload":{"type":"function_call","name":"request_user_input","call_id":"call_question_1","arguments":"{\"questions\":[{\"header\":\"Choice\",\"id\":\"choice\",\"question\":\"Pick one\",\"options\":[{\"label\":\"Preview\",\"description\":\"Open staging\"},{\"label\":\"Ship\",\"description\":\"Release now\"}]}]}"}}
"#,
        );

        let pending = extract_pending_codex_user_input(&path).expect("pending question");
        assert_eq!(pending.call_id, "call_question_1");
        assert_eq!(pending.question, "Pick one");
        assert_eq!(pending.header.as_deref(), Some("Choice"));
        assert_eq!(pending.options, vec!["Preview", "Ship"]);
        assert_eq!(
            pending.descriptions,
            vec!["Open staging".to_string(), "Release now".to_string()]
        );
        assert_eq!(pending.questions[0].id.as_deref(), Some("choice"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn clears_codex_request_user_input_after_output() {
        let path = write_temp_jsonl(
            "codex-request-user-input-resolved",
            r#"{"timestamp":"2026-01-01T00:00:00Z","type":"response_item","payload":{"type":"function_call","name":"request_user_input","call_id":"call_question_1","arguments":"{\"questions\":[{\"id\":\"choice\",\"question\":\"Pick one\",\"options\":[{\"label\":\"Preview\"}]}]}"}}
{"timestamp":"2026-01-01T00:00:01Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_question_1","output":"{\"answers\":{\"choice\":{\"answers\":[\"Preview\"]}}}"}}
"#,
        );

        assert!(extract_pending_codex_user_input(&path).is_none());
        let _ = std::fs::remove_file(path);
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
    fn extracts_subagents_from_codex_multi_agent_transcript() {
        let path = write_temp_jsonl(
            "codex-subagents",
            r#"{"timestamp":"2026-05-29T13:34:17.090Z","type":"response_item","payload":{"type":"function_call","name":"spawn_agent","namespace":"multi_agent_v1","arguments":"{\"message\":\"请只计算这个表达式并返回最终结果：1+1。\"}","call_id":"call-a"}}
{"timestamp":"2026-05-29T13:34:17.093Z","type":"response_item","payload":{"type":"function_call","name":"spawn_agent","namespace":"multi_agent_v1","arguments":"{\"message\":\"请只计算这个表达式并返回最终结果：2+2。\"}","call_id":"call-b"}}
{"timestamp":"2026-05-29T13:34:17.359Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call-a","output":"{\"agent_id\":\"019e73f1-5808-7a91-bfd4-2aadc13d2c77\",\"nickname\":\"Laplace\"}"}}
{"timestamp":"2026-05-29T13:34:17.505Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call-b","output":"{\"agent_id\":\"019e73f1-5899-7342-9013-b3ffa5404cac\",\"nickname\":\"Newton\"}"}}
{"timestamp":"2026-05-29T13:34:24.662Z","type":"response_item","payload":{"type":"function_call_output","call_id":"wait-1","output":"{\"status\":{\"019e73f1-5808-7a91-bfd4-2aadc13d2c77\":{\"completed\":\"2\"}},\"timed_out\":false}"}}
{"timestamp":"2026-05-29T13:34:28.636Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<subagent_notification>\n{\"agent_path\":\"019e73f1-5899-7342-9013-b3ffa5404cac\",\"status\":{\"completed\":\"4\"}}\n</subagent_notification>"}]}}
"#,
        );

        let subagents = extract_subagents_from_transcript(&path);

        assert_eq!(subagents.len(), 2);
        assert_eq!(
            subagents[0].agent_id,
            "019e73f1-5808-7a91-bfd4-2aadc13d2c77"
        );
        assert_eq!(subagents[0].launch_tool_use_id.as_deref(), Some("call-a"));
        assert_eq!(subagents[0].name.as_deref(), Some("Laplace"));
        assert_eq!(
            subagents[0].description,
            "请只计算这个表达式并返回最终结果：1+1。"
        );
        assert_eq!(subagents[0].status, "completed");
        assert_eq!(subagents[0].last_assistant_message.as_deref(), Some("2"));
        assert_eq!(
            subagents[1].agent_id,
            "019e73f1-5899-7342-9013-b3ffa5404cac"
        );
        assert_eq!(subagents[1].name.as_deref(), Some("Newton"));
        assert_eq!(subagents[1].status, "completed");
        assert_eq!(subagents[1].last_assistant_message.as_deref(), Some("4"));

        let _ = std::fs::remove_file(path);
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

    fn write_assistant_text_lines(name: &str, count: usize) -> PathBuf {
        let mut body = String::new();
        for i in 0..count {
            body.push_str(&format!(
                r#"{{"type":"assistant","uuid":"u{i}","message":{{"role":"assistant","content":[{{"type":"text","text":"m{i}"}}]}}}}"#,
            ));
            body.push('\n');
        }
        write_temp_jsonl(name, &body)
    }

    #[test]
    fn parse_incremental_retains_only_tail_under_cap() {
        let total = MAX_RETAINED_MESSAGES + 50;
        let path = write_assistant_text_lines("retain-tail", total);

        let mut parser = ConversationParser::new(path.clone());
        let result = parser.parse_incremental().expect("parse incremental");

        assert_eq!(result.total_count, total);
        assert_eq!(result.all_messages.len(), MAX_RETAINED_MESSAGES);
        assert!(result.truncated);

        let last = result.all_messages.last().expect("tail present");
        match &last.blocks[0] {
            MessageBlock::Text { text } => assert_eq!(text, &format!("m{}", total - 1)),
            other => panic!("unexpected block: {other:?}"),
        }

        let first = result.all_messages.first().expect("first present");
        match &first.blocks[0] {
            MessageBlock::Text { text } => assert_ne!(text, "m0"),
            other => panic!("unexpected block: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parse_full_returns_complete_history_then_primes_tail() {
        let total = MAX_RETAINED_MESSAGES + 25;
        let path = write_assistant_text_lines("parse-full", total);

        let mut parser = ConversationParser::new(path.clone());
        let full = parser.parse_full().expect("parse full");
        assert_eq!(full.len(), total, "parse_full must return everything");

        let follow_up = parser.parse_incremental().expect("incremental follow-up");
        assert_eq!(follow_up.total_count, total);
        assert_eq!(follow_up.all_messages.len(), MAX_RETAINED_MESSAGES);
        assert!(follow_up.truncated);
        assert!(
            follow_up.new_messages.is_empty(),
            "no new bytes since parse_full"
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn clear_command_resets_total_count_and_truncated_flag() {
        let mut body = String::new();
        for i in 0..3 {
            body.push_str(&format!(
                r#"{{"type":"assistant","uuid":"a{i}","message":{{"role":"assistant","content":[{{"type":"text","text":"m{i}"}}]}}}}"#,
            ));
            body.push('\n');
        }
        body.push_str("<command-name>/clear</command-name>\n");
        for i in 0..2 {
            body.push_str(&format!(
                r#"{{"type":"assistant","uuid":"b{i}","message":{{"role":"assistant","content":[{{"type":"text","text":"after{i}"}}]}}}}"#,
            ));
            body.push('\n');
        }
        let path = write_temp_jsonl("clear-reset", &body);

        let mut parser = ConversationParser::new(path.clone());
        let result = parser.parse_incremental().expect("parse incremental");
        assert!(result.clear_detected);
        assert_eq!(result.all_messages.len(), 2);
        assert_eq!(result.total_count, 2, "/clear should reset total_parsed");
        assert!(!result.truncated);

        let _ = std::fs::remove_file(path);
    }
}
