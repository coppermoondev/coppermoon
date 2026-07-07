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
/// Timer callbacks currently executing (spawned on the event loop).
/// Keeps the process alive until a callback in flight has finished, even if
/// its registration has already been consumed (one-shot timeouts).
static RUNNING_CALLBACK_COUNT: AtomicUsize = AtomicUsize::new(0);

static TIMER_CALLBACKS: OnceLock<Mutex<HashMap<u64, TimerCallback>>> = OnceLock::new();
static CANCELLED_TIMERS: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();

/// Channel pair for timer events (sender, receiver).
static TIMER_CHANNEL: OnceLock<(
    std::sync::mpsc::Sender<TimerEvent>,
    Mutex<std::sync::mpsc::Receiver<TimerEvent>>,
)> = OnceLock::new();

/// Async notifier fired whenever a timer event is pushed on the channel.
static TIMER_NOTIFY: OnceLock<tokio::sync::Notify> = OnceLock::new();

// ---------------------------------------------------------------------------
// Accessors for lazy-initialised global state
// ---------------------------------------------------------------------------

/// Lock a mutex, recovering from poisoning.
///
/// The state protected here (timer maps, event receiver) stays consistent
/// even if a thread panicked while holding the lock, so a poisoned lock
/// must not take the whole timer system down with it.
fn lock_ok<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

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

fn notify() -> &'static tokio::sync::Notify {
    TIMER_NOTIFY.get_or_init(tokio::sync::Notify::new)
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
    lock_ok(callbacks()).insert(id, callback);
    PENDING_TIMER_COUNT.fetch_add(1, Ordering::SeqCst);
}

/// Cancel a timer. Decrements the pending timer count.
pub fn cancel_timer(id: u64) {
    lock_ok(cancelled()).insert(id);
    // Remove the callback if it exists and decrement counter
    if lock_ok(callbacks()).remove(&id).is_some() {
        PENDING_TIMER_COUNT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Check whether a timer has been cancelled.
pub fn is_timer_cancelled(id: u64) -> bool {
    lock_ok(cancelled()).contains(&id)
}

/// Returns `true` if there are timers that have not yet fired or been
/// cancelled, or timer callbacks still executing.
pub fn has_pending_timers() -> bool {
    PENDING_TIMER_COUNT.load(Ordering::SeqCst) > 0
        || RUNNING_CALLBACK_COUNT.load(Ordering::SeqCst) > 0
}

/// Mark a timer callback as started (called by the timer pump before
/// spawning the callback task).
pub fn callback_started() {
    RUNNING_CALLBACK_COUNT.fetch_add(1, Ordering::SeqCst);
}

/// Mark a timer callback as finished.
pub fn callback_finished() {
    RUNNING_CALLBACK_COUNT.fetch_sub(1, Ordering::SeqCst);
}

/// Run a closure against a registered timer callback without consuming it.
/// Used for intervals: the registration stays in place so the next tick can
/// fire even while the current callback is still running.
pub fn with_timer_callback<R>(id: u64, f: impl FnOnce(&TimerCallback) -> R) -> Option<R> {
    let cbs = lock_ok(callbacks());
    cbs.get(&id).map(f)
}

// ---------------------------------------------------------------------------
// Public API — event channel
// ---------------------------------------------------------------------------

/// Called by Tokio timer tasks when a timer is ready to fire.
pub fn send_timer_ready(id: u64) {
    let (tx, _) = channel();
    // Ignore send error — the receiver may have been dropped (shutdown).
    let _ = tx.send(TimerEvent::Ready(id));
    // Wake any async waiter (event-loop timer pump).
    notify().notify_one();
}

/// Try to receive a timer event, blocking for at most `timeout`.
/// Returns `None` on timeout or if the channel is disconnected.
pub fn try_recv_timer_event(timeout: Duration) -> Option<TimerEvent> {
    let (_, rx) = channel();
    let rx = lock_ok(rx);
    rx.recv_timeout(timeout).ok()
}

/// Receive the next timer event without blocking the thread.
///
/// Used by the async event loop (timer pump): while awaiting here, other
/// tasks on the same `LocalSet` (HTTP handlers, the main chunk) keep running.
pub async fn recv_timer_event() -> TimerEvent {
    loop {
        // Fast path — drain anything already queued. The lock guard is
        // dropped before awaiting.
        {
            let (_, rx) = channel();
            if let Ok(event) = lock_ok(rx).try_recv() {
                return event;
            }
        }
        // `notify_one` stores a permit when no waiter is registered, so an
        // event sent between the `try_recv` above and this await is not lost.
        notify().notified().await;
    }
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
    let mut cbs = lock_ok(callbacks());
    let cb = cbs.remove(&id)?;
    match cb.timer_type {
        TimerType::Timeout => {
            PENDING_TIMER_COUNT.fetch_sub(1, Ordering::SeqCst);
            // Clean up cancellation set entry if present
            lock_ok(cancelled()).remove(&id);
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
        lock_ok(callbacks()).insert(id, callback);
    } else {
        // Timer was cancelled while we were invoking the callback.
        PENDING_TIMER_COUNT.fetch_sub(1, Ordering::SeqCst);
        lock_ok(cancelled()).remove(&id);
    }
}

/// Remove a timer callback and decrement count (used for final cleanup).
pub fn remove_timer_callback(id: u64) {
    if lock_ok(callbacks()).remove(&id).is_some() {
        PENDING_TIMER_COUNT.fetch_sub(1, Ordering::SeqCst);
    }
    lock_ok(cancelled()).remove(&id);
}
