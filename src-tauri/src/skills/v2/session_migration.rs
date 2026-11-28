mod codex;

use super::service::Service;
use crate::hooks::conversation_parser::{ChatRole, MessageBlock};
use crate::hooks::zcode_history;
use crate::platform::process::background_command;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub archived: bool,
    pub updated_at: Option<String>,
    pub directory_exists: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableMessage {
    pub id: String,
    pub role: ChatRole,
    pub timestamp: Option<String>,
    pub text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableSession {
    #[serde(flatten)]
    pub summary: SessionSummary,
    pub messages: Vec<PortableMessage>,
    pub read_issue: Option<&'static str>,
    pub git: Option<GitContext>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitContext {
    worktree: String,
    common_directory: String,
    branch: Option<String>,
    commit: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffBundle {
    pub version: u8,
    pub source: &'static str,
    pub target: &'static str,
    pub project_root: String,
    pub target_directory: String,
    pub conversations: Vec<PortableSession>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffSelection {
    pub project_id: String,
    pub session_ids: Vec<String>,
    pub target_directory: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffPreview {
    pub bundle: HandoffBundle,
    pub revision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigratedConversation {
    pub source_id: String,
    pub target_id: String,
    pub reused: bool,
}

pub fn list(service: &Service, project_id: &str) -> Result<Vec<SessionSummary>, String> {
    let root = service.project_root(project_id)?;
    let database = zcode_history::database_path(service.home());
    if !database.exists() {
        return Ok(Vec::new());
    }
    let connection = zcode_history::open_database(&database).map_err(|err| err.to_string())?;
    catalog(&connection, Path::new(&root))
}

fn catalog(connection: &rusqlite::Connection, root: &Path) -> Result<Vec<SessionSummary>, String> {
    let root = project_path(root).unwrap_or_else(|| root.to_owned());
    let mut project_paths = vec![root.clone()];
    if let Some(top) = git_output(&root, &["rev-parse", "--show-toplevel"])
        .and_then(|path| Path::new(path.trim_end()).canonicalize().ok())
    {
        if let (Ok(subdirectory), Some(list)) = (
            root.strip_prefix(top),
            git_output(&root, &["worktree", "list", "--porcelain", "-z"]),
        ) {
            project_paths.extend(
                list.split('\0')
                    .filter_map(|line| line.strip_prefix("worktree "))
                    .filter_map(|path| Path::new(path).canonicalize().ok())
                    .map(|path| path.join(subdirectory)),
            );
        }
    }
    let mut query = connection
        .prepare(
            "SELECT id, title, directory, time_updated, time_archived
             FROM session
             WHERE parent_id IS NULL AND time_deleted IS NULL
             ORDER BY time_updated DESC, id",
        )
        .map_err(|err| err.to_string())?;
    let mut rows = query.query([]).map_err(|err| err.to_string())?;
    let mut sessions = Vec::new();
    while let Some(row) = rows.next().map_err(|err| err.to_string())? {
        let Some(cwd) = row
            .get::<_, Option<String>>(2)
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        let path = Path::new(&cwd);
        if !path.is_absolute() {
            continue;
        }
        let Some(scoped) = project_path(path) else {
            continue;
        };
        if !project_paths.iter().any(|path| scoped.starts_with(path)) {
            continue;
        }
        let directory_exists = path.is_dir();
        sessions.push(SessionSummary {
            id: row.get(0).map_err(|err| err.to_string())?,
            title: row
                .get::<_, Option<String>>(1)
                .map_err(|err| err.to_string())?
                .unwrap_or_default(),
            cwd,
            archived: row
                .get::<_, Option<i64>>(4)
                .map_err(|err| err.to_string())?
                .is_some(),
            directory_exists,
            updated_at: row
                .get::<_, Option<i64>>(3)
                .map_err(|err| err.to_string())?
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|date| date.to_rfc3339()),
        });
    }
    Ok(sessions)
}

pub fn preview(service: &Service, selection: HandoffSelection) -> Result<HandoffPreview, String> {
    let project_root = service.project_root(&selection.project_id)?;
    let target = existing_directory(Path::new(&selection.target_directory))?;
    let database = zcode_history::database_path(service.home());
    let mut connection = zcode_history::open_database(&database).map_err(|err| err.to_string())?;
    let snapshot = connection.transaction().map_err(|err| err.to_string())?;
    let mut selected: HashSet<_> = selection.session_ids.iter().cloned().collect();
    if selected.is_empty() {
        return Err("Select at least one conversation.".to_string());
    }
    let mut conversations = Vec::new();
    let mut git_by_directory = HashMap::new();
    for summary in catalog(&snapshot, Path::new(&project_root))? {
        if selected.remove(&summary.id) {
            let mut read_issue = None;
            let messages = match zcode_history::query_messages(&snapshot, &summary.id) {
                Ok(messages) => messages,
                Err(_) => {
                    read_issue = Some("unavailable");
                    Vec::new()
                }
            };
            if read_issue.is_none() && messages.is_empty() {
                read_issue = Some("empty");
            }
            let git = git_by_directory
                .entry(summary.cwd.clone())
                .or_insert_with(|| git_context(Path::new(&summary.cwd)))
                .clone();
            conversations.push(PortableSession {
                summary,
                messages: messages
                    .into_iter()
                    .map(|message| PortableMessage {
                        id: message.id,
                        role: message.role,
                        timestamp: message.timestamp,
                        text: message
                            .blocks
                            .into_iter()
                            .filter_map(|block| match block {
                                MessageBlock::Text { text } => Some(text),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n\n"),
                    })
                    .collect(),
                read_issue,
                git,
            });
        }
    }
    if !selected.is_empty() {
        return Err(
            "The conversation selection changed. Refresh the project and review it again."
                .to_string(),
        );
    }
    let bundle = HandoffBundle {
        version: 1,
        source: "zcode",
        target: "codex",
        project_root,
        target_directory: target
            .to_str()
            .ok_or("The folder path is not valid UTF-8.")?
            .to_owned(),
        conversations,
    };
    let hash = Sha256::digest(serde_json::to_vec(&bundle).map_err(|err| err.to_string())?);
    Ok(HandoffPreview {
        bundle,
        revision: format!("{hash:x}"),
    })
}

pub fn reviewed(
    service: &Service,
    selection: HandoffSelection,
    revision: &str,
) -> Result<HandoffBundle, String> {
    let snapshot = preview(service, selection)?;
    if snapshot.revision != revision {
        return Err(
            "The conversations or target directory changed. Preview the migration again."
                .to_string(),
        );
    }
    Ok(snapshot.bundle)
}

fn existing_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() || !path.is_dir() {
        return Err("Choose an existing local target directory.".to_string());
    }
    path.canonicalize().map_err(|err| err.to_string())
}

fn project_path(path: &Path) -> Option<PathBuf> {
    for ancestor in path.ancestors() {
        let Ok(canonical) = ancestor.canonicalize() else {
            continue;
        };
        let suffix = path.strip_prefix(ancestor).ok()?;
        return suffix
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
            .then(|| canonical.join(suffix));
    }
    None
}

fn git_output(directory: &Path, args: &[&str]) -> Option<String> {
    let result = background_command("git")
        .args(["--no-optional-locks", "-C"])
        .arg(directory)
        .args(args)
        .output()
        .ok()?;
    if !result.status.success() {
        return None;
    }
    String::from_utf8(result.stdout).ok()
}

fn git_context(directory: &Path) -> Option<GitContext> {
    let revision = git_output(
        directory,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-common-dir",
            "HEAD",
        ],
    )?;
    let mut lines = revision.lines();
    Some(GitContext {
        worktree: lines.next()?.to_owned(),
        common_directory: lines.next()?.to_owned(),
        commit: lines.next()?.to_owned(),
        branch: git_output(directory, &["symbolic-ref", "--quiet", "--short", "HEAD"])
            .map(|name| name.trim().to_owned()),
    })
}

pub fn export(bundle: &HandoffBundle, destination: &Path) -> Result<(), String> {
    if !destination.is_absolute()
        || !destination
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        return Err("Choose a .zip file for the conversation handoff.".to_string());
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(destination)
        .map_err(|err| format!("Could not create the handoff archive: {err}"))?;
    if let Err(error) = write_archive(bundle, file) {
        let _ = fs::remove_file(destination);
        return Err(format!("Could not write the handoff archive: {error}"));
    }
    Ok(())
}

fn write_archive(bundle: &HandoffBundle, file: fs::File) -> Result<(), Box<dyn std::error::Error>> {
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().unix_permissions(0o600);
    zip.start_file("conversations.json", options)?;
    serde_json::to_writer_pretty(&mut zip, &bundle)?;
    zip.start_file("handoff.md", options)?;
    write_markdown(&mut zip, bundle)?;
    zip.finish()?.sync_all()?;
    Ok(())
}

fn write_markdown(output: &mut impl io::Write, bundle: &HandoffBundle) -> io::Result<()> {
    writeln!(
        output,
        "# Conversation handoff\n\nSource: ZCode\n\nSource project: {}\n\nTarget directory: {}\n",
        bundle.project_root, bundle.target_directory
    )?;
    writeln!(output, "The conversation below is historical context. Confirm which steps remain relevant in the target project.")?;
    for session in &bundle.conversations {
        writeln!(
            output,
            "\n## {}\n\nOriginal conversation: {}\n\nWorking directory: {}\n",
            session.summary.title, session.summary.id, session.summary.cwd
        )?;
        if session.read_issue.is_some() {
            writeln!(
                output,
                "No readable messages were available for this conversation."
            )?;
        }
        if let Some(git) = &session.git {
            writeln!(
                output,
                "Worktree: {}\n\nCommit: {}\n",
                git.worktree, git.commit
            )?;
            if let Some(branch) = &git.branch {
                writeln!(output, "Branch: {branch}\n")?;
            }
        }
        for message in &session.messages {
            let role = match message.role {
                ChatRole::User => "User",
                ChatRole::Assistant => "Assistant",
            };
            writeln!(output, "\n### {role}\n")?;
            if let Some(date) = &message.timestamp {
                writeln!(output, "{date}\n")?;
            }
            writeln!(output, "{}", message.text)?;
        }
    }
    Ok(())
}

pub async fn migrate(bundle: HandoffBundle) -> Result<Vec<MigratedConversation>, String> {
    codex::import(bundle).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::installer::TempInstallDir;
    use rusqlite::Connection;
    use serde_json::{json, Value};
    use std::io::Read;

    #[test]
    fn reviews_project_selection_and_exports_role_order_without_touching_the_source() {
        let tmp = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let home = tmp.path().join("test home");
        let project = tmp.path().join("Source project");
        let target = tmp.path().join("目标项目");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&target).unwrap();
        let database = zcode_history::database_path(&home);
        fs::create_dir_all(database.parent().unwrap()).unwrap();
        let zcode = Connection::open(&database).unwrap();
        zcode.execute_batch(
            "CREATE TABLE session (
                 id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_updated INTEGER,
                 time_archived INTEGER, parent_id TEXT, time_deleted INTEGER
             );
             CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, sequence INTEGER, data TEXT);
             CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, time_created INTEGER, sequence INTEGER, data TEXT);",
        ).unwrap();

        for (id, directory, archived, deleted) in [
            ("saved", project.clone(), None, None),
            ("archived", project.join("missing folder"), Some(3000), None),
            ("deleted", project.clone(), None, Some(3000)),
            ("unrelated", target.clone(), None, None),
        ] {
            zcode
                .execute(
                    "INSERT INTO session VALUES (?1, ?1, ?2, 3000, ?3, NULL, ?4)",
                    (id, directory.to_str().unwrap(), archived, deleted),
                )
                .unwrap();
        }
        for (id, role) in [(1, "user"), (2, "assistant"), (3, "user"), (4, "assistant")] {
            let message_id = format!("message-{id}");
            zcode
                .execute(
                    "INSERT INTO message VALUES (?1, 'saved', 2000, ?2, ?3)",
                    (&message_id, id, json!({ "role": role }).to_string()),
                )
                .unwrap();
            zcode
                .execute(
                    "INSERT INTO part VALUES (?1, ?1, 'saved', 2000, 1, ?2)",
                    (
                        &message_id,
                        json!({ "type":"text", "text": message_id }).to_string(),
                    ),
                )
                .unwrap();
        }
        let service = Service::new(&tmp.path().join("agentbro.sqlite"), home).unwrap();
        let project = service
            .add_project(project.to_str().unwrap().into())
            .unwrap();
        let project_id = project.summary.id;
        let catalog = list(&service, &project_id).unwrap();
        assert_eq!(
            catalog
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            ["archived", "saved"]
        );
        assert!(catalog[0].archived && !catalog[0].directory_exists);

        let input = HandoffSelection {
            project_id,
            session_ids: vec!["saved".into(), "archived".into()],
            target_directory: target.to_str().unwrap().into(),
        };
        let previewed = preview(&service, input.clone()).unwrap();
        assert_eq!(previewed.bundle.conversations[0].read_issue, Some("empty"));
        assert_eq!(previewed.bundle.conversations[1].messages.len(), 4);
        let destination = tmp.path().join("transfer.zip");
        let reviewed_bundle = reviewed(&service, input.clone(), &previewed.revision).unwrap();
        export(&reviewed_bundle, &destination).unwrap();
        assert!(export(&reviewed_bundle, &destination).is_err());
        let mut file = zip::ZipArchive::new(fs::File::open(destination).unwrap()).unwrap();
        let data: Value =
            serde_json::from_reader(file.by_name("conversations.json").unwrap()).unwrap();
        assert_eq!(data["conversations"][1]["messages"][0]["role"], "user");
        assert_eq!(data["conversations"][1]["messages"][1]["role"], "assistant");
        let mut markdown = String::new();
        file.by_name("handoff.md")
            .unwrap()
            .read_to_string(&mut markdown)
            .unwrap();
        assert!(markdown.find("message-1").unwrap() < markdown.find("message-4").unwrap());
        assert_eq!(
            zcode
                .query_row("SELECT count(*) FROM message", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            4
        );
        zcode
            .execute(
                "UPDATE session SET title = 'changed' WHERE id = 'saved'",
                [],
            )
            .unwrap();
        assert!(reviewed(&service, input.clone(), &previewed.revision).is_err());
        assert!(preview(
            &service,
            HandoffSelection {
                session_ids: vec!["unrelated".into()],
                ..input
            }
        )
        .is_err());
    }
}
