use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

fn themes_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".config").join("agentbro").join("themes")
}

fn codex_pets_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".codex").join("pets")
}

pub fn scan_themes() -> Vec<serde_json::Value> {
    let dir = themes_dir();

    let mut themes = Vec::new();
    if dir.exists() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let theme_json = path.join("theme.json");
                if theme_json.exists() {
                    if let Ok(content) = fs::read_to_string(&theme_json) {
                        if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(obj) = val.as_object_mut() {
                                obj.insert(
                                    "_dir".to_string(),
                                    serde_json::json!(path.to_string_lossy()),
                                );
                            }
                            themes.push(val);
                        }
                    }
                }
            }
        }
    }

    themes.extend(discover_codex_pet_themes(&codex_pets_dir()));
    themes
}

pub fn get_theme_bundle(name: &str) -> Option<serde_json::Value> {
    if let Some(pet_id) = name.strip_prefix("codex-pet:") {
        let dir = codex_pets_dir().join(pet_id);
        return codex_pet_theme_from_dir(&dir);
    }

    let dir = themes_dir().join(name);
    let theme_json = dir.join("theme.json");
    if !theme_json.exists() {
        return None;
    }

    let content = fs::read_to_string(&theme_json).ok()?;
    let mut val: serde_json::Value = serde_json::from_str(&content).ok()?;

    if let Some(obj) = val.as_object_mut() {
        if let Some(character) = obj.get("character").cloned() {
            if let Some(sprite_path) = character.get("spriteSheet").and_then(|s| s.as_str()) {
                let full_path = dir.join(sprite_path);
                if full_path.exists() {
                    if let Ok(bytes) = fs::read(&full_path) {
                        let b64 = STANDARD.encode(&bytes);
                        let mime = image_mime_for_path(&full_path);
                        let data_url = format!("data:{};base64,{}", mime, b64);
                        if let Some(char_obj) =
                            obj.get_mut("character").and_then(|c| c.as_object_mut())
                        {
                            char_obj.insert(
                                "spriteSheetDataUrl".to_string(),
                                serde_json::json!(data_url),
                            );
                        }
                    }
                }
            }
        }
    }

    Some(val)
}

fn discover_codex_pet_themes(pets_dir: &Path) -> Vec<serde_json::Value> {
    let mut themes = Vec::new();
    if !pets_dir.exists() {
        return themes;
    }

    if let Ok(entries) = fs::read_dir(pets_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if let Some(theme) = codex_pet_theme_from_dir(&path) {
                themes.push(theme);
            }
        }
    }
    themes
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexPetJson {
    id: Option<String>,
    display_name: Option<String>,
    description: Option<String>,
    spritesheet_path: Option<String>,
}

fn codex_pet_theme_from_dir(dir: &Path) -> Option<serde_json::Value> {
    let pet_json_path = dir.join("pet.json");
    if !pet_json_path.exists() {
        return None;
    }

    let content = fs::read_to_string(&pet_json_path).ok()?;
    let pet: CodexPetJson = serde_json::from_str(&content).ok()?;
    let fallback_id = dir.file_name()?.to_string_lossy().to_string();
    let id = safe_theme_id(pet.id.as_deref().unwrap_or(&fallback_id));
    let display_name = pet.display_name.unwrap_or_else(|| id.clone());
    let description = pet
        .description
        .unwrap_or_else(|| "Codex digital pet".to_string());
    let sprite_rel = pet
        .spritesheet_path
        .unwrap_or_else(|| "spritesheet.webp".to_string());
    let sprite_path = dir.join(&sprite_rel);
    let bytes = fs::read(&sprite_path).ok()?;
    let (atlas_width, atlas_height) =
        image_dimensions(&bytes, &sprite_path).unwrap_or((1536, 1872));
    let frame_width = if atlas_width >= 8 {
        atlas_width / 8
    } else {
        192
    };
    let frame_height = if atlas_height >= 9 {
        atlas_height / 9
    } else {
        208
    };
    let data_url = format!(
        "data:{};base64,{}",
        image_mime_for_path(&sprite_path),
        STANDARD.encode(&bytes)
    );

    Some(serde_json::json!({
        "name": format!("codex-pet:{}", id),
        "version": "1.0.0",
        "author": "user",
        "provider": "codex",
        "isCodexPet": true,
        "displayName": display_name,
        "description": description,
        "_dir": dir.to_string_lossy(),
        "pixelGrid": { "cols": 5, "rows": 5 },
        "priorityColors": default_priority_colors(),
        "prioritySpeeds": default_priority_speeds(),
        "priorityPatterns": default_priority_patterns(),
        "character": {
            "spriteSheet": data_url,
            "spriteSheetDataUrl": data_url,
            "frameSize": { "width": frame_width, "height": frame_height },
            "scale": 1,
            "animations": {
                "idle": { "row": 0, "frames": 6, "fps": 6 },
                "runningRight": { "row": 1, "frames": 8, "fps": 8 },
                "runningLeft": { "row": 2, "frames": 8, "fps": 8 },
                "waving": { "row": 3, "frames": 4, "fps": 6 },
                "jumping": { "row": 4, "frames": 5, "fps": 7 },
                "failed": { "row": 5, "frames": 8, "fps": 7 },
                "waiting": { "row": 6, "frames": 6, "fps": 5 },
                "running": { "row": 7, "frames": 6, "fps": 8 },
                "review": { "row": 8, "frames": 6, "fps": 6 }
            }
        },
        "stateMapping": {
            "dormant": "idle",
            "idle": "idle",
            "done": "waving",
            "thinking": "review",
            "working": "running",
            "compacting": "waiting",
            "attention": "failed"
        },
        "sounds": { "pack": "8bit" },
        "compactHeight": 24,
        "pixelCursor": { "enabled": true, "color": "#ffd166" }
    }))
}

fn safe_theme_id(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "pet".to_string()
    } else {
        trimmed
    }
}

fn image_mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
    {
        Some(ext) if ext == "webp" => "image/webp",
        Some(ext) if ext == "jpg" || ext == "jpeg" => "image/jpeg",
        Some(ext) if ext == "gif" => "image/gif",
        _ => "image/png",
    }
}

fn image_dimensions(bytes: &[u8], path: &Path) -> Option<(u32, u32)> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
    {
        Some(ext) if ext == "webp" => webp_dimensions(bytes),
        Some(ext) if ext == "png" => png_dimensions(bytes),
        _ => None,
    }
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    Some((
        u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        u32::from_be_bytes(bytes[20..24].try_into().ok()?),
    ))
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 20 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }

    let mut offset = 12usize;
    while offset + 8 <= bytes.len() {
        let chunk = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        let data_start = offset + 8;
        let data_end = data_start.checked_add(size)?;
        if data_end > bytes.len() {
            return None;
        }
        let data = &bytes[data_start..data_end];

        match chunk {
            b"VP8X" if data.len() >= 10 => {
                let width = 1 + read_24_le(&data[4..7]);
                let height = 1 + read_24_le(&data[7..10]);
                return Some((width, height));
            }
            b"VP8L" if data.len() >= 5 && data[0] == 0x2f => {
                let bits = u32::from_le_bytes(data[1..5].try_into().ok()?);
                let width = 1 + (bits & 0x3fff);
                let height = 1 + ((bits >> 14) & 0x3fff);
                return Some((width, height));
            }
            b"VP8 " if data.len() >= 10 && data[3..6] == [0x9d, 0x01, 0x2a] => {
                let width = u16::from_le_bytes(data[6..8].try_into().ok()?) as u32 & 0x3fff;
                let height = u16::from_le_bytes(data[8..10].try_into().ok()?) as u32 & 0x3fff;
                return Some((width, height));
            }
            _ => {}
        }

        offset = data_end + (size % 2);
    }

    None
}

fn read_24_le(bytes: &[u8]) -> u32 {
    (bytes[0] as u32) | ((bytes[1] as u32) << 8) | ((bytes[2] as u32) << 16)
}

fn default_priority_colors() -> serde_json::Value {
    serde_json::json!({
        "dormant": "#666666",
        "idle": "#30D158",
        "done": "#30D158",
        "thinking": "#007AFF",
        "working": "#FF9500",
        "compacting": "#9C27B0",
        "attention": "#FF3B30"
    })
}

fn default_priority_speeds() -> serde_json::Value {
    serde_json::json!({
        "dormant": 0,
        "idle": 2000,
        "done": 1500,
        "thinking": 800,
        "working": 600,
        "compacting": 500,
        "attention": 300
    })
}

fn default_priority_patterns() -> serde_json::Value {
    serde_json::json!({
        "dormant": { "activePixels": [{ "row": 2, "col": 2 }], "animation": "pulse", "fps": 1 },
        "idle": { "activePixels": [{ "row": 1, "col": 1 }, { "row": 1, "col": 3 }, { "row": 3, "col": 2 }], "animation": "breath", "fps": 2 },
        "done": { "activePixels": [{ "row": 1, "col": 1 }, { "row": 2, "col": 2 }, { "row": 3, "col": 3 }], "animation": "wave", "fps": 3 },
        "thinking": { "activePixels": [{ "row": 0, "col": 2 }, { "row": 2, "col": 0 }, { "row": 2, "col": 4 }, { "row": 4, "col": 2 }], "animation": "spin", "fps": 4 },
        "working": { "activePixels": [{ "row": 1, "col": 1 }, { "row": 1, "col": 3 }, { "row": 3, "col": 1 }, { "row": 3, "col": 3 }], "animation": "wave", "fps": 5 },
        "compacting": { "activePixels": [{ "row": 0, "col": 0 }, { "row": 0, "col": 4 }, { "row": 4, "col": 0 }, { "row": 4, "col": 4 }, { "row": 2, "col": 2 }], "animation": "spin", "fps": 6 },
        "attention": { "activePixels": [{ "row": 0, "col": 2 }, { "row": 1, "col": 1 }, { "row": 1, "col": 3 }, { "row": 2, "col": 0 }, { "row": 2, "col": 4 }, { "row": 3, "col": 1 }, { "row": 3, "col": 3 }, { "row": 4, "col": 2 }], "animation": "blink", "fps": 8 }
    })
}

pub fn import_theme_from_path(src: &Path) -> Result<String, String> {
    if !src.is_dir() {
        return Err("Source path is not a directory".to_string());
    }

    let theme_json = src.join("theme.json");
    if !theme_json.exists() {
        return Err("No theme.json found in source directory".to_string());
    }

    let content = fs::read_to_string(&theme_json).map_err(|e| e.to_string())?;
    let val: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let name = val
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("imported");

    let dest = themes_dir().join(name);
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    copy_dir_all(src, &dest).map_err(|e| e.to_string())?;

    Ok(name.to_string())
}

pub fn seed_builtin_themes(resources_dir: &Path) {
    let builtin_dir = resources_dir.join("themes");
    if !builtin_dir.exists() {
        return;
    }

    let dest_base = themes_dir();
    fs::create_dir_all(&dest_base).ok();

    if let Ok(entries) = fs::read_dir(&builtin_dir) {
        for entry in entries.flatten() {
            let src = entry.path();
            if !src.is_dir() {
                continue;
            }

            let src_theme_json = src.join("theme.json");
            if !src_theme_json.exists() {
                continue;
            }

            let name = src.file_name().unwrap().to_string_lossy().to_string();
            let dest = dest_base.join(&name);

            let should_copy = if dest.join("theme.json").exists() {
                let src_content = fs::read_to_string(&src_theme_json).unwrap_or_default();
                let dest_content = fs::read_to_string(dest.join("theme.json")).unwrap_or_default();
                let src_ver = serde_json::from_str::<serde_json::Value>(&src_content)
                    .ok()
                    .and_then(|v| v.get("version").and_then(|x| x.as_str()).map(String::from))
                    .unwrap_or_default();
                let dest_ver = serde_json::from_str::<serde_json::Value>(&dest_content)
                    .ok()
                    .and_then(|v| v.get("version").and_then(|x| x.as_str()).map(String::from))
                    .unwrap_or_default();
                let dest_author = serde_json::from_str::<serde_json::Value>(&dest_content)
                    .ok()
                    .and_then(|v| v.get("author").and_then(|x| x.as_str()).map(String::from))
                    .unwrap_or_default();
                dest_author != "user" && src_ver > dest_ver
            } else {
                true
            };

            if should_copy {
                fs::create_dir_all(&dest).ok();
                copy_dir_all(&src, &dest).ok();
            }
        }
    }
}

fn copy_dir_all(src: &Path, dest: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_webp_vp8x(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&18u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8X");
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        let width_minus_one = width - 1;
        let height_minus_one = height - 1;
        bytes.extend_from_slice(&[
            (width_minus_one & 0xff) as u8,
            ((width_minus_one >> 8) & 0xff) as u8,
            ((width_minus_one >> 16) & 0xff) as u8,
            (height_minus_one & 0xff) as u8,
            ((height_minus_one >> 8) & 0xff) as u8,
            ((height_minus_one >> 16) & 0xff) as u8,
        ]);
        bytes
    }

    #[test]
    fn parses_webp_vp8x_dimensions() {
        let bytes = fixture_webp_vp8x(1536, 1872);
        assert_eq!(webp_dimensions(&bytes), Some((1536, 1872)));
    }

    #[test]
    fn converts_codex_pet_to_theme() {
        let dir = std::env::temp_dir().join(format!("agentbro-pet-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("pet.json"),
            r#"{
              "id": "Nami Pet",
              "displayName": "Nami",
              "description": "A navigator pet.",
              "spritesheetPath": "spritesheet.webp"
            }"#,
        )
        .unwrap();
        fs::write(dir.join("spritesheet.webp"), fixture_webp_vp8x(1536, 1872)).unwrap();

        let theme = codex_pet_theme_from_dir(&dir).unwrap();
        assert_eq!(theme["name"], "codex-pet:nami-pet");
        assert_eq!(theme["provider"], "codex");
        assert_eq!(theme["displayName"], "Nami");
        assert_eq!(theme["character"]["frameSize"]["width"], 192);
        assert_eq!(theme["character"]["frameSize"]["height"], 208);
        assert!(theme["character"]["spriteSheet"]
            .as_str()
            .unwrap()
            .starts_with("data:image/webp;base64,"));

        fs::remove_dir_all(&dir).ok();
    }
}
