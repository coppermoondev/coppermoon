//! File system module for CopperMoon
//!
//! Provides file and directory operations.

use crate::buffer::Buffer;
use coppermoon_core::Result;
use mlua::{Lua, MultiValue, Table, Value};
use std::fs;
use std::path::Path;

/// Register the fs module
pub fn register(lua: &Lua) -> Result<Table> {
    let fs_table = lua.create_table()?;

    // ---- Read / Write ----
    fs_table.set("read", lua.create_function(fs_read)?)?;
    fs_table.set("read_bytes", lua.create_function(fs_read_bytes)?)?;
    fs_table.set("write", lua.create_function(fs_write)?)?;
    fs_table.set("write_bytes", lua.create_function(fs_write_bytes)?)?;
    fs_table.set("append", lua.create_function(fs_append)?)?;

    // ---- Existence / type checks ----
    fs_table.set("exists", lua.create_function(fs_exists)?)?;
    fs_table.set("is_file", lua.create_function(fs_is_file)?)?;
    fs_table.set("is_dir", lua.create_function(fs_is_dir)?)?;
    fs_table.set("is_symlink", lua.create_function(fs_is_symlink)?)?;

    // ---- File operations ----
    fs_table.set("remove", lua.create_function(fs_remove)?)?;
    fs_table.set("copy", lua.create_function(fs_copy)?)?;
    fs_table.set("rename", lua.create_function(fs_rename)?)?;
    fs_table.set("move", lua.create_function(fs_move)?)?;
    fs_table.set("touch", lua.create_function(fs_touch)?)?;
    fs_table.set("size", lua.create_function(fs_size)?)?;

    // ---- Directory operations ----
    fs_table.set("mkdir", lua.create_function(fs_mkdir)?)?;
    fs_table.set("mkdir_all", lua.create_function(fs_mkdir_all)?)?;
    fs_table.set("rmdir", lua.create_function(fs_rmdir)?)?;
    fs_table.set("rmdir_all", lua.create_function(fs_rmdir_all)?)?;
    fs_table.set("readdir", lua.create_function(fs_readdir)?)?;
    fs_table.set("copy_dir", lua.create_function(fs_copy_dir)?)?;

    // ---- Metadata ----
    fs_table.set("stat", lua.create_function(fs_stat)?)?;

    // ---- Path utilities ----
    fs_table.set("abs", lua.create_function(fs_abs)?)?;
    fs_table.set("join", lua.create_function(fs_join)?)?;
    fs_table.set("basename", lua.create_function(fs_basename)?)?;
    fs_table.set("dirname", lua.create_function(fs_dirname)?)?;
    fs_table.set("ext", lua.create_function(fs_ext)?)?;

    // ---- Search ----
    fs_table.set("glob", lua.create_function(fs_glob)?)?;

    // ---- Environment ----
    fs_table.set("cwd", lua.create_function(fs_cwd)?)?;
    fs_table.set("temp_dir", lua.create_function(fs_temp_dir)?)?;

    Ok(fs_table)
}

// ---------------------------------------------------------------------------
// Read / Write
// ---------------------------------------------------------------------------

fn fs_read(_: &Lua, path: String) -> mlua::Result<String> {
    fs::read_to_string(&path)
        .map_err(|e| mlua::Error::runtime(format!("Failed to read file '{}': {}", path, e)))
}

fn fs_read_bytes(_: &Lua, path: String) -> mlua::Result<Buffer> {
    let data = fs::read(&path)
        .map_err(|e| mlua::Error::runtime(format!("Failed to read file '{}': {}", path, e)))?;
    Ok(Buffer::from_bytes(data))
}

fn fs_write(_: &Lua, (path, content): (String, String)) -> mlua::Result<bool> {
    fs::write(&path, content)
        .map(|_| true)
        .map_err(|e| mlua::Error::runtime(format!("Failed to write file '{}': {}", path, e)))
}

fn fs_write_bytes(_: &Lua, (path, content): (String, Value)) -> mlua::Result<bool> {
    let bytes = extract_bytes(content)?;
    fs::write(&path, bytes)
        .map(|_| true)
        .map_err(|e| mlua::Error::runtime(format!("Failed to write file '{}': {}", path, e)))
}

fn fs_append(_: &Lua, (path, content): (String, String)) -> mlua::Result<bool> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| mlua::Error::runtime(format!("Failed to open file '{}': {}", path, e)))?;

    file.write_all(content.as_bytes())
        .map(|_| true)
        .map_err(|e| mlua::Error::runtime(format!("Failed to append to file '{}': {}", path, e)))
}

// ---------------------------------------------------------------------------
// Existence / type checks
// ---------------------------------------------------------------------------

fn fs_exists(_: &Lua, path: String) -> mlua::Result<bool> {
    Ok(Path::new(&path).exists())
}

fn fs_is_file(_: &Lua, path: String) -> mlua::Result<bool> {
    Ok(Path::new(&path).is_file())
}

fn fs_is_dir(_: &Lua, path: String) -> mlua::Result<bool> {
    Ok(Path::new(&path).is_dir())
}

fn fs_is_symlink(_: &Lua, path: String) -> mlua::Result<bool> {
    Ok(Path::new(&path).is_symlink())
}

// ---------------------------------------------------------------------------
// File operations
// ---------------------------------------------------------------------------

fn fs_remove(_: &Lua, path: String) -> mlua::Result<bool> {
    fs::remove_file(&path)
        .map(|_| true)
        .map_err(|e| mlua::Error::runtime(format!("Failed to remove file '{}': {}", path, e)))
}

fn fs_copy(_: &Lua, (src, dest): (String, String)) -> mlua::Result<u64> {
    fs::copy(&src, &dest)
        .map_err(|e| mlua::Error::runtime(format!("Failed to copy '{}' to '{}': {}", src, dest, e)))
}

fn fs_rename(_: &Lua, (src, dest): (String, String)) -> mlua::Result<bool> {
    fs::rename(&src, &dest)
        .map(|_| true)
        .map_err(|e| mlua::Error::runtime(format!("Failed to rename '{}' to '{}': {}", src, dest, e)))
}

/// Move a file or directory. Tries rename first (fast, same filesystem),
/// falls back to copy + delete for cross-filesystem moves.
fn fs_move(_: &Lua, (src, dest): (String, String)) -> mlua::Result<bool> {
    // Try rename first (instant if same filesystem)
    if fs::rename(&src, &dest).is_ok() {
        return Ok(true);
    }

    let src_path = Path::new(&src);

    if src_path.is_dir() {
        // Cross-filesystem directory move: recursive copy then remove
        copy_dir_recursive(src_path, Path::new(&dest))
            .map_err(|e| mlua::Error::runtime(format!("Failed to move '{}' to '{}': {}", src, dest, e)))?;
        fs::remove_dir_all(&src)
            .map_err(|e| mlua::Error::runtime(format!("Failed to remove source '{}' after move: {}", src, e)))?;
    } else {
        // Cross-filesystem file move: copy then remove
        fs::copy(&src, &dest)
            .map_err(|e| mlua::Error::runtime(format!("Failed to move '{}' to '{}': {}", src, dest, e)))?;
        fs::remove_file(&src)
            .map_err(|e| mlua::Error::runtime(format!("Failed to remove source '{}' after move: {}", src, e)))?;
    }

    Ok(true)
}

fn fs_touch(_: &Lua, path: String) -> mlua::Result<bool> {
    let p = Path::new(&path);
    if p.exists() {
        // Update modification time by opening and setting file length to current length
        let file = fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|e| mlua::Error::runtime(format!("Failed to touch '{}': {}", path, e)))?;
        let metadata = file.metadata()
            .map_err(|e| mlua::Error::runtime(format!("Failed to touch '{}': {}", path, e)))?;
        file.set_len(metadata.len())
            .map_err(|e| mlua::Error::runtime(format!("Failed to touch '{}': {}", path, e)))?;
    } else {
        // Create parent directories if needed, then create empty file
        if let Some(parent) = p.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| mlua::Error::runtime(format!("Failed to touch '{}': {}", path, e)))?;
            }
        }
        fs::write(&path, b"")
            .map_err(|e| mlua::Error::runtime(format!("Failed to touch '{}': {}", path, e)))?;
    }
    Ok(true)
}

fn fs_size(_: &Lua, path: String) -> mlua::Result<u64> {
    let metadata = fs::metadata(&path)
        .map_err(|e| mlua::Error::runtime(format!("Failed to get size of '{}': {}", path, e)))?;
    Ok(metadata.len())
}

// ---------------------------------------------------------------------------
// Directory operations
// ---------------------------------------------------------------------------

fn fs_mkdir(_: &Lua, path: String) -> mlua::Result<bool> {
    fs::create_dir(&path)
        .map(|_| true)
        .map_err(|e| mlua::Error::runtime(format!("Failed to create directory '{}': {}", path, e)))
}

fn fs_mkdir_all(_: &Lua, path: String) -> mlua::Result<bool> {
    fs::create_dir_all(&path)
        .map(|_| true)
        .map_err(|e| mlua::Error::runtime(format!("Failed to create directories '{}': {}", path, e)))
}

fn fs_rmdir(_: &Lua, path: String) -> mlua::Result<bool> {
    fs::remove_dir(&path)
        .map(|_| true)
        .map_err(|e| mlua::Error::runtime(format!("Failed to remove directory '{}': {}", path, e)))
}

fn fs_rmdir_all(_: &Lua, path: String) -> mlua::Result<bool> {
    fs::remove_dir_all(&path)
        .map(|_| true)
        .map_err(|e| mlua::Error::runtime(format!("Failed to remove directories '{}': {}", path, e)))
}

fn fs_readdir(lua: &Lua, path: String) -> mlua::Result<Table> {
    let entries = fs::read_dir(&path)
        .map_err(|e| mlua::Error::runtime(format!("Failed to read directory '{}': {}", path, e)))?;

    let result = lua.create_table()?;
    let mut index = 1;

    for entry in entries {
        if let Ok(entry) = entry {
            if let Some(name) = entry.file_name().to_str() {
                result.set(index, name)?;
                index += 1;
            }
        }
    }

    Ok(result)
}

/// Recursively copy a directory.
fn fs_copy_dir(_: &Lua, (src, dest): (String, String)) -> mlua::Result<bool> {
    copy_dir_recursive(Path::new(&src), Path::new(&dest))
        .map(|_| true)
        .map_err(|e| mlua::Error::runtime(format!("Failed to copy directory '{}' to '{}': {}", src, dest, e)))
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

fn fs_stat(lua: &Lua, path: String) -> mlua::Result<Table> {
    let metadata = fs::metadata(&path)
        .map_err(|e| mlua::Error::runtime(format!("Failed to get metadata for '{}': {}", path, e)))?;

    let result = lua.create_table()?;

    result.set("size", metadata.len())?;
    result.set("is_file", metadata.is_file())?;
    result.set("is_dir", metadata.is_dir())?;
    result.set("readonly", metadata.permissions().readonly())?;

    // Symlink check uses symlink_metadata
    let is_symlink = fs::symlink_metadata(&path)
        .map(|m| m.is_symlink())
        .unwrap_or(false);
    result.set("is_symlink", is_symlink)?;

    // Modified time (Unix timestamp)
    if let Ok(modified) = metadata.modified() {
        if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
            result.set("modified", duration.as_secs())?;
        }
    }

    // Created time (Unix timestamp)
    if let Ok(created) = metadata.created() {
        if let Ok(duration) = created.duration_since(std::time::UNIX_EPOCH) {
            result.set("created", duration.as_secs())?;
        }
    }

    // Accessed time (Unix timestamp)
    if let Ok(accessed) = metadata.accessed() {
        if let Ok(duration) = accessed.duration_since(std::time::UNIX_EPOCH) {
            result.set("accessed", duration.as_secs())?;
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Path utilities
// ---------------------------------------------------------------------------

fn fs_abs(_: &Lua, path: String) -> mlua::Result<String> {
    let abs = fs::canonicalize(&path)
        .map_err(|e| mlua::Error::runtime(format!("Failed to resolve path '{}': {}", path, e)))?;
    // On Windows, canonicalize returns \\?\ prefix — strip it for usability
    let s = abs.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s).to_string();
    Ok(s)
}

fn fs_join(_: &Lua, parts: MultiValue) -> mlua::Result<String> {
    let mut path = std::path::PathBuf::new();
    for part in parts {
        match part {
            Value::String(s) => {
                let borrowed = s.to_str()?;
                path.push(borrowed.as_ref() as &str);
            }
            _ => {
                return Err(mlua::Error::runtime("fs.join: all arguments must be strings"));
            }
        }
    }
    Ok(path.to_string_lossy().to_string())
}

fn fs_basename(_: &Lua, path: String) -> mlua::Result<String> {
    let p = Path::new(&path);
    Ok(p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default())
}

fn fs_dirname(_: &Lua, path: String) -> mlua::Result<String> {
    let p = Path::new(&path);
    Ok(p.parent()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default())
}

fn fs_ext(_: &Lua, path: String) -> mlua::Result<String> {
    let p = Path::new(&path);
    Ok(p.extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

fn fs_glob(lua: &Lua, pattern: String) -> mlua::Result<Table> {
    let entries = glob::glob(&pattern)
        .map_err(|e| mlua::Error::runtime(format!("Invalid glob pattern '{}': {}", pattern, e)))?;

    let result = lua.create_table()?;
    let mut index = 1;

    for entry in entries {
        if let Ok(path) = entry {
            result.set(index, path.to_string_lossy().to_string())?;
            index += 1;
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

fn fs_cwd(_: &Lua, _: ()) -> mlua::Result<String> {
    let cwd = std::env::current_dir()
        .map_err(|e| mlua::Error::runtime(format!("Failed to get current directory: {}", e)))?;
    Ok(cwd.to_string_lossy().to_string())
}

fn fs_temp_dir(_: &Lua, _: ()) -> mlua::Result<String> {
    Ok(std::env::temp_dir().to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract bytes from a Lua string or Buffer value.
fn extract_bytes(value: Value) -> mlua::Result<Vec<u8>> {
    match &value {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::UserData(ud) => {
            let buf = ud.borrow::<Buffer>()?;
            buf.get_data()
        }
        _ => Err(mlua::Error::runtime(
            "Expected string or Buffer",
        )),
    }
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path)?;
        }
    }

    Ok(())
}
