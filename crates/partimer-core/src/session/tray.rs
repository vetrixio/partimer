//! Tray menu actions. Each acts on the live timer state at click time, and a stale click
//! changes nothing instead of failing.

use chrono::{DateTime, Utc};

use super::{Session, status_of};
use crate::model::Id;
use crate::storage::Fs;
use crate::views::TimerStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayPick {
    Started,
    /// Already timing this project.
    NoChange,
    /// Timing another project; the caller asks, then calls `tray_switch`.
    ConfirmSwitch {
        running_entry: Id,
    },
}

impl<F: Fs> Session<F> {
    /// A pinned project was clicked: the same as selecting it in the panel and pressing START.
    pub fn tray_pick(&mut self, project_id: &str, now: DateTime<Utc>) -> Result<TrayPick, String> {
        if let Some(e) = self.running() {
            return Ok(if e.project_id == project_id {
                TrayPick::NoChange
            } else {
                TrayPick::ConfirmSwitch {
                    running_entry: e.id.clone(),
                }
            });
        }
        let (title, content) = (self.draft.title.clone(), self.draft.content.clone());
        self.select_project(project_id, &title, &content, now)?;
        let (title, content) = (self.draft.title.clone(), self.draft.content.clone());
        self.start(project_id, &title, &content, now)?;
        Ok(TrayPick::Started)
    }

    /// The confirmed switch. `Ok(false)` when the timer is no longer the entry that was asked about.
    pub fn tray_switch(
        &mut self,
        project_id: &str,
        running_entry: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, String> {
        if self.running().is_none_or(|e| e.id != running_entry) {
            return Ok(false);
        }
        let (title, content) = self.stop_text();
        self.select_project(project_id, &title, &content, now)?;
        Ok(true)
    }

    pub fn tray_pause(&mut self, now: DateTime<Utc>) -> Result<bool, String> {
        if self.running().map(status_of) != Some(TimerStatus::Running) {
            return Ok(false);
        }
        self.pause(now).map(|()| true)
    }

    pub fn tray_resume(&mut self, now: DateTime<Utc>) -> Result<bool, String> {
        if self.running().map(status_of) != Some(TimerStatus::Paused) {
            return Ok(false);
        }
        self.resume(now).map(|()| true)
    }

    pub fn tray_stop(&mut self, now: DateTime<Utc>) -> Result<bool, String> {
        if self.running().is_none() {
            return Ok(false);
        }
        let (title, content) = self.stop_text();
        self.stop(&title, &content, now).map(|()| true)
    }

    /// Text to end the running entry with: the panel draft, unless it conflicts with an edit
    /// made elsewhere (same rule as quitting).
    fn stop_text(&self) -> (String, String) {
        match self.running() {
            Some(e) if self.draft_conflict => (e.title.clone(), e.content.clone()),
            _ => (self.draft.title.clone(), self.draft.content.clone()),
        }
    }
}
