//! WebSocket module for CopperMoon
//!
//! Provides WebSocket client and server capabilities via `net.ws`.

use coppermoon_core::Result;
use mlua::{Lua, Table, UserData, UserDataMethods};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, WebSocket};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::Message;

// ============ WsStream (unified client/server) ============

enum WsStream {
    Client(WebSocket<MaybeTlsStream<TcpStream>>),
    Server(WebSocket<TcpStream>),
}

impl WsStream {
    fn read(&mut self) -> tungstenite::Result<Message> {
        match self {
            WsStream::Client(ws) => ws.read(),
            WsStream::Server(ws) => ws.read(),
        }
    }

    fn send(&mut self, msg: Message) -> tungstenite::Result<()> {
        match self {
            WsStream::Client(ws) => ws.send(msg),
            WsStream::Server(ws) => ws.send(msg),
        }
    }

    fn close(&mut self, frame: Option<CloseFrame>) -> tungstenite::Result<()> {
        match self {
            WsStream::Client(ws) => ws.close(frame),
            WsStream::Server(ws) => ws.close(frame),
        }
    }

    fn can_read(&self) -> bool {
        match self {
            WsStream::Client(ws) => ws.can_read(),
            WsStream::Server(ws) => ws.can_read(),
        }
    }

    fn set_read_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        match self {
            WsStream::Client(ws) => match ws.get_ref() {
                MaybeTlsStream::Plain(s) => s.set_read_timeout(timeout),
                MaybeTlsStream::NativeTls(s) => s.get_ref().set_read_timeout(timeout),
                _ => Ok(()),
            },
            WsStream::Server(ws) => ws.get_ref().set_read_timeout(timeout),
        }
    }

    fn set_write_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        match self {
            WsStream::Client(ws) => match ws.get_ref() {
                MaybeTlsStream::Plain(s) => s.set_write_timeout(timeout),
                MaybeTlsStream::NativeTls(s) => s.get_ref().set_write_timeout(timeout),
                _ => Ok(()),
            },
            WsStream::Server(ws) => ws.get_ref().set_write_timeout(timeout),
        }
    }

    fn peer_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        match self {
            WsStream::Client(ws) => match ws.get_ref() {
                MaybeTlsStream::Plain(s) => s.peer_addr(),
                MaybeTlsStream::NativeTls(s) => s.get_ref().peer_addr(),
                _ => Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "unsupported stream type",
                )),
            },
            WsStream::Server(ws) => ws.get_ref().peer_addr(),
        }
    }

    fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        match self {
            WsStream::Client(ws) => match ws.get_ref() {
                MaybeTlsStream::Plain(s) => s.local_addr(),
                MaybeTlsStream::NativeTls(s) => s.get_ref().local_addr(),
                _ => Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "unsupported stream type",
                )),
            },
            WsStream::Server(ws) => ws.get_ref().local_addr(),
        }
    }
}

// ============ WsConnection ============

struct WsConnection {
    ws: Arc<Mutex<WsStream>>,
}

impl UserData for WsConnection {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ws:send(data, type?)
        methods.add_method("send", |_, this, (data, msg_type): (mlua::String, Option<String>)| {
            let mut ws = this.ws.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let msg_type = msg_type.unwrap_or_else(|| "text".to_string());
            let message = match msg_type.as_str() {
                "text" => {
                    let text = data.to_str()
                        .map_err(|e| mlua::Error::runtime(format!("Invalid UTF-8: {}", e)))?;
                    Message::Text(text.to_string().into())
                }
                "binary" => Message::Binary(data.as_bytes().to_vec().into()),
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "Invalid message type '{}': expected 'text' or 'binary'",
                        other
                    )));
                }
            };

            ws.send(message)
                .map_err(|e| mlua::Error::runtime(format!("WebSocket send error: {}", e)))?;

            Ok(())
        });

        // ws:recv() -> table | nil
        methods.add_method("recv", |lua, this, _: ()| {
            let mut ws = this.ws.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            if !ws.can_read() {
                return Ok(mlua::Value::Nil);
            }

            let msg = match ws.read() {
                Ok(msg) => msg,
                Err(tungstenite::Error::ConnectionClosed) => return Ok(mlua::Value::Nil),
                Err(tungstenite::Error::AlreadyClosed) => return Ok(mlua::Value::Nil),
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    return Err(mlua::Error::runtime("WebSocket recv timeout"));
                }
                Err(e) => {
                    return Err(mlua::Error::runtime(format!("WebSocket recv error: {}", e)));
                }
            };

            let table = lua.create_table()?;

            match msg {
                Message::Text(text) => {
                    table.set("type", "text")?;
                    table.set("data", text.to_string())?;
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
                        table.set("reason", cf.reason.to_string())?;
                        table.set("data", cf.reason.to_string())?;
                    } else {
                        table.set("code", 1005)?;
                        table.set("reason", "")?;
                        table.set("data", "")?;
                    }
                }
                Message::Frame(_) => {
                    return Ok(mlua::Value::Nil);
                }
            }

            Ok(mlua::Value::Table(table))
        });

        // ws:ping(data?)
        methods.add_method("ping", |_, this, data: Option<mlua::String>| {
            let mut ws = this.ws.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let payload = data
                .map(|d| d.as_bytes().to_vec())
                .unwrap_or_default();
            ws.send(Message::Ping(payload.into()))
                .map_err(|e| mlua::Error::runtime(format!("WebSocket ping error: {}", e)))?;

            Ok(())
        });

        // ws:pong(data?)
        methods.add_method("pong", |_, this, data: Option<mlua::String>| {
            let mut ws = this.ws.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let payload = data
                .map(|d| d.as_bytes().to_vec())
                .unwrap_or_default();
            ws.send(Message::Pong(payload.into()))
                .map_err(|e| mlua::Error::runtime(format!("WebSocket pong error: {}", e)))?;

            Ok(())
        });

        // ws:close(code?, reason?)
        methods.add_method("close", |_, this, (code, reason): (Option<u16>, Option<String>)| {
            let mut ws = this.ws.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let frame = Some(CloseFrame {
                code: CloseCode::from(code.unwrap_or(1000)),
                reason: reason.unwrap_or_default().into(),
            });

            ws.close(frame)
                .map_err(|e| mlua::Error::runtime(format!("WebSocket close error: {}", e)))?;

            Ok(())
        });

        // ws:set_timeout(ms?)
        methods.add_method("set_timeout", |_, this, ms: Option<u64>| {
            let ws = this.ws.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let timeout = ms.map(Duration::from_millis);
            ws.set_read_timeout(timeout)
                .map_err(|e| mlua::Error::runtime(format!("Set timeout error: {}", e)))?;
            ws.set_write_timeout(timeout)
                .map_err(|e| mlua::Error::runtime(format!("Set timeout error: {}", e)))?;

            Ok(())
        });

        // ws:peer_addr() -> string
        methods.add_method("peer_addr", |_, this, _: ()| {
            let ws = this.ws.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let addr = ws.peer_addr()
                .map_err(|e| mlua::Error::runtime(format!("Peer addr error: {}", e)))?;

            Ok(addr.to_string())
        });

        // ws:local_addr() -> string
        methods.add_method("local_addr", |_, this, _: ()| {
            let ws = this.ws.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let addr = ws.local_addr()
                .map_err(|e| mlua::Error::runtime(format!("Local addr error: {}", e)))?;

            Ok(addr.to_string())
        });
    }
}

// ============ WsServer ============

struct WsServer {
    listener: Arc<Mutex<TcpListener>>,
}

impl UserData for WsServer {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // server:accept() -> WsConnection
        methods.add_method("accept", |_, this, _: ()| {
            let listener = this.listener.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let (stream, _addr) = listener.accept()
                .map_err(|e| mlua::Error::runtime(format!("Accept error: {}", e)))?;

            let ws = tungstenite::accept(stream)
                .map_err(|e| mlua::Error::runtime(format!("WebSocket accept error: {}", e)))?;

            Ok(WsConnection {
                ws: Arc::new(Mutex::new(WsStream::Server(ws))),
            })
        });

        // server:local_addr() -> string
        methods.add_method("local_addr", |_, this, _: ()| {
            let listener = this.listener.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let addr = listener.local_addr()
                .map_err(|e| mlua::Error::runtime(format!("Local addr error: {}", e)))?;

            Ok(addr.to_string())
        });

        // server:set_nonblocking(bool)
        methods.add_method("set_nonblocking", |_, this, nonblocking: bool| {
            let listener = this.listener.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            listener.set_nonblocking(nonblocking)
                .map_err(|e| mlua::Error::runtime(format!("Set nonblocking error: {}", e)))?;

            Ok(())
        });
    }
}

// ============ Module functions ============

fn ws_connect(
    _: &Lua,
    (url, options): (String, Option<Table>),
) -> mlua::Result<WsConnection> {
    // Parse optional headers
    let mut custom_headers: Vec<(String, String)> = Vec::new();
    if let Some(ref opts) = options {
        if let Ok(headers_table) = opts.get::<Table>("headers") {
            for pair in headers_table.pairs::<String, String>() {
                if let Ok((k, v)) = pair {
                    custom_headers.push((k, v));
                }
            }
        }
    }

    let ws = if custom_headers.is_empty() {
        let (ws, _response) = tungstenite::connect(&url)
            .map_err(|e| mlua::Error::runtime(format!("WebSocket connect error: {}", e)))?;
        ws
    } else {
        use tungstenite::http::Request;

        let mut builder = Request::builder().uri(&url);
        // Add required WebSocket headers
        let uri: tungstenite::http::Uri = url
            .parse()
            .map_err(|e| mlua::Error::runtime(format!("Invalid URL: {}", e)))?;
        let host = uri
            .host()
            .ok_or_else(|| mlua::Error::runtime("URL missing host"))?;
        let host_header = if let Some(port) = uri.port() {
            format!("{}:{}", host, port)
        } else {
            host.to_string()
        };
        builder = builder.header("Host", &host_header);
        builder = builder.header("Connection", "Upgrade");
        builder = builder.header("Upgrade", "websocket");
        builder = builder.header("Sec-WebSocket-Version", "13");

        // Generate a random key
        let key = tungstenite::handshake::client::generate_key();
        builder = builder.header("Sec-WebSocket-Key", &key);

        for (k, v) in &custom_headers {
            builder = builder.header(k.as_str(), v.as_str());
        }

        let request = builder
            .body(())
            .map_err(|e| mlua::Error::runtime(format!("Request build error: {}", e)))?;

        let (ws, _response) = tungstenite::connect(request)
            .map_err(|e| mlua::Error::runtime(format!("WebSocket connect error: {}", e)))?;
        ws
    };

    let connection = WsConnection {
        ws: Arc::new(Mutex::new(WsStream::Client(ws))),
    };

    // Apply timeout option
    if let Some(ref opts) = options {
        if let Ok(ms) = opts.get::<u64>("timeout") {
            let timeout = Some(Duration::from_millis(ms));
            let ws = connection.ws.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            ws.set_read_timeout(timeout)
                .map_err(|e| mlua::Error::runtime(format!("Set timeout error: {}", e)))?;
            ws.set_write_timeout(timeout)
                .map_err(|e| mlua::Error::runtime(format!("Set timeout error: {}", e)))?;
        }
    }

    Ok(connection)
}

fn ws_listen(
    _: &Lua,
    (host, port): (Option<String>, u16),
) -> mlua::Result<WsServer> {
    let host = host.unwrap_or_else(|| "0.0.0.0".to_string());
    let addr = format!("{}:{}", host, port);

    let listener = TcpListener::bind(&addr)
        .map_err(|e| mlua::Error::runtime(format!("Bind error: {}", e)))?;

    Ok(WsServer {
        listener: Arc::new(Mutex::new(listener)),
    })
}

// ============ Registration ============

pub fn register(lua: &Lua) -> Result<Table> {
    let ws_table = lua.create_table()?;

    ws_table.set("connect", lua.create_function(ws_connect)?)?;
    ws_table.set("listen", lua.create_function(ws_listen)?)?;

    Ok(ws_table)
}
