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
    // Bail if the root isn't an object — a non-object config can't hold hooks,
    // and indexing it would panic.
    if settings.as_object_mut().is_none() {
        return;
    }
    // Replace `hooks` if it's missing OR a non-object (e.g. a user hand-edited
    // it to a string/null); only then is as_object_mut guaranteed to succeed.
    if !settings.get("hooks").is_some_and(|hooks| hooks.is_object()) {
        settings["hooks"] = serde_json::json!({});
    }
    let Some(hooks) = settings["hooks"].as_object_mut() else {
        return;
    };
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
    if !dest.exists() {
        return false;
    }

    let Some(source) = find_source_bridge() else {
        return true;
    };

    match (std::fs::read(dest), std::fs::read(source)) {
        (Ok(dest_bytes), Ok(source_bytes)) => dest_bytes == source_bytes,
        _ => false,
    }
}

fn raw_bridge_binary_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    home.join(".agentbro").join("bin").join("agentbro-bridge")
}

pub fn endpoint_env_assignments() -> Vec<String> {
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
    let mut parts = vec!["/usr/bin/env".to_string()];
    parts.extend(endpoint_env_assignments());
    parts.push(shell_quote(&bridge.display().to_string()));
    parts.extend(args.iter().map(|arg| shell_quote(arg)));
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
        .find(|bridge| bridge.exists())
}

fn bridge_source_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("agentbro-bridge"));
            if let Some(contents_dir) = exe_dir.parent() {
                candidates.push(contents_dir.join("Resources").join("agentbro-bridge"));
                if let Some(app_dir) = contents_dir.parent() {
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
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/agentbro-bridge"));
    candidates
        .push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/agentbro-bridge"));
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/agentbro-bridge-resource/agentbro-bridge"),
    );
    candidates
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

    #[test]
    fn inject_hooks_json_replaces_non_object_hooks_without_panicking() {
        // A user hand-edited "hooks" to a string — must not panic.
        let mut settings = serde_json::json!({ "hooks": "oops" });
        inject_hooks_json(&mut settings, &["PreToolUse"], "agentbro-bridge");
        assert!(settings["hooks"].is_object());
        assert_eq!(
            settings["hooks"]["PreToolUse"][0]["command"],
            "agentbro-bridge"
        );
    }

    #[test]
    fn inject_hooks_json_handles_null_hooks_and_missing_key() {
        for mut settings in [
            serde_json::json!({ "hooks": serde_json::Value::Null }),
            serde_json::json!({}),
        ] {
            inject_hooks_json(&mut settings, &["Stop"], "agentbro-bridge");
            assert_eq!(settings["hooks"]["Stop"][0]["command"], "agentbro-bridge");
        }
    }

    #[test]
    fn inject_hooks_json_ignores_non_object_root() {
        // Whole config isn't an object — bail out instead of panicking.
        let mut settings = serde_json::json!([1, 2, 3]);
        inject_hooks_json(&mut settings, &["Stop"], "agentbro-bridge");
        assert_eq!(settings, serde_json::json!([1, 2, 3]));
    }
}
