# Manual verification checklist

## macOS

- [ ] Tray icon is clear on both light and dark menu bars (rest and awake frames).
- [ ] Launching the app opens no window at all, and no Dock icon appears (LSUIElement / Accessory policy).
- [ ] The app shows as 「打工人計時器」 in Launchpad, the Applications folder (note whether Finder shows `partimer.app` instead), Activity Monitor and Login Items.
- [ ] Opening the app a second time (double-click in Applications) shows the punch panel near the top centre of the main screen, on top and focused; no second process stays running.
- [ ] Closing the records / settings window leaves the app running.

## Windows

- [ ] Tray icon is clear in the Windows system tray (light and dark taskbar, 16px and scaled).
- [ ] Launching the app opens no window and no taskbar button; the process keeps running (Task Manager shows `partimer.exe`).
- [ ] Start menu entry and Settings → Apps → Installed apps show 「打工人計時器」.
- [ ] Launching the app a second time shows the punch panel near the top centre of the main screen, on top and focused, with no taskbar button; no second process stays running.
- [ ] Closing the records / settings window leaves the app running; Alt+F4 on the panel only hides it.

## Ubuntu

- [ ] Tray icon is clear in the Ubuntu top bar / indicator area (light and dark themes).
- [ ] Launching `partimer` opens no window; the process keeps running (`pgrep -x partimer`).
- [ ] The app list shows 「打工人計時器」; the terminal command is `partimer`.
- [ ] Running `partimer` again shows the punch panel near the top centre of the main screen, on top and focused (checked headless under Xvfb only: panel 360×480 at 20% from the top, centred).
- [ ] Closing the records / settings window leaves the app running.
