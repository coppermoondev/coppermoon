//! Path manipulation module for CopperMoon
//!
//! Provides path manipulation utilities.

use coppermoon_core::Result;
use mlua::{Lua, Table, Value, MultiValue};
use std::path::{Path, PathBuf};

/// Register the path module
pub fn register(lua: &Lua) -> Result<Table> {
    let path_table = lua.create_table()?;

    // path.join(...) -> string
    path_table.set("join", lua.create_function(path_join)?)?;

    // path.dirname(path) -> string
    path_table.set("dirname", lua.create_function(path_dirname)?)?;

    // path.basename(path) -> string
    path_table.set("basename", lua.create_function(path_basename)?)?;

    // path.extname(path) -> string
    path_table.set("extname", lua.create_function(path_extname)?)?;

    // path.resolve(path) -> string
    path_table.set("resolve", lua.create_function(path_resolve)?)?;

    // path.normalize(path) -> string
    path_table.set("normalize", lua.create_function(path_normalize)?)?;

    // path.is_absolute(path) -> boolean
    path_table.set("is_absolute", lua.create_function(path_is_absolute)?)?;

    // path.is_relative(path) -> boolean
    path_table.set("is_relative", lua.create_function(path_is_relative)?)?;

    // path.sep -> string (path separator)
    path_table.set("sep", std::path::MAIN_SEPARATOR.to_string())?;

    Ok(path_table)
}

fn path_join(_: &Lua, args: MultiValue) -> mlua::Result<String> {
    let mut result = PathBuf::new();

    for arg in args {
        if let Value::String(s) = arg {
            let part = s.to_str()
                .map_err(|e| mlua::Error::runtime(format!("Invalid UTF-8 in path: {}", e)))?;
            result.push(part.as_ref());
        }
    }

    Ok(result.to_string_lossy().to_string())
}

fn path_dirname(_: &Lua, path: String) -> mlua::Result<Option<String>> {
    let p = Path::new(&path);
    Ok(p.parent().map(|p| p.to_string_lossy().to_string()))
}

fn path_basename(_: &Lua, path: String) -> mlua::Result<Option<String>> {
    let p = Path::new(&path);
    Ok(p.file_name().map(|n| n.to_string_lossy().to_string()))
}

fn path_extname(_: &Lua, path: String) -> mlua::Result<Option<String>> {
    let p = Path::new(&path);
    Ok(p.extension().map(|e| format!(".{}", e.to_string_lossy())))
}

fn path_resolve(_: &Lua, path: String) -> mlua::Result<String> {
    let p = Path::new(&path);

    if p.is_absolute() {
        Ok(p.to_string_lossy().to_string())
    } else {
        let cwd = std::env::current_dir()
            .map_err(|e| mlua::Error::runtime(format!("Failed to get current directory: {}", e)))?;
        Ok(cwd.join(p).to_string_lossy().to_string())
    }
}

fn path_normalize(_: &Lua, path: String) -> mlua::Result<String> {
    let p = Path::new(&path);
    let mut result = PathBuf::new();

    for component in p.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            _ => {
                result.push(component);
            }
        }
    }

    Ok(result.to_string_lossy().to_string())
}

fn path_is_absolute(_: &Lua, path: String) -> mlua::Result<bool> {
    Ok(Path::new(&path).is_absolute())
}

fn path_is_relative(_: &Lua, path: String) -> mlua::Result<bool> {
    Ok(Path::new(&path).is_relative())
}
