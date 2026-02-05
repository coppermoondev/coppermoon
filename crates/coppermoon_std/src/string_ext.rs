//! String extensions for Lua's built-in `string` table
//!
//! These functions are injected directly into Lua's `string` global table,
//! which means they also work as methods on string values: `("hello"):trim()`

use mlua::{Lua, Table, Result};

/// Register string extensions into the existing Lua `string` table
pub fn register(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    let string_table: Table = globals.get("string")?;

    string_table.set("split", lua.create_function(string_split)?)?;
    string_table.set("trim", lua.create_function(string_trim)?)?;
    string_table.set("ltrim", lua.create_function(string_ltrim)?)?;
    string_table.set("rtrim", lua.create_function(string_rtrim)?)?;
    string_table.set("starts_with", lua.create_function(string_starts_with)?)?;
    string_table.set("ends_with", lua.create_function(string_ends_with)?)?;
    string_table.set("contains", lua.create_function(string_contains)?)?;
    string_table.set("pad_left", lua.create_function(string_pad_left)?)?;
    string_table.set("pad_right", lua.create_function(string_pad_right)?)?;
    string_table.set("pad_center", lua.create_function(string_pad_center)?)?;
    string_table.set("truncate", lua.create_function(string_truncate)?)?;
    string_table.set("lines", lua.create_function(string_lines)?)?;
    string_table.set("chars", lua.create_function(string_chars)?)?;
    string_table.set("replace_all", lua.create_function(string_replace_all)?)?;
    string_table.set("count", lua.create_function(string_count)?)?;
    string_table.set("slug", lua.create_function(string_slug)?)?;

    Ok(())
}

/// Split a string by separator
/// string.split("a,b,c", ",") --> {"a", "b", "c"}
fn string_split(lua: &Lua, (s, sep): (String, String)) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    if sep.is_empty() {
        // Empty separator splits into characters
        for (i, c) in s.chars().enumerate() {
            table.set(i as i64 + 1, c.to_string())?;
        }
    } else {
        for (i, part) in s.split(&*sep).enumerate() {
            table.set(i as i64 + 1, part)?;
        }
    }
    Ok(table)
}

/// Trim whitespace from both ends
/// string.trim("  hello  ") --> "hello"
fn string_trim(_: &Lua, s: String) -> mlua::Result<String> {
    Ok(s.trim().to_string())
}

/// Trim whitespace from the left
/// string.ltrim("  hello  ") --> "hello  "
fn string_ltrim(_: &Lua, s: String) -> mlua::Result<String> {
    Ok(s.trim_start().to_string())
}

/// Trim whitespace from the right
/// string.rtrim("  hello  ") --> "  hello"
fn string_rtrim(_: &Lua, s: String) -> mlua::Result<String> {
    Ok(s.trim_end().to_string())
}

/// Check if string starts with prefix
/// string.starts_with("hello", "he") --> true
fn string_starts_with(_: &Lua, (s, prefix): (String, String)) -> mlua::Result<bool> {
    Ok(s.starts_with(&*prefix))
}

/// Check if string ends with suffix
/// string.ends_with("hello", "lo") --> true
fn string_ends_with(_: &Lua, (s, suffix): (String, String)) -> mlua::Result<bool> {
    Ok(s.ends_with(&*suffix))
}

/// Check if string contains substring
/// string.contains("hello", "ell") --> true
fn string_contains(_: &Lua, (s, substr): (String, String)) -> mlua::Result<bool> {
    Ok(s.contains(&*substr))
}

/// Pad string on the left to reach target width
/// string.pad_left("hi", 10, ".") --> "........hi"
fn string_pad_left(_: &Lua, (s, width, fill): (String, usize, Option<String>)) -> mlua::Result<String> {
    let char_count = s.chars().count();
    if char_count >= width {
        return Ok(s);
    }
    let fill_char = fill.as_deref().and_then(|f| f.chars().next()).unwrap_or(' ');
    let padding = width - char_count;
    let mut result = String::with_capacity(s.len() + padding);
    for _ in 0..padding {
        result.push(fill_char);
    }
    result.push_str(&s);
    Ok(result)
}

/// Pad string on the right to reach target width
/// string.pad_right("hi", 10) --> "hi        "
fn string_pad_right(_: &Lua, (s, width, fill): (String, usize, Option<String>)) -> mlua::Result<String> {
    let char_count = s.chars().count();
    if char_count >= width {
        return Ok(s);
    }
    let fill_char = fill.as_deref().and_then(|f| f.chars().next()).unwrap_or(' ');
    let padding = width - char_count;
    let mut result = String::with_capacity(s.len() + padding);
    result.push_str(&s);
    for _ in 0..padding {
        result.push(fill_char);
    }
    Ok(result)
}

/// Pad string on both sides to center it
/// string.pad_center("hi", 10) --> "    hi    "
fn string_pad_center(_: &Lua, (s, width, fill): (String, usize, Option<String>)) -> mlua::Result<String> {
    let char_count = s.chars().count();
    if char_count >= width {
        return Ok(s);
    }
    let fill_char = fill.as_deref().and_then(|f| f.chars().next()).unwrap_or(' ');
    let total_padding = width - char_count;
    let left_pad = total_padding / 2;
    let right_pad = total_padding - left_pad;
    let mut result = String::with_capacity(s.len() + total_padding);
    for _ in 0..left_pad {
        result.push(fill_char);
    }
    result.push_str(&s);
    for _ in 0..right_pad {
        result.push(fill_char);
    }
    Ok(result)
}

/// Truncate string to max length, optionally adding a suffix
/// string.truncate("hello world", 5, "...") --> "he..."
fn string_truncate(_: &Lua, (s, max_len, suffix): (String, usize, Option<String>)) -> mlua::Result<String> {
    let char_count = s.chars().count();
    if char_count <= max_len {
        return Ok(s);
    }
    let suffix = suffix.unwrap_or_default();
    let suffix_len = suffix.chars().count();
    let cut_at = max_len.saturating_sub(suffix_len);
    let mut result: String = s.chars().take(cut_at).collect();
    result.push_str(&suffix);
    Ok(result)
}

/// Split string into lines
/// string.lines("a\nb\nc") --> {"a", "b", "c"}
fn string_lines(lua: &Lua, s: String) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (i, line) in s.lines().enumerate() {
        table.set(i as i64 + 1, line)?;
    }
    Ok(table)
}

/// Split string into individual characters
/// string.chars("abc") --> {"a", "b", "c"}
fn string_chars(lua: &Lua, s: String) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (i, c) in s.chars().enumerate() {
        table.set(i as i64 + 1, c.to_string())?;
    }
    Ok(table)
}

/// Replace all occurrences of a substring
/// string.replace_all("aabbcc", "bb", "xx") --> "aaxxcc"
fn string_replace_all(_: &Lua, (s, old, new): (String, String, String)) -> mlua::Result<String> {
    Ok(s.replace(&*old, &*new))
}

/// Count non-overlapping occurrences of a substring
/// string.count("banana", "an") --> 2
fn string_count(_: &Lua, (s, substr): (String, String)) -> mlua::Result<i64> {
    if substr.is_empty() {
        return Ok(0);
    }
    Ok(s.matches(&*substr).count() as i64)
}

/// Convert string to URL-friendly slug
/// string.slug("Hello World!") --> "hello-world"
fn string_slug(_: &Lua, s: String) -> mlua::Result<String> {
    let mut result = String::with_capacity(s.len());
    let mut prev_hyphen = false;

    for c in s.to_lowercase().chars() {
        if c.is_alphanumeric() {
            result.push(c);
            prev_hyphen = false;
        } else if !prev_hyphen && !result.is_empty() {
            result.push('-');
            prev_hyphen = true;
        }
    }

    let result = result.trim_end_matches('-').to_string();
    Ok(result)
}
