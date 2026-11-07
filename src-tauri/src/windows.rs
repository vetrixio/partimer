//! Creating, placing and toggling the app's windows.
//!
//! The public functions do their work on a worker thread: on Windows, building a WebView2
//! window from the main thread (sync commands, tray/menu handlers, plugin callbacks) deadlocks.
//! So every caller can use them without thinking about which thread it is on.

use std::sync::Mutex;

use chrono::Utc;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition, PhysicalRect,
    PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use partimer_core::TimerStatus;
use partimer_core::model::WidgetSettings;

use crate::{AppState, lock, platform};

pub const PANEL: &str = "panel";
pub const RECORDS: &str = "records";
pub const SETTINGS: &str = "settings";
pub const WIDGET: &str = "widget";

const PANEL_WIDTH: f64 = 360.0;
const PANEL_HEIGHT: f64 = 480.0;

/// The floating clock, in logical pixels.
const WIDGET_WIDTH: f64 = 130.0;
const WIDGET_HEIGHT: f64 = 40.0;
/// Default place: this far from the primary screen's right and top edges (logical pixels).
const WIDGET_MARGIN_RIGHT: f64 = 24.0;
const WIDGET_MARGIN_TOP: f64 = 64.0;

/// Serialises the worker jobs, so two of them (e.g. a double click on 設定, or the ticker and
/// the settings window both calling `ensure_widget`) never both see "no window" and build the
/// same label twice. Taken before the session lock, never the other way round, and never on
/// the main thread.
static WINDOW_OPS: Mutex<()> = Mutex::new(());

/// Where the panel was asked to open from; decides where it is placed.
#[derive(Debug, Clone, Copy)]
pub enum Origin {
    /// Tray icon click, with the icon's rectangle in physical pixels.
    Tray(PhysicalRect<i32, u32>),
    /// Tray menu without a known icon position (Linux AppIndicator never reports it):
    /// top-right corner of the main screen.
    TrayMenu,
    Shortcut,
    SecondInstance,
    Widget,
}

pub fn show_panel(app: &AppHandle, origin: Origin) {
    on_worker(app, move |app| show_panel_now(app, origin));
}

pub fn hide_panel(app: &AppHandle) {
    on_worker(app, |app| {
        if let Some(w) = app.get_webview_window(PANEL) {
            w.hide()?;
        }
        Ok(())
    });
}

pub fn toggle_panel(app: &AppHandle, origin: Origin) {
    on_worker(app, move |app| match app.get_webview_window(PANEL) {
        Some(w) if w.is_visible()? => w.hide(),
        _ => show_panel_now(app, origin),
    });
}

/// Open (or focus) the records window; with `entry_id`, have it open that entry.
///
/// The id is parked in `AppState::pending_entry` (a fresh window takes it on mount, after its
/// listener exists) and also emitted for a window that is already running.
pub fn open_records(app: &AppHandle, entry_id: Option<String>) {
    on_worker(app, move |app| {
        *crate::lock(&app.state::<AppState>().pending_entry) = entry_id.clone();
        if let Some(w) = app.get_webview_window(RECORDS) {
            focus(&w)?;
            if let Some(id) = entry_id {
                w.emit_to(RECORDS, "open-entry", Some(id))?;
            }
            return Ok(());
        }
        WebviewWindowBuilder::new(app, RECORDS, WebviewUrl::App("index.html".into()))
            .title("紀錄 — 打工人計時器")
            .inner_size(900.0, 640.0)
            .build()?;
        Ok(())
    });
}

pub fn open_settings(app: &AppHandle) {
    on_worker(app, |app| {
        if let Some(w) = app.get_webview_window(SETTINGS) {
            return focus(&w);
        }
        WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("index.html".into()))
            .title("設定 — 打工人計時器")
            .inner_size(560.0, 640.0)
            .build()?;
        Ok(())
    });
}

/// Create or remove the floating clock to match the widget settings and the timer state.
/// Called by the ticker on its first tick and whenever the status changes, and after the
/// widget settings change.
///
/// "Hidden" means destroyed, not `hide()`: showing an existing window again activates it on
/// Windows (`SW_SHOW`) and macOS (`makeKeyAndOrderFront`), which would take the focus away
/// from whatever the user is typing in. A new window built with `focused(false)` does not.
pub fn ensure_widget(app: &AppHandle) {
    on_worker(app, ensure_widget_now);
}

fn on_worker(app: &AppHandle, f: impl FnOnce(&AppHandle) -> tauri::Result<()> + Send + 'static) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _ops = lock(&WINDOW_OPS);
        if let Err(e) = f(&app) {
            eprintln!("partimer: window: {e}");
        }
    });
}

fn ensure_widget_now(app: &AppHandle) -> tauri::Result<()> {
    let Some(state) = app.try_state::<AppState>() else {
        return Ok(());
    };
    // Snapshot under the session lock, then let go before touching any window.
    let (visible, saved) = {
        let session = lock(&state.session);
        let widget = &session.settings().widget;
        let status = session.panel_state(Utc::now()).status;
        let visible = widget_wanted(widget, status);
        let saved = widget
            .x
            .zip(widget.y)
            .map(|(x, y)| PhysicalPosition::new(x, y));
        (visible, saved)
    };
    match (app.get_webview_window(WIDGET), visible) {
        (Some(w), false) => w.destroy(),
        (None, true) => build_widget(app, saved),
        _ => Ok(()),
    }
}

fn build_widget(app: &AppHandle, saved: Option<PhysicalPosition<i32>>) -> tauri::Result<()> {
    let screens = app
        .available_monitors()?
        .iter()
        .map(Screen::of)
        .collect::<Vec<_>>();
    let primary = app.primary_monitor()?.as_ref().map(Screen::of);
    let mut builder = WebviewWindowBuilder::new(app, WIDGET, WebviewUrl::App("index.html".into()))
        .title("打工人計時器")
        .inner_size(WIDGET_WIDTH, WIDGET_HEIGHT)
        // GTK makes a non-resizable window at least as big as the webview's natural size
        // (200x200 seen under X11), so on Linux the window stays "resizable" and the equal
        // min/max size hints keep it at 130x40 (window managers then offer no resizing).
        .min_inner_size(WIDGET_WIDTH, WIDGET_HEIGHT)
        .max_inner_size(WIDGET_WIDTH, WIDGET_HEIGHT)
        .resizable(cfg!(target_os = "linux"))
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible_on_all_workspaces(true)
        // macOS: the first click on the inactive clock reaches the page (toggles the panel)
        // instead of only activating the window.
        .accept_first_mouse(true);
    if let Some((pos, scale)) = widget_position(saved, &screens, primary) {
        // The builder takes logical pixels; dividing by the chosen screen's scale makes
        // Windows (which converts with the DPI of the screen the point falls on) land on
        // the same physical pixel.
        builder = builder.position(pos.x as f64 / scale, pos.y as f64 / scale);
    }
    builder.build()?;
    Ok(())
}

/// `widget_visible` for the stored settings (resolving the platform default).
pub fn widget_wanted(widget: &WidgetSettings, status: TimerStatus) -> bool {
    widget_visible(
        platform::widget_enabled(widget.enabled),
        status,
        widget.show_when_idle,
    )
}

/// Whether the floating clock window currently exists.
pub fn widget_exists(app: &AppHandle) -> bool {
    app.get_webview_window(WIDGET).is_some()
}

/// Whether the floating clock should be on screen: switched on, and timing (running or
/// paused) unless the user asked to see it while idle too.
fn widget_visible(enabled: bool, status: TimerStatus, show_when_idle: bool) -> bool {
    enabled && (status != TimerStatus::Idle || show_when_idle)
}

/// A monitor as placement needs it: bounds in physical pixels and its scale factor.
#[derive(Debug, Clone, Copy)]
struct Screen {
    rect: PhysicalRect<i32, u32>,
    scale: f64,
}

impl Screen {
    fn of(m: &Monitor) -> Self {
        Self {
            rect: monitor_rect(m),
            scale: m.scale_factor(),
        }
    }

    /// The widget's size in physical pixels on this screen.
    fn widget_size(&self) -> (i64, i64) {
        (
            (WIDGET_WIDTH * self.scale).round() as i64,
            (WIDGET_HEIGHT * self.scale).round() as i64,
        )
    }

    /// Whether the whole widget at `pos` lies within this screen.
    fn holds_widget_at(&self, pos: PhysicalPosition<i32>) -> bool {
        let (w, h) = self.widget_size();
        let (x, y) = (pos.x as i64, pos.y as i64);
        let left = self.rect.position.x as i64;
        let top = self.rect.position.y as i64;
        x >= left
            && y >= top
            && x + w <= left + self.rect.size.width as i64
            && y + h <= top + self.rect.size.height as i64
    }

    fn widget_top_right(&self) -> PhysicalPosition<i32> {
        let from_right = ((WIDGET_MARGIN_RIGHT + WIDGET_WIDTH) * self.scale).round() as i32;
        let from_top = (WIDGET_MARGIN_TOP * self.scale).round() as i32;
        PhysicalPosition::new(
            self.rect.position.x + self.rect.size.width as i32 - from_right,
            self.rect.position.y + from_top,
        )
    }
}

/// Where the widget's top-left corner goes (physical pixels, like the saved `widget.x/y`
/// that `widget_moved` stores from the window's `onMoved` event), with the scale of the
/// screen it lands on. The saved position is kept only when the whole widget lies within one
/// screen; otherwise top-right of the primary screen (or the first one when the system names
/// no primary). `None` when there are no screens at all.
fn widget_position(
    saved: Option<PhysicalPosition<i32>>,
    screens: &[Screen],
    primary: Option<Screen>,
) -> Option<(PhysicalPosition<i32>, f64)> {
    if let Some(pos) = saved
        && let Some(s) = screens.iter().find(|s| s.holds_widget_at(pos))
    {
        return Some((pos, s.scale));
    }
    let main = primary.or_else(|| screens.first().copied())?;
    Some((main.widget_top_right(), main.scale))
}

fn focus(w: &WebviewWindow) -> tauri::Result<()> {
    w.unminimize()?;
    w.show()?;
    w.set_focus()
}

fn show_panel_now(app: &AppHandle, origin: Origin) -> tauri::Result<()> {
    let w = match app.get_webview_window(PANEL) {
        Some(w) => w,
        None => WebviewWindowBuilder::new(app, PANEL, WebviewUrl::App("index.html".into()))
            .title("打工人計時器")
            .inner_size(PANEL_WIDTH, PANEL_HEIGHT)
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .build()?,
    };
    let mut size = w.outer_size()?;
    if size.width == 0 || size.height == 0 {
        // A window that was never shown reports 0x0 on Linux; use the size we asked for.
        size = LogicalSize::new(PANEL_WIDTH, PANEL_HEIGHT).to_physical(w.scale_factor()?);
    }
    if let Some(pos) = panel_position(app, origin, size)? {
        w.set_position(pos)?;
    }
    w.show()?;
    w.set_focus()
}

fn panel_position(
    app: &AppHandle,
    origin: Origin,
    size: PhysicalSize<u32>,
) -> tauri::Result<Option<PhysicalPosition<i32>>> {
    Ok(match origin {
        Origin::Tray(tray) => {
            let (cx, cy) = center(tray);
            let monitor = match app.monitor_from_point(cx as f64, cy as f64)? {
                Some(m) => Some(m),
                None => app.primary_monitor()?,
            };
            monitor.map(|m| tray_position(tray, size, monitor_rect(&m), *m.work_area()))
        }
        Origin::TrayMenu => app
            .primary_monitor()?
            .map(|m| top_right_position(*m.work_area(), size)),
        Origin::Shortcut | Origin::SecondInstance | Origin::Widget => app
            .primary_monitor()?
            .map(|m| upper_center_position(*m.work_area(), size)),
    })
}

fn monitor_rect(m: &Monitor) -> PhysicalRect<i32, u32> {
    PhysicalRect {
        position: *m.position(),
        size: *m.size(),
    }
}

fn center(r: PhysicalRect<i32, u32>) -> (i32, i32) {
    (
        r.position.x + (r.size.width / 2) as i32,
        r.position.y + (r.size.height / 2) as i32,
    )
}

/// Keep `start..start+len` inside `area_start..area_start+area_len`; when it does not fit,
/// align with the area's start.
fn clamp_axis(start: i32, len: u32, area_start: i32, area_len: u32) -> i32 {
    let max = area_start + area_len as i32 - len as i32;
    start.min(max).max(area_start)
}

fn clamp_into(
    pos: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    area: PhysicalRect<i32, u32>,
) -> PhysicalPosition<i32> {
    PhysicalPosition::new(
        clamp_axis(pos.x, size.width, area.position.x, area.size.width),
        clamp_axis(pos.y, size.height, area.position.y, area.size.height),
    )
}

/// Under the tray icon, or above it when the icon is in the lower half of its screen
/// (bottom taskbar), horizontally centred on the icon and kept inside the work area.
fn tray_position(
    tray: PhysicalRect<i32, u32>,
    panel: PhysicalSize<u32>,
    monitor: PhysicalRect<i32, u32>,
    work_area: PhysicalRect<i32, u32>,
) -> PhysicalPosition<i32> {
    let (cx, cy) = center(tray);
    let (_, monitor_cy) = center(monitor);
    let x = cx - (panel.width / 2) as i32;
    let y = if cy > monitor_cy {
        tray.position.y - panel.height as i32
    } else {
        tray.position.y + tray.size.height as i32
    };
    clamp_into(PhysicalPosition::new(x, y), panel, work_area)
}

/// Horizontally centred, top edge at 20% of the work area's height.
fn upper_center_position(
    work_area: PhysicalRect<i32, u32>,
    panel: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    let x = work_area.position.x + (work_area.size.width as i32 - panel.width as i32) / 2;
    let y = work_area.position.y + (work_area.size.height / 5) as i32;
    clamp_into(PhysicalPosition::new(x, y), panel, work_area)
}

/// Top-right corner of the work area (under a top bar, beside the indicator area).
fn top_right_position(
    work_area: PhysicalRect<i32, u32>,
    panel: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    let x = work_area.position.x + work_area.size.width as i32 - panel.width as i32;
    clamp_into(
        PhysicalPosition::new(x, work_area.position.y),
        panel,
        work_area,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect<i32, u32> {
        PhysicalRect {
            position: PhysicalPosition::new(x, y),
            size: PhysicalSize::new(w, h),
        }
    }

    const PANEL_SIZE: PhysicalSize<u32> = PhysicalSize::new(360, 480);

    #[test]
    fn tray_at_top_puts_panel_below_icon() {
        // macOS menu bar: screen 1920x1080, work area starts under the 25px bar.
        let monitor = rect(0, 0, 1920, 1080);
        let work = rect(0, 25, 1920, 1055);
        let pos = tray_position(rect(1500, 0, 30, 24), PANEL_SIZE, monitor, work);
        assert_eq!(pos, PhysicalPosition::new(1515 - 180, 25));
    }

    #[test]
    fn tray_at_bottom_puts_panel_above_icon() {
        // Windows taskbar at the bottom: work area ends at 1040.
        let monitor = rect(0, 0, 1920, 1080);
        let work = rect(0, 0, 1920, 1040);
        let pos = tray_position(rect(1700, 1044, 32, 32), PANEL_SIZE, monitor, work);
        assert_eq!(pos, PhysicalPosition::new(1716 - 180, 1040 - 480));
    }

    #[test]
    fn tray_near_screen_edge_is_clamped_into_work_area() {
        let monitor = rect(0, 0, 1920, 1080);
        let work = rect(0, 0, 1920, 1040);
        let pos = tray_position(rect(1900, 1044, 20, 32), PANEL_SIZE, monitor, work);
        assert_eq!(pos, PhysicalPosition::new(1920 - 360, 560));
    }

    #[test]
    fn tray_on_secondary_monitor_uses_that_monitor() {
        // Second screen to the right, starting at x=1920, with a top bar.
        let monitor = rect(1920, 0, 2560, 1440);
        let work = rect(1920, 30, 2560, 1410);
        let pos = tray_position(rect(4000, 2, 28, 26), PANEL_SIZE, monitor, work);
        assert_eq!(pos, PhysicalPosition::new(4014 - 180, 30));
    }

    #[test]
    fn upper_center_is_centered_at_twenty_percent() {
        let pos = upper_center_position(rect(0, 0, 1920, 1040), PANEL_SIZE);
        assert_eq!(pos, PhysicalPosition::new(780, 208));
        let offset = upper_center_position(rect(-1280, 100, 1280, 1000), PANEL_SIZE);
        assert_eq!(offset, PhysicalPosition::new(-1280 + 460, 300));
    }

    #[test]
    fn top_right_sits_in_the_work_area_corner() {
        // Ubuntu: top bar 32px, so the work area starts at y=32.
        let pos = top_right_position(rect(0, 32, 1920, 1048), PANEL_SIZE);
        assert_eq!(pos, PhysicalPosition::new(1920 - 360, 32));
        let offset = top_right_position(rect(1920, 0, 1280, 1024), PANEL_SIZE);
        assert_eq!(offset, PhysicalPosition::new(1920 + 1280 - 360, 0));
    }

    #[test]
    fn widget_shows_only_when_enabled_and_timing_or_always_on() {
        use TimerStatus::*;
        for status in [Idle, Running, Paused] {
            assert!(!widget_visible(false, status, false));
            assert!(!widget_visible(false, status, true));
        }
        assert!(widget_visible(true, Running, false));
        assert!(widget_visible(true, Paused, false));
        assert!(!widget_visible(true, Idle, false));
        assert!(widget_visible(true, Idle, true));
    }

    fn screen(x: i32, y: i32, w: u32, h: u32, scale: f64) -> Screen {
        Screen {
            rect: rect(x, y, w, h),
            scale,
        }
    }

    const MAIN: Screen = Screen {
        rect: PhysicalRect {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(1920, 1080),
        },
        scale: 1.0,
    };

    #[test]
    fn widget_without_saved_position_goes_top_right_of_primary() {
        let pos = widget_position(None, &[MAIN], Some(MAIN));
        assert_eq!(pos, Some((PhysicalPosition::new(1920 - 24 - 130, 64), 1.0)));
    }

    #[test]
    fn widget_default_uses_logical_margins_on_hidpi_and_offset_primary() {
        let hidpi = screen(0, 0, 3840, 2160, 2.0);
        assert_eq!(
            widget_position(None, &[hidpi], Some(hidpi)),
            Some((PhysicalPosition::new(3840 - 48 - 260, 128), 2.0))
        );
        let offset = screen(-1280, 200, 1280, 1024, 1.25);
        // 24 + 130 logical = 192.5 physical, 64 logical = 80 physical.
        assert_eq!(
            widget_position(None, &[MAIN, offset], Some(offset)),
            Some((PhysicalPosition::new(-1280 + 1280 - 193, 280), 1.25))
        );
    }

    #[test]
    fn widget_keeps_a_saved_position_inside_any_screen() {
        let left = screen(-2560, -200, 2560, 1440, 1.0);
        let screens = [MAIN, left];
        let saved = PhysicalPosition::new(100, 900);
        assert_eq!(
            widget_position(Some(saved), &screens, Some(MAIN)),
            Some((saved, 1.0))
        );
        let on_left = PhysicalPosition::new(-2000, -150);
        assert_eq!(
            widget_position(Some(on_left), &screens, Some(MAIN)),
            Some((on_left, 1.0))
        );
        // Exactly touching the bottom-right corner still fits.
        let corner = PhysicalPosition::new(1920 - 130, 1080 - 40);
        assert_eq!(
            widget_position(Some(corner), &screens, Some(MAIN)),
            Some((corner, 1.0))
        );
    }

    #[test]
    fn widget_saved_position_reports_the_scale_of_its_screen() {
        let hidpi = screen(1920, 0, 2880, 1800, 2.0);
        let saved = PhysicalPosition::new(2000, 100);
        assert_eq!(
            widget_position(Some(saved), &[MAIN, hidpi], Some(MAIN)),
            Some((saved, 2.0))
        );
    }

    #[test]
    fn widget_saved_position_off_every_screen_falls_back_to_top_right() {
        let fallback = Some((PhysicalPosition::new(1920 - 154, 64), 1.0));
        // A monitor that is no longer connected.
        let gone = PhysicalPosition::new(3000, 300);
        assert_eq!(widget_position(Some(gone), &[MAIN], Some(MAIN)), fallback);
        // Partly past the right edge.
        let straddling = PhysicalPosition::new(1920 - 100, 300);
        assert_eq!(
            widget_position(Some(straddling), &[MAIN], Some(MAIN)),
            fallback
        );
        // Above the top.
        let above = PhysicalPosition::new(500, -10);
        assert_eq!(widget_position(Some(above), &[MAIN], Some(MAIN)), fallback);
        // Too big at this screen's scale: 130x40 logical is 260x80 at 2x.
        let hidpi = screen(0, 0, 1920, 1080, 2.0);
        let near_edge = PhysicalPosition::new(1920 - 200, 0);
        assert_eq!(
            widget_position(Some(near_edge), &[hidpi], Some(hidpi)),
            Some((PhysicalPosition::new(1920 - 308, 128), 2.0))
        );
    }

    #[test]
    fn widget_spanning_two_screens_is_not_inside_either() {
        let right = screen(1920, 0, 1920, 1080, 1.0);
        let across = PhysicalPosition::new(1920 - 60, 500);
        assert_eq!(
            widget_position(Some(across), &[MAIN, right], Some(MAIN)),
            Some((PhysicalPosition::new(1920 - 154, 64), 1.0))
        );
    }

    #[test]
    fn widget_without_primary_uses_first_screen_and_without_screens_none() {
        let only = screen(100, 50, 1600, 900, 1.0);
        assert_eq!(
            widget_position(None, &[only], None),
            Some((PhysicalPosition::new(100 + 1600 - 154, 50 + 64), 1.0))
        );
        assert_eq!(
            widget_position(Some(PhysicalPosition::new(0, 0)), &[], None),
            None
        );
    }

    #[test]
    fn panel_larger_than_work_area_aligns_to_its_top_left() {
        let pos = upper_center_position(rect(0, 0, 300, 400), PANEL_SIZE);
        assert_eq!(pos, PhysicalPosition::new(0, 0));
    }
}
