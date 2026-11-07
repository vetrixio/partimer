use chrono::{DateTime, Duration, Utc};

use crate::model::{Data, Entry, Id, new_id};

/// Form values for editing or manually adding an entry (already converted to UTC).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryPatch {
    pub project_id: Id,
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,
    pub still_running: bool,
    pub paused_minutes: i64,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("找不到這筆紀錄")]
    NotFound,
    #[error("找不到這個專案")]
    UnknownProject,
    #[error("結束時間必須晚於開始時間")]
    EndNotAfterStart,
    #[error("暫停分鐘不能是負的")]
    NegativePause,
    #[error("暫停分鐘不能比整段時間長")]
    PauseExceedsSpan,
    #[error("已結束的紀錄不能改回計時中")]
    CannotReopen,
    #[error("請填結束時間")]
    EndRequired,
    #[error("仍在計時時不能填結束時間")]
    EndWhileRunning,
    #[error("開始時間不能晚於現在")]
    StartInFuture,
    #[error("結束時間不能晚於現在")]
    EndInFuture,
    #[error("已經有一筆正在計時")]
    AnotherRunning,
}

/// Checks `0 <= paused_minutes * 60 <= span` and returns the pause in seconds.
fn check_pause(
    paused_minutes: i64,
    start: DateTime<Utc>,
    span_end: DateTime<Utc>,
) -> Result<i64, EditError> {
    if paused_minutes < 0 {
        return Err(EditError::NegativePause);
    }
    let paused = paused_minutes
        .checked_mul(60)
        .ok_or(EditError::PauseExceedsSpan)?;
    if paused > (span_end - start).num_seconds() {
        return Err(EditError::PauseExceedsSpan);
    }
    Ok(paused)
}

/// How far past `now` a finished entry's start or end may be: absorbs clock skew between
/// the form and the save, and typing the current minute.
const FUTURE_TOLERANCE_SECS: i64 = 60;

/// Validates a patch that finishes an entry; returns (end, paused_seconds).
fn check_finished(
    patch: &EntryPatch,
    now: DateTime<Utc>,
) -> Result<(DateTime<Utc>, i64), EditError> {
    let end = patch.end.ok_or(EditError::EndRequired)?;
    let latest = now + Duration::seconds(FUTURE_TOLERANCE_SECS);
    if patch.start > latest {
        return Err(EditError::StartInFuture);
    }
    if end > latest {
        return Err(EditError::EndInFuture);
    }
    if end <= patch.start {
        return Err(EditError::EndNotAfterStart);
    }
    let paused = check_pause(patch.paused_minutes, patch.start, end)?;
    Ok((end, paused))
}

/// Applies `patch` to the entry `id`. Nothing is modified when an error is returned.
pub fn apply_edit(
    data: &mut Data,
    id: &str,
    patch: EntryPatch,
    now: DateTime<Utc>,
) -> Result<(), EditError> {
    let idx = data
        .entries
        .iter()
        .position(|e| e.id == id)
        .ok_or(EditError::NotFound)?;
    if !data.projects.iter().any(|p| p.id == patch.project_id) {
        return Err(EditError::UnknownProject);
    }
    let was_running = data.entries[idx].end.is_none();
    if !was_running && patch.still_running {
        return Err(EditError::CannotReopen);
    }

    // Some(end) finishes the entry; None keeps it running.
    let (end, paused_seconds) = if patch.still_running {
        if patch.end.is_some() {
            return Err(EditError::EndWhileRunning);
        }
        // No tolerance here: a running entry starting after `now` would have no span yet.
        if patch.start > now {
            return Err(EditError::StartInFuture);
        }
        let span_end = data.entries[idx].pause_started_at.unwrap_or(now);
        (
            None,
            check_pause(patch.paused_minutes, patch.start, span_end)?,
        )
    } else {
        let (end, paused) = check_finished(&patch, now)?;
        (Some(end), paused)
    };

    let entry = &mut data.entries[idx];
    entry.project_id = patch.project_id;
    entry.start = patch.start;
    entry.end = end;
    entry.paused_seconds = paused_seconds;
    if end.is_some() {
        entry.pause_started_at = None;
    }
    entry.title = patch.title;
    entry.content = patch.content;
    Ok(())
}

/// Adds a finished entry by hand; never creates a running entry.
pub fn add_entry(data: &mut Data, patch: EntryPatch, now: DateTime<Utc>) -> Result<Id, EditError> {
    if patch.still_running {
        return Err(EditError::EndRequired);
    }
    if !data.projects.iter().any(|p| p.id == patch.project_id) {
        return Err(EditError::UnknownProject);
    }
    let (end, paused_seconds) = check_finished(&patch, now)?;
    let id = new_id();
    data.entries.push(Entry {
        extra: Default::default(),
        id: id.clone(),
        project_id: patch.project_id,
        start: patch.start,
        end: Some(end),
        title: patch.title,
        content: patch.content,
        paused_seconds,
        pause_started_at: None,
    });
    Ok(id)
}

pub fn delete_entry(data: &mut Data, id: &str) -> Result<Entry, EditError> {
    let idx = data
        .entries
        .iter()
        .position(|e| e.id == id)
        .ok_or(EditError::NotFound)?;
    Ok(data.entries.remove(idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Client, Project};
    use crate::testutil::utc;

    fn data() -> Data {
        let mut d = Data::default();
        d.clients.push(Client {
            extra: Default::default(),
            id: "c1".into(),
            name: "客戶".into(),
            archived: false,
        });
        for (id, archived) in [("p1", false), ("p2", true)] {
            d.projects.push(Project {
                extra: Default::default(),
                id: id.into(),
                client_id: "c1".into(),
                name: id.into(),
                color: 0,
                archived,
            });
        }
        d
    }

    fn now() -> DateTime<Utc> {
        utc("2026-11-06T12:00:00Z")
    }

    fn entry(id: &str, start: &str, end: Option<&str>) -> Entry {
        Entry {
            extra: Default::default(),
            id: id.into(),
            project_id: "p1".into(),
            start: utc(start),
            end: end.map(utc),
            title: "舊".into(),
            content: "舊內容".into(),
            paused_seconds: 0,
            pause_started_at: None,
        }
    }

    fn patch(start: &str, end: Option<&str>) -> EntryPatch {
        EntryPatch {
            project_id: "p1".into(),
            start: utc(start),
            end: end.map(utc),
            still_running: false,
            paused_minutes: 0,
            title: "新".into(),
            content: "新內容".into(),
        }
    }

    fn finished_data() -> Data {
        let mut d = data();
        d.entries.push(entry(
            "e1",
            "2026-11-06T01:00:00Z",
            Some("2026-11-06T02:00:00Z"),
        ));
        d
    }

    fn running_data() -> Data {
        let mut d = data();
        d.entries.push(entry("r1", "2026-11-06T10:00:00Z", None));
        d
    }

    #[test]
    fn edit_finished_entry_ok() {
        let mut d = finished_data();
        let mut p = patch("2026-11-06T03:00:00Z", Some("2026-11-06T05:00:00Z"));
        p.project_id = "p2".into();
        p.paused_minutes = 30;
        apply_edit(&mut d, "e1", p, now()).unwrap();
        let e = &d.entries[0];
        assert_eq!(e.project_id, "p2");
        assert_eq!(e.start, utc("2026-11-06T03:00:00Z"));
        assert_eq!(e.end, Some(utc("2026-11-06T05:00:00Z")));
        assert_eq!(e.paused_seconds, 1800);
        assert_eq!(e.title, "新");
        assert_eq!(e.content, "新內容");
    }

    #[test]
    fn edit_rejects_end_before_start() {
        let mut d = finished_data();
        let before = d.clone();
        for end in ["2026-11-06T03:00:00Z", "2026-11-06T04:00:00Z"] {
            let r = apply_edit(
                &mut d,
                "e1",
                patch("2026-11-06T04:00:00Z", Some(end)),
                now(),
            );
            assert_eq!(r, Err(EditError::EndNotAfterStart));
            assert_eq!(d, before);
        }
    }

    #[test]
    fn edit_rejects_negative_pause() {
        let mut d = finished_data();
        let before = d.clone();
        let mut p = patch("2026-11-06T01:00:00Z", Some("2026-11-06T02:00:00Z"));
        p.paused_minutes = -1;
        assert_eq!(
            apply_edit(&mut d, "e1", p, now()),
            Err(EditError::NegativePause)
        );
        assert_eq!(d, before);
    }

    #[test]
    fn edit_rejects_pause_longer_than_span() {
        let mut d = finished_data();
        let before = d.clone();
        let mut p = patch("2026-11-06T01:00:00Z", Some("2026-11-06T02:00:00Z"));
        p.paused_minutes = 61;
        assert_eq!(
            apply_edit(&mut d, "e1", p, now()),
            Err(EditError::PauseExceedsSpan)
        );
        assert_eq!(d, before);
        let mut p = patch("2026-11-06T01:00:00Z", Some("2026-11-06T02:00:00Z"));
        p.paused_minutes = 60;
        apply_edit(&mut d, "e1", p, now()).unwrap();
    }

    #[test]
    fn cannot_reopen_finished() {
        let mut d = finished_data();
        let before = d.clone();
        let mut p = patch("2026-11-06T01:00:00Z", None);
        p.still_running = true;
        assert_eq!(
            apply_edit(&mut d, "e1", p, now()),
            Err(EditError::CannotReopen)
        );
        assert_eq!(d, before);
    }

    #[test]
    fn turning_off_still_running_finishes_entry() {
        let mut d = running_data();
        let mut p = patch("2026-11-06T09:00:00Z", Some("2026-11-06T11:00:00Z"));
        p.paused_minutes = 10;
        apply_edit(&mut d, "r1", p, now()).unwrap();
        assert_eq!(d.entries[0].end, Some(utc("2026-11-06T11:00:00Z")));
        assert_eq!(d.entries[0].paused_seconds, 600);
        assert!(crate::timer::running(&d).is_none());
    }

    #[test]
    fn turning_off_while_paused_clears_pause_start() {
        let mut d = running_data();
        d.entries[0].pause_started_at = Some(utc("2026-11-06T11:00:00Z"));
        let p = patch("2026-11-06T10:00:00Z", Some("2026-11-06T11:30:00Z"));
        apply_edit(&mut d, "r1", p, now()).unwrap();
        assert_eq!(d.entries[0].pause_started_at, None);
        assert!(d.entries[0].end.is_some());
    }

    #[test]
    fn end_given_while_running_rejected() {
        let mut d = running_data();
        let before = d.clone();
        let mut p = patch("2026-11-06T10:00:00Z", Some("2026-11-06T11:00:00Z"));
        p.still_running = true;
        assert_eq!(
            apply_edit(&mut d, "r1", p, now()),
            Err(EditError::EndWhileRunning)
        );
        assert_eq!(d, before);
    }

    #[test]
    fn running_kept_running_updates_fields() {
        let mut d = running_data();
        let mut p = patch("2026-11-06T09:00:00Z", None);
        p.still_running = true;
        p.paused_minutes = 20;
        apply_edit(&mut d, "r1", p, now()).unwrap();
        let e = &d.entries[0];
        assert_eq!(e.end, None);
        assert_eq!(e.start, utc("2026-11-06T09:00:00Z"));
        assert_eq!(e.paused_seconds, 1200);
        assert_eq!(e.title, "新");
    }

    #[test]
    fn running_start_in_future_rejected() {
        let mut d = running_data();
        let before = d.clone();
        let mut p = patch("2026-11-06T12:00:01Z", None);
        p.still_running = true;
        assert_eq!(
            apply_edit(&mut d, "r1", p, now()),
            Err(EditError::StartInFuture)
        );
        assert_eq!(d, before);
    }

    #[test]
    fn running_pause_bounded_by_now() {
        let mut d = running_data();
        let before = d.clone();
        let mut p = patch("2026-11-06T10:00:00Z", None);
        p.still_running = true;
        p.paused_minutes = 121;
        assert_eq!(
            apply_edit(&mut d, "r1", p.clone(), now()),
            Err(EditError::PauseExceedsSpan)
        );
        assert_eq!(d, before);
        p.paused_minutes = 120;
        apply_edit(&mut d, "r1", p, now()).unwrap();
    }

    #[test]
    fn running_paused_keeps_pause_start_and_bounds_by_it() {
        let mut d = running_data();
        let ps = utc("2026-11-06T11:00:00Z");
        d.entries[0].pause_started_at = Some(ps);
        let before = d.clone();
        let mut p = patch("2026-11-06T10:00:00Z", None);
        p.still_running = true;
        p.paused_minutes = 61;
        assert_eq!(
            apply_edit(&mut d, "r1", p.clone(), now()),
            Err(EditError::PauseExceedsSpan)
        );
        assert_eq!(d, before);
        p.paused_minutes = 30;
        apply_edit(&mut d, "r1", p, now()).unwrap();
        assert_eq!(d.entries[0].pause_started_at, Some(ps));
        assert_eq!(d.entries[0].paused_seconds, 1800);
    }

    #[test]
    fn edit_requires_end_when_not_running() {
        let mut d = running_data();
        let before = d.clone();
        let p = patch("2026-11-06T10:00:00Z", None);
        assert_eq!(
            apply_edit(&mut d, "r1", p, now()),
            Err(EditError::EndRequired)
        );
        assert_eq!(d, before);
        let mut d = finished_data();
        let p = patch("2026-11-06T01:00:00Z", None);
        assert_eq!(
            apply_edit(&mut d, "e1", p, now()),
            Err(EditError::EndRequired)
        );
    }

    #[test]
    fn edit_not_found_and_unknown_project() {
        let mut d = finished_data();
        let before = d.clone();
        let p = patch("2026-11-06T01:00:00Z", Some("2026-11-06T02:00:00Z"));
        assert_eq!(
            apply_edit(&mut d, "nope", p.clone(), now()),
            Err(EditError::NotFound)
        );
        let mut p2 = p;
        p2.project_id = "zzz".into();
        assert_eq!(
            apply_edit(&mut d, "e1", p2, now()),
            Err(EditError::UnknownProject)
        );
        assert_eq!(d, before);
    }

    #[test]
    fn add_entry_ok_assigns_id_and_allows_archived_project() {
        let mut d = data();
        let mut p = patch("2026-11-06T01:00:00Z", Some("2026-11-06T02:00:00Z"));
        p.project_id = "p2".into();
        p.paused_minutes = 5;
        let id = add_entry(&mut d, p, now()).unwrap();
        assert_eq!(d.entries.len(), 1);
        let e = &d.entries[0];
        assert_eq!(e.id, id);
        assert!(!id.is_empty());
        assert_eq!(e.paused_seconds, 300);
        assert_eq!(e.pause_started_at, None);
        assert!(e.end.is_some());
    }

    #[test]
    fn add_entry_requires_end() {
        let mut d = data();
        let mut p = patch("2026-11-06T01:00:00Z", None);
        assert_eq!(
            add_entry(&mut d, p.clone(), now()),
            Err(EditError::EndRequired)
        );
        p.still_running = true;
        p.end = Some(utc("2026-11-06T02:00:00Z"));
        assert_eq!(add_entry(&mut d, p, now()), Err(EditError::EndRequired));
        assert!(d.entries.is_empty());
    }

    #[test]
    fn add_entry_unknown_project() {
        let mut d = data();
        let mut p = patch("2026-11-06T01:00:00Z", Some("2026-11-06T02:00:00Z"));
        p.project_id = "zzz".into();
        assert_eq!(add_entry(&mut d, p, now()), Err(EditError::UnknownProject));
        assert!(d.entries.is_empty());
    }

    #[test]
    fn add_entry_validates_span() {
        let mut d = data();
        let p = patch("2026-11-06T02:00:00Z", Some("2026-11-06T01:00:00Z"));
        assert_eq!(
            add_entry(&mut d, p, now()),
            Err(EditError::EndNotAfterStart)
        );
        let mut p = patch("2026-11-06T01:00:00Z", Some("2026-11-06T02:00:00Z"));
        p.paused_minutes = 61;
        assert_eq!(
            add_entry(&mut d, p, now()),
            Err(EditError::PauseExceedsSpan)
        );
        assert!(d.entries.is_empty());
    }

    #[test]
    fn add_entry_allows_overlap_with_running() {
        let mut d = running_data();
        let p = patch("2026-11-06T10:30:00Z", Some("2026-11-06T11:00:00Z"));
        add_entry(&mut d, p, now()).unwrap();
        assert_eq!(d.entries.iter().filter(|e| e.end.is_none()).count(), 1);
    }

    #[test]
    fn add_entry_rejects_times_in_future() {
        let mut d = data();
        // now = 12:00:00; up to 60 s ahead is tolerated (clock skew, typed current minute).
        let p = patch("2026-11-06T11:00:00Z", Some("2026-11-06T12:01:00Z"));
        add_entry(&mut d, p, now()).unwrap();
        let p = patch("2026-11-06T11:00:00Z", Some("2026-11-06T12:01:01Z"));
        assert_eq!(add_entry(&mut d, p, now()), Err(EditError::EndInFuture));
        let p = patch("2026-11-06T12:01:00Z", Some("2026-11-06T12:01:00Z"));
        assert_eq!(
            add_entry(&mut d, p, now()),
            Err(EditError::EndNotAfterStart)
        );
        let p = patch("2026-11-06T12:01:01Z", Some("2026-11-06T13:00:00Z"));
        assert_eq!(add_entry(&mut d, p, now()), Err(EditError::StartInFuture));
        assert_eq!(d.entries.len(), 1);
    }

    #[test]
    fn edit_rejects_times_in_future() {
        let mut d = finished_data();
        let before = d.clone();
        let p = patch("2026-11-06T01:00:00Z", Some("2026-11-06T12:01:01Z"));
        assert_eq!(
            apply_edit(&mut d, "e1", p, now()),
            Err(EditError::EndInFuture)
        );
        let p = patch("2026-11-06T12:01:01Z", Some("2026-11-06T14:00:00Z"));
        assert_eq!(
            apply_edit(&mut d, "e1", p, now()),
            Err(EditError::StartInFuture)
        );
        assert_eq!(d, before);
        let p = patch("2026-11-06T01:00:00Z", Some("2026-11-06T12:01:00Z"));
        apply_edit(&mut d, "e1", p, now()).unwrap();
        assert_eq!(d.entries[0].end, Some(utc("2026-11-06T12:01:00Z")));
    }

    #[test]
    fn finishing_running_entry_rejects_end_in_future() {
        let mut d = running_data();
        let before = d.clone();
        let p = patch("2026-11-06T10:00:00Z", Some("2026-11-06T12:01:01Z"));
        assert_eq!(
            apply_edit(&mut d, "r1", p, now()),
            Err(EditError::EndInFuture)
        );
        assert_eq!(d, before);
    }

    #[test]
    fn future_time_messages_name_the_field() {
        assert_eq!(EditError::StartInFuture.to_string(), "開始時間不能晚於現在");
        assert_eq!(EditError::EndInFuture.to_string(), "結束時間不能晚於現在");
    }

    #[test]
    fn delete_returns_entry() {
        let mut d = finished_data();
        let e = delete_entry(&mut d, "e1").unwrap();
        assert_eq!(e.id, "e1");
        assert!(d.entries.is_empty());
        assert_eq!(delete_entry(&mut d, "e1"), Err(EditError::NotFound));
    }

    #[test]
    fn error_messages_are_chinese() {
        assert_eq!(
            EditError::EndNotAfterStart.to_string(),
            "結束時間必須晚於開始時間"
        );
        assert_eq!(EditError::AnotherRunning.to_string(), "已經有一筆正在計時");
    }
}
