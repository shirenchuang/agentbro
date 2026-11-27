// ConfigStore — JSON-based persistent configuration
// Platform-appropriate config path with auto-save and Tauri event emission

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Emitter};

fn hook_intent_id(agent: &str) -> &str {
    match agent {
        crate::agents::gemini_desktop::ID => "antigravity",
        _ => agent,
    }
}

pub(crate) fn hook_intent_enabled(enabled_agents: &[String], agent: &str) -> bool {
    let id = hook_intent_id(agent);
    enabled_agents
        .iter()
        .any(|saved| hook_intent_id(saved) == id)
}

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
pub struct PetWindowAnchor {
    pub left: bool,
    pub top: bool,
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
pub struct SessionSilenceRuleConfig {
    pub id: String,
    pub kind: String,
    pub pattern: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: u64,
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
    #[serde(default = "default_codex_app_server_sync_enabled")]
    pub codex_app_server_sync_enabled: bool,
    #[serde(default = "default_codex_app_server_sync_interval_seconds")]
    pub codex_app_server_sync_interval_seconds: u32,
    pub theme: String,
    #[serde(default = "default_language")]
    pub language: String,
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
    /// Drop non-blocking hook events whose cwd contains one of these legacy patterns.
    #[serde(default)]
    pub excluded_hook_cwd_substrings: String,
    /// Drop non-blocking hook events matching a directory or prompt silence rule.
    #[serde(default)]
    pub session_silence_rules: Vec<SessionSilenceRuleConfig>,
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
    /// Leave blocking prompts in the originating terminal when the user is away.
    #[serde(default)]
    pub idle_interaction_routing_enabled: bool,
    /// Minutes of system idle time before idle interaction routing activates.
    #[serde(default = "default_idle_interaction_routing_minutes")]
    pub idle_interaction_routing_minutes: u32,
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
    /// Optional anonymous product analytics.
    #[serde(default = "default_true")]
    pub analytics_enabled: bool,
    /// Whether the user has seen or acted on the analytics consent prompt.
    #[serde(default = "default_true")]
    pub analytics_consent_prompt_completed: bool,
    /// Filter sessions by focused terminal window
    #[serde(default)]
    pub follow_focus: bool,
    /// Island surface mode: "island" or "pet"
    #[serde(default = "default_island_surface_mode")]
    pub island_surface_mode: String,
    /// Dev-only pet vitals debug panel visibility.
    #[serde(default)]
    pub pet_vitals_debug_open: bool,
    /// Pet scale percentage
    #[serde(default = "default_island_pet_scale")]
    pub island_pet_scale: u32,
    /// Pet window origin
    #[serde(default)]
    pub island_pet_window_origin: Option<WindowOrigin>,
    /// Pet sprite anchor inside the transparent pet window.
    #[serde(default)]
    pub island_pet_window_anchor: Option<PetWindowAnchor>,
    /// Active pet identifier (e.g. "codex:dewey", "user:my-cat"). `None` = auto-follow active session's agent.
    #[serde(default)]
    pub island_active_pet_id: Option<String>,
    /// AUTO 模式下每个 agent 默认显示的宠物。Key 是 adapter.name()（"claude-code"、"codex" ...），
    /// value 是 pet id（"codex:dewey"）。缺失的 agent 在 AUTO 模式下回退到 registry[0]。
    #[serde(default)]
    pub island_agent_pet_map: std::collections::HashMap<String, String>,
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
    /// Agents (adapter names) whose AgentBro hooks the user has enabled. This is
    /// the persisted *intent* that survives external tools (e.g. cc-switch)
    /// overwriting the agent's settings file: hook recovery re-installs hooks for
    /// these agents even when the on-disk config no longer contains any trace of
    /// them, so a wiped hook is restored without a manual reinstall or restart.
    #[serde(default)]
    pub enabled_agents: Vec<String>,
}

fn default_display_id() -> String {
    "primary".to_string()
}

fn default_language() -> String {
    "en".to_string()
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

fn default_idle_interaction_routing_minutes() -> u32 {
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

const DEFAULT_CODEX_APP_SERVER_SYNC_INTERVAL_SECONDS: u32 = 30;
const LEGACY_CHIME_SOUND_CHOICE: &str = concat!("builtin:", "p", "i", "n", "g");

fn default_codex_app_server_sync_enabled() -> bool {
    !cfg!(target_os = "windows")
}

fn default_codex_app_server_sync_interval_seconds() -> u32 {
    DEFAULT_CODEX_APP_SERVER_SYNC_INTERVAL_SECONDS
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
            codex_app_server_sync_enabled: default_codex_app_server_sync_enabled(),
            codex_app_server_sync_interval_seconds: DEFAULT_CODEX_APP_SERVER_SYNC_INTERVAL_SECONDS,
            theme: "midnight".to_string(),
            language: default_language(),
            display_id: "primary".to_string(),
            auto_hide_no_sessions: false,
            sound_events: std::collections::HashMap::new(),
            sound_rules: std::collections::HashMap::new(),
            custom_sounds: Vec::new(),
            sound_pack: "synth".to_string(),
            boot_sound_default_migrated: true,
            probe_session_filter: false,
            excluded_hook_cwd_substrings: String::new(),
            session_silence_rules: Vec::new(),
            quiet_hours_enabled: false,
            quiet_hours_start: default_quiet_hours_start(),
            quiet_hours_end: default_quiet_hours_end(),
            idle_timeout_minutes: default_idle_timeout_minutes(),
            idle_interaction_routing_enabled: false,
            idle_interaction_routing_minutes: default_idle_interaction_routing_minutes(),
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
            analytics_enabled: true,
            analytics_consent_prompt_completed: false,
            follow_focus: false,
            island_surface_mode: default_island_surface_mode(),
            pet_vitals_debug_open: false,
            island_pet_scale: default_island_pet_scale(),
            island_pet_window_origin: None,
            island_pet_window_anchor: None,
            island_active_pet_id: None,
            island_agent_pet_map: std::collections::HashMap::new(),
            global_shortcut: "CommandOrControl+Shift+I".to_string(),
            shortcut_approve: default_shortcut_approve(),
            shortcut_approve_enabled: false,
            shortcut_deny: default_shortcut_deny(),
            shortcut_deny_enabled: false,
            shortcut_skip: default_shortcut_skip(),
            shortcut_skip_enabled: false,
            permission_shortcut_defaults_migrated: true,
            enabled_agents: Vec::new(),
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

    fn migrate_legacy_sound_choices(&mut self) {
        for rule in self.sound_rules.values_mut() {
            if rule.sound == LEGACY_CHIME_SOUND_CHOICE {
                rule.sound = "builtin:chime".to_string();
            }
        }
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
        config.migrate_legacy_sound_choices();
        Some(config)
    }

    /// Save config to disk
    fn save_to_disk(&self, config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let path = match self.config_path.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => self.config_path.clone(),
            Err(error) => return Err(error.into()),
        };
        let permissions = fs::metadata(&path).ok().map(|meta| meta.permissions());
        if permissions.as_ref().is_some_and(fs::Permissions::readonly) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Configuration file is read-only",
            )
            .into());
        }
        let content = serde_json::to_vec_pretty(config)?;
        let staged = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&staged)?;
        let result = (|| {
            file.write_all(&content)?;
            if let Some(permissions) = permissions {
                file.set_permissions(permissions)?;
            }
            drop(file);
            fs::rename(&staged, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&staged);
        }
        result?;
        Ok(())
    }

    /// Get the current config
    pub fn get(&self) -> AppConfig {
        self.config.read().map(|c| c.clone()).unwrap_or_default()
    }

    /// Update the config, save to disk, and emit event
    pub fn update(&self, new_config: AppConfig) -> Result<(), String> {
        self.commit(|_| Some(new_config)).map(|_| ())
    }

    pub(crate) fn complete_first_run(
        &self,
        surface_mode: String,
    ) -> Result<Option<(AppConfig, AppConfig)>, String> {
        if !matches!(surface_mode.as_str(), "island" | "pet") {
            return Err(format!("Unknown island surface mode: {surface_mode}"));
        }
        self.commit(|current| {
            if current.analytics_consent_prompt_completed {
                return None;
            }
            let mut next = current.clone();
            next.island_surface_mode = surface_mode;
            next.island_pet_scale = next.island_pet_scale.clamp(10, 120);
            next.island_pet_window_origin = None;
            next.island_pet_window_anchor = None;
            next.analytics_enabled = true;
            next.analytics_consent_prompt_completed = true;
            Some(next)
        })
    }

    fn commit(
        &self,
        change: impl FnOnce(&AppConfig) -> Option<AppConfig>,
    ) -> Result<Option<(AppConfig, AppConfig)>, String> {
        let mut config = self
            .config
            .write()
            .map_err(|e| format!("Lock error: {e}"))?;
        let Some(new_config) = change(&config) else {
            return Ok(None);
        };
        if let Err(e) = self.save_to_disk(&new_config) {
            log::error!("Failed to save config: {}", e);
            return Err(format!("Failed to save config: {}", e));
        }
        let previous = std::mem::replace(&mut *config, new_config.clone());
        drop(config);

        // Emit config change event to frontend
        if let Some(ref handle) = self.app_handle {
            if let Err(e) = handle.emit("config-changed", &new_config) {
                log::error!("Failed to emit config-changed: {}", e);
            }
        }

        log::info!("Config updated and saved");
        Ok(Some((previous, new_config)))
    }

    /// Record that the user has enabled AgentBro hooks for `agent`. Returns
    /// `Ok(true)` when the intent set changed (and was persisted). No-op when the
    /// agent is already present, to avoid needless writes / config-changed churn.
    pub fn mark_agent_enabled(&self, agent: &str) -> Result<bool, String> {
        self.commit(|current| {
            if hook_intent_enabled(&current.enabled_agents, agent) {
                return None;
            }
            let mut next = current.clone();
            next.enabled_agents.push(hook_intent_id(agent).to_string());
            Some(next)
        })
        .map(|change| change.is_some())
    }

    /// Drop `agent` from the enabled-hooks intent set. Returns `Ok(true)` when the
    /// set changed.
    pub fn mark_agent_disabled(&self, agent: &str) -> Result<bool, String> {
        self.commit(|current| {
            if !hook_intent_enabled(&current.enabled_agents, agent) {
                return None;
            }
            let id = hook_intent_id(agent);
            let mut next = current.clone();
            next.enabled_agents
                .retain(|saved| hook_intent_id(saved) != id);
            Some(next)
        })
        .map(|change| change.is_some())
    }
}

impl Default for ConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::installer::TempInstallDir;

    #[test]
    fn first_run_setup_saves_surface_and_completion_once() {
        let fixture = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = fixture.path().join("首次设置.json");
        let initial = AppConfig {
            language: "zh".to_string(),
            analytics_enabled: false,
            island_pet_window_origin: Some(WindowOrigin { x: 25.0, y: 110.0 }),
            island_pet_window_anchor: Some(PetWindowAnchor {
                left: true,
                top: true,
            }),
            ..AppConfig::default()
        };
        let store = ConfigStore {
            config: Arc::new(RwLock::new(initial.clone())),
            config_path: path.clone(),
            app_handle: None,
        };
        store.update(initial).unwrap();
        let (_, committed) = store.complete_first_run("pet".into()).unwrap().unwrap();
        let saved = ConfigStore::load_from_disk(&path).unwrap();
        for config in [committed, saved, store.get()] {
            assert_eq!(config.island_surface_mode, "pet");
            assert_eq!(config.language, "zh");
            assert!(config.island_pet_window_anchor.is_none());
            assert!(config.island_pet_window_origin.is_none());
            assert!(config.analytics_enabled);
            assert!(config.analytics_consent_prompt_completed);
        }
        let bytes = fs::read(&path).unwrap();
        assert!(store.complete_first_run("island".into()).unwrap().is_none());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(store.get().island_surface_mode, "pet");
    }

    #[test]
    fn first_run_setup_leaves_previous_config_on_save_error() {
        let fixture = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = fixture.path().join("config.json");
        let store = ConfigStore {
            config: Arc::new(RwLock::new(AppConfig::default())),
            config_path: path.clone(),
            app_handle: None,
        };
        store.update(store.get()).unwrap();
        let bytes = fs::read(&path).unwrap();
        let permissions = fs::metadata(&path).unwrap().permissions();
        let mut read_only = permissions.clone();
        read_only.set_readonly(true);
        fs::set_permissions(&path, read_only).unwrap();
        let result = store.complete_first_run("pet".into());
        fs::set_permissions(&path, permissions).unwrap();
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(store.get().island_surface_mode, "island");
        assert!(!store.get().analytics_consent_prompt_completed);
        assert_eq!(fs::read_dir(fixture.path()).unwrap().count(), 1);
        assert!(store.complete_first_run("pet".into()).unwrap().is_some());
    }

    #[test]
    fn defaults_match_agentbro_island_behavior() {
        let config = AppConfig::default();

        assert_eq!(config.completion_timeout, 5);
        assert_eq!(config.idle_timeout_minutes, 5);
        assert_eq!(config.sound_volume, 0.7);
        assert_eq!(config.volume, 70);
        assert!(!config.shortcut_approve_enabled);
        assert!(!config.shortcut_deny_enabled);
        assert!(config.permission_shortcut_defaults_migrated);
        assert!(config.boot_sound_default_migrated);
        assert!(config.analytics_enabled);
        assert!(!config.analytics_consent_prompt_completed);
    }

    #[test]
    fn enabled_agents_defaults_to_empty_when_field_is_missing() {
        let mut value = serde_json::to_value(AppConfig::default()).expect("serialize config");
        value
            .as_object_mut()
            .expect("config object")
            .remove("enabledAgents");

        let config: AppConfig = serde_json::from_value(value).expect("deserialize legacy config");

        assert!(config.enabled_agents.is_empty());
    }

    #[test]
    fn enabled_agents_roundtrips_through_serde() {
        let config = AppConfig {
            enabled_agents: vec!["claude-code".to_string(), "codex".to_string()],
            ..AppConfig::default()
        };
        let value = serde_json::to_value(&config).expect("serialize config");
        assert_eq!(
            value
                .get("enabledAgents")
                .and_then(|v| v.as_array())
                .map(Vec::len),
            Some(2)
        );

        let restored: AppConfig = serde_json::from_value(value).expect("deserialize config");
        assert_eq!(restored.enabled_agents, vec!["claude-code", "codex"]);
    }

    #[test]
    fn shared_antigravity_hook_intent_is_removed_from_either_entry() {
        let fixture = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = fixture.path().join("config.json");
        let store = ConfigStore {
            config: Arc::new(RwLock::new(AppConfig::default())),
            config_path: path.clone(),
            app_handle: None,
        };
        for (enable, disable) in [
            ("gemini-desktop", "antigravity"),
            ("antigravity", "gemini-desktop"),
        ] {
            assert!(store.mark_agent_enabled(enable).unwrap());
            assert!(store.mark_agent_disabled(disable).unwrap());
            assert!(ConfigStore::load_from_disk(&path)
                .unwrap()
                .enabled_agents
                .is_empty());
        }

        store
            .update(AppConfig {
                enabled_agents: ["gemini-desktop", "gemini", "antigravity", "codex"]
                    .map(str::to_string)
                    .to_vec(),
                ..AppConfig::default()
            })
            .unwrap();
        assert!(!store.mark_agent_enabled("gemini-desktop").unwrap());
        assert!(store.mark_agent_disabled("antigravity").unwrap());
        assert_eq!(
            ConfigStore::load_from_disk(&path).unwrap().enabled_agents,
            ["gemini", "codex"]
        );
    }

    #[test]
    fn keeps_legacy_analytics_defaults_when_fields_are_missing() {
        let mut value = serde_json::to_value(AppConfig::default()).expect("serialize config");
        let object = value.as_object_mut().expect("config object");
        object.remove("analyticsEnabled");
        object.remove("analyticsConsentPromptCompleted");

        let config: AppConfig = serde_json::from_value(value).expect("deserialize legacy config");

        assert!(config.analytics_enabled);
        assert!(config.analytics_consent_prompt_completed);
    }

    #[test]
    fn codex_app_server_sync_uses_platform_default_when_field_is_missing() {
        let mut value = serde_json::to_value(AppConfig::default()).expect("serialize config");
        let object = value.as_object_mut().expect("config object");
        object.remove("codexAppServerSyncEnabled");

        let config: AppConfig = serde_json::from_value(value).expect("deserialize fresh config");

        assert_eq!(
            config.codex_app_server_sync_enabled,
            default_codex_app_server_sync_enabled()
        );
    }

    #[test]
    fn codex_app_server_sync_preserves_existing_false() {
        // Users who explicitly turned the feature off (back when default was
        // false) must keep their choice after we flip the default to true.
        let mut value = serde_json::to_value(AppConfig::default()).expect("serialize config");
        value.as_object_mut().unwrap().insert(
            "codexAppServerSyncEnabled".into(),
            serde_json::Value::Bool(false),
        );

        let config: AppConfig = serde_json::from_value(value).expect("deserialize stored config");

        assert!(!config.codex_app_server_sync_enabled);
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

    #[test]
    fn migrates_legacy_chime_sound_choice() {
        let mut config = AppConfig::default();
        config.sound_rules.insert(
            "permission-request".to_string(),
            super::SoundRuleConfig {
                enabled: true,
                sound: concat!("builtin:", "p", "i", "n", "g").to_string(),
            },
        );

        config.migrate_legacy_sound_choices();

        assert_eq!(
            config
                .sound_rules
                .get("permission-request")
                .map(|rule| rule.sound.as_str()),
            Some("builtin:chime")
        );
    }
}
