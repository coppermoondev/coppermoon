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

use coppermoon_core::Result;
use mlua::{Lua, Table};

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

    // Extend built-in string table with utility functions
    string_ext::register(lua)?;

    // Extend built-in table table with utility functions
    table_ext::register(lua)?;

    Ok(())
}
