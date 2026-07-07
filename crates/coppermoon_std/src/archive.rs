//! Archive module for CopperMoon
//!
//! Provides compression and archive operations: ZIP, TAR/TAR.GZ, and raw GZIP.
//!
//! # Async design
//!
//! (De)compression is CPU- and disk-bound, so every heavy operation is split
//! into a pure synchronous `*_impl` function (plain Rust data in, plain Rust
//! data out — no Lua values) that can run on Tokio's blocking thread pool.
//!
//! Each Lua entry point is a *hybrid* (see [`crate::hybrid_fn`]):
//! - in yieldable contexts the impl runs via `tokio::task::spawn_blocking`
//!   and the event loop keeps ticking during the operation;
//! - in non-yieldable contexts (module top-level inside `require`,
//!   metamethods, …) the impl is called directly on the current thread —
//!   identical result, event loop paused (the pre-async behaviour).
//!
//! Userdata methods (`ZipReader`, `ZipWriter`, `TarReader`, `TarWriter`) get
//! the same treatment through a custom `__index` metamethod: the public
//! method name resolves to a cached Lua dispatcher which forwards to a
//! `_async_<name>` or `_sync_<name>` implementation depending on
//! `coroutine.isyieldable()`.
//!
//! Lua values are always extracted *before* entering `spawn_blocking` and
//! results are converted back to Lua *after* the await — the blocking pool
//! never touches the Lua state.

use crate::buffer::Buffer;
use coppermoon_core::Result;
use mlua::{Function, Lua, MetaMethod, Table, UserData, UserDataMethods, Value};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

/// Result type of the pure blocking implementations.
type WorkResult<T> = std::result::Result<T, String>;

const ZIP_READER_CLOSED: &str = "ZipReader is already closed";
const ZIP_WRITER_CLOSED: &str = "ZipWriter is already closed";
const TAR_WRITER_CLOSED: &str = "TarWriter is closed";
const TAR_WRITER_ALREADY_CLOSED: &str = "TarWriter is already closed";

// ============================================================================
// Helpers
// ============================================================================

/// Extract bytes from a Lua string or a Buffer userdata
fn extract_bytes(value: Value) -> mlua::Result<Vec<u8>> {
    match value {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::UserData(ud) => {
            let buf = ud.borrow::<Buffer>()?;
            buf.get_data()
        }
        _ => Err(mlua::Error::runtime(
            "Expected string or Buffer",
        )),
    }
}

/// Run a pure blocking closure on Tokio's blocking pool and await the result.
/// Never call Lua from inside `f`.
async fn run_blocking<T, F>(f: F) -> mlua::Result<T>
where
    F: FnOnce() -> WorkResult<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| mlua::Error::runtime(format!("Task join error: {}", e)))?
        .map_err(mlua::Error::runtime)
}

/// Lock a `Mutex<Option<S>>` state and run `f` on the live state, mapping the
/// "already closed" case to `closed_msg`.
fn with_open<S, T>(
    inner: &Mutex<Option<S>>,
    closed_msg: &str,
    f: impl FnOnce(&mut S) -> WorkResult<T>,
) -> WorkResult<T> {
    let mut guard = inner
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let state = guard
        .as_mut()
        .ok_or_else(|| closed_msg.to_string())?;
    f(state)
}

/// Lock a `Mutex<Option<S>>` state and take it out (for `close`).
fn take_open<S>(inner: &Mutex<Option<S>>, closed_msg: &str) -> WorkResult<S> {
    let mut guard = inner
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    guard.take().ok_or_else(|| closed_msg.to_string())
}

/// Build a hybrid async/blocking module-level Lua function from three pure
/// stages (mirrors the `hybrid` helper in `http.rs`):
/// - `extract`: Lua arguments -> plain `Send` parameters (runs on the Lua thread)
/// - `work`: the blocking implementation (runs on the blocking pool in the
///   async path, inline in the sync fallback)
/// - `convert`: plain result -> Lua value (runs on the Lua thread)
fn hybrid_blocking<A, P, D, R>(
    lua: &Lua,
    extract: fn(A) -> mlua::Result<P>,
    work: fn(P) -> WorkResult<D>,
    convert: fn(&Lua, D) -> mlua::Result<R>,
) -> mlua::Result<Function>
where
    A: mlua::FromLuaMulti + Send + 'static,
    P: Send + 'static,
    D: Send + 'static,
    R: mlua::IntoLuaMulti + Send + 'static,
{
    let async_fn = lua.create_async_function(move |lua, args: A| async move {
        let params = extract(args)?;
        let data = run_blocking(move || work(params)).await?;
        convert(&lua, data)
    })?;
    let sync_fn = lua.create_function(move |lua, args: A| {
        let params = extract(args)?;
        let data = work(params).map_err(mlua::Error::runtime)?;
        convert(lua, data)
    })?;
    crate::hybrid_fn(lua, async_fn, sync_fn)
}

/// Return (and cache in the Lua registry) the hybrid dispatcher for a method
/// name: `self:<name>(...)` forwards to `self:_async_<name>(...)` in yieldable
/// contexts and `self:_sync_<name>(...)` otherwise. The dispatcher only uses
/// `self` and the method name, so one dispatcher per name is shared by every
/// archive userdata type.
fn hybrid_method_dispatch(lua: &Lua, name: &str) -> mlua::Result<Function> {
    let key = format!("coppermoon.archive.hybrid_method.{}", name);
    if let Ok(f) = lua.named_registry_value::<Function>(&key) {
        return Ok(f);
    }
    let f = lua
        .load(
            r#"
            local async_name, sync_name = ...
            return function(self, ...)
                if coroutine.isyieldable() then
                    return self[async_name](self, ...)
                end
                return self[sync_name](self, ...)
            end
            "#,
        )
        .set_name("=[coppermoon hybrid method dispatch]")
        .call::<Function>((format!("_async_{}", name), format!("_sync_{}", name)))?;
    lua.set_named_registry_value(&key, f.clone())?;
    Ok(f)
}

/// `__index` fallback shared by the archive userdata types: resolve the public
/// hybrid method names to their dispatcher, error on anything else (matching
/// mlua's default behaviour for unknown userdata fields).
fn hybrid_method_index(lua: &Lua, key: Value, hybrid_names: &[&str]) -> mlua::Result<Value> {
    if let Value::String(s) = &key {
        let name = s.to_string_lossy().to_string();
        if hybrid_names.contains(&name.as_str()) {
            return Ok(Value::Function(hybrid_method_dispatch(lua, &name)?));
        }
        return Err(mlua::Error::runtime(format!(
            "attempt to get an unknown field '{}'",
            name
        )));
    }
    Err(mlua::Error::runtime(format!(
        "attempt to get an unknown field '<{}>'",
        key.type_name()
    )))
}

/// Convert an optional Lua filter table (array of names) to a plain HashSet.
/// Runs on the Lua thread, before any spawn_blocking.
fn filter_to_set(filter: Option<Table>) -> Option<HashSet<String>> {
    filter.map(|t| {
        let mut set = HashSet::new();
        for i in 1..=t.raw_len() {
            if let Ok(name) = t.get::<String>(i) {
                set.insert(name);
            }
        }
        set
    })
}

// ============================================================================
// ZIP Reader (supports file and in-memory sources)
// ============================================================================

enum ZipSource {
    File(zip::ZipArchive<std::fs::File>),
    Memory(zip::ZipArchive<std::io::Cursor<Vec<u8>>>),
}

impl ZipSource {
    fn len(&self) -> usize {
        match self {
            ZipSource::File(a) => a.len(),
            ZipSource::Memory(a) => a.len(),
        }
    }

    fn by_index(&mut self, i: usize) -> zip::result::ZipResult<zip::read::ZipFile<'_>> {
        match self {
            ZipSource::File(a) => a.by_index(i),
            ZipSource::Memory(a) => a.by_index(i),
        }
    }

    fn by_name(&mut self, name: &str) -> zip::result::ZipResult<zip::read::ZipFile<'_>> {
        match self {
            ZipSource::File(a) => a.by_name(name),
            ZipSource::Memory(a) => a.by_name(name),
        }
    }
}

/// Plain-data description of a ZIP entry (safe to send across threads).
struct ZipEntryInfo {
    name: String,
    size: u64,
    compressed_size: u64,
    is_dir: bool,
}

struct ZipReader {
    inner: Arc<Mutex<Option<ZipSource>>>,
}

impl ZipReader {
    fn new(source: ZipSource) -> Self {
        ZipReader {
            inner: Arc::new(Mutex::new(Some(source))),
        }
    }
}

// ---- pure blocking implementations (no Lua) ----

fn zip_list_impl(inner: &Mutex<Option<ZipSource>>) -> WorkResult<Vec<ZipEntryInfo>> {
    with_open(inner, ZIP_READER_CLOSED, |archive| {
        let mut entries = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let file = archive
                .by_index(i)
                .map_err(|e| format!("ZIP entry error: {}", e))?;
            entries.push(ZipEntryInfo {
                name: file.name().to_string(),
                size: file.size(),
                compressed_size: file.compressed_size(),
                is_dir: file.is_dir(),
            });
        }
        Ok(entries)
    })
}

fn zip_read_impl(inner: &Mutex<Option<ZipSource>>, name: &str) -> WorkResult<Vec<u8>> {
    with_open(inner, ZIP_READER_CLOSED, |archive| {
        let mut file = archive
            .by_name(name)
            .map_err(|e| format!("File '{}' not found in ZIP: {}", name, e))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|e| format!("Failed to read '{}': {}", name, e))?;
        Ok(buf)
    })
}

fn zip_extract_impl(
    inner: &Mutex<Option<ZipSource>>,
    output_dir: &str,
    filter_set: Option<HashSet<String>>,
) -> WorkResult<()> {
    with_open(inner, ZIP_READER_CLOSED, |archive| {
        let out_path = std::path::Path::new(output_dir);

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("ZIP entry error: {}", e))?;

            let name = file.name().to_string();

            if let Some(ref filter) = filter_set {
                if !filter.contains(&name) {
                    continue;
                }
            }

            let target = out_path.join(&name);

            // Security: prevent path traversal
            let canonical_out = out_path
                .canonicalize()
                .unwrap_or_else(|_| out_path.to_path_buf());
            if let Ok(canonical_target) = target.canonicalize() {
                if !canonical_target.starts_with(&canonical_out) {
                    return Err(format!("ZIP path traversal detected: '{}'", name));
                }
            }

            if file.is_dir() {
                std::fs::create_dir_all(&target)
                    .map_err(|e| format!("Failed to create dir: {}", e))?;
            } else {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create dir: {}", e))?;
                }
                let mut out_file = std::fs::File::create(&target)
                    .map_err(|e| format!("Failed to create file: {}", e))?;
                std::io::copy(&mut file, &mut out_file)
                    .map_err(|e| format!("Failed to extract file: {}", e))?;
            }
        }
        Ok(())
    })
}

// ---- Lua conversion (event-loop thread only) ----

fn zip_entries_to_table(lua: &Lua, entries: Vec<ZipEntryInfo>) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    for (i, e) in entries.into_iter().enumerate() {
        let entry = lua.create_table()?;
        entry.set("name", e.name)?;
        entry.set("size", e.size)?;
        entry.set("compressed_size", e.compressed_size)?;
        entry.set("is_dir", e.is_dir)?;
        result.set(i + 1, entry)?;
    }
    Ok(result)
}

impl UserData for ZipReader {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // z:list() -> array of {name, size, compressed_size, is_dir}
        methods.add_method("_sync_list", |lua, this, _: ()| {
            let entries = zip_list_impl(&this.inner).map_err(mlua::Error::runtime)?;
            zip_entries_to_table(lua, entries)
        });
        methods.add_async_method("_async_list", |lua, this, _: ()| async move {
            let inner = Arc::clone(&this.inner);
            // mlua userdata borrows are exclusive: release `this` before
            // the first await so other coroutines can use the same object.
            drop(this);
            let entries = run_blocking(move || zip_list_impl(&inner)).await?;
            zip_entries_to_table(&lua, entries)
        });

        // z:read(name) -> string
        methods.add_method("_sync_read", |lua, this, name: String| {
            let bytes = zip_read_impl(&this.inner, &name).map_err(mlua::Error::runtime)?;
            lua.create_string(&bytes)
        });
        methods.add_async_method("_async_read", |lua, this, name: String| async move {
            let inner = Arc::clone(&this.inner);
            // mlua userdata borrows are exclusive: release `this` before
            // the first await so other coroutines can use the same object.
            drop(this);
            let bytes = run_blocking(move || zip_read_impl(&inner, &name)).await?;
            lua.create_string(&bytes)
        });

        // z:read_buffer(name) -> Buffer
        methods.add_method("_sync_read_buffer", |_, this, name: String| {
            let bytes = zip_read_impl(&this.inner, &name).map_err(mlua::Error::runtime)?;
            Ok(Buffer::from_bytes(bytes))
        });
        methods.add_async_method("_async_read_buffer", |_, this, name: String| async move {
            let inner = Arc::clone(&this.inner);
            // mlua userdata borrows are exclusive: release `this` before
            // the first await so other coroutines can use the same object.
            drop(this);
            let bytes = run_blocking(move || zip_read_impl(&inner, &name)).await?;
            Ok(Buffer::from_bytes(bytes))
        });

        // z:extract(output_dir, filter?)
        methods.add_method(
            "_sync_extract",
            |_, this, (output_dir, filter): (String, Option<Table>)| {
                let filter_set = filter_to_set(filter);
                zip_extract_impl(&this.inner, &output_dir, filter_set)
                    .map_err(mlua::Error::runtime)
            },
        );
        methods.add_async_method(
            "_async_extract",
            |_, this, (output_dir, filter): (String, Option<Table>)| async move {
                let filter_set = filter_to_set(filter);
                let inner = Arc::clone(&this.inner);
                // mlua userdata borrows are exclusive: release `this` before
                // the first await so other coroutines can use the same object.
                drop(this);
                run_blocking(move || zip_extract_impl(&inner, &output_dir, filter_set)).await
            },
        );

        // z:exists(name) -> boolean  (in-memory central-directory lookup, stays sync)
        methods.add_method("exists", |_, this, name: String| {
            with_open(&this.inner, ZIP_READER_CLOSED, |archive| {
                Ok(archive.by_name(&name).is_ok())
            })
            .map_err(mlua::Error::runtime)
        });

        // z:close()  (drops the archive handle, stays sync)
        methods.add_method("close", |_, this, _: ()| {
            let mut guard = this
                .inner
                .lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            guard.take();
            Ok(())
        });

        // Hybrid dispatch for the heavy public methods.
        methods.add_meta_method(MetaMethod::Index, |lua, _this, key: Value| {
            hybrid_method_index(lua, key, &["list", "read", "read_buffer", "extract"])
        });
    }
}

// ============================================================================
// ZIP Writer
// ============================================================================

struct ZipWriterObj {
    inner: Arc<Mutex<Option<zip::ZipWriter<std::fs::File>>>>,
}

impl ZipWriterObj {
    fn new(writer: zip::ZipWriter<std::fs::File>) -> Self {
        ZipWriterObj {
            inner: Arc::new(Mutex::new(Some(writer))),
        }
    }
}

// ---- pure blocking implementations (no Lua) ----

fn zip_add_impl(
    inner: &Mutex<Option<zip::ZipWriter<std::fs::File>>>,
    disk_path: &str,
    archive_name: Option<String>,
) -> WorkResult<()> {
    with_open(inner, ZIP_WRITER_CLOSED, |writer| {
        let name = archive_name.unwrap_or_else(|| {
            std::path::Path::new(disk_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| disk_path.to_string())
        });

        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        writer
            .start_file(&name, options)
            .map_err(|e| format!("Failed to start ZIP entry '{}': {}", name, e))?;

        let mut file = std::fs::File::open(disk_path)
            .map_err(|e| format!("Failed to open '{}': {}", disk_path, e))?;
        std::io::copy(&mut file, writer)
            .map_err(|e| format!("Failed to write '{}' to ZIP: {}", name, e))?;

        Ok(())
    })
}

fn zip_add_data_impl(
    inner: &Mutex<Option<zip::ZipWriter<std::fs::File>>>,
    name: &str,
    bytes: Vec<u8>,
) -> WorkResult<()> {
    with_open(inner, ZIP_WRITER_CLOSED, |writer| {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        writer
            .start_file(name, options)
            .map_err(|e| format!("Failed to start ZIP entry '{}': {}", name, e))?;

        writer
            .write_all(&bytes)
            .map_err(|e| format!("Failed to write '{}': {}", name, e))?;

        Ok(())
    })
}

fn zip_add_dir_impl(
    inner: &Mutex<Option<zip::ZipWriter<std::fs::File>>>,
    disk_path: &str,
    prefix: Option<String>,
) -> WorkResult<()> {
    with_open(inner, ZIP_WRITER_CLOSED, |writer| {
        let base = std::path::Path::new(disk_path);
        let prefix = prefix.unwrap_or_default();
        zip_add_dir_recursive(writer, base, base, &prefix)
    })
}

fn zip_close_impl(inner: &Mutex<Option<zip::ZipWriter<std::fs::File>>>) -> WorkResult<()> {
    let writer = take_open(inner, ZIP_WRITER_CLOSED)?;
    writer
        .finish()
        .map_err(|e| format!("Failed to finalize ZIP: {}", e))?;
    Ok(())
}

impl UserData for ZipWriterObj {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // z:add(disk_path, archive_name?)
        methods.add_method(
            "_sync_add",
            |_, this, (disk_path, archive_name): (String, Option<String>)| {
                zip_add_impl(&this.inner, &disk_path, archive_name).map_err(mlua::Error::runtime)
            },
        );
        methods.add_async_method(
            "_async_add",
            |_, this, (disk_path, archive_name): (String, Option<String>)| async move {
                let inner = Arc::clone(&this.inner);
                // mlua userdata borrows are exclusive: release `this` before
                // the first await so other coroutines can use the same object.
                drop(this);
                run_blocking(move || zip_add_impl(&inner, &disk_path, archive_name)).await
            },
        );

        // z:add_data(name, contents) / z:add_string(name, contents)
        // -- accepts string or Buffer
        for method_name in ["add_data", "add_string"] {
            methods.add_method(
                format!("_sync_{}", method_name),
                |_, this, (name, contents): (String, Value)| {
                    let bytes = extract_bytes(contents)?;
                    zip_add_data_impl(&this.inner, &name, bytes).map_err(mlua::Error::runtime)
                },
            );
            methods.add_async_method(
                format!("_async_{}", method_name),
                |_, this, (name, contents): (String, Value)| async move {
                    let bytes = extract_bytes(contents)?;
                    let inner = Arc::clone(&this.inner);
                // mlua userdata borrows are exclusive: release `this` before
                // the first await so other coroutines can use the same object.
                drop(this);
                    run_blocking(move || zip_add_data_impl(&inner, &name, bytes)).await
                },
            );
        }

        // z:add_dir(disk_path, prefix?)
        methods.add_method(
            "_sync_add_dir",
            |_, this, (disk_path, prefix): (String, Option<String>)| {
                zip_add_dir_impl(&this.inner, &disk_path, prefix).map_err(mlua::Error::runtime)
            },
        );
        methods.add_async_method(
            "_async_add_dir",
            |_, this, (disk_path, prefix): (String, Option<String>)| async move {
                let inner = Arc::clone(&this.inner);
                // mlua userdata borrows are exclusive: release `this` before
                // the first await so other coroutines can use the same object.
                drop(this);
                run_blocking(move || zip_add_dir_impl(&inner, &disk_path, prefix)).await
            },
        );

        // z:close()  (flushes and finalizes the deflate stream — heavy, hybrid)
        methods.add_method("_sync_close", |_, this, _: ()| {
            zip_close_impl(&this.inner).map_err(mlua::Error::runtime)
        });
        methods.add_async_method("_async_close", |_, this, _: ()| async move {
            let inner = Arc::clone(&this.inner);
            // mlua userdata borrows are exclusive: release `this` before
            // the first await so other coroutines can use the same object.
            drop(this);
            run_blocking(move || zip_close_impl(&inner)).await
        });

        methods.add_meta_method(MetaMethod::Index, |lua, _this, key: Value| {
            hybrid_method_index(
                lua,
                key,
                &["add", "add_data", "add_string", "add_dir", "close"],
            )
        });
    }
}

fn zip_add_dir_recursive(
    writer: &mut zip::ZipWriter<std::fs::File>,
    root: &std::path::Path,
    current: &std::path::Path,
    prefix: &str,
) -> WorkResult<()> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for entry in std::fs::read_dir(current)
        .map_err(|e| format!("Failed to read dir '{}': {}", current.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Dir entry error: {}", e))?;
        let entry_path = entry.path();

        let relative = entry_path
            .strip_prefix(root)
            .map_err(|e| format!("Path error: {}", e))?;

        let archive_name = if prefix.is_empty() {
            relative.to_string_lossy().to_string()
        } else {
            format!(
                "{}/{}",
                prefix.trim_end_matches('/'),
                relative.to_string_lossy()
            )
        };

        // Normalize path separators to forward slashes
        let archive_name = archive_name.replace('\\', "/");

        if entry_path.is_dir() {
            writer
                .add_directory(format!("{}/", archive_name), options)
                .map_err(|e| format!("Failed to add dir '{}': {}", archive_name, e))?;
            zip_add_dir_recursive(writer, root, &entry_path, prefix)?;
        } else {
            writer
                .start_file(&archive_name, options)
                .map_err(|e| format!("Failed to start '{}': {}", archive_name, e))?;
            let mut file = std::fs::File::open(&entry_path)
                .map_err(|e| format!("Failed to open '{}': {}", entry_path.display(), e))?;
            std::io::copy(&mut file, writer)
                .map_err(|e| format!("Failed to write '{}': {}", archive_name, e))?;
        }
    }
    Ok(())
}

// ============================================================================
// TAR Reader
// ============================================================================

struct TarReader {
    path: String,
    is_gzipped: bool,
}

fn open_tar_archive(path: &str, is_gzipped: bool) -> WorkResult<tar::Archive<Box<dyn Read>>> {
    let file =
        std::fs::File::open(path).map_err(|e| format!("Failed to open '{}': {}", path, e))?;

    let reader: Box<dyn Read> = if is_gzipped {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };

    Ok(tar::Archive::new(reader))
}

/// Plain-data description of a TAR entry (safe to send across threads).
struct TarEntryInfo {
    name: String,
    size: u64,
    is_dir: bool,
}

// ---- pure blocking implementations (no Lua) ----

fn tar_list_impl(path: &str, is_gzipped: bool) -> WorkResult<Vec<TarEntryInfo>> {
    let mut archive = open_tar_archive(path, is_gzipped)?;
    let entries = archive
        .entries()
        .map_err(|e| format!("Failed to read tar entries: {}", e))?;

    let mut result = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("Tar entry error: {}", e))?;
        let header = entry.header();

        result.push(TarEntryInfo {
            name: entry
                .path()
                .map_err(|e| format!("Path error: {}", e))?
                .to_string_lossy()
                .to_string(),
            size: header.size().map_err(|e| format!("Size error: {}", e))?,
            is_dir: header.entry_type().is_dir(),
        });
    }
    Ok(result)
}

fn tar_read_impl(path: &str, is_gzipped: bool, name: &str) -> WorkResult<Vec<u8>> {
    let mut archive = open_tar_archive(path, is_gzipped)?;
    let entries = archive
        .entries()
        .map_err(|e| format!("Failed to read tar entries: {}", e))?;

    for entry in entries {
        let mut entry = entry.map_err(|e| format!("Tar entry error: {}", e))?;
        let entry_path = entry
            .path()
            .map_err(|e| format!("Path error: {}", e))?
            .to_string_lossy()
            .to_string();

        if entry_path == name {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| format!("Failed to read '{}': {}", name, e))?;
            return Ok(buf);
        }
    }

    Err(format!("File '{}' not found in tar archive", name))
}

fn tar_extract_impl(path: &str, is_gzipped: bool, output_dir: &str) -> WorkResult<()> {
    let mut archive = open_tar_archive(path, is_gzipped)?;
    archive
        .unpack(output_dir)
        .map_err(|e| format!("Failed to extract tar to '{}': {}", output_dir, e))?;
    Ok(())
}

// ---- Lua conversion (event-loop thread only) ----

fn tar_entries_to_table(lua: &Lua, entries: Vec<TarEntryInfo>) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    for (i, e) in entries.into_iter().enumerate() {
        let info = lua.create_table()?;
        info.set("name", e.name)?;
        info.set("size", e.size)?;
        info.set("is_dir", e.is_dir)?;
        result.set(i + 1, info)?;
    }
    Ok(result)
}

impl UserData for TarReader {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // t:list() -> array of {name, size, is_dir}
        methods.add_method("_sync_list", |lua, this, _: ()| {
            let entries =
                tar_list_impl(&this.path, this.is_gzipped).map_err(mlua::Error::runtime)?;
            tar_entries_to_table(lua, entries)
        });
        methods.add_async_method("_async_list", |lua, this, _: ()| async move {
            let path = this.path.clone();
            let gz = this.is_gzipped;
            // mlua userdata borrows are exclusive: release `this` before
            // the first await so other coroutines can use the same object.
            drop(this);
            let entries = run_blocking(move || tar_list_impl(&path, gz)).await?;
            tar_entries_to_table(&lua, entries)
        });

        // t:read(name) -> string
        methods.add_method("_sync_read", |lua, this, name: String| {
            let bytes =
                tar_read_impl(&this.path, this.is_gzipped, &name).map_err(mlua::Error::runtime)?;
            lua.create_string(&bytes)
        });
        methods.add_async_method("_async_read", |lua, this, name: String| async move {
            let path = this.path.clone();
            let gz = this.is_gzipped;
            // mlua userdata borrows are exclusive: release `this` before
            // the first await so other coroutines can use the same object.
            drop(this);
            let bytes = run_blocking(move || tar_read_impl(&path, gz, &name)).await?;
            lua.create_string(&bytes)
        });

        // t:read_buffer(name) -> Buffer
        methods.add_method("_sync_read_buffer", |_, this, name: String| {
            let bytes =
                tar_read_impl(&this.path, this.is_gzipped, &name).map_err(mlua::Error::runtime)?;
            Ok(Buffer::from_bytes(bytes))
        });
        methods.add_async_method("_async_read_buffer", |_, this, name: String| async move {
            let path = this.path.clone();
            let gz = this.is_gzipped;
            // mlua userdata borrows are exclusive: release `this` before
            // the first await so other coroutines can use the same object.
            drop(this);
            let bytes = run_blocking(move || tar_read_impl(&path, gz, &name)).await?;
            Ok(Buffer::from_bytes(bytes))
        });

        // t:extract(output_dir)
        methods.add_method("_sync_extract", |_, this, output_dir: String| {
            tar_extract_impl(&this.path, this.is_gzipped, &output_dir)
                .map_err(mlua::Error::runtime)
        });
        methods.add_async_method("_async_extract", |_, this, output_dir: String| async move {
            let path = this.path.clone();
            let gz = this.is_gzipped;
            // mlua userdata borrows are exclusive: release `this` before
            // the first await so other coroutines can use the same object.
            drop(this);
            run_blocking(move || tar_extract_impl(&path, gz, &output_dir)).await
        });

        // t:close() -- no-op for consistency
        methods.add_method("close", |_, _this, _: ()| Ok(()));

        methods.add_meta_method(MetaMethod::Index, |lua, _this, key: Value| {
            hybrid_method_index(lua, key, &["list", "read", "read_buffer", "extract"])
        });
    }
}

// ============================================================================
// TAR Writer
// ============================================================================

enum TarWriterInner {
    Plain(tar::Builder<std::fs::File>),
    Gzipped(tar::Builder<flate2::write::GzEncoder<std::fs::File>>),
}

struct TarWriterObj {
    inner: Arc<Mutex<Option<TarWriterInner>>>,
}

impl TarWriterObj {
    fn new(inner: TarWriterInner) -> Self {
        TarWriterObj {
            inner: Arc::new(Mutex::new(Some(inner))),
        }
    }
}

macro_rules! with_tar_builder {
    ($writer:expr, $builder:ident => $body:expr) => {
        match $writer {
            TarWriterInner::Plain(ref mut $builder) => $body,
            TarWriterInner::Gzipped(ref mut $builder) => $body,
        }
    };
}

// ---- pure blocking implementations (no Lua) ----

fn tar_add_impl(
    inner: &Mutex<Option<TarWriterInner>>,
    disk_path: &str,
    archive_name: Option<String>,
) -> WorkResult<()> {
    let mut guard = inner
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;

    let name = archive_name.unwrap_or_else(|| {
        std::path::Path::new(disk_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| disk_path.to_string())
    });
    let name = name.replace('\\', "/");

    // Match the historical error ordering: the file is opened before the
    // "TarWriter is closed" check.
    let mut file = std::fs::File::open(disk_path)
        .map_err(|e| format!("Failed to open '{}': {}", disk_path, e))?;

    let writer = guard
        .as_mut()
        .ok_or_else(|| TAR_WRITER_CLOSED.to_string())?;

    with_tar_builder!(writer, builder => {
        builder
            .append_file(&name, &mut file)
            .map_err(|e| format!("Failed to add '{}' to tar: {}", name, e))?;
    });

    Ok(())
}

fn tar_add_data_impl(
    inner: &Mutex<Option<TarWriterInner>>,
    name: &str,
    bytes: Vec<u8>,
) -> WorkResult<()> {
    with_open(inner, TAR_WRITER_CLOSED, |writer| {
        let name = name.replace('\\', "/");

        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();

        with_tar_builder!(writer, builder => {
            builder
                .append_data(&mut header, &name, &bytes[..])
                .map_err(|e| format!("Failed to add '{}': {}", name, e))?;
        });

        Ok(())
    })
}

fn tar_add_dir_impl(
    inner: &Mutex<Option<TarWriterInner>>,
    disk_path: &str,
    prefix: Option<String>,
) -> WorkResult<()> {
    with_open(inner, TAR_WRITER_CLOSED, |writer| {
        let base = std::path::Path::new(disk_path);
        let prefix_str = prefix.unwrap_or_default();

        with_tar_builder!(writer, builder => {
            tar_add_dir_recursive(builder, base, base, &prefix_str)?;
        });

        Ok(())
    })
}

fn tar_close_impl(inner: &Mutex<Option<TarWriterInner>>) -> WorkResult<()> {
    let writer = take_open(inner, TAR_WRITER_ALREADY_CLOSED)?;

    match writer {
        TarWriterInner::Plain(builder) => {
            builder
                .into_inner()
                .map_err(|e| format!("Failed to finalize tar: {}", e))?;
        }
        TarWriterInner::Gzipped(builder) => {
            let gz_encoder = builder
                .into_inner()
                .map_err(|e| format!("Failed to finalize tar: {}", e))?;
            gz_encoder
                .finish()
                .map_err(|e| format!("Failed to finalize gzip: {}", e))?;
        }
    }

    Ok(())
}

impl UserData for TarWriterObj {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // t:add(disk_path, archive_name?)
        methods.add_method(
            "_sync_add",
            |_, this, (disk_path, archive_name): (String, Option<String>)| {
                tar_add_impl(&this.inner, &disk_path, archive_name).map_err(mlua::Error::runtime)
            },
        );
        methods.add_async_method(
            "_async_add",
            |_, this, (disk_path, archive_name): (String, Option<String>)| async move {
                let inner = Arc::clone(&this.inner);
                // mlua userdata borrows are exclusive: release `this` before
                // the first await so other coroutines can use the same object.
                drop(this);
                run_blocking(move || tar_add_impl(&inner, &disk_path, archive_name)).await
            },
        );

        // t:add_data(name, contents) / t:add_string(name, contents)
        // -- accepts string or Buffer
        for method_name in ["add_data", "add_string"] {
            methods.add_method(
                format!("_sync_{}", method_name),
                |_, this, (name, contents): (String, Value)| {
                    let bytes = extract_bytes(contents)?;
                    tar_add_data_impl(&this.inner, &name, bytes).map_err(mlua::Error::runtime)
                },
            );
            methods.add_async_method(
                format!("_async_{}", method_name),
                |_, this, (name, contents): (String, Value)| async move {
                    let bytes = extract_bytes(contents)?;
                    let inner = Arc::clone(&this.inner);
                // mlua userdata borrows are exclusive: release `this` before
                // the first await so other coroutines can use the same object.
                drop(this);
                    run_blocking(move || tar_add_data_impl(&inner, &name, bytes)).await
                },
            );
        }

        // t:add_dir(disk_path, prefix?)
        methods.add_method(
            "_sync_add_dir",
            |_, this, (disk_path, prefix): (String, Option<String>)| {
                tar_add_dir_impl(&this.inner, &disk_path, prefix).map_err(mlua::Error::runtime)
            },
        );
        methods.add_async_method(
            "_async_add_dir",
            |_, this, (disk_path, prefix): (String, Option<String>)| async move {
                let inner = Arc::clone(&this.inner);
                // mlua userdata borrows are exclusive: release `this` before
                // the first await so other coroutines can use the same object.
                drop(this);
                run_blocking(move || tar_add_dir_impl(&inner, &disk_path, prefix)).await
            },
        );

        // t:close()  (flushes tar/gzip streams — heavy, hybrid)
        methods.add_method("_sync_close", |_, this, _: ()| {
            tar_close_impl(&this.inner).map_err(mlua::Error::runtime)
        });
        methods.add_async_method("_async_close", |_, this, _: ()| async move {
            let inner = Arc::clone(&this.inner);
            // mlua userdata borrows are exclusive: release `this` before
            // the first await so other coroutines can use the same object.
            drop(this);
            run_blocking(move || tar_close_impl(&inner)).await
        });

        methods.add_meta_method(MetaMethod::Index, |lua, _this, key: Value| {
            hybrid_method_index(
                lua,
                key,
                &["add", "add_data", "add_string", "add_dir", "close"],
            )
        });
    }
}

fn tar_add_dir_recursive<W: Write>(
    builder: &mut tar::Builder<W>,
    root: &std::path::Path,
    current: &std::path::Path,
    prefix: &str,
) -> WorkResult<()> {
    for entry in std::fs::read_dir(current)
        .map_err(|e| format!("Failed to read dir '{}': {}", current.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Dir entry error: {}", e))?;
        let entry_path = entry.path();

        let relative = entry_path
            .strip_prefix(root)
            .map_err(|e| format!("Path error: {}", e))?;

        let archive_name = if prefix.is_empty() {
            relative.to_string_lossy().to_string()
        } else {
            format!(
                "{}/{}",
                prefix.trim_end_matches('/'),
                relative.to_string_lossy()
            )
        };
        let archive_name = archive_name.replace('\\', "/");

        if entry_path.is_dir() {
            builder
                .append_dir(&archive_name, &entry_path)
                .map_err(|e| format!("Failed to add dir '{}': {}", archive_name, e))?;
            tar_add_dir_recursive(builder, root, &entry_path, prefix)?;
        } else {
            let mut file = std::fs::File::open(&entry_path)
                .map_err(|e| format!("Failed to open '{}': {}", entry_path.display(), e))?;
            builder
                .append_file(&archive_name, &mut file)
                .map_err(|e| format!("Failed to add '{}': {}", archive_name, e))?;
        }
    }
    Ok(())
}

// ============================================================================
// GZIP (stateless compress/decompress) — accepts string or Buffer
// ============================================================================

/// Extract (bytes, level) from gzip.compress arguments. Runs on the Lua thread.
fn gzip_compress_args((data, options): (Value, Option<Table>)) -> mlua::Result<(Vec<u8>, u32)> {
    let bytes = extract_bytes(data)?;
    let level = options
        .and_then(|t| t.get::<u32>("level").ok())
        .unwrap_or(6);
    Ok((bytes, level))
}

fn gzip_compress_impl((bytes, level): (Vec<u8>, u32)) -> WorkResult<Vec<u8>> {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
    encoder
        .write_all(&bytes)
        .map_err(|e| format!("Gzip compress error: {}", e))?;
    encoder
        .finish()
        .map_err(|e| format!("Gzip compress error: {}", e))
}

fn gzip_decompress_impl(bytes: Vec<u8>) -> WorkResult<Vec<u8>> {
    use flate2::read::GzDecoder;

    let mut decoder = GzDecoder::new(&bytes[..]);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| format!("Gzip decompress error: {}", e))?;
    Ok(decompressed)
}

// ============================================================================
// Module-level implementations (pure, blocking-pool safe)
// ============================================================================

fn zip_open_impl(path: String) -> WorkResult<ZipSource> {
    let file =
        std::fs::File::open(&path).map_err(|e| format!("Failed to open '{}': {}", path, e))?;
    let archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Failed to read ZIP '{}': {}", path, e))?;
    Ok(ZipSource::File(archive))
}

fn zip_from_data_impl(bytes: Vec<u8>) -> WorkResult<ZipSource> {
    let cursor = std::io::Cursor::new(bytes);
    let archive = zip::ZipArchive::new(cursor)
        .map_err(|e| format!("Failed to read ZIP from memory: {}", e))?;
    Ok(ZipSource::Memory(archive))
}

fn zip_create_impl(path: String) -> WorkResult<zip::ZipWriter<std::fs::File>> {
    let file =
        std::fs::File::create(&path).map_err(|e| format!("Failed to create '{}': {}", path, e))?;
    Ok(zip::ZipWriter::new(file))
}

fn tar_open_impl(path: String) -> WorkResult<(String, bool)> {
    if !std::path::Path::new(&path).exists() {
        return Err(format!("File not found: '{}'", path));
    }

    let lower = path.to_lowercase();
    let is_gzipped = lower.ends_with(".tar.gz") || lower.ends_with(".tgz");

    Ok((path, is_gzipped))
}

fn tar_create_impl(path: String) -> WorkResult<TarWriterInner> {
    let lower = path.to_lowercase();
    let is_gzipped = lower.ends_with(".tar.gz") || lower.ends_with(".tgz");

    let file =
        std::fs::File::create(&path).map_err(|e| format!("Failed to create '{}': {}", path, e))?;

    let inner = if is_gzipped {
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        TarWriterInner::Gzipped(tar::Builder::new(encoder))
    } else {
        TarWriterInner::Plain(tar::Builder::new(file))
    };

    Ok(inner)
}

// ============================================================================
// Registration
// ============================================================================

/// Identity extraction for functions whose only argument is already plain data.
fn pass_string(path: String) -> mlua::Result<String> {
    Ok(path)
}

pub fn register(lua: &Lua) -> Result<Table> {
    let archive_table = lua.create_table()?;

    // archive.zip
    let zip_table = lua.create_table()?;
    zip_table.set(
        "open",
        hybrid_blocking(lua, pass_string, zip_open_impl, |_: &Lua, src: ZipSource| {
            Ok(ZipReader::new(src))
        })?,
    )?;
    zip_table.set(
        "create",
        hybrid_blocking(
            lua,
            pass_string,
            zip_create_impl,
            |_: &Lua, writer: zip::ZipWriter<std::fs::File>| Ok(ZipWriterObj::new(writer)),
        )?,
    )?;
    let zip_from_data = hybrid_blocking(
        lua,
        extract_bytes,
        zip_from_data_impl,
        |_: &Lua, src: ZipSource| Ok(ZipReader::new(src)),
    )?;
    zip_table.set("from_string", zip_from_data.clone())?;
    zip_table.set("from_buffer", zip_from_data)?;
    archive_table.set("zip", zip_table)?;

    // archive.tar
    let tar_table = lua.create_table()?;
    tar_table.set(
        "open",
        hybrid_blocking(
            lua,
            pass_string,
            tar_open_impl,
            |_: &Lua, (path, is_gzipped): (String, bool)| Ok(TarReader { path, is_gzipped }),
        )?,
    )?;
    tar_table.set(
        "create",
        hybrid_blocking(
            lua,
            pass_string,
            tar_create_impl,
            |_: &Lua, inner: TarWriterInner| Ok(TarWriterObj::new(inner)),
        )?,
    )?;
    archive_table.set("tar", tar_table)?;

    // archive.gzip
    let gzip_table = lua.create_table()?;
    gzip_table.set(
        "compress",
        hybrid_blocking(
            lua,
            gzip_compress_args,
            gzip_compress_impl,
            |lua: &Lua, data: Vec<u8>| lua.create_string(&data),
        )?,
    )?;
    gzip_table.set(
        "decompress",
        hybrid_blocking(
            lua,
            extract_bytes,
            gzip_decompress_impl,
            |lua: &Lua, data: Vec<u8>| lua.create_string(&data),
        )?,
    )?;
    gzip_table.set(
        "compress_buffer",
        hybrid_blocking(
            lua,
            gzip_compress_args,
            gzip_compress_impl,
            |_: &Lua, data: Vec<u8>| Ok(Buffer::from_bytes(data)),
        )?,
    )?;
    gzip_table.set(
        "decompress_buffer",
        hybrid_blocking(
            lua,
            extract_bytes,
            gzip_decompress_impl,
            |_: &Lua, data: Vec<u8>| Ok(Buffer::from_bytes(data)),
        )?,
    )?;
    archive_table.set("gzip", gzip_table)?;

    Ok(archive_table)
}
