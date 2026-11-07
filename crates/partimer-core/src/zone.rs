use chrono::offset::LocalResult;
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, Offset, TimeZone, Utc};
use thiserror::Error;

const INPUT_FORMAT: &str = "%Y-%m-%dT%H:%M";

/// A local time zone, injected by the caller so core never reads the system clock/zone.
pub trait LocalZone: Send + Sync {
    fn to_local(&self, t: DateTime<Utc>) -> NaiveDateTime;
    #[allow(clippy::wrong_self_convention)] // signature fixed by the task interface
    fn from_local(&self, t: NaiveDateTime) -> LocalResult<DateTime<Utc>>;
    /// UTC offset at instant `t`, e.g. "+08:00".
    fn offset_label(&self, t: DateTime<Utc>) -> String;
}

fn offset_string(secs: i32) -> String {
    let sign = if secs < 0 { '-' } else { '+' };
    let a = secs.abs();
    format!("{sign}{:02}:{:02}", a / 3600, (a % 3600) / 60)
}

/// The machine's local zone. The only place allowed to touch `chrono::Local`.
pub struct SystemZone;

impl LocalZone for SystemZone {
    fn to_local(&self, t: DateTime<Utc>) -> NaiveDateTime {
        t.with_timezone(&Local).naive_local()
    }

    fn from_local(&self, t: NaiveDateTime) -> LocalResult<DateTime<Utc>> {
        Local.from_local_datetime(&t).map(|d| d.with_timezone(&Utc))
    }

    fn offset_label(&self, t: DateTime<Utc>) -> String {
        offset_string(t.with_timezone(&Local).offset().fix().local_minus_utc())
    }
}

#[cfg(test)]
impl LocalZone for chrono_tz::Tz {
    fn to_local(&self, t: DateTime<Utc>) -> NaiveDateTime {
        t.with_timezone(self).naive_local()
    }

    fn from_local(&self, t: NaiveDateTime) -> LocalResult<DateTime<Utc>> {
        self.from_local_datetime(&t).map(|d| d.with_timezone(&Utc))
    }

    fn offset_label(&self, t: DateTime<Utc>) -> String {
        offset_string(t.with_timezone(self).offset().fix().local_minus_utc())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TimeInputError {
    #[error("時間格式不正確")]
    Format,
    #[error("這個時間因夏令時間切換而不存在")]
    Nonexistent,
}

pub fn local_date(zone: &dyn LocalZone, t: DateTime<Utc>) -> NaiveDate {
    zone.to_local(t).date()
}

/// Parse `YYYY-MM-DDTHH:MM` as local time. Ambiguous (DST fall-back) picks the earlier instant.
pub fn parse_local_input(zone: &dyn LocalZone, s: &str) -> Result<DateTime<Utc>, TimeInputError> {
    let naive =
        NaiveDateTime::parse_from_str(s, INPUT_FORMAT).map_err(|_| TimeInputError::Format)?;
    match zone.from_local(naive) {
        LocalResult::Single(t) => Ok(t),
        LocalResult::Ambiguous(a, b) => Ok(a.min(b)),
        LocalResult::None => Err(TimeInputError::Nonexistent),
    }
}

pub fn format_local_input(zone: &dyn LocalZone, t: DateTime<Utc>) -> String {
    zone.to_local(t).format(INPUT_FORMAT).to_string()
}

pub fn format_hhmm(zone: &dyn LocalZone, t: DateTime<Utc>) -> String {
    zone.to_local(t).format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{tz, utc};

    #[test]
    fn local_date_across_midnight_taipei() {
        let z = tz("Asia/Taipei");
        let d = local_date(z.as_ref(), utc("2026-10-06T16:30:00Z"));
        assert_eq!(d, NaiveDate::from_ymd_opt(2026, 10, 7).unwrap());
    }

    #[test]
    fn parse_local_input_rejects_dst_gap() {
        let z = tz("America/New_York");
        assert_eq!(
            parse_local_input(z.as_ref(), "2026-03-08T02:30"),
            Err(TimeInputError::Nonexistent)
        );
    }

    #[test]
    fn parse_local_input_ambiguous_takes_earlier() {
        let z = tz("America/New_York");
        assert_eq!(
            parse_local_input(z.as_ref(), "2026-11-01T01:30"),
            Ok(utc("2026-11-01T05:30:00Z"))
        );
    }

    #[test]
    fn parse_local_input_bad_format() {
        let z = tz("Asia/Taipei");
        assert_eq!(
            parse_local_input(z.as_ref(), "2026/10/06 9:30"),
            Err(TimeInputError::Format)
        );
    }

    #[test]
    fn formats_and_offset_label() {
        let z = tz("Asia/Taipei");
        let t = utc("2026-10-06T01:30:00Z");
        assert_eq!(format_local_input(z.as_ref(), t), "2026-10-06T09:30");
        assert_eq!(format_hhmm(z.as_ref(), t), "09:30");
        assert_eq!(z.offset_label(t), "+08:00");
        assert_eq!(tz("America/New_York").offset_label(t), "-04:00");
    }

    #[test]
    fn system_zone_roundtrips() {
        // Whatever zone CI runs in: a real instant never maps into a DST gap, and in a
        // fall-back overlap it may be either of the two candidates.
        for t in [
            "2026-10-06T01:30:00Z",
            "2026-03-08T07:30:00Z",
            "2026-03-29T01:30:00Z",
            "2026-11-01T06:30:00Z",
            "2026-10-25T01:30:00Z",
        ]
        .map(utc)
        {
            let back = SystemZone.from_local(SystemZone.to_local(t));
            match back {
                LocalResult::Single(a) => assert_eq!(a, t),
                LocalResult::Ambiguous(a, b) => assert!(a == t || b == t, "{t}: {a} / {b}"),
                LocalResult::None => panic!("{t} mapped into a gap"),
            }
        }
    }

    #[test]
    fn parse_local_input_accepts_single_digit_fields_but_checks_ranges() {
        // Lenient on purpose: typed input like 9:30 is friendlier than an error.
        let z = tz("Asia/Taipei");
        assert_eq!(
            parse_local_input(z.as_ref(), "2026-10-6T9:30"),
            Ok(utc("2026-10-06T01:30:00Z"))
        );
        for bad in [
            "2026-13-06T09:30",
            "2026-10-32T09:30",
            "2026-02-30T09:30",
            "2026-10-06T24:00",
            "2026-10-06T09:60",
            "2026-10-06T09:30:00",
            "",
        ] {
            assert_eq!(
                parse_local_input(z.as_ref(), bad),
                Err(TimeInputError::Format),
                "{bad}"
            );
        }
    }

    #[test]
    fn display_messages() {
        assert_eq!(TimeInputError::Format.to_string(), "時間格式不正確");
        assert_eq!(
            TimeInputError::Nonexistent.to_string(),
            "這個時間因夏令時間切換而不存在"
        );
    }
}
