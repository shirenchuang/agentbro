// Cursor-monitor tracker: a single background poller that watches which
// monitor the cursor currently lives on, and notifies subscribers only when
// the answer changes. The notch window listens via the Tauri event bus.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[cfg(not(target_os = "windows"))]
use crate::platform::display::find_cursor_monitor;

/// Emitted to the frontend on the Tauri event bus when the cursor crosses
/// from one monitor to another.
pub const CURSOR_MONITOR_CHANGED_EVENT: &str = "cursor-monitor-changed";

/// Polling interval. 250 ms keeps the latency below the visual threshold
/// the notch's previous 500 ms cadence already established, while the
/// change-detection short-circuit means we only do real work on transitions.
#[cfg(target_os = "windows")]
const POLL_INTERVAL: Duration = Duration::from_millis(500);

#[cfg(not(target_os = "windows"))]
const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorMonitorChange {
    /// Stable identifier for the monitor — Tauri's `Monitor::name()` value,
    /// or "primary" if unnamed. Subscribers compare this to drive their
    /// own positioning.
    pub monitor_id: String,
}

type Listener = Box<dyn Fn(&AppHandle, &CursorMonitorChange) + Send + Sync + 'static>;

struct TrackerState {
    last_monitor_id: Option<String>,
    listeners: Vec<Listener>,
}

static TRACKER_STATE: OnceLock<Mutex<TrackerState>> = OnceLock::new();
static TRACKER_STARTED: OnceLock<()> = OnceLock::new();

fn state() -> &'static Mutex<TrackerState> {
    TRACKER_STATE.get_or_init(|| {
        Mutex::new(TrackerState {
            last_monitor_id: None,
            listeners: Vec::new(),
        })
    })
}

#[cfg(not(target_os = "windows"))]
fn monitor_identity(monitor: &tauri::Monitor) -> String {
    monitor
        .name()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "primary".to_string())
}

/// Register an in-process callback invoked whenever the cursor moves to a
/// different monitor. Always dispatched on the Tauri main thread.
pub fn subscribe<F>(callback: F)
where
    F: Fn(&AppHandle, &CursorMonitorChange) + Send + Sync + 'static,
{
    if let Ok(mut guard) = state().lock() {
        guard.listeners.push(Box::new(callback));
    }
}

/// Spawn the background poller. Idempotent — safe to call from `setup`.
pub fn start(app: AppHandle) {
    if TRACKER_STARTED.set(()).is_err() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            // `check_for_change` calls `app.available_monitors()` which on macOS
            // returns ObjC NSScreen objects that internally allocate autoreleased
            // NSDictionary instances for `deviceDescription` and friends. Without
            // an explicit pool here those objects live forever (the tokio task
            // never drains the thread's default autoreleasepool), leaking ~25
            // NSConcreteValue / NSDeviceDescription dicts every 250ms — measured
            // at ~3-4 MB/min on a 2-display setup, several GB after a day.
            let change = poll_for_change(&app);
            if let Some(change) = change {
                let _ = app.emit(CURSOR_MONITOR_CHANGED_EVENT, change.clone());
                dispatch_listeners(&app, change);
            }
        }
    });
}

#[cfg(target_os = "macos")]
fn poll_for_change(app: &AppHandle) -> Option<CursorMonitorChange> {
    objc2::rc::autoreleasepool(|_pool| check_for_change(app))
}

#[cfg(not(target_os = "macos"))]
fn poll_for_change(app: &AppHandle) -> Option<CursorMonitorChange> {
    check_for_change(app)
}

fn check_for_change(app: &AppHandle) -> Option<CursorMonitorChange> {
    let id = current_monitor_id(app)?;
    let mut guard = state().lock().ok()?;
    if guard.last_monitor_id.as_deref() == Some(id.as_str()) {
        return None;
    }
    guard.last_monitor_id = Some(id.clone());
    Some(CursorMonitorChange { monitor_id: id })
}

#[cfg(target_os = "windows")]
fn current_monitor_id(_app: &AppHandle) -> Option<String> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return None;
    }
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        None
    } else {
        Some(format!("windows-monitor-{:x}", monitor as usize))
    }
}

#[cfg(not(target_os = "windows"))]
fn current_monitor_id(app: &AppHandle) -> Option<String> {
    let monitor = find_cursor_monitor(app)?;
    Some(monitor_identity(&monitor))
}

fn dispatch_listeners(app: &AppHandle, change: CursorMonitorChange) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Ok(guard) = state().lock() {
            for listener in guard.listeners.iter() {
                listener(&handle, &change);
            }
        }
    });
}
