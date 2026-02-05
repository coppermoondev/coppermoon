//! CopperMoon SQLite Module
//!
//! Provides SQLite database bindings for CopperMoon Lua runtime.
//! This is an independent module, not part of the standard library.

use mlua::{Lua, Result, Table, UserData, UserDataMethods, Value, MultiValue, FromLua};
use rusqlite::{Connection, types::ValueRef};
use std::cell::RefCell;

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
pub struct Database {
    conn: RefCell<Connection>,
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
            conn: RefCell::new(conn),
        })
    }
}

impl UserData for Database {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Execute a SQL statement (INSERT, UPDATE, DELETE, CREATE, etc.)
        methods.add_method("exec", |_lua, this, sql: String| {
            let conn = this.conn.borrow();
            match conn.execute(&sql, []) {
                Ok(rows_affected) => Ok(Value::Integer(rows_affected as i64)),
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Execute a SQL statement with parameters
        methods.add_method("execute", |lua, this, args: MultiValue| {
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

            let conn = this.conn.borrow();
            let param_refs: Vec<&dyn rusqlite::ToSql> = params
                .iter()
                .map(|p| p as &dyn rusqlite::ToSql)
                .collect();

            match conn.execute(&sql, param_refs.as_slice()) {
                Ok(rows_affected) => Ok(Value::Integer(rows_affected as i64)),
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Query and return all rows
        methods.add_method("query", |lua, this, args: MultiValue| {
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

            let conn = this.conn.borrow();
            let param_refs: Vec<&dyn rusqlite::ToSql> = params
                .iter()
                .map(|p| p as &dyn rusqlite::ToSql)
                .collect();

            let mut stmt = conn.prepare(&sql).map_err(mlua::Error::external)?;
            
            let column_count = stmt.column_count();
            let column_names: Vec<String> = stmt
                .column_names()
                .iter()
                .map(|s| s.to_string())
                .collect();

            let rows = stmt
                .query_map(param_refs.as_slice(), |row| {
                    let mut values: Vec<(String, SqliteValue)> = Vec::with_capacity(column_count);
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
                })
                .map_err(mlua::Error::external)?;

            let result = lua.create_table()?;
            let mut idx = 1;

            for row in rows {
                let row = row.map_err(mlua::Error::external)?;
                let row_table = lua.create_table()?;
                
                for (name, value) in row {
                    let lua_value = value.to_lua(lua)?;
                    row_table.set(name, lua_value)?;
                }
                
                result.set(idx, row_table)?;
                idx += 1;
            }

            Ok(Value::Table(result))
        });

        // Query and return first row only
        methods.add_method("query_row", |lua, this, args: MultiValue| {
            let mut args_iter = args.into_iter();
            
            let sql: String = match args_iter.next() {
                Some(Value::String(s)) => s.to_str()?.to_string(),
                _ => return Err(mlua::Error::external("First argument must be SQL string")),
            };

            let params: Vec<SqliteValue> = args_iter
                .map(|v| SqliteValue::from_lua(v, lua))
                .collect::<Result<Vec<_>>>()?;

            let conn = this.conn.borrow();
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
                let mut values: Vec<(String, SqliteValue)> = Vec::new();
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
            });

            match result {
                Ok(row) => {
                    let row_table = lua.create_table()?;
                    for (name, value) in row {
                        let lua_value = value.to_lua(lua)?;
                        row_table.set(name, lua_value)?;
                    }
                    Ok(Value::Table(row_table))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Value::Nil),
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Get last insert rowid
        methods.add_method("last_insert_id", |_, this, ()| {
            let conn = this.conn.borrow();
            Ok(conn.last_insert_rowid())
        });

        // Get changes count from last statement
        methods.add_method("changes", |_, this, ()| {
            let conn = this.conn.borrow();
            Ok(conn.changes() as i64)
        });

        // Begin transaction
        methods.add_method("begin", |_, this, ()| {
            let conn = this.conn.borrow();
            conn.execute("BEGIN", []).map_err(mlua::Error::external)?;
            Ok(())
        });

        // Commit transaction
        methods.add_method("commit", |_, this, ()| {
            let conn = this.conn.borrow();
            conn.execute("COMMIT", []).map_err(mlua::Error::external)?;
            Ok(())
        });

        // Rollback transaction
        methods.add_method("rollback", |_, this, ()| {
            let conn = this.conn.borrow();
            conn.execute("ROLLBACK", []).map_err(mlua::Error::external)?;
            Ok(())
        });

        // Transaction helper
        methods.add_method("transaction", |_lua, this, func: mlua::Function| {
            let conn = this.conn.borrow();
            conn.execute("BEGIN", []).map_err(mlua::Error::external)?;
            
            match func.call::<()>(()) {
                Ok(_) => {
                    conn.execute("COMMIT", []).map_err(mlua::Error::external)?;
                    Ok(true)
                }
                Err(e) => {
                    let _ = conn.execute("ROLLBACK", []);
                    Err(e)
                }
            }
        });

        // Close connection
        methods.add_method("close", |_, _this, ()| {
            // Connection will be closed when dropped
            // We can't really close it explicitly with RefCell
            Ok(())
        });

        // Check if table exists
        methods.add_method("table_exists", |_, this, table_name: String| {
            let conn = this.conn.borrow();
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name=?")
                .map_err(mlua::Error::external)?;
            
            let exists = stmt
                .exists([&table_name])
                .map_err(mlua::Error::external)?;
            
            Ok(exists)
        });

        // Get table info (columns)
        methods.add_method("table_info", |lua, this, table_name: String| {
            let conn = this.conn.borrow();
            let sql = format!("PRAGMA table_info({})", table_name);
            let mut stmt = conn.prepare(&sql).map_err(mlua::Error::external)?;
            
            let columns = stmt
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

            let result = lua.create_table()?;
            let mut idx = 1;

            for col in columns {
                let col = col.map_err(mlua::Error::external)?;
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
    module.set("open", lua.create_function(|_, path: String| {
        match Database::open(&path) {
            Ok(db) => Ok(db),
            Err(e) => Err(mlua::Error::external(e)),
        }
    })?)?;

    // sqlite.memory() - Open an in-memory database
    module.set("memory", lua.create_function(|_, ()| {
        match Database::open(":memory:") {
            Ok(db) => Ok(db),
            Err(e) => Err(mlua::Error::external(e)),
        }
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
