use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};

use super::*;
use crate::catalog::DeletionImpact;
use crate::model::{Client, Data, Entry, Project, parse_data, to_json};
use crate::storage::{Fs, Paths};
use crate::testutil::{MemFs, tz, utc};
use crate::timer::TimerError;

/// 09:00 on 2026-10-06 in Asia/Taipei.
const T0: &str = "2026-10-06T01:00:00Z";
const H: i64 = 3600;

fn paths() -> Paths {
    Paths {
        dir: PathBuf::from("/d/partimer"),
    }
}

fn at(secs: i64) -> DateTime<Utc> {
    utc(T0) + Duration::seconds(secs)
}

fn open_in(fs: MemFs, zone: &str, now: DateTime<Utc>) -> Session<MemFs> {
    Session::open(fs, paths(), tz(zone), now)
}

fn open(fs: MemFs, now: DateTime<Utc>) -> Session<MemFs> {
    open_in(fs, "Asia/Taipei", now)
}

fn entry(id: &str, project: &str, start: &str, end: Option<&str>, title: &str) -> Entry {
    Entry {
        extra: Default::default(),
        id: id.into(),
        project_id: project.into(),
        start: utc(start),
        end: end.map(utc),
        title: title.into(),
        content: String::new(),
        paused_seconds: 0,
        pause_started_at: None,
    }
}

/// Client 王小明 with 官網改版 (p1, last title 首頁切版) and 設計 (p2, last title 配色).
fn base_data() -> Data {
    let mut d = Data::default();
    d.clients.push(Client {
        extra: Default::default(),
        id: "c1".into(),
        name: "王小明".into(),
        archived: false,
    });
    for (id, name, color) in [("p1", "官網改版", 0), ("p2", "設計", 1)] {
        d.projects.push(Project {
            extra: Default::default(),
            id: id.into(),
            client_id: "c1".into(),
            name: name.into(),
            color,
            archived: false,
        });
    }
    d.entries.push(entry(
        "e1",
        "p1",
        "2026-10-05T01:00:00Z",
        Some("2026-10-05T02:00:00Z"),
        "首頁切版",
    ));
    d.entries.push(entry(
        "e2",
        "p2",
        "2026-10-05T03:00:00Z",
        Some("2026-10-05T04:00:00Z"),
        "配色",
    ));
    d.settings.last_project_id = Some("p1".into());
    d
}

fn seed(d: &Data) -> MemFs {
    let fs = MemFs::new();
    fs.put(paths().data(), &to_json(d));
    fs
}

fn seeded() -> MemFs {
    seed(&base_data())
}

fn on_disk(s: &Session<MemFs>) -> Data {
    parse_data(&s.fs.get(&paths().data()).expect("data.json exists")).unwrap()
}

fn disk_entry(s: &Session<MemFs>, id: &str) -> Entry {
    on_disk(s)
        .entries
        .into_iter()
        .find(|e| e.id == id)
        .expect("entry on disk")
}

fn log_text(s: &Session<MemFs>) -> String {
    String::from_utf8(s.fs.get(&paths().activity_log()).unwrap_or_default()).unwrap()
}

fn count_action(s: &Session<MemFs>, action: &str) -> usize {
    log_text(s).matches(&format!("\t{action}\t")).count()
}

fn running_id(s: &Session<MemFs>, now: DateTime<Utc>) -> String {
    s.panel_state(now).running.expect("running").id
}

fn notice(s: &Session<MemFs>, kind: NoticeKind) -> Option<Notice> {
    s.panel_state(utc(T0))
        .notices
        .into_iter()
        .find(|n| n.kind == kind)
}

#[test]
fn open_fresh_has_no_projects_and_empty_draft() {
    let s = open(MemFs::new(), at(0));
    let p = s.panel_state(at(0));
    assert!(p.groups.is_empty());
    assert_eq!(p.selected_project_id, None);
    assert_eq!(p.status, TimerStatus::Idle);
    assert!(p.running.is_none());
    assert_eq!(p.elapsed_seconds, 0);
    assert_eq!(p.draft.title, "");
    assert_eq!(p.draft.content, "");
    assert!(p.draft_saved);
    assert!(p.notices.is_empty());
    assert!(!p.long_running_prompt);
    assert!(p.today.is_empty());
}

#[test]
fn open_prefills_title_of_selected_project() {
    let s = open(seeded(), at(0));
    let p = s.panel_state(at(0));
    assert_eq!(p.selected_project_id.as_deref(), Some("p1"));
    assert_eq!(p.draft.title, "首頁切版");
    assert_eq!(p.draft.content, "");
    assert_eq!(p.groups.len(), 1);
    let names: Vec<_> = p.groups[0].projects.iter().map(|p| &p.name).collect();
    assert_eq!(names, ["官網改版", "設計"]);
}

#[test]
fn open_while_running_uses_entry_text_as_draft() {
    let mut d = base_data();
    let mut e = entry("e3", "p2", T0, None, "進行中");
    e.content = "筆記".into();
    d.entries.push(e);
    let s = open(seed(&d), at(60));
    let p = s.panel_state(at(60));
    assert_eq!(p.status, TimerStatus::Running);
    assert_eq!(p.selected_project_id.as_deref(), Some("p2"));
    assert_eq!(
        (p.draft.title.as_str(), p.draft.content.as_str()),
        ("進行中", "筆記")
    );
    assert!(p.draft_saved);
    assert_eq!(p.elapsed_seconds, 60);
}

#[test]
fn start_stop_cycle_persists_and_clears_draft() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "首頁切版", "- 改 banner", at(0)).unwrap();
    let p = s.panel_state(at(60));
    assert_eq!(p.status, TimerStatus::Running);
    assert_eq!(p.elapsed_seconds, 60);
    assert_eq!(p.draft.content, "- 改 banner");
    assert!(p.draft_saved);
    let id = p.running.unwrap().id;
    assert_eq!(disk_entry(&s, &id).end, None);
    let gen_running = p.draft.generation;

    s.set_draft(
        "首頁切版".into(),
        "- 改 banner\n- 修 RWD".into(),
        s.draft.generation,
    );
    assert!(!s.panel_state(at(60)).draft_saved);
    s.stop("首頁切版", "- 改 banner\n- 修 RWD", at(H)).unwrap();

    let p = s.panel_state(at(H));
    assert_eq!(p.status, TimerStatus::Idle);
    assert_eq!(p.draft.title, "首頁切版");
    assert_eq!(p.draft.content, "");
    assert!(p.draft.generation > gen_running);
    assert!(p.draft_saved);
    let e = disk_entry(&s, &id);
    assert_eq!(e.end, Some(at(H)));
    assert_eq!(e.content, "- 改 banner\n- 修 RWD");
    assert_eq!(count_action(&s, "START"), 1);
    assert_eq!(count_action(&s, "STOP"), 1);
}

#[test]
fn draft_is_memory_only() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "a", "", at(0)).unwrap();
    let before = s.fs.get(&paths().data()).unwrap();
    let generation = s.panel_state(at(1)).draft.generation;
    s.set_draft("新標題".into(), "打字中".into(), s.draft.generation);
    assert_eq!(s.fs.get(&paths().data()).unwrap(), before);
    let p = s.panel_state(at(1));
    assert_eq!(p.draft.title, "新標題");
    assert_eq!(p.draft.content, "打字中");
    assert_eq!(p.draft.generation, generation);
}

#[test]
fn stopped_entry_not_touched_by_later_draft() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "B", at(0)).unwrap();
    let id = running_id(&s, at(1));
    s.stop("A", "B", at(60)).unwrap();
    s.set_draft(String::new(), String::new(), s.draft.generation);
    assert!(s.save_draft("", "", at(120)).is_err());
    let e = disk_entry(&s, &id);
    assert_eq!((e.title.as_str(), e.content.as_str()), ("A", "B"));
    let mem = s.data.entries.iter().find(|e| e.id == id).unwrap();
    assert_eq!((mem.title.as_str(), mem.content.as_str()), ("A", "B"));
}

#[test]
fn double_stop_is_noop() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "B", at(0)).unwrap();
    s.stop("A", "B", at(60)).unwrap();
    let data = s.data.clone();
    let draft = s.panel_state(at(61)).draft;
    let bytes = s.fs.get(&paths().data()).unwrap();
    let err = s.stop("x", "y", at(61)).unwrap_err();
    assert_eq!(err, TimerError::NotRunning.to_string());
    assert_eq!(s.data, data);
    assert_eq!(s.panel_state(at(61)).draft, draft);
    assert_eq!(s.fs.get(&paths().data()).unwrap(), bytes);
    assert_eq!(count_action(&s, "STOP"), 1);
}

#[test]
fn stale_set_draft_is_ignored() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    let old_generation = s.panel_state(at(1)).draft.generation;
    s.stop("A", "finished text", at(60)).unwrap();
    let draft = s.panel_state(at(61)).draft;
    assert_ne!(draft.generation, old_generation);
    // A late sync from the fields of the finished entry must not become the next draft.
    s.set_draft("A".into(), "finished text".into(), old_generation);
    assert_eq!(s.panel_state(at(62)).draft, draft);
    // The current fields still sync.
    s.set_draft("B".into(), "next".into(), draft.generation);
    let now = s.panel_state(at(63)).draft;
    assert_eq!((now.title.as_str(), now.content.as_str()), ("B", "next"));
    assert_eq!(now.generation, draft.generation);
}

#[test]
fn switch_project_while_running_stops_and_starts() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "首頁切版", "", at(0)).unwrap();
    let old = running_id(&s, at(1));
    let generation = s.panel_state(at(1)).draft.generation;
    s.select_project("p2", "首頁切版", "做完了", at(1800))
        .unwrap();

    let e = disk_entry(&s, &old);
    assert_eq!(e.end, Some(at(1800)));
    assert_eq!(e.content, "做完了");
    let p = s.panel_state(at(1801));
    let new = p.running.clone().unwrap();
    assert_ne!(new.id, old);
    assert_eq!(new.project_id, "p2");
    assert_eq!((new.title.as_str(), new.content.as_str()), ("配色", ""));
    assert_eq!(disk_entry(&s, &new.id).start, at(1800));
    assert_eq!(
        (p.draft.title.as_str(), p.draft.content.as_str()),
        ("配色", "")
    );
    assert!(p.draft.generation > generation);
    assert_eq!(p.selected_project_id.as_deref(), Some("p2"));
    assert_eq!(count_action(&s, "SWITCH"), 1);
}

#[test]
fn switch_while_paused_ends_at_pause_start() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    let old = running_id(&s, at(1));
    s.pause(at(600)).unwrap();
    s.select_project("p2", "A", "", at(1800)).unwrap();
    let e = disk_entry(&s, &old);
    assert_eq!(e.end, Some(at(600)));
    assert_eq!(e.paused_seconds, 0);
    let new = s.panel_state(at(1800)).running.unwrap();
    assert_eq!(disk_entry(&s, &new.id).start, at(1800));
    assert_eq!(new.status, TimerStatus::Running);
}

#[test]
fn select_same_project_while_running_is_noop() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    let id = running_id(&s, at(1));
    s.select_project("p1", "A", "x", at(60)).unwrap();
    assert_eq!(running_id(&s, at(61)), id);
    assert_eq!(count_action(&s, "SWITCH"), 0);
}

#[test]
fn select_project_while_idle_keeps_typed_content() {
    let mut s = open(seeded(), at(0));
    let generation = s.panel_state(at(0)).draft.generation;
    s.set_draft("首頁切版".into(), "筆記".into(), s.draft.generation);
    s.select_project("p2", "首頁切版", "筆記", at(1)).unwrap();
    let p = s.panel_state(at(1));
    assert_eq!(p.status, TimerStatus::Idle);
    assert_eq!(p.selected_project_id.as_deref(), Some("p2"));
    assert_eq!(
        (p.draft.title.as_str(), p.draft.content.as_str()),
        ("配色", "筆記")
    );
    assert!(p.draft.generation > generation);
    assert_eq!(on_disk(&s).settings.last_project_id.as_deref(), Some("p2"));
    assert!(s.select_project("nope", "", "", at(2)).is_err());
}

#[test]
fn add_project_selects_the_new_project() {
    let mut s = open(seeded(), at(0));
    s.add_project(" 王小明 ", "新專案", "首頁切版", "x", at(1))
        .unwrap();
    let p = s.panel_state(at(1));
    let new_id = p.selected_project_id.clone().unwrap();
    assert!(new_id != "p1" && new_id != "p2");
    assert_eq!(
        (p.draft.title.as_str(), p.draft.content.as_str()),
        ("", "x")
    );
    assert_eq!(on_disk(&s).projects.len(), 3);
    assert!(s.add_project("王小明", "新專案", "", "", at(2)).is_err());
    assert!(s.add_project("", "x", "", "", at(2)).is_err());
}

#[test]
fn save_failure_keeps_changes_and_retries() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    let id = running_id(&s, at(1));
    s.fs.fail_writes.set(true);
    assert_eq!(s.stop("A", "B", at(60)), Ok(()));
    let n = notice(&s, NoticeKind::SaveError).expect("save error notice");
    assert!(
        n.message.starts_with("存檔失敗，每秒自動重試："),
        "{}",
        n.message
    );
    assert!(!n.dismissable);
    s.dismiss_notice(n.id);
    assert!(notice(&s, NoticeKind::SaveError).is_some());
    assert_eq!(disk_entry(&s, &id).end, None);

    assert!(s.tick(at(61)).is_empty());
    assert_eq!(disk_entry(&s, &id).end, None);
    let still: Vec<_> = s
        .panel_state(at(61))
        .notices
        .into_iter()
        .filter(|n| n.kind == NoticeKind::SaveError)
        .collect();
    assert_eq!(still.len(), 1, "notice is updated, not duplicated");

    s.fs.fail_writes.set(false);
    s.tick(at(62));
    let e = disk_entry(&s, &id);
    assert_eq!(e.end, Some(at(60)));
    assert_eq!(e.content, "B");
    assert!(notice(&s, NoticeKind::SaveError).is_none());
}

#[test]
fn paused_state_survives_reopen() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    s.pause(at(600)).unwrap();
    assert_eq!(s.panel_state(at(600)).elapsed_seconds, 600);
    let s = open(s.fs, at(2 * H));
    let p = s.panel_state(at(2 * H));
    assert_eq!(p.status, TimerStatus::Paused);
    assert_eq!(p.elapsed_seconds, 600);
    assert_eq!(p.running.unwrap().status, TimerStatus::Paused);
}

#[test]
fn pause_resume_errors_are_chinese() {
    let mut s = open(seeded(), at(0));
    assert_eq!(s.pause(at(0)), Err("目前沒有在計時".to_string()));
    s.start("p1", "A", "", at(0)).unwrap();
    assert_eq!(s.resume(at(1)), Err("目前沒有暫停".to_string()));
    s.pause(at(10)).unwrap();
    s.resume(at(70)).unwrap();
    assert_eq!(s.panel_state(at(100)).elapsed_seconds, 40);
    assert_eq!(count_action(&s, "PAUSE"), 1);
    assert_eq!(count_action(&s, "RESUME"), 1);
}

#[test]
fn long_running_reminder_fires_once_after_jump() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    assert!(s.tick(at(1)).is_empty());
    assert!(!s.panel_state(at(1)).long_running_prompt);

    let r = s.tick(at(13 * H));
    assert_eq!(
        r,
        vec![Reminder::LongRunning {
            project_name: "官網改版".into(),
            hours: 13
        }]
    );
    assert!(s.panel_state(at(13 * H)).long_running_prompt);
    assert!(s.tick(at(13 * H + 1)).is_empty());
    assert!(s.tick(at(13 * H + 2)).is_empty());

    s.snooze_long_running(at(13 * H + 3));
    assert!(!s.panel_state(at(13 * H + 3)).long_running_prompt);
    assert!(s.tick(at(20 * H)).is_empty());
    let r = s.tick(at(24 * H));
    assert_eq!(r.len(), 1);
    assert!(matches!(r[0], Reminder::LongRunning { hours: 24, .. }));
    assert!(s.tick(at(24 * H + 1)).is_empty());
}

#[test]
fn long_running_reminder_resets_for_new_entry_and_counts_paused() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    s.pause(at(13 * H)).unwrap();
    assert_eq!(s.tick(at(14 * H)).len(), 1, "paused but already past 12h");
    s.snooze_long_running(at(14 * H));
    s.select_project("p2", "A", "", at(14 * H)).unwrap();
    assert!(s.tick(at(15 * H)).is_empty());
    assert_eq!(
        s.tick(at(26 * H)).len(),
        1,
        "new entry starts from 12h again"
    );
}

#[test]
fn snooze_without_prompt_does_nothing() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    s.snooze_long_running(at(H));
    assert_eq!(s.tick(at(12 * H)).len(), 1);
}

#[test]
fn hourly_reminder_does_not_backfill_after_open() {
    let mut d = base_data();
    d.entries.push(entry("e3", "p1", T0, None, "A"));
    d.settings.hourly_reminder = true;
    let mut s = open(seed(&d), at(5 * H));
    assert!(s.tick(at(5 * H)).is_empty());
    assert!(s.tick(at(5 * H + 1800)).is_empty());
    assert_eq!(
        s.tick(at(6 * H)),
        vec![Reminder::Hourly {
            project_name: "官網改版".into(),
            hours: 6
        }]
    );
    assert!(s.tick(at(6 * H + 1)).is_empty());
}

#[test]
fn hourly_reminder_off_or_paused_is_silent() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    assert!(s.tick(at(H)).is_empty(), "off by default");
    s.update_settings(|st| st.hourly_reminder = true, at(H));
    assert!(on_disk(&s).settings.hourly_reminder);
    assert!(
        s.tick(at(H + 60)).is_empty(),
        "turning it on does not backfill"
    );
    assert_eq!(s.tick(at(2 * H)).len(), 1);
    s.pause(at(2 * H + 10)).unwrap();
    assert!(s.tick(at(5 * H)).is_empty());
}

#[test]
fn selected_project_falls_back_when_last_deleted() {
    let mut d = base_data();
    d.settings.last_project_id = Some("p2".into());
    let mut s = open(seed(&d), at(0));
    assert_eq!(
        s.panel_state(at(0)).selected_project_id.as_deref(),
        Some("p2")
    );

    assert_eq!(s.panel_state(at(0)).draft.title, "配色");
    s.set_draft("配色".into(), "筆記".into(), s.draft.generation);
    let generation = s.panel_state(at(0)).draft.generation;

    s.set_project_archived("p2", true, at(1)).unwrap();
    let p = s.panel_state(at(1));
    assert_eq!(p.selected_project_id.as_deref(), Some("p1"));
    assert_eq!(
        (p.draft.title.as_str(), p.draft.content.as_str()),
        ("首頁切版", "筆記")
    );
    assert!(p.draft.generation > generation);
    s.set_project_archived("p2", false, at(2)).unwrap();
    let p = s.panel_state(at(2));
    assert_eq!(p.selected_project_id.as_deref(), Some("p2"));
    assert_eq!(p.draft.title, "配色");
    let generation = p.draft.generation;
    s.rename_project("p2", "設計稿", at(2)).unwrap();
    assert_eq!(
        s.panel_state(at(2)).draft.generation,
        generation,
        "unchanged selection keeps the draft"
    );

    s.delete_item(ItemKind::Project, "p2", at(3)).unwrap();
    let p = s.panel_state(at(3));
    assert_eq!(p.selected_project_id.as_deref(), Some("p1"));
    assert_eq!(p.draft.title, "首頁切版");
    assert_eq!(on_disk(&s).settings.last_project_id, None);

    s.set_client_archived("c1", true, at(4)).unwrap();
    let p = s.panel_state(at(4));
    assert_eq!(p.selected_project_id, None);
    assert_eq!(p.draft.title, "");
    assert!(p.groups.is_empty());
}

#[test]
fn archived_running_project_stays_in_groups() {
    let mut s = open(seeded(), at(0));
    s.start("p2", "A", "", at(0)).unwrap();
    s.set_project_archived("p2", true, at(1)).unwrap();
    let p = s.panel_state(at(2));
    assert_eq!(p.status, TimerStatus::Running);
    assert_eq!(p.selected_project_id.as_deref(), Some("p2"));
    assert!(
        p.groups
            .iter()
            .any(|g| g.projects.iter().any(|o| o.id == "p2"))
    );
}

#[test]
fn deleting_running_project_stops_and_resets_draft() {
    let mut s = open(seeded(), at(0));
    s.start("p2", "配色", "abc", at(0)).unwrap();
    let generation = s.panel_state(at(1)).draft.generation;
    assert_eq!(
        s.deletion_impact(ItemKind::Project, "p2", at(H)),
        Ok(DeletionImpact {
            entry_count: 2,
            total_minutes: 120,
            includes_running: true
        })
    );
    s.delete_item(ItemKind::Project, "p2", at(H)).unwrap();
    let p = s.panel_state(at(H));
    assert_eq!(p.status, TimerStatus::Idle);
    assert_eq!(
        (p.draft.title.as_str(), p.draft.content.as_str()),
        ("首頁切版", "")
    );
    assert!(p.draft.generation > generation);
    assert_eq!(count_action(&s, "DELETE"), 2);
    assert!(on_disk(&s).entries.iter().all(|e| e.project_id == "p1"));
    assert!(s.delete_item(ItemKind::Project, "p2", at(H)).is_err());
}

#[test]
fn delete_client_removes_everything() {
    let mut s = open(seeded(), at(0));
    assert_eq!(
        s.deletion_impact(ItemKind::Client, "c1", at(0))
            .unwrap()
            .entry_count,
        2
    );
    s.delete_item(ItemKind::Client, "c1", at(1)).unwrap();
    let d = on_disk(&s);
    assert!(d.clients.is_empty() && d.projects.is_empty() && d.entries.is_empty());
    assert_eq!(count_action(&s, "DELETE"), 2);
    assert_eq!(s.panel_state(at(1)).draft.title, "");
}

#[test]
fn catalog_view_and_edits() {
    let mut s = open(seeded(), at(0));
    s.rename_client("c1", "  王大明 ", at(1)).unwrap();
    s.rename_project("p2", "視覺設計", at(1)).unwrap();
    s.set_project_color("p2", 5, at(1)).unwrap();
    assert_eq!(
        s.set_project_color("p2", 8, at(1)),
        Err("顏色不正確".to_string())
    );
    assert_eq!(
        s.rename_project("p2", "官網改版", at(1)),
        Err("這個客戶底下已經有同名的專案".to_string())
    );
    let c = s.catalog();
    assert_eq!(c.clients.len(), 1);
    assert_eq!(c.clients[0].name, "王大明");
    let p2 = c.clients[0].projects.iter().find(|p| p.id == "p2").unwrap();
    assert_eq!(
        (p2.name.as_str(), p2.color, p2.entry_count),
        ("視覺設計", 5, 1)
    );
    assert_eq!(on_disk(&s).projects[1].name, "視覺設計");
}

#[test]
fn records_filter_resets_when_project_deleted() {
    let mut d = base_data();
    d.entries.push(entry(
        "e3",
        "p2",
        "2026-10-06T05:00:00Z",
        Some("2026-10-06T05:30:00Z"),
        "改色",
    ));
    let mut s = open(seed(&d), at(0));
    let q = RecordsQuery {
        year: 2026,
        month: 10,
        client_id: Some("c1".into()),
        project_id: Some("p2".into()),
    };
    let v = s.records(q.clone(), at(0));
    assert_eq!(v.query, q);
    assert_eq!(v.total_minutes, 90);
    assert_eq!(
        (v.total_hm.as_str(), v.total_hours.as_str()),
        ("1h 30m", "1.50")
    );
    assert_eq!(v.days.len(), 2);
    assert_eq!(v.days[0].date, "2026-10-06");
    assert_eq!(v.days[0].weekday, "週二");
    assert_eq!(v.days[1].weekday, "週一");
    assert_eq!(v.per_project.len(), 1);
    assert_eq!(v.per_project[0].label, "王小明 / 設計");
    assert_eq!(v.per_project[0].hm, "1h 30m");
    assert_eq!(v.clients, vec![("c1".to_string(), "王小明".to_string())]);
    assert_eq!(v.projects.len(), 2);

    s.delete_item(ItemKind::Project, "p2", at(1)).unwrap();
    let v = s.records(q.clone(), at(1));
    assert_eq!(v.query.project_id, None);
    assert_eq!(v.query.client_id.as_deref(), Some("c1"));
    assert_eq!(v.total_minutes, 60);

    s.delete_item(ItemKind::Client, "c1", at(2)).unwrap();
    let v = s.records(q, at(2));
    assert_eq!(v.query.client_id, None);
    assert!(v.days.is_empty());
}

#[test]
fn records_show_running_entry_live() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    let q = RecordsQuery {
        year: 2026,
        month: 10,
        client_id: None,
        project_id: None,
    };
    let v = s.records(q.clone(), at(90));
    let e = &v.days[0].entries[0];
    assert_eq!(e.status, TimerStatus::Running);
    assert_eq!(e.seconds, 90);
    assert_eq!(e.end_hhmm, None);
    assert_eq!(e.start_hhmm, "09:00");
    assert_eq!(v.total_minutes, 120, "running entries are not in totals");
    let csv = String::from_utf8(s.export_csv(q)).unwrap();
    assert_eq!(csv.matches("\r\n").count(), 3, "header + 2 finished rows");
}

#[test]
fn today_lists_entries_and_total() {
    let mut d = base_data();
    d.entries.push(entry(
        "e3",
        "p2",
        "2026-10-06T00:00:00Z",
        Some("2026-10-06T00:30:00Z"),
        "早上",
    ));
    let mut s = open(seed(&d), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    let p = s.panel_state(at(600));
    let ids: Vec<_> = p.today.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(ids, ["A", "早上"]);
    assert_eq!(p.today_total_seconds, 1800 + 600);
    assert_eq!(p.today[1].client_name, "王小明");
    assert_eq!(p.today[1].color, 1);
    assert_eq!(p.today[1].end_hhmm.as_deref(), Some("08:30"));
}

#[test]
fn save_entry_reports_invalid_local_time() {
    let mut s = open_in(seeded(), "America/New_York", utc("2026-03-10T15:00:00Z"));
    let form = EntryForm {
        project_id: "p1".into(),
        start_local: "2026-03-08T02:30".into(),
        end_local: Some("2026-03-08T04:00".into()),
        still_running: false,
        paused_minutes: 0,
        title: "t".into(),
        content: String::new(),
    };
    let err = s
        .save_entry(None, form, utc("2026-03-10T15:00:00Z"))
        .unwrap_err();
    assert!(err.contains("夏令時間"), "{err}");
    assert_eq!(s.data.entries.len(), 2);
}

#[test]
fn add_and_edit_entry_via_forms() {
    let mut s = open(seeded(), at(0));
    let mut form = s.new_entry_form(at(0));
    assert_eq!(form.project_id, "p1");
    assert_eq!(form.start_local, "2026-10-06T08:00");
    assert_eq!(form.end_local.as_deref(), Some("2026-10-06T09:00"));
    assert!(!form.still_running);
    form.title = "補登".into();
    form.paused_minutes = 10;
    s.save_entry(None, form, at(0)).unwrap();
    assert_eq!(count_action(&s, "ADD"), 1);
    let added = on_disk(&s)
        .entries
        .into_iter()
        .find(|e| e.title == "補登")
        .unwrap();
    assert_eq!(added.paused_seconds, 600);

    let mut f = s.entry_form(&added.id).unwrap();
    f.end_local = Some(String::new());
    assert_eq!(
        s.save_entry(Some(&added.id), f.clone(), at(0)),
        Err("請填結束時間".to_string())
    );
    f.end_local = Some("2026-10-06T08:30".into());
    s.save_entry(Some(&added.id), f, at(0)).unwrap();
    assert_eq!(
        disk_entry(&s, &added.id).end,
        Some(utc("2026-10-06T00:30:00Z"))
    );
    assert_eq!(count_action(&s, "EDIT"), 1);
    assert!(s.entry_form("nope").is_none());

    s.delete_entry(&added.id, at(1)).unwrap();
    assert_eq!(count_action(&s, "DELETE"), 1);
    assert_eq!(on_disk(&s).entries.len(), 2);
    assert_eq!(
        s.delete_entry(&added.id, at(1)),
        Err("找不到這筆紀錄".to_string())
    );
}

#[test]
fn editing_without_touching_times_keeps_seconds() {
    let mut d = base_data();
    let mut e = entry(
        "e3",
        "p1",
        "2026-10-06T00:00:30Z",
        Some("2026-10-06T00:40:45Z"),
        "t",
    );
    e.paused_seconds = 90;
    d.entries.push(e);
    let mut s = open(seed(&d), at(0));
    let mut f = s.entry_form("e3").unwrap();
    assert_eq!(f.start_local, "2026-10-06T08:00");
    assert_eq!(f.paused_minutes, 2);
    f.title = "改標題".into();
    s.save_entry(Some("e3"), f, at(0)).unwrap();
    let e = disk_entry(&s, "e3");
    assert_eq!(e.start, utc("2026-10-06T00:00:30Z"));
    assert_eq!(e.end, Some(utc("2026-10-06T00:40:45Z")));
    assert_eq!(e.paused_seconds, 90);
    assert_eq!(e.title, "改標題");
}

#[test]
fn editing_running_entry_updates_clean_draft() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "B", at(0)).unwrap();
    let id = running_id(&s, at(1));
    let generation = s.panel_state(at(1)).draft.generation;
    let mut f = s.entry_form(&id).unwrap();
    assert!(f.still_running);
    assert_eq!(f.end_local, None);
    f.title = "A2".into();
    f.content = "B2".into();
    s.save_entry(Some(&id), f, at(60)).unwrap();
    let p = s.panel_state(at(60));
    assert_eq!(
        (p.draft.title.as_str(), p.draft.content.as_str()),
        ("A2", "B2")
    );
    assert!(p.draft.generation > generation);
    assert!(p.draft_saved);
    assert!(notice(&s, NoticeKind::EditedElsewhere).is_none());
    assert_eq!(disk_entry(&s, &id).title, "A2");
}

#[test]
fn editing_running_entry_keeps_dirty_draft_and_notifies() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "B", at(0)).unwrap();
    let id = running_id(&s, at(1));
    s.set_draft("A".into(), "打字中".into(), s.draft.generation);
    let generation = s.panel_state(at(1)).draft.generation;
    let mut f = s.entry_form(&id).unwrap();
    f.title = "A2".into();
    s.save_entry(Some(&id), f, at(60)).unwrap();
    let p = s.panel_state(at(60));
    assert_eq!(
        (p.draft.title.as_str(), p.draft.content.as_str()),
        ("A", "打字中")
    );
    assert_eq!(p.draft.generation, generation);
    assert!(!p.draft_saved);
    let n = notice(&s, NoticeKind::EditedElsewhere).expect("notice");
    assert_eq!(
        n.message,
        "這筆已在紀錄視窗被修改，面板上的文字尚未儲存（按儲存才會寫入，結束 app 時不會自動存入）"
    );
    assert!(n.dismissable);
    s.dismiss_notice(n.id);
    assert!(notice(&s, NoticeKind::EditedElsewhere).is_none());
}

#[test]
fn finishing_running_entry_in_records_resets_clean_draft() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "B", at(0)).unwrap();
    let id = running_id(&s, at(1));
    let mut f = s.entry_form(&id).unwrap();
    f.still_running = false;
    f.end_local = Some("2026-10-06T09:30".into());
    s.save_entry(Some(&id), f, at(H)).unwrap();
    let p = s.panel_state(at(H));
    assert_eq!(p.status, TimerStatus::Idle);
    assert_eq!(
        (p.draft.title.as_str(), p.draft.content.as_str()),
        ("A", "")
    );
}

#[test]
fn stop_at_local_writes_text_and_replaces_draft() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    let id = running_id(&s, at(1));
    let generation = s.panel_state(at(1)).draft.generation;

    assert_eq!(
        s.stop_at_local("A", "x", "2026-10-06 13:00", at(14 * H)),
        Err("時間格式不正確".to_string())
    );
    assert_eq!(
        s.stop_at_local("A", "x", "2026-10-06T08:00", at(14 * H)),
        Err("結束時間必須晚於開始時間".to_string())
    );
    assert_eq!(
        s.stop_at_local("A", "x", "2026-10-07T09:00", at(14 * H)),
        Err("結束時間不能晚於現在".to_string())
    );
    let mem = s.data.entries.iter().find(|e| e.id == id).unwrap();
    assert_eq!((mem.end, mem.content.as_str()), (None, ""));

    s.stop_at_local("A", "寫完", "2026-10-06T13:00", at(14 * H))
        .unwrap();
    let e = disk_entry(&s, &id);
    assert_eq!(e.end, Some(utc("2026-10-06T05:00:00Z")));
    assert_eq!(e.content, "寫完");
    let p = s.panel_state(at(14 * H));
    assert_eq!(p.status, TimerStatus::Idle);
    assert_eq!(
        (p.draft.title.as_str(), p.draft.content.as_str()),
        ("A", "")
    );
    assert!(p.draft.generation > generation);
    assert!(!p.long_running_prompt);
    assert_eq!(count_action(&s, "AUTO_STOP"), 1);
}

#[test]
fn save_draft_writes_running_entry() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    let id = running_id(&s, at(1));
    s.set_draft("A".into(), "note".into(), s.draft.generation);
    let generation = s.panel_state(at(1)).draft.generation;
    s.save_draft("A", "note", at(5)).unwrap();
    assert_eq!(disk_entry(&s, &id).content, "note");
    let p = s.panel_state(at(5));
    assert!(p.draft_saved);
    assert_eq!(p.draft.generation, generation);
    assert_eq!(count_action(&s, "SAVE"), 1);
}

#[test]
fn restore_notice_and_activity_logged() {
    let fs = MemFs::new();
    fs.put(paths().data(), b"{broken");
    fs.put(
        paths().backups().join("data-2026-10-05.json"),
        &to_json(&base_data()),
    );
    let s = open(fs, at(0));
    let n = notice(&s, NoticeKind::Restored).expect("restored notice");
    assert_eq!(
        n.message,
        "資料檔損壞，已從 2026-10-05 的備份還原，壞檔保留為 data.corrupt-20261006-090000.json"
    );
    assert!(n.dismissable);
    assert_eq!(count_action(&s, "RESTORE"), 1);
    assert_eq!(s.panel_state(at(0)).groups.len(), 1);
    assert_eq!(on_disk(&s).projects.len(), 2);
}

#[test]
fn restore_write_back_failure_is_retried() {
    let fs = MemFs::new();
    fs.put(
        paths().backups().join("data-2026-10-05.json"),
        &to_json(&base_data()),
    );
    fs.fail_writes.set(true);
    let mut s = open(fs, at(0));
    let n = notice(&s, NoticeKind::Restored).unwrap();
    assert_eq!(n.message, "找不到資料檔，已從 2026-10-05 的備份還原");
    assert!(notice(&s, NoticeKind::SaveError).is_some());
    assert!(!s.fs.exists(&paths().data()));
    s.fs.fail_writes.set(false);
    s.tick(at(1));
    assert_eq!(on_disk(&s).projects.len(), 2);
    assert!(notice(&s, NoticeKind::SaveError).is_none());
}

#[test]
fn corrupt_without_backup_warns_permanently() {
    let fs = MemFs::new();
    fs.put(paths().data(), b"{broken");
    let mut s = open(fs, at(0));
    let n = notice(&s, NoticeKind::CorruptNoBackup).unwrap();
    assert_eq!(
        n.message,
        "資料檔損壞且沒有可用的備份，已從空白資料開始。壞檔保留為 data.corrupt-20261006-090000.json"
    );
    assert!(!n.dismissable);
    s.dismiss_notice(n.id);
    assert!(notice(&s, NoticeKind::CorruptNoBackup).is_some());
}

#[test]
fn unreadable_data_file_blocks_saving() {
    let fs = seeded();
    let original = fs.get(&paths().data()).unwrap();
    fs.put(
        paths().backups().join("data-2026-10-05.json"),
        &to_json(&base_data()),
    );
    *fs.fail_reads_of.borrow_mut() = Some(paths().data());
    let mut s = open(fs, at(0));
    let n = notice(&s, NoticeKind::Unreadable).expect("unreadable notice");
    let io = std::io::Error::from(std::io::ErrorKind::PermissionDenied).to_string();
    assert_eq!(
        n.message,
        format!(
            "資料檔無法讀取（{io}），為避免覆蓋原檔，這次開啟期間的變更不會存檔。請檢查檔案權限後重開 app。已暫時顯示 2026-10-05 的備份內容。"
        )
    );
    assert!(!n.dismissable);

    s.start("p1", "A", "", at(0)).unwrap();
    s.tick(at(1));
    s.stop("A", "", at(60)).unwrap();
    s.flush_on_exit(at(61));
    assert_eq!(s.fs.get(&paths().data()).unwrap(), original);
    assert!(
        !s.fs
            .exists(&paths().backup_for(chrono::NaiveDate::from_ymd_opt(2026, 10, 6).unwrap()))
    );
    assert!(notice(&s, NoticeKind::SaveError).is_none());
    assert!(s.dirty);
}

#[test]
fn newer_version_data_file_blocks_saving() {
    let mut d = base_data();
    d.version = 2;
    let fs = seed(&d);
    let original = fs.get(&paths().data()).unwrap();
    let mut s = open(fs, at(0));
    let n = notice(&s, NoticeKind::NewerVersion).expect("newer version notice");
    assert_eq!(
        n.message,
        "資料檔是由較新版本的打工人計時器建立的，為避免覆蓋，這次開啟期間的變更不會存檔。請改用較新的版本。"
    );
    assert!(!n.dismissable);
    s.dismiss_notice(n.id);
    assert!(notice(&s, NoticeKind::NewerVersion).is_some());
    assert_eq!(s.panel_state(at(0)).groups.len(), 1, "data is shown");
    assert!(log_text(&s).contains("note=newer data version 2, saving blocked"));

    s.start("p1", "A", "", at(0)).unwrap();
    s.tick(at(1));
    s.stop("A", "", at(60)).unwrap();
    s.flush_on_exit(at(61));
    assert_eq!(s.fs.get(&paths().data()).unwrap(), original);
    assert!(
        !s.fs
            .exists(&paths().backup_for(chrono::NaiveDate::from_ymd_opt(2026, 10, 6).unwrap()))
    );
    assert!(notice(&s, NoticeKind::SaveError).is_none());
}

#[test]
fn unreadable_without_backup_omits_backup_sentence() {
    let fs = seeded();
    *fs.fail_reads_of.borrow_mut() = Some(paths().data());
    let s = open(fs, at(0));
    let n = notice(&s, NoticeKind::Unreadable).unwrap();
    assert!(
        n.message.ends_with("請檢查檔案權限後重開 app。"),
        "{}",
        n.message
    );
}

#[test]
fn day_rollover_triggers_backup_on_next_save() {
    let mut s = open(seeded(), utc("2026-10-06T01:00:00Z"));
    let day = |d| paths().backups().join(format!("data-2026-10-{d:02}.json"));
    assert!(s.fs.exists(&day(6)));
    assert!(!s.fs.exists(&day(7)));
    s.start("p1", "A", "", utc("2026-10-07T01:00:00Z")).unwrap();
    let backup = parse_data(&s.fs.get(&day(7)).expect("rollover backup")).unwrap();
    assert!(
        backup.entries.iter().all(|e| e.end.is_some()),
        "taken before the save"
    );
    assert!(on_disk(&s).entries.iter().any(|e| e.end.is_none()));
}

#[test]
fn flush_on_exit_saves_dirty_draft() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    let id = running_id(&s, at(1));
    s.set_draft("A".into(), "late note".into(), s.draft.generation);
    s.flush_on_exit(at(60));
    let e = disk_entry(&s, &id);
    assert_eq!(e.content, "late note");
    assert_eq!(e.end, None);
    assert_eq!(count_action(&s, "SAVE"), 1);

    s.flush_on_exit(at(61));
    assert_eq!(
        count_action(&s, "SAVE"),
        1,
        "clean draft is not saved again"
    );
}

#[test]
fn push_and_dismiss_info_notice() {
    let mut s = open(MemFs::new(), at(0));
    s.push_notice(NoticeKind::Info, "全域快捷鍵註冊失敗".into());
    let n = notice(&s, NoticeKind::Info).unwrap();
    assert!(n.dismissable);
    s.dismiss_notice(n.id);
    assert!(s.panel_state(at(0)).notices.is_empty());
    assert_eq!(s.data_dir(), paths().dir.as_path());
    assert!(!s.settings().hourly_reminder);
}

#[test]
fn finishing_running_entry_in_records_resets_dirty_draft_and_keeps_text_in_notice() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "B", at(0)).unwrap();
    let id = running_id(&s, at(1));
    s.set_draft("A".into(), "沒存的字".into(), s.draft.generation);
    let generation = s.panel_state(at(1)).draft.generation;
    let mut f = s.entry_form(&id).unwrap();
    f.still_running = false;
    f.end_local = Some("2026-10-06T09:30".into());
    s.save_entry(Some(&id), f, at(H)).unwrap();
    let p = s.panel_state(at(H));
    assert_eq!(p.status, TimerStatus::Idle);
    assert!(
        p.draft.generation > generation,
        "draft detached from the ended entry"
    );
    assert_eq!(
        (p.draft.title.as_str(), p.draft.content.as_str()),
        ("A", "")
    );
    let n = notice(&s, NoticeKind::EditedElsewhere).expect("notice");
    assert_eq!(
        n.message,
        "這筆已在紀錄視窗結束，面板上未儲存的文字沒有寫入。原文：標題「A」，內容：沒存的字（請到紀錄視窗補寫）"
    );
    assert!(n.dismissable);
    assert_eq!(disk_entry(&s, &id).content, "B");

    // The next START must not carry the old entry's text.
    let stale = s.draft.generation - 1;
    s.set_draft("A".into(), "沒存的字".into(), stale);
    s.start("p1", &p.draft.title, &p.draft.content, at(H + 60))
        .unwrap();
    let next = running_id(&s, at(H + 61));
    assert_ne!(next, id);
    assert_eq!(disk_entry(&s, &next).content, "");
}

#[test]
fn finishing_running_entry_in_records_notice_omits_empty_parts() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "B", at(0)).unwrap();
    let id = running_id(&s, at(1));
    s.set_draft(String::new(), "只有內容".into(), s.draft.generation);
    let mut f = s.entry_form(&id).unwrap();
    f.still_running = false;
    f.end_local = Some("2026-10-06T09:30".into());
    s.save_entry(Some(&id), f, at(H)).unwrap();
    let n = notice(&s, NoticeKind::EditedElsewhere).expect("notice");
    assert_eq!(
        n.message,
        "這筆已在紀錄視窗結束，面板上未儲存的文字沒有寫入。原文：內容：只有內容（請到紀錄視窗補寫）"
    );
}

#[test]
fn flush_on_exit_does_not_overwrite_records_window_edit() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "B", at(0)).unwrap();
    let id = running_id(&s, at(1));
    s.set_draft("A".into(), "打字中".into(), s.draft.generation);
    let mut f = s.entry_form(&id).unwrap();
    f.title = "A2".into();
    f.content = "紀錄視窗的內容".into();
    s.save_entry(Some(&id), f, at(60)).unwrap();
    assert!(notice(&s, NoticeKind::EditedElsewhere).is_some());
    s.flush_on_exit(at(120));
    let e = disk_entry(&s, &id);
    assert_eq!(
        (e.title.as_str(), e.content.as_str()),
        ("A2", "紀錄視窗的內容")
    );
    assert_eq!(count_action(&s, "SAVE"), 0);
}

#[test]
fn quarantined_items_are_reported_once_on_open() {
    let mut d = base_data();
    d.quarantined_entries
        .push(serde_json::json!({"id": "bad1"}));
    d.quarantined_entries
        .push(serde_json::json!({"id": "bad2"}));
    d.quarantined_projects
        .push(serde_json::json!({"id": "badp"}));
    let s = open(seed(&d), at(0));
    let n = notice(&s, NoticeKind::Info).expect("info notice");
    assert_eq!(
        n.message,
        "有 3 筆資料格式不正確，已另外保留在資料檔中，不會顯示在清單與合計裡。"
    );
    assert!(n.dismissable);
    assert_eq!(s.panel_state(at(0)).notices.len(), 1);
    assert_eq!(log_text(&s).matches("note=quarantined=3").count(), 1);
    assert_eq!(count_action(&s, "RESTORE"), 1);
}

#[test]
fn no_quarantine_notice_when_everything_parses() {
    let s = open(seeded(), at(0));
    assert!(notice(&s, NoticeKind::Info).is_none());
    assert!(!log_text(&s).contains("quarantined"));
}

#[test]
fn save_draft_clears_edited_elsewhere_notice() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "B", at(0)).unwrap();
    let id = running_id(&s, at(1));
    s.set_draft("A".into(), "打字中".into(), s.draft.generation);
    let mut f = s.entry_form(&id).unwrap();
    f.title = "A2".into();
    s.save_entry(Some(&id), f, at(60)).unwrap();
    assert!(notice(&s, NoticeKind::EditedElsewhere).is_some());
    s.save_draft("A", "打字中", at(61)).unwrap();
    assert!(notice(&s, NoticeKind::EditedElsewhere).is_none());
    assert_eq!(disk_entry(&s, &id).content, "打字中");
}

#[test]
fn open_repairs_multiple_running_entries() {
    let mut d = base_data();
    let mut r1 = entry("r1", "p1", "2026-10-05T22:00:00Z", None, "r1");
    r1.paused_seconds = 120;
    let mut r2 = entry("r2", "p2", "2026-10-05T23:00:00Z", None, "r2");
    r2.pause_started_at = Some(utc("2026-10-05T23:10:00Z"));
    let r3 = entry("r3", "p1", "2026-10-06T00:00:00Z", None, "r3");
    // Data order differs from start order on purpose.
    d.entries.push(r3);
    d.entries.push(r1);
    d.entries.push(r2);
    let s = open(seed(&d), at(0));

    let p = s.panel_state(at(0));
    assert_eq!(p.running.unwrap().id, "r3");
    let disk = on_disk(&s);
    assert_eq!(disk.entries.len(), 5);
    assert_eq!(disk.entries.iter().filter(|e| e.end.is_none()).count(), 1);
    let r1 = disk_entry(&s, "r1");
    assert_eq!(r1.end, Some(utc("2026-10-05T23:00:00Z")));
    assert_eq!(r1.paused_seconds, 120);
    let r2 = disk_entry(&s, "r2");
    assert_eq!(
        r2.end,
        Some(utc("2026-10-05T23:10:00Z")),
        "paused: ends at pause start"
    );
    assert_eq!(r2.pause_started_at, None);

    let n = notice(&s, NoticeKind::Info).expect("info notice");
    assert_eq!(
        n.message,
        "發現多筆同時在計時的紀錄，已保留最新的一筆，其餘已在下一筆開始時結束，請到紀錄視窗確認。"
    );
    assert!(n.dismissable);
    let log = log_text(&s);
    assert_eq!(log.matches("\tEDIT\t").count(), 2);
    assert_eq!(log.matches("note=repaired-multiple-running").count(), 2);
}

#[test]
fn single_running_entry_is_not_repaired() {
    let mut d = base_data();
    d.entries.push(entry("r1", "p1", T0, None, "r1"));
    let s = open(seed(&d), at(60));
    assert!(notice(&s, NoticeKind::Info).is_none());
    assert_eq!(count_action(&s, "EDIT"), 0);
}

/// Running entry edited in the records window while the panel draft is dirty.
fn conflicted_session() -> (Session<MemFs>, String) {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "B", at(0)).unwrap();
    let id = running_id(&s, at(1));
    s.set_draft("A".into(), "打字中".into(), s.draft.generation);
    let mut f = s.entry_form(&id).unwrap();
    f.title = "A2".into();
    f.content = "紀錄視窗的內容".into();
    s.save_entry(Some(&id), f, at(60)).unwrap();
    (s, id)
}

#[test]
fn flush_on_exit_after_dismissing_conflict_keeps_records_edit() {
    let (mut s, id) = conflicted_session();
    let n = notice(&s, NoticeKind::EditedElsewhere).expect("notice");
    assert_eq!(
        n.message,
        "這筆已在紀錄視窗被修改，面板上的文字尚未儲存（按儲存才會寫入，結束 app 時不會自動存入）"
    );
    s.dismiss_notice(n.id);
    assert!(notice(&s, NoticeKind::EditedElsewhere).is_none());
    s.flush_on_exit(at(120));
    let e = disk_entry(&s, &id);
    assert_eq!(
        (e.title.as_str(), e.content.as_str()),
        ("A2", "紀錄視窗的內容")
    );
    assert_eq!(count_action(&s, "SAVE"), 0);
}

#[test]
fn save_after_conflict_clears_it_so_exit_saves_again() {
    let (mut s, id) = conflicted_session();
    s.save_draft("A", "打字中", at(61)).unwrap();
    let g = s.draft.generation;
    s.set_draft("A".into(), "再改一次".into(), g);
    s.flush_on_exit(at(120));
    assert_eq!(disk_entry(&s, &id).content, "再改一次");
}

#[test]
fn typing_back_to_stored_text_clears_conflict() {
    let (mut s, id) = conflicted_session();
    let g = s.draft.generation;
    s.set_draft("A2".into(), "紀錄視窗的內容".into(), g);
    assert!(s.panel_state(at(61)).draft_saved);
    assert!(notice(&s, NoticeKind::EditedElsewhere).is_none());
    s.set_draft("A2".into(), "接著打".into(), g);
    s.flush_on_exit(at(120));
    assert_eq!(disk_entry(&s, &id).content, "接著打");
}

#[test]
fn stop_after_conflict_clears_it() {
    let (mut s, _) = conflicted_session();
    s.stop("A", "打字中", at(90)).unwrap();
    assert!(notice(&s, NoticeKind::EditedElsewhere).is_none());
    s.start("p1", "N", "", at(100)).unwrap();
    let id = running_id(&s, at(101));
    let g = s.draft.generation;
    s.set_draft("N".into(), "新的".into(), g);
    s.flush_on_exit(at(120));
    assert_eq!(disk_entry(&s, &id).content, "新的");
}

#[test]
fn save_entry_rejects_future_end_from_form() {
    // now = 09:00:30 local; the typed minute 09:01 is within the 60 s tolerance.
    let now = at(30);
    let mut s = open(seeded(), now);
    let mut form = s.new_entry_form(now);
    form.end_local = Some("2026-10-06T10:00".into());
    let err = s.save_entry(None, form.clone(), now).unwrap_err();
    assert_eq!(err, "結束時間不能晚於現在");
    form.end_local = Some("2026-10-06T09:01".into());
    s.save_entry(None, form, now).unwrap();
    assert_eq!(on_disk(&s).entries.len(), 3);
}

#[test]
fn backup_folder_error_is_shown_and_logged() {
    let fs = MemFs::new();
    fs.put(
        paths().backups().join("data-2026-10-05.json"),
        &to_json(&base_data()),
    );
    fs.fail_lists.set(true);
    let s = open(fs, at(0));
    let io = std::io::Error::from(std::io::ErrorKind::PermissionDenied).to_string();
    let n = notice(&s, NoticeKind::Info).expect("info notice");
    assert_eq!(
        n.message,
        format!("無法讀取備份資料夾（{io}），這次沒辦法從備份還原。")
    );
    assert!(log_text(&s).contains("backups unreadable"));
}

#[test]
fn corrupt_file_that_cannot_be_kept_names_the_reason() {
    let fs = MemFs::new();
    fs.put(paths().data(), b"{broken");
    fs.fail_renames.set(true);
    let s = open(fs, at(0));
    let io = std::io::Error::from(std::io::ErrorKind::StorageFull).to_string();
    let n = notice(&s, NoticeKind::Unreadable).expect("unreadable notice");
    assert_eq!(
        n.message,
        format!(
            "資料檔損壞，且無法改名保留（{io}），為避免覆蓋原檔，這次開啟期間的變更不會存檔。請檢查檔案權限後重開 app。"
        )
    );
    assert_eq!(s.fs.get(&paths().data()).unwrap(), b"{broken");
}

#[test]
fn activity_log_failure_does_not_affect_saving() {
    let fs = seeded();
    fs.fail_appends.set(true);
    let mut s = open(fs, at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    let id = running_id(&s, at(1));
    s.stop("A", "寫完", at(60)).unwrap();
    assert_eq!(disk_entry(&s, &id).content, "寫完");
    assert!(notice(&s, NoticeKind::SaveError).is_none());
    assert!(!s.dirty);
    assert_eq!(log_text(&s), "");
}

#[test]
fn select_project_rejects_archived_and_unknown() {
    let mut d = base_data();
    d.projects[1].archived = true; // p2
    d.clients.push(Client {
        extra: Default::default(),
        id: "c2".into(),
        name: "封存客戶".into(),
        archived: true,
    });
    d.projects.push(Project {
        extra: Default::default(),
        id: "p3".into(),
        client_id: "c2".into(),
        name: "藏起來".into(),
        color: 2,
        archived: false,
    });
    let mut s = open(seed(&d), at(0));
    let generation = s.draft.generation;
    for id in ["p2", "p3"] {
        assert_eq!(
            s.select_project(id, "首頁切版", "", at(1)),
            Err("這個專案已封存，請先在設定中取消封存".to_string())
        );
    }
    assert_eq!(
        s.select_project("nope", "首頁切版", "", at(1)),
        Err("找不到這個專案".to_string())
    );
    let p = s.panel_state(at(1));
    assert_eq!(p.selected_project_id.as_deref(), Some("p1"));
    assert_eq!(p.draft.generation, generation);
    assert_eq!(on_disk(&s).settings.last_project_id.as_deref(), Some("p1"));

    // Running: switching to an archived project is rejected too, nothing is stopped.
    s.start("p1", "A", "", at(2)).unwrap();
    let id = running_id(&s, at(3));
    assert!(s.select_project("p2", "A", "", at(4)).is_err());
    assert_eq!(running_id(&s, at(5)), id);
    assert_eq!(count_action(&s, "SWITCH"), 0);
}

#[test]
fn select_running_archived_project_is_a_noop() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    s.set_project_archived("p1", true, at(1)).unwrap();
    s.select_project("p1", "A", "", at(2)).unwrap();
    assert_eq!(s.panel_state(at(3)).status, TimerStatus::Running);
}

#[test]
fn timer_status_and_notices_match_panel_state() {
    let mut s = open(seeded(), at(0));
    assert_eq!(s.timer_status(at(0)), (TimerStatus::Idle, 0));
    s.start("p1", "A", "", at(0)).unwrap();
    s.pause(at(90)).unwrap();
    let p = s.panel_state(at(200));
    assert_eq!(s.timer_status(at(200)), (p.status, p.elapsed_seconds));
    assert_eq!(s.timer_status(at(200)), (TimerStatus::Paused, 90));
    s.push_notice(NoticeKind::Info, "x".into());
    assert_eq!(s.notices(), s.panel_state(at(200)).notices.as_slice());
    assert_eq!(s.notices().len(), 1);
}
