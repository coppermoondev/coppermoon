//! HTTP client module for CopperMoon
//!
//! Provides HTTP client functionality for making web requests.

use coppermoon_core::Result;
use mlua::{Lua, Table};
use std::time::Duration;
use std::collections::HashMap;
use std::sync::Arc;

/// Register the http module
pub fn register(lua: &Lua) -> Result<Table> {
    let http_table = lua.create_table()?;

    // http.get(url, options?) -> response
    http_table.set("get", lua.create_function(http_get)?)?;

    // http.post(url, body, options?) -> response
    http_table.set("post", lua.create_function(http_post)?)?;

    // http.put(url, body, options?) -> response
    http_table.set("put", lua.create_function(http_put)?)?;

    // http.delete(url, options?) -> response
    http_table.set("delete", lua.create_function(http_delete)?)?;

    // http.patch(url, body, options?) -> response
    http_table.set("patch", lua.create_function(http_patch)?)?;

    // http.request(options) -> response
    http_table.set("request", lua.create_function(http_request)?)?;

    // http.create_session() -> session (with cookie jar)
    http_table.set("create_session", lua.create_function(create_session)?)?;

    Ok(http_table)
}

/// Options for HTTP requests
struct RequestOptions {
    headers: HashMap<String, String>,
    timeout: Option<Duration>,
    body: Option<String>,
    cookies: HashMap<String, String>,
}

impl RequestOptions {
    fn from_table(table: &Table) -> mlua::Result<Self> {
        let mut headers = HashMap::new();
        let mut cookies = HashMap::new();

        // Parse headers
        if let Ok(headers_table) = table.get::<Table>("headers") {
            for pair in headers_table.pairs::<String, String>() {
                if let Ok((k, v)) = pair {
                    headers.insert(k, v);
                }
            }
        }

        // Parse cookies
        if let Ok(cookies_table) = table.get::<Table>("cookies") {
            for pair in cookies_table.pairs::<String, String>() {
                if let Ok((k, v)) = pair {
                    cookies.insert(k, v);
                }
            }
        }

        // Parse timeout (in milliseconds)
        let timeout = table.get::<u64>("timeout")
            .ok()
            .map(Duration::from_millis);

        // Parse body
        let body = table.get::<String>("body").ok();

        Ok(Self { headers, timeout, body, cookies })
    }

    fn empty() -> Self {
        Self {
            headers: HashMap::new(),
            timeout: None,
            body: None,
            cookies: HashMap::new(),
        }
    }
}

fn build_response(lua: &Lua, response: reqwest::blocking::Response) -> mlua::Result<Table> {
    let status = response.status().as_u16();
    let status_text = response.status().canonical_reason().unwrap_or("").to_string();
    let url = response.url().to_string();

    // Get headers before consuming response
    let mut headers_map = HashMap::new();
    let mut cookies_vec = Vec::new();

    for (key, value) in response.headers() {
        if let Ok(v) = value.to_str() {
            headers_map.insert(key.as_str().to_string(), v.to_string());

            // Collect Set-Cookie headers
            if key.as_str().to_lowercase() == "set-cookie" {
                cookies_vec.push(v.to_string());
            }
        }
    }

    let body = response.text()
        .map_err(|e| mlua::Error::runtime(format!("Failed to read response body: {}", e)))?;

    let result = lua.create_table()?;
    result.set("status", status)?;
    result.set("status_text", status_text)?;
    result.set("body", body)?;
    result.set("ok", status >= 200 && status < 300)?;
    result.set("url", url)?;

    // Add headers table
    let headers_table = lua.create_table()?;
    for (k, v) in headers_map {
        headers_table.set(k, v)?;
    }
    result.set("headers", headers_table)?;

    // Add cookies table (parsed from Set-Cookie headers)
    let cookies_table = lua.create_table()?;
    for cookie_str in &cookies_vec {
        if let Some((name_value, _rest)) = cookie_str.split_once(';') {
            if let Some((name, value)) = name_value.split_once('=') {
                cookies_table.set(name.trim().to_string(), value.trim().to_string())?;
            }
        } else if let Some((name, value)) = cookie_str.split_once('=') {
            cookies_table.set(name.trim().to_string(), value.trim().to_string())?;
        }
    }
    result.set("cookies", cookies_table)?;

    Ok(result)
}

fn create_client(options: &RequestOptions) -> mlua::Result<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::Client::builder()
        .cookie_store(true);

    if let Some(timeout) = options.timeout {
        builder = builder.timeout(timeout);
    }

    builder.build()
        .map_err(|e| mlua::Error::runtime(format!("Failed to create HTTP client: {}", e)))
}

fn apply_cookies(request: reqwest::blocking::RequestBuilder, cookies: &HashMap<String, String>) -> reqwest::blocking::RequestBuilder {
    if cookies.is_empty() {
        return request;
    }

    let cookie_header: String = cookies
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ");

    request.header("Cookie", cookie_header)
}

fn http_get(lua: &Lua, (url, options): (String, Option<Table>)) -> mlua::Result<Table> {
    let opts = options.map(|t| RequestOptions::from_table(&t))
        .transpose()?
        .unwrap_or_else(RequestOptions::empty);

    let client = create_client(&opts)?;
    let mut request = client.get(&url);

    for (key, value) in &opts.headers {
        request = request.header(key, value);
    }

    request = apply_cookies(request, &opts.cookies);

    let response = coppermoon_core::block_on(async {
        tokio::task::spawn_blocking(move || request.send())
            .await
            .map_err(|e| mlua::Error::runtime(format!("Task join error: {}", e)))?
            .map_err(|e| mlua::Error::runtime(format!("HTTP request failed: {}", e)))
    })?;

    build_response(lua, response)
}

fn http_post(lua: &Lua, (url, body, options): (String, Option<String>, Option<Table>)) -> mlua::Result<Table> {
    let mut opts = options.map(|t| RequestOptions::from_table(&t))
        .transpose()?
        .unwrap_or_else(RequestOptions::empty);

    if let Some(b) = body {
        opts.body = Some(b);
    }

    let client = create_client(&opts)?;
    let mut request = client.post(&url);

    for (key, value) in &opts.headers {
        request = request.header(key, value);
    }

    request = apply_cookies(request, &opts.cookies);

    if let Some(body) = &opts.body {
        request = request.body(body.clone());
    }

    let response = coppermoon_core::block_on(async {
        tokio::task::spawn_blocking(move || request.send())
            .await
            .map_err(|e| mlua::Error::runtime(format!("Task join error: {}", e)))?
            .map_err(|e| mlua::Error::runtime(format!("HTTP request failed: {}", e)))
    })?;

    build_response(lua, response)
}

fn http_put(lua: &Lua, (url, body, options): (String, Option<String>, Option<Table>)) -> mlua::Result<Table> {
    let mut opts = options.map(|t| RequestOptions::from_table(&t))
        .transpose()?
        .unwrap_or_else(RequestOptions::empty);

    if let Some(b) = body {
        opts.body = Some(b);
    }

    let client = create_client(&opts)?;
    let mut request = client.put(&url);

    for (key, value) in &opts.headers {
        request = request.header(key, value);
    }

    request = apply_cookies(request, &opts.cookies);

    if let Some(body) = &opts.body {
        request = request.body(body.clone());
    }

    let response = coppermoon_core::block_on(async {
        tokio::task::spawn_blocking(move || request.send())
            .await
            .map_err(|e| mlua::Error::runtime(format!("Task join error: {}", e)))?
            .map_err(|e| mlua::Error::runtime(format!("HTTP request failed: {}", e)))
    })?;

    build_response(lua, response)
}

fn http_delete(lua: &Lua, (url, options): (String, Option<Table>)) -> mlua::Result<Table> {
    let opts = options.map(|t| RequestOptions::from_table(&t))
        .transpose()?
        .unwrap_or_else(RequestOptions::empty);

    let client = create_client(&opts)?;
    let mut request = client.delete(&url);

    for (key, value) in &opts.headers {
        request = request.header(key, value);
    }

    request = apply_cookies(request, &opts.cookies);

    let response = coppermoon_core::block_on(async {
        tokio::task::spawn_blocking(move || request.send())
            .await
            .map_err(|e| mlua::Error::runtime(format!("Task join error: {}", e)))?
            .map_err(|e| mlua::Error::runtime(format!("HTTP request failed: {}", e)))
    })?;

    build_response(lua, response)
}

fn http_patch(lua: &Lua, (url, body, options): (String, Option<String>, Option<Table>)) -> mlua::Result<Table> {
    let mut opts = options.map(|t| RequestOptions::from_table(&t))
        .transpose()?
        .unwrap_or_else(RequestOptions::empty);

    if let Some(b) = body {
        opts.body = Some(b);
    }

    let client = create_client(&opts)?;
    let mut request = client.patch(&url);

    for (key, value) in &opts.headers {
        request = request.header(key, value);
    }

    request = apply_cookies(request, &opts.cookies);

    if let Some(body) = &opts.body {
        request = request.body(body.clone());
    }

    let response = coppermoon_core::block_on(async {
        tokio::task::spawn_blocking(move || request.send())
            .await
            .map_err(|e| mlua::Error::runtime(format!("Task join error: {}", e)))?
            .map_err(|e| mlua::Error::runtime(format!("HTTP request failed: {}", e)))
    })?;

    build_response(lua, response)
}

fn http_request(lua: &Lua, options: Table) -> mlua::Result<Table> {
    let method: String = options.get("method")
        .unwrap_or_else(|_| "GET".to_string());
    let url: String = options.get("url")
        .map_err(|_| mlua::Error::runtime("Missing 'url' in request options"))?;

    let opts = RequestOptions::from_table(&options)?;
    let client = create_client(&opts)?;

    let mut request = match method.to_uppercase().as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        "PATCH" => client.patch(&url),
        "HEAD" => client.head(&url),
        "OPTIONS" => client.request(reqwest::Method::OPTIONS, &url),
        _ => return Err(mlua::Error::runtime(format!("Unsupported HTTP method: {}", method))),
    };

    for (key, value) in &opts.headers {
        request = request.header(key, value);
    }

    request = apply_cookies(request, &opts.cookies);

    if let Some(body) = &opts.body {
        request = request.body(body.clone());
    }

    let response = coppermoon_core::block_on(async {
        tokio::task::spawn_blocking(move || request.send())
            .await
            .map_err(|e| mlua::Error::runtime(format!("Task join error: {}", e)))?
            .map_err(|e| mlua::Error::runtime(format!("HTTP request failed: {}", e)))
    })?;

    build_response(lua, response)
}

// HTTP Session with persistent cookies
use mlua::{UserData, UserDataMethods};
use std::sync::Mutex;

struct HttpSession {
    client: Arc<reqwest::blocking::Client>,
    cookies: Arc<Mutex<HashMap<String, String>>>,
}

impl UserData for HttpSession {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get", |lua, this, (url, options): (String, Option<Table>)| {
            session_request(lua, this, "GET", url, None, options)
        });

        methods.add_method("post", |lua, this, (url, body, options): (String, Option<String>, Option<Table>)| {
            session_request(lua, this, "POST", url, body, options)
        });

        methods.add_method("put", |lua, this, (url, body, options): (String, Option<String>, Option<Table>)| {
            session_request(lua, this, "PUT", url, body, options)
        });

        methods.add_method("delete", |lua, this, (url, options): (String, Option<Table>)| {
            session_request(lua, this, "DELETE", url, None, options)
        });

        methods.add_method("set_cookie", |_, this, (name, value): (String, String)| {
            let mut cookies = this.cookies.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            cookies.insert(name, value);
            Ok(())
        });

        methods.add_method("get_cookie", |_, this, name: String| {
            let cookies = this.cookies.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            Ok(cookies.get(&name).cloned())
        });

        methods.add_method("get_cookies", |lua, this, _: ()| {
            let cookies = this.cookies.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            let table = lua.create_table()?;
            for (k, v) in cookies.iter() {
                table.set(k.clone(), v.clone())?;
            }
            Ok(table)
        });

        methods.add_method("clear_cookies", |_, this, _: ()| {
            let mut cookies = this.cookies.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            cookies.clear();
            Ok(())
        });
    }
}

fn session_request(
    lua: &Lua,
    session: &HttpSession,
    method: &str,
    url: String,
    body: Option<String>,
    options: Option<Table>,
) -> mlua::Result<Table> {
    let opts = options.map(|t| RequestOptions::from_table(&t))
        .transpose()?
        .unwrap_or_else(RequestOptions::empty);

    // Get session cookies
    let session_cookies = session.cookies.lock()
        .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?
        .clone();

    // Merge cookies
    let mut all_cookies = session_cookies;
    for (k, v) in opts.cookies {
        all_cookies.insert(k, v);
    }

    let client = session.client.clone();
    let mut request = match method {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        "PATCH" => client.patch(&url),
        _ => return Err(mlua::Error::runtime(format!("Unsupported method: {}", method))),
    };

    for (key, value) in &opts.headers {
        request = request.header(key, value);
    }

    request = apply_cookies(request, &all_cookies);

    if let Some(b) = body.or(opts.body) {
        request = request.body(b);
    }

    let response = coppermoon_core::block_on(async {
        tokio::task::spawn_blocking(move || request.send())
            .await
            .map_err(|e| mlua::Error::runtime(format!("Task join error: {}", e)))?
            .map_err(|e| mlua::Error::runtime(format!("HTTP request failed: {}", e)))
    })?;

    // Extract Set-Cookie headers and update session
    for (key, value) in response.headers() {
        if key.as_str().to_lowercase() == "set-cookie" {
            if let Ok(v) = value.to_str() {
                if let Some((name_value, _rest)) = v.split_once(';') {
                    if let Some((name, val)) = name_value.split_once('=') {
                        let mut cookies = session.cookies.lock()
                            .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
                        cookies.insert(name.trim().to_string(), val.trim().to_string());
                    }
                }
            }
        }
    }

    build_response(lua, response)
}

fn create_session(_: &Lua, _: ()) -> mlua::Result<HttpSession> {
    let client = reqwest::blocking::Client::builder()
        .cookie_store(true)
        .build()
        .map_err(|e| mlua::Error::runtime(format!("Failed to create client: {}", e)))?;

    Ok(HttpSession {
        client: Arc::new(client),
        cookies: Arc::new(Mutex::new(HashMap::new())),
    })
}
