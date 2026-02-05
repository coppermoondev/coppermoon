//! CopperMoon Core - The Lua runtime engine
//!
//! This crate provides the core functionality for running Lua code,
//! including the Lua VM integration, module system, and async bridge.

pub mod error;
pub mod runtime;
pub mod module;
pub mod async_runtime;

pub use error::{Error, Result};
pub use runtime::Runtime;
pub use async_runtime::{block_on, spawn, get_runtime};
