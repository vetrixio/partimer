//! Tauri shell: a thin adapter around `partimer_core::Session`.

mod commands;
pub mod platform;
mod ticker;
mod tray;
pub mod windows;

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use chrono::Utc;
use partimer_core::{NoticeKind, Paths, RealFs, Session, SystemZone, default_dir};
use serde::Serialize;
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::windows::Origin;

pub struct AppState {
    pub session: Mutex<Session<RealFs>>,
    pub shortcut: Mutex<ShortcutStatus>,
    pub autostart_error: Mutex<Option<String>>,
    /// Entry the records window should open on load. The `open-entry` event can fire before a
    /// new window has registered its listener, so the window also asks for it on mount.
    pub pending_entry: Mutex<Option<String>>,
}

/// Return and clear the pending entry id.
pub fn take_pending(slot: &Mutex<Option<String>>) -> Option<String> {
    lock(slot).take()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    pub accelerator: String,
    pub registered: bool,
    pub wayland: bool,
}

/// Lock ignoring poisoning: a panic in one command must not take every later command down.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

const DATA_DIR_FALLBACK: &str = "partimer-data";

/// Ctrl+Alt+P everywhere; on macOS this is Ctrl+Option+P.
const SHORTCUT: &str = "Ctrl+Alt+P";

/// The data folder, or `<exe dir>/partimer-data` when the system has no data folder.
/// Returns whether the fallback was used, so the panel can say where the data went.
fn resolve_data_dir(system: Option<PathBuf>, exe: Option<PathBuf>) -> (PathBuf, bool) {
    match system {
        Some(dir) => (dir, false),
        None => {
            let base = exe
                .as_deref()
                .and_then(|e| e.parent())
                .map(PathBuf::from)
                .unwrap_or_default();
            (base.join(DATA_DIR_FALLBACK), true)
        }
    }
}

fn open_session() -> Session<RealFs> {
    let (dir, fallback) = resolve_data_dir(default_dir(), std::env::current_exe().ok());
    let mut session = Session::open(
        RealFs,
        Paths { dir: dir.clone() },
        Box::new(SystemZone),
        Utc::now(),
    );
    if fallback {
        session.push_notice(
            NoticeKind::Info,
            format!("找不到系統的應用程式資料夾，資料改存在：{}", dir.display()),
        );
    }
    session
}

pub fn run() {
    let app = tauri::Builder::default()
        // Must be the first plugin: a second launch exits before anything else runs.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::show_panel(app, Origin::SecondInstance);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            app.manage(AppState {
                session: Mutex::new(open_session()),
                shortcut: Mutex::new(ShortcutStatus {
                    accelerator: SHORTCUT.into(),
                    registered: false,
                    wayland: platform::is_wayland_session(),
                }),
                autostart_error: Mutex::new(None),
                pending_entry: Mutex::new(None),
            });
            // Without a tray the app is still reachable through the shortcut and a second
            // launch, so a missing indicator service must not stop it from starting.
            if let Err(e) = tray::build(app.handle()) {
                eprintln!("partimer: cannot create the tray icon: {e}");
            }
            register_shortcut(app.handle());
            // The ticker's first tick also shows the floating clock if it should be up.
            ticker::spawn(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            // The panel is only ever hidden (Alt+F4 etc. must not destroy it).
            if window.label() == windows::PANEL
                && let WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_panel_state,
            commands::get_timer_status,
            commands::set_draft,
            commands::start,
            commands::stop,
            commands::save_draft,
            commands::pause,
            commands::resume,
            commands::select_project,
            commands::add_project,
            commands::stop_at,
            commands::snooze_long_running,
            commands::dismiss_notice,
            commands::get_records,
            commands::take_pending_entry,
            commands::get_entry_form,
            commands::save_entry,
            commands::delete_entry,
            commands::export_csv,
            commands::get_catalog,
            commands::rename_client,
            commands::rename_project,
            commands::set_project_color,
            commands::set_archived,
            commands::deletion_impact,
            commands::delete_item,
            commands::get_settings,
            commands::set_autostart,
            commands::set_widget_settings,
            commands::widget_moved,
            commands::set_hourly_reminder,
            commands::open_data_folder,
            commands::open_window,
            commands::toggle_panel,
            commands::quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the Tauri app");

    app.run(|app, event| match event {
        RunEvent::ExitRequested { code, api, .. } => {
            // `code` is None when the last window closed; only `quit_app` (app.exit) quits.
            if code.is_none() {
                api.prevent_exit();
                return;
            }
            ticker::stop();
            flush(app);
        }
        // An OS-initiated quit (e.g. macOS logout) ends the loop without ExitRequested.
        // After an ExitRequested flush this second one finds nothing left to save.
        RunEvent::Exit => {
            ticker::stop();
            flush(app);
        }
        // macOS does not start a second process when a running app is opened again; it
        // sends "reopen" to the running one instead.
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => windows::show_panel(app, Origin::SecondInstance),
        _ => {}
    });
}

/// Save the unsaved draft and anything still pending before the process ends.
fn flush(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        lock(&state.session).flush_on_exit(Utc::now());
    }
}

/// Register the global shortcut and record the result for the settings window. On Wayland
/// `registered` still holds the real result; the settings window goes by `wayland`.
fn register_shortcut(app: &AppHandle) {
    let result = app
        .global_shortcut()
        .on_shortcut(SHORTCUT, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                windows::toggle_panel(app, Origin::Shortcut);
            }
        });
    if let Err(e) = &result {
        eprintln!("partimer: cannot register {SHORTCUT}: {e}");
    }
    lock(&app.state::<AppState>().shortcut).registered = result.is_ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_pending_returns_once_then_clears() {
        let slot = Mutex::new(Some("e1".to_string()));
        assert_eq!(take_pending(&slot), Some("e1".to_string()));
        assert_eq!(take_pending(&slot), None);
    }

    #[test]
    fn system_data_dir_is_used_when_available() {
        let (dir, fallback) = resolve_data_dir(
            Some(PathBuf::from("/home/u/.local/share/partimer")),
            Some(PathBuf::from("/opt/partimer/partimer")),
        );
        assert_eq!(dir, PathBuf::from("/home/u/.local/share/partimer"));
        assert!(!fallback);
    }

    #[test]
    fn missing_system_dir_falls_back_next_to_executable() {
        let (dir, fallback) = resolve_data_dir(None, Some(PathBuf::from("/opt/partimer/partimer")));
        assert_eq!(dir, PathBuf::from("/opt/partimer/partimer-data"));
        assert!(fallback);
    }

    #[test]
    fn missing_exe_path_falls_back_to_relative_folder() {
        let (dir, fallback) = resolve_data_dir(None, None);
        assert_eq!(dir, PathBuf::from("partimer-data"));
        assert!(fallback);
    }
}
