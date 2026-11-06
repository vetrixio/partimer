// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Wayland forbids apps from keeping windows on top or placing them; go through XWayland
    // unless the user chose a backend.
    #[cfg(target_os = "linux")]
    if std::env::var_os("GDK_BACKEND").is_none() {
        // SAFETY: first statement of `main`, before any other thread exists.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }
    partimer_lib::run()
}
