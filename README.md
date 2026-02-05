# CopperMoon

> **A high-performance Lua runtime written in Rust.**

CopperMoon is a complete Lua execution environment, similar to what Node.js is for JavaScript. It provides a modern, batteries-included runtime with async I/O, a package manager, a project toolchain, a web framework, and a growing ecosystem of packages.

**Write Lua. Run at the speed of Rust.**

🌐 **Website:** [coppermoon.dev](https://coppermoon.dev)

---

## Ecosystem

| Component | Role |
|-----------|------|
| **CopperMoon** | Lua runtime (execution engine, async I/O, Lua VM) |
| **[Harbor](https://github.com/coppermoondev/harbor)** | Package manager (dependency management) |
| **[Shipyard](https://github.com/coppermoondev/shipyard)** | Toolchain (CLI, project scaffolding, build, dev server) |
| **[HoneyMoon](https://github.com/coppermoondev/honeymoon)** | Web framework (routing, middleware, plugins) |

---

## Architecture

```
┌─────────────────────────────────────────┐
│              CopperMoon                 │  ← Runtime (Rust)
│  ┌───────────────────────────────────┐  │
│  │  Async I/O (Tokio) + Lua VM       │  │
│  └───────────────────────────────────┘  │
│                   │                      │
│  ┌────────────────┼────────────────┐    │
│  │                │                │    │
│  ▼                ▼                ▼    │
│ Harbor        Shipyard        HoneyMoon │
│ (packages)    (toolchain)    (web framework)
└─────────────────────────────────────────┘
```

---

## Quick Start

### Install

Download the latest release from [coppermoon.dev](https://coppermoon.dev), or build from source:

```bash
git clone https://github.com/coppermoondev/coppermoon.git
cd coppermoon
cargo build --release
```

### Hello World

```lua
-- hello.lua
print("Hello from CopperMoon!")
print("Version:", _COPPERMOON_VERSION)
```

```bash
coppermoon hello.lua
```

### Create a Web Project

```bash
shipyard new my-app --template web
cd my-app
shipyard dev
# Open http://localhost:3000
```

---

## CopperMoon Runtime

The core of the ecosystem. CopperMoon embeds Lua 5.4 via `mlua` and exposes high-performance native APIs.

### Features

- High-performance Lua execution
- Asynchronous I/O (networking, files, HTTP)
- Transparent async bridge (Lua code stays synchronous)
- Native APIs exposed to Lua (crypto, fs, http, json, etc.)

### Example

```lua
local fs = require("fs")
local http = require("http")

-- File I/O
local content = fs.read("config.json")
local config = json.decode(content)

-- HTTP request
local response = http.get("https://api.example.com/data")
print(response.body)
```

### Standard Library

| Module | Description |
|--------|-------------|
| `fs` | File system operations (read, write, copy, stat, etc.) |
| `path` | Path manipulation (join, dirname, basename, resolve) |
| `os_ext` | Extended OS functions (env, cwd, platform, homedir) |
| `process` | Process management (spawn, exec, exit, pid) |
| `json` | JSON encoding and decoding |
| `crypto` | SHA-256, MD5, HMAC, UUID, Base64, random bytes |
| `time` | Timers, sleep, monotonic clock |
| `http` | HTTP client (GET, POST, PUT, DELETE) |
| `http.server` | HTTP server |
| `net` | TCP and UDP sockets |
| `net.ws` | WebSocket client |
| `buffer` | Binary data manipulation |
| `console` | Interactive input |
| `term` | Terminal styling and colors |
| `archive` | ZIP, TAR, GZIP |

### Native Database Bindings

| Module | Description |
|--------|-------------|
| `sqlite` | SQLite (file and in-memory) |
| `mysql` | MySQL / MariaDB |

---

## Harbor (Package Manager)

Fast, simple package management for the CopperMoon ecosystem.

```bash
harbor init                # Initialize a project
harbor install <package>   # Install a package
harbor install             # Install all dependencies
harbor uninstall <pkg>     # Remove a package
harbor update              # Update dependencies
harbor list                # List installed packages
harbor search <query>      # Search the registry
harbor publish             # Publish a package
```

### Manifest (`harbor.toml`)

```toml
[package]
name = "my-app"
version = "1.0.0"
main = "init.lua"

[dependencies]
honeymoon = "0.2.0"
freight = "0.1.0"

[dev-dependencies]
assay = "0.1.0"
```

---

## Shipyard (Toolchain)

CLI tools for creating, developing, and building CopperMoon applications.

```bash
shipyard new my-app              # Create a new project
shipyard new my-web --template web  # Web project with HoneyMoon
shipyard dev                     # Development server
shipyard run                     # Run in production mode
shipyard build                   # Build for deployment
```

---

## HoneyMoon (Web Framework)

A production-ready web framework inspired by Express.js.

```lua
local honeymoon = require("honeymoon")
local app = honeymoon.new()

-- Middleware
app:use(honeymoon.logger())
app:use(honeymoon.cors())
app:use(honeymoon.json())

-- Routes
app:get("/", function(req, res)
    res:html("<h1>Hello, World!</h1>")
end)

app:get("/users/:id", function(req, res)
    res:json({ id = req.params.id, name = "Alice" })
end)

-- Start
app:listen(3000)
```

### Built-in Middleware

Logger, CORS, body parsers (JSON, URL-encoded), static files, rate limiting, authentication (Basic, Bearer, API Key, JWT), sessions, Helmet security headers, CSRF protection, request IDs, response time, and more.

---

## Packages

| Package | Description |
|---------|-------------|
| [HoneyMoon](https://github.com/coppermoondev/honeymoon) | Web framework (Express.js-like) |
| [Freight](https://github.com/coppermoondev/freight) | ORM inspired by GORM |
| [Vein](https://github.com/coppermoondev/vein) | Templating engine |
| [Ember](https://github.com/coppermoondev/ember) | Structured logging |
| [Lantern](https://github.com/coppermoondev/lantern) | Debug toolbar for HoneyMoon |
| [Assay](https://github.com/coppermoondev/assay) | Unit testing framework (Jest-like) |
| [Dotenv](https://github.com/coppermoondev/dotenv) | .env file loader |
| [Redis](https://github.com/coppermoondev/redis) | Redis client |
| [MQTT](https://github.com/coppermoondev/mqtt) | MQTT client |
| [S3](https://github.com/coppermoondev/s3) | S3-compatible storage client |
| [Tailwind](https://github.com/coppermoondev/tailwind) | TailwindCSS integration |

---

## Roadmap

### ✅ Phase 1 — Core Runtime
- [x] Rust project structure (Cargo workspace)
- [x] Lua 5.4 integration via `mlua`
- [x] Script execution and REPL
- [x] Custom `require()` with module resolution
- [x] CLI (`coppermoon run`, `coppermoon repl`, `--version`)

### ✅ Phase 2 — Standard Library
- [x] `fs` — File system operations
- [x] `path` — Path manipulation
- [x] `os_ext` — Environment, CWD, platform detection
- [x] `process` — Spawn, exec, exit
- [x] `json` — Encode, decode, pretty-print
- [x] `crypto` — SHA, MD5, HMAC, UUID, Base64

### ✅ Phase 3 — Async Runtime
- [x] Tokio integration
- [x] Event loop
- [x] Transparent async bridge (Rust futures → blocking Lua)
- [x] Timers (sleep, setTimeout, setInterval)

### ✅ Phase 4 — Networking
- [x] HTTP client (GET, POST, PUT, DELETE, custom)
- [x] TCP/UDP sockets
- [x] HTTP server
- [x] WebSocket client

### ✅ Phase 5 — Shipyard (Toolchain)
- [x] `shipyard new` with templates (minimal, web, api)
- [x] `shipyard dev` / `shipyard run` / `shipyard build`
- [x] `Shipyard.toml` configuration

### ✅ Phase 6 — Harbor (Package Manager)
- [x] `harbor init`, `install`, `uninstall`, `update`
- [x] Registry, semver, lockfile
- [x] Local path dependencies
- [x] Publishing (`harbor publish`, `harbor login`)

### ✅ Phase 7 — HoneyMoon (Web Framework)
- [x] Express-style routing with parameters
- [x] Middleware chain (onion model)
- [x] Built-in middleware (logger, CORS, auth, sessions, etc.)
- [x] Schema validation
- [x] Error handling with HTML/JSON error pages
- [x] Vein template integration

### 🔨 Phase 8 — Native Services
- [x] SQLite bindings
- [x] MySQL bindings
- [ ] PostgreSQL bindings
- [x] Redis client
- [x] MQTT client
- [x] S3 client

### 📋 Phase 9 — Developer Experience
- [ ] Improved error messages with source maps
- [ ] Hot reload
- [ ] IDE support (LSP)

### 📋 Phase 10 — Performance & Production
- [ ] Benchmarks (vs Node.js, Deno, Bun)
- [ ] Worker threads
- [ ] Graceful shutdown
- [ ] Health checks / Metrics

### 📋 Phase 11 — Documentation & Ecosystem
- [ ] Documentation website ([coppermoon.dev](https://coppermoon.dev))
- [ ] Tutorials and guides
- [ ] Package registry

### 📋 Phase 12 — Stabilization & v1.0
- [ ] API freeze and review
- [ ] Security audit
- [ ] Binaries for all platforms
- [ ] Public release

---

## Project Structure

```
coppermoon/
├── crates/
│   ├── coppermoon/        # Main CLI binary
│   ├── coppermoon_core/   # Core runtime engine
│   ├── coppermoon_std/    # Standard library modules
│   ├── harbor/            # Package manager
│   ├── shipyard/          # Project toolchain
│   ├── sqlite/            # SQLite bindings
│   └── mysql/             # MySQL bindings
├── packages/
│   ├── honeymoon/         # Web framework
│   ├── freight/           # ORM
│   ├── vein/              # Templating engine
│   ├── ember/             # Logging
│   ├── lantern/           # Debug toolbar
│   ├── assay/             # Testing framework
│   ├── dotenv/            # .env file loader
│   ├── redis/             # Redis client
│   ├── mqtt/              # MQTT client
│   ├── s3/                # S3 storage client
│   └── tailwind/          # TailwindCSS integration
└── README.md
```

---

## Vision

CopperMoon aims to be for Lua what Node.js is for JavaScript:

- A modern, high-performance runtime
- A complete ecosystem (packages, tooling, framework)
- A simple, intuitive developer experience

**Write Lua. Run at the speed of Rust.**

---

## License

MIT License
