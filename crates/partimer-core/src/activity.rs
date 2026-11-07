use chrono::{DateTime, Utc};

use crate::model::Entry;
use crate::storage::{Fs, Paths};
use crate::zone::LocalZone;

/// What happened. Rendered upper-case (`AutoStop` -> `AUTO_STOP`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Pause,
    Resume,
    Stop,
    Save,
    Edit,
    Add,
    Delete,
    Restore,
    Switch,
    AutoStop,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Action::Start => "START",
            Action::Pause => "PAUSE",
            Action::Resume => "RESUME",
            Action::Stop => "STOP",
            Action::Save => "SAVE",
            Action::Edit => "EDIT",
            Action::Add => "ADD",
            Action::Delete => "DELETE",
            Action::Restore => "RESTORE",
            Action::Switch => "SWITCH",
            Action::AutoStop => "AUTO_STOP",
        }
    }
}

/// Local RFC 3339 with offset, seconds precision, e.g. `2026-11-06T14:05:00+08:00`.
fn local_rfc3339(zone: &dyn LocalZone, t: DateTime<Utc>) -> String {
    format!(
        "{}{}",
        zone.to_local(t).format("%Y-%m-%dT%H:%M:%S"),
        zone.offset_label(t)
    )
}

/// One tab-separated log line ending in `\n`. Never includes title/content text, only lengths.
pub fn format_line(
    at: DateTime<Utc>,
    zone: &dyn LocalZone,
    action: Action,
    entry: Option<&Entry>,
    note: Option<&str>,
) -> String {
    let (id, start, end, title_len, content_len) = match entry {
        Some(e) => (
            e.id.chars().take(8).collect::<String>(),
            local_rfc3339(zone, e.start),
            e.end
                .map_or_else(|| "-".to_string(), |t| local_rfc3339(zone, t)),
            e.title.chars().count(),
            e.content.chars().count(),
        ),
        None => ("-".to_string(), "-".to_string(), "-".to_string(), 0, 0),
    };
    let mut line = format!(
        "{}\t{}\t{id}\tstart={start}\tend={end}\ttitle_len={title_len}\tcontent_len={content_len}",
        local_rfc3339(zone, at),
        action.as_str(),
    );
    if let Some(n) = note {
        let clean: String = n
            .chars()
            .map(|c| {
                if matches!(c, '\t' | '\r' | '\n') {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        line.push_str("\tnote=");
        line.push_str(&clean);
    }
    line.push('\n');
    line
}

/// Append a line to the activity log, creating it if missing. Errors are ignored on purpose.
pub fn append(fs: &dyn Fs, paths: &Paths, line: &str) {
    let _ = fs.append(&paths.activity_log(), line.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{MemFs, tz, utc};
    use std::path::PathBuf;

    fn entry(title: &str, content: &str, end: Option<&str>) -> Entry {
        Entry {
            extra: Default::default(),
            id: "abcdef12-3456-7890-abcd-ef1234567890".into(),
            project_id: "p".into(),
            start: utc("2026-11-06T04:00:00Z"),
            end: end.map(utc),
            title: title.into(),
            content: content.into(),
            paused_seconds: 0,
            pause_started_at: None,
        }
    }

    #[test]
    fn line_format_matches_spec() {
        let z = tz("Asia/Taipei");
        let at = utc("2026-11-06T06:05:00Z");
        let e = entry("abc", "hello", Some("2026-11-06T06:05:00Z"));
        assert_eq!(
            format_line(
                at,
                z.as_ref(),
                Action::Stop,
                Some(&e),
                Some("restored-from=2026-10-05")
            ),
            "2026-11-06T14:05:00+08:00\tSTOP\tabcdef12\tstart=2026-11-06T12:00:00+08:00\tend=2026-11-06T14:05:00+08:00\ttitle_len=3\tcontent_len=5\tnote=restored-from=2026-10-05\n"
        );
        let running = entry("", "", None);
        assert_eq!(
            format_line(at, z.as_ref(), Action::AutoStop, Some(&running), None),
            "2026-11-06T14:05:00+08:00\tAUTO_STOP\tabcdef12\tstart=2026-11-06T12:00:00+08:00\tend=-\ttitle_len=0\tcontent_len=0\n"
        );
        assert_eq!(
            format_line(at, z.as_ref(), Action::Switch, None, None),
            "2026-11-06T14:05:00+08:00\tSWITCH\t-\tstart=-\tend=-\ttitle_len=0\tcontent_len=0\n"
        );
    }

    #[test]
    fn line_never_contains_text() {
        let z = tz("Asia/Taipei");
        let e = entry("機密客戶", "密碼123", None);
        let l = format_line(
            utc("2026-11-06T06:05:00Z"),
            z.as_ref(),
            Action::Save,
            Some(&e),
            None,
        );
        assert!(!l.contains("機密客戶") && !l.contains("密碼123"));
        assert!(l.contains("title_len=4") && l.contains("content_len=5"));
    }

    #[test]
    fn note_has_no_tab_or_newline() {
        let z = tz("Asia/Taipei");
        let l = format_line(
            utc("2026-11-06T06:05:00Z"),
            z.as_ref(),
            Action::Edit,
            None,
            Some("a\tb\r\nc"),
        );
        assert!(l.ends_with("\tnote=a b  c\n"));
        assert_eq!(l.matches('\n').count(), 1);
    }

    #[test]
    fn append_creates_and_appends() {
        let fs = MemFs::new();
        let paths = Paths {
            dir: PathBuf::from("/d"),
        };
        append(&fs, &paths, "a\n");
        append(&fs, &paths, "b\n");
        assert_eq!(fs.get(&paths.activity_log()).unwrap(), b"a\nb\n");
    }

    #[test]
    fn append_failure_is_silent() {
        let fs = MemFs::new();
        let paths = Paths {
            dir: PathBuf::from("/d"),
        };
        // Appending really writes (so a no-op would fail this test)…
        append(&fs, &paths, "a\n");
        assert_eq!(fs.get(&paths.activity_log()).unwrap(), b"a\n");
        // …and a failing append returns quietly without touching the log.
        fs.fail_appends.set(true);
        append(&fs, &paths, "b\n");
        assert_eq!(fs.get(&paths.activity_log()).unwrap(), b"a\n");
        fs.fail_appends.set(false);
        append(&fs, &paths, "c\n");
        assert_eq!(fs.get(&paths.activity_log()).unwrap(), b"a\nc\n");
    }
}
