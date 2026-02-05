//! Interactive console input module
//!
//! Provides functions for user interaction: prompts, password input,
//! confirmations, and selection menus.

use mlua::{Lua, Table, Result};

/// Guard that disables raw mode when dropped, preventing stuck terminal state
struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// Register the `console` module
pub fn register(lua: &Lua) -> Result<Table> {
    let console = lua.create_table()?;

    console.set("prompt", lua.create_function(console_prompt)?)?;
    console.set("password", lua.create_function(console_password)?)?;
    console.set("confirm", lua.create_function(console_confirm)?)?;
    console.set("select", lua.create_function(console_select)?)?;
    console.set("multiselect", lua.create_function(console_multiselect)?)?;

    Ok(console)
}

/// Display a prompt and read user input
/// console.prompt("Name: ") --> "Alice"
/// console.prompt("Name: ", "default") --> uses default if empty
fn console_prompt(_: &Lua, (message, default): (String, Option<String>)) -> mlua::Result<String> {
    use std::io::{self, Write};

    if let Some(ref default_val) = default {
        print!("{} [{}] ", message, default_val);
    } else {
        print!("{}", message);
    }
    io::stdout().flush()
        .map_err(|e| mlua::Error::runtime(format!("IO error: {}", e)))?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)
        .map_err(|e| mlua::Error::runtime(format!("IO error: {}", e)))?;

    let input = input.trim_end_matches(|c| c == '\n' || c == '\r').to_string();
    if input.is_empty() {
        Ok(default.unwrap_or_default())
    } else {
        Ok(input)
    }
}

/// Read password input without echoing characters
/// console.password("Password: ") --> "secret"
fn console_password(_: &Lua, message: String) -> mlua::Result<String> {
    use std::io::{self, Write};
    use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

    print!("{}", message);
    io::stdout().flush()
        .map_err(|e| mlua::Error::runtime(format!("IO error: {}", e)))?;

    crossterm::terminal::enable_raw_mode()
        .map_err(|e| mlua::Error::runtime(format!("Failed to enable raw mode: {}", e)))?;

    // Guard ensures raw mode is disabled even on error/panic
    let _guard = RawModeGuard;

    let mut password = String::new();
    loop {
        if let Ok(Event::Key(KeyEvent { code, modifiers, .. })) = event::read() {
            match code {
                KeyCode::Enter => break,
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                    // Print newline before returning error
                    drop(_guard);
                    println!();
                    return Err(mlua::Error::runtime("Interrupted"));
                }
                KeyCode::Backspace | KeyCode::Delete => {
                    password.pop();
                }
                KeyCode::Char(c) => {
                    password.push(c);
                }
                _ => {}
            }
        }
    }

    // Guard drops here, disabling raw mode
    drop(_guard);
    println!();

    Ok(password)
}

/// Ask a yes/no confirmation question
/// console.confirm("Continue?") --> true/false
/// console.confirm("Continue?", true) --> default=true [Y/n]
fn console_confirm(_: &Lua, (message, default): (String, Option<bool>)) -> mlua::Result<bool> {
    use std::io::{self, Write};

    let hint = match default {
        Some(true) => " [Y/n] ",
        Some(false) => " [y/N] ",
        None => " [y/n] ",
    };

    print!("{}{}", message, hint);
    io::stdout().flush()
        .map_err(|e| mlua::Error::runtime(format!("IO error: {}", e)))?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)
        .map_err(|e| mlua::Error::runtime(format!("IO error: {}", e)))?;

    let input = input.trim().to_lowercase();
    if input.is_empty() {
        Ok(default.unwrap_or(false))
    } else {
        Ok(input == "y" || input == "yes" || input == "oui" || input == "o")
    }
}

/// Present a numbered list and let the user pick one option
/// local idx, val = console.select("Choose:", {"A", "B", "C"})
fn console_select(_: &Lua, (message, options): (String, Table)) -> mlua::Result<(i64, String)> {
    use std::io::{self, Write};

    println!("{}", message);

    let mut items: Vec<String> = Vec::new();
    let len = options.raw_len();
    for i in 1..=len {
        let value: String = options.get(i)?;
        items.push(value);
    }

    for (i, item) in items.iter().enumerate() {
        println!("  {}) {}", i + 1, item);
    }

    loop {
        print!("> ");
        io::stdout().flush()
            .map_err(|e| mlua::Error::runtime(format!("IO error: {}", e)))?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)
            .map_err(|e| mlua::Error::runtime(format!("IO error: {}", e)))?;

        if let Ok(n) = input.trim().parse::<usize>() {
            if n >= 1 && n <= items.len() {
                return Ok((n as i64, items[n - 1].clone()));
            }
        }

        println!("  Please enter a number between 1 and {}", items.len());
    }
}

/// Present a numbered list and let the user pick multiple options
/// local idxs, vals = console.multiselect("Pick:", {"X", "Y", "Z"})
fn console_multiselect(lua: &Lua, (message, options): (String, Table)) -> mlua::Result<(Table, Table)> {
    use std::io::{self, Write};

    println!("{}", message);

    let mut items: Vec<String> = Vec::new();
    let len = options.raw_len();
    for i in 1..=len {
        let value: String = options.get(i)?;
        items.push(value);
    }

    for (i, item) in items.iter().enumerate() {
        println!("  {}) {}", i + 1, item);
    }

    println!("  (Enter numbers separated by commas, e.g. 1,3)");

    loop {
        print!("> ");
        io::stdout().flush()
            .map_err(|e| mlua::Error::runtime(format!("IO error: {}", e)))?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)
            .map_err(|e| mlua::Error::runtime(format!("IO error: {}", e)))?;

        let input = input.trim();

        // Parse comma or space separated numbers
        let nums: Vec<usize> = input
            .split(|c: char| c == ',' || c == ' ')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .filter(|&n| n >= 1 && n <= items.len())
            .collect();

        if !nums.is_empty() {
            let indices = lua.create_table()?;
            let values = lua.create_table()?;

            for (i, &n) in nums.iter().enumerate() {
                indices.set(i as i64 + 1, n as i64)?;
                values.set(i as i64 + 1, items[n - 1].clone())?;
            }

            return Ok((indices, values));
        }

        println!("  Please enter at least one valid number (1-{})", items.len());
    }
}
