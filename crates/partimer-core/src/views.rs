//! Serializable view types handed to the frontend (camelCase JSON).

use serde::{Deserialize, Serialize};

use crate::model::Id;

/// The panel's title/content draft. Lives in memory only; `generation` changes whenever the
/// draft is replaced from the backend, so the frontend can rebuild its inputs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub title: String,
    pub content: String,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TimerStatus {
    Idle,
    Running,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    pub id: Id,
    pub project_id: Id,
    pub project_name: String,
    pub client_name: String,
    pub color: u8,
    pub start_hhmm: String,
    pub end_hhmm: Option<String>,
    /// Local start date, `YYYY-MM-DD`.
    pub date: String,
    pub title: String,
    pub content: String,
    pub seconds: i64,
    pub paused_minutes: i64,
    /// `Idle` once the entry has ended.
    pub status: TimerStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOption {
    pub id: Id,
    pub name: String,
    pub color: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGroup {
    pub client_id: Id,
    pub client_name: String,
    pub projects: Vec<ProjectOption>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    Restored,
    CorruptNoBackup,
    SaveError,
    EditedElsewhere,
    Unreadable,
    Info,
}

impl NoticeKind {
    /// Warnings about data safety stay until the condition is gone.
    pub fn dismissable(self) -> bool {
        matches!(
            self,
            NoticeKind::Restored | NoticeKind::EditedElsewhere | NoticeKind::Info
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub id: u64,
    pub kind: NoticeKind,
    pub message: String,
    pub dismissable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelState {
    pub groups: Vec<ProjectGroup>,
    pub selected_project_id: Option<Id>,
    pub status: TimerStatus,
    pub running: Option<EntryView>,
    pub elapsed_seconds: i64,
    pub draft: Draft,
    pub draft_saved: bool,
    pub today: Vec<EntryView>,
    pub today_total_seconds: i64,
    pub notices: Vec<Notice>,
    pub long_running_prompt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordsQuery {
    pub year: i32,
    pub month: u32,
    pub client_id: Option<Id>,
    pub project_id: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTotalView {
    pub project_id: Id,
    pub label: String,
    pub color: u8,
    pub minutes: i64,
    pub hm: String,
    pub hours: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayView {
    pub date: String,
    /// 「週一」…「週日」.
    pub weekday: String,
    pub entries: Vec<EntryView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordsView {
    /// The query actually applied: filters pointing at deleted items are reset to `None`.
    pub query: RecordsQuery,
    pub clients: Vec<(Id, String)>,
    pub projects: Vec<ProjectOption>,
    pub total_minutes: i64,
    pub total_hm: String,
    pub total_hours: String,
    pub per_project: Vec<ProjectTotalView>,
    pub days: Vec<DayView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryForm {
    pub project_id: Id,
    /// Local `YYYY-MM-DDTHH:MM`.
    pub start_local: String,
    pub end_local: Option<String>,
    pub still_running: bool,
    pub paused_minutes: i64,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub id: Id,
    pub name: String,
    pub color: u8,
    pub archived: bool,
    pub entry_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientView {
    pub id: Id,
    pub name: String,
    pub archived: bool,
    pub projects: Vec<ProjectView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogView {
    pub clients: Vec<ClientView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Client,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Reminder {
    LongRunning { project_name: String, hours: i64 },
    Hourly { project_name: String, hours: i64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_kind_uses_lowercase_names() {
        assert_eq!(
            serde_json::to_string(&ItemKind::Client).unwrap(),
            "\"client\""
        );
        assert_eq!(
            serde_json::from_str::<ItemKind>("\"project\"").unwrap(),
            ItemKind::Project
        );
    }

    #[test]
    fn reminder_and_notice_serialize_camel_case() {
        let r = Reminder::LongRunning {
            project_name: "官網改版".into(),
            hours: 12,
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"kind":"longRunning","projectName":"官網改版","hours":12}"#
        );
        let n = Notice {
            id: 1,
            kind: NoticeKind::SaveError,
            message: "m".into(),
            dismissable: false,
        };
        assert_eq!(
            serde_json::to_string(&n).unwrap(),
            r#"{"id":1,"kind":"saveError","message":"m","dismissable":false}"#
        );
    }

    #[test]
    fn entry_form_round_trips() {
        let json = r#"{"projectId":"p1","startLocal":"2026-10-06T09:00","endLocal":null,
            "stillRunning":true,"pausedMinutes":0,"title":"t","content":"c"}"#;
        let f: EntryForm = serde_json::from_str(json).unwrap();
        assert!(f.still_running);
        let q: RecordsQuery =
            serde_json::from_str(r#"{"year":2026,"month":10,"clientId":null,"projectId":"p1"}"#)
                .unwrap();
        assert_eq!(q.project_id.as_deref(), Some("p1"));
    }
}
