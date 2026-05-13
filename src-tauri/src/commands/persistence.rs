// Session persistence commands — save/load session state to JSON file

use std::fs;
use std::path::PathBuf;

const SESSIONS_FILE: &str = "sessions.json";
const APP_SUPPORT_DIR: &str = "agentbro";

fn get_sessions_path() -> Option<PathBuf> {
    dirs::data_dir()
        .or_else(dirs::data_local_dir) // fallback
        .map(|p| p.join(APP_SUPPORT_DIR).join(SESSIONS_FILE))
}

#[tauri::command]
pub fn save_sessions(sessions_json: String) -> Result<(), String> {
    let path = get_sessions_path().ok_or("Cannot get data directory")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, sessions_json).map_err(|e| e.to_string())?;
    log::info!("Sessions persisted to {:?}", path);
    Ok(())
}

#[tauri::command]
pub fn load_sessions() -> Result<String, String> {
    let path = get_sessions_path().ok_or("Cannot get data directory")?;
    if !path.exists() {
        return Ok("[]".to_string());
    }
    let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    log::info!("Sessions loaded from {:?}", path);
    Ok(data)
}
