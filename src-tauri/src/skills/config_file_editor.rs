use crate::skills::v2::service::Service;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_CONFIG_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigDocument {
    pub path: String,
    pub content: String,
    pub revision: String,
}

pub fn read_agent_config_file(
    service: &Service,
    agent_id: &str,
    requested_path: &str,
) -> Result<AgentConfigDocument, String> {
    let path = resolve_editable_path(service, agent_id, requested_path)?;
    read_document_at(agent_id, &path)
}

pub fn write_agent_config_file(
    service: &Service,
    agent_id: &str,
    requested_path: &str,
    content: &str,
    expected_revision: &str,
) -> Result<AgentConfigDocument, String> {
    let path = resolve_editable_path(service, agent_id, requested_path)?;
    write_document_at(
        agent_id,
        &path,
        content,
        expected_revision,
        &crate::skills::v2::fsutil::agentbro_home()
            .join("config")
            .join("backups"),
    )
}

fn resolve_editable_path(
    service: &Service,
    agent_id: &str,
    requested_path: &str,
) -> Result<PathBuf, String> {
    let detail = service.get_agent_detail(agent_id)?;
    let requested = crate::skills::v2::fsutil::normalized_path(
        &crate::skills::v2::fsutil::expand_tilde(requested_path.trim()),
    );
    let allowed = [detail.config_path, detail.mcp_config_path];
    let matched = allowed.into_iter().flatten().find_map(|candidate| {
        if is_url(&candidate) {
            return None;
        }
        let candidate = crate::skills::v2::fsutil::normalized_path(
            &crate::skills::v2::fsutil::expand_tilde(&candidate),
        );
        (candidate == requested).then_some(candidate)
    });
    let path = matched.ok_or_else(|| {
        "This file is not an editable configuration resource for the selected Agent.".to_string()
    })?;
    ensure_supported_config_file(&path)?;
    Ok(path)
}

fn is_url(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.starts_with("http://") || value.starts_with("https://")
}

fn ensure_supported_config_file(path: &Path) -> Result<(), String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "json" | "toml") {
        return Err(
            "Only JSON and TOML configuration files can be edited in AgentBro.".to_string(),
        );
    }
    Ok(())
}

fn read_document_at(agent_id: &str, path: &Path) -> Result<AgentConfigDocument, String> {
    ensure_supported_config_file(path)?;
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Failed to inspect {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("Configuration file not found: {}", path.display()));
    }
    if metadata.len() > MAX_CONFIG_BYTES as u64 {
        return Err(format!(
            "Configuration file is larger than {} MB and cannot be edited here.",
            MAX_CONFIG_BYTES / 1024 / 1024
        ));
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    let content = String::from_utf8(bytes.clone())
        .map_err(|_| "Configuration file is not valid UTF-8.".to_string())?;
    Ok(AgentConfigDocument {
        path: path.display().to_string(),
        content,
        revision: revision_for(agent_id, path, &bytes),
    })
}

fn write_document_at(
    agent_id: &str,
    path: &Path,
    content: &str,
    expected_revision: &str,
    backup_root: &Path,
) -> Result<AgentConfigDocument, String> {
    ensure_supported_config_file(path)?;
    if content.len() > MAX_CONFIG_BYTES {
        return Err(format!(
            "Configuration file is larger than {} MB and cannot be saved here.",
            MAX_CONFIG_BYTES / 1024 / 1024
        ));
    }
    validate_content(path, content)?;
    let original =
        fs::read(path).map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    if revision_for(agent_id, path, &original) != expected_revision {
        return Err("Configuration changed outside AgentBro. Reload it before saving.".to_string());
    }

    write_backup(backup_root, agent_id, path, &original)?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("Invalid configuration path: {}", path.display()))?;
    let temp = parent.join(format!(
        ".{}.agentbro-{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config"),
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        write_private_file(&temp, content.as_bytes())?;
        fs::rename(&temp, path)
            .map_err(|error| format!("Failed to replace {}: {error}", path.display()))?;
        let written = fs::read(path)
            .map_err(|error| format!("Failed to verify {}: {error}", path.display()))?;
        let written_content = std::str::from_utf8(&written)
            .map_err(|_| "Saved configuration is not valid UTF-8.".to_string())?;
        validate_content(path, written_content)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        let _ = write_private_file(path, &original);
        return Err(error);
    }
    read_document_at(agent_id, path)
}

fn validate_content(path: &Path, content: &str) -> Result<(), String> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "json" => serde_json::from_str::<serde_json::Value>(content)
            .map(|_| ())
            .map_err(|error| format!("Invalid JSON: {error}")),
        "toml" => crate::skills::mcp_management::validate_toml_shape(content)
            .map_err(|error| format!("Invalid TOML: {error}")),
        _ => Err("Only JSON and TOML configuration files can be edited in AgentBro.".to_string()),
    }
}

fn revision_for(agent_id: &str, path: &Path, content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(agent_id.as_bytes());
    hasher.update(path.as_os_str().as_encoded_bytes());
    hasher.update(content);
    format!("sha256:{:x}", hasher.finalize())
}

fn write_backup(
    backup_root: &Path,
    agent_id: &str,
    path: &Path,
    content: &[u8],
) -> Result<(), String> {
    let dir = backup_root.join(sanitize_component(agent_id));
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Failed to create config backup directory: {error}"))?;
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");
    write_private_file(&dir.join(format!("{timestamp}-{filename}")), content)
}

fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn write_private_file(path: &Path, content: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))?;
    file.write_all(content)
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("Failed to sync {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("Failed to secure {}: {error}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("agentbro-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn saves_json_with_backup_and_new_revision() {
        let root = temp_dir("config-editor-save");
        let path = root.join("config.json");
        fs::write(&path, b"{\"enabled\":false}\n").unwrap();
        let original = read_document_at("codex", &path).unwrap();

        let saved = write_document_at(
            "codex",
            &path,
            "{\"enabled\":true}\n",
            &original.revision,
            &root.join("backups"),
        )
        .unwrap();

        assert_eq!(saved.content, "{\"enabled\":true}\n");
        assert_ne!(saved.revision, original.revision);
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"enabled\":true}\n");
        assert_eq!(fs::read_dir(root.join("backups/codex")).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_invalid_json_without_changing_the_file() {
        let root = temp_dir("config-editor-invalid");
        let path = root.join("config.json");
        fs::write(&path, b"{\"enabled\":false}\n").unwrap();
        let original = read_document_at("codex", &path).unwrap();

        let error = write_document_at(
            "codex",
            &path,
            "{\"enabled\":",
            &original.revision,
            &root.join("backups"),
        )
        .unwrap_err();

        assert!(error.contains("Invalid JSON"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"enabled\":false}\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_an_external_change_before_saving() {
        let root = temp_dir("config-editor-conflict");
        let path = root.join("config.json");
        fs::write(&path, b"{\"value\":1}\n").unwrap();
        let original = read_document_at("codex", &path).unwrap();
        fs::write(&path, b"{\"value\":2}\n").unwrap();

        let error = write_document_at(
            "codex",
            &path,
            "{\"value\":3}\n",
            &original.revision,
            &root.join("backups"),
        )
        .unwrap_err();

        assert!(error.contains("changed outside AgentBro"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"value\":2}\n");
        fs::remove_dir_all(root).unwrap();
    }
}
