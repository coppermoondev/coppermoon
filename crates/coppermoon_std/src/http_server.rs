//! HTTP Server module for CopperMoon
//!
//! Provides an HTTP server with concurrent connection handling.
//! Connections are accepted and socket I/O is performed asynchronously on
//! Tokio worker threads. Each request is dispatched to its Lua handler as a
//! task on the event-loop `LocalSet` (Node.js-style): handlers run
//! interleaved on the main thread, so while one handler awaits an async
//! operation (http client, time.sleep, DB…), other requests keep being
//! served.

use coppermoon_core::Result;
use mlua::{Lua, Table, Function, Value, RegistryKey};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

// ---------------------------------------------------------------------------
// Security limits
// ---------------------------------------------------------------------------

const MAX_REQUEST_LINE: usize = 8 * 1024;       // 8 KB
const MAX_HEADER_LINE: usize = 8 * 1024;        // 8 KB per header
const MAX_HEADER_COUNT: usize = 100;             // max number of headers
const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;  // 10 MB
const CONNECTION_TIMEOUT_SECS: u64 = 30;         // 30s idle / per-request timeout
const MAX_REQUESTS_PER_CONNECTION: usize = 1000; // keep-alive requests per connection
const SHUTDOWN_GRACE_SECS: u64 = 10;             // drain deadline on graceful shutdown
const MAX_JSON_DEPTH: usize = 128;               // nesting guard (cyclic tables)

/// Backpressure limits, overridable via environment variables.
const DEFAULT_MAX_IN_FLIGHT: usize = 1024;       // concurrent Lua handlers
const DEFAULT_MAX_CONNECTIONS: usize = 10_240;   // concurrent TCP connections

fn env_limit(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn max_in_flight() -> usize {
    static V: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *V.get_or_init(|| env_limit("COPPERMOON_MAX_IN_FLIGHT", DEFAULT_MAX_IN_FLIGHT))
}

fn max_connections() -> usize {
    static V: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *V.get_or_init(|| env_limit("COPPERMOON_MAX_CONNECTIONS", DEFAULT_MAX_CONNECTIONS))
}

// ---------------------------------------------------------------------------
// Server-wide statistics (shared by every listener in the process)
// ---------------------------------------------------------------------------

mod stats {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::time::Instant;

    pub static REQUESTS_TOTAL: AtomicU64 = AtomicU64::new(0);
    pub static REJECTED_TOTAL: AtomicU64 = AtomicU64::new(0);
    pub static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
    /// Responses by status class: [1xx, 2xx, 3xx, 4xx, 5xx].
    pub static STATUS_CLASSES: [AtomicU64; 5] = [
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
    ];

    static STARTED: OnceLock<Instant> = OnceLock::new();

    pub fn mark_started() {
        let _ = STARTED.set(Instant::now());
    }

    pub fn uptime_seconds() -> u64 {
        STARTED.get().map(|s| s.elapsed().as_secs()).unwrap_or(0)
    }

    pub fn record_response(status: u16) {
        let class = usize::from((status / 100).clamp(1, 5)) - 1;
        STATUS_CLASSES[class].fetch_add(1, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Plain-data types that cross the channel boundary (no Lua objects)
// ---------------------------------------------------------------------------

struct ParsedRequest {
    method: String,
    path: String,
    query_string: Option<String>,
    headers: HashMap<String, String>,
    body: String,
    /// Whether the client allows connection reuse (HTTP/1.1 semantics).
    keep_alive: bool,
}

struct HttpResponse {
    status: u16,
    content_type: String,
    body: Vec<u8>,
    headers: Vec<(String, String)>,
}

/// Message sent from a connection task to the main Lua thread.
type RequestMessage = (ParsedRequest, tokio::sync::oneshot::Sender<HttpResponse>);

// ---------------------------------------------------------------------------
// Module registration (unchanged API surface)
// ---------------------------------------------------------------------------

/// Register the http.server module
pub fn register(lua: &Lua) -> Result<Table> {
    let server_table = lua.create_table()?;
    server_table.set("new", lua.create_function(server_new)?)?;

    // http.server.stats() -> counters for monitoring / health endpoints.
    server_table.set("stats", lua.create_function(|lua, ()| {
        use std::sync::atomic::Ordering;
        let t = lua.create_table()?;
        t.set("requests_total", stats::REQUESTS_TOTAL.load(Ordering::Relaxed))?;
        t.set("rejected_total", stats::REJECTED_TOTAL.load(Ordering::Relaxed))?;
        t.set("in_flight", stats::IN_FLIGHT.load(Ordering::Relaxed))?;
        t.set("uptime_seconds", stats::uptime_seconds())?;
        for (i, name) in ["status_1xx", "status_2xx", "status_3xx", "status_4xx", "status_5xx"]
            .iter()
            .enumerate()
        {
            t.set(*name, stats::STATUS_CLASSES[i].load(Ordering::Relaxed))?;
        }
        Ok(t)
    })?)?;

    Ok(server_table)
}

fn server_new(lua: &Lua, _: ()) -> mlua::Result<Table> {
    let server = lua.create_table()?;

    let routes = lua.create_table()?;
    server.set("_routes", routes)?;
    server.set("_port", 3000u16)?;

    // Route registration helpers — identical API to before.
    server.set("get", lua.create_function(|_, (server, path, handler): (Table, String, Function)| {
        let routes: Table = server.get("_routes")?;
        routes.set(format!("GET:{}", path), handler)?;
        Ok(server)
    })?)?;

    server.set("post", lua.create_function(|_, (server, path, handler): (Table, String, Function)| {
        let routes: Table = server.get("_routes")?;
        routes.set(format!("POST:{}", path), handler)?;
        Ok(server)
    })?)?;

    server.set("put", lua.create_function(|_, (server, path, handler): (Table, String, Function)| {
        let routes: Table = server.get("_routes")?;
        routes.set(format!("PUT:{}", path), handler)?;
        Ok(server)
    })?)?;

    server.set("delete", lua.create_function(|_, (server, path, handler): (Table, String, Function)| {
        let routes: Table = server.get("_routes")?;
        routes.set(format!("DELETE:{}", path), handler)?;
        Ok(server)
    })?)?;

    server.set("all", lua.create_function(|_, (server, path, handler): (Table, String, Function)| {
        let routes: Table = server.get("_routes")?;
        routes.set(format!("ALL:{}", path), handler)?;
        Ok(server)
    })?)?;

    server.set("options", lua.create_function(|_, (server, path, handler): (Table, String, Function)| {
        let routes: Table = server.get("_routes")?;
        routes.set(format!("OPTIONS:{}", path), handler)?;
        Ok(server)
    })?)?;

    server.set("patch", lua.create_function(|_, (server, path, handler): (Table, String, Function)| {
        let routes: Table = server.get("_routes")?;
        routes.set(format!("PATCH:{}", path), handler)?;
        Ok(server)
    })?)?;

    server.set("head", lua.create_function(|_, (server, path, handler): (Table, String, Function)| {
        let routes: Table = server.get("_routes")?;
        routes.set(format!("HEAD:{}", path), handler)?;
        Ok(server)
    })?)?;

    server.set("listen", lua.create_async_function(server_listen)?)?;

    Ok(server)
}

// ---------------------------------------------------------------------------
// server:listen(port, host?, callback?)
// ---------------------------------------------------------------------------

async fn server_listen(lua: Lua, (server, port, rest): (Table, u16, mlua::MultiValue)) -> mlua::Result<()> {
    server.set("_port", port)?;

    // Flexible trailing arguments: an optional host string and an optional
    // callback function, in any order. Fallback: COPPERMOON_HOST env var,
    // then loopback (safe default; use "0.0.0.0" to expose, e.g. in Docker).
    let mut host: Option<String> = None;
    let mut callback: Option<Function> = None;
    for value in rest {
        match value {
            Value::String(s) => host = Some(s.to_str()?.to_string()),
            Value::Function(f) => callback = Some(f),
            Value::Nil => {}
            other => {
                return Err(mlua::Error::runtime(format!(
                    "listen: unexpected argument of type {}",
                    other.type_name()
                )));
            }
        }
    }
    let host = host
        .or_else(|| std::env::var("COPPERMOON_HOST").ok().filter(|h| !h.is_empty()))
        .unwrap_or_else(|| "127.0.0.1".to_string());

    let routes: Table = server.get("_routes")?;

    // Store route handlers in the Lua registry so they stay alive.
    let mut route_handlers: HashMap<String, RegistryKey> = HashMap::new();
    for pair in routes.pairs::<String, Function>() {
        let (key, handler) = pair?;
        let reg_key = lua.create_registry_value(handler)?;
        route_handlers.insert(key, reg_key);
    }
    // Shared with the per-request dispatch tasks.
    let route_handlers = Arc::new(route_handlers);

    let addr = format!("{}:{}", host, port);

    // Channel carrying parsed requests from Tokio connection tasks to the
    // Lua event loop.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<RequestMessage>();

    // Bind before spawning the accept loop so that a bind failure is
    // reported synchronously to the caller instead of a background eprintln.
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| mlua::Error::runtime(format!("Failed to bind to {}: {}", addr, e)))?;

    // Spawn the async accept loop on the Tokio runtime. It stops (and the
    // listener closes) as soon as a graceful shutdown is requested.
    // A semaphore caps concurrent connections: beyond the limit, clients
    // get an immediate 503 instead of an ever-growing task pile.
    let conn_limiter = Arc::new(tokio::sync::Semaphore::new(max_connections()));
    coppermoon_core::spawn(async move {
        loop {
            tokio::select! {
                _ = coppermoon_core::shutdown::requested() => break,
                accepted = listener.accept() => match accepted {
                    Ok((mut stream, _peer)) => {
                        match conn_limiter.clone().try_acquire_owned() {
                            Ok(permit) => {
                                let tx = tx.clone();
                                tokio::spawn(async move {
                                    let _permit = permit; // released when the connection ends
                                    handle_connection(stream, tx).await;
                                });
                            }
                            Err(_) => {
                                stats::REJECTED_TOTAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                stats::record_response(503);
                                tokio::spawn(async move {
                                    let resp = build_response_bytes(
                                        503,
                                        "text/plain",
                                        "Service Unavailable (connection limit reached)",
                                        &[],
                                    );
                                    let _ = stream.write_all(&resp).await;
                                });
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Accept error: {}", e);
                    }
                },
            }
        }
        // Listener dropped here: no new connections are accepted.
    });

    // Notify callback if provided
    if let Some(cb) = callback {
        cb.call_async::<()>(port).await?;
    }

    println!("CopperMoon server listening on http://{}", addr);

    // ---------- Request dispatch loop ----------
    // Each request is dispatched as its own task on the event-loop
    // `LocalSet`. Handlers therefore run concurrently (interleaved on the
    // main thread): while one awaits an async operation, others progress.
    // Timers are fired by the runtime's global timer pump — no special
    // handling needed here.
    stats::mark_started();
    loop {
        tokio::select! {
            _ = coppermoon_core::shutdown::requested() => break,
            msg = rx.recv() => match msg {
                Some((request, resp_tx)) => {
                    use std::sync::atomic::Ordering;
                    stats::REQUESTS_TOTAL.fetch_add(1, Ordering::Relaxed);

                    // Backpressure: past the in-flight cap, shed load with
                    // an immediate 503 instead of queueing without bound.
                    if stats::IN_FLIGHT.load(Ordering::SeqCst) >= max_in_flight() {
                        stats::REJECTED_TOTAL.fetch_add(1, Ordering::Relaxed);
                        stats::record_response(503);
                        let _ = resp_tx.send(HttpResponse {
                            status: 503,
                            content_type: "text/plain".into(),
                            body: b"Service Unavailable (overloaded)".to_vec(),
                            headers: vec![("Retry-After".to_string(), "1".to_string())],
                        });
                        continue;
                    }

                    let lua = lua.clone();
                    let handlers = route_handlers.clone();
                    stats::IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
                    tokio::task::spawn_local(async move {
                        let response = dispatch_to_lua(&lua, &request, &handlers).await;
                        stats::record_response(response.status);
                        // Ignore send error — the connection task may have dropped.
                        let _ = resp_tx.send(response);
                        stats::IN_FLIGHT.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    });
                }
                None => break,
            },
        }
    }

    // ---------- Graceful drain ----------
    // Wait for in-flight handlers to finish, up to a grace deadline.
    let pending = stats::IN_FLIGHT.load(std::sync::atomic::Ordering::SeqCst);
    if pending > 0 {
        println!("Shutting down: waiting for {} in-flight request(s)...", pending);
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(SHUTDOWN_GRACE_SECS);
    while stats::IN_FLIGHT.load(std::sync::atomic::Ordering::SeqCst) > 0
        && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let leftover = stats::IN_FLIGHT.load(std::sync::atomic::Ordering::SeqCst);
    if leftover > 0 {
        eprintln!("Shutdown grace period expired with {} request(s) still running", leftover);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Async connection handler (runs on a Tokio worker thread)
// ---------------------------------------------------------------------------

async fn handle_connection(
    stream: tokio::net::TcpStream,
    tx: tokio::sync::mpsc::UnboundedSender<RequestMessage>,
) {
    if let Err(e) = handle_connection_inner(stream, tx).await {
        eprintln!("Connection error: {}", e);
    }
}

/// Read a line with a size limit. Returns `None` if the limit is exceeded.
async fn read_limited_line(
    reader: &mut tokio::io::BufReader<tokio::net::tcp::ReadHalf<'_>>,
    limit: usize,
) -> std::result::Result<Option<String>, std::io::Error> {
    let mut line = String::new();
    loop {
        let n = reader.read_line(&mut line).await?;
        if n == 0 || line.ends_with('\n') {
            break;
        }
        if line.len() > limit {
            return Ok(None);
        }
    }
    if line.len() > limit {
        return Ok(None);
    }
    Ok(Some(line))
}

async fn handle_connection_inner(
    mut stream: tokio::net::TcpStream,
    tx: tokio::sync::mpsc::UnboundedSender<RequestMessage>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = stream.split();
    let mut reader = tokio::io::BufReader::new(reader);

    // HTTP/1.1 keep-alive: serve multiple requests on the same connection.
    let mut served: usize = 0;
    loop {
        // Timeout covers both the idle wait for the next request and the
        // parsing of a request in progress.
        let result = tokio::time::timeout(
            Duration::from_secs(CONNECTION_TIMEOUT_SECS),
            parse_request(&mut reader),
        )
        .await;

        let request = match result {
            // Client closed the connection cleanly.
            Ok(Ok(None)) => return Ok(()),
            Ok(Ok(Some(req))) => req,
            Ok(Err(e)) => {
                // Parse error — determine appropriate status code
                let err_msg = e.to_string();
                let (status, msg) = if err_msg.contains("line too long") {
                    (414u16, "URI Too Long")
                } else if err_msg.contains("Header too long") {
                    (431u16, "Request Header Fields Too Large")
                } else if err_msg.contains("Too many headers") {
                    (431u16, "Request Header Fields Too Large")
                } else if err_msg.contains("Body too large") {
                    (413u16, "Payload Too Large")
                } else {
                    (400u16, "Bad Request")
                };
                let resp = build_response_bytes(status, "text/plain", msg, &[]);
                writer.write_all(&resp).await.ok();
                return Ok(());
            }
            Err(_timeout) => {
                // Only answer 408 if the timeout hit the *first* request;
                // an idle keep-alive connection is simply closed.
                if served == 0 {
                    let resp = build_response_bytes(408, "text/plain", "Request Timeout", &[]);
                    writer.write_all(&resp).await.ok();
                }
                return Ok(());
            }
        };

        served += 1;

        // Send to the Lua event loop and wait for the response.
        let is_head = request.method == "HEAD";
        let client_keep_alive = request.keep_alive;
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        if tx.send((request, resp_tx)).is_err() {
            // Dispatch loop is gone (shutdown).
            let resp = build_response_bytes(503, "text/plain", "Service Unavailable", &[]);
            writer.write_all(&resp).await.ok();
            return Ok(());
        }

        match resp_rx.await {
            Ok(response) => {
                // Reuse the connection unless the client refused, the
                // per-connection cap is reached, or we are shutting down.
                let keep_alive = client_keep_alive
                    && served < MAX_REQUESTS_PER_CONNECTION
                    && !coppermoon_core::shutdown::is_requested();

                let bytes = build_response_bytes_ex(
                    response.status,
                    &response.content_type,
                    &response.body,
                    &response.headers,
                    is_head,
                    keep_alive,
                );
                writer.write_all(&bytes).await.ok();
                writer.flush().await.ok();

                if !keep_alive {
                    return Ok(());
                }
            }
            Err(_) => {
                // The dispatch task was dropped: happens when a graceful
                // shutdown discards queued requests.
                let (status, msg) = if coppermoon_core::shutdown::is_requested() {
                    (503, "Service Unavailable")
                } else {
                    (500, "Internal Server Error")
                };
                let bytes = build_response_bytes(status, "text/plain", msg, &[]);
                writer.write_all(&bytes).await.ok();
                return Ok(());
            }
        }
    }
}

/// Parse an HTTP request with enforced size limits.
///
/// Returns `Ok(None)` when the client closed the connection cleanly before
/// sending a request (normal end of a keep-alive connection).
async fn parse_request(
    reader: &mut tokio::io::BufReader<tokio::net::tcp::ReadHalf<'_>>,
) -> std::result::Result<Option<ParsedRequest>, Box<dyn std::error::Error + Send + Sync>> {
    // --- Parse request line (bounded) ---
    let request_line = read_limited_line(reader, MAX_REQUEST_LINE)
        .await?
        .ok_or("Request line too long")?;

    // EOF before any byte of a new request: clean close.
    if request_line.is_empty() {
        return Ok(None);
    }

    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return Err("Bad request line".into());
    }

    let method = parts[0].to_uppercase();
    let full_path = parts[1].to_string();
    // HTTP/0.9 has no version token; treat anything unknown as HTTP/1.0
    // (keep-alive only when explicitly requested).
    let is_http11 = parts.get(2).is_some_and(|v| v.eq_ignore_ascii_case("HTTP/1.1"));

    let (path, query_string) = if let Some(pos) = full_path.find('?') {
        (full_path[..pos].to_string(), Some(full_path[pos + 1..].to_string()))
    } else {
        (full_path, None)
    };

    // --- Parse headers (bounded count and size) ---
    let mut headers: HashMap<String, String> = HashMap::new();
    let mut content_length: usize = 0;
    let mut chunked = false;

    for _ in 0..MAX_HEADER_COUNT + 1 {
        let line = read_limited_line(reader, MAX_HEADER_LINE)
            .await?
            .ok_or("Header too long")?;

        if line.trim().is_empty() {
            break;
        }

        if headers.len() >= MAX_HEADER_COUNT {
            return Err("Too many headers".into());
        }

        if let Some((key, value)) = line.trim().split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim().to_string();
            if key == "content-length" {
                content_length = value.parse().unwrap_or(0);
            } else if key == "transfer-encoding"
                && value.to_lowercase().contains("chunked")
            {
                chunked = true;
            }
            headers.insert(key, value);
        }
    }

    // Connection semantics: HTTP/1.1 defaults to keep-alive, HTTP/1.0 to
    // close, both overridable by the Connection header.
    let connection = headers
        .get("connection")
        .map(|v| v.to_lowercase())
        .unwrap_or_default();
    let keep_alive = if is_http11 {
        connection != "close"
    } else {
        connection == "keep-alive"
    };

    // --- Read body (bounded) ---
    // Per RFC 7230 §3.3.3, Transfer-Encoding takes precedence over
    // Content-Length.
    let body = if chunked {
        read_chunked_body(reader).await?
    } else if content_length > 0 {
        if content_length > MAX_BODY_SIZE {
            return Err("Body too large".into());
        }
        let mut buf = vec![0u8; content_length];
        reader.read_exact(&mut buf).await?;
        String::from_utf8_lossy(&buf).to_string()
    } else {
        String::new()
    };

    Ok(Some(ParsedRequest { method, path, query_string, headers, body, keep_alive }))
}

/// Decode a `Transfer-Encoding: chunked` request body (RFC 7230 §4.1),
/// enforcing the same total size limit as Content-Length bodies.
async fn read_chunked_body(
    reader: &mut tokio::io::BufReader<tokio::net::tcp::ReadHalf<'_>>,
) -> std::result::Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut body: Vec<u8> = Vec::new();

    loop {
        // Chunk size line: hex size, optionally followed by ";extensions".
        let size_line = read_limited_line(reader, MAX_HEADER_LINE)
            .await?
            .ok_or("Chunk size line too long")?;
        let size_str = size_line
            .trim()
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        let size = usize::from_str_radix(size_str, 16)
            .map_err(|_| format!("Invalid chunk size: '{}'", size_str))?;

        if body.len() + size > MAX_BODY_SIZE {
            return Err("Body too large".into());
        }

        if size == 0 {
            // Trailer section: read (and discard) until the empty line.
            for _ in 0..MAX_HEADER_COUNT + 1 {
                let line = read_limited_line(reader, MAX_HEADER_LINE)
                    .await?
                    .ok_or("Trailer line too long")?;
                if line.trim().is_empty() {
                    return Ok(String::from_utf8_lossy(&body).to_string());
                }
            }
            return Err("Too many trailer lines".into());
        }

        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..]).await?;

        // Chunk data is followed by CRLF.
        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf).await?;
        if &crlf != b"\r\n" {
            return Err("Malformed chunk terminator".into());
        }
    }
}

// ---------------------------------------------------------------------------
// Lua handler dispatch (runs as a task on the event-loop LocalSet)
// ---------------------------------------------------------------------------

async fn dispatch_to_lua(
    lua: &Lua,
    request: &ParsedRequest,
    route_handlers: &HashMap<String, RegistryKey>,
) -> HttpResponse {
    match dispatch_to_lua_inner(lua, request, route_handlers).await {
        Ok(resp) => resp,
        Err(e) => {
            // Route through the uncaught-error policy (process.on_error
            // hook, or default log). The request itself gets a 500.
            coppermoon_core::uncaught::report(lua, "http handler", &e).await;
            HttpResponse {
                status: 500,
                content_type: "text/plain".into(),
                body: format!("Internal Server Error: {}", e).into_bytes(),
                headers: Vec::new(),
            }
        }
    }
}

async fn dispatch_to_lua_inner(
    lua: &Lua,
    request: &ParsedRequest,
    route_handlers: &HashMap<String, RegistryKey>,
) -> mlua::Result<HttpResponse> {
    // Find handler — exact match, then wildcard, then ALL method
    let route_key = format!("{}:{}", request.method, request.path);
    let wildcard_key = format!("{}:*", request.method);
    let all_key = format!("ALL:{}", request.path);
    let all_wildcard = "ALL:*".to_string();

    let mut handler_key = route_handlers.get(&route_key)
        .or_else(|| route_handlers.get(&wildcard_key))
        .or_else(|| route_handlers.get(&all_key))
        .or_else(|| route_handlers.get(&all_wildcard));

    // HEAD falls back to GET per HTTP spec (RFC 7231 §4.3.2)
    if handler_key.is_none() && request.method == "HEAD" {
        let get_key = format!("GET:{}", request.path);
        let get_wildcard = "GET:*".to_string();
        handler_key = route_handlers.get(&get_key)
            .or_else(|| route_handlers.get(&get_wildcard));
    }

    let Some(reg_key) = handler_key else {
        return Ok(HttpResponse {
            status: 404,
            content_type: "text/plain".into(),
            body: b"Not Found".to_vec(),
            headers: Vec::new(),
        });
    };

    // Build the request context table (same API as before)
    let ctx = lua.create_table()?;
    ctx.set("method", request.method.as_str())?;
    ctx.set("path", request.path.as_str())?;
    ctx.set("body", request.body.as_str())?;

    // Headers table
    let headers_table = lua.create_table()?;
    for (k, v) in &request.headers {
        headers_table.set(k.as_str(), v.as_str())?;
    }
    ctx.set("headers", headers_table)?;

    // Query params table
    let query_table = lua.create_table()?;
    if let Some(ref qs) = request.query_string {
        for pair in qs.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                query_table.set(
                    urlencoding_decode(key),
                    urlencoding_decode(value),
                )?;
            }
        }
    }
    ctx.set("query", query_table)?;

    // Response state
    ctx.set("_status", 200u16)?;
    ctx.set("_content_type", "text/plain")?;
    ctx.set("_body", "")?;

    // ctx:status(code)
    ctx.set("status", lua.create_function(|_, (ctx, code): (Table, u16)| {
        ctx.set("_status", code)?;
        Ok(ctx)
    })?)?;

    // ctx:json(data)
    ctx.set("json", lua.create_function(|_lua, (ctx, data): (Table, Value)| {
        let json_str = value_to_json(&data)?;
        ctx.set("_content_type", "application/json")?;
        ctx.set("_body", json_str)?;
        Ok(ctx)
    })?)?;

    // ctx:text(str)
    ctx.set("text", lua.create_function(|_, (ctx, text): (Table, String)| {
        ctx.set("_content_type", "text/plain")?;
        ctx.set("_body", text)?;
        Ok(ctx)
    })?)?;

    // ctx:html(str)
    ctx.set("html", lua.create_function(|_, (ctx, html): (Table, String)| {
        ctx.set("_content_type", "text/html")?;
        ctx.set("_body", html)?;
        Ok(ctx)
    })?)?;

    // Call the handler on its own Lua coroutine: the call suspends whenever
    // the handler performs an async operation, letting other requests run.
    let handler: Function = lua.registry_value(reg_key)?;
    let result = handler.call_async::<Value>(ctx.clone()).await?;

    let status: u16 = ctx.get("_status").unwrap_or(200);
    let content_type: String = ctx.get("_content_type").unwrap_or_else(|_| "text/plain".to_string());
    let body: Vec<u8> = match ctx.get::<mlua::String>("_body") {
        Ok(s) => s.as_bytes().to_vec(),
        Err(_) => match result {
            Value::String(s) => s.as_bytes().to_vec(),
            Value::Nil => Vec::new(),
            _ => value_to_json(&result).unwrap_or_default().into_bytes(),
        },
    };

    // Read custom headers from ctx._headers
    let mut extra_headers = Vec::new();
    if let Ok(headers_table) = ctx.get::<mlua::Table>("_headers") {
        for pair in headers_table.pairs::<String, mlua::Value>() {
            if let Ok((key, value)) = pair {
                let key_lower = key.to_lowercase();
                if key_lower != "content-type" && key_lower != "content-length" {
                    let val_str = match &value {
                        mlua::Value::String(s) => s.to_str().map(|v| v.to_string()).unwrap_or_default(),
                        _ => format!("{:?}", value),
                    };
                    extra_headers.push((key, val_str));
                }
            }
        }
    }

    Ok(HttpResponse { status, content_type, body, headers: extra_headers })
}

// ---------------------------------------------------------------------------
// Utility functions (kept from original)
// ---------------------------------------------------------------------------

fn urlencoding_decode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }

    result
}

/// Escape a string for safe JSON embedding (RFC 8259).
fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if c < '\u{0020}' => {
                // Other control characters → \u00XX
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn value_to_json(value: &Value) -> mlua::Result<String> {
    value_to_json_depth(value, 0)
}

fn value_to_json_depth(value: &Value, depth: usize) -> mlua::Result<String> {
    // Depth guard: prevents a stack overflow (= process abort) on cyclic or
    // pathologically nested tables passed to ctx:json().
    if depth > MAX_JSON_DEPTH {
        return Err(mlua::Error::runtime(
            "JSON encoding exceeds maximum nesting depth (cyclic table?)",
        ));
    }
    match value {
        Value::Nil => Ok("null".to_string()),
        Value::Boolean(b) => Ok(b.to_string()),
        Value::Integer(i) => Ok(i.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::String(s) => {
            let s = s.to_str().map_err(|e| mlua::Error::runtime(e.to_string()))?;
            Ok(escape_json_string(&s))
        }
        Value::Table(t) => {
            // Check if array or object
            let mut is_array = true;
            let mut max_index = 0i64;

            for pair in t.clone().pairs::<Value, Value>() {
                if let Ok((key, _)) = pair {
                    match key {
                        Value::Integer(i) if i > 0 => {
                            if i > max_index {
                                max_index = i;
                            }
                        }
                        _ => {
                            is_array = false;
                            break;
                        }
                    }
                }
            }

            if is_array && max_index > 0 {
                let mut items = Vec::new();
                for i in 1..=max_index {
                    let val: Value = t.get(i)?;
                    items.push(value_to_json_depth(&val, depth + 1)?);
                }
                Ok(format!("[{}]", items.join(",")))
            } else {
                let mut items = Vec::new();
                for pair in t.clone().pairs::<Value, Value>() {
                    if let Ok((key, val)) = pair {
                        let key_str = match &key {
                            Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                            Value::Integer(i) => i.to_string(),
                            _ => continue,
                        };
                        items.push(format!("{}:{}", escape_json_string(&key_str), value_to_json_depth(&val, depth + 1)?));
                    }
                }
                Ok(format!("{{{}}}", items.join(",")))
            }
        }
        _ => Ok("null".to_string()),
    }
}

/// Error/short-circuit responses always close the connection.
fn build_response_bytes(
    status: u16,
    content_type: &str,
    body: &str,
    extra_headers: &[(String, String)],
) -> Vec<u8> {
    build_response_bytes_ex(status, content_type, body.as_bytes(), extra_headers, false, false)
}

/// Build HTTP response bytes. When `head_only` is true, Content-Length reflects
/// the body size but the body itself is omitted (HTTP HEAD semantics).
fn build_response_bytes_ex(
    status: u16,
    content_type: &str,
    body: &[u8],
    extra_headers: &[(String, String)],
    head_only: bool,
    keep_alive: bool,
) -> Vec<u8> {
    let status_text = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Payload Too Large",
        414 => "URI Too Long",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unknown",
    };

    let mut header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n",
        status,
        status_text,
        content_type,
        body.len(),
        if keep_alive { "keep-alive" } else { "close" },
    );

    for (key, value) in extra_headers {
        header.push_str(&format!("{}: {}\r\n", key, value));
    }

    header.push_str("\r\n");

    let mut bytes = header.into_bytes();
    if !head_only {
        bytes.extend_from_slice(body);
    }
    bytes
}
