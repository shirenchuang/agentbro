// AgentBro — Rust Backend Library
pub mod agents;
pub mod commands;
pub mod config;
pub mod hook_endpoint;
pub mod hooks;
pub mod network_monitor;
pub mod pets;
pub mod platform;
pub mod remote;
pub mod skills;
pub mod sound;
pub mod switch;
pub mod telemetry;
pub mod terminal;
pub mod theme;
pub mod webhook;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};

use agents::claude_code::ClaudeCodeAdapter;
use agents::AdapterStatus;
use agents::AgentAdapter;
use commands::persistence::{load_sessions, save_sessions};
use commands::AppState;
use config::{ConfigStore, CustomSoundConfig, SoundRuleConfig};
use hooks::conversation_parser::{
    all_projects_dirs, discover_active_sessions_in_dirs, projects_dirs_from_roots,
};
use hooks::file_watcher::ConversationWatcher;
use hooks::server::HookServer;
use hooks::session_store::{SessionPhase, SessionState, SessionStore};
use network_monitor::NetworkMonitor;
use platform::display::{find_target_monitor, list_displays_inner, DisplayInfo};
use sound::{SoundEngine, SoundEvent, SoundPack};
use telemetry::TelemetryService;

#[derive(Debug, Clone, Copy)]
struct NotchDragState {
    start_x: f64,
    start_offset: f64,
    current_offset: f64,
    width: f64,
    y: f64,
    base_center: f64,
    min_center: f64,
    max_center: f64,
    last_window_x: f64,
}

#[derive(Debug, Clone, Copy)]
struct PetDragState {
    start_cursor_x: f64,
    start_cursor_y: f64,
    start_window_x: f64,
    start_window_y: f64,
    current_x: f64,
    current_y: f64,
}

static NOTCH_DRAG_STATE: OnceLock<Mutex<Option<NotchDragState>>> = OnceLock::new();
static PET_DRAG_STATE: OnceLock<Mutex<Option<PetDragState>>> = OnceLock::new();

fn notch_drag_state() -> &'static Mutex<Option<NotchDragState>> {
    NOTCH_DRAG_STATE.get_or_init(|| Mutex::new(None))
}

fn pet_drag_state() -> &'static Mutex<Option<PetDragState>> {
    PET_DRAG_STATE.get_or_init(|| Mutex::new(None))
}

// ── Display Controller Commands ─────────────────────────────────

#[tauri::command]
async fn get_display_level(
    state: tauri::State<'_, commands::AppState>,
) -> Result<platform::display_controller::DisplayLevel, String> {
    Ok(state.display_controller.current_level())
}

#[tauri::command]
async fn notify_cursor_enter(state: tauri::State<'_, commands::AppState>) -> Result<(), String> {
    state.display_controller.on_cursor_enter().await;
    Ok(())
}

#[tauri::command]
async fn notify_cursor_leave(state: tauri::State<'_, commands::AppState>) -> Result<(), String> {
    state.display_controller.on_cursor_leave().await;
    Ok(())
}

#[tauri::command]
async fn notify_esc(state: tauri::State<'_, commands::AppState>) -> Result<(), String> {
    state.display_controller.on_esc().await;
    Ok(())
}

#[tauri::command]
async fn notify_expand(state: tauri::State<'_, commands::AppState>) -> Result<(), String> {
    state.display_controller.on_expand();
    Ok(())
}

#[tauri::command]
async fn set_notch_focusable(app: tauri::AppHandle, focusable: bool) -> Result<(), String> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("notch") {
            #[cfg(target_os = "macos")]
            {
                use objc2_app_kit::NSWindow;
                if let Ok(ptr) = window.ns_window() {
                    unsafe {
                        let ns_window = ptr as *const NSWindow;
                        if focusable {
                            (*ns_window).makeKeyWindow();
                        } else {
                            (*ns_window).resignKeyWindow();
                        }
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                if focusable {
                    let _ = window.set_focus();
                }
            }
        }
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_notch_ignore_cursor_events(
    app: tauri::AppHandle,
    ignore: bool,
    window_label: Option<String>,
) -> Result<(), String> {
    let label = window_label.unwrap_or_else(|| "notch".to_string());
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(&label) {
            let _ = window.set_ignore_cursor_events(ignore);
        }
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_image(src: String) -> Result<(), String> {
    if src.starts_with("data:") {
        let target = persist_data_url_image(&src)?;
        return open_system_target(&target);
    }
    if src.starts_with("http://") || src.starts_with("https://") {
        return open_system_target(&src);
    }
    // Otherwise treat as a local file: it must exist and be a recognized image.
    let expanded = expand_tilde_target(&src);
    let path = std::path::Path::new(&expanded);
    if !path.is_file() {
        return Err("Image path does not exist".to_string());
    }
    let ext_ok = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "bmp"
                    | "svg"
                    | "heic"
                    | "tif"
                    | "tiff"
                    | "ico"
                    | "avif"
            )
        })
        .unwrap_or(false);
    if !ext_ok {
        return Err("Unsupported image file".to_string());
    }
    open_system_target(&expanded)
}

#[tauri::command]
async fn open_system_path(path: String) -> Result<(), String> {
    // Only open real local paths. This rejects injected URLs / app-launch
    // schemes (e.g. `https://…`, `x-app://…`), which never resolve to a path.
    let expanded = expand_tilde_target(&path);
    if !std::path::Path::new(&expanded).exists() {
        return Err("Path does not exist".to_string());
    }
    open_system_target(&path)
}

fn persist_data_url_image(src: &str) -> Result<String, String> {
    let (header, data) = src
        .split_once(',')
        .ok_or_else(|| "Invalid image data URL".to_string())?;
    if !header.contains(";base64") {
        return Err("Only base64 image data URLs can be opened".to_string());
    }

    let media_type = header
        .strip_prefix("data:")
        .unwrap_or("image/png")
        .split(';')
        .next()
        .unwrap_or("image/png");
    let bytes = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| format!("Invalid image data: {}", e))?
    };
    let ext = image_extension(media_type);
    let path =
        std::env::temp_dir().join(format!("agentbro-image-{}.{}", uuid::Uuid::new_v4(), ext));
    std::fs::write(&path, bytes).map_err(|e| format!("Failed to write temp image: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}

fn image_extension(media_type: &str) -> &'static str {
    match media_type {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        "image/svg+xml" => "svg",
        "image/heic" => "heic",
        _ => "png",
    }
}

fn open_system_target(target: &str) -> Result<(), String> {
    let target = expand_tilde_target(target);

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(&target);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", &target]);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(&target);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open path: {}", e))
}

fn expand_tilde_target(target: &str) -> String {
    if let Some(rest) = target.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).display().to_string();
        }
    }
    target.to_string()
}

// ── Agent Detection & Hook Management Commands ──────────────────

/// Refuse to install hooks for an adapter whose CLI is not installed.
/// Re-runs the per-adapter detection probe (not the cached value set at
/// startup) so that newly-installed CLIs become installable without an
/// app restart. `Installed` (config-dir-only, e.g. IDE extensions) counts
/// as installed — only `Unavailable` blocks.
fn ensure_installable(adapter: &dyn AgentAdapter) -> Result<(), String> {
    match adapter.detect_status_now() {
        AdapterStatus::Unavailable => Err(format!(
            "{} CLI not found on PATH. Install it first, then try again.",
            adapter.display_name()
        )),
        _ => Ok(()),
    }
}

#[tauri::command]
async fn detect_tools() -> Vec<agents::detection::DetectedTool> {
    agents::detection::detect_installed_tools()
}

#[tauri::command]
async fn install_agent_hook(
    state: tauri::State<'_, commands::AppState>,
    tool_name: String,
) -> Result<(), String> {
    if let Some(custom_id) = tool_name.strip_prefix("custom:") {
        let cfg = state.config_store.get();
        let entry = cfg
            .custom_hook_installs
            .iter()
            .find(|e| e.id == custom_id)
            .ok_or_else(|| format!("Unknown custom hook: {}", custom_id))?
            .clone();
        let profile = agents::profiles::profile_for_agent(&entry.profile_id)
            .ok_or_else(|| format!("Unknown hook profile: {}", entry.profile_id))?;
        let base_dir = std::path::PathBuf::from(expand_tilde_target(&entry.install_directory));
        let dir_str = base_dir.display().to_string();
        agents::profiles::install_custom_at_labeled(
            &profile,
            &base_dir,
            Some(&entry.display_name),
            Some(&dir_str),
        )
        .map_err(|e| e.to_string())?;
        state
            .telemetry
            .record_hook_install(&cfg, &entry.profile_id)
            .await;
        return Ok(());
    }
    let adapter = state
        .adapters
        .iter()
        .find(|a| a.name() == tool_name || a.display_name() == tool_name)
        .ok_or_else(|| format!("Unknown tool: {}", tool_name))?;
    ensure_installable(adapter.as_ref())?;
    adapter.install_hooks().map_err(|e| e.to_string())?;
    let config = state.config_store.get();
    state
        .telemetry
        .record_hook_install(&config, &tool_name)
        .await;
    Ok(())
}

#[tauri::command]
async fn install_custom_agent_hook(
    state: tauri::State<'_, commands::AppState>,
    profile_id: String,
    install_directory: String,
    custom_name: Option<String>,
) -> Result<String, String> {
    let profile = agents::profiles::profile_for_agent(&profile_id)
        .ok_or_else(|| format!("Unknown hook profile: {}", profile_id))?;
    let base_directory = std::path::PathBuf::from(expand_tilde_target(&install_directory));
    if !base_directory.is_dir() {
        return Err(format!(
            "Install directory does not exist: {}",
            base_directory.display()
        ));
    }

    // Validate the directory looks like a valid config root for this agent.
    // For JSON-based hooks (claude-code, codex, etc.), the parent of configuration_path
    // must exist. E.g. for claude-code, configuration_path is ".claude/settings.json",
    // so the base_directory should look like a .claude directory or contain one.
    let config_file_name = std::path::Path::new(profile.configuration_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let target_path = agents::profiles::custom_installation_path(&profile, &base_directory);
    if let Some(parent) = target_path.parent() {
        if !parent.is_dir() {
            return Err(format!(
                "Config parent directory does not exist: {}. Expected a valid config root for {}.",
                parent.display(),
                profile.id
            ));
        }
    }

    // Check for duplicate install directory
    let cfg = state.config_store.get();
    let dir_lower = install_directory.trim().to_lowercase();
    if cfg
        .custom_hook_installs
        .iter()
        .any(|e| e.install_directory.to_lowercase() == dir_lower && e.profile_id == profile_id)
    {
        return Err(format!(
            "A custom hook for {} at this directory already exists.",
            profile.id
        ));
    }
    drop(cfg);

    let display_name = custom_name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("{} ({})", profile.id, install_directory));
    let dir_str = base_directory.display().to_string();
    let target = agents::profiles::install_custom_at_labeled(
        &profile,
        &base_directory,
        Some(&display_name),
        Some(&dir_str),
    )
    .map_err(|e| e.to_string())?;
    let entry = config::CustomHookInstall {
        id: uuid::Uuid::new_v4().to_string(),
        profile_id: profile_id.clone(),
        display_name,
        install_directory,
    };
    let mut cfg = state.config_store.get();
    cfg.custom_hook_installs.push(entry);
    state.config_store.update(cfg).map_err(|e| e.to_string())?;
    let config = state.config_store.get();
    state
        .telemetry
        .record_hook_install(&config, &profile_id)
        .await;

    // Provide a helpful hint about what was validated
    let hint = if target_path.exists() || config_file_name.is_empty() {
        String::new()
    } else {
        format!(" (note: {} was created at this path)", config_file_name)
    };
    Ok(format!("{}{}", display_path_with_home(&target), hint))
}

#[tauri::command]
async fn uninstall_agent_hook(
    state: tauri::State<'_, commands::AppState>,
    tool_name: String,
) -> Result<(), String> {
    if let Some(custom_id) = tool_name.strip_prefix("custom:") {
        let mut cfg = state.config_store.get();
        let idx = cfg
            .custom_hook_installs
            .iter()
            .position(|e| e.id == custom_id)
            .ok_or_else(|| format!("Unknown custom hook: {}", custom_id))?;
        let entry = cfg.custom_hook_installs[idx].clone();
        let profile = agents::profiles::profile_for_agent(&entry.profile_id)
            .ok_or_else(|| format!("Unknown hook profile: {}", entry.profile_id))?;
        let base_dir = std::path::PathBuf::from(expand_tilde_target(&entry.install_directory));
        let target = agents::profiles::custom_installation_path(&profile, &base_dir);
        if target.exists() {
            agents::profiles::uninstall_at(&profile, &target).map_err(|e| e.to_string())?;
        }
        cfg.custom_hook_installs.remove(idx);
        state.config_store.update(cfg).map_err(|e| e.to_string())?;
        let config = state.config_store.get();
        state
            .telemetry
            .record_hook_uninstall(&config, &entry.profile_id)
            .await;
        return Ok(());
    }
    let adapter = state
        .adapters
        .iter()
        .find(|a| a.name() == tool_name || a.display_name() == tool_name)
        .ok_or_else(|| format!("Unknown tool: {}", tool_name))?;
    adapter.remove_hooks().map_err(|e| e.to_string())?;
    let config = state.config_store.get();
    state
        .telemetry
        .record_hook_uninstall(&config, &tool_name)
        .await;
    Ok(())
}

#[tauri::command]
async fn configure_agent_hook_events(
    state: tauri::State<'_, commands::AppState>,
    tool_name: String,
    enabled_events: Vec<String>,
) -> Result<(), String> {
    if let Some(custom_id) = tool_name.strip_prefix("custom:") {
        let cfg = state.config_store.get();
        let entry = cfg
            .custom_hook_installs
            .iter()
            .find(|e| e.id == custom_id)
            .ok_or_else(|| format!("Unknown custom hook: {}", custom_id))?
            .clone();
        let profile = agents::profiles::profile_for_agent(&entry.profile_id)
            .ok_or_else(|| format!("Unknown hook profile: {}", entry.profile_id))?;
        agents::profiles::save_event_selection(&profile, &enabled_events)
            .map_err(|e| e.to_string())?;
        let base_dir = std::path::PathBuf::from(expand_tilde_target(&entry.install_directory));
        let dir_str = base_dir.display().to_string();
        agents::profiles::install_custom_at_labeled(
            &profile,
            &base_dir,
            Some(&entry.display_name),
            Some(&dir_str),
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let adapter = state
        .adapters
        .iter()
        .find(|a| a.name() == tool_name || a.display_name() == tool_name)
        .ok_or_else(|| format!("Unknown tool: {}", tool_name))?;
    let profile = agents::profiles::profile_for_agent(adapter.name())
        .ok_or_else(|| format!("Unknown hook profile: {}", adapter.name()))?;
    agents::profiles::save_event_selection(&profile, &enabled_events).map_err(|e| e.to_string())?;
    ensure_installable(adapter.as_ref())?;
    adapter.install_hooks().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_all_hook_status(
    state: tauri::State<'_, commands::AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut results: Vec<serde_json::Value> = state
        .adapters
        .iter()
        .map(|a| {
            let tool_id = if a.name() == "claude-code" && a.display_name() != "Claude Code" {
                a.display_name().to_string()
            } else {
                a.name().to_string()
            };
            let profile = agents::profiles::profile_for_agent(a.name());
            let supports_event_selection = profile
                .as_ref()
                .map(agents::profiles::supports_event_selection)
                .unwrap_or(false);
            let events = profile
                .as_ref()
                .map(agents::profiles::event_statuses)
                .unwrap_or_default();
            let enabled_event_names = profile
                .as_ref()
                .map(agents::profiles::selected_event_names)
                .unwrap_or_default();
            let paths = a.hook_config_paths();
            let hook_health = profile
                .as_ref()
                .and_then(|profile| {
                    paths
                        .first()
                        .map(|path| agents::profiles::install_health(profile, path))
                })
                .unwrap_or_else(|| {
                    if a.hooks_installed() {
                        agents::profiles::HookInstallHealth::Installed
                    } else {
                        agents::profiles::HookInstallHealth::NotInstalled
                    }
                });
            let installed = hook_health.is_present();
            let config_path = paths
                .first()
                .map(|path| display_path_with_home(path))
                .unwrap_or_default();
            let config_dir = paths
                .first()
                .and_then(|path| path.parent())
                .map(display_path_with_home)
                .unwrap_or_default();
            let install_status = hook_health.as_status_str();
            serde_json::json!({
                "toolId": tool_id,
                "adapterId": a.name(),
                "profileId": profile.as_ref().map(|profile| profile.id).unwrap_or(a.name()),
                "name": a.name(),
                "displayName": a.display_name(),
                "installed": installed,
                "installStatus": install_status,
                "configPath": config_path,
                "configDir": config_dir,
                "status": format!("{:?}", a.detect_status_now()),
                "supportsEventSelection": supports_event_selection,
                "events": events,
                "enabledEventNames": enabled_event_names,
            })
        })
        .collect();

    // Append custom hook installs from config
    let cfg = state.config_store.get();
    for entry in &cfg.custom_hook_installs {
        let profile = agents::profiles::profile_for_agent(&entry.profile_id);
        let base_dir = std::path::PathBuf::from(expand_tilde_target(&entry.install_directory));
        let target_path = profile
            .as_ref()
            .map(|p| agents::profiles::custom_installation_path(p, &base_dir));
        let hook_health = match (&profile, &target_path) {
            (Some(p), Some(path)) => agents::profiles::install_health(p, path),
            _ => agents::profiles::HookInstallHealth::NotInstalled,
        };
        let installed = hook_health.is_present();
        let config_path = target_path
            .as_ref()
            .map(|p| display_path_with_home(p))
            .unwrap_or_default();
        let config_dir = target_path
            .as_ref()
            .and_then(|p| p.parent())
            .map(display_path_with_home)
            .unwrap_or_default();
        let install_status = hook_health.as_status_str();
        let supports_event_selection = profile
            .as_ref()
            .map(agents::profiles::supports_event_selection)
            .unwrap_or(false);
        let events = profile
            .as_ref()
            .map(agents::profiles::event_statuses)
            .unwrap_or_default();
        let enabled_event_names = profile
            .as_ref()
            .map(agents::profiles::selected_event_names)
            .unwrap_or_default();
        let tool_id = format!("custom:{}", entry.id);
        results.push(serde_json::json!({
            "toolId": tool_id,
            "adapterId": entry.profile_id,
            "profileId": entry.profile_id,
            "name": entry.profile_id,
            "displayName": entry.display_name,
            "installed": installed,
            "installStatus": install_status,
            "configPath": config_path,
            "configDir": config_dir,
            "status": if installed { "Installed" } else { "NotInstalled" },
            "supportsEventSelection": supports_event_selection,
            "events": events,
            "enabledEventNames": enabled_event_names,
            "isCustom": true,
            "customId": entry.id,
        }));
    }

    Ok(results)
}

fn display_path_with_home(path: &std::path::Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

#[tauri::command]
async fn reinstall_all_hooks(
    state: tauri::State<'_, commands::AppState>,
) -> Result<Vec<String>, String> {
    let mut errors = Vec::new();
    for adapter in &state.adapters {
        if let Err(skip_reason) = ensure_installable(adapter.as_ref()) {
            log::info!(
                "Skipping hook reinstall for {}: {}",
                adapter.display_name(),
                skip_reason
            );
            continue;
        }
        let result = adapter.install_hooks().map_err(|e| e.to_string());
        if let Err(e) = result {
            errors.push(format!("{}: {}", adapter.name(), e));
        } else {
            let config = state.config_store.get();
            state
                .telemetry
                .record_hook_install(&config, adapter.name())
                .await;
        }
    }
    let cfg = state.config_store.get();
    for entry in &cfg.custom_hook_installs {
        if let Some(profile) = agents::profiles::profile_for_agent(&entry.profile_id) {
            let base_dir = std::path::PathBuf::from(expand_tilde_target(&entry.install_directory));
            let dir_str = base_dir.display().to_string();
            let result = agents::profiles::install_custom_at_labeled(
                &profile,
                &base_dir,
                Some(&entry.display_name),
                Some(&dir_str),
            )
            .map_err(|e| e.to_string());
            if let Err(e) = result {
                errors.push(format!("{}: {}", entry.display_name, e));
            } else {
                let config = state.config_store.get();
                state
                    .telemetry
                    .record_hook_install(&config, &entry.profile_id)
                    .await;
            }
        }
    }
    Ok(errors)
}

#[tauri::command]
async fn uninstall_all_hooks(
    state: tauri::State<'_, commands::AppState>,
) -> Result<Vec<String>, String> {
    let mut errors = Vec::new();
    for adapter in &state.adapters {
        let result = adapter.remove_hooks().map_err(|e| e.to_string());
        if let Err(e) = result {
            errors.push(format!("{}: {}", adapter.display_name(), e));
        } else {
            let config = state.config_store.get();
            state
                .telemetry
                .record_hook_uninstall(&config, adapter.name())
                .await;
        }
    }
    let mut cfg = state.config_store.get();
    let custom_entries = std::mem::take(&mut cfg.custom_hook_installs);
    for entry in custom_entries {
        let Some(profile) = agents::profiles::profile_for_agent(&entry.profile_id) else {
            continue;
        };
        let base_dir = std::path::PathBuf::from(expand_tilde_target(&entry.install_directory));
        let target = agents::profiles::custom_installation_path(&profile, &base_dir);
        if target.exists() {
            let result =
                agents::profiles::uninstall_at(&profile, &target).map_err(|e| e.to_string());
            if let Err(e) = result {
                errors.push(format!("{}: {}", entry.display_name, e));
            } else {
                let config = state.config_store.get();
                state
                    .telemetry
                    .record_hook_uninstall(&config, &entry.profile_id)
                    .await;
            }
        }
    }
    state.config_store.update(cfg).map_err(|e| e.to_string())?;
    Ok(errors)
}

// ── Remote SSH Commands ─────────────────────────────────────────

#[tauri::command]
async fn list_remote_hosts(
    state: tauri::State<'_, commands::AppState>,
) -> Result<Vec<remote::RemoteHost>, String> {
    Ok(state.remote_manager.hosts())
}

#[tauri::command]
async fn add_remote_host(
    state: tauri::State<'_, commands::AppState>,
    host: remote::RemoteHost,
) -> Result<(), String> {
    state.remote_manager.add_host(host.clone());
    let mut cfg = state.config_store.get();
    cfg.remote_hosts.push(host);
    state.config_store.update(cfg)
}

#[tauri::command]
async fn remove_remote_host(
    state: tauri::State<'_, commands::AppState>,
    id: String,
) -> Result<(), String> {
    state.remote_manager.remove_host(&id);
    let mut cfg = state.config_store.get();
    cfg.remote_hosts.retain(|h| h.id != id);
    state.config_store.update(cfg)
}

#[tauri::command]
async fn connect_remote(
    state: tauri::State<'_, commands::AppState>,
    id: String,
) -> Result<(), String> {
    state.remote_manager.connect(&id);
    Ok(())
}

#[tauri::command]
async fn disconnect_remote(
    state: tauri::State<'_, commands::AppState>,
    id: String,
) -> Result<(), String> {
    state.remote_manager.disconnect(&id);
    Ok(())
}

#[tauri::command]
async fn install_remote_hooks(
    state: tauri::State<'_, commands::AppState>,
    id: String,
) -> Result<String, String> {
    let hosts = state.remote_manager.hosts();
    let host = hosts
        .iter()
        .find(|h| h.id == id)
        .ok_or_else(|| format!("Host {} not found", id))?
        .clone();
    let result = remote::installer::RemoteInstaller::install_hooks(&host).await;
    if result.ok {
        let config = state.config_store.get();
        state
            .telemetry
            .record_hook_install(&config, "remote:all")
            .await;
        Ok(result.message)
    } else {
        Err(result.message)
    }
}

#[tauri::command]
async fn uninstall_remote_hooks(
    state: tauri::State<'_, commands::AppState>,
    id: String,
) -> Result<String, String> {
    let hosts = state.remote_manager.hosts();
    let host = hosts
        .iter()
        .find(|h| h.id == id)
        .ok_or_else(|| format!("Host {} not found", id))?
        .clone();
    let result = remote::installer::RemoteInstaller::uninstall_hooks(&host).await;
    if result.ok {
        let config = state.config_store.get();
        state
            .telemetry
            .record_hook_uninstall(&config, "remote:all")
            .await;
        Ok(result.message)
    } else {
        Err(result.message)
    }
}

#[tauri::command]
async fn install_remote_agent_hooks(
    state: tauri::State<'_, commands::AppState>,
    id: String,
    agent_id: String,
) -> Result<String, String> {
    let hosts = state.remote_manager.hosts();
    let host = hosts
        .iter()
        .find(|h| h.id == id)
        .ok_or_else(|| format!("Host {} not found", id))?
        .clone();
    let result =
        remote::installer::RemoteInstaller::install_hooks_for_agent(&host, &agent_id).await;
    if result.ok {
        let config = state.config_store.get();
        state
            .telemetry
            .record_hook_install(&config, &format!("remote:{agent_id}"))
            .await;
        Ok(result.message)
    } else {
        Err(result.message)
    }
}

#[tauri::command]
async fn uninstall_remote_agent_hooks(
    state: tauri::State<'_, commands::AppState>,
    id: String,
    agent_id: String,
) -> Result<String, String> {
    let hosts = state.remote_manager.hosts();
    let host = hosts
        .iter()
        .find(|h| h.id == id)
        .ok_or_else(|| format!("Host {} not found", id))?
        .clone();
    let result =
        remote::installer::RemoteInstaller::uninstall_hooks_for_agent(&host, &agent_id).await;
    if result.ok {
        let config = state.config_store.get();
        state
            .telemetry
            .record_hook_uninstall(&config, &format!("remote:{agent_id}"))
            .await;
        Ok(result.message)
    } else {
        Err(result.message)
    }
}

#[tauri::command]
async fn check_remote_hooks(
    state: tauri::State<'_, commands::AppState>,
    id: String,
) -> Result<Vec<String>, String> {
    let hosts = state.remote_manager.hosts();
    let host = hosts
        .iter()
        .find(|h| h.id == id)
        .ok_or_else(|| format!("Host {} not found", id))?
        .clone();
    Ok(remote::installer::RemoteInstaller::check_installed_agents(&host).await)
}

#[tauri::command]
fn list_remote_installable_agents() -> Vec<String> {
    remote::installer::REMOTE_INSTALLABLE_AGENTS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[tauri::command]
async fn get_remote_status(
    state: tauri::State<'_, commands::AppState>,
    id: String,
) -> Result<remote::ConnectionStatus, String> {
    Ok(state.remote_manager.status(&id))
}

#[tauri::command]
async fn list_ssh_config_hosts() -> Result<Vec<remote::SshConfigHost>, String> {
    Ok(remote::ssh_config::read_ssh_config_hosts())
}

// ── Webhook Commands ────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebhookFormConfig {
    enabled: bool,
    url: String,
    secret: Option<String>,
    #[serde(default)]
    events: Vec<String>,
    #[serde(default)]
    delay_enabled: bool,
    #[serde(default = "default_webhook_delay_minutes")]
    delay_minutes: u32,
}

fn default_webhook_delay_minutes() -> u32 {
    1
}

fn webhook_provider_id(provider: &webhook::WebhookPlatform) -> &'static str {
    match provider {
        webhook::WebhookPlatform::DingTalk => "dingtalk",
        webhook::WebhookPlatform::Feishu => "feishu",
    }
}

fn webhook_provider_name(provider: &webhook::WebhookPlatform) -> &'static str {
    match provider {
        webhook::WebhookPlatform::DingTalk => "DingTalk",
        webhook::WebhookPlatform::Feishu => "Feishu",
    }
}

fn normalize_webhook_secret(secret: Option<String>) -> Option<String> {
    secret
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn webhook_config_from_form(
    provider: webhook::WebhookPlatform,
    form: WebhookFormConfig,
    force_enabled: bool,
) -> webhook::WebhookConfig {
    let WebhookFormConfig {
        enabled,
        url,
        secret,
        events,
        delay_enabled,
        delay_minutes,
    } = form;

    webhook::WebhookConfig {
        id: webhook_provider_id(&provider).to_string(),
        name: webhook_provider_name(&provider).to_string(),
        platform: provider,
        url: url.trim().to_string(),
        secret: normalize_webhook_secret(secret),
        sources: Vec::new(),
        events,
        enabled: force_enabled || enabled,
        delay_enabled,
        delay_minutes: delay_minutes.max(1),
    }
}

fn upsert_provider_webhook_config(
    configs: &mut Vec<webhook::WebhookConfig>,
    config: webhook::WebhookConfig,
) {
    let id = config.id.clone();
    let platform = config.platform.clone();
    configs.retain(|existing| existing.id != id && existing.platform != platform);
    configs.push(config);
}

#[cfg(test)]
mod webhook_config_tests {
    use super::*;

    fn webhook_config(
        id: &str,
        platform: webhook::WebhookPlatform,
        enabled: bool,
    ) -> webhook::WebhookConfig {
        webhook::WebhookConfig {
            id: id.to_string(),
            name: id.to_string(),
            platform,
            url: format!("https://example.com/{id}"),
            secret: None,
            sources: Vec::new(),
            events: Vec::new(),
            enabled,
            delay_enabled: false,
            delay_minutes: 1,
        }
    }

    #[test]
    fn upsert_provider_webhook_config_removes_duplicate_platform_configs() {
        let mut configs = vec![
            webhook_config("legacy-dingtalk", webhook::WebhookPlatform::DingTalk, true),
            webhook_config("feishu", webhook::WebhookPlatform::Feishu, true),
        ];

        upsert_provider_webhook_config(
            &mut configs,
            webhook_config("dingtalk", webhook::WebhookPlatform::DingTalk, false),
        );

        assert_eq!(configs.len(), 2);
        assert!(configs
            .iter()
            .any(|config| config.id == "dingtalk" && !config.enabled));
        assert!(configs.iter().any(|config| config.id == "feishu"));
        assert!(!configs.iter().any(|config| config.id == "legacy-dingtalk"));
    }
}

#[tauri::command]
async fn list_webhooks(
    state: tauri::State<'_, commands::AppState>,
) -> Result<Vec<webhook::WebhookConfig>, String> {
    Ok(state.config_store.get().webhook_configs)
}

#[tauri::command]
async fn add_webhook(
    state: tauri::State<'_, commands::AppState>,
    config: webhook::WebhookConfig,
) -> Result<(), String> {
    let mut cfg = state.config_store.get();
    cfg.webhook_configs.push(config);
    state.config_store.update(cfg)
}

#[tauri::command]
async fn remove_webhook(
    state: tauri::State<'_, commands::AppState>,
    id: String,
) -> Result<(), String> {
    let mut cfg = state.config_store.get();
    cfg.webhook_configs.retain(|w| w.id != id);
    state.config_store.update(cfg)
}

#[tauri::command]
async fn update_webhook(
    state: tauri::State<'_, commands::AppState>,
    config: webhook::WebhookConfig,
) -> Result<(), String> {
    let mut cfg = state.config_store.get();
    let id = config.id.clone();
    match cfg.webhook_configs.iter_mut().find(|w| w.id == id) {
        Some(existing) => {
            *existing = config;
            state.config_store.update(cfg)
        }
        None => Err(format!("Webhook {} not found", id)),
    }
}

#[tauri::command]
async fn save_webhook_config(
    state: tauri::State<'_, commands::AppState>,
    provider: webhook::WebhookPlatform,
    config: WebhookFormConfig,
) -> Result<(), String> {
    let wh = webhook_config_from_form(provider, config, false);
    let mut cfg = state.config_store.get();
    upsert_provider_webhook_config(&mut cfg.webhook_configs, wh);

    state.config_store.update(cfg)
}

#[tauri::command]
async fn test_webhook(
    state: tauri::State<'_, commands::AppState>,
    id: Option<String>,
    provider: Option<webhook::WebhookPlatform>,
    url: Option<String>,
    secret: Option<String>,
) -> Result<String, String> {
    let wh = if let Some(id) = id {
        let cfg = state.config_store.get();
        cfg.webhook_configs
            .iter()
            .find(|w| w.id == id)
            .ok_or_else(|| format!("Webhook {} not found", id))?
            .clone()
    } else {
        let provider = provider.ok_or_else(|| "Webhook provider is required".to_string())?;
        let url = url
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Webhook URL is required".to_string())?;

        webhook::WebhookConfig {
            id: format!("{}-test", webhook_provider_id(&provider)),
            name: format!("{} Test", webhook_provider_name(&provider)),
            platform: provider,
            url,
            secret: normalize_webhook_secret(secret),
            sources: Vec::new(),
            events: Vec::new(),
            enabled: true,
            delay_enabled: false,
            delay_minutes: default_webhook_delay_minutes(),
        }
    };

    let event = webhook::templates::NotificationEvent::Custom {
        title: "AgentBro Test".to_string(),
        body: "This is a test notification from AgentBro.".to_string(),
    };
    let language = state.config_store.get().language;
    let results =
        webhook::WebhookForwarder::send(&[wh], &event, "test", "test-session", &language).await;
    if let Some((_, result)) = results.first() {
        match result {
            webhook::WebhookResult::Success => {
                return Ok("Test notification sent successfully".to_string())
            }
            webhook::WebhookResult::Failed(msg) => return Err(msg.clone()),
            webhook::WebhookResult::Skipped => {
                return Ok("Webhook skipped (disabled or source filter)".to_string())
            }
        }
    }
    Ok("No webhooks matched".to_string())
}

#[tauri::command]
async fn get_webhook_logs(
    state: tauri::State<'_, commands::AppState>,
) -> Result<Vec<hooks::diagnostics::DiagnosticEvent>, String> {
    Ok(state.diagnostic_buffer.query(
        hooks::diagnostics::DiagnosticSeverity::Debug,
        Some("webhook"),
    ))
}

// ── Diagnostic Event Commands ───────────────────────────────────

#[tauri::command]
async fn get_diagnostic_events(
    state: tauri::State<'_, commands::AppState>,
    since_seq: Option<u64>,
    component: Option<String>,
) -> Result<Vec<hooks::diagnostics::DiagnosticEvent>, String> {
    Ok(match since_seq {
        Some(seq) => {
            let mut events = state.diagnostic_buffer.since(seq);
            if let Some(ref comp) = component {
                events.retain(|e| &e.component == comp);
            }
            events
        }
        None => state.diagnostic_buffer.query(
            hooks::diagnostics::DiagnosticSeverity::Debug,
            component.as_deref(),
        ),
    })
}

// ── Theme Commands ──────────────────────────────────────────────

#[tauri::command]
async fn get_themes() -> Result<Vec<serde_json::Value>, String> {
    Ok(theme::scanner::scan_themes())
}

#[tauri::command]
async fn list_themes() -> Result<Vec<serde_json::Value>, String> {
    Ok(theme::scanner::scan_themes())
}

#[tauri::command]
async fn get_active_theme_bundle(name: String) -> Result<serde_json::Value, String> {
    theme::scanner::get_theme_bundle(&name).ok_or_else(|| format!("Theme '{}' not found", name))
}

#[tauri::command]
async fn import_theme(path: String) -> Result<String, String> {
    let src = std::path::Path::new(&path);
    theme::scanner::import_theme_from_path(src)
}

#[tauri::command]
async fn set_active_theme(
    state: tauri::State<'_, commands::AppState>,
    name: String,
) -> Result<(), String> {
    let mut config = state.config_store.get();
    config.theme = name;
    state.config_store.update(config)
}

// ── Suppression Commands ────────────────────────────────────────

#[tauri::command]
async fn should_suppress(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<bool, String> {
    let session = state
        .session_store
        .get_session(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;
    let pid = session.pid.unwrap_or(0);
    if pid == 0 {
        return Ok(false);
    }
    Ok(terminal::suppression::is_terminal_focused(pid))
}

// ── Cursor Hot-zone Commands ────────────────────────────────────

#[tauri::command]
async fn get_cursor_position() -> Result<(f64, f64), String> {
    // Use osascript instead — more reliable, no dependencies
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .args([
                "-e",
                r#"
                use framework "AppKit"
                set mouseLoc to current application's NSEvent's mouseLocation()
                set x to mouseLoc's x as real
                set y to mouseLoc's y as real
                return (x as text) & "," & (y as text)
            "#,
            ])
            .output()
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split(',').collect();
        if parts.len() == 2 {
            let x: f64 = parts[0]
                .parse()
                .map_err(|e: std::num::ParseFloatError| e.to_string())?;
            let y: f64 = parts[1]
                .parse()
                .map_err(|e: std::num::ParseFloatError| e.to_string())?;
            Ok((x, y))
        } else {
            Err("Failed to parse cursor position".to_string())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Cursor position not supported on this platform".to_string())
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogicalRect {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

/// Hit-tests the cursor against a list of logical (CSS px) rects expressed in
/// the target webview's viewport coordinates. Returns true if the cursor is
/// inside any rect; used to drive per-zone click-through.
///
/// `window_label` defaults to "notch" for backward compatibility; callers in
/// the pet webview should pass "pet".
#[tauri::command]
async fn is_cursor_in_window_zones(
    app: tauri::AppHandle,
    zones: Vec<LogicalRect>,
    window_label: Option<String>,
) -> Result<bool, String> {
    if zones.is_empty() {
        return Ok(false);
    }
    let label = window_label.as_deref().unwrap_or("notch");
    let Some(window) = app.get_webview_window(label) else {
        return Ok(false);
    };
    let cursor = app.cursor_position().map_err(|e| e.to_string())?;
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let scale = window.scale_factor().unwrap_or(1.0).max(1.0);
    let cx_logical = (cursor.x - position.x as f64) / scale;
    let cy_logical = (cursor.y - position.y as f64) / scale;

    Ok(zones.iter().any(|r| {
        cx_logical >= r.left
            && cx_logical <= r.left + r.width
            && cy_logical >= r.top
            && cy_logical <= r.top + r.height
    }))
}

#[tauri::command]
async fn is_cursor_over_notch(
    app: tauri::AppHandle,
    width: Option<f64>,
    height: Option<f64>,
    anchor_offset_x: Option<f64>,
) -> Result<bool, String> {
    let Some(window) = app.get_webview_window("notch") else {
        return Ok(false);
    };

    let cursor = app.cursor_position().map_err(|e| e.to_string())?;
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let hit_width = width
        .filter(|value| *value > 0.0)
        .map(|value| value * scale)
        .unwrap_or(size.width as f64);
    let hit_height = height
        .filter(|value| *value > 0.0)
        .map(|value| value * scale)
        .unwrap_or(size.height as f64);

    let anchor_offset_x = anchor_offset_x.unwrap_or(0.0) * scale;
    let left =
        position.x as f64 + ((size.width as f64 - hit_width) / 2.0).max(0.0) + anchor_offset_x;
    let top = position.y as f64;
    let right = left + hit_width.min(size.width as f64);
    let bottom = top + hit_height.min(size.height as f64);

    Ok(cursor.x >= left && cursor.x <= right && cursor.y >= top && cursor.y <= bottom)
}

#[cfg(target_os = "macos")]
fn get_cursor_position_sync() -> Result<(f64, f64), String> {
    use objc2_app_kit::NSEvent;
    let point = NSEvent::mouseLocation();
    Ok((point.x, point.y))
}

#[cfg(not(target_os = "macos"))]
fn get_cursor_position_sync() -> Result<(f64, f64), String> {
    Err("Cursor position not supported on this platform".to_string())
}

// ── Path Validation Command ─────────────────────────────────────

#[tauri::command]
async fn validate_path(path: String) -> Result<bool, String> {
    Ok(std::path::Path::new(&path).exists())
}

// ── Global Shortcut Commands ────────────────────────────────────

fn toggle_notch_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("notch") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            configure_notch_window_for_spaces(app);
            let _ = window.set_focus();
        }
    }
}

fn show_notch_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("notch") {
        let _ = window.show();
        configure_notch_window_for_spaces(app);
        let _ = window.set_focus();
        let _ = app.emit("tray-open-agentbro", ());
    }
}

fn menu_bar_icon() -> tauri::image::Image<'static> {
    tauri::image::Image::from_bytes(include_bytes!("../icons/tray-ink-amber.png"))
        .expect("embedded AgentBro tray icon must be a valid PNG")
        .to_owned()
}

fn first_pending_permission(store: &SessionStore) -> Option<SessionState> {
    let mut sessions: Vec<_> = store
        .get_all_sessions()
        .into_iter()
        .filter(|session| session.pending_permission.is_some())
        .collect();
    sessions.sort_by_key(|session| session.started_at);
    sessions.into_iter().next()
}

fn first_pending_question(store: &SessionStore) -> Option<SessionState> {
    let mut sessions: Vec<_> = store
        .get_all_sessions()
        .into_iter()
        .filter(|session| session.pending_question.is_some())
        .collect();
    sessions.sort_by_key(|session| session.started_at);
    sessions.into_iter().next()
}

fn handle_permission_shortcut(app: tauri::AppHandle, allowed: bool) {
    let state = app.state::<AppState>();
    let Some(session) = first_pending_permission(&state.session_store) else {
        return;
    };
    let session_id = session.id.clone();
    let hook_server = state.hook_server.clone();
    let session_store = state.session_store.clone();

    tauri::async_runtime::spawn(async move {
        let hook_result = hook_server
            .respond_permission(&session_id, allowed, false)
            .await;
        if let Err(err) = hook_result {
            log::warn!(
                "Global permission shortcut hook response failed for {}: {}. Falling back to tmux.",
                session_id,
                err
            );
            let fallback_result = (|| -> Result<(), String> {
                let pid = session
                    .pid
                    .ok_or_else(|| "Session has no PID for tmux fallback".to_string())?;
                let tmux_target = crate::terminal::approval::resolve_tmux_target(pid)
                    .ok_or_else(|| "Could not find tmux pane for session".to_string())?;
                if allowed {
                    crate::terminal::approval::approve_once(&tmux_target)
                        .map_err(|e| e.to_string())?;
                } else {
                    crate::terminal::approval::reject(
                        &tmux_target,
                        Some("Denied via keyboard shortcut"),
                    )
                    .map_err(|e| e.to_string())?;
                }
                Ok(())
            })();
            if let Err(fallback_err) = fallback_result {
                log::warn!(
                    "Global permission shortcut tmux fallback failed for {}: {}",
                    session_id,
                    fallback_err
                );
                return;
            }
        }
        session_store.set_pending_permission(&session_id, None);
        session_store.update_phase(&session_id, SessionPhase::Processing);
    });
}

fn handle_question_skip_shortcut(app: tauri::AppHandle) {
    let state = app.state::<AppState>();
    let Some(session) = first_pending_question(&state.session_store) else {
        return;
    };
    let session_id = session.id.clone();
    let answer = session
        .pending_question
        .as_ref()
        .and_then(|question| question.options.first().cloned())
        .unwrap_or_default();
    let hook_server = state.hook_server.clone();
    let session_store = state.session_store.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(err) = hook_server.respond_question(&session_id, answer).await {
            log::warn!(
                "Global question skip shortcut failed for {}: {}",
                session_id,
                err
            );
            return;
        }
        session_store.set_pending_question(&session_id, None);
        session_store.update_phase(&session_id, SessionPhase::Processing);
    });
}

fn register_one_global_shortcut<F>(
    app: &tauri::AppHandle,
    accelerator: &str,
    action: F,
) -> Result<(), String>
where
    F: Fn(tauri::AppHandle) + Send + Sync + 'static,
{
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let accelerator = accelerator.trim();
    if accelerator.is_empty() {
        return Ok(());
    }
    let app_handle = app.clone();
    app.global_shortcut()
        .on_shortcut(accelerator, move |_app, _shortcut, _event| {
            action(app_handle.clone());
        })
        .map_err(|err| format!("Failed to register global shortcut {accelerator}: {err}"))
}

fn register_island_global_shortcuts_for_config(
    app: &tauri::AppHandle,
    config: &crate::config::AppConfig,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    app.global_shortcut()
        .unregister_all()
        .map_err(|e| format!("{e}"))?;

    register_one_global_shortcut(app, &config.global_shortcut, |app| {
        toggle_notch_window(&app)
    })?;
    if config.shortcut_approve_enabled {
        register_one_global_shortcut(app, &config.shortcut_approve, |app| {
            handle_permission_shortcut(app, true)
        })?;
    }
    if config.shortcut_deny_enabled {
        register_one_global_shortcut(app, &config.shortcut_deny, |app| {
            handle_permission_shortcut(app, false)
        })?;
    }
    if config.shortcut_skip_enabled {
        register_one_global_shortcut(app, &config.shortcut_skip, handle_question_skip_shortcut)?;
    }
    Ok(())
}

fn register_island_global_shortcuts(app: &tauri::AppHandle) -> Result<(), String> {
    let config = app.state::<AppState>().config_store.get();
    register_island_global_shortcuts_for_config(app, &config)
}

#[tauri::command]
async fn register_global_shortcut(app: tauri::AppHandle, shortcut: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let previous = state.config_store.get();
    let mut next = previous.clone();
    next.global_shortcut = shortcut;
    if let Err(err) = register_island_global_shortcuts_for_config(&app, &next) {
        let _ = register_island_global_shortcuts_for_config(&app, &previous);
        return Err(err);
    }
    state.config_store.update(next)
}

#[tauri::command]
async fn unregister_global_shortcut(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlobalActionShortcuts {
    approve: String,
    approve_enabled: bool,
    deny: String,
    deny_enabled: bool,
    skip: String,
    skip_enabled: bool,
}

#[tauri::command]
async fn set_global_action_shortcuts(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    shortcuts: GlobalActionShortcuts,
) -> Result<(), String> {
    let previous = state.config_store.get();
    let mut next = previous.clone();
    next.shortcut_approve = shortcuts.approve;
    next.shortcut_approve_enabled = shortcuts.approve_enabled;
    next.shortcut_deny = shortcuts.deny;
    next.shortcut_deny_enabled = shortcuts.deny_enabled;
    next.shortcut_skip = shortcuts.skip;
    next.shortcut_skip_enabled = shortcuts.skip_enabled;
    if let Err(err) = register_island_global_shortcuts_for_config(&app, &next) {
        let _ = register_island_global_shortcuts_for_config(&app, &previous);
        return Err(err);
    }
    state.config_store.update(next)
}

// ── Quit Command ────────────────────────────────────────────────

#[tauri::command]
async fn quit_app(
    app: tauri::AppHandle,
    state: tauri::State<'_, commands::AppState>,
) -> Result<(), String> {
    let config = state.config_store.get();
    state
        .telemetry
        .upload_pending_daily_usage_snapshots(&config)
        .await;
    app.exit(0);
    Ok(())
}

#[tauri::command]
#[cfg(target_os = "macos")]
fn set_dock_visible(app: tauri::AppHandle, visible: bool) {
    let policy = if visible {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    };
    let _ = app.set_activation_policy(policy);
    if visible {
        // Delay icon reset by one runloop cycle so macOS applies the policy first
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = app.run_on_main_thread(|| unsafe {
                let cls = objc2::runtime::AnyClass::get("NSApplication").unwrap();
                let ns_app: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![cls, sharedApplication];
                let _: () = objc2::msg_send![ns_app, setApplicationIconImage: std::ptr::null::<objc2::runtime::AnyObject>()];
            });
        });
    } else {
        if let Some(notch) = app.get_webview_window("notch") {
            let _ = notch.show();
        }
    }
}

#[tauri::command]
#[cfg(not(target_os = "macos"))]
fn set_dock_visible(_app: tauri::AppHandle, _visible: bool) {}

#[cfg(target_os = "macos")]
fn activate_agentbro_app() {
    unsafe {
        let cls = objc2::runtime::AnyClass::get("NSApplication").unwrap();
        let ns_app: *mut objc2::runtime::AnyObject = objc2::msg_send![cls, sharedApplication];
        let _: () = objc2::msg_send![ns_app, activateIgnoringOtherApps: objc2::runtime::Bool::YES];
    }
}

#[cfg(target_os = "macos")]
fn apply_settings_window_for_spaces(window: &tauri::WebviewWindow) {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};

    let _ = window.set_visible_on_all_workspaces(true);
    if let Ok(ptr) = window.ns_window() {
        unsafe {
            let ns_window = ptr as *const NSWindow;
            let mut behavior = (*ns_window).collectionBehavior();

            behavior &= !(NSWindowCollectionBehavior::Primary
                | NSWindowCollectionBehavior::Auxiliary
                | NSWindowCollectionBehavior::Managed
                | NSWindowCollectionBehavior::Transient
                | NSWindowCollectionBehavior::MoveToActiveSpace
                | NSWindowCollectionBehavior::FullScreenPrimary
                | NSWindowCollectionBehavior::FullScreenNone
                | NSWindowCollectionBehavior::FullScreenAllowsTiling
                | NSWindowCollectionBehavior::FullScreenDisallowsTiling);
            behavior |= NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::CanJoinAllApplications
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::ParticipatesInCycle;
            (*ns_window).setCollectionBehavior(behavior);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn apply_settings_window_for_spaces(_window: &tauri::WebviewWindow) {}

#[cfg(target_os = "macos")]
fn focus_settings_window_native(window: &tauri::WebviewWindow) {
    use objc2_app_kit::NSWindow;

    if let Ok(ptr) = window.ns_window() {
        unsafe {
            let ns_window = ptr as *const NSWindow;
            (*ns_window).makeKeyAndOrderFront(None);
            (*ns_window).orderFrontRegardless();
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn focus_settings_window_native(_window: &tauri::WebviewWindow) {}

const SETTINGS_MIN_WIDTH: f64 = 980.0;
const SETTINGS_MIN_HEIGHT: f64 = 640.0;
const SETTINGS_DEFAULT_WIDTH: f64 = 1280.0;
const SETTINGS_DEFAULT_HEIGHT: f64 = 840.0;

fn normalize_settings_window_frame(window: &tauri::WebviewWindow) {
    let min_size = tauri::Size::Logical(tauri::LogicalSize::new(
        SETTINGS_MIN_WIDTH,
        SETTINGS_MIN_HEIGHT,
    ));
    let _ = window.set_min_size(Some(min_size));

    let should_restore_size = window
        .outer_size()
        .ok()
        .and_then(|size| {
            window.scale_factor().ok().map(|scale| {
                let width = size.width as f64 / scale;
                let height = size.height as f64 / scale;
                width < SETTINGS_MIN_WIDTH || height < SETTINGS_MIN_HEIGHT
            })
        })
        .unwrap_or(true);

    if should_restore_size {
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
            SETTINGS_DEFAULT_WIDTH,
            SETTINGS_DEFAULT_HEIGHT,
        )));
        let _ = window.center();
    }
}

fn show_settings_window(app: &tauri::AppHandle) -> Result<(), String> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        {
            let _ = handle.set_activation_policy(tauri::ActivationPolicy::Regular);
        }

        if let Some(window) = handle.get_webview_window("settings") {
            normalize_settings_window_frame(&window);
            apply_settings_window_for_spaces(&window);
            let _ = window.show();
            let _ = window.set_focus();

            #[cfg(target_os = "macos")]
            activate_agentbro_app();
            focus_settings_window_native(&window);
        }
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    show_settings_window(&app)
}

#[tauri::command]
async fn restart_app(app: tauri::AppHandle) -> Result<(), String> {
    app.request_restart();
    Ok(())
}

// ── Sound Commands ───────────────────────────────────────────────

fn custom_sounds_dir() -> PathBuf {
    let base = dirs::data_dir()
        .or_else(dirs::config_dir)
        .unwrap_or_else(std::env::temp_dir);
    base.join("agentbro").join("sounds")
}

fn supported_audio_extension(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp3") => Some("mp3"),
        Some("wav") => Some("wav"),
        Some("ogg") => Some("ogg"),
        Some("flac") => Some("flac"),
        _ => None,
    }
}

#[tauri::command]
async fn play_sound(state: tauri::State<'_, AppState>, event: String) -> Result<(), String> {
    let event =
        SoundEvent::from_id(&event).ok_or_else(|| format!("Unknown sound event: {event}"))?;
    if let Some(ref engine) = state.sound_engine {
        engine.play(event);
    }
    Ok(())
}

#[tauri::command]
async fn preview_sound(
    state: tauri::State<'_, AppState>,
    event: String,
    sound: String,
) -> Result<(), String> {
    let event =
        SoundEvent::from_id(&event).ok_or_else(|| format!("Unknown sound event: {event}"))?;
    if let Some(ref engine) = state.sound_engine {
        engine.preview(event, sound);
    }
    Ok(())
}

#[tauri::command]
async fn set_sound_volume(state: tauri::State<'_, AppState>, volume: f32) -> Result<(), String> {
    let normalized = if volume > 1.0 { volume / 100.0 } else { volume }.clamp(0.0, 1.0);
    if let Some(ref engine) = state.sound_engine {
        engine.set_volume(normalized);
    }
    let mut config = state.config_store.get();
    config.sound_volume = normalized;
    config.volume = (normalized * 100.0).round() as u8;
    state.config_store.update(config)?;
    Ok(())
}

#[tauri::command]
async fn set_sound_enabled(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    if let Some(ref engine) = state.sound_engine {
        engine.set_enabled(enabled);
    }
    let mut config = state.config_store.get();
    config.sound_enabled = enabled;
    state.config_store.update(config)?;
    Ok(())
}

#[tauri::command]
async fn set_sound_pack(state: tauri::State<'_, AppState>, pack: String) -> Result<(), String> {
    let sound_pack =
        SoundPack::from_id(&pack).ok_or_else(|| format!("Unknown sound pack: {pack}"))?;
    if let Some(ref engine) = state.sound_engine {
        engine.set_sound_pack(sound_pack);
    }
    let mut config = state.config_store.get();
    config.sound_pack = sound_pack.to_string();
    state.config_store.update(config)?;
    Ok(())
}

#[tauri::command]
async fn set_probe_session_filter(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    if let Some(ref engine) = state.sound_engine {
        engine.set_probe_filter(enabled);
    }
    let mut config = state.config_store.get();
    config.probe_session_filter = enabled;
    state.config_store.update(config)?;
    Ok(())
}

#[tauri::command]
async fn set_sound_quiet_hours(
    state: tauri::State<'_, AppState>,
    enabled: bool,
    start: String,
    end: String,
) -> Result<(), String> {
    if let Some(ref engine) = state.sound_engine {
        engine.set_quiet_hours(enabled, start.clone(), end.clone());
    }
    let mut config = state.config_store.get();
    config.quiet_hours_enabled = enabled;
    config.quiet_hours_start = start;
    config.quiet_hours_end = end;
    state.config_store.update(config)?;
    Ok(())
}

#[tauri::command]
async fn set_sound_event_enabled(
    state: tauri::State<'_, AppState>,
    event_id: String,
    enabled: bool,
) -> Result<(), String> {
    let event =
        SoundEvent::from_id(&event_id).ok_or_else(|| format!("Unknown sound event: {event_id}"))?;
    if let Some(ref engine) = state.sound_engine {
        engine.set_event_enabled(event, enabled);
    }
    let mut config = state.config_store.get();
    config.sound_events.insert(event_id, enabled);
    state.config_store.update(config)?;
    Ok(())
}

#[tauri::command]
async fn set_sound_event_rule(
    state: tauri::State<'_, AppState>,
    event_id: String,
    enabled: bool,
    sound: String,
) -> Result<(), String> {
    let event =
        SoundEvent::from_id(&event_id).ok_or_else(|| format!("Unknown sound event: {event_id}"))?;
    if let Some(ref engine) = state.sound_engine {
        engine.set_event_rule(event, enabled, sound.clone());
    }
    let mut config = state.config_store.get();
    config.sound_events.insert(event_id.clone(), enabled);
    config
        .sound_rules
        .insert(event_id, SoundRuleConfig { enabled, sound });
    state.config_store.update(config)?;
    Ok(())
}

#[tauri::command]
async fn import_custom_sound(
    state: tauri::State<'_, AppState>,
    file_path: String,
) -> Result<CustomSoundConfig, String> {
    let source = PathBuf::from(&file_path);
    if !source.exists() {
        return Err("Sound file not found".to_string());
    }
    if !source.is_file() {
        return Err("Sound path is not a file".to_string());
    }
    let ext = supported_audio_extension(&source)
        .ok_or_else(|| "Unsupported sound file type. Use mp3, wav, ogg, or flac.".to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let dest_dir = custom_sounds_dir();
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("Failed to create sounds dir: {e}"))?;
    let dest = dest_dir.join(format!("{id}.{ext}"));
    std::fs::copy(&source, &dest).map_err(|e| format!("Failed to import sound: {e}"))?;

    let name = source
        .file_stem()
        .and_then(|name| name.to_str())
        .map(|name| name.replace(['-', '_'], " "))
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Custom Sound".to_string());
    let sound = CustomSoundConfig {
        id,
        name,
        path: dest.to_string_lossy().to_string(),
        data_url: None,
    };

    if let Some(ref engine) = state.sound_engine {
        let mut sounds = state.config_store.get().custom_sounds;
        sounds.push(sound.clone());
        engine.set_custom_sounds(
            sounds
                .iter()
                .map(|sound| (sound.id.clone(), sound.path.clone()))
                .collect(),
        );
    }

    Ok(sound)
}

#[tauri::command]
async fn set_custom_sounds(
    state: tauri::State<'_, AppState>,
    sounds: Vec<CustomSoundConfig>,
) -> Result<(), String> {
    if let Some(ref engine) = state.sound_engine {
        engine.set_custom_sounds(
            sounds
                .iter()
                .map(|sound| (sound.id.clone(), sound.path.clone()))
                .collect(),
        );
    }
    let mut config = state.config_store.get();
    config.custom_sounds = sounds;
    state.config_store.update(config)?;
    Ok(())
}

// ── Haptic Feedback Command ─────────────────────────────────

#[tauri::command]
async fn perform_haptic(intensity: u8) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let pattern = match intensity {
            1 => "1",
            2 => "3",
            _ => "6",
        };
        let script = format!(
            r#"use framework "AppKit"
            set mgr to current application's NSHapticFeedbackManager's defaultPerformer()
            mgr's performFeedbackPattern:{} performanceTime:0"#,
            pattern
        );
        std::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = intensity;
        Ok(())
    }
}

// ── Skill Management Commands ───────────────────────────────────

#[tauri::command]
async fn scan_all_skills(
) -> Result<std::collections::HashMap<String, Vec<skills::ScannedSkill>>, String> {
    Ok(skills::scanner::scan_all())
}

#[tauri::command]
async fn scan_agent_skills(agent: String) -> Result<Vec<skills::ScannedSkill>, String> {
    Ok(skills::scanner::scan_agent(&agent))
}

#[tauri::command]
async fn get_central_skill_bundles() -> Result<Vec<skills::CentralSkillBundle>, String> {
    let scan = skills::scanner::scan_all();
    let mut linked_by_id: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (agent, agent_skills) in &scan {
        if agent == "central" {
            continue;
        }
        for skill in agent_skills {
            *linked_by_id.entry(skill.id.clone()).or_default() += 1;
        }
    }

    let mut grouped: std::collections::HashMap<String, skills::CentralSkillBundle> =
        std::collections::HashMap::new();
    for skill in scan.get("central").into_iter().flatten() {
        let Some(path) = central_skill_path(skill) else {
            continue;
        };
        let name = central_bundle_name_for_path(&path);
        let entry = grouped
            .entry(name.clone())
            .or_insert_with(|| skills::CentralSkillBundle {
                name: name.clone(),
                path: central_bundle_path_for_path(&path),
                skill_count: 0,
                linked_agent_count: 0,
                skill_ids: Vec::new(),
            });
        entry.skill_count += 1;
        entry.linked_agent_count += linked_by_id.get(&skill.id).copied().unwrap_or(0);
        if !entry.skill_ids.iter().any(|id| id == &skill.id) {
            entry.skill_ids.push(skill.id.clone());
        }
    }

    let mut bundles = grouped.into_values().collect::<Vec<_>>();
    for bundle in &mut bundles {
        bundle.skill_ids.sort();
    }
    bundles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(bundles)
}

#[tauri::command]
async fn get_central_skill_bundle_detail(
    bundle_name: String,
) -> Result<Vec<skills::ScannedSkill>, String> {
    Ok(skills::scanner::scan_agent("central")
        .into_iter()
        .filter(|skill| {
            central_skill_path(skill)
                .map(|path| central_bundle_matches(&path, &bundle_name))
                .unwrap_or(false)
        })
        .collect())
}

#[tauri::command]
async fn preview_delete_central_skill_bundle(
    bundle_name: String,
) -> Result<skills::CentralDeletePreview, String> {
    central_delete_preview(|path| central_bundle_matches(path, &bundle_name))
}

#[tauri::command]
async fn delete_central_skill_bundle(
    bundle_name: String,
    remove_linked: Option<bool>,
) -> Result<(), String> {
    let preview = central_delete_preview(|path| central_bundle_matches(path, &bundle_name))?;
    delete_central_paths(preview, remove_linked.unwrap_or(false))
}

#[tauri::command]
async fn preview_delete_central_skill(
    skill_path: String,
) -> Result<skills::CentralDeletePreview, String> {
    let target = PathBuf::from(skill_path);
    central_delete_preview(|path| path == target)
}

#[tauri::command]
async fn delete_central_skill(
    skill_path: String,
    remove_linked: Option<bool>,
) -> Result<(), String> {
    let preview = preview_delete_central_skill(skill_path).await?;
    delete_central_paths(preview, remove_linked.unwrap_or(false))
}

#[tauri::command]
async fn discover_project_skills_cmd(
    roots: Vec<String>,
) -> Result<Vec<skills::DiscoveredSkill>, String> {
    let discovered = skills::scanner::discover_project_skills(&roots);
    skills::registry::cache_discovered_skills(discovered)
}

#[tauri::command]
async fn discover_enabled_project_skills_cmd() -> Result<Vec<skills::DiscoveredSkill>, String> {
    let discovered = skills::scanner::discover_project_skills_from_scan_roots();
    skills::registry::cache_discovered_skills(discovered)
}

#[tauri::command]
async fn get_discovered_skills_cmd() -> Result<Vec<skills::DiscoveredSkill>, String> {
    Ok(skills::registry::list_discovered_skills())
}

#[tauri::command]
async fn clear_discovered_skills_cmd() -> Result<(), String> {
    skills::registry::clear_discovered_skills()
}

#[tauri::command]
async fn stop_project_scan() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
async fn get_scan_roots_cmd() -> Result<Vec<skills::ScanRoot>, String> {
    Ok(skills::registry::list_scan_roots())
}

#[tauri::command]
async fn set_scan_roots_cmd(roots: Vec<skills::ScanRoot>) -> Result<(), String> {
    skills::registry::set_scan_roots(roots)
}

#[tauri::command]
async fn set_scan_root_enabled_cmd(path: String, enabled: bool) -> Result<(), String> {
    skills::registry::set_scan_root_enabled(&path, enabled)
}

#[tauri::command]
async fn get_obsidian_vaults_cmd() -> Result<Vec<skills::ObsidianVault>, String> {
    Ok(skills::scanner::get_obsidian_vaults())
}

#[tauri::command]
async fn get_obsidian_vault_skills_cmd(
    vault_path: String,
) -> Result<Vec<skills::DiscoveredSkill>, String> {
    Ok(skills::scanner::get_obsidian_vault_skills(&vault_path))
}

#[tauri::command]
async fn install_skill_cmd(
    source: String,
    targets: Vec<skills::TargetConfig>,
    mode: skills::InstallMode,
) -> Result<(), String> {
    for skill_id in skills::installer::install_skill(&source, &targets, &mode)? {
        skills::registry::add_source(&skill_id, &source)?;
    }
    Ok(())
}

#[tauri::command]
async fn batch_import_discovered_skills_cmd(
    skills_to_import: Vec<skills::DiscoveredSkill>,
    target_agents: Vec<String>,
    mode: skills::InstallMode,
) -> Result<Vec<String>, String> {
    let targets = target_agents
        .into_iter()
        .map(|agent| skills::TargetConfig {
            agent,
            install_mode: mode.clone(),
        })
        .collect::<Vec<_>>();
    let mut imported = Vec::new();
    for skill in skills_to_import {
        for installed_id in skills::installer::install_skill(&skill.dir_path, &targets, &mode)? {
            skills::registry::add_source(&installed_id, &skill.dir_path)?;
            imported.push(installed_id);
        }
    }
    imported.sort();
    imported.dedup();
    Ok(imported)
}

#[tauri::command]
async fn preview_github_skills_cmd(
    source: String,
) -> Result<Vec<skills::GitHubSkillPreview>, String> {
    skills::installer::preview_github_skills(&source)
}

#[tauri::command]
async fn preview_github_repo_import(repo_url: String) -> Result<serde_json::Value, String> {
    let repo = normalize_github_repo_ref(&repo_url)?;
    let previews = skills::installer::preview_github_skills(&repo.spec)?;
    let installed_ids = skills::scanner::scan_agent("central")
        .into_iter()
        .map(|skill| skill.id)
        .collect::<std::collections::HashSet<_>>();
    let skills = previews
        .into_iter()
        .map(|preview| {
            let skill_id = preview.name.clone();
            let skill_name = preview.name.clone();
            let conflict_skill_id = skill_id.clone();
            let conflict_skill_name = skill_name.clone();
            let conflict = installed_ids.contains(&skill_id).then(|| {
                serde_json::json!({
                    "existingSkillId": conflict_skill_id.clone(),
                    "existingName": conflict_skill_name.clone(),
                    "existingCanonicalPath": null,
                    "proposedSkillId": conflict_skill_id.clone(),
                    "proposedName": conflict_skill_name.clone(),
                })
            });
            serde_json::json!({
                "sourcePath": preview.source_path,
                "skillId": skill_id,
                "skillName": preview.name,
                "description": preview.description,
                "rootDirectory": repo.repo.clone(),
                "skillDirectoryName": preview.directory_name,
                "downloadUrl": github_import_source(&repo.spec, &preview.source_path),
                "conflict": conflict,
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "repo": {
            "owner": repo.owner,
            "repo": repo.repo,
            "branch": repo.branch,
            "normalizedUrl": repo.normalized_url,
        },
        "skills": skills,
    }))
}

#[tauri::command]
async fn import_github_repo_skills(
    repo_url: String,
    selections: Vec<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let repo = normalize_github_repo_ref(&repo_url)?;
    let targets = vec![skills::TargetConfig {
        agent: "central".to_string(),
        install_mode: skills::InstallMode::Direct,
    }];
    let mut imported = Vec::new();
    let mut skipped = Vec::new();
    for selection in selections {
        let source_path = selection
            .get("sourcePath")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let resolution = selection
            .get("resolution")
            .and_then(|value| value.as_str())
            .unwrap_or("overwrite");
        if resolution == "skip" {
            skipped.push(source_path);
            continue;
        }
        let rename_to = selection
            .get("renamedSkillId")
            .or_else(|| selection.get("renamed_skill_id"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let source = github_import_source(&repo.spec, &source_path);
        let installed_ids = skills::installer::install_skill_named(
            &source,
            &targets,
            &skills::InstallMode::Direct,
            rename_to,
            rename_to,
        )?;
        for installed_id in installed_ids {
            skills::registry::add_source(&installed_id, &source)?;
            imported.push(serde_json::json!({
                "sourcePath": source_path,
                "originalSkillId": installed_id,
                "importedSkillId": installed_id,
                "skillName": installed_id,
                "targetDirectory": skills::agent_paths::agentbro_skills_dir()
                    .join(
                        rename_to
                            .map(sanitize_skill_directory_name)
                            .unwrap_or_else(|| sanitize_skill_directory_name(&installed_id)),
                    )
                    .display()
                    .to_string(),
                "resolution": resolution,
            }));
        }
    }
    Ok(serde_json::json!({
        "repo": {
            "owner": repo.owner,
            "repo": repo.repo,
            "branch": repo.branch,
            "normalizedUrl": repo.normalized_url,
        },
        "importedSkills": imported,
        "skippedSkills": skipped,
    }))
}

struct GithubRepoRefCompat {
    owner: String,
    repo: String,
    branch: String,
    normalized_url: String,
    spec: String,
}

fn normalize_github_repo_ref(input: &str) -> Result<GithubRepoRefCompat, String> {
    let trimmed = input.trim().trim_end_matches('/');
    let without_scheme = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .or_else(|| trimmed.strip_prefix("github:"))
        .unwrap_or(trimmed);
    let parts = without_scheme
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err("GitHub repo must be owner/repo or a github.com URL".to_string());
    }
    let owner = parts[0].to_string();
    let repo = parts[1].trim_end_matches(".git").to_string();
    let tree_branch = (parts.len() >= 4 && parts[2] == "tree").then_some(parts[3]);
    let branch = tree_branch.unwrap_or("HEAD").to_string();
    let spec = if parts.len() >= 5 && parts[2] == "tree" {
        format!("{owner}/{repo}/tree/{branch}/{}", parts[4..].join("/"))
    } else if parts.len() == 4 && parts[2] == "tree" {
        format!("{owner}/{repo}/tree/{branch}")
    } else if parts.len() > 2 && parts[2] != "tree" {
        format!("{owner}/{repo}/{}", parts[2..].join("/"))
    } else {
        format!("{owner}/{repo}")
    };
    Ok(GithubRepoRefCompat {
        owner: owner.clone(),
        repo: repo.clone(),
        branch,
        normalized_url: format!("https://github.com/{owner}/{repo}"),
        spec,
    })
}

fn github_import_source(repo_spec: &str, source_path: &str) -> String {
    let path = source_path.trim().trim_matches('/');
    if path.is_empty() || path == "." {
        format!("github:{repo_spec}")
    } else {
        format!("github:{repo_spec}/{path}")
    }
}

fn sanitize_skill_directory_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "downloaded-skill".to_string()
    } else {
        sanitized
    }
}

fn central_skill_path(skill: &skills::ScannedSkill) -> Option<PathBuf> {
    skill
        .agents
        .iter()
        .find(|agent| {
            agent.agent == "central"
                || agent.install_path.contains("/.agents/skills/")
                || agent.install_path.contains("/.agentbro/skills/")
        })
        .map(|agent| PathBuf::from(&agent.install_path))
        .or_else(|| {
            (skill.file_path.contains("/.agents/skills/")
                || skill.file_path.contains("/.agentbro/skills/"))
            .then(|| PathBuf::from(&skill.file_path))
        })
}

fn central_bundle_name_for_path(path: &Path) -> String {
    let value = path.display().to_string();
    let marker = if value.contains("/.agents/skills/") {
        "/.agents/skills/"
    } else {
        "/.agentbro/skills/"
    };
    value
        .split(marker)
        .nth(1)
        .and_then(|rest| rest.split('/').find(|part| !part.is_empty()))
        .unwrap_or("root")
        .to_string()
}

fn central_bundle_path_for_path(path: &Path) -> String {
    let value = path.display().to_string();
    let marker = if value.contains("/.agents/skills/") {
        "/.agents/skills/"
    } else {
        "/.agentbro/skills/"
    };
    let Some((root, rest)) = value.split_once(marker) else {
        return value;
    };
    let Some(first) = rest.split('/').find(|part| !part.is_empty()) else {
        return value;
    };
    format!("{root}{marker}{first}")
}

fn central_bundle_matches(path: &Path, bundle_name: &str) -> bool {
    let query = bundle_name.trim();
    if query.is_empty() {
        return false;
    }
    central_bundle_name_for_path(path) == query || central_bundle_path_for_path(path) == query
}

fn is_central_path(path: &Path) -> bool {
    skills::agent_paths::central_skill_dirs()
        .into_iter()
        .any(|root| path.starts_with(root))
}

fn central_delete_preview<F>(matches_path: F) -> Result<skills::CentralDeletePreview, String>
where
    F: Fn(&Path) -> bool,
{
    let scan = skills::scanner::scan_all();
    let central_skills = scan
        .get("central")
        .into_iter()
        .flatten()
        .filter_map(|skill| {
            let path = central_skill_path(skill)?;
            (is_central_path(&path) && matches_path(&path)).then(|| (skill.clone(), path))
        })
        .collect::<Vec<_>>();
    if central_skills.is_empty() {
        return Err("Central skill or bundle not found".to_string());
    }

    let skill_ids = central_skills
        .iter()
        .map(|(skill, _)| skill.id.clone())
        .collect::<std::collections::HashSet<_>>();
    let central_paths = central_skills
        .iter()
        .map(|(_, path)| path.display().to_string())
        .collect::<std::collections::HashSet<_>>();

    let mut linked_install_paths = Vec::new();
    for (agent, agent_skills) in &scan {
        if agent == "central" {
            continue;
        }
        for skill in agent_skills {
            if !skill_ids.contains(&skill.id) {
                continue;
            }
            for state in &skill.agents {
                if state.agent == "central" || central_paths.contains(&state.install_path) {
                    continue;
                }
                linked_install_paths.push(state.install_path.clone());
            }
        }
    }
    linked_install_paths.sort();
    linked_install_paths.dedup();

    let mut removable_paths = central_paths.into_iter().collect::<Vec<_>>();
    removable_paths.sort();
    let mut ids = skill_ids.into_iter().collect::<Vec<_>>();
    ids.sort();

    Ok(skills::CentralDeletePreview {
        path: removable_paths.first().cloned().unwrap_or_default(),
        skill_ids: ids,
        linked_install_paths,
        removable_paths,
        warnings: Vec::new(),
    })
}

fn delete_central_paths(
    preview: skills::CentralDeletePreview,
    remove_linked: bool,
) -> Result<(), String> {
    let mut paths = preview.removable_paths.clone();
    if remove_linked {
        paths.extend(preview.linked_install_paths.clone());
    }
    paths.sort();
    paths.dedup();
    for path in paths {
        skills::installer::uninstall_skill(&path)?;
    }
    for skill_id in preview.skill_ids {
        skills::registry::remove_source(&skill_id)?;
    }
    Ok(())
}

#[tauri::command]
async fn install_plugin_cmd(request: skills::PluginInstallRequest) -> Result<String, String> {
    skills::installer::install_plugin(&request)
}

#[tauri::command]
async fn uninstall_skill_cmd(skill_path: String) -> Result<(), String> {
    skills::installer::uninstall_skill(&skill_path)
}

#[tauri::command]
async fn upsert_mcp_server_cmd(
    agent: String,
    server: skills::McpServerConfig,
) -> Result<(), String> {
    skills::installer::upsert_mcp_server(&agent, &server)
}

#[tauri::command]
async fn remove_mcp_server_cmd(agent: String, server_name: String) -> Result<(), String> {
    skills::installer::remove_mcp_server(&agent, &server_name)
}

#[tauri::command]
async fn validate_mcp_server_cmd(
    agent: String,
    server_name: String,
) -> Result<skills::McpValidationResult, String> {
    skills::installer::validate_mcp_server(&agent, &server_name)
}

#[tauri::command]
async fn toggle_skill_cmd(skill_id: String, agent: String, enabled: bool) -> Result<(), String> {
    skills::installer::toggle_skill(&skill_id, &agent, enabled)
}

#[tauri::command]
async fn read_skill_files(skill_path: String) -> Result<skills::FileTreeNode, String> {
    if !skills::agent_paths::is_within_skill_roots(std::path::Path::new(&skill_path)) {
        return Err("Path is outside the skills directories".to_string());
    }
    Ok(skills::scanner::read_file_tree(&skill_path))
}

#[tauri::command]
async fn read_skill_file_content(file_path: String) -> Result<String, String> {
    if !skills::agent_paths::is_within_skill_roots(std::path::Path::new(&file_path)) {
        return Err("Path is outside the skills directories".to_string());
    }
    std::fs::read_to_string(&file_path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_skill_explanation_cmd(
    skill_id: String,
    lang: String,
) -> Result<Option<skills::explanation::SkillExplanation>, String> {
    Ok(skills::explanation::get_cached(&skill_id, &lang))
}

#[tauri::command]
async fn generate_skill_explanation_cmd(
    skill_id: String,
    skill_path: String,
    lang: String,
    refresh: bool,
) -> Result<skills::explanation::SkillExplanation, String> {
    skills::explanation::generate(&skill_id, &skill_path, &lang, refresh)
}

#[tauri::command]
async fn list_packs_cmd() -> Result<Vec<skills::SkillPack>, String> {
    Ok(skills::registry::list_packs())
}

#[tauri::command]
async fn create_pack_cmd(pack: skills::SkillPack) -> Result<(), String> {
    skills::registry::create_pack(pack)
}

#[tauri::command]
async fn update_pack_cmd(pack: skills::SkillPack) -> Result<(), String> {
    skills::registry::update_pack(pack)
}

#[tauri::command]
async fn delete_pack_cmd(id: String) -> Result<(), String> {
    skills::registry::delete_pack(&id)
}

#[tauri::command]
async fn list_collections_cmd() -> Result<Vec<skills::SkillCollection>, String> {
    Ok(skills::registry::list_collections())
}

#[tauri::command]
async fn upsert_collection_cmd(
    collection: skills::SkillCollection,
) -> Result<skills::SkillCollection, String> {
    skills::registry::upsert_collection(collection)
}

#[tauri::command]
async fn delete_collection_cmd(id: String) -> Result<(), String> {
    skills::registry::delete_collection(&id)
}

#[tauri::command]
async fn export_collection_cmd(id: String) -> Result<String, String> {
    skills::registry::export_collection(&id)
}

#[tauri::command]
async fn import_collection_cmd(json: String) -> Result<skills::SkillCollection, String> {
    skills::registry::import_collection(&json)
}

#[tauri::command]
async fn batch_install_collection_cmd(
    collection: skills::SkillCollection,
    target_agents: Vec<String>,
) -> Result<(), String> {
    let pack = skills::SkillPack {
        id: collection.id,
        name: collection.name,
        description: collection.description,
        skills: collection.skills,
        target_agents,
    };
    skills::installer::apply_pack(&pack)
}

#[tauri::command]
async fn apply_pack_cmd(pack: skills::SkillPack) -> Result<(), String> {
    skills::installer::apply_pack(&pack)
}

#[tauri::command]
async fn configure_sync_cmd(config: skills::SyncConfig) -> Result<(), String> {
    skills::registry::set_sync_config(config)
}

#[tauri::command]
async fn push_sync_cmd() -> Result<skills::SyncResult, String> {
    skills::sync::push_to_github()
}

#[tauri::command]
async fn pull_sync_cmd() -> Result<skills::SyncResult, String> {
    skills::sync::pull_from_github()
}

#[tauri::command]
async fn resolve_conflicts_cmd(resolutions: Vec<skills::ConflictResolution>) -> Result<(), String> {
    skills::sync::resolve_conflicts(resolutions)
}

#[tauri::command]
async fn sync_agent_to_agent_cmd(from: String, to: String) -> Result<skills::SyncPreview, String> {
    skills::sync::sync_agent_to_agent(&from, &to)
}

#[tauri::command]
async fn execute_agent_sync_cmd(from: String, to: String) -> Result<(), String> {
    skills::sync::execute_agent_sync(&from, &to)
}

#[tauri::command]
async fn export_backup_cmd(path: String) -> Result<(), String> {
    skills::sync::export_backup(&path)
}

#[tauri::command]
async fn import_backup_cmd(path: String) -> Result<(), String> {
    skills::sync::import_backup(&path)
}

#[tauri::command]
async fn get_registry_metadata() -> Result<skills::registry::Metadata, String> {
    Ok(skills::registry::load())
}

#[tauri::command]
async fn list_marketplace_items_cmd() -> Result<Vec<skills::MarketplaceItem>, String> {
    skills::marketplace::list_items()
}

#[tauri::command]
async fn list_registries() -> Result<Vec<skills::marketplace::SkillRegistry>, String> {
    Ok(skills::marketplace::list_registries())
}

#[tauri::command]
async fn add_registry(
    name: String,
    source_type: String,
    url: String,
) -> Result<skills::marketplace::SkillRegistry, String> {
    skills::marketplace::add_registry(name, source_type, url)
}

#[tauri::command]
async fn remove_registry(registry_id: String) -> Result<(), String> {
    skills::marketplace::remove_registry(&registry_id)
}

#[tauri::command]
async fn sync_registry(
    registry_id: String,
) -> Result<Vec<skills::marketplace::MarketplaceSkill>, String> {
    skills::marketplace::sync_registry(
        &registry_id,
        skills::marketplace::SyncRegistryOptions::default(),
    )
}

#[tauri::command]
async fn sync_registry_with_options(
    registry_id: String,
    options: Option<skills::marketplace::SyncRegistryOptions>,
) -> Result<Vec<skills::marketplace::MarketplaceSkill>, String> {
    skills::marketplace::sync_registry(&registry_id, options.unwrap_or_default())
}

#[tauri::command]
async fn search_marketplace_skills(
    registry_id: Option<String>,
    query: Option<String>,
) -> Result<Vec<skills::marketplace::MarketplaceSkill>, String> {
    skills::marketplace::search_marketplace_skills(registry_id, query)
}

#[tauri::command]
async fn install_marketplace_skill(skill_id: String) -> Result<(), String> {
    skills::marketplace::install_marketplace_skill(&skill_id)
}

#[tauri::command]
async fn list_marketplace_sources_cmd() -> Result<Vec<skills::MarketplaceSource>, String> {
    Ok(skills::registry::list_marketplace_sources())
}

#[tauri::command]
async fn upsert_marketplace_source_cmd(source: skills::MarketplaceSource) -> Result<(), String> {
    skills::registry::upsert_marketplace_source(source)
}

#[tauri::command]
async fn remove_marketplace_source_cmd(id: String) -> Result<(), String> {
    skills::registry::remove_marketplace_source(&id)
}

// ── Display reconfiguration handler ──────────────────────────────
#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGDisplayRegisterReconfigurationCallback(
        callback: unsafe extern "C" fn(u32, u32, *mut std::ffi::c_void),
        user_info: *mut std::ffi::c_void,
    ) -> i32;
}

#[cfg(target_os = "macos")]
static DISPLAY_RECONFIG_APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

#[cfg(target_os = "macos")]
unsafe extern "C" fn display_reconfig_callback(_display: u32, _flags: u32, _user_info: *mut std::ffi::c_void) {
    // The callback fires before and after reconfiguration.
    // Only reposition after the change settles.
    if _flags & 1 != 0 {
        return; // kCGDisplayBeginConfigurationFlag — skip, wait for completion callback
    }
    if let Some(handle) = DISPLAY_RECONFIG_APP_HANDLE.get() {
        let handle = handle.clone();
        // Delay briefly so macOS finishes updating internal display state
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(600));
            let h = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                let _ = reposition_notch_to_display(&h, None, None);
            });
        });
    }
}

// ── Opacity helpers ─────────────────────────────────────────────
// macOS breaks transparent window compositing after a hide()/show() cycle,
// so we manage visibility via opacity instead.

/// Set the alpha value on a WebviewWindow using the native NSWindow API.
#[cfg(target_os = "macos")]
fn set_window_alpha(window: &tauri::WebviewWindow, alpha: f64) {
    use objc2_app_kit::NSWindow;
    if let Ok(ptr) = window.ns_window() {
        unsafe {
            let ns_window = ptr as *const NSWindow;
            (*ns_window).setAlphaValue(alpha);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_window_alpha(_window: &tauri::WebviewWindow, _alpha: f64) {}

#[cfg(target_os = "macos")]
fn apply_notch_window_for_spaces(window: &tauri::WebviewWindow) {
    use objc2_app_kit::{NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior};
    let _ = window.set_visible_on_all_workspaces(true);
    if let Ok(ptr) = window.ns_window() {
        unsafe {
            let ns_window = ptr as *const NSWindow;
            let mut behavior = (*ns_window).collectionBehavior();

            // Keep only one option from each AppKit mutual-exclusion group before
            // adding the overlay behaviors required for fullscreen Spaces.
            behavior &= !(NSWindowCollectionBehavior::Primary
                | NSWindowCollectionBehavior::Auxiliary
                | NSWindowCollectionBehavior::Managed
                | NSWindowCollectionBehavior::Transient
                | NSWindowCollectionBehavior::MoveToActiveSpace
                | NSWindowCollectionBehavior::ParticipatesInCycle
                | NSWindowCollectionBehavior::FullScreenPrimary
                | NSWindowCollectionBehavior::FullScreenNone
                | NSWindowCollectionBehavior::FullScreenAllowsTiling
                | NSWindowCollectionBehavior::FullScreenDisallowsTiling);
            behavior |= NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::CanJoinAllApplications
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle;
            (*ns_window).setCollectionBehavior(behavior);
            (*ns_window).setCanHide(false);
            (*ns_window).setLevel(NSScreenSaverWindowLevel + 1);
            (*ns_window).orderFrontRegardless();
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn apply_notch_window_for_spaces(_window: &tauri::WebviewWindow) {}

fn configure_notch_window_for_spaces(app: &tauri::AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("notch") {
            apply_notch_window_for_spaces(&window);
        }
    });
}

#[tauri::command]
async fn set_notch_opacity(app: tauri::AppHandle, opacity: f64) -> Result<(), String> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("notch") {
            if opacity > 0.0 {
                apply_notch_window_for_spaces(&window);
            }
            set_window_alpha(&window, opacity);
        }
    })
    .map_err(|e| e.to_string())
}

// ── Display Commands ────────────────────────────────────────────

#[tauri::command]
async fn list_displays(app: tauri::AppHandle) -> Result<Vec<DisplayInfo>, String> {
    Ok(list_displays_inner(&app))
}

#[tauri::command]
async fn set_display_id(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    display_id: String,
) -> Result<(), String> {
    let mut config = state.config_store.get();
    if config.display_id != display_id {
        config.island_pet_window_origin = None;
    }
    config.display_id = display_id.clone();
    state.config_store.update(config)?;
    reposition_notch_to_display(&app, Some(display_id), None)
}

fn reposition_notch_to_display(
    app: &tauri::AppHandle,
    display_id: Option<String>,
    horizontal_offset: Option<f64>,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window("notch") else {
        return Ok(());
    };

    let config_store = app.state::<AppState>();
    let configured_display_id = config_store.config_store.get().display_id;
    let display_id = display_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .unwrap_or(configured_display_id.as_str());
    let monitor = find_target_monitor(app, display_id)
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());

    if let Some(monitor) = monitor {
        // The pet now lives in its own dedicated window (label: "pet"); the
        // notch window always uses the standard island top-center geometry,
        // regardless of surface mode.
        let current_scale = window
            .current_monitor()
            .ok()
            .flatten()
            .map(|m| m.scale_factor())
            .unwrap_or_else(|| monitor.scale_factor());
        let width = window
            .outer_size()
            .map(|size| size.width as f64 / current_scale)
            .unwrap_or(420.0);
        position_notch_window(
            app,
            &window,
            &monitor,
            width,
            horizontal_offset.unwrap_or(0.0),
        );
    }
    configure_notch_window_for_spaces(app);

    Ok(())
}

fn position_notch_window(
    _app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    monitor: &tauri::Monitor,
    width: f64,
    horizontal_offset: f64,
) -> f64 {
    let (x, y, anchor_offset_x) = notch_window_geometry(monitor, width, horizontal_offset);
    let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(x, y)));
    anchor_offset_x
}

fn notch_window_geometry(
    monitor: &tauri::Monitor,
    width: f64,
    horizontal_offset: f64,
) -> (f64, f64, f64) {
    let scale = monitor.scale_factor();
    let screen_width = monitor.size().width as f64 / scale;
    let monitor_x = monitor.position().x as f64 / scale;
    let monitor_y = monitor.position().y as f64 / scale;
    let margin = 8.0;
    let base_x = monitor_x + (screen_width - width) / 2.0;
    let desired_x = base_x + horizontal_offset;
    let min_x = monitor_x + margin;
    let max_x = monitor_x + screen_width - width - margin;
    let x = if min_x <= max_x {
        desired_x.clamp(min_x, max_x)
    } else {
        base_x
    };
    (x, monitor_y, desired_x - x)
}

#[cfg(target_os = "macos")]
fn set_notch_window_frame(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    x: f64,
    top_y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    use objc2_app_kit::NSWindow;
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    let handle = window.clone();
    let scale = handle
        .current_monitor()
        .ok()
        .flatten()
        .map(|monitor| monitor.scale_factor())
        .unwrap_or(1.0);
    if let Ok(position) = handle.outer_position() {
        let current_top_y = position.y as f64 / scale;
        if (current_top_y - top_y).abs() > 0.5 {
            let _ = handle.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
                x, top_y,
            )));
        }
    }
    app.run_on_main_thread(move || {
        if let Ok(ptr) = handle.ns_window() {
            unsafe {
                let ns_window = ptr as *const NSWindow;
                let current = (*ns_window).frame();
                let top = current.origin.y + current.size.height;
                let target_x = x.round();
                let target_width = width.round();
                let target_height = height.round();
                let target_y = top - target_height;
                let frame_is_unchanged = (current.origin.x - target_x).abs() < 0.5
                    && (current.origin.y - target_y).abs() < 0.5
                    && (current.size.width - target_width).abs() < 0.5
                    && (current.size.height - target_height).abs() < 0.5;
                if frame_is_unchanged {
                    return;
                }
                let frame = NSRect::new(
                    NSPoint::new(target_x, target_y),
                    NSSize::new(target_width, target_height),
                );
                (*ns_window).setFrame_display(frame, true);
            }
        } else {
            let _ = handle.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
            let _ = handle.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
                x, top_y,
            )));
        }
    })
    .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
fn set_notch_window_frame(
    _app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    x: f64,
    top_y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
    let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
        x, top_y,
    )));
    Ok(())
}

const PET_DEFAULT_TRAILING_INSET: f64 = 24.0;
const PET_DEFAULT_BOTTOM_INSET: f64 = 36.0;

/// Show / position / hide the pet companion window based on the active
/// island surface mode. The pet is its own Tauri window so dragging it
/// doesn't drag the island shell along with it.
pub fn sync_pet_window_visibility(app: &tauri::AppHandle, config: &config::AppConfig) {
    let handle = app.clone();
    let is_pet_mode = config.island_surface_mode == "pet";
    let saved_origin = config.island_pet_window_origin.clone();

    let _ = app.run_on_main_thread(move || {
        sync_pet_window_visibility_inner(&handle, is_pet_mode, saved_origin.as_ref());
    });
}

/// Inner logic for pet/island window switching. **Must be called on the main thread.**
pub fn sync_pet_window_visibility_inner(
    handle: &tauri::AppHandle,
    is_pet_mode: bool,
    saved_origin: Option<&config::WindowOrigin>,
) {
    let Some(pet_window) = handle.get_webview_window("pet") else {
        return;
    };
    if !is_pet_mode {
        let _ = pet_window.hide();
        if let Some(notch_window) = handle.get_webview_window("notch") {
            let _ = notch_window.show();
            apply_notch_window_for_spaces(&notch_window);
        }
        return;
    }

    if let Some(notch_window) = handle.get_webview_window("notch") {
        let _ = notch_window.hide();
    }

    let monitor = pet_window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| pet_window.primary_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        if let Ok(size) = pet_window.outer_size() {
            position_pet_window(
                &pet_window,
                &monitor,
                size.width as f64,
                size.height as f64,
                saved_origin,
            );
        }
    }
    let _ = pet_window.show();
    apply_notch_window_for_spaces(&pet_window);
}

fn position_pet_window(
    window: &tauri::WebviewWindow,
    monitor: &tauri::Monitor,
    width: f64,
    height: f64,
    saved_origin: Option<&config::WindowOrigin>,
) {
    if let Some(origin) = saved_origin {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            origin.x.round() as i32,
            origin.y.round() as i32,
        )));
        return;
    }

    // Pet now lives in its own window; the whole window IS the pet area, so
    // we just place it near the bottom-right of the monitor.
    let pos = monitor.position();
    let size = monitor.size();
    let scale = monitor.scale_factor();
    let trailing_inset = PET_DEFAULT_TRAILING_INSET * scale;
    let bottom_inset = PET_DEFAULT_BOTTOM_INSET * scale;
    let margin = 8.0 * scale;
    let monitor_x = pos.x as f64;
    let monitor_y = pos.y as f64;
    let monitor_width = size.width as f64;
    let monitor_height = size.height as f64;
    let desired_x = monitor_x + monitor_width - trailing_inset - width;
    let desired_y = monitor_y + monitor_height - bottom_inset - height;
    let min_x = monitor_x + margin;
    let max_x = monitor_x + monitor_width - margin - width;
    let min_y = monitor_y + margin;
    let max_y = monitor_y + monitor_height - margin - height;
    let x = desired_x.clamp(min_x, max_x.max(min_x)).round();
    let y = desired_y.clamp(min_y, max_y.max(min_y)).round();
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        x as i32, y as i32,
    )));
}

fn notch_drag_geometry(
    monitor: &tauri::Monitor,
    width: f64,
    offset: f64,
) -> (f64, f64, f64, f64, f64) {
    let scale = monitor.scale_factor();
    let screen_width = monitor.size().width as f64 / scale;
    let monitor_x = monitor.position().x as f64 / scale;
    let monitor_y = monitor.position().y as f64 / scale;
    let margin = 8.0;
    let base_center = monitor_x + screen_width / 2.0;
    let min_center = monitor_x + margin + width / 2.0;
    let max_center = monitor_x + screen_width - margin - width / 2.0;
    let clamped_offset = if min_center > max_center {
        0.0
    } else {
        (base_center + offset).clamp(min_center, max_center) - base_center
    };
    let window_x = base_center + clamped_offset - width / 2.0;
    (base_center, min_center, max_center, monitor_y, window_x)
}

#[tauri::command]
async fn reposition_notch(
    app: tauri::AppHandle,
    display_id: Option<String>,
    horizontal_offset: Option<f64>,
) -> Result<(), String> {
    reposition_notch_to_display(&app, display_id, horizontal_offset)
}

#[tauri::command]
async fn preview_island_layout(
    app: tauri::AppHandle,
    mode: String,
    options: Option<serde_json::Value>,
) -> Result<(), String> {
    if !matches!(
        mode.as_str(),
        "micro" | "compact" | "expanded" | "completion"
    ) {
        return Err(format!("Unknown island layout preview mode: {mode}"));
    }
    if let Some(window) = app.get_webview_window("notch") {
        let mut payload = options.unwrap_or_else(|| serde_json::json!({}));
        if !payload.is_object() {
            payload = serde_json::json!({});
        }
        payload["mode"] = serde_json::Value::String(mode);
        window
            .emit("island-layout-preview", payload)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn clear_island_layout_preview(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("notch") {
        window
            .emit("island-layout-preview-clear", ())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn start_notch_drag(
    app: tauri::AppHandle,
    horizontal_offset: f64,
    width: f64,
    height: f64,
    display_id: Option<String>,
) -> Result<bool, String> {
    let Some(window) = app.get_webview_window("notch") else {
        return Ok(false);
    };
    let (cursor_x, _) = get_cursor_position_sync()?;
    let config_store = app.state::<AppState>();
    let configured_display_id = config_store.config_store.get().display_id;
    let display_id = display_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .unwrap_or(configured_display_id.as_str());
    let monitor = find_target_monitor(&app, display_id)
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return Ok(false);
    };
    let (base_center, min_center, max_center, y, window_x) =
        notch_drag_geometry(&monitor, width, horizontal_offset);
    let start_offset = if min_center > max_center {
        0.0
    } else {
        (window_x + width / 2.0) - base_center
    };

    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
    let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
        window_x.round(),
        y,
    )));
    configure_notch_window_for_spaces(&app);

    {
        let mut drag = notch_drag_state()
            .lock()
            .map_err(|e| format!("Drag lock error: {}", e))?;
        *drag = Some(NotchDragState {
            start_x: cursor_x,
            start_offset,
            current_offset: start_offset,
            width,
            y,
            base_center,
            min_center,
            max_center,
            last_window_x: window_x.round(),
        });
    }

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let started_at = std::time::Instant::now();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
            if started_at.elapsed() > std::time::Duration::from_secs(30) {
                if let Ok(mut drag) = notch_drag_state().lock() {
                    *drag = None;
                }
                break;
            }
            let keep_dragging = update_notch_drag_position(&app_handle).unwrap_or(false);
            if !keep_dragging {
                break;
            }
        }
    });

    Ok(true)
}

fn update_notch_drag_position(app: &tauri::AppHandle) -> Result<bool, String> {
    let Some(window) = app.get_webview_window("notch") else {
        return Ok(false);
    };
    let (cursor_x, _) = get_cursor_position_sync()?;

    let next_position = {
        let mut drag = notch_drag_state()
            .lock()
            .map_err(|e| format!("Drag lock error: {}", e))?;
        let Some(state) = drag.as_mut() else {
            return Ok(false);
        };
        let desired_offset = state.start_offset + cursor_x - state.start_x;
        let next_offset = if state.min_center > state.max_center {
            0.0
        } else {
            (state.base_center + desired_offset).clamp(state.min_center, state.max_center)
                - state.base_center
        };
        let window_x = (state.base_center + next_offset - state.width / 2.0).round();
        if (window_x - state.last_window_x).abs() < 1.0 {
            return Ok(true);
        }
        state.current_offset = next_offset;
        state.last_window_x = window_x;
        (window_x, state.y)
    };

    let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
        next_position.0,
        next_position.1,
    )));
    Ok(true)
}

#[tauri::command]
async fn end_notch_drag(app: tauri::AppHandle) -> Result<Option<f64>, String> {
    let final_offset = {
        let mut drag = notch_drag_state()
            .lock()
            .map_err(|e| format!("Drag lock error: {}", e))?;
        drag.take().map(|state| state.current_offset.round())
    };

    if let Some(offset) = final_offset {
        reposition_notch_to_display(&app, None, Some(offset))?;
    }

    Ok(final_offset)
}

#[tauri::command]
async fn start_pet_drag(app: tauri::AppHandle) -> Result<bool, String> {
    let Some(window) = app.get_webview_window("pet") else {
        return Ok(false);
    };
    let cursor = app.cursor_position().map_err(|e| e.to_string())?;
    let position = window.outer_position().map_err(|e| e.to_string())?;
    {
        let mut drag = pet_drag_state()
            .lock()
            .map_err(|e| format!("Pet drag lock error: {}", e))?;
        *drag = Some(PetDragState {
            start_cursor_x: cursor.x,
            start_cursor_y: cursor.y,
            start_window_x: position.x as f64,
            start_window_y: position.y as f64,
            current_x: position.x as f64,
            current_y: position.y as f64,
        });
    }

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let started_at = std::time::Instant::now();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
            if started_at.elapsed() > std::time::Duration::from_secs(30) {
                if let Ok(mut drag) = pet_drag_state().lock() {
                    *drag = None;
                }
                break;
            }
            let keep_dragging = update_pet_drag_position(&app_handle).unwrap_or(false);
            if !keep_dragging {
                break;
            }
        }
    });

    Ok(true)
}

fn update_pet_drag_position(app: &tauri::AppHandle) -> Result<bool, String> {
    let Some(window) = app.get_webview_window("pet") else {
        return Ok(false);
    };
    let cursor = app.cursor_position().map_err(|e| e.to_string())?;
    let next_position = {
        let mut drag = pet_drag_state()
            .lock()
            .map_err(|e| format!("Pet drag lock error: {}", e))?;
        let Some(state) = drag.as_mut() else {
            return Ok(false);
        };
        let x = (state.start_window_x + cursor.x - state.start_cursor_x).round();
        let y = (state.start_window_y + cursor.y - state.start_cursor_y).round();
        if (x - state.current_x).abs() < 1.0 && (y - state.current_y).abs() < 1.0 {
            return Ok(true);
        }
        state.current_x = x;
        state.current_y = y;
        (x, y)
    };

    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        next_position.0 as i32,
        next_position.1 as i32,
    )));
    Ok(true)
}

#[tauri::command]
async fn end_pet_drag(
    app: tauri::AppHandle,
) -> Result<Option<crate::config::WindowOrigin>, String> {
    let final_origin = {
        let mut drag = pet_drag_state()
            .lock()
            .map_err(|e| format!("Pet drag lock error: {}", e))?;
        drag.take().map(|state| crate::config::WindowOrigin {
            x: state.current_x.round(),
            y: state.current_y.round(),
        })
    };

    if let Some(origin) = final_origin.clone() {
        let state = app.state::<AppState>();
        let mut config = state.config_store.get();
        config.island_pet_window_origin = Some(origin);
        state.config_store.update(config)?;
    }

    Ok(final_origin)
}

/// Resize the notch window dynamically from the frontend and re-center
/// on the display selected in config (falls back to primary).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ResizeNotchResult {
    anchor_offset_x: f64,
}

#[tauri::command]
async fn resize_notch(
    app: tauri::AppHandle,
    width: f64,
    height: f64,
    horizontal_offset: Option<f64>,
    display_id: Option<String>,
) -> Result<ResizeNotchResult, String> {
    if let Some(window) = app.get_webview_window("notch") {
        // Determine which monitor to center on from config
        let config_store = app.state::<AppState>();
        let config = config_store.config_store.get();
        let configured_display_id = config.display_id;
        let display_id = display_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .unwrap_or(configured_display_id.as_str());

        let monitor = find_target_monitor(&app, display_id)
            .or_else(|| window.current_monitor().ok().flatten())
            .or_else(|| window.primary_monitor().ok().flatten());

        if let Some(monitor) = monitor {
            // The notch window always uses standard island top-center geometry.
            // Pet placement is handled by sync_pet_window_visibility / drag commands
            // on the dedicated "pet" window.
            let (x, y, anchor_offset_x) =
                notch_window_geometry(&monitor, width, horizontal_offset.unwrap_or(0.0));
            set_notch_window_frame(&app, &window, x, y, width, height)?;
            return Ok(ResizeNotchResult { anchor_offset_x });
        }
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
    }
    Ok(ResizeNotchResult {
        anchor_offset_x: 0.0,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("agentbro".to_string()),
                    }),
                ])
                .max_file_size(2_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
                .level(log::LevelFilter::Info)
                .level_for("agentbro", log::LevelFilter::Info)
                .level_for("agentbro_lib", log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Position notch window at top center
            if let Some(window) = app.get_webview_window("notch") {
                let monitor = window
                    .current_monitor()
                    .ok()
                    .flatten()
                    .or_else(|| window.primary_monitor().ok().flatten());

                if let Some(monitor) = monitor {
                    let scale = monitor.scale_factor();
                    let screen_width = monitor.size().width as f64 / scale;
                    let monitor_x = monitor.position().x as f64 / scale;
                    let monitor_y = monitor.position().y as f64 / scale;
                    let window_width = 420.0; // 400 panel + 20 shadow padding
                    let x = monitor_x + (screen_width - window_width) / 2.0;
                    let _ = window.set_position(tauri::Position::Logical(
                        tauri::LogicalPosition::new(x, monitor_y),
                    ));
                }

                // Set initial compact size (will be dynamically resized by frontend)
                let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(420.0, 52.0)));

                // Force shadow off for clean transparent look
                let _ = window.set_shadow(false);

                // Keep the island above the menu bar and present in fullscreen Spaces.
                apply_notch_window_for_spaces(&window);

                // Reposition the island when display configuration changes (e.g. external monitor connected/disconnected).
                #[cfg(target_os = "macos")]
                {
                    let _ = DISPLAY_RECONFIG_APP_HANDLE.set(app.handle().clone());
                    unsafe {
                        CGDisplayRegisterReconfigurationCallback(
                            display_reconfig_callback,
                            std::ptr::null_mut(),
                        );
                    }
                }
            }

            // Ensure settings window is hidden on startup
            if let Some(settings_window) = app.get_webview_window("settings") {
                let _ = settings_window.hide();
                let app_handle = app.handle().clone();
                settings_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = app_handle.get_webview_window("settings").map(|w| w.hide());
                        let _ =
                            app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
                        if let Some(notch) = app_handle.get_webview_window("notch") {
                            let _ = notch.show();
                        }
                    }
                });
            }

            // Initialize session store
            let mut session_store = SessionStore::new();
            session_store.set_app_handle(app.handle().clone());
            let session_store = Arc::new(session_store);

            // Initialize config store
            let mut config_store = ConfigStore::new();
            config_store.set_app_handle(app.handle().clone());

            // Pet window: position to the saved (or default) corner and show
            // only when pet surface mode is active. The pet lives in its own
            // Tauri window so dragging it doesn't move the island shell.
            sync_pet_window_visibility(app.handle(), &config_store.get());

            // Bootstrap: discover sessions that were already running before we launched
            {
                let config = config_store.get();
                let extra_roots: Vec<std::path::PathBuf> = config
                    .engine_instances
                    .iter()
                    .filter(|i| i.enabled)
                    .map(|i| agents::claude_code::expand_tilde(&i.config_root))
                    .collect();
                let mut projects_dirs = all_projects_dirs();
                projects_dirs.extend(projects_dirs_from_roots(&extra_roots));

                let bootstrap_age_secs = if config.idle_timeout_minutes > 0 {
                    u64::from(config.idle_timeout_minutes) * 60
                } else {
                    5 * 60
                };
                let discovered = discover_active_sessions_in_dirs(
                    std::time::Duration::from_secs(bootstrap_age_secs.max(60)),
                    &projects_dirs,
                );
                if !discovered.is_empty() {
                    log::info!(
                        "Startup bootstrap: discovered {} active session(s)",
                        discovered.len()
                    );
                    for ds in &discovered {
                        let matched_instance = config.engine_instances.iter().find(|inst| {
                            if !inst.enabled {
                                return false;
                            }
                            let expected = agents::claude_code::expand_tilde(&inst.config_root)
                                .join("projects");
                            let expected = expected.canonicalize().unwrap_or(expected);
                            let actual = ds
                                .projects_dir
                                .canonicalize()
                                .unwrap_or_else(|_| ds.projects_dir.clone());
                            expected == actual
                        });
                        let _session = session_store.get_or_create_session(
                            &ds.session_id,
                            "claude-code",
                            &ds.project,
                            &ds.cwd,
                            "", // terminal unknown at startup
                        );
                        session_store.update_session(&ds.session_id, |s| {
                            s.started_at = ds.modified_at;
                            s.duration = chrono::Utc::now().timestamp() - ds.modified_at;
                        });
                        // Set session title if we extracted one
                        if let Some(ref title) = ds.session_title {
                            session_store.update_session(&ds.session_id, |s| {
                                s.session_title = Some(title.clone());
                            });
                        }
                        if let Some(inst) = matched_instance {
                            session_store.update_session(&ds.session_id, |s| {
                                s.engine_label = Some(inst.label.clone());
                                s.engine_config_root = Some(inst.config_root.clone());
                            });
                        }
                        log::info!(
                            "  session {}: project={}, title={:?}",
                            &ds.session_id[..8.min(ds.session_id.len())],
                            ds.project,
                            ds.session_title,
                        );
                    }
                }
            }

            // Initialize themes: ensure built-in themes exist in user dir
            if let Ok(resource_path) = app.path().resource_dir() {
                theme::scanner::seed_builtin_themes(&resource_path);
            }

            // Initialize adapters: default ~/.claude + custom engine instances
            let default_cc = ClaudeCodeAdapter::new();
            let mut cc_adapters: Vec<ClaudeCodeAdapter> = vec![];
            {
                let config = config_store.get();
                for inst in &config.engine_instances {
                    if inst.enabled {
                        let root = agents::claude_code::expand_tilde(&inst.config_root);
                        cc_adapters.push(ClaudeCodeAdapter::with_config_root(
                            root,
                            inst.label.clone(),
                        ));
                    }
                }
            }

            // Update hook script if app was updated (compare embedded vs deployed)
            if ClaudeCodeAdapter::update_hook_script_if_needed() {
                log::info!("Hook script was updated to match new app version");
            }

            // Build the shared adapter list. Claude Code instances are expanded
            // from config roots; other tools come from the standard adapter set.
            let mut adapter_list: Vec<Arc<dyn AgentAdapter>> = vec![Arc::new(default_cc)];
            for cc in cc_adapters {
                adapter_list.push(Arc::new(cc));
            }
            for adapter in agents::all_adapters() {
                if adapter.name() != "claude-code" {
                    adapter_list.push(Arc::from(adapter));
                }
            }
            let adapters: Arc<Vec<Arc<dyn AgentAdapter>>> = Arc::new(adapter_list);

            // Auto-install Claude hooks on startup; other tools are controlled
            // explicitly from the Integration tab.
            for adapter in adapters.iter() {
                if adapter.name() != "claude-code" {
                    continue;
                }
                if let Err(skip_reason) = ensure_installable(adapter.as_ref()) {
                    log::info!(
                        "Skipping startup hook install for {}: {}",
                        adapter.display_name(),
                        skip_reason
                    );
                    continue;
                }
                if let Err(e) = adapter.install_hooks() {
                    log::warn!(
                        "Failed to install hooks for {}: {}",
                        adapter.display_name(),
                        e
                    );
                } else {
                    log::info!("Hooks installed for {}", adapter.display_name());
                }
            }

            // Initialize and start hook server
            let hook_server = HookServer::new(session_store.clone(), adapters.clone());
            let hook_server = Arc::new(hook_server);
            hook_server.set_app_handle(app.handle().clone());
            hook_server.set_config_store(config_store.clone());

            // Start hook server — exit if port is already in use (another instance running)
            let server = hook_server.clone();
            let app_handle_for_server = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = server.start().await {
                    log::error!("Failed to start HookServer: {}", e);
                    app_handle_for_server.exit(1);
                }
            });

            // Start hook auto-recovery watcher
            hooks::recovery::start_hook_recovery(adapters.clone(), app.handle().clone());

            // Initialize sound engine and share with HookServer
            let sound_engine: Option<Arc<SoundEngine>> = SoundEngine::new().map(Arc::new);
            if let Some(ref engine) = sound_engine {
                log::info!("Sound engine initialized");
                let cfg = config_store.get();
                engine.set_volume(cfg.sound_volume);
                engine.set_enabled(cfg.sound_enabled);
                if let Some(pack) = SoundPack::from_id(&cfg.sound_pack) {
                    engine.set_sound_pack(pack);
                }
                engine.set_probe_filter(cfg.probe_session_filter);
                engine.set_quiet_hours(
                    cfg.quiet_hours_enabled,
                    cfg.quiet_hours_start.clone(),
                    cfg.quiet_hours_end.clone(),
                );
                for (event_id, enabled) in cfg.sound_events.iter() {
                    if let Some(event) = SoundEvent::from_id(event_id) {
                        engine.set_event_enabled(event, *enabled);
                    }
                }
                for (event_id, rule) in cfg.sound_rules.iter() {
                    if let Some(event) = SoundEvent::from_id(event_id) {
                        engine.set_event_rule(event, rule.enabled, rule.sound.clone());
                    }
                }
                engine.set_custom_sounds(
                    cfg.custom_sounds
                        .iter()
                        .map(|sound| (sound.id.clone(), sound.path.clone()))
                        .collect(),
                );
                hook_server.set_sound_engine(engine.clone());
            } else {
                log::warn!("Sound engine failed to initialize (no audio output)");
            }

            // Build macOS menu bar / system tray shortcut with menu.
            let show_item = MenuItemBuilder::with_id("show", "Open AgentBro").build(app)?;
            let settings_item = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .item(&settings_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let tray_icon = TrayIconBuilder::new()
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .tooltip("AgentBro")
                .icon(menu_bar_icon())
                .icon_as_template(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_notch_window(tray.app_handle());
                    }
                })
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        show_notch_window(app);
                    }
                    "settings" => {
                        let _ = show_settings_window(app);
                    }
                    "quit" => {
                        std::process::exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Start conversation file watcher (watches projects dirs for JSONL changes)
            let extra_roots: Vec<std::path::PathBuf> = {
                let config = config_store.get();
                config
                    .engine_instances
                    .iter()
                    .filter(|i| i.enabled)
                    .map(|i| agents::claude_code::expand_tilde(&i.config_root))
                    .collect()
            };
            let conversation_watcher =
                ConversationWatcher::start_with_roots(app.handle().clone(), &extra_roots);
            if conversation_watcher.is_some() {
                log::info!("Conversation watcher started");
            }
            let conversation_watcher = Arc::new(std::sync::Mutex::new(conversation_watcher));

            // Store shared state for Tauri commands
            // Initialize display controller
            let display_controller =
                Arc::new(platform::display_controller::DisplayController::new());
            display_controller.set_app_handle(app.handle().clone());

            // Initialize remote manager with persisted hosts
            // Use the hook server's socket so the reverse tunnel delivers events
            // directly to the running HookServer listener.
            let local_socket = hook_endpoint::current().socket_path;
            let remote_manager = Arc::new(remote::RemoteManager::new(local_socket));
            {
                let cfg = config_store.get();
                for host in cfg.remote_hosts {
                    remote_manager.add_host(host);
                }
            }
            remote_manager.startup();

            // Initialize diagnostic ring buffer
            let diagnostic_buffer = Arc::new(hooks::diagnostics::DiagnosticRingBuffer::new());
            let network_monitor = Arc::new(NetworkMonitor::new());

            let switch_db = Arc::new(
                switch::db::SwitchDatabase::open().expect("failed to open switch database"),
            );
            let telemetry = Arc::new(TelemetryService::new());

            let app_state = AppState {
                session_store,
                hook_server,
                config_store,
                adapters: (*adapters).clone(),
                sound_engine,
                conversation_watcher,
                display_controller,
                remote_manager,
                diagnostic_buffer,
                network_monitor,
                switch_db,
                telemetry,
                tray_icon,
            };
            {
                let telemetry = app_state.telemetry.clone();
                let config = app_state.config_store.get();
                tauri::async_runtime::spawn(async move {
                    telemetry.record_app_launch(&config).await;
                });
            }
            let buddy_device_config = app_state.config_store.get().buddy_device;
            commands::buddy::start_buddy_device_server(
                buddy_device_config,
                app_state.session_store.clone(),
            );
            app.manage(app_state);

            if let Err(err) = register_island_global_shortcuts(app.handle()) {
                log::warn!("Failed to register island global shortcuts: {}", err);
            }

            // Deep link handler
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        if let Some(payload) = switch::deeplink::parse_deep_link(url.as_str()) {
                            let _ = handle.emit("switch-deep-link", &payload);
                        }
                    }
                });
            }

            log::info!("AgentBro started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            quit_app,
            set_dock_visible,
            open_settings_window,
            commands::get_sessions,
            commands::get_usage_rate_limits,
            commands::get_usage_snapshots,
            commands::list_usage_providers,
            commands::authorize_usage_provider,
            commands::respond_permission,
            commands::respond_auto_approve,
            commands::respond_question,
            commands::respond_plan,
            commands::send_message,
            commands::jump_to_terminal,
            commands::get_config,
            commands::update_config,
            commands::set_language,
            commands::set_analytics_enabled,
            commands::set_launch_at_login,
            commands::set_island_feature_flags,
            commands::set_island_surface_options,
            commands::install_hooks,
            commands::remove_hooks,
            commands::get_adapter_status,
            commands::verify_hooks,
            commands::is_terminal_focused,
            commands::run_hook_doctor,
            commands::get_chat_history,
            commands::get_subagent_chat_history,
            commands::monitor::get_monitor_sessions,
            commands::monitor::get_monitor_session_detail,
            commands::monitor::get_monitor_timeline,
            commands::monitor::get_network_monitor_status,
            commands::monitor::set_network_monitor_enabled,
            commands::monitor::get_network_monitor_requests,
            commands::monitor::get_network_monitor_request_detail,
            commands::monitor::get_claude_wrapper_status,
            commands::monitor::install_claude_wrapper,
            commands::monitor::remove_claude_wrapper,
            commands::export_diagnostics,
            commands::add_engine_instance,
            commands::remove_engine_instance,
            commands::set_engine_instance_enabled,
            commands::verify_engine_path,
            resize_notch,
            play_sound,
            preview_sound,
            set_sound_volume,
            set_sound_enabled,
            set_sound_pack,
            set_probe_session_filter,
            set_sound_quiet_hours,
            set_sound_event_enabled,
            set_sound_event_rule,
            import_custom_sound,
            set_custom_sounds,
            set_notch_opacity,
            list_displays,
            set_display_id,
            reposition_notch,
            preview_island_layout,
            clear_island_layout_preview,
            start_notch_drag,
            end_notch_drag,
            start_pet_drag,
            end_pet_drag,
            pets::discover_pets,
            commands::set_active_pet_id,
            is_cursor_in_window_zones,
            should_suppress,
            get_cursor_position,
            is_cursor_over_notch,
            set_notch_ignore_cursor_events,
            save_sessions,
            load_sessions,
            get_themes,
            set_active_theme,
            get_display_level,
            notify_cursor_enter,
            notify_cursor_leave,
            notify_esc,
            notify_expand,
            detect_tools,
            install_agent_hook,
            install_custom_agent_hook,
            uninstall_agent_hook,
            configure_agent_hook_events,
            get_all_hook_status,
            reinstall_all_hooks,
            uninstall_all_hooks,
            commands::simulate_hook_event,
            list_remote_hosts,
            add_remote_host,
            remove_remote_host,
            connect_remote,
            disconnect_remote,
            install_remote_hooks,
            uninstall_remote_hooks,
            install_remote_agent_hooks,
            uninstall_remote_agent_hooks,
            check_remote_hooks,
            list_remote_installable_agents,
            get_remote_status,
            list_ssh_config_hosts,
            list_webhooks,
            add_webhook,
            remove_webhook,
            update_webhook,
            save_webhook_config,
            test_webhook,
            get_webhook_logs,
            get_diagnostic_events,
            commands::buddy::read_buddy_data,
            commands::buddy::buddy_device_snapshot,
            commands::buddy::get_buddy_device_config,
            commands::buddy::set_buddy_device_config,
            commands::buddy::buddy_reverse_focus,
            validate_path,
            register_global_shortcut,
            unregister_global_shortcut,
            set_global_action_shortcuts,
            perform_haptic,
            set_notch_focusable,
            restart_app,
            open_image,
            open_system_path,
            list_themes,
            get_active_theme_bundle,
            import_theme,
            scan_all_skills,
            scan_agent_skills,
            get_central_skill_bundles,
            get_central_skill_bundle_detail,
            preview_delete_central_skill_bundle,
            delete_central_skill_bundle,
            preview_delete_central_skill,
            delete_central_skill,
            discover_project_skills_cmd,
            discover_enabled_project_skills_cmd,
            get_discovered_skills_cmd,
            clear_discovered_skills_cmd,
            stop_project_scan,
            get_scan_roots_cmd,
            set_scan_roots_cmd,
            set_scan_root_enabled_cmd,
            get_obsidian_vaults_cmd,
            get_obsidian_vault_skills_cmd,
            install_skill_cmd,
            batch_import_discovered_skills_cmd,
            preview_github_skills_cmd,
            preview_github_repo_import,
            import_github_repo_skills,
            install_plugin_cmd,
            uninstall_skill_cmd,
            upsert_mcp_server_cmd,
            remove_mcp_server_cmd,
            validate_mcp_server_cmd,
            toggle_skill_cmd,
            read_skill_files,
            read_skill_file_content,
            get_skill_explanation_cmd,
            generate_skill_explanation_cmd,
            list_packs_cmd,
            create_pack_cmd,
            update_pack_cmd,
            delete_pack_cmd,
            list_collections_cmd,
            upsert_collection_cmd,
            delete_collection_cmd,
            export_collection_cmd,
            import_collection_cmd,
            batch_install_collection_cmd,
            apply_pack_cmd,
            configure_sync_cmd,
            push_sync_cmd,
            pull_sync_cmd,
            resolve_conflicts_cmd,
            sync_agent_to_agent_cmd,
            execute_agent_sync_cmd,
            export_backup_cmd,
            import_backup_cmd,
            get_registry_metadata,
            list_marketplace_items_cmd,
            list_registries,
            add_registry,
            remove_registry,
            sync_registry,
            sync_registry_with_options,
            search_marketplace_skills,
            install_marketplace_skill,
            list_marketplace_sources_cmd,
            upsert_marketplace_source_cmd,
            remove_marketplace_source_cmd,
            agents::programs::agent_list,
            agents::programs::agent_refresh,
            agents::programs::agent_install,
            agents::programs::agent_update,
            agents::programs::agent_uninstall,
            agents::programs::agent_open_download,
            agents::programs::agent_open_app,
            agents::programs::add_custom_agent,
            agents::programs::update_custom_agent,
            agents::programs::remove_custom_agent,
            switch::commands::switch_list_providers,
            switch::commands::switch_create_provider,
            switch::commands::switch_update_provider,
            switch::commands::switch_delete_provider,
            switch::commands::switch_duplicate_provider,
            switch::commands::switch_set_current,
            switch::commands::switch_get_current,
            switch::commands::switch_detect_cc_switch,
            switch::commands::switch_import_cc_switch_preview,
            switch::commands::switch_import_cc_switch,
            switch::commands::switch_clear_all_data,
            switch::commands::switch_list_prompts,
            switch::commands::switch_create_prompt,
            switch::commands::switch_update_prompt,
            switch::commands::switch_delete_prompt,
            switch::commands::switch_toggle_prompt,
            switch::commands::switch_apply_prompts,
            switch::commands::switch_list_presets,
            switch::commands::switch_get_usage_summary,
            switch::commands::switch_get_usage_by_provider,
            switch::commands::switch_get_usage_by_model,
            switch::commands::switch_get_daily_cost,
            switch::commands::switch_list_model_pricing,
            switch::commands::switch_get_provider_health,
            switch::commands::switch_speed_test,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AgentBro");
}
