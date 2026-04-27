use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::Mutex;

use crate::agents::AgentAdapter;

const MAX_RESTORES_PER_MINUTE: u32 = 3;

pub struct HookRecovery {
    restore_count: AtomicU32,
    window_start: Mutex<Instant>,
    disabled: AtomicU32,
}

impl HookRecovery {
    pub fn new() -> Self {
        Self {
            restore_count: AtomicU32::new(0),
            window_start: Mutex::new(Instant::now()),
            disabled: AtomicU32::new(0),
        }
    }

    async fn should_restore(&self) -> bool {
        if self.disabled.load(Ordering::Relaxed) != 0 {
            return false;
        }

        let mut start = self.window_start.lock().await;
        let now = Instant::now();

        if now.duration_since(*start) > Duration::from_secs(60) {
            // Reset window
            *start = now;
            self.restore_count.store(1, Ordering::Relaxed);
            return true;
        }

        let count = self.restore_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count > MAX_RESTORES_PER_MINUTE {
            self.disabled.store(1, Ordering::Relaxed);
            log::warn!("Hook recovery disabled: too many restorations (>{}/min)", MAX_RESTORES_PER_MINUTE);
            return false;
        }

        true
    }
}

/// Start the hook recovery watcher as a background task.
/// Watches `~/.claude/settings.json` for modifications and re-installs hooks if they were removed.
pub fn start_hook_recovery(
    adapters: Arc<Vec<Arc<dyn AgentAdapter>>>,
    app_handle: tauri::AppHandle,
) {
    let settings_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".claude")
        .join("settings.json");

    if !settings_path.exists() {
        log::debug!("Hook recovery: settings.json not found, skipping watcher");
        return;
    }

    let watch_dir = settings_path.parent().unwrap().to_path_buf();
    let recovery = Arc::new(HookRecovery::new());

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime for hook recovery");

        rt.block_on(async move {
            let adapters_inner = adapters.clone();
            let recovery_inner = recovery.clone();
            let app_handle_inner = app_handle.clone();
            let settings_path_inner = settings_path.clone();

            let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(16);

            let _watcher = {
                let tx = tx.clone();
                let settings_path = settings_path_inner.clone();
                let mut watcher = RecommendedWatcher::new(
                    move |res: Result<Event, notify::Error>| {
                        if let Ok(event) = res {
                            let is_settings_change = matches!(
                                event.kind,
                                EventKind::Modify(_) | EventKind::Create(_)
                            ) && event.paths.iter().any(|p| p == &settings_path);

                            if is_settings_change {
                                let _ = tx.blocking_send(());
                            }
                        }
                    },
                    notify::Config::default().with_poll_interval(Duration::from_secs(2)),
                )
                .expect("Failed to create file watcher for hook recovery");

                watcher
                    .watch(&watch_dir, RecursiveMode::NonRecursive)
                    .expect("Failed to watch .claude directory");

                watcher
            };

            // Debounce: wait 500ms after last event before checking
            loop {
                if rx.recv().await.is_none() {
                    break;
                }

                // Drain any additional events within debounce window
                tokio::time::sleep(Duration::from_millis(500)).await;
                while rx.try_recv().is_ok() {}

                // Verify hooks are intact
                let needs_restore = adapters_inner.iter().any(|a| {
                    if a.name() == "claude-code" {
                        let cc = crate::agents::claude_code::ClaudeCodeAdapter::new();
                        let result = cc.verify_hooks();
                        matches!(
                            result,
                            crate::agents::claude_code::HookVerificationResult::SettingsCorrupted
                        )
                    } else {
                        false
                    }
                });

                if !needs_restore {
                    continue;
                }

                if !recovery_inner.should_restore().await {
                    // Rate limited — emit failure event
                    use tauri::Emitter;
                    let _ = app_handle_inner.emit("hook-recovery-failed", ());
                    log::error!("Hook recovery rate-limited. Manual intervention needed.");
                    break;
                }

                log::info!("Hook recovery: settings.json modified, re-installing hooks...");

                for adapter in adapters_inner.iter() {
                    if let Err(e) = adapter.install_hooks() {
                        log::warn!("Hook recovery failed for {}: {}", adapter.name(), e);
                    } else {
                        log::info!("Hook recovery: restored hooks for {}", adapter.name());
                    }
                }

                // Emit recovery event to frontend
                use tauri::Emitter;
                let _ = app_handle_inner.emit("hook-recovery", "restored");
            }
        });
    });
}
