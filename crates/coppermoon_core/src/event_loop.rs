//! Event loop infrastructure for CopperMoon
//!
//! Provides global timer management used by setTimeout/setInterval.
//! Timer callbacks are stored in a global registry and fired via
//! a channel-based event system. The main Lua thread processes
//! events after script execution or between HTTP request dispatches.

use mlua::RegistryKey;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Timer types
// ---------------------------------------------------------------------------

/// How a timer should behave after firing.
#[derive(Debug)]
pub enum TimerType {
    /// Fire once then remove.
    Timeout,
    /// Fire repeatedly with the given interval.
    Interval { ms: u64 },
}

/// A registered timer callback.
pub struct TimerCallback {
    pub registry_key: RegistryKey,
    pub timer_type: TimerType,
}

/// An event sent from a Tokio timer task to the main Lua thread.
#[derive(Debug)]
pub enum TimerEvent {
    /// The timer with the given ID is ready to fire.
    Ready(u64),
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

static TIMER_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static PENDING_TIMER_COUNT: AtomicUsize = AtomicUsize::new(0);

static TIMER_CALLBACKS: OnceLock<Mutex<HashMap<u64, TimerCallback>>> = OnceLock::new();
static CANCELLED_TIMERS: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();

/// Channel pair for timer events (sender, receiver).
static TIMER_CHANNEL: OnceLock<(
    std::sync::mpsc::Sender<TimerEvent>,
    Mutex<std::sync::mpsc::Receiver<TimerEvent>>,
)> = OnceLock::new();

// ---------------------------------------------------------------------------
// Accessors for lazy-initialised global state
// ---------------------------------------------------------------------------

fn callbacks() -> &'static Mutex<HashMap<u64, TimerCallback>> {
    TIMER_CALLBACKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cancelled() -> &'static Mutex<HashSet<u64>> {
    CANCELLED_TIMERS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn channel() -> &'static (
    std::sync::mpsc::Sender<TimerEvent>,
    Mutex<std::sync::mpsc::Receiver<TimerEvent>>,
) {
    TIMER_CHANNEL.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel();
        (tx, Mutex::new(rx))
    })
}

// ---------------------------------------------------------------------------
// Public API — registration / cancellation
// ---------------------------------------------------------------------------

/// Generate a new unique timer ID.
pub fn next_timer_id() -> u64 {
    TIMER_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Register a timer callback. Increments the pending timer count.
pub fn register_timer(id: u64, callback: TimerCallback) {
    callbacks().lock().unwrap().insert(id, callback);
    PENDING_TIMER_COUNT.fetch_add(1, Ordering::SeqCst);
}

/// Cancel a timer. Decrements the pending timer count.
pub fn cancel_timer(id: u64) {
    cancelled().lock().unwrap().insert(id);
    // Remove the callback if it exists and decrement counter
    if callbacks().lock().unwrap().remove(&id).is_some() {
        PENDING_TIMER_COUNT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Check whether a timer has been cancelled.
pub fn is_timer_cancelled(id: u64) -> bool {
    cancelled().lock().unwrap().contains(&id)
}

/// Returns `true` if there are timers that have not yet fired or been cancelled.
pub fn has_pending_timers() -> bool {
    PENDING_TIMER_COUNT.load(Ordering::SeqCst) > 0
}

// ---------------------------------------------------------------------------
// Public API — event channel
// ---------------------------------------------------------------------------

/// Called by Tokio timer tasks when a timer is ready to fire.
pub fn send_timer_ready(id: u64) {
    let (tx, _) = channel();
    // Ignore send error — the receiver may have been dropped (shutdown).
    let _ = tx.send(TimerEvent::Ready(id));
}

/// Try to receive a timer event, blocking for at most `timeout`.
/// Returns `None` on timeout or if the channel is disconnected.
pub fn try_recv_timer_event(timeout: Duration) -> Option<TimerEvent> {
    let (_, rx) = channel();
    let rx = rx.lock().unwrap();
    rx.recv_timeout(timeout).ok()
}

// ---------------------------------------------------------------------------
// Public API — callback retrieval
// ---------------------------------------------------------------------------

/// Take a timer callback out of the store.
///
/// * For `Timeout` timers the callback is removed and the pending count decremented.
/// * For `Interval` timers the callback is **kept** (it will fire again) — the
///   caller receives a *reference-like* view by temporarily removing it.
///   Call [`restore_timer_callback`] after invoking the callback.
///
/// Returns `None` if the timer was already cancelled / consumed.
pub fn take_timer_callback(id: u64) -> Option<TimerCallback> {
    let mut cbs = callbacks().lock().unwrap();
    let cb = cbs.remove(&id)?;
    match cb.timer_type {
        TimerType::Timeout => {
            PENDING_TIMER_COUNT.fetch_sub(1, Ordering::SeqCst);
            // Clean up cancellation set entry if present
            cancelled().lock().unwrap().remove(&id);
            Some(cb)
        }
        TimerType::Interval { .. } => {
            // Temporarily removed — caller must restore after use.
            Some(cb)
        }
    }
}

/// Put an interval callback back after it was invoked.
pub fn restore_timer_callback(id: u64, callback: TimerCallback) {
    // Only restore if the timer has not been cancelled in the meantime.
    if !is_timer_cancelled(id) {
        callbacks().lock().unwrap().insert(id, callback);
    } else {
        // Timer was cancelled while we were invoking the callback.
        PENDING_TIMER_COUNT.fetch_sub(1, Ordering::SeqCst);
        cancelled().lock().unwrap().remove(&id);
    }
}

/// Remove a timer callback and decrement count (used for final cleanup).
pub fn remove_timer_callback(id: u64) {
    if callbacks().lock().unwrap().remove(&id).is_some() {
        PENDING_TIMER_COUNT.fetch_sub(1, Ordering::SeqCst);
    }
    cancelled().lock().unwrap().remove(&id);
}
