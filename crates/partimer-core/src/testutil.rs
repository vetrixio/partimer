use chrono::{DateTime, Utc};

#[allow(dead_code)] // used by tests added in later tasks
/// Parse an RFC 3339 timestamp into UTC. Panics on bad input (test helper).
pub fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .expect("valid RFC 3339 timestamp")
        .with_timezone(&Utc)
}
