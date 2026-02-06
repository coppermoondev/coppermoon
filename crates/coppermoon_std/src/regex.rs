//! Regex module for CopperMoon
//!
//! Provides regular expression matching, searching, replacing, and splitting
//! using the Rust `regex` crate. Exposes both one-off convenience functions
//! and a compiled Pattern object for repeated use.
//!
//! Registered as global `re`.
//!
//! ## Lua Usage
//!
//! ```lua
//! -- One-off test
//! if re.test("\\d+", "abc123") then print("has numbers") end
//!
//! -- Match with captures
//! local m = re.match("(\\w+)@(\\w+)", "user@host")
//! print(m.groups[1])  -- "user"
//!
//! -- Compiled pattern for reuse
//! local p = re.compile("\\d+")
//! local all = p:findAll("a1b22c333")
//! ```

use coppermoon_core::Result;
use mlua::{Lua, MetaMethod, Table, UserData, UserDataMethods, Value};

// ---------------------------------------------------------------------------
// Pattern struct (UserData)
// ---------------------------------------------------------------------------

struct Pattern {
    regex: regex::Regex,
    source: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a regex::Regex from a pattern string and optional flag characters.
///
/// Supported flags:
/// - `i` — case-insensitive
/// - `m` — multiline (^ and $ match line boundaries)
/// - `s` — dotall (. matches \n)
/// - `x` — extended mode (ignore whitespace + # comments)
/// - `U` — ungreedy (swap meaning of greedy/lazy quantifiers)
fn build_pattern_with_flags(pattern: &str, flags: Option<&str>) -> mlua::Result<regex::Regex> {
    let prefix = match flags {
        Some(f) if !f.is_empty() => {
            let mut prefix = String::from("(?");
            for ch in f.chars() {
                match ch {
                    'i' | 'm' | 's' | 'x' | 'U' => prefix.push(ch),
                    _ => {
                        return Err(mlua::Error::runtime(format!(
                            "re: unknown flag '{}'. Valid flags: i, m, s, x, U",
                            ch
                        )))
                    }
                }
            }
            prefix.push(')');
            prefix
        }
        _ => String::new(),
    };

    let full_pattern = format!("{}{}", prefix, pattern);
    regex::Regex::new(&full_pattern)
        .map_err(|e| mlua::Error::runtime(format!("re: invalid pattern: {}", e)))
}

/// Convert regex::Captures into a Lua table with match info.
///
/// Result table structure:
/// ```lua
/// {
///     match = "matched text",
///     start = 5,                    -- 1-indexed byte position
///     ["end"] = 8,                  -- 1-indexed end (inclusive)
///     groups = {"cap1", "cap2"},    -- numbered capture groups
///     named = { name = "value" }    -- only if named groups exist
/// }
/// ```
fn captures_to_table(
    lua: &Lua,
    caps: &regex::Captures,
    re: &regex::Regex,
) -> mlua::Result<Table> {
    let result = lua.create_table()?;

    // Full match info
    if let Some(m) = caps.get(0) {
        result.set("match", lua.create_string(m.as_str())?)?;
        result.set("start", (m.start() + 1) as i64)?; // 1-indexed for Lua
        result.set("end", m.end() as i64)?; // end is exclusive in regex, so +1-1 = same as inclusive 1-indexed
    }

    // Numbered captures (1-indexed array)
    let groups = lua.create_table()?;
    for i in 1..caps.len() {
        match caps.get(i) {
            Some(m) => groups.set(i as i64, lua.create_string(m.as_str())?)?,
            None => groups.set(i as i64, Value::Nil)?,
        }
    }
    result.set("groups", groups)?;

    // Named captures (only if pattern has any)
    let named = lua.create_table()?;
    let mut has_named = false;
    for name in re.capture_names().flatten() {
        has_named = true;
        match caps.name(name) {
            Some(m) => named.set(name, lua.create_string(m.as_str())?)?,
            None => named.set(name, Value::Nil)?,
        }
    }
    if has_named {
        result.set("named", named)?;
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// UserData implementation for Pattern
// ---------------------------------------------------------------------------

impl UserData for Pattern {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // pattern:test(text) -> boolean
        methods.add_method("test", |_, this, text: String| {
            Ok(this.regex.is_match(&text))
        });

        // pattern:match(text) -> table|nil
        methods.add_method("match", |lua, this, text: String| {
            match this.regex.captures(&text) {
                Some(caps) => Ok(Value::Table(captures_to_table(lua, &caps, &this.regex)?)),
                None => Ok(Value::Nil),
            }
        });

        // pattern:find(text) -> table|nil (alias for match)
        methods.add_method("find", |lua, this, text: String| {
            match this.regex.captures(&text) {
                Some(caps) => Ok(Value::Table(captures_to_table(lua, &caps, &this.regex)?)),
                None => Ok(Value::Nil),
            }
        });

        // pattern:findAll(text) -> table (array of match results)
        methods.add_method("findAll", |lua, this, text: String| {
            let results = lua.create_table()?;
            let mut idx = 1i64;
            for caps in this.regex.captures_iter(&text) {
                results.set(idx, captures_to_table(lua, &caps, &this.regex)?)?;
                idx += 1;
            }
            Ok(results)
        });

        // pattern:replace(text, replacement) -> string
        methods.add_method("replace", |_, this, (text, replacement): (String, String)| {
            Ok(this.regex.replace(&text, replacement.as_str()).into_owned())
        });

        // pattern:replaceAll(text, replacement) -> string
        methods.add_method(
            "replaceAll",
            |_, this, (text, replacement): (String, String)| {
                Ok(this
                    .regex
                    .replace_all(&text, replacement.as_str())
                    .into_owned())
            },
        );

        // pattern:split(text) -> table (array of parts)
        methods.add_method("split", |lua, this, text: String| {
            let table = lua.create_table()?;
            for (i, part) in this.regex.split(&text).enumerate() {
                table.set((i + 1) as i64, lua.create_string(part)?)?;
            }
            Ok(table)
        });

        // pattern:source() -> string (original pattern)
        methods.add_method("source", |_, this, _: ()| Ok(this.source.clone()));

        // __tostring metamethod
        methods.add_meta_method(MetaMethod::ToString, |_, this, _: ()| {
            Ok(format!("Pattern({})", this.source))
        });
    }
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

/// re.compile(pattern, flags?) -> Pattern
fn re_compile(_: &Lua, (pattern, flags): (String, Option<String>)) -> mlua::Result<Pattern> {
    let regex = build_pattern_with_flags(&pattern, flags.as_deref())?;
    Ok(Pattern { regex, source: pattern })
}

/// re.test(pattern, text, flags?) -> boolean
fn re_test(
    _: &Lua,
    (pattern, text, flags): (String, String, Option<String>),
) -> mlua::Result<bool> {
    let regex = build_pattern_with_flags(&pattern, flags.as_deref())?;
    Ok(regex.is_match(&text))
}

/// re.match(pattern, text, flags?) -> table|nil
fn re_match(
    lua: &Lua,
    (pattern, text, flags): (String, String, Option<String>),
) -> mlua::Result<Value> {
    let regex = build_pattern_with_flags(&pattern, flags.as_deref())?;
    match regex.captures(&text) {
        Some(caps) => Ok(Value::Table(captures_to_table(lua, &caps, &regex)?)),
        None => Ok(Value::Nil),
    }
}

/// re.find(pattern, text, flags?) -> table|nil
fn re_find(
    lua: &Lua,
    (pattern, text, flags): (String, String, Option<String>),
) -> mlua::Result<Value> {
    let regex = build_pattern_with_flags(&pattern, flags.as_deref())?;
    match regex.captures(&text) {
        Some(caps) => Ok(Value::Table(captures_to_table(lua, &caps, &regex)?)),
        None => Ok(Value::Nil),
    }
}

/// re.findAll(pattern, text, flags?) -> table
fn re_find_all(
    lua: &Lua,
    (pattern, text, flags): (String, String, Option<String>),
) -> mlua::Result<Table> {
    let regex = build_pattern_with_flags(&pattern, flags.as_deref())?;
    let results = lua.create_table()?;
    let mut idx = 1i64;
    for caps in regex.captures_iter(&text) {
        results.set(idx, captures_to_table(lua, &caps, &regex)?)?;
        idx += 1;
    }
    Ok(results)
}

/// re.replace(pattern, text, replacement, flags?) -> string
fn re_replace(
    _: &Lua,
    (pattern, text, replacement, flags): (String, String, String, Option<String>),
) -> mlua::Result<String> {
    let regex = build_pattern_with_flags(&pattern, flags.as_deref())?;
    Ok(regex.replace(&text, replacement.as_str()).into_owned())
}

/// re.replaceAll(pattern, text, replacement, flags?) -> string
fn re_replace_all(
    _: &Lua,
    (pattern, text, replacement, flags): (String, String, String, Option<String>),
) -> mlua::Result<String> {
    let regex = build_pattern_with_flags(&pattern, flags.as_deref())?;
    Ok(regex
        .replace_all(&text, replacement.as_str())
        .into_owned())
}

/// re.split(pattern, text, flags?) -> table
fn re_split(
    lua: &Lua,
    (pattern, text, flags): (String, String, Option<String>),
) -> mlua::Result<Table> {
    let regex = build_pattern_with_flags(&pattern, flags.as_deref())?;
    let table = lua.create_table()?;
    for (i, part) in regex.split(&text).enumerate() {
        table.set((i + 1) as i64, lua.create_string(part)?)?;
    }
    Ok(table)
}

/// re.escape(text) -> string
fn re_escape(_: &Lua, text: String) -> mlua::Result<String> {
    Ok(regex::escape(&text))
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

pub fn register(lua: &Lua) -> Result<Table> {
    let re_table = lua.create_table()?;

    re_table.set("compile", lua.create_function(re_compile)?)?;
    re_table.set("test", lua.create_function(re_test)?)?;
    re_table.set("match", lua.create_function(re_match)?)?;
    re_table.set("find", lua.create_function(re_find)?)?;
    re_table.set("findAll", lua.create_function(re_find_all)?)?;
    re_table.set("replace", lua.create_function(re_replace)?)?;
    re_table.set("replaceAll", lua.create_function(re_replace_all)?)?;
    re_table.set("split", lua.create_function(re_split)?)?;
    re_table.set("escape", lua.create_function(re_escape)?)?;

    Ok(re_table)
}
