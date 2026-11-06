//! Platform-specific defaults and help texts.

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
    fn autostart_help_names_the_app_or_command() {
        let help = autostart_help();
        assert!(help.contains("打工人計時器") || help.contains("`partimer`"));
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
