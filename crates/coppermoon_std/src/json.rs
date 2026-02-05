//! JSON module for CopperMoon
//!
//! Provides JSON encoding and decoding.

use coppermoon_core::Result;
use mlua::{Lua, Table, Value};
use serde_json::{self, Value as JsonValue};

/// Register the json module
pub fn register(lua: &Lua) -> Result<Table> {
    let json_table = lua.create_table()?;

    // json.encode(value) -> string
    json_table.set("encode", lua.create_function(json_encode)?)?;

    // json.decode(string) -> value
    json_table.set("decode", lua.create_function(json_decode)?)?;

    // json.pretty(value) -> string (formatted JSON)
    json_table.set("pretty", lua.create_function(json_pretty)?)?;

    Ok(json_table)
}

fn json_encode(_: &Lua, value: Value) -> mlua::Result<String> {
    let json_value = lua_to_json(&value)?;
    serde_json::to_string(&json_value)
        .map_err(|e| mlua::Error::runtime(format!("JSON encode error: {}", e)))
}

fn json_pretty(_: &Lua, value: Value) -> mlua::Result<String> {
    let json_value = lua_to_json(&value)?;
    serde_json::to_string_pretty(&json_value)
        .map_err(|e| mlua::Error::runtime(format!("JSON encode error: {}", e)))
}

fn json_decode(lua: &Lua, json_str: String) -> mlua::Result<Value> {
    let json_value: JsonValue = serde_json::from_str(&json_str)
        .map_err(|e| mlua::Error::runtime(format!("JSON decode error: {}", e)))?;
    json_to_lua(lua, &json_value)
}

/// Convert a Lua value to a JSON value
fn lua_to_json(value: &Value) -> mlua::Result<JsonValue> {
    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Boolean(b) => Ok(JsonValue::Bool(*b)),
        Value::Integer(i) => Ok(JsonValue::Number((*i).into())),
        Value::Number(n) => {
            serde_json::Number::from_f64(*n)
                .map(JsonValue::Number)
                .ok_or_else(|| mlua::Error::runtime("Invalid number for JSON (NaN or Infinity)"))
        }
        Value::String(s) => {
            let str = s.to_str()
                .map_err(|e| mlua::Error::runtime(format!("Invalid UTF-8: {}", e)))?;
            Ok(JsonValue::String(str.to_string()))
        }
        Value::Table(t) => {
            // Check if it's an array (sequential integer keys starting from 1)
            let mut is_array = true;
            let mut max_index = 0i64;

            for pair in t.clone().pairs::<Value, Value>() {
                if let Ok((key, _)) = pair {
                    match key {
                        Value::Integer(i) if i > 0 => {
                            if i > max_index {
                                max_index = i;
                            }
                        }
                        _ => {
                            is_array = false;
                            break;
                        }
                    }
                }
            }

            if is_array && max_index > 0 {
                // Convert as array
                let mut arr = Vec::with_capacity(max_index as usize);
                for i in 1..=max_index {
                    let val: Value = t.get(i)?;
                    arr.push(lua_to_json(&val)?);
                }
                Ok(JsonValue::Array(arr))
            } else {
                // Convert as object
                let mut obj = serde_json::Map::new();
                for pair in t.clone().pairs::<Value, Value>() {
                    if let Ok((key, val)) = pair {
                        let key_str = match &key {
                            Value::String(s) => s.to_str()
                                .map_err(|e| mlua::Error::runtime(format!("Invalid UTF-8 in key: {}", e)))?
                                .to_string(),
                            Value::Integer(i) => i.to_string(),
                            Value::Number(n) => n.to_string(),
                            _ => return Err(mlua::Error::runtime("JSON keys must be strings or numbers")),
                        };
                        obj.insert(key_str, lua_to_json(&val)?);
                    }
                }
                Ok(JsonValue::Object(obj))
            }
        }
        _ => Err(mlua::Error::runtime(format!(
            "Cannot convert {} to JSON",
            value.type_name()
        ))),
    }
}

/// Convert a JSON value to a Lua value
fn json_to_lua(lua: &Lua, value: &JsonValue) -> mlua::Result<Value> {
    match value {
        JsonValue::Null => Ok(Value::Nil),
        JsonValue::Bool(b) => Ok(Value::Boolean(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Err(mlua::Error::runtime("Invalid JSON number"))
            }
        }
        JsonValue::String(s) => {
            let lua_str = lua.create_string(s)?;
            Ok(Value::String(lua_str))
        }
        JsonValue::Array(arr) => {
            let table = lua.create_table()?;
            for (i, val) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua(lua, val)?)?;
            }
            Ok(Value::Table(table))
        }
        JsonValue::Object(obj) => {
            let table = lua.create_table()?;
            for (key, val) in obj {
                table.set(key.as_str(), json_to_lua(lua, val)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}
