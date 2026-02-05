//! Buffer module for CopperMoon
//!
//! Provides binary data manipulation with cursor-based read/write operations,
//! little-endian and big-endian support, and encoding utilities.

use coppermoon_core::Result;
use mlua::{Lua, MetaMethod, MultiValue, Table, UserData, UserDataMethods, Value};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Core structs
// ---------------------------------------------------------------------------

pub(crate) struct BufferInner {
    pub(crate) data: Vec<u8>,
    position: usize,
}

pub(crate) struct Buffer {
    inner: Mutex<BufferInner>,
}

impl Buffer {
    fn new(size: usize) -> Self {
        Buffer {
            inner: Mutex::new(BufferInner {
                data: vec![0u8; size],
                position: 0,
            }),
        }
    }

    pub(crate) fn from_bytes(bytes: Vec<u8>) -> Self {
        Buffer {
            inner: Mutex::new(BufferInner {
                data: bytes,
                position: 0,
            }),
        }
    }

    /// Get a copy of the buffer's data (for cross-module access)
    pub(crate) fn get_data(&self) -> mlua::Result<Vec<u8>> {
        let inner = self.inner
            .lock()
            .map_err(|e| mlua::Error::runtime(format!("Buffer lock error: {}", e)))?;
        Ok(inner.data.clone())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn lock_inner(buf: &Buffer) -> mlua::Result<std::sync::MutexGuard<'_, BufferInner>> {
    buf.inner
        .lock()
        .map_err(|e| mlua::Error::runtime(format!("Buffer lock error: {}", e)))
}

fn read_bytes_at(inner: &mut BufferInner, n: usize) -> mlua::Result<Vec<u8>> {
    let pos = inner.position;
    if pos + n > inner.data.len() {
        return Err(mlua::Error::runtime(format!(
            "Buffer underflow: need {} bytes at position {}, but only {} available",
            n,
            pos + 1,
            inner.data.len().saturating_sub(pos)
        )));
    }
    let bytes = inner.data[pos..pos + n].to_vec();
    inner.position += n;
    Ok(bytes)
}

fn write_bytes_at(inner: &mut BufferInner, bytes: &[u8]) {
    let pos = inner.position;
    let end = pos + bytes.len();
    if end > inner.data.len() {
        inner.data.resize(end, 0);
    }
    inner.data[pos..end].copy_from_slice(bytes);
    inner.position = end;
}

// ---------------------------------------------------------------------------
// Macros for read/write method registration
// ---------------------------------------------------------------------------

macro_rules! register_read_int {
    ($methods:expr, $name:expr, $rust_ty:ty, $size:literal) => {
        $methods.add_method($name, |_, this, _: ()| {
            let mut inner = lock_inner(this)?;
            let bytes = read_bytes_at(&mut inner, $size)?;
            let arr: [u8; $size] = bytes.try_into().unwrap();
            Ok(<$rust_ty>::from_ne_bytes(arr) as i64)
        });
    };
    ($methods:expr, $name:expr, $rust_ty:ty, $size:literal, le) => {
        $methods.add_method($name, |_, this, _: ()| {
            let mut inner = lock_inner(this)?;
            let bytes = read_bytes_at(&mut inner, $size)?;
            let arr: [u8; $size] = bytes.try_into().unwrap();
            Ok(<$rust_ty>::from_le_bytes(arr) as i64)
        });
    };
    ($methods:expr, $name:expr, $rust_ty:ty, $size:literal, be) => {
        $methods.add_method($name, |_, this, _: ()| {
            let mut inner = lock_inner(this)?;
            let bytes = read_bytes_at(&mut inner, $size)?;
            let arr: [u8; $size] = bytes.try_into().unwrap();
            Ok(<$rust_ty>::from_be_bytes(arr) as i64)
        });
    };
}

macro_rules! register_read_float {
    ($methods:expr, $name:expr, $rust_ty:ty, $size:literal, le) => {
        $methods.add_method($name, |_, this, _: ()| {
            let mut inner = lock_inner(this)?;
            let bytes = read_bytes_at(&mut inner, $size)?;
            let arr: [u8; $size] = bytes.try_into().unwrap();
            Ok(<$rust_ty>::from_le_bytes(arr) as f64)
        });
    };
    ($methods:expr, $name:expr, $rust_ty:ty, $size:literal, be) => {
        $methods.add_method($name, |_, this, _: ()| {
            let mut inner = lock_inner(this)?;
            let bytes = read_bytes_at(&mut inner, $size)?;
            let arr: [u8; $size] = bytes.try_into().unwrap();
            Ok(<$rust_ty>::from_be_bytes(arr) as f64)
        });
    };
}

macro_rules! register_write_int {
    ($methods:expr, $name:expr, $rust_ty:ty, le) => {
        $methods.add_method($name, |_, this, val: i64| {
            let mut inner = lock_inner(this)?;
            let bytes = (val as $rust_ty).to_le_bytes();
            write_bytes_at(&mut inner, &bytes);
            Ok(())
        });
    };
    ($methods:expr, $name:expr, $rust_ty:ty, be) => {
        $methods.add_method($name, |_, this, val: i64| {
            let mut inner = lock_inner(this)?;
            let bytes = (val as $rust_ty).to_be_bytes();
            write_bytes_at(&mut inner, &bytes);
            Ok(())
        });
    };
}

macro_rules! register_write_float {
    ($methods:expr, $name:expr, $rust_ty:ty, le) => {
        $methods.add_method($name, |_, this, val: f64| {
            let mut inner = lock_inner(this)?;
            let bytes = (val as $rust_ty).to_le_bytes();
            write_bytes_at(&mut inner, &bytes);
            Ok(())
        });
    };
    ($methods:expr, $name:expr, $rust_ty:ty, be) => {
        $methods.add_method($name, |_, this, val: f64| {
            let mut inner = lock_inner(this)?;
            let bytes = (val as $rust_ty).to_be_bytes();
            write_bytes_at(&mut inner, &bytes);
            Ok(())
        });
    };
}

// ---------------------------------------------------------------------------
// UserData implementation
// ---------------------------------------------------------------------------

impl UserData for Buffer {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ---- Cursor management ----

        methods.add_method("tell", |_, this, _: ()| {
            let inner = lock_inner(this)?;
            Ok(inner.position + 1) // 1-indexed
        });

        methods.add_method("seek", |_, this, pos: usize| {
            let mut inner = lock_inner(this)?;
            if pos < 1 {
                return Err(mlua::Error::runtime("Buffer seek: position must be >= 1"));
            }
            let idx = pos - 1;
            if idx > inner.data.len() {
                return Err(mlua::Error::runtime(format!(
                    "Buffer seek: position {} is beyond buffer length {}",
                    pos,
                    inner.data.len()
                )));
            }
            inner.position = idx;
            Ok(())
        });

        methods.add_method("reset", |_, this, _: ()| {
            let mut inner = lock_inner(this)?;
            inner.position = 0;
            Ok(())
        });

        methods.add_method("len", |_, this, _: ()| {
            let inner = lock_inner(this)?;
            Ok(inner.data.len())
        });

        methods.add_method("capacity", |_, this, _: ()| {
            let inner = lock_inner(this)?;
            Ok(inner.data.capacity())
        });

        // ---- Integer reads (1 byte) ----

        methods.add_method("readUInt8", |_, this, _: ()| {
            let mut inner = lock_inner(this)?;
            let bytes = read_bytes_at(&mut inner, 1)?;
            Ok(bytes[0] as i64)
        });

        methods.add_method("readInt8", |_, this, _: ()| {
            let mut inner = lock_inner(this)?;
            let bytes = read_bytes_at(&mut inner, 1)?;
            Ok(bytes[0] as i8 as i64)
        });

        // ---- Integer reads (2 bytes) ----

        register_read_int!(methods, "readUInt16LE", u16, 2, le);
        register_read_int!(methods, "readUInt16BE", u16, 2, be);
        register_read_int!(methods, "readInt16LE", i16, 2, le);
        register_read_int!(methods, "readInt16BE", i16, 2, be);

        // ---- Integer reads (4 bytes) ----

        register_read_int!(methods, "readUInt32LE", u32, 4, le);
        register_read_int!(methods, "readUInt32BE", u32, 4, be);
        register_read_int!(methods, "readInt32LE", i32, 4, le);
        register_read_int!(methods, "readInt32BE", i32, 4, be);

        // ---- Integer reads (8 bytes) ----

        register_read_int!(methods, "readInt64LE", i64, 8, le);
        register_read_int!(methods, "readInt64BE", i64, 8, be);

        // ---- Float reads ----

        register_read_float!(methods, "readFloatLE", f32, 4, le);
        register_read_float!(methods, "readFloatBE", f32, 4, be);
        register_read_float!(methods, "readDoubleLE", f64, 8, le);
        register_read_float!(methods, "readDoubleBE", f64, 8, be);

        // ---- Integer writes (1 byte) ----

        methods.add_method("writeUInt8", |_, this, val: i64| {
            let mut inner = lock_inner(this)?;
            write_bytes_at(&mut inner, &[val as u8]);
            Ok(())
        });

        methods.add_method("writeInt8", |_, this, val: i64| {
            let mut inner = lock_inner(this)?;
            write_bytes_at(&mut inner, &[(val as i8) as u8]);
            Ok(())
        });

        // ---- Integer writes (2 bytes) ----

        register_write_int!(methods, "writeUInt16LE", u16, le);
        register_write_int!(methods, "writeUInt16BE", u16, be);
        register_write_int!(methods, "writeInt16LE", i16, le);
        register_write_int!(methods, "writeInt16BE", i16, be);

        // ---- Integer writes (4 bytes) ----

        register_write_int!(methods, "writeUInt32LE", u32, le);
        register_write_int!(methods, "writeUInt32BE", u32, be);
        register_write_int!(methods, "writeInt32LE", i32, le);
        register_write_int!(methods, "writeInt32BE", i32, be);

        // ---- Integer writes (8 bytes) ----

        register_write_int!(methods, "writeInt64LE", i64, le);
        register_write_int!(methods, "writeInt64BE", i64, be);

        // ---- Float writes ----

        register_write_float!(methods, "writeFloatLE", f32, le);
        register_write_float!(methods, "writeFloatBE", f32, be);
        register_write_float!(methods, "writeDoubleLE", f64, le);
        register_write_float!(methods, "writeDoubleBE", f64, be);

        // ---- Byte access ----

        methods.add_method("get", |_, this, idx: usize| {
            let inner = lock_inner(this)?;
            if idx < 1 || idx > inner.data.len() {
                return Err(mlua::Error::runtime(format!(
                    "Buffer index {} out of range [1, {}]",
                    idx,
                    inner.data.len()
                )));
            }
            Ok(inner.data[idx - 1] as i64)
        });

        methods.add_method("set", |_, this, (idx, val): (usize, i64)| {
            let mut inner = lock_inner(this)?;
            if idx < 1 || idx > inner.data.len() {
                return Err(mlua::Error::runtime(format!(
                    "Buffer index {} out of range [1, {}]",
                    idx,
                    inner.data.len()
                )));
            }
            inner.data[idx - 1] = val as u8;
            Ok(())
        });

        // ---- String read/write ----

        methods.add_method("writeString", |_, this, data: mlua::String| {
            let bytes = data.as_bytes().to_vec();
            let mut inner = lock_inner(this)?;
            let len = bytes.len();
            write_bytes_at(&mut inner, &bytes);
            Ok(len)
        });

        methods.add_method("readString", |lua, this, len: usize| {
            let mut inner = lock_inner(this)?;
            let bytes = read_bytes_at(&mut inner, len)?;
            lua.create_string(&bytes)
        });

        // ---- Buffer operations ----

        methods.add_method("slice", |_, this, (start, end): (usize, Option<usize>)| {
            let inner = lock_inner(this)?;
            if start < 1 {
                return Err(mlua::Error::runtime("Buffer slice: start must be >= 1"));
            }
            let start_idx = start - 1;
            let end_idx = end.unwrap_or(inner.data.len());
            if start_idx > inner.data.len() || end_idx > inner.data.len() || start_idx > end_idx {
                return Err(mlua::Error::runtime(format!(
                    "Buffer slice out of range: [{}, {}] for buffer of length {}",
                    start,
                    end_idx,
                    inner.data.len()
                )));
            }
            let slice_data = inner.data[start_idx..end_idx].to_vec();
            Ok(Buffer::from_bytes(slice_data))
        });

        methods.add_method(
            "copy",
            |_, this, (target, target_start, source_start, source_end): (mlua::AnyUserData, Option<usize>, Option<usize>, Option<usize>)| {
                let src_start = source_start.unwrap_or(1).max(1) - 1;

                // Read source bytes first (locks this)
                let source_bytes = {
                    let inner = lock_inner(this)?;
                    let src_end = source_end.unwrap_or(inner.data.len());
                    if src_start > inner.data.len() || src_end > inner.data.len() || src_start > src_end {
                        return Err(mlua::Error::runtime("Buffer copy: source range out of bounds"));
                    }
                    inner.data[src_start..src_end].to_vec()
                };

                // Write to target (locks target)
                let target_buf = target.borrow::<Buffer>()?;
                let tgt_start = target_start.unwrap_or(1).max(1) - 1;
                let mut target_inner = lock_inner(&target_buf)?;
                let tgt_end = tgt_start + source_bytes.len();
                if tgt_end > target_inner.data.len() {
                    target_inner.data.resize(tgt_end, 0);
                }
                target_inner.data[tgt_start..tgt_end].copy_from_slice(&source_bytes);
                Ok(source_bytes.len())
            },
        );

        methods.add_method(
            "fill",
            |_, this, (value, start, end): (i64, Option<usize>, Option<usize>)| {
                let mut inner = lock_inner(this)?;
                let start_idx = start.unwrap_or(1).max(1) - 1;
                let end_idx = end.unwrap_or(inner.data.len());
                let byte = value as u8;
                for i in start_idx..end_idx.min(inner.data.len()) {
                    inner.data[i] = byte;
                }
                Ok(())
            },
        );

        methods.add_method("clear", |_, this, _: ()| {
            let mut inner = lock_inner(this)?;
            for b in inner.data.iter_mut() {
                *b = 0;
            }
            inner.position = 0;
            Ok(())
        });

        // ---- Encoding / conversion ----

        methods.add_method("toString", |lua, this, _: ()| {
            let inner = lock_inner(this)?;
            lua.create_string(&inner.data)
        });

        methods.add_method("bytes", |lua, this, _: ()| {
            let inner = lock_inner(this)?;
            lua.create_string(&inner.data)
        });

        methods.add_method("toHex", |_, this, _: ()| {
            let inner = lock_inner(this)?;
            Ok(hex::encode(&inner.data))
        });

        methods.add_method("toBase64", |_, this, _: ()| {
            use base64::{engine::general_purpose::STANDARD, Engine};
            let inner = lock_inner(this)?;
            Ok(STANDARD.encode(&inner.data))
        });

        // ---- Metamethods ----

        methods.add_meta_method(MetaMethod::ToString, |_, this, _: ()| {
            let inner = lock_inner(this)?;
            Ok(format!("Buffer({} bytes)", inner.data.len()))
        });

        methods.add_meta_method(MetaMethod::Len, |_, this, _: ()| {
            let inner = lock_inner(this)?;
            Ok(inner.data.len())
        });
    }
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

fn buffer_new(_: &Lua, size: usize) -> mlua::Result<Buffer> {
    Ok(Buffer::new(size))
}

fn buffer_from(_: &Lua, data: mlua::String) -> mlua::Result<Buffer> {
    Ok(Buffer::from_bytes(data.as_bytes().to_vec()))
}

fn buffer_from_hex(_: &Lua, data: String) -> mlua::Result<Buffer> {
    let bytes = hex::decode(&data)
        .map_err(|e| mlua::Error::runtime(format!("Buffer.fromHex: invalid hex: {}", e)))?;
    Ok(Buffer::from_bytes(bytes))
}

fn buffer_from_base64(_: &Lua, data: String) -> mlua::Result<Buffer> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let bytes = STANDARD
        .decode(&data)
        .map_err(|e| mlua::Error::runtime(format!("Buffer.fromBase64: decode error: {}", e)))?;
    Ok(Buffer::from_bytes(bytes))
}

fn buffer_alloc(_: &Lua, (size, fill): (usize, Option<i64>)) -> mlua::Result<Buffer> {
    let byte = fill.unwrap_or(0) as u8;
    Ok(Buffer {
        inner: Mutex::new(BufferInner {
            data: vec![byte; size],
            position: 0,
        }),
    })
}

fn buffer_concat(_: &Lua, args: MultiValue) -> mlua::Result<Buffer> {
    let mut combined = Vec::new();
    for arg in args {
        match arg {
            Value::UserData(ud) => {
                let buf = ud.borrow::<Buffer>()?;
                let inner = lock_inner(&buf)?;
                combined.extend_from_slice(&inner.data);
            }
            _ => {
                return Err(mlua::Error::runtime(
                    "buffer.concat: all arguments must be Buffers",
                ))
            }
        }
    }
    Ok(Buffer::from_bytes(combined))
}

fn buffer_is_buffer(_: &Lua, value: Value) -> mlua::Result<bool> {
    match value {
        Value::UserData(ud) => Ok(ud.borrow::<Buffer>().is_ok()),
        _ => Ok(false),
    }
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

pub fn register(lua: &Lua) -> Result<Table> {
    let buffer_table = lua.create_table()?;

    buffer_table.set("new", lua.create_function(buffer_new)?)?;
    buffer_table.set("from", lua.create_function(buffer_from)?)?;
    buffer_table.set("fromHex", lua.create_function(buffer_from_hex)?)?;
    buffer_table.set("fromBase64", lua.create_function(buffer_from_base64)?)?;
    buffer_table.set("alloc", lua.create_function(buffer_alloc)?)?;
    buffer_table.set("concat", lua.create_function(buffer_concat)?)?;
    buffer_table.set("isBuffer", lua.create_function(buffer_is_buffer)?)?;

    Ok(buffer_table)
}
