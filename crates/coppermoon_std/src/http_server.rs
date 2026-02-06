//! HTTP Server module for CopperMoon
//!
//! Provides an HTTP server with concurrent connection handling.
//! Connections are accepted and I/O is performed asynchronously on Tokio
//! worker threads, while Lua handler execution is serialised on the main
//! thread (Node.js-style event loop).

use coppermoon_core::Result;
use coppermoon_core::event_loop;
use mlua::{Lua, Table, Function, Value, RegistryKey};
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

// ---------------------------------------------------------------------------
// Security limits
// ---------------------------------------------------------------------------

const MAX_REQUEST_LINE: usize = 8 * 1024;       // 8 KB
const MAX_HEADER_LINE: usize = 8 * 1024;        // 8 KB per header
const MAX_HEADER_COUNT: usize = 100;             // max number of headers
const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;  // 10 MB
const CONNECTION_TIMEOUT_SECS: u64 = 30;         // 30s idle timeout

// ---------------------------------------------------------------------------
// Plain-data types that cross the channel boundary (no Lua objects)
// ---------------------------------------------------------------------------

struct ParsedRequest {
    method: String,
    path: String,
    query_string: Option<String>,
    headers: HashMap<String, String>,
    body: String,
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

    server.set("listen", lua.create_function(server_listen)?)?;

    Ok(server)
}

// ---------------------------------------------------------------------------
// server:listen(port, callback?)
// ---------------------------------------------------------------------------

fn server_listen(lua: &Lua, (server, port, callback): (Table, u16, Option<Function>)) -> mlua::Result<()> {
    server.set("_port", port)?;

    let routes: Table = server.get("_routes")?;

    // Store route handlers in the Lua registry so they stay alive.
    let mut route_handlers: HashMap<String, RegistryKey> = HashMap::new();
    for pair in routes.pairs::<String, Function>() {
        let (key, handler) = pair?;
        let reg_key = lua.create_registry_value(handler)?;
        route_handlers.insert(key, reg_key);
    }

    let addr = format!("127.0.0.1:{}", port);

    // Create a std::sync::mpsc channel for request dispatch.
    // The main Lua thread receives on this channel (blocking, NOT inside
    // a Tokio context) so that Lua handlers can freely call block_on().
    let (tx, rx) = std::sync::mpsc::channel::<RequestMessage>();

    // Spawn the async accept loop on the Tokio runtime.
    let addr_clone = addr.clone();
    coppermoon_core::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(&addr_clone).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to bind to {}: {}", addr_clone, e);
                return;
            }
        };

        loop {
            match listener.accept().await {
                Ok((stream, _peer)) => {
                    let tx = tx.clone();
                    tokio::spawn(handle_connection(stream, tx));
                }
                Err(e) => {
                    eprintln!("Accept error: {}", e);
                }
            }
        }
    });

    // Notify callback if provided
    if let Some(cb) = callback {
        cb.call::<()>(port)?;
    }

    println!("CopperMoon server listening on http://{}", addr);

    // ---------- Main Lua event loop ----------
    // We use recv_timeout so we can also drain pending timers.
    loop {
        // Process any ready timer callbacks between requests.
        drain_timers(lua);

        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok((request, resp_tx)) => {
                let response = dispatch_to_lua(lua, &request, &route_handlers);
                // Ignore send error — the connection task may have dropped.
                let _ = resp_tx.send(response);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Async connection handler (runs on a Tokio worker thread)
// ---------------------------------------------------------------------------

async fn handle_connection(
    stream: tokio::net::TcpStream,
    tx: std::sync::mpsc::Sender<RequestMessage>,
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
    tx: std::sync::mpsc::Sender<RequestMessage>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = stream.split();
    let mut reader = tokio::io::BufReader::new(reader);

    // Apply connection timeout to the entire request parsing phase.
    let result = tokio::time::timeout(
        Duration::from_secs(CONNECTION_TIMEOUT_SECS),
        parse_request(&mut reader),
    )
    .await;

    let request = match result {
        Ok(Ok(req)) => req,
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
            let resp = build_response_bytes(status as u16, "text/plain", msg, &[]);
            writer.write_all(&resp).await.ok();
            return Ok(());
        }
        Err(_timeout) => {
            let resp = build_response_bytes(408, "text/plain", "Request Timeout", &[]);
            writer.write_all(&resp).await.ok();
            return Ok(());
        }
    };

    // Send to main Lua thread and wait for response.
    let is_head = request.method == "HEAD";
    let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
    tx.send((request, resp_tx))?;

    match resp_rx.await {
        Ok(response) => {
            let bytes = build_response_bytes_ex(
                response.status,
                &response.content_type,
                &response.body,
                &response.headers,
                is_head,
            );
            writer.write_all(&bytes).await.ok();
            writer.flush().await.ok();
        }
        Err(_) => {
            let bytes = build_response_bytes(500, "text/plain", "Internal Server Error", &[]);
            writer.write_all(&bytes).await.ok();
        }
    }

    Ok(())
}

/// Parse an HTTP request with enforced size limits.
async fn parse_request(
    reader: &mut tokio::io::BufReader<tokio::net::tcp::ReadHalf<'_>>,
) -> std::result::Result<ParsedRequest, Box<dyn std::error::Error + Send + Sync>> {
    // --- Parse request line (bounded) ---
    let request_line = read_limited_line(reader, MAX_REQUEST_LINE)
        .await?
        .ok_or("Request line too long")?;

    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return Err("Bad request line".into());
    }

    let method = parts[0].to_uppercase();
    let full_path = parts[1].to_string();

    let (path, query_string) = if let Some(pos) = full_path.find('?') {
        (full_path[..pos].to_string(), Some(full_path[pos + 1..].to_string()))
    } else {
        (full_path, None)
    };

    // --- Parse headers (bounded count and size) ---
    let mut headers: HashMap<String, String> = HashMap::new();
    let mut content_length: usize = 0;

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
            }
            headers.insert(key, value);
        }
    }

    // --- Read body (bounded) ---
    let body = if content_length > 0 {
        if content_length > MAX_BODY_SIZE {
            return Err("Body too large".into());
        }
        let mut buf = vec![0u8; content_length];
        reader.read_exact(&mut buf).await?;
        String::from_utf8_lossy(&buf).to_string()
    } else {
        String::new()
    };

    Ok(ParsedRequest { method, path, query_string, headers, body })
}

// ---------------------------------------------------------------------------
// Lua handler dispatch (runs on the main thread)
// ---------------------------------------------------------------------------

fn dispatch_to_lua(
    lua: &Lua,
    request: &ParsedRequest,
    route_handlers: &HashMap<String, RegistryKey>,
) -> HttpResponse {
    match dispatch_to_lua_inner(lua, request, route_handlers) {
        Ok(resp) => resp,
        Err(e) => {
            eprintln!("Handler error: {}", e);
            HttpResponse {
                status: 500,
                content_type: "text/plain".into(),
                body: format!("Internal Server Error: {}", e).into_bytes(),
                headers: Vec::new(),
            }
        }
    }
}

fn dispatch_to_lua_inner(
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

    // Call the handler
    let handler: Function = lua.registry_value(reg_key)?;
    let result = handler.call::<Value>(ctx.clone())?;

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
// Timer integration
// ---------------------------------------------------------------------------

/// Drain all ready timer events and execute their Lua callbacks.
fn drain_timers(lua: &Lua) {
    use coppermoon_core::event_loop::{TimerEvent, TimerType};

    while let Some(event) = event_loop::try_recv_timer_event(Duration::from_millis(0)) {
        match event {
            TimerEvent::Ready(id) => {
                if let Some(cb) = event_loop::take_timer_callback(id) {
                    let func: mlua::Result<Function> = lua.registry_value(&cb.registry_key);
                    if let Ok(func) = func {
                        if let Err(e) = func.call::<()>(()) {
                            eprintln!("Timer callback error: {}", e);
                        }
                    }
                    match cb.timer_type {
                        TimerType::Timeout => {
                            let _ = lua.remove_registry_value(cb.registry_key);
                        }
                        TimerType::Interval { .. } => {
                            event_loop::restore_timer_callback(id, cb);
                        }
                    }
                }
            }
        }
    }
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
                    items.push(value_to_json(&val)?);
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
                        items.push(format!("{}:{}", escape_json_string(&key_str), value_to_json(&val)?));
                    }
                }
                Ok(format!("{{{}}}", items.join(",")))
            }
        }
        _ => Ok("null".to_string()),
    }
}

fn build_response_bytes(
    status: u16,
    content_type: &str,
    body: &str,
    extra_headers: &[(String, String)],
) -> Vec<u8> {
    build_response_bytes_ex(status, content_type, body.as_bytes(), extra_headers, false)
}

/// Build HTTP response bytes. When `head_only` is true, Content-Length reflects
/// the body size but the body itself is omitted (HTTP HEAD semantics).
fn build_response_bytes_ex(
    status: u16,
    content_type: &str,
    body: &[u8],
    extra_headers: &[(String, String)],
    head_only: bool,
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
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        status,
        status_text,
        content_type,
        body.len(),
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
