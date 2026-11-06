use chrono::{DateTime, Utc};

use crate::model::Entry;

/// Worked seconds: end point (end, else pause start, else now) minus start minus paused total, floored at 0.
pub fn effective_seconds(e: &Entry, now: DateTime<Utc>) -> i64 {
    let end_point = e.end.or(e.pause_started_at).unwrap_or(now);
    ((end_point - e.start).num_seconds() - e.paused_seconds).max(0)
}

/// Round seconds to the nearest minute; negatives count as 0.
pub fn round_minutes(secs: i64) -> i64 {
    (secs.max(0) + 30).div_euclid(60)
}

/// Minutes as decimal hours with two digits, using integer math only.
pub fn hours_decimal(minutes: i64) -> String {
    let h100 = (minutes.max(0) * 100 + 30) / 60;
    format!("{}.{:02}", h100 / 100, h100 % 100)
}

pub fn format_hm(minutes: i64) -> String {
    let m = minutes.max(0);
    if m < 60 {
        format!("{m}m")
    } else {
        format!("{}h {}m", m / 60, m % 60)
    }
}

pub fn format_hms(secs: i64) -> String {
    let s = secs.max(0);
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::utc;

    fn entry(start: &str, end: Option<&str>, paused: i64, pause_at: Option<&str>) -> Entry {
        Entry {
            id: "e".into(),
            project_id: "p".into(),
            start: utc(start),
            end: end.map(utc),
            title: String::new(),
            content: String::new(),
            paused_seconds: paused,
            pause_started_at: pause_at.map(utc),
        }
    }

    #[test]
    fn finished_entry_subtracts_pause() {
        let e = entry(
            "2026-10-06T01:00:00Z",
            Some("2026-10-06T03:00:00Z"),
            1200,
            None,
        );
        assert_eq!(effective_seconds(&e, utc("2026-10-06T09:00:00Z")), 6000);
    }

    #[test]
    fn running_entry_uses_now() {
        let e = entry("2026-10-06T01:00:00Z", None, 0, None);
        assert_eq!(effective_seconds(&e, utc("2026-10-06T01:10:00Z")), 600);
    }

    #[test]
    fn paused_entry_freezes_at_pause_start() {
        let e = entry(
            "2026-10-06T01:00:00Z",
            None,
            0,
            Some("2026-10-06T01:05:00Z"),
        );
        assert_eq!(effective_seconds(&e, utc("2026-10-06T05:00:00Z")), 300);
    }

    #[test]
    fn clock_going_backwards_is_zero() {
        let e = entry("2026-10-06T01:00:00Z", None, 0, None);
        assert_eq!(effective_seconds(&e, utc("2026-10-06T00:50:00Z")), 0);
    }

    #[test]
    fn rounding() {
        assert_eq!(round_minutes(89), 1);
        assert_eq!(round_minutes(90), 2);
        assert_eq!(round_minutes(29), 0);
        assert_eq!(round_minutes(-5), 0);
    }

    #[test]
    fn decimal_hours() {
        assert_eq!(hours_decimal(90), "1.50");
        assert_eq!(hours_decimal(1), "0.02");
        assert_eq!(hours_decimal(10), "0.17");
        assert_eq!(hours_decimal(0), "0.00");
    }

    #[test]
    fn hm_format() {
        assert_eq!(format_hm(90), "1h 30m");
        assert_eq!(format_hm(45), "45m");
        assert_eq!(format_hm(120), "2h 0m");
    }

    #[test]
    fn hms_format() {
        assert_eq!(format_hms(5025), "01:23:45");
        assert_eq!(format_hms(100 * 3600 + 1), "100:00:01");
    }
}
