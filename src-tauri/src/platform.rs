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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_help_names_the_app_or_command() {
        let help = autostart_help();
        assert!(help.contains("打工人計時器") || help.contains("`partimer`"));
    }
}
