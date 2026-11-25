use super::models::{
    ProjectLockedSkill, ProjectLockedSkillStatus, ProjectSkillLock, ProjectSkillRestoreResult,
};
use crate::data_dir::rename_path_if_absent;
use crate::skills::installer::{self, TempInstallDir};
use regex::Regex;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const LOCK_FILE: &str = "skills-lock.json";
const LOCK_LIMIT: u64 = 2 * 1024 * 1024;

#[derive(Deserialize)]
struct LockFile {
    version: u32,
    skills: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockEntry {
    source: String,
    source_url: Option<String>,
    computed_hash: String,
    #[serde(rename = "ref")]
    reference: Option<String>,
    commit: Option<String>,
    skill_name: Option<String>,
    skill_path: Option<String>,
}

#[derive(Deserialize)]
struct NodeModuleHints {
    version: Option<String>,
    via: Option<String>,
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct RepositoryRevision {
    url: String,
    reference: String,
}

struct RestoreSource {
    location: RestoreLocation,
    hash: String,
}

enum RestoreLocation {
    Git(GitSkill),
    NodeModules(NodeModuleSkill),
}

struct GitSkill {
    repository: RepositoryRevision,
    commit: Option<String>,
    subpath: PathBuf,
    skill_path: Option<PathBuf>,
    skill_name: String,
}

struct NodeModuleSkill {
    package: PathBuf,
    declared_by: Option<PathBuf>,
    version: Option<String>,
    skill_path: PathBuf,
}

fn read_lock(root: &Path) -> Result<Option<(LockFile, String)>, String> {
    let path = root.join(LOCK_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not inspect {LOCK_FILE}: {error}")),
    };
    if !metadata.is_file() {
        return Err(format!("{LOCK_FILE} must be a regular file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .and_then(|file| file.take(LOCK_LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("Could not read {LOCK_FILE}: {error}"))?;
    if bytes.len() as u64 > LOCK_LIMIT {
        return Err(format!("{LOCK_FILE} exceeds 2 MiB"));
    }
    let revision = format!("{:x}", Sha256::digest(&bytes));
    let lock =
        serde_json::from_slice(&bytes).map_err(|error| format!("Invalid {LOCK_FILE}: {error}"))?;
    Ok(Some((lock, revision)))
}

pub(super) fn preview(root: &Path) -> Option<ProjectSkillLock> {
    use ProjectLockedSkillStatus::{Existing, Invalid};

    let (lock, revision) = match read_lock(root) {
        Ok(Some(file)) => file,
        Ok(None) => return None,
        Err(error) => {
            return Some(ProjectSkillLock {
                version: None,
                revision: String::new(),
                skills: Vec::new(),
                error: Some(error),
            })
        }
    };
    let destination = root.join(".agents").join("skills");
    let duplicates = duplicate_names(lock.skills.keys());
    Some(ProjectSkillLock {
        version: Some(lock.version),
        revision,
        skills: lock
            .skills
            .iter()
            .map(|(name, entry)| {
                let source_type = entry.get("sourceType").and_then(|value| value.as_str());
                let source = match source_type {
                    Some("git" | "gitlab") => entry
                        .get("sourceUrl")
                        .and_then(|value| value.as_str())
                        .or_else(|| entry.get("source").and_then(|value| value.as_str())),
                    _ => entry.get("source").and_then(|value| value.as_str()),
                }
                .unwrap_or("");
                let reference = if source_type == Some("node_modules") {
                    entry.get("version").and_then(|value| value.as_str())
                } else {
                    ["commit", "ref"]
                        .into_iter()
                        .find_map(|key| entry.get(key).and_then(|value| value.as_str()))
                };
                let status = if !valid_skill_name(name) || duplicates.contains(&name.to_lowercase())
                {
                    Invalid
                } else if fs::symlink_metadata(destination.join(name)).is_ok() {
                    Existing
                } else {
                    restore_source(lock.version, name, entry)
                        .map_or_else(|status| status, |_| ProjectLockedSkillStatus::Ready)
                };
                ProjectLockedSkill {
                    name: name.clone(),
                    source: safe_display_source(source),
                    reference: reference.map(ToOwned::to_owned),
                    status,
                }
            })
            .collect(),
        error: None,
    })
}

fn safe_display_source(source: &str) -> String {
    if let Ok(url) = reqwest::Url::parse(source) {
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return url.host_str().unwrap_or_default().to_string();
        }
        if url.scheme() == "https" {
            return url.into();
        }
    }
    source.to_string()
}

fn duplicate_names<'a>(names: impl Iterator<Item = &'a String>) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    names
        .map(|name| name.to_lowercase())
        .filter(|name| !seen.insert(name.clone()))
        .collect()
}

pub(super) fn valid_component(name: &str) -> bool {
    !name.is_empty()
        && !name.ends_with(['.', ' '])
        && !name
            .chars()
            .any(|ch| ch.is_control() || "<>:\"/\\|?*".contains(ch))
        && !matches!(
            name.split('.')
                .next()
                .unwrap_or_default()
                .trim_end_matches(' ')
                .to_ascii_uppercase()
                .as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "CONIN$"
                | "CONOUT$"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "COM¹"
                | "COM²"
                | "COM³"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
                | "LPT¹"
                | "LPT²"
                | "LPT³"
        )
}

pub(super) fn valid_skill_name(name: &str) -> bool {
    valid_component(name) && !name.starts_with(['.', ' '])
}

fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn restore_source(
    version: u32,
    name: &str,
    raw: &serde_json::Value,
) -> Result<RestoreSource, ProjectLockedSkillStatus> {
    use ProjectLockedSkillStatus::{Invalid, Legacy, Unsupported};

    if version == 1 {
        return Err(Legacy);
    }
    let source_type = match raw.get("sourceType").and_then(|value| value.as_str()) {
        Some(kind @ ("github" | "gitlab" | "git" | "node_modules")) if version == 2 => kind,
        _ => return Err(Unsupported),
    };
    let entry: LockEntry = serde_json::from_value(raw.clone()).map_err(|_| Invalid)?;
    if !is_hex(&entry.computed_hash, 64) {
        return Err(Invalid);
    }
    let skill_path = entry
        .skill_path
        .as_deref()
        .map(locked_skill_directory)
        .transpose()?;
    let location = if source_type == "node_modules" {
        let hints: NodeModuleHints = serde_json::from_value(raw.clone()).map_err(|_| Invalid)?;
        RestoreLocation::NodeModules(NodeModuleSkill {
            package: npm_package_path(&entry.source)?,
            declared_by: hints
                .via
                .as_deref()
                .filter(|via| *via != ".")
                .map(npm_package_path)
                .transpose()?,
            version: hints.version,
            skill_path: skill_path.ok_or(Invalid)?,
        })
    } else {
        RestoreLocation::Git(git_lock_source(&entry, source_type, name, skill_path)?)
    };
    Ok(RestoreSource {
        location,
        hash: entry.computed_hash,
    })
}

pub(super) fn locked_skill_directory(value: &str) -> Result<PathBuf, ProjectLockedSkillStatus> {
    let mut parts = value.split('/');
    if parts.next_back() != Some("SKILL.md") {
        return Err(ProjectLockedSkillStatus::Invalid);
    }
    let mut directory = PathBuf::new();
    for part in parts {
        if !valid_component(part) || part.eq_ignore_ascii_case(".git") {
            return Err(ProjectLockedSkillStatus::Invalid);
        }
        directory.push(part);
    }
    Ok(directory)
}

fn npm_package_path(value: &str) -> Result<PathBuf, ProjectLockedSkillStatus> {
    let valid_name = |part: &str| {
        valid_skill_name(part)
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    };
    match value.split_once('/') {
        Some((scope, name))
            if scope.strip_prefix('@').is_some_and(valid_name) && valid_name(name) =>
        {
            Ok(PathBuf::from(scope).join(name))
        }
        None if valid_name(value) => Ok(PathBuf::from(value)),
        _ => Err(ProjectLockedSkillStatus::Invalid),
    }
}

fn github_repository(source: &str) -> Result<(String, PathBuf), ProjectLockedSkillStatus> {
    use ProjectLockedSkillStatus::Invalid;

    let source = source.strip_prefix("https://github.com/").unwrap_or(source);
    let mut parts = source.split('/');
    let owner = parts.next().ok_or(Invalid)?;
    let repository = parts.next().ok_or(Invalid)?.trim_end_matches(".git");
    if !valid_component(owner)
        || !valid_component(repository)
        || ![owner, repository].into_iter().all(|part| {
            part.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
    {
        return Err(Invalid);
    }
    let mut subpath = PathBuf::new();
    for part in parts {
        if !valid_component(part) || part.eq_ignore_ascii_case(".git") {
            return Err(Invalid);
        }
        subpath.push(part);
    }
    Ok((
        format!("https://github.com/{owner}/{repository}.git"),
        subpath,
    ))
}

fn recorded_git_repository(source: &str) -> Result<String, ProjectLockedSkillStatus> {
    use ProjectLockedSkillStatus::{Invalid, Unsupported};

    if !source.starts_with("https://") {
        return Err(Unsupported);
    }
    if source.chars().any(|ch| ch.is_control() || ch == '\\') {
        return Err(Invalid);
    }
    let url = reqwest::Url::parse(source).map_err(|_| Invalid)?;
    if url.host_str().is_none()
        || url.path() == "/"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Invalid);
    }
    Ok(url.into())
}

fn git_lock_source(
    entry: &LockEntry,
    source_type: &str,
    name: &str,
    skill_path: Option<PathBuf>,
) -> Result<GitSkill, ProjectLockedSkillStatus> {
    use ProjectLockedSkillStatus::Invalid;

    let (url, subpath) = if source_type == "github" {
        github_repository(&entry.source)?
    } else {
        (
            recorded_git_repository(entry.source_url.as_deref().unwrap_or(&entry.source))?,
            PathBuf::new(),
        )
    };
    if skill_path
        .as_ref()
        .is_some_and(|path| !path.starts_with(&subpath))
    {
        return Err(Invalid);
    }
    let reference = if let Some(commit) = &entry.commit {
        if !is_hex(commit, 40) {
            return Err(Invalid);
        }
        commit.clone()
    } else if let Some(reference) = &entry.reference {
        if !reference
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_./-".contains(&byte))
            || reference.starts_with('-')
            || reference
                .split('/')
                .any(|part| part.is_empty() || part.starts_with('.'))
            || reference.contains("..")
        {
            return Err(Invalid);
        }
        reference.clone()
    } else {
        "HEAD".to_string()
    };
    let skill_name = entry.skill_name.as_deref().unwrap_or(name);
    if !valid_skill_name(skill_name) {
        return Err(Invalid);
    }
    Ok(GitSkill {
        repository: RepositoryRevision { url, reference },
        commit: entry.commit.clone(),
        subpath,
        skill_path,
        skill_name: skill_name.to_string(),
    })
}

pub(super) fn restore(
    root: &Path,
    revision: &str,
    skill_names: Vec<String>,
) -> Result<Vec<ProjectSkillRestoreResult>, String> {
    restore_with(root, revision, skill_names, |repository| {
        installer::checkout_skill_lock_ref(&repository.url, &repository.reference)
    })
}

pub(super) fn verify_installed(
    root: &Path,
    revision: &str,
) -> Result<BTreeMap<String, ProjectLockedSkillStatus>, String> {
    use ProjectLockedSkillStatus::{Modified, Unverified, Verified};

    let lock = lock_at_revision(root, revision)?;
    let destination = checked_destination(root, false)?;
    let duplicates = duplicate_names(lock.skills.keys());
    let mut results = BTreeMap::new();
    for (name, entry) in lock.skills {
        if !valid_skill_name(&name) || duplicates.contains(&name.to_lowercase()) {
            continue;
        }
        let skill = destination.join(&name);
        let file_type = match fs::symlink_metadata(&skill) {
            Ok(metadata) => Some(metadata.file_type()),
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(_) => None,
        };
        let expected = entry
            .get("computedHash")
            .and_then(|hash| hash.as_str())
            .filter(|hash| lock.version == 2 && is_hex(hash, 64));
        let status = expected
            .and_then(|expected| {
                let file_type = file_type?;
                let actual = if file_type.is_dir() {
                    hash_installed_skill(&destination, &skill).ok()?
                } else if file_type.is_symlink() {
                    let source = restore_source(lock.version, &name, &entry).ok()?;
                    hash_linked_npm_skill(root, &skill, &source.location)?
                } else {
                    return None;
                };
                Some(if actual.eq_ignore_ascii_case(expected) {
                    Verified
                } else {
                    Modified
                })
            })
            .unwrap_or(Unverified);
        results.insert(name, status);
    }
    lock_at_revision(root, revision)?;
    Ok(results)
}

fn hash_installed_skill(destination: &Path, skill: &Path) -> Result<String, String> {
    let skill = skill.canonicalize().map_err(|error| error.to_string())?;
    if skill.parent() != Some(destination) {
        return Err("The installed path is not a Skill directory".to_string());
    }
    fingerprint_canonical_skill_directory(&skill, true)
}

fn hash_linked_npm_skill(root: &Path, skill: &Path, location: &RestoreLocation) -> Option<String> {
    let RestoreLocation::NodeModules(package) = location else {
        return None;
    };
    let target = skill.canonicalize().ok()?;
    if !target.starts_with(root) || target != select_node_module_skill(root, package).ok()? {
        return None;
    }
    let actual = fingerprint_canonical_skill_directory(&target, true).ok()?;
    (skill.canonicalize().ok()? == target).then_some(actual)
}

pub(super) fn fingerprint_canonical_skill_directory(
    skill: &Path,
    skip_package_metadata: bool,
) -> Result<String, String> {
    if !skill
        .join("SKILL.md")
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.is_file())
    {
        return Err("The installed path is not a Skill directory".to_string());
    }
    let files = portable_skill_files(skill, skip_package_metadata)?;
    let mut hash = Sha256::new();
    for (path, file) in files {
        add_file_to_hash(&mut hash, &path, &file)?;
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn restore_with(
    root: &Path,
    revision: &str,
    skill_names: Vec<String>,
    mut fetch: impl FnMut(&RepositoryRevision) -> Result<(PathBuf, TempInstallDir), String>,
) -> Result<Vec<ProjectSkillRestoreResult>, String> {
    let lock = lock_at_revision(root, revision)?;
    let duplicates = duplicate_names(lock.skills.keys());
    let mut checkouts = HashMap::new();
    let mut results = Vec::new();
    for name in skill_names.into_iter().collect::<BTreeSet<_>>() {
        let result = (|| -> Result<(), String> {
            lock_at_revision(root, revision)?;
            if !valid_skill_name(&name) || duplicates.contains(&name.to_lowercase()) {
                return Err(
                    "The recorded Skill name cannot be used as a project folder".to_string()
                );
            }
            let raw = lock
                .skills
                .get(&name)
                .ok_or("The Skill is missing from the lock file")?;
            let source =
                restore_source(lock.version, &name, raw).map_err(|status| match status {
                    ProjectLockedSkillStatus::Invalid => "The locked Skill source is invalid",
                    _ => "This locked Skill has no supported, verifiable source",
                })?;
            let destination = checked_destination(root, false)?.join(&name);
            match fs::symlink_metadata(&destination) {
                Ok(_) => return Err("The destination already exists".to_string()),
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
            let skill = match &source.location {
                RestoreLocation::Git(git) => {
                    let checkout = checkouts
                        .entry(git.repository.clone())
                        .or_insert_with(|| fetch(&git.repository));
                    let (checkout, _temporary) = checkout.as_ref().map_err(Clone::clone)?;
                    if let Some(commit) = &git.commit {
                        if !installer::read_checkout_commit(checkout)
                            .is_some_and(|actual| actual.eq_ignore_ascii_case(commit))
                        {
                            return Err(
                                "The checkout does not contain the locked commit".to_string()
                            );
                        }
                    }
                    select_git_skill(checkout, git)?
                }
                RestoreLocation::NodeModules(package) => select_node_module_skill(root, package)?,
            };
            install_verified_skill(root, revision, &name, &skill, &source)
        })();
        results.push(ProjectSkillRestoreResult {
            name,
            error: result.err(),
        });
    }
    Ok(results)
}

fn lock_at_revision(root: &Path, revision: &str) -> Result<LockFile, String> {
    let (lock, actual) = read_lock(root)?.ok_or("The project lock file is missing")?;
    if revision != actual {
        return Err("The project lock file changed. Rescan the project and try again.".to_string());
    }
    Ok(lock)
}

fn checked_destination(root: &Path, create: bool) -> Result<PathBuf, String> {
    if root.canonicalize().map_err(|error| error.to_string())? != root {
        return Err("The project path changed. Re-add the project before restoring.".to_string());
    }
    let mut path = root.to_path_buf();
    for segment in [".agents", "skills"] {
        path.push(segment);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if !create {
                    continue;
                }
                match fs::create_dir(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
            _ => {
                return Err(format!(
                    "{} must be a directory inside the project",
                    path.display()
                ))
            }
        }
        match path.canonicalize() {
            Ok(resolved) if resolved == path && resolved.is_dir() => {}
            _ => {
                return Err(format!(
                    "{} must be a directory inside the project",
                    path.display()
                ))
            }
        }
    }
    Ok(path)
}

fn select_git_skill(checkout: &Path, source: &GitSkill) -> Result<PathBuf, String> {
    let root = checkout.canonicalize().map_err(|error| error.to_string())?;
    let skill = if let Some(path) = &source.skill_path {
        root.join(path)
    } else {
        installer::locate_skillssh_skill_dir(&root.join(&source.subpath), &source.skill_name)?
    };
    checked_source_skill(&root, &skill)
}

fn select_node_module_skill(root: &Path, source: &NodeModuleSkill) -> Result<PathBuf, String> {
    let declaring = source
        .declared_by
        .as_deref()
        .and_then(|package| find_node_module(root, root, package));
    let origin = declaring.as_deref().unwrap_or(root);
    let package = find_node_module(root, origin, &source.package).ok_or_else(|| {
        format!(
            "Could not find {}. Install the project's npm dependencies and retry.",
            source.package.display()
        )
    })?;
    if let Some(version) = &source.version {
        let manifest = fs::read(package.join("package.json")).map_err(|error| error.to_string())?;
        let manifest: serde_json::Value =
            serde_json::from_slice(&manifest).map_err(|error| error.to_string())?;
        if manifest.get("version").and_then(serde_json::Value::as_str) != Some(version) {
            return Err(format!(
                "The installed npm package {} does not match the locked version {version}.",
                source.package.display()
            ));
        }
    }
    checked_source_skill(&package, &package.join(&source.skill_path))
}

fn find_node_module(root: &Path, from: &Path, package: &Path) -> Option<PathBuf> {
    from.ancestors()
        .take_while(|directory| directory.starts_with(root))
        .find_map(|directory| {
            let candidate = directory
                .join("node_modules")
                .join(package)
                .canonicalize()
                .ok()?;
            (candidate.starts_with(root)
                && candidate
                    .join("package.json")
                    .symlink_metadata()
                    .is_ok_and(|metadata| metadata.is_file()))
            .then_some(candidate)
        })
}

pub(super) fn checked_source_skill(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let skill = path.canonicalize().map_err(|error| error.to_string())?;
    if !skill.starts_with(root)
        || !skill
            .join("SKILL.md")
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.is_file())
    {
        return Err("The source has no matching Skill directory".to_string());
    }
    Ok(skill)
}

fn install_verified_skill(
    root: &Path,
    revision: &str,
    name: &str,
    skill: &Path,
    source: &RestoreSource,
) -> Result<(), String> {
    let destination = checked_destination(root, false)?;
    let staging_parent = destination
        .ancestors()
        .find(|ancestor| ancestor.is_dir())
        .ok_or("The project Skill directory is unavailable")?;
    let temporary = TempInstallDir::create_in(staging_parent)?;
    let staged = temporary.path().join("skill");
    if !stage_skill_copy(skill, &staged, true)?.eq_ignore_ascii_case(&source.hash) {
        // Older entries can match the source bytes even with a different destination name.
        let renamed = matches!(
            &source.location,
            RestoreLocation::Git(git) if git.skill_name != name
        );
        if !renamed || !verify_staged_alias(&staged, name, &source.hash)? {
            return Err("Skill content does not match the lock file".to_string());
        }
    }
    lock_at_revision(root, revision)?;
    let destination = checked_destination(root, true)?;
    rename_path_if_absent(&staged, &destination.join(name))
        .map_err(|error| format!("Could not install the missing Skill: {error}"))
}

fn verify_staged_alias(staged: &Path, name: &str, expected: &str) -> Result<bool, String> {
    let path = staged.join("SKILL.md");
    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let Some(renamed) = skills_sh_rename(&content, name) else {
        return Ok(false);
    };
    fs::write(path, renamed).map_err(|error| error.to_string())?;
    Ok(fingerprint_canonical_skill_directory(staged, true)?.eq_ignore_ascii_case(expected))
}

fn skills_sh_rename(content: &str, name: &str) -> Option<String> {
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte))
    {
        return None;
    }
    static FRONTMATTER: OnceLock<Regex> = OnceLock::new();
    static YAML_NON_STRING: OnceLock<Regex> = OnceLock::new();
    let frontmatter = FRONTMATTER.get_or_init(|| {
        Regex::new(r"\A(\u{feff}?---(\r?\n))([\s\S]*?)(\r?\n---(?:\r?\n|\z))")
            .expect("static Skill frontmatter regex")
    });
    let non_string = YAML_NON_STRING.get_or_init(|| {
        // skills.sh emits bare strings unless the YAML 1.2 core schema requires quotes.
        Regex::new(
            r"(?x)\A(?:
                null | true | false | - | -?\.inf | \.nan | 0o[0-7]+ | 0x[0-9a-f]+
                | -?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:e-?[0-9]+)?
            )\z",
        )
        .expect("static YAML scalar regex")
    });
    let captures = frontmatter.captures(content)?;
    let name_line = if non_string.is_match(name) {
        format!("name: \"{name}\"")
    } else {
        format!("name: {name}")
    };
    let fields = captures[3].replace("\r\n", "\n");
    let mut lines: Vec<_> = fields.split('\n').collect();
    if let Some(index) = lines.iter().position(|line| {
        line.strip_prefix("name")
            .is_some_and(|rest| rest.trim_start().starts_with(':'))
    }) {
        let continuation = lines[index + 1..]
            .iter()
            .take_while(|line| line.starts_with([' ', '\t']))
            .count();
        lines.splice(index..=index + continuation, [name_line.as_str()]);
    } else {
        lines.insert(0, name_line.as_str());
    }
    let end = captures.get(0)?.end();
    Some(format!(
        "{}{}{}{}",
        &captures[1],
        lines.join(&captures[2]),
        &captures[4],
        &content[end..]
    ))
}

pub(super) fn stage_skill_copy(
    skill: &Path,
    staged: &Path,
    skip_package_metadata: bool,
) -> Result<String, String> {
    let files = portable_skill_files(skill, skip_package_metadata)?;
    fs::create_dir(staged).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    for (path, original) in files {
        let target = staged.join(&path);
        let parent = target.parent().ok_or("Invalid Skill file destination")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        fs::copy(original, &target).map_err(|error| error.to_string())?;
        add_file_to_hash(&mut hash, &path, &target)?;
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(super) fn is_skill_cache_dir(name: &str) -> bool {
    matches!(name, ".git" | "__pycache__" | "__pypackages__")
}

pub(super) fn portable_skill_files(
    skill: &Path,
    skip_package_metadata: bool,
) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut files = BTreeMap::new();
    collect_skill_files(
        skill,
        skill,
        "",
        &mut Vec::new(),
        &mut files,
        skip_package_metadata,
    )?;
    Ok(files)
}

fn collect_skill_files(
    root: &Path,
    directory: &Path,
    prefix: &str,
    ancestors: &mut Vec<PathBuf>,
    files: &mut BTreeMap<String, PathBuf>,
    skip_package_metadata: bool,
) -> Result<(), String> {
    let absolute = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !absolute.starts_with(root) || ancestors.contains(&absolute) {
        return Ok(());
    }
    ancestors.push(absolute.clone());
    for item in fs::read_dir(&absolute).map_err(|error| error.to_string())? {
        let item = item.map_err(|error| error.to_string())?;
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| "Skill file names must be UTF-8")?;
        if skip_package_metadata && name == "metadata.json" {
            continue;
        }
        let file_type = item.file_type().map_err(|error| error.to_string())?;
        let source = if file_type.is_symlink() {
            let Ok(target) = item.path().canonicalize() else {
                continue;
            };
            if !target.starts_with(root) {
                continue;
            }
            target
        } else {
            item.path()
        };
        let metadata = source.metadata().map_err(|error| error.to_string())?;
        let relative = format!("{prefix}{name}");
        if metadata.is_dir() && is_skill_cache_dir(&name) {
            continue;
        }
        if !valid_component(&name) || name.eq_ignore_ascii_case(".git") {
            return Err(format!("Unsupported Skill file name: {name}"));
        }
        if metadata.is_dir() {
            collect_skill_files(
                root,
                &source,
                &format!("{relative}/"),
                ancestors,
                files,
                skip_package_metadata,
            )?;
        } else if metadata.is_file() {
            files.insert(relative, source);
        }
    }
    ancestors.pop();
    Ok(())
}

fn add_file_to_hash(hash: &mut Sha256, path: &str, source: &Path) -> Result<(), String> {
    let mut file = fs::File::open(source).map_err(|error| error.to_string())?;
    let mut buffer = [0; 16 * 1024];
    let mut size = 0;
    let mut crlf = 0;
    let mut binary = false;
    let mut trailing_cr = false;
    // The portable format prefixes the normalized length and leaves any file containing NUL untouched.
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        let bytes = &buffer[..count];
        size += count as u64;
        binary |= bytes.contains(&0);
        if !binary {
            crlf += bytes.windows(2).filter(|window| *window == b"\r\n").count() as u64;
            crlf += u64::from(trailing_cr && bytes[0] == b'\n');
            trailing_cr = bytes[count - 1] == b'\r';
        }
    }
    let normalize = !binary && crlf != 0;
    let normalized_size = if normalize { size - crlf } else { size };
    hash.update(path.as_bytes());
    hash.update([0]);
    hash.update(normalized_size.to_string().as_bytes());
    hash.update([0]);

    file.seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut hashed = 0;
    trailing_cr = false;
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        if !normalize {
            hash.update(&buffer[..count]);
            hashed += count as u64;
            continue;
        }
        if trailing_cr && buffer[0] != b'\n' {
            hash.update(b"\r");
            hashed += 1;
        }
        trailing_cr = buffer[count - 1] == b'\r';
        let mut length = 0;
        for index in 0..count - usize::from(trailing_cr) {
            if buffer[index] != b'\r' || buffer[index + 1] != b'\n' {
                buffer[length] = buffer[index];
                length += 1;
            }
        }
        hash.update(&buffer[..length]);
        hashed += length as u64;
    }
    if trailing_cr {
        hash.update(b"\r");
        hashed += 1;
    }
    if hashed != normalized_size {
        return Err("A Skill file changed while it was being verified".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::v2::models::ProjectLockedSkillStatus::*;
    use serde_json::json;

    const PORTABLE_HASH: &str = "fd3e459f44c5edbece0fd70f8abd169df0a4d64fa9c28dd89eed88a6ca0a14a7";
    const ROOT_PACKAGE_HASH: &str =
        "ab488bfd61ddf0b54f931a50046bb662437a90608b401c7601569d2a480664c1";
    const MANIFEST: &str =
        "---\r\nname: original\r\ndescription: Local skill\r\n---\r\nOriginal content.\r\n";

    fn write_lock(root: &Path, content: serde_json::Value) {
        fs::write(root.join(LOCK_FILE), serde_json::to_vec(&content).unwrap()).unwrap();
    }

    fn run_git(root: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .current_dir(root)
            .args([
                "-c",
                "user.name=AgentBro",
                "-c",
                "user.email=fixture@example.test",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn previews_legacy_unknown_and_occupied_skills_without_fetching() {
        let temporary = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let existing = root.join(".agents/skills/present");
        fs::create_dir_all(&existing).unwrap();
        fs::write(existing.join("SKILL.md"), "existing").unwrap();
        write_lock(
            &root,
            json!({
                "version": 1,
                "skills": {
                    "../outside": { "source": "example/skills", "sourceType": "github" },
                    "missing": { "source": "example/skills", "sourceType": "github" },
                    "present": { "source": "example/skills", "sourceType": "github" }
                }
            }),
        );
        let lock = preview(&root).unwrap();
        assert_eq!(lock.version, Some(1));
        assert_eq!(
            lock.skills
                .iter()
                .map(|skill| skill.status)
                .collect::<Vec<_>>(),
            [Invalid, Legacy, Existing]
        );

        write_lock(
            &root,
            json!({
                "version": 2,
                "skills": {
                    "missing": { "source": "https://example.test/skills.zip", "sourceType": "download" },
                    "bad": { "source": "example/skills", "sourceType": "github", "computedHash": PORTABLE_HASH, "skillPath": "../SKILL.md" },
                }
            }),
        );
        let current = preview(&root).unwrap();
        assert_eq!(current.skills[0].status, Invalid);
        assert_eq!(current.skills[1].status, Unsupported);
        let result = restore_with(&root, &lock.revision, vec!["missing".into()], |_| {
            panic!("A changed preview must not fetch anything")
        });
        assert!(result.unwrap_err().contains("lock file changed"));
    }

    #[test]
    fn verifies_installed_content_without_changing_existing_paths() {
        let temporary = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let target = root.join(".agents/skills");
        let matching = target.join("match");
        let modified = target.join("modified");
        let incomplete = target.join("incomplete");
        fs::create_dir_all(matching.join("scripts")).unwrap();
        fs::write(matching.join("SKILL.md"), MANIFEST).unwrap();
        fs::write(matching.join("scripts/data.bin"), b"\0\r\n").unwrap();
        fs::write(matching.join("metadata.json"), "{}").unwrap();
        fs::create_dir_all(&modified).unwrap();
        fs::write(modified.join("SKILL.md"), "local edit").unwrap();
        fs::create_dir_all(&incomplete).unwrap();
        let entry = json!({
            "source": "example/skills",
            "sourceType": "github",
            "computedHash": PORTABLE_HASH,
        });
        write_lock(
            &root,
            json!({
                "version": 2,
                "skills": {
                    "match": entry,
                    "modified": entry,
                    "incomplete": entry,
                    "missing": entry,
                }
            }),
        );
        let before = fs::read(root.join(LOCK_FILE)).unwrap();
        let lock = preview(&root).unwrap();
        let results = verify_installed(&root, &lock.revision).unwrap();
        assert_eq!(
            results,
            BTreeMap::from([
                ("match".to_string(), Verified),
                ("modified".to_string(), Modified),
                ("incomplete".to_string(), Unverified),
            ])
        );
        assert_eq!(fs::read(root.join(LOCK_FILE)).unwrap(), before);
        assert_eq!(
            fs::read_to_string(modified.join("SKILL.md")).unwrap(),
            "local edit"
        );
        assert!(!target.join("missing").exists());

        write_lock(&root, json!({ "version": 2, "skills": {} }));
        assert!(verify_installed(&root, &lock.revision)
            .unwrap_err()
            .contains("lock file changed"));
    }

    #[test]
    fn restores_selected_npm_skill_and_reports_missing_changed_or_unsafe_entries() {
        let temporary = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let package = root.join("node_modules/@example/toolkit");
        let skill = package.join("skills/original");
        fs::create_dir_all(skill.join("scripts")).unwrap();
        fs::write(package.join("package.json"), br#"{"version":"1.2.3"}"#).unwrap();
        fs::write(skill.join("SKILL.md"), MANIFEST).unwrap();
        fs::write(skill.join("scripts/data.bin"), b"\0\r\n").unwrap();

        let entry = json!({
            "source": "@example/toolkit",
            "sourceType": "node_modules",
            "version": "1.2.3",
            "skillPath": "skills/original/SKILL.md",
            "computedHash": PORTABLE_HASH,
        });
        let mut changed = entry.clone();
        changed["computedHash"] = "0".repeat(64).into();
        let mut missing = entry.clone();
        missing["source"] = "missing-package".into();
        let mut unsafe_path = entry.clone();
        unsafe_path["skillPath"] = "../SKILL.md".into();
        let mut wrong_version = entry.clone();
        wrong_version["version"] = "9.0.0".into();
        write_lock(
            &root,
            json!({
                "version": 2,
                "skills": {
                    "alias": entry,
                    "changed": changed,
                    "missing": missing,
                    "unsafe": unsafe_path,
                    "wrong-version": wrong_version,
                }
            }),
        );
        let before = fs::read(root.join(LOCK_FILE)).unwrap();
        let lock = preview(&root).unwrap();
        assert_eq!(lock.skills[0].status, Ready);
        assert_eq!(lock.skills[0].reference.as_deref(), Some("1.2.3"));
        assert_eq!(lock.skills[3].status, Invalid);

        let names = lock.skills.iter().map(|skill| skill.name.clone()).collect();
        let results = restore_with(&root, &lock.revision, names, |_| {
            panic!("An installed package does not require a Git checkout")
        })
        .unwrap();
        assert!(results[0].error.is_none(), "{:?}", results[0].error);
        for (result, message) in results[1..].iter().zip([
            "content does not match",
            "Install the project's npm dependencies",
            "invalid",
            "locked version 9.0.0",
        ]) {
            assert!(
                result
                    .error
                    .as_ref()
                    .is_some_and(|error| error.contains(message)),
                "{result:?}"
            );
        }
        let destination = root.join(".agents/skills");
        assert_eq!(
            fs::read_to_string(destination.join("alias/SKILL.md")).unwrap(),
            MANIFEST
        );
        assert_eq!(
            fs::read(destination.join("alias/scripts/data.bin")).unwrap(),
            b"\0\r\n"
        );
        assert_eq!(fs::read_dir(destination).unwrap().count(), 1);
        assert_eq!(fs::read(root.join(LOCK_FILE)).unwrap(), before);
        assert_eq!(
            verify_installed(&root, &lock.revision)
                .unwrap()
                .get("alias"),
            Some(&Verified)
        );
    }

    #[test]
    fn resolves_npm_skill_via_the_declaring_dependency() {
        let temporary = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let declaring = root.join("node_modules/declaring-package");
        let package = declaring.join("node_modules/skill-package");
        fs::create_dir_all(package.join("scripts")).unwrap();
        fs::write(declaring.join("package.json"), "{}").unwrap();
        fs::write(package.join("package.json"), br#"{"version":"2.0.0"}"#).unwrap();
        fs::write(package.join("SKILL.md"), MANIFEST).unwrap();
        fs::write(package.join("scripts/data.bin"), b"\0\r\n").unwrap();
        write_lock(
            &root,
            json!({
                "version": 2,
                "skills": {
                    "nested": {
                        "source": "skill-package",
                        "sourceType": "node_modules",
                        "via": "declaring-package",
                        "skillPath": "SKILL.md",
                        "computedHash": ROOT_PACKAGE_HASH
                    }
                }
            }),
        );
        let lock = preview(&root).unwrap();
        let result = restore(&root, &lock.revision, vec!["nested".into()]).unwrap();
        assert!(result[0].error.is_none(), "{:?}", result[0].error);
        assert!(root.join(".agents/skills/nested/SKILL.md").is_file());
    }

    #[test]
    fn supports_pnpm_package_links_inside_the_project_and_rejects_external_links() {
        let temp = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let root = temp.path().join("project");
        let store = root.join("node_modules/.pnpm/declaring-package@1.0.0/node_modules");
        let declaring = store.join("declaring-package");
        let package = store.join("skill-package");
        let outside = temp.path().join("outside-package");
        let unrelated = root.join("unrelated-skill");
        fs::create_dir_all(&declaring).unwrap();
        fs::write(declaring.join("package.json"), "{}").unwrap();
        for directory in [&package, &outside, &unrelated] {
            fs::create_dir_all(directory.join("scripts")).unwrap();
            fs::write(directory.join("package.json"), br#"{"version":"2.0.0"}"#).unwrap();
            fs::write(directory.join("SKILL.md"), MANIFEST).unwrap();
            fs::write(directory.join("scripts/data.bin"), b"\0\r\n").unwrap();
        }
        let destination = root.join(".agents/skills");
        fs::create_dir_all(&destination).unwrap();
        for (source, link) in [
            (&declaring, root.join("node_modules/declaring-package")),
            (&outside, root.join("node_modules/outside-package")),
            (&package, destination.join("linked")),
            (&package, destination.join("wrong-version")),
            (&outside, destination.join("escaped-link")),
            (&unrelated, destination.join("wrong-target")),
        ] {
            #[cfg(unix)]
            std::os::unix::fs::symlink(source, link).unwrap();
            #[cfg(windows)]
            {
                let result = std::process::Command::new("powershell.exe")
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        "New-Item -ItemType Junction -Path $env:AGENTBRO_TEST_JUNCTION -Target $env:AGENTBRO_TEST_JUNCTION_TARGET -ErrorAction Stop | Out-Null",
                    ])
                    .env("AGENTBRO_TEST_JUNCTION", &link)
                    .env("AGENTBRO_TEST_JUNCTION_TARGET", source)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
        }
        let entry = json!({
            "source": "skill-package",
            "sourceType": "node_modules",
            "via": "declaring-package",
            "skillPath": "SKILL.md",
            "version": "2.0.0",
            "computedHash": ROOT_PACKAGE_HASH,
        });
        let mut escaped = entry.clone();
        escaped["source"] = "outside-package".into();
        let mut wrong_version = entry.clone();
        wrong_version["version"] = "3.0.0".into();
        write_lock(
            &root,
            json!({
                "version": 2,
                "skills": {
                    "inside": entry,
                    "outside": escaped,
                    "linked": entry,
                    "escaped-link": entry,
                    "wrong-target": entry,
                    "wrong-version": wrong_version
                }
            }),
        );
        let root = root.canonicalize().unwrap();
        let lock = preview(&root).unwrap();
        let results = restore(
            &root,
            &lock.revision,
            vec!["inside".into(), "outside".into()],
        )
        .unwrap();
        assert!(results[0].error.is_none(), "{:?}", results[0].error);
        assert!(results[1].error.is_some());
        assert!(root.join(".agents/skills/inside/SKILL.md").is_file());
        assert!(!root.join(".agents/skills/outside").exists());

        let verified = verify_installed(&root, &lock.revision).unwrap();
        assert_eq!(verified.get("inside"), Some(&Verified));
        assert_eq!(verified.get("linked"), Some(&Verified));
        assert_eq!(verified.get("escaped-link"), Some(&Unverified));
        assert_eq!(verified.get("wrong-target"), Some(&Unverified));
        assert_eq!(verified.get("wrong-version"), Some(&Unverified));
        assert!(!verified.contains_key("outside"));

        fs::write(package.join("SKILL.md"), "locally edited").unwrap();
        let verified = verify_installed(&root, &lock.revision).unwrap();
        assert_eq!(verified.get("inside"), Some(&Verified));
        assert_eq!(verified.get("linked"), Some(&Modified));
        assert_eq!(preview(&root).unwrap().revision, lock.revision);
        assert_eq!(
            destination.join("linked").canonicalize().unwrap(),
            package.canonicalize().unwrap()
        );
    }

    #[test]
    fn portable_hash_normalizes_crlf_across_reads_and_keeps_a_trailing_cr() {
        let temporary = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let file = temporary.path().join("SKILL.md");
        let prefix = vec![b'x'; 16 * 1024 - 1];
        let raw = [prefix.as_slice(), b"\r\nnext\r\nlast\r"].concat();
        let normalized = [prefix.as_slice(), b"\nnext\nlast\r"].concat();
        fs::write(&file, raw).unwrap();
        let mut hash = Sha256::new();
        add_file_to_hash(&mut hash, "SKILL.md", &file).unwrap();
        let expected = Sha256::digest(
            [
                format!("SKILL.md\0{}\0", normalized.len()).as_bytes(),
                &normalized,
            ]
            .concat(),
        );
        assert_eq!(hash.finalize(), expected);
    }

    #[test]
    fn restores_the_pinned_skill_and_preserves_other_entries() {
        restore_pinned_fixture(
            "github",
            "example/skills",
            None,
            "https://github.com/example/skills.git",
        );
    }

    #[test]
    fn restores_pinned_gitlab_and_generic_https_skills() {
        restore_pinned_fixture(
            "gitlab",
            "engineering/team/skills",
            Some("https://gitlab.com/engineering/team/skills.git"),
            "https://gitlab.com/engineering/team/skills.git",
        );
        restore_pinned_fixture(
            "git",
            "https://git.example.test:8443/team/_git/skills",
            None,
            "https://git.example.test:8443/team/_git/skills",
        );
    }

    #[test]
    fn refuses_ambiguous_sources_and_sensitive_https_urls_before_fetch() {
        let temporary = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let cases = [
            ("ambiguous", None, Unsupported, "team/skills"),
            (
                "credentials",
                Some("https://user:example@git.example.test/team/skills.git"),
                Invalid,
                "git.example.test",
            ),
            (
                "query",
                Some("https://git.example.test/team/skills.git?token=example"),
                Invalid,
                "git.example.test",
            ),
            (
                "fragment",
                Some("https://git.example.test/team/skills.git#example"),
                Invalid,
                "git.example.test",
            ),
            (
                "ssh",
                Some("ssh://git@git.example.test/team/skills.git"),
                Unsupported,
                "git.example.test",
            ),
            (
                "local",
                Some("file:///local/team/skills.git"),
                Unsupported,
                "file:///local/team/skills.git",
            ),
        ];
        let entries: BTreeMap<_, _> = cases
            .iter()
            .map(|(name, source_url, _, _)| {
                (
                    *name,
                    json!({
                        "source": "team/skills",
                        "sourceType": "git",
                        "sourceUrl": source_url,
                        "computedHash": PORTABLE_HASH,
                    }),
                )
            })
            .collect();
        write_lock(&root, json!({ "version": 2, "skills": entries }));
        let lock = preview(&root).unwrap();
        for (name, _, status, source) in cases {
            let skill = lock.skills.iter().find(|skill| skill.name == name).unwrap();
            assert_eq!(skill.source, source);
            assert_eq!(skill.status, status);
        }
        let names = lock.skills.iter().map(|skill| skill.name.clone()).collect();
        let results = restore_with(&root, &lock.revision, names, |_| {
            panic!("a rejected HTTPS Git source must not fetch")
        })
        .unwrap();
        assert!(results.iter().all(|result| result.error.is_some()));
        assert!(!root.join(".agents").exists());
    }

    fn restore_pinned_fixture(
        source_type: &str,
        source: &str,
        source_url: Option<&str>,
        checkout_url: &str,
    ) {
        let temporary = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let repo = temporary.path().join("source repository");
        let project = temporary.path().join("target project");
        let skill = repo.join("skills/original");
        fs::create_dir_all(skill.join("scripts")).unwrap();
        fs::create_dir(&project).unwrap();
        let project = project.canonicalize().unwrap();
        let repo = repo.canonicalize().unwrap();
        run_git(&repo, &["init", "--template="]);
        fs::write(skill.join("SKILL.md"), MANIFEST).unwrap();
        fs::write(skill.join("scripts/data.bin"), b"\0\r\n").unwrap();
        fs::write(skill.join("metadata.json"), "{}").unwrap();
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-m", "lock fixture"]);
        let commit = run_git(&repo, &["rev-parse", "HEAD"]);
        fs::write(skill.join("SKILL.md"), "newer content").unwrap();
        run_git(&repo, &["commit", "-am", "newer release"]);

        let entry = json!({
            "source": source,
            "sourceType": source_type,
            "sourceUrl": source_url,
            "commit": commit,
            "ref": "main",
            "skillName": "original",
            "skillPath": "skills/original/SKILL.md",
            "computedHash": PORTABLE_HASH,
        });
        let mut mismatch = entry.clone();
        mismatch["computedHash"] = "0".repeat(64).into();
        // skills.sh hashes the installed SKILL.md with the alias already written.
        let mut renamed = entry.clone();
        renamed["computedHash"] =
            "ddb178167edf5f0b7122a27abe4a16319ba1ca12e77f27dcf469641e9830370c".into();
        let mut quoted = entry.clone();
        quoted["computedHash"] =
            "5cd9396c9b857a67d507e4d7873284156def649ff9fc0ba82958661c498b948a".into();
        quoted.as_object_mut().unwrap().remove("skillPath");
        write_lock(
            &project,
            json!({
                "version": 2,
                "skills": {
                    "alias": entry,
                    "existing": entry,
                    "local-review": renamed,
                    "mismatch": mismatch,
                    "null": quoted
                }
            }),
        );
        let target = project.join(".agents/skills");
        fs::create_dir_all(target.join("existing")).unwrap();
        fs::write(target.join("existing/SKILL.md"), "local edit").unwrap();
        let lock = preview(&project).unwrap();
        assert_eq!(lock.skills[0].status, Ready);
        assert_eq!(lock.skills[1].status, Existing);
        assert_eq!(lock.skills[0].source, source_url.unwrap_or(source));
        let names = lock.skills.iter().map(|skill| skill.name.clone()).collect();
        let fetch_path = repo.to_str().unwrap();
        #[cfg(windows)]
        let fetch_path = fetch_path.strip_prefix(r"\\?\").unwrap_or(fetch_path);
        let mut fetches = 0;
        let results = restore_with(&project, &lock.revision, names, |remote| {
            assert_eq!(remote.url, checkout_url);
            assert_eq!(remote.reference, commit);
            fetches += 1;
            installer::checkout_skill_lock_ref(fetch_path, &remote.reference)
        })
        .unwrap();
        assert_eq!(fetches, 1);
        assert!(results[0].error.is_none(), "{:?}", results[0].error);
        assert_eq!(
            results[1].error.as_deref(),
            Some("The destination already exists")
        );
        assert!(results[2].error.is_none(), "{:?}", results[2].error);
        assert!(results[3]
            .error
            .as_ref()
            .unwrap()
            .contains("content does not match"));
        assert!(results[4].error.is_none(), "{:?}", results[4].error);
        assert!(!target.join("mismatch").exists());
        assert!(target.join("alias").is_dir());
        assert_eq!(
            fs::read_to_string(target.join("alias/SKILL.md"))
                .unwrap()
                .replace("\r\n", "\n"),
            MANIFEST.replace("\r\n", "\n")
        );
        assert_eq!(
            fs::read(target.join("alias/scripts/data.bin")).unwrap(),
            b"\0\r\n"
        );
        assert!(!target.join("alias/metadata.json").exists());
        assert_eq!(
            fs::read_to_string(target.join("existing/SKILL.md")).unwrap(),
            "local edit"
        );
        let verified = verify_installed(&project, &lock.revision).unwrap();
        for (name, scalar) in [("local-review", "local-review"), ("null", "\"null\"")] {
            assert_eq!(
                fs::read_to_string(target.join(name).join("SKILL.md"))
                    .unwrap()
                    .replace("\r\n", "\n"),
                MANIFEST
                    .replace("name: original", &format!("name: {scalar}"))
                    .replace("\r\n", "\n")
            );
            assert_eq!(verified.get(name), Some(&Verified));
        }
        assert_eq!(preview(&project).unwrap().revision, lock.revision);
    }

    #[test]
    fn project_alias_preserves_bom_line_endings_and_other_fields() {
        let source = "\u{feff}---\r\nmetadata:\r\n  name: keep\r\nname : >-\r\n  old name\r\n---\r\nBody\r\n---\r\n";
        let expected =
            "\u{feff}---\r\nmetadata:\r\n  name: keep\r\nname: \"1e-8\"\r\n---\r\nBody\r\n---\r\n";
        assert_eq!(skills_sh_rename(source, "1e-8").as_deref(), Some(expected));
        assert_eq!(skills_sh_rename("name: outside frontmatter", "alias"), None);
    }

    #[test]
    fn never_replaces_a_destination_created_during_the_restore() {
        let temporary = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let source = temporary.path().join("staged");
        let occupied = temporary.path().join("occupied");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&occupied).unwrap();
        fs::write(source.join("SKILL.md"), "from archive").unwrap();
        assert!(rename_path_if_absent(&source, &occupied).is_err());
        assert!(source.join("SKILL.md").is_file());
        assert!(!occupied.join("SKILL.md").exists());
    }
}
