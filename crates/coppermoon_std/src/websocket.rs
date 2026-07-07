//! WebSocket module for CopperMoon
//!
//! Async WebSocket client and server via `net.ws`, built on
//! `tokio-tungstenite`. Every I/O method is a true async Lua method that
//! suspends its coroutine while awaiting, so the event loop keeps running
//! (HTTP handlers, timers, other sockets…).
//!
//! The stream is `split()` into independent read and write halves behind
//! separate mutexes: a blocked `recv()` never prevents a concurrent `send()`
//! (WebSocket is full-duplex).

use coppermoon_core::Result;
use futures_util::{SinkExt, StreamExt};
use mlua::{Lua, Table, UserData, UserDataMethods, Value};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex as TokioMutex;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{accept_async, connect_async, MaybeTlsStream, WebSocketStream};

/// Shared, per-connection I/O timeout (set via `set_timeout`).
type TimeoutCell = Arc<StdMutex<Option<Duration>>>;

/// Boxed, type-erased read and write halves so client (TLS-capable) and
/// server connections share one `WsConnection` type.
type WsSink = Pin<Box<dyn futures_util::Sink<Message, Error = WsError> + Send>>;
type WsRx = Pin<Box<dyn futures_util::Stream<Item = std::result::Result<Message, WsError>> + Send>>;

fn get_timeout(cell: &TimeoutCell) -> mlua::Result<Option<Duration>> {
    Ok(*cell
        .lock()
        .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?)
}

/// Await a WebSocket future, applying the configured timeout (if any).
async fn ws_io<F, T>(timeout: Option<Duration>, what: &str, fut: F) -> mlua::Result<T>
where
    F: std::future::Future<Output = std::result::Result<T, WsError>>,
{
    let result = match timeout {
        Some(d) => tokio::time::timeout(d, fut)
            .await
            .map_err(|_| mlua::Error::runtime(format!("{} error: timed out", what)))?,
        None => fut.await,
    };
    result.map_err(|e| mlua::Error::runtime(format!("{} error: {}", what, e)))
}

// ============ WsConnection ============

struct WsConnection {
    sink: Arc<TokioMutex<WsSink>>,
    stream: Arc<TokioMutex<WsRx>>,
    timeout: TimeoutCell,
    peer: Option<String>,
    local: Option<String>,
}

impl WsConnection {
    /// Wrap a handshaked WebSocket stream, splitting it into read/write halves.
    fn new<S>(ws: WebSocketStream<S>, peer: Option<String>, local: Option<String>) -> Self
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let (sink, stream) = ws.split();
        Self {
            sink: Arc::new(TokioMutex::new(Box::pin(sink))),
            stream: Arc::new(TokioMutex::new(Box::pin(stream))),
            timeout: Arc::new(StdMutex::new(None)),
            peer,
            local,
        }
    }
}

/// Convert an incoming message into the Lua table `{ type, data, ... }`.
/// Returns `Nil` for raw frames (never surfaced to Lua).
fn message_to_table(lua: &Lua, msg: Message) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    match msg {
        Message::Text(text) => {
            table.set("type", "text")?;
            table.set("data", text.as_str())?;
        }
        Message::Binary(bytes) => {
            table.set("type", "binary")?;
            table.set("data", lua.create_string(&bytes[..])?)?;
        }
        Message::Ping(bytes) => {
            table.set("type", "ping")?;
            table.set("data", lua.create_string(&bytes[..])?)?;
        }
        Message::Pong(bytes) => {
            table.set("type", "pong")?;
            table.set("data", lua.create_string(&bytes[..])?)?;
        }
        Message::Close(frame) => {
            table.set("type", "close")?;
            if let Some(cf) = frame {
                let code: u16 = cf.code.into();
                table.set("code", code)?;
                table.set("reason", cf.reason.as_str())?;
                table.set("data", cf.reason.as_str())?;
            } else {
                table.set("code", 1005)?;
                table.set("reason", "")?;
                table.set("data", "")?;
            }
        }
        Message::Frame(_) => return Ok(Value::Nil),
    }
    Ok(Value::Table(table))
}

impl UserData for WsConnection {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // INVARIANT (see net.rs): userdata borrows are exclusive; clone the
        // Arcs and drop `this` before the first `.await`.

        // ws:send(data, type?) — type is "text" (default) or "binary"
        methods.add_async_method(
            "send",
            |_, this, (data, msg_type): (mlua::String, Option<String>)| {
                let sink = Arc::clone(&this.sink);
                let timeout = get_timeout(&this.timeout);
                let msg_type = msg_type.unwrap_or_else(|| "text".to_string());
                let bytes = data.as_bytes().to_vec();
                drop(this);
                async move {
                    let timeout = timeout?;
                    let message = match msg_type.as_str() {
                        "text" => {
                            let text = String::from_utf8(bytes).map_err(|_| {
                                mlua::Error::runtime("send: text message is not valid UTF-8")
                            })?;
                            Message::Text(text.into())
                        }
                        "binary" => Message::Binary(bytes.into()),
                        other => {
                            return Err(mlua::Error::runtime(format!(
                                "Invalid message type '{}': expected 'text' or 'binary'",
                                other
                            )))
                        }
                    };
                    let mut sink = sink.lock().await;
                    ws_io(timeout, "WebSocket send", sink.send(message)).await
                }
            },
        );

        // ws:recv() -> { type, data, ... } | nil (nil when the stream ends)
        methods.add_async_method("recv", |lua, this, _: ()| {
            let stream = Arc::clone(&this.stream);
            let timeout = get_timeout(&this.timeout);
            drop(this);
            async move {
                let timeout = timeout?;
                let mut stream = stream.lock().await;
                let item = match timeout {
                    Some(d) => tokio::time::timeout(d, stream.next())
                        .await
                        .map_err(|_| mlua::Error::runtime("WebSocket recv error: timed out"))?,
                    None => stream.next().await,
                };
                match item {
                    None => Ok(Value::Nil),
                    Some(Ok(msg)) => message_to_table(&lua, msg),
                    Some(Err(WsError::ConnectionClosed))
                    | Some(Err(WsError::AlreadyClosed)) => Ok(Value::Nil),
                    Some(Err(e)) => {
                        Err(mlua::Error::runtime(format!("WebSocket recv error: {}", e)))
                    }
                }
            }
        });

        // ws:ping(data?)
        methods.add_async_method("ping", |_, this, data: Option<mlua::String>| {
            let sink = Arc::clone(&this.sink);
            let timeout = get_timeout(&this.timeout);
            let payload = data.map(|d| d.as_bytes().to_vec()).unwrap_or_default();
            drop(this);
            async move {
                let timeout = timeout?;
                let mut sink = sink.lock().await;
                ws_io(timeout, "WebSocket ping", sink.send(Message::Ping(payload.into()))).await
            }
        });

        // ws:pong(data?)
        methods.add_async_method("pong", |_, this, data: Option<mlua::String>| {
            let sink = Arc::clone(&this.sink);
            let timeout = get_timeout(&this.timeout);
            let payload = data.map(|d| d.as_bytes().to_vec()).unwrap_or_default();
            drop(this);
            async move {
                let timeout = timeout?;
                let mut sink = sink.lock().await;
                ws_io(timeout, "WebSocket pong", sink.send(Message::Pong(payload.into()))).await
            }
        });

        // ws:close(code?, reason?)
        methods.add_async_method(
            "close",
            |_, this, (code, reason): (Option<u16>, Option<String>)| {
                let sink = Arc::clone(&this.sink);
                let timeout = get_timeout(&this.timeout);
                drop(this);
                async move {
                    let timeout = timeout?;
                    let frame = CloseFrame {
                        code: CloseCode::from(code.unwrap_or(1000)),
                        reason: reason.unwrap_or_default().into(),
                    };
                    let mut sink = sink.lock().await;
                    // Send the close frame, then close the write half.
                    ws_io(timeout, "WebSocket close", sink.send(Message::Close(Some(frame))))
                        .await?;
                    ws_io(timeout, "WebSocket close", sink.close()).await
                }
            },
        );

        // ws:set_timeout(ms?) — applies to subsequent send/recv operations
        methods.add_method("set_timeout", |_, this, ms: Option<u64>| {
            let mut timeout = this
                .timeout
                .lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            *timeout = ms.map(Duration::from_millis);
            Ok(())
        });

        // ws:peer_addr() -> string
        methods.add_method("peer_addr", |_, this, _: ()| {
            this.peer
                .clone()
                .ok_or_else(|| mlua::Error::runtime("Peer address unavailable"))
        });

        // ws:local_addr() -> string
        methods.add_method("local_addr", |_, this, _: ()| {
            this.local
                .clone()
                .ok_or_else(|| mlua::Error::runtime("Local address unavailable"))
        });
    }
}

// ============ WsServer ============

struct WsServer {
    listener: Arc<TcpListener>,
}

impl UserData for WsServer {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // server:accept() -> connection, peer_ip, peer_port
        methods.add_async_method("accept", |_, this, _: ()| {
            let listener = Arc::clone(&this.listener);
            drop(this);
            async move {
                let (stream, addr) = listener
                    .accept()
                    .await
                    .map_err(|e| mlua::Error::runtime(format!("Accept error: {}", e)))?;
                let conn = accept_stream(stream, addr).await?;
                Ok((conn, addr.ip().to_string(), addr.port()))
            }
        });

        // server:serve(handler) — accept loop; each client's handshake and
        // its handler(conn, ip, port) run on their own coroutine, so a slow
        // handshake never blocks accept. Never returns.
        methods.add_async_method("serve", |lua, this, handler: mlua::Function| {
            let listener = Arc::clone(&this.listener);
            drop(this);
            async move {
                loop {
                    let (stream, addr) = match listener.accept().await {
                        Ok(pair) => pair,
                        Err(e) => {
                            eprintln!("Accept error: {}", e);
                            continue;
                        }
                    };
                    let handler = handler.clone();
                    let lua = lua.clone();
                    tokio::task::spawn_local(async move {
                        let conn = match accept_stream(stream, addr).await {
                            Ok(c) => c,
                            Err(e) => {
                                eprintln!("WebSocket handshake error ({}): {}", addr, e);
                                return;
                            }
                        };
                        let ud = match lua.create_userdata(conn) {
                            Ok(ud) => ud,
                            Err(_) => return,
                        };
                        let args = (ud, addr.ip().to_string(), addr.port());
                        if let Err(e) = handler.call_async::<()>(args).await {
                            coppermoon_core::uncaught::report(&lua, "net.ws serve", &e).await;
                        }
                    });
                }
                #[allow(unreachable_code)]
                Ok::<(), mlua::Error>(())
            }
        });

        // server:local_addr() -> string
        methods.add_method("local_addr", |_, this, _: ()| {
            let addr = this
                .listener
                .local_addr()
                .map_err(|e| mlua::Error::runtime(format!("Local addr error: {}", e)))?;
            Ok(addr.to_string())
        });

        // Kept for API compatibility; tokio sockets are always non-blocking.
        methods.add_method("set_nonblocking", |_, _this, _nonblocking: bool| Ok(()));
    }
}

/// Perform the server-side WebSocket handshake on an accepted TCP stream.
async fn accept_stream(stream: TcpStream, peer: SocketAddr) -> mlua::Result<WsConnection> {
    let local = stream.local_addr().ok().map(|a| a.to_string());
    let ws = accept_async(stream)
        .await
        .map_err(|e| mlua::Error::runtime(format!("WebSocket accept error: {}", e)))?;
    Ok(WsConnection::new(ws, Some(peer.to_string()), local))
}

// ============ Module functions ============

/// Extract (peer, local) addresses from a client stream when it is a plain
/// TCP connection; `wss://` (TLS) connections report them as unavailable.
fn client_addrs(s: &MaybeTlsStream<TcpStream>) -> (Option<String>, Option<String>) {
    match s {
        MaybeTlsStream::Plain(t) => (
            t.peer_addr().ok().map(|a| a.to_string()),
            t.local_addr().ok().map(|a| a.to_string()),
        ),
        _ => (None, None),
    }
}

async fn ws_connect(lua: Lua, (url, options): (String, Option<Table>)) -> mlua::Result<WsConnection> {
    // Optional custom headers → build an explicit upgrade request.
    let mut custom_headers: Vec<(String, String)> = Vec::new();
    let mut timeout_ms: Option<u64> = None;
    if let Some(ref opts) = options {
        if let Ok(headers_table) = opts.get::<Table>("headers") {
            for pair in headers_table.pairs::<String, String>() {
                if let Ok((k, v)) = pair {
                    custom_headers.push((k, v));
                }
            }
        }
        timeout_ms = opts.get::<u64>("timeout").ok();
    }

    let (ws, _resp) = if custom_headers.is_empty() {
        connect_async(&url)
            .await
            .map_err(|e| mlua::Error::runtime(format!("WebSocket connect error: {}", e)))?
    } else {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|e| mlua::Error::runtime(format!("Invalid URL: {}", e)))?;
        let headers = request.headers_mut();
        for (k, v) in &custom_headers {
            let name = tokio_tungstenite::tungstenite::http::header::HeaderName::from_bytes(
                k.as_bytes(),
            )
            .map_err(|e| mlua::Error::runtime(format!("Invalid header name '{}': {}", k, e)))?;
            let value = v
                .parse()
                .map_err(|e| mlua::Error::runtime(format!("Invalid header value '{}': {}", v, e)))?;
            headers.insert(name, value);
        }
        connect_async(request)
            .await
            .map_err(|e| mlua::Error::runtime(format!("WebSocket connect error: {}", e)))?
    };

    let (peer, local) = client_addrs(ws.get_ref());
    let conn = WsConnection::new(ws, peer, local);
    if let Some(ms) = timeout_ms {
        *conn
            .timeout
            .lock()
            .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))? =
            Some(Duration::from_millis(ms));
    }
    let _ = lua;
    Ok(conn)
}

async fn ws_listen(_: Lua, (host, port): (Option<String>, u16)) -> mlua::Result<WsServer> {
    let host = host.unwrap_or_else(|| "0.0.0.0".to_string());
    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| mlua::Error::runtime(format!("Bind error: {}", e)))?;
    Ok(WsServer {
        listener: Arc::new(listener),
    })
}

// ============ Registration ============

pub fn register(lua: &Lua) -> Result<Table> {
    let ws_table = lua.create_table()?;
    ws_table.set("connect", lua.create_async_function(ws_connect)?)?;
    ws_table.set("listen", lua.create_async_function(ws_listen)?)?;
    Ok(ws_table)
}
