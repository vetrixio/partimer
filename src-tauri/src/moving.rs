//! Commands that move data in and out of the app through native file and confirm dialogs.
//! The logic lives in `partimer_core::Session`; this layer only runs dialogs and file io.
//! The session lock is never held while a dialog is open.

use chrono::{Local, Utc};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt as _, MessageDialogButtons, MessageDialogKind};

use crate::AppState;
use crate::commands::{changed, session, settings_changed, write_error};
use crate::windows::reposition_widget;

type Res<T> = Result<T, String>;

fn transfer_file_name(today: &str) -> String {
    format!("partimer-transfer-{today}.json")
}

fn file_name_of(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn confirm(app: &AppHandle, title: &str, text: String, ok: &str) -> bool {
    app.dialog()
        .message(text)
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            ok.into(),
            "取消".into(),
        ))
        .blocking_show()
}

/// Save everything to a transfer file. `None` when the user cancels.
#[tauri::command]
pub async fn transfer_out(app: AppHandle, state: State<'_, AppState>) -> Res<Option<String>> {
    let bytes = session(&state).transfer_out(Utc::now())?;
    let today = Local::now().format("%Y-%m-%d").to_string();
    let picked = app
        .dialog()
        .file()
        .set_title("轉出資料")
        .set_file_name(transfer_file_name(&today))
        .add_filter("轉移檔", &["json"])
        .blocking_save_file();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|e| format!("無法使用這個位置（{e}）"))?;
    std::fs::write(&path, bytes).map_err(|e| write_error("轉出失敗", &e))?;
    session(&state).note_transfer_out(Utc::now());
    Ok(Some(format!("已轉出到 {}", file_name_of(&path))))
}

/// Replace all data with a transfer file, after a confirmation. `None` when cancelled.
#[tauri::command]
pub async fn transfer_in(app: AppHandle, state: State<'_, AppState>) -> Res<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .set_title("轉入資料")
        .add_filter("轉移檔", &["json"])
        .blocking_pick_file();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|e| format!("無法使用這個位置（{e}）"))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("無法讀取這個檔案（{e}）"))?;
    let plan = session(&state).prepare_transfer_in(&bytes)?;
    if !confirm(&app, "轉入資料", plan.confirm_text.clone(), "轉入") {
        return Ok(None);
    }
    let message = session(&state).transfer_in(plan, Utc::now())?;
    changed(&app);
    settings_changed(&app);
    reposition_widget(&app);
    Ok(Some(message))
}

/// Import one or more exported CSV files into an empty session, after a confirmation.
#[tauri::command]
pub async fn import_csv(app: AppHandle, state: State<'_, AppState>) -> Res<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .set_title("匯入 CSV")
        .add_filter("CSV", &["csv"])
        .blocking_pick_files();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let mut files = Vec::with_capacity(picked.len());
    for p in picked {
        let path = p
            .into_path()
            .map_err(|e| format!("無法使用這個位置（{e}）"))?;
        let bytes = std::fs::read(&path).map_err(|e| format!("無法讀取這個檔案（{e}）"))?;
        files.push((file_name_of(&path), bytes));
    }
    let plan = session(&state).prepare_csv_import(&files)?;
    if !confirm(&app, "匯入 CSV", plan.confirm_text.clone(), "匯入") {
        return Ok(None);
    }
    let message = session(&state).import_csv(plan, Utc::now())?;
    changed(&app);
    Ok(Some(message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_transfer_file_name() {
        assert_eq!(
            transfer_file_name("2026-11-13"),
            "partimer-transfer-2026-11-13.json"
        );
    }
}
