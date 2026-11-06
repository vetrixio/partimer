//! Creating, placing and toggling the app's windows.
//!
//! The public functions do their work on a worker thread: on Windows, building a WebView2
//! window from the main thread (sync commands, tray/menu handlers, plugin callbacks) deadlocks.
//! So every caller can use them without thinking about which thread it is on.

use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition, PhysicalRect,
    PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder, webview::PageLoadEvent,
};

pub const PANEL: &str = "panel";
pub const RECORDS: &str = "records";
pub const SETTINGS: &str = "settings";
pub const WIDGET: &str = "widget";

const PANEL_WIDTH: f64 = 360.0;
const PANEL_HEIGHT: f64 = 480.0;

/// Where the panel was asked to open from; decides where it is placed.
#[derive(Debug, Clone, Copy)]
pub enum Origin {
    /// Tray icon click, with the icon's rectangle in physical pixels.
    Tray(PhysicalRect<i32, u32>),
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

/// Open (or focus) the records window; with `entry_id`, tell it to open that entry.
pub fn open_records(app: &AppHandle, entry_id: Option<String>) {
    on_worker(app, move |app| {
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
            .on_page_load(move |w, payload| {
                if payload.event() == PageLoadEvent::Finished
                    && let Some(id) = entry_id.clone()
                    && let Err(e) = w.emit_to(RECORDS, "open-entry", Some(id))
                {
                    eprintln!("partimer: open-entry: {e}");
                }
            })
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

/// Create/show/hide the floating clock to match settings and timer state. Filled in by the
/// floating clock task; called at startup and whenever widget settings change.
pub fn ensure_widget(_app: &AppHandle) {}

fn on_worker(app: &AppHandle, f: impl FnOnce(&AppHandle) -> tauri::Result<()> + Send + 'static) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = f(&app) {
            eprintln!("partimer: window: {e}");
        }
    });
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
    fn panel_larger_than_work_area_aligns_to_its_top_left() {
        let pos = upper_center_position(rect(0, 0, 300, 400), PANEL_SIZE);
        assert_eq!(pos, PhysicalPosition::new(0, 0));
    }
}
