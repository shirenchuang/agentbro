use super::{
    codex, in_project_scope, project_path, ConversationAgent, HandoffBundle, MigratedConversation,
    PortableMessage, PortableSession, ReturnContext, SessionSummary,
};
use crate::agents::{claude_code::configured_config_root, executable};
use crate::hooks::conversation_parser::ChatRole;
use crate::skills::installer::TempInstallDir;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(super) struct ClaudeRollout {
    pub summary: SessionSummary,
    path: PathBuf,
}

pub(super) struct ClaudeHistory {
    pub messages: Vec<PortableMessage>,
    pub partial: bool,
    pub context: Option<ReturnContext>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Provenance {
    source: ConversationAgent,
    session_id: String,
    message_count: usize,
    digest: String,
}

fn projects_root(home: &Path) -> PathBuf {
    configured_config_root()
        .unwrap_or_else(|| home.join(".claude"))
        .join("projects")
}

pub(super) fn catalog(
    home: &Path,
    scope: Option<&[PathBuf]>,
) -> Result<Vec<ClaudeRollout>, String> {
    catalog_at(&projects_root(home), scope)
}

fn catalog_at(projects: &Path, scope: Option<&[PathBuf]>) -> Result<Vec<ClaudeRollout>, String> {
    let locations = match fs::read_dir(projects) {
        Ok(items) => items,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("Could not read Claude Code projects: {err}")),
    };
    let mut result = Vec::new();
    for dir in locations.flatten() {
        if !dir.file_type().is_ok_and(|ty| ty.is_dir()) {
            continue;
        }
        let folder = dir.path();
        let Ok(files) = fs::read_dir(&folder) else {
            continue;
        };
        let index = fs::File::open(folder.join("sessions-index.json"))
            .ok()
            .and_then(|file| serde_json::from_reader::<_, Value>(BufReader::new(file)).ok());
        let entries = index
            .as_ref()
            .and_then(|value| value["entries"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|value| value["sessionId"].as_str().map(|id| (id, value)))
            .collect::<HashMap<_, _>>();
        for file in files.flatten() {
            let path = file.path();
            let Some(id) = filename_id(&path) else {
                continue;
            };
            if !file.file_type().is_ok_and(|ty| ty.is_file()) {
                continue;
            }
            let Some((cwd, first_prompt)) = first_dialog(&path, &id) else {
                continue;
            };
            if scope.is_some_and(|roots| !in_project_scope(Path::new(&cwd), roots)) {
                continue;
            }
            let details = entries.get(id.as_str());
            let title = tail_title(&path, &id)
                .or_else(|| {
                    details
                        .and_then(|value| {
                            value["customTitle"]
                                .as_str()
                                .or_else(|| value["summary"].as_str())
                        })
                        .map(str::to_owned)
                })
                .or(first_prompt)
                .unwrap_or_default();
            let archived = details.is_some_and(|value| {
                value["isArchived"].as_bool() == Some(true)
                    || value["archived"].as_bool() == Some(true)
            });
            let updated_at = file
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339());
            result.push(ClaudeRollout {
                summary: SessionSummary {
                    id,
                    title,
                    directory_exists: Path::new(&cwd).is_dir(),
                    cwd,
                    archived,
                    updated_at,
                },
                path,
            });
        }
    }
    result.sort_by(|left, right| {
        right
            .summary
            .updated_at
            .cmp(&left.summary.updated_at)
            .then_with(|| left.summary.id.cmp(&right.summary.id))
            .then_with(|| left.path.cmp(&right.path))
    });
    let mut seen = HashSet::new();
    result.retain(|row| seen.insert(row.summary.id.clone()));
    Ok(result)
}

fn filename_id(path: &Path) -> Option<String> {
    if path.extension()?.to_str()? != "jsonl" {
        return None;
    }
    let raw = path.file_stem()?.to_str()?;
    Uuid::parse_str(raw).ok().map(|id| id.to_string())
}

fn first_dialog(path: &Path, session: &str) -> Option<(String, Option<String>)> {
    let reader = BufReader::new(fs::File::open(path).ok()?);
    for line in reader.lines().map_while(Result::ok) {
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if !is_main_record(&record, session)
            || !matches!(record["type"].as_str(), Some("user" | "assistant"))
        {
            continue;
        }
        if let Some(cwd) = record["cwd"].as_str() {
            if !Path::new(cwd).is_absolute() {
                continue;
            }
            let title = parse_dialog(&record, 0)
                .0
                .filter(|message| message.role == ChatRole::User)
                .map(|message| message.text.chars().take(120).collect());
            return Some((cwd.to_owned(), title));
        }
    }
    None
}

fn tail_title(path: &Path, session: &str) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let offset = file.metadata().ok()?.len().saturating_sub(64 * 1024);
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut reader = BufReader::new(file);
    if offset > 0 {
        reader.read_until(b'\n', &mut Vec::new()).ok()?;
    }
    reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| {
            let record: Value = serde_json::from_str(&line).ok()?;
            if record["type"] != "custom-title" || record["sessionId"] != session {
                return None;
            }
            record["customTitle"]
                .as_str()
                .filter(|title| !title.is_empty())
                .map(str::to_owned)
        })
        .last()
}

fn is_main_record(record: &Value, session: &str) -> bool {
    record["sessionId"] == session && record["isSidechain"].as_bool() != Some(true)
}

struct HistoryRow {
    id: String,
    parent: Option<String>,
    message: Option<PortableMessage>,
}

pub(super) fn messages(rollout: &ClaudeRollout) -> io::Result<ClaudeHistory> {
    let mut partial = false;
    let mut rows = Vec::new();
    let mut provenance = None;
    let reader = BufReader::new(fs::File::open(&rollout.path)?);
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            partial = true;
            continue;
        };
        if !is_main_record(&record, &rollout.summary.id) {
            continue;
        }
        if provenance.is_none() {
            provenance =
                serde_json::from_value::<Provenance>(record["agentbroMigration"].clone()).ok();
        }
        let parent = record["parentUuid"].as_str().map(str::to_owned);
        let id = record["uuid"]
            .as_str()
            .map_or_else(|| format!("record-{index}"), str::to_owned);
        let (message, missing_blocks) = parse_dialog(&record, index);
        partial |= missing_blocks;
        if message.is_some() || record["uuid"].is_string() {
            rows.push(HistoryRow {
                id,
                parent,
                message,
            });
        }
    }
    let (mut messages, lost_parent) = active_history(rows);
    partial |= lost_parent;
    let context = provenance.and_then(|source| {
        let original = messages.get(..source.message_count)?;
        (source.message_count > 0 && source.digest == digest(original)).then_some(ReturnContext {
            original_agent: source.source,
            original_session_id: source.session_id,
            matching_messages: source.message_count,
        })
    });
    // Native assistant output can span multiple records with the same message ID.
    // IDs here represent the stored transcript records and stay unique in previews.
    for (index, message) in messages.iter_mut().enumerate() {
        message.id = format!("{}-{index}", message.id);
    }
    Ok(ClaudeHistory {
        messages,
        partial,
        context,
    })
}

fn active_history(mut rows: Vec<HistoryRow>) -> (Vec<PortableMessage>, bool) {
    if !rows.iter().any(|row| row.parent.is_some()) {
        return (
            rows.into_iter().filter_map(|row| row.message).collect(),
            false,
        );
    }
    let mut by_id = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.id.clone(), index))
        .collect::<HashMap<_, _>>();
    let mut index = rows.iter().rposition(|row| row.message.is_some());
    let mut result = Vec::new();
    let mut partial = false;
    while let Some(current) = index {
        let row = &mut rows[current];
        by_id.remove(&row.id);
        if let Some(message) = row.message.take() {
            result.push(message);
        }
        index = row
            .parent
            .as_ref()
            .and_then(|parent| by_id.get(parent).copied());
        if index.is_none() && row.parent.is_some() {
            partial = true;
        }
    }
    result.reverse();
    (result, partial)
}

fn parse_dialog(record: &Value, index: usize) -> (Option<PortableMessage>, bool) {
    if record["isMeta"].as_bool() == Some(true)
        || record["isCompactSummary"].as_bool() == Some(true)
        || record["isVisibleInTranscriptOnly"].as_bool() == Some(true)
    {
        return (None, false);
    }
    let role = match (record["type"].as_str(), record["message"]["role"].as_str()) {
        (Some("user"), Some("user")) => ChatRole::User,
        (Some("assistant"), Some("assistant")) => ChatRole::Assistant,
        _ => return (None, false),
    };
    let content = &record["message"]["content"];
    let mut partial = false;
    let text = if let Some(text) = content.as_str() {
        if allowed_text(text) {
            text.to_owned()
        } else {
            String::new()
        }
    } else if let Some(blocks) = content.as_array() {
        partial = blocks
            .iter()
            .any(|block| matches!(block["type"].as_str(), Some("image" | "document" | "file")));
        blocks
            .iter()
            .filter(|block| block["type"] == "text")
            .filter_map(|block| block["text"].as_str())
            .filter(|text| allowed_text(text))
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        return (None, !content.is_null());
    };
    let message = (!text.trim().is_empty()).then(|| PortableMessage {
        id: record["uuid"]
            .as_str()
            .map_or_else(|| format!("record-{index}"), str::to_owned),
        role,
        timestamp: record["timestamp"].as_str().map(str::to_owned),
        text,
    });
    (message, partial)
}

fn allowed_text(text: &str) -> bool {
    let text = text.trim_start();
    !text.is_empty()
        && ![
            "<command-name>",
            "<local-command-",
            "<system-reminder>",
            "<environment_context>",
        ]
        .iter()
        .any(|marker| text.starts_with(marker))
}

fn digest(messages: &[PortableMessage]) -> String {
    let mut hasher = Sha256::new();
    for message in messages {
        hasher.update([if message.role == ChatRole::User { 0 } else { 1 }]);
        hasher.update((message.text.len() as u64).to_le_bytes());
        hasher.update(message.text.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

pub(super) fn return_context(
    home: &Path,
    parent: &str,
    directory: &str,
    imported: &[PortableMessage],
) -> Option<ReturnContext> {
    let expected = Uuid::parse_str(parent).ok()?;
    let target = project_path(Path::new(directory))?;
    let directory = target.to_str()?;
    let original = catalog(home, None).ok()?.into_iter().find(|row| {
        codex::provenance_id(ConversationAgent::ClaudeCode, &row.summary.id, directory) == expected
    })?;
    let matching_messages = messages(&original)
        .ok()?
        .messages
        .iter()
        .zip(imported)
        .take_while(|(left, right)| left.role == right.role && left.text == right.text)
        .count();
    (matching_messages > 0).then_some(ReturnContext {
        original_agent: ConversationAgent::ClaudeCode,
        original_session_id: original.summary.id,
        matching_messages,
    })
}

pub(super) fn import(
    home: &Path,
    bundle: &HandoffBundle,
) -> Result<Vec<MigratedConversation>, String> {
    if executable::find_binary("claude").is_none() {
        return Err("Claude Code was not found. You can still save a handoff archive.".to_string());
    }
    import_at(&projects_root(home), bundle)
}

fn import_at(projects: &Path, bundle: &HandoffBundle) -> Result<Vec<MigratedConversation>, String> {
    if !bundle
        .conversations
        .iter()
        .any(|session| !session.messages.is_empty())
    {
        return Err("There are no readable conversations to migrate.".to_string());
    }
    let target = Path::new(&bundle.target_directory)
        .canonicalize()
        .map_err(|err| err.to_string())?;
    let target = native_directory(&target)?;
    let folder = destination_folder(projects, &target)?;
    let mut result = Vec::new();
    for session in bundle
        .conversations
        .iter()
        .filter(|session| !session.messages.is_empty())
    {
        let id = imported_id(bundle.source, &session.summary.id, &bundle.target_directory);
        let provenance = Provenance {
            source: bundle.source,
            session_id: session.summary.id.clone(),
            message_count: session.messages.len(),
            digest: digest(&session.messages),
        };
        let destination = folder.join(format!("{id}.jsonl"));
        let reused = if matching_import(&destination, &target, &provenance)? {
            true
        } else {
            let scratch = TempInstallDir::create_hidden_in(&folder)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(scratch.path(), fs::Permissions::from_mode(0o700))
                    .map_err(|err| err.to_string())?;
            }
            let staged = scratch.path().join("history.jsonl");
            write_native(&staged, &target, id, session, &provenance)
                .map_err(|err| err.to_string())?;
            match fs::hard_link(&staged, &destination) {
                Ok(()) => false,
                Err(err)
                    if err.kind() == io::ErrorKind::AlreadyExists
                        && matching_import(&destination, &target, &provenance)? =>
                {
                    true
                }
                Err(err) => {
                    return Err(format!(
                        "Could not save the Claude Code conversation: {err}"
                    ))
                }
            }
        };
        result.push(MigratedConversation {
            source_id: session.summary.id.clone(),
            target_id: id.to_string(),
            reused,
        });
    }
    Ok(result)
}

fn native_directory(target: &Path) -> Result<String, String> {
    let name = target
        .to_str()
        .ok_or("The Claude Code target folder is not valid UTF-8.")?;
    if cfg!(windows) {
        if let Some(unc) = name.strip_prefix(r"\\?\UNC\") {
            return Ok(format!(r"\\{unc}"));
        }
        return Ok(name.strip_prefix(r"\\?\").unwrap_or(name).to_owned());
    }
    Ok(name.to_owned())
}

fn project_slug(directory: &str) -> String {
    let mut slug = String::new();
    let mut hash = 0_i32;
    for unit in directory.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
        let character = u8::try_from(unit)
            .ok()
            .filter(u8::is_ascii_alphanumeric)
            .map(char::from)
            .unwrap_or('-');
        slug.push(character);
    }
    if slug.len() > 200 {
        slug.truncate(200);
        slug.push('-');
        let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let mut hash = hash.unsigned_abs();
        let mut suffix = Vec::new();
        loop {
            suffix.push(char::from(alphabet[(hash % 36) as usize]));
            hash /= 36;
            if hash == 0 {
                break;
            }
        }
        slug.extend(suffix.into_iter().rev());
    }
    slug
}

fn destination_folder(projects: &Path, target: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(projects).map_err(|err| err.to_string())?;
    let projects = projects.canonicalize().map_err(|err| err.to_string())?;
    let folder = projects.join(project_slug(target));
    fs::create_dir_all(&folder).map_err(|err| err.to_string())?;
    let canonical = folder.canonicalize().map_err(|err| err.to_string())?;
    if canonical.parent() != Some(projects.as_path()) {
        return Err("The Claude Code project folder resolves outside the selected profile.".into());
    }
    Ok(canonical)
}

fn imported_id(source: ConversationAgent, session: &str, directory: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(b"AgentBro portability v1\0");
    hasher.update(source.id());
    hasher.update(b"\0claude-code\0");
    hasher.update(session);
    hasher.update(b"\0");
    hasher.update(directory);
    let hash = hasher.finalize();
    let mut id = [0; 16];
    id.copy_from_slice(&hash[..16]);
    id[6] = (id[6] & 0x0f) | 0x80;
    id[8] = (id[8] & 0x3f) | 0x80;
    Uuid::from_bytes(id)
}

fn matching_import(path: &Path, directory: &str, provenance: &Provenance) -> Result<bool, String> {
    let file = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            fs::File::open(path).map_err(|err| err.to_string())?
        }
        Ok(_) => {
            return Err(
                "A different file already exists at the Claude Code session path.".to_string(),
            )
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err.to_string()),
    };
    let mut text = String::new();
    BufReader::new(file)
        .read_line(&mut text)
        .map_err(|err| err.to_string())?;
    let row: Value = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    let previous: Provenance = serde_json::from_value(row["agentbroMigration"].clone())
        .map_err(|_| "A different Claude Code session already has this ID.".to_string())?;
    if previous.source != provenance.source
        || previous.session_id != provenance.session_id
        || row["cwd"] != directory
        || row["sessionId"].as_str() != path.file_stem().and_then(|id| id.to_str())
    {
        return Err("A different Claude Code session already has this ID.".into());
    }
    Ok(true)
}

fn write_native(
    path: &Path,
    target: &str,
    id: Uuid,
    source: &PortableSession,
    provenance: &Provenance,
) -> io::Result<()> {
    let mut open = fs::OpenOptions::new();
    open.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.mode(0o600);
    }
    let mut file = BufWriter::new(open.open(path)?);
    let mut parent = None;
    let now = chrono::Utc::now().to_rfc3339();
    for (index, message) in source.messages.iter().enumerate() {
        let uuid = Uuid::new_v4();
        let (role, native) = match message.role {
            ChatRole::User => ("user", json!({"role":"user","content":message.text})),
            ChatRole::Assistant => (
                "assistant",
                json!({
                    "id":format!("msg_{}",uuid.simple()),"type":"message","role":"assistant","model":"<synthetic>",
                    "content":[{"type":"text","text":message.text}],"stop_reason":"end_turn","stop_sequence":null,
                    "usage":{"input_tokens":0,"output_tokens":0},
                }),
            ),
        };
        let mut row = json!({
            "parentUuid":parent,"isSidechain":false,"userType":"external","entrypoint":"cli",
            "cwd":target,"sessionId":id,"version":"2.0.0","type":role,"uuid":uuid,
            "timestamp":message.timestamp.as_deref().unwrap_or(&now),"message":native,
        });
        if index == 0 {
            row["agentbroMigration"] = serde_json::to_value(provenance)?;
        }
        serde_json::to_writer(&mut file, &row)?;
        file.write_all(b"\n")?;
        parent = Some(uuid);
    }
    if !source.summary.title.trim().is_empty() {
        serde_json::to_writer(
            &mut file,
            &json!({"type":"custom-title","customTitle":source.summary.title,"sessionId":id}),
        )?;
        file.write_all(b"\n")?;
    }
    file.flush()?;
    file.get_ref().sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_project_names_match_windows_unicode_and_long_paths() {
        assert_eq!(
            project_slug(r"C:\Users\me\中文 🎯.git"),
            "C--Users-me-------git"
        );
        assert_eq!(
            project_slug(&format!("/Projects/{}", "1234567890".repeat(22))),
            format!("-Projects-{}-pnqulk", "1234567890".repeat(19))
        );
        #[cfg(windows)]
        {
            assert_eq!(
                native_directory(Path::new(r"\\?\C:\Users\me")).unwrap(),
                r"C:\Users\me"
            );
            assert_eq!(
                native_directory(Path::new(r"\\?\UNC\server\projects")).unwrap(),
                r"\\server\projects"
            );
        }
    }

    #[test]
    fn reads_only_the_active_claude_branch_and_preserves_native_title() {
        let test = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let cwd = test.path().join("Source 项目");
        let projects = test.path().join("projects");
        let folder = projects.join("native-project");
        fs::create_dir_all(&folder).unwrap();
        fs::create_dir_all(&cwd).unwrap();
        let id = Uuid::new_v4();
        let path = folder.join(format!("{id}.jsonl"));
        let mut file = fs::File::create(&path).unwrap();
        for (uuid, parent, role, text) in [
            ("first", None, "user", "First question"),
            ("old", Some("first"), "assistant", "Discarded answer"),
            ("new", Some("first"), "assistant", "Current answer"),
            ("tail", Some("new"), "user", "Follow-up"),
        ] {
            writeln!(
                file,
                "{}",
                json!({
                    "uuid":uuid, "parentUuid":parent, "sessionId":id, "cwd":cwd,"type":role,
                    "message":{"role":role,"content":text}
                })
            )
            .unwrap();
        }
        writeln!(
            file,
            "{}",
            json!({
                "type":"assistant", "uuid":"other", "sessionId":id, "isSidechain":true,
                "message":{"role":"assistant","content":"Sidechain"}
            })
        )
        .unwrap();
        writeln!(
            file,
            "{}",
            json!({"type":"custom-title","sessionId":id,"customTitle":"Reviewed title"})
        )
        .unwrap();
        fs::write(
            folder.join("sessions-index.json"),
            json!({"entries":[{"sessionId":id,"isArchived":true}]}).to_string(),
        )
        .unwrap();
        drop(file);
        let scope = vec![cwd.canonicalize().unwrap()];
        let listing = catalog_at(&projects, Some(&scope)).unwrap();
        assert_eq!(listing[0].summary.title, "Reviewed title");
        assert!(listing[0].summary.archived);
        let loaded = messages(&listing[0]).unwrap();
        assert!(!loaded.partial);
        assert_eq!(
            loaded
                .messages
                .into_iter()
                .map(|message| message.text)
                .collect::<Vec<_>>(),
            ["First question", "Current answer", "Follow-up"]
        );
    }

    #[test]
    fn native_import_keeps_continued_history_on_reimport() {
        let test = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let cwd = test.path().join("返回项目");
        fs::create_dir_all(&cwd).unwrap();
        let cwd = cwd.canonicalize().unwrap();
        let profile = test.path().join("projects");
        let bundle = HandoffBundle {
            version: 1,
            source: ConversationAgent::Codex,
            target: ConversationAgent::ClaudeCode,
            project_root: "/original".to_string(),
            target_directory: cwd.to_str().unwrap().to_owned(),
            conversations: vec![PortableSession {
                summary: SessionSummary {
                    id: "native-original".to_string(),
                    title: "Preserved role sequence".to_string(),
                    cwd: "/original".to_string(),
                    archived: false,
                    updated_at: None,
                    directory_exists: false,
                },
                messages: [
                    (ChatRole::User, "Original question"),
                    (ChatRole::Assistant, "Original answer"),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (role, text))| PortableMessage {
                    id: index.to_string(),
                    role,
                    text: text.to_owned(),
                    timestamp: None,
                })
                .collect(),
                read_issue: None,
                git: None,
                return_context: None,
            }],
        };
        let first = import_at(&profile, &bundle).unwrap();
        assert!(!first[0].reused);
        let listing = catalog_at(&profile, Some(std::slice::from_ref(&cwd))).unwrap();
        let stored = listing[0].path.clone();
        let parent = fs::read_to_string(&stored)
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|value| value["uuid"].as_str().map(str::to_owned))
            .last()
            .unwrap();
        let mut file = fs::OpenOptions::new().append(true).open(&stored).unwrap();
        writeln!(
            file,
            "{}",
            json!({
                "uuid":"continued","parentUuid":parent,"sessionId":first[0].target_id,"type":"user",
                "cwd":native_directory(&cwd).unwrap(),
                "message":{"role":"user","content":"Continued locally in Claude Code"}
            })
        )
        .unwrap();
        drop(file);
        let before = fs::read(&stored).unwrap();
        let second = import_at(&profile, &bundle).unwrap();
        assert!(second[0].reused);
        assert_eq!(second[0].target_id, first[0].target_id);
        assert_eq!(fs::read(&stored).unwrap(), before);
        let history = messages(&listing[0]).unwrap();
        assert_eq!(history.messages.len(), 3);
        let context = history.context.unwrap();
        assert!(context.original_agent == ConversationAgent::Codex);
        assert_eq!(context.original_session_id, "native-original");
        assert_eq!(context.matching_messages, 2);
    }
}
