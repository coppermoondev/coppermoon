# CopperMoon

> **A high-performance Lua runtime written in Rust**

This crate is the main CLI binary for CopperMoon. It provides the `coppermoon` command-line tool to execute Lua scripts, start an interactive REPL, and manage your CopperMoon applications.

CopperMoon embeds Lua 5.4 via `mlua`, provides a comprehensive standard library, async I/O powered by Tokio, and native database bindings — all accessible from Lua with zero external dependencies needed by the user.

## Installation

Download the latest release from [coppermoon.dev](https://coppermoon.dev), or build from source:

```bash
cargo build --release
```

## Usage

### Run a Lua file

```bash
coppermoon app.lua
coppermoon run app.lua

# With arguments
coppermoon app.lua --port 8080
```

### Interactive REPL

```bash
coppermoon repl
# Or just:
coppermoon
```

### Version

```bash
coppermoon version
coppermoon --version
```

## Script Arguments

Arguments passed after the script name are available in the `arg` global table:

```lua
-- coppermoon app.lua hello world
print(arg[0])  -- "app.lua"
print(arg[1])  -- "hello"
print(arg[2])  -- "world"
```

## What's Included

The `coppermoon` binary bundles the entire runtime:

- **Core runtime** (`coppermoon_core`) — Lua VM, module system, async bridge
- **Standard library** (`coppermoon_std`) — fs, path, os, process, json, crypto, time, http, net, websocket, archive, buffer, terminal, and more
- **SQLite** (`coppermoon_sqlite`) — Native SQLite bindings
- **MySQL** (`coppermoon_mysql`) — Native MySQL/MariaDB bindings

## Project Structure

```
crates/coppermoon/
├── src/
│   ├── main.rs   # Entry point, file execution
│   ├── cli.rs    # Command-line argument parsing (clap)
│   └── repl.rs   # Interactive REPL
└── Cargo.toml
```

## Related Crates

| Crate | Description |
|-------|-------------|
| [`coppermoon_core`](../coppermoon_core/README.md) | Core runtime engine |
| [`coppermoon_std`](../coppermoon_std/README.md) | Standard library modules |
| [`coppermoon_sqlite`](../sqlite/README.md) | SQLite bindings |
| [`coppermoon_mysql`](../mysql/README.md) | MySQL bindings |

## Related Tools

| Tool | Description |
|------|-------------|
| [Harbor](../harbor/README.md) | Package manager |
| [Shipyard](../shipyard/README.md) | Project toolchain |

## Documentation

For full documentation, visit [coppermoon.dev](https://coppermoon.dev).

## License

MIT License
