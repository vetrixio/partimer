# Manual verification checklist

## macOS

- [ ] Tray icon is clear on both light and dark menu bars (rest and awake frames).
- [ ] Launching the app opens no window at all, and no Dock icon appears (LSUIElement / Accessory policy).
- [ ] The app shows as 「打工人計時器」 in Launchpad, the Applications folder (note whether Finder shows `partimer.app` instead), Activity Monitor and Login Items.
- [ ] Opening the app a second time (double-click in Applications) shows the punch panel near the top centre of the main screen, on top and focused; no second process stays running.
- [ ] Closing the records / settings window leaves the app running.
- [ ] Not timing: the menu bar shows the resting plant (drooping leaves, closed eyes, "z") with no text beside it.
- [ ] Timing: the plant is awake and its leaves sway (4 frames, one every 0.5 s); the time shows to the right as `1:23` with a visible gap between icon and digits; it updates when the minute changes.
- [ ] Paused: resting plant with `⏸ 0:47` beside it; no animation.
- [ ] Left-click on the icon opens the panel just below the icon; a second left-click hides it. Left-click shows no menu.
- [ ] Right-click shows the menu 打開面板／紀錄／設定／結束; each item works (結束 quits; the unsaved title/content typed in the panel is in the entry after restart).
- [ ] Ctrl+Option+P opens the panel near the top centre of the main screen, focused; pressing it again hides it. The settings window shows no red text about the shortcut.
- [ ] Notifications: on first reminder macOS asks for permission (or the app appears in System Settings → Notifications as 打工人計時器); if denied, nothing breaks.
- [ ] Hourly reminder on (settings): each full hour of running time shows 「已計時 N 小時・專案名」 once; nothing while paused; reopening the app after hours of running does not send a backlog.
- [ ] 12-hour reminder: after 12 h of effective time (e.g. lid closed overnight) one notification 「是不是忘了停？已計時 12 小時・專案名」, not repeated every second; after 「沒忘，繼續計時」 the next one comes at 24 h.
- [ ] Logging out / shutting down while timing keeps the unsaved title/content (OS quit path without the menu).

## Windows

- [ ] Tray icon is clear in the Windows system tray (light and dark taskbar, 16px and scaled).
- [ ] Launching the app opens no window and no taskbar button; the process keeps running (Task Manager shows `partimer.exe`).
- [ ] Start menu entry and Settings → Apps → Installed apps show 「打工人計時器」.
- [ ] Launching the app a second time shows the punch panel near the top centre of the main screen, on top and focused, with no taskbar button; no second process stays running.
- [ ] Closing the records / settings window leaves the app running; Alt+F4 on the panel only hides it.
- [ ] Tray icon: resting plant when not timing; awake plant swaying (4 frames, 0.5 s) while timing; resting plant while paused. Clear at 100%, 125%, 150% and 200% display scaling.
- [ ] Hover tooltip reads exactly 「計時中 1:23・專案名」 while timing, 「暫停中 ⏸ 0:47・專案名」 while paused, 「打工人計時器」 otherwise, and updates each minute.
- [ ] Left-click on the icon opens the panel just above the icon (taskbar at the bottom); a second left-click hides it. Also check with the icon in the overflow area (^) and with the taskbar on another edge.
- [ ] Right-click shows 打開面板／紀錄／設定／結束; each item works.
- [ ] Ctrl+Alt+P opens / hides the panel. If another app owns Ctrl+Alt+P, the settings window explains it in red.
- [ ] Hourly and 12-hour notifications appear in the Action Center as 打工人計時器 with the texts 「已計時 N 小時・專案名」 / 「是不是忘了停？已計時 12 小時・專案名」, once each (see the macOS items for the rules). With notifications turned off for the app, nothing breaks.

## Ubuntu

- [ ] Tray icon is clear in the Ubuntu top bar / indicator area (light and dark themes).
- [ ] Launching `partimer` opens no window; the process keeps running (`pgrep -x partimer`).
- [ ] The app list shows 「打工人計時器」; the terminal command is `partimer`.
- [ ] Running `partimer` again shows the punch panel near the top centre of the main screen, on top and focused (checked headless under Xvfb only: panel 360×480 at 20% from the top, centred).
- [ ] Closing the records / settings window leaves the app running.
- [ ] Tray (AppIndicator): resting plant when not timing; awake plant swaying while timing; resting plant while paused. (Checked headless only: the item is exported on D-Bus and sends a new icon every 0.5 s while running, none while paused.)
- [ ] Next to the icon the time shows as `1:23` / `⏸ 0:47` where the desktop supports indicator labels (GNOME's AppIndicator extension may not; note what you see). (Checked headless only: label `  11:59` → `  12:00`, `  ⏸ 0:47`.)
- [ ] Clicking the icon (left or right) always shows the menu 打開面板／紀錄／設定／結束; each item works. 打開面板 opens the panel in the top-right corner under the top bar (checked headless only: 360×480 at the top-right of the Xvfb screen; 結束 exited with code 0).
- [ ] X11 session: Ctrl+Alt+P opens / hides the panel, and the settings window shows no red text.
- [ ] Wayland session (Ubuntu default): the settings window shows the red text about Wayland and the custom-shortcut steps; a custom shortcut in 設定 → 鍵盤 → 檢視及自訂快捷鍵 → 自訂快捷鍵 running `partimer` opens the panel.
- [ ] Hourly and 12-hour notifications appear in GNOME's notification list as 打工人計時器 with the texts above, once each. (Checked headless only: one D-Bus Notify call each, title 打工人計時器, bodies 「是不是忘了停？已計時 12 小時・官網改版」 and 「已計時 12 小時・官網改版」.)
