use super::models::{
    LinkedSkillPackManifest, SkillPackFingerprint, SkillPackGitHubSource, SkillPackManifest,
};
use super::project_lock::{
    checked_source_skill, fingerprint_canonical_skill_directory, locked_skill_directory,
    stage_skill_copy, valid_skill_name,
};
use crate::skills::installer::{self, TempInstallDir};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(super) const MAX_BYTES: usize = 1024 * 1024;
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
    decode(&bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<SkillPackManifest, String> {
    check_size(bytes.len())?;
    let manifest = serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes))
        .map_err(|error| format!("Invalid Skill Pack JSON. {error}"))?;
    validate(&manifest)?;
    Ok(manifest)
}

fn github_file_url(link: &str) -> Result<reqwest::Url, String> {
    let invalid = || "Paste an HTTPS link to a file in a public GitHub repository.".to_string();
    let link = link.trim();
    if link.contains('\\') {
        return Err(invalid());
    }
    let url = reqwest::Url::parse(link).map_err(|_| invalid())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(invalid());
    }
    let segments: Vec<_> = url.path_segments().ok_or_else(invalid)?.collect();
    let (owner, repo, file) = match (url.host_str(), segments.as_slice()) {
        (Some("github.com"), [owner, repo, "blob" | "raw", file @ ..]) => (owner, repo, file),
        (Some("raw.githubusercontent.com"), [owner, repo, file @ ..]) => (owner, repo, file),
        _ => return Err(invalid()),
    };
    if !github_repository_component(owner)
        || !github_repository_component(repo)
        || file.len() < 2
        || file.iter().any(|part| part.is_empty())
    {
        return Err(invalid());
    }
    reqwest::Url::parse(&format!(
        "https://raw.githubusercontent.com/{owner}/{repo}/{}",
        file.join("/")
    ))
    .map_err(|_| invalid())
}

pub(super) async fn read_link(link: &str) -> Result<LinkedSkillPackManifest, String> {
    let url = github_file_url(link)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client.get(url.clone()).send().await.map_err(|error| {
        format!(
            "Could not download the GitHub file: {}",
            error.without_url()
        )
    })?;
    Ok(LinkedSkillPackManifest {
        manifest: read_link_response(response).await?,
        source_url: url.into(),
    })
}

async fn read_link_response(mut response: reqwest::Response) -> Result<SkillPackManifest, String> {
    if !response.status().is_success() {
        return Err(format!(
            "Could not open the public GitHub file (HTTP {}). Check the file link and repository visibility.",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_BYTES as u64)
    {
        return Err("Skill Pack JSON must be 1 MiB or smaller.".to_string());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Could not read the GitHub file: {}", error.without_url()))?
    {
        check_size(bytes.len() + chunk.len())?;
        bytes.extend_from_slice(&chunk);
    }
    decode(&bytes)
}

pub(super) fn write(path: &Path, manifest: &SkillPackManifest) -> Result<(), String> {
    let bytes = encode(manifest)?;
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or("Choose an export file inside a folder.")?;
    let staging = TempInstallDir::create_in(parent)?;
    let staged = staging.path().join("pack.json");
    fs::write(&staged, bytes).map_err(|error| error.to_string())?;
    fs::rename(&staged, path).map_err(|error| error.to_string())
}

pub(super) fn encode(manifest: &SkillPackManifest) -> Result<Vec<u8>, String> {
    validate(manifest)?;
    let mut bytes = serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    check_size(bytes.len())?;
    Ok(bytes)
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
    if !matches!(manifest.version, 1..=4) {
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
    if manifest.version < 4 && !manifest.suggested_agent_ids.is_empty() {
        return Err("Suggested Agents require Skill Pack version 4.".to_string());
    }
    let mut suggested = HashSet::new();
    for id in &manifest.suggested_agent_ids {
        if id == "agents"
            || id.len() > 64
            || !id.split('-').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            })
            || !suggested.insert(id)
        {
            return Err("Suggested Agent IDs must be unique lowercase identifiers.".to_string());
        }
    }
    Ok(())
}

fn github_repository_component(part: &str) -> bool {
    !part.is_empty()
        && !part.ends_with('.')
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

pub(super) fn verify_export_sources(
    fingerprints: &mut [SkillPackFingerprint],
    mut fetch: impl FnMut(&str, &str) -> Result<(PathBuf, TempInstallDir), String>,
) {
    let mut groups = BTreeMap::<_, Vec<_>>::new();
    for member in fingerprints {
        if let Some(source) = member.source.take() {
            groups
                .entry((source.git_url(), source.commit.clone()))
                .or_default()
                .push((member, source));
        }
    }

    for ((repository, commit), members) in groups {
        let Ok((checkout, temporary)) = fetch(&repository, &commit) else {
            continue;
        };
        let Ok(checkout) = verified_checkout(&checkout, &commit) else {
            continue;
        };
        for (member, source) in members {
            let Some(expected) = member.fingerprint.as_deref() else {
                continue;
            };
            let Ok(staging) = TempInstallDir::create_in(temporary.path()) else {
                continue;
            };
            if source
                .stage_verified_skill(&checkout, &staging.path().join("skill"), expected)
                .is_ok()
            {
                member.source = Some(source);
            }
        }
    }
}

pub(super) fn verified_checkout(checkout: &Path, commit: &str) -> Result<PathBuf, String> {
    if !installer::read_checkout_commit(checkout)
        .is_some_and(|actual| actual.eq_ignore_ascii_case(commit))
    {
        return Err("Git did not return the recorded Skill commit.".to_string());
    }
    checkout.canonicalize().map_err(|error| error.to_string())
}

impl SkillPackGitHubSource {
    fn validate(&self) -> Result<(), String> {
        if !self
            .repository
            .split_once('/')
            .is_some_and(|(owner, repo)| {
                github_repository_component(owner)
                    && github_repository_component(repo)
                    && !repo.ends_with(".git")
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

    pub(super) fn stage_verified_skill(
        &self,
        checkout: &Path,
        staged: &Path,
        expected: &str,
    ) -> Result<PathBuf, String> {
        let directory = checked_source_skill(checkout, &checkout.join(self.directory()?))?;
        super::fsutil::copy_dir_recursive(&directory, staged)?;
        let fingerprint = || {
            let canonical = staged.canonicalize().map_err(|error| error.to_string())?;
            fingerprint_canonical_skill_directory(&canonical, false)
        };
        if fingerprint()? != expected {
            // SSH imports can retain files that the local library omits.
            super::fsutil::remove_path(staged)?;
            stage_skill_copy(&directory, staged, false)?;
            if fingerprint()? != expected {
                return Err("Skill content does not match the recorded fingerprint.".to_string());
            }
        }
        Ok(directory)
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
        if let Some(occupied) = &member.occupied_by {
            return Err(format!(
                "The destination for '{id}' is occupied by '{occupied}'."
            ));
        }
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
            suggested_agent_ids: Vec::new(),
        }
    }

    #[test]
    fn migration_pack_preserves_only_valid_suggested_agent_ids() {
        let mut pack = manifest();
        pack.suggested_agent_ids = vec!["claude-code".into(), "codex".into()];
        assert!(validate(&pack).unwrap_err().contains("version 4"));

        pack.version = 4;
        let json = serde_json::to_vec(&pack).unwrap();
        assert_eq!(decode(&json).unwrap(), pack);

        for invalid in ["", "../codex", "Claude-Code", "-codex", "agents", "codex"] {
            let mut invalid_pack = pack.clone();
            invalid_pack.suggested_agent_ids.push(invalid.into());
            assert!(validate(&invalid_pack).unwrap_err().contains("Agent IDs"));
        }
    }

    #[test]
    fn github_pack_links_preserve_branch_and_file_paths() {
        for link in [
            " https://github.com/owner/repo/blob/feature%2Freview/技能包.json?plain=1#L2 ",
            "https://github.com/owner/repo/raw/feature%2Freview/技能包.json",
            "https://raw.githubusercontent.com/owner/repo/feature%2Freview/技能包.json?raw=1",
        ] {
            assert_eq!(
                github_file_url(link).unwrap().as_str(),
                "https://raw.githubusercontent.com/owner/repo/feature%2Freview/%E6%8A%80%E8%83%BD%E5%8C%85.json"
            );
        }
        let commit = "a".repeat(40);
        for reference in ["refs/heads/feature/review", &commit] {
            let link = format!("https://github.com/owner/repo/blob/{reference}/team/pack.json");
            assert_eq!(
                github_file_url(&link).unwrap().as_str(),
                format!("https://raw.githubusercontent.com/owner/repo/{reference}/team/pack.json")
            );
        }
        for link in [
            "http://github.com/owner/repo/blob/main/pack.json",
            "https://github.com.evil.test/owner/repo/blob/main/pack.json",
            "https://user:secret@raw.githubusercontent.com/owner/repo/main/pack.json",
            "https://github.com:444/owner/repo/blob/main/pack.json",
            "https://github.com/owner/repo/tree/main/pack.json",
            "https://github.com/owner/repo/blob/main/",
            "https://raw.githubusercontent.com/owner/repo/main//pack.json",
            "file:///tmp/pack.json",
        ] {
            assert!(github_file_url(link).is_err(), "{link}");
        }
    }

    async fn fixture_response(status: &str, headers: &str, bytes: Vec<u8>) -> reqwest::Response {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let header = format!("HTTP/1.1 {status}\r\nConnection: close\r\n{headers}\r\n");
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            assert!(stream.read(&mut request).await.unwrap() > 0);
            stream.write_all(header.as_bytes()).await.unwrap();
            let _ = stream.write_all(&bytes).await;
        });
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
            .get(format!("http://{address}/fixture.json"))
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn github_pack_downloads_use_the_shared_schema_and_bound_the_body() {
        let manifest = manifest();
        let body = [
            b"\xef\xbb\xbf".as_slice(),
            &serde_json::to_vec(&manifest).unwrap(),
        ]
        .concat();
        let response = fixture_response("200 OK", "", body).await;
        assert_eq!(read_link_response(response).await.unwrap(), manifest);

        let response = fixture_response("200 OK", "", b"<html>Not a Pack</html>".to_vec()).await;
        assert!(read_link_response(response)
            .await
            .unwrap_err()
            .contains("Invalid Skill Pack JSON"));
        let response = fixture_response("200 OK", "", vec![b' '; MAX_BYTES + 1]).await;
        assert!(read_link_response(response)
            .await
            .unwrap_err()
            .contains("1 MiB"));
        let response = fixture_response(
            "302 Found",
            "Location: http://127.0.0.1:1/unwanted\r\nContent-Length: 0\r\n",
            Vec::new(),
        )
        .await;
        assert!(read_link_response(response)
            .await
            .unwrap_err()
            .contains("HTTP 302"));
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
