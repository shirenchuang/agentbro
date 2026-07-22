use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use super::session_store::{SessionPhase, SessionStore};
use crate::terminal::process_tree::ProcessInfo;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const CORRELATED_STORAGE_WINDOW: Duration = Duration::from_secs(6);
const ACTIVE_GRACE: Duration = Duration::from_secs(20);
const IDLE_SESSION_TTL: Duration = Duration::from_secs(2 * 60);
const FINISHED_GRACE: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HelperProcess {
    pid: u32,
    main_pid: u32,
}

#[derive(Debug)]
struct WatchedSession {
    session_id: String,
    last_activity_at: Instant,
    finished_at: Option<Instant>,
}

#[derive(Default)]
struct WatchState {
    initialized: bool,
    known_helper_pids: HashSet<u32>,
    sessions: HashMap<u32, WatchedSession>,
    last_chat_mtime: Option<SystemTime>,
    last_agent_mtime: Option<SystemTime>,
    last_chat_signal_at: Option<Instant>,
    last_agent_signal_at: Option<Instant>,
}

pub fn start(session_store: Arc<SessionStore>) {
    #[cfg(target_os = "macos")]
    tauri::async_runtime::spawn(async move {
        run_loop(session_store).await;
    });

    #[cfg(not(target_os = "macos"))]
    let _ = session_store;
}

#[cfg(target_os = "macos")]
async fn run_loop(session_store: Arc<SessionStore>) {
    let mut state = WatchState::default();
    loop {
        poll(&mut state, &session_store);
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[cfg(target_os = "macos")]
fn poll(state: &mut WatchState, session_store: &SessionStore) {
    let now = Instant::now();
    let tree = crate::terminal::process_tree::build_tree();
    let main_pids = doubao_main_pids(&tree);
    let helpers = doubao_helpers(&tree);
    let helper_pids = helpers
        .iter()
        .map(|helper| helper.pid)
        .collect::<HashSet<_>>();
    let chat_mtime = doubao_state_dir("chrome_doubao-chat_0.indexeddb.leveldb")
        .as_deref()
        .and_then(latest_data_mtime);
    let agent_mtime = doubao_state_dir("chrome_doubao-general-agent-cot_0.indexeddb.leveldb")
        .as_deref()
        .and_then(latest_data_mtime);
    let chat_changed = is_newer(chat_mtime, state.last_chat_mtime);
    let agent_changed = is_newer(agent_mtime, state.last_agent_mtime);
    if chat_changed {
        state.last_chat_signal_at = Some(now);
    }
    if agent_changed {
        state.last_agent_signal_at = Some(now);
    }
    let storage_changed = (chat_changed || agent_changed)
        && signals_are_correlated(state.last_chat_signal_at, state.last_agent_signal_at, now);
    let storage_is_recent = [chat_mtime, agent_mtime].into_iter().all(|mtime| {
        mtime.is_some_and(|mtime| {
            SystemTime::now()
                .duration_since(mtime)
                .is_ok_and(|age| age <= ACTIVE_GRACE)
        })
    });

    for main_pid in &main_pids {
        let newly_started = state.initialized
            && helpers.iter().any(|helper| {
                helper.main_pid == *main_pid && !state.known_helper_pids.contains(&helper.pid)
            });
        let activity_detected =
            newly_started || storage_changed || (!state.initialized && storage_is_recent);
        if activity_detected {
            let watched = state
                .sessions
                .entry(*main_pid)
                .or_insert_with(|| create_session(session_store, *main_pid, now));
            watched.last_activity_at = now;
            watched.finished_at = None;
            set_session_phase(
                session_store,
                &watched.session_id,
                SessionPhase::Processing,
                "Doubao task activity detected",
            );
            continue;
        }

        let Some(watched) = state.sessions.get(main_pid) else {
            continue;
        };
        let session_id = watched.session_id.clone();
        let idle_for = now.duration_since(watched.last_activity_at);
        if idle_for >= IDLE_SESSION_TTL {
            session_store.remove_session(&session_id);
            state.sessions.remove(main_pid);
        } else if idle_for >= ACTIVE_GRACE {
            set_session_phase(
                session_store,
                &session_id,
                SessionPhase::Idle,
                "Doubao task is open; waiting for observable activity",
            );
        }
    }

    let exited_apps = state
        .sessions
        .keys()
        .filter(|pid| !main_pids.contains(pid))
        .copied()
        .collect::<Vec<_>>();
    for main_pid in exited_apps {
        let Some((session_id, finished_at)) = state.sessions.get_mut(&main_pid).map(|watched| {
            (
                watched.session_id.clone(),
                *watched.finished_at.get_or_insert(now),
            )
        }) else {
            continue;
        };
        if now.duration_since(finished_at) >= FINISHED_GRACE {
            session_store.remove_session(&session_id);
            state.sessions.remove(&main_pid);
        } else {
            set_session_phase(
                session_store,
                &session_id,
                SessionPhase::Done,
                "Doubao App exited",
            );
        }
    }

    state.known_helper_pids = helper_pids;
    state.last_chat_mtime = chat_mtime;
    state.last_agent_mtime = agent_mtime;
    state.initialized = true;
}

fn create_session(session_store: &SessionStore, main_pid: u32, now: Instant) -> WatchedSession {
    let session_id = format!("doubao-app-{main_pid}");
    session_store.get_or_create_session(&session_id, "doubao", "Doubao", "", "Doubao");
    session_store.update_session(&session_id, |session| {
        session.engine_label = Some("Doubao App".to_string());
        session.session_title = Some("Task mode activity".to_string());
        session.term_bundle_id = Some("com.bot.pc.doubao".to_string());
        session.pid = Some(main_pid);
        session.phase = SessionPhase::Processing;
        session.description = Some("Doubao task activity detected".to_string());
    });
    WatchedSession {
        session_id,
        last_activity_at: now,
        finished_at: None,
    }
}

fn set_session_phase(
    session_store: &SessionStore,
    session_id: &str,
    phase: SessionPhase,
    description: &str,
) {
    let needs_update = session_store
        .get_session(session_id)
        .is_some_and(|session| {
            session.phase != phase || session.description.as_deref() != Some(description)
        });
    if !needs_update {
        return;
    }
    session_store.update_session(session_id, |session| {
        session.phase = phase;
        session.description = Some(description.to_string());
        if session.phase == SessionPhase::Done {
            session.last_response = Some("Doubao App exited".to_string());
        }
    });
}

fn doubao_main_pids(tree: &HashMap<u32, ProcessInfo>) -> HashSet<u32> {
    tree.values()
        .filter(|process| is_doubao_main(&process.command))
        .map(|process| process.pid)
        .collect()
}

fn doubao_helpers(tree: &HashMap<u32, ProcessInfo>) -> Vec<HelperProcess> {
    let main_pids = doubao_main_pids(tree);

    tree.values()
        .filter(|process| is_doubao_command_helper(&process.command))
        .filter(|process| main_pids.contains(&process.ppid))
        .map(|process| HelperProcess {
            pid: process.pid,
            main_pid: process.ppid,
        })
        .collect()
}

fn is_doubao_main(command: &str) -> bool {
    command.ends_with("/Doubao.app/Contents/MacOS/Doubao")
}

fn is_doubao_command_helper(command: &str) -> bool {
    command.ends_with("/Doubao.app/Contents/Helpers/command_helper")
}

fn doubao_state_dir(origin: &str) -> Option<PathBuf> {
    Some(
        dirs::home_dir()?
            .join("Library/Application Support/Doubao/Default/IndexedDB")
            .join(origin),
    )
}

fn latest_data_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::read_dir(path)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            matches!(
                entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str()),
                Some("log" | "ldb")
            )
        })
        .filter_map(|entry| entry.metadata().ok()?.modified().ok())
        .max()
}

fn is_newer(current: Option<SystemTime>, previous: Option<SystemTime>) -> bool {
    matches!((current, previous), (Some(current), Some(previous)) if current > previous)
}

fn signals_are_correlated(chat: Option<Instant>, agent: Option<Instant>, now: Instant) -> bool {
    [chat, agent].into_iter().all(|signal| {
        signal.is_some_and(|signal| now.duration_since(signal) <= CORRELATED_STORAGE_WINDOW)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, ppid: u32, command: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid,
            command: command.to_string(),
            tty: None,
        }
    }

    #[test]
    fn detects_command_helper_owned_by_doubao_main_process() {
        let tree = HashMap::from([
            (
                10,
                process(10, 1, "/Applications/Doubao.app/Contents/MacOS/Doubao"),
            ),
            (
                11,
                process(
                    11,
                    10,
                    "/Applications/Doubao.app/Contents/Helpers/command_helper",
                ),
            ),
        ]);

        assert_eq!(
            doubao_helpers(&tree),
            vec![HelperProcess {
                pid: 11,
                main_pid: 10
            }]
        );
    }

    #[test]
    fn ignores_unrelated_command_helpers() {
        let tree = HashMap::from([
            (
                10,
                process(10, 1, "/Applications/Other.app/Contents/MacOS/Other"),
            ),
            (
                11,
                process(
                    11,
                    10,
                    "/Applications/Doubao.app/Contents/Helpers/command_helper",
                ),
            ),
        ]);

        assert!(doubao_helpers(&tree).is_empty());
    }

    #[test]
    fn storage_activity_requires_a_strictly_newer_timestamp() {
        let earlier = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let later = SystemTime::UNIX_EPOCH + Duration::from_secs(11);

        assert!(is_newer(Some(later), Some(earlier)));
        assert!(!is_newer(Some(earlier), Some(earlier)));
        assert!(!is_newer(Some(later), None));
    }

    #[test]
    fn storage_activity_requires_recent_signals_from_both_origins() {
        let now = Instant::now();

        assert!(signals_are_correlated(Some(now), Some(now), now));
        assert!(!signals_are_correlated(Some(now), None, now));
        assert!(!signals_are_correlated(
            Some(now - CORRELATED_STORAGE_WINDOW - Duration::from_secs(1)),
            Some(now),
            now,
        ));
    }
}
