use super::*;

fn entry_of(s: &Session<MemFs>, id: &str) -> Entry {
    s.data.entries.iter().find(|e| e.id == id).unwrap().clone()
}

fn running_of(s: &Session<MemFs>) -> Entry {
    s.running().expect("running").clone()
}

#[test]
fn tray_pick_idle_starts_with_draft() {
    let mut s = open(seeded(), at(0));
    s.set_draft("寫好的標題".into(), "內容".into(), s.draft.generation);
    assert_eq!(s.tray_pick("p1", at(10)).unwrap(), TrayPick::Started);
    let e = running_of(&s);
    assert_eq!(e.project_id, "p1");
    assert_eq!(
        (e.title.as_str(), e.content.as_str()),
        ("寫好的標題", "內容")
    );
}

#[test]
fn tray_pick_other_project_uses_its_last_title() {
    let mut s = open(seeded(), at(0));
    s.set_draft("首頁切版".into(), "x".into(), s.draft.generation);
    assert_eq!(s.tray_pick("p2", at(10)).unwrap(), TrayPick::Started);
    let e = running_of(&s);
    assert_eq!(e.project_id, "p2");
    assert_eq!((e.title.as_str(), e.content.as_str()), ("配色", "x"));
}

#[test]
fn tray_pick_running_project_is_noop() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "t", "c", at(1)).unwrap();
    let before = on_disk(&s);
    assert_eq!(s.tray_pick("p1", at(5)).unwrap(), TrayPick::NoChange);
    assert_eq!(on_disk(&s), before);
}

#[test]
fn tray_pick_while_running_other_asks() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "t", "c", at(1)).unwrap();
    let id = running_id(&s, at(2));
    assert_eq!(
        s.tray_pick("p2", at(5)).unwrap(),
        TrayPick::ConfirmSwitch {
            running_entry: id.clone()
        }
    );
    assert_eq!(running_of(&s).id, id);
    assert_eq!(running_of(&s).project_id, "p1");
}

#[test]
fn tray_pick_archived_project_errors() {
    let mut s = open(seeded(), at(0));
    s.set_project_archived("p2", true, at(1)).unwrap();
    assert!(s.tray_pick("p2", at(5)).is_err());
    assert!(s.running().is_none());
}

#[test]
fn tray_switch_ends_current_with_draft_and_starts_next() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "舊", "舊內容", at(1)).unwrap();
    let id = running_id(&s, at(2));
    s.set_draft("面板文字".into(), "面板內容".into(), s.draft.generation);
    assert!(s.tray_switch("p2", &id, at(60)).unwrap());
    let old = entry_of(&s, &id);
    assert!(old.end.is_some());
    assert_eq!(
        (old.title.as_str(), old.content.as_str()),
        ("面板文字", "面板內容")
    );
    let e = running_of(&s);
    assert_eq!(e.project_id, "p2");
    assert_eq!((e.title.as_str(), e.content.as_str()), ("配色", ""));
    let log = log_text(&s);
    let sw = log.find("\tSWITCH\t").expect("SWITCH");
    let st = log.rfind("\tSTART\t").expect("START");
    assert!(sw < st);
}

#[test]
fn tray_switch_after_state_changed_is_noop() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "t", "c", at(1)).unwrap();
    let id = running_id(&s, at(2));
    assert!(!s.tray_switch("p2", "other-id", at(3)).unwrap());
    assert_eq!(running_of(&s).id, id);
    s.stop("t", "c", at(4)).unwrap();
    assert!(!s.tray_switch("p2", &id, at(5)).unwrap());
    assert!(s.running().is_none());
}

#[test]
fn tray_stop_uses_draft() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "t", "c", at(1)).unwrap();
    let id = running_id(&s, at(2));
    s.set_draft("未存的字".into(), "未存內容".into(), s.draft.generation);
    assert!(s.tray_stop(at(60)).unwrap());
    let e = entry_of(&s, &id);
    assert!(e.end.is_some());
    assert_eq!(
        (e.title.as_str(), e.content.as_str()),
        ("未存的字", "未存內容")
    );
}

#[test]
fn tray_stop_with_conflict_keeps_stored_text() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "舊標題", "舊內容", at(1)).unwrap();
    let id = running_id(&s, at(2));
    s.set_draft("打字中".into(), "打字中內容".into(), s.draft.generation);
    let mut f = s.entry_form(&id).unwrap();
    f.title = "別處改的".into();
    s.save_entry(Some(&id), f, at(30)).unwrap();
    assert!(s.draft_conflict);
    assert!(s.tray_stop(at(60)).unwrap());
    let e = entry_of(&s, &id);
    assert_eq!(
        (e.title.as_str(), e.content.as_str()),
        ("別處改的", "舊內容")
    );
}

#[test]
fn tray_stop_when_idle_is_noop() {
    let mut s = open(seeded(), at(0));
    assert!(!s.tray_stop(at(5)).unwrap());
}

#[test]
fn tray_pause_resume_are_idempotent() {
    let mut s = open(seeded(), at(0));
    assert!(!s.tray_pause(at(1)).unwrap());
    assert!(!s.tray_resume(at(1)).unwrap());
    s.start("p1", "t", "c", at(2)).unwrap();
    assert!(!s.tray_resume(at(3)).unwrap());
    assert!(s.tray_pause(at(4)).unwrap());
    assert!(!s.tray_pause(at(5)).unwrap());
    assert!(s.tray_resume(at(6)).unwrap());
    assert!(!s.tray_resume(at(7)).unwrap());
}
