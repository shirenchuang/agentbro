use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[repr(u8)]
pub enum DisplayLevel {
    Dormant = 0,
    Compact = 1,
    Visible = 2,
}

impl From<u8> for DisplayLevel {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Dormant,
            1 => Self::Compact,
            2 => Self::Visible,
            _ => Self::Compact,
        }
    }
}

pub struct DisplayController {
    level: AtomicU8,
    silence_until: Mutex<Option<Instant>>,
    peek_until: Mutex<Option<Instant>>,
    peek_cooldown: Mutex<Option<Instant>>,
    app_handle: Arc<std::sync::Mutex<Option<AppHandle>>>,
}

const SILENCE_DURATION: Duration = Duration::from_secs(30);
const PEEK_DURATION: Duration = Duration::from_millis(3000);
const PEEK_COOLDOWN: Duration = Duration::from_secs(30);

impl DisplayController {
    pub fn new() -> Self {
        Self {
            level: AtomicU8::new(DisplayLevel::Compact as u8),
            silence_until: Mutex::new(None),
            peek_until: Mutex::new(None),
            peek_cooldown: Mutex::new(None),
            app_handle: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn set_app_handle(&self, handle: AppHandle) {
        if let Ok(mut h) = self.app_handle.lock() {
            *h = Some(handle);
        }
    }

    pub fn current_level(&self) -> DisplayLevel {
        DisplayLevel::from(self.level.load(Ordering::Relaxed))
    }

    fn set_level(&self, new_level: DisplayLevel) {
        let old = self.level.swap(new_level as u8, Ordering::Relaxed);
        if old != new_level as u8 {
            self.emit_level_change(new_level);
        }
    }

    fn emit_level_change(&self, level: DisplayLevel) {
        if let Ok(guard) = self.app_handle.lock() {
            if let Some(ref handle) = *guard {
                let _ = handle.emit("display-level-changed", level);
            }
        }
    }

    /// Called when cursor enters the notch hotzone
    pub async fn on_cursor_enter(&self) {
        // Check silence period
        let silenced = {
            let guard = self.silence_until.lock().await;
            guard.map(|t| Instant::now() < t).unwrap_or(false)
        };
        if silenced {
            return;
        }

        let current = self.current_level();
        if current == DisplayLevel::Dormant {
            self.set_level(DisplayLevel::Compact);
        }
    }

    /// Called when cursor leaves the notch area
    pub async fn on_cursor_leave(&self) {
        let current = self.current_level();
        if current == DisplayLevel::Visible {
            self.set_level(DisplayLevel::Compact);
        }
    }

    /// Called when user hovers or clicks to expand
    pub fn on_expand(&self) {
        self.set_level(DisplayLevel::Visible);
    }

    /// Progressive ESC handling
    pub async fn on_esc(&self) {
        let current = self.current_level();
        match current {
            DisplayLevel::Visible => {
                self.set_level(DisplayLevel::Compact);
            }
            DisplayLevel::Compact => {
                self.set_level(DisplayLevel::Dormant);
                // Set silence period
                let mut silence = self.silence_until.lock().await;
                *silence = Some(Instant::now() + SILENCE_DURATION);
            }
            DisplayLevel::Dormant => {}
        }
    }

    /// Called when a new session starts or an alert arrives
    pub async fn on_alert(&self, is_blocking: bool) {
        let current = self.current_level();

        if is_blocking {
            // Blocking events (permission/question/plan) force visible
            self.set_level(DisplayLevel::Visible);
        } else if current == DisplayLevel::Dormant {
            // Non-blocking event during dormant: peek
            self.peek().await;
        }
    }

    /// Called when a new session is created
    pub async fn on_new_session(&self) {
        let current = self.current_level();
        if current == DisplayLevel::Dormant {
            self.peek().await;
        }
    }

    /// Called when all sessions are gone
    pub async fn on_no_sessions(&self) {
        // Brief delay then go dormant
        tokio::time::sleep(Duration::from_secs(1)).await;
        if self.current_level() == DisplayLevel::Compact {
            self.set_level(DisplayLevel::Dormant);
        }
    }

    /// Briefly show compact pill then return to dormant
    async fn peek(&self) {
        // Check cooldown
        {
            let guard = self.peek_cooldown.lock().await;
            if let Some(t) = *guard {
                if Instant::now() < t {
                    return;
                }
            }
        }

        self.set_level(DisplayLevel::Compact);

        // Set peek timer
        {
            let mut peek = self.peek_until.lock().await;
            *peek = Some(Instant::now() + PEEK_DURATION);
        }
        {
            let mut cooldown = self.peek_cooldown.lock().await;
            *cooldown = Some(Instant::now() + PEEK_COOLDOWN);
        }

        // Auto-return to dormant after peek duration
        tokio::time::sleep(PEEK_DURATION).await;

        // Only return to dormant if still in compact (user may have interacted)
        if self.current_level() == DisplayLevel::Compact {
            let still_peeking = {
                let guard = self.peek_until.lock().await;
                guard.map(|t| Instant::now() >= t).unwrap_or(true)
            };
            if still_peeking {
                self.set_level(DisplayLevel::Dormant);
            }
        }
    }
}
