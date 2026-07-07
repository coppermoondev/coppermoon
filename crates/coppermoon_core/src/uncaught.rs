//! Uncaught error policy for CopperMoon
//!
//! Errors raised by background Lua callbacks (HTTP handlers, timers) don't
//! bubble up to a caller: the runtime logs them and keeps going. Lua code
//! can install its own hook with `process.on_error(fn)` to report them
//! (structured logging, alerting, …).
//!
//! The hook receives `(message, context)` where `context` is a short slug
//! like `"http handler"` or `"timer"`. Errors raised *by the hook itself*
//! are logged and never propagate — the policy code must not create new
//! uncaught errors.

use mlua::{Function, Lua, RegistryKey};
use std::sync::{Mutex, OnceLock};

static HANDLER: OnceLock<Mutex<Option<RegistryKey>>> = OnceLock::new();

fn handler() -> &'static Mutex<Option<RegistryKey>> {
    HANDLER.get_or_init(|| Mutex::new(None))
}

/// Install (or clear, with `None`) the Lua uncaught-error hook.
pub fn set_handler(lua: &Lua, func: Option<Function>) -> mlua::Result<()> {
    let mut guard = handler().lock().unwrap_or_else(|p| p.into_inner());
    // Drop any previous handler from the registry.
    if let Some(old) = guard.take() {
        let _ = lua.remove_registry_value(old);
    }
    if let Some(func) = func {
        *guard = Some(lua.create_registry_value(func)?);
    }
    Ok(())
}

/// Report an uncaught error from a background callback.
///
/// Calls the Lua hook when one is installed; always logs otherwise (and
/// also when the hook itself fails).
pub async fn report(lua: &Lua, context: &str, err: &mlua::Error) {
    let message = err.to_string();

    // Fetch the hook (clone the function out so the lock is not held
    // across the await below).
    let hook: Option<Function> = {
        let guard = handler().lock().unwrap_or_else(|p| p.into_inner());
        guard
            .as_ref()
            .and_then(|key| lua.registry_value::<Function>(key).ok())
    };

    if let Some(hook) = hook {
        match hook.call_async::<()>((message.clone(), context.to_string())).await {
            Ok(()) => return,
            Err(hook_err) => {
                tracing::error!(context, "uncaught error hook failed: {hook_err}");
                eprintln!("Uncaught error hook failed: {}", hook_err);
                // Fall through to default logging of the original error.
            }
        }
    }

    tracing::error!(context, "uncaught Lua error: {message}");
    eprintln!("Uncaught error ({}): {}", context, message);
}
