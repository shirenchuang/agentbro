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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundRuleConfig {
    pub enabled: bool,
    pub sound: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomSoundConfig {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowOrigin {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomHookInstall {
    pub id: String,
    pub profile_id: String,
    pub display_name: String,
    pub install_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuddyDeviceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_buddy_device_transport")]
    pub transport: String,
    #[serde(default = "default_buddy_device_port")]
    pub port: u16,
    #[serde(default)]
    pub shared_secret: String,
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub sound_enabled: bool,
    pub sound_volume: f32,
    #[serde(default)]
    pub launch_at_login: bool,
    pub auto_hide: bool,
    pub smart_suppression: bool,
    pub completion_timeout: u32,
    pub show_token_usage: bool,
    #[serde(default = "default_true")]
    pub usage_query_enabled: bool,
    pub theme: String,
    /// Which display to position on: "primary" or a monitor name
    #[serde(default = "default_display_id")]
    pub display_id: String,
    /// Hide the notch (via opacity) when there are no active sessions
    #[serde(default)]
    pub auto_hide_no_sessions: bool,
    /// Per-event sound enable map (event name -> enabled)
    #[serde(default)]
    pub sound_events: std::collections::HashMap<String, bool>,
    /// Per-event sound choice rules (event name -> enabled + sound choice)
    #[serde(default)]
    pub sound_rules: std::collections::HashMap<String, SoundRuleConfig>,
    /// Imported custom sound files
    #[serde(default)]
    pub custom_sounds: Vec<CustomSoundConfig>,
    /// Active sound pack
    #[serde(default = "default_sound_pack")]
    pub sound_pack: String,
    /// Migrate the legacy boot sound default to the built-in Hey Bro sound once.
    #[serde(default)]
    pub boot_sound_default_migrated: bool,
    /// Filter out sounds for probe/health-check sessions
    #[serde(default)]
    pub probe_session_filter: bool,
    /// Suppress sounds during a configured local time window
    #[serde(default)]
    pub quiet_hours_enabled: bool,
    #[serde(default = "default_quiet_hours_start")]
    pub quiet_hours_start: String,
    #[serde(default = "default_quiet_hours_end")]
    pub quiet_hours_end: String,
    /// Minutes of inactivity before auto-hiding (0 = disabled)
    #[serde(default = "default_idle_timeout_minutes")]
    pub idle_timeout_minutes: u32,
    /// Notification mode: "turnEnd" or "every"
    #[serde(default = "default_notification_mode")]
    pub notification_mode: String,
    /// Custom engine instances (Claude Code at non-default paths)
    #[serde(default)]
    pub engine_instances: Vec<EngineInstance>,
    /// Custom hook installations at non-standard paths
    #[serde(default)]
    pub custom_hook_installs: Vec<CustomHookInstall>,
    /// Webhook forwarding configurations (DingTalk / Feishu)
    #[serde(default)]
    pub webhook_configs: Vec<crate::webhook::WebhookConfig>,
    /// SSH remote host configurations
    #[serde(default)]
    pub remote_hosts: Vec<crate::remote::RemoteHost>,
    /// Sound volume (0-100)
    #[serde(default = "default_volume")]
    pub volume: u8,
    /// Custom hooks path override
    #[serde(default)]
    pub custom_hooks_path: String,
    /// External Buddy device bridge for Apple Watch / ESP32 companion devices.
    #[serde(default)]
    pub buddy_device: BuddyDeviceConfig,
    /// Show tips when all sessions are idle
    #[serde(default = "default_true")]
    pub tips_enabled: bool,
    /// Show animated pixel cursor border dots
    #[serde(default = "default_true")]
    pub pixel_cursor_enabled: bool,
    /// Show confetti on task completion
    #[serde(default = "default_true")]
    pub confetti_enabled: bool,
    /// Filter sessions by focused terminal window
    #[serde(default)]
    pub follow_focus: bool,
    /// Island surface mode: "island" or "pet"
    #[serde(default = "default_island_surface_mode")]
    pub island_surface_mode: String,
    /// Pet scale percentage
    #[serde(default = "default_island_pet_scale")]
    pub island_pet_scale: u32,
    /// Pet window origin
    #[serde(default)]
    pub island_pet_window_origin: Option<WindowOrigin>,
    /// Global keyboard shortcut to toggle island visibility
    #[serde(default = "default_global_shortcut")]
    pub global_shortcut: String,
    /// Global keyboard shortcut to approve current permission request
    #[serde(default = "default_shortcut_approve")]
    pub shortcut_approve: String,
    #[serde(default)]
    pub shortcut_approve_enabled: bool,
    /// Global keyboard shortcut to deny current permission request
    #[serde(default = "default_shortcut_deny")]
    pub shortcut_deny: String,
    #[serde(default)]
    pub shortcut_deny_enabled: bool,
    /// Global keyboard shortcut to skip current question by selecting the first option
    #[serde(default = "default_shortcut_skip")]
    pub shortcut_skip: String,
    #[serde(default)]
    pub shortcut_skip_enabled: bool,
    #[serde(default)]
    pub permission_shortcut_defaults_migrated: bool,
}

fn default_display_id() -> String {
    "primary".to_string()
}

fn default_true() -> bool {
    true
}

fn default_sound_pack() -> String {
    "synth".to_string()
}

fn default_notification_mode() -> String {
    "turnEnd".to_string()
}

fn default_volume() -> u8 {
    70
}

fn default_idle_timeout_minutes() -> u32 {
    5
}

fn default_quiet_hours_start() -> String {
    "22:00".to_string()
}

fn default_quiet_hours_end() -> String {
    "08:00".to_string()
}

fn default_global_shortcut() -> String {
    "CommandOrControl+Shift+I".to_string()
}

fn default_shortcut_approve() -> String {
    "CommandOrControl+Shift+A".to_string()
}

fn default_shortcut_deny() -> String {
    "CommandOrControl+Shift+D".to_string()
}

fn default_shortcut_skip() -> String {
    "CommandOrControl+Shift+S".to_string()
}

fn default_island_surface_mode() -> String {
    "island".to_string()
}

fn default_island_pet_scale() -> u32 {
    72
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            sound_enabled: true,
            sound_volume: 0.7,
            launch_at_login: false,
            auto_hide: true,
            smart_suppression: true,
            completion_timeout: 5,
            show_token_usage: true,
            usage_query_enabled: true,
            theme: "midnight".to_string(),
            display_id: "primary".to_string(),
            auto_hide_no_sessions: false,
            sound_events: std::collections::HashMap::new(),
            sound_rules: std::collections::HashMap::new(),
            custom_sounds: Vec::new(),
            sound_pack: "synth".to_string(),
            boot_sound_default_migrated: true,
            probe_session_filter: false,
            quiet_hours_enabled: false,
            quiet_hours_start: default_quiet_hours_start(),
            quiet_hours_end: default_quiet_hours_end(),
            idle_timeout_minutes: default_idle_timeout_minutes(),
            notification_mode: "turnEnd".to_string(),
            engine_instances: Vec::new(),
            custom_hook_installs: Vec::new(),
            webhook_configs: Vec::new(),
            remote_hosts: Vec::new(),
            volume: default_volume(),
            custom_hooks_path: String::new(),
            buddy_device: BuddyDeviceConfig::default(),
            tips_enabled: true,
            pixel_cursor_enabled: true,
            confetti_enabled: true,
            follow_focus: false,
            island_surface_mode: default_island_surface_mode(),
            island_pet_scale: default_island_pet_scale(),
            island_pet_window_origin: None,
            global_shortcut: "CommandOrControl+Shift+I".to_string(),
            shortcut_approve: default_shortcut_approve(),
            shortcut_approve_enabled: false,
            shortcut_deny: default_shortcut_deny(),
            shortcut_deny_enabled: false,
            shortcut_skip: default_shortcut_skip(),
            shortcut_skip_enabled: false,
            permission_shortcut_defaults_migrated: true,
        }
    }
}

impl AppConfig {
    fn migrate_permission_shortcut_defaults(&mut self) {
        if self.permission_shortcut_defaults_migrated {
            return;
        }

        if self.shortcut_approve_enabled && self.shortcut_approve == default_shortcut_approve() {
            self.shortcut_approve_enabled = false;
        }
        if self.shortcut_deny_enabled && self.shortcut_deny == default_shortcut_deny() {
            self.shortcut_deny_enabled = false;
        }
        self.permission_shortcut_defaults_migrated = true;
    }

    fn migrate_boot_sound_default(&mut self) {
        if self.boot_sound_default_migrated {
            return;
        }

        let use_hey_bro_default = self
            .sound_rules
            .get("boot")
            .map(|rule| rule.sound == "default")
            .unwrap_or(true);

        if use_hey_bro_default {
            let enabled = self.sound_events.get("boot").copied().unwrap_or(true);
            self.sound_events.insert("boot".to_string(), enabled);
            self.sound_rules.insert(
                "boot".to_string(),
                SoundRuleConfig {
                    enabled,
                    sound: "builtin:hey-bro".to_string(),
                },
            );
        }

        self.boot_sound_default_migrated = true;
    }
}

fn default_buddy_device_transport() -> String {
    "http".to_string()
}

fn default_buddy_device_port() -> u16 {
    17893
}

impl Default for BuddyDeviceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            transport: default_buddy_device_transport(),
            port: default_buddy_device_port(),
            shared_secret: String::new(),
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
            .or_else(dirs::data_local_dir)
            .unwrap_or_else(std::env::temp_dir);
        base.join("agentbro").join("config.json")
    }

    /// Load config from disk
    fn load_from_disk(path: &PathBuf) -> Option<AppConfig> {
        let content = std::fs::read_to_string(path).ok()?;
        let mut config: AppConfig = serde_json::from_str(&content).ok()?;
        config.migrate_permission_shortcut_defaults();
        config.migrate_boot_sound_default();
        Some(config)
    }

    /// Save config to disk
    fn save_to_disk(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let config = self
            .config
            .read()
            .map_err(|e| format!("Lock error: {}", e))?;
        let content = serde_json::to_string_pretty(&*config)?;
        std::fs::write(&self.config_path, content)?;
        Ok(())
    }

    /// Get the current config
    pub fn get(&self) -> AppConfig {
        self.config.read().map(|c| c.clone()).unwrap_or_default()
    }

    /// Update the config, save to disk, and emit event
    pub fn update(&self, new_config: AppConfig) -> Result<(), String> {
        {
            let mut config = self
                .config
                .write()
                .map_err(|e| format!("Lock error: {}", e))?;
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

impl Default for ConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;

    #[test]
    fn defaults_match_evolab_island_behavior() {
        let config = AppConfig::default();

        assert_eq!(config.completion_timeout, 5);
        assert_eq!(config.idle_timeout_minutes, 5);
        assert_eq!(config.sound_volume, 0.7);
        assert_eq!(config.volume, 70);
        assert!(!config.shortcut_approve_enabled);
        assert!(!config.shortcut_deny_enabled);
        assert!(config.permission_shortcut_defaults_migrated);
        assert!(config.boot_sound_default_migrated);
    }

    #[test]
    fn migrates_legacy_permission_shortcut_defaults_once() {
        let mut config = AppConfig {
            shortcut_approve_enabled: true,
            shortcut_deny_enabled: true,
            permission_shortcut_defaults_migrated: false,
            ..AppConfig::default()
        };

        config.migrate_permission_shortcut_defaults();

        assert!(!config.shortcut_approve_enabled);
        assert!(!config.shortcut_deny_enabled);
        assert!(config.permission_shortcut_defaults_migrated);
    }

    #[test]
    fn keeps_custom_permission_shortcuts_enabled_during_migration() {
        let mut config = AppConfig {
            shortcut_approve: "CommandOrControl+Shift+P".to_string(),
            shortcut_approve_enabled: true,
            shortcut_deny: "CommandOrControl+Shift+R".to_string(),
            shortcut_deny_enabled: true,
            permission_shortcut_defaults_migrated: false,
            ..AppConfig::default()
        };

        config.migrate_permission_shortcut_defaults();

        assert!(config.shortcut_approve_enabled);
        assert!(config.shortcut_deny_enabled);
        assert!(config.permission_shortcut_defaults_migrated);
    }

    #[test]
    fn migrates_legacy_boot_sound_default_to_hey_bro() {
        let mut config = AppConfig {
            boot_sound_default_migrated: false,
            ..AppConfig::default()
        };
        config.sound_events.insert("boot".to_string(), true);
        config.sound_rules.insert(
            "boot".to_string(),
            super::SoundRuleConfig {
                enabled: true,
                sound: "default".to_string(),
            },
        );

        config.migrate_boot_sound_default();

        assert_eq!(
            config
                .sound_rules
                .get("boot")
                .map(|rule| rule.sound.as_str()),
            Some("builtin:hey-bro")
        );
        assert!(config.boot_sound_default_migrated);
    }

    #[test]
    fn preserves_custom_boot_sound_during_migration() {
        let mut config = AppConfig {
            boot_sound_default_migrated: false,
            ..AppConfig::default()
        };
        config.sound_rules.insert(
            "boot".to_string(),
            super::SoundRuleConfig {
                enabled: true,
                sound: "custom:startup".to_string(),
            },
        );

        config.migrate_boot_sound_default();

        assert_eq!(
            config
                .sound_rules
                .get("boot")
                .map(|rule| rule.sound.as_str()),
            Some("custom:startup")
        );
        assert!(config.boot_sound_default_migrated);
    }
}
