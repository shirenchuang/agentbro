use super::models::{ReviewedSkillPackArchive, SkillPackGitHubSource, SkillPackManifest};
use super::{pack_manifest, project_lock};
use crate::skills::installer::TempInstallDir;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const MANIFEST_NAME: &str = "agentbro-skill-pack.json";
const MAX_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_CONTENT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;
#[cfg(unix)]
const EXECUTABLE_BITS: u32 = 0o111;

pub(super) struct Archive {
    pub(super) review: ReviewedSkillPackArchive,
    files: ZipArchive<Cursor<Vec<u8>>>,
    members: BTreeMap<String, Vec<(String, usize)>>,
}

impl Archive {
    pub(super) fn open(path: &Path) -> Result<Self, String> {
        let metadata = fs::metadata(path).map_err(archive_error)?;
        if !metadata.is_file() || metadata.len() > MAX_ARCHIVE_BYTES {
            return Err("Choose a regular Skill Pack ZIP no larger than 128 MiB.".into());
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options.open(path).map_err(archive_error)?;
        if !file.metadata().map_err(archive_error)?.is_file() {
            return Err("Choose a regular Skill Pack ZIP.".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_ARCHIVE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(archive_error)?;
        if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
            return Err("Skill Pack ZIP must be 128 MiB or smaller.".into());
        }
        let revision = format!("{:x}", Sha256::digest(&bytes));
        let mut files = ZipArchive::new(Cursor::new(bytes)).map_err(archive_error)?;
        if files.len() > MAX_ENTRIES {
            return Err("Skill Pack ZIP has too many files.".into());
        }
        let mut members: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new();
        #[cfg(unix)]
        let mut modes = BTreeMap::<String, BTreeMap<String, u32>>::new();
        let mut names = HashMap::new();
        let mut manifest_index = None;
        let mut content_size = 0;
        for index in 0..files.len() {
            let entry = files.by_index_raw(index).map_err(archive_error)?;
            let name = std::str::from_utf8(entry.name_raw()).map_err(archive_error)?;
            if !entry.is_file()
                || entry.encrypted()
                || entry
                    .unix_mode()
                    .is_some_and(|mode| !matches!(mode & 0o170000, 0 | 0o100000))
                || !matches!(
                    entry.compression(),
                    CompressionMethod::Stored | CompressionMethod::Deflated
                )
            {
                return Err(format!("Unsupported Skill Pack ZIP entry: {name}"));
            }
            content_size = check_content_size(content_size, entry.size())?;
            if name == MANIFEST_NAME {
                if entry.size() > pack_manifest::MAX_BYTES as u64 {
                    return Err("Skill Pack JSON must be 1 MiB or smaller.".into());
                }
                manifest_index = Some(index);
                continue;
            }
            let (skill_id, relative) = name
                .strip_prefix("skills/")
                .and_then(|name| name.split_once('/'))
                .filter(|(id, _)| project_lock::valid_skill_name(id))
                .ok_or("Invalid Skill Pack ZIP entry path.")?;
            register_path(&mut names, name)?;
            #[cfg(unix)]
            {
                let mode = entry.unix_mode().unwrap_or_default() & EXECUTABLE_BITS;
                if mode != 0 {
                    modes
                        .entry(skill_id.to_string())
                        .or_default()
                        .insert(relative.to_string(), mode);
                }
            }
            members
                .entry(skill_id.to_string())
                .or_default()
                .push((relative.to_string(), index));
        }
        let mut entry = files
            .by_index(manifest_index.ok_or("Skill Pack ZIP is missing its manifest.")?)
            .map_err(archive_error)?;
        let mut manifest_bytes = Vec::new();
        copy_entry(&mut entry, &mut manifest_bytes)?;
        drop(entry);
        let manifest = pack_manifest::decode(&manifest_bytes)?;
        validate_member_ids(&manifest)?;
        if members.len() != manifest.skill_ids.len()
            || manifest.skill_ids.iter().any(|id| {
                !manifest.skill_fingerprints.contains_key(id)
                    || !members
                        .get(id)
                        .is_some_and(|paths| paths.iter().any(|(name, _)| name == "SKILL.md"))
            })
        {
            return Err(
                "Skill Pack ZIP must contain every listed Skill and its fingerprint.".into(),
            );
        }
        #[cfg(unix)]
        let executable_fingerprints = members
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    executable_fingerprint(modes.remove(id).unwrap_or_default()),
                )
            })
            .collect();
        #[cfg(not(unix))]
        let executable_fingerprints = BTreeMap::new();
        Ok(Self {
            review: ReviewedSkillPackArchive {
                manifest,
                revision,
                executable_fingerprints,
            },
            files,
            members,
        })
    }

    pub(super) fn stage_member(&mut self, id: &str, destination: &Path) -> Result<(), String> {
        let paths = self
            .members
            .get(id)
            .ok_or("Skill is missing from the Skill Pack ZIP.")?;
        fs::create_dir(destination).map_err(archive_error)?;
        for (path, index) in paths {
            let mut entry = self.files.by_index(*index).map_err(archive_error)?;
            let mut file = create_file(&destination.join(path))?;
            copy_entry(&mut entry, &mut file)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = 0o644 | (entry.unix_mode().unwrap_or(0) & EXECUTABLE_BITS);
                file.set_permissions(fs::Permissions::from_mode(mode))
                    .map_err(archive_error)?;
            }
        }
        let actual = fingerprint(destination)?;
        if self.review.manifest.skill_fingerprints.get(id) != Some(&actual) {
            return Err(format!(
                "Archived Skill '{id}' does not match its recorded fingerprint."
            ));
        }
        Ok(())
    }
}

pub(super) fn write(
    path: &Path,
    mut manifest: SkillPackManifest,
    skill_source: impl Fn(&str) -> Result<(PathBuf, Option<SkillPackGitHubSource>), String>,
) -> Result<(), String> {
    validate_member_ids(&manifest)?;
    let parent = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .ok_or("Choose an export file inside a folder.")?;
    let parent = parent.canonicalize().map_err(archive_error)?;
    let sources = manifest
        .skill_ids
        .iter()
        .map(|id| {
            let (directory, origin) = skill_source(id)?;
            let directory = directory.canonicalize().map_err(archive_error)?;
            if parent.starts_with(&directory) {
                return Err("Choose an export folder outside the Skills in this Pack.".into());
            }
            Ok((directory, origin))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let staging = TempInstallDir::create_hidden_in(&parent)?;
    let output = staging.path().join("pack.zip");
    let mut zip = ZipWriter::new(File::create(&output).map_err(archive_error)?);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashMap::new();
    let mut content_size = 0;
    let mut file_count = 1;
    for (id, (source, origin)) in manifest.skill_ids.iter().zip(sources) {
        let files = project_lock::portable_skill_files(&source, false)?;
        file_count += files.len();
        if file_count > MAX_ENTRIES {
            return Err("Skill Pack ZIP has too many files.".into());
        }
        let member = TempInstallDir::create_in(staging.path())?;
        for (relative, source_file) in files {
            let name = format!("skills/{id}/{relative}");
            register_path(&mut names, &name)?;
            let saved = member.path().join(relative);
            let mut staged = create_file(&saved)?;
            let input = File::open(source_file).map_err(archive_error)?;
            let metadata = input.metadata().map_err(archive_error)?;
            if !metadata.is_file() {
                return Err(format!("Expected a regular Skill file: {name}"));
            }
            let copied = io::copy(
                &mut input.take(MAX_CONTENT_BYTES - content_size + 1),
                &mut staged,
            )
            .map_err(archive_error)?;
            content_size = check_content_size(content_size, copied)?;
            drop(staged);
            #[cfg(unix)]
            let options = {
                use std::os::unix::fs::PermissionsExt;
                options.unix_permissions(0o644 | (metadata.permissions().mode() & EXECUTABLE_BITS))
            };
            zip.start_file(name, options).map_err(archive_error)?;
            io::copy(&mut File::open(saved).map_err(archive_error)?, &mut zip)
                .map_err(archive_error)?;
        }
        manifest
            .skill_fingerprints
            .insert(id.clone(), fingerprint(member.path())?);
        if let Some(origin) = origin {
            manifest.skill_sources.insert(id.clone(), origin);
            manifest.version = manifest.version.max(3);
        }
    }
    let bytes = pack_manifest::encode(&manifest)?;
    check_content_size(content_size, bytes.len() as u64)?;
    zip.start_file(MANIFEST_NAME, options)
        .map_err(archive_error)?;
    zip.write_all(&bytes).map_err(archive_error)?;
    let output_file = zip.finish().map_err(archive_error)?;
    if output_file.metadata().map_err(archive_error)?.len() > MAX_ARCHIVE_BYTES {
        return Err("Skill Pack ZIP must be 128 MiB or smaller.".into());
    }
    drop(output_file);
    fs::rename(output, path).map_err(archive_error)
}

fn validate_member_ids(manifest: &SkillPackManifest) -> Result<(), String> {
    pack_manifest::validate(manifest)?;
    let mut names = HashSet::new();
    if manifest.version < 2
        || manifest
            .skill_ids
            .iter()
            .any(|id| !project_lock::valid_skill_name(id) || !names.insert(id.to_lowercase()))
    {
        return Err("Skill Pack ZIP requires distinct, portable Skill IDs and a manifest with fingerprints.".into());
    }
    Ok(())
}

fn register_path(names: &mut HashMap<String, String>, path: &str) -> Result<(), String> {
    if path.len() > 1024
        || path.split('/').count() > 32
        || path
            .split('/')
            .any(|part| !project_lock::valid_component(part) || part.eq_ignore_ascii_case(".git"))
        || path.rsplit_once('/').is_some_and(|(directories, _)| {
            directories
                .split('/')
                .skip(2)
                .any(project_lock::is_skill_cache_dir)
        })
    {
        return Err(format!("Invalid Skill Pack ZIP entry path: {path}"));
    }
    for prefix in path
        .match_indices('/')
        .map(|(index, _)| &path[..=index])
        .chain([path])
    {
        let key = prefix.trim_end_matches('/').to_lowercase();
        match names.entry(key) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(prefix.to_string());
            }
            std::collections::hash_map::Entry::Occupied(entry) => {
                if !prefix.ends_with('/') || entry.get() != prefix {
                    return Err(format!("Conflicting Skill Pack ZIP entry path: {path}"));
                }
            }
        }
    }
    Ok(())
}

fn check_content_size(total: u64, bytes: u64) -> Result<u64, String> {
    total
        .checked_add(bytes)
        .filter(|size| *size <= MAX_CONTENT_BYTES)
        .ok_or_else(|| "Uncompressed Skill Pack ZIP must be 256 MiB or smaller.".into())
}

fn copy_entry(
    entry: &mut zip::read::ZipFile<'_>,
    destination: &mut impl Write,
) -> Result<(), String> {
    let size = entry.size();
    if io::copy(&mut entry.take(size + 1), destination).map_err(archive_error)? != size {
        return Err("Skill Pack ZIP entry size does not match its header.".into());
    }
    Ok(())
}

fn create_file(path: &Path) -> Result<File, String> {
    fs::create_dir_all(path.parent().ok_or("Invalid Skill file destination.")?)
        .map_err(archive_error)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(archive_error)
}

fn fingerprint(directory: &Path) -> Result<String, String> {
    project_lock::fingerprint_canonical_skill_directory(
        &directory.canonicalize().map_err(archive_error)?,
        false,
    )
}

pub(super) fn directory_executable_fingerprint(directory: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let files = project_lock::portable_skill_files(directory, false).ok()?;
        let mut modes = BTreeMap::new();
        for (path, file) in files {
            let mode = file.metadata().ok()?.permissions().mode() & EXECUTABLE_BITS;
            if mode != 0 {
                modes.insert(path, mode);
            }
        }
        Some(executable_fingerprint(modes))
    }
    #[cfg(not(unix))]
    {
        let _ = directory;
        None
    }
}

#[cfg(unix)]
fn executable_fingerprint(modes: BTreeMap<String, u32>) -> String {
    let mut hash = Sha256::new();
    for (path, mode) in modes {
        hash.update(path.as_bytes());
        hash.update([0, mode as u8]);
    }
    format!("{:x}", hash.finalize())
}

fn archive_error(error: impl std::fmt::Display) -> String {
    format!("Skill Pack ZIP: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_zip(path: &Path, files: &[&str], link: Option<&str>) {
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        let options = SimpleFileOptions::default();
        for name in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(b"test archive content").unwrap();
        }
        if let Some(name) = link {
            zip.add_symlink(name, "SKILL.md", options).unwrap();
        }
        let manifest = SkillPackManifest {
            format: "agentbro-skill-pack".into(),
            version: 2,
            name: "Fixture".into(),
            description: String::new(),
            tags: vec![],
            skill_ids: vec!["test".into()],
            skill_fingerprints: BTreeMap::from([("test".into(), "0".repeat(64))]),
            skill_sources: BTreeMap::new(),
            suggested_agent_ids: vec![],
        };
        zip.start_file(MANIFEST_NAME, options).unwrap();
        zip.write_all(&pack_manifest::encode(&manifest).unwrap())
            .unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn rejects_invalid_paths_and_conflicting_file_names_before_extraction() {
        let fixture = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = fixture.path().join("external-pack.zip");
        for extra in [
            "skills/test/../elsewhere",
            "skills/test/docs\\unsafe",
            "skills/test/CONOUT$.txt",
            "skills/test/CON .txt",
            "skills/test/__pycache__/hidden.pyc",
            "skills/test/skill.md",
            "skills/test/SKILL.md/data.txt",
            "elsewhere.txt",
        ] {
            fixture_zip(&path, &["skills/test/SKILL.md", extra], None);
            assert!(Archive::open(&path).is_err(), "{extra}");
        }
        fixture_zip(
            &path,
            &["skills/test/SKILL.md"],
            Some("skills/test/reference.md"),
        );
        assert!(Archive::open(&path).is_err());
    }

    #[test]
    fn verifies_the_fingerprint_of_staged_archive_content() {
        let fixture = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = fixture.path().join("invalid-content.zip");
        fixture_zip(&path, &["skills/test/SKILL.md"], None);
        let mut archive = Archive::open(&path).unwrap();
        assert!(archive
            .stage_member("test", &fixture.path().join("staged"))
            .unwrap_err()
            .contains("does not match"));
    }
}
