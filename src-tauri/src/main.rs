// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // SAFETY: first statement of `main`, before any other thread exists.
    unsafe { partimer_lib::platform::prefer_x11_backend() };
    partimer_lib::run()
}
