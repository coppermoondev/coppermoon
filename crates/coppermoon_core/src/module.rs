//! Custom module loader for CopperMoon

use crate::Result;
use mlua::{Lua, Function, Value, Table};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::debug;

/// Stores native library handles to keep them alive for the Lua state's lifetime.
/// When a native module is loaded via `libloading`, the `Library` handle must remain
/// alive for as long as the Lua functions referencing its code exist.
pub struct NativeLibStore {
    libs: Mutex<Vec<libloading::Library>>,
}

impl NativeLibStore {
    pub fn new() -> Self {
        Self {
            libs: Mutex::new(Vec::new()),
        }
    }
}

/// Pre-load lua54.dll on Windows so native modules can resolve Lua symbols.
/// Modules compiled with mlua's "module" feature use raw_dylib linking that
/// expects lua54.dll to be available at runtime.
#[cfg(windows)]
fn preload_lua_shared_lib(store: &NativeLibStore) {
    // Search for lua54.dll in these locations (in order):
    // 1. Next to the coppermoon executable
    // 2. Current working directory
    let search_paths: Vec<PathBuf> = vec![
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("lua54.dll"))),
        Some(PathBuf::from("lua54.dll")),
    ]
    .into_iter()
    .flatten()
    .collect();

    for path in &search_paths {
        if path.exists() {
            match unsafe { libloading::Library::new(path) } {
                Ok(lib) => {
                    debug!("Pre-loaded lua54.dll from {}", path.display());
                    store.libs.lock().unwrap().push(lib);
                    return;
                }
                Err(e) => {
                    debug!("Failed to pre-load lua54.dll from {}: {}", path.display(), e);
                }
            }
        }
    }

    debug!("lua54.dll not found — native modules may fail to load");
}

/// Setup the custom module loader
pub fn setup_loader(lua: &Lua, base_path: &Path) -> Result<()> {
    // Pre-load lua54.dll on Windows for native module support
    #[cfg(windows)]
    if let Some(store) = lua.app_data_ref::<NativeLibStore>() {
        preload_lua_shared_lib(&store);
    }

    let base_path_owned = base_path.to_path_buf();
    let base_path_for_lua = base_path_owned.clone();
    let base_path_for_native = base_path_owned.clone();

    // Create our custom Lua file searcher
    let searcher = lua.create_function(move |lua, module_name: String| {
        let path = resolve_module_path(&base_path_for_lua, &module_name);

        debug!("Searching for module '{}' at {:?}", module_name, path);

        if let Some(path) = path {
            if path.exists() {
                let code = std::fs::read_to_string(&path)
                    .map_err(|e| mlua::Error::runtime(format!("Failed to read module: {}", e)))?;

                let chunk = lua.load(&code)
                    .set_name(path.to_string_lossy());

                let loader: Function = chunk.into_function()?;
                let path_str = path.to_string_lossy().to_string();

                Ok((Value::Function(loader), Value::String(lua.create_string(&path_str)?)))
            } else {
                let err_msg = format!("\n\tno file '{}'", path.display());
                Ok((Value::Nil, Value::String(lua.create_string(&err_msg)?)))
            }
        } else {
            let err_msg = format!("\n\tno module '{}'", module_name);
            Ok((Value::Nil, Value::String(lua.create_string(&err_msg)?)))
        }
    })?;

    // Create native module searcher
    let native_searcher = lua.create_function(move |lua, module_name: String| {
        let native_path = resolve_native_path(&base_path_for_native, &module_name);

        debug!("Searching for native module '{}' at {:?}", module_name, native_path);

        if let Some(ref path) = native_path {
            if path.exists() {
                // Build the entry point symbol name: luaopen_<name_with_underscores>
                let symbol_name = format!(
                    "luaopen_{}",
                    module_name.replace('.', "_").replace('-', "_")
                );
                let symbol_name_null = format!("{}\0", symbol_name);

                debug!("Loading native module '{}' from {:?}, symbol: {}", module_name, path, symbol_name);

                unsafe {
                    let lib = libloading::Library::new(path)
                        .map_err(|e| mlua::Error::runtime(
                            format!("Failed to load native module '{}': {}", module_name, e)
                        ))?;

                    let func: libloading::Symbol<unsafe extern "C-unwind" fn(*mut mlua::ffi::lua_State) -> std::ffi::c_int>
                        = lib.get(symbol_name_null.as_bytes())
                        .map_err(|e| mlua::Error::runtime(
                            format!("Symbol '{}' not found in '{}': {}", symbol_name, path.display(), e)
                        ))?;

                    let func_ptr = *func;

                    // Store library handle to keep it alive for the Lua state's lifetime
                    let store = lua.app_data_ref::<NativeLibStore>()
                        .ok_or_else(|| mlua::Error::runtime("NativeLibStore not initialized"))?;
                    store.libs.lock().unwrap().push(lib);

                    // Wrap the C function as a Lua function
                    let loader = lua.create_c_function(func_ptr)?;
                    let path_str = path.to_string_lossy().to_string();

                    Ok((Value::Function(loader), Value::String(lua.create_string(&path_str)?)))
                }
            } else {
                let err_msg = format!("\n\tno native module '{}'", module_name);
                Ok((Value::Nil, Value::String(lua.create_string(&err_msg)?)))
            }
        } else {
            let err_msg = format!("\n\tno native module '{}'", module_name);
            Ok((Value::Nil, Value::String(lua.create_string(&err_msg)?)))
        }
    })?;

    // Get package.searchers table
    let package: Table = lua.globals().get("package")?;
    let searchers: Table = package.get("searchers")?;

    // Insert our Lua searcher at position 2 (after the preload searcher)
    searchers.set(2, searcher)?;

    // Insert native searcher at position 3 (after Lua searcher, so .lua files take precedence)
    searchers.set(3, native_searcher)?;

    // Set package.path to include our paths
    let lua_path = format!(
        "{0}/?.lua;{0}/?/init.lua;{0}/harbor_modules/?.lua;{0}/harbor_modules/?/init.lua",
        base_path_owned.display()
    );
    package.set("path", lua_path)?;

    Ok(())
}

/// Resolve a module name to a Lua file path
fn resolve_module_path(base_path: &Path, module_name: &str) -> Option<PathBuf> {
    // Convert module name to path (e.g., "foo.bar" -> "foo/bar")
    let module_path = module_name.replace('.', "/");

    // Try different patterns
    let patterns = [
        format!("{}.lua", module_path),
        format!("{}/init.lua", module_path),
        format!("harbor_modules/{}.lua", module_path),
        format!("harbor_modules/{}/init.lua", module_path),
    ];

    for pattern in patterns {
        let path = base_path.join(&pattern);
        if path.exists() {
            return Some(path);
        }
    }

    // Return the first pattern for error reporting
    Some(base_path.join(format!("{}.lua", module_path)))
}

/// Resolve a module name to a native library path
fn resolve_native_path(base_path: &Path, module_name: &str) -> Option<PathBuf> {
    let module_path = module_name.replace('.', "/");

    // Platform-specific library naming
    let (prefix, ext) = if cfg!(windows) {
        ("", "dll")
    } else if cfg!(target_os = "macos") {
        ("lib", "dylib")
    } else {
        ("lib", "so")
    };

    // The leaf name (last segment after dots), with hyphens replaced by underscores
    let leaf = module_name
        .rsplit('.')
        .next()
        .unwrap_or(module_name)
        .replace('-', "_");
    let lib_filename = format!("{}{}.{}", prefix, leaf, ext);

    // Search patterns:
    // 1. harbor_modules/<path>/native/<lib>  (installed packages where dir matches module name)
    // 2. <path>/native/<lib>                 (local native modules)
    let patterns = [
        format!("harbor_modules/{}/native/{}", module_path, lib_filename),
        format!("{}/native/{}", module_path, lib_filename),
    ];

    for pattern in &patterns {
        let path = base_path.join(pattern);
        if path.exists() {
            return Some(path);
        }
    }

    // 3. Scan all harbor_modules/*/native/ for the library file.
    //    This handles the case where a Lua package wraps a native module with a
    //    different name (e.g. package "redis" contains native lib "copper_redis").
    let harbor_dir = base_path.join("harbor_modules");
    if let Ok(entries) = std::fs::read_dir(&harbor_dir) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("native").join(&lib_filename);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_resolve_module_path() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        // Create a test module
        fs::write(base.join("mymodule.lua"), "return 42").unwrap();

        let path = resolve_module_path(base, "mymodule");
        assert!(path.is_some());
        assert!(path.unwrap().exists());
    }

    #[test]
    fn test_resolve_nested_module() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        // Create nested module
        fs::create_dir_all(base.join("foo")).unwrap();
        fs::write(base.join("foo/bar.lua"), "return 'nested'").unwrap();

        let path = resolve_module_path(base, "foo.bar");
        assert!(path.is_some());
        assert!(path.unwrap().exists());
    }

    #[test]
    fn test_resolve_native_path_not_found() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        // No native library exists
        let path = resolve_native_path(base, "mymodule");
        assert!(path.is_none());
    }

    #[test]
    fn test_resolve_native_path_found() {
        let dir = tempdir().unwrap();
        let base = dir.path();

        // Create a fake native library in harbor_modules
        let native_dir = base.join("harbor_modules/mymodule/native");
        fs::create_dir_all(&native_dir).unwrap();

        let lib_name = if cfg!(windows) {
            "mymodule.dll"
        } else if cfg!(target_os = "macos") {
            "libmymodule.dylib"
        } else {
            "libmymodule.so"
        };
        fs::write(native_dir.join(lib_name), "fake library").unwrap();

        let path = resolve_native_path(base, "mymodule");
        assert!(path.is_some());
        assert!(path.unwrap().exists());
    }
}
