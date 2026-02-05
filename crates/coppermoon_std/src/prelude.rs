//! Prelude module - global functions and utilities

use coppermoon_core::Result;
use mlua::Lua;

/// Register prelude functions
pub fn register(lua: &Lua) -> Result<()> {
    let globals = lua.globals();

    // Enhanced print function with better formatting
    let print_fn = lua.create_function(|_, args: mlua::MultiValue| {
        let output: Vec<String> = args
            .iter()
            .map(|v| format_value(v))
            .collect();
        println!("{}", output.join("\t"));
        Ok(())
    })?;
    globals.set("print", print_fn)?;

    // Version information
    globals.set("_COPPERMOON_VERSION", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}

fn format_value(value: &mlua::Value) -> String {
    match value {
        mlua::Value::Nil => "nil".to_string(),
        mlua::Value::Boolean(b) => b.to_string(),
        mlua::Value::Integer(i) => i.to_string(),
        mlua::Value::Number(n) => {
            if n.fract() == 0.0 {
                format!("{:.0}", n)
            } else {
                n.to_string()
            }
        }
        mlua::Value::String(s) => {
            match s.to_str() {
                Ok(str) => str.to_string(),
                Err(_) => "<invalid utf8>".to_string(),
            }
        }
        mlua::Value::Table(t) => format_table(t),
        mlua::Value::Function(_) => "function".to_string(),
        mlua::Value::Thread(_) => "thread".to_string(),
        mlua::Value::UserData(_) => "userdata".to_string(),
        mlua::Value::LightUserData(_) => "lightuserdata".to_string(),
        mlua::Value::Error(e) => format!("error: {}", e),
        _ => "unknown".to_string(),
    }
}

fn format_table(table: &mlua::Table) -> String {
    let mut parts = Vec::new();
    let mut is_array = true;
    let mut index = 1i64;

    // First pass: check if it's an array
    for pair in table.clone().pairs::<mlua::Value, mlua::Value>() {
        if let Ok((key, _)) = pair {
            match key {
                mlua::Value::Integer(i) if i == index => {
                    index += 1;
                }
                _ => {
                    is_array = false;
                    break;
                }
            }
        }
    }

    // Second pass: format
    for pair in table.clone().pairs::<mlua::Value, mlua::Value>() {
        if let Ok((key, value)) = pair {
            if is_array {
                parts.push(format_value(&value));
            } else {
                let key_str = match &key {
                    mlua::Value::String(s) => {
                        match s.to_str() {
                            Ok(str) => str.to_string(),
                            Err(_) => "?".to_string(),
                        }
                    }
                    _ => format_value(&key),
                };
                parts.push(format!("{} = {}", key_str, format_value(&value)));
            }
        }
    }

    if is_array {
        format!("{{ {} }}", parts.join(", "))
    } else {
        format!("{{ {} }}", parts.join(", "))
    }
}
