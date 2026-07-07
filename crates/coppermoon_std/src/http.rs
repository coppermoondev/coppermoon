//! HTTP client module for CopperMoon
//!
//! Provides HTTP client functionality for making web requests.
//!
//! All request functions are registered as async Lua functions: while a
//! request is in flight the event loop keeps running (other HTTP handlers,
//! timers, …). From Lua's perspective the API is still synchronous.

use coppermoon_core::Result;
use mlua::{Lua, Table};
use std::time::Duration;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

// ---------------------------------------------------------------------------
// Global connection-pooled HTTP client
// ---------------------------------------------------------------------------

static GLOBAL_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn global_client() -> &'static reqwest::Client {
    GLOBAL_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .expect("Failed to create global HTTP client")
    })
}

/// Register the http module.
///
/// Every request function is registered as a *hybrid* function (see
/// [`crate::hybrid_fn`]): in yieldable contexts the request suspends the
/// calling coroutine and the event loop keeps running; in non-yieldable
/// contexts it blocks like before.
pub fn register(lua: &Lua) -> Result<Table> {
    let http_table = lua.create_table()?;

    // http.get(url, options?) -> response
    http_table.set("get", hybrid(lua, get_impl)?)?;

    // http.post(url, body, options?) -> response
    http_table.set("post", hybrid(lua, post_impl)?)?;

    // http.put(url, body, options?) -> response
    http_table.set("put", hybrid(lua, put_impl)?)?;

    // http.delete(url, options?) -> response
    http_table.set("delete", hybrid(lua, delete_impl)?)?;

    // http.patch(url, body, options?) -> response
    http_table.set("patch", hybrid(lua, patch_impl)?)?;

    // http.request(options) -> response
    http_table.set("request", hybrid(lua, request_impl)?)?;

    // http.create_session() -> session (with cookie jar)
    http_table.set("create_session", lua.create_function(create_session)?)?;

    Ok(http_table)
}

/// Build a hybrid async/blocking Lua function from a single async
/// implementation. The blocking path simply drives the same future to
/// completion on the Tokio runtime.
fn hybrid<A, FR>(
    lua: &Lua,
    imp: fn(Lua, A) -> FR,
) -> mlua::Result<mlua::Function>
where
    A: mlua::FromLuaMulti + Send + 'static,
    FR: std::future::Future<Output = mlua::Result<Table>> + Send + 'static,
{
    let async_fn = lua.create_async_function(move |lua, args: A| imp(lua, args))?;
    let sync_fn = lua.create_function(move |lua, args: A| {
        coppermoon_core::block_on(imp(lua.clone(), args))
    })?;
    crate::hybrid_fn(lua, async_fn, sync_fn)
}

async fn get_impl(lua: Lua, (url, options): (String, Option<Table>)) -> mlua::Result<Table> {
    let opts = parse_options(options)?;
    send_request(lua, reqwest::Method::GET, url, opts).await
}

async fn post_impl(lua: Lua, (url, body, options): (String, Option<String>, Option<Table>)) -> mlua::Result<Table> {
    let mut opts = parse_options(options)?;
    if let Some(b) = body {
        opts.body = Some(b);
    }
    send_request(lua, reqwest::Method::POST, url, opts).await
}

async fn put_impl(lua: Lua, (url, body, options): (String, Option<String>, Option<Table>)) -> mlua::Result<Table> {
    let mut opts = parse_options(options)?;
    if let Some(b) = body {
        opts.body = Some(b);
    }
    send_request(lua, reqwest::Method::PUT, url, opts).await
}

async fn delete_impl(lua: Lua, (url, options): (String, Option<Table>)) -> mlua::Result<Table> {
    let opts = parse_options(options)?;
    send_request(lua, reqwest::Method::DELETE, url, opts).await
}

async fn patch_impl(lua: Lua, (url, body, options): (String, Option<String>, Option<Table>)) -> mlua::Result<Table> {
    let mut opts = parse_options(options)?;
    if let Some(b) = body {
        opts.body = Some(b);
    }
    send_request(lua, reqwest::Method::PATCH, url, opts).await
}

async fn request_impl(lua: Lua, options: Table) -> mlua::Result<Table> {
    let method_str: String = options.get("method")
        .unwrap_or_else(|_| "GET".to_string());
    let url: String = options.get("url")
        .map_err(|_| mlua::Error::runtime("Missing 'url' in request options"))?;

    let method = parse_method(&method_str)?;
    let opts = RequestOptions::from_table(&options)?;
    send_request(lua, method, url, opts).await
}

fn parse_method(method_str: &str) -> mlua::Result<reqwest::Method> {
    match method_str.to_uppercase().as_str() {
        "GET" => Ok(reqwest::Method::GET),
        "POST" => Ok(reqwest::Method::POST),
        "PUT" => Ok(reqwest::Method::PUT),
        "DELETE" => Ok(reqwest::Method::DELETE),
        "PATCH" => Ok(reqwest::Method::PATCH),
        "HEAD" => Ok(reqwest::Method::HEAD),
        "OPTIONS" => Ok(reqwest::Method::OPTIONS),
        _ => Err(mlua::Error::runtime(format!("Unsupported HTTP method: {}", method_str))),
    }
}

fn parse_options(options: Option<Table>) -> mlua::Result<RequestOptions> {
    options
        .map(|t| RequestOptions::from_table(&t))
        .transpose()
        .map(|o| o.unwrap_or_else(RequestOptions::empty))
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

async fn build_response(lua: &Lua, response: reqwest::Response) -> mlua::Result<Table> {
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

    let body = response.text().await
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

fn build_request(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    opts: &RequestOptions,
) -> reqwest::RequestBuilder {
    let mut request = client.request(method, url);

    if let Some(timeout) = opts.timeout {
        request = request.timeout(timeout);
    }

    for (key, value) in &opts.headers {
        request = request.header(key, value);
    }

    request = apply_cookies(request, &opts.cookies);

    if let Some(body) = &opts.body {
        request = request.body(body.clone());
    }

    request
}

fn apply_cookies(request: reqwest::RequestBuilder, cookies: &HashMap<String, String>) -> reqwest::RequestBuilder {
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

async fn send_request(
    lua: Lua,
    method: reqwest::Method,
    url: String,
    opts: RequestOptions,
) -> mlua::Result<Table> {
    let request = build_request(global_client(), method, &url, &opts);

    let response = request.send().await
        .map_err(|e| mlua::Error::runtime(format!("HTTP request failed: {}", e)))?;

    build_response(&lua, response).await
}

// HTTP Session with persistent cookies
use mlua::{UserData, UserDataMethods};
use std::sync::Mutex;

struct HttpSession {
    client: reqwest::Client,
    cookies: Arc<Mutex<HashMap<String, String>>>,
}

impl UserData for HttpSession {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("get", |lua, this, (url, options): (String, Option<Table>)| async move {
            session_request(lua, &this, "GET", url, None, options).await
        });

        methods.add_async_method("post", |lua, this, (url, body, options): (String, Option<String>, Option<Table>)| async move {
            session_request(lua, &this, "POST", url, body, options).await
        });

        methods.add_async_method("put", |lua, this, (url, body, options): (String, Option<String>, Option<Table>)| async move {
            session_request(lua, &this, "PUT", url, body, options).await
        });

        methods.add_async_method("delete", |lua, this, (url, options): (String, Option<Table>)| async move {
            session_request(lua, &this, "DELETE", url, None, options).await
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

async fn session_request(
    lua: Lua,
    session: &HttpSession,
    method: &str,
    url: String,
    body: Option<String>,
    options: Option<Table>,
) -> mlua::Result<Table> {
    let mut opts = parse_options(options)?;

    // Merge session cookies into opts (guard dropped before any await)
    let session_cookies = session.cookies.lock()
        .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?
        .clone();
    for (k, v) in session_cookies {
        opts.cookies.entry(k).or_insert(v);
    }

    if let Some(b) = body {
        opts.body = Some(b);
    }

    let req_method = parse_method(method)?;
    let request = build_request(&session.client, req_method, &url, &opts);

    let response = request.send().await
        .map_err(|e| mlua::Error::runtime(format!("HTTP request failed: {}", e)))?;

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

    build_response(&lua, response).await
}

fn create_session(_: &Lua, _: ()) -> mlua::Result<HttpSession> {
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .map_err(|e| mlua::Error::runtime(format!("Failed to create client: {}", e)))?;

    Ok(HttpSession {
        client,
        cookies: Arc::new(Mutex::new(HashMap::new())),
    })
}
