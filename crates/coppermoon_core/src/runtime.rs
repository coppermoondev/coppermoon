//! Lua runtime management

use crate::{Error, Result};
use mlua::{Lua, MultiValue, Value, StdLib};
use std::path::{Path, PathBuf};
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
        self.lua.load(code).exec()?;
        Ok(())
    }

    /// Execute a Lua script and return its result as a string (for REPL)
    pub fn eval(&self, code: &str) -> Result<String> {
        let chunk = self.lua.load(code);
        let result: MultiValue = chunk.eval()?;

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

        let chunk = self.lua
            .load(&code)
            .set_name(absolute_path.to_string_lossy());

        chunk.exec()?;

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
}
