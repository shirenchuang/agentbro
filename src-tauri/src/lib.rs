// Agent Island — Rust Backend Library
pub mod commands;
pub mod agents;
pub mod hooks;
pub mod config;
pub mod terminal;
pub mod license;
pub mod sound;
pub mod platform;
pub mod theme;

use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;

use agents::claude_code::ClaudeCodeAdapter;
use agents::AgentAdapter;
use commands::persistence::{save_sessions, load_sessions};
use commands::AppState;
use config::ConfigStore;
use hooks::conversation_parser::discover_active_sessions;
use hooks::file_watcher::ConversationWatcher;
use hooks::server::HookServer;
use hooks::session_store::SessionStore;
use license::LicenseManager;
use platform::display::{DisplayInfo, list_displays_inner, find_target_monitor};
use sound::{SoundEngine, SoundEvent};

// ── Display Controller Commands ─────────────────────────────────

#[tauri::command]
async fn get_display_level(state: tauri::State<'_, commands::AppState>) -> Result<platform::display_controller::DisplayLevel, String> {
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

// ── Theme Commands ──────────────────────────────────────────────

#[tauri::command]
async fn get_themes() -> Result<Vec<serde_json::Value>, String> {
    Ok(theme::scanner::scan_themes())
}

#[tauri::command]
async fn set_active_theme(state: tauri::State<'_, commands::AppState>, name: String) -> Result<(), String> {
    let mut config = state.config_store.get();
    config.theme = name;
    state.config_store.update(config)
}

// ── Suppression Commands ────────────────────────────────────────

#[tauri::command]
async fn should_suppress(state: tauri::State<'_, AppState>, session_id: String) -> Result<bool, String> {
    let session = state.session_store
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
            .args(["-e", r#"
                use framework "AppKit"
                set mouseLoc to current application's NSEvent's mouseLocation()
                set x to mouseLoc's x as real
                set y to mouseLoc's y as real
                return (x as text) & "," & (y as text)
            "#])
            .output()
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split(',').collect();
        if parts.len() == 2 {
            let x: f64 = parts[0].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
            let y: f64 = parts[1].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
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

/// Start a background task that polls cursor position and emits hot-zone events.
/// Hot zone = top-center of screen, +/-300px horizontally from center, within
/// menu bar height + 6px vertically.
fn start_cursor_hotzone_poller(app_handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut in_zone = false;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;

            // Get screen dimensions from the notch window's monitor
            let screen_width = {
                if let Some(window) = app_handle.get_webview_window("notch") {
                    let monitor = window.current_monitor()
                        .ok()
                        .flatten()
                        .or_else(|| window.primary_monitor().ok().flatten());
                    if let Some(m) = monitor {
                        m.size().width as f64 / m.scale_factor()
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            };

            // Get cursor position via osascript+AppKit (no pyobjc dependency)
            #[cfg(target_os = "macos")]
            let cursor = {
                let output = std::process::Command::new("osascript")
                    .args(["-e", r#"
                        use framework "AppKit"
                        set mouseLoc to current application's NSEvent's mouseLocation()
                        set x to mouseLoc's x as real
                        set y to mouseLoc's y as real
                        return (x as text) & "," & (y as text)
                    "#])
                    .output();
                match output {
                    Ok(o) if o.status.success() => {
                        let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
                        let parts: Vec<&str> = text.split(',').collect();
                        if parts.len() == 2 {
                            match (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                                (Ok(x), Ok(y)) => Some((x, y)),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            };

            #[cfg(not(target_os = "macos"))]
            let cursor: Option<(f64, f64)> = None;

            let (cx, cy) = match cursor {
                Some(pos) => pos,
                None => continue,
            };

            // Hot zone: top center, +/-300px horizontal, top 31px (menu bar 25 + 6) vertical
            let center_x = screen_width / 2.0;
            let now_in_zone = (cx - center_x).abs() <= 300.0 && cy <= 31.0;

            if now_in_zone && !in_zone {
                in_zone = true;
                let _ = app_handle.emit("cursor-in-zone", true);
            } else if !now_in_zone && in_zone {
                in_zone = false;
                let _ = app_handle.emit("cursor-in-zone", false);
            }
        }
    });
}

// ── Quit Command ────────────────────────────────────────────────

#[tauri::command]
async fn quit_app(app: tauri::AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

// ── Sound Commands ───────────────────────────────────────────────

#[tauri::command]
async fn play_sound(state: tauri::State<'_, AppState>, event: SoundEvent) -> Result<(), String> {
    if let Some(ref engine) = state.sound_engine {
        engine.play(event);
    }
    Ok(())
}

#[tauri::command]
async fn set_sound_volume(state: tauri::State<'_, AppState>, volume: f32) -> Result<(), String> {
    if let Some(ref engine) = state.sound_engine {
        engine.set_volume(volume);
    }
    Ok(())
}

#[tauri::command]
async fn set_sound_enabled(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    if let Some(ref engine) = state.sound_engine {
        engine.set_enabled(enabled);
    }
    Ok(())
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

#[tauri::command]
async fn set_notch_opacity(app: tauri::AppHandle, opacity: f64) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("notch") {
        set_window_alpha(&window, opacity);
    }
    Ok(())
}

// ── Display Commands ────────────────────────────────────────────

#[tauri::command]
async fn list_displays(app: tauri::AppHandle) -> Result<Vec<DisplayInfo>, String> {
    Ok(list_displays_inner(&app))
}

/// Resize the notch window dynamically from the frontend and re-center
/// on the display selected in config (falls back to primary).
#[tauri::command]
async fn resize_notch(app: tauri::AppHandle, width: f64, height: f64) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("notch") {
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));

        // Determine which monitor to center on from config
        let config_store = app.state::<AppState>();
        let display_id = config_store.config_store.get().display_id;

        let monitor = find_target_monitor(&app, &display_id)
            .or_else(|| window.current_monitor().ok().flatten())
            .or_else(|| window.primary_monitor().ok().flatten());

        if let Some(monitor) = monitor {
            let scale = monitor.scale_factor();
            let screen_width = monitor.size().width as f64 / scale;
            let monitor_x = monitor.position().x as f64 / scale;
            let x = monitor_x + (screen_width - width) / 2.0;
            let _ = window.set_position(tauri::Position::Logical(
                tauri::LogicalPosition::new(x, 0.0),
            ));
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Position notch window at top center
            if let Some(window) = app.get_webview_window("notch") {
                let monitor = window.current_monitor()
                    .ok()
                    .flatten()
                    .or_else(|| window.primary_monitor().ok().flatten());

                if let Some(monitor) = monitor {
                    let screen_width = monitor.size().width as f64 / monitor.scale_factor();
                    let window_width = 420.0; // 400 panel + 20 shadow padding
                    let x = (screen_width - window_width) / 2.0;
                    let _ = window.set_position(tauri::Position::Logical(
                        tauri::LogicalPosition::new(x, 0.0),
                    ));
                }

                // Set initial compact size (will be dynamically resized by frontend)
                let _ = window.set_size(tauri::Size::Logical(
                    tauri::LogicalSize::new(420.0, 52.0),
                ));

                // Force shadow off for clean transparent look
                let _ = window.set_shadow(false);

                // Set window level to overlap menu bar (same as Vibe Island)
                #[cfg(target_os = "macos")]
                {
                    use objc2_app_kit::NSWindow;
                    let ns_win = window.ns_window().unwrap() as *mut NSWindow;
                    unsafe {
                        // NSMainMenuWindowLevel (24) + 3 = 27, same as Vibe Island
                        ns_win.as_ref().unwrap().setLevel(27);
                    }
                }
            }

            // Ensure settings window is hidden on startup
            if let Some(settings_window) = app.get_webview_window("settings") {
                let _ = settings_window.hide();
            }

            // Initialize session store
            let mut session_store = SessionStore::new();
            session_store.set_app_handle(app.handle().clone());
            let session_store = Arc::new(session_store);

            // Bootstrap: discover sessions that were already running before we launched
            {
                let discovered = discover_active_sessions(std::time::Duration::from_secs(7200));
                if !discovered.is_empty() {
                    log::info!("Startup bootstrap: discovered {} active session(s)", discovered.len());
                    for ds in &discovered {
                        let _session = session_store.get_or_create_session(
                            &ds.session_id,
                            "claude-code",
                            &ds.project,
                            &ds.cwd,
                            "", // terminal unknown at startup
                        );
                        // Set session title if we extracted one
                        if let Some(ref title) = ds.session_title {
                            session_store.update_session(&ds.session_id, |s| {
                                s.session_title = Some(title.clone());
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

            // Initialize config store
            let mut config_store = ConfigStore::new();
            config_store.set_app_handle(app.handle().clone());

            // Initialize themes: ensure built-in themes exist in user dir
            if let Some(resource_path) = app.path().resource_dir().ok() {
                theme::scanner::ensure_builtin_themes(&resource_path);
            }

            // Initialize adapters: default ~/.claude + custom engine instances
            let default_cc = ClaudeCodeAdapter::new();
            let mut cc_adapters: Vec<ClaudeCodeAdapter> = vec![];
            {
                let config = config_store.get();
                for inst in &config.engine_instances {
                    if inst.enabled {
                        let root = agents::claude_code::expand_tilde(&inst.config_root);
                        cc_adapters.push(
                            ClaudeCodeAdapter::with_config_root(root, inst.label.clone()),
                        );
                    }
                }
            }

            // Update hook script if app was updated (compare embedded vs deployed)
            if ClaudeCodeAdapter::update_hook_script_if_needed() {
                log::info!("Hook script was updated to match new app version");
            }

            // Build the shared adapter list
            let mut adapter_list: Vec<Arc<dyn AgentAdapter>> = vec![
                Arc::new(default_cc),
            ];
            for cc in cc_adapters {
                adapter_list.push(Arc::new(cc));
            }
            let adapters: Arc<Vec<Arc<dyn AgentAdapter>>> = Arc::new(adapter_list);

            // Auto-install hooks on startup
            for adapter in adapters.iter() {
                if let Err(e) = adapter.install_hooks() {
                    log::warn!("Failed to install hooks for {}: {}", adapter.display_name(), e);
                } else {
                    log::info!("Hooks installed for {}", adapter.display_name());
                }
            }

            // Initialize and start hook server
            let hook_server = HookServer::new(session_store.clone(), adapters.clone());
            let hook_server = Arc::new(hook_server);
            hook_server.set_app_handle(app.handle().clone());

            // Start hook server in background
            let server = hook_server.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = server.start().await {
                    log::error!("Failed to start HookServer: {}", e);
                }
            });

            // Start hook auto-recovery watcher
            hooks::recovery::start_hook_recovery(adapters.clone(), app.handle().clone());

            // Initialize license manager
            let mut license_manager = LicenseManager::new();
            license_manager.set_app_handle(app.handle().clone());

            // Initialize sound engine and share with HookServer
            let sound_engine: Option<Arc<SoundEngine>> = SoundEngine::new().map(Arc::new);
            if let Some(ref engine) = sound_engine {
                log::info!("Sound engine initialized");
                let cfg = config_store.get();
                engine.set_volume(cfg.sound_volume);
                engine.set_enabled(cfg.sound_enabled);
                hook_server.set_sound_engine(engine.clone());
            } else {
                log::warn!("Sound engine failed to initialize (no audio output)");
            }

            // ── Background: display change listener ─────────────────
            {
                let app_handle = app.handle().clone();
                let cfg_store = config_store.clone();
                tauri::async_runtime::spawn(async move {
                    let mut prev_count: usize = 0;
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        let displays = list_displays_inner(&app_handle);
                        let count = displays.len();
                        if count != prev_count {
                            prev_count = count;
                            log::info!("Display configuration changed: {} monitors", count);

                            // Check if selected display is still connected
                            let display_id = cfg_store.get().display_id;
                            if display_id != "primary"
                                && !displays.iter().any(|d| d.name == display_id)
                            {
                                log::warn!(
                                    "Selected display '{}' disconnected, falling back to primary",
                                    display_id
                                );
                            }

                            // Emit event so frontend can react
                            let _ = app_handle.emit("display-changed", &displays);
                        }
                    }
                });
            }

            // ── Background: cursor hot-zone detection ─────────────────
            start_cursor_hotzone_poller(app.handle().clone());

            // ── Background: fullscreen detection ────────────────────
            {
                let app_handle = app.handle().clone();
                let cfg_store = config_store.clone();
                tauri::async_runtime::spawn(async move {
                    let mut was_fullscreen = false;
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                        let cfg = cfg_store.get();
                        if !cfg.hide_in_fullscreen {
                            if was_fullscreen {
                                // Config changed — restore opacity
                                if let Some(w) = app_handle.get_webview_window("notch") {
                                    set_window_alpha(&w, 1.0);
                                }
                                was_fullscreen = false;
                            }
                            continue;
                        }

                        let is_fs = platform::fullscreen::check_fullscreen();
                        if is_fs != was_fullscreen {
                            was_fullscreen = is_fs;
                            // Emit event so frontend can also react
                            let _ = app_handle.emit("fullscreen-changed", is_fs);
                            if let Some(w) = app_handle.get_webview_window("notch") {
                                let opacity = if is_fs { 0.0 } else { 1.0 };
                                set_window_alpha(&w, opacity);
                                log::info!(
                                    "Fullscreen {}: notch opacity -> {}",
                                    if is_fs { "entered" } else { "exited" },
                                    if is_fs { 0.0 } else { 1.0 }
                                );
                            }
                        }
                    }
                });
            }

            // Build system tray icon with menu
            let show_item = MenuItemBuilder::with_id("show", "Show Panel").build(app)?;
            let settings_item = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .item(&settings_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .menu(&tray_menu)
                .icon(app.default_window_icon().cloned().unwrap_or_else(|| {
                    tauri::image::Image::new(&[], 0, 0)
                }))
                .icon_as_template(true)
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("notch") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "settings" => {
                            if let Some(window) = app.get_webview_window("settings") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
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
                config.engine_instances.iter()
                    .filter(|i| i.enabled)
                    .map(|i| agents::claude_code::expand_tilde(&i.config_root))
                    .collect()
            };
            let conversation_watcher = ConversationWatcher::start_with_roots(app.handle().clone(), &extra_roots);
            if conversation_watcher.is_some() {
                log::info!("Conversation watcher started");
            }
            let conversation_watcher = Arc::new(std::sync::Mutex::new(conversation_watcher));

            // Store shared state for Tauri commands
            // Initialize display controller
            let display_controller = Arc::new(platform::display_controller::DisplayController::new());
            display_controller.set_app_handle(app.handle().clone());

            let app_state = AppState {
                session_store,
                hook_server,
                config_store,
                adapters: (*adapters).clone(),
                license_manager,
                sound_engine,
                conversation_watcher,
                display_controller,
            };
            app.manage(app_state);

            log::info!("Agent Island started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            quit_app,
            commands::get_sessions,
            commands::respond_permission,
            commands::respond_auto_approve,
            commands::send_message,
            commands::jump_to_terminal,
            commands::get_config,
            commands::update_config,
            commands::install_hooks,
            commands::remove_hooks,
            commands::get_adapter_status,
            commands::verify_hooks,
            commands::is_terminal_focused,
            commands::get_license_status,
            commands::activate_license,
            commands::deactivate_license,
            commands::get_chat_history,
            commands::export_diagnostics,
            commands::add_engine_instance,
            commands::remove_engine_instance,
            commands::verify_engine_path,
            resize_notch,
            play_sound,
            set_sound_volume,
            set_sound_enabled,
            set_notch_opacity,
            list_displays,
            should_suppress,
            get_cursor_position,
            save_sessions,
            load_sessions,
            get_themes,
            set_active_theme,
            get_display_level,
            notify_cursor_enter,
            notify_cursor_leave,
            notify_esc,
            notify_expand,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Agent Island");
}
