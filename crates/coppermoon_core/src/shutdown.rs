//! Graceful shutdown coordination for CopperMoon
//!
//! Holds a process-wide shutdown flag that OS signals (Ctrl+C, SIGTERM) or
//! Lua code (`process.shutdown()`) can raise. The event loop and the HTTP
//! server observe it:
//!
//! - the HTTP server stops accepting connections, drains in-flight requests
//!   (with a grace deadline) and returns from `listen`
//! - the timer keep-alive loop exits even if timers are still pending
//!
//! A second signal forces an immediate exit (exit code 130), so a stuck
//! script can always be terminated from the terminal.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::watch;

static STATE: OnceLock<(watch::Sender<bool>, watch::Receiver<bool>)> = OnceLock::new();
static HANDLERS_INSTALLED: AtomicBool = AtomicBool::new(false);

fn state() -> &'static (watch::Sender<bool>, watch::Receiver<bool>) {
    STATE.get_or_init(|| watch::channel(false))
}

/// Request a graceful shutdown (idempotent).
pub fn request() {
    let _ = state().0.send(true);
}

/// Whether a graceful shutdown has been requested.
pub fn is_requested() -> bool {
    *state().1.borrow()
}

/// Wait until a graceful shutdown is requested.
/// Returns immediately if one has already been requested.
pub async fn requested() {
    let mut rx = state().1.clone();
    let _ = rx.wait_for(|v| *v).await;
}

/// Install OS signal handlers (idempotent).
///
/// The first signal (Ctrl+C / SIGINT / SIGTERM / console close) requests a
/// graceful shutdown; a second one force-exits with code 130.
pub fn install_signal_handlers() {
    if HANDLERS_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::async_runtime::spawn(async {
        wait_for_signal().await;
        request();
        wait_for_signal().await;
        eprintln!("Forced shutdown");
        std::process::exit(130);
    });
}

#[cfg(unix)]
async fn wait_for_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(_) => {
            // Fall back to Ctrl+C only.
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

#[cfg(windows)]
async fn wait_for_signal() {
    use tokio::signal::windows;
    // Register what we can; any of these triggers the shutdown path.
    let mut ctrl_break = windows::ctrl_break().ok();
    let mut ctrl_close = windows::ctrl_close().ok();
    let mut ctrl_shutdown = windows::ctrl_shutdown().ok();

    let break_fut = async {
        match ctrl_break.as_mut() {
            Some(s) => { s.recv().await; }
            None => std::future::pending().await,
        }
    };
    let close_fut = async {
        match ctrl_close.as_mut() {
            Some(s) => { s.recv().await; }
            None => std::future::pending().await,
        }
    };
    let shutdown_fut = async {
        match ctrl_shutdown.as_mut() {
            Some(s) => { s.recv().await; }
            None => std::future::pending().await,
        }
    };

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = break_fut => {}
        _ = close_fut => {}
        _ = shutdown_fut => {}
    }
}

#[cfg(all(not(unix), not(windows)))]
async fn wait_for_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
