# CopperMoon SQLite

> SQLite bindings for CopperMoon Lua runtime

This crate provides native SQLite database bindings for CopperMoon, enabling Lua scripts to interact with SQLite databases.

## Features

- Open SQLite databases (file or in-memory)
- Execute SQL statements with parameterized queries
- Query data with automatic type conversion
- Transaction support
- Table introspection

## Usage

```lua
-- The sqlite module must be registered in the runtime
local db = sqlite.open("mydb.sqlite")

-- Or use in-memory database
local db = sqlite.memory()

-- Execute statements
db:exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")

-- Insert with parameters
db:execute("INSERT INTO users (name) VALUES (?)", "John")

-- Query data
local users = db:query("SELECT * FROM users WHERE id > ?", 0)
for _, user in ipairs(users) do
    print(user.id, user.name)
end

-- Query single row
local user = db:query_row("SELECT * FROM users WHERE id = ?", 1)

-- Transactions
db:transaction(function()
    db:execute("INSERT INTO users (name) VALUES (?)", "Alice")
    db:execute("INSERT INTO users (name) VALUES (?)", "Bob")
end)

-- Get last insert ID
local id = db:last_insert_id()

-- Check table exists
if db:table_exists("users") then
    print("Table exists!")
end

-- Get table info
local columns = db:table_info("users")
for _, col in ipairs(columns) do
    print(col.name, col.type)
end

-- Close connection
db:close()
```

## API Reference

### Module Functions

| Function | Description |
|----------|-------------|
| `sqlite.open(path)` | Open a database file |
| `sqlite.memory()` | Open an in-memory database |
| `sqlite.version()` | Get SQLite version string |

### Database Methods

| Method | Description |
|--------|-------------|
| `db:exec(sql)` | Execute SQL without parameters |
| `db:execute(sql, ...)` | Execute SQL with parameters |
| `db:query(sql, ...)` | Query and return all rows |
| `db:query_row(sql, ...)` | Query and return first row |
| `db:last_insert_id()` | Get last inserted row ID |
| `db:changes()` | Get number of changes from last statement |
| `db:begin()` | Begin transaction |
| `db:commit()` | Commit transaction |
| `db:rollback()` | Rollback transaction |
| `db:transaction(fn)` | Execute function in transaction |
| `db:table_exists(name)` | Check if table exists |
| `db:table_info(name)` | Get column information |
| `db:close()` | Close connection |

## Type Mapping

| SQLite Type | Lua Type |
|-------------|----------|
| INTEGER | integer |
| REAL | number |
| TEXT | string |
| BLOB | string |
| NULL | nil |

## Integration

To use this module in CopperMoon, register it in the runtime:

```rust
use coppermoon_sqlite;

// Register globally
coppermoon_sqlite::register_global(lua)?;

// Or get the module table
let sqlite_module = coppermoon_sqlite::register(lua)?;
```

## Documentation

For full documentation, visit [coppermoon.dev](https://coppermoon.dev).

## License

MIT License
