use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use super::v2::models::PluginStatus;
use super::v2::service::Service;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCapabilities {
    pub editable: bool,
    pub requires_new_session: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInventory {
    pub agent_id: String,
    pub config_path: Option<String>,
    pub revision: String,
    pub capabilities: PluginCapabilities,
    pub plugins: Vec<PluginStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginFileNode {
    pub name: String,
    pub node_type: String,
    pub path: String,
    pub children: Option<Vec<PluginFileNode>>,
    pub omitted_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDetail {
    #[serde(flatten)]
    pub plugin: PluginStatus,
    pub description: Option<String>,
    pub author: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub install_path: Option<String>,
    pub manifest_path: Option<String>,
    pub files: Option<PluginFileNode>,
    pub file_count: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginFileContent {
    pub path: String,
    pub kind: String,
    pub mime_type: Option<String>,
    pub content: Option<String>,
    pub data_base64: Option<String>,
    pub size: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy)]
enum PluginConfigKind {
    Codex,
    Json,
    Zcode,
    Kimi,
}

struct PluginConfig {
    path: PathBuf,
    kind: PluginConfigKind,
}

pub fn list_plugins(svc: &Service, agent_id: &str) -> Result<PluginInventory, String> {
    let config = config_for_agent(svc.home(), agent_id);
    let config_path = config.as_ref().map(|config| config.path.as_path());
    Ok(PluginInventory {
        agent_id: agent_id.to_string(),
        config_path: config_path.map(|path| path.display().to_string()),
        revision: revision_for(agent_id, config_path)?,
        capabilities: PluginCapabilities {
            editable: config.is_some(),
            requires_new_session: true,
        },
        plugins: crate::skills::v2::diagnosis::read_plugins(svc, agent_id),
    })
}

pub fn set_plugin_enabled(
    svc: &Service,
    agent_id: &str,
    plugin_id: &str,
    revision: &str,
    enabled: bool,
) -> Result<PluginInventory, String> {
    let config = config_for_agent(svc.home(), agent_id).ok_or_else(|| {
        format!("Agent {agent_id} does not support editable plugin configuration")
    })?;
    ensure_revision(agent_id, &config.path, revision)?;
    let inventory = list_plugins(svc, agent_id)?;
    if !inventory
        .plugins
        .iter()
        .any(|plugin| plugin.id == plugin_id)
    {
        return Err(format!(
            "Plugin '{plugin_id}' is no longer installed. Reload the plugin list."
        ));
    }

    match config.kind {
        PluginConfigKind::Codex => {
            let content = read_text_or_empty(&config.path)?;
            let updated =
                crate::skills::codex_config::set_plugin_enabled(&content, plugin_id, enabled)?;
            safe_write(&config.path, updated.as_bytes(), agent_id, |written| {
                let content = std::str::from_utf8(written).map_err(|error| error.to_string())?;
                let actual = crate::skills::codex_config::parse_plugin_enabled_config(content)
                    .get(plugin_id)
                    .copied();
                (actual == Some(enabled))
                    .then_some(())
                    .ok_or_else(|| format!("Plugin '{plugin_id}' state could not be verified"))
            })?;
        }
        PluginConfigKind::Json | PluginConfigKind::Zcode | PluginConfigKind::Kimi => {
            let mut root = read_json_or_empty(&config.path)?;
            set_json_plugin_enabled(&mut root, config.kind, plugin_id, enabled)?;
            let content =
                serde_json::to_string_pretty(&root).map_err(|error| error.to_string())? + "\n";
            safe_write(&config.path, content.as_bytes(), agent_id, |written| {
                let parsed =
                    serde_json::from_slice::<Value>(written).map_err(|error| error.to_string())?;
                let actual = json_plugin_enabled(&parsed, config.kind, plugin_id);
                (actual == Some(enabled))
                    .then_some(())
                    .ok_or_else(|| format!("Plugin '{plugin_id}' state could not be verified"))
            })?;
        }
    }

    list_plugins(svc, agent_id)
}

pub fn get_plugin_detail(
    svc: &Service,
    agent_id: &str,
    plugin_id: &str,
) -> Result<PluginDetail, String> {
    let inventory = list_plugins(svc, agent_id)?;
    let plugin = inventory
        .plugins
        .into_iter()
        .find(|plugin| plugin.id == plugin_id)
        .ok_or_else(|| {
            format!("Plugin '{plugin_id}' is no longer installed. Reload the plugin list.")
        })?;
    let Some(location) = crate::skills::v2::diagnosis::find_plugin_location(svc, agent_id, &plugin)
    else {
        return Ok(PluginDetail {
            plugin,
            description: None,
            author: None,
            homepage: None,
            license: None,
            install_path: None,
            manifest_path: None,
            files: None,
            file_count: 0,
            truncated: false,
        });
    };

    let manifest = location
        .manifest
        .as_ref()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|content| serde_json::from_str::<Value>(&content).ok());
    let description = manifest.as_ref().and_then(manifest_description);
    let author = manifest.as_ref().and_then(manifest_author);
    let homepage = manifest.as_ref().and_then(manifest_homepage);
    let license = manifest
        .as_ref()
        .and_then(|manifest| manifest.get("license"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let (files, file_count, truncated) = if location.root.is_dir() {
        let mut visible_count = 0;
        let mut truncated = false;
        let files = build_file_tree(
            &location.root,
            &location.root,
            0,
            &mut visible_count,
            &mut truncated,
        );
        (Some(files), visible_count, truncated)
    } else {
        (None, 0, false)
    };

    Ok(PluginDetail {
        plugin,
        description,
        author,
        homepage,
        license,
        install_path: Some(location.root.display().to_string()),
        manifest_path: location
            .manifest
            .filter(|path| path.is_file())
            .map(|path| path.display().to_string()),
        files,
        file_count,
        truncated,
    })
}

pub fn read_plugin_file(
    svc: &Service,
    agent_id: &str,
    plugin_id: &str,
    relative_path: &str,
) -> Result<PluginFileContent, String> {
    let inventory = list_plugins(svc, agent_id)?;
    let plugin = inventory
        .plugins
        .into_iter()
        .find(|plugin| plugin.id == plugin_id)
        .ok_or_else(|| {
            format!("Plugin '{plugin_id}' is no longer installed. Reload the plugin list.")
        })?;
    let location = crate::skills::v2::diagnosis::find_plugin_location(svc, agent_id, &plugin)
        .ok_or_else(|| format!("Plugin '{plugin_id}' is not available in the local cache."))?;
    read_plugin_file_at(&location.root, relative_path)
}

fn manifest_description(manifest: &Value) -> Option<String> {
    manifest
        .pointer("/interface/longDescription")
        .and_then(Value::as_str)
        .or_else(|| manifest.get("description").and_then(Value::as_str))
        .or_else(|| {
            manifest
                .pointer("/interface/shortDescription")
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

fn manifest_author(manifest: &Value) -> Option<String> {
    manifest
        .get("author")
        .and_then(|author| {
            author
                .as_str()
                .or_else(|| author.get("name").and_then(Value::as_str))
        })
        .or_else(|| {
            manifest
                .pointer("/interface/developerName")
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

fn manifest_homepage(manifest: &Value) -> Option<String> {
    manifest
        .pointer("/interface/websiteURL")
        .and_then(Value::as_str)
        .or_else(|| manifest.get("homepage").and_then(Value::as_str))
        .map(str::to_string)
}

const FILE_TREE_MAX_DEPTH: usize = 4;
const FILE_TREE_MAX_ENTRIES: usize = 240;
const TEXT_PREVIEW_MAX_BYTES: usize = 512 * 1024;
const IMAGE_PREVIEW_MAX_BYTES: u64 = 8 * 1024 * 1024;

fn build_file_tree(
    path: &Path,
    root: &Path,
    depth: usize,
    visible_count: &mut usize,
    truncated: &mut bool,
) -> PluginFileNode {
    let name = if depth == 0 {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("plugin")
            .to_string()
    } else {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("item")
            .to_string()
    };
    let relative_path = path
        .strip_prefix(root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|| ".".to_string());
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return PluginFileNode {
            name,
            node_type: "file".to_string(),
            path: relative_path,
            children: None,
            omitted_count: None,
        };
    };
    if metadata.file_type().is_symlink() {
        return PluginFileNode {
            name,
            node_type: "symlink".to_string(),
            path: relative_path,
            children: None,
            omitted_count: None,
        };
    }
    if !metadata.is_dir() {
        return PluginFileNode {
            name,
            node_type: "file".to_string(),
            path: relative_path,
            children: None,
            omitted_count: None,
        };
    }
    if depth > 0 && is_collapsed_directory(path) {
        return PluginFileNode {
            name,
            node_type: "directory".to_string(),
            path: relative_path,
            children: Some(Vec::new()),
            omitted_count: Some(
                fs::read_dir(path)
                    .map(|entries| entries.flatten().count())
                    .unwrap_or_default(),
            ),
        };
    }
    if depth >= FILE_TREE_MAX_DEPTH {
        if fs::read_dir(path)
            .ok()
            .and_then(|mut entries| entries.next())
            .is_some()
        {
            *truncated = true;
        }
        return PluginFileNode {
            name,
            node_type: "directory".to_string(),
            path: relative_path,
            children: Some(Vec::new()),
            omitted_count: None,
        };
    }

    let mut entries = fs::read_dir(path)
        .map(|entries| entries.flatten().collect::<Vec<_>>())
        .unwrap_or_default();
    entries.sort_by(|left, right| {
        let left_is_dir = left.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        let right_is_dir = right.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        right_is_dir
            .cmp(&left_is_dir)
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });
    let mut children = Vec::new();
    for entry in entries {
        if *visible_count >= FILE_TREE_MAX_ENTRIES {
            *truncated = true;
            break;
        }
        *visible_count += 1;
        children.push(build_file_tree(
            &entry.path(),
            root,
            depth + 1,
            visible_count,
            truncated,
        ));
    }
    PluginFileNode {
        name,
        node_type: "directory".to_string(),
        path: relative_path,
        children: Some(children),
        omitted_count: None,
    }
}

fn is_collapsed_directory(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("node_modules" | ".git" | "target" | "vendor" | "__pycache__")
    )
}

fn read_plugin_file_at(root: &Path, relative_path: &str) -> Result<PluginFileContent, String> {
    let relative = Path::new(relative_path);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("Plugin file path must be a relative path inside the plugin.".to_string());
    }

    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Failed to resolve plugin directory: {error}"))?;
    let mut candidate = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err("Plugin file path is invalid.".to_string());
        };
        candidate.push(segment);
        let metadata = fs::symlink_metadata(&candidate)
            .map_err(|error| format!("Failed to inspect plugin file: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("Symbolic links cannot be opened from the plugin preview.".to_string());
        }
    }

    let canonical_file = fs::canonicalize(&candidate)
        .map_err(|error| format!("Failed to resolve plugin file: {error}"))?;
    if !canonical_file.starts_with(&canonical_root) {
        return Err("Plugin file path escapes the plugin directory.".to_string());
    }
    let metadata = fs::metadata(&canonical_file)
        .map_err(|error| format!("Failed to inspect plugin file: {error}"))?;
    if !metadata.is_file() {
        return Err("Only regular plugin files can be previewed.".to_string());
    }

    let path = relative.display().to_string();
    if let Some(mime_type) = preview_image_mime(relative) {
        if metadata.len() > IMAGE_PREVIEW_MAX_BYTES {
            return Ok(PluginFileContent {
                path,
                kind: "image".to_string(),
                mime_type: Some(mime_type.to_string()),
                content: None,
                data_base64: None,
                size: metadata.len(),
                truncated: true,
            });
        }
        let mut bytes = Vec::new();
        fs::File::open(&canonical_file)
            .map_err(|error| format!("Failed to open plugin image: {error}"))?
            .take(IMAGE_PREVIEW_MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Failed to read plugin image: {error}"))?;
        if bytes.len() as u64 > IMAGE_PREVIEW_MAX_BYTES {
            return Ok(PluginFileContent {
                path,
                kind: "image".to_string(),
                mime_type: Some(mime_type.to_string()),
                content: None,
                data_base64: None,
                size: bytes.len() as u64,
                truncated: true,
            });
        }
        return Ok(PluginFileContent {
            path,
            kind: "image".to_string(),
            mime_type: Some(mime_type.to_string()),
            content: None,
            data_base64: Some(BASE64_STANDARD.encode(bytes)),
            size: metadata.len(),
            truncated: false,
        });
    }

    let mut bytes = Vec::new();
    fs::File::open(&canonical_file)
        .map_err(|error| format!("Failed to open plugin file: {error}"))?
        .take((TEXT_PREVIEW_MAX_BYTES + 4) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read plugin file: {error}"))?;
    let truncated = metadata.len() > TEXT_PREVIEW_MAX_BYTES as u64;
    if bytes.contains(&0) {
        return Ok(binary_plugin_file(path, metadata.len()));
    }

    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(error) if truncated && error.error_len().is_none() => {
            std::str::from_utf8(&bytes[..error.valid_up_to()])
                .map_err(|_| "Plugin file is not valid UTF-8 text.".to_string())?
        }
        Err(_) => return Ok(binary_plugin_file(path, metadata.len())),
    };
    let mut content = text.to_string();
    if content.len() > TEXT_PREVIEW_MAX_BYTES {
        let mut boundary = TEXT_PREVIEW_MAX_BYTES;
        while !content.is_char_boundary(boundary) {
            boundary -= 1;
        }
        content.truncate(boundary);
    }
    Ok(PluginFileContent {
        path,
        kind: "text".to_string(),
        mime_type: Some("text/plain; charset=utf-8".to_string()),
        content: Some(content),
        data_base64: None,
        size: metadata.len(),
        truncated,
    })
}

fn preview_image_mime(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        Some("avif") => Some("image/avif"),
        Some("bmp") => Some("image/bmp"),
        Some("ico") => Some("image/x-icon"),
        _ => None,
    }
}

fn binary_plugin_file(path: String, size: u64) -> PluginFileContent {
    PluginFileContent {
        path,
        kind: "binary".to_string(),
        mime_type: None,
        content: None,
        data_base64: None,
        size,
        truncated: false,
    }
}

fn config_for_agent(home: &Path, agent_id: &str) -> Option<PluginConfig> {
    let (path, kind) = match agent_id {
        "codex" => (home.join(".codex/config.toml"), PluginConfigKind::Codex),
        "claude-code" => (home.join(".claude/settings.json"), PluginConfigKind::Json),
        "workbuddy" => (
            home.join(".workbuddy/settings.json"),
            PluginConfigKind::Json,
        ),
        "zcode" => (home.join(".zcode/cli/config.json"), PluginConfigKind::Zcode),
        "kimi" => (
            crate::skills::agent_paths::kimi_code_home_for(home).join("plugins/installed.json"),
            PluginConfigKind::Kimi,
        ),
        _ => return None,
    };
    Some(PluginConfig { path, kind })
}

fn read_text_or_empty(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("Failed to read {}: {error}", path.display())),
    }
}

fn read_json_or_empty(path: &Path) -> Result<Value, String> {
    let content = read_text_or_empty(path)?;
    if content.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    serde_json::from_str(&content)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))
}

fn set_json_plugin_enabled(
    root: &mut Value,
    kind: PluginConfigKind,
    plugin_id: &str,
    enabled: bool,
) -> Result<(), String> {
    match kind {
        PluginConfigKind::Json => {
            root.as_object_mut()
                .ok_or_else(|| "Plugin settings must be a JSON object".to_string())?
                .entry("enabledPlugins")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .ok_or_else(|| "enabledPlugins must be a JSON object".to_string())?
                .insert(plugin_id.to_string(), Value::Bool(enabled));
            Ok(())
        }
        PluginConfigKind::Zcode => {
            crate::skills::zcode_config::enabled_plugins_mut(root)?
                .insert(plugin_id.to_string(), Value::Bool(enabled));
            Ok(())
        }
        PluginConfigKind::Kimi => {
            let entries = root
                .get_mut("plugins")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| "Kimi installed plugins list is missing".to_string())?;
            let entry = entries
                .iter_mut()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(plugin_id))
                .and_then(Value::as_object_mut)
                .ok_or_else(|| format!("Kimi plugin '{plugin_id}' is no longer installed"))?;
            entry.insert("enabled".to_string(), Value::Bool(enabled));
            Ok(())
        }
        PluginConfigKind::Codex => Err("Codex plugin settings require TOML".to_string()),
    }
}

fn json_plugin_enabled(root: &Value, kind: PluginConfigKind, plugin_id: &str) -> Option<bool> {
    match kind {
        PluginConfigKind::Json => root
            .get("enabledPlugins")
            .and_then(Value::as_object)
            .and_then(|plugins| plugins.get(plugin_id))
            .and_then(Value::as_bool),
        PluginConfigKind::Zcode => crate::skills::zcode_config::enabled_plugins(root)
            .and_then(|plugins| plugins.get(plugin_id))
            .and_then(Value::as_bool),
        PluginConfigKind::Kimi => root
            .get("plugins")
            .and_then(Value::as_array)
            .and_then(|plugins| {
                plugins
                    .iter()
                    .find(|entry| entry.get("id").and_then(Value::as_str) == Some(plugin_id))
            })
            .and_then(|entry| entry.get("enabled"))
            .and_then(Value::as_bool),
        PluginConfigKind::Codex => None,
    }
}

fn revision_for(agent_id: &str, config_path: Option<&Path>) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(agent_id.as_bytes());
    if let Some(path) = config_path {
        match fs::read(path) {
            Ok(content) => hasher.update(content),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Failed to read {}: {error}", path.display())),
        }
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn ensure_revision(agent_id: &str, path: &Path, expected: &str) -> Result<(), String> {
    let current = revision_for(agent_id, Some(path))?;
    if current != expected {
        return Err(
            "Plugin configuration changed outside AgentBro. Reload before saving.".to_string(),
        );
    }
    Ok(())
}

fn safe_write(
    path: &Path,
    content: &[u8],
    agent_id: &str,
    validate: impl Fn(&[u8]) -> Result<(), String>,
) -> Result<(), String> {
    validate(content)?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("Invalid plugin config path: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
    let original = fs::read(path).ok();
    if let Some(bytes) = original.as_ref() {
        write_backup(path, bytes, agent_id)?;
    }
    let temp = parent.join(format!(
        ".{}.agentbro-{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("plugins"),
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        write_private_file(&temp, content)?;
        fs::rename(&temp, path)
            .map_err(|error| format!("Failed to replace {}: {error}", path.display()))?;
        let written = fs::read(path)
            .map_err(|error| format!("Failed to verify {}: {error}", path.display()))?;
        validate(&written)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        if let Some(bytes) = original {
            let _ = write_private_file(path, &bytes);
        } else {
            let _ = fs::remove_file(path);
        }
        return Err(error);
    }
    Ok(())
}

fn write_backup(path: &Path, content: &[u8], agent_id: &str) -> Result<(), String> {
    let dir = super::v2::fsutil::agentbro_home()
        .join("plugins")
        .join("backups")
        .join(agent_id);
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Failed to create plugin backup directory: {error}"))?;
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("plugin-config");
    write_private_file(&dir.join(format!("{timestamp}-{filename}")), content)
}

fn write_private_file(path: &Path, content: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
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

    #[test]
    fn updates_supported_json_shapes() {
        let mut claude = serde_json::json!({"enabledPlugins": {"reviewer": true}});
        set_json_plugin_enabled(&mut claude, PluginConfigKind::Json, "reviewer", false).unwrap();
        assert_eq!(claude["enabledPlugins"]["reviewer"], false);

        let mut zcode =
            serde_json::json!({"plugins": {"enabledPlugins": {"reviewer@official": true}}});
        set_json_plugin_enabled(
            &mut zcode,
            PluginConfigKind::Zcode,
            "reviewer@official",
            false,
        )
        .unwrap();
        assert_eq!(
            zcode["plugins"]["enabledPlugins"]["reviewer@official"],
            false
        );

        let mut kimi = serde_json::json!({"plugins": [{"id": "reviewer", "enabled": true}]});
        set_json_plugin_enabled(&mut kimi, PluginConfigKind::Kimi, "reviewer", false).unwrap();
        assert_eq!(kimi["plugins"][0]["enabled"], false);
    }

    #[test]
    fn revision_detects_config_changes() {
        let dir = std::env::temp_dir().join(format!(
            "agentbro-plugin-revision-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, "{}").unwrap();
        let revision = revision_for("claude-code", Some(&path)).unwrap();
        fs::write(&path, r#"{"enabledPlugins":{}}"#).unwrap();
        assert!(ensure_revision("claude-code", &path, &revision).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_tree_is_relative_and_does_not_follow_symlinks() {
        let dir = std::env::temp_dir().join(format!(
            "agentbro-plugin-tree-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(dir.join("skills/reviewer")).unwrap();
        fs::write(dir.join("skills/reviewer/SKILL.md"), "test").unwrap();
        fs::create_dir_all(dir.join("node_modules/package")).unwrap();
        fs::write(dir.join("node_modules/package/index.js"), "test").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&dir, dir.join("recursive-link")).unwrap();

        let mut count = 0;
        let mut truncated = false;
        let tree = build_file_tree(&dir, &dir, 0, &mut count, &mut truncated);
        assert_eq!(tree.path, ".");
        assert!(count >= 3);
        assert!(!truncated);
        let dependencies = tree
            .children
            .as_ref()
            .unwrap()
            .iter()
            .find(|node| node.name == "node_modules")
            .unwrap();
        assert_eq!(dependencies.omitted_count, Some(1));
        assert!(dependencies.children.as_ref().unwrap().is_empty());
        #[cfg(unix)]
        assert_eq!(
            tree.children
                .as_ref()
                .unwrap()
                .iter()
                .find(|node| node.name == "recursive-link")
                .unwrap()
                .node_type,
            "symlink"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_preview_reads_text_and_images_without_escaping_plugin_root() {
        let dir = std::env::temp_dir().join(format!(
            "agentbro-plugin-preview-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(dir.join("docs")).unwrap();
        fs::write(dir.join("docs/README.md"), "# Plugin\n安全预览").unwrap();
        fs::write(dir.join("preview.png"), [0x89, b'P', b'N', b'G']).unwrap();
        fs::write(dir.join("binary.bin"), [0, 1, 2, 3]).unwrap();

        let text = read_plugin_file_at(&dir, "docs/README.md").unwrap();
        assert_eq!(text.kind, "text");
        assert_eq!(text.content.as_deref(), Some("# Plugin\n安全预览"));
        assert!(!text.truncated);

        let image = read_plugin_file_at(&dir, "preview.png").unwrap();
        assert_eq!(image.kind, "image");
        assert_eq!(image.mime_type.as_deref(), Some("image/png"));
        assert!(image.data_base64.is_some());

        let binary = read_plugin_file_at(&dir, "binary.bin").unwrap();
        assert_eq!(binary.kind, "binary");
        assert!(binary.content.is_none());

        assert!(read_plugin_file_at(&dir, "../outside.txt").is_err());
        assert!(read_plugin_file_at(&dir, "/tmp/outside.txt").is_err());

        #[cfg(unix)]
        {
            let outside = dir.with_extension("outside.txt");
            fs::write(&outside, "outside").unwrap();
            std::os::unix::fs::symlink(&outside, dir.join("outside-link")).unwrap();
            assert!(read_plugin_file_at(&dir, "outside-link").is_err());
            fs::remove_file(outside).unwrap();
        }

        fs::remove_dir_all(dir).unwrap();
    }
}
