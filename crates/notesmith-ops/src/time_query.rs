//! Natural-language time-expression parsing for the `time_query` MCP tool
//! (issue #200).
//!
//! [`parse_time_range`] turns an expression such as `"last week"`, `"in May"`,
//! `"yesterday"` or `"last 3 days"` into a half-open `[start, end)` datetime
//! range. The heavy lifting is delegated to the [`two_timer`] crate; a thin
//! preprocessing layer handles a couple of common phrasings `two_timer` does
//! not accept on its own:
//!
//! * a leading preposition (`"in May"`, `"during 2021"`) is stripped so the
//!   remainder (`"May"`, `"2021"`) parses cleanly;
//! * trailing rolling windows (`"last 3 days"`, `"past 2 weeks"`) are resolved
//!   directly to `[now - N units, now]`.
//!
//! Ranges are half-open (`start` inclusive, `end` exclusive), matching
//! `two_timer`'s own convention, so adjacent expressions (e.g. consecutive
//! days) never double-count a boundary instant.

use std::sync::LazyLock;

use chrono::{Duration, Months, NaiveDateTime};
use regex::Regex;
use two_timer::{Config, parse as two_timer_parse};

/// Matches trailing rolling-window expressions like `last 3 days`,
/// `past 2 weeks`, `previous 6 months`, `last 1 year`.
static TRAILING_WINDOW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(?:last|past|previous)\s+(\d+)\s+(day|week|month|year)s?\s*$").unwrap()
});

/// Resolve a natural-language time expression into a half-open `[start, end)`
/// datetime range, evaluated relative to `now`.
///
/// Returns an error for expressions that cannot be understood, so the caller
/// can surface a clear message rather than silently returning everything.
pub fn parse_time_range(
    expr: &str,
    now: NaiveDateTime,
) -> anyhow::Result<(NaiveDateTime, NaiveDateTime)> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        anyhow::bail!("empty time expression");
    }

    // Trailing rolling windows: two_timer does not accept "last N days".
    if let Some(caps) = TRAILING_WINDOW.captures(trimmed) {
        let n: i64 = caps[1]
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid quantity in '{expr}'"))?;
        let unit = caps[2].to_ascii_lowercase();
        let start = match unit.as_str() {
            "day" => now - Duration::days(n),
            "week" => now - Duration::weeks(n),
            "month" => now
                .checked_sub_months(Months::new(n as u32))
                .ok_or_else(|| anyhow::anyhow!("time expression '{expr}' is out of range"))?,
            "year" => now
                .checked_sub_months(Months::new((n as u32).saturating_mul(12)))
                .ok_or_else(|| anyhow::anyhow!("time expression '{expr}' is out of range"))?,
            _ => unreachable!("regex only captures known units"),
        };
        return Ok((start, now));
    }

    // Strip a leading preposition two_timer chokes on ("in May", "during 2021").
    let normalized = strip_leading_preposition(trimmed);
    let config = Config::new().now(now);
    match two_timer_parse(normalized, Some(config)) {
        Ok((start, end, _)) => Ok((start, end)),
        Err(_) => anyhow::bail!("could not parse '{expr}' as a time expression"),
    }
}

/// Remove a leading `in`/`during`/`on` so the remainder parses on its own.
///
/// Only a small, unambiguous set is stripped; expressions like `from May to
/// June` are left untouched because two_timer handles them directly.
fn strip_leading_preposition(expr: &str) -> &str {
    for prefix in ["in ", "during ", "on "] {
        if expr.len() > prefix.len() && expr[..prefix.len()].eq_ignore_ascii_case(prefix) {
            return expr[prefix.len()..].trim_start();
        }
    }
    expr
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn now() -> NaiveDateTime {
        // Wednesday, 2026-07-08 12:00:00.
        NaiveDate::from_ymd_opt(2026, 7, 8)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
    }

    fn dt(y: i32, m: u32, d: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
    }

    #[test]
    fn parses_yesterday() {
        let (start, end) = parse_time_range("yesterday", now()).unwrap();
        assert_eq!(start, dt(2026, 7, 7));
        assert_eq!(end, dt(2026, 7, 8));
    }

    #[test]
    fn parses_last_week() {
        let (start, end) = parse_time_range("last week", now()).unwrap();
        // ISO week: previous Monday..this Monday.
        assert_eq!(start, dt(2026, 6, 29));
        assert_eq!(end, dt(2026, 7, 6));
    }

    #[test]
    fn parses_this_month() {
        let (start, end) = parse_time_range("this month", now()).unwrap();
        assert_eq!(start, dt(2026, 7, 1));
        assert_eq!(end, dt(2026, 8, 1));
    }

    #[test]
    fn parses_in_may_with_leading_preposition() {
        let (start, end) = parse_time_range("in May", now()).unwrap();
        assert_eq!(start, dt(2026, 5, 1));
        assert_eq!(end, dt(2026, 6, 1));
    }

    #[test]
    fn parses_explicit_year() {
        let (start, end) = parse_time_range("2021", now()).unwrap();
        assert_eq!(start, dt(2021, 1, 1));
        assert_eq!(end, dt(2022, 1, 1));
    }

    #[test]
    fn parses_month_and_year() {
        let (start, end) = parse_time_range("May 2021", now()).unwrap();
        assert_eq!(start, dt(2021, 5, 1));
        assert_eq!(end, dt(2021, 6, 1));
    }

    #[test]
    fn parses_trailing_rolling_window_days() {
        let (start, end) = parse_time_range("last 3 days", now()).unwrap();
        assert_eq!(end, now());
        assert_eq!(start, now() - Duration::days(3));
    }

    #[test]
    fn parses_trailing_rolling_window_months() {
        let (start, end) = parse_time_range("past 2 months", now()).unwrap();
        assert_eq!(end, now());
        // 2026-07-08 12:00 minus two calendar months.
        assert_eq!(
            start,
            NaiveDate::from_ymd_opt(2026, 5, 8)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap()
        );
    }

    #[test]
    fn rejects_gibberish() {
        assert!(parse_time_range("not a time at all zzz", now()).is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!(parse_time_range("   ", now()).is_err());
    }
}
