//! Archive module for CopperMoon
//!
//! Provides compression and archive operations: ZIP, TAR/TAR.GZ, and raw GZIP.

use crate::buffer::Buffer;
use coppermoon_core::Result;
use mlua::{Lua, Table, UserData, UserDataMethods, Value};
use std::io::{Read, Write};
use std::sync::Mutex;

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

struct ZipReader {
    inner: Mutex<Option<ZipSource>>,
}

impl UserData for ZipReader {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // z:list() -> array of {name, size, compressed_size, is_dir}
        methods.add_method("list", |lua, this, _: ()| {
            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            let archive = guard.as_mut()
                .ok_or_else(|| mlua::Error::runtime("ZipReader is already closed"))?;

            let result = lua.create_table()?;
            for i in 0..archive.len() {
                let file = archive.by_index(i)
                    .map_err(|e| mlua::Error::runtime(format!("ZIP entry error: {}", e)))?;
                let entry = lua.create_table()?;
                entry.set("name", file.name().to_string())?;
                entry.set("size", file.size())?;
                entry.set("compressed_size", file.compressed_size())?;
                entry.set("is_dir", file.is_dir())?;
                result.set(i + 1, entry)?;
            }
            Ok(result)
        });

        // z:read(name) -> string
        methods.add_method("read", |lua, this, name: String| {
            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            let archive = guard.as_mut()
                .ok_or_else(|| mlua::Error::runtime("ZipReader is already closed"))?;

            let mut file = archive.by_name(&name)
                .map_err(|e| mlua::Error::runtime(format!("File '{}' not found in ZIP: {}", name, e)))?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)
                .map_err(|e| mlua::Error::runtime(format!("Failed to read '{}': {}", name, e)))?;
            lua.create_string(&buf)
        });

        // z:read_buffer(name) -> Buffer
        methods.add_method("read_buffer", |_, this, name: String| {
            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            let archive = guard.as_mut()
                .ok_or_else(|| mlua::Error::runtime("ZipReader is already closed"))?;

            let mut file = archive.by_name(&name)
                .map_err(|e| mlua::Error::runtime(format!("File '{}' not found in ZIP: {}", name, e)))?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)
                .map_err(|e| mlua::Error::runtime(format!("Failed to read '{}': {}", name, e)))?;
            Ok(Buffer::from_bytes(buf))
        });

        // z:exists(name) -> boolean
        methods.add_method("exists", |_, this, name: String| {
            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            let archive = guard.as_mut()
                .ok_or_else(|| mlua::Error::runtime("ZipReader is already closed"))?;
            let result = archive.by_name(&name).is_ok();
            Ok(result)
        });

        // z:extract(output_dir, filter?)
        methods.add_method("extract", |_, this, (output_dir, filter): (String, Option<Table>)| {
            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            let archive = guard.as_mut()
                .ok_or_else(|| mlua::Error::runtime("ZipReader is already closed"))?;

            let filter_set: Option<std::collections::HashSet<String>> = filter.map(|t| {
                let mut set = std::collections::HashSet::new();
                for i in 1..=t.raw_len() {
                    if let Ok(name) = t.get::<String>(i) {
                        set.insert(name);
                    }
                }
                set
            });

            let out_path = std::path::Path::new(&output_dir);

            for i in 0..archive.len() {
                let mut file = archive.by_index(i)
                    .map_err(|e| mlua::Error::runtime(format!("ZIP entry error: {}", e)))?;

                let name = file.name().to_string();

                if let Some(ref filter) = filter_set {
                    if !filter.contains(&name) {
                        continue;
                    }
                }

                let target = out_path.join(&name);

                // Security: prevent path traversal
                let canonical_out = out_path.canonicalize().unwrap_or_else(|_| out_path.to_path_buf());
                if let Ok(canonical_target) = target.canonicalize() {
                    if !canonical_target.starts_with(&canonical_out) {
                        return Err(mlua::Error::runtime(format!(
                            "ZIP path traversal detected: '{}'", name
                        )));
                    }
                }

                if file.is_dir() {
                    std::fs::create_dir_all(&target)
                        .map_err(|e| mlua::Error::runtime(format!("Failed to create dir: {}", e)))?;
                } else {
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| mlua::Error::runtime(format!("Failed to create dir: {}", e)))?;
                    }
                    let mut out_file = std::fs::File::create(&target)
                        .map_err(|e| mlua::Error::runtime(format!("Failed to create file: {}", e)))?;
                    std::io::copy(&mut file, &mut out_file)
                        .map_err(|e| mlua::Error::runtime(format!("Failed to extract file: {}", e)))?;
                }
            }
            Ok(())
        });

        // z:close()
        methods.add_method("close", |_, this, _: ()| {
            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            guard.take();
            Ok(())
        });
    }
}

// ============================================================================
// ZIP Writer
// ============================================================================

struct ZipWriterObj {
    inner: Mutex<Option<zip::ZipWriter<std::fs::File>>>,
}

impl UserData for ZipWriterObj {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // z:add(disk_path, archive_name?)
        methods.add_method("add", |_, this, (disk_path, archive_name): (String, Option<String>)| {
            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            let writer = guard.as_mut()
                .ok_or_else(|| mlua::Error::runtime("ZipWriter is already closed"))?;

            let name = archive_name.unwrap_or_else(|| {
                std::path::Path::new(&disk_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| disk_path.clone())
            });

            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            writer.start_file(&name, options)
                .map_err(|e| mlua::Error::runtime(format!("Failed to start ZIP entry '{}': {}", name, e)))?;

            let mut file = std::fs::File::open(&disk_path)
                .map_err(|e| mlua::Error::runtime(format!("Failed to open '{}': {}", disk_path, e)))?;
            std::io::copy(&mut file, writer)
                .map_err(|e| mlua::Error::runtime(format!("Failed to write '{}' to ZIP: {}", name, e)))?;

            Ok(())
        });

        // z:add_data(name, contents) -- accepts string or Buffer
        methods.add_method("add_data", |_, this, (name, contents): (String, Value)| {
            let bytes = extract_bytes(contents)?;

            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            let writer = guard.as_mut()
                .ok_or_else(|| mlua::Error::runtime("ZipWriter is already closed"))?;

            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            writer.start_file(&name, options)
                .map_err(|e| mlua::Error::runtime(format!("Failed to start ZIP entry '{}': {}", name, e)))?;

            writer.write_all(&bytes)
                .map_err(|e| mlua::Error::runtime(format!("Failed to write '{}': {}", name, e)))?;

            Ok(())
        });

        // z:add_string(name, contents) -- accepts string or Buffer (alias for add_data)
        methods.add_method("add_string", |_, this, (name, contents): (String, Value)| {
            let bytes = extract_bytes(contents)?;

            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            let writer = guard.as_mut()
                .ok_or_else(|| mlua::Error::runtime("ZipWriter is already closed"))?;

            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            writer.start_file(&name, options)
                .map_err(|e| mlua::Error::runtime(format!("Failed to start ZIP entry '{}': {}", name, e)))?;

            writer.write_all(&bytes)
                .map_err(|e| mlua::Error::runtime(format!("Failed to write '{}': {}", name, e)))?;

            Ok(())
        });

        // z:add_dir(disk_path, prefix?)
        methods.add_method("add_dir", |_, this, (disk_path, prefix): (String, Option<String>)| {
            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            let writer = guard.as_mut()
                .ok_or_else(|| mlua::Error::runtime("ZipWriter is already closed"))?;

            let base = std::path::Path::new(&disk_path);
            let prefix = prefix.unwrap_or_default();

            zip_add_dir_recursive(writer, base, base, &prefix)?;
            Ok(())
        });

        // z:close()
        methods.add_method("close", |_, this, _: ()| {
            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            let writer = guard.take()
                .ok_or_else(|| mlua::Error::runtime("ZipWriter is already closed"))?;
            writer.finish()
                .map_err(|e| mlua::Error::runtime(format!("Failed to finalize ZIP: {}", e)))?;
            Ok(())
        });
    }
}

fn zip_add_dir_recursive(
    writer: &mut zip::ZipWriter<std::fs::File>,
    root: &std::path::Path,
    current: &std::path::Path,
    prefix: &str,
) -> mlua::Result<()> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for entry in std::fs::read_dir(current)
        .map_err(|e| mlua::Error::runtime(format!("Failed to read dir '{}': {}", current.display(), e)))?
    {
        let entry = entry
            .map_err(|e| mlua::Error::runtime(format!("Dir entry error: {}", e)))?;
        let entry_path = entry.path();

        let relative = entry_path.strip_prefix(root)
            .map_err(|e| mlua::Error::runtime(format!("Path error: {}", e)))?;

        let archive_name = if prefix.is_empty() {
            relative.to_string_lossy().to_string()
        } else {
            format!("{}/{}", prefix.trim_end_matches('/'), relative.to_string_lossy())
        };

        // Normalize path separators to forward slashes
        let archive_name = archive_name.replace('\\', "/");

        if entry_path.is_dir() {
            writer.add_directory(format!("{}/", archive_name), options)
                .map_err(|e| mlua::Error::runtime(format!("Failed to add dir '{}': {}", archive_name, e)))?;
            zip_add_dir_recursive(writer, root, &entry_path, prefix)?;
        } else {
            writer.start_file(&archive_name, options)
                .map_err(|e| mlua::Error::runtime(format!("Failed to start '{}': {}", archive_name, e)))?;
            let mut file = std::fs::File::open(&entry_path)
                .map_err(|e| mlua::Error::runtime(format!("Failed to open '{}': {}", entry_path.display(), e)))?;
            std::io::copy(&mut file, writer)
                .map_err(|e| mlua::Error::runtime(format!("Failed to write '{}': {}", archive_name, e)))?;
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

fn open_tar_archive(path: &str, is_gzipped: bool) -> mlua::Result<tar::Archive<Box<dyn Read>>> {
    let file = std::fs::File::open(path)
        .map_err(|e| mlua::Error::runtime(format!("Failed to open '{}': {}", path, e)))?;

    let reader: Box<dyn Read> = if is_gzipped {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };

    Ok(tar::Archive::new(reader))
}

impl UserData for TarReader {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // t:list() -> array of {name, size, is_dir}
        methods.add_method("list", |lua, this, _: ()| {
            let mut archive = open_tar_archive(&this.path, this.is_gzipped)?;
            let entries = archive.entries()
                .map_err(|e| mlua::Error::runtime(format!("Failed to read tar entries: {}", e)))?;

            let result = lua.create_table()?;
            let mut index = 1;

            for entry in entries {
                let entry = entry
                    .map_err(|e| mlua::Error::runtime(format!("Tar entry error: {}", e)))?;
                let header = entry.header();

                let info = lua.create_table()?;
                info.set("name", entry.path()
                    .map_err(|e| mlua::Error::runtime(format!("Path error: {}", e)))?
                    .to_string_lossy()
                    .to_string())?;
                info.set("size", header.size()
                    .map_err(|e| mlua::Error::runtime(format!("Size error: {}", e)))?)?;
                info.set("is_dir", header.entry_type().is_dir())?;

                result.set(index, info)?;
                index += 1;
            }
            Ok(result)
        });

        // t:read(name) -> string
        methods.add_method("read", |lua, this, name: String| {
            let mut archive = open_tar_archive(&this.path, this.is_gzipped)?;
            let entries = archive.entries()
                .map_err(|e| mlua::Error::runtime(format!("Failed to read tar entries: {}", e)))?;

            for entry in entries {
                let mut entry = entry
                    .map_err(|e| mlua::Error::runtime(format!("Tar entry error: {}", e)))?;
                let entry_path = entry.path()
                    .map_err(|e| mlua::Error::runtime(format!("Path error: {}", e)))?
                    .to_string_lossy()
                    .to_string();

                if entry_path == name {
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf)
                        .map_err(|e| mlua::Error::runtime(format!("Failed to read '{}': {}", name, e)))?;
                    return Ok(Value::String(lua.create_string(&buf)?));
                }
            }

            Err(mlua::Error::runtime(format!("File '{}' not found in tar archive", name)))
        });

        // t:read_buffer(name) -> Buffer
        methods.add_method("read_buffer", |_, this, name: String| {
            let mut archive = open_tar_archive(&this.path, this.is_gzipped)?;
            let entries = archive.entries()
                .map_err(|e| mlua::Error::runtime(format!("Failed to read tar entries: {}", e)))?;

            for entry in entries {
                let mut entry = entry
                    .map_err(|e| mlua::Error::runtime(format!("Tar entry error: {}", e)))?;
                let entry_path = entry.path()
                    .map_err(|e| mlua::Error::runtime(format!("Path error: {}", e)))?
                    .to_string_lossy()
                    .to_string();

                if entry_path == name {
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf)
                        .map_err(|e| mlua::Error::runtime(format!("Failed to read '{}': {}", name, e)))?;
                    return Ok(Buffer::from_bytes(buf));
                }
            }

            Err(mlua::Error::runtime(format!("File '{}' not found in tar archive", name)))
        });

        // t:extract(output_dir)
        methods.add_method("extract", |_, this, output_dir: String| {
            let mut archive = open_tar_archive(&this.path, this.is_gzipped)?;
            archive.unpack(&output_dir)
                .map_err(|e| mlua::Error::runtime(format!("Failed to extract tar to '{}': {}", output_dir, e)))?;
            Ok(())
        });

        // t:close() -- no-op for consistency
        methods.add_method("close", |_, _this, _: ()| {
            Ok(())
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
    inner: Mutex<Option<TarWriterInner>>,
}

macro_rules! with_tar_builder {
    ($guard:expr, $builder:ident => $body:expr) => {
        match $guard.as_mut().ok_or_else(|| mlua::Error::runtime("TarWriter is closed"))? {
            TarWriterInner::Plain(ref mut $builder) => { $body }
            TarWriterInner::Gzipped(ref mut $builder) => { $body }
        }
    };
}

impl UserData for TarWriterObj {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // t:add(disk_path, archive_name?)
        methods.add_method("add", |_, this, (disk_path, archive_name): (String, Option<String>)| {
            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let name = archive_name.unwrap_or_else(|| {
                std::path::Path::new(&disk_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| disk_path.clone())
            });
            let name = name.replace('\\', "/");

            let mut file = std::fs::File::open(&disk_path)
                .map_err(|e| mlua::Error::runtime(format!("Failed to open '{}': {}", disk_path, e)))?;

            with_tar_builder!(guard, builder => {
                builder.append_file(&name, &mut file)
                    .map_err(|e| mlua::Error::runtime(format!("Failed to add '{}' to tar: {}", name, e)))?;
            });

            Ok(())
        });

        // t:add_data(name, contents) -- accepts string or Buffer
        methods.add_method("add_data", |_, this, (name, contents): (String, Value)| {
            let bytes = extract_bytes(contents)?;

            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let name = name.replace('\\', "/");

            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            with_tar_builder!(guard, builder => {
                builder.append_data(&mut header, &name, &bytes[..])
                    .map_err(|e| mlua::Error::runtime(format!("Failed to add '{}': {}", name, e)))?;
            });

            Ok(())
        });

        // t:add_string(name, contents) -- accepts string or Buffer (alias for add_data)
        methods.add_method("add_string", |_, this, (name, contents): (String, Value)| {
            let bytes = extract_bytes(contents)?;

            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let name = name.replace('\\', "/");

            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            with_tar_builder!(guard, builder => {
                builder.append_data(&mut header, &name, &bytes[..])
                    .map_err(|e| mlua::Error::runtime(format!("Failed to add '{}': {}", name, e)))?;
            });

            Ok(())
        });

        // t:add_dir(disk_path, prefix?)
        methods.add_method("add_dir", |_, this, (disk_path, prefix): (String, Option<String>)| {
            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;

            let base = std::path::Path::new(&disk_path);
            let prefix_str = prefix.unwrap_or_default();

            with_tar_builder!(guard, builder => {
                tar_add_dir_recursive(builder, base, base, &prefix_str)?;
            });

            Ok(())
        });

        // t:close()
        methods.add_method("close", |_, this, _: ()| {
            let mut guard = this.inner.lock()
                .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))?;
            let inner = guard.take()
                .ok_or_else(|| mlua::Error::runtime("TarWriter is already closed"))?;

            match inner {
                TarWriterInner::Plain(builder) => {
                    builder.into_inner()
                        .map_err(|e| mlua::Error::runtime(format!("Failed to finalize tar: {}", e)))?;
                }
                TarWriterInner::Gzipped(builder) => {
                    let gz_encoder = builder.into_inner()
                        .map_err(|e| mlua::Error::runtime(format!("Failed to finalize tar: {}", e)))?;
                    gz_encoder.finish()
                        .map_err(|e| mlua::Error::runtime(format!("Failed to finalize gzip: {}", e)))?;
                }
            }

            Ok(())
        });
    }
}

fn tar_add_dir_recursive<W: Write>(
    builder: &mut tar::Builder<W>,
    root: &std::path::Path,
    current: &std::path::Path,
    prefix: &str,
) -> mlua::Result<()> {
    for entry in std::fs::read_dir(current)
        .map_err(|e| mlua::Error::runtime(format!("Failed to read dir '{}': {}", current.display(), e)))?
    {
        let entry = entry
            .map_err(|e| mlua::Error::runtime(format!("Dir entry error: {}", e)))?;
        let entry_path = entry.path();

        let relative = entry_path.strip_prefix(root)
            .map_err(|e| mlua::Error::runtime(format!("Path error: {}", e)))?;

        let archive_name = if prefix.is_empty() {
            relative.to_string_lossy().to_string()
        } else {
            format!("{}/{}", prefix.trim_end_matches('/'), relative.to_string_lossy())
        };
        let archive_name = archive_name.replace('\\', "/");

        if entry_path.is_dir() {
            builder.append_dir(&archive_name, &entry_path)
                .map_err(|e| mlua::Error::runtime(format!("Failed to add dir '{}': {}", archive_name, e)))?;
            tar_add_dir_recursive(builder, root, &entry_path, prefix)?;
        } else {
            let mut file = std::fs::File::open(&entry_path)
                .map_err(|e| mlua::Error::runtime(format!("Failed to open '{}': {}", entry_path.display(), e)))?;
            builder.append_file(&archive_name, &mut file)
                .map_err(|e| mlua::Error::runtime(format!("Failed to add '{}': {}", archive_name, e)))?;
        }
    }
    Ok(())
}

// ============================================================================
// GZIP (stateless compress/decompress) — accepts string or Buffer
// ============================================================================

fn gzip_compress(lua: &Lua, (data, options): (Value, Option<Table>)) -> mlua::Result<mlua::String> {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let bytes = extract_bytes(data)?;

    let level = options
        .and_then(|t| t.get::<u32>("level").ok())
        .unwrap_or(6);

    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(&bytes)
        .map_err(|e| mlua::Error::runtime(format!("Gzip compress error: {}", e)))?;
    let compressed = encoder.finish()
        .map_err(|e| mlua::Error::runtime(format!("Gzip compress error: {}", e)))?;

    lua.create_string(&compressed)
}

fn gzip_decompress(lua: &Lua, data: Value) -> mlua::Result<mlua::String> {
    use flate2::read::GzDecoder;

    let bytes = extract_bytes(data)?;
    let mut decoder = GzDecoder::new(&bytes[..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)
        .map_err(|e| mlua::Error::runtime(format!("Gzip decompress error: {}", e)))?;

    lua.create_string(&decompressed)
}

fn gzip_compress_buffer(_: &Lua, (data, options): (Value, Option<Table>)) -> mlua::Result<Buffer> {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let bytes = extract_bytes(data)?;

    let level = options
        .and_then(|t| t.get::<u32>("level").ok())
        .unwrap_or(6);

    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(&bytes)
        .map_err(|e| mlua::Error::runtime(format!("Gzip compress error: {}", e)))?;
    let compressed = encoder.finish()
        .map_err(|e| mlua::Error::runtime(format!("Gzip compress error: {}", e)))?;

    Ok(Buffer::from_bytes(compressed))
}

fn gzip_decompress_buffer(_: &Lua, data: Value) -> mlua::Result<Buffer> {
    use flate2::read::GzDecoder;

    let bytes = extract_bytes(data)?;
    let mut decoder = GzDecoder::new(&bytes[..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)
        .map_err(|e| mlua::Error::runtime(format!("Gzip decompress error: {}", e)))?;

    Ok(Buffer::from_bytes(decompressed))
}

// ============================================================================
// Module-level functions
// ============================================================================

fn zip_open(_: &Lua, path: String) -> mlua::Result<ZipReader> {
    let file = std::fs::File::open(&path)
        .map_err(|e| mlua::Error::runtime(format!("Failed to open '{}': {}", path, e)))?;
    let archive = zip::ZipArchive::new(file)
        .map_err(|e| mlua::Error::runtime(format!("Failed to read ZIP '{}': {}", path, e)))?;
    Ok(ZipReader {
        inner: Mutex::new(Some(ZipSource::File(archive))),
    })
}

fn zip_from_data(_: &Lua, data: Value) -> mlua::Result<ZipReader> {
    let bytes = extract_bytes(data)?;
    let cursor = std::io::Cursor::new(bytes);
    let archive = zip::ZipArchive::new(cursor)
        .map_err(|e| mlua::Error::runtime(format!("Failed to read ZIP from memory: {}", e)))?;
    Ok(ZipReader {
        inner: Mutex::new(Some(ZipSource::Memory(archive))),
    })
}

fn zip_create(_: &Lua, path: String) -> mlua::Result<ZipWriterObj> {
    let file = std::fs::File::create(&path)
        .map_err(|e| mlua::Error::runtime(format!("Failed to create '{}': {}", path, e)))?;
    let writer = zip::ZipWriter::new(file);
    Ok(ZipWriterObj {
        inner: Mutex::new(Some(writer)),
    })
}

fn tar_open(_: &Lua, path: String) -> mlua::Result<TarReader> {
    if !std::path::Path::new(&path).exists() {
        return Err(mlua::Error::runtime(format!("File not found: '{}'", path)));
    }

    let lower = path.to_lowercase();
    let is_gzipped = lower.ends_with(".tar.gz") || lower.ends_with(".tgz");

    Ok(TarReader { path, is_gzipped })
}

fn tar_create(_: &Lua, path: String) -> mlua::Result<TarWriterObj> {
    let lower = path.to_lowercase();
    let is_gzipped = lower.ends_with(".tar.gz") || lower.ends_with(".tgz");

    let file = std::fs::File::create(&path)
        .map_err(|e| mlua::Error::runtime(format!("Failed to create '{}': {}", path, e)))?;

    let inner = if is_gzipped {
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        TarWriterInner::Gzipped(tar::Builder::new(encoder))
    } else {
        TarWriterInner::Plain(tar::Builder::new(file))
    };

    Ok(TarWriterObj {
        inner: Mutex::new(Some(inner)),
    })
}

// ============================================================================
// Registration
// ============================================================================

pub fn register(lua: &Lua) -> Result<Table> {
    let archive_table = lua.create_table()?;

    // archive.zip
    let zip_table = lua.create_table()?;
    zip_table.set("open", lua.create_function(zip_open)?)?;
    zip_table.set("create", lua.create_function(zip_create)?)?;
    zip_table.set("from_string", lua.create_function(zip_from_data)?)?;
    zip_table.set("from_buffer", lua.create_function(zip_from_data)?)?;
    archive_table.set("zip", zip_table)?;

    // archive.tar
    let tar_table = lua.create_table()?;
    tar_table.set("open", lua.create_function(tar_open)?)?;
    tar_table.set("create", lua.create_function(tar_create)?)?;
    archive_table.set("tar", tar_table)?;

    // archive.gzip
    let gzip_table = lua.create_table()?;
    gzip_table.set("compress", lua.create_function(gzip_compress)?)?;
    gzip_table.set("decompress", lua.create_function(gzip_decompress)?)?;
    gzip_table.set("compress_buffer", lua.create_function(gzip_compress_buffer)?)?;
    gzip_table.set("decompress_buffer", lua.create_function(gzip_decompress_buffer)?)?;
    archive_table.set("gzip", gzip_table)?;

    Ok(archive_table)
}
