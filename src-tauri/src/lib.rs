// AgentBro — Rust Backend Library
pub mod agents;
pub mod commands;
pub mod config;
pub mod data_dir;
pub mod energy;
pub mod hook_endpoint;
pub mod hooks;
pub mod market;
pub mod menu_bar;
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
use sound::{SoundEngine, SoundEvent, SoundPack, SoundPackImportResult};
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
    native_drag: bool,
    start_anchor: PetStageAnchor,
}

static NOTCH_DRAG_STATE: OnceLock<Mutex<Option<NotchDragState>>> = OnceLock::new();
static PET_DRAG_STATE: OnceLock<Mutex<Option<PetDragState>>> = OnceLock::new();
static TRAY_ICON_RECT: OnceLock<Mutex<Option<tauri::Rect>>> = OnceLock::new();

fn notch_drag_state() -> &'static Mutex<Option<NotchDragState>> {
    NOTCH_DRAG_STATE.get_or_init(|| Mutex::new(None))
}

fn pet_drag_state() -> &'static Mutex<Option<PetDragState>> {
    PET_DRAG_STATE.get_or_init(|| Mutex::new(None))
}

fn tray_icon_rect() -> &'static Mutex<Option<tauri::Rect>> {
    TRAY_ICON_RECT.get_or_init(|| Mutex::new(None))
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
                            let _ = window.set_ignore_cursor_events(false);
                            activate_agentbro_app();
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
                    let _ = window.set_ignore_cursor_events(false);
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
    let target = if src.starts_with("data:") {
        persist_data_url_image(&src)?
    } else {
        src
    };
    open_system_target(&target)
}

#[tauri::command]
async fn read_image_data_url(src: String) -> Result<String, String> {
    if src.starts_with("data:") {
        return Ok(src);
    }
    let path = image_source_path(&src)?;
    let metadata =
        std::fs::metadata(&path).map_err(|e| format!("Failed to read image metadata: {}", e))?;
    if !metadata.is_file() {
        return Err("Image source is not a file".to_string());
    }
    const MAX_INLINE_IMAGE_BYTES: u64 = 30 * 1024 * 1024;
    if metadata.len() > MAX_INLINE_IMAGE_BYTES {
        return Err("Image is too large to preview inline".to_string());
    }

    let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read image: {}", e))?;
    let media_type = infer_image_media_type(&path, &bytes)
        .ok_or_else(|| "Unsupported image format".to_string())?;
    let encoded = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(bytes)
    };
    Ok(format!("data:{};base64,{}", media_type, encoded))
}

#[tauri::command]
async fn open_system_path(path: String) -> Result<(), String> {
    open_system_target(&path)
}

fn image_source_path(src: &str) -> Result<PathBuf, String> {
    let trimmed = src.trim();
    if let Some(rest) = trimmed.strip_prefix("file://") {
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        return Ok(PathBuf::from(percent_decode_path(rest)?));
    }
    if Path::new(trimmed).is_absolute() || trimmed.starts_with("~/") {
        return Ok(PathBuf::from(expand_tilde_target(trimmed)));
    }
    #[cfg(target_os = "windows")]
    if trimmed.starts_with("~\\") {
        return Ok(PathBuf::from(expand_tilde_target(trimmed)));
    }
    Err("Image source is not a local file path".to_string())
}

fn percent_decode_path(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = bytes
                .get(i + 1)
                .copied()
                .and_then(hex_value)
                .ok_or_else(|| "Invalid file URL escape".to_string())?;
            let lo = bytes
                .get(i + 2)
                .copied()
                .and_then(hex_value)
                .ok_or_else(|| "Invalid file URL escape".to_string())?;
            decoded.push((hi << 4) | lo);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| "Invalid UTF-8 file URL".to_string())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn infer_image_media_type(path: &Path, bytes: &[u8]) -> Option<&'static str> {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png") => return Some("image/png"),
        Some("jpg") | Some("jpeg") => return Some("image/jpeg"),
        Some("gif") => return Some("image/gif"),
        Some("webp") => return Some("image/webp"),
        Some("bmp") => return Some("image/bmp"),
        Some("svg") => return Some("image/svg+xml"),
        Some("heic") | Some("heif") => return Some("image/heic"),
        Some("avif") => return Some("image/avif"),
        _ => {}
    }

    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"BM") {
        Some("image/bmp")
    } else {
        None
    }
}

#[cfg(test)]
mod image_preview_tests {
    use super::*;

    #[test]
    fn file_url_image_paths_are_decoded() {
        let path = image_source_path("file:///tmp/agentbro%20preview/image.png").unwrap();
        assert_eq!(path, PathBuf::from("/tmp/agentbro preview/image.png"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_image_source_accepts_absolute_paths() {
        let path = image_source_path(r"C:\Users\admin\Pictures\preview.png").unwrap();
        assert_eq!(path, PathBuf::from(r"C:\Users\admin\Pictures\preview.png"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_tilde_backslash_paths_expand_to_home() {
        let Some(home) = dirs::home_dir() else {
            return;
        };

        assert_eq!(
            expand_tilde_target(r"~\AgentBro\hooks"),
            home.join(r"AgentBro\hooks").display().to_string()
        );
        assert_eq!(
            image_source_path(r"~\Pictures\preview.png").unwrap(),
            home.join(r"Pictures\preview.png")
        );
    }

    #[test]
    fn image_media_type_uses_magic_bytes_without_extension() {
        let bytes = b"\x89PNG\r\n\x1a\nrest";
        assert_eq!(
            infer_image_media_type(Path::new("/tmp/preview"), bytes),
            Some("image/png")
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_system_open_recognizes_urls_without_matching_paths() {
        assert!(is_system_url("https://github.com/shirenchuang/agentbro"));
        assert!(is_system_url("ccswitch://provider"));
        assert!(!is_system_url(r"C:\Users\admin\Pictures\preview.png"));
        assert!(!is_system_url(r"\\server\share\preview.png"));
    }
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
    let target = expand_tilde_target(target.trim());

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(&target);
        command
    };

    #[cfg(target_os = "windows")]
    {
        return open_system_target_windows(&target);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(&target);
        command
    };

    #[cfg(not(target_os = "windows"))]
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open path: {}", e))
}

#[cfg(target_os = "windows")]
fn open_system_target_windows(target: &str) -> Result<(), String> {
    if target.is_empty() {
        return Err("Failed to open path: target is empty".to_string());
    }

    let mut command = if is_system_url(target) {
        let mut command = std::process::Command::new("rundll32.exe");
        command.args(["url.dll,FileProtocolHandler", target]);
        command
    } else {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(target);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open path: {}", e))
}

#[cfg(target_os = "windows")]
fn is_system_url(target: &str) -> bool {
    target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with("agentbro:")
        || target.starts_with("ccswitch:")
}

fn expand_tilde_target(target: &str) -> String {
    if let Some(rest) = target.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).display().to_string();
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(rest) = target.strip_prefix("~\\") {
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
            "{} CLI not found. Searched process PATH, login shell PATH, \
             and common directories (homebrew, nvm, volta, mise, cargo). \
             Confirm it is installed and try restarting AgentBro.",
            adapter.display_name()
        )),
        _ => Ok(()),
    }
}

#[tauri::command]
async fn detect_tools() -> Vec<agents::detection::DetectedTool> {
    agents::detection::detect_installed_tools()
}

fn configured_engine_instance(
    state: &commands::AppState,
    tool_name: &str,
) -> Option<config::EngineInstance> {
    let id = tool_name.strip_prefix("engine:")?;
    state
        .config_store
        .get()
        .engine_instances
        .into_iter()
        .find(|instance| instance.id == id)
}

fn configured_engine_adapter(
    instance: &config::EngineInstance,
) -> agents::claude_code::ClaudeCodeAdapter {
    agents::claude_code::ClaudeCodeAdapter::with_config_root(
        agents::claude_code::expand_tilde(&instance.config_root),
        instance.label.clone(),
    )
}

fn persist_engine_enabled(
    state: &commands::AppState,
    engine_id: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut config = state.config_store.get();
    let instance = config
        .engine_instances
        .iter_mut()
        .find(|instance| instance.id == engine_id)
        .ok_or_else(|| format!("Engine instance not found: {engine_id}"))?;
    instance.enabled = enabled;
    state.config_store.update(config)
}

#[tauri::command]
async fn install_agent_hook(
    state: tauri::State<'_, commands::AppState>,
    tool_name: String,
) -> Result<(), String> {
    if let Some(instance) = configured_engine_instance(&state, &tool_name) {
        configured_engine_adapter(&instance)
            .install_hooks()
            .map_err(|error| error.to_string())?;
        persist_engine_enabled(&state, &instance.id, true)?;
        return Ok(());
    }
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

    if adapter.name() == "claude-code" {
        if let Some(warning) = commands::check_bare_mode() {
            log::warn!("Claude Code bare mode detected: {}", warning);
        }
    }
    if adapter.name() == "gemini" {
        commands::ensure_gemini_folder_trust();
    }

    if let Err(e) = state.config_store.mark_agent_enabled(adapter.name()) {
        log::warn!(
            "Failed to persist enabled-agent intent for {}: {}",
            adapter.name(),
            e
        );
    }
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
    if let Some(instance) = configured_engine_instance(&state, &tool_name) {
        configured_engine_adapter(&instance)
            .remove_hooks()
            .map_err(|error| error.to_string())?;
        persist_engine_enabled(&state, &instance.id, false)?;
        return Ok(());
    }
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
    if let Err(e) = state.config_store.mark_agent_disabled(adapter.name()) {
        log::warn!(
            "Failed to clear enabled-agent intent for {}: {}",
            adapter.name(),
            e
        );
    }
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
    if let Some(instance) = configured_engine_instance(&state, &tool_name) {
        let profile = agents::profiles::claude_code_profile();
        agents::profiles::save_event_selection(&profile, &enabled_events)
            .map_err(|error| error.to_string())?;
        configured_engine_adapter(&instance)
            .install_hooks()
            .map_err(|error| error.to_string())?;
        persist_engine_enabled(&state, &instance.id, true)?;
        return Ok(());
    }
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
    adapter.install_hooks().map_err(|e| e.to_string())?;
    if let Err(e) = state.config_store.mark_agent_enabled(adapter.name()) {
        log::warn!(
            "Failed to persist enabled-agent intent for {}: {}",
            adapter.name(),
            e
        );
    }
    Ok(())
}

#[tauri::command]
async fn get_all_hook_status(
    state: tauri::State<'_, commands::AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let primary_claude_settings = agents::claude_code::default_config_root().join("settings.json");
    let mut results: Vec<serde_json::Value> = state
        .adapters
        .iter()
        .filter(|adapter| {
            adapter.name() != "claude-code"
                || adapter.hook_config_paths().first() == Some(&primary_claude_settings)
        })
        .map(|a| {
            let tool_id = a.name().to_string();
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
            let bridge_command = profile
                .as_ref()
                .and_then(|profile| agents::profiles::managed_bridge_command(profile).ok());
            let bridge_path = profile
                .as_ref()
                .map(|_| display_path_with_home(&agents::hook_manager::bridge_binary_path()));
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
                "bridgeCommand": bridge_command,
                "bridgePath": bridge_path,
            })
        })
        .collect();

    let cfg = state.config_store.get();
    for instance in &cfg.engine_instances {
        let root = agents::claude_code::expand_tilde(&instance.config_root);
        let settings_path = root.join("settings.json");
        let profile = agents::profiles::claude_code_profile();
        let hook_health = agents::profiles::install_health(&profile, &settings_path);
        let installed = hook_health.is_present();
        let root_string = root.display().to_string();
        let bridge_command = agents::profiles::managed_bridge_command_labeled(
            &profile,
            Some(&instance.label),
            Some(&root_string),
        )
        .ok();
        results.push(serde_json::json!({
            "toolId": format!("engine:{}", instance.id),
            "adapterId": "claude-code",
            "profileId": "claude-code",
            "name": "claude-code",
            "displayName": instance.label,
            "installed": installed,
            "installStatus": hook_health.as_status_str(),
            "configPath": display_path_with_home(&settings_path),
            "configDir": display_path_with_home(&root),
            "status": if root.is_dir() { "Available" } else { "Unavailable" },
            "supportsEventSelection": agents::profiles::supports_event_selection(&profile),
            "events": agents::profiles::event_statuses(&profile),
            "enabledEventNames": agents::profiles::selected_event_names(&profile),
            "bridgeCommand": bridge_command,
            "bridgePath": display_path_with_home(&agents::hook_manager::bridge_binary_path()),
            "isCustom": true,
            "customId": instance.id,
        }));
    }

    // Append custom hook installs from config
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
        let bridge_command = profile.as_ref().and_then(|profile| {
            agents::profiles::managed_bridge_command_labeled(
                profile,
                Some(&entry.display_name),
                Some(&base_dir.display().to_string()),
            )
            .ok()
        });
        let bridge_path = profile
            .as_ref()
            .map(|_| display_path_with_home(&agents::hook_manager::bridge_binary_path()));
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
            "bridgeCommand": bridge_command,
            "bridgePath": bridge_path,
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
            if let Err(e) = state.config_store.mark_agent_disabled(adapter.name()) {
                log::warn!(
                    "Failed to clear enabled-agent intent for {}: {}",
                    adapter.name(),
                    e
                );
            }
            let config = state.config_store.get();
            state
                .telemetry
                .record_hook_uninstall(&config, adapter.name())
                .await;
        }
    }
    // Keep user-defined custom hook registrations (display name + install directory)
    // so the user doesn't have to re-enter them after a bulk uninstall.
    // We only remove the injected hook files on disk.
    let cfg = state.config_store.get();
    for entry in &cfg.custom_hook_installs {
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
                state
                    .telemetry
                    .record_hook_uninstall(&cfg, &entry.profile_id)
                    .await;
            }
        }
    }
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
async fn probe_remote_host(
    state: tauri::State<'_, commands::AppState>,
    id: String,
) -> Result<remote::installer::RemoteProbeReport, String> {
    let hosts = state.remote_manager.hosts();
    let host = hosts
        .iter()
        .find(|h| h.id == id)
        .ok_or_else(|| format!("Host {} not found", id))?
        .clone();
    Ok(remote::installer::RemoteInstaller::probe_host(&host).await)
}

#[tauri::command]
async fn remote_skill_manager_invoke(
    state: tauri::State<'_, commands::AppState>,
    id: String,
    command: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let host = state
        .remote_manager
        .hosts()
        .into_iter()
        .find(|host| host.id == id)
        .ok_or_else(|| format!("Host {id} not found"))?;
    if command == "open_skill_path"
        || command == "reveal_skill_path"
        || command == "open_system_path"
    {
        let target = remote::skill_manager::invoke(&host, &command, args).await?;
        let path = target
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Remote path response is missing path".to_string())?;
        let parent = target
            .get("parentPath")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Remote path response is missing parentPath".to_string())?;
        let name = target
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let is_directory = target
            .get("isDirectory")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let reveal = command == "reveal_skill_path";
        let directory = if is_directory && !reveal {
            path
        } else {
            parent
        };
        let target_name = (!is_directory || reveal).then_some(name);
        remote::terminal::launch_at_path(&host, directory, target_name)?;
        return Ok(serde_json::Value::Null);
    }
    if command == "get_skill_explanation_cmd" {
        let skill_id = args
            .get("skillId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Missing skillId".to_string())?;
        let lang = args
            .get("lang")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Missing lang".to_string())?;
        let cache_id = format!("remote:{}:{skill_id}", host.id);
        let mut explanation = skills::explanation::get_cached(&cache_id, lang);
        if let Some(ref mut value) = explanation {
            value.skill_id = skill_id.to_string();
        }
        return serde_json::to_value(explanation).map_err(|error| error.to_string());
    }
    if command == "generate_skill_explanation_cmd" {
        let skill_id = args
            .get("skillId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Missing skillId".to_string())?;
        let skill_path = args
            .get("skillPath")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Missing skillPath".to_string())?;
        let lang = args
            .get("lang")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Missing lang".to_string())?;
        let refresh = args
            .get("refresh")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let file_path = if skill_path.ends_with(".md") {
            skill_path.to_string()
        } else {
            format!("{}/SKILL.md", skill_path.trim_end_matches('/'))
        };
        let content = remote::skill_manager::invoke(
            &host,
            "read_skill_file_content",
            serde_json::json!({ "filePath": file_path }),
        )
        .await?
        .as_str()
        .ok_or_else(|| "Remote SKILL.md was not text".to_string())?
        .to_string();
        let temp_path = std::env::temp_dir().join(format!(
            "agentbro-remote-skill-explanation-{}.md",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&temp_path, content)
            .map_err(|error| format!("Write remote Skill explanation input: {error}"))?;
        let cache_id = format!("remote:{}:{skill_id}", host.id);
        let generated = skills::explanation::generate(
            &cache_id,
            &temp_path.display().to_string(),
            lang,
            refresh,
        );
        let _ = std::fs::remove_file(&temp_path);
        let mut explanation = generated?;
        explanation.skill_id = skill_id.to_string();
        return serde_json::to_value(explanation).map_err(|error| error.to_string());
    }
    remote::skill_manager::invoke(&host, &command, args).await
}

#[tauri::command]
fn probe_codex_app_server() -> agents::codex::CodexAppServerProbe {
    agents::codex::probe_app_server_readiness()
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
    #[cfg(target_os = "windows")]
    {
        get_cursor_position_sync()
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        get_cursor_position_sync()
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

fn monitor_uses_primary_origin(monitor: &tauri::Monitor) -> bool {
    let pos = monitor.position();
    pos.x == 0 && pos.y == 0
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
    drain_pool(|| {
        let (cursor_x, cursor_y) = app_cursor_position(&app)?;
        let position = window.outer_position().map_err(|e| e.to_string())?;
        let scale = window.scale_factor().unwrap_or(1.0).max(1.0);

        let monitor = window.current_monitor().ok().flatten();
        if label == "pet" {
            if let Some(monitor) = monitor.as_ref() {
                if !monitor_uses_primary_origin(monitor) {
                    return Ok(true);
                }
            }
        }

        let monitor_origin_logical = monitor
            .map(|m| {
                let s = m.scale_factor().max(1.0);
                (m.position().x as f64 / s, m.position().y as f64 / s)
            })
            .unwrap_or((0.0, 0.0));

        let cx_logical = cursor_x / scale - monitor_origin_logical.0;
        let cy_logical = cursor_y / scale - monitor_origin_logical.1;
        let win_left_logical = position.x as f64 / scale - monitor_origin_logical.0;
        let win_top_logical = position.y as f64 / scale - monitor_origin_logical.1;

        Ok(zones.iter().any(|r| {
            let zone_left = win_left_logical + r.left;
            let zone_top = win_top_logical + r.top;
            cx_logical >= zone_left
                && cx_logical <= zone_left + r.width
                && cy_logical >= zone_top
                && cy_logical <= zone_top + r.height
        }))
    })
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

    drain_pool(|| {
        let (cursor_x, cursor_y) = app_cursor_position(&app)?;
        let position = window.outer_position().map_err(|e| e.to_string())?;
        let size = window.outer_size().map_err(|e| e.to_string())?;
        let scale = window.scale_factor().unwrap_or(1.0).max(1.0);

        let monitor = window.current_monitor().ok().flatten();
        let monitor_origin_logical = monitor
            .map(|m| {
                let s = m.scale_factor().max(1.0);
                (m.position().x as f64 / s, m.position().y as f64 / s)
            })
            .unwrap_or((0.0, 0.0));

        let window_left_logical = position.x as f64 / scale - monitor_origin_logical.0;
        let window_top_logical = position.y as f64 / scale - monitor_origin_logical.1;
        let window_width_logical = size.width as f64 / scale;
        let window_height_logical = size.height as f64 / scale;

        let cursor_x_logical = cursor_x / scale - monitor_origin_logical.0;
        let cursor_y_logical = cursor_y / scale - monitor_origin_logical.1;

        let hit_width = width
            .filter(|value| *value > 0.0)
            .unwrap_or(window_width_logical);
        let hit_height = height
            .filter(|value| *value > 0.0)
            .unwrap_or(window_height_logical);

        let anchor_offset_x = anchor_offset_x.unwrap_or(0.0);
        let left = window_left_logical
            + ((window_width_logical - hit_width) / 2.0).max(0.0)
            + anchor_offset_x;
        let top = window_top_logical;
        let right = left + hit_width.min(window_width_logical);
        let bottom = top + hit_height.min(window_height_logical);

        Ok(cursor_x_logical >= left
            && cursor_x_logical <= right
            && cursor_y_logical >= top
            && cursor_y_logical <= bottom)
    })
}

#[cfg(target_os = "macos")]
fn get_cursor_position_sync() -> Result<(f64, f64), String> {
    use objc2_app_kit::NSEvent;
    let point = NSEvent::mouseLocation();
    Ok((point.x, point.y))
}

#[cfg(target_os = "windows")]
fn get_cursor_position_sync() -> Result<(f64, f64), String> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    let ok = unsafe { GetCursorPos(&mut point) };
    if ok == 0 {
        return Err("Failed to read Windows cursor position".to_string());
    }
    Ok((point.x as f64, point.y as f64))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn get_cursor_position_sync() -> Result<(f64, f64), String> {
    Err("Cursor position not supported on this platform".to_string())
}

fn app_cursor_position(app: &tauri::AppHandle) -> Result<(f64, f64), String> {
    match app.cursor_position() {
        Ok(cursor) => Ok((cursor.x, cursor.y)),
        Err(error) => {
            #[cfg(target_os = "windows")]
            {
                get_cursor_position_sync()
                    .map_err(|fallback| format!("{}; Windows fallback failed: {}", error, fallback))
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(error.to_string())
            }
        }
    }
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

pub(crate) fn refresh_skill_pack_tray_menu(app: &tauri::AppHandle) -> Result<(), String> {
    let language = app
        .try_state::<AppState>()
        .map(|state| state.config_store.get().language)
        .unwrap_or_else(|| "en".to_string());
    let menu = menu_bar::build_tray_menu(app, &language).map_err(|error| error.to_string())?;
    let tray = app
        .tray_by_id(menu_bar::TRAY_ID)
        .ok_or_else(|| "AgentBro tray icon is unavailable".to_string())?;
    tray.set_menu(Some(menu)).map_err(|error| error.to_string())
}

const SKILL_PACK_PICKER_WIDTH: f64 = 360.0;
const SKILL_PACK_PICKER_HEIGHT: f64 = 460.0;

fn show_skill_pack_picker(app: &tauri::AppHandle) -> Result<(), String> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let window = match handle.get_webview_window(menu_bar::SKILL_PACK_PICKER_ID) {
            Some(existing) => existing,
            None => match build_skill_pack_picker_window(&handle) {
                Ok(window) => window,
                Err(error) => {
                    log::warn!("Failed to create skill pack picker: {error}");
                    return;
                }
            },
        };
        position_skill_pack_picker(&window);
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("skill-pack-picker-shown", ());
    })
    .map_err(|error| error.to_string())
}

fn build_skill_pack_picker_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    let window = tauri::WebviewWindowBuilder::new(
        app,
        menu_bar::SKILL_PACK_PICKER_ID,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("AgentBro Skill Packs")
    .inner_size(SKILL_PACK_PICKER_WIDTH, SKILL_PACK_PICKER_HEIGHT)
    .transparent(true)
    .decorations(false)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .focused(false)
    .accept_first_mouse(true)
    .background_color(tauri::webview::Color(0, 0, 0, 0))
    .visible(false)
    .build()
    .map_err(|error| format!("skill pack picker window: {error}"))?;

    let window_for_event = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(false) = event {
            let _ = window_for_event.hide();
        }
    });

    Ok(window)
}

fn position_skill_pack_picker(window: &tauri::WebviewWindow) {
    let anchor = tray_icon_rect().lock().ok().and_then(|value| *value);
    let monitors = window.available_monitors().unwrap_or_default();
    let monitor = anchor
        .and_then(|rect| {
            monitors.iter().find(|monitor| {
                let position = rect.position.to_physical::<f64>(monitor.scale_factor());
                let monitor_position = monitor.position();
                let monitor_size = monitor.size();
                position.x >= monitor_position.x as f64
                    && position.x < (monitor_position.x + monitor_size.width as i32) as f64
                    && position.y >= monitor_position.y as f64
                    && position.y < (monitor_position.y + monitor_size.height as i32) as f64
            })
        })
        .cloned()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let window_size = window.outer_size().unwrap_or_else(|_| {
        tauri::PhysicalSize::new(
            (SKILL_PACK_PICKER_WIDTH * scale).round() as u32,
            (SKILL_PACK_PICKER_HEIGHT * scale).round() as u32,
        )
    });
    let (anchor_x, anchor_bottom) = anchor
        .map(|rect| {
            let position = rect.position.to_physical::<f64>(scale);
            let size = rect.size.to_physical::<f64>(scale);
            (position.x + size.width / 2.0, position.y + size.height)
        })
        .unwrap_or_else(|| {
            (
                monitor_position.x as f64 + monitor_size.width as f64 / 2.0,
                monitor_position.y as f64 + 28.0 * scale,
            )
        });
    let inset = 8.0 * scale;
    let min_x = monitor_position.x as f64 + inset;
    let max_x =
        monitor_position.x as f64 + monitor_size.width as f64 - window_size.width as f64 - inset;
    let min_y = monitor_position.y as f64 + inset;
    let max_y =
        monitor_position.y as f64 + monitor_size.height as f64 - window_size.height as f64 - inset;
    let x = (anchor_x - window_size.width as f64 / 2.0).clamp(min_x, max_x.max(min_x));
    let y = (anchor_bottom + 6.0 * scale).clamp(min_y, max_y.max(min_y));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        x.round() as i32,
        y.round() as i32,
    )));
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
    let codex_app_server = state.codex_app_server.clone();
    let hook_server = state.hook_server.clone();
    let session_store = state.session_store.clone();

    tauri::async_runtime::spawn(async move {
        match codex_app_server
            .respond_permission(&session_id, allowed, false)
            .await
        {
            Ok(true) => return,
            Ok(false) => {}
            Err(err) => log::warn!(
                "Global permission shortcut Codex app-server response failed for {}: {}",
                session_id,
                err
            ),
        }
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
    let codex_app_server = state.codex_app_server.clone();
    let session_store = state.session_store.clone();

    tauri::async_runtime::spawn(async move {
        if let Some(question) = session.pending_question.as_ref() {
            if question.source.as_deref() == Some("codex_app_server_request_user_input")
                && question.response_mode.as_deref() == Some("app_server")
            {
                let mut answers = std::collections::BTreeMap::new();
                let answer_id = question
                    .questions
                    .first()
                    .and_then(|item| item.id.clone())
                    .unwrap_or_else(|| question.question.clone());
                answers.insert(answer_id, vec![answer.clone()]);
                match codex_app_server
                    .respond_question(&session_id, answers)
                    .await
                {
                    Ok(true) => return,
                    Ok(false) => {}
                    Err(err) => log::warn!(
                        "Global question shortcut Codex app-server response failed for {}: {}",
                        session_id,
                        err
                    ),
                }
            }
        }
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
const SETTINGS_DEFAULT_WIDTH: f64 = 1420.0;
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
    }

    // `window.center()` can silently fail on invisible windows or when the
    // display layout has changed. Compute the position explicitly from the
    // current monitor's work area.
    center_on_current_monitor(window, SETTINGS_DEFAULT_WIDTH, SETTINGS_DEFAULT_HEIGHT);
}

fn center_on_current_monitor(window: &tauri::WebviewWindow, width: f64, height: f64) {
    let monitor = find_cursor_monitor(window)
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());

    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let work_area = monitor.work_area();
        let screen_w = work_area.size.width as f64 / scale;
        let screen_h = work_area.size.height as f64 / scale;
        let screen_x = work_area.position.x as f64 / scale;
        let screen_y = work_area.position.y as f64 / scale;

        let x = screen_x + (screen_w - width) / 2.0;
        let y = screen_y + (screen_h - height) / 2.0;

        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
            x.max(screen_x),
            y.max(screen_y),
        )));
    } else {
        let _ = window.center();
    }
}

fn find_cursor_monitor(window: &tauri::WebviewWindow) -> Option<tauri::Monitor> {
    let (cx, cy) = get_cursor_position_sync().ok()?;
    let monitors = window.available_monitors().ok()?;
    monitors.into_iter().find(|m| {
        let s = m.scale_factor();
        let pos = m.position();
        let size = m.size();
        let x = pos.x as f64 / s;
        let y = pos.y as f64 / s;
        let w = size.width as f64 / s;
        let h = size.height as f64 / s;
        cx >= x && cx < x + w && cy >= y && cy < y + h
    })
}

fn show_settings_window(app: &tauri::AppHandle) -> Result<(), String> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Some(notch) = handle.get_webview_window("notch") {
            let _ = notch.set_ignore_cursor_events(true);
        }
        let _ = handle.emit("settings-window-opened", ());

        #[cfg(target_os = "macos")]
        {
            let _ = handle.set_activation_policy(tauri::ActivationPolicy::Regular);
        }

        // Create the settings webview on demand. The window was previously
        // declared in tauri.conf.json with `visible: false`, which spun the
        // process up at app launch (~170 MB resident, idle forever). Building
        // it lazily here means we only pay the cost the first time the user
        // opens settings, and `attach_settings_close_handler` rigs it to
        // `destroy()` on close so the process exits when they leave.
        let window = match handle.get_webview_window("settings") {
            Some(existing) => existing,
            None => match build_settings_window(&handle) {
                Ok(w) => w,
                Err(e) => {
                    log::warn!("Failed to create settings window: {e}");
                    return;
                }
            },
        };

        normalize_settings_window_frame(&window);
        apply_settings_window_for_spaces(&window);
        let _ = window.show();
        let _ = window.set_focus();

        #[cfg(target_os = "macos")]
        activate_agentbro_app();
        focus_settings_window_native(&window);
    })
    .map_err(|e| e.to_string())
}

/// Build the settings webview from scratch. Mirrors the descriptor that used
/// to live in `tauri.conf.json::app.windows[settings]`. Kept in one place so
/// the parameters don't drift between create-and-show paths.
fn build_settings_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    let window = tauri::WebviewWindowBuilder::new(
        app,
        "settings",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("AgentBro")
    .inner_size(SETTINGS_DEFAULT_WIDTH, SETTINGS_DEFAULT_HEIGHT)
    .min_inner_size(SETTINGS_MIN_WIDTH, SETTINGS_MIN_HEIGHT)
    .center()
    .background_color(tauri::webview::Color(0, 0, 0, 0))
    .visible(false)
    .build()
    .map_err(|e| format!("settings window: {e}"))?;

    attach_settings_close_handler(app, &window);
    Ok(window)
}

/// Wire the settings webview's close-button to `destroy()` so the process
/// actually exits when the user is done. Previously the handler called
/// `prevent_close` + `hide`, which left the renderer + GPU XPC processes
/// resident (~170 MB) for the lifetime of the app.
fn attach_settings_close_handler(app: &tauri::AppHandle, window: &tauri::WebviewWindow) {
    let app_handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { .. } = event {
            #[cfg(target_os = "macos")]
            let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
            restore_island_surface_after_settings_close(&app_handle);
            // Don't `prevent_close()`: let Tauri tear the webview down. The
            // next `show_settings_window` call will rebuild it via
            // `build_settings_window`.
        }
    });
}

fn restore_island_surface_after_settings_close(app: &tauri::AppHandle) {
    let config = app.state::<AppState>().config_store.get();
    let is_pet_mode = config.island_surface_mode == "pet";
    let saved_origin = config.island_pet_window_origin.clone();
    sync_pet_window_visibility_inner(app, is_pet_mode, saved_origin.as_ref());
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

#[tauri::command]
async fn is_homebrew_install() -> Result<bool, String> {
    Ok(is_homebrew_install_path())
}

fn is_homebrew_install_path() -> bool {
    if PathBuf::from("/opt/homebrew/Caskroom/agentbro").exists()
        || PathBuf::from("/usr/local/Caskroom/agentbro").exists()
    {
        return true;
    }

    std::env::current_exe()
        .ok()
        .map(|path| path.canonicalize().unwrap_or(path))
        .map(|path| {
            let path = path.to_string_lossy();
            path.contains("/opt/homebrew/Caskroom/agentbro/")
                || path.contains("/usr/local/Caskroom/agentbro/")
        })
        .unwrap_or(false)
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
    config.sound_pack = if pack == "custom" {
        "custom".to_string()
    } else {
        sound_pack.to_string()
    };
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
async fn import_sound_pack(
    state: tauri::State<'_, AppState>,
    pack_path: String,
) -> Result<SoundPackImportResult, String> {
    let result =
        sound::import_openpeon_sound_pack(&PathBuf::from(&pack_path), &custom_sounds_dir())?;
    let imported_configs = result
        .imported_sounds
        .iter()
        .map(|sound| CustomSoundConfig {
            id: sound.id.clone(),
            name: sound.name.clone(),
            path: sound.path.clone(),
            data_url: None,
        });

    let mut config = state.config_store.get();
    config.custom_sounds.extend(imported_configs);
    config.sound_pack = "custom".to_string();

    for rule in &result.applied_rules {
        let enabled = config
            .sound_rules
            .get(&rule.event_id)
            .map(|rule| rule.enabled)
            .or_else(|| config.sound_events.get(&rule.event_id).copied())
            .unwrap_or(true);
        let sound = format!("custom:{}", rule.sound_id);
        config.sound_events.insert(rule.event_id.clone(), enabled);
        config
            .sound_rules
            .insert(rule.event_id.clone(), SoundRuleConfig { enabled, sound });
    }

    if let Some(ref engine) = state.sound_engine {
        engine.set_sound_pack(SoundPack::Synth);
        engine.set_custom_sounds(
            config
                .custom_sounds
                .iter()
                .map(|sound| (sound.id.clone(), sound.path.clone()))
                .collect(),
        );
        for rule in &result.applied_rules {
            if let Some(event) = SoundEvent::from_id(&rule.event_id) {
                if let Some(rule_config) = config.sound_rules.get(&rule.event_id) {
                    engine.set_event_rule(event, rule_config.enabled, rule_config.sound.clone());
                }
            }
        }
    }

    state.config_store.update(config)?;
    Ok(result)
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
    bundles.sort_by_key(|a| a.name.to_lowercase());
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
async fn preview_github_repo_import(
    repo_url: String,
    github_token: Option<String>,
) -> Result<serde_json::Value, String> {
    let repo = normalize_github_repo_ref(&repo_url)?;
    let previews =
        skills::installer::preview_github_skills_with_token(&repo.spec, github_token.as_deref())?;
    let manager = skills::v2::service()?;
    manager.scan_center_into_db()?;
    let installed_ids = manager
        .list_center_skills()?
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
    github_token: Option<String>,
) -> Result<serde_json::Value, String> {
    let repo = normalize_github_repo_ref(&repo_url)?;
    let manager = skills::v2::service()?;
    let center_path = manager.center_path()?;
    let skipped = selections
        .iter()
        .filter(|selection| {
            selection
                .get("resolution")
                .and_then(|value| value.as_str())
                .unwrap_or("overwrite")
                == "skip"
        })
        .map(|selection| {
            selection
                .get("sourcePath")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .collect::<Vec<_>>();
    let active_selections = selections
        .iter()
        .filter(|selection| {
            selection
                .get("resolution")
                .and_then(|value| value.as_str())
                .unwrap_or("overwrite")
                != "skip"
        })
        .collect::<Vec<_>>();
    let started_at = std::time::Instant::now();
    log::info!(
        "GitHub Skill import started: selected={}, skipped={}",
        active_selections.len(),
        skipped.len()
    );
    let mut imported = Vec::new();
    if !active_selections.is_empty() {
        let clone_source = github_repo_clone_source(&repo);
        let (repo_root, temp_root) = skills::installer::resolve_external_skill_source_with_token(
            &clone_source,
            github_token.as_deref(),
        )?;
        let install_result = (|| {
            for selection in active_selections {
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
                let rename_to = selection
                    .get("renamedSkillId")
                    .or_else(|| selection.get("renamed_skill_id"))
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let source = github_import_source(&repo.spec, &source_path);
                let local_source =
                    resolve_github_import_skill_path(&repo_root, &repo.spec, &source_path)?;
                let (original_skill_id, imported_skill_id) = install_github_skill_into_center(
                    &manager,
                    &local_source,
                    &source,
                    resolution,
                    rename_to,
                )?;
                imported.push(serde_json::json!({
                    "sourcePath": source_path,
                    "originalSkillId": original_skill_id,
                    "importedSkillId": imported_skill_id,
                    "skillName": imported_skill_id,
                    "targetDirectory": center_path.join(&imported_skill_id).display().to_string(),
                    "resolution": resolution,
                }));
            }
            Ok::<(), String>(())
        })();
        if let Some(root) = temp_root {
            let _ = std::fs::remove_dir_all(root);
        }
        if let Err(error) = &install_result {
            log::warn!("GitHub Skill import failed: {error}");
        }
        install_result?;
    }
    log::info!(
        "GitHub Skill import completed: imported={}, skipped={}, elapsed_ms={}",
        imported.len(),
        skipped.len(),
        started_at.elapsed().as_millis()
    );
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

fn install_github_skill_into_center(
    manager: &skills::v2::service::Service,
    local_source: &Path,
    source_uri: &str,
    resolution: &str,
    rename_to: Option<&str>,
) -> Result<(String, String), String> {
    let original_skill_id = skills::v2::fsutil::infer_skill_id(local_source);
    if original_skill_id.is_empty() {
        return Err("GitHub Skill id cannot be empty".to_string());
    }
    let proposed_skill_id = match resolution {
        "overwrite" => original_skill_id.clone(),
        "rename" => rename_to
            .map(sanitize_skill_directory_name)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Renamed GitHub Skill id cannot be empty".to_string())?,
        other => return Err(format!("Unsupported GitHub Skill resolution: {other}")),
    };
    let decision = skills::v2::models::AddCenterSkillDecision {
        skill_id: original_skill_id.clone(),
        proposed_skill_id: Some(proposed_skill_id.clone()),
        resolution: if resolution == "rename" {
            "create".to_string()
        } else {
            "update".to_string()
        },
    };
    let result = manager.execute_add_center_skill(
        skills::v2::models::AddCenterSkillInput {
            source_path: local_source.display().to_string(),
            source_type: "github".to_string(),
            source_uri: Some(source_uri.to_string()),
            imported_from_agent: None,
            imported_from_path: None,
            multi: Some(false),
            import_mode: Some("copy".to_string()),
        },
        vec![decision],
    )?;
    let imported_skill_id = result
        .skill_ids
        .first()
        .or_else(|| result.updated.first())
        .cloned()
        .ok_or_else(|| format!("GitHub Skill '{}' was not imported", original_skill_id))?;
    Ok((original_skill_id, imported_skill_id))
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

fn github_repo_clone_source(repo: &GithubRepoRefCompat) -> String {
    if repo.branch == "HEAD" {
        format!("github:{}/{}", repo.owner, repo.repo)
    } else {
        format!("github:{}/{}/tree/{}", repo.owner, repo.repo, repo.branch)
    }
}

fn resolve_github_import_skill_path(
    repo_root: &Path,
    repo_spec: &str,
    source_path: &str,
) -> Result<PathBuf, String> {
    let import_source = github_import_source(repo_spec, source_path);
    let relative_path = github_import_repo_path(&import_source)?;
    let canonical_root = repo_root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve cloned GitHub repository: {error}"))?;
    let candidate = if relative_path.as_os_str().is_empty() {
        canonical_root.clone()
    } else {
        canonical_root.join(relative_path)
    };
    let canonical_candidate = candidate.canonicalize().map_err(|error| {
        format!(
            "GitHub Skill path '{}' was not found in the cloned repository: {error}",
            source_path
        )
    })?;
    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(format!(
            "GitHub Skill path '{}' resolves outside the cloned repository",
            source_path
        ));
    }
    if !canonical_candidate.join("SKILL.md").is_file() {
        return Err(format!(
            "GitHub Skill path '{}' does not contain SKILL.md",
            source_path
        ));
    }
    Ok(canonical_candidate)
}

fn github_import_repo_path(import_source: &str) -> Result<PathBuf, String> {
    let spec = import_source
        .strip_prefix("github:")
        .ok_or_else(|| format!("Invalid GitHub import source: {import_source}"))?;
    let parts = spec
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(format!("Invalid GitHub import source: {import_source}"));
    }
    let path_parts = if parts.get(2) == Some(&"tree") {
        parts.get(4..).unwrap_or_default()
    } else {
        parts.get(2..).unwrap_or_default()
    };
    if path_parts.iter().any(|part| matches!(*part, "." | "..")) {
        return Err("GitHub Skill path cannot contain '.' or '..' components".to_string());
    }
    let mut path = PathBuf::new();
    for part in path_parts {
        path.push(part);
    }
    Ok(path)
}

fn github_import_source(repo_spec: &str, source_path: &str) -> String {
    let path = normalize_github_import_path(source_path);
    if path.is_empty() || path == "." {
        format!("github:{repo_spec}")
    } else if let Some(spec) = rebase_github_import_spec(repo_spec, &path) {
        format!("github:{spec}")
    } else {
        format!("github:{repo_spec}/{path}")
    }
}

fn normalize_github_import_path(path: &str) -> String {
    path.trim()
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn rebase_github_import_spec(repo_spec: &str, source_path: &str) -> Option<String> {
    let spec = repo_spec.trim().trim_matches('/');
    let parts = spec
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    if parts.len() >= 5 && parts[2] == "tree" {
        let base_path = normalize_github_import_path(&parts[4..].join("/"));
        if repo_path_contains(&base_path, source_path) {
            return Some(format!(
                "{}/{}/tree/{}/{}",
                parts[0], parts[1], parts[3], source_path
            ));
        }
        return None;
    }
    if parts[2] != "tree" {
        let base_path = normalize_github_import_path(&parts[2..].join("/"));
        if repo_path_contains(&base_path, source_path) {
            return Some(format!("{}/{}/{}", parts[0], parts[1], source_path));
        }
    }
    None
}

fn repo_path_contains(base_path: &str, source_path: &str) -> bool {
    source_path == base_path
        || source_path
            .strip_prefix(base_path)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod github_import_source_tests {
    use super::{github_import_repo_path, github_import_source, install_github_skill_into_center};
    use crate::skills::v2::models::SettingsUpdate;
    use crate::skills::v2::service::Service;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn specific_tree_skill_path_is_not_appended_twice() {
        assert_eq!(
            github_import_source(
                "vercel-labs/skills/tree/main/skills/find-skills",
                "skills/find-skills",
            ),
            "github:vercel-labs/skills/tree/main/skills/find-skills",
        );
    }

    #[test]
    fn tree_parent_path_uses_preview_repo_path_without_repeating_parent() {
        assert_eq!(
            github_import_source("vercel-labs/skills/tree/main/skills", "skills/find-skills"),
            "github:vercel-labs/skills/tree/main/skills/find-skills",
        );
    }

    #[test]
    fn shorthand_subpath_uses_preview_repo_path_without_repeating_parent() {
        assert_eq!(
            github_import_source("owner/repo/skills", "skills/find-skills"),
            "github:owner/repo/skills/find-skills",
        );
    }

    #[test]
    fn relative_preview_path_still_appends_to_repo_spec() {
        assert_eq!(
            github_import_source("vercel-labs/skills/tree/main/skills", "find-skills"),
            "github:vercel-labs/skills/tree/main/skills/find-skills",
        );
    }

    #[test]
    fn import_source_maps_to_one_repo_relative_path() {
        assert_eq!(
            github_import_repo_path("github:vercel-labs/skills/tree/main/skills/find-skills")
                .unwrap(),
            PathBuf::from("skills/find-skills"),
        );
        assert_eq!(
            github_import_repo_path("github:owner/repo/skills/find-skills").unwrap(),
            PathBuf::from("skills/find-skills"),
        );
    }

    #[test]
    fn import_source_rejects_parent_traversal() {
        assert!(github_import_repo_path("github:owner/repo/skills/../secret").is_err());
    }

    #[test]
    fn github_install_writes_to_fixed_agentbro_center() {
        let _lock = crate::skills::lock_shared_test_home();
        let previous_home = std::env::var_os("HOME");
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_home = std::env::temp_dir().join(format!("agentbro-github-center-{suffix}"));
        fs::create_dir_all(&temp_home).unwrap();
        std::env::set_var("HOME", &temp_home);

        let test_result = (|| -> Result<(String, bool, bool, bool, bool), String> {
            let source = temp_home.join("repo").join("skills").join("github-skill");
            fs::create_dir_all(&source).map_err(|error| error.to_string())?;
            fs::write(
                source.join("SKILL.md"),
                "---\nname: github-skill\ndescription: GitHub import test\n---\n",
            )
            .map_err(|error| error.to_string())?;
            let requested_center = temp_home.join("custom-center");
            let fixed_center = temp_home.join(".agentbro").join("skills");
            let sqlite = temp_home.join("skill-manager.db");
            let manager = Service::new(&sqlite, temp_home.clone())?;
            manager.update_settings(SettingsUpdate {
                center_path: Some(requested_center.display().to_string()),
                sqlite_path: Some(sqlite.display().to_string()),
                default_distribute_mode: None,
                link_fail_policy: None,
                startup_scan: None,
                show_unmanaged: None,
                auto_sync_skill_packs: None,
            })?;
            manager.init()?;

            let (_, imported_skill_id) = install_github_skill_into_center(
                &manager,
                &source,
                "github:owner/repo/skills/github-skill",
                "overwrite",
                None,
            )?;
            let listed = manager
                .list_center_skills()?
                .into_iter()
                .any(|skill| skill.id == imported_skill_id);
            Ok((
                imported_skill_id.clone(),
                fixed_center
                    .join(&imported_skill_id)
                    .join("SKILL.md")
                    .is_file(),
                requested_center
                    .join(&imported_skill_id)
                    .join("SKILL.md")
                    .is_file(),
                temp_home.join(".agents/skills").exists(),
                listed,
            ))
        })();

        if let Some(home) = previous_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(&temp_home);

        let (
            imported_skill_id,
            exists_in_fixed_center,
            wrote_requested_center,
            wrote_legacy_agents_path,
            listed,
        ) = test_result.unwrap();
        assert_eq!(imported_skill_id, "github-skill");
        assert!(exists_in_fixed_center);
        assert!(!wrote_requested_center);
        assert!(!wrote_legacy_agents_path);
        assert!(listed);
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
async fn list_mcp_inventory_cmd(
    agent: String,
) -> Result<skills::mcp_management::McpInventory, String> {
    skills::mcp_management::list_mcp_servers(&agent)
}

#[tauri::command]
async fn validate_mcp_server_draft_cmd(
    agent: String,
    server: skills::mcp_management::McpServerDraft,
    original_name: Option<String>,
) -> Result<skills::mcp_management::McpValidationResultV2, String> {
    skills::mcp_management::validate_mcp_server_draft(&agent, &server, original_name.as_deref())
}

#[tauri::command]
async fn save_mcp_server_cmd(
    agent: String,
    server: skills::mcp_management::McpServerDraft,
    original_name: Option<String>,
    revision: String,
) -> Result<skills::mcp_management::McpInventory, String> {
    skills::mcp_management::save_mcp_server(&agent, original_name.as_deref(), &revision, &server)
}

#[tauri::command]
async fn set_mcp_server_enabled_cmd(
    agent: String,
    server_name: String,
    revision: String,
    enabled: bool,
) -> Result<skills::mcp_management::McpInventory, String> {
    skills::mcp_management::set_mcp_server_enabled(&agent, &server_name, &revision, enabled)
}

#[tauri::command]
async fn delete_mcp_server_v2_cmd(
    agent: String,
    server_name: String,
    revision: String,
) -> Result<skills::mcp_management::McpInventory, String> {
    skills::mcp_management::delete_mcp_server(&agent, &server_name, &revision)
}

#[tauri::command]
async fn test_mcp_server_connection_cmd(
    agent: String,
    server_name: String,
) -> Result<skills::mcp_management::McpConnectionTestResult, String> {
    skills::mcp_management::test_mcp_server_connection(&agent, &server_name).await
}

#[tauri::command]
async fn inspect_mcp_server_cmd(
    agent: String,
    server_name: String,
    inspection_id: String,
) -> Result<skills::mcp_management::McpInspectionReport, String> {
    skills::mcp_management::inspect_mcp_server(&agent, &server_name, &inspection_id).await
}

#[tauri::command]
async fn cancel_mcp_inspection_cmd(inspection_id: String) -> Result<(), String> {
    skills::mcp_management::cancel_mcp_inspection(&inspection_id)
}

#[tauri::command]
async fn call_mcp_tool_cmd(
    agent: String,
    server_name: String,
    operation_id: String,
    tool_name: String,
    arguments: serde_json::Value,
) -> Result<skills::mcp_management::McpOperationResult, String> {
    skills::mcp_management::call_mcp_tool(
        &agent,
        &server_name,
        &operation_id,
        &tool_name,
        arguments,
    )
    .await
}

#[tauri::command]
async fn get_mcp_prompt_cmd(
    agent: String,
    server_name: String,
    operation_id: String,
    prompt_name: String,
    arguments: serde_json::Value,
) -> Result<skills::mcp_management::McpOperationResult, String> {
    skills::mcp_management::get_mcp_prompt(
        &agent,
        &server_name,
        &operation_id,
        &prompt_name,
        arguments,
    )
    .await
}

#[tauri::command]
async fn cancel_mcp_operation_cmd(operation_id: String) -> Result<(), String> {
    skills::mcp_management::cancel_mcp_operation(&operation_id)
}

#[tauri::command]
async fn toggle_skill_cmd(skill_id: String, agent: String, enabled: bool) -> Result<(), String> {
    skills::installer::toggle_skill(&skill_id, &agent, enabled)
}

#[tauri::command]
async fn read_skill_files(skill_path: String) -> Result<skills::FileTreeNode, String> {
    Ok(skills::scanner::read_file_tree(&skill_path))
}

#[tauri::command]
async fn read_skill_file_content(file_path: String) -> Result<String, String> {
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
    board: Option<String>,
) -> Result<Vec<skills::marketplace::MarketplaceSkill>, String> {
    skills::marketplace::search_marketplace_skills_async(registry_id, query, board).await
}

#[tauri::command]
async fn fetch_marketplace_skill_detail(
    source: String,
    skill_id: String,
) -> Result<skills::marketplace::MarketplaceSkillDetail, String> {
    skills::marketplace::fetch_skills_sh_skill_detail(source, skill_id).await
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
unsafe extern "C" fn display_reconfig_callback(
    _display: u32,
    _flags: u32,
    _user_info: *mut std::ffi::c_void,
) {
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
                if let Some(window) = h.get_webview_window("pet") {
                    apply_pet_window_for_spaces(&window);
                }
            });
        });
    }
}

// ── Opacity helpers ─────────────────────────────────────────────
// macOS breaks transparent window compositing after a hide()/show() cycle,
// so it keeps true alpha. Windows falls back to hide/show to avoid leaving a
// disabled invisible island as a topmost transparent hit target.

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

#[cfg(target_os = "windows")]
fn set_window_alpha(window: &tauri::WebviewWindow, alpha: f64) {
    if alpha <= 0.0 {
        let _ = window.hide();
    } else {
        let _ = window.show();
    }
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
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

#[cfg(target_os = "windows")]
fn apply_notch_window_for_spaces(window: &tauri::WebviewWindow) {
    let _ = window.set_always_on_top(true);
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn apply_notch_window_for_spaces(_window: &tauri::WebviewWindow) {}

#[cfg(target_os = "macos")]
fn apply_pet_window_for_spaces(window: &tauri::WebviewWindow) {
    use objc2_app_kit::{NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior};

    let _ = window.set_always_on_top(true);
    let _ = window.set_visible_on_all_workspaces(true);
    if let Ok(ptr) = window.ns_window() {
        unsafe {
            let ns_window = ptr as *const NSWindow;
            let mut behavior = (*ns_window).collectionBehavior();

            behavior &= !(NSWindowCollectionBehavior::Primary
                | NSWindowCollectionBehavior::Auxiliary
                | NSWindowCollectionBehavior::Managed
                | NSWindowCollectionBehavior::Transient
                | NSWindowCollectionBehavior::FullScreenPrimary
                | NSWindowCollectionBehavior::FullScreenNone
                | NSWindowCollectionBehavior::FullScreenAllowsTiling
                | NSWindowCollectionBehavior::FullScreenDisallowsTiling
                | NSWindowCollectionBehavior::MoveToActiveSpace
                | NSWindowCollectionBehavior::ParticipatesInCycle
                | NSWindowCollectionBehavior::Stationary);
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

#[cfg(target_os = "windows")]
fn apply_pet_window_for_spaces(window: &tauri::WebviewWindow) {
    let _ = window.set_always_on_top(true);
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn apply_pet_window_for_spaces(_window: &tauri::WebviewWindow) {}

fn configure_notch_window_for_spaces(app: &tauri::AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("notch") {
            apply_notch_window_for_spaces(&window);
        }
    });
}

fn configure_pet_window_for_spaces(app: &tauri::AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("pet") {
            apply_pet_window_for_spaces(&window);
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
    config.display_id = display_id.clone();
    state.config_store.update(config)?;
    reposition_notch_to_display(&app, Some(display_id.clone()), None)?;
    Ok(())
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
const PET_SLOT_SIZE_LOGICAL: f64 = 160.0;
const PET_ANCHOR_RIGHT_LOGICAL: f64 = 132.0;
const PET_ANCHOR_BOTTOM_LOGICAL: f64 = 44.0;

#[derive(Debug, Clone, Copy)]
struct PetStageAnchor {
    left: bool,
    top: bool,
}

fn pet_stage_anchor_from_config(anchor: &config::PetWindowAnchor) -> PetStageAnchor {
    PetStageAnchor {
        left: anchor.left,
        top: anchor.top,
    }
}

fn pet_stage_anchor_to_config(anchor: PetStageAnchor) -> config::PetWindowAnchor {
    config::PetWindowAnchor {
        left: anchor.left,
        top: anchor.top,
    }
}

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
    if !is_pet_mode {
        // Leaving pet mode: destroy the webview entirely instead of hiding it.
        // The descriptor lives in `build_pet_window`, so the next switch back
        // to pet mode recreates it. This drops ~74 MB resident plus the
        // associated WebKit XPC processes when the user is back on the notch.
        if let Some(pet_window) = handle.get_webview_window("pet") {
            let _ = pet_window.destroy();
        }
        if let Some(notch_window) = handle.get_webview_window("notch") {
            let _ = notch_window.show();
            apply_notch_window_for_spaces(&notch_window);
        }
        return;
    }

    let pet_window = match handle.get_webview_window("pet") {
        Some(existing) => existing,
        None => match build_pet_window(handle) {
            Ok(w) => w,
            Err(e) => {
                log::warn!("Failed to create pet window: {e}");
                return;
            }
        },
    };

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
    apply_pet_window_for_spaces(&pet_window);
    let _ = pet_window.show();
    apply_pet_window_for_spaces(&pet_window);
}

/// Build the pet webview on demand. Mirrors the descriptor that used to live
/// in `tauri.conf.json::app.windows[pet]`. We rebuild it every time the user
/// switches into pet mode rather than parking the process idle on the notch.
fn build_pet_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    tauri::WebviewWindowBuilder::new(app, "pet", tauri::WebviewUrl::App("index.html".into()))
        .title("AgentBro Pet")
        .inner_size(820.0, 360.0)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .accept_first_mouse(true)
        .background_color(tauri::webview::Color(0, 0, 0, 0))
        .visible(false)
        .build()
        .map_err(|e| format!("pet window: {e}"))
}

fn position_pet_window(
    window: &tauri::WebviewWindow,
    monitor: &tauri::Monitor,
    width: f64,
    height: f64,
    saved_origin: Option<&config::WindowOrigin>,
) {
    if let Some(origin) = saved_origin {
        if origin.x.is_finite()
            && origin.y.is_finite()
            && pet_origin_is_visible_on_any_monitor(window, width, height, origin.x, origin.y)
        {
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
                origin.x.round() as i32,
                origin.y.round() as i32,
            )));
            apply_pet_window_for_spaces(window);
            return;
        }
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
    apply_pet_window_for_spaces(window);
}

fn pet_origin_is_visible_on_any_monitor(
    window: &tauri::WebviewWindow,
    window_width: f64,
    window_height: f64,
    x: f64,
    y: f64,
) -> bool {
    let Ok(monitors) = window.available_monitors() else {
        return false;
    };
    monitors.into_iter().any(|monitor| {
        let pos = monitor.position();
        let size = monitor.size();
        pet_window_rect_has_visible_area(
            Rect {
                x: pos.x as f64,
                y: pos.y as f64,
                width: size.width as f64,
                height: size.height as f64,
            },
            Rect {
                x,
                y,
                width: window_width,
                height: window_height,
            },
        )
    })
}

#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn pet_window_rect_has_visible_area(monitor: Rect, window: Rect) -> bool {
    if monitor.width <= 0.0 || monitor.height <= 0.0 || window.width <= 0.0 || window.height <= 0.0
    {
        return false;
    }

    let visible_width =
        (window.x + window.width).min(monitor.x + monitor.width) - window.x.max(monitor.x);
    let visible_height =
        (window.y + window.height).min(monitor.y + monitor.height) - window.y.max(monitor.y);
    let required_width = window.width.min(64.0);
    let required_height = window.height.min(64.0);

    visible_width >= required_width && visible_height >= required_height
}

fn clamp_point_into_rect(rect: Rect, x: f64, y: f64, margin: f64) -> (f64, f64) {
    let min_x = rect.x + margin;
    let max_x = rect.x + rect.width - margin;
    let min_y = rect.y + margin;
    let max_y = rect.y + rect.height - margin;
    (
        x.clamp(min_x, max_x.max(min_x)),
        y.clamp(min_y, max_y.max(min_y)),
    )
}

fn distance_point_to_rect(rect: Rect, x: f64, y: f64) -> f64 {
    let right = rect.x + rect.width;
    let bottom = rect.y + rect.height;
    let dx = if x < rect.x {
        rect.x - x
    } else if x > right {
        x - right
    } else {
        0.0
    };
    let dy = if y < rect.y {
        rect.y - y
    } else if y > bottom {
        y - bottom
    } else {
        0.0
    };
    dx.hypot(dy)
}

#[cfg(test)]
mod pet_window_tests {
    use super::{
        clamp_point_into_rect, distance_point_to_rect, pet_window_rect_has_visible_area, Rect,
    };

    #[test]
    fn saved_pet_window_origin_must_leave_visible_area_on_screen() {
        assert!(!pet_window_rect_has_visible_area(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1728.0,
                height: 1117.0,
            },
            Rect {
                x: 2185.0,
                y: -1098.0,
                width: 820.0,
                height: 360.0,
            },
        ));
    }

    #[test]
    fn saved_pet_window_origin_can_live_on_monitor_above_primary() {
        assert!(pet_window_rect_has_visible_area(
            Rect {
                x: 0.0,
                y: -1117.0,
                width: 1728.0,
                height: 1117.0,
            },
            Rect {
                x: 864.0,
                y: -1098.0,
                width: 820.0,
                height: 360.0,
            },
        ));
    }

    #[test]
    fn clamp_pulls_offscreen_point_back_inside_rect() {
        let monitor = Rect {
            x: 0.0,
            y: 0.0,
            width: 1728.0,
            height: 1117.0,
        };
        let (cx, cy) = clamp_point_into_rect(monitor, -500.0, 9999.0, 24.0);
        assert_eq!(cx, 24.0);
        assert_eq!(cy, 1117.0 - 24.0);
    }

    #[test]
    fn clamp_leaves_onscreen_point_unchanged() {
        let monitor = Rect {
            x: 0.0,
            y: 0.0,
            width: 1728.0,
            height: 1117.0,
        };
        let (cx, cy) = clamp_point_into_rect(monitor, 500.0, 500.0, 24.0);
        assert_eq!(cx, 500.0);
        assert_eq!(cy, 500.0);
    }

    #[test]
    fn nearest_rect_is_the_one_closer_to_an_offscreen_point() {
        let primary = Rect {
            x: 0.0,
            y: 0.0,
            width: 1728.0,
            height: 1117.0,
        };
        let above = Rect {
            x: 0.0,
            y: -1117.0,
            width: 1728.0,
            height: 1117.0,
        };
        // A point far above the primary screen sits closer to the upper monitor.
        assert!(
            distance_point_to_rect(above, 864.0, -1500.0)
                < distance_point_to_rect(primary, 864.0, -1500.0)
        );
    }
}

fn current_pet_scale_percent(app: &tauri::AppHandle) -> f64 {
    app.state::<AppState>()
        .config_store
        .get()
        .island_pet_scale
        .clamp(10, 120) as f64
}

fn pet_rect_in_window(
    window_width: f64,
    window_height: f64,
    window_scale: f64,
    pet_scale_percent: f64,
    anchor: PetStageAnchor,
) -> (f64, f64, f64) {
    let scale = window_scale.max(1.0);
    let display_scale = (pet_scale_percent / 100.0).clamp(0.1, 1.2);
    let pet_size = PET_SLOT_SIZE_LOGICAL * display_scale * scale;
    let pet_left = if anchor.left {
        PET_ANCHOR_RIGHT_LOGICAL * scale
    } else {
        window_width - PET_ANCHOR_RIGHT_LOGICAL * scale - pet_size
    };
    let pet_top = if anchor.top {
        PET_ANCHOR_BOTTOM_LOGICAL * scale
    } else {
        window_height - PET_ANCHOR_BOTTOM_LOGICAL * scale - pet_size
    };
    (pet_left, pet_top, pet_size)
}

fn pet_stage_anchor_for_origin(
    monitor: &tauri::Monitor,
    window_width: f64,
    window_height: f64,
    window_scale: f64,
    pet_scale_percent: f64,
    x: f64,
    y: f64,
) -> PetStageAnchor {
    let default_anchor = PetStageAnchor {
        left: false,
        top: false,
    };
    let (pet_left, pet_top, pet_size) = pet_rect_in_window(
        window_width,
        window_height,
        window_scale,
        pet_scale_percent,
        default_anchor,
    );
    let pet_center_x = x + pet_left + pet_size / 2.0;
    let pet_center_y = y + pet_top + pet_size / 2.0;
    let pos = monitor.position();
    let size = monitor.size();
    PetStageAnchor {
        left: pet_center_x < pos.x as f64 + size.width as f64 / 2.0,
        top: pet_center_y < pos.y as f64 + size.height as f64 / 2.0,
    }
}

fn pet_stage_anchor_for_center(
    monitor: &tauri::Monitor,
    center_x: f64,
    center_y: f64,
) -> PetStageAnchor {
    let pos = monitor.position();
    let size = monitor.size();
    PetStageAnchor {
        left: center_x < pos.x as f64 + size.width as f64 / 2.0,
        top: center_y < pos.y as f64 + size.height as f64 / 2.0,
    }
}

fn monitor_containing_point(app: &tauri::AppHandle, x: f64, y: f64) -> Option<tauri::Monitor> {
    app.available_monitors().ok()?.into_iter().find(|monitor| {
        let pos = monitor.position();
        let size = monitor.size();
        let left = pos.x as f64;
        let top = pos.y as f64;
        let right = left + size.width as f64;
        let bottom = top + size.height as f64;
        x >= left && x < right && y >= top && y < bottom
    })
}

fn monitor_rect(monitor: &tauri::Monitor) -> Rect {
    let pos = monitor.position();
    let size = monitor.size();
    Rect {
        x: pos.x as f64,
        y: pos.y as f64,
        width: size.width as f64,
        height: size.height as f64,
    }
}

fn clamp_point_into_monitor(monitor: &tauri::Monitor, x: f64, y: f64) -> (f64, f64) {
    let scale = monitor.scale_factor().max(1.0);
    clamp_point_into_rect(monitor_rect(monitor), x, y, 24.0 * scale)
}

fn nearest_monitor_for_point(app: &tauri::AppHandle, x: f64, y: f64) -> Option<tauri::Monitor> {
    app.available_monitors().ok()?.into_iter().min_by(|a, b| {
        let da = distance_point_to_rect(monitor_rect(a), x, y);
        let db = distance_point_to_rect(monitor_rect(b), x, y);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn monitor_for_pet_origin(
    app: &tauri::AppHandle,
    window_width: f64,
    window_height: f64,
    window_scale: f64,
    pet_scale_percent: f64,
    x: f64,
    y: f64,
) -> Option<tauri::Monitor> {
    let monitors = app.available_monitors().ok()?;
    for monitor in &monitors {
        let anchor = pet_stage_anchor_for_origin(
            monitor,
            window_width,
            window_height,
            window_scale,
            pet_scale_percent,
            x,
            y,
        );
        let (pet_left, pet_top, pet_size) = pet_rect_in_window(
            window_width,
            window_height,
            window_scale,
            pet_scale_percent,
            anchor,
        );
        let center_x = x + pet_left + pet_size / 2.0;
        let center_y = y + pet_top + pet_size / 2.0;
        let pos = monitor.position();
        let size = monitor.size();
        let left = pos.x as f64;
        let top = pos.y as f64;
        if center_x >= left
            && center_x < left + size.width as f64
            && center_y >= top
            && center_y < top + size.height as f64
        {
            return Some(monitor.clone());
        }
    }
    None
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
            let keep_dragging =
                drain_pool(|| update_notch_drag_position(&app_handle)).unwrap_or(false);
            if !keep_dragging {
                break;
            }
        }
    });

    Ok(true)
}

/// Run `f` inside a macOS autorelease pool so any NSScreen / NSWindow /
/// NSCursor objects created by Tauri's AppKit calls (e.g. `cursor_position`,
/// `available_monitors`, `outer_position`) drain before the next iteration.
/// On non-macOS this is a no-op. Used by the 60Hz drag loops to prevent
/// runaway accumulation of autoreleased NSDictionary instances.
#[inline]
fn drain_pool<T>(f: impl FnOnce() -> T) -> T {
    #[cfg(target_os = "macos")]
    {
        objc2::rc::autoreleasepool(|_pool| f())
    }
    #[cfg(not(target_os = "macos"))]
    {
        f()
    }
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
async fn start_pet_drag(
    app: tauri::AppHandle,
    anchor_left: Option<bool>,
    anchor_top: Option<bool>,
) -> Result<bool, String> {
    let Some(window) = app.get_webview_window("pet") else {
        return Ok(false);
    };
    let (cursor_x, cursor_y) = app_cursor_position(&app)?;
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let window_scale = window.scale_factor().unwrap_or(1.0);
    let pet_scale = current_pet_scale_percent(&app);
    let start_anchor = match (anchor_left, anchor_top) {
        (Some(left), Some(top)) => PetStageAnchor { left, top },
        _ => app
            .try_state::<AppState>()
            .and_then(|state| state.config_store.get().island_pet_window_anchor)
            .map(|anchor| pet_stage_anchor_from_config(&anchor))
            .or_else(|| {
                monitor_for_pet_origin(
                    &app,
                    size.width as f64,
                    size.height as f64,
                    window_scale,
                    pet_scale,
                    position.x as f64,
                    position.y as f64,
                )
                .map(|m| {
                    pet_stage_anchor_for_origin(
                        &m,
                        size.width as f64,
                        size.height as f64,
                        window_scale,
                        pet_scale,
                        position.x as f64,
                        position.y as f64,
                    )
                })
            })
            .unwrap_or(PetStageAnchor {
                left: false,
                top: false,
            }),
    };
    {
        let mut drag = pet_drag_state()
            .lock()
            .map_err(|e| format!("Pet drag lock error: {}", e))?;
        *drag = Some(PetDragState {
            start_cursor_x: cursor_x,
            start_cursor_y: cursor_y,
            start_window_x: position.x as f64,
            start_window_y: position.y as f64,
            current_x: position.x as f64,
            current_y: position.y as f64,
            native_drag: false,
            start_anchor,
        });
    }

    if window.start_dragging().is_ok() {
        if let Ok(mut drag) = pet_drag_state().lock() {
            if let Some(state) = drag.as_mut() {
                state.native_drag = true;
            }
        }
        return Ok(true);
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
            let keep_dragging =
                drain_pool(|| update_pet_drag_position(&app_handle)).unwrap_or(false);
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
    let (cursor_x, cursor_y) = app_cursor_position(app)?;
    let next_origin = {
        let drag = pet_drag_state()
            .lock()
            .map_err(|e| format!("Pet drag lock error: {}", e))?;
        let Some(state) = drag.as_ref() else {
            return Ok(false);
        };
        config::WindowOrigin {
            x: (state.start_window_x + cursor_x - state.start_cursor_x).round(),
            y: (state.start_window_y + cursor_y - state.start_cursor_y).round(),
        }
    };

    {
        let mut drag = pet_drag_state()
            .lock()
            .map_err(|e| format!("Pet drag lock error: {}", e))?;
        let Some(state) = drag.as_mut() else {
            return Ok(false);
        };
        if (next_origin.x - state.current_x).abs() < 1.0
            && (next_origin.y - state.current_y).abs() < 1.0
        {
            return Ok(true);
        }
        state.current_x = next_origin.x;
        state.current_y = next_origin.y;
    }

    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        next_origin.x as i32,
        next_origin.y as i32,
    )));
    Ok(true)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PetDragResult {
    origin: crate::config::WindowOrigin,
    anchor_left: bool,
    anchor_top: bool,
}

#[tauri::command]
async fn end_pet_drag(app: tauri::AppHandle) -> Result<Option<PetDragResult>, String> {
    let drag_snapshot = {
        let mut drag = pet_drag_state()
            .lock()
            .map_err(|e| format!("Pet drag lock error: {}", e))?;
        drag.take()
    };
    let Some(snap) = drag_snapshot else {
        return Ok(None);
    };
    let mut origin = crate::config::WindowOrigin {
        x: snap.current_x.round(),
        y: snap.current_y.round(),
    };
    let mut result_anchor = snap.start_anchor;

    if let Some(window) = app.get_webview_window("pet") {
        if snap.native_drag {
            if let Ok(position) = window.outer_position() {
                origin.x = position.x as f64;
                origin.y = position.y as f64;
            }
        }
        if let Ok(size) = window.outer_size() {
            let window_scale = window.scale_factor().unwrap_or(1.0).max(1.0);
            let pet_scale = current_pet_scale_percent(&app);
            let w = size.width as f64;
            let h = size.height as f64;
            let (old_left, old_top, old_size) =
                pet_rect_in_window(w, h, window_scale, pet_scale, snap.start_anchor);
            let pet_center_x = origin.x + old_left + old_size / 2.0;
            let pet_center_y = origin.y + old_top + old_size / 2.0;
            if let Some(monitor) = monitor_containing_point(&app, pet_center_x, pet_center_y) {
                let new_anchor = pet_stage_anchor_for_center(&monitor, pet_center_x, pet_center_y);
                if snap.start_anchor.left != new_anchor.left
                    || snap.start_anchor.top != new_anchor.top
                {
                    let (new_left, new_top, new_size) =
                        pet_rect_in_window(w, h, window_scale, pet_scale, new_anchor);
                    origin.x = (pet_center_x - new_left - new_size / 2.0).round();
                    origin.y = (pet_center_y - new_top - new_size / 2.0).round();
                }
                result_anchor = new_anchor;
            } else if let Some(monitor) =
                nearest_monitor_for_point(&app, pet_center_x, pet_center_y)
            {
                // The OS-native drag has no screen bounds, so a pet can land
                // entirely off-screen. Pull its center back inside the closest
                // monitor before committing the position so it stays reachable.
                let (cx, cy) = clamp_point_into_monitor(&monitor, pet_center_x, pet_center_y);
                let new_anchor = pet_stage_anchor_for_center(&monitor, cx, cy);
                let (new_left, new_top, new_size) =
                    pet_rect_in_window(w, h, window_scale, pet_scale, new_anchor);
                origin.x = (cx - new_left - new_size / 2.0).round();
                origin.y = (cy - new_top - new_size / 2.0).round();
                result_anchor = new_anchor;
            }
        }
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            origin.x as i32,
            origin.y as i32,
        )));
    }

    let state = app.state::<AppState>();
    let mut config = state.config_store.get();
    config.island_pet_window_origin = Some(origin.clone());
    config.island_pet_window_anchor = Some(pet_stage_anchor_to_config(result_anchor));
    state.config_store.update(config)?;

    Ok(Some(PetDragResult {
        origin,
        anchor_left: result_anchor.left,
        anchor_top: result_anchor.top,
    }))
}

#[tauri::command]
async fn reset_pet_position(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut config = state.config_store.get();
    config.island_pet_window_origin = None;
    config.island_pet_window_anchor = None;
    state.config_store.update(config.clone())?;
    // With no saved origin, sync_pet_window_visibility -> position_pet_window
    // drops the pet back at its default bottom-right spot. A no-op when the
    // app is not in pet mode (the window is destroyed instead).
    sync_pet_window_visibility(&app, &config);
    Ok(())
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

            // Settings window is created on demand by `show_settings_window`.
            // No setup work needed here — it doesn't exist at launch anymore.

            // Initialize session store
            let mut session_store = SessionStore::new();
            session_store.set_app_handle(app.handle().clone());
            let session_store = Arc::new(session_store);

            // Initialize config store
            let mut config_store = ConfigStore::new();
            config_store.set_app_handle(app.handle().clone());
            let should_show_onboarding = !config_store.get().analytics_consent_prompt_completed;

            // Pet window: position to the saved (or default) corner and show
            // only when pet surface mode is active. The pet lives in its own
            // Tauri window so dragging it doesn't move the island shell.
            sync_pet_window_visibility(app.handle(), &config_store.get());

            // Cursor-monitor tracker: a single background poller that emits
            // `cursor-monitor-changed` to the frontend (notch) only on real
            // monitor transitions.
            platform::monitor_tracker::start(app.handle().clone());

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
                tauri::async_runtime::spawn_blocking(move || {
                    theme::scanner::seed_builtin_themes(&resource_path);
                });
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
            //
            // Beyond Claude Code, we also re-assert hooks for any agent the user
            // previously enabled (persisted intent in `enabled_agents`). This is
            // the startup safety net for the case where an external tool — e.g.
            // cc-switch swapping providers — overwrote the agent's settings file
            // while AgentBro was not running and wiped our hooks. The live
            // recovery watcher handles the same case while running.
            let startup_adapters = adapters.clone();
            let startup_config_store = config_store.clone();
            tauri::async_runtime::spawn_blocking(move || {
                if ClaudeCodeAdapter::update_hook_script_if_needed() {
                    log::info!("Hook script was updated to match new app version");
                }
                let enabled_intent = startup_config_store.get().enabled_agents;
                for adapter in startup_adapters.iter() {
                    let name = adapter.name();
                    let is_claude = name == "claude-code";
                    if !is_claude && !enabled_intent.iter().any(|a| a == name) {
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
                        if let Err(e) = startup_config_store.mark_agent_enabled(name) {
                            log::warn!(
                                "Failed to persist enabled-agent intent for {}: {}",
                                name,
                                e
                            );
                        }
                    }
                }
            });

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
            hooks::recovery::start_hook_recovery(
                adapters.clone(),
                app.handle().clone(),
                config_store.clone(),
            );

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
            let tray_menu = menu_bar::build_tray_menu(app, &config_store.get().language)?;

            let tray_icon = TrayIconBuilder::with_id(menu_bar::TRAY_ID)
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .tooltip("AgentBro")
                .icon(menu_bar_icon())
                .icon_as_template(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button,
                        button_state,
                        rect,
                        ..
                    } = event
                    {
                        if let Ok(mut anchor) = tray_icon_rect().lock() {
                            *anchor = Some(rect);
                        }
                        if button == MouseButton::Left && button_state == MouseButtonState::Up {
                            if let Err(error) = show_skill_pack_picker(tray.app_handle()) {
                                log::warn!("Failed to show skill pack picker: {error}");
                            }
                        }
                    }
                })
                .on_menu_event(|app, event| {
                    let menu_id = event.id().as_ref();
                    if menu_id == menu_bar::SKILL_PACK_PICKER_ID {
                        if let Err(error) = show_skill_pack_picker(app) {
                            log::warn!("Failed to show skill pack picker: {error}");
                        }
                        return;
                    }
                    match menu_id {
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
                    }
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

            let codex_app_server = Arc::new(commands::CodexAppServerBridge::new());
            commands::register_codex_app_server_bridge(codex_app_server.clone());

            hooks::claude_desktop_watcher::start(session_store.clone(), app.handle().clone());
            hooks::doubao_watcher::start(session_store.clone());
            commands::start_codex_app_server_background_sync(
                config_store.clone(),
                session_store.clone(),
                codex_app_server.clone(),
            );

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
            commands::start_remote_codex_state_sync(
                config_store.clone(),
                session_store.clone(),
                remote_manager.clone(),
            );

            // Initialize diagnostic ring buffer
            let diagnostic_buffer = Arc::new(hooks::diagnostics::DiagnosticRingBuffer::new());
            let network_monitor = Arc::new(NetworkMonitor::new());

            let switch_db = Arc::new(switch::db::SwitchDatabase::open().unwrap_or_else(|err| {
                log::error!("Failed to open switch database: {err}");
                switch::db::SwitchDatabase::open_in_memory().unwrap_or_else(|fallback_err| {
                    log::error!("Failed to open in-memory switch database: {fallback_err}");
                    std::process::exit(1);
                })
            }));
            let telemetry = Arc::new(TelemetryService::new());

            let app_state = AppState {
                session_store,
                hook_server,
                codex_app_server,
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

            if let Err(error) = build_skill_pack_picker_window(app.handle()) {
                log::warn!("Failed to prewarm skill pack picker: {error}");
            }

            if let Err(err) = register_island_global_shortcuts(app.handle()) {
                log::warn!("Failed to register island global shortcuts: {}", err);
            }
            if should_show_onboarding {
                let _ = show_settings_window(app.handle());
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
            commands::get_app_state_flags,
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
            commands::get_chat_history_tail,
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
            import_sound_pack,
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
            reset_pet_position,
            pets::discover_pets,
            market::check_abpets_available,
            market::install_abpets_globally,
            market::install_pet_from_market,
            market::uninstall_pet_from_market,
            market::fetch_market_manifest,
            market::ping_market_download,
            commands::set_active_pet_id,
            commands::set_agent_default_pet,
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
            probe_remote_host,
            remote_skill_manager_invoke,
            probe_codex_app_server,
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
            is_homebrew_install,
            open_image,
            read_image_data_url,
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
            list_mcp_inventory_cmd,
            validate_mcp_server_draft_cmd,
            save_mcp_server_cmd,
            set_mcp_server_enabled_cmd,
            delete_mcp_server_v2_cmd,
            test_mcp_server_connection_cmd,
            inspect_mcp_server_cmd,
            cancel_mcp_inspection_cmd,
            call_mcp_tool_cmd,
            get_mcp_prompt_cmd,
            cancel_mcp_operation_cmd,
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
            fetch_marketplace_skill_detail,
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
            skills::v2::commands::skill_manager_bootstrap,
            skills::v2::commands::skill_manager_init,
            skills::v2::commands::skill_manager_overview,
            skills::v2::commands::skill_pack_picker_data,
            skills::v2::commands::skill_manager_refresh,
            skills::v2::commands::skill_manager_refresh_overview,
            skills::v2::commands::skill_manager_settings,
            skills::v2::commands::skill_manager_update_settings,
            skills::v2::commands::list_center_skills_v2,
            skills::v2::commands::get_skill_detail_v2,
            skills::v2::commands::preview_add_center_skill,
            skills::v2::commands::execute_add_center_skill,
            skills::v2::commands::execute_marketplace_skill_batch,
            skills::v2::commands::cancel_marketplace_skill_batch,
            skills::v2::commands::preview_delete_center_skill,
            skills::v2::commands::execute_delete_center_skill,
            skills::v2::commands::preview_delete_center_skills,
            skills::v2::commands::execute_delete_center_skills,
            skills::v2::commands::preview_distribute_skill,
            skills::v2::commands::execute_distribute_skill,
            skills::v2::commands::scan_agent_inventory,
            skills::v2::commands::preview_adopt_agent_skill,
            skills::v2::commands::execute_adopt_agent_skill,
            skills::v2::commands::execute_adopt_agent_skills,
            skills::v2::commands::takeover_center_agent_skills,
            skills::v2::commands::delete_unmanaged_agent_skill,
            skills::v2::commands::delete_unmanaged_agent_skills,
            skills::v2::commands::preview_sync_copy_target,
            skills::v2::commands::preview_copy_target_diff,
            skills::v2::commands::execute_sync_copy_target,
            skills::v2::commands::delete_skill_target_distribution,
            skills::v2::commands::delete_skill_target_distributions,
            skills::v2::commands::list_skill_packs_v2,
            skills::v2::commands::get_skill_pack_detail,
            skills::v2::commands::execute_upsert_skill_pack,
            skills::v2::commands::preview_delete_skill_pack,
            skills::v2::commands::execute_delete_skill_pack,
            skills::v2::commands::preview_apply_skill_pack,
            skills::v2::commands::execute_apply_skill_pack,
            skills::v2::commands::execute_sync_skill_pack_to_agents,
            skills::v2::commands::preview_remove_skill_pack_from_agent,
            skills::v2::commands::execute_remove_skill_pack_from_agent,
            skills::v2::commands::preview_remove_skill_from_pack,
            skills::v2::commands::execute_remove_skill_from_pack,
            skills::v2::commands::preview_move_direct_skill_to_pack,
            skills::v2::commands::execute_move_direct_skill_to_pack,
            skills::v2::commands::list_managed_agents_v2,
            skills::v2::commands::get_agent_detail_v2,
            skills::v2::commands::refresh_agent_skill_view_v2,
            skills::v2::commands::read_agent_config_file_v2,
            skills::v2::commands::write_agent_config_file_v2,
            skills::v2::commands::list_plugin_inventory_v2,
            skills::v2::commands::get_plugin_detail_v2,
            skills::v2::commands::read_plugin_file_v2,
            skills::v2::commands::set_plugin_enabled_v2,
            skills::v2::commands::list_unmanaged_v2,
            skills::v2::commands::list_agent_skill_inventory_v2,
            skills::v2::commands::list_skill_projects_v2,
            skills::v2::commands::add_skill_project_v2,
            skills::v2::commands::remove_skill_project_v2,
            skills::v2::commands::get_skill_project_detail_v2,
            skills::v2::commands::scan_skill_project_v2,
            skills::v2::commands::install_center_skills_to_project_v2,
            skills::v2::commands::install_skill_pack_to_project_v2,
            skills::v2::commands::run_skill_manager_diagnosis,
            skills::v2::commands::list_diagnosis_issues,
            skills::v2::commands::preview_fix_diagnosis_issue,
            skills::v2::commands::execute_fix_diagnosis_issue,
            skills::v2::commands::execute_safe_fixes,
            skills::v2::commands::skill_manager_export_snapshot,
            skills::v2::commands::skill_manager_get_snapshot,
            skills::v2::commands::open_skill_path,
            skills::v2::commands::reveal_skill_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AgentBro");
}
