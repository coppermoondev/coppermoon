//! Lua runtime management

use crate::{Error, Result, event_loop};
use crate::event_loop::{TimerEvent, TimerType};
use mlua::{Lua, Function, MultiValue, Value, StdLib};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, info};

/// The CopperMoon Lua runtime
pub struct Runtime {
    lua: Lua,
    /// Base path for module resolution
    base_path: PathBuf,
}

impl Runtime {
    /// Create a new runtime instance
    pub fn new() -> Result<Self> {
        let lua = Lua::new();

        // Open standard libraries
        lua.load_std_libs(StdLib::ALL_SAFE)?;

        // Optional VM memory limit (e.g. COPPERMOON_MEMORY_LIMIT=512M).
        // When exceeded, allocations fail with a *catchable* Lua error
        // instead of the process growing without bound.
        if let Ok(raw) = std::env::var("COPPERMOON_MEMORY_LIMIT") {
            match parse_byte_size(&raw) {
                Some(limit) if limit > 0 => {
                    if let Err(e) = lua.set_memory_limit(limit) {
                        eprintln!("warning: cannot apply COPPERMOON_MEMORY_LIMIT: {}", e);
                    } else {
                        MEMORY_LIMIT.store(limit, std::sync::atomic::Ordering::Relaxed);
                        debug!("Lua memory limit set to {} bytes", limit);
                    }
                }
                _ => {
                    eprintln!(
                        "warning: invalid COPPERMOON_MEMORY_LIMIT '{}' (expected e.g. 268435456, 256M, 1G)",
                        raw
                    );
                }
            }
        }

        // Initialize native module library store
        lua.set_app_data(crate::module::NativeLibStore::new());

        let base_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        debug!("CopperMoon runtime initialized");

        Ok(Self { lua, base_path })
    }

    /// Create a runtime with a specific base path
    pub fn with_base_path<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let mut runtime = Self::new()?;
        runtime.base_path = base_path.as_ref().to_path_buf();
        Ok(runtime)
    }

    /// Set the base path for module resolution
    pub fn set_base_path<P: AsRef<Path>>(&mut self, path: P) {
        self.base_path = path.as_ref().to_path_buf();
    }

    /// Get a reference to the Lua state
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// Execute a Lua script from a string
    pub fn exec(&self, code: &str) -> Result<()> {
        let chunk = self.lua.load(code);
        self.drive(chunk.exec_async(), true)?;
        Ok(())
    }

    /// Execute a Lua script and return its result as a string (for REPL)
    pub fn eval(&self, code: &str) -> Result<String> {
        let chunk = self.lua.load(code);
        let result: MultiValue = self.drive(chunk.eval_async(), false)?;

        let formatted = result
            .iter()
            .map(|v| format_value(v))
            .collect::<Vec<_>>()
            .join("\t");

        Ok(formatted)
    }

    /// Execute a Lua file
    pub fn exec_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_path.join(path)
        };

        info!("Executing file: {}", absolute_path.display());

        let code = std::fs::read_to_string(&absolute_path).map_err(|e| {
            Error::Runtime(format!(
                "Failed to read file '{}': {}",
                absolute_path.display(),
                e
            ))
        })?;

        // The `@` prefix marks the chunk name as a file name, so error
        // messages and tracebacks display `path:line` instead of the
        // truncated `[string "..."]` form.
        let chunk = self.lua
            .load(&code)
            .set_name(format!("@{}", absolute_path.display()));

        // Handle Ctrl+C / SIGTERM gracefully while running a script.
        crate::shutdown::install_signal_handlers();

        // Execute the chunk on the async event loop, then keep the process
        // alive while timers (setTimeout, setInterval) are pending.
        self.drive(chunk.exec_async(), true)?;

        Ok(())
    }

    /// Drive a Lua future to completion on the CopperMoon event loop.
    ///
    /// The future runs on the current thread inside a Tokio `LocalSet`
    /// (see [`coppermoon_core::run_local`]); a background *timer pump* task
    /// fires `setTimeout`/`setInterval` callbacks, and tasks spawned with
    /// `tokio::task::spawn_local` (e.g. HTTP request handlers) are
    /// interleaved whenever Lua suspends on an async operation.
    ///
    /// When `keep_alive` is true, this keeps running after the future
    /// completes for as long as timers are pending — like Node.js.
    fn drive<T>(
        &self,
        fut: impl std::future::Future<Output = mlua::Result<T>>,
        keep_alive: bool,
    ) -> Result<T> {
        let pump_lua = self.lua.clone();
        let result = crate::async_runtime::run_local(async move {
            let pump = tokio::task::spawn_local(timer_pump(pump_lua));
            let result = fut.await;
            if result.is_ok() && keep_alive {
                // Node.js-style keep-alive: stay up while timers are pending,
                // unless a graceful shutdown was requested.
                while event_loop::has_pending_timers() && !crate::shutdown::is_requested() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
            pump.abort();
            result
        })?;
        Ok(result)
    }

    /// Run the event loop to drain pending timer callbacks.
    ///
    /// This keeps the process alive as long as there are pending timers,
    /// similar to how Node.js keeps running while timers are active.
    pub fn run_event_loop(&self) -> Result<()> {
        let pump_lua = self.lua.clone();
        crate::async_runtime::run_local(async move {
            let pump = tokio::task::spawn_local(timer_pump(pump_lua));
            while event_loop::has_pending_timers() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            pump.abort();
        });
        Ok(())
    }

    /// Set a global variable
    pub fn set_global<V: mlua::IntoLua>(&self, name: &str, value: V) -> Result<()> {
        self.lua.globals().set(name, value)?;
        Ok(())
    }

    /// Get a global variable
    pub fn get_global<V: mlua::FromLua>(&self, name: &str) -> Result<V> {
        let value = self.lua.globals().get(name)?;
        Ok(value)
    }

    /// Setup the custom module loader
    pub fn setup_module_loader(&self) -> Result<()> {
        crate::module::setup_loader(&self.lua, &self.base_path)?;
        Ok(())
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new().expect("Failed to create default runtime")
    }
}

/// Background task that fires ready timer callbacks.
///
/// Runs on the event-loop `LocalSet` for the whole lifetime of a script.
/// Each callback is spawned as its own task and invoked with `call_async`,
/// so callbacks run *concurrently*: a callback suspended on an async
/// operation (http, time.sleep, accept, …) does not delay other timers —
/// Node.js semantics.
async fn timer_pump(lua: Lua) {
    loop {
        let TimerEvent::Ready(id) = event_loop::recv_timer_event().await;

        // Intervals: fetch the function without consuming the registration,
        // so the next tick can fire even while this one is still running.
        let interval_func = event_loop::with_timer_callback(id, |cb| match cb.timer_type {
            TimerType::Interval { .. } => lua.registry_value::<Function>(&cb.registry_key).ok(),
            TimerType::Timeout => None,
        })
        .flatten();

        if let Some(func) = interval_func {
            let lua = lua.clone();
            event_loop::callback_started();
            tokio::task::spawn_local(async move {
                if let Err(e) = func.call_async::<()>(()).await {
                    crate::uncaught::report(&lua, "timer", &e).await;
                }
                event_loop::callback_finished();
            });
            continue;
        }

        // One-shot timeouts: consume the registration (this decrements the
        // pending count — the running-callback count keeps the process
        // alive until the spawned task finishes).
        if let Some(cb) = event_loop::take_timer_callback(id) {
            match cb.timer_type {
                TimerType::Timeout => {
                    let func: mlua::Result<Function> = lua.registry_value(&cb.registry_key);
                    let lua = lua.clone();
                    event_loop::callback_started();
                    tokio::task::spawn_local(async move {
                        if let Ok(func) = func {
                            if let Err(e) = func.call_async::<()>(()).await {
                                crate::uncaught::report(&lua, "timer", &e).await;
                            }
                        }
                        let _ = lua.remove_registry_value(cb.registry_key);
                        event_loop::callback_finished();
                    });
                }
                TimerType::Interval { .. } => {
                    // Raced with a concurrent removal — put it back.
                    event_loop::restore_timer_callback(id, cb);
                }
            }
        }
    }
}

/// Applied VM memory limit in bytes (0 = unlimited).
static MEMORY_LIMIT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The VM memory limit applied at startup, in bytes (0 = unlimited).
pub fn memory_limit() -> usize {
    MEMORY_LIMIT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Parse a human-readable byte size: plain bytes, or with a K/M/G suffix
/// (optionally followed by "B" or "iB", case-insensitive). Examples:
/// `268435456`, `256K`, `512M`, `1G`, `2GiB`.
fn parse_byte_size(raw: &str) -> Option<usize> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    let (num_part, multiplier) = if let Some(pos) = lower.find(|c: char| !c.is_ascii_digit()) {
        let suffix = lower[pos..].trim();
        let mult: usize = match suffix.trim_end_matches("ib").trim_end_matches('b') {
            "k" => 1024,
            "m" => 1024 * 1024,
            "g" => 1024 * 1024 * 1024,
            "" => 1, // bare "b" suffix
            _ => return None,
        };
        (&lower[..pos], mult)
    } else {
        (lower.as_str(), 1)
    };
    num_part.parse::<usize>().ok()?.checked_mul(multiplier)
}

/// Format a Lua value for display
fn format_value(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => {
            if n.fract() == 0.0 {
                format!("{:.0}", n)
            } else {
                n.to_string()
            }
        }
        Value::String(s) => {
            match s.to_str() {
                Ok(str) => format!("\"{}\"", str),
                Err(_) => "\"<invalid utf8>\"".to_string(),
            }
        }
        Value::Table(_) => "table".to_string(),
        Value::Function(_) => "function".to_string(),
        Value::Thread(_) => "thread".to_string(),
        Value::UserData(_) => "userdata".to_string(),
        Value::LightUserData(_) => "lightuserdata".to_string(),
        Value::Error(e) => format!("error: {}", e),
        _ => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let runtime = Runtime::new();
        assert!(runtime.is_ok());
    }

    #[test]
    fn test_exec_simple() {
        let runtime = Runtime::new().unwrap();
        let result = runtime.exec("x = 1 + 1");
        assert!(result.is_ok());

        let x: i64 = runtime.get_global("x").unwrap();
        assert_eq!(x, 2);
    }

    #[test]
    fn test_eval() {
        let runtime = Runtime::new().unwrap();
        let result = runtime.eval("return 1 + 1").unwrap();
        assert_eq!(result, "2");
    }

    #[test]
    fn test_eval_string() {
        let runtime = Runtime::new().unwrap();
        let result = runtime.eval("return 'hello'").unwrap();
        assert_eq!(result, "\"hello\"");
    }

    #[test]
    fn test_eval_multiple_values() {
        let runtime = Runtime::new().unwrap();
        let result = runtime.eval("return 1, 2, 3").unwrap();
        assert_eq!(result, "1\t2\t3");
    }

    #[test]
    fn test_parse_byte_size() {
        assert_eq!(parse_byte_size("1024"), Some(1024));
        assert_eq!(parse_byte_size("256K"), Some(256 * 1024));
        assert_eq!(parse_byte_size("512M"), Some(512 * 1024 * 1024));
        assert_eq!(parse_byte_size("1G"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_byte_size("2GiB"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_byte_size("128MB"), Some(128 * 1024 * 1024));
        assert_eq!(parse_byte_size(" 64m "), Some(64 * 1024 * 1024));
        assert_eq!(parse_byte_size("abc"), None);
        assert_eq!(parse_byte_size("10T"), None);
        assert_eq!(parse_byte_size(""), None);
    }
}
