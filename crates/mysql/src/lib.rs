//! CopperMoon MySQL/MariaDB Module
//!
//! Provides MySQL and MariaDB database bindings for CopperMoon Lua runtime.
//! This module provides a compatible interface with the SQLite module.
//!
//! All database operations run on the Tokio blocking thread pool via
//! `tokio::task::spawn_blocking`, so they never block the Lua event loop:
//! while a statement executes, other coroutines (HTTP handlers, timers, …)
//! keep running. From Lua's perspective the API is still synchronous.
//!
//! Pattern (see also `coppermoon_std/src/http.rs` and `crates/sqlite`):
//! 1. Parse Lua arguments into plain Rust data on the Lua thread.
//! 2. Run the MySQL work inside `spawn_blocking` (no Lua access there).
//! 3. Convert results back into Lua values after the `.await`, on the Lua thread.

use mlua::{FromLua, Lua, MultiValue, Result, Table, UserData, UserDataMethods, Value};
use mysql::prelude::*;
use mysql::{Opts, OptsBuilder, Pool, PooledConn, Row as MySqlRow};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// MySQL error types
#[derive(Debug, thiserror::Error)]
pub enum MysqlError {
    #[error("MySQL error: {0}")]
    Mysql(#[from] mysql::Error),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Query error: {0}")]
    Query(String),
    #[error("URL parse error: {0}")]
    UrlParse(#[from] mysql::UrlError),
}

/// MySQL Database connection wrapper
///
/// A single pooled connection is shared behind an `Arc<Mutex<..>>` so it can
/// be moved into `spawn_blocking` closures (`PooledConn` is `Send`). Keeping
/// one persistent connection (instead of grabbing a fresh one from the pool
/// per call) preserves the transaction semantics of `begin`/`commit`/
/// `rollback`, which must all run on the same connection.
///
/// Every field is `Arc`-wrapped so async methods can clone what they need and
/// release the userdata borrow (`drop(this)`) BEFORE the first `.await`:
/// mlua userdata borrows are exclusive, so holding `this` across an await
/// would make any reentrant or concurrent access to the same object fail
/// with "error borrowing userdata" (e.g. a `transaction` callback calling
/// `db:execute`).
pub struct Database {
    conn: Arc<Mutex<PooledConn>>,
    last_insert_id: Arc<AtomicU64>,
    affected_rows: Arc<AtomicU64>,
}

/// Connection options for MySQL
#[derive(Debug, Clone)]
pub struct ConnectionOptions {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 3306,
            user: "root".to_string(),
            password: None,
            database: None,
        }
    }
}

impl Database {
    /// Open a database connection with options
    pub fn open(options: ConnectionOptions) -> std::result::Result<Self, MysqlError> {
        let mut builder = OptsBuilder::new()
            .ip_or_hostname(Some(options.host))
            .tcp_port(options.port)
            .user(Some(options.user));

        if let Some(password) = options.password {
            builder = builder.pass(Some(password));
        }

        if let Some(database) = options.database {
            builder = builder.db_name(Some(database));
        }

        let opts: Opts = builder.into();
        let pool = Pool::new(opts)?;
        let conn = pool.get_conn()?;

        Ok(Self::from_conn(conn))
    }

    /// Open a database connection with URL
    pub fn open_url(url: &str) -> std::result::Result<Self, MysqlError> {
        let opts = Opts::from_url(url)?;
        let pool = Pool::new(opts)?;
        let conn = pool.get_conn()?;

        Ok(Self::from_conn(conn))
    }

    fn from_conn(conn: PooledConn) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
            last_insert_id: Arc::new(AtomicU64::new(0)),
            affected_rows: Arc::new(AtomicU64::new(0)),
        }
    }
}

/// Lock the shared connection, mapping poisoning to a Lua error.
fn lock_conn(conn: &Mutex<PooledConn>) -> Result<MutexGuard<'_, PooledConn>> {
    conn.lock()
        .map_err(|e| mlua::Error::runtime(format!("Lock error: {}", e)))
}

/// Run a blocking closure on the Tokio blocking pool.
///
/// The closure must not touch Lua: it works on plain Rust data only.
async fn run_blocking<T, F>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| mlua::Error::runtime(format!("Blocking task failed: {}", e)))?
}

/// Extract one MySQL row into thread-safe `(column, value)` pairs.
fn row_to_pairs(row: &MySqlRow) -> Vec<(String, mysql::Value)> {
    row.columns_ref()
        .iter()
        .enumerate()
        .map(|(col_idx, column)| {
            let col_name = column.name_str().to_string();
            let value: mysql::Value = row.get(col_idx).unwrap_or(mysql::Value::NULL);
            (col_name, value)
        })
        .collect()
}

/// Build a Lua table from a row's `(column, value)` pairs (Lua thread only).
fn row_to_table(lua: &Lua, row: Vec<(String, mysql::Value)>) -> Result<Table> {
    let row_table = lua.create_table()?;
    for (name, value) in row {
        let lua_value = mysql_value_to_lua(&value, lua)?;
        row_table.set(name, lua_value)?;
    }
    Ok(row_table)
}

/// Parse `(sql, params...)` from a Lua MultiValue (Lua thread only).
fn parse_sql_args(lua: &Lua, args: MultiValue) -> Result<(String, Vec<mysql::Value>)> {
    let mut args_iter = args.into_iter();

    // First argument is SQL
    let sql: String = match args_iter.next() {
        Some(Value::String(s)) => s.to_str()?.to_string(),
        _ => return Err(mlua::Error::external("First argument must be SQL string")),
    };

    // Remaining arguments are parameters, converted to plain mysql::Value
    // so they can be moved into a spawn_blocking closure.
    let params: Vec<mysql::Value> = args_iter
        .map(|v| MysqlValue::from_lua(v, lua).map(|p| p.to_mysql()))
        .collect::<Result<Vec<_>>>()?;

    Ok((sql, params))
}

/// Plain-Rust column description extracted inside `spawn_blocking`.
struct ColumnInfo {
    cid: i64,
    name: String,
    col_type: String,
    full_type: String,
    notnull: bool,
    default: Option<String>,
    pk: bool,
    extra: String,
}

impl UserData for Database {
    // INVARIANT (all async methods): mlua userdata borrows are exclusive, so
    // clone the needed Arc fields and `drop(this)` BEFORE the first `.await`.
    // Holding `this` across an await would break reentrant/concurrent access
    // to the same object ("error borrowing userdata").
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Execute a SQL statement (INSERT, UPDATE, DELETE, CREATE, etc.)
        methods.add_async_method("exec", |_lua, this, sql: String| async move {
            let conn = this.conn.clone();
            let affected_rows = this.affected_rows.clone();
            drop(this);

            let affected = run_blocking(move || {
                let mut conn = lock_conn(&conn)?;
                conn.query_drop(&sql).map_err(mlua::Error::external)?;
                Ok(conn.affected_rows())
            })
            .await?;

            affected_rows.store(affected, Ordering::Relaxed);
            Ok(Value::Integer(affected as i64))
        });

        // Execute a SQL statement with parameters
        methods.add_async_method("execute", |lua, this, args: MultiValue| async move {
            let (sql, params) = parse_sql_args(&lua, args)?;

            let conn = this.conn.clone();
            let affected_rows = this.affected_rows.clone();
            let last_insert_id = this.last_insert_id.clone();
            drop(this);

            let (affected, last_id) = run_blocking(move || {
                let mut conn = lock_conn(&conn)?;
                conn.exec_drop(&sql, params).map_err(mlua::Error::external)?;
                Ok((conn.affected_rows(), conn.last_insert_id()))
            })
            .await?;

            affected_rows.store(affected, Ordering::Relaxed);
            last_insert_id.store(last_id, Ordering::Relaxed);
            Ok(Value::Integer(affected as i64))
        });

        // Query and return all rows
        methods.add_async_method("query", |lua, this, args: MultiValue| async move {
            let (sql, params) = parse_sql_args(&lua, args)?;

            let conn = this.conn.clone();
            drop(this);

            let rows: Vec<Vec<(String, mysql::Value)>> = run_blocking(move || {
                let mut conn = lock_conn(&conn)?;
                let rows: Vec<MySqlRow> =
                    conn.exec(&sql, params).map_err(mlua::Error::external)?;
                Ok(rows.iter().map(row_to_pairs).collect())
            })
            .await?;

            // Build Lua tables on the Lua thread, after the blocking work.
            let result = lua.create_table()?;
            for (idx, row) in rows.into_iter().enumerate() {
                result.set(idx + 1, row_to_table(&lua, row)?)?;
            }

            Ok(Value::Table(result))
        });

        // Query and return first row only
        methods.add_async_method("query_row", |lua, this, args: MultiValue| async move {
            let (sql, params) = parse_sql_args(&lua, args)?;

            let conn = this.conn.clone();
            drop(this);

            let row: Option<Vec<(String, mysql::Value)>> = run_blocking(move || {
                let mut conn = lock_conn(&conn)?;
                let row: Option<MySqlRow> = conn
                    .exec_first(&sql, params)
                    .map_err(mlua::Error::external)?;
                Ok(row.as_ref().map(row_to_pairs))
            })
            .await?;

            match row {
                Some(row) => Ok(Value::Table(row_to_table(&lua, row)?)),
                None => Ok(Value::Nil),
            }
        });

        // Get last insert id
        methods.add_method("last_insert_id", |_, this, ()| {
            Ok(this.last_insert_id.load(Ordering::Relaxed) as i64)
        });

        // Alias for compatibility
        methods.add_method("last_insert_rowid", |_, this, ()| {
            Ok(this.last_insert_id.load(Ordering::Relaxed) as i64)
        });

        // Get changes count from last statement
        methods.add_method("changes", |_, this, ()| {
            Ok(this.affected_rows.load(Ordering::Relaxed) as i64)
        });

        // Begin transaction
        methods.add_async_method("begin", |_, this, ()| async move {
            let conn = this.conn.clone();
            drop(this);

            run_blocking(move || {
                let mut conn = lock_conn(&conn)?;
                conn.query_drop("START TRANSACTION")
                    .map_err(mlua::Error::external)
            })
            .await
        });

        // Commit transaction
        methods.add_async_method("commit", |_, this, ()| async move {
            let conn = this.conn.clone();
            drop(this);

            run_blocking(move || {
                let mut conn = lock_conn(&conn)?;
                conn.query_drop("COMMIT").map_err(mlua::Error::external)
            })
            .await
        });

        // Rollback transaction
        methods.add_async_method("rollback", |_, this, ()| async move {
            let conn = this.conn.clone();
            drop(this);

            run_blocking(move || {
                let mut conn = lock_conn(&conn)?;
                conn.query_drop("ROLLBACK").map_err(mlua::Error::external)
            })
            .await
        });

        // Transaction helper: START TRANSACTION, call func, COMMIT on success /
        // ROLLBACK on error. The Lua callback runs on the Lua thread via
        // `call_async` (never inside spawn_blocking); only the SQL statements
        // go through the blocking pool.
        //
        // `this` is dropped before any await so the callback can re-borrow the
        // same userdata (e.g. call `db:execute`) without a borrow conflict.
        methods.add_async_method("transaction", |_lua, this, func: mlua::Function| async move {
            let conn = this.conn.clone();
            drop(this);

            let begin_conn = conn.clone();
            run_blocking(move || {
                let mut conn = lock_conn(&begin_conn)?;
                conn.query_drop("START TRANSACTION")
                    .map_err(mlua::Error::external)
            })
            .await?;

            match func.call_async::<()>(()).await {
                Ok(_) => {
                    run_blocking(move || {
                        let mut conn = lock_conn(&conn)?;
                        conn.query_drop("COMMIT").map_err(mlua::Error::external)
                    })
                    .await?;
                    Ok(true)
                }
                Err(e) => {
                    let _ = run_blocking(move || {
                        let mut conn = lock_conn(&conn)?;
                        let _ = conn.query_drop("ROLLBACK");
                        Ok(())
                    })
                    .await;
                    Err(e)
                }
            }
        });

        // Close connection
        methods.add_method("close", |_, _this, ()| {
            // Connection will be returned to pool when dropped
            Ok(())
        });

        // Check if table exists
        methods.add_async_method("table_exists", |_, this, table_name: String| async move {
            let conn = this.conn.clone();
            drop(this);

            run_blocking(move || {
                let mut conn = lock_conn(&conn)?;

                let sql = "SELECT COUNT(*) as cnt FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = ?";
                let row: Option<MySqlRow> = conn
                    .exec_first(sql, (table_name,))
                    .map_err(mlua::Error::external)?;

                Ok(match row {
                    Some(row) => row.get::<i64, _>("cnt").unwrap_or(0) > 0,
                    None => false,
                })
            })
            .await
        });

        // Get table info (columns)
        methods.add_async_method("table_info", |lua, this, table_name: String| async move {
            let conn = this.conn.clone();
            drop(this);

            let columns: Vec<ColumnInfo> = run_blocking(move || {
                let mut conn = lock_conn(&conn)?;

                let sql = r#"
                    SELECT
                        ORDINAL_POSITION as cid,
                        COLUMN_NAME as name,
                        DATA_TYPE as type,
                        IS_NULLABLE = 'NO' as notnull,
                        COLUMN_DEFAULT as `default`,
                        COLUMN_KEY = 'PRI' as pk,
                        COLUMN_TYPE as full_type,
                        EXTRA as extra
                    FROM information_schema.columns
                    WHERE table_schema = DATABASE() AND table_name = ?
                    ORDER BY ORDINAL_POSITION
                "#;

                let rows: Vec<MySqlRow> = conn
                    .exec(sql, (table_name,))
                    .map_err(mlua::Error::external)?;

                Ok(rows
                    .iter()
                    .map(|row| ColumnInfo {
                        cid: row.get("cid").unwrap_or(0),
                        name: row.get("name").unwrap_or_default(),
                        col_type: row.get("type").unwrap_or_default(),
                        full_type: row.get("full_type").unwrap_or_default(),
                        notnull: row.get::<i64, _>("notnull").unwrap_or(0) != 0,
                        default: row.get("default"),
                        pk: row.get::<i64, _>("pk").unwrap_or(0) != 0,
                        extra: row.get("extra").unwrap_or_default(),
                    })
                    .collect())
            })
            .await?;

            let result = lua.create_table()?;
            for (idx, col) in columns.into_iter().enumerate() {
                let col_table = lua.create_table()?;
                col_table.set("cid", col.cid)?;
                col_table.set("name", col.name)?;
                col_table.set("type", col.col_type)?;
                col_table.set("full_type", col.full_type)?;
                col_table.set("notnull", col.notnull)?;
                col_table.set("default", col.default)?;
                col_table.set("pk", col.pk)?;
                col_table.set("extra", col.extra)?;

                result.set(idx + 1, col_table)?;
            }

            Ok(Value::Table(result))
        });

        // Get index list
        methods.add_async_method("index_list", |lua, this, table_name: String| async move {
            let conn = this.conn.clone();
            drop(this);

            let indexes: Vec<(String, bool)> = run_blocking(move || {
                let mut conn = lock_conn(&conn)?;

                let sql = r#"
                    SELECT DISTINCT
                        INDEX_NAME as name,
                        NOT NON_UNIQUE as `unique`
                    FROM information_schema.statistics
                    WHERE table_schema = DATABASE() AND table_name = ?
                "#;

                let rows: Vec<MySqlRow> = conn
                    .exec(sql, (table_name,))
                    .map_err(mlua::Error::external)?;

                Ok(rows
                    .iter()
                    .map(|row| {
                        let name: String = row.get("name").unwrap_or_default();
                        let unique: i64 = row.get("unique").unwrap_or(0);
                        (name, unique != 0)
                    })
                    .collect())
            })
            .await?;

            let result = lua.create_table()?;
            for (idx, (name, unique)) in indexes.into_iter().enumerate() {
                let index_table = lua.create_table()?;
                index_table.set("name", name)?;
                index_table.set("unique", unique)?;

                result.set(idx + 1, index_table)?;
            }

            Ok(Value::Table(result))
        });

        // Ping to check connection
        methods.add_async_method("ping", |_, this, ()| async move {
            let conn = this.conn.clone();
            drop(this);

            run_blocking(move || {
                let mut conn = lock_conn(&conn)?;
                Ok(conn.query_drop("SELECT 1").is_ok())
            })
            .await
        });

        // Get server version
        //
        // No network round-trip, but it needs the connection mutex: run it on
        // the blocking pool so a long-running query on another coroutine never
        // stalls the Lua event loop while we wait for the lock.
        methods.add_async_method("server_version", |_, this, ()| async move {
            let conn = this.conn.clone();
            drop(this);

            run_blocking(move || {
                let conn = lock_conn(&conn)?;
                Ok(conn.server_version())
            })
            .await
        });
    }
}

/// Convert MySQL value to Lua value
fn mysql_value_to_lua(value: &mysql::Value, lua: &Lua) -> Result<Value> {
    match value {
        mysql::Value::NULL => Ok(Value::Nil),
        mysql::Value::Int(i) => Ok(Value::Integer(*i)),
        mysql::Value::UInt(u) => Ok(Value::Integer(*u as i64)),
        mysql::Value::Float(f) => Ok(Value::Number(*f as f64)),
        mysql::Value::Double(d) => Ok(Value::Number(*d)),
        mysql::Value::Bytes(b) => {
            // Try to convert to string first
            match String::from_utf8(b.clone()) {
                Ok(s) => Ok(Value::String(lua.create_string(&s)?)),
                Err(_) => Ok(Value::String(lua.create_string(b)?)),
            }
        }
        mysql::Value::Date(year, month, day, hour, min, sec, _micro) => {
            let formatted = format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                year, month, day, hour, min, sec
            );
            Ok(Value::String(lua.create_string(&formatted)?))
        }
        mysql::Value::Time(negative, days, hours, minutes, seconds, _micro) => {
            let total_hours = (*days as u32) * 24 + (*hours as u32);
            let sign = if *negative { "-" } else { "" };
            let formatted = format!("{}{}:{:02}:{:02}", sign, total_hours, minutes, seconds);
            Ok(Value::String(lua.create_string(&formatted)?))
        }
    }
}

/// Wrapper for MySQL values that can be converted to/from Lua
#[derive(Debug, Clone)]
enum MysqlValue {
    Null,
    Integer(i64),
    Float(f64),
    Text(String),
}

impl MysqlValue {
    fn to_mysql(&self) -> mysql::Value {
        match self {
            MysqlValue::Null => mysql::Value::NULL,
            MysqlValue::Integer(i) => mysql::Value::Int(*i),
            MysqlValue::Float(f) => mysql::Value::Double(*f),
            MysqlValue::Text(s) => mysql::Value::Bytes(s.as_bytes().to_vec()),
        }
    }
}

impl FromLua for MysqlValue {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::Nil => Ok(MysqlValue::Null),
            Value::Boolean(b) => Ok(MysqlValue::Integer(if b { 1 } else { 0 })),
            Value::Integer(i) => Ok(MysqlValue::Integer(i)),
            Value::Number(n) => Ok(MysqlValue::Float(n)),
            Value::String(s) => Ok(MysqlValue::Text(s.to_str()?.to_string())),
            _ => Err(mlua::Error::external("Unsupported value type for MySQL")),
        }
    }
}

/// Connection target parsed from Lua arguments (Lua thread only).
enum ConnectTarget {
    Options(ConnectionOptions),
    Url(String),
}

impl ConnectTarget {
    fn open(self) -> std::result::Result<Database, MysqlError> {
        match self {
            ConnectTarget::Options(opts) => Database::open(opts),
            ConnectTarget::Url(url) => Database::open_url(&url),
        }
    }
}

/// Register the mysql module with the Lua state
pub fn register(lua: &Lua) -> Result<Table> {
    let module = lua.create_table()?;

    // mysql.connect(options) - Connect with options table or URL string.
    // Connecting is network I/O, so it runs on the blocking pool too.
    module.set(
        "connect",
        lua.create_async_function(|_lua, options: Value| async move {
            // Parse Lua arguments on the Lua thread, before spawn_blocking.
            let target = match options {
                Value::Table(t) => {
                    let host: String = t.get("host").unwrap_or_else(|_| "localhost".to_string());
                    let port: u16 = t.get("port").unwrap_or(3306);
                    let user: String = t.get("user").unwrap_or_else(|_| "root".to_string());
                    let password: Option<String> = t.get("password").ok();
                    let database: Option<String> = t.get("database").ok();

                    ConnectTarget::Options(ConnectionOptions {
                        host,
                        port,
                        user,
                        password,
                        database,
                    })
                }
                Value::String(s) => ConnectTarget::Url(s.to_str()?.to_string()),
                _ => {
                    return Err(mlua::Error::external(
                        "connect() requires options table or URL string",
                    ))
                }
            };

            run_blocking(move || target.open().map_err(mlua::Error::external)).await
        })?,
    )?;

    // mysql.open(url) - Open with URL string (alias)
    module.set(
        "open",
        lua.create_async_function(|_, url: String| async move {
            run_blocking(move || Database::open_url(&url).map_err(mlua::Error::external)).await
        })?,
    )?;

    // mysql.version() - Get client library version
    module.set(
        "version",
        lua.create_function(|_, ()| Ok("mysql-rs 25.0"))?,
    )?;

    Ok(module)
}

/// Register the mysql module globally
pub fn register_global(lua: &Lua) -> Result<()> {
    let module = register(lua)?;
    lua.globals().set("mysql", module)?;
    Ok(())
}
