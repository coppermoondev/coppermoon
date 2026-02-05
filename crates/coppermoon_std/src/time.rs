//! Time module for CopperMoon
//!
//! Provides time-related utilities including sleep, timers, and time measurement.

use coppermoon_core::Result;
use coppermoon_core::event_loop::{
    self, TimerCallback, TimerType,
};
use mlua::{Lua, Table, Function};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use chrono::{DateTime, Utc, NaiveDateTime};

/// Register the time module
pub fn register(lua: &Lua) -> Result<Table> {
    let time_table = lua.create_table()?;

    // time.sleep(ms) — Sleep for milliseconds
    time_table.set("sleep", lua.create_function(time_sleep)?)?;

    // time.now() — Current Unix timestamp in seconds
    time_table.set("now", lua.create_function(time_now)?)?;

    // time.now_ms() — Current Unix timestamp in milliseconds
    time_table.set("now_ms", lua.create_function(time_now_ms)?)?;

    // time.monotonic() — Monotonic time in seconds (for measuring durations)
    time_table.set("monotonic", lua.create_function(time_monotonic)?)?;

    // time.monotonic_ms() — Monotonic time in milliseconds
    time_table.set("monotonic_ms", lua.create_function(time_monotonic_ms)?)?;

    // time.format(timestamp, format) — Format a timestamp
    time_table.set("format", lua.create_function(time_format)?)?;

    // time.parse(str, format) — Parse a time string
    time_table.set("parse", lua.create_function(time_parse)?)?;

    // DateTime API (time.date, time.utc, time.isLeapYear, time.daysInMonth)
    crate::datetime::register(lua, &time_table)?;

    Ok(time_table)
}

/// Register global timer functions (setTimeout, setInterval, clearTimeout, clearInterval)
pub fn register_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();

    // setTimeout(fn, ms) -> timer_id
    globals.set("setTimeout", lua.create_function(set_timeout)?)?;

    // setInterval(fn, ms) -> timer_id
    globals.set("setInterval", lua.create_function(set_interval)?)?;

    // clearTimeout(timer_id)
    globals.set("clearTimeout", lua.create_function(clear_timeout)?)?;

    // clearInterval(timer_id) - alias for clearTimeout
    globals.set("clearInterval", lua.create_function(clear_timeout)?)?;

    Ok(())
}

fn time_sleep(_: &Lua, ms: u64) -> mlua::Result<()> {
    coppermoon_core::async_runtime::sleep_blocking(ms);
    Ok(())
}

fn time_now(_: &Lua, _: ()) -> mlua::Result<f64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| mlua::Error::runtime(format!("Time error: {}", e)))?;
    Ok(duration.as_secs_f64())
}

fn time_now_ms(_: &Lua, _: ()) -> mlua::Result<u64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| mlua::Error::runtime(format!("Time error: {}", e)))?;
    Ok(duration.as_millis() as u64)
}

// Store the start time for monotonic measurements
static START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn get_start_time() -> &'static Instant {
    START_TIME.get_or_init(Instant::now)
}

fn time_monotonic(_: &Lua, _: ()) -> mlua::Result<f64> {
    let elapsed = get_start_time().elapsed();
    Ok(elapsed.as_secs_f64())
}

fn time_monotonic_ms(_: &Lua, _: ()) -> mlua::Result<u64> {
    let elapsed = get_start_time().elapsed();
    Ok(elapsed.as_millis() as u64)
}

fn time_format(_: &Lua, (timestamp, format): (f64, Option<String>)) -> mlua::Result<String> {
    let secs = timestamp as i64;
    let nsecs = ((timestamp - secs as f64).abs() * 1_000_000_000.0) as u32;

    let dt = DateTime::<Utc>::from_timestamp(secs, nsecs)
        .ok_or_else(|| mlua::Error::runtime("Time error: invalid timestamp"))?;

    let format_str = format.unwrap_or_else(|| "%Y-%m-%dT%H:%M:%SZ".to_string());
    Ok(dt.format(&format_str).to_string())
}

fn time_parse(_: &Lua, (time_str, format): (String, Option<String>)) -> mlua::Result<f64> {
    // With explicit format string
    if let Some(fmt) = format {
        let naive = NaiveDateTime::parse_from_str(&time_str, &fmt)
            .map_err(|e| mlua::Error::runtime(format!("Parse error: {}", e)))?;
        return Ok(naive.and_utc().timestamp() as f64);
    }

    // Try RFC 3339 / ISO 8601
    if let Ok(dt) = DateTime::parse_from_rfc3339(&time_str) {
        return Ok(dt.timestamp() as f64 + dt.timestamp_subsec_millis() as f64 / 1000.0);
    }

    // Try common formats
    let formats = [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
    ];
    for fmt in formats {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&time_str, fmt) {
            return Ok(naive.and_utc().timestamp() as f64);
        }
    }

    // Try date-only
    if let Ok(naive_date) = chrono::NaiveDate::parse_from_str(&time_str, "%Y-%m-%d") {
        let naive = naive_date.and_hms_opt(0, 0, 0).unwrap();
        return Ok(naive.and_utc().timestamp() as f64);
    }

    Err(mlua::Error::runtime(format!("Cannot parse time string: '{}'", time_str)))
}

// ---------------------------------------------------------------------------
// setTimeout / setInterval / clearTimeout
// ---------------------------------------------------------------------------

fn set_timeout(lua: &Lua, (callback, ms): (Function, u64)) -> mlua::Result<u64> {
    let timer_id = event_loop::next_timer_id();

    // Store callback in the Lua registry so it stays alive
    let registry_key = lua.create_registry_value(callback)?;

    // Register with the global event loop infrastructure
    event_loop::register_timer(timer_id, TimerCallback {
        registry_key,
        timer_type: TimerType::Timeout,
    });

    // Spawn a Tokio task that sleeps then signals readiness
    coppermoon_core::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        if !event_loop::is_timer_cancelled(timer_id) {
            event_loop::send_timer_ready(timer_id);
        }
    });

    Ok(timer_id)
}

fn set_interval(lua: &Lua, (callback, ms): (Function, u64)) -> mlua::Result<u64> {
    let timer_id = event_loop::next_timer_id();

    // Store callback in the Lua registry
    let registry_key = lua.create_registry_value(callback)?;

    // Register with the global event loop infrastructure
    event_loop::register_timer(timer_id, TimerCallback {
        registry_key,
        timer_type: TimerType::Interval { ms },
    });

    // Spawn a Tokio task that repeatedly sleeps and signals
    coppermoon_core::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            if event_loop::is_timer_cancelled(timer_id) {
                break;
            }
            event_loop::send_timer_ready(timer_id);
        }
    });

    Ok(timer_id)
}

fn clear_timeout(_: &Lua, timer_id: u64) -> mlua::Result<()> {
    event_loop::cancel_timer(timer_id);
    Ok(())
}
