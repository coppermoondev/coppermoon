//! CopperMoon Standard Library
//!
//! This crate provides the standard library modules for CopperMoon,
//! including fs, path, os, process, json, crypto, time, http, net and more.

pub mod prelude;
pub mod fs;
pub mod path;
pub mod os;
pub mod process;
pub mod json;
pub mod crypto;
pub mod time;
pub mod http;
pub mod http_server;
pub mod net;
pub mod websocket;
pub mod buffer;
pub mod term;
pub mod console;
pub mod string_ext;
pub mod table_ext;
pub mod archive;
pub mod datetime;
pub mod regex;

use coppermoon_core::Result;
use mlua::{Function, Lua, Table};

/// Build a *hybrid* Lua function that dispatches between an async and a
/// synchronous implementation of the same operation.
///
/// Async Lua functions can only suspend when the current coroutine is
/// yieldable; calling one across a Lua C-call boundary (module top-level code
/// inside `require`, metamethods, `table.sort` comparators, …) raises
/// "attempt to yield across a C-call boundary". The returned function checks
/// `coroutine.isyieldable()` at call time: in yieldable contexts it takes the
/// async path (the event loop keeps running during the operation), otherwise
/// it falls back to the blocking implementation (identical result, event loop
/// paused — the pre-async behaviour).
pub(crate) fn hybrid_fn(lua: &Lua, async_fn: Function, sync_fn: Function) -> mlua::Result<Function> {
    lua.load(
        r#"
        local async_fn, sync_fn = ...
        return function(...)
            if coroutine.isyieldable() then
                return async_fn(...)
            end
            return sync_fn(...)
        end
        "#,
    )
    .set_name("=[coppermoon hybrid dispatch]")
    .call((async_fn, sync_fn))
}

/// Register all standard library modules in the Lua state
pub fn register_all(lua: &Lua) -> Result<()> {
    // Register prelude (global functions)
    prelude::register(lua)?;

    // Register global timer functions (setTimeout, setInterval, etc.)
    time::register_globals(lua)?;

    // Create and register modules
    let globals = lua.globals();

    // fs module
    globals.set("fs", fs::register(lua)?)?;

    // path module
    globals.set("path", path::register(lua)?)?;

    // os_ext module (extends built-in os)
    globals.set("os_ext", os::register(lua)?)?;

    // process module
    globals.set("process", process::register(lua)?)?;

    // json module
    globals.set("json", json::register(lua)?)?;

    // crypto module
    globals.set("crypto", crypto::register(lua)?)?;

    // time module
    globals.set("time", time::register(lua)?)?;

    // http module (with server sub-module)
    let http_module: Table = http::register(lua)?;
    http_module.set("server", http_server::register(lua)?)?;
    globals.set("http", http_module)?;

    // net module (TCP/UDP/WebSocket)
    let net_module: Table = net::register(lua)?;
    net_module.set("ws", websocket::register(lua)?)?;
    globals.set("net", net_module)?;

    // buffer module (binary data manipulation)
    globals.set("buffer", buffer::register(lua)?)?;

    // term module (terminal styling and control)
    globals.set("term", term::register(lua)?)?;

    // console module (interactive input)
    globals.set("console", console::register(lua)?)?;

    // archive module (zip, tar, gzip)
    globals.set("archive", archive::register(lua)?)?;

    // regex module (regular expressions)
    globals.set("re", regex::register(lua)?)?;

    // Extend built-in string table with utility functions
    string_ext::register(lua)?;

    // Extend built-in table table with utility functions
    table_ext::register(lua)?;

    Ok(())
}
