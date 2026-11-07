use chrono::{DateTime, Utc};

use crate::model::{Data, Entry, Id, new_id};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TimerError {
    #[error("已經有一筆正在計時")]
    AlreadyRunning,
    #[error("目前沒有在計時")]
    NotRunning,
    #[error("目前沒有暫停")]
    NotPaused,
    #[error("已經在暫停中")]
    AlreadyPaused,
    #[error("找不到這個專案")]
    UnknownProject,
    #[error("結束時間必須晚於開始時間")]
    EndNotAfterStart,
    #[error("暫停時間不能比整段時間長")]
    PauseExceedsSpan,
    #[error("結束時間不能晚於現在")]
    EndInFuture,
}

/// The unfinished entry, if any.
pub fn running(data: &Data) -> Option<&Entry> {
    data.entries.iter().find(|e| e.end.is_none())
}

pub fn running_mut(data: &mut Data) -> Option<&mut Entry> {
    data.entries.iter_mut().find(|e| e.end.is_none())
}

pub fn start(
    data: &mut Data,
    project_id: &str,
    title: &str,
    content: &str,
    now: DateTime<Utc>,
) -> Result<Id, TimerError> {
    if running(data).is_some() {
        return Err(TimerError::AlreadyRunning);
    }
    if !data.projects.iter().any(|p| p.id == project_id) {
        return Err(TimerError::UnknownProject);
    }
    let id = new_id();
    data.entries.push(Entry {
        extra: Default::default(),
        id: id.clone(),
        project_id: project_id.to_string(),
        start: now,
        end: None,
        title: title.to_string(),
        content: content.to_string(),
        paused_seconds: 0,
        pause_started_at: None,
    });
    data.settings.last_project_id = Some(project_id.to_string());
    Ok(id)
}

pub fn pause(data: &mut Data, now: DateTime<Utc>) -> Result<Id, TimerError> {
    let e = running_mut(data).ok_or(TimerError::NotRunning)?;
    if e.pause_started_at.is_some() {
        return Err(TimerError::AlreadyPaused);
    }
    e.pause_started_at = Some(now);
    Ok(e.id.clone())
}

pub fn resume(data: &mut Data, now: DateTime<Utc>) -> Result<Id, TimerError> {
    let e = running_mut(data).ok_or(TimerError::NotRunning)?;
    let since = e.pause_started_at.ok_or(TimerError::NotPaused)?;
    e.paused_seconds += (now - since).num_seconds().max(0);
    e.pause_started_at = None;
    Ok(e.id.clone())
}

/// Finish the running entry. While paused it ends at the pause start and that pause is not counted.
/// Never fails on odd times (pause start or clock before the start): the end is clamped to the
/// start and the pause total to the span, so the entry is never negative.
pub fn stop(
    data: &mut Data,
    title: &str,
    content: &str,
    now: DateTime<Utc>,
) -> Result<Id, TimerError> {
    let e = running_mut(data).ok_or(TimerError::NotRunning)?;
    let end = e.pause_started_at.take().unwrap_or(now).max(e.start);
    e.end = Some(end);
    e.paused_seconds = e.paused_seconds.clamp(0, (end - e.start).num_seconds());
    e.title = title.to_string();
    e.content = content.to_string();
    Ok(e.id.clone())
}

/// Finish the running entry at a chosen time (12h reminder), which may not be after `now`.
/// Leaves data untouched on error.
pub fn stop_at(data: &mut Data, end: DateTime<Utc>, now: DateTime<Utc>) -> Result<Id, TimerError> {
    let e = running_mut(data).ok_or(TimerError::NotRunning)?;
    if end > now {
        return Err(TimerError::EndInFuture);
    }
    if end <= e.start {
        return Err(TimerError::EndNotAfterStart);
    }
    let mut paused = e.paused_seconds;
    if let Some(since) = e.pause_started_at
        && end > since
    {
        paused += (end - since).num_seconds();
    }
    if paused > (end - e.start).num_seconds() {
        return Err(TimerError::PauseExceedsSpan);
    }
    e.paused_seconds = paused;
    e.pause_started_at = None;
    e.end = Some(end);
    Ok(e.id.clone())
}

/// Title of the entry with the latest start in the project, or "".
pub fn last_title_for_project(data: &Data, project_id: &str) -> String {
    data.entries
        .iter()
        .filter(|e| e.project_id == project_id)
        .max_by_key(|e| e.start)
        .map(|e| e.title.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Project;
    use crate::testutil::utc;

    fn data_with_project() -> Data {
        let mut d = Data::default();
        d.projects.push(Project {
            extra: Default::default(),
            id: "p1".into(),
            client_id: "c1".into(),
            name: "P".into(),
            color: 0,
            archived: false,
        });
        d.projects.push(Project {
            extra: Default::default(),
            id: "p2".into(),
            client_id: "c1".into(),
            name: "Q".into(),
            color: 1,
            archived: false,
        });
        d
    }

    fn started() -> (Data, Id) {
        let mut d = data_with_project();
        let id = start(&mut d, "p1", "t", "", utc("2026-10-06T01:00:00Z")).unwrap();
        (d, id)
    }

    #[test]
    fn start_creates_running_entry() {
        let mut d = data_with_project();
        let id = start(&mut d, "p1", "標題", "內容", utc("2026-10-06T01:00:00Z")).unwrap();
        assert_eq!(d.entries.len(), 1);
        let e = running(&d).unwrap();
        assert_eq!(e.id, id);
        assert_eq!(e.project_id, "p1");
        assert_eq!(e.start, utc("2026-10-06T01:00:00Z"));
        assert_eq!(e.end, None);
        assert_eq!(e.title, "標題");
        assert_eq!(e.content, "內容");
        assert_eq!(e.paused_seconds, 0);
        assert_eq!(d.settings.last_project_id.as_deref(), Some("p1"));
    }

    #[test]
    fn cannot_start_two() {
        let (mut d, _) = started();
        let r = start(&mut d, "p2", "", "", utc("2026-10-06T02:00:00Z"));
        assert!(matches!(r, Err(TimerError::AlreadyRunning)));
        assert_eq!(d.entries.len(), 1);
        assert_eq!(d.settings.last_project_id.as_deref(), Some("p1"));
    }

    #[test]
    fn start_unknown_project() {
        let mut d = data_with_project();
        let r = start(&mut d, "nope", "", "", utc("2026-10-06T01:00:00Z"));
        assert!(matches!(r, Err(TimerError::UnknownProject)));
        assert!(d.entries.is_empty());
        assert_eq!(d.settings.last_project_id, None);
    }

    #[test]
    fn start_archived_project_allowed() {
        let mut d = data_with_project();
        d.projects[0].archived = true;
        assert!(start(&mut d, "p1", "", "", utc("2026-10-06T01:00:00Z")).is_ok());
    }

    #[test]
    fn pause_resume_accumulates() {
        let (mut d, id) = started();
        assert_eq!(pause(&mut d, utc("2026-10-06T01:10:00Z")).unwrap(), id);
        assert_eq!(resume(&mut d, utc("2026-10-06T01:30:00Z")).unwrap(), id);
        pause(&mut d, utc("2026-10-06T02:00:00Z")).unwrap();
        resume(&mut d, utc("2026-10-06T02:05:00Z")).unwrap();
        assert_eq!(d.entries.len(), 1);
        let e = running(&d).unwrap();
        assert_eq!(e.paused_seconds, 1500);
        assert_eq!(e.pause_started_at, None);
    }

    #[test]
    fn resume_negative_gap_counts_zero() {
        let (mut d, _) = started();
        pause(&mut d, utc("2026-10-06T02:00:00Z")).unwrap();
        resume(&mut d, utc("2026-10-06T01:50:00Z")).unwrap();
        assert_eq!(running(&d).unwrap().paused_seconds, 0);
    }

    #[test]
    fn pause_resume_state_errors() {
        let mut d = data_with_project();
        let now = utc("2026-10-06T01:00:00Z");
        assert!(matches!(pause(&mut d, now), Err(TimerError::NotRunning)));
        assert!(matches!(resume(&mut d, now), Err(TimerError::NotRunning)));
        start(&mut d, "p1", "", "", now).unwrap();
        assert!(matches!(resume(&mut d, now), Err(TimerError::NotPaused)));
        pause(&mut d, now).unwrap();
        assert!(matches!(pause(&mut d, now), Err(TimerError::AlreadyPaused)));
    }

    #[test]
    fn stop_while_paused_ends_at_pause_start() {
        let (mut d, id) = started();
        pause(&mut d, utc("2026-10-06T01:40:00Z")).unwrap();
        let r = stop(&mut d, "T", "C", utc("2026-10-06T05:00:00Z")).unwrap();
        assert_eq!(r, id);
        let e = &d.entries[0];
        assert_eq!(e.end, Some(utc("2026-10-06T01:40:00Z")));
        assert_eq!(e.paused_seconds, 0);
        assert_eq!(e.pause_started_at, None);
        assert!(running(&d).is_none());
    }

    #[test]
    fn stop_writes_title_and_content() {
        let (mut d, _) = started();
        stop(&mut d, "標題", "- 做了事", utc("2026-10-06T03:00:00Z")).unwrap();
        let e = &d.entries[0];
        assert_eq!(e.title, "標題");
        assert_eq!(e.content, "- 做了事");
        assert_eq!(e.end, Some(utc("2026-10-06T03:00:00Z")));
    }

    #[test]
    fn stop_when_idle_errors() {
        let mut d = data_with_project();
        let r = stop(&mut d, "x", "y", utc("2026-10-06T03:00:00Z"));
        assert!(matches!(r, Err(TimerError::NotRunning)));
        assert!(d.entries.is_empty());
        let (mut d, _) = started();
        stop(&mut d, "a", "b", utc("2026-10-06T03:00:00Z")).unwrap();
        let before = d.clone();
        assert!(matches!(
            stop(&mut d, "c", "d", utc("2026-10-06T04:00:00Z")),
            Err(TimerError::NotRunning)
        ));
        assert_eq!(d, before);
    }

    #[test]
    fn stop_at_rejects_end_before_start() {
        let (mut d, _) = started();
        let before = d.clone();
        for end in ["2026-10-06T00:00:00Z", "2026-10-06T01:00:00Z"] {
            let r = stop_at(&mut d, utc(end), utc("2026-10-06T20:00:00Z"));
            assert!(matches!(r, Err(TimerError::EndNotAfterStart)));
        }
        assert_eq!(d, before);
    }

    #[test]
    fn stop_at_rejects_pause_exceeding_span() {
        let (mut d, _) = started();
        pause(&mut d, utc("2026-10-06T01:10:00Z")).unwrap();
        resume(&mut d, utc("2026-10-06T01:50:00Z")).unwrap();
        let before = d.clone();
        let r = stop_at(
            &mut d,
            utc("2026-10-06T01:30:00Z"),
            utc("2026-10-06T20:00:00Z"),
        );
        assert!(matches!(r, Err(TimerError::PauseExceedsSpan)));
        assert_eq!(d, before);
    }

    #[test]
    fn stop_at_while_paused_counts_tail_pause() {
        let (mut d, id) = started();
        pause(&mut d, utc("2026-10-06T02:00:00Z")).unwrap();
        let r = stop_at(
            &mut d,
            utc("2026-10-06T03:00:00Z"),
            utc("2026-10-06T20:00:00Z"),
        );
        assert_eq!(r.unwrap(), id);
        let e = &d.entries[0];
        assert_eq!(e.end, Some(utc("2026-10-06T03:00:00Z")));
        assert_eq!(e.paused_seconds, 3600);
        assert_eq!(e.pause_started_at, None);
    }

    #[test]
    fn stop_at_while_paused_end_before_pause_start() {
        let (mut d, _) = started();
        pause(&mut d, utc("2026-10-06T02:00:00Z")).unwrap();
        stop_at(
            &mut d,
            utc("2026-10-06T01:30:00Z"),
            utc("2026-10-06T20:00:00Z"),
        )
        .unwrap();
        let e = &d.entries[0];
        assert_eq!(e.end, Some(utc("2026-10-06T01:30:00Z")));
        assert_eq!(e.paused_seconds, 0);
        assert_eq!(e.pause_started_at, None);
    }

    #[test]
    fn stop_with_pause_start_before_start_never_ends_before_start() {
        // Corrupt data (hand-edited file, or the clock jumped back): pause before start.
        let (mut d, _) = started();
        d.entries[0].pause_started_at = Some(utc("2026-10-06T00:30:00Z"));
        d.entries[0].paused_seconds = 600;
        stop(&mut d, "t", "", utc("2026-10-06T05:00:00Z")).unwrap();
        let e = &d.entries[0];
        assert_eq!(e.end, Some(e.start));
        assert_eq!(e.paused_seconds, 0);
        assert_eq!(
            crate::duration::effective_seconds(e, utc("2026-10-06T05:00:00Z")),
            0
        );
    }

    #[test]
    fn stop_with_clock_behind_start_never_ends_before_start() {
        let (mut d, _) = started();
        stop(&mut d, "t", "", utc("2026-10-06T00:59:00Z")).unwrap();
        let e = &d.entries[0];
        assert_eq!(e.end, Some(utc("2026-10-06T01:00:00Z")));
    }

    #[test]
    fn stop_at_end_equal_to_pause_start() {
        let (mut d, _) = started();
        pause(&mut d, utc("2026-10-06T01:10:00Z")).unwrap();
        resume(&mut d, utc("2026-10-06T01:20:00Z")).unwrap();
        pause(&mut d, utc("2026-10-06T02:00:00Z")).unwrap();
        stop_at(
            &mut d,
            utc("2026-10-06T02:00:00Z"),
            utc("2026-10-06T20:00:00Z"),
        )
        .unwrap();
        let e = &d.entries[0];
        assert_eq!(e.end, Some(utc("2026-10-06T02:00:00Z")));
        assert_eq!(e.paused_seconds, 600);
        assert_eq!(e.pause_started_at, None);
    }

    #[test]
    fn stop_at_rejects_end_after_now() {
        let (mut d, _) = started();
        let before = d.clone();
        let r = stop_at(
            &mut d,
            utc("2026-10-06T03:00:01Z"),
            utc("2026-10-06T03:00:00Z"),
        );
        assert_eq!(r, Err(TimerError::EndInFuture));
        assert_eq!(d, before);
        stop_at(
            &mut d,
            utc("2026-10-06T03:00:00Z"),
            utc("2026-10-06T03:00:00Z"),
        )
        .unwrap();
    }

    #[test]
    fn stop_at_not_running_errors() {
        let mut d = data_with_project();
        let r = stop_at(
            &mut d,
            utc("2026-10-06T03:00:00Z"),
            utc("2026-10-06T04:00:00Z"),
        );
        assert!(matches!(r, Err(TimerError::NotRunning)));
    }

    #[test]
    fn last_title_uses_latest_start() {
        let mut d = data_with_project();
        let mk = |id: &str, p: &str, start: &str, title: &str| Entry {
            extra: Default::default(),
            id: id.into(),
            project_id: p.into(),
            start: utc(start),
            end: Some(utc(start)),
            title: title.into(),
            content: String::new(),
            paused_seconds: 0,
            pause_started_at: None,
        };
        d.entries
            .push(mk("a", "p1", "2026-10-06T05:00:00Z", "newest"));
        d.entries
            .push(mk("b", "p1", "2026-10-06T01:00:00Z", "older"));
        d.entries
            .push(mk("c", "p2", "2026-10-07T01:00:00Z", "other project"));
        assert_eq!(last_title_for_project(&d, "p1"), "newest");
        assert_eq!(last_title_for_project(&d, "p2"), "other project");
        assert_eq!(last_title_for_project(&d, "none"), "");
    }
}
