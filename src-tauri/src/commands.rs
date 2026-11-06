//! `#[tauri::command]` adapters around `partimer_core::Session`. Command names are the
//! frontend `invoke` names. Every command that changes data emits `state-changed` on success.

use chrono::Utc;
use partimer_core::catalog::DeletionImpact;
use partimer_core::model::WidgetSettings;
use partimer_core::storage::RealFs;
use partimer_core::{
    CatalogView, EntryForm, ItemKind, PanelState, RecordsQuery, RecordsView, Session,
};
use serde::Serialize;
use std::sync::MutexGuard;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt as _;
use tauri_plugin_opener::OpenerExt as _;

use crate::windows::{self, Origin};
use crate::{AppState, ShortcutStatus, platform};

type Res<T> = Result<T, String>;

pub const STATE_CHANGED: &str = "state-changed";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub autostart_enabled: bool,
    pub autostart_error: Option<String>,
    pub autostart_help: String,
    /// `enabled` always has the platform default applied.
    pub widget: WidgetSettings,
    pub hourly_reminder: bool,
    pub shortcut: ShortcutStatus,
    pub shortcut_help: Option<String>,
    pub data_dir: String,
}

fn session<'a>(state: &'a State<'_, AppState>) -> MutexGuard<'a, Session<RealFs>> {
    crate::lock(&state.session)
}

pub(crate) fn changed(app: &AppHandle) {
    if let Err(e) = app.emit(STATE_CHANGED, ()) {
        eprintln!("partimer: emit {STATE_CHANGED}: {e}");
    }
}

/// Run a mutating session call; on success emit `state-changed` and return the panel state.
fn panel_after(
    app: &AppHandle,
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut Session<RealFs>) -> Res<()>,
) -> Res<PanelState> {
    let mut s = session(state);
    f(&mut s)?;
    let panel = s.panel_state(Utc::now());
    drop(s);
    changed(app);
    Ok(panel)
}

/// Run a mutating session call that returns nothing; emit `state-changed` on success.
fn mutate(
    app: &AppHandle,
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut Session<RealFs>) -> Res<()>,
) -> Res<()> {
    f(&mut session(state))?;
    changed(app);
    Ok(())
}

// ---- panel ----

#[tauri::command]
pub fn get_panel_state(state: State<'_, AppState>) -> PanelState {
    session(&state).panel_state(Utc::now())
}

/// Draft lives in memory only; no event (other windows do not show it). Ignored when
/// `generation` is not the current draft's (fields that were already replaced).
#[tauri::command]
pub fn set_draft(state: State<'_, AppState>, title: String, content: String, generation: u64) {
    session(&state).set_draft(title, content, generation);
}

#[tauri::command]
pub fn start(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    title: String,
    content: String,
) -> Res<PanelState> {
    panel_after(&app, &state, |s| {
        s.start(&project_id, &title, &content, Utc::now())
    })
}

#[tauri::command]
pub fn stop(
    app: AppHandle,
    state: State<'_, AppState>,
    title: String,
    content: String,
) -> Res<PanelState> {
    panel_after(&app, &state, |s| s.stop(&title, &content, Utc::now()))
}

#[tauri::command]
pub fn save_draft(
    app: AppHandle,
    state: State<'_, AppState>,
    title: String,
    content: String,
) -> Res<PanelState> {
    panel_after(&app, &state, |s| s.save_draft(&title, &content, Utc::now()))
}

#[tauri::command]
pub fn pause(app: AppHandle, state: State<'_, AppState>) -> Res<PanelState> {
    panel_after(&app, &state, |s| s.pause(Utc::now()))
}

#[tauri::command]
pub fn resume(app: AppHandle, state: State<'_, AppState>) -> Res<PanelState> {
    panel_after(&app, &state, |s| s.resume(Utc::now()))
}

#[tauri::command]
pub fn select_project(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    title: String,
    content: String,
) -> Res<PanelState> {
    panel_after(&app, &state, |s| {
        s.select_project(&project_id, &title, &content, Utc::now())
    })
}

#[tauri::command]
pub fn add_project(
    app: AppHandle,
    state: State<'_, AppState>,
    client_name: String,
    project_name: String,
    title: String,
    content: String,
) -> Res<PanelState> {
    panel_after(&app, &state, |s| {
        s.add_project(&client_name, &project_name, &title, &content, Utc::now())
    })
}

#[tauri::command]
pub fn stop_at(
    app: AppHandle,
    state: State<'_, AppState>,
    title: String,
    content: String,
    end_local: String,
) -> Res<PanelState> {
    panel_after(&app, &state, |s| {
        s.stop_at_local(&title, &content, &end_local, Utc::now())
    })
}

#[tauri::command]
pub fn snooze_long_running(app: AppHandle, state: State<'_, AppState>) -> Res<PanelState> {
    panel_after(&app, &state, |s| {
        s.snooze_long_running(Utc::now());
        Ok(())
    })
}

#[tauri::command]
pub fn dismiss_notice(app: AppHandle, state: State<'_, AppState>, id: u64) -> Res<PanelState> {
    panel_after(&app, &state, |s| {
        s.dismiss_notice(id);
        Ok(())
    })
}

// ---- records ----

#[tauri::command]
pub fn get_records(state: State<'_, AppState>, query: RecordsQuery) -> RecordsView {
    session(&state).records(query, Utc::now())
}

/// The entry the records window was opened for, once (see `AppState::pending_entry`).
#[tauri::command]
pub fn take_pending_entry(state: State<'_, AppState>) -> Option<String> {
    crate::take_pending(&state.pending_entry)
}

#[tauri::command]
pub fn get_entry_form(state: State<'_, AppState>, id: Option<String>) -> Res<EntryForm> {
    let s = session(&state);
    match id {
        None => Ok(s.new_entry_form(Utc::now())),
        Some(id) => s
            .entry_form(&id)
            .ok_or_else(|| "找不到這筆紀錄，可能已被刪除".to_string()),
    }
}

#[tauri::command]
pub fn save_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Option<String>,
    form: EntryForm,
) -> Res<()> {
    mutate(&app, &state, |s| {
        s.save_entry(id.as_deref(), form, Utc::now())
    })
}

#[tauri::command]
pub fn delete_entry(app: AppHandle, state: State<'_, AppState>, id: String) -> Res<()> {
    mutate(&app, &state, |s| s.delete_entry(&id, Utc::now()))
}

/// Default file name for a month's export.
fn csv_file_name(q: &RecordsQuery) -> String {
    format!("partimer-{:04}-{:02}.csv", q.year, q.month)
}

/// Ask where to save, then write the CSV. `false` when the user cancels. Async so the
/// blocking dialog does not run on the main thread.
#[tauri::command]
pub async fn export_csv(
    app: AppHandle,
    state: State<'_, AppState>,
    query: RecordsQuery,
) -> Res<bool> {
    let picked = app
        .dialog()
        .file()
        .set_title("匯出 CSV")
        .set_file_name(csv_file_name(&query))
        .add_filter("CSV", &["csv"])
        .blocking_save_file();
    let Some(picked) = picked else {
        return Ok(false);
    };
    let path = picked
        .into_path()
        .map_err(|e| format!("無法使用這個位置：{e}"))?;
    let bytes = session(&state).export_csv(query);
    std::fs::write(&path, bytes).map_err(|e| format!("匯出失敗：{e}"))?;
    Ok(true)
}

// ---- catalog ----

#[tauri::command]
pub fn get_catalog(state: State<'_, AppState>) -> CatalogView {
    session(&state).catalog()
}

#[tauri::command]
pub fn rename_client(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Res<()> {
    mutate(&app, &state, |s| s.rename_client(&id, &name, Utc::now()))
}

#[tauri::command]
pub fn rename_project(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Res<()> {
    mutate(&app, &state, |s| s.rename_project(&id, &name, Utc::now()))
}

#[tauri::command]
pub fn set_project_color(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    color: u8,
) -> Res<()> {
    mutate(&app, &state, |s| {
        s.set_project_color(&id, color, Utc::now())
    })
}

#[tauri::command]
pub fn set_archived(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: ItemKind,
    id: String,
    archived: bool,
) -> Res<()> {
    mutate(&app, &state, |s| match kind {
        ItemKind::Client => s.set_client_archived(&id, archived, Utc::now()),
        ItemKind::Project => s.set_project_archived(&id, archived, Utc::now()),
    })
}

#[tauri::command]
pub fn deletion_impact(
    state: State<'_, AppState>,
    kind: ItemKind,
    id: String,
) -> Res<DeletionImpact> {
    session(&state).deletion_impact(kind, &id, Utc::now())
}

#[tauri::command]
pub fn delete_item(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: ItemKind,
    id: String,
) -> Res<()> {
    mutate(&app, &state, |s| s.delete_item(kind, &id, Utc::now()))
}

// ---- settings ----

/// The stored widget settings with `enabled` resolved to the platform default.
fn widget_view(stored: &WidgetSettings) -> WidgetSettings {
    WidgetSettings {
        enabled: Some(
            stored
                .enabled
                .unwrap_or_else(platform::widget_default_enabled),
        ),
        ..stored.clone()
    }
}

/// Apply the user-editable widget fields. The position is owned by `widget_moved`, so a
/// stale x/y from the settings window never moves the clock back. Opacity stays in 40%–100%.
fn merge_widget(stored: &WidgetSettings, incoming: WidgetSettings) -> WidgetSettings {
    let opacity = if incoming.opacity.is_finite() {
        incoming.opacity.clamp(0.4, 1.0)
    } else {
        stored.opacity
    };
    WidgetSettings {
        enabled: incoming.enabled,
        opacity,
        show_when_idle: incoming.show_when_idle,
        x: stored.x,
        y: stored.y,
    }
}

fn settings_view(app: &AppHandle, state: &State<'_, AppState>) -> SettingsView {
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let (widget, hourly_reminder, data_dir) = {
        let s = session(state);
        let settings = s.settings();
        (
            widget_view(&settings.widget),
            settings.hourly_reminder,
            s.data_dir().display().to_string(),
        )
    };
    SettingsView {
        autostart_enabled,
        autostart_error: crate::lock(&state.autostart_error).clone(),
        autostart_help: platform::autostart_help(),
        widget,
        hourly_reminder,
        shortcut: crate::lock(&state.shortcut).clone(),
        shortcut_help: platform::shortcut_help(),
        data_dir,
    }
}

#[tauri::command]
pub fn get_settings(app: AppHandle, state: State<'_, AppState>) -> SettingsView {
    settings_view(&app, &state)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> SettingsView {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    *crate::lock(&state.autostart_error) = result.err().map(|e| {
        format!(
            "無法{}開機自動啟動：{e}",
            if enabled { "開啟" } else { "關閉" }
        )
    });
    changed(&app);
    settings_view(&app, &state)
}

#[tauri::command]
pub fn set_widget_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    widget: WidgetSettings,
) -> SettingsView {
    session(&state).update_settings(|s| s.widget = merge_widget(&s.widget, widget), Utc::now());
    windows::ensure_widget(&app);
    changed(&app);
    settings_view(&app, &state)
}

#[tauri::command]
pub fn widget_moved(app: AppHandle, state: State<'_, AppState>, x: i32, y: i32) {
    session(&state).update_settings(
        |s| {
            s.widget.x = Some(x);
            s.widget.y = Some(y);
        },
        Utc::now(),
    );
    changed(&app);
}

#[tauri::command]
pub fn set_hourly_reminder(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> SettingsView {
    session(&state).update_settings(|s| s.hourly_reminder = enabled, Utc::now());
    changed(&app);
    settings_view(&app, &state)
}

#[tauri::command]
pub fn open_data_folder(app: AppHandle, state: State<'_, AppState>) -> Res<()> {
    let dir = session(&state).data_dir().to_path_buf();
    std::fs::create_dir_all(&dir).map_err(|e| format!("無法建立資料夾：{e}"))?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| format!("無法打開資料夾：{e}"))
}

// ---- windows ----

#[tauri::command]
pub fn open_window(app: AppHandle, kind: String, entry_id: Option<String>) -> Res<()> {
    match kind.as_str() {
        "records" => windows::open_records(&app, entry_id),
        "settings" => windows::open_settings(&app),
        other => return Err(format!("未知的視窗：{other}")),
    }
    Ok(())
}

/// From the frontend there is no tray rectangle, so "tray" places the panel like the
/// shortcut does.
#[tauri::command]
pub fn toggle_panel(app: AppHandle, origin: String) -> Res<()> {
    let origin = match origin.as_str() {
        "widget" => Origin::Widget,
        "shortcut" | "tray" => Origin::Shortcut,
        other => return Err(format!("未知的來源：{other}")),
    };
    windows::toggle_panel(&app, origin);
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_file_name_pads_month() {
        let q = RecordsQuery {
            year: 2026,
            month: 3,
            client_id: None,
            project_id: None,
        };
        assert_eq!(csv_file_name(&q), "partimer-2026-03.csv");
    }

    #[test]
    fn widget_view_applies_platform_default_only_when_unset() {
        let unset = WidgetSettings::default();
        assert_eq!(
            widget_view(&unset).enabled,
            Some(platform::widget_default_enabled())
        );
        let chosen = WidgetSettings {
            enabled: Some(!platform::widget_default_enabled()),
            ..WidgetSettings::default()
        };
        assert_eq!(widget_view(&chosen).enabled, chosen.enabled);
    }

    #[test]
    fn merge_widget_keeps_position_and_clamps_opacity() {
        let stored = WidgetSettings {
            x: Some(100),
            y: Some(200),
            ..WidgetSettings::default()
        };
        let incoming = WidgetSettings {
            enabled: Some(true),
            opacity: 0.1,
            show_when_idle: true,
            x: Some(5),
            y: None,
        };
        let merged = merge_widget(&stored, incoming);
        assert_eq!(merged.enabled, Some(true));
        assert_eq!(merged.opacity, 0.4);
        assert!(merged.show_when_idle);
        assert_eq!((merged.x, merged.y), (Some(100), Some(200)));

        let too_high = WidgetSettings {
            opacity: 3.0,
            ..WidgetSettings::default()
        };
        assert_eq!(merge_widget(&stored, too_high).opacity, 1.0);
        let nan = WidgetSettings {
            opacity: f64::NAN,
            ..WidgetSettings::default()
        };
        assert_eq!(merge_widget(&stored, nan).opacity, stored.opacity);
    }
}
