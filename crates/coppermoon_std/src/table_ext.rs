//! Table extensions for Lua's built-in `table` table
//!
//! These functions are injected directly into Lua's `table` global table.
//! Called as `table.keys(t)`, `table.map(t, fn)`, etc.

use mlua::{Lua, Table, Function, Value, MultiValue, Result};

/// Register table extensions into the existing Lua `table` table
pub fn register(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    let table_mod: Table = globals.get("table")?;

    table_mod.set("keys", lua.create_function(table_keys)?)?;
    table_mod.set("values", lua.create_function(table_values)?)?;
    table_mod.set("merge", lua.create_function(table_merge)?)?;
    table_mod.set("map", lua.create_function(table_map)?)?;
    table_mod.set("filter", lua.create_function(table_filter)?)?;
    table_mod.set("find", lua.create_function(table_find)?)?;
    table_mod.set("reduce", lua.create_function(table_reduce)?)?;
    table_mod.set("contains", lua.create_function(table_contains)?)?;
    table_mod.set("slice", lua.create_function(table_slice)?)?;
    table_mod.set("reverse", lua.create_function(table_reverse)?)?;
    table_mod.set("count", lua.create_function(table_count)?)?;
    table_mod.set("clone", lua.create_function(table_clone)?)?;
    table_mod.set("is_empty", lua.create_function(table_is_empty)?)?;
    table_mod.set("flat", lua.create_function(table_flat)?)?;
    table_mod.set("freeze", lua.create_function(table_freeze)?)?;
    table_mod.set("is_frozen", lua.create_function(table_is_frozen)?)?;

    Ok(())
}

/// Get all keys from a table as an array
/// table.keys({name="Alice", age=30}) --> {"name", "age"}
fn table_keys(lua: &Lua, t: Table) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    let mut i = 1i64;
    for pair in t.pairs::<Value, Value>() {
        let (key, _) = pair?;
        result.set(i, key)?;
        i += 1;
    }
    Ok(result)
}

/// Get all values from a table as an array
/// table.values({name="Alice", age=30}) --> {"Alice", 30}
fn table_values(lua: &Lua, t: Table) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    let mut i = 1i64;
    for pair in t.pairs::<Value, Value>() {
        let (_, value) = pair?;
        result.set(i, value)?;
        i += 1;
    }
    Ok(result)
}

/// Shallow merge multiple tables (later tables overwrite earlier ones)
/// table.merge({a=1}, {b=2}, {a=3}) --> {a=3, b=2}
fn table_merge(lua: &Lua, args: MultiValue) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    for arg in args {
        if let Value::Table(t) = arg {
            for pair in t.pairs::<Value, Value>() {
                let (key, value) = pair?;
                result.set(key, value)?;
            }
        }
    }
    Ok(result)
}

/// Map over array elements, applying function to each
/// table.map({1,2,3}, function(v) return v * 2 end) --> {2,4,6}
fn table_map(lua: &Lua, (t, func): (Table, Function)) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    let len = t.raw_len();
    for i in 1..=len {
        let value: Value = t.get(i)?;
        let mapped: Value = func.call((value, i))?;
        result.set(i, mapped)?;
    }
    Ok(result)
}

/// Filter array elements, keeping only those where function returns true
/// table.filter({1,2,3,4}, function(v) return v > 2 end) --> {3,4}
fn table_filter(lua: &Lua, (t, func): (Table, Function)) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    let len = t.raw_len();
    let mut out_idx = 1i64;
    for i in 1..=len {
        let value: Value = t.get(i)?;
        let keep: bool = func.call((value.clone(), i))?;
        if keep {
            result.set(out_idx, value)?;
            out_idx += 1;
        }
    }
    Ok(result)
}

/// Find the first element where function returns true
/// table.find({1,2,3}, function(v) return v > 1 end) --> 2
fn table_find(_: &Lua, (t, func): (Table, Function)) -> mlua::Result<Value> {
    let len = t.raw_len();
    for i in 1..=len {
        let value: Value = t.get(i)?;
        let found: bool = func.call((value.clone(), i))?;
        if found {
            return Ok(value);
        }
    }
    Ok(Value::Nil)
}

/// Reduce array to single value using accumulator function
/// table.reduce({1,2,3}, function(acc, v) return acc + v end, 0) --> 6
fn table_reduce(_: &Lua, (t, func, init): (Table, Function, Option<Value>)) -> mlua::Result<Value> {
    let len = t.raw_len() as i64;
    let mut start = 1i64;

    let mut acc = if let Some(init_val) = init {
        init_val
    } else {
        // No initial value: use first element
        if len == 0 {
            return Ok(Value::Nil);
        }
        start = 2;
        t.get(1)?
    };

    for i in start..=len {
        let value: Value = t.get(i)?;
        acc = func.call((acc, value))?;
    }
    Ok(acc)
}

/// Check if table contains a specific value
/// table.contains({1,2,3}, 2) --> true
fn table_contains(_: &Lua, (t, target): (Table, Value)) -> mlua::Result<bool> {
    for pair in t.pairs::<Value, Value>() {
        let (_, value) = pair?;
        if value == target {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Get a slice of an array (1-indexed, inclusive)
/// table.slice({10,20,30,40,50}, 2, 4) --> {20,30,40}
fn table_slice(lua: &Lua, (t, from, to): (Table, i64, Option<i64>)) -> mlua::Result<Table> {
    let len = t.raw_len() as i64;
    let to = to.unwrap_or(len);
    let result = lua.create_table()?;
    let mut out_idx = 1i64;
    for i in from..=to {
        if i >= 1 && i <= len {
            let value: Value = t.get(i)?;
            result.set(out_idx, value)?;
            out_idx += 1;
        }
    }
    Ok(result)
}

/// Reverse an array
/// table.reverse({1,2,3}) --> {3,2,1}
fn table_reverse(lua: &Lua, t: Table) -> mlua::Result<Table> {
    let len = t.raw_len() as i64;
    let result = lua.create_table()?;
    let mut out_idx = 1i64;
    for i in (1..=len).rev() {
        let value: Value = t.get(i)?;
        result.set(out_idx, value)?;
        out_idx += 1;
    }
    Ok(result)
}

/// Count all entries in a table (works for hash tables too)
/// table.count({a=1, b=2, c=3}) --> 3
fn table_count(_: &Lua, t: Table) -> mlua::Result<i64> {
    let mut count = 0i64;
    for pair in t.pairs::<Value, Value>() {
        let _ = pair?;
        count += 1;
    }
    Ok(count)
}

/// Shallow clone a table
/// table.clone({a=1, b=2}) --> {a=1, b=2} (new table)
fn table_clone(lua: &Lua, t: Table) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    for pair in t.pairs::<Value, Value>() {
        let (key, value) = pair?;
        result.set(key, value)?;
    }
    Ok(result)
}

/// Check if a table has no entries
/// table.is_empty({}) --> true
fn table_is_empty(_: &Lua, t: Table) -> mlua::Result<bool> {
    for pair in t.pairs::<Value, Value>() {
        let _ = pair?;
        return Ok(false);
    }
    Ok(true)
}

/// Flatten nested arrays
/// table.flat({{1,2},{3,{4,5}}}) --> {1,2,3,{4,5}}
/// table.flat({{1,{2}},{3}}, 2) --> {1,2,3}
fn table_flat(lua: &Lua, (t, depth): (Table, Option<i32>)) -> mlua::Result<Table> {
    let depth = depth.unwrap_or(1);
    let result = lua.create_table()?;
    let mut out_idx = 1i64;
    flatten_into(&t, &result, &mut out_idx, depth)?;
    Ok(result)
}

fn flatten_into(source: &Table, dest: &Table, idx: &mut i64, depth: i32) -> mlua::Result<()> {
    let len = source.raw_len() as i64;
    for i in 1..=len {
        let value: Value = source.get(i)?;
        if depth > 0 {
            if let Value::Table(ref inner) = value {
                flatten_into(inner, dest, idx, depth - 1)?;
                continue;
            }
        }
        dest.set(*idx, value)?;
        *idx += 1;
    }
    Ok(())
}

/// Make a table read-only by wrapping it in a proxy with restricted metatable
/// table.freeze({a=1, b=2}) --> frozen proxy table
fn table_freeze(lua: &Lua, t: Table) -> mlua::Result<Table> {
    let proxy = lua.create_table()?;
    let meta = lua.create_table()?;

    // __index reads from the original table
    meta.set("__index", t.clone())?;

    // __newindex blocks writes
    meta.set("__newindex", lua.create_function(|_, (_t, _k, _v): (Value, Value, Value)| -> mlua::Result<()> {
        Err(mlua::Error::runtime("cannot modify a frozen table"))
    })?)?;

    // __len forwards to original
    let t_for_len = t.clone();
    meta.set("__len", lua.create_function(move |_, _: Value| {
        Ok(t_for_len.raw_len())
    })?)?;

    // __pairs for iteration support — return (next, original_table, nil)
    let t_for_pairs = t.clone();
    meta.set("__pairs", lua.create_function(move |lua, _: Value| {
        let next_fn: Function = lua.globals().get("next")?;
        Ok((next_fn, Value::Table(t_for_pairs.clone()), Value::Nil))
    })?)?;

    // __tostring
    meta.set("__tostring", lua.create_function(|_, _: Value| {
        Ok("frozen table")
    })?)?;

    // Mark as frozen
    meta.set("__frozen", true)?;

    proxy.set_metatable(Some(meta));
    Ok(proxy)
}

/// Check if a table is frozen
/// table.is_frozen(t) --> boolean
fn table_is_frozen(_: &Lua, t: Table) -> mlua::Result<bool> {
    if let Some(meta) = t.metatable() {
        Ok(meta.get::<bool>("__frozen").unwrap_or(false))
    } else {
        Ok(false)
    }
}
