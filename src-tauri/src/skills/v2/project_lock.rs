use super::models::{
    ProjectLockedSkill, ProjectLockedSkillStatus, ProjectSkillLock, ProjectSkillRestoreResult,
};
use crate::skills::installer::{self, TempInstallDir};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

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
    GitHub(GitHubSkill),
    NodeModules(NodeModuleSkill),
}

struct GitHubSkill {
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
                let source = entry
                    .get("source")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let reference = if entry.get("sourceType").and_then(|value| value.as_str())
                    == Some("node_modules")
                {
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
        if !url.username().is_empty() || url.password().is_some() || url.query().is_some() {
            return url.host_str().unwrap_or_default().to_string();
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

fn valid_component(name: &str) -> bool {
    !name.is_empty()
        && !name.ends_with(['.', ' '])
        && !name
            .chars()
            .any(|ch| ch.is_control() || "<>:\"/\\|?*".contains(ch))
        && !matches!(
            name.split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase()
                .as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
}

fn valid_skill_name(name: &str) -> bool {
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
    let source_type = raw.get("sourceType").and_then(|value| value.as_str());
    if version != 2 || !matches!(source_type, Some("github" | "node_modules")) {
        return Err(Unsupported);
    }
    let entry: LockEntry = serde_json::from_value(raw.clone()).map_err(|_| Invalid)?;
    if !is_hex(&entry.computed_hash, 64) {
        return Err(Invalid);
    }
    let skill_path = entry
        .skill_path
        .as_deref()
        .map(locked_skill_directory)
        .transpose()?;
    let location = if source_type == Some("node_modules") {
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
        RestoreLocation::GitHub(github_lock_source(&entry, name, skill_path)?)
    };
    Ok(RestoreSource {
        location,
        hash: entry.computed_hash,
    })
}

fn locked_skill_directory(value: &str) -> Result<PathBuf, ProjectLockedSkillStatus> {
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

fn github_lock_source(
    entry: &LockEntry,
    name: &str,
    skill_path: Option<PathBuf>,
) -> Result<GitHubSkill, ProjectLockedSkillStatus> {
    use ProjectLockedSkillStatus::Invalid;

    let source = entry
        .source
        .strip_prefix("https://github.com/")
        .unwrap_or(&entry.source);
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
    Ok(GitHubSkill {
        repository: RepositoryRevision {
            url: format!("https://github.com/{owner}/{repository}.git"),
            reference,
        },
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
        let is_directory = match fs::symlink_metadata(&skill) {
            Ok(metadata) => metadata.is_dir(),
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(_) => false,
        };
        let expected = entry
            .get("computedHash")
            .and_then(|hash| hash.as_str())
            .filter(|hash| lock.version == 2 && is_hex(hash, 64));
        let status = match expected {
            Some(hash) if is_directory => match hash_installed_skill(&destination, &skill) {
                Ok(actual) if actual.eq_ignore_ascii_case(hash) => Verified,
                Ok(_) => Modified,
                Err(_) => Unverified,
            },
            _ => Unverified,
        };
        results.insert(name, status);
    }
    lock_at_revision(root, revision)?;
    Ok(results)
}

fn hash_installed_skill(destination: &Path, skill: &Path) -> Result<String, String> {
    let skill = skill.canonicalize().map_err(|error| error.to_string())?;
    if skill.parent() != Some(destination)
        || !skill
            .join("SKILL.md")
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.is_file())
    {
        return Err("The installed path is not a Skill directory".to_string());
    }
    let mut files = BTreeMap::new();
    collect_skill_files(&skill, &skill, "", &mut Vec::new(), &mut files)?;
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
                RestoreLocation::GitHub(github) => {
                    let checkout = checkouts
                        .entry(github.repository.clone())
                        .or_insert_with(|| fetch(&github.repository));
                    let (checkout, _temporary) = checkout.as_ref().map_err(Clone::clone)?;
                    if let Some(commit) = &github.commit {
                        if !installer::read_checkout_commit(checkout)
                            .is_some_and(|actual| actual.eq_ignore_ascii_case(commit))
                        {
                            return Err(
                                "The checkout does not contain the locked commit".to_string()
                            );
                        }
                    }
                    select_github_skill(checkout, github)?
                }
                RestoreLocation::NodeModules(package) => select_node_module_skill(root, package)?,
            };
            install_verified_skill(root, revision, &name, &skill, &source.hash)
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

fn select_github_skill(checkout: &Path, source: &GitHubSkill) -> Result<PathBuf, String> {
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

fn checked_source_skill(root: &Path, path: &Path) -> Result<PathBuf, String> {
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
    expected: &str,
) -> Result<(), String> {
    let mut files = BTreeMap::new();
    collect_skill_files(skill, skill, "", &mut Vec::new(), &mut files)?;
    let destination = checked_destination(root, false)?;
    let staging_parent = destination
        .ancestors()
        .find(|ancestor| ancestor.is_dir())
        .ok_or("The project Skill directory is unavailable")?;
    let temporary = TempInstallDir::create_in(staging_parent)?;
    let staged = temporary.path().join("skill");
    fs::create_dir(&staged).map_err(|error| error.to_string())?;
    let mut staged_hash = Sha256::new();
    for (path, original) in files {
        let target = staged.join(&path);
        let parent = target.parent().ok_or("Invalid Skill file destination")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        fs::copy(original, &target).map_err(|error| error.to_string())?;
        add_file_to_hash(&mut staged_hash, &path, &target)?;
    }
    if !format!("{:x}", staged_hash.finalize()).eq_ignore_ascii_case(expected) {
        return Err("Skill content does not match the lock file".to_string());
    }
    lock_at_revision(root, revision)?;
    let destination = checked_destination(root, true)?;
    publish_new_directory(&staged, &destination.join(name))
        .map_err(|error| format!("Could not install the missing Skill: {error}"))
}

fn collect_skill_files(
    root: &Path,
    directory: &Path,
    prefix: &str,
    ancestors: &mut Vec<PathBuf>,
    files: &mut BTreeMap<String, PathBuf>,
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
        if name == "metadata.json" {
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
        if metadata.is_dir() && matches!(name.as_str(), ".git" | "__pycache__" | "__pypackages__") {
            continue;
        }
        if !valid_component(&name) || name.eq_ignore_ascii_case(".git") {
            return Err(format!("Unsupported Skill file name: {name}"));
        }
        if metadata.is_dir() {
            collect_skill_files(root, &source, &format!("{relative}/"), ancestors, files)?;
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

#[cfg(unix)]
fn publish_new_directory(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let source = CString::new(source.as_os_str().as_bytes())?;
    let target = CString::new(target.as_os_str().as_bytes())?;
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let result = -1;
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn publish_new_directory(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileW;

    let source: Vec<_> = source.as_os_str().encode_wide().chain([0]).collect();
    let target: Vec<_> = target.as_os_str().encode_wide().chain([0]).collect();
    if unsafe { MoveFileW(source.as_ptr(), target.as_ptr()) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
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
        fs::create_dir_all(&declaring).unwrap();
        fs::create_dir_all(package.join("scripts")).unwrap();
        fs::create_dir_all(outside.join("scripts")).unwrap();
        fs::write(declaring.join("package.json"), "{}").unwrap();
        for directory in [&package, &outside] {
            fs::write(directory.join("package.json"), br#"{"version":"2.0.0"}"#).unwrap();
            fs::write(directory.join("SKILL.md"), MANIFEST).unwrap();
            fs::write(directory.join("scripts/data.bin"), b"\0\r\n").unwrap();
        }
        for (source, name) in [
            (&declaring, "declaring-package"),
            (&outside, "outside-package"),
        ] {
            let link = root.join("node_modules").join(name);
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
        write_lock(
            &root,
            json!({ "version": 2, "skills": { "inside": entry, "outside": escaped } }),
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
            "source": "example/skills",
            "sourceType": "github",
            "commit": commit,
            "ref": "main",
            "skillName": "original",
            "skillPath": "skills/original/SKILL.md",
            "computedHash": PORTABLE_HASH,
        });
        let mut mismatch = entry.clone();
        mismatch["computedHash"] = "0".repeat(64).into();
        write_lock(
            &project,
            json!({
                "version": 2,
                "skills": { "alias": entry, "mismatch": mismatch, "existing": entry }
            }),
        );
        let target = project.join(".agents/skills");
        fs::create_dir_all(target.join("existing")).unwrap();
        fs::write(target.join("existing/SKILL.md"), "local edit").unwrap();
        let lock = preview(&project).unwrap();
        assert_eq!(lock.skills[0].status, Ready);
        assert_eq!(lock.skills[1].status, Existing);
        let names = vec!["alias".into(), "existing".into(), "mismatch".into()];
        let fetch_path = repo.to_str().unwrap();
        #[cfg(windows)]
        let fetch_path = fetch_path.strip_prefix(r"\\?\").unwrap_or(fetch_path);
        let mut fetches = 0;
        let results = restore_with(&project, &lock.revision, names, |remote| {
            assert_eq!(remote.url, "https://github.com/example/skills.git");
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
        assert!(results[2]
            .error
            .as_ref()
            .unwrap()
            .contains("content does not match"));
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
        assert_eq!(preview(&project).unwrap().revision, lock.revision);
    }

    #[test]
    fn never_replaces_a_destination_created_during_the_restore() {
        let temporary = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let source = temporary.path().join("staged");
        let occupied = temporary.path().join("occupied");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&occupied).unwrap();
        fs::write(source.join("SKILL.md"), "from archive").unwrap();
        assert!(publish_new_directory(&source, &occupied).is_err());
        assert!(source.join("SKILL.md").is_file());
        assert!(!occupied.join("SKILL.md").exists());
    }
}
