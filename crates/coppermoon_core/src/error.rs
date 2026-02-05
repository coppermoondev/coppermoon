//! Error types for CopperMoon

use thiserror::Error;

/// Main error type for CopperMoon operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("Lua error: {0}")]
    Lua(#[from] mlua::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Module not found: {0}")]
    ModuleNotFound(String),

    #[error("Runtime error: {0}")]
    Runtime(String),

    #[error("Script error in {file}:{line}: {message}")]
    Script {
        file: String,
        line: u32,
        message: String,
    },
}

/// Result type alias for CopperMoon operations
pub type Result<T> = std::result::Result<T, Error>;
