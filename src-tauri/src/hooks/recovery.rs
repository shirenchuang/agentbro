use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{Event, EventKind, PollWatcher, RecursiveMode, Watcher};
use tokio::sync::Mutex;

use crate::agents::profiles::{self, InstallationKind};
use crate::agents::AgentAdapter;
use crate::config::{hook_intent_enabled, ConfigStore};

const MAX_RESTORES_PER_MINUTE: u32 = 3;
const SELF_WRITE_SUPPRESSION_WINDOW: Duration = Duration::from_secs(6);

/// Decide whether an adapter's hooks should be re-installed in response to a
/// settings-file change.
///
/// Two cases trigger a restore:
/// 1. `NeedsReinstall` — our hook is present but stale (e.g. an outdated bridge
///    path or socket endpoint).
/// 2. `NotInstalled` *and* the user previously enabled this agent (persisted in
///    `enabled_agents`). This covers an external tool — notably cc-switch
///    swapping providers — overwriting the settings file with a snapshot that
///    contains no AgentBro hook at all. Without the persisted intent we cannot
///    tell a user-requested removal apart from an external wipe, so we only
///    self-heal agents the user explicitly opted into. The CLI-availability
///    check happens later (we skip restores for tools that aren't installed).
fn adapter_needs_restore(adapter: &dyn AgentAdapter, enabled_agents: &[String]) -> bool {
    let Some(profile) = crate::agents::profiles::profile_for_agent(adapter.name()) else {
        return adapter.hook_config_paths().iter().any(|p| {
            std::fs::read_to_string(p)
                .map(|content| {
                    content.contains("agentbro-bridge")
                        && !content.contains(crate::hook_endpoint::HOOK_PORT_ENV)
                })
                .unwrap_or(false)
        });
    };
    if profiles::uses_desktop_plugin(&profile) {
        return false;
    }

    let is_enabled = hook_intent_enabled(enabled_agents, adapter.name());

    adapter.hook_config_paths().iter().any(|p| {
        match crate::agents::profiles::install_health(&profile, p) {
            crate::agents::profiles::HookInstallHealth::NeedsReinstall => true,
            crate::agents::profiles::HookInstallHealth::NotInstalled => is_enabled,
            _ => false,
        }
    })
}

fn watch_target_for_path(adapter: &dyn AgentAdapter, path: &Path) -> Option<PathBuf> {
    let is_removable_hook_artifact =
        profiles::profile_for_agent(adapter.name()).is_some_and(|profile| {
            matches!(
                profile.installation_kind,
                InstallationKind::PluginFile | InstallationKind::PluginDirectory
            )
        });

    if is_removable_hook_artifact {
        return path
            .parent()
            .filter(|parent| parent.exists())
            .map(Path::to_path_buf);
    }

    if path.exists() {
        Some(path.to_path_buf())
    } else {
        path.parent()
            .filter(|parent| parent.exists())
            .map(Path::to_path_buf)
    }
}

fn event_touches_watched_path(event_paths: &[PathBuf], watched_paths: &[PathBuf]) -> bool {
    event_paths.iter().any(|event_path| {
        watched_paths
            .iter()
            .any(|watched_path| event_path == watched_path || event_path.starts_with(watched_path))
    })
}

pub struct HookRecovery {
    restore_count: AtomicU32,
    window_start: Mutex<Instant>,
    disabled: AtomicU32,
    suppress_until: Mutex<Option<Instant>>,
}

impl HookRecovery {
    pub fn new() -> Self {
        Self {
            restore_count: AtomicU32::new(0),
            window_start: Mutex::new(Instant::now()),
            disabled: AtomicU32::new(0),
            suppress_until: Mutex::new(None),
        }
    }

    async fn suppress_self_writes(&self) {
        let mut suppress_until = self.suppress_until.lock().await;
        *suppress_until = Some(Instant::now() + SELF_WRITE_SUPPRESSION_WINDOW);
    }

    async fn is_self_write_suppressed(&self) -> bool {
        let mut suppress_until = self.suppress_until.lock().await;
        match *suppress_until {
            Some(until) if Instant::now() < until => true,
            Some(_) => {
                *suppress_until = None;
                false
            }
            None => false,
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

impl Default for HookRecovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Start the hook recovery watcher as a background task.
/// Watches all adapter settings paths for modifications and re-installs hooks if they were removed.
pub fn start_hook_recovery(
    adapters: Arc<Vec<Arc<dyn AgentAdapter>>>,
    app_handle: tauri::AppHandle,
    config_store: ConfigStore,
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

            if let Some(target) = watch_target_for_path(adapter.as_ref(), &path) {
                if watch_targets.contains(&target) {
                    continue;
                }
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
                let mut watcher = PollWatcher::new(
                    move |res: Result<Event, notify::Error>| {
                        if let Ok(event) = res {
                            let is_settings_change =
                                matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
                                    && event_touches_watched_path(&event.paths, &watched);

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

                if recovery_inner.is_self_write_suppressed().await {
                    log::debug!(
                        "Hook recovery: ignoring settings change from recent managed restore"
                    );
                    continue;
                }

                // Check which existing AgentBro hooks need a managed refresh.
                // Missing configs are normal for tools the user has not installed;
                // auto-recovery should not turn those into repeated bulk installs.
                // The persisted `enabled_agents` intent lets us also restore a
                // hook that was wiped entirely (e.g. by cc-switch) for an agent
                // the user explicitly opted into.
                let enabled_agents = config_store.get().enabled_agents;
                let adapters_to_restore: Vec<Arc<dyn AgentAdapter>> = adapters_inner
                    .iter()
                    .filter(|adapter| adapter_needs_restore(adapter.as_ref(), &enabled_agents))
                    .cloned()
                    .collect();

                if adapters_to_restore.is_empty() {
                    continue;
                }

                if !recovery_inner.should_restore().await {
                    use tauri::Emitter;
                    let _ = app_handle_inner.emit("hook-recovery-failed", ());
                    log::error!("Hook recovery rate-limited. Manual intervention needed.");
                    break;
                }

                let adapter_names = adapters_to_restore
                    .iter()
                    .map(|adapter| adapter.display_name())
                    .collect::<Vec<_>>()
                    .join(", ");
                log::info!(
                    "Hook recovery: settings modified, re-installing hooks for {}...",
                    adapter_names
                );

                for adapter in adapters_to_restore.iter() {
                    if matches!(
                        adapter.detect_status_now(),
                        crate::agents::AdapterStatus::Unavailable
                    ) {
                        log::debug!(
                            "Hook recovery: skipping {} (CLI not installed)",
                            adapter.display_name()
                        );
                        continue;
                    }
                    if !adapter_needs_restore(adapter.as_ref(), &config_store.get().enabled_agents)
                    {
                        continue;
                    }
                    if let Err(e) = adapter.install_hooks() {
                        log::warn!("Hook recovery failed for {}: {}", adapter.display_name(), e);
                    } else {
                        log::info!(
                            "Hook recovery: restored hooks for {}",
                            adapter.display_name()
                        );
                    }
                }

                recovery_inner.suppress_self_writes().await;

                use tauri::Emitter;
                let _ = app_handle_inner.emit("hook-recovery", "restored");
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn suppresses_recent_managed_writes() {
        let recovery = HookRecovery::new();
        assert!(!recovery.is_self_write_suppressed().await);

        recovery.suppress_self_writes().await;

        assert!(recovery.is_self_write_suppressed().await);
    }

    #[test]
    fn event_matches_exact_or_nested_watched_path() {
        let watched = vec![PathBuf::from("/tmp/example/plugin")];

        assert!(event_touches_watched_path(
            &[PathBuf::from("/tmp/example/plugin")],
            &watched
        ));
        assert!(event_touches_watched_path(
            &[PathBuf::from("/tmp/example/plugin/plugin.yaml")],
            &watched
        ));
        assert!(!event_touches_watched_path(
            &[PathBuf::from("/tmp/example-other/plugin")],
            &watched
        ));
    }

    /// Minimal adapter whose `claude-code` profile resolves through the real
    /// `profiles` registry but whose settings file we control on disk.
    struct StubAdapter {
        id: &'static str,
        path: PathBuf,
    }

    impl AgentAdapter for StubAdapter {
        fn name(&self) -> &str {
            self.id
        }
        fn display_name(&self) -> &str {
            "Claude Code"
        }
        fn icon(&self) -> &str {
            ""
        }
        fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
        fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
        fn status(&self) -> crate::agents::AdapterStatus {
            crate::agents::AdapterStatus::Available
        }
        fn parse_event(
            &self,
            _raw: &serde_json::Value,
        ) -> Result<crate::agents::AgentEvent, Box<dyn std::error::Error>> {
            Err("not used in test".into())
        }
        fn hook_config_paths(&self) -> Vec<PathBuf> {
            vec![self.path.clone()]
        }
    }

    fn unique_temp_path(tag: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("agentbro-recovery-test-{tag}-{pid}-{nanos}.json"))
    }

    // A settings file with no AgentBro hook at all simulates a cc-switch wipe:
    // it must only trigger a restore when the agent is in the enabled intent.
    #[test]
    fn wiped_hook_restores_only_when_agent_is_enabled() {
        let path = unique_temp_path("wiped");
        std::fs::write(&path, r#"{"hooks":{}}"#).expect("write settings");
        let adapter = StubAdapter {
            id: "claude-code",
            path: path.clone(),
        };

        assert!(
            !adapter_needs_restore(&adapter, &[]),
            "wiped hook with empty intent must NOT auto-restore"
        );
        assert!(
            adapter_needs_restore(&adapter, &["claude-code".to_string()]),
            "wiped hook for an enabled agent MUST restore"
        );

        let _ = std::fs::remove_file(&path);
    }

    // A missing settings file is `NotInstalled` too; same intent gate applies.
    #[test]
    fn missing_settings_restores_only_when_agent_is_enabled() {
        let path = unique_temp_path("missing");
        let _ = std::fs::remove_file(&path);
        let adapter = StubAdapter {
            id: "claude-code",
            path,
        };

        assert!(!adapter_needs_restore(&adapter, &[]));
        assert!(adapter_needs_restore(
            &adapter,
            &["claude-code".to_string()]
        ));
    }

    #[test]
    fn shared_antigravity_hook_recovers_legacy_intent_from_gemini_desktop() {
        let fixture =
            crate::skills::installer::TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let path = fixture.path().join("hooks.json");
        std::fs::write(&path, "{}").unwrap();
        let adapter = StubAdapter {
            id: "antigravity",
            path,
        };
        assert!(adapter_needs_restore(&adapter, &["gemini-desktop".into()]));
        assert!(!adapter_needs_restore(&adapter, &["gemini".into()]));
        #[cfg(target_os = "macos")]
        {
            let desktop = StubAdapter {
                id: "gemini-desktop",
                path: adapter.path.clone(),
            };
            assert!(adapter_needs_restore(&desktop, &["antigravity".into()]));
            assert!(!adapter_needs_restore(&desktop, &["gemini".into()]));
        }
    }
}
