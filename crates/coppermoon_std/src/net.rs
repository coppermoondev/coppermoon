//! Network module for CopperMoon
//!
//! Provides low-level TCP and UDP networking capabilities.
//! Built on Tokio's async sockets: every I/O operation is a true async
//! Lua function/method that suspends its coroutine while awaiting, so the
//! event loop keeps running (HTTP handlers, timers, other sockets…).

use coppermoon_core::Result;
use mlua::{Lua, Table, UserData, UserDataMethods};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::Mutex as TokioMutex;

/// Shared, cheaply-clonable timeout cell (set via `set_timeout`, read before
/// each I/O operation). Guard is never held across an await.
type TimeoutCell = Arc<StdMutex<Option<Duration>>>;

fn get_timeout(cell: &TimeoutCell) -> mlua::Result<Option<Duration>> {
    Ok(*cell
        .lock()
        .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?)
}

/// Run an async I/O operation, applying the configured timeout (if any).
/// `what` is the error-message prefix ("Read", "Write", …) so messages keep
/// the same shape as before ("Read error: …").
async fn io_with_timeout<F, T>(
    timeout: Option<Duration>,
    what: &str,
    fut: F,
) -> mlua::Result<T>
where
    F: std::future::Future<Output = std::io::Result<T>>,
{
    let result = match timeout {
        Some(duration) => tokio::time::timeout(duration, fut)
            .await
            .map_err(|_| mlua::Error::runtime(format!("{} error: timed out", what)))?,
        None => fut.await,
    };
    result.map_err(|e| mlua::Error::runtime(format!("{} error: {}", what, e)))
}

/// Register the net module
pub fn register(lua: &Lua) -> Result<Table> {
    let net_table = lua.create_table()?;

    // TCP sub-module
    let tcp_table = lua.create_table()?;
    tcp_table.set("connect", lua.create_async_function(tcp_connect)?)?;
    tcp_table.set("listen", lua.create_async_function(tcp_listen)?)?;
    net_table.set("tcp", tcp_table)?;

    // UDP sub-module
    let udp_table = lua.create_table()?;
    udp_table.set("bind", lua.create_async_function(udp_bind)?)?;
    net_table.set("udp", udp_table)?;

    // Utility functions
    net_table.set("resolve", lua.create_async_function(net_resolve)?)?;

    Ok(net_table)
}

// ============ TCP Client ============

struct TcpConnection {
    /// The stream is wrapped in a `BufReader` so `read_line` keeps its
    /// buffered bytes between calls (tokio's `BufReader` forwards
    /// `AsyncWrite` to the inner stream, so writes go through unchanged).
    ///
    /// A `tokio::sync::Mutex` is required because the guard is held across
    /// awaits (std guards are not `Send`).
    stream: Arc<TokioMutex<BufReader<TcpStream>>>,
    timeout: TimeoutCell,
}

impl TcpConnection {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream: Arc::new(TokioMutex::new(BufReader::new(stream))),
            timeout: Arc::new(StdMutex::new(None)),
        }
    }
}

impl UserData for TcpConnection {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // INVARIANT: mlua userdata borrows are exclusive. Holding `this`
        // (a `UserDataRef`) across an `.await` blocks every other coroutine
        // that touches the same object ("error borrowing userdata", or a
        // deadlock: accept() pending while another coroutine calls a method
        // on the same userdata). Every async method below therefore clones
        // the needed `Arc`s and drops `this` BEFORE the first `.await`.

        // conn:read(n) -> string
        methods.add_async_method("read", |lua, this, n: Option<usize>| {
            let stream = Arc::clone(&this.stream);
            let timeout = get_timeout(&this.timeout);
            drop(this);
            async move {
                let timeout = timeout?;
                let n = n.unwrap_or(4096);
                let mut stream = stream.lock().await;
                let mut buffer = vec![0u8; n];
                let bytes_read =
                    io_with_timeout(timeout, "Read", stream.read(&mut buffer)).await?;
                buffer.truncate(bytes_read);
                lua.create_string(&buffer)
            }
        });

        // conn:read_line() -> string
        methods.add_async_method("read_line", |_lua, this, _: ()| {
            let stream = Arc::clone(&this.stream);
            let timeout = get_timeout(&this.timeout);
            drop(this);
            async move {
                let timeout = timeout?;
                let mut stream = stream.lock().await;
                let mut line = String::new();
                io_with_timeout(timeout, "Read", stream.read_line(&mut line)).await?;
                Ok(line)
            }
        });

        // conn:read_all() -> string
        methods.add_async_method("read_all", |lua, this, _: ()| {
            let stream = Arc::clone(&this.stream);
            let timeout = get_timeout(&this.timeout);
            drop(this);
            async move {
                let timeout = timeout?;
                let mut stream = stream.lock().await;
                let mut buffer = Vec::new();
                io_with_timeout(timeout, "Read", stream.read_to_end(&mut buffer)).await?;
                lua.create_string(&buffer)
            }
        });

        // conn:write(data) -> bytes_written
        methods.add_async_method("write", |_, this, data: mlua::String| {
            let stream = Arc::clone(&this.stream);
            let timeout = get_timeout(&this.timeout);
            drop(this);
            async move {
                let timeout = timeout?;
                let bytes = data.as_bytes().to_vec();
                let mut stream = stream.lock().await;
                let written =
                    io_with_timeout(timeout, "Write", stream.write(&bytes)).await?;
                Ok(written)
            }
        });

        // conn:write_all(data)
        methods.add_async_method("write_all", |_, this, data: mlua::String| {
            let stream = Arc::clone(&this.stream);
            let timeout = get_timeout(&this.timeout);
            drop(this);
            async move {
                let timeout = timeout?;
                let bytes = data.as_bytes().to_vec();
                let mut stream = stream.lock().await;
                io_with_timeout(timeout, "Write", stream.write_all(&bytes)).await?;
                Ok(())
            }
        });

        // conn:flush()
        methods.add_async_method("flush", |_, this, _: ()| {
            let stream = Arc::clone(&this.stream);
            let timeout = get_timeout(&this.timeout);
            drop(this);
            async move {
                let timeout = timeout?;
                let mut stream = stream.lock().await;
                io_with_timeout(timeout, "Flush", stream.flush()).await?;
                Ok(())
            }
        });

        // conn:close()
        methods.add_async_method("close", |_, this, _: ()| {
            let stream = Arc::clone(&this.stream);
            drop(this);
            async move {
                let mut stream = stream.lock().await;
                // Graceful shutdown of the write side; the socket itself is
                // closed when the userdata is garbage-collected.
                stream
                    .shutdown()
                    .await
                    .map_err(|e| mlua::Error::runtime(format!("Close error: {}", e)))?;
                Ok(())
            }
        });

        // conn:set_timeout(ms) — applies to subsequent read/write operations
        methods.add_method("set_timeout", |_, this, ms: Option<u64>| {
            let mut timeout = this
                .timeout
                .lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            *timeout = ms.map(Duration::from_millis);
            Ok(())
        });

        // conn:peer_addr() -> string
        methods.add_async_method("peer_addr", |_, this, _: ()| {
            let stream = Arc::clone(&this.stream);
            drop(this);
            async move {
                let stream = stream.lock().await;
                let addr = stream
                    .get_ref()
                    .peer_addr()
                    .map_err(|e| mlua::Error::runtime(format!("Peer addr error: {}", e)))?;
                Ok(addr.to_string())
            }
        });

        // conn:local_addr() -> string
        methods.add_async_method("local_addr", |_, this, _: ()| {
            let stream = Arc::clone(&this.stream);
            drop(this);
            async move {
                let stream = stream.lock().await;
                let addr = stream
                    .get_ref()
                    .local_addr()
                    .map_err(|e| mlua::Error::runtime(format!("Local addr error: {}", e)))?;
                Ok(addr.to_string())
            }
        });
    }
}

async fn tcp_connect(_: Lua, (host, port): (String, u16)) -> mlua::Result<TcpConnection> {
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| mlua::Error::runtime(format!("Connect error: {}", e)))?;

    Ok(TcpConnection::new(stream))
}

// ============ TCP Server ============

struct TcpServer {
    // `TcpListener::accept` takes `&self`, so no mutex is needed.
    listener: Arc<TcpListener>,
}

impl UserData for TcpServer {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // server:accept() -> connection
        // Same invariant as TcpConnection: drop `this` before awaiting so
        // other coroutines can still use this server object (local_addr,
        // a second accept, …) while this accept is pending.
        methods.add_async_method("accept", |_, this, _: ()| {
            let listener = Arc::clone(&this.listener);
            drop(this);
            async move {
                let (stream, _addr) = listener
                    .accept()
                    .await
                    .map_err(|e| mlua::Error::runtime(format!("Accept error: {}", e)))?;

                Ok(TcpConnection::new(stream))
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

        // server:set_nonblocking(bool)
        // Tokio sockets are inherently non-blocking and `accept` never
        // blocks the event loop, so this is kept as a no-op for API
        // compatibility.
        methods.add_method("set_nonblocking", |_, _this, _nonblocking: bool| Ok(()));
    }
}

async fn tcp_listen(_: Lua, (host, port): (Option<String>, u16)) -> mlua::Result<TcpServer> {
    let host = host.unwrap_or_else(|| "0.0.0.0".to_string());
    let addr = format!("{}:{}", host, port);

    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| mlua::Error::runtime(format!("Bind error: {}", e)))?;

    Ok(TcpServer {
        listener: Arc::new(listener),
    })
}

// ============ UDP ============

struct UdpConnection {
    // Tokio's `UdpSocket` methods take `&self`, so no mutex is needed.
    socket: Arc<UdpSocket>,
    timeout: TimeoutCell,
}

impl UserData for UdpConnection {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Same invariant as TcpConnection: clone the Arcs and drop `this`
        // before the first `.await` (exclusive userdata borrows).

        // udp:send(data, host, port) -> bytes_sent
        methods.add_async_method(
            "send",
            |_, this, (data, host, port): (mlua::String, String, u16)| {
                let socket = Arc::clone(&this.socket);
                let timeout = get_timeout(&this.timeout);
                drop(this);
                async move {
                    let timeout = timeout?;
                    let bytes = data.as_bytes().to_vec();
                    let addr = format!("{}:{}", host, port);
                    let sent =
                        io_with_timeout(timeout, "Send", socket.send_to(&bytes, &addr))
                            .await?;
                    Ok(sent)
                }
            },
        );

        // udp:recv(n) -> data, host, port
        methods.add_async_method("recv", |lua, this, n: Option<usize>| {
            let socket = Arc::clone(&this.socket);
            let timeout = get_timeout(&this.timeout);
            drop(this);
            async move {
                let timeout = timeout?;
                let n = n.unwrap_or(65535);
                let mut buffer = vec![0u8; n];
                let (bytes_read, addr) =
                    io_with_timeout(timeout, "Recv", socket.recv_from(&mut buffer)).await?;
                buffer.truncate(bytes_read);

                let data = lua.create_string(&buffer)?;
                Ok((data, addr.ip().to_string(), addr.port()))
            }
        });

        // udp:connect(host, port) - Connect to a specific address
        methods.add_async_method(
            "connect",
            |_, this, (host, port): (String, u16)| {
                let socket = Arc::clone(&this.socket);
                drop(this);
                async move {
                    let addr = format!("{}:{}", host, port);
                    socket
                        .connect(&addr)
                        .await
                        .map_err(|e| mlua::Error::runtime(format!("Connect error: {}", e)))?;
                    Ok(())
                }
            },
        );

        // udp:send_connected(data) -> bytes_sent (for connected sockets)
        methods.add_async_method(
            "send_connected",
            |_, this, data: mlua::String| {
                let socket = Arc::clone(&this.socket);
                let timeout = get_timeout(&this.timeout);
                drop(this);
                async move {
                    let timeout = timeout?;
                    let bytes = data.as_bytes().to_vec();
                    let sent = io_with_timeout(timeout, "Send", socket.send(&bytes)).await?;
                    Ok(sent)
                }
            },
        );

        // udp:set_timeout(ms) — applies to subsequent send/recv operations
        methods.add_method("set_timeout", |_, this, ms: Option<u64>| {
            let mut timeout = this
                .timeout
                .lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            *timeout = ms.map(Duration::from_millis);
            Ok(())
        });

        // udp:local_addr() -> string
        methods.add_method("local_addr", |_, this, _: ()| {
            let addr = this
                .socket
                .local_addr()
                .map_err(|e| mlua::Error::runtime(format!("Local addr error: {}", e)))?;

            Ok(addr.to_string())
        });

        // udp:set_broadcast(bool)
        methods.add_method("set_broadcast", |_, this, broadcast: bool| {
            this.socket
                .set_broadcast(broadcast)
                .map_err(|e| mlua::Error::runtime(format!("Set broadcast error: {}", e)))?;

            Ok(())
        });
    }
}

async fn udp_bind(_: Lua, (host, port): (Option<String>, u16)) -> mlua::Result<UdpConnection> {
    let host = host.unwrap_or_else(|| "0.0.0.0".to_string());
    let addr = format!("{}:{}", host, port);

    let socket = UdpSocket::bind(&addr)
        .await
        .map_err(|e| mlua::Error::runtime(format!("Bind error: {}", e)))?;

    Ok(UdpConnection {
        socket: Arc::new(socket),
        timeout: Arc::new(StdMutex::new(None)),
    })
}

// ============ Utility Functions ============

async fn net_resolve(lua: Lua, hostname: String) -> mlua::Result<Table> {
    let addrs = tokio::net::lookup_host(format!("{}:0", hostname))
        .await
        .map_err(|e| mlua::Error::runtime(format!("Resolve error: {}", e)))?;

    let result = lua.create_table()?;
    for (i, addr) in addrs.enumerate() {
        result.set(i + 1, addr.ip().to_string())?;
    }

    Ok(result)
}
