//! CopperMoon SQLite Module
//!
//! Provides SQLite database bindings for CopperMoon Lua runtime.
//! This is an independent module, not part of the standard library.
//!
//! All database operations run on the Tokio blocking thread pool via
//! `tokio::task::spawn_blocking`, so they never block the Lua event loop:
//! while a statement executes, other coroutines (HTTP handlers, timers, …)
//! keep running. From Lua's perspective the API is still synchronous.
//!
//! Pattern (see also `coppermoon_std/src/http.rs`):
//! 1. Parse Lua arguments into plain Rust data on the Lua thread.
//! 2. Run the SQLite work inside `spawn_blocking` (no Lua access there).
//! 3. Convert results back into Lua values after the `.await`, on the Lua thread.

use mlua::{Lua, Result, Table, UserData, UserDataMethods, Value, MultiValue, FromLua};
use rusqlite::{Connection, types::ValueRef};
use std::sync::{Arc, Mutex, MutexGuard};

/// SQLite error types
#[derive(Debug, thiserror::Error)]
pub enum SqliteError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Query error: {0}")]
    Query(String),
}

/// SQLite Database connection wrapper
///
/// The connection is shared behind an `Arc<Mutex<..>>` so it can be moved
/// into `spawn_blocking` closures (rusqlite's `Connection` is `Send`).
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Open a database connection
    pub fn open(path: &str) -> std::result::Result<Self, SqliteError> {
        let conn = if path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

/// Lock the shared connection, mapping poisoning to a Lua error.
fn lock_conn(conn: &Mutex<Connection>) -> Result<MutexGuard<'_, Connection>> {
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

/// Extract one rusqlite row into thread-safe `(column, value)` pairs.
fn row_to_values(
    row: &rusqlite::Row<'_>,
    column_names: &[String],
) -> rusqlite::Result<Vec<(String, SqliteValue)>> {
    let mut values: Vec<(String, SqliteValue)> = Vec::with_capacity(column_names.len());
    for (i, name) in column_names.iter().enumerate() {
        let value = match row.get_ref(i)? {
            ValueRef::Null => SqliteValue::Null,
            ValueRef::Integer(i) => SqliteValue::Integer(i),
            ValueRef::Real(f) => SqliteValue::Real(f),
            ValueRef::Text(s) => SqliteValue::Text(String::from_utf8_lossy(s).to_string()),
            ValueRef::Blob(b) => SqliteValue::Blob(b.to_vec()),
        };
        values.push((name.clone(), value));
    }
    Ok(values)
}

/// Build a Lua table from a row's `(column, value)` pairs (Lua thread only).
fn row_to_table(lua: &Lua, row: Vec<(String, SqliteValue)>) -> Result<Table> {
    let row_table = lua.create_table()?;
    for (name, value) in row {
        let lua_value = value.to_lua(lua)?;
        row_table.set(name, lua_value)?;
    }
    Ok(row_table)
}

/// Parse `(sql, params...)` from a Lua MultiValue (Lua thread only).
fn parse_sql_args(lua: &Lua, args: MultiValue) -> Result<(String, Vec<SqliteValue>)> {
    let mut args_iter = args.into_iter();

    // First argument is SQL
    let sql: String = match args_iter.next() {
        Some(Value::String(s)) => s.to_str()?.to_string(),
        _ => return Err(mlua::Error::external("First argument must be SQL string")),
    };

    // Remaining arguments are parameters
    let params: Vec<SqliteValue> = args_iter
        .map(|v| SqliteValue::from_lua(v, lua))
        .collect::<Result<Vec<_>>>()?;

    Ok((sql, params))
}

impl UserData for Database {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Execute a SQL statement (INSERT, UPDATE, DELETE, CREATE, etc.)
        methods.add_async_method("exec", |_lua, this, sql: String| async move {
            let conn = this.conn.clone();
            // Userdata borrows are exclusive: release before the first await.
            drop(this);
            let rows_affected = run_blocking(move || {
                let conn = lock_conn(&conn)?;
                conn.execute(&sql, []).map_err(mlua::Error::external)
            })
            .await?;
            Ok(Value::Integer(rows_affected as i64))
        });

        // Execute a SQL statement with parameters
        methods.add_async_method("execute", |lua, this, args: MultiValue| async move {
            let conn = this.conn.clone();
            drop(this);
            let (sql, params) = parse_sql_args(&lua, args)?;

            let rows_affected = run_blocking(move || {
                let conn = lock_conn(&conn)?;
                let param_refs: Vec<&dyn rusqlite::ToSql> = params
                    .iter()
                    .map(|p| p as &dyn rusqlite::ToSql)
                    .collect();
                conn.execute(&sql, param_refs.as_slice())
                    .map_err(mlua::Error::external)
            })
            .await?;
            Ok(Value::Integer(rows_affected as i64))
        });

        // Query and return all rows
        methods.add_async_method("query", |lua, this, args: MultiValue| async move {
            let conn = this.conn.clone();
            drop(this);
            let (sql, params) = parse_sql_args(&lua, args)?;

            let rows: Vec<Vec<(String, SqliteValue)>> = run_blocking(move || {
                let conn = lock_conn(&conn)?;
                let param_refs: Vec<&dyn rusqlite::ToSql> = params
                    .iter()
                    .map(|p| p as &dyn rusqlite::ToSql)
                    .collect();

                let mut stmt = conn.prepare(&sql).map_err(mlua::Error::external)?;

                let column_names: Vec<String> = stmt
                    .column_names()
                    .iter()
                    .map(|s| s.to_string())
                    .collect();

                let mapped = stmt
                    .query_map(param_refs.as_slice(), |row| {
                        row_to_values(row, &column_names)
                    })
                    .map_err(mlua::Error::external)?;

                let mut rows = Vec::new();
                for row in mapped {
                    rows.push(row.map_err(mlua::Error::external)?);
                }
                Ok(rows)
            })
            .await?;

            // Build Lua tables on the Lua thread, after the blocking work.
            let result = lua.create_table()?;
            let mut idx = 1;

            for row in rows {
                result.set(idx, row_to_table(&lua, row)?)?;
                idx += 1;
            }

            Ok(Value::Table(result))
        });

        // Query and return first row only
        methods.add_async_method("query_row", |lua, this, args: MultiValue| async move {
            let conn = this.conn.clone();
            drop(this);
            let (sql, params) = parse_sql_args(&lua, args)?;

            let row: Option<Vec<(String, SqliteValue)>> = run_blocking(move || {
                let conn = lock_conn(&conn)?;
                let param_refs: Vec<&dyn rusqlite::ToSql> = params
                    .iter()
                    .map(|p| p as &dyn rusqlite::ToSql)
                    .collect();

                let mut stmt = conn.prepare(&sql).map_err(mlua::Error::external)?;

                let column_names: Vec<String> = stmt
                    .column_names()
                    .iter()
                    .map(|s| s.to_string())
                    .collect();

                let result = stmt.query_row(param_refs.as_slice(), |row| {
                    row_to_values(row, &column_names)
                });

                match result {
                    Ok(row) => Ok(Some(row)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(mlua::Error::external(e)),
                }
            })
            .await?;

            match row {
                Some(row) => Ok(Value::Table(row_to_table(&lua, row)?)),
                None => Ok(Value::Nil),
            }
        });

        // Get last insert rowid
        methods.add_method("last_insert_id", |_, this, ()| {
            let conn = lock_conn(&this.conn)?;
            Ok(conn.last_insert_rowid())
        });

        // Get changes count from last statement
        methods.add_method("changes", |_, this, ()| {
            let conn = lock_conn(&this.conn)?;
            Ok(conn.changes() as i64)
        });

        // Begin transaction
        methods.add_async_method("begin", |_, this, ()| async move {
            let conn = this.conn.clone();
            drop(this);
            run_blocking(move || {
                let conn = lock_conn(&conn)?;
                conn.execute("BEGIN", []).map_err(mlua::Error::external)?;
                Ok(())
            })
            .await
        });

        // Commit transaction
        methods.add_async_method("commit", |_, this, ()| async move {
            let conn = this.conn.clone();
            drop(this);
            run_blocking(move || {
                let conn = lock_conn(&conn)?;
                conn.execute("COMMIT", []).map_err(mlua::Error::external)?;
                Ok(())
            })
            .await
        });

        // Rollback transaction
        methods.add_async_method("rollback", |_, this, ()| async move {
            let conn = this.conn.clone();
            drop(this);
            run_blocking(move || {
                let conn = lock_conn(&conn)?;
                conn.execute("ROLLBACK", []).map_err(mlua::Error::external)?;
                Ok(())
            })
            .await
        });

        // Transaction helper: BEGIN, call func, COMMIT on success / ROLLBACK on error.
        // The Lua callback runs on the Lua thread (never inside spawn_blocking);
        // only the BEGIN/COMMIT/ROLLBACK statements go through the blocking pool.
        methods.add_async_method("transaction", |_lua, this, func: mlua::Function| async move {
            let conn = this.conn.clone();
            // Release the userdata borrow before awaiting: the callback (and
            // other coroutines) must be able to borrow this same userdata.
            drop(this);

            let begin_conn = conn.clone();
            run_blocking(move || {
                let conn = lock_conn(&begin_conn)?;
                conn.execute("BEGIN", []).map_err(mlua::Error::external)?;
                Ok(())
            })
            .await?;

            match func.call_async::<mlua::Value>(()).await {
                Ok(_) => {
                    let commit_conn = conn.clone();
                    run_blocking(move || {
                        let conn = lock_conn(&commit_conn)?;
                        conn.execute("COMMIT", []).map_err(mlua::Error::external)?;
                        Ok(())
                    })
                    .await?;
                    Ok(true)
                }
                Err(e) => {
                    let rollback_conn = conn.clone();
                    let _ = run_blocking(move || {
                        let conn = lock_conn(&rollback_conn)?;
                        let _ = conn.execute("ROLLBACK", []);
                        Ok(())
                    })
                    .await;
                    Err(e)
                }
            }
        });

        // Close connection
        methods.add_method("close", |_, _this, ()| {
            // Connection will be closed when the last reference is dropped
            Ok(())
        });

        // Check if table exists
        methods.add_async_method("table_exists", |_, this, table_name: String| async move {
            let conn = this.conn.clone();
            drop(this);
            run_blocking(move || {
                let conn = lock_conn(&conn)?;
                let mut stmt = conn
                    .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name=?")
                    .map_err(mlua::Error::external)?;

                let exists = stmt
                    .exists([&table_name])
                    .map_err(mlua::Error::external)?;

                Ok(exists)
            })
            .await
        });

        // Get table info (columns)
        methods.add_async_method("table_info", |lua, this, table_name: String| async move {
            let conn = this.conn.clone();
            drop(this);
            let columns: Vec<ColumnInfo> = run_blocking(move || {
                let conn = lock_conn(&conn)?;
                let sql = format!("PRAGMA table_info({})", table_name);
                let mut stmt = conn.prepare(&sql).map_err(mlua::Error::external)?;

                let mapped = stmt
                    .query_map([], |row| {
                        Ok(ColumnInfo {
                            cid: row.get(0)?,
                            name: row.get(1)?,
                            col_type: row.get(2)?,
                            notnull: row.get(3)?,
                            default_value: row.get(4)?,
                            pk: row.get(5)?,
                        })
                    })
                    .map_err(mlua::Error::external)?;

                let mut columns = Vec::new();
                for col in mapped {
                    columns.push(col.map_err(mlua::Error::external)?);
                }
                Ok(columns)
            })
            .await?;

            let result = lua.create_table()?;
            let mut idx = 1;

            for col in columns {
                let col_table = lua.create_table()?;
                col_table.set("cid", col.cid)?;
                col_table.set("name", col.name)?;
                col_table.set("type", col.col_type)?;
                col_table.set("notnull", col.notnull != 0)?;
                col_table.set("default", col.default_value)?;
                col_table.set("pk", col.pk != 0)?;

                result.set(idx, col_table)?;
                idx += 1;
            }

            Ok(Value::Table(result))
        });
    }
}

#[derive(Debug)]
struct ColumnInfo {
    cid: i32,
    name: String,
    col_type: String,
    notnull: i32,
    default_value: Option<String>,
    pk: i32,
}

/// Wrapper for SQLite values that can be converted to/from Lua
#[derive(Debug, Clone)]
enum SqliteValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl SqliteValue {
    fn to_lua(self, lua: &Lua) -> Result<Value> {
        match self {
            SqliteValue::Null => Ok(Value::Nil),
            SqliteValue::Integer(i) => Ok(Value::Integer(i)),
            SqliteValue::Real(f) => Ok(Value::Number(f)),
            SqliteValue::Text(s) => Ok(Value::String(lua.create_string(&s)?)),
            SqliteValue::Blob(b) => Ok(Value::String(lua.create_string(&b)?)),
        }
    }
}

impl FromLua for SqliteValue {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::Nil => Ok(SqliteValue::Null),
            Value::Boolean(b) => Ok(SqliteValue::Integer(if b { 1 } else { 0 })),
            Value::Integer(i) => Ok(SqliteValue::Integer(i)),
            Value::Number(n) => Ok(SqliteValue::Real(n)),
            Value::String(s) => Ok(SqliteValue::Text(s.to_str()?.to_string())),
            _ => Err(mlua::Error::external("Unsupported value type for SQLite")),
        }
    }
}

impl rusqlite::ToSql for SqliteValue {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            SqliteValue::Null => Ok(rusqlite::types::ToSqlOutput::Owned(rusqlite::types::Value::Null)),
            SqliteValue::Integer(i) => Ok(rusqlite::types::ToSqlOutput::Owned(rusqlite::types::Value::Integer(*i))),
            SqliteValue::Real(f) => Ok(rusqlite::types::ToSqlOutput::Owned(rusqlite::types::Value::Real(*f))),
            SqliteValue::Text(s) => Ok(rusqlite::types::ToSqlOutput::Owned(rusqlite::types::Value::Text(s.clone()))),
            SqliteValue::Blob(b) => Ok(rusqlite::types::ToSqlOutput::Owned(rusqlite::types::Value::Blob(b.clone()))),
        }
    }
}

/// Register the sqlite module with the Lua state
pub fn register(lua: &Lua) -> Result<Table> {
    let module = lua.create_table()?;

    // sqlite.open(path) - Open a database
    module.set("open", lua.create_async_function(|_, path: String| async move {
        run_blocking(move || Database::open(&path).map_err(mlua::Error::external)).await
    })?)?;

    // sqlite.memory() - Open an in-memory database
    module.set("memory", lua.create_async_function(|_, ()| async move {
        run_blocking(|| Database::open(":memory:").map_err(mlua::Error::external)).await
    })?)?;

    // sqlite.version() - Get SQLite version
    module.set("version", lua.create_function(|_, ()| {
        Ok(rusqlite::version())
    })?)?;

    Ok(module)
}

/// Register the sqlite module globally
pub fn register_global(lua: &Lua) -> Result<()> {
    let module = register(lua)?;
    lua.globals().set("sqlite", module)?;
    Ok(())
}
