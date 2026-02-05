//! CopperMoon MySQL/MariaDB Module
//!
//! Provides MySQL and MariaDB database bindings for CopperMoon Lua runtime.
//! This module provides a compatible interface with the SQLite module.

use mlua::{FromLua, Lua, MultiValue, Result, Table, UserData, UserDataMethods, Value};
use mysql::prelude::*;
use mysql::{Conn, Opts, OptsBuilder, Pool, PooledConn, Row as MySqlRow};
use std::cell::RefCell;
use std::sync::Arc;

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
pub struct Database {
    pool: Arc<Pool>,
    conn: RefCell<PooledConn>,
    last_insert_id: RefCell<u64>,
    affected_rows: RefCell<u64>,
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

        Ok(Self {
            pool: Arc::new(pool),
            conn: RefCell::new(conn),
            last_insert_id: RefCell::new(0),
            affected_rows: RefCell::new(0),
        })
    }

    /// Open a database connection with URL
    pub fn open_url(url: &str) -> std::result::Result<Self, MysqlError> {
        let opts = Opts::from_url(url)?;
        let pool = Pool::new(opts)?;
        let conn = pool.get_conn()?;

        Ok(Self {
            pool: Arc::new(pool),
            conn: RefCell::new(conn),
            last_insert_id: RefCell::new(0),
            affected_rows: RefCell::new(0),
        })
    }

    /// Get a fresh connection from the pool
    fn get_conn(&self) -> std::result::Result<PooledConn, mysql::Error> {
        self.pool.get_conn()
    }
}

impl UserData for Database {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Execute a SQL statement (INSERT, UPDATE, DELETE, CREATE, etc.)
        methods.add_method("exec", |_lua, this, sql: String| {
            let mut conn = this.conn.borrow_mut();
            match conn.query_drop(&sql) {
                Ok(_) => {
                    let affected = conn.affected_rows();
                    *this.affected_rows.borrow_mut() = affected;
                    Ok(Value::Integer(affected as i64))
                }
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
            let params: Vec<MysqlValue> = args_iter
                .map(|v| MysqlValue::from_lua(v, lua))
                .collect::<Result<Vec<_>>>()?;

            let mut conn = this.conn.borrow_mut();

            // Convert params to mysql::Value
            let mysql_params: Vec<mysql::Value> = params.iter().map(|p| p.to_mysql()).collect();

            match conn.exec_drop(&sql, mysql_params) {
                Ok(_) => {
                    let affected = conn.affected_rows();
                    let last_id = conn.last_insert_id();
                    *this.affected_rows.borrow_mut() = affected;
                    *this.last_insert_id.borrow_mut() = last_id;
                    Ok(Value::Integer(affected as i64))
                }
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
            let params: Vec<MysqlValue> = args_iter
                .map(|v| MysqlValue::from_lua(v, lua))
                .collect::<Result<Vec<_>>>()?;

            let mut conn = this.conn.borrow_mut();
            let mysql_params: Vec<mysql::Value> = params.iter().map(|p| p.to_mysql()).collect();

            let rows: std::result::Result<Vec<MySqlRow>, mysql::Error> =
                conn.exec(&sql, mysql_params);

            match rows {
                Ok(rows) => {
                    let result = lua.create_table()?;

                    for (idx, row) in rows.iter().enumerate() {
                        let row_table = lua.create_table()?;

                        // Get column names and values
                        for (col_idx, column) in row.columns_ref().iter().enumerate() {
                            let col_name = column.name_str().to_string();
                            let value: mysql::Value = row.get(col_idx).unwrap_or(mysql::Value::NULL);
                            let lua_value = mysql_value_to_lua(&value, lua)?;
                            row_table.set(col_name, lua_value)?;
                        }

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

            let params: Vec<MysqlValue> = args_iter
                .map(|v| MysqlValue::from_lua(v, lua))
                .collect::<Result<Vec<_>>>()?;

            let mut conn = this.conn.borrow_mut();
            let mysql_params: Vec<mysql::Value> = params.iter().map(|p| p.to_mysql()).collect();

            let result: std::result::Result<Option<MySqlRow>, mysql::Error> =
                conn.exec_first(&sql, mysql_params);

            match result {
                Ok(Some(row)) => {
                    let row_table = lua.create_table()?;

                    for (col_idx, column) in row.columns_ref().iter().enumerate() {
                        let col_name = column.name_str().to_string();
                        let value: mysql::Value = row.get(col_idx).unwrap_or(mysql::Value::NULL);
                        let lua_value = mysql_value_to_lua(&value, lua)?;
                        row_table.set(col_name, lua_value)?;
                    }

                    Ok(Value::Table(row_table))
                }
                Ok(None) => Ok(Value::Nil),
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Get last insert id
        methods.add_method("last_insert_id", |_, this, ()| {
            Ok(*this.last_insert_id.borrow() as i64)
        });

        // Alias for compatibility
        methods.add_method("last_insert_rowid", |_, this, ()| {
            Ok(*this.last_insert_id.borrow() as i64)
        });

        // Get changes count from last statement
        methods.add_method("changes", |_, this, ()| {
            Ok(*this.affected_rows.borrow() as i64)
        });

        // Begin transaction
        methods.add_method("begin", |_, this, ()| {
            let mut conn = this.conn.borrow_mut();
            conn.query_drop("START TRANSACTION")
                .map_err(mlua::Error::external)?;
            Ok(())
        });

        // Commit transaction
        methods.add_method("commit", |_, this, ()| {
            let mut conn = this.conn.borrow_mut();
            conn.query_drop("COMMIT")
                .map_err(mlua::Error::external)?;
            Ok(())
        });

        // Rollback transaction
        methods.add_method("rollback", |_, this, ()| {
            let mut conn = this.conn.borrow_mut();
            conn.query_drop("ROLLBACK")
                .map_err(mlua::Error::external)?;
            Ok(())
        });

        // Transaction helper
        methods.add_method("transaction", |_lua, this, func: mlua::Function| {
            {
                let mut conn = this.conn.borrow_mut();
                conn.query_drop("START TRANSACTION")
                    .map_err(mlua::Error::external)?;
            }

            match func.call::<()>(()) {
                Ok(_) => {
                    let mut conn = this.conn.borrow_mut();
                    conn.query_drop("COMMIT")
                        .map_err(mlua::Error::external)?;
                    Ok(true)
                }
                Err(e) => {
                    let mut conn = this.conn.borrow_mut();
                    let _ = conn.query_drop("ROLLBACK");
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
        methods.add_method("table_exists", |_, this, table_name: String| {
            let mut conn = this.conn.borrow_mut();

            let sql = "SELECT COUNT(*) as cnt FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = ?";
            let result: std::result::Result<Option<MySqlRow>, mysql::Error> =
                conn.exec_first(sql, (table_name,));

            match result {
                Ok(Some(row)) => {
                    let count: i64 = row.get("cnt").unwrap_or(0);
                    Ok(count > 0)
                }
                Ok(None) => Ok(false),
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Get table info (columns)
        methods.add_method("table_info", |lua, this, table_name: String| {
            let mut conn = this.conn.borrow_mut();

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

            let rows: std::result::Result<Vec<MySqlRow>, mysql::Error> =
                conn.exec(sql, (table_name,));

            match rows {
                Ok(rows) => {
                    let result = lua.create_table()?;

                    for (idx, row) in rows.iter().enumerate() {
                        let col_table = lua.create_table()?;

                        let cid: i64 = row.get("cid").unwrap_or(0);
                        let name: String = row.get("name").unwrap_or_default();
                        let col_type: String = row.get("type").unwrap_or_default();
                        let full_type: String = row.get("full_type").unwrap_or_default();
                        let notnull: i64 = row.get("notnull").unwrap_or(0);
                        let default: Option<String> = row.get("default");
                        let pk: i64 = row.get("pk").unwrap_or(0);
                        let extra: String = row.get("extra").unwrap_or_default();

                        col_table.set("cid", cid)?;
                        col_table.set("name", name)?;
                        col_table.set("type", col_type)?;
                        col_table.set("full_type", full_type)?;
                        col_table.set("notnull", notnull != 0)?;
                        col_table.set("default", default)?;
                        col_table.set("pk", pk != 0)?;
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
            let mut conn = this.conn.borrow_mut();

            let sql = r#"
                SELECT DISTINCT
                    INDEX_NAME as name,
                    NOT NON_UNIQUE as `unique`
                FROM information_schema.statistics
                WHERE table_schema = DATABASE() AND table_name = ?
            "#;

            let rows: std::result::Result<Vec<MySqlRow>, mysql::Error> =
                conn.exec(sql, (table_name,));

            match rows {
                Ok(rows) => {
                    let result = lua.create_table()?;

                    for (idx, row) in rows.iter().enumerate() {
                        let index_table = lua.create_table()?;

                        let name: String = row.get("name").unwrap_or_default();
                        let unique: i64 = row.get("unique").unwrap_or(0);

                        index_table.set("name", name)?;
                        index_table.set("unique", unique != 0)?;

                        result.set(idx + 1, index_table)?;
                    }

                    Ok(Value::Table(result))
                }
                Err(e) => Err(mlua::Error::external(e)),
            }
        });

        // Ping to check connection
        methods.add_method("ping", |_, this, ()| {
            let mut conn = this.conn.borrow_mut();
            match conn.query_drop("SELECT 1") {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        });

        // Get server version
        methods.add_method("server_version", |_, this, ()| {
            let conn = this.conn.borrow();
            Ok(conn.server_version())
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
    Bytes(Vec<u8>),
}

impl MysqlValue {
    fn to_mysql(&self) -> mysql::Value {
        match self {
            MysqlValue::Null => mysql::Value::NULL,
            MysqlValue::Integer(i) => mysql::Value::Int(*i),
            MysqlValue::Float(f) => mysql::Value::Double(*f),
            MysqlValue::Text(s) => mysql::Value::Bytes(s.as_bytes().to_vec()),
            MysqlValue::Bytes(b) => mysql::Value::Bytes(b.clone()),
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

/// Register the mysql module with the Lua state
pub fn register(lua: &Lua) -> Result<Table> {
    let module = lua.create_table()?;

    // mysql.connect(options) - Connect with options table
    module.set(
        "connect",
        lua.create_function(|lua, options: Value| {
            let opts = match options {
                Value::Table(t) => {
                    let host: String = t.get("host").unwrap_or_else(|_| "localhost".to_string());
                    let port: u16 = t.get("port").unwrap_or(3306);
                    let user: String = t.get("user").unwrap_or_else(|_| "root".to_string());
                    let password: Option<String> = t.get("password").ok();
                    let database: Option<String> = t.get("database").ok();

                    ConnectionOptions {
                        host,
                        port,
                        user,
                        password,
                        database,
                    }
                }
                Value::String(s) => {
                    // URL format
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

    // mysql.open(url) - Open with URL string (alias)
    module.set(
        "open",
        lua.create_function(|_, url: String| match Database::open_url(&url) {
            Ok(db) => Ok(db),
            Err(e) => Err(mlua::Error::external(e)),
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
