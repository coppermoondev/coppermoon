//! CopperMoon PostgreSQL Module
//!
//! Provides PostgreSQL database bindings for CopperMoon Lua runtime.
//! This module provides a compatible interface with the MySQL and SQLite modules.

use mlua::{FromLua, Lua, MultiValue, Result, Table, UserData, UserDataMethods, Value};
use postgres::types::Type;
use postgres::NoTls;
use std::cell::RefCell;

/// PostgreSQL error types
#[derive(Debug, thiserror::Error)]
pub enum PostgresError {
    #[error("PostgreSQL error: {0}")]
    Postgres(#[from] postgres::Error),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Query error: {0}")]
    Query(String),
}

/// PostgreSQL Database connection wrapper
pub struct Database {
    client: RefCell<postgres::Client>,
    last_insert_id: RefCell<i64>,
    affected_rows: RefCell<u64>,
}

/// Connection options for PostgreSQL
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
            port: 5432,
            user: "postgres".to_string(),
            password: None,
            database: None,
        }
    }
}

impl Database {
    /// Open a database connection with options
    pub fn open(options: ConnectionOptions) -> std::result::Result<Self, PostgresError> {
        let mut params = format!(
            "host={} port={} user={}",
            options.host, options.port, options.user
        );

        if let Some(ref password) = options.password {
            params.push_str(&format!(" password={}", password));
        }

        if let Some(ref database) = options.database {
            params.push_str(&format!(" dbname={}", database));
        }

        let client = postgres::Client::connect(&params, NoTls)?;

        Ok(Self {
            client: RefCell::new(client),
            last_insert_id: RefCell::new(0),
            affected_rows: RefCell::new(0),
        })
    }

    /// Open a database connection with URL
    pub fn open_url(url: &str) -> std::result::Result<Self, PostgresError> {
        let client = postgres::Client::connect(url, NoTls)?;

        Ok(Self {
            client: RefCell::new(client),
            last_insert_id: RefCell::new(0),
            affected_rows: RefCell::new(0),
        })
    }
}

// ---------------------------------------------------------------------------
// Placeholder conversion: ? → $1, $2, ...
// ---------------------------------------------------------------------------

/// Convert `?` placeholders to PostgreSQL's `$N` style.
/// Respects single-quoted strings (doesn't convert `?` inside them).
fn convert_placeholders(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len() + 16);
    let mut param_index = 0u32;
    let mut in_string = false;
    let mut prev_was_escape = false;

    for ch in sql.chars() {
        if ch == '\'' && !prev_was_escape {
            in_string = !in_string;
        }
        prev_was_escape = ch == '\\' && !prev_was_escape;

        if ch == '?' && !in_string {
            param_index += 1;
            result.push('$');
            result.push_str(&param_index.to_string());
        } else {
            result.push(ch);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Value conversion helpers
// ---------------------------------------------------------------------------

/// Wrapper for PostgreSQL values that can be converted to/from Lua
#[derive(Debug, Clone)]
enum PgValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    Text(String),
}

impl FromLua for PgValue {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::Nil => Ok(PgValue::Null),
            Value::Boolean(b) => Ok(PgValue::Bool(b)),
            Value::Integer(i) => Ok(PgValue::Integer(i)),
            Value::Number(n) => Ok(PgValue::Float(n)),
            Value::String(s) => Ok(PgValue::Text(s.to_str()?.to_string())),
            _ => Err(mlua::Error::external("Unsupported value type for PostgreSQL")),
        }
    }
}

/// Build a vector of boxed ToSql trait objects from PgValue list.
fn build_params(values: &[PgValue]) -> Vec<Box<dyn postgres::types::ToSql + Sync>> {
    values
        .iter()
        .map(|v| -> Box<dyn postgres::types::ToSql + Sync> {
            match v {
                PgValue::Null => Box::new(None::<String>),
                PgValue::Bool(b) => Box::new(*b),
                PgValue::Integer(i) => Box::new(*i),
                PgValue::Float(f) => Box::new(*f),
                PgValue::Text(s) => Box::new(s.clone()),
            }
        })
        .collect()
}

/// Create a slice of references from boxed params (needed by postgres crate API).
fn params_as_refs(params: &[Box<dyn postgres::types::ToSql + Sync>]) -> Vec<&(dyn postgres::types::ToSql + Sync)> {
    params.iter().map(|p| p.as_ref()).collect()
}

/// Convert a PostgreSQL row to a Lua table.
fn pg_row_to_lua_table(row: &postgres::Row, lua: &Lua) -> Result<Table> {
    let table = lua.create_table()?;

    for (i, col) in row.columns().iter().enumerate() {
        let name = col.name();
        let lua_val = pg_column_to_lua(row, i, col.type_(), lua)?;
        table.set(name, lua_val)?;
    }

    Ok(table)
}

/// Convert a single column value from a PostgreSQL row to a Lua value.
fn pg_column_to_lua(row: &postgres::Row, idx: usize, pg_type: &Type, lua: &Lua) -> Result<Value> {
    // Match on PostgreSQL type and extract with the appropriate Rust type
    match *pg_type {
        Type::BOOL => match row.try_get::<_, Option<bool>>(idx) {
            Ok(Some(v)) => Ok(Value::Boolean(v)),
            _ => Ok(Value::Nil),
        },
        Type::INT2 => match row.try_get::<_, Option<i16>>(idx) {
            Ok(Some(v)) => Ok(Value::Integer(v as i64)),
            _ => Ok(Value::Nil),
        },
        Type::INT4 => match row.try_get::<_, Option<i32>>(idx) {
            Ok(Some(v)) => Ok(Value::Integer(v as i64)),
            _ => Ok(Value::Nil),
        },
        Type::INT8 => match row.try_get::<_, Option<i64>>(idx) {
            Ok(Some(v)) => Ok(Value::Integer(v)),
            _ => Ok(Value::Nil),
        },
        Type::FLOAT4 => match row.try_get::<_, Option<f32>>(idx) {
            Ok(Some(v)) => Ok(Value::Number(v as f64)),
            _ => Ok(Value::Nil),
        },
        Type::FLOAT8 => match row.try_get::<_, Option<f64>>(idx) {
            Ok(Some(v)) => Ok(Value::Number(v)),
            _ => Ok(Value::Nil),
        },
        _ => {
            // Default: try to get as string (works for TEXT, VARCHAR, TIMESTAMP,
            // DATE, TIME, JSON, JSONB, UUID, NUMERIC, etc.)
            match row.try_get::<_, Option<String>>(idx) {
                Ok(Some(v)) => Ok(Value::String(lua.create_string(&v)?)),
                Ok(None) => Ok(Value::Nil),
                Err(_) => {
                    // Last resort: try as bytes
                    match row.try_get::<_, Option<Vec<u8>>>(idx) {
                        Ok(Some(v)) => Ok(Value::String(lua.create_string(&v)?)),
                        _ => Ok(Value::Nil),
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// UserData implementation
// ---------------------------------------------------------------------------

impl UserData for Database {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Execute a SQL statement without parameters
        methods.add_method("exec", |_lua, this, sql: String| {
            let mut client = this.client.borrow_mut();
            match client.execute(sql.as_str(), &[]) {
                Ok(affected) => {
                    *this.affected_rows.borrow_mut() = affected;
                    Ok(Value::Integer(affected as i64))
                }
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Execute a SQL statement with parameters (? placeholders)
        methods.add_method("execute", |lua, this, args: MultiValue| {
            let mut args_iter = args.into_iter();

            let sql: String = match args_iter.next() {
                Some(Value::String(s)) => s.to_str()?.to_string(),
                _ => return Err(mlua::Error::external("First argument must be SQL string")),
            };

            let params: Vec<PgValue> = args_iter
                .map(|v| PgValue::from_lua(v, lua))
                .collect::<Result<Vec<_>>>()?;

            let converted_sql = convert_placeholders(&sql);
            let boxed_params = build_params(&params);
            let param_refs = params_as_refs(&boxed_params);

            let mut client = this.client.borrow_mut();

            // Check if this is an INSERT to capture last_insert_id
            let is_insert = sql.trim_start().to_uppercase().starts_with("INSERT");

            match client.execute(converted_sql.as_str(), &param_refs) {
                Ok(affected) => {
                    *this.affected_rows.borrow_mut() = affected;

                    // Try to get last inserted ID via lastval()
                    if is_insert {
                        if let Ok(row) = client.query_one("SELECT lastval()", &[]) {
                            if let Ok(id) = row.try_get::<_, i64>(0) {
                                *this.last_insert_id.borrow_mut() = id;
                            }
                        }
                    }

                    Ok(Value::Integer(affected as i64))
                }
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Query and return all rows
        methods.add_method("query", |lua, this, args: MultiValue| {
            let mut args_iter = args.into_iter();

            let sql: String = match args_iter.next() {
                Some(Value::String(s)) => s.to_str()?.to_string(),
                _ => return Err(mlua::Error::external("First argument must be SQL string")),
            };

            let params: Vec<PgValue> = args_iter
                .map(|v| PgValue::from_lua(v, lua))
                .collect::<Result<Vec<_>>>()?;

            let converted_sql = convert_placeholders(&sql);
            let boxed_params = build_params(&params);
            let param_refs = params_as_refs(&boxed_params);

            let mut client = this.client.borrow_mut();

            match client.query(converted_sql.as_str(), &param_refs) {
                Ok(rows) => {
                    let result = lua.create_table()?;

                    for (idx, row) in rows.iter().enumerate() {
                        let row_table = pg_row_to_lua_table(row, lua)?;
                        result.set(idx + 1, row_table)?;
                    }

                    Ok(Value::Table(result))
                }
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Query and return first row only
        methods.add_method("query_row", |lua, this, args: MultiValue| {
            let mut args_iter = args.into_iter();

            let sql: String = match args_iter.next() {
                Some(Value::String(s)) => s.to_str()?.to_string(),
                _ => return Err(mlua::Error::external("First argument must be SQL string")),
            };

            let params: Vec<PgValue> = args_iter
                .map(|v| PgValue::from_lua(v, lua))
                .collect::<Result<Vec<_>>>()?;

            let converted_sql = convert_placeholders(&sql);
            let boxed_params = build_params(&params);
            let param_refs = params_as_refs(&boxed_params);

            let mut client = this.client.borrow_mut();

            match client.query_opt(converted_sql.as_str(), &param_refs) {
                Ok(Some(row)) => {
                    let row_table = pg_row_to_lua_table(&row, lua)?;
                    Ok(Value::Table(row_table))
                }
                Ok(None) => Ok(Value::Nil),
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Get last insert id (via lastval())
        methods.add_method("last_insert_id", |_, this, ()| {
            Ok(*this.last_insert_id.borrow())
        });

        // Alias for compatibility with SQLite module
        methods.add_method("last_insert_rowid", |_, this, ()| {
            Ok(*this.last_insert_id.borrow())
        });

        // Get changes count from last statement
        methods.add_method("changes", |_, this, ()| {
            Ok(*this.affected_rows.borrow() as i64)
        });

        // Begin transaction
        methods.add_method("begin", |_, this, ()| {
            let mut client = this.client.borrow_mut();
            client
                .execute("BEGIN", &[])
                .map_err(mlua::Error::external)?;
            Ok(())
        });

        // Commit transaction
        methods.add_method("commit", |_, this, ()| {
            let mut client = this.client.borrow_mut();
            client
                .execute("COMMIT", &[])
                .map_err(mlua::Error::external)?;
            Ok(())
        });

        // Rollback transaction
        methods.add_method("rollback", |_, this, ()| {
            let mut client = this.client.borrow_mut();
            client
                .execute("ROLLBACK", &[])
                .map_err(mlua::Error::external)?;
            Ok(())
        });

        // Transaction helper
        methods.add_method("transaction", |_lua, this, func: mlua::Function| {
            {
                let mut client = this.client.borrow_mut();
                client
                    .execute("BEGIN", &[])
                    .map_err(mlua::Error::external)?;
            }

            match func.call::<()>(()) {
                Ok(_) => {
                    let mut client = this.client.borrow_mut();
                    client
                        .execute("COMMIT", &[])
                        .map_err(mlua::Error::external)?;
                    Ok(true)
                }
                Err(e) => {
                    let mut client = this.client.borrow_mut();
                    let _ = client.execute("ROLLBACK", &[]);
                    Err(e)
                }
            }
        });

        // Close connection
        methods.add_method("close", |_, _this, ()| {
            // Connection will be closed when dropped
            Ok(())
        });

        // Check if table exists
        methods.add_method("table_exists", |_, this, table_name: String| {
            let mut client = this.client.borrow_mut();

            let sql = "SELECT COUNT(*) as cnt FROM information_schema.tables WHERE table_catalog = current_database() AND table_schema = 'public' AND table_name = $1";

            match client.query_one(sql, &[&table_name]) {
                Ok(row) => {
                    let count: i64 = row.get("cnt");
                    Ok(count > 0)
                }
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Get table info (columns)
        methods.add_method("table_info", |lua, this, table_name: String| {
            let mut client = this.client.borrow_mut();

            let sql = r#"
                SELECT
                    ordinal_position as cid,
                    column_name as name,
                    data_type as type,
                    CASE WHEN character_maximum_length IS NOT NULL
                         THEN data_type || '(' || character_maximum_length || ')'
                         ELSE data_type
                    END as full_type,
                    is_nullable = 'NO' as notnull,
                    column_default as "default",
                    CASE WHEN pk.column_name IS NOT NULL THEN true ELSE false END as pk,
                    COALESCE(column_default, '') as extra
                FROM information_schema.columns c
                LEFT JOIN (
                    SELECT kcu.column_name
                    FROM information_schema.table_constraints tc
                    JOIN information_schema.key_column_usage kcu
                        ON tc.constraint_name = kcu.constraint_name
                        AND tc.table_schema = kcu.table_schema
                    WHERE tc.constraint_type = 'PRIMARY KEY'
                        AND tc.table_name = $1
                        AND tc.table_schema = 'public'
                ) pk ON c.column_name = pk.column_name
                WHERE c.table_catalog = current_database()
                    AND c.table_schema = 'public'
                    AND c.table_name = $1
                ORDER BY c.ordinal_position
            "#;

            match client.query(sql, &[&table_name]) {
                Ok(rows) => {
                    let result = lua.create_table()?;

                    for (idx, row) in rows.iter().enumerate() {
                        let col_table = lua.create_table()?;

                        let cid: i32 = row.get("cid");
                        let name: String = row.get("name");
                        let col_type: String = row.get("type");
                        let full_type: String = row.get("full_type");
                        let notnull: bool = row.get("notnull");
                        let default: Option<String> = row.get("default");
                        let pk: bool = row.get("pk");
                        let extra: String = row.get("extra");

                        col_table.set("cid", cid as i64)?;
                        col_table.set("name", name)?;
                        col_table.set("type", col_type)?;
                        col_table.set("full_type", full_type)?;
                        col_table.set("notnull", notnull)?;
                        col_table.set("default", default)?;
                        col_table.set("pk", pk)?;
                        col_table.set("extra", extra)?;

                        result.set(idx + 1, col_table)?;
                    }

                    Ok(Value::Table(result))
                }
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Get index list
        methods.add_method("index_list", |lua, this, table_name: String| {
            let mut client = this.client.borrow_mut();

            let sql = r#"
                SELECT
                    indexname as name,
                    indexdef LIKE '%UNIQUE%' as "unique"
                FROM pg_indexes
                WHERE schemaname = 'public' AND tablename = $1
            "#;

            match client.query(sql, &[&table_name]) {
                Ok(rows) => {
                    let result = lua.create_table()?;

                    for (idx, row) in rows.iter().enumerate() {
                        let index_table = lua.create_table()?;

                        let name: String = row.get("name");
                        let unique: bool = row.get("unique");

                        index_table.set("name", name)?;
                        index_table.set("unique", unique)?;

                        result.set(idx + 1, index_table)?;
                    }

                    Ok(Value::Table(result))
                }
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Ping to check connection
        methods.add_method("ping", |_, this, ()| {
            let mut client = this.client.borrow_mut();
            match client.simple_query("SELECT 1") {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        });

        // Get server version
        methods.add_method("server_version", |_, this, ()| {
            let mut client = this.client.borrow_mut();
            match client.query_one("SHOW server_version", &[]) {
                Ok(row) => {
                    let version: String = row.get(0);
                    Ok(version)
                }
                Err(e) => Err(mlua::Error::external(e)),
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// Register the postgresql module with the Lua state
pub fn register(lua: &Lua) -> Result<Table> {
    let module = lua.create_table()?;

    // postgresql.connect(options) - Connect with options table or URL string
    module.set(
        "connect",
        lua.create_function(|_lua, options: Value| {
            let opts = match options {
                Value::Table(t) => {
                    let host: String =
                        t.get("host").unwrap_or_else(|_| "localhost".to_string());
                    let port: u16 = t.get("port").unwrap_or(5432);
                    let user: String =
                        t.get("user").unwrap_or_else(|_| "postgres".to_string());
                    let password: Option<String> = t.get("password").ok();
                    let database: Option<String> =
                        t.get("database").or_else(|_| t.get("dbname")).ok();

                    ConnectionOptions {
                        host,
                        port,
                        user,
                        password,
                        database,
                    }
                }
                Value::String(s) => {
                    let url = s.to_str()?.to_string();
                    return match Database::open_url(&url) {
                        Ok(db) => Ok(db),
                        Err(e) => Err(mlua::Error::external(e)),
                    };
                }
                _ => {
                    return Err(mlua::Error::external(
                        "connect() requires options table or URL string",
                    ))
                }
            };

            match Database::open(opts) {
                Ok(db) => Ok(db),
                Err(e) => Err(mlua::Error::external(e)),
            }
        })?,
    )?;

    // postgresql.open(url) - Open with URL string (alias)
    module.set(
        "open",
        lua.create_function(|_, url: String| match Database::open_url(&url) {
            Ok(db) => Ok(db),
            Err(e) => Err(mlua::Error::external(e)),
        })?,
    )?;

    // postgresql.version() - Get client library version
    module.set(
        "version",
        lua.create_function(|_, ()| Ok("postgres-rs 0.19"))?,
    )?;

    Ok(module)
}

/// Register the postgresql module globally
pub fn register_global(lua: &Lua) -> Result<()> {
    let module = register(lua)?;
    lua.globals().set("postgresql", module)?;
    Ok(())
}
