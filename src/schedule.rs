//! Cron expression parsing and matching.
//!
//! Supports the classic 5-field format:
//!
//! ```text
//! ┌───────────── minute       (0-59)
//! │ ┌───────────── hour       (0-23)
//! │ │ ┌───────────── day of month (1-31)
//! │ │ │ ┌───────────── month    (1-12)
//! │ │ │ │ ┌───────────── day of week (0-6, Sunday = 0; 7 also = Sunday)
//! │ │ │ │ │
//! * * * * *
//! ```
//!
//! Each field accepts `*`, a single number, a range `a-b`, a step `*/n`
//! or `a-b/n`, and comma-separated lists of any of these. A handful of
//! shorthand macros (`@daily`, `@hourly`, ...) are also recognized.

use chrono::{DateTime, Datelike, Local, Timelike};

/// A parsed cron schedule. Each field holds the set of values it matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    minutes: Vec<u32>,
    hours: Vec<u32>,
    days_of_month: Vec<u32>,
    months: Vec<u32>,
    days_of_week: Vec<u32>,
    /// Whether the day-of-month field was restricted (not `*`).
    dom_restricted: bool,
    /// Whether the day-of-week field was restricted (not `*`).
    dow_restricted: bool,
}

impl Schedule {
    /// Parse a cron expression such as `"*/5 9-17 * * 1-5"` or `"@daily"`.
    pub fn parse(expr: &str) -> Result<Schedule, String> {
        let expr = expr.trim();
        if let Some(expanded) = expand_macro(expr) {
            return Schedule::parse(expanded);
        }

        let fields: Vec<&str> = expr.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(format!(
                "expected 5 fields (minute hour day-of-month month day-of-week), got {}",
                fields.len()
            ));
        }

        let minutes = parse_field(fields[0], 0, 59)?;
        let hours = parse_field(fields[1], 0, 23)?;
        let days_of_month = parse_field(fields[2], 1, 31)?;
        let months = parse_field(fields[3], 1, 12)?;
        // Day-of-week accepts 0-7; normalize 7 -> 0 (both mean Sunday).
        let mut days_of_week = parse_field(fields[4], 0, 7)?;
        for d in days_of_week.iter_mut() {
            if *d == 7 {
                *d = 0;
            }
        }
        days_of_week.sort_unstable();
        days_of_week.dedup();

        Ok(Schedule {
            minutes,
            hours,
            days_of_month,
            months,
            days_of_week,
            dom_restricted: fields[2] != "*",
            dow_restricted: fields[4] != "*",
        })
    }

    /// Returns true if the schedule should fire at `when` (to the minute).
    pub fn matches(&self, when: &DateTime<Local>) -> bool {
        if !self.minutes.contains(&when.minute()) {
            return false;
        }
        if !self.hours.contains(&when.hour()) {
            return false;
        }
        if !self.months.contains(&when.month()) {
            return false;
        }

        let dom_match = self.days_of_month.contains(&when.day());
        // chrono: Sunday = 6 via weekday().num_days_from_monday(); use
        // num_days_from_sunday() so Sunday = 0 like cron.
        let dow = when.weekday().num_days_from_sunday();
        let dow_match = self.days_of_week.contains(&dow);

        // Classic cron semantics: when both day fields are restricted, the
        // job fires if EITHER matches. Otherwise honor the restricted one.
        match (self.dom_restricted, self.dow_restricted) {
            (true, true) => dom_match || dow_match,
            (true, false) => dom_match,
            (false, true) => dow_match,
            (false, false) => true,
        }
    }
}

/// Expand `@`-style macros into their 5-field equivalents.
fn expand_macro(expr: &str) -> Option<&'static str> {
    match expr {
        "@yearly" | "@annually" => Some("0 0 1 1 *"),
        "@monthly" => Some("0 0 1 * *"),
        "@weekly" => Some("0 0 * * 0"),
        "@daily" | "@midnight" => Some("0 0 * * *"),
        "@hourly" => Some("0 * * * *"),
        _ => None,
    }
}

/// Parse a single cron field into the sorted, deduplicated set of values it
/// matches, validating that everything stays within `[min, max]`.
fn parse_field(field: &str, min: u32, max: u32) -> Result<Vec<u32>, String> {
    let mut values = Vec::new();

    for part in field.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(format!("empty term in field `{field}`"));
        }

        // Split off an optional `/step`.
        let (range_part, step) = match part.split_once('/') {
            Some((r, s)) => {
                let step: u32 = s
                    .parse()
                    .map_err(|_| format!("invalid step `{s}` in field `{field}`"))?;
                if step == 0 {
                    return Err(format!("step must be > 0 in field `{field}`"));
                }
                (r, step)
            }
            None => (part, 1),
        };

        // Resolve the base range the step applies over.
        let (start, end) = if range_part == "*" {
            (min, max)
        } else if let Some((a, b)) = range_part.split_once('-') {
            let a = parse_num(a, field)?;
            let b = parse_num(b, field)?;
            if a > b {
                return Err(format!(
                    "range start > end in `{range_part}` of field `{field}`"
                ));
            }
            (a, b)
        } else {
            let n = parse_num(range_part, field)?;
            (n, n)
        };

        if start < min || end > max {
            return Err(format!(
                "value out of range {min}-{max} in `{part}` of field `{field}`"
            ));
        }

        let mut v = start;
        while v <= end {
            values.push(v);
            v += step;
        }
    }

    values.sort_unstable();
    values.dedup();
    Ok(values)
}

fn parse_num(s: &str, field: &str) -> Result<u32, String> {
    s.trim()
        .parse()
        .map_err(|_| format!("invalid number `{s}` in field `{field}`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn every_minute() {
        let s = Schedule::parse("* * * * *").unwrap();
        assert!(s.matches(&at(2026, 5, 30, 12, 34)));
    }

    #[test]
    fn step_minutes() {
        let s = Schedule::parse("*/15 * * * *").unwrap();
        assert!(s.matches(&at(2026, 5, 30, 12, 0)));
        assert!(s.matches(&at(2026, 5, 30, 12, 15)));
        assert!(!s.matches(&at(2026, 5, 30, 12, 7)));
    }

    #[test]
    fn range_and_list() {
        let s = Schedule::parse("0 9-17 * * 1,3,5").unwrap();
        // 2026-05-29 is a Friday (dow 5) at 10:00.
        assert!(s.matches(&at(2026, 5, 29, 10, 0)));
        // 10:30 -> minute not 0.
        assert!(!s.matches(&at(2026, 5, 29, 10, 30)));
    }

    #[test]
    fn macros_expand() {
        assert_eq!(
            Schedule::parse("@daily").unwrap(),
            Schedule::parse("0 0 * * *").unwrap()
        );
        assert_eq!(
            Schedule::parse("@hourly").unwrap(),
            Schedule::parse("0 * * * *").unwrap()
        );
    }

    #[test]
    fn dom_or_dow_when_both_restricted() {
        // Fires on the 1st OR on Mondays.
        let s = Schedule::parse("0 0 1 * 1").unwrap();
        // 2026-06-01 is a Monday — matches both, fine.
        assert!(s.matches(&at(2026, 6, 1, 0, 0)));
        // 2026-06-08 is a Monday but not the 1st — still matches (dow).
        assert!(s.matches(&at(2026, 6, 8, 0, 0)));
        // 2026-07-01 is a Wednesday, not Monday — matches via dom.
        assert!(s.matches(&at(2026, 7, 1, 0, 0)));
        // 2026-06-09 is a Tuesday, not the 1st — no match.
        assert!(!s.matches(&at(2026, 6, 9, 0, 0)));
    }

    #[test]
    fn sunday_is_zero_and_seven() {
        let zero = Schedule::parse("0 0 * * 0").unwrap();
        let seven = Schedule::parse("0 0 * * 7").unwrap();
        assert_eq!(zero, seven);
        // 2026-05-31 is a Sunday.
        assert!(zero.matches(&at(2026, 5, 31, 0, 0)));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Schedule::parse("* * * *").is_err()); // too few fields
        assert!(Schedule::parse("60 * * * *").is_err()); // minute out of range
        assert!(Schedule::parse("* 24 * * *").is_err()); // hour out of range
        assert!(Schedule::parse("*/0 * * * *").is_err()); // zero step
        assert!(Schedule::parse("5-1 * * * *").is_err()); // reversed range
    }
}
