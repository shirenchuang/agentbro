use super::{git_context, source_summaries, ConversationAgent, GitContext};
use crate::skills::v2::fsutil::normalized_path;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDiscovery {
    pub candidates: Vec<ProjectCandidate>,
    pub unavailable_sources: Vec<ConversationAgent>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCandidate {
    pub path: String,
    pub name: String,
    pub sources: Vec<ConversationAgent>,
    pub last_used_at: Option<String>,
    pub directory_available: bool,
    pub registered_project_id: Option<String>,
    pub git: Option<GitContext>,
}

pub(in crate::skills::v2) fn discover(
    home: &Path,
    registered: Vec<(String, String)>,
) -> ProjectDiscovery {
    let mut report = ProjectDiscovery {
        candidates: Vec::new(),
        unavailable_sources: Vec::new(),
    };
    let mut by_directory = HashMap::new();
    for source in [
        ConversationAgent::Codex,
        ConversationAgent::ClaudeCode,
        ConversationAgent::Zcode,
        ConversationAgent::Antigravity,
    ] {
        match source_summaries(home, source, None) {
            Ok(rows) => insert_source(
                &mut by_directory,
                source,
                rows.into_iter()
                    .filter(|row| !row.archived)
                    .map(|row| (row.cwd, row.updated_at)),
            ),
            Err(error) => {
                log::debug!("Project discovery could not read a session source: {error}");
                report.unavailable_sources.push(source);
            }
        }
    }
    let registered: HashMap<_, _> = registered
        .into_iter()
        .map(|(path, id)| (normalized_path(Path::new(&path)), id))
        .collect();
    let mut grouped = by_directory.into_iter().collect::<Vec<_>>();
    grouped.sort_by(|(left_path, left), (right_path, right)| {
        right
            .last_used_at
            .cmp(&left.last_used_at)
            .then_with(|| left_path.cmp(right_path))
    });
    report.candidates = grouped
        .into_iter()
        .take(30)
        .map(|(path, activity)| {
            let directory_available = path.is_dir();
            let git = directory_available.then(|| git_context(&path)).flatten();
            ProjectCandidate {
                name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_else(|| path.to_str().unwrap_or_default())
                    .to_owned(),
                path: path.to_string_lossy().into_owned(),
                sources: activity.sources,
                last_used_at: activity.last_used_at,
                directory_available,
                registered_project_id: registered.get(&path).cloned(),
                git,
            }
        })
        .collect();
    report
}

#[derive(Default)]
struct DirectoryActivity {
    last_used_at: Option<String>,
    sources: Vec<ConversationAgent>,
}

fn insert_source(
    directories: &mut HashMap<PathBuf, DirectoryActivity>,
    source: ConversationAgent,
    rows: impl IntoIterator<Item = (String, Option<String>)>,
) {
    let mut normalized = HashMap::new();
    for (raw_path, last_used_at) in rows {
        if !Path::new(&raw_path).is_absolute() {
            continue;
        }
        let path = normalized
            .entry(raw_path)
            .or_insert_with_key(|key| normalized_path(Path::new(key)));
        let existing = directories.entry(path.clone()).or_default();
        if !existing.sources.contains(&source) {
            existing.sources.push(source);
        }
        if existing.last_used_at < last_used_at {
            existing.last_used_at = last_used_at;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::installer::TempInstallDir;

    #[test]
    fn merges_path_aliases_across_agents_and_keeps_same_named_workspaces_distinct() {
        let scratch = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let left = scratch.path().join("left/repo");
        let right = scratch.path().join("right/repo");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let mut candidates = HashMap::new();
        insert_source(
            &mut candidates,
            ConversationAgent::Codex,
            [
                (
                    left.display().to_string(),
                    Some("2026-11-27T10:00:00Z".into()),
                ),
                (right.display().to_string(), None),
            ],
        );
        insert_source(
            &mut candidates,
            ConversationAgent::ClaudeCode,
            [
                (
                    left.join("../repo").display().to_string(),
                    Some("2026-11-28T10:00:00Z".into()),
                ),
                (left.display().to_string(), None),
                ("relative/project".into(), None),
            ],
        );
        assert_eq!(candidates.len(), 2);
        let entry = &candidates[&left.canonicalize().unwrap()];
        assert!(entry.sources == [ConversationAgent::Codex, ConversationAgent::ClaudeCode]);
        assert_eq!(entry.last_used_at.as_deref(), Some("2026-11-28T10:00:00Z"));
        assert!(candidates[&right.canonicalize().unwrap()].sources == [ConversationAgent::Codex]);
    }
}
