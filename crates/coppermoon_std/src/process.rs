//! Process module for CopperMoon
//!
//! Provides process management and execution utilities.

use coppermoon_core::Result;
use mlua::{Lua, Table};
use std::process::{Command, Stdio};

/// Register the process module
pub fn register(lua: &Lua) -> Result<Table> {
    let process_table = lua.create_table()?;

    // process.exit(code)
    process_table.set("exit", lua.create_function(process_exit)?)?;

    // process.shutdown() — request a graceful shutdown: the HTTP server
    // drains in-flight requests, the event loop exits, the process ends
    // cleanly. Same path as Ctrl+C / SIGTERM.
    process_table.set("shutdown", lua.create_function(|_, ()| {
        coppermoon_core::shutdown::request();
        Ok(())
    })?)?;

    // process.on_error(fn | nil) — install/clear the uncaught-error hook.
    // Called with (message, context) for errors raised by background
    // callbacks (HTTP handlers, timers). Default policy: log + continue.
    process_table.set("on_error", lua.create_function(
        |lua, func: Option<mlua::Function>| {
            coppermoon_core::uncaught::set_handler(lua, func)
        },
    )?)?;

    // process.memory() -> { used, limit } — Lua VM memory usage in bytes
    // (limit is 0 when COPPERMOON_MEMORY_LIMIT is not set).
    process_table.set("memory", lua.create_function(|lua, ()| {
        let t = lua.create_table()?;
        t.set("used", lua.used_memory())?;
        t.set("limit", coppermoon_core::runtime::memory_limit())?;
        Ok(t)
    })?)?;

    // process.pid() -> number
    process_table.set("pid", lua.create_function(process_pid)?)?;

    // process.exec(cmd) -> { stdout, stderr, status }
    process_table.set("exec", lua.create_function(process_exec)?)?;

    // process.spawn(cmd, args) -> { stdout, stderr, status }
    process_table.set("spawn", lua.create_function(process_spawn)?)?;

    Ok(process_table)
}

fn process_exit(_: &Lua, code: Option<i32>) -> mlua::Result<()> {
    std::process::exit(code.unwrap_or(0));
}

fn process_pid(_: &Lua, _: ()) -> mlua::Result<u32> {
    Ok(std::process::id())
}

fn process_exec(lua: &Lua, cmd: String) -> mlua::Result<Table> {
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", &cmd])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    } else {
        Command::new("sh")
            .args(["-c", &cmd])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    };

    let output = output
        .map_err(|e| mlua::Error::runtime(format!("Failed to execute command: {}", e)))?;

    let result = lua.create_table()?;
    result.set("stdout", String::from_utf8_lossy(&output.stdout).to_string())?;
    result.set("stderr", String::from_utf8_lossy(&output.stderr).to_string())?;
    result.set("status", output.status.code().unwrap_or(-1))?;
    result.set("success", output.status.success())?;

    Ok(result)
}

fn process_spawn(lua: &Lua, (cmd, args): (String, Option<Table>)) -> mlua::Result<Table> {
    let mut command = Command::new(&cmd);

    // Add arguments if provided
    if let Some(args_table) = args {
        for pair in args_table.pairs::<i64, String>() {
            if let Ok((_, arg)) = pair {
                command.arg(arg);
            }
        }
    }

    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| mlua::Error::runtime(format!("Failed to spawn process '{}': {}", cmd, e)))?;

    let result = lua.create_table()?;
    result.set("stdout", String::from_utf8_lossy(&output.stdout).to_string())?;
    result.set("stderr", String::from_utf8_lossy(&output.stderr).to_string())?;
    result.set("status", output.status.code().unwrap_or(-1))?;
    result.set("success", output.status.success())?;

    Ok(result)
}
