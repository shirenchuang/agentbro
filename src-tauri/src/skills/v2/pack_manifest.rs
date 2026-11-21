use super::models::SkillPackManifest;
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

fn validate(manifest: &SkillPackManifest) -> Result<(), String> {
    if manifest.format != FORMAT {
        return Err("This file does not describe an AgentBro Skill Pack.".to_string());
    }
    if manifest.version != 1 {
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
        if !seen.insert(id) {
            return Err(format!("Duplicate Skill ID '{id}'."));
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
