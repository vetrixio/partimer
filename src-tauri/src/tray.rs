//! The tray / menu bar icon: pictures, awake animation, title and tooltip, menu.
//!
//! `update` and `advance_frame` are called from the ticker thread only (never while the
//! session lock is held); the tray API forwards each call to the main thread itself.
//! `refresh_menu` is called from the ticker and after each menu action, also without the
//! session lock held.

use std::sync::Mutex;

use std::thread;

use chrono::{DateTime, Utc};
use partimer_core::session::TrayPick;
use partimer_core::tray_menu::{PinItem, TrayMenu};
use partimer_core::{RealFs, Session, TimerStatus};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalRect, Rect, Wry};

use crate::windows::{self, Origin};
use crate::{AppState, commands, lock, moving, ticker};

const TRAY_ID: &str = "partimer";
const APP_NAME: &str = "打工人計時器";

const MENU_PANEL: &str = "open-panel";
const MENU_RECORDS: &str = "records";
const MENU_SETTINGS: &str = "settings";
const MENU_QUIT: &str = "quit";
const MENU_PIN_PREFIX: &str = "pin:";
const MENU_PAUSE: &str = "pause";
const MENU_RESUME: &str = "resume";
const MENU_STOP: &str = "stop";
// Disabled rows; never clicked.
const MENU_HINT: &str = "hint";
const MENU_STATUS: &str = "status";
const MENU_OTHERS: &str = "others";

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
    /// The model the tray menu was last built from; the menu is replaced only when it changes.
    /// Read and written on the main thread only (see `refresh_menu`).
    menu: Mutex<TrayMenu>,
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

/// One row of the tray menu, before it becomes a native menu item.
#[derive(Debug, PartialEq)]
enum Row {
    Item {
        id: String,
        text: String,
        enabled: bool,
    },
    Separator,
}

fn item(id: &str, text: String, enabled: bool) -> Row {
    Row::Item {
        id: id.to_string(),
        text,
        enabled,
    }
}

/// A name shown literally: muda reads `&` as a mnemonic marker and `&&` as a plain `&`.
/// (`_`, GTK's marker, is escaped by muda itself.)
fn menu_label(label: &str) -> String {
    label.replace('&', "&&")
}

fn pin_rows(pins: &[PinItem]) -> impl Iterator<Item = Row> + '_ {
    pins.iter().map(|p| Row::Item {
        id: format!("{MENU_PIN_PREFIX}{}", p.project_id),
        text: format!("▶ {}", menu_label(&p.label)),
        enabled: true,
    })
}

/// The whole tray menu for a model: the timer part (spec §4.1 / §4.2), then the fixed tail.
fn menu_spec(model: &TrayMenu) -> Vec<Row> {
    let mut rows = Vec::new();
    match model {
        TrayMenu::Idle { pins } if pins.is_empty() => {
            rows.push(item(
                MENU_HINT,
                "到設定釘選專案，就能從這裡開始計時".into(),
                false,
            ));
        }
        TrayMenu::Idle { pins } => rows.extend(pin_rows(pins)),
        TrayMenu::Active {
            paused,
            current,
            others,
        } => {
            let (status, toggle_id, toggle) = if *paused {
                ("暫停中", MENU_RESUME, "繼續")
            } else {
                ("計時中", MENU_PAUSE, "暫停")
            };
            rows.push(item(
                MENU_STATUS,
                format!("{status}：{}", menu_label(current)),
                false,
            ));
            rows.push(item(toggle_id, toggle.into(), true));
            rows.push(item(MENU_STOP, "停止計時".into(), true));
            if !others.is_empty() {
                rows.push(Row::Separator);
                rows.push(item(MENU_OTHERS, "改記其他專案：".into(), false));
                rows.extend(pin_rows(others));
            }
        }
    }
    rows.extend([
        Row::Separator,
        item(MENU_PANEL, "打開面板".into(), true),
        item(MENU_RECORDS, "紀錄".into(), true),
        item(MENU_SETTINGS, "設定".into(), true),
        Row::Separator,
        item(MENU_QUIT, "結束".into(), true),
    ]);
    rows
}

fn build_menu(app: &AppHandle, rows: &[Row]) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    for row in rows {
        match row {
            Row::Item { id, text, enabled } => {
                menu.append(&MenuItem::with_id(app, id, text, *enabled, None::<&str>)?)?
            }
            Row::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
        }
    }
    Ok(menu)
}

fn current_menu(app: &AppHandle) -> TrayMenu {
    app.try_state::<AppState>()
        .map(|state| lock(&state.session).tray_menu())
        .unwrap_or(TrayMenu::Idle { pins: Vec::new() })
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let model = current_menu(app);
    // Always attached: it is the right-click menu on macOS / Windows and the only way to
    // interact on Linux, where AppIndicator never delivers left clicks to the app.
    let menu = build_menu(app, &menu_spec(&model))?;
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
        menu: Mutex::new(model),
    };

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(state.rest.clone())
        .tooltip(APP_NAME)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()))
        .on_tray_icon_event(on_tray_event)
        .build(app)?;

    app.manage(state);
    Ok(())
}

/// Runs on the main thread. Timer actions follow the state at the moment of the click, not
/// the (possibly stale) menu that was clicked.
fn on_menu_event(app: &AppHandle, id: &str) {
    match id {
        MENU_PANEL => windows::show_panel(app, menu_origin(app)),
        MENU_RECORDS => windows::open_records(app, None),
        MENU_SETTINGS => windows::open_settings(app),
        MENU_QUIT => app.exit(0),
        MENU_PAUSE => act(app, |s, now| s.tray_pause(now)),
        MENU_RESUME => act(app, |s, now| s.tray_resume(now)),
        MENU_STOP => act(app, |s, now| s.tray_stop(now)),
        _ => {
            if let Some(project_id) = id.strip_prefix(MENU_PIN_PREFIX) {
                pick(app, project_id);
            }
        }
    }
}

/// Run one session action under the lock, then report it with the lock released.
fn act(
    app: &AppHandle,
    f: impl FnOnce(&mut Session<RealFs>, DateTime<Utc>) -> Result<bool, String>,
) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let result = f(&mut lock(&state.session), Utc::now());
    finish(app, result);
}

/// `Ok(false)`: the click was stale and nothing changed, which needs no notification.
fn finish(app: &AppHandle, result: Result<bool, String>) {
    match result {
        Ok(true) => commands::changed(app),
        Ok(false) => {}
        Err(msg) => ticker::notify(app, &msg),
    }
    refresh_menu(app);
}

fn pick(app: &AppHandle, project_id: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let picked = lock(&state.session).tray_pick(project_id, Utc::now());
    match picked {
        Ok(TrayPick::Started) => finish(app, Ok(true)),
        Ok(TrayPick::NoChange) => finish(app, Ok(false)),
        Ok(TrayPick::ConfirmSwitch { running_entry }) => {
            confirm_switch(app, project_id.to_string(), running_entry)
        }
        Err(msg) => finish(app, Err(msg)),
    }
}

/// The native dialog blocks, so it runs on its own thread (never on the main thread), and the
/// session lock is not held while it is open. `tray_switch` rechecks the state afterwards.
fn confirm_switch(app: &AppHandle, project_id: String, running_entry: String) {
    let handle = app.clone();
    let spawned = thread::Builder::new()
        .name("partimer-tray-confirm".into())
        .spawn(move || {
            let app = handle;
            let ok = moving::confirm(
                &app,
                APP_NAME,
                "結束目前這筆，改記另一個專案？".into(),
                "結束並切換",
            );
            if ok {
                act(&app, |s, now| {
                    s.tray_switch(&project_id, &running_entry, now)
                });
            }
        });
    if let Err(e) = spawned {
        eprintln!("partimer: tray: cannot open the confirm dialog: {e}");
        ticker::notify(app, &format!("無法顯示確認視窗：{e}"));
        refresh_menu(app);
    }
}

/// Rebuild the tray menu if the timer state or the pins changed since it was last built.
/// Call without holding the session lock.
pub fn refresh_menu(app: &AppHandle) {
    let handle = app.clone();
    // Compute, compare, replace and remember on the main thread only, so refreshes from the
    // ticker and from menu actions are applied one at a time, each from the state as it is when
    // it runs, and the remembered model is the shown one. The closure takes the session lock,
    // so callers must not hold it.
    let sent = app.run_on_main_thread(move || {
        let model = current_menu(&handle);
        let (Some(state), Some(tray)) =
            (handle.try_state::<TrayState>(), handle.tray_by_id(TRAY_ID))
        else {
            return;
        };
        if *lock(&state.menu) == model {
            return;
        }
        let set = build_menu(&handle, &menu_spec(&model)).and_then(|m| tray.set_menu(Some(m)));
        match set {
            Ok(()) => *lock(&state.menu) = model,
            Err(e) => eprintln!("partimer: tray: {e}"),
        }
    });
    log(sent);
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

    fn pin(id: &str, label: &str) -> PinItem {
        PinItem {
            project_id: id.into(),
            label: label.into(),
        }
    }

    fn item(id: &str, text: &str, enabled: bool) -> Row {
        Row::Item {
            id: id.into(),
            text: text.into(),
            enabled,
        }
    }

    fn tail() -> Vec<Row> {
        vec![
            Row::Separator,
            item(MENU_PANEL, "打開面板", true),
            item(MENU_RECORDS, "紀錄", true),
            item(MENU_SETTINGS, "設定", true),
            Row::Separator,
            item(MENU_QUIT, "結束", true),
        ]
    }

    fn with_tail(mut rows: Vec<Row>) -> Vec<Row> {
        rows.extend(tail());
        rows
    }

    #[test]
    fn escape_ampersand() {
        assert_eq!(menu_label("A&B_c"), "A&&B_c");
        assert_eq!(menu_label("A&&B"), "A&&&&B");
    }

    #[test]
    fn idle_spec_rows() {
        let model = TrayMenu::Idle {
            pins: vec![pin("p1", "王小明・官網改版"), pin("p2", "R&D・設計")],
        };
        assert_eq!(
            menu_spec(&model),
            with_tail(vec![
                item("pin:p1", "▶ 王小明・官網改版", true),
                item("pin:p2", "▶ R&&D・設計", true),
            ])
        );
    }

    #[test]
    fn idle_without_pins_shows_disabled_hint() {
        let model = TrayMenu::Idle { pins: vec![] };
        assert_eq!(
            menu_spec(&model),
            with_tail(vec![item(
                "hint",
                "到設定釘選專案，就能從這裡開始計時",
                false
            )])
        );
    }

    #[test]
    fn running_spec_rows() {
        let model = TrayMenu::Active {
            paused: false,
            current: "王小明・官網改版".into(),
            others: vec![pin("p2", "王小明・設計")],
        };
        assert_eq!(
            menu_spec(&model),
            with_tail(vec![
                item("status", "計時中：王小明・官網改版", false),
                item("pause", "暫停", true),
                item("stop", "停止計時", true),
                Row::Separator,
                item("others", "改記其他專案：", false),
                item("pin:p2", "▶ 王小明・設計", true),
            ])
        );
    }

    #[test]
    fn paused_spec_shows_resume() {
        let model = TrayMenu::Active {
            paused: true,
            current: "A&B・官網".into(),
            others: vec![],
        };
        let rows = menu_spec(&model);
        assert_eq!(rows[0], item("status", "暫停中：A&&B・官網", false));
        assert_eq!(rows[1], item("resume", "繼續", true));
        assert_eq!(rows[2], item("stop", "停止計時", true));
    }

    #[test]
    fn running_without_others_omits_section() {
        let model = TrayMenu::Active {
            paused: false,
            current: "王小明・官網改版".into(),
            others: vec![],
        };
        assert_eq!(
            menu_spec(&model),
            with_tail(vec![
                item("status", "計時中：王小明・官網改版", false),
                item("pause", "暫停", true),
                item("stop", "停止計時", true),
            ])
        );
    }

    #[test]
    fn tooltip_without_project_name_drops_the_separator() {
        assert_eq!(tooltip_text(TimerStatus::Running, 60, None), "計時中 0:01");
    }
}
