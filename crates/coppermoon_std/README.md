# CopperMoon Standard Library

> **Standard library modules for the CopperMoon Lua runtime**

This crate provides all the built-in modules that are available globally in CopperMoon. These modules give Lua scripts access to the file system, networking, cryptography, HTTP, WebSocket, process management, and more — all backed by high-performance Rust implementations.

## Modules

All modules are registered globally when the runtime starts. No `require()` is needed.

### `fs` — File System

```lua
fs.read(path)              -- Read file contents
fs.write(path, content)    -- Write to file
fs.append(path, content)   -- Append to file
fs.exists(path)            -- Check existence
fs.remove(path)            -- Delete file
fs.mkdir(path)             -- Create directory
fs.mkdir_all(path)         -- Create directory tree
fs.rmdir(path)             -- Remove directory
fs.readdir(path)           -- List directory contents
fs.stat(path)              -- File metadata (size, modified, etc.)
fs.copy(src, dest)         -- Copy file
fs.rename(src, dest)       -- Rename / move file
```

### `path` — Path Manipulation

```lua
path.join(...)             -- Join path segments
path.dirname(path)         -- Parent directory
path.basename(path)        -- File name
path.extname(path)         -- File extension
path.resolve(path)         -- Absolute path
path.normalize(path)       -- Normalize separators
```

### `os_ext` — Extended OS Functions

```lua
os_ext.env(key)            -- Get environment variable
os_ext.setenv(key, value)  -- Set environment variable
os_ext.cwd()               -- Current working directory
os_ext.chdir(path)         -- Change directory
os_ext.platform()          -- "windows", "linux", or "macos"
os_ext.arch()              -- "x64", "arm64", etc.
os_ext.homedir()           -- Home directory
os_ext.tmpdir()            -- Temp directory
```

### `process` — Process Management

```lua
process.exit(code)         -- Exit with code
process.pid()              -- Current process ID
process.spawn(cmd, args)   -- Spawn subprocess
process.exec(cmd)          -- Execute shell command
arg                        -- Command-line arguments (global)
```

### `json` — JSON Encoding/Decoding

```lua
json.encode(table)         -- Table → JSON string
json.decode(string)        -- JSON string → table
json.pretty(table)         -- Pretty-printed JSON
```

### `crypto` — Cryptography

```lua
crypto.sha256(data)        -- SHA-256 hash
crypto.sha1(data)          -- SHA-1 hash
crypto.md5(data)           -- MD5 hash
crypto.hmac(algo, key, data) -- HMAC
crypto.random_bytes(n)     -- Cryptographic random bytes
crypto.uuid()              -- UUID v4
crypto.base64_encode(data) -- Base64 encode
crypto.base64_decode(data) -- Base64 decode
```

### `time` — Timers and Time

```lua
time.sleep(ms)             -- Async sleep (ms)
time.now()                 -- Current time (seconds, high-res)
time.monotonic()           -- Monotonic clock (seconds)
time.monotonic_ms()        -- Monotonic clock (ms)
setTimeout(fn, ms)         -- Delayed execution
setInterval(fn, ms)        -- Repeated execution
clearTimeout(id)           -- Cancel timeout
clearInterval(id)          -- Cancel interval
```

### `http` — HTTP Client

```lua
http.get(url, options?)          -- GET request
http.post(url, body, options?)   -- POST request
http.put(url, body, options?)    -- PUT request
http.delete(url, options?)       -- DELETE request
http.request(options)            -- Custom request
```

### `http.server` — HTTP Server

```lua
local server = http.server.new()
server:get("/", handler)
server:post("/", handler)
server:listen(port)
```

### `net` — TCP/UDP Networking

```lua
net.tcp.connect(host, port)  -- TCP client
net.tcp.listen(port)         -- TCP server
net.udp.bind(port)           -- UDP socket
```

### `net.ws` — WebSocket

```lua
net.ws.connect(url)          -- WebSocket client
```

### `buffer` — Binary Data

Binary buffer manipulation for working with raw bytes.

### `console` — Interactive Input

```lua
console.input(prompt?)       -- Read line from stdin
console.password(prompt?)    -- Read password (hidden)
```

### `term` — Terminal Styling

```lua
term.red(text)               -- Colored output
term.bold(text)              -- Bold text
term.clear()                 -- Clear screen
```

### `archive` — ZIP/TAR/GZIP

```lua
archive.zip.create(path, files)
archive.zip.extract(path, dest)
archive.tar.create(path, files)
archive.tar.extract(path, dest)
archive.gzip.compress(data)
archive.gzip.decompress(data)
```

### String & Table Extensions

CopperMoon extends Lua's built-in `string` and `table` libraries with additional utility functions.

## Rust Integration

Register all modules in one call:

```rust
use coppermoon_std;

// Register all standard library modules
coppermoon_std::register_all(lua)?;
```

Or register individual modules:

```rust
let globals = lua.globals();
globals.set("fs", coppermoon_std::fs::register(lua)?)?;
globals.set("json", coppermoon_std::json::register(lua)?)?;
globals.set("crypto", coppermoon_std::crypto::register(lua)?)?;
```

## Dependencies

- `reqwest` — HTTP client
- `tungstenite` — WebSocket
- `sha2`, `sha1`, `md5`, `hmac` — Cryptographic primitives
- `uuid` — UUID generation
- `base64` — Base64 encoding
- `chrono` — Date/time
- `crossterm` — Terminal control
- `zip`, `tar`, `flate2` — Archive formats
- `glob` — File globbing
- `dirs` — System directories
- `rand` — Random number generation

## Related

- [CopperMoon](https://github.com/coppermoondev/coppermoon) — Main repository and runtime
- [Harbor](https://github.com/coppermoondev/harbor) — Package manager
- [Shipyard](https://github.com/coppermoondev/shipyard) — Project toolchain

## Documentation

For full documentation, visit [coppermoon.dev](https://coppermoon.dev).

## License

MIT License
