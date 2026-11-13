//! Validation and summary helpers for transfer files (a full data export).

use chrono::NaiveDate;
use serde_json::Value;

use crate::model::{DATA_VERSION, Data, parse_data};
use crate::zone::{LocalZone, local_date};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TransferError {
    #[error("這不是打工人計時器的轉移檔，或檔案已損壞")]
    NotTransferFile,
    #[error("這個轉移檔來自較新版的打工人計時器，請先更新 app")]
    NewerVersion,
    #[error("轉移檔裡有一筆還在計時，請在舊電腦按 STOP 後重新轉出")]
    HasRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataCounts {
    pub clients: usize,
    pub projects: usize,
    pub entries: usize,
}

/// Check the top-level shape first: `parse_data` accepts any JSON object as empty data.
pub fn parse_transfer(bytes: &[u8]) -> Result<Data, TransferError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| TransferError::NotTransferFile)?;
    let obj = value.as_object().ok_or(TransferError::NotTransferFile)?;
    let version = obj
        .get("version")
        .and_then(Value::as_u64)
        .ok_or(TransferError::NotTransferFile)?;
    for key in ["clients", "projects", "entries"] {
        if !obj.get(key).is_some_and(Value::is_array) {
            return Err(TransferError::NotTransferFile);
        }
    }
    if version > u64::from(DATA_VERSION) {
        return Err(TransferError::NewerVersion);
    }
    let data = parse_data(bytes).map_err(|_| TransferError::NotTransferFile)?;
    if data.entries.iter().any(|e| e.end.is_none()) {
        return Err(TransferError::HasRunning);
    }
    Ok(data)
}

pub fn counts(data: &Data) -> DataCounts {
    DataCounts {
        clients: data.clients.len() + data.quarantined_clients.len(),
        projects: data.projects.len() + data.quarantined_projects.len(),
        entries: data.entries.len() + data.quarantined_entries.len(),
    }
}

pub fn is_empty(data: &Data) -> bool {
    let c = counts(data);
    c.clients == 0 && c.projects == 0 && c.entries == 0
}

/// Earliest and latest local start date among `entries`.
pub fn date_range(data: &Data, zone: &dyn LocalZone) -> Option<(NaiveDate, NaiveDate)> {
    let mut dates = data.entries.iter().map(|e| local_date(zone, e.start));
    let first = dates.next()?;
    Some(dates.fold((first, first), |(lo, hi), d| (lo.min(d), hi.max(d))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Client, Entry, to_json};
    use crate::testutil::{tz, utc};
    use serde_json::json;

    fn entry(id: &str, start: &str, end: Option<&str>) -> Entry {
        Entry {
            id: id.into(),
            project_id: "p".into(),
            start: utc(start),
            end: end.map(utc),
            title: "t".into(),
            content: String::new(),
            paused_seconds: 0,
            pause_started_at: None,
            extra: Default::default(),
        }
    }

    fn sample() -> Data {
        let mut d = Data::default();
        d.clients.push(Client {
            id: "c".into(),
            name: "C".into(),
            archived: false,
            extra: json!({"x": 1}).as_object().unwrap().clone(),
        });
        d.entries.push(entry(
            "e1",
            "2026-10-05T01:00:00Z",
            Some("2026-10-05T02:00:00Z"),
        ));
        d.quarantined_projects.push(json!({"bad": true}));
        d.settings.widget.x = Some(10);
        d.settings.widget.y = Some(-20);
        d.extra.insert("future".into(), json!([1, 2]));
        d
    }

    #[test]
    fn round_trips_to_json_exactly() {
        let d = sample();
        assert_eq!(parse_transfer(&to_json(&d)).unwrap(), d);
    }

    #[test]
    fn rejects_empty_object() {
        assert_eq!(parse_transfer(b"{}"), Err(TransferError::NotTransferFile));
    }

    #[test]
    fn rejects_missing_entries_array() {
        let b = br#"{"version":1,"clients":[],"projects":[]}"#;
        assert_eq!(parse_transfer(b), Err(TransferError::NotTransferFile));
    }

    #[test]
    fn rejects_non_numeric_version() {
        let b = br#"{"version":"1","clients":[],"projects":[],"entries":[]}"#;
        assert_eq!(parse_transfer(b), Err(TransferError::NotTransferFile));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(
            parse_transfer(b"not json"),
            Err(TransferError::NotTransferFile)
        );
    }

    #[test]
    fn rejects_newer_version() {
        let b = format!(
            r#"{{"version":{},"clients":[],"projects":[],"entries":[]}}"#,
            DATA_VERSION + 1
        );
        assert_eq!(
            parse_transfer(b.as_bytes()),
            Err(TransferError::NewerVersion)
        );
    }

    #[test]
    fn rejects_running_or_paused_entry() {
        let mut d = sample();
        d.entries.push(entry("e2", "2026-10-05T03:00:00Z", None));
        assert_eq!(parse_transfer(&to_json(&d)), Err(TransferError::HasRunning));
    }

    #[test]
    fn counts_include_quarantined() {
        let mut d = Data::default();
        d.entries.push(entry(
            "a",
            "2026-10-05T01:00:00Z",
            Some("2026-10-05T02:00:00Z"),
        ));
        d.entries.push(entry(
            "b",
            "2026-10-05T03:00:00Z",
            Some("2026-10-05T04:00:00Z"),
        ));
        d.quarantined_entries.push(json!({"x": 1}));
        d.quarantined_clients.push(json!({"x": 1}));
        let c = counts(&d);
        assert_eq!((c.clients, c.projects, c.entries), (1, 0, 3));
        assert!(!is_empty(&d));
        assert!(is_empty(&Data::default()));
    }

    #[test]
    fn date_range_uses_local_dates() {
        let mut d = Data::default();
        d.entries.push(entry(
            "a",
            "2026-10-05T17:00:00Z",
            Some("2026-10-05T18:00:00Z"),
        ));
        d.entries.push(entry(
            "b",
            "2026-10-09T01:00:00Z",
            Some("2026-10-09T02:00:00Z"),
        ));
        let z = tz("Asia/Taipei");
        let ymd = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        assert_eq!(date_range(&d, z.as_ref()), Some((ymd(10, 6), ymd(10, 9))));
        assert_eq!(date_range(&Data::default(), z.as_ref()), None);
    }
}
