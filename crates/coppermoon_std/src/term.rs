//! Terminal styling and control module
//!
//! Provides ANSI color/style functions that return styled strings,
//! plus terminal control functions (clear, size, cursor, is_tty).

use mlua::{Lua, Table, Result};

/// Helper macro to register a styling function that wraps text in ANSI codes
macro_rules! register_style {
    ($table:expr, $lua:expr, $name:expr, $open:expr) => {
        $table.set($name, $lua.create_function(|_, text: String| {
            Ok(format!("\x1b[{}m{}\x1b[0m", $open, text))
        })?)?;
    };
}

/// Register the `term` module
pub fn register(lua: &Lua) -> Result<Table> {
    let term = lua.create_table()?;

    // -- Foreground colors --
    register_style!(term, lua, "black", "30");
    register_style!(term, lua, "red", "31");
    register_style!(term, lua, "green", "32");
    register_style!(term, lua, "yellow", "33");
    register_style!(term, lua, "blue", "34");
    register_style!(term, lua, "magenta", "35");
    register_style!(term, lua, "cyan", "36");
    register_style!(term, lua, "white", "37");
    register_style!(term, lua, "gray", "90");
    register_style!(term, lua, "grey", "90");

    // -- Bright foreground colors --
    register_style!(term, lua, "bright_red", "91");
    register_style!(term, lua, "bright_green", "92");
    register_style!(term, lua, "bright_yellow", "93");
    register_style!(term, lua, "bright_blue", "94");
    register_style!(term, lua, "bright_magenta", "95");
    register_style!(term, lua, "bright_cyan", "96");
    register_style!(term, lua, "bright_white", "97");

    // -- Text decorations --
    register_style!(term, lua, "bold", "1");
    register_style!(term, lua, "dim", "2");
    register_style!(term, lua, "italic", "3");
    register_style!(term, lua, "underline", "4");
    register_style!(term, lua, "strikethrough", "9");

    // -- Background colors --
    register_style!(term, lua, "bg_black", "40");
    register_style!(term, lua, "bg_red", "41");
    register_style!(term, lua, "bg_green", "42");
    register_style!(term, lua, "bg_yellow", "43");
    register_style!(term, lua, "bg_blue", "44");
    register_style!(term, lua, "bg_magenta", "45");
    register_style!(term, lua, "bg_cyan", "46");
    register_style!(term, lua, "bg_white", "47");

    // -- RGB and 256-color --
    term.set("rgb", lua.create_function(|_, (r, g, b, text): (u8, u8, u8, String)| {
        Ok(format!("\x1b[38;2;{};{};{}m{}\x1b[0m", r, g, b, text))
    })?)?;

    term.set("bg_rgb", lua.create_function(|_, (r, g, b, text): (u8, u8, u8, String)| {
        Ok(format!("\x1b[48;2;{};{};{}m{}\x1b[0m", r, g, b, text))
    })?)?;

    term.set("color256", lua.create_function(|_, (code, text): (u8, String)| {
        Ok(format!("\x1b[38;5;{}m{}\x1b[0m", code, text))
    })?)?;

    term.set("bg_color256", lua.create_function(|_, (code, text): (u8, String)| {
        Ok(format!("\x1b[48;5;{}m{}\x1b[0m", code, text))
    })?)?;

    // -- Utility --
    term.set("strip", lua.create_function(|_, text: String| {
        Ok(strip_ansi(&text))
    })?)?;

    term.set("reset", lua.create_function(|_, _: ()| {
        Ok("\x1b[0m".to_string())
    })?)?;

    // -- Terminal control --
    term.set("clear", lua.create_function(|_, _: ()| {
        use crossterm::{execute, terminal::{Clear, ClearType}, cursor::MoveTo};
        use std::io::stdout;
        execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0))
            .map_err(|e| mlua::Error::runtime(format!("Failed to clear screen: {}", e)))?;
        Ok(())
    })?)?;

    term.set("clear_line", lua.create_function(|_, _: ()| {
        use crossterm::{execute, terminal::{Clear, ClearType}};
        use std::io::stdout;
        execute!(stdout(), Clear(ClearType::CurrentLine))
            .map_err(|e| mlua::Error::runtime(format!("Failed to clear line: {}", e)))?;
        Ok(())
    })?)?;

    term.set("size", lua.create_function(|_, _: ()| {
        let (cols, rows) = crossterm::terminal::size()
            .map_err(|e| mlua::Error::runtime(format!("Failed to get terminal size: {}", e)))?;
        Ok((cols, rows))
    })?)?;

    term.set("is_tty", lua.create_function(|_, _: ()| {
        use std::io::IsTerminal;
        Ok(std::io::stdout().is_terminal())
    })?)?;

    // -- Cursor control --
    term.set("cursor_to", lua.create_function(|_, (col, row): (u16, u16)| {
        use crossterm::{execute, cursor::MoveTo};
        use std::io::stdout;
        // Lua uses 1-indexed, crossterm uses 0-indexed
        let col = col.saturating_sub(1);
        let row = row.saturating_sub(1);
        execute!(stdout(), MoveTo(col, row))
            .map_err(|e| mlua::Error::runtime(format!("Failed to move cursor: {}", e)))?;
        Ok(())
    })?)?;

    term.set("cursor_up", lua.create_function(|_, n: Option<u16>| {
        use crossterm::{execute, cursor::MoveUp};
        use std::io::stdout;
        execute!(stdout(), MoveUp(n.unwrap_or(1)))
            .map_err(|e| mlua::Error::runtime(format!("Cursor error: {}", e)))?;
        Ok(())
    })?)?;

    term.set("cursor_down", lua.create_function(|_, n: Option<u16>| {
        use crossterm::{execute, cursor::MoveDown};
        use std::io::stdout;
        execute!(stdout(), MoveDown(n.unwrap_or(1)))
            .map_err(|e| mlua::Error::runtime(format!("Cursor error: {}", e)))?;
        Ok(())
    })?)?;

    term.set("cursor_left", lua.create_function(|_, n: Option<u16>| {
        use crossterm::{execute, cursor::MoveLeft};
        use std::io::stdout;
        execute!(stdout(), MoveLeft(n.unwrap_or(1)))
            .map_err(|e| mlua::Error::runtime(format!("Cursor error: {}", e)))?;
        Ok(())
    })?)?;

    term.set("cursor_right", lua.create_function(|_, n: Option<u16>| {
        use crossterm::{execute, cursor::MoveRight};
        use std::io::stdout;
        execute!(stdout(), MoveRight(n.unwrap_or(1)))
            .map_err(|e| mlua::Error::runtime(format!("Cursor error: {}", e)))?;
        Ok(())
    })?)?;

    term.set("cursor_hide", lua.create_function(|_, _: ()| {
        use crossterm::{execute, cursor::Hide};
        use std::io::stdout;
        execute!(stdout(), Hide)
            .map_err(|e| mlua::Error::runtime(format!("Cursor error: {}", e)))?;
        Ok(())
    })?)?;

    term.set("cursor_show", lua.create_function(|_, _: ()| {
        use crossterm::{execute, cursor::Show};
        use std::io::stdout;
        execute!(stdout(), Show)
            .map_err(|e| mlua::Error::runtime(format!("Cursor error: {}", e)))?;
        Ok(())
    })?)?;

    term.set("cursor_save", lua.create_function(|_, _: ()| {
        use crossterm::{execute, cursor::SavePosition};
        use std::io::stdout;
        execute!(stdout(), SavePosition)
            .map_err(|e| mlua::Error::runtime(format!("Cursor error: {}", e)))?;
        Ok(())
    })?)?;

    term.set("cursor_restore", lua.create_function(|_, _: ()| {
        use crossterm::{execute, cursor::RestorePosition};
        use std::io::stdout;
        execute!(stdout(), RestorePosition)
            .map_err(|e| mlua::Error::runtime(format!("Cursor error: {}", e)))?;
        Ok(())
    })?)?;

    Ok(term)
}

/// Strip all ANSI escape sequences from a string
fn strip_ansi(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ESC [ ... <letter> sequences (CSI sequences)
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Skip until we hit a letter (the terminator)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            // Skip other ESC sequences (ESC followed by a single char)
            else if chars.peek().is_some() {
                chars.next();
            }
        } else {
            result.push(c);
        }
    }

    result
}
