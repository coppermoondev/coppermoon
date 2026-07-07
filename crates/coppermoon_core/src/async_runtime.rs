//! Async runtime for CopperMoon
//!
//! Provides the bridge between Rust async operations and Lua.
//! Lua code remains synchronous but can call async Rust functions
//! that yield transparently.

use std::future::Future;
use std::time::Duration;
use tokio::runtime::Runtime as TokioRuntime;
use std::sync::OnceLock;

/// Global Tokio runtime for async operations
static TOKIO_RUNTIME: OnceLock<TokioRuntime> = OnceLock::new();

/// Get or create the global Tokio runtime
pub fn get_runtime() -> &'static TokioRuntime {
    TOKIO_RUNTIME.get_or_init(|| {
        TokioRuntime::new().expect("Failed to create Tokio runtime")
    })
}

/// Run a future to completion on the Tokio runtime
pub fn block_on<F: Future>(future: F) -> F::Output {
    get_runtime().block_on(future)
}

/// Run a future to completion on the current thread while driving a Tokio
/// [`LocalSet`](tokio::task::LocalSet), without entering Tokio's blocking guard.
///
/// This is the entry point of the CopperMoon event loop: the future (usually
/// Lua chunk execution via `exec_async`) is polled on the calling thread by a
/// lightweight executor, and `tokio::task::spawn_local` tasks (timer callbacks,
/// HTTP request handlers) are interleaved with it whenever it suspends on an
/// `await`. Actual I/O and timers are driven by the global multi-threaded
/// Tokio runtime's worker threads.
///
/// Because the calling thread never enters Tokio's `block_on`, synchronous
/// stdlib functions that call [`block_on`] internally — as well as embedded
/// libraries that create their own Tokio runtime (e.g. the `postgres` crate) —
/// keep working when invoked from Lua code running inside this loop. They
/// simply pause the event loop for their duration, exactly like before.
pub fn run_local<F: Future>(future: F) -> F::Output {
    // Make the runtime handle ambient so Tokio I/O objects, timers and
    // `spawn` work from within the future.
    let _guard = get_runtime().enter();
    let local = tokio::task::LocalSet::new();
    futures_executor::block_on(local.run_until(future))
}

/// Spawn a task on the Tokio runtime
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    get_runtime().spawn(future)
}

/// Sleep for the specified duration (async)
pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

/// Sleep for milliseconds (blocking from Lua's perspective)
pub fn sleep_blocking(ms: u64) {
    block_on(sleep(Duration::from_millis(ms)));
}

/// Execute an async operation with a timeout
pub fn with_timeout<F, T>(duration: Duration, future: F) -> std::result::Result<T, tokio::time::error::Elapsed>
where
    F: Future<Output = T>,
{
    block_on(async {
        tokio::time::timeout(duration, future).await
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sleep_blocking() {
        let start = std::time::Instant::now();
        sleep_blocking(100);
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(90));
    }

    #[test]
    fn test_block_on() {
        let result = block_on(async {
            42
        });
        assert_eq!(result, 42);
    }
}
