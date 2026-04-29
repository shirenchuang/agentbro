// buddy.rs — Read Claude Buddy data from ~/.claude.json

use std::fs;
use std::path::PathBuf;

fn claude_json_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude.json"))
}

/// Read the buddy section from ~/.claude.json and return it as a JSON string.
/// Returns a default stub if the file doesn't exist or has no buddy section.
#[tauri::command]
pub fn read_buddy_data() -> Result<String, String> {
    let path = claude_json_path().ok_or("Cannot determine home directory")?;

    if !path.exists() {
        return Ok(default_buddy_json());
    }

    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    // Extract buddy sub-object if present
    if let Some(buddy) = parsed.get("buddy") {
        return serde_json::to_string(buddy).map_err(|e| e.to_string());
    }

    Ok(default_buddy_json())
}

fn default_buddy_json() -> String {
    r#"{
  "species": "cat",
  "name": "Claude",
  "level": 1,
  "xp": 0,
  "xpMax": 100,
  "happiness": 50,
  "energy": 50,
  "interactions": 0
}"#
    .to_string()
}
