// ConfigStore — JSON-based persistent configuration
// Platform-appropriate config path with auto-save and Tauri event emission

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Emitter};

/// A custom Claude Code engine instance at a non-default path
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInstance {
    pub id: String,
    pub label: String,
    pub config_root: String,
    pub enabled: bool,
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub sound_enabled: bool,
    pub sound_volume: f32,
    pub auto_hide: bool,
    pub smart_suppression: bool,
    pub completion_timeout: u32,
    pub show_token_usage: bool,
    pub theme: String,
    /// Which display to position on: "primary" or a monitor name
    #[serde(default = "default_display_id")]
    pub display_id: String,
    /// Hide the notch (via opacity) when there are no active sessions
    #[serde(default)]
    pub auto_hide_no_sessions: bool,
    /// Hide the notch when the frontmost app is fullscreen
    #[serde(default = "default_true")]
    pub hide_in_fullscreen: bool,
    /// Per-event sound enable map (event name -> enabled)
    #[serde(default)]
    pub sound_events: std::collections::HashMap<String, bool>,
    /// Active sound pack
    #[serde(default = "default_sound_pack")]
    pub sound_pack: String,
    /// Filter out sounds for probe/health-check sessions
    #[serde(default)]
    pub probe_session_filter: bool,
    /// Minutes of inactivity before auto-hiding (0 = disabled)
    #[serde(default)]
    pub idle_timeout_minutes: u32,
    /// Notification mode: "turnEnd" or "every"
    #[serde(default = "default_notification_mode")]
    pub notification_mode: String,
    /// Custom engine instances (Claude Code at non-default paths)
    #[serde(default)]
    pub engine_instances: Vec<EngineInstance>,
}

fn default_display_id() -> String {
    "primary".to_string()
}

fn default_true() -> bool {
    true
}

fn default_sound_pack() -> String {
    "eight-bit".to_string()
}

fn default_notification_mode() -> String {
    "turnEnd".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            sound_enabled: true,
            sound_volume: 0.7,
            auto_hide: true,
            smart_suppression: true,
            completion_timeout: 5,
            show_token_usage: true,
            theme: "system".to_string(),
            display_id: "primary".to_string(),
            auto_hide_no_sessions: false,
            hide_in_fullscreen: true,
            sound_events: std::collections::HashMap::new(),
            sound_pack: "eight-bit".to_string(),
            probe_session_filter: false,
            idle_timeout_minutes: 0,
            notification_mode: "turnEnd".to_string(),
            engine_instances: Vec::new(),
        }
    }
}

/// Persistent configuration store
#[derive(Clone)]
pub struct ConfigStore {
    config: Arc<RwLock<AppConfig>>,
    config_path: PathBuf,
    app_handle: Option<AppHandle>,
}

impl ConfigStore {
    /// Create a new ConfigStore, loading from disk if available
    pub fn new() -> Self {
        let config_path = Self::config_file_path();
        let config = Self::load_from_disk(&config_path).unwrap_or_default();

        Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
            app_handle: None,
        }
    }

    /// Set the app handle for emitting Tauri events
    pub fn set_app_handle(&mut self, handle: AppHandle) {
        self.app_handle = Some(handle);
    }

    /// Get the platform-appropriate config file path
    fn config_file_path() -> PathBuf {
        let base = dirs::config_dir()
            .or_else(|| dirs::data_local_dir())
            .unwrap_or_else(|| std::env::temp_dir());
        base.join("agent-island").join("config.json")
    }

    /// Load config from disk
    fn load_from_disk(path: &PathBuf) -> Option<AppConfig> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save config to disk
    fn save_to_disk(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let config = self.config.read().map_err(|e| format!("Lock error: {}", e))?;
        let content = serde_json::to_string_pretty(&*config)?;
        std::fs::write(&self.config_path, content)?;
        Ok(())
    }

    /// Get the current config
    pub fn get(&self) -> AppConfig {
        self.config.read()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    /// Update the config, save to disk, and emit event
    pub fn update(&self, new_config: AppConfig) -> Result<(), String> {
        {
            let mut config = self.config.write().map_err(|e| format!("Lock error: {}", e))?;
            *config = new_config.clone();
        }

        if let Err(e) = self.save_to_disk() {
            log::error!("Failed to save config: {}", e);
            return Err(format!("Failed to save config: {}", e));
        }

        // Emit config change event to frontend
        if let Some(ref handle) = self.app_handle {
            if let Err(e) = handle.emit("config-changed", &new_config) {
                log::error!("Failed to emit config-changed: {}", e);
            }
        }

        log::info!("Config updated and saved");
        Ok(())
    }
}
