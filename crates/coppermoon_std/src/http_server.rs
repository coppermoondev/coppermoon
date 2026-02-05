//! HTTP Server module for CopperMoon
//!
//! Provides a simple HTTP server that can be used from Lua.
//! This is a basic server for development purposes.

use coppermoon_core::Result;
use mlua::{Lua, Table, Function, Value, RegistryKey};
use std::collections::HashMap;
use std::io::{Read, Write, BufReader, BufRead};
use std::net::TcpListener;

/// Register the http.server module
pub fn register(lua: &Lua) -> Result<Table> {
    let server_table = lua.create_table()?;

    // http.server.new() -> Server
    server_table.set("new", lua.create_function(server_new)?)?;

    Ok(server_table)
}

fn server_new(lua: &Lua, _: ()) -> mlua::Result<Table> {
    let server = lua.create_table()?;

    // Store routes in a table
    let routes = lua.create_table()?;
    server.set("_routes", routes)?;
    server.set("_port", 3000u16)?;

    // server:get(path, handler)
    server.set("get", lua.create_function(|_, (server, path, handler): (Table, String, Function)| {
        let routes: Table = server.get("_routes")?;
        routes.set(format!("GET:{}", path), handler)?;
        Ok(server)
    })?)?;

    // server:post(path, handler)
    server.set("post", lua.create_function(|_, (server, path, handler): (Table, String, Function)| {
        let routes: Table = server.get("_routes")?;
        routes.set(format!("POST:{}", path), handler)?;
        Ok(server)
    })?)?;

    // server:put(path, handler)
    server.set("put", lua.create_function(|_, (server, path, handler): (Table, String, Function)| {
        let routes: Table = server.get("_routes")?;
        routes.set(format!("PUT:{}", path), handler)?;
        Ok(server)
    })?)?;

    // server:delete(path, handler)
    server.set("delete", lua.create_function(|_, (server, path, handler): (Table, String, Function)| {
        let routes: Table = server.get("_routes")?;
        routes.set(format!("DELETE:{}", path), handler)?;
        Ok(server)
    })?)?;

    // server:listen(port, callback?)
    server.set("listen", lua.create_function(server_listen)?)?;

    Ok(server)
}

fn server_listen(lua: &Lua, (server, port, callback): (Table, u16, Option<Function>)) -> mlua::Result<()> {
    server.set("_port", port)?;

    let routes: Table = server.get("_routes")?;

    // Store routes with registry keys
    let mut route_handlers: HashMap<String, RegistryKey> = HashMap::new();
    for pair in routes.pairs::<String, Function>() {
        let (key, handler) = pair?;
        let reg_key = lua.create_registry_value(handler)?;
        route_handlers.insert(key, reg_key);
    }

    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr)
        .map_err(|e| mlua::Error::runtime(format!("Failed to bind to {}: {}", addr, e)))?;

    // Notify callback if provided
    if let Some(cb) = callback {
        cb.call::<()>(port)?;
    }

    println!("CopperMoon server listening on http://{}", addr);

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(e) = handle_request(lua, &mut stream, &route_handlers) {
                    eprintln!("Request error: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Connection error: {}", e);
            }
        }
    }

    Ok(())
}

fn handle_request(
    lua: &Lua,
    stream: &mut std::net::TcpStream,
    route_handlers: &HashMap<String, RegistryKey>,
) -> mlua::Result<()> {
    let mut reader = BufReader::new(&*stream);
    let mut request_line = String::new();

    reader.read_line(&mut request_line)?;

    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }

    let method = parts[0].to_uppercase();
    let full_path = parts[1].to_string();

    // Split path and query string
    let (path, query_string) = if let Some(pos) = full_path.find('?') {
        (&full_path[..pos], Some(&full_path[pos + 1..]))
    } else {
        (full_path.as_str(), None)
    };

    // Read headers
    let mut headers: HashMap<String, String> = HashMap::new();
    let mut content_length: usize = 0;

    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.trim().is_empty() {
            break;
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

    // Read body if present
    let mut body = String::new();
    if content_length > 0 {
        let mut body_bytes = vec![0u8; content_length];
        reader.read_exact(&mut body_bytes)?;
        body = String::from_utf8_lossy(&body_bytes).to_string();
    }

    // Find handler - try exact match first, then wildcard
    let route_key = format!("{}:{}", method, path);
    let wildcard_key = format!("{}:*", method);

    let handler_key = route_handlers.get(&route_key)
        .or_else(|| route_handlers.get(&wildcard_key));

    let response = if let Some(reg_key) = handler_key {
        // Create request context
        let ctx = lua.create_table()?;
        ctx.set("method", method.clone())?;
        ctx.set("path", path)?;
        ctx.set("body", body)?;

        // Headers table
        let headers_table = lua.create_table()?;
        for (k, v) in &headers {
            headers_table.set(k.clone(), v.clone())?;
        }
        ctx.set("headers", headers_table)?;

        // Query params table
        let query_table = lua.create_table()?;
        if let Some(qs) = query_string {
            for pair in qs.split('&') {
                if let Some((key, value)) = pair.split_once('=') {
                    query_table.set(
                        urlencoding_decode(key),
                        urlencoding_decode(value)
                    )?;
                }
            }
        }
        ctx.set("query", query_table)?;

        // Response state
        ctx.set("_status", 200u16)?;
        ctx.set("_content_type", "text/plain")?;
        ctx.set("_body", "")?;

        // ctx:status(code) - set status
        ctx.set("status", lua.create_function(|_, (ctx, code): (Table, u16)| {
            ctx.set("_status", code)?;
            Ok(ctx)
        })?)?;

        // ctx:json(data) - send JSON response
        ctx.set("json", lua.create_function(|_lua, (ctx, data): (Table, Value)| {
            let json_str = value_to_json(&data)?;
            ctx.set("_content_type", "application/json")?;
            ctx.set("_body", json_str)?;
            Ok(ctx)
        })?)?;

        // ctx:text(str) - send text response
        ctx.set("text", lua.create_function(|_, (ctx, text): (Table, String)| {
            ctx.set("_content_type", "text/plain")?;
            ctx.set("_body", text)?;
            Ok(ctx)
        })?)?;

        // ctx:html(str) - send HTML response
        ctx.set("html", lua.create_function(|_, (ctx, html): (Table, String)| {
            ctx.set("_content_type", "text/html")?;
            ctx.set("_body", html)?;
            Ok(ctx)
        })?)?;

        // Call the handler
        let handler: Function = lua.registry_value(reg_key)?;
        match handler.call::<Value>(ctx.clone()) {
            Ok(result) => {
                let status: u16 = ctx.get("_status").unwrap_or(200);
                let content_type: String = ctx.get("_content_type").unwrap_or("text/plain".to_string());
                let body: String = ctx.get("_body").unwrap_or_else(|_| {
                    // If no body set, try to use return value
                    match result {
                        Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                        Value::Nil => "".to_string(),
                        _ => value_to_json(&result).unwrap_or_default(),
                    }
                });

                // Read custom headers from ctx._headers (set by Lua framework)
                let mut extra_headers = Vec::new();
                if let Ok(headers_table) = ctx.get::<mlua::Table>("_headers") {
                    for pair in headers_table.pairs::<String, mlua::Value>() {
                        if let Ok((key, value)) = pair {
                            let key_lower = key.to_lowercase();
                            // Skip Content-Type (already handled) and Content-Length (computed)
                            if key_lower != "content-type" && key_lower != "content-length" {
                                let val_str = match &value {
                                    mlua::Value::String(s) => {
                                        match s.to_str() {
                                            Ok(v) => v.to_string(),
                                            Err(_) => String::new(),
                                        }
                                    }
                                    _ => format!("{:?}", value),
                                };
                                extra_headers.push((key, val_str));
                            }
                        }
                    }
                }

                build_response(status, &content_type, &body, &extra_headers)
            }
            Err(e) => {
                eprintln!("Handler error: {}", e);
                build_response(500, "text/plain", &format!("Internal Server Error: {}", e), &[])
            }
        }
    } else {
        build_response(404, "text/plain", "Not Found", &[])
    };

    stream.write_all(response.as_bytes())?;
    stream.flush()?;

    Ok(())
}

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

fn value_to_json(value: &Value) -> mlua::Result<String> {
    match value {
        Value::Nil => Ok("null".to_string()),
        Value::Boolean(b) => Ok(b.to_string()),
        Value::Integer(i) => Ok(i.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::String(s) => {
            let str = s.to_str().map_err(|e| mlua::Error::runtime(e.to_string()))?;
            Ok(format!("\"{}\"", str.replace('\\', "\\\\").replace('"', "\\\"")))
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
                        items.push(format!("\"{}\":{}", key_str, value_to_json(&val)?));
                    }
                }
                Ok(format!("{{{}}}", items.join(",")))
            }
        }
        _ => Ok("null".to_string()),
    }
}

fn build_response(status: u16, content_type: &str, body: &str, extra_headers: &[(String, String)]) -> String {
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
        409 => "Conflict",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unknown",
    };

    let mut headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        status,
        status_text,
        content_type,
        body.len(),
    );

    for (key, value) in extra_headers {
        headers.push_str(&format!("{}: {}\r\n", key, value));
    }

    headers.push_str("\r\n");
    headers.push_str(body);
    headers
}
