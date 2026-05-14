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
            log::warn!(
                "Hook recovery disabled: too many restorations (>{}/min)",
                MAX_RESTORES_PER_MINUTE
            );
            return false;
        }

        true
    }
}

/// Start the hook recovery watcher as a background task.
/// Watches all adapter settings paths for modifications and re-installs hooks if they were removed.
pub fn start_hook_recovery(
    adapters: Arc<Vec<Arc<dyn AgentAdapter>>>,
    app_handle: tauri::AppHandle,
) {
    // Collect all settings paths from all adapters. Prefer watching the file
    // itself: watching the config root directory for AntCC/CodeFuse opens a
    // descriptor for every task/plugin/cache entry on macOS kqueue.
    let mut watch_paths: Vec<PathBuf> = Vec::new();
    let mut watch_targets: Vec<PathBuf> = Vec::new();
    for adapter in adapters.iter() {
        for path in adapter.hook_config_paths() {
            if !watch_paths.contains(&path) {
                watch_paths.push(path.clone());
            }

            let target = if path.exists() {
                path.clone()
            } else if let Some(parent) = path.parent() {
                parent.to_path_buf()
            } else {
                continue;
            };

            if !watch_targets.contains(&target) {
                watch_targets.push(target);
            }
        }
    }

    if watch_paths.is_empty() {
        log::debug!("Hook recovery: no settings paths to watch, skipping");
        return;
    }

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
            let watch_paths_inner = watch_paths.clone();

            let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(16);

            let _watcher = {
                let tx = tx.clone();
                let watched = watch_paths_inner.clone();
                let mut watcher = RecommendedWatcher::new(
                    move |res: Result<Event, notify::Error>| {
                        if let Ok(event) = res {
                            let is_settings_change =
                                matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
                                    && event.paths.iter().any(|p| watched.contains(p));

                            if is_settings_change {
                                let _ = tx.blocking_send(());
                            }
                        }
                    },
                    notify::Config::default().with_poll_interval(Duration::from_secs(2)),
                )
                .expect("Failed to create file watcher for hook recovery");

                for target in &watch_targets {
                    if target.exists() {
                        if let Err(e) = watcher.watch(target, RecursiveMode::NonRecursive) {
                            log::warn!(
                                "Hook recovery: failed to watch {}: {}",
                                target.display(),
                                e
                            );
                        }
                    }
                }

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

                // Check if any adapter needs hook restoration
                let needs_restore = adapters_inner.iter().any(|a| {
                    let paths = a.hook_config_paths();
                    paths.iter().any(|p| {
                        if !p.exists() {
                            return true;
                        }
                        let content = match std::fs::read_to_string(p) {
                            Ok(c) => c,
                            Err(_) => return true,
                        };
                        !content.contains("agentbro-bridge")
                    })
                });

                if !needs_restore {
                    continue;
                }

                if !recovery_inner.should_restore().await {
                    use tauri::Emitter;
                    let _ = app_handle_inner.emit("hook-recovery-failed", ());
                    log::error!("Hook recovery rate-limited. Manual intervention needed.");
                    break;
                }

                log::info!("Hook recovery: settings modified, re-installing hooks...");

                for adapter in adapters_inner.iter() {
                    if let Err(e) = adapter.install_hooks() {
                        log::warn!("Hook recovery failed for {}: {}", adapter.display_name(), e);
                    } else {
                        log::info!(
                            "Hook recovery: restored hooks for {}",
                            adapter.display_name()
                        );
                    }
                }

                use tauri::Emitter;
                let _ = app_handle_inner.emit("hook-recovery", "restored");
            }
        });
    });
}
