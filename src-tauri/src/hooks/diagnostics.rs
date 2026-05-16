// Diagnostics — Ring buffer of the last 100 diagnostic events for observability

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const RING_BUFFER_CAPACITY: usize = 100;

/// Severity level of a diagnostic event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Debug,
    Info,
    Warning,
    Error,
}

/// A single diagnostic event stored in the ring buffer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    /// Monotonic sequence number (never reused within a session)
    pub seq: u64,
    /// Unix timestamp (milliseconds)
    pub timestamp_ms: u64,
    pub severity: DiagnosticSeverity,
    /// Component that emitted the event (e.g. "terminal", "webhook", "remote")
    pub component: String,
    pub message: String,
    /// Optional structured payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

/// Fixed-capacity ring buffer for diagnostic events.
/// Oldest entries are evicted when capacity is reached.
/// Thread-safe via internal mutex.
#[derive(Clone)]
pub struct DiagnosticRingBuffer {
    inner: Arc<Mutex<RingBufferInner>>,
}

struct RingBufferInner {
    events: VecDeque<DiagnosticEvent>,
    next_seq: u64,
    capacity: usize,
}

impl DiagnosticRingBuffer {
    pub fn new() -> Self {
        Self::with_capacity(RING_BUFFER_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RingBufferInner {
                events: VecDeque::with_capacity(capacity),
                next_seq: 0,
                capacity,
            })),
        }
    }

    /// Push a new event; evicts the oldest if at capacity.
    pub fn push(
        &self,
        severity: DiagnosticSeverity,
        component: impl Into<String>,
        message: impl Into<String>,
        payload: Option<serde_json::Value>,
    ) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        let seq = inner.next_seq;
        inner.next_seq += 1;

        let event = DiagnosticEvent {
            seq,
            timestamp_ms: now_ms(),
            severity,
            component: component.into(),
            message: message.into(),
            payload,
        };

        if inner.events.len() == inner.capacity {
            inner.events.pop_front();
        }
        inner.events.push_back(event);
        seq
    }

    /// Return all events with seq > `after_seq`, in insertion order.
    pub fn since(&self, after_seq: u64) -> Vec<DiagnosticEvent> {
        let inner = self.inner.lock().unwrap();
        inner
            .events
            .iter()
            .filter(|e| e.seq > after_seq)
            .cloned()
            .collect()
    }

    /// Return all events at or above the given severity, optionally filtered by component.
    pub fn query(
        &self,
        min_severity: DiagnosticSeverity,
        component: Option<&str>,
    ) -> Vec<DiagnosticEvent> {
        let inner = self.inner.lock().unwrap();
        inner
            .events
            .iter()
            .filter(|e| e.severity >= min_severity && component.is_none_or(|c| e.component == c))
            .cloned()
            .collect()
    }

    /// Snapshot of all events (oldest first)
    pub fn all(&self) -> Vec<DiagnosticEvent> {
        let inner = self.inner.lock().unwrap();
        inner.events.iter().cloned().collect()
    }

    /// Number of events currently stored
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Highest sequence number stored, or None if empty
    pub fn last_seq(&self) -> Option<u64> {
        self.inner.lock().unwrap().events.back().map(|e| e.seq)
    }

    /// Clear all events (useful for testing)
    pub fn clear(&self) {
        self.inner.lock().unwrap().events.clear();
    }
}

impl Default for DiagnosticRingBuffer {
    fn default() -> Self {
        Self::new()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ─── Convenience macros ───────────────────────────────────────────────────────

/// Log a debug-level diagnostic event
#[macro_export]
macro_rules! diag_debug {
    ($buf:expr, $component:expr, $msg:expr) => {
        $buf.push(
            $crate::hooks::diagnostics::DiagnosticSeverity::Debug,
            $component,
            $msg,
            None,
        )
    };
    ($buf:expr, $component:expr, $msg:expr, $payload:expr) => {
        $buf.push(
            $crate::hooks::diagnostics::DiagnosticSeverity::Debug,
            $component,
            $msg,
            Some($payload),
        )
    };
}

/// Log an info-level diagnostic event
#[macro_export]
macro_rules! diag_info {
    ($buf:expr, $component:expr, $msg:expr) => {
        $buf.push(
            $crate::hooks::diagnostics::DiagnosticSeverity::Info,
            $component,
            $msg,
            None,
        )
    };
}

/// Log an error-level diagnostic event
#[macro_export]
macro_rules! diag_error {
    ($buf:expr, $component:expr, $msg:expr) => {
        $buf.push(
            $crate::hooks::diagnostics::DiagnosticSeverity::Error,
            $component,
            $msg,
            None,
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_capacity() {
        let buf = DiagnosticRingBuffer::with_capacity(3);
        for i in 0..5u64 {
            buf.push(DiagnosticSeverity::Info, "test", format!("msg {}", i), None);
        }
        assert_eq!(buf.len(), 3);
        let all = buf.all();
        // Oldest 2 should have been evicted; remaining are seq 2, 3, 4
        assert_eq!(all[0].seq, 2);
        assert_eq!(all[2].seq, 4);
    }

    #[test]
    fn test_since() {
        let buf = DiagnosticRingBuffer::with_capacity(10);
        for i in 0..5u64 {
            buf.push(DiagnosticSeverity::Info, "test", format!("msg {}", i), None);
        }
        let events = buf.since(2);
        assert_eq!(events.len(), 2); // seq 3 and 4
        assert_eq!(events[0].seq, 3);
    }

    #[test]
    fn test_severity_filter() {
        let buf = DiagnosticRingBuffer::with_capacity(10);
        buf.push(DiagnosticSeverity::Debug, "comp", "debug msg", None);
        buf.push(DiagnosticSeverity::Warning, "comp", "warn msg", None);
        buf.push(DiagnosticSeverity::Error, "comp", "error msg", None);

        let warnings_and_above = buf.query(DiagnosticSeverity::Warning, None);
        assert_eq!(warnings_and_above.len(), 2);
    }

    #[test]
    fn test_component_filter() {
        let buf = DiagnosticRingBuffer::with_capacity(10);
        buf.push(DiagnosticSeverity::Info, "terminal", "t msg", None);
        buf.push(DiagnosticSeverity::Info, "webhook", "w msg", None);
        buf.push(DiagnosticSeverity::Info, "terminal", "t msg 2", None);

        let terminal_events = buf.query(DiagnosticSeverity::Debug, Some("terminal"));
        assert_eq!(terminal_events.len(), 2);
    }

    #[test]
    fn test_seq_monotonic() {
        let buf = DiagnosticRingBuffer::with_capacity(10);
        let s0 = buf.push(DiagnosticSeverity::Info, "x", "a", None);
        let s1 = buf.push(DiagnosticSeverity::Info, "x", "b", None);
        assert!(s1 > s0);
    }
}
