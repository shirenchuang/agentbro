// hook_manager.rs — Shared hook install/uninstall logic for JSON, YAML, and TOML configs
// NEVER corrupts user's existing config: all operations are atomic read-modify-write.

use std::path::{Path, PathBuf};

const AGENT_ISLAND_MARKER: &str = "agent-island";
const BLOCK_START: &str = "# [AGENT-ISLAND-START]";
const BLOCK_END: &str = "# [AGENT-ISLAND-END]";

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
pub fn write_json_config(path: &Path, value: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(value)?;
    atomic_write(path, content.as_bytes())?;
    Ok(())
}

/// Inject agent-island hook entries into a JSON "hooks" object.
/// Keys are event names; each value is an array of hook entries.
/// Existing non-agent-island entries are preserved.
pub fn inject_hooks_json(
    settings: &mut serde_json::Value,
    events: &[&str],
    hook_command: &str,
) {
    if settings.get("hooks").is_none() {
        settings["hooks"] = serde_json::json!({});
    }
    let hooks = settings["hooks"].as_object_mut().unwrap();
    for event in events {
        let entry = hooks
            .entry(event.to_string())
            .or_insert_with(|| serde_json::json!([]));
        if let Some(arr) = entry.as_array_mut() {
            // Remove stale agent-island entries
            arr.retain(|e| {
                !e.get("command")
                    .and_then(|c| c.as_str())
                    .map(|c| c.contains(AGENT_ISLAND_MARKER))
                    .unwrap_or(false)
            });
            arr.push(serde_json::json!({"type": "command", "command": hook_command}));
        }
    }
}

/// Remove all agent-island hook entries from a JSON config.
pub fn remove_hooks_json(settings: &mut serde_json::Value) {
    if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for (_, v) in hooks.iter_mut() {
            if let Some(arr) = v.as_array_mut() {
                arr.retain(|e| {
                    !e.get("command")
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains(AGENT_ISLAND_MARKER))
                        .unwrap_or(false)
                });
            }
        }
    }
}

// ── YAML ─────────────────────────────────────────────────────────────────────

/// Inject agent-island hooks into a YAML config using sentinel block markers.
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

/// Remove the agent-island sentinel block from a YAML config.
pub fn remove_hooks_yaml(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !config_path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(config_path)?;
    if !content.contains(AGENT_ISLAND_MARKER) {
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

/// Inject agent-island hooks into a TOML config using sentinel block markers.
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

/// Remove the agent-island sentinel block from a TOML config.
pub fn remove_hooks_toml(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !config_path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(config_path)?;
    if !content.contains(AGENT_ISLAND_MARKER) {
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
        lines.push(format!("[[hooks]]"));
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
pub fn has_agent_island_hooks(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|s| s.contains(AGENT_ISLAND_MARKER))
        .unwrap_or(false)
}

/// Return the bridge binary path for use in hook commands.
pub fn bridge_binary_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::env::temp_dir());
    home.join(".agent-island").join("bin").join("agent-island-bridge")
}
