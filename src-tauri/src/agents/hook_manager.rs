// hook_manager.rs — Shared hook install/uninstall logic for JSON, YAML, and TOML configs
// NEVER corrupts user's existing config: all operations are atomic read-modify-write.

use std::path::{Path, PathBuf};

const AGENTBRO_MARKER: &str = "agentbro";
const AGENTBRO_BRIDGE_MARKER: &str = "agentbro-bridge";
const BLOCK_START: &str = "# [AGENTBRO-START]";
const BLOCK_END: &str = "# [AGENTBRO-END]";

// ── JSON ─────────────────────────────────────────────────────────────────────

/// Read a JSON config file, returning an empty object on parse failure.
/// Never panics; malformed files are treated as empty rather than corrupted.
pub fn read_json_config(path: &Path) -> serde_json::Value {
    if !path.exists() {
        return serde_json::json!({});
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Write a JSON value to a file atomically (write to tmp, then rename).
pub fn write_json_config(
    path: &Path,
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(value)?;
    atomic_write(path, content.as_bytes())?;
    Ok(())
}

/// Inject agentbro hook entries into a JSON "hooks" object.
/// Keys are event names; each value is an array of hook entries.
/// Existing non-agentbro entries are preserved.
pub fn inject_hooks_json(settings: &mut serde_json::Value, events: &[&str], hook_command: &str) {
    if settings.get("hooks").is_none() {
        settings["hooks"] = serde_json::json!({});
    }
    let hooks = settings["hooks"].as_object_mut().unwrap();
    for event in events {
        let entry = hooks
            .entry(event.to_string())
            .or_insert_with(|| serde_json::json!([]));
        if let Some(arr) = entry.as_array_mut() {
            // Remove stale agentbro entries
            arr.retain(|e| {
                !e.get("command")
                    .and_then(|c| c.as_str())
                    .map(|c| c.contains(AGENTBRO_MARKER))
                    .unwrap_or(false)
            });
            arr.push(serde_json::json!({"type": "command", "command": hook_command}));
        }
    }
}

/// Remove all agentbro hook entries from a JSON config.
pub fn remove_hooks_json(settings: &mut serde_json::Value) {
    if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for (_, v) in hooks.iter_mut() {
            if let Some(arr) = v.as_array_mut() {
                arr.retain(|e| {
                    !e.get("command")
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains(AGENTBRO_MARKER))
                        .unwrap_or(false)
                });
            }
        }
    }
}

// ── YAML ─────────────────────────────────────────────────────────────────────

/// Inject agentbro hooks into a YAML config using sentinel block markers.
/// The user's existing YAML content is preserved outside the sentinel block.
pub fn inject_hooks_yaml(
    config_path: &Path,
    hook_command: &str,
    events: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let existing = if config_path.exists() {
        std::fs::read_to_string(config_path)?
    } else {
        String::new()
    };

    let stripped = strip_sentinel_block(&existing);
    let block = build_yaml_block(hook_command, events);
    let new_content = if stripped.trim().is_empty() {
        block
    } else {
        format!("{}\n{}", stripped.trim_end(), block)
    };

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(config_path, new_content.as_bytes())?;
    Ok(())
}

/// Remove the agentbro sentinel block from a YAML config.
pub fn remove_hooks_yaml(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !config_path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(config_path)?;
    if !content.contains(AGENTBRO_MARKER) {
        return Ok(());
    }
    let stripped = strip_sentinel_block(&content);
    atomic_write(config_path, stripped.as_bytes())?;
    Ok(())
}

fn build_yaml_block(hook_command: &str, events: &[&str]) -> String {
    let cmd = hook_command.replace('"', "\\\"");
    let mut lines = vec![BLOCK_START.to_string(), "hooks:".to_string()];
    for event in events {
        lines.push(format!("  {}:", event));
        lines.push(format!("    - command: \"{}\"", cmd));
    }
    lines.push(BLOCK_END.to_string());
    lines.join("\n")
}

// ── TOML ─────────────────────────────────────────────────────────────────────

/// Inject agentbro hooks into a TOML config using sentinel block markers.
pub fn inject_hooks_toml(
    config_path: &Path,
    hook_command: &str,
    events: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let existing = if config_path.exists() {
        std::fs::read_to_string(config_path)?
    } else {
        String::new()
    };

    let stripped = strip_sentinel_block(&existing);
    let block = build_toml_block(hook_command, events);
    let new_content = if stripped.trim().is_empty() {
        block
    } else {
        format!("{}\n{}", stripped.trim_end(), block)
    };

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(config_path, new_content.as_bytes())?;
    Ok(())
}

/// Remove the agentbro sentinel block from a TOML config.
pub fn remove_hooks_toml(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !config_path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(config_path)?;
    if !content.contains(AGENTBRO_MARKER) {
        return Ok(());
    }
    let stripped = strip_sentinel_block(&content);
    atomic_write(config_path, stripped.as_bytes())?;
    Ok(())
}

fn build_toml_block(hook_command: &str, events: &[&str]) -> String {
    let cmd = hook_command.replace('"', "\\\"");
    let mut lines = vec![BLOCK_START.to_string()];
    for event in events {
        lines.push("[[hooks]]".to_string());
        lines.push(format!("event = \"{}\"", event));
        lines.push(format!("command = \"{}\"", cmd));
        lines.push(String::new());
    }
    lines.push(BLOCK_END.to_string());
    lines.join("\n")
}

// ── Shared utilities ──────────────────────────────────────────────────────────

/// Remove the sentinel block from any text format (YAML, TOML, etc.)
fn strip_sentinel_block(content: &str) -> String {
    let mut result = String::new();
    let mut inside = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == BLOCK_START {
            inside = true;
            continue;
        }
        if trimmed == BLOCK_END {
            inside = false;
            continue;
        }
        if !inside {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

/// Write bytes to a file atomically: write to a `.tmp` sibling, then rename.
fn atomic_write(path: &Path, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, data)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Check whether a config file (any format) already contains our hooks.
pub fn has_agentbro_hooks(path: &Path) -> bool {
    if path.is_dir() {
        for name in ["plugin.yaml", "__init__.py"] {
            let candidate = path.join(name);
            if std::fs::read_to_string(candidate)
                .map(|s| s.contains(AGENTBRO_BRIDGE_MARKER) || s.contains(BLOCK_START))
                .unwrap_or(false)
            {
                return true;
            }
        }
        return false;
    }

    std::fs::read_to_string(path)
        .map(|s| s.contains(AGENTBRO_BRIDGE_MARKER) || s.contains(BLOCK_START))
        .unwrap_or(false)
}

/// Return the bridge binary path for use in hook commands.
pub fn bridge_binary_path() -> PathBuf {
    ensure_bridge_binary().unwrap_or_else(|_| raw_bridge_binary_path())
}

pub fn bridge_binary_is_current() -> bool {
    let dest = raw_bridge_binary_path();
    bridge_binary_is_current_at(&dest, find_source_bridge().as_deref())
}

fn bridge_binary_is_current_at(dest: &Path, source: Option<&Path>) -> bool {
    if !dest.exists() {
        return false;
    }

    let Some(source) = source else {
        return bridge_candidate_can_run(dest);
    };

    match (std::fs::read(dest), std::fs::read(source)) {
        (Ok(dest_bytes), Ok(source_bytes)) => dest_bytes == source_bytes,
        _ => false,
    }
}

fn raw_bridge_binary_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    home.join(".agentbro")
        .join("bin")
        .join(bridge_binary_name())
}

fn bridge_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "agentbro-bridge.exe"
    } else {
        "agentbro-bridge"
    }
}

pub fn endpoint_env_assignments() -> Vec<String> {
    if cfg!(target_os = "windows") {
        return Vec::new();
    }
    let endpoint = crate::hook_endpoint::current();
    vec![
        format!(
            "{}={}",
            crate::hook_endpoint::HOOK_SOCKET_ENV,
            shell_quote(&endpoint.socket_path)
        ),
        format!(
            "{}={}",
            crate::hook_endpoint::HOOK_PORT_ENV,
            endpoint.tcp_port
        ),
    ]
}

pub fn bridge_command_parts(bridge: &Path, args: &[String]) -> Vec<String> {
    let mut parts = if cfg!(target_os = "windows") {
        vec![command_quote(&bridge.display().to_string())]
    } else {
        let mut parts = vec!["/usr/bin/env".to_string()];
        parts.extend(endpoint_env_assignments());
        parts.push(command_quote(&bridge.display().to_string()));
        parts
    };
    parts.extend(args.iter().map(|arg| command_quote(arg)));
    parts
}

/// Ensure the bridge binary is deployed to ~/.agentbro/bin.
pub fn ensure_bridge_binary() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dest = raw_bridge_binary_path();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if let Some(source) = find_source_bridge() {
        let should_copy = match (std::fs::read(&dest), std::fs::read(&source)) {
            (Ok(dest_bytes), Ok(source_bytes)) => dest_bytes != source_bytes,
            _ => true,
        };
        if should_copy {
            std::fs::copy(source, &dest)?;
        }
    } else if !dest.exists() {
        let searched = bridge_source_candidates()
            .into_iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("Bridge binary not found. Searched: {searched}").into());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
    }

    Ok(dest)
}

fn find_source_bridge() -> Option<PathBuf> {
    bridge_source_candidates()
        .into_iter()
        .find(|bridge| bridge_candidate_is_usable(bridge))
}

fn bridge_candidate_is_usable(bridge: &Path) -> bool {
    bridge.exists() && bridge_candidate_is_fresh(bridge) && bridge_candidate_can_run(bridge)
}

fn bridge_candidate_is_fresh(bridge: &Path) -> bool {
    #[cfg(not(debug_assertions))]
    let _ = bridge;

    #[cfg(debug_assertions)]
    {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if bridge.starts_with(manifest_dir.join("target")) {
            return bridge_is_newer_than_source(bridge, &manifest_dir.join("src/bridge/main.rs"));
        }
    }
    true
}

fn bridge_candidate_can_run(bridge: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        return std::process::Command::new(bridge)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = bridge;
        true
    }
}

#[cfg(debug_assertions)]
fn bridge_is_newer_than_source(bridge: &Path, source: &Path) -> bool {
    let Ok(bridge_modified) = std::fs::metadata(bridge).and_then(|meta| meta.modified()) else {
        return false;
    };
    let Ok(source_modified) = std::fs::metadata(source).and_then(|meta| meta.modified()) else {
        return true;
    };
    bridge_modified >= source_modified
}

fn bridge_source_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join(bridge_binary_name()));
            candidates.push(exe_dir.join("agentbro-bridge"));
            if let Some(contents_dir) = exe_dir.parent() {
                candidates.push(contents_dir.join("Resources").join(bridge_binary_name()));
                candidates.push(contents_dir.join("Resources").join("agentbro-bridge"));
                if let Some(app_dir) = contents_dir.parent() {
                    candidates.push(
                        app_dir
                            .join("Contents")
                            .join("Resources")
                            .join(bridge_binary_name()),
                    );
                    candidates.push(
                        app_dir
                            .join("Contents")
                            .join("Resources")
                            .join("agentbro-bridge"),
                    );
                }
            }
        }
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("target/debug/{}", bridge_binary_name())),
    );
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/agentbro-bridge"));
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("target/release/{}", bridge_binary_name())),
    );
    candidates
        .push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/agentbro-bridge"));
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/agentbro-bridge-resource/agentbro-bridge"),
    );
    candidates
}

fn command_quote(value: &str) -> String {
    if cfg!(target_os = "windows") {
        windows_quote(value)
    } else {
        shell_quote(value)
    }
}

fn windows_quote(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_string();
    }
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '\\' | '.' | '_' | '-' | ':' | '='))
    {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('"', "\\\""))
}

pub fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':' | '='))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn debug_bridge_freshness_rejects_older_bridge_binary() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "agentbro-bridge-freshness-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let bridge = dir.join("agentbro-bridge");
        let source = dir.join("main.rs");
        std::fs::write(&bridge, b"old bridge").unwrap();
        std::thread::sleep(Duration::from_millis(10));
        std::fs::write(&source, b"new source").unwrap();

        #[cfg(debug_assertions)]
        assert!(!bridge_is_newer_than_source(&bridge, &source));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn current_bridge_accepts_runnable_dest_when_source_is_unavailable() {
        let dir = std::env::temp_dir().join(format!(
            "agentbro-bridge-current-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        #[cfg(target_os = "windows")]
        let bridge = {
            let path = dir.join("agentbro-bridge.cmd");
            std::fs::write(&path, "@echo off\r\nexit /b 0\r\n").expect("write bridge script");
            path
        };
        #[cfg(not(target_os = "windows"))]
        let bridge = {
            let path = dir.join("agentbro-bridge");
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("write bridge script");
            path
        };

        assert!(bridge_binary_is_current_at(&bridge, None));

        let _ = std::fs::remove_dir_all(dir);
    }
}
