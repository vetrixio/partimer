//! The tray / menu bar icon: pictures, awake animation, title and tooltip, menu.
//!
//! `update` and `advance_frame` are called from the ticker thread only (never while the
//! session lock is held); the tray API forwards each call to the main thread itself.

use std::sync::Mutex;

use partimer_core::TimerStatus;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalRect, Rect};

use crate::lock;
use crate::windows::{self, Origin};

const TRAY_ID: &str = "partimer";
const APP_NAME: &str = "打工人計時器";

const MENU_PANEL: &str = "open-panel";
const MENU_RECORDS: &str = "records";
const MENU_SETTINGS: &str = "settings";
const MENU_QUIT: &str = "quit";

/// 32px pictures; on macOS the 64px ones, which the menu bar shows as @2x.
#[cfg(target_os = "macos")]
macro_rules! tray_png {
    ($name:literal) => {
        include_bytes!(concat!("../icons/tray/", $name, "-64.png"))
    };
}
#[cfg(not(target_os = "macos"))]
macro_rules! tray_png {
    ($name:literal) => {
        include_bytes!(concat!("../icons/tray/", $name, "-32.png"))
    };
}

const REST_PNG: &[u8] = tray_png!("rest");
const AWAKE_PNG: [&[u8]; 4] = [
    tray_png!("awake-0"),
    tray_png!("awake-1"),
    tray_png!("awake-2"),
    tray_png!("awake-3"),
];

struct TrayState {
    rest: Image<'static>,
    awake: Vec<Image<'static>>,
    shown: Mutex<Shown>,
    /// Last icon rectangle seen in a tray event, so the menu's 「打開面板」 can open the panel
    /// next to the icon. Never set on Linux (AppIndicator sends no tray events).
    last_rect: Mutex<Option<PhysicalRect<i32, u32>>>,
}

/// What the tray currently shows, so unchanged values are not pushed again every second.
#[derive(Debug)]
struct Shown {
    running: bool,
    frame: usize,
    title: Option<String>,
    tooltip: String,
}

/// What `Shown::apply` says must be pushed to the tray; `None` = unchanged.
#[derive(Debug, PartialEq)]
struct Changes {
    /// `Some(true)`: first awake frame, `Some(false)`: resting icon.
    icon_running: Option<bool>,
    title: Option<Option<String>>,
    tooltip: Option<String>,
}

impl Shown {
    /// Record the new state and return only what differs from what is shown.
    fn apply(&mut self, running: bool, title: Option<String>, tooltip: String) -> Changes {
        let icon_running = (running != self.running).then(|| {
            self.running = running;
            self.frame = 0;
            running
        });
        let title = (title != self.title).then(|| {
            self.title = title.clone();
            title
        });
        let tooltip = (tooltip != self.tooltip).then(|| {
            self.tooltip = tooltip.clone();
            tooltip
        });
        Changes {
            icon_running,
            title,
            tooltip,
        }
    }
}

/// `1:23`: whole hours (not wrapped at 24) and zero-padded minutes, negatives as 0.
fn clock(elapsed_seconds: i64) -> String {
    let s = elapsed_seconds.max(0);
    format!("{}:{:02}", s / 3600, (s % 3600) / 60)
}

/// Text beside the icon (macOS menu bar, Linux AppIndicator label). The two leading spaces
/// keep a gap between the icon and the time.
fn title_text(status: TimerStatus, elapsed_seconds: i64) -> Option<String> {
    match status {
        TimerStatus::Running => Some(format!("  {}", clock(elapsed_seconds))),
        TimerStatus::Paused => Some(format!("  ⏸ {}", clock(elapsed_seconds))),
        TimerStatus::Idle => None,
    }
}

/// Hover text (Windows has no text beside tray icons).
fn tooltip_text(status: TimerStatus, elapsed_seconds: i64, project_name: Option<&str>) -> String {
    let base = match status {
        TimerStatus::Running => format!("計時中 {}", clock(elapsed_seconds)),
        TimerStatus::Paused => format!("暫停中 ⏸ {}", clock(elapsed_seconds)),
        TimerStatus::Idle => return APP_NAME.to_string(),
    };
    match project_name {
        Some(name) => format!("{base}・{name}"),
        None => base,
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let state = TrayState {
        rest: Image::from_bytes(REST_PNG)?,
        awake: AWAKE_PNG
            .iter()
            .map(|png| Image::from_bytes(png))
            .collect::<tauri::Result<_>>()?,
        shown: Mutex::new(Shown {
            running: false,
            frame: 0,
            title: None,
            tooltip: APP_NAME.to_string(),
        }),
        last_rect: Mutex::new(None),
    };

    // Always attached: it is the right-click menu on macOS / Windows and the only way to
    // interact on Linux, where AppIndicator never delivers left clicks to the app.
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, MENU_PANEL, "打開面板", true, None::<&str>)?,
            &MenuItem::with_id(app, MENU_RECORDS, "紀錄", true, None::<&str>)?,
            &MenuItem::with_id(app, MENU_SETTINGS, "設定", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, MENU_QUIT, "結束", true, None::<&str>)?,
        ],
    )?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(state.rest.clone())
        .tooltip(APP_NAME)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_PANEL => windows::show_panel(app, menu_origin(app)),
            MENU_RECORDS => windows::open_records(app, None),
            MENU_SETTINGS => windows::open_settings(app),
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(on_tray_event)
        .build(app)?;

    app.manage(state);
    Ok(())
}

fn on_tray_event(tray: &TrayIcon, event: TrayIconEvent) {
    let app = tray.app_handle();
    let (rect, left_click) = match event {
        TrayIconEvent::Click {
            rect,
            button,
            button_state,
            ..
        } => (
            rect,
            button == MouseButton::Left && button_state == MouseButtonState::Up,
        ),
        // The icon's rectangle does not change while the pointer moves over it, so `Move`
        // (sent on every mouse move) is ignored; `Enter` and clicks keep it current.
        TrayIconEvent::Enter { rect, .. } => (rect, false),
        _ => return,
    };
    let rect = physical_rect(rect);
    if let Some(state) = app.try_state::<TrayState>() {
        *lock(&state.last_rect) = Some(rect);
    }
    if left_click {
        windows::toggle_panel(app, Origin::Tray(rect));
    }
}

/// Tray events carry physical values, so the scale factor is never applied.
fn physical_rect(rect: Rect) -> PhysicalRect<i32, u32> {
    PhysicalRect {
        position: rect.position.to_physical(1.0),
        size: rect.size.to_physical(1.0),
    }
}

fn menu_origin(app: &AppHandle) -> Origin {
    app.try_state::<TrayState>()
        .and_then(|state| *lock(&state.last_rect))
        .map_or(Origin::TrayMenu, Origin::Tray)
}

/// Show the timer state: picture (rest / awake), title beside the icon, tooltip.
pub fn update(
    app: &AppHandle,
    status: TimerStatus,
    elapsed_seconds: i64,
    project_name: Option<&str>,
) {
    let (Some(state), Some(tray)) = (app.try_state::<TrayState>(), app.tray_by_id(TRAY_ID)) else {
        return;
    };
    let running = status == TimerStatus::Running;
    let title = title_text(status, elapsed_seconds);
    let tooltip = tooltip_text(status, elapsed_seconds, project_name);

    // Decide under the lock, call the tray API after releasing it.
    let changes = lock(&state.shown).apply(running, title, tooltip);
    if let Some(running) = changes.icon_running {
        let icon = if running {
            &state.awake[0]
        } else {
            &state.rest
        };
        log(tray.set_icon(Some(icon.clone())));
    }
    if let Some(title) = changes.title {
        log(tray.set_title(title));
    }
    if let Some(tooltip) = changes.tooltip {
        log(tray.set_tooltip(Some(tooltip)));
    }
}

/// Next frame of the awake animation; does nothing unless the timer is running.
pub fn advance_frame(app: &AppHandle) {
    let (Some(state), Some(tray)) = (app.try_state::<TrayState>(), app.tray_by_id(TRAY_ID)) else {
        return;
    };
    let frame = {
        let mut shown = lock(&state.shown);
        if !shown.running {
            return;
        }
        shown.frame = (shown.frame + 1) % state.awake.len();
        shown.frame
    };
    log(tray.set_icon(Some(state.awake[frame].clone())));
}

fn log(result: tauri::Result<()>) {
    if let Err(e) = result {
        eprintln!("partimer: tray: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_shows_hours_and_minutes_after_a_gap() {
        assert_eq!(
            title_text(TimerStatus::Running, 3600 + 23 * 60 + 59),
            Some("  1:23".to_string())
        );
        assert_eq!(
            title_text(TimerStatus::Paused, 47 * 60),
            Some("  ⏸ 0:47".to_string())
        );
        assert_eq!(title_text(TimerStatus::Idle, 0), None);
    }

    #[test]
    fn title_pads_minutes_and_does_not_wrap_hours() {
        assert_eq!(
            title_text(TimerStatus::Running, 5 * 60),
            Some("  0:05".to_string())
        );
        assert_eq!(
            title_text(TimerStatus::Running, 13 * 3600 + 60),
            Some("  13:01".to_string())
        );
        assert_eq!(
            title_text(TimerStatus::Running, -10),
            Some("  0:00".to_string())
        );
    }

    #[test]
    fn tooltip_names_status_time_and_project() {
        assert_eq!(
            tooltip_text(TimerStatus::Running, 83 * 60, Some("官網改版")),
            "計時中 1:23・官網改版"
        );
        assert_eq!(
            tooltip_text(TimerStatus::Paused, 47 * 60, Some("官網改版")),
            "暫停中 ⏸ 0:47・官網改版"
        );
        assert_eq!(tooltip_text(TimerStatus::Idle, 0, None), "打工人計時器");
    }

    #[test]
    fn shown_cache_reports_only_changes() {
        let mut shown = Shown {
            running: false,
            frame: 0,
            title: None,
            tooltip: APP_NAME.to_string(),
        };
        let none = Changes {
            icon_running: None,
            title: None,
            tooltip: None,
        };
        assert_eq!(shown.apply(false, None, APP_NAME.into()), none);
        let start = shown.apply(true, Some("  0:00".into()), "計時中 0:00".into());
        assert_eq!(
            start,
            Changes {
                icon_running: Some(true),
                title: Some(Some("  0:00".into())),
                tooltip: Some("計時中 0:00".into()),
            }
        );
        // Same second again: nothing to push.
        assert_eq!(
            shown.apply(true, Some("  0:00".into()), "計時中 0:00".into()),
            none
        );
        // A new minute: only the texts.
        shown.frame = 3;
        let minute = shown.apply(true, Some("  0:01".into()), "計時中 0:01".into());
        assert_eq!(minute.icon_running, None);
        assert_eq!(minute.title, Some(Some("  0:01".into())));
        assert_eq!(shown.frame, 3, "animation frame kept while running");
        // Stop: resting icon, title cleared, frame reset.
        let stop = shown.apply(false, None, APP_NAME.into());
        assert_eq!(stop.icon_running, Some(false));
        assert_eq!(stop.title, Some(None));
        assert_eq!(shown.frame, 0);
    }

    #[test]
    fn tooltip_without_project_name_drops_the_separator() {
        assert_eq!(tooltip_text(TimerStatus::Running, 60, None), "計時中 0:01");
    }
}
