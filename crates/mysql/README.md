# CopperMoon MySQL

> **MySQL/MariaDB bindings for the CopperMoon Lua runtime**

This crate provides native MySQL and MariaDB database bindings for CopperMoon, enabling Lua scripts to connect to MySQL servers, execute queries with parameterized inputs, and manage transactions. The API is designed to be compatible with the SQLite module.

## Features

- Connect via options table or URL string
- Connection pooling (powered by `mysql` crate)
- Parameterized queries (SQL injection safe)
- Transaction support (auto and manual)
- Table introspection (columns, indexes)
- Automatic type conversion between MySQL and Lua

## Usage

```lua
-- Connect with options
local db = mysql.connect({
    host = "localhost",
    port = 3306,
    user = "root",
    password = "secret",
    database = "myapp",
})

-- Or connect with URL
local db = mysql.connect("mysql://root:secret@localhost:3306/myapp")
local db = mysql.open("mysql://root:secret@localhost:3306/myapp")  -- alias

-- Execute statements
db:exec("CREATE TABLE users (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(100))")

-- Insert with parameters
db:execute("INSERT INTO users (name) VALUES (?)", "Alice")
db:execute("INSERT INTO users (name) VALUES (?)", "Bob")

-- Get last insert ID
local id = db:last_insert_id()

-- Query multiple rows
local users = db:query("SELECT * FROM users WHERE id > ?", 0)
for _, user in ipairs(users) do
    print(user.id, user.name)
end

-- Query single row
local user = db:query_row("SELECT * FROM users WHERE id = ?", 1)
if user then
    print(user.name)
end

-- Transactions
db:transaction(function()
    db:execute("INSERT INTO users (name) VALUES (?)", "Charlie")
    db:execute("INSERT INTO users (name) VALUES (?)", "Diana")
    -- Automatically committed; rolled back on error
end)

-- Manual transaction control
db:begin()
db:execute("UPDATE users SET name = ? WHERE id = ?", "Alicia", 1)
db:commit()
-- or db:rollback()

-- Get changes from last statement
local affected = db:changes()

-- Check table existence
if db:table_exists("users") then
    print("Table exists!")
end

-- Get table info (columns)
local columns = db:table_info("users")
for _, col in ipairs(columns) do
    print(col.name, col.type, col.full_type, col.pk, col.notnull)
end

-- Get indexes
local indexes = db:index_list("users")
for _, idx in ipairs(indexes) do
    print(idx.name, idx.unique)
end

-- Server info
print(db:server_version())
print(db:ping())  -- true

-- Close connection (returns to pool)
db:close()
```

## API Reference

### Module Functions

| Function | Description |
|----------|-------------|
| `mysql.connect(options\|url)` | Connect to a MySQL server |
| `mysql.open(url)` | Connect with URL string (alias) |
| `mysql.version()` | Get driver version string |

### Connection Options

```lua
mysql.connect({
    host = "localhost",    -- Hostname (default: "localhost")
    port = 3306,           -- Port (default: 3306)
    user = "root",         -- Username (default: "root")
    password = "secret",   -- Password (optional)
    database = "myapp",    -- Database name (optional)
})
```

### Database Methods

| Method | Description |
|--------|-------------|
| `db:exec(sql)` | Execute SQL without parameters |
| `db:execute(sql, ...)` | Execute SQL with parameters |
| `db:query(sql, ...)` | Query returning all rows |
| `db:query_row(sql, ...)` | Query returning first row |
| `db:last_insert_id()` | Last auto-increment ID |
| `db:last_insert_rowid()` | Alias for `last_insert_id` |
| `db:changes()` | Affected rows from last statement |
| `db:begin()` | Start transaction |
| `db:commit()` | Commit transaction |
| `db:rollback()` | Rollback transaction |
| `db:transaction(fn)` | Execute function in transaction |
| `db:table_exists(name)` | Check if table exists |
| `db:table_info(name)` | Get column information |
| `db:index_list(name)` | Get index information |
| `db:ping()` | Check connection health |
| `db:server_version()` | Get MySQL server version |
| `db:close()` | Close connection (return to pool) |

## Type Mapping

| MySQL Type | Lua Type |
|------------|----------|
| INT, BIGINT | integer |
| FLOAT, DOUBLE | number |
| VARCHAR, TEXT | string |
| BLOB | string (binary) |
| DATE, DATETIME, TIMESTAMP | string (formatted) |
| TIME | string (formatted) |
| NULL | nil |
| BOOLEAN | integer (0/1) |

## Rust Integration

```rust
use coppermoon_mysql;

// Register globally as `mysql`
coppermoon_mysql::register_global(lua)?;

// Or get the module table
let mysql_module = coppermoon_mysql::register(lua)?;
```

## Related

- [CopperMoon](https://github.com/coppermoondev/coppermoon) — Main repository and runtime
- [Freight](https://github.com/coppermoondev/freight) — ORM built on top of database bindings

## Documentation

For full documentation, visit [coppermoon.dev](https://coppermoon.dev).

## License

MIT License
