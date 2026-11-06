use chrono::{DateTime, Utc};

use crate::zone::LocalZone;

/// Parse an RFC 3339 timestamp into UTC. Panics on bad input (test helper).
pub fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .expect("valid RFC 3339 timestamp")
        .with_timezone(&Utc)
}

/// Build a fixed IANA zone for tests. Panics on unknown names.
pub fn tz(name: &str) -> Box<dyn LocalZone> {
    Box::new(name.parse::<chrono_tz::Tz>().expect("valid IANA zone name"))
}
