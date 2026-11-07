//! Platform-specific defaults, help texts and process launching.

#[cfg(any(target_os = "linux", test))]
use std::ffi::OsStr;
use std::path::Path;
use std::sync::OnceLock;

/// Whether partimer itself set `GDK_BACKEND=x11` (see `prefer_x11_backend`).
static GDK_BACKEND_SET_BY_US: OnceLock<bool> = OnceLock::new();

/// Linux: Wayland forbids apps from keeping windows on top or placing them, so go through
/// XWayland unless the user chose a backend. Remembers whether it did, so child processes
/// (file manager…) are not pushed onto XWayland too.
///
/// # Safety
/// Must run before any other thread exists (first statement of `main`): it sets an
/// environment variable.
pub unsafe fn prefer_x11_backend() {
    let set = cfg!(target_os = "linux") && std::env::var_os("GDK_BACKEND").is_none();
    if set {
        // SAFETY: guaranteed single-threaded by the caller.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }
    let _ = GDK_BACKEND_SET_BY_US.set(set);
}

/// Child processes drop `GDK_BACKEND` only when partimer put it there and it still holds
/// partimer's value; a backend the user chose is passed on untouched.
#[cfg(any(target_os = "linux", test))]
fn should_strip_gdk_backend(set_by_us: bool, current: Option<&OsStr>) -> bool {
    set_by_us && current == Some(OsStr::new("x11"))
}

/// Open a folder in the system file manager. On Linux `xdg-open` is spawned directly so the
/// forced `GDK_BACKEND` can be removed from its environment; elsewhere the opener plugin.
#[cfg(target_os = "linux")]
pub fn open_folder(_app: &tauri::AppHandle, dir: &Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(dir);
    let set_by_us = GDK_BACKEND_SET_BY_US.get().copied().unwrap_or(false);
    if should_strip_gdk_backend(set_by_us, std::env::var_os("GDK_BACKEND").as_deref()) {
        cmd.env_remove("GDK_BACKEND");
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    // Reap it so no zombie is left behind; xdg-open returns once the file manager started.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn open_folder(app: &tauri::AppHandle, dir: &Path) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Manual steps shown in the settings window when the app cannot enable autostart itself.
pub fn autostart_help() -> String {
    #[cfg(target_os = "macos")]
    let text = "系統設定 → 一般 → 登入項目 → 按 + 加入 打工人計時器";
    #[cfg(target_os = "windows")]
    let text = "按 Win+R 輸入 `shell:startup`，把 打工人計時器 的捷徑放進去";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let text = "開啟『初始啟動程式』，新增指令 `partimer`";
    text.to_string()
}

/// Whether the floating clock is on when the user never chose: off on macOS (the menu bar
/// shows the time), on elsewhere.
pub fn widget_default_enabled() -> bool {
    !cfg!(target_os = "macos")
}

/// The stored `widget.enabled`, or the platform default when the user never chose.
pub fn widget_enabled(stored: Option<bool>) -> bool {
    stored.unwrap_or_else(widget_default_enabled)
}

/// Whether this is a Wayland desktop session (even when the app itself runs through XWayland):
/// global shortcuts may "register" there and still never fire.
pub fn is_wayland_session() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    is_wayland(session_type.as_deref(), wayland_display.as_deref())
}

fn is_wayland(session_type: Option<&str>, wayland_display: Option<&str>) -> bool {
    session_type == Some("wayland") || wayland_display.is_some_and(|d| !d.is_empty())
}

/// The alternative to the global shortcut, shown in red in the settings window on Wayland.
pub fn shortcut_help() -> Option<String> {
    is_wayland_session().then(|| {
        "Ubuntu 在 Wayland 下無法由 app 註冊全域快捷鍵。請到『設定 → 鍵盤 → 檢視及自訂快捷鍵 → 自訂快捷鍵』新增一組，指令填 `partimer`，按下就會叫出打卡面板。"
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_help_is_the_platform_text() {
        let expected = if cfg!(target_os = "macos") {
            "系統設定 → 一般 → 登入項目 → 按 + 加入 打工人計時器"
        } else if cfg!(target_os = "windows") {
            "按 Win+R 輸入 `shell:startup`，把 打工人計時器 的捷徑放進去"
        } else {
            "開啟『初始啟動程式』，新增指令 `partimer`"
        };
        assert_eq!(autostart_help(), expected);
    }

    #[test]
    fn gdk_backend_is_stripped_only_when_we_set_it() {
        let x11 = Some(OsStr::new("x11"));
        assert!(should_strip_gdk_backend(true, x11));
        // The user's own choice (we did not set it) is passed on.
        assert!(!should_strip_gdk_backend(false, x11));
        // Changed since, or already gone: leave it alone.
        assert!(!should_strip_gdk_backend(true, Some(OsStr::new("wayland"))));
        assert!(!should_strip_gdk_backend(true, None));
    }

    #[test]
    fn wayland_session_type_means_wayland() {
        assert!(is_wayland(Some("wayland"), None));
    }

    #[test]
    fn wayland_display_means_wayland_even_on_other_session_types() {
        assert!(is_wayland(Some("x11"), Some("wayland-0")));
        assert!(is_wayland(None, Some("wayland-0")));
    }

    #[test]
    fn x11_or_unset_is_not_wayland() {
        assert!(!is_wayland(Some("x11"), None));
        assert!(!is_wayland(None, None));
        // An exported but empty WAYLAND_DISPLAY has no value.
        assert!(!is_wayland(Some("x11"), Some("")));
    }
}
