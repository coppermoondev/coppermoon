# CopperMoon Core

> **The core runtime engine for CopperMoon**

This crate provides the foundational runtime that powers CopperMoon: the Lua VM integration via `mlua`, the module resolution system, the async bridge between Rust and Lua, and error handling.

## Architecture

```
coppermoon_core
├── Runtime        # Main runtime — creates Lua state, executes files
├── Module         # Custom require() with path resolution and caching
├── AsyncRuntime   # Tokio integration — block_on, spawn, get_runtime
└── Error          # Unified error types (Lua errors, IO errors, etc.)
```

## Components

### Runtime

The `Runtime` struct is the central object. It creates a configured Lua state, sets up the module loader, and executes Lua files:

```rust
use coppermoon_core::Runtime;

// Create runtime with a base path for module resolution
let runtime = Runtime::with_base_path("./my-project")?;

// Setup the custom module loader (require)
runtime.setup_module_loader()?;

// Access the Lua state for registering modules
let lua = runtime.lua();

// Execute a Lua file
runtime.exec_file("app.lua")?;
```

### Module System

The module system provides a custom `require()` implementation that:

- Resolves relative and absolute paths
- Searches `harbor_modules/` for installed packages
- Supports `init.lua` resolution for directories
- Caches loaded modules to avoid re-execution
- Handles the `package.path` and `package.cpath` configuration

### Async Bridge

CopperMoon uses Tokio under the hood for async operations, but Lua code remains synchronous. The async bridge transparently converts Rust futures into blocking Lua calls:

```rust
use coppermoon_core::{block_on, spawn, get_runtime};

// Block the current thread until a future completes
let result = block_on(async {
    // async work
    Ok(42)
})?;

// Spawn a background task
spawn(async {
    // runs concurrently
});

// Access the Tokio runtime
let rt = get_runtime();
```

### Error Handling

Unified error types that bridge Lua and Rust error domains:

```rust
use coppermoon_core::{Error, Result};

// Error variants: Lua, Io, Module, Runtime, Other
fn do_work() -> Result<()> {
    // ...
    Ok(())
}
```

## Dependencies

- `mlua` — Lua 5.4 bindings for Rust
- `tokio` — Async runtime
- `serde` / `serde_json` — Serialization
- `libloading` — Dynamic library loading
- `thiserror` / `anyhow` — Error handling
- `tracing` — Logging

## Documentation

For full documentation, visit [coppermoon.dev](https://coppermoon.dev).

## License

MIT License
