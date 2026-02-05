//! Extended OS module for CopperMoon
//!
//! Provides operating system utilities beyond the standard Lua os module.

use coppermoon_core::Result;
use mlua::{Lua, Table};

/// Register the os_ext module (extends built-in os)
pub fn register(lua: &Lua) -> Result<Table> {
    let os_table = lua.create_table()?;

    // os.env(key) -> string | nil
    os_table.set("env", lua.create_function(os_env)?)?;

    // os.setenv(key, value)
    os_table.set("setenv", lua.create_function(os_setenv)?)?;

    // os.unsetenv(key)
    os_table.set("unsetenv", lua.create_function(os_unsetenv)?)?;

    // os.cwd() -> string
    os_table.set("cwd", lua.create_function(os_cwd)?)?;

    // os.chdir(path) -> boolean
    os_table.set("chdir", lua.create_function(os_chdir)?)?;

    // os.platform() -> string
    os_table.set("platform", lua.create_function(os_platform)?)?;

    // os.arch() -> string
    os_table.set("arch", lua.create_function(os_arch)?)?;

    // os.homedir() -> string
    os_table.set("homedir", lua.create_function(os_homedir)?)?;

    // os.tmpdir() -> string
    os_table.set("tmpdir", lua.create_function(os_tmpdir)?)?;

    // os.hostname() -> string
    os_table.set("hostname", lua.create_function(os_hostname)?)?;

    // os.cpus() -> number
    os_table.set("cpus", lua.create_function(os_cpus)?)?;

    Ok(os_table)
}

fn os_env(_: &Lua, key: String) -> mlua::Result<Option<String>> {
    Ok(std::env::var(&key).ok())
}

fn os_setenv(_: &Lua, (key, value): (String, String)) -> mlua::Result<()> {
    // Note: This is unsafe in multi-threaded contexts, but Lua is single-threaded per state
    unsafe {
        std::env::set_var(&key, &value);
    }
    Ok(())
}

fn os_unsetenv(_: &Lua, key: String) -> mlua::Result<()> {
    unsafe {
        std::env::remove_var(&key);
    }
    Ok(())
}

fn os_cwd(_: &Lua, _: ()) -> mlua::Result<String> {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| mlua::Error::runtime(format!("Failed to get current directory: {}", e)))
}

fn os_chdir(_: &Lua, path: String) -> mlua::Result<bool> {
    std::env::set_current_dir(&path)
        .map(|_| true)
        .map_err(|e| mlua::Error::runtime(format!("Failed to change directory to '{}': {}", path, e)))
}

fn os_platform(_: &Lua, _: ()) -> mlua::Result<String> {
    Ok(std::env::consts::OS.to_string())
}

fn os_arch(_: &Lua, _: ()) -> mlua::Result<String> {
    Ok(std::env::consts::ARCH.to_string())
}

fn os_homedir(_: &Lua, _: ()) -> mlua::Result<Option<String>> {
    Ok(dirs::home_dir().map(|p| p.to_string_lossy().to_string()))
}

fn os_tmpdir(_: &Lua, _: ()) -> mlua::Result<String> {
    Ok(std::env::temp_dir().to_string_lossy().to_string())
}

fn os_hostname(_: &Lua, _: ()) -> mlua::Result<String> {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .map_err(|e| mlua::Error::runtime(format!("Failed to get hostname: {}", e)))
}

fn os_cpus(_: &Lua, _: ()) -> mlua::Result<usize> {
    Ok(std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1))
}
