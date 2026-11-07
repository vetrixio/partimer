//! `Session`: the single API the Tauri shell calls. Owns the data, the in-memory draft,
//! save retry, reminders and notices. Every data change goes: mutate -> activity log -> persist.

use std::path::Path;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

use crate::activity::{self, Action};
use crate::catalog::{self, CatalogError, DeletionImpact};
use crate::duration::{effective_seconds, format_hm, hours_decimal, round_minutes};
use crate::edit::{self, EntryPatch};
use crate::model::{Data, Entry, Id, Settings};
use crate::report::{self, Filter};
use crate::storage::{self, Fs, LoadNotice, Paths, UnreadableReason};
use crate::timer::{self, TimerError};
use crate::views::*;
use crate::zone::{LocalZone, format_hhmm, format_local_input, local_date, parse_local_input};

const LONG_RUNNING_SECS: i64 = 12 * 3600;
const WEEKDAYS: [&str; 7] = ["週一", "週二", "週三", "週四", "週五", "週六", "週日"];
const EDITED_ELSEWHERE: &str =
    "這筆已在紀錄視窗被修改，面板上的文字尚未儲存（按儲存才會寫入，結束 app 時不會自動存入）";
const ENDED_ELSEWHERE: &str = "這筆已在紀錄視窗結束，面板上未儲存的文字沒有寫入";
const REPAIRED_RUNNING: &str =
    "發現多筆同時在計時的紀錄，已保留最新的一筆，其餘已在下一筆開始時結束，請到紀錄視窗確認。";
const REPAIRED_RUNNING_NOTE: &str = "repaired-multiple-running";
const ARCHIVED_PROJECT: &str = "這個專案已封存，請先在設定中取消封存";

fn msg(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn ymd(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Notice for a running entry finished in the records window while the panel had unsaved
/// text; the text itself is quoted so the user can copy it into the records window.
fn ended_elsewhere_message(title: &str, content: &str) -> String {
    let mut parts = Vec::new();
    if !title.is_empty() {
        parts.push(format!("標題「{title}」"));
    }
    if !content.is_empty() {
        parts.push(format!("內容：{content}"));
    }
    if parts.is_empty() {
        format!("{ENDED_ELSEWHERE}，請到紀錄視窗補寫。")
    } else {
        format!(
            "{ENDED_ELSEWHERE}。原文：{}（請到紀錄視窗補寫）",
            parts.join("，")
        )
    }
}

fn project_option(p: &crate::model::Project) -> ProjectOption {
    ProjectOption {
        id: p.id.clone(),
        name: p.name.clone(),
        color: p.color,
    }
}

fn status_of(e: &Entry) -> TimerStatus {
    if e.end.is_some() {
        TimerStatus::Idle
    } else if e.pause_started_at.is_some() {
        TimerStatus::Paused
    } else {
        TimerStatus::Running
    }
}

fn to_filter(q: &RecordsQuery) -> Filter {
    Filter {
        year: q.year,
        month: q.month,
        client_id: q.client_id.clone(),
        project_id: q.project_id.clone(),
    }
}

pub struct Session<F: Fs> {
    fs: F,
    paths: Paths,
    zone: Box<dyn LocalZone>,
    data: Data,
    draft: Draft,
    /// The running entry was edited in the records window while the draft had unsaved text.
    /// Outlives the notice (dismissing it decides nothing); cleared only by the user's own
    /// resolution: saving, a new draft (stop/start/switch/reset), or typing back to the
    /// stored text. While set, quitting never auto-saves the draft (spec §5.4).
    draft_conflict: bool,
    /// Changes that are not on disk yet.
    dirty: bool,
    /// `data.json` could not be read or is from a newer version: never write it this run.
    save_blocked: bool,
    last_backup_date: Option<NaiveDate>,
    notices: Vec<Notice>,
    next_notice_id: u64,
    /// The 12h prompt shows once effective time reaches `12h * (snoozes + 1)`.
    snoozes: i64,
    /// Whether `tick` already returned a reminder for the prompt currently showing.
    long_prompt_reported: bool,
    last_hour_notified: i64,
}

impl<F: Fs> Session<F> {
    /// Load (recovering from backups), take the daily backup, turn load problems into notices
    /// and prepare the draft.
    pub fn open(fs: F, paths: Paths, zone: Box<dyn LocalZone>, now: DateTime<Utc>) -> Self {
        let loaded = storage::load(&fs, &paths, zone.as_ref(), now);
        let mut s = Session {
            fs,
            paths,
            zone,
            data: loaded.data,
            draft: Draft::default(),
            draft_conflict: false,
            dirty: false,
            save_blocked: loaded.save_blocked,
            last_backup_date: None,
            notices: Vec::new(),
            next_notice_id: 1,
            snoozes: 0,
            long_prompt_reported: false,
            last_hour_notified: 0,
        };
        if let Some(err) = loaded.write_back_error {
            s.dirty = true;
            s.set_save_error(&err);
        }
        if !s.save_blocked {
            s.backup_if_new_day(now);
        }
        if let Some(n) = loaded.notice {
            s.report_load_notice(n, now);
        }
        if let Some(e) = loaded.backup_error {
            s.log(
                now,
                Action::Restore,
                None,
                Some(&format!("backups unreadable: {e}")),
            );
            s.push_notice(
                NoticeKind::Info,
                format!("無法讀取備份資料夾（{e}），這次沒辦法從備份還原。"),
            );
        }
        s.report_quarantined(now);
        s.repair_multiple_running(now);
        s.draft = match s.running() {
            Some(e) => Draft {
                title: e.title.clone(),
                content: e.content.clone(),
                generation: 0,
            },
            None => Draft {
                title: s.selected_title(),
                content: String::new(),
                generation: 0,
            },
        };
        s.reset_reminders(now);
        s
    }

    pub fn panel_state(&self, now: DateTime<Utc>) -> PanelState {
        let z = self.zone.as_ref();
        let running = self.running();
        PanelState {
            groups: self.panel_groups(),
            selected_project_id: self.selected_project_id(),
            status: running.map_or(TimerStatus::Idle, status_of),
            running: running.map(|e| self.entry_view(e, now)),
            elapsed_seconds: running.map_or(0, |e| effective_seconds(e, now)),
            draft: self.draft.clone(),
            draft_saved: self.draft_saved(),
            today: report::today_entries(&self.data, z, now)
                .into_iter()
                .map(|e| self.entry_view(e, now))
                .collect(),
            today_total_seconds: report::today_total_seconds(&self.data, z, now),
            notices: self.notices.clone(),
            long_running_prompt: self.long_running_prompt(now),
        }
    }

    /// Cheap status for callers that need no lists: status and elapsed seconds.
    pub fn timer_status(&self, now: DateTime<Utc>) -> (TimerStatus, i64) {
        self.running().map_or((TimerStatus::Idle, 0), |e| {
            (status_of(e), effective_seconds(e, now))
        })
    }

    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }

    /// Memory only: never writes the file and keeps the generation. `generation` is the one
    /// the caller's fields were built from; a sync from fields that were since replaced (e.g.
    /// arriving after STOP) is ignored, so old text never becomes the next entry's draft.
    pub fn set_draft(&mut self, title: String, content: String, generation: u64) {
        if generation != self.draft.generation {
            return;
        }
        self.draft.title = title;
        self.draft.content = content;
        if self.draft_conflict && self.draft_saved() {
            self.clear_draft_conflict();
        }
    }

    pub fn start(
        &mut self,
        project_id: &str,
        title: &str,
        content: &str,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        let id = timer::start(&mut self.data, project_id, title, content, now).map_err(msg)?;
        self.log_entry(now, Action::Start, &id);
        self.after_start(now);
        self.persist(now);
        Ok(())
    }

    /// Writes title/content into the running entry and ends it in one step.
    pub fn stop(&mut self, title: &str, content: &str, now: DateTime<Utc>) -> Result<(), String> {
        let id = timer::stop(&mut self.data, title, content, now).map_err(msg)?;
        self.after_end(&id, Action::Stop, now);
        Ok(())
    }

    pub fn save_draft(
        &mut self,
        title: &str,
        content: &str,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        let e = timer::running_mut(&mut self.data).ok_or_else(|| msg(TimerError::NotRunning))?;
        e.title = title.to_string();
        e.content = content.to_string();
        let id = e.id.clone();
        self.draft.title = title.to_string();
        self.draft.content = content.to_string();
        self.clear_draft_conflict();
        self.log_entry(now, Action::Save, &id);
        self.persist(now);
        Ok(())
    }

    pub fn pause(&mut self, now: DateTime<Utc>) -> Result<(), String> {
        let id = timer::pause(&mut self.data, now).map_err(msg)?;
        self.log_entry(now, Action::Pause, &id);
        self.persist(now);
        Ok(())
    }

    pub fn resume(&mut self, now: DateTime<Utc>) -> Result<(), String> {
        let id = timer::resume(&mut self.data, now).map_err(msg)?;
        self.log_entry(now, Action::Resume, &id);
        self.persist(now);
        Ok(())
    }

    /// Running: end the current entry with the given text and start the project right away.
    /// Idle: remember the project and prefill its last title, keeping the typed content.
    pub fn select_project(
        &mut self,
        project_id: &str,
        title: &str,
        content: &str,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        if !self.data.projects.iter().any(|p| p.id == project_id) {
            return Err(msg(TimerError::UnknownProject));
        }
        let running_project = self.running().map(|e| e.project_id.clone());
        // Only what the panel lists can be picked: archived projects (or clients) are hidden,
        // except the running one.
        let visible = catalog::active_groups(&self.data)
            .iter()
            .any(|(_, ps)| ps.iter().any(|p| p.id == project_id));
        if !visible && running_project.as_deref() != Some(project_id) {
            return Err(ARCHIVED_PROJECT.to_string());
        }
        match running_project {
            Some(current) if current == project_id => Ok(()),
            Some(_) => {
                let old = timer::stop(&mut self.data, title, content, now).map_err(msg)?;
                self.log_entry(now, Action::Switch, &old);
                let next_title = timer::last_title_for_project(&self.data, project_id);
                match timer::start(&mut self.data, project_id, &next_title, "", now) {
                    Ok(new) => {
                        self.log_entry(now, Action::Start, &new);
                        self.after_start(now);
                        self.persist(now);
                        Ok(())
                    }
                    // Unreachable in practice (project checked, nothing running), but keep the
                    // finished entry saved and the draft detached from it.
                    Err(e) => {
                        self.reset_draft_after_end();
                        self.persist(now);
                        Err(msg(e))
                    }
                }
            }
            None => {
                if self.selected_project_id().as_deref() != Some(project_id) {
                    let t = timer::last_title_for_project(&self.data, project_id);
                    self.replace_draft(t, content.to_string());
                }
                if self.data.settings.last_project_id.as_deref() != Some(project_id) {
                    self.data.settings.last_project_id = Some(project_id.to_string());
                    self.persist(now);
                }
                Ok(())
            }
        }
    }

    pub fn add_project(
        &mut self,
        client: &str,
        project: &str,
        title: &str,
        content: &str,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        let id = catalog::add_project(&mut self.data, client, project).map_err(msg)?;
        self.select_project(&id, title, content, now)
    }

    /// 12h prompt: write the text, then end the running entry at a chosen local time.
    pub fn stop_at_local(
        &mut self,
        title: &str,
        content: &str,
        end_local: &str,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        let end = parse_local_input(self.zone.as_ref(), end_local).map_err(msg)?;
        let id = timer::stop_at(&mut self.data, end, now).map_err(msg)?;
        if let Some(e) = self.data.entries.iter_mut().find(|e| e.id == id) {
            e.title = title.to_string();
            e.content = content.to_string();
        }
        self.after_end(&id, Action::AutoStop, now);
        Ok(())
    }

    /// "沒忘，繼續計時": next prompt at the next multiple of 12h. Ignored when no prompt shows.
    pub fn snooze_long_running(&mut self, now: DateTime<Utc>) {
        if !self.long_running_prompt(now) {
            return;
        }
        if let Some(e) = self.running() {
            self.snoozes = effective_seconds(e, now) / LONG_RUNNING_SECS;
            self.long_prompt_reported = false;
        }
    }

    /// Data-safety warnings (save error, corrupt, unreadable) cannot be dismissed.
    pub fn dismiss_notice(&mut self, id: u64) {
        self.notices.retain(|n| n.id != id || !n.dismissable);
    }

    pub fn push_notice(&mut self, kind: NoticeKind, message: String) {
        let id = self.next_notice_id;
        self.next_notice_id += 1;
        self.notices.push(Notice {
            id,
            kind,
            message,
            dismissable: kind.dismissable(),
        });
    }

    pub fn records(&self, q: RecordsQuery, now: DateTime<Utc>) -> RecordsView {
        let z = self.zone.as_ref();
        let query = self.fix_query(q);
        let filter = to_filter(&query);
        let summary = report::summarize(&self.data, &filter, z);

        let mut clients: Vec<(Id, String)> = self
            .data
            .clients
            .iter()
            .map(|c| (c.id.clone(), c.name.clone()))
            .collect();
        clients.sort_by(|a, b| a.1.cmp(&b.1));

        let mut projects: Vec<ProjectOption> = self
            .data
            .projects
            .iter()
            .filter(|p| query.client_id.as_ref().is_none_or(|c| &p.client_id == c))
            .map(project_option)
            .collect();
        projects.sort_by(|a, b| a.name.cmp(&b.name));

        let per_project = summary
            .per_project
            .iter()
            .map(|(pid, minutes)| {
                let (project, client, color) = self.labels(pid);
                ProjectTotalView {
                    project_id: pid.clone(),
                    label: format!("{client} / {project}"),
                    color,
                    minutes: *minutes,
                    hm: format_hm(*minutes),
                    hours: hours_decimal(*minutes),
                }
            })
            .collect();

        let days = report::day_groups(&self.data, &filter, z)
            .into_iter()
            .map(|(d, entries)| DayView {
                date: ymd(d),
                weekday: WEEKDAYS[d.weekday().num_days_from_monday() as usize].to_string(),
                entries: entries
                    .into_iter()
                    .map(|e| self.entry_view(e, now))
                    .collect(),
            })
            .collect();

        RecordsView {
            query,
            clients,
            projects,
            total_minutes: summary.total_minutes,
            total_hm: format_hm(summary.total_minutes),
            total_hours: hours_decimal(summary.total_minutes),
            per_project,
            days,
        }
    }

    pub fn entry_form(&self, id: &str) -> Option<EntryForm> {
        let z = self.zone.as_ref();
        let e = self.data.entries.iter().find(|e| e.id == id)?;
        Some(EntryForm {
            project_id: e.project_id.clone(),
            start_local: format_local_input(z, e.start),
            end_local: e.end.map(|t| format_local_input(z, t)),
            still_running: e.end.is_none(),
            paused_minutes: round_minutes(e.paused_seconds),
            title: e.title.clone(),
            content: e.content.clone(),
        })
    }

    /// Manual entry defaults: selected project, the last hour.
    pub fn new_entry_form(&self, now: DateTime<Utc>) -> EntryForm {
        let z = self.zone.as_ref();
        EntryForm {
            project_id: self.selected_project_id().unwrap_or_default(),
            start_local: format_local_input(z, now - Duration::hours(1)),
            end_local: Some(format_local_input(z, now)),
            still_running: false,
            paused_minutes: 0,
            title: String::new(),
            content: String::new(),
        }
    }

    /// `id = None` adds an entry. Form fields left as shown keep the stored value, so editing
    /// only the text never shifts seconds. An empty `end_local` counts as no end.
    pub fn save_entry(
        &mut self,
        id: Option<&str>,
        form: EntryForm,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        let before = id
            .and_then(|id| self.data.entries.iter().find(|e| e.id == id))
            .cloned();
        let z = self.zone.as_ref();
        let time = |s: &str, stored: Option<DateTime<Utc>>| match stored {
            Some(t) if format_local_input(z, t) == s => Ok(t),
            _ => parse_local_input(z, s).map_err(msg),
        };
        let start = time(&form.start_local, before.as_ref().map(|e| e.start))?;
        let end = match form.end_local.as_deref().map(str::trim) {
            Some(s) if !s.is_empty() => Some(time(s, before.as_ref().and_then(|e| e.end))?),
            _ => None,
        };
        let patch = EntryPatch {
            project_id: form.project_id,
            start,
            end,
            still_running: form.still_running,
            paused_minutes: form.paused_minutes,
            title: form.title,
            content: form.content,
        };
        match (id, before) {
            (Some(id), Some(before)) => {
                edit::apply_edit(&mut self.data, id, patch, now).map_err(msg)?;
                self.keep_paused_seconds(id, before.paused_seconds, form.paused_minutes, now);
                self.log_entry(now, Action::Edit, id);
                if before.end.is_none() {
                    self.after_running_edited(&before);
                }
            }
            (Some(_), None) => return Err(msg(edit::EditError::NotFound)),
            (None, _) => {
                let new = edit::add_entry(&mut self.data, patch, now).map_err(msg)?;
                self.log_entry(now, Action::Add, &new);
            }
        }
        self.persist(now);
        Ok(())
    }

    pub fn delete_entry(&mut self, id: &str, now: DateTime<Utc>) -> Result<(), String> {
        let removed = edit::delete_entry(&mut self.data, id).map_err(msg)?;
        self.log(now, Action::Delete, Some(&removed), None);
        if removed.end.is_none() {
            self.reset_draft_after_end();
        }
        self.persist(now);
        Ok(())
    }

    pub fn catalog(&self) -> CatalogView {
        let mut clients: Vec<ClientView> = self
            .data
            .clients
            .iter()
            .map(|c| {
                let mut projects: Vec<ProjectView> = self
                    .data
                    .projects
                    .iter()
                    .filter(|p| p.client_id == c.id)
                    .map(|p| ProjectView {
                        id: p.id.clone(),
                        name: p.name.clone(),
                        color: p.color,
                        archived: p.archived,
                        entry_count: self
                            .data
                            .entries
                            .iter()
                            .filter(|e| e.project_id == p.id)
                            .count(),
                    })
                    .collect();
                projects.sort_by(|a, b| a.name.cmp(&b.name));
                ClientView {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    archived: c.archived,
                    projects,
                }
            })
            .collect();
        clients.sort_by(|a, b| a.name.cmp(&b.name));
        CatalogView { clients }
    }

    pub fn rename_client(
        &mut self,
        id: &str,
        name: &str,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        self.catalog_change(now, |d| catalog::rename_client(d, id, name))
    }

    pub fn rename_project(
        &mut self,
        id: &str,
        name: &str,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        self.catalog_change(now, |d| catalog::rename_project(d, id, name))
    }

    pub fn set_project_color(
        &mut self,
        id: &str,
        color: u8,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        self.catalog_change(now, |d| catalog::set_project_color(d, id, color))
    }

    pub fn set_client_archived(
        &mut self,
        id: &str,
        archived: bool,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        self.catalog_change(now, |d| catalog::set_client_archived(d, id, archived))
    }

    pub fn set_project_archived(
        &mut self,
        id: &str,
        archived: bool,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        self.catalog_change(now, |d| catalog::set_project_archived(d, id, archived))
    }

    pub fn deletion_impact(
        &self,
        kind: ItemKind,
        id: &str,
        now: DateTime<Utc>,
    ) -> Result<DeletionImpact, String> {
        match kind {
            ItemKind::Client => catalog::client_deletion_impact(&self.data, id, now),
            ItemKind::Project => catalog::project_deletion_impact(&self.data, id, now),
        }
        .map_err(msg)
    }

    /// Deletes a client or project with all its entries (one `DELETE` line per entry).
    pub fn delete_item(
        &mut self,
        kind: ItemKind,
        id: &str,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        let selected_before = self.selected_project_id();
        let removed = match kind {
            ItemKind::Client => catalog::delete_client(&mut self.data, id),
            ItemKind::Project => catalog::delete_project(&mut self.data, id),
        }
        .map_err(msg)?;
        for e in &removed {
            self.log(now, Action::Delete, Some(e), None);
        }
        if removed.iter().any(|e| e.end.is_none()) {
            self.reset_draft_after_end();
        } else {
            self.follow_selection(selected_before);
        }
        self.persist(now);
        Ok(())
    }

    pub fn settings(&self) -> &Settings {
        &self.data.settings
    }

    pub fn update_settings(&mut self, f: impl FnOnce(&mut Settings), now: DateTime<Utc>) {
        f(&mut self.data.settings);
        self.persist(now);
    }

    pub fn export_csv(&self, q: RecordsQuery) -> Vec<u8> {
        let filter = to_filter(&self.fix_query(q));
        report::export_csv(&self.data, &filter, self.zone.as_ref())
    }

    /// Called every second: retry a pending save, then check reminders.
    pub fn tick(&mut self, now: DateTime<Utc>) -> Vec<Reminder> {
        if self.dirty && !self.save_blocked {
            self.persist(now);
        }
        let mut out = Vec::new();
        let Some(e) = self.running() else {
            self.long_prompt_reported = false;
            return out;
        };
        let hours = effective_seconds(e, now) / 3600;
        let paused = e.pause_started_at.is_some();
        let (project_name, _, _) = self.labels(&e.project_id);

        let prompt = self.long_running_prompt(now);
        if prompt && !self.long_prompt_reported {
            out.push(Reminder::LongRunning {
                project_name: project_name.clone(),
                hours,
            });
        }
        self.long_prompt_reported = prompt;

        if !paused {
            if hours > self.last_hour_notified && self.data.settings.hourly_reminder {
                out.push(Reminder::Hourly {
                    project_name,
                    hours,
                });
            }
            // Tracked even when the setting is off, so turning it on never backfills.
            self.last_hour_notified = hours;
        }
        out
    }

    /// On quit: save an unsaved draft into the running entry, then try one last save.
    /// Not while the draft conflicts with a records-window edit, even if its notice was
    /// dismissed: only 儲存 decides, and saving here would silently overwrite that edit.
    pub fn flush_on_exit(&mut self, now: DateTime<Utc>) {
        if !self.draft_saved() && !self.draft_conflict {
            let (title, content) = (self.draft.title.clone(), self.draft.content.clone());
            let _ = self.save_draft(&title, &content, now);
        }
        if self.dirty {
            self.persist(now);
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.paths.dir
    }

    // ---- internals ----

    fn running(&self) -> Option<&Entry> {
        timer::running(&self.data)
    }

    /// Running entry's project; else the last used project if still visible in the panel;
    /// else the first visible project.
    fn selected_project_id(&self) -> Option<Id> {
        if let Some(e) = self.running() {
            return Some(e.project_id.clone());
        }
        let groups = catalog::active_groups(&self.data);
        let visible = |id: &str| groups.iter().any(|(_, ps)| ps.iter().any(|p| p.id == id));
        if let Some(id) = &self.data.settings.last_project_id
            && visible(id)
        {
            return Some(id.clone());
        }
        groups
            .first()
            .and_then(|(_, ps)| ps.first())
            .map(|p| p.id.clone())
    }

    fn selected_title(&self) -> String {
        self.selected_project_id()
            .map(|p| timer::last_title_for_project(&self.data, &p))
            .unwrap_or_default()
    }

    /// Visible groups, plus the running project even when it is archived.
    fn panel_groups(&self) -> Vec<ProjectGroup> {
        let mut groups: Vec<ProjectGroup> = catalog::active_groups(&self.data)
            .into_iter()
            .map(|(c, ps)| ProjectGroup {
                client_id: c.id.clone(),
                client_name: c.name.clone(),
                projects: ps.into_iter().map(project_option).collect(),
            })
            .collect();
        let hidden_running = self
            .running()
            .filter(|e| {
                !groups
                    .iter()
                    .any(|g| g.projects.iter().any(|p| p.id == e.project_id))
            })
            .and_then(|e| self.data.projects.iter().find(|p| p.id == e.project_id));
        if let Some(p) = hidden_running {
            match groups.iter_mut().find(|g| g.client_id == p.client_id) {
                Some(g) => g.projects.push(project_option(p)),
                None => groups.push(ProjectGroup {
                    client_id: p.client_id.clone(),
                    client_name: self.labels(&p.id).1,
                    projects: vec![project_option(p)],
                }),
            }
        }
        groups
    }

    /// (project name, client name, color); empty names for unknown ids.
    fn labels(&self, project_id: &str) -> (String, String, u8) {
        let p = self.data.projects.iter().find(|p| p.id == project_id);
        let c = p.and_then(|p| self.data.clients.iter().find(|c| c.id == p.client_id));
        (
            p.map(|p| p.name.clone()).unwrap_or_default(),
            c.map(|c| c.name.clone()).unwrap_or_default(),
            p.map_or(0, |p| p.color),
        )
    }

    fn entry_view(&self, e: &Entry, now: DateTime<Utc>) -> EntryView {
        let z = self.zone.as_ref();
        let (project_name, client_name, color) = self.labels(&e.project_id);
        EntryView {
            id: e.id.clone(),
            project_id: e.project_id.clone(),
            project_name,
            client_name,
            color,
            start_hhmm: format_hhmm(z, e.start),
            end_hhmm: e.end.map(|t| format_hhmm(z, t)),
            date: ymd(local_date(z, e.start)),
            title: e.title.clone(),
            content: e.content.clone(),
            seconds: effective_seconds(e, now),
            paused_minutes: round_minutes(e.paused_seconds),
            status: status_of(e),
        }
    }

    /// Filters pointing at deleted clients/projects fall back to "all".
    fn fix_query(&self, mut q: RecordsQuery) -> RecordsQuery {
        if q.client_id
            .as_ref()
            .is_some_and(|id| !self.data.clients.iter().any(|c| &c.id == id))
        {
            q.client_id = None;
        }
        if q.project_id
            .as_ref()
            .is_some_and(|id| !self.data.projects.iter().any(|p| &p.id == id))
        {
            q.project_id = None;
        }
        q
    }

    fn draft_saved(&self) -> bool {
        self.running()
            .is_none_or(|e| e.title == self.draft.title && e.content == self.draft.content)
    }

    /// A new draft replaces any conflicting text (stop/start/switch/reset), which ends the
    /// conflict and its notice.
    fn replace_draft(&mut self, title: String, content: String) {
        self.draft = Draft {
            title,
            content,
            generation: self.draft.generation + 1,
        };
        if self.draft_conflict {
            self.clear_draft_conflict();
        }
    }

    fn clear_draft_conflict(&mut self) {
        self.draft_conflict = false;
        self.notices
            .retain(|n| n.kind != NoticeKind::EditedElsewhere);
    }

    /// The fields now belong to the next entry: selected project's last title, empty content.
    fn reset_draft_after_end(&mut self) {
        let title = self.selected_title();
        self.replace_draft(title, String::new());
    }

    fn after_start(&mut self, now: DateTime<Utc>) {
        if let Some(e) = self.running() {
            let (title, content) = (e.title.clone(), e.content.clone());
            self.replace_draft(title, content);
        }
        self.reset_reminders(now);
    }

    /// Shared tail of stop / auto-stop: remember the project, log, detach the draft, save.
    fn after_end(&mut self, id: &str, action: Action, now: DateTime<Utc>) {
        if let Some(e) = self.data.entries.iter().find(|e| e.id == id) {
            self.data.settings.last_project_id = Some(e.project_id.clone());
        }
        self.log_entry(now, action, id);
        self.reset_draft_after_end();
        self.persist(now);
    }

    /// The running entry was edited in the records window (`before` = its state before).
    /// Still running: a clean draft follows the edit, a dirty one is kept with a notice.
    /// Finished: the fields always become the next entry's draft (spec §5.3); unsaved text
    /// is only shown in the notice so it is not lost.
    fn after_running_edited(&mut self, before: &Entry) {
        let clean = self.draft.title == before.title && self.draft.content == before.content;
        let still_running = self.running().filter(|e| e.id == before.id).cloned();
        match still_running {
            Some(e) if !clean => {
                if self.draft.title == e.title && self.draft.content == e.content {
                    // The records window saved exactly the panel's text: nothing to decide.
                    self.clear_draft_conflict();
                } else {
                    self.draft_conflict = true;
                    self.upsert_notice(NoticeKind::EditedElsewhere, EDITED_ELSEWHERE.to_string());
                }
            }
            Some(e) => {
                self.clear_draft_conflict();
                self.replace_draft(e.title, e.content);
            }
            None => {
                // Superseded by the "ended elsewhere" notice below (same kind).
                self.draft_conflict = false;
                if let Some(e) = self.data.entries.iter().find(|e| e.id == before.id) {
                    self.data.settings.last_project_id = Some(e.project_id.clone());
                }
                if clean {
                    self.notices
                        .retain(|n| n.kind != NoticeKind::EditedElsewhere);
                } else {
                    let message = ended_elsewhere_message(&self.draft.title, &self.draft.content);
                    self.upsert_notice(NoticeKind::EditedElsewhere, message);
                }
                self.reset_draft_after_end();
            }
        }
    }

    /// Keep the stored pause seconds when the form showed them unchanged and they still fit.
    fn keep_paused_seconds(
        &mut self,
        id: &str,
        stored: i64,
        form_minutes: i64,
        now: DateTime<Utc>,
    ) {
        if round_minutes(stored) != form_minutes {
            return;
        }
        if let Some(e) = self.data.entries.iter_mut().find(|e| e.id == id) {
            let span_end = e.end.or(e.pause_started_at).unwrap_or(now);
            if stored <= (span_end - e.start).num_seconds() {
                e.paused_seconds = stored;
            }
        }
    }

    fn catalog_change(
        &mut self,
        now: DateTime<Utc>,
        f: impl FnOnce(&mut Data) -> Result<(), CatalogError>,
    ) -> Result<(), String> {
        let selected_before = self.selected_project_id();
        f(&mut self.data).map_err(msg)?;
        self.follow_selection(selected_before);
        self.persist(now);
        Ok(())
    }

    /// When idle and the panel's project changed (archived, deleted…), behave like switching:
    /// prefill the new project's last title, keep the typed content.
    fn follow_selection(&mut self, selected_before: Option<Id>) {
        if self.running().is_none() && self.selected_project_id() != selected_before {
            let title = self.selected_title();
            let content = std::mem::take(&mut self.draft.content);
            self.replace_draft(title, content);
        }
    }

    /// Spec §9.3: keep the newest running entry; end each older one when the next one started
    /// (or at its pause start, if earlier). Nothing is removed; each repair is logged.
    fn repair_multiple_running(&mut self, now: DateTime<Utc>) {
        let mut running: Vec<(DateTime<Utc>, usize)> = self
            .data
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.end.is_none())
            .map(|(i, e)| (e.start, i))
            .collect();
        if running.len() < 2 {
            return;
        }
        running.sort_by_key(|(start, _)| *start);
        for pair in running.windows(2) {
            let (next_start, idx) = (pair[1].0, pair[0].1);
            let e = &mut self.data.entries[idx];
            let end = e
                .pause_started_at
                .map_or(next_start, |p| p.min(next_start))
                .max(e.start);
            e.end = Some(end);
            e.pause_started_at = None;
            e.paused_seconds = e.paused_seconds.min((end - e.start).num_seconds());
        }
        for &(_, idx) in &running[..running.len() - 1] {
            self.log(
                now,
                Action::Edit,
                Some(&self.data.entries[idx]),
                Some(REPAIRED_RUNNING_NOTE),
            );
        }
        self.push_notice(NoticeKind::Info, REPAIRED_RUNNING.to_string());
        self.persist(now);
    }

    fn long_running_prompt(&self, now: DateTime<Utc>) -> bool {
        self.running()
            .is_some_and(|e| effective_seconds(e, now) >= LONG_RUNNING_SECS * (self.snoozes + 1))
    }

    /// A different entry is running now: start reminders from scratch, without backfill.
    fn reset_reminders(&mut self, now: DateTime<Utc>) {
        self.snoozes = 0;
        self.long_prompt_reported = false;
        self.last_hour_notified = self
            .running()
            .map_or(0, |e| effective_seconds(e, now) / 3600);
    }

    fn backup_if_new_day(&mut self, now: DateTime<Utc>) {
        let today = local_date(self.zone.as_ref(), now);
        if self.last_backup_date != Some(today)
            && storage::ensure_daily_backup(&self.fs, &self.paths, today).is_ok()
        {
            self.last_backup_date = Some(today);
        }
    }

    /// Save everything. On failure the changes stay in memory and `tick` retries.
    fn persist(&mut self, now: DateTime<Utc>) {
        self.dirty = true;
        if self.save_blocked {
            return;
        }
        self.backup_if_new_day(now);
        match storage::save_atomic(&self.fs, &self.paths, &self.data) {
            Ok(()) => {
                self.dirty = false;
                self.notices.retain(|n| n.kind != NoticeKind::SaveError);
            }
            Err(e) => self.set_save_error(&e.to_string()),
        }
    }

    fn set_save_error(&mut self, err: &str) {
        self.upsert_notice(
            NoticeKind::SaveError,
            format!("存檔失敗，每秒自動重試：{err}"),
        );
    }

    fn upsert_notice(&mut self, kind: NoticeKind, message: String) {
        match self.notices.iter_mut().find(|n| n.kind == kind) {
            Some(n) => n.message = message,
            None => self.push_notice(kind, message),
        }
    }

    fn report_load_notice(&mut self, notice: LoadNotice, now: DateTime<Utc>) {
        let (kind, message, note) = match notice {
            LoadNotice::RestoredCorrupt {
                backup_date,
                corrupt_path,
            } => (
                NoticeKind::Restored,
                format!(
                    "資料檔損壞，已從 {} 的備份還原，壞檔保留為 {}",
                    ymd(backup_date),
                    file_name(&corrupt_path)
                ),
                format!(
                    "corrupt data file restored from backup {}; kept as {}",
                    ymd(backup_date),
                    file_name(&corrupt_path)
                ),
            ),
            LoadNotice::RestoredMissing { backup_date } => (
                NoticeKind::Restored,
                format!("找不到資料檔，已從 {} 的備份還原", ymd(backup_date)),
                format!(
                    "missing data file restored from backup {}",
                    ymd(backup_date)
                ),
            ),
            LoadNotice::CorruptNoBackup { corrupt_path } => (
                NoticeKind::CorruptNoBackup,
                format!(
                    "資料檔損壞且沒有可用的備份，已從空白資料開始。壞檔保留為 {}",
                    file_name(&corrupt_path)
                ),
                format!(
                    "corrupt data file and no backup, started empty; kept as {}",
                    file_name(&corrupt_path)
                ),
            ),
            LoadNotice::Unreadable {
                reason,
                error,
                backup_date,
            } => {
                let (what, mut note) = match reason {
                    UnreadableReason::ReadFailed => (
                        "資料檔無法讀取",
                        format!("data file unreadable ({error}), saving blocked"),
                    ),
                    UnreadableReason::CannotKeepCorrupt => (
                        "資料檔損壞，且無法改名保留",
                        format!("corrupt data file cannot be kept ({error}), saving blocked"),
                    ),
                };
                let mut message = format!(
                    "{what}（{error}），為避免覆蓋原檔，這次開啟期間的變更不會存檔。請檢查檔案權限後重開 app。"
                );
                if let Some(d) = backup_date {
                    message.push_str(&format!("已暫時顯示 {} 的備份內容。", ymd(d)));
                    note.push_str(&format!("; showing backup {}", ymd(d)));
                }
                (NoticeKind::Unreadable, message, note)
            }
            LoadNotice::NewerVersion { version } => (
                NoticeKind::NewerVersion,
                "資料檔是由較新版本的打工人計時器建立的，為避免覆蓋，這次開啟期間的變更不會存檔。請改用較新的版本。".to_string(),
                format!("newer data version {version}, saving blocked"),
            ),
        };
        self.push_notice(kind, message);
        self.log(now, Action::Restore, None, Some(&note));
    }

    /// Items that failed to parse stay in the file but are invisible: say so once per open.
    fn report_quarantined(&mut self, now: DateTime<Utc>) {
        let d = &self.data;
        let n = d.quarantined_clients.len()
            + d.quarantined_projects.len()
            + d.quarantined_entries.len();
        if n == 0 {
            return;
        }
        self.log(
            now,
            Action::Restore,
            None,
            Some(&format!("quarantined={n}")),
        );
        self.push_notice(
            NoticeKind::Info,
            format!("有 {n} 筆資料格式不正確，已另外保留在資料檔中，不會顯示在清單與合計裡。"),
        );
    }

    fn log(&self, now: DateTime<Utc>, action: Action, entry: Option<&Entry>, note: Option<&str>) {
        let line = activity::format_line(now, self.zone.as_ref(), action, entry, note);
        activity::append(&self.fs, &self.paths, &line);
    }

    fn log_entry(&self, now: DateTime<Utc>, action: Action, id: &str) {
        let entry = self.data.entries.iter().find(|e| e.id == id);
        self.log(now, action, entry, None);
    }
}

#[cfg(test)]
mod tests;
