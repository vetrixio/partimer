use serde_json::json;

use super::*;
use crate::session::moving::SAVE_BLOCKED_MOVE;

/// A catalog different from `base_data`, so a transfer visibly replaces it.
fn other_data() -> Data {
    let mut d = Data::default();
    d.clients.push(Client {
        extra: Default::default(),
        id: "c9".into(),
        name: "別人".into(),
        archived: false,
    });
    d.projects.push(Project {
        extra: Default::default(),
        id: "p9".into(),
        client_id: "c9".into(),
        name: "舊案".into(),
        color: 2,
        archived: false,
    });
    d.entries.push(entry(
        "e9",
        "p9",
        "2026-09-01T01:00:00Z",
        Some("2026-09-01T02:00:00Z"),
        "舊的",
    ));
    d
}

fn rich_data() -> Data {
    let mut d = base_data();
    d.settings.widget.x = Some(120);
    d.settings.widget.y = Some(80);
    d.extra.insert("futureField".into(), json!({"a": 1}));
    d.quarantined_entries
        .push(json!({"id": "bad", "oops": true}));
    d
}

fn snapshot_names(s: &Session<MemFs>) -> Vec<PathBuf> {
    s.fs.names()
        .into_iter()
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("data.before-transfer-")
        })
        .collect()
}

fn transfer(from: &Session<MemFs>, to: &mut Session<MemFs>, now: DateTime<Utc>) -> String {
    let bytes = from.transfer_out(now).unwrap();
    let plan = to.prepare_transfer_in(&bytes).unwrap();
    to.transfer_in(plan, now).unwrap()
}

#[test]
fn transfer_out_refuses_while_running() {
    let mut s = open(seeded(), at(0));
    s.start("p1", "A", "", at(0)).unwrap();
    assert_eq!(
        s.transfer_out(at(10)),
        Err("請先按 STOP 結束計時再轉出".to_string())
    );
    s.pause(at(20)).unwrap();
    assert!(s.transfer_out(at(30)).is_err());
}

#[test]
fn transfer_out_is_data_json_bytes() {
    let s = open(seeded(), at(0));
    assert_eq!(s.transfer_out(at(0)).unwrap(), to_json(&base_data()));
    s.note_transfer_out(at(1));
    assert_eq!(count_action(&s, "TRANSFER_OUT"), 1);
    assert!(log_text(&s).contains("entries=2"));
}

#[test]
fn transfer_round_trip_replaces_everything() {
    let a = open(seed(&rich_data()), at(0));
    let mut b = open(seed(&other_data()), at(0));
    transfer(&a, &mut b, at(10));
    assert_eq!(on_disk(&b), rich_data());
    assert_eq!(b.data, rich_data());
}

#[test]
fn transfer_in_into_empty_skips_snapshot() {
    let a = open(seeded(), at(0));
    let mut b = open(MemFs::new(), at(0));
    let msg = transfer(&a, &mut b, at(10));
    assert_eq!(msg, "已轉入 2 筆紀錄。");
    assert!(snapshot_names(&b).is_empty());
    assert_eq!(on_disk(&b), base_data());
}

#[test]
fn transfer_in_snapshots_current_memory() {
    let a = open(seeded(), at(0));
    let mut b = open(seed(&other_data()), at(0));
    b.fs.fail_renames.set(true);
    b.rename_client("c9", "改名後", at(1)).unwrap();
    assert!(b.dirty);
    assert_eq!(on_disk(&b).clients[0].name, "別人");
    let memory = b.data.clone();
    b.fs.fail_renames.set(false);
    let msg = transfer(&a, &mut b, at(10));
    let snaps = snapshot_names(&b);
    assert_eq!(snaps.len(), 1);
    let snap = parse_data(&b.fs.get(&snaps[0]).unwrap()).unwrap();
    assert_eq!(snap, memory);
    assert_eq!(snap.clients[0].name, "改名後");
    let name = snaps[0].file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(msg, format!("已轉入 2 筆紀錄。原本的資料另存為 {name}。"));
}

#[test]
fn transfer_in_snapshot_failure_changes_nothing() {
    let a = open(seeded(), at(0));
    let mut b = open(seed(&other_data()), at(0));
    let bytes = a.transfer_out(at(0)).unwrap();
    let plan = b.prepare_transfer_in(&bytes).unwrap();
    b.fs.fail_write_syncs.set(true);
    let err = b.transfer_in(plan, at(10)).unwrap_err();
    assert!(err.starts_with("無法另存目前的資料，沒有轉入（"), "{err}");
    assert_eq!(b.data, other_data());
    assert_eq!(on_disk(&b), other_data());
    assert_eq!(count_action(&b, "TRANSFER_IN"), 0);
}

#[test]
fn transfer_in_while_running_drops_entry_without_stop() {
    let a = open(seeded(), at(0));
    let mut b = open(seeded(), at(0));
    b.start("p1", "A", "", at(0)).unwrap();
    assert!(!b.tick(at(13 * H)).is_empty());
    let stops = count_action(&b, "STOP");
    transfer(&a, &mut b, at(13 * H + 1));
    assert_eq!(b.timer_status(at(13 * H + 2)).0, TimerStatus::Idle);
    assert_eq!(count_action(&b, "STOP"), stops);
    assert!(b.tick(at(26 * H)).is_empty());
    assert!(!b.panel_state(at(26 * H)).long_running_prompt);
}

#[test]
fn transfer_in_resets_draft_and_conflict() {
    let a = open(seeded(), at(0));
    let mut b = open(seeded(), at(0));
    b.start("p1", "A", "", at(0)).unwrap();
    let id = running_id(&b, at(1));
    b.set_draft("A".into(), "打字中".into(), b.draft.generation);
    let mut f = b.entry_form(&id).unwrap();
    f.title = "A2".into();
    b.save_entry(Some(&id), f, at(60)).unwrap();
    assert!(notice(&b, NoticeKind::EditedElsewhere).is_some());
    let generation = b.draft.generation;
    transfer(&a, &mut b, at(100));
    assert!(b.draft.generation > generation);
    assert_eq!(b.draft.title, b.selected_title());
    assert_eq!(b.draft.content, "");
    assert!(notice(&b, NoticeKind::EditedElsewhere).is_none());
}

#[test]
fn transfer_in_refused_when_save_blocked() {
    let mut d = base_data();
    d.version = 2;
    let mut s = open(seed(&d), at(0));
    let bytes = open(seeded(), at(0)).transfer_out(at(0)).unwrap();
    let plan = s.prepare_transfer_in(&bytes).unwrap();
    assert_eq!(
        s.transfer_in(plan, at(1)),
        Err(SAVE_BLOCKED_MOVE.to_string())
    );
}

#[test]
fn confirm_text_mentions_replacement_and_running() {
    let a = open(seeded(), at(0));
    let bytes = a.transfer_out(at(0)).unwrap();
    let mut b = open(seeded(), at(0));
    let text = b.prepare_transfer_in(&bytes).unwrap().confirm_text;
    assert!(text.starts_with("將轉入 1 個客戶、2 個專案、2 筆紀錄（2026-10-05 ～ 2026-10-05）。"));
    assert!(text.contains("這台電腦目前的 1 個客戶、2 個專案、2 筆紀錄會全部被取代。"));
    assert!(text.contains("data.before-transfer-"));
    assert!(!text.contains("計時中"));
    b.start("p1", "A", "", at(0)).unwrap();
    let text = b.prepare_transfer_in(&bytes).unwrap().confirm_text;
    assert!(text.ends_with("\n計時中的那筆也會被取代。"), "{text}");

    let empty = open(MemFs::new(), at(0));
    let text = empty.prepare_transfer_in(&bytes).unwrap().confirm_text;
    assert!(!text.contains("會全部被取代"));
}

#[test]
fn prepare_rejects_bad_files_with_user_message() {
    let s = open(seeded(), at(0));
    let err = s.prepare_transfer_in(b"nope").unwrap_err();
    assert_eq!(
        err,
        crate::transfer::TransferError::NotTransferFile.to_string()
    );
}

#[test]
fn snapshot_can_be_transferred_back_in() {
    let a = open(seeded(), at(0));
    let mut b = open(seed(&other_data()), at(0));
    let before = b.data.clone();
    transfer(&a, &mut b, at(10));
    let snaps = snapshot_names(&b);
    let plan = b
        .prepare_transfer_in(&b.fs.get(&snaps[0]).unwrap())
        .unwrap();
    b.transfer_in(plan, at(20)).unwrap();
    assert_eq!(on_disk(&b), before);
}

#[test]
fn transfer_in_logs_counts() {
    let a = open(seeded(), at(0));
    let mut b = open(seed(&other_data()), at(0));
    transfer(&a, &mut b, at(10));
    assert_eq!(count_action(&b, "TRANSFER_IN"), 1);
    let log = log_text(&b);
    assert!(log.contains("entries=2"));
    assert!(log.contains("replaced=1"));
    assert!(log.contains("snapshot=data.before-transfer-"));
}
