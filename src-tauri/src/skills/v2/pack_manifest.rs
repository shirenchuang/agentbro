use super::models::{SkillPackFingerprint, SkillPackGitHubSource, SkillPackManifest};
use super::project_lock::{locked_skill_directory, valid_skill_name};
use crate::skills::installer::TempInstallDir;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::Path;

const MAX_BYTES: usize = 1024 * 1024;
const FORMAT: &str = "agentbro-skill-pack";

pub(super) fn read(path: &Path) -> Result<SkillPackManifest, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("Choose a regular Skill Pack JSON file.".to_string());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    check_size(bytes.len())?;
    let manifest = serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes))
        .map_err(|error| format!("Invalid Skill Pack JSON. {error}"))?;
    validate(&manifest)?;
    Ok(manifest)
}

pub(super) fn write(path: &Path, manifest: &SkillPackManifest) -> Result<(), String> {
    validate(manifest)?;
    let mut bytes = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    check_size(bytes.len())?;
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or("Choose an export file inside a folder.")?;
    let staging = TempInstallDir::create_in(parent)?;
    let staged = staging.path().join("pack.json");
    fs::write(&staged, bytes).map_err(|error| error.to_string())?;
    fs::rename(&staged, path).map_err(|error| error.to_string())
}

fn check_size(bytes: usize) -> Result<(), String> {
    if bytes > MAX_BYTES {
        return Err("Skill Pack JSON must be 1 MiB or smaller.".to_string());
    }
    Ok(())
}

pub(super) fn validate(manifest: &SkillPackManifest) -> Result<(), String> {
    if manifest.format != FORMAT {
        return Err("This file does not describe an AgentBro Skill Pack.".to_string());
    }
    if !matches!(manifest.version, 1..=3) {
        return Err(format!(
            "Unsupported Skill Pack version {}.",
            manifest.version
        ));
    }
    if manifest.name.trim().is_empty() {
        return Err("Skill Pack name is required.".to_string());
    }
    let mut seen = HashSet::new();
    for id in &manifest.skill_ids {
        if id.is_empty() || id.trim() != id || id.chars().any(char::is_control) {
            return Err("Each Skill ID must be nonempty and have no surrounding spaces or control characters.".to_string());
        }
        if !seen.insert(id.as_str()) {
            return Err(format!("Duplicate Skill ID '{id}'."));
        }
    }
    if manifest.version == 1 && !manifest.skill_fingerprints.is_empty() {
        return Err("Skill fingerprints require Skill Pack version 2.".to_string());
    }
    for (id, hash) in &manifest.skill_fingerprints {
        if !seen.contains(id.as_str())
            || hash.len() != 64
            || !hash
                .bytes()
                .all(|ch| ch.is_ascii_digit() || (b'a'..=b'f').contains(&ch))
        {
            return Err(format!("Invalid Skill fingerprint for '{id}'."));
        }
    }
    if manifest.version < 3 && !manifest.skill_sources.is_empty() {
        return Err("Pinned GitHub sources require Skill Pack version 3.".to_string());
    }
    for (id, source) in &manifest.skill_sources {
        if !valid_skill_name(id)
            || !manifest.skill_fingerprints.contains_key(id)
            || source.validate().is_err()
        {
            return Err(format!("Invalid pinned GitHub source for '{id}'."));
        }
    }
    Ok(())
}

impl SkillPackGitHubSource {
    fn validate(&self) -> Result<(), String> {
        let component = |part: &str| {
            !part.is_empty()
                && !part.ends_with('.')
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        };
        if !self
            .repository
            .split_once('/')
            .is_some_and(|(owner, repo)| {
                component(owner) && component(repo) && !repo.ends_with(".git")
            })
            || !matches!(self.commit.len(), 40 | 64)
            || !self.commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Invalid GitHub repository or commit".to_string());
        }
        self.directory()?;
        Ok(())
    }

    pub(super) fn from_recorded_uri(uri: &str, commit: &str) -> Option<Self> {
        let (path, encoded) = if let Some(path) = uri.strip_prefix("https://github.com/") {
            (path, true)
        } else {
            (uri.strip_prefix("github:")?, false)
        };
        if path.contains(['?', '#']) {
            return None;
        }
        let mut parts = path.trim_end_matches('/').split('/');
        let owner = parts.next()?;
        let repo = parts.next()?.trim_end_matches(".git");
        let mut parts: Vec<&str> = parts.collect();
        if parts.first() == Some(&"tree") {
            if parts.len() < 2 || parts[1].is_empty() {
                return None;
            }
            parts.drain(..2);
        }
        let mut segments = parts
            .into_iter()
            .map(|part| {
                let segment = if encoded {
                    crate::percent_decode_path(part).ok()?
                } else {
                    part.to_string()
                };
                (!segment.contains('/')).then_some(segment)
            })
            .collect::<Option<Vec<_>>>()?;
        if segments.last().map(String::as_str) != Some("SKILL.md") {
            segments.push("SKILL.md".to_string());
        }
        let pin = Self {
            repository: format!("{owner}/{repo}"),
            commit: commit.to_ascii_lowercase(),
            skill_path: segments.join("/"),
        };
        pin.validate().ok()?;
        Some(pin)
    }

    pub(super) fn directory(&self) -> Result<std::path::PathBuf, String> {
        locked_skill_directory(&self.skill_path)
            .map_err(|_| "Invalid repository-relative Skill path".to_string())
    }

    pub(super) fn git_url(&self) -> String {
        format!("https://github.com/{}.git", self.repository)
    }

    pub(super) fn source_uri(&self) -> Result<String, String> {
        let mut url =
            reqwest::Url::parse("https://github.com").map_err(|error| error.to_string())?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| "Invalid GitHub source URL".to_string())?;
            segments
                .extend(self.repository.split('/'))
                .extend(["tree", &self.commit]);
            if let Some((directory, _)) = self.skill_path.rsplit_once('/') {
                segments.extend(directory.split('/'));
            }
        }
        Ok(url.to_string())
    }
}

pub(super) fn validate_review(
    manifest: &SkillPackManifest,
    reviewed: &[SkillPackFingerprint],
    current: &[SkillPackFingerprint],
    accept_local_copies: bool,
) -> Result<(), String> {
    if current != reviewed {
        return Err(
            "The selected library changed since review. Recheck the Skill Pack.".to_string(),
        );
    }
    for member in current {
        let id = &member.skill_id;
        if member.missing {
            return Err(format!("Skill '{id}' is missing from the center library."));
        }
        let fingerprint = member
            .fingerprint
            .as_ref()
            .ok_or_else(|| format!("Could not verify Skill '{id}' in the center library."))?;
        if !accept_local_copies && manifest.skill_fingerprints.get(id) != Some(fingerprint) {
            return Err(format!(
                "Confirm the local copy of Skill '{id}' before importing."
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> SkillPackManifest {
        SkillPackManifest {
            format: FORMAT.to_string(),
            version: 1,
            name: "团队 Pack".to_string(),
            description: "Review and testing".to_string(),
            tags: vec!["团队".to_string()],
            skill_ids: vec!["review".to_string(), "a-test".to_string()],
            skill_fingerprints: Default::default(),
            skill_sources: Default::default(),
        }
    }

    #[test]
    fn round_trips_an_ordered_pack_in_a_unicode_path() {
        let directory = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = directory.path().join("技能包, team.agentbro-pack.json");
        let expected = manifest();
        write(&path, &expected).unwrap();
        assert_eq!(read(&path).unwrap(), expected);
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.ends_with(b"\n"));
        write(&path, &expected).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::write(&path, [b"\xef\xbb\xbf".as_slice(), &bytes].concat()).unwrap();
        assert_eq!(read(&path).unwrap(), expected);
    }

    #[test]
    fn version_two_fingerprints_round_trip_and_require_a_member_and_valid_hash() {
        let directory = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = directory.path().join("pack.json");
        let mut v2 = manifest();
        v2.version = 2;
        v2.skill_fingerprints
            .insert("review".to_string(), "a".repeat(64));
        write(&path, &v2).unwrap();
        assert_eq!(read(&path).unwrap(), v2);

        v2.version = 1;
        assert!(validate(&v2).is_err());
        v2.version = 2;
        v2.skill_fingerprints
            .insert("not-a-member".to_string(), "b".repeat(64));
        assert!(validate(&v2).is_err());
        v2.skill_fingerprints.remove("not-a-member");
        v2.skill_fingerprints
            .insert("review".to_string(), "no-hash".to_string());
        assert!(validate(&v2).is_err());
    }

    #[test]
    fn pinned_pack_validates_github_repository_and_directory_in_version_three() {
        let mut manifest = manifest();
        manifest.version = 3;
        manifest
            .skill_fingerprints
            .insert("review".to_string(), "a".repeat(64));
        let valid = SkillPackGitHubSource {
            repository: "example/workflows".to_string(),
            commit: "b".repeat(40),
            skill_path: "skills/review/SKILL.md".to_string(),
        };
        manifest
            .skill_sources
            .insert("review".to_string(), valid.clone());
        assert!(validate(&manifest).is_ok());
        manifest.version = 2;
        assert!(validate(&manifest).is_err());
        manifest.version = 3;
        for invalid in [
            SkillPackGitHubSource {
                repository: "https://user@github.com/example/workflows".to_string(),
                ..valid.clone()
            },
            SkillPackGitHubSource {
                commit: "main".to_string(),
                ..valid.clone()
            },
            SkillPackGitHubSource {
                skill_path: "../review/SKILL.md".to_string(),
                ..valid.clone()
            },
            SkillPackGitHubSource {
                skill_path: "skills/.git/SKILL.md".to_string(),
                ..valid.clone()
            },
        ] {
            manifest.skill_sources.insert("review".to_string(), invalid);
            assert!(validate(&manifest).is_err());
        }
        assert!(SkillPackGitHubSource::from_recorded_uri(
            "https://github.com/example/workflows/tree/main/skills/review?token=fixture",
            &valid.commit,
        )
        .is_none());
    }

    #[test]
    fn rejects_invalid_files_and_preserves_a_valid_export() {
        let directory = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let export = directory.path().join("export.json");
        let import = directory.path().join("import.json");
        let mut invalid = manifest();
        write(&export, &invalid).unwrap();
        invalid.skill_ids.push("review".to_string());
        assert!(write(&export, &invalid).unwrap_err().contains("Duplicate"));
        assert_eq!(read(&export).unwrap(), manifest());

        invalid.version = 42;
        fs::write(&import, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(read(&import).unwrap_err().contains("version 42"));
        fs::write(&import, br#"{"id":"default","targets":[]}"#).unwrap();
        assert!(read(&import).is_err());
        fs::write(&import, vec![b' '; MAX_BYTES + 1]).unwrap();
        assert!(read(&import).unwrap_err().contains("1 MiB"));
    }
}
