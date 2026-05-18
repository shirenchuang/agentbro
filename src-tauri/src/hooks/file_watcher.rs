// FileWatcher — Watches Claude Code JSONL conversation files for changes
// Uses the `notify` crate to monitor the top-level projects directories.
// On file modification, triggers incremental re-parse and emits Tauri events.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{Config, Event, EventKind, PollWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

use super::conversation_parser::{
    claude_projects_dir, projects_dirs_from_roots, ConversationParser, IncrementalParseResult,
};

/// Tauri event name emitted when new conversation messages are parsed.
pub const CONVERSATION_UPDATE_EVENT: &str = "conversation-update";

/// Payload emitted to the frontend when conversation messages change.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationUpdatePayload {
    /// The session ID extracted from the JSONL filename.
    pub session_id: String,
    /// Incremental parse result with new + all messages.
    pub result: IncrementalParseResult,
}

/// Manages file watching for JSONL conversation files.
///
/// Watches top-level projects directories and triggers incremental re-parsing
/// when `.jsonl` files are reported. We intentionally avoid recursive watching:
/// Claude/AntCC project stores can contain thousands of historical files, and
/// kqueue-backed recursive watches keep one descriptor per file on macOS.
pub struct ConversationWatcher {
    /// The underlying notify watcher (kept alive to maintain the watch).
    _watcher: PollWatcher,
    /// Shared parser state: session_id -> ConversationParser
    parsers: Arc<Mutex<HashMap<String, ConversationParser>>>,
}

impl ConversationWatcher {
    /// Create and start a new conversation watcher.
    ///
    /// Accepts optional extra config roots (custom engine instances) to watch
    /// in addition to the default `~/.claude/projects/`.
    /// Returns `None` if no projects directories exist or the watcher fails.
    pub fn start(app_handle: AppHandle) -> Option<Self> {
        Self::start_with_roots(app_handle, &[])
    }

    /// Start watching default + custom engine instance project directories.
    pub fn start_with_roots(app_handle: AppHandle, extra_roots: &[PathBuf]) -> Option<Self> {
        let mut projects_dirs = Vec::new();
        if let Some(d) = claude_projects_dir() {
            projects_dirs.push(d);
        }
        projects_dirs.extend(projects_dirs_from_roots(extra_roots));

        if projects_dirs.is_empty() {
            log::info!("No projects directories found — conversation watcher disabled");
            return None;
        }

        let parsers: Arc<Mutex<HashMap<String, ConversationParser>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let parsers_clone = parsers.clone();
        let handle_clone = app_handle.clone();

        // Debounce state: path -> last event time
        let debounce_map: Arc<Mutex<HashMap<PathBuf, Instant>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let debounce_duration = Duration::from_millis(100);

        let mut watcher = match PollWatcher::new(
            move |res: Result<Event, notify::Error>| {
                let event = match res {
                    Ok(e) => e,
                    Err(e) => {
                        log::debug!("File watcher error: {}", e);
                        return;
                    }
                };

                // Only care about data modifications (writes)
                if !matches!(
                    event.kind,
                    EventKind::Modify(notify::event::ModifyKind::Data(_))
                        | EventKind::Modify(notify::event::ModifyKind::Any)
                        | EventKind::Create(_)
                ) {
                    return;
                }

                for path in &event.paths {
                    // Only process .jsonl files
                    let ext = path.extension().and_then(|e| e.to_str());
                    if ext != Some("jsonl") {
                        continue;
                    }

                    // Debounce: skip if we processed this file within debounce window
                    {
                        let mut dmap = match debounce_map.lock() {
                            Ok(m) => m,
                            Err(_) => continue,
                        };
                        let now = Instant::now();
                        if let Some(last) = dmap.get(path) {
                            if now.duration_since(*last) < debounce_duration {
                                continue;
                            }
                        }
                        dmap.insert(path.clone(), now);
                    }

                    // Extract session_id from filename (strip .jsonl extension)
                    let session_id = match path.file_stem().and_then(|s| s.to_str()) {
                        Some(id) => id.to_string(),
                        None => continue,
                    };

                    // Skip subagent files (agent-*.jsonl) — they're handled separately
                    if session_id.starts_with("agent-") {
                        continue;
                    }

                    // Get or create parser for this session
                    let result = {
                        let mut parsers_guard = match parsers_clone.lock() {
                            Ok(p) => p,
                            Err(_) => continue,
                        };

                        let parser = parsers_guard
                            .entry(session_id.clone())
                            .or_insert_with(|| ConversationParser::new(path.clone()));

                        match parser.parse_incremental() {
                            Ok(r) => r,
                            Err(e) => {
                                log::warn!("Failed to parse {}: {}", path.display(), e);
                                continue;
                            }
                        }
                    };

                    // Only emit if there are new messages or a clear was detected
                    if !result.new_messages.is_empty() || result.clear_detected {
                        let payload = ConversationUpdatePayload {
                            session_id: session_id.clone(),
                            result,
                        };

                        if let Err(e) = handle_clone.emit(CONVERSATION_UPDATE_EVENT, &payload) {
                            log::debug!(
                                "Failed to emit conversation update for {}: {}",
                                session_id,
                                e
                            );
                        }
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        ) {
            Ok(w) => w,
            Err(e) => {
                log::warn!("Failed to create file watcher: {}", e);
                return None;
            }
        };

        // Watch only the top-level project roots. Recursive kqueue watches keep
        // thousands of historical transcript descriptors open and can starve
        // the hook socket server with EMFILE ("Too many open files").
        let mut watching_any = false;
        for dir in &projects_dirs {
            if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                log::warn!("Failed to watch {}: {}", dir.display(), e);
            } else {
                log::info!("Conversation watcher started on {}", dir.display());
                watching_any = true;
            }
        }
        if !watching_any {
            log::warn!("Conversation watcher: could not watch any projects directory");
            return None;
        }

        Some(Self {
            _watcher: watcher,
            parsers,
        })
    }

    /// Register a session for watching by pre-creating a parser.
    ///
    /// Called when a new session is detected via hooks, so we're ready
    /// to parse its JSONL file as soon as it's created/modified.
    pub fn register_session(&self, session_id: &str, file_path: PathBuf) {
        if let Ok(mut parsers) = self.parsers.lock() {
            parsers
                .entry(session_id.to_string())
                .or_insert_with(|| ConversationParser::new(file_path));
        }
    }

    /// Remove a session's parser (on session end).
    pub fn unregister_session(&self, session_id: &str) {
        if let Ok(mut parsers) = self.parsers.lock() {
            parsers.remove(session_id);
        }
    }

    /// Manually trigger a full parse for a session.
    /// Returns the full message list or None if the session is unknown.
    pub fn parse_session_full(
        &self,
        session_id: &str,
        file_path: PathBuf,
    ) -> Option<IncrementalParseResult> {
        let mut parsers = self.parsers.lock().ok()?;
        let parser = parsers
            .entry(session_id.to_string())
            .or_insert_with(|| ConversationParser::new(file_path));

        parser.reset();
        parser.parse_incremental().ok()
    }
}
