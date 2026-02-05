//! DateTime module for CopperMoon
//!
//! Provides a complete DateTime type inspired by Moment.js with immutable operations,
//! Moment.js-style format tokens, relative time humanization, and calendar arithmetic.

use chrono::{
    DateTime, Datelike, Duration, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime,
    Timelike, Utc, Weekday,
};
use mlua::prelude::*;
use mlua::{MetaMethod, Table, UserData, UserDataMethods, Value};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn dt_err(msg: impl std::fmt::Display) -> LuaError {
    LuaError::runtime(format!("DateTime: {}", msg))
}

fn value_to_i32(v: &Value) -> LuaResult<i32> {
    match v {
        Value::Integer(n) => Ok(*n as i32),
        Value::Number(n) => Ok(*n as i32),
        _ => Err(dt_err("expected number")),
    }
}

fn value_to_u32(v: &Value) -> LuaResult<u32> {
    match v {
        Value::Integer(n) => Ok(*n as u32),
        Value::Number(n) => Ok(*n as u32),
        _ => Err(dt_err("expected number")),
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(year) { 29 } else { 28 },
        _ => 30,
    }
}

fn utc_offset() -> FixedOffset {
    FixedOffset::east_opt(0).unwrap()
}

fn local_offset() -> FixedOffset {
    *Local::now().offset()
}

fn normalize_unit(unit: &str) -> &str {
    match unit {
        "year" | "years" | "y" => "years",
        "month" | "months" | "M" => "months",
        "week" | "weeks" | "w" => "weeks",
        "day" | "days" | "d" => "days",
        "hour" | "hours" | "h" => "hours",
        "minute" | "minutes" | "m" => "minutes",
        "second" | "seconds" | "s" => "seconds",
        "millisecond" | "milliseconds" | "ms" => "milliseconds",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Month/weekday names
// ---------------------------------------------------------------------------

fn month_name_full(month: u32) -> &'static str {
    match month {
        1 => "January", 2 => "February", 3 => "March", 4 => "April",
        5 => "May", 6 => "June", 7 => "July", 8 => "August",
        9 => "September", 10 => "October", 11 => "November", 12 => "December",
        _ => "Unknown",
    }
}

fn month_name_short(month: u32) -> &'static str {
    match month {
        1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr",
        5 => "May", 6 => "Jun", 7 => "Jul", 8 => "Aug",
        9 => "Sep", 10 => "Oct", 11 => "Nov", 12 => "Dec",
        _ => "???",
    }
}

fn weekday_name_full(wd: Weekday) -> &'static str {
    match wd {
        Weekday::Mon => "Monday", Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday", Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday", Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

fn weekday_name_short(wd: Weekday) -> &'static str {
    match wd {
        Weekday::Mon => "Mon", Weekday::Tue => "Tue",
        Weekday::Wed => "Wed", Weekday::Thu => "Thu",
        Weekday::Fri => "Fri", Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
}

// ---------------------------------------------------------------------------
// Calendar arithmetic
// ---------------------------------------------------------------------------

fn add_months_to(dt: DateTime<FixedOffset>, months: i32) -> LuaResult<DateTime<FixedOffset>> {
    let total = dt.year() as i64 * 12 + (dt.month() as i64 - 1) + months as i64;
    let new_year = (total.div_euclid(12)) as i32;
    let new_month = (total.rem_euclid(12) + 1) as u32;
    let max_day = days_in_month(new_year, new_month);
    let new_day = dt.day().min(max_day);

    let date = NaiveDate::from_ymd_opt(new_year, new_month, new_day)
        .ok_or_else(|| dt_err(format!("invalid date after month arithmetic: {}-{}-{}", new_year, new_month, new_day)))?;
    let time = NaiveTime::from_hms_milli_opt(
        dt.hour(), dt.minute(), dt.second(), dt.timestamp_subsec_millis()
    ).unwrap();
    let naive = NaiveDateTime::new(date, time);
    Ok(naive.and_local_timezone(*dt.offset()).unwrap())
}

fn apply_duration(dt: DateTime<FixedOffset>, amount: i64, unit: &str) -> LuaResult<DateTime<FixedOffset>> {
    match normalize_unit(unit) {
        "years" => add_months_to(dt, amount as i32 * 12),
        "months" => add_months_to(dt, amount as i32),
        "weeks" => Ok(dt + Duration::weeks(amount)),
        "days" => Ok(dt + Duration::days(amount)),
        "hours" => Ok(dt + Duration::hours(amount)),
        "minutes" => Ok(dt + Duration::minutes(amount)),
        "seconds" => Ok(dt + Duration::seconds(amount)),
        "milliseconds" => Ok(dt + Duration::milliseconds(amount)),
        other => Err(dt_err(format!("unknown unit '{}'", other))),
    }
}

fn apply_table(dt: DateTime<FixedOffset>, tbl: &Table, sign: i64) -> LuaResult<DateTime<FixedOffset>> {
    let mut result = dt;

    // Calendar arithmetic first (months/years)
    let mut months: i32 = 0;
    if let Ok(y) = tbl.get::<i64>("years") { months += y as i32 * 12; }
    if let Ok(y) = tbl.get::<i64>("year") { months += y as i32 * 12; }
    if let Ok(m) = tbl.get::<i64>("months") { months += m as i32; }
    if let Ok(m) = tbl.get::<i64>("month") { months += m as i32; }
    if months != 0 {
        result = add_months_to(result, months * sign as i32)?;
    }

    // Duration arithmetic (weeks, days, hours, minutes, seconds, ms)
    let mut total_ms: i64 = 0;
    if let Ok(w) = tbl.get::<i64>("weeks") { total_ms += w * 7 * 86_400_000; }
    if let Ok(w) = tbl.get::<i64>("week") { total_ms += w * 7 * 86_400_000; }
    if let Ok(d) = tbl.get::<i64>("days") { total_ms += d * 86_400_000; }
    if let Ok(d) = tbl.get::<i64>("day") { total_ms += d * 86_400_000; }
    if let Ok(h) = tbl.get::<i64>("hours") { total_ms += h * 3_600_000; }
    if let Ok(h) = tbl.get::<i64>("hour") { total_ms += h * 3_600_000; }
    if let Ok(m) = tbl.get::<i64>("minutes") { total_ms += m * 60_000; }
    if let Ok(m) = tbl.get::<i64>("minute") { total_ms += m * 60_000; }
    if let Ok(s) = tbl.get::<i64>("seconds") { total_ms += s * 1_000; }
    if let Ok(s) = tbl.get::<i64>("second") { total_ms += s * 1_000; }
    if let Ok(ms) = tbl.get::<i64>("milliseconds") { total_ms += ms; }
    if let Ok(ms) = tbl.get::<i64>("ms") { total_ms += ms; }

    if total_ms != 0 {
        result = result + Duration::milliseconds(total_ms * sign);
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Moment.js-style format engine
// ---------------------------------------------------------------------------

fn format_moment(dt: &DateTime<FixedOffset>, fmt: &str) -> String {
    let chars: Vec<char> = fmt.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(fmt.len() + 16);
    let mut i = 0;

    while i < len {
        // Bracket escaping: [literal text]
        if chars[i] == '[' {
            i += 1;
            while i < len && chars[i] != ']' {
                result.push(chars[i]);
                i += 1;
            }
            if i < len { i += 1; }
            continue;
        }

        let remaining = len - i;

        // 4-char tokens
        if remaining >= 4 {
            let t4: String = chars[i..i + 4].iter().collect();
            match t4.as_str() {
                "YYYY" => { result.push_str(&format!("{:04}", dt.year())); i += 4; continue; }
                "MMMM" => { result.push_str(month_name_full(dt.month())); i += 4; continue; }
                "dddd" => { result.push_str(weekday_name_full(dt.weekday())); i += 4; continue; }
                _ => {}
            }
        }

        // 3-char tokens
        if remaining >= 3 {
            let t3: String = chars[i..i + 3].iter().collect();
            match t3.as_str() {
                "MMM" => { result.push_str(month_name_short(dt.month())); i += 3; continue; }
                "ddd" => { result.push_str(weekday_name_short(dt.weekday())); i += 3; continue; }
                "SSS" => { result.push_str(&format!("{:03}", dt.timestamp_subsec_millis())); i += 3; continue; }
                _ => {}
            }
        }

        // 2-char tokens
        if remaining >= 2 {
            let t2: String = chars[i..i + 2].iter().collect();
            match t2.as_str() {
                "YY" => { result.push_str(&format!("{:02}", (dt.year() % 100).unsigned_abs())); i += 2; continue; }
                "MM" => { result.push_str(&format!("{:02}", dt.month())); i += 2; continue; }
                "DD" => { result.push_str(&format!("{:02}", dt.day())); i += 2; continue; }
                "dd" => {
                    let name = weekday_name_short(dt.weekday());
                    result.push_str(&name[..2]);
                    i += 2; continue;
                }
                "HH" => { result.push_str(&format!("{:02}", dt.hour())); i += 2; continue; }
                "hh" => {
                    let h = dt.hour() % 12;
                    result.push_str(&format!("{:02}", if h == 0 { 12 } else { h }));
                    i += 2; continue;
                }
                "mm" => { result.push_str(&format!("{:02}", dt.minute())); i += 2; continue; }
                "ss" => { result.push_str(&format!("{:02}", dt.second())); i += 2; continue; }
                "ZZ" => {
                    let off = dt.offset().local_minus_utc();
                    let sign = if off >= 0 { '+' } else { '-' };
                    let abs = off.unsigned_abs();
                    result.push_str(&format!("{}{:02}{:02}", sign, abs / 3600, (abs % 3600) / 60));
                    i += 2; continue;
                }
                _ => {}
            }
        }

        // 1-char tokens
        match chars[i] {
            'Y' => { result.push_str(&format!("{}", dt.year())); i += 1; }
            'M' => { result.push_str(&format!("{}", dt.month())); i += 1; }
            'D' => { result.push_str(&format!("{}", dt.day())); i += 1; }
            'd' => { result.push_str(&format!("{}", dt.weekday().num_days_from_monday() + 1)); i += 1; }
            'H' => { result.push_str(&format!("{}", dt.hour())); i += 1; }
            'h' => {
                let h = dt.hour() % 12;
                result.push_str(&format!("{}", if h == 0 { 12 } else { h }));
                i += 1;
            }
            'm' => { result.push_str(&format!("{}", dt.minute())); i += 1; }
            's' => { result.push_str(&format!("{}", dt.second())); i += 1; }
            'A' => { result.push_str(if dt.hour() < 12 { "AM" } else { "PM" }); i += 1; }
            'a' => { result.push_str(if dt.hour() < 12 { "am" } else { "pm" }); i += 1; }
            'X' => { result.push_str(&format!("{}", dt.timestamp())); i += 1; }
            'x' => { result.push_str(&format!("{}", dt.timestamp_millis())); i += 1; }
            'Z' => {
                let off = dt.offset().local_minus_utc();
                let sign = if off >= 0 { '+' } else { '-' };
                let abs = off.unsigned_abs();
                result.push_str(&format!("{}{:02}:{:02}", sign, abs / 3600, (abs % 3600) / 60));
                i += 1;
            }
            c => { result.push(c); i += 1; }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Relative time humanization
// ---------------------------------------------------------------------------

fn humanize_duration(seconds: i64, invert: bool) -> String {
    let abs = seconds.unsigned_abs();
    let is_past = if invert { seconds < 0 } else { seconds > 0 };

    let text = if abs < 45 {
        "a few seconds".to_string()
    } else if abs < 90 {
        "a minute".to_string()
    } else if abs < 2700 {
        format!("{} minutes", abs / 60)
    } else if abs < 5400 {
        "an hour".to_string()
    } else if abs < 79200 {
        format!("{} hours", abs / 3600)
    } else if abs < 129600 {
        "a day".to_string()
    } else if abs < 2246400 {
        format!("{} days", abs / 86400)
    } else if abs < 3888000 {
        "a month".to_string()
    } else if abs < 29808000 {
        format!("{} months", abs / 2592000)
    } else if abs < 47304000 {
        "a year".to_string()
    } else {
        format!("{} years", abs / 31536000)
    };

    if is_past {
        format!("{} ago", text)
    } else {
        format!("in {}", text)
    }
}

// ---------------------------------------------------------------------------
// Start/end of period
// ---------------------------------------------------------------------------

fn start_of(dt: DateTime<FixedOffset>, unit: &str) -> LuaResult<DateTime<FixedOffset>> {
    let offset = *dt.offset();
    let naive = match normalize_unit(unit) {
        "years" => NaiveDate::from_ymd_opt(dt.year(), 1, 1).unwrap()
            .and_hms_opt(0, 0, 0).unwrap(),
        "months" => NaiveDate::from_ymd_opt(dt.year(), dt.month(), 1).unwrap()
            .and_hms_opt(0, 0, 0).unwrap(),
        "weeks" => {
            let days_since_monday = dt.weekday().num_days_from_monday();
            let monday = dt.naive_local().date() - Duration::days(days_since_monday as i64);
            monday.and_hms_opt(0, 0, 0).unwrap()
        }
        "days" => dt.naive_local().date().and_hms_opt(0, 0, 0).unwrap(),
        "hours" => NaiveDateTime::new(
            dt.naive_local().date(),
            NaiveTime::from_hms_opt(dt.hour(), 0, 0).unwrap(),
        ),
        "minutes" => NaiveDateTime::new(
            dt.naive_local().date(),
            NaiveTime::from_hms_opt(dt.hour(), dt.minute(), 0).unwrap(),
        ),
        "seconds" => NaiveDateTime::new(
            dt.naive_local().date(),
            NaiveTime::from_hms_opt(dt.hour(), dt.minute(), dt.second()).unwrap(),
        ),
        other => return Err(dt_err(format!("unknown unit '{}' for startOf", other))),
    };
    Ok(naive.and_local_timezone(offset).unwrap())
}

fn end_of(dt: DateTime<FixedOffset>, unit: &str) -> LuaResult<DateTime<FixedOffset>> {
    let offset = *dt.offset();
    let naive = match normalize_unit(unit) {
        "years" => NaiveDate::from_ymd_opt(dt.year(), 12, 31).unwrap()
            .and_hms_milli_opt(23, 59, 59, 999).unwrap(),
        "months" => {
            let last_day = days_in_month(dt.year(), dt.month());
            NaiveDate::from_ymd_opt(dt.year(), dt.month(), last_day).unwrap()
                .and_hms_milli_opt(23, 59, 59, 999).unwrap()
        }
        "weeks" => {
            let days_until_sunday = 6 - dt.weekday().num_days_from_monday();
            let sunday = dt.naive_local().date() + Duration::days(days_until_sunday as i64);
            sunday.and_hms_milli_opt(23, 59, 59, 999).unwrap()
        }
        "days" => dt.naive_local().date().and_hms_milli_opt(23, 59, 59, 999).unwrap(),
        "hours" => NaiveDateTime::new(
            dt.naive_local().date(),
            NaiveTime::from_hms_milli_opt(dt.hour(), 59, 59, 999).unwrap(),
        ),
        "minutes" => NaiveDateTime::new(
            dt.naive_local().date(),
            NaiveTime::from_hms_milli_opt(dt.hour(), dt.minute(), 59, 999).unwrap(),
        ),
        "seconds" => NaiveDateTime::new(
            dt.naive_local().date(),
            NaiveTime::from_hms_milli_opt(dt.hour(), dt.minute(), dt.second(), 999).unwrap(),
        ),
        other => return Err(dt_err(format!("unknown unit '{}' for endOf", other))),
    };
    Ok(naive.and_local_timezone(offset).unwrap())
}

// ---------------------------------------------------------------------------
// Diff helpers
// ---------------------------------------------------------------------------

fn diff_in_unit(a: &DateTime<FixedOffset>, b: &DateTime<FixedOffset>, unit: &str) -> LuaResult<f64> {
    match normalize_unit(unit) {
        "years" => {
            let months = diff_months(a, b);
            Ok(months as f64 / 12.0)
        }
        "months" => Ok(diff_months(a, b) as f64),
        "weeks" => {
            let dur = a.signed_duration_since(*b);
            Ok(dur.num_milliseconds() as f64 / (7.0 * 86_400_000.0))
        }
        "days" => {
            let dur = a.signed_duration_since(*b);
            Ok(dur.num_milliseconds() as f64 / 86_400_000.0)
        }
        "hours" => {
            let dur = a.signed_duration_since(*b);
            Ok(dur.num_milliseconds() as f64 / 3_600_000.0)
        }
        "minutes" => {
            let dur = a.signed_duration_since(*b);
            Ok(dur.num_milliseconds() as f64 / 60_000.0)
        }
        "seconds" => {
            let dur = a.signed_duration_since(*b);
            Ok(dur.num_milliseconds() as f64 / 1_000.0)
        }
        "milliseconds" => {
            let dur = a.signed_duration_since(*b);
            Ok(dur.num_milliseconds() as f64)
        }
        other => Err(dt_err(format!("unknown unit '{}' for diff", other))),
    }
}

fn diff_months(a: &DateTime<FixedOffset>, b: &DateTime<FixedOffset>) -> i64 {
    let months_a = a.year() as i64 * 12 + a.month() as i64;
    let months_b = b.year() as i64 * 12 + b.month() as i64;
    let mut diff = months_a - months_b;

    // Adjust: if we haven't completed a full month
    if diff > 0 && (a.day() < b.day() || (a.day() == b.day() && a.time() < b.time())) {
        diff -= 1;
    } else if diff < 0 && (a.day() > b.day() || (a.day() == b.day() && a.time() > b.time())) {
        diff += 1;
    }

    diff
}

// ---------------------------------------------------------------------------
// CopperDateTime struct
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct CopperDateTime {
    inner: DateTime<FixedOffset>,
}

impl CopperDateTime {
    fn now_local() -> Self {
        CopperDateTime { inner: Local::now().fixed_offset() }
    }

    fn now_utc() -> Self {
        CopperDateTime { inner: Utc::now().with_timezone(&utc_offset()) }
    }

    fn from_timestamp(ts: f64) -> LuaResult<Self> {
        let secs = ts as i64;
        let nsecs = ((ts - secs as f64).abs() * 1_000_000_000.0) as u32;
        let dt = DateTime::<Utc>::from_timestamp(secs, nsecs)
            .ok_or_else(|| dt_err("invalid timestamp"))?;
        Ok(CopperDateTime { inner: dt.with_timezone(&utc_offset()) })
    }

    fn from_components(
        year: i32, month: u32, day: u32,
        hour: u32, min: u32, sec: u32, ms: u32,
        offset: FixedOffset,
    ) -> LuaResult<Self> {
        let date = NaiveDate::from_ymd_opt(year, month, day)
            .ok_or_else(|| dt_err(format!("invalid date: {}-{}-{}", year, month, day)))?;
        let time = NaiveTime::from_hms_milli_opt(hour, min, sec, ms)
            .ok_or_else(|| dt_err(format!("invalid time: {}:{}:{}.{}", hour, min, sec, ms)))?;
        let naive = NaiveDateTime::new(date, time);
        Ok(CopperDateTime { inner: naive.and_local_timezone(offset).unwrap() })
    }

    fn parse_string(s: &str, default_offset: FixedOffset) -> LuaResult<Self> {
        // Try RFC 3339 / ISO 8601 with timezone
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Ok(CopperDateTime { inner: dt });
        }

        // Try ISO with offset variations
        let tz_formats = [
            "%Y-%m-%d %H:%M:%S%z",
            "%Y-%m-%dT%H:%M:%S%z",
            "%Y-%m-%d %H:%M:%S%.f%z",
            "%Y-%m-%dT%H:%M:%S%.f%z",
        ];
        for fmt in tz_formats {
            if let Ok(dt) = DateTime::parse_from_str(s, fmt) {
                return Ok(CopperDateTime { inner: dt });
            }
        }

        // Try datetime formats without timezone (use default_offset)
        let datetime_formats = [
            "%Y-%m-%dT%H:%M:%S%.f",
            "%Y-%m-%dT%H:%M:%S",
            "%Y-%m-%d %H:%M:%S%.f",
            "%Y-%m-%d %H:%M:%S",
            "%Y-%m-%d %H:%M",
            "%Y/%m/%d %H:%M:%S",
            "%Y/%m/%d %H:%M",
        ];
        for fmt in datetime_formats {
            if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
                return Ok(CopperDateTime {
                    inner: naive.and_local_timezone(default_offset).unwrap(),
                });
            }
        }

        // Try date-only formats
        let date_formats = ["%Y-%m-%d", "%Y/%m/%d"];
        for fmt in date_formats {
            if let Ok(naive_date) = NaiveDate::parse_from_str(s, fmt) {
                let naive = naive_date.and_hms_opt(0, 0, 0).unwrap();
                return Ok(CopperDateTime {
                    inner: naive.and_local_timezone(default_offset).unwrap(),
                });
            }
        }

        Err(dt_err(format!("cannot parse date string: '{}'", s)))
    }
}

// ---------------------------------------------------------------------------
// UserData implementation
// ---------------------------------------------------------------------------

impl UserData for CopperDateTime {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ---- Getters ----

        methods.add_method("year", |_, this, _: ()| Ok(this.inner.year()));
        methods.add_method("month", |_, this, _: ()| Ok(this.inner.month()));
        methods.add_method("day", |_, this, _: ()| Ok(this.inner.day()));
        methods.add_method("hour", |_, this, _: ()| Ok(this.inner.hour()));
        methods.add_method("minute", |_, this, _: ()| Ok(this.inner.minute()));
        methods.add_method("second", |_, this, _: ()| Ok(this.inner.second()));
        methods.add_method("milli", |_, this, _: ()| Ok(this.inner.timestamp_subsec_millis()));
        methods.add_method("weekday", |_, this, _: ()| {
            Ok(this.inner.weekday().num_days_from_monday() + 1) // 1=Mon, 7=Sun
        });
        methods.add_method("yearday", |_, this, _: ()| Ok(this.inner.ordinal()));
        methods.add_method("timestamp", |_, this, _: ()| {
            Ok(this.inner.timestamp() as f64 + this.inner.timestamp_subsec_millis() as f64 / 1000.0)
        });
        methods.add_method("timestamp_ms", |_, this, _: ()| Ok(this.inner.timestamp_millis()));
        methods.add_method("offset", |_, this, _: ()| {
            Ok(this.inner.offset().local_minus_utc() as f64 / 3600.0)
        });
        methods.add_method("isUTC", |_, this, _: ()| {
            Ok(this.inner.offset().local_minus_utc() == 0)
        });

        // ---- Setters (return new) ----

        methods.add_method("set", |_, this, tbl: Table| {
            let year = tbl.get::<i32>("year").unwrap_or(this.inner.year());
            let month = tbl.get::<u32>("month").unwrap_or(this.inner.month());
            let day = tbl.get::<u32>("day").unwrap_or(this.inner.day());
            let hour = tbl.get::<u32>("hour").unwrap_or(this.inner.hour());
            let minute = tbl.get::<u32>("minute").unwrap_or(this.inner.minute());
            let second = tbl.get::<u32>("second").unwrap_or(this.inner.second());
            let milli = tbl.get::<u32>("milli").unwrap_or(this.inner.timestamp_subsec_millis());
            CopperDateTime::from_components(year, month, day, hour, minute, second, milli, *this.inner.offset())
        });

        // ---- Arithmetic (return new, immutable) ----

        methods.add_method("add", |_, this, (amount, unit): (Value, Option<String>)| {
            match amount {
                Value::Table(ref tbl) => {
                    let result = apply_table(this.inner, tbl, 1)?;
                    Ok(CopperDateTime { inner: result })
                }
                Value::Integer(n) => {
                    let u = unit.ok_or_else(|| dt_err("add: unit string required as second argument"))?;
                    let result = apply_duration(this.inner, n, &u)?;
                    Ok(CopperDateTime { inner: result })
                }
                Value::Number(n) => {
                    let u = unit.ok_or_else(|| dt_err("add: unit string required as second argument"))?;
                    let result = apply_duration(this.inner, n as i64, &u)?;
                    Ok(CopperDateTime { inner: result })
                }
                _ => Err(dt_err("add: expected number or table as first argument")),
            }
        });

        methods.add_method("sub", |_, this, (amount, unit): (Value, Option<String>)| {
            match amount {
                Value::Table(ref tbl) => {
                    let result = apply_table(this.inner, tbl, -1)?;
                    Ok(CopperDateTime { inner: result })
                }
                Value::Integer(n) => {
                    let u = unit.ok_or_else(|| dt_err("sub: unit string required as second argument"))?;
                    let result = apply_duration(this.inner, -n, &u)?;
                    Ok(CopperDateTime { inner: result })
                }
                Value::Number(n) => {
                    let u = unit.ok_or_else(|| dt_err("sub: unit string required as second argument"))?;
                    let result = apply_duration(this.inner, -(n as i64), &u)?;
                    Ok(CopperDateTime { inner: result })
                }
                _ => Err(dt_err("sub: expected number or table as first argument")),
            }
        });

        // ---- Formatting ----

        methods.add_method("format", |_, this, fmt: Option<String>| {
            let pattern = fmt.unwrap_or_else(|| "YYYY-MM-DDTHH:mm:ssZ".to_string());
            Ok(format_moment(&this.inner, &pattern))
        });

        methods.add_method("toISO", |_, this, _: ()| {
            Ok(format_moment(&this.inner, "YYYY-MM-DDTHH:mm:ss.SSSZ"))
        });

        methods.add_method("toDate", |_, this, _: ()| {
            Ok(format_moment(&this.inner, "YYYY-MM-DD"))
        });

        methods.add_method("toTime", |_, this, _: ()| {
            Ok(format_moment(&this.inner, "HH:mm:ss"))
        });

        // ---- Comparison ----

        methods.add_method("isBefore", |_, this, other: mlua::AnyUserData| {
            let other_dt = other.borrow::<CopperDateTime>()?;
            Ok(this.inner < other_dt.inner)
        });

        methods.add_method("isAfter", |_, this, other: mlua::AnyUserData| {
            let other_dt = other.borrow::<CopperDateTime>()?;
            Ok(this.inner > other_dt.inner)
        });

        methods.add_method("isSame", |_, this, other: mlua::AnyUserData| {
            let other_dt = other.borrow::<CopperDateTime>()?;
            Ok(this.inner == other_dt.inner)
        });

        methods.add_method("isBetween", |_, this, (a, b): (mlua::AnyUserData, mlua::AnyUserData)| {
            let a_dt = a.borrow::<CopperDateTime>()?;
            let b_dt = b.borrow::<CopperDateTime>()?;
            let (lo, hi) = if a_dt.inner < b_dt.inner {
                (a_dt.inner, b_dt.inner)
            } else {
                (b_dt.inner, a_dt.inner)
            };
            Ok(this.inner > lo && this.inner < hi)
        });

        // ---- Diff ----

        methods.add_method("diff", |_, this, (other, unit): (mlua::AnyUserData, Option<String>)| {
            let other_dt = other.borrow::<CopperDateTime>()?;
            let u = unit.unwrap_or_else(|| "seconds".to_string());
            diff_in_unit(&this.inner, &other_dt.inner, &u)
        });

        // ---- Period ----

        methods.add_method("startOf", |_, this, unit: String| {
            Ok(CopperDateTime { inner: start_of(this.inner, &unit)? })
        });

        methods.add_method("endOf", |_, this, unit: String| {
            Ok(CopperDateTime { inner: end_of(this.inner, &unit)? })
        });

        // ---- Utilities ----

        methods.add_method("isLeapYear", |_, this, _: ()| {
            Ok(is_leap_year(this.inner.year()))
        });

        methods.add_method("daysInMonth", |_, this, _: ()| {
            Ok(days_in_month(this.inner.year(), this.inner.month()))
        });

        methods.add_method("clone", |_, this, _: ()| {
            Ok(CopperDateTime { inner: this.inner })
        });

        methods.add_method("toUTC", |_, this, _: ()| {
            Ok(CopperDateTime { inner: this.inner.with_timezone(&utc_offset()) })
        });

        methods.add_method("toLocal", |_, this, _: ()| {
            Ok(CopperDateTime { inner: this.inner.with_timezone(&local_offset()) })
        });

        // ---- Relative time ----

        methods.add_method("fromNow", |_, this, _: ()| {
            let diff = Utc::now().timestamp() - this.inner.timestamp();
            Ok(humanize_duration(diff, false))
        });

        methods.add_method("toNow", |_, this, _: ()| {
            let diff = Utc::now().timestamp() - this.inner.timestamp();
            Ok(humanize_duration(diff, true))
        });

        methods.add_method("from", |_, this, other: mlua::AnyUserData| {
            let other_dt = other.borrow::<CopperDateTime>()?;
            let diff = other_dt.inner.timestamp() - this.inner.timestamp();
            Ok(humanize_duration(diff, false))
        });

        methods.add_method("to", |_, this, other: mlua::AnyUserData| {
            let other_dt = other.borrow::<CopperDateTime>()?;
            let diff = other_dt.inner.timestamp() - this.inner.timestamp();
            Ok(humanize_duration(diff, true))
        });

        // ---- Metamethods ----

        methods.add_meta_method(MetaMethod::ToString, |_, this, _: ()| {
            Ok(format_moment(&this.inner, "YYYY-MM-DDTHH:mm:ss.SSSZ"))
        });

        methods.add_meta_method(MetaMethod::Eq, |_, this, other: mlua::AnyUserData| {
            let other_dt = other.borrow::<CopperDateTime>()?;
            Ok(this.inner == other_dt.inner)
        });

        methods.add_meta_method(MetaMethod::Lt, |_, this, other: mlua::AnyUserData| {
            let other_dt = other.borrow::<CopperDateTime>()?;
            Ok(this.inner < other_dt.inner)
        });

        methods.add_meta_method(MetaMethod::Le, |_, this, other: mlua::AnyUserData| {
            let other_dt = other.borrow::<CopperDateTime>()?;
            Ok(this.inner <= other_dt.inner)
        });

        methods.add_meta_method(MetaMethod::Sub, |_, this, other: mlua::AnyUserData| {
            let other_dt = other.borrow::<CopperDateTime>()?;
            let dur = this.inner.signed_duration_since(other_dt.inner);
            Ok(dur.num_milliseconds() as f64 / 1000.0)
        });
    }
}

// ---------------------------------------------------------------------------
// Factory functions
// ---------------------------------------------------------------------------

fn datetime_factory(_lua: &Lua, args: mlua::MultiValue, is_utc: bool) -> LuaResult<CopperDateTime> {
    let args: Vec<Value> = args.into_iter().collect();
    let default_offset = if is_utc { utc_offset() } else { local_offset() };

    match args.len() {
        0 => {
            if is_utc { Ok(CopperDateTime::now_utc()) }
            else { Ok(CopperDateTime::now_local()) }
        }
        1 => {
            match &args[0] {
                Value::Integer(n) => CopperDateTime::from_timestamp(*n as f64),
                Value::Number(n) => CopperDateTime::from_timestamp(*n),
                Value::String(s) => {
                    let str_ref = s.to_str().map_err(|e| dt_err(e))?;
                    CopperDateTime::parse_string(&str_ref, default_offset)
                }
                _ => Err(dt_err("expected number or string")),
            }
        }
        2 => Err(dt_err("expected 0, 1, or 3-7 arguments")),
        _ => {
            // 3-7 args: year, month, day[, hour, min, sec, ms]
            let year = value_to_i32(&args[0])?;
            let month = value_to_u32(&args[1])?;
            let day = value_to_u32(&args[2])?;
            let hour = if args.len() > 3 { value_to_u32(&args[3])? } else { 0 };
            let min  = if args.len() > 4 { value_to_u32(&args[4])? } else { 0 };
            let sec  = if args.len() > 5 { value_to_u32(&args[5])? } else { 0 };
            let ms   = if args.len() > 6 { value_to_u32(&args[6])? } else { 0 };
            CopperDateTime::from_components(year, month, day, hour, min, sec, ms, default_offset)
        }
    }
}

fn date_factory(lua: &Lua, args: mlua::MultiValue) -> LuaResult<CopperDateTime> {
    datetime_factory(lua, args, false)
}

fn utc_factory(lua: &Lua, args: mlua::MultiValue) -> LuaResult<CopperDateTime> {
    datetime_factory(lua, args, true)
}

fn is_leap_year_fn(_: &Lua, year: i32) -> LuaResult<bool> {
    Ok(is_leap_year(year))
}

fn days_in_month_fn(_: &Lua, (year, month): (i32, u32)) -> LuaResult<u32> {
    if month < 1 || month > 12 {
        return Err(dt_err(format!("invalid month: {}", month)));
    }
    Ok(days_in_month(year, month))
}

// ---------------------------------------------------------------------------
// Registration — called from time.rs
// ---------------------------------------------------------------------------

pub fn register(lua: &Lua, time_table: &Table) -> LuaResult<()> {
    time_table.set("date", lua.create_function(date_factory)?)?;
    time_table.set("utc", lua.create_function(utc_factory)?)?;
    time_table.set("isLeapYear", lua.create_function(is_leap_year_fn)?)?;
    time_table.set("daysInMonth", lua.create_function(days_in_month_fn)?)?;
    Ok(())
}
