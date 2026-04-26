// ConversationParser — Incremental JSONL conversation file parser
// Reads Claude Code conversation history from JSONL files,
// tracking file offset to only parse new lines on subsequent calls.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
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
            });
        }

        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(self.last_offset))?;

        let mut new_messages = Vec::new();
        let mut clear_detected = false;

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

        // Only parse user and assistant message lines
        if msg_type != "user" && msg_type != "assistant" {
            return None;
        }

        // Skip meta messages (system injections, command wrappers, etc.)
        if json.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false) {
            return None;
        }

        let uuid = json.get("uuid").and_then(|v| v.as_str()).unwrap_or("")
            .to_string();
        let timestamp = json.get("timestamp").and_then(|v| v.as_str())
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

                        let input = Self::flatten_input(
                            block.get("input").and_then(|v| v.as_object()),
                        );

                        blocks.push(MessageBlock::ToolUse { id, name, input });
                    }
                    "tool_result" => {
                        let tool_use_id = match block
                            .get("tool_use_id")
                            .and_then(|v| v.as_str())
                        {
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
                        if let Some(thinking) =
                            block.get("thinking").and_then(|v| v.as_str())
                        {
                            blocks.push(MessageBlock::Thinking {
                                thinking: thinking.to_string(),
                            });
                        }
                    }
                    _ => {
                        // Skip unknown block types (image, etc.) gracefully
                    }
                }
            }
        }

        Some(blocks)
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
    let home = dirs::home_dir()?;
    let projects_dir = home.join(".claude").join("projects");

    let project_dir_name = cwd.replace('/', "-").replace('.', "-");
    let session_file = projects_dir
        .join(&project_dir_name)
        .join(format!("{}.jsonl", session_id));

    if session_file.exists() {
        return Some(session_file);
    }

    // Fallback: search all project directories for this session ID
    if let Ok(entries) = std::fs::read_dir(&projects_dir) {
        for entry in entries.flatten() {
            let candidate = entry.path().join(format!("{}.jsonl", session_id));
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Get the Claude projects base directory.
pub fn claude_projects_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let dir = home.join(".claude").join("projects");
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

// ── Startup session discovery ─────────────────────────────────

/// A session discovered by scanning JSONL files at startup.
#[derive(Debug, Clone)]
pub struct DiscoveredSession {
    pub session_id: String,
    pub cwd: String,
    pub project: String,
    pub session_title: Option<String>,
}

/// Scan `~/.claude/projects/` for recently-active JSONL files.
///
/// Returns sessions whose JSONL file was modified within `max_age`.
/// For each file, reads the first ~30 lines to extract session metadata
/// and the first user message as a title.
pub fn discover_active_sessions(max_age: Duration) -> Vec<DiscoveredSession> {
    let projects_dir = match claude_projects_dir() {
        Some(d) => d,
        None => return Vec::new(),
    };

    let cutoff = std::time::SystemTime::now()
        .checked_sub(max_age)
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    let mut results = Vec::new();

    let project_entries = match std::fs::read_dir(&projects_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
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
            if let Some(session) = parse_session_header(&file_path) {
                results.push(session);
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
            let msg_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let is_meta = json.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false);

            if msg_type == "user" && !is_meta {
                if let Some(message) = json.get("message") {
                    if let Some(content) = message.get("content") {
                        let text = extract_text_from_content(content);
                        if !text.is_empty() {
                            first_user_text = Some(text);
                        }
                    }
                }
            }
        }

        // Stop early if we have everything
        if session_id.is_some() && cwd.is_some() && first_user_text.is_some() {
            break;
        }
    }

    let sid = session_id.or(filename_id)?;
    let cwd_str = cwd.unwrap_or_default();
    let project = cwd_str
        .rsplit('/')
        .next()
        .unwrap_or(&cwd_str)
        .to_string();

    Some(DiscoveredSession {
        session_id: sid,
        cwd: cwd_str,
        project,
        session_title: first_user_text,
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
            if block.get("type").and_then(|v| v.as_str()) == Some("text") {
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
    let cleaned = raw
        .lines()
        .next()
        .unwrap_or(&raw)
        .trim()
        .to_string();

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

        let msg_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let is_meta = json.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false);

        if msg_type == "user" && !is_meta {
            if let Some(message) = json.get("message") {
                if let Some(content) = message.get("content") {
                    let text = extract_text_from_content(content);
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
