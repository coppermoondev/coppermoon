//! Network module for CopperMoon
//!
//! Provides low-level TCP and UDP networking capabilities.
//! Blocking I/O is offloaded to Tokio's blocking thread pool via
//! `spawn_blocking` so it doesn't interfere with async workers.

use coppermoon_core::Result;
use mlua::{Lua, Table, UserData, UserDataMethods};
use std::io::{Read, Write, BufReader, BufRead};
use std::net::{TcpStream, TcpListener, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Helper: run a blocking closure on Tokio's thread pool and wait for the result.
fn spawn_blocking<F, T>(f: F) -> std::result::Result<T, mlua::Error>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    coppermoon_core::block_on(async {
        tokio::task::spawn_blocking(f)
            .await
            .map_err(|e| mlua::Error::runtime(format!("Task join error: {}", e)))
    })
}

/// Register the net module
pub fn register(lua: &Lua) -> Result<Table> {
    let net_table = lua.create_table()?;

    // TCP sub-module
    let tcp_table = lua.create_table()?;
    tcp_table.set("connect", lua.create_function(tcp_connect)?)?;
    tcp_table.set("listen", lua.create_function(tcp_listen)?)?;
    net_table.set("tcp", tcp_table)?;

    // UDP sub-module
    let udp_table = lua.create_table()?;
    udp_table.set("bind", lua.create_function(udp_bind)?)?;
    net_table.set("udp", udp_table)?;

    // Utility functions
    net_table.set("resolve", lua.create_function(net_resolve)?)?;

    Ok(net_table)
}

// ============ TCP Client ============

struct TcpConnection {
    stream: Arc<Mutex<TcpStream>>,
}

impl UserData for TcpConnection {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // conn:read(n) -> string
        methods.add_method("read", |lua, this, n: Option<usize>| {
            let stream = Arc::clone(&this.stream);
            let n = n.unwrap_or(4096);
            let bytes = spawn_blocking(move || {
                let mut stream = stream.lock()
                    .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
                let mut buffer = vec![0u8; n];
                let bytes_read = stream.read(&mut buffer)
                    .map_err(|e| mlua::Error::runtime(format!("Read error: {}", e)))?;
                buffer.truncate(bytes_read);
                Ok::<Vec<u8>, mlua::Error>(buffer)
            })??;
            lua.create_string(&bytes)
        });

        // conn:read_line() -> string
        methods.add_method("read_line", |_lua, this, _: ()| {
            let stream = Arc::clone(&this.stream);
            spawn_blocking(move || {
                let stream = stream.lock()
                    .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
                let mut reader = BufReader::new(&*stream);
                let mut line = String::new();
                reader.read_line(&mut line)
                    .map_err(|e| mlua::Error::runtime(format!("Read error: {}", e)))?;
                Ok::<String, mlua::Error>(line)
            })?
        });

        // conn:read_all() -> string
        methods.add_method("read_all", |lua, this, _: ()| {
            let stream = Arc::clone(&this.stream);
            let bytes = spawn_blocking(move || {
                let mut stream = stream.lock()
                    .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
                let mut buffer = Vec::new();
                stream.read_to_end(&mut buffer)
                    .map_err(|e| mlua::Error::runtime(format!("Read error: {}", e)))?;
                Ok::<Vec<u8>, mlua::Error>(buffer)
            })??;
            lua.create_string(&bytes)
        });

        // conn:write(data) -> bytes_written
        methods.add_method("write", |_, this, data: mlua::String| {
            let stream = Arc::clone(&this.stream);
            let bytes: Vec<u8> = data.as_bytes().to_vec();
            spawn_blocking(move || {
                let mut stream = stream.lock()
                    .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
                let written = stream.write(&bytes)
                    .map_err(|e| mlua::Error::runtime(format!("Write error: {}", e)))?;
                Ok::<usize, mlua::Error>(written)
            })?
        });

        // conn:write_all(data)
        methods.add_method("write_all", |_, this, data: mlua::String| {
            let stream = Arc::clone(&this.stream);
            let bytes: Vec<u8> = data.as_bytes().to_vec();
            spawn_blocking(move || {
                let mut stream = stream.lock()
                    .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
                stream.write_all(&bytes)
                    .map_err(|e| mlua::Error::runtime(format!("Write error: {}", e)))?;
                Ok::<(), mlua::Error>(())
            })?
        });

        // conn:flush()
        methods.add_method("flush", |_, this, _: ()| {
            let stream = Arc::clone(&this.stream);
            spawn_blocking(move || {
                let mut stream = stream.lock()
                    .map_err(|e| mlua::Error::runtime(format!("Flush error: {}", e)))?;
                stream.flush()
                    .map_err(|e| mlua::Error::runtime(format!("Flush error: {}", e)))?;
                Ok::<(), mlua::Error>(())
            })?
        });

        // conn:close()
        methods.add_method("close", |_, this, _: ()| {
            let stream = Arc::clone(&this.stream);
            spawn_blocking(move || {
                let stream = stream.lock()
                    .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
                stream.shutdown(std::net::Shutdown::Both)
                    .map_err(|e| mlua::Error::runtime(format!("Close error: {}", e)))?;
                Ok::<(), mlua::Error>(())
            })?
        });

        // conn:set_timeout(ms)
        methods.add_method("set_timeout", |_, this, ms: Option<u64>| {
            let stream = this.stream.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let timeout = ms.map(Duration::from_millis);
            stream.set_read_timeout(timeout)
                .map_err(|e| mlua::Error::runtime(format!("Set timeout error: {}", e)))?;
            stream.set_write_timeout(timeout)
                .map_err(|e| mlua::Error::runtime(format!("Set timeout error: {}", e)))?;

            Ok(())
        });

        // conn:peer_addr() -> string
        methods.add_method("peer_addr", |_, this, _: ()| {
            let stream = this.stream.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let addr = stream.peer_addr()
                .map_err(|e| mlua::Error::runtime(format!("Peer addr error: {}", e)))?;

            Ok(addr.to_string())
        });

        // conn:local_addr() -> string
        methods.add_method("local_addr", |_, this, _: ()| {
            let stream = this.stream.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let addr = stream.local_addr()
                .map_err(|e| mlua::Error::runtime(format!("Local addr error: {}", e)))?;

            Ok(addr.to_string())
        });
    }
}

fn tcp_connect(_: &Lua, (host, port): (String, u16)) -> mlua::Result<TcpConnection> {
    let addr = format!("{}:{}", host, port);
    let stream = spawn_blocking(move || {
        TcpStream::connect(&addr)
            .map_err(|e| mlua::Error::runtime(format!("Connect error: {}", e)))
    })??;

    Ok(TcpConnection {
        stream: Arc::new(Mutex::new(stream)),
    })
}

// ============ TCP Server ============

struct TcpServer {
    listener: Arc<Mutex<TcpListener>>,
}

impl UserData for TcpServer {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // server:accept() -> connection
        methods.add_method("accept", |_, this, _: ()| {
            let listener = Arc::clone(&this.listener);
            let stream = spawn_blocking(move || {
                let listener = listener.lock()
                    .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
                let (stream, _addr) = listener.accept()
                    .map_err(|e| mlua::Error::runtime(format!("Accept error: {}", e)))?;
                Ok::<TcpStream, mlua::Error>(stream)
            })??;

            Ok(TcpConnection {
                stream: Arc::new(Mutex::new(stream)),
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

fn tcp_listen(_: &Lua, (host, port): (Option<String>, u16)) -> mlua::Result<TcpServer> {
    let host = host.unwrap_or_else(|| "0.0.0.0".to_string());
    let addr = format!("{}:{}", host, port);

    let listener = spawn_blocking(move || {
        TcpListener::bind(&addr)
            .map_err(|e| mlua::Error::runtime(format!("Bind error: {}", e)))
    })??;

    Ok(TcpServer {
        listener: Arc::new(Mutex::new(listener)),
    })
}

// ============ UDP ============

struct UdpConnection {
    socket: Arc<Mutex<UdpSocket>>,
}

impl UserData for UdpConnection {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // udp:send(data, host, port) -> bytes_sent
        methods.add_method("send", |_, this, (data, host, port): (mlua::String, String, u16)| {
            let socket = Arc::clone(&this.socket);
            let bytes: Vec<u8> = data.as_bytes().to_vec();
            let addr = format!("{}:{}", host, port);
            spawn_blocking(move || {
                let socket = socket.lock()
                    .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
                let sent = socket.send_to(&bytes, &addr)
                    .map_err(|e| mlua::Error::runtime(format!("Send error: {}", e)))?;
                Ok::<usize, mlua::Error>(sent)
            })?
        });

        // udp:recv(n) -> data, host, port
        methods.add_method("recv", |lua, this, n: Option<usize>| {
            let socket = Arc::clone(&this.socket);
            let n = n.unwrap_or(65535);
            let result = spawn_blocking(move || {
                let socket = socket.lock()
                    .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
                let mut buffer = vec![0u8; n];
                let (bytes_read, addr) = socket.recv_from(&mut buffer)
                    .map_err(|e| mlua::Error::runtime(format!("Recv error: {}", e)))?;
                buffer.truncate(bytes_read);
                let host = addr.ip().to_string();
                let port = addr.port();
                Ok::<(Vec<u8>, String, u16), mlua::Error>((buffer, host, port))
            })??;

            let data = lua.create_string(&result.0)?;
            Ok((data, result.1, result.2))
        });

        // udp:connect(host, port) - Connect to a specific address
        methods.add_method("connect", |_, this, (host, port): (String, u16)| {
            let socket = Arc::clone(&this.socket);
            let addr = format!("{}:{}", host, port);
            spawn_blocking(move || {
                let socket = socket.lock()
                    .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
                socket.connect(&addr)
                    .map_err(|e| mlua::Error::runtime(format!("Connect error: {}", e)))?;
                Ok::<(), mlua::Error>(())
            })?
        });

        // udp:send_connected(data) -> bytes_sent (for connected sockets)
        methods.add_method("send_connected", |_, this, data: mlua::String| {
            let socket = Arc::clone(&this.socket);
            let bytes: Vec<u8> = data.as_bytes().to_vec();
            spawn_blocking(move || {
                let socket = socket.lock()
                    .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
                let sent = socket.send(&bytes)
                    .map_err(|e| mlua::Error::runtime(format!("Send error: {}", e)))?;
                Ok::<usize, mlua::Error>(sent)
            })?
        });

        // udp:set_timeout(ms) — lightweight metadata op, no need for spawn_blocking
        methods.add_method("set_timeout", |_, this, ms: Option<u64>| {
            let socket = this.socket.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let timeout = ms.map(Duration::from_millis);
            socket.set_read_timeout(timeout)
                .map_err(|e| mlua::Error::runtime(format!("Set timeout error: {}", e)))?;
            socket.set_write_timeout(timeout)
                .map_err(|e| mlua::Error::runtime(format!("Set timeout error: {}", e)))?;

            Ok(())
        });

        // udp:local_addr() -> string
        methods.add_method("local_addr", |_, this, _: ()| {
            let socket = this.socket.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let addr = socket.local_addr()
                .map_err(|e| mlua::Error::runtime(format!("Local addr error: {}", e)))?;

            Ok(addr.to_string())
        });

        // udp:set_broadcast(bool)
        methods.add_method("set_broadcast", |_, this, broadcast: bool| {
            let socket = this.socket.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            socket.set_broadcast(broadcast)
                .map_err(|e| mlua::Error::runtime(format!("Set broadcast error: {}", e)))?;

            Ok(())
        });
    }
}

fn udp_bind(_: &Lua, (host, port): (Option<String>, u16)) -> mlua::Result<UdpConnection> {
    let host = host.unwrap_or_else(|| "0.0.0.0".to_string());
    let addr = format!("{}:{}", host, port);

    let socket = spawn_blocking(move || {
        UdpSocket::bind(&addr)
            .map_err(|e| mlua::Error::runtime(format!("Bind error: {}", e)))
    })??;

    Ok(UdpConnection {
        socket: Arc::new(Mutex::new(socket)),
    })
}

// ============ Utility Functions ============

fn net_resolve(lua: &Lua, hostname: String) -> mlua::Result<Table> {
    use std::net::ToSocketAddrs;

    let addrs = spawn_blocking(move || {
        let addrs: Vec<_> = format!("{}:0", hostname)
            .to_socket_addrs()
            .map_err(|e| mlua::Error::runtime(format!("Resolve error: {}", e)))?
            .collect();
        Ok::<Vec<std::net::SocketAddr>, mlua::Error>(addrs)
    })??;

    let result = lua.create_table()?;
    for (i, addr) in addrs.iter().enumerate() {
        result.set(i + 1, addr.ip().to_string())?;
    }

    Ok(result)
}
