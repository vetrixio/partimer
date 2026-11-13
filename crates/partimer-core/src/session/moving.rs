//! Moving all data between computers: transfer out (the whole `data.json`) and transfer in
//! (replace everything, after saving the current data next to it).

use chrono::{DateTime, Utc};

use super::{Session, file_name, msg, ymd};
use crate::activity::Action;
use crate::csv_import::{self, CsvImport};
use crate::model::{Data, to_json};
use crate::storage::{self, Fs, SnapshotKind};
use crate::timer;
use crate::transfer::{counts, date_range, is_empty, parse_transfer};

pub(crate) const SAVE_BLOCKED_MOVE: &str =
    "資料檔目前不能寫入（由較新版建立或讀不到），為避免覆蓋，這次開啟期間不能轉入或匯入。";

pub(crate) const CSV_NEEDS_EMPTY: &str =
    "匯入 CSV 只能用在還沒有任何紀錄的時候。要在兩台電腦間搬資料，請改用設定裡的『轉入資料』。";

/// Checked CSV files waiting for the user's confirmation.
#[derive(Debug)]
pub struct CsvImportPlan {
    import: CsvImport,
    pub confirm_text: String,
}

/// A checked transfer file waiting for the user's confirmation.
#[derive(Debug)]
pub struct TransferInPlan {
    data: Data,
    pub confirm_text: String,
}

impl<F: Fs> Session<F> {
    /// A copy of the data for a pre-change snapshot, with a running or paused entry ended the
    /// way STOP would at `now`, so the snapshot can be transferred back in.
    pub(super) fn snapshot_data(&self, now: DateTime<Utc>) -> Data {
        let mut copy = self.data.clone();
        if let Some(e) = self.running() {
            let _ = timer::stop(&mut copy, &e.title, &e.content, now);
        }
        copy
    }

    /// The bytes to save as the transfer file. Not allowed while a timer is running.
    pub fn transfer_out(&self, _now: DateTime<Utc>) -> Result<Vec<u8>, String> {
        if self.running().is_some() {
            return Err("請先按 STOP 結束計時再轉出".to_string());
        }
        Ok(to_json(&self.data))
    }

    /// Log a finished transfer out (after the file was written).
    pub fn note_transfer_out(&self, now: DateTime<Utc>) {
        let note = format!("entries={}", counts(&self.data).entries);
        self.log(now, Action::TransferOut, None, Some(&note));
    }

    pub fn prepare_transfer_in(&self, bytes: &[u8]) -> Result<TransferInPlan, String> {
        let data = parse_transfer(bytes).map_err(msg)?;
        let c = counts(&data);
        let mut text = format!(
            "將轉入 {} 個客戶、{} 個專案、{} 筆紀錄",
            c.clients, c.projects, c.entries
        );
        if let Some((first, last)) = date_range(&data, self.zone.as_ref()) {
            text.push_str(&format!("（{} ～ {}）", ymd(first), ymd(last)));
        }
        text.push('。');
        if !is_empty(&self.data) {
            let n = counts(&self.data);
            text.push_str(&format!(
                "\n這台電腦目前的 {} 個客戶、{} 個專案、{} 筆紀錄會全部被取代。原本的資料會另存為 data.before-transfer-…json。",
                n.clients, n.projects, n.entries
            ));
            if self.running().is_some() {
                text.push_str("\n計時中的那筆也會被取代。");
            }
        }
        Ok(TransferInPlan {
            data,
            confirm_text: text,
        })
    }

    /// Replace everything with the plan's data. A running entry is dropped without a STOP.
    pub fn transfer_in(
        &mut self,
        plan: TransferInPlan,
        now: DateTime<Utc>,
    ) -> Result<String, String> {
        if self.save_blocked {
            return Err(SAVE_BLOCKED_MOVE.to_string());
        }
        let replaced = counts(&self.data);
        let snapshot = if is_empty(&self.data) {
            None
        } else {
            let path = storage::write_snapshot(
                &self.fs,
                &self.paths,
                SnapshotKind::Transfer,
                &self.snapshot_data(now),
                self.zone.as_ref(),
                now,
            )
            .map_err(|e| format!("無法另存目前的資料，沒有轉入（{e}）"))?;
            Some(file_name(&path))
        };
        let entries = counts(&plan.data).entries;
        self.data = plan.data;
        self.replace_draft(self.selected_title(), String::new());
        self.reset_reminders(now);
        self.persist(now);
        let note = format!(
            "entries={entries} replaced={} snapshot={}",
            replaced.entries,
            snapshot.as_deref().unwrap_or("-")
        );
        self.log(now, Action::TransferIn, None, Some(&note));
        let mut message = format!("已轉入 {entries} 筆紀錄。");
        if let Some(name) = snapshot {
            message.push_str(&format!("原本的資料另存為 {name}。"));
        }
        Ok(message)
    }

    fn check_csv_import_allowed(&self) -> Result<(), String> {
        if self.save_blocked {
            return Err(SAVE_BLOCKED_MOVE.to_string());
        }
        if !self.data.entries.is_empty() || !self.data.quarantined_entries.is_empty() {
            return Err(CSV_NEEDS_EMPTY.to_string());
        }
        Ok(())
    }

    pub fn prepare_csv_import(&self, files: &[(String, Vec<u8>)]) -> Result<CsvImportPlan, String> {
        self.check_csv_import_allowed()?;
        let import = csv_import::prepare(files, self.zone.as_ref())?;
        let (new_clients, new_projects) = csv_import::new_catalog_counts(&self.data, &import);
        let mut text = format!(
            "將匯入 {} 筆紀錄（{} ～ {}），新增 {new_clients} 個客戶、{new_projects} 個專案",
            import.rows.len(),
            ymd(import.first_date),
            ymd(import.last_date)
        );
        if import.skipped_duplicates > 0 {
            text.push_str(&format!("；略過重複 {} 筆", import.skipped_duplicates));
        }
        text.push_str("。時間依這台電腦的時區解讀。");
        Ok(CsvImportPlan {
            import,
            confirm_text: text,
        })
    }

    /// Add the plan's rows as finished entries. Only allowed while there are no entries.
    pub fn import_csv(
        &mut self,
        plan: CsvImportPlan,
        now: DateTime<Utc>,
    ) -> Result<String, String> {
        self.check_csv_import_allowed()?;
        if !is_empty(&self.data) {
            storage::write_snapshot(
                &self.fs,
                &self.paths,
                SnapshotKind::Import,
                &self.snapshot_data(now),
                self.zone.as_ref(),
                now,
            )
            .map_err(|e| format!("無法另存目前的資料，沒有匯入（{e}）"))?;
        }
        let files = plan.import.files;
        let skipped = plan.import.skipped_duplicates;
        let entries = csv_import::apply(&mut self.data, plan.import);
        if self.running().is_none() {
            self.replace_draft(self.selected_title(), String::new());
        }
        self.persist(now);
        let note = format!("files={files} entries={entries} skipped={skipped}");
        self.log(now, Action::ImportCsv, None, Some(&note));
        Ok(format!("已匯入 {entries} 筆紀錄。"))
    }
}
