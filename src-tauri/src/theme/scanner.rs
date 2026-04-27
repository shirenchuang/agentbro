use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde_json::Value;

use super::themes_dir;

/// Scan the themes directory and return all valid theme configs.
/// If a theme has a character.spriteSheet field, the PNG is read and
/// converted to a base64 data URL inline.
pub fn scan_themes() -> Vec<Value> {
    let dir = themes_dir();
    if !dir.exists() {
        return Vec::new();
    }

    let mut themes = Vec::new();

    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("Failed to read themes directory: {}", e);
            return Vec::new();
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let theme_json = path.join("theme.json");
        if !theme_json.exists() {
            continue;
        }

        match load_theme(&theme_json, &path) {
            Ok(theme) => themes.push(theme),
            Err(e) => {
                log::warn!("Failed to load theme from {:?}: {}", path, e);
            }
        }
    }

    themes
}

/// Load a single theme from its directory, inlining sprite sheet as base64.
fn load_theme(theme_json: &Path, theme_dir: &Path) -> anyhow::Result<Value> {
    let content = fs::read_to_string(theme_json)?;
    let mut theme: Value = serde_json::from_str(&content)?;

    // If theme has character.spriteSheet, convert to base64 data URL
    if let Some(sprite_filename) = theme
        .get("character")
        .and_then(|c| c.get("spriteSheet"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
    {
        let sprite_path = theme_dir.join(&sprite_filename);
        if sprite_path.exists() {
            if let Ok(bytes) = fs::read(&sprite_path) {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                let ext = sprite_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("png");
                let data_url = format!("data:image/{};base64,{}", ext, b64);

                if let Some(character) = theme.get_mut("character") {
                    character["spriteSheet"] = Value::String(data_url);
                }
            }
        }
    }

    Ok(theme)
}

/// Ensure built-in themes exist in the user's themes directory.
/// Copies from bundled resources on first run.
pub fn ensure_builtin_themes(resource_dir: &Path) {
    let target = themes_dir();

    let builtin_themes = ["default", "minimal-dot"];

    for name in &builtin_themes {
        let target_dir = target.join(name);
        let target_json = target_dir.join("theme.json");

        // Only copy if the target doesn't already exist
        if target_json.exists() {
            continue;
        }

        let source_dir = resource_dir.join("themes").join(name);
        let source_json = source_dir.join("theme.json");

        if !source_json.exists() {
            log::debug!("Built-in theme source not found: {:?}", source_json);
            continue;
        }

        if let Err(e) = fs::create_dir_all(&target_dir) {
            log::warn!("Failed to create theme dir {:?}: {}", target_dir, e);
            continue;
        }

        if let Err(e) = copy_dir_contents(&source_dir, &target_dir) {
            log::warn!("Failed to copy theme {}: {}", name, e);
        } else {
            log::info!("Installed built-in theme: {}", name);
        }
    }
}

/// Copy all files from src directory to dst directory (non-recursive).
fn copy_dir_contents(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_file() {
            let dest_path = dst.join(entry.file_name());
            fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}

/// Get the path to a specific theme directory
pub fn theme_path(name: &str) -> PathBuf {
    themes_dir().join(name)
}
