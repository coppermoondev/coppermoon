# CopperMoon PostgreSQL

> **PostgreSQL bindings for the CopperMoon Lua runtime**

This crate provides native PostgreSQL database bindings for CopperMoon, enabling Lua scripts to connect to PostgreSQL servers, execute queries with parameterized inputs, and manage transactions. The API is designed to be compatible with the SQLite and MySQL modules.

## Features

- Connect via options table or URL string
- Parameterized queries with `?` placeholders (auto-converted to `$1, $2, ...`)
- Transaction support (auto and manual)
- Table introspection (columns, indexes)
- Automatic type conversion between PostgreSQL and Lua

## Usage

```lua
-- Connect with options
local db = postgresql.connect({
    host = "localhost",
    port = 5432,
    user = "postgres",
    password = "secret",
    database = "myapp",
})

-- Or connect with URL
local db = postgresql.connect("postgres://postgres:secret@localhost:5432/myapp")
local db = postgresql.open("postgres://postgres:secret@localhost:5432/myapp")  -- alias

-- Execute statements
db:exec("CREATE TABLE users (id SERIAL PRIMARY KEY, name VARCHAR(100))")

-- Insert with parameters (use ? — auto-converted to $1, $2, ...)
db:execute("INSERT INTO users (name) VALUES (?)", "Alice")
db:execute("INSERT INTO users (name) VALUES (?)", "Bob")

-- Get last insert ID (via SELECT lastval())
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
    print(col.name, col.type, col.pk, col.notnull)
end

-- Get indexes
local indexes = db:index_list("users")
for _, idx in ipairs(indexes) do
    print(idx.name, idx.unique)
end

-- Server info
print(db:server_version())
print(db:ping())  -- true

-- Close connection
db:close()
```

## API Reference

### Module Functions

| Function | Description |
|----------|-------------|
| `postgresql.connect(options\|url)` | Connect to a PostgreSQL server |
| `postgresql.open(url)` | Connect with URL string (alias) |
| `postgresql.version()` | Get driver version string |

### Connection Options

```lua
postgresql.connect({
    host = "localhost",    -- Hostname (default: "localhost")
    port = 5432,           -- Port (default: 5432)
    user = "postgres",     -- Username (default: "postgres")
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
| `db:last_insert_id()` | Last inserted sequence value |
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
| `db:server_version()` | Get PostgreSQL server version |
| `db:close()` | Close connection |

## Placeholder Conversion

For API consistency with SQLite and MySQL, you write `?` placeholders in your SQL. The driver transparently converts them to PostgreSQL's native `$1, $2, ...` format:

```lua
-- You write:
db:query("SELECT * FROM users WHERE age > ? AND name = ?", 18, "Alice")

-- Driver sends to PostgreSQL:
-- SELECT * FROM users WHERE age > $1 AND name = $2
```

## Type Mapping

| PostgreSQL Type | Lua Type |
|-----------------|----------|
| BOOL | boolean |
| INT2, INT4, INT8 | integer |
| FLOAT4, FLOAT8 | number |
| VARCHAR, TEXT, CHAR, NAME | string |
| UUID, JSON, JSONB | string |
| TIMESTAMP, TIMESTAMPTZ, DATE, TIME | string (formatted) |
| BYTEA | string (binary) |
| NULL | nil |

## Rust Integration

```rust
use coppermoon_postgresql;

// Register globally as `postgresql`
coppermoon_postgresql::register_global(lua)?;

// Or get the module table
let pg_module = coppermoon_postgresql::register(lua)?;
```

## Related

- [CopperMoon](https://github.com/coppermoondev/coppermoon) — Main repository and runtime
- [Freight](https://github.com/coppermoondev/freight) — ORM built on top of database bindings

## Documentation

For full documentation, visit [coppermoon.dev](https://coppermoon.dev).

## License

MIT License
