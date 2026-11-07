# 實機驗證清單 / Manual verification checklist

What cannot be checked automatically. Install the build from the draft release on each platform and go through **your platform's list** plus **every item of the common lists** (each common item has one box per platform). Report by item number, e.g. 「W7 失敗：…」 or 「P12 Ubuntu：…」, and write what you saw for every item that says *note* or *record*. Items marked *(Checked headless only …)* were tried on a headless Linux (Xvfb) and still need a real desktop.

## Release assets (checked automatically on the rc build)

Checked on the test build `v0.1.0-rc.1` (2026-11-07; tag deleted afterwards) from the release workflow log, and the `.deb` control file from a local Linux build with the same config and Tauri CLI. Re-check when the bundle config changes.

- [x] Every asset file name is English: `partimer_0.1.0_darwin_universal.dmg`, `partimer_0.1.0_darwin_universal.app.tar.gz`, `partimer_0.1.0_windows_x64-setup.exe`, `partimer_0.1.0_linux_amd64.deb`, `partimer_0.1.0_linux_amd64.AppImage`.
- [x] `dpkg-deb -f partimer_0.1.0_linux_amd64.deb Package` prints `partimer`. (With the Chinese product name it printed `打工人計時器`, which dpkg rejects; Linux now uses the product name `partimer` and the desktop entry keeps `Name=打工人計時器`.)
- [x] Expected names, still to be confirmed on the machines (M2, W2, U2): the macOS bundle inside the `.dmg` is `打工人計時器.app`; the Windows installer creates the Start-menu shortcut `打工人計時器` (no folder) and the Installed-apps entry `打工人計時器`, installing to `%LOCALAPPDATA%\打工人計時器`; the Ubuntu app list shows `打工人計時器` from `partimer.desktop`.

## macOS

Test on macOS 15 or later; note the macOS version and whether the Mac is Apple Silicon or Intel (the `.dmg` is universal — if you can, try both).

### Install, names and uninstall

- [ ] **M1** Download `partimer_<version>_darwin_universal.dmg`, open it, drag 打工人計時器 to Applications. The first open is blocked; 系統設定 → 隱私權與安全性 → 強制打開 lets it run (note the exact wording macOS showed, and whether the README's `xattr` line was needed instead).
- [ ] **M2** The app shows as 「打工人計時器」 in Launchpad, the Applications folder (note whether Finder shows `partimer.app` instead), Activity Monitor and Login Items.
- [ ] **M3** Uninstall: quit the app, move 打工人計時器 from Applications to the Trash. It no longer starts at login (if 開機自動啟動 was on, note whether a stale Login Items entry stays). The data folder `~/Library/Application Support/partimer/` is still there.

### Start, menu bar and shortcut

- [ ] **M4** Tray icon is clear on both light and dark menu bars (rest and awake frames).
- [ ] **M5** Launching the app opens no window at all, and no Dock icon appears (LSUIElement / Accessory policy).
- [ ] **M6** Opening the app a second time (double-click in Applications) shows the punch panel near the top centre of the main screen, on top and focused; no second process stays running.
- [ ] **M7** Not timing: the menu bar shows the resting plant (drooping leaves, closed eyes, "z") with no text beside it.
- [ ] **M8** Timing: the plant is awake and its leaves sway (4 frames, one every 0.5 s); the time shows to the right as `1:23` with a visible gap between icon and digits; it updates when the minute changes.
- [ ] **M9** Paused: resting plant with `⏸ 0:47` beside it; no animation.
- [ ] **M10** Left-click on the icon opens the panel just below the icon; a second left-click hides it. Left-click shows no menu.
- [ ] **M11** Right-click shows the menu 打開面板／紀錄／設定／結束; each item works (結束 quits; the unsaved title/content typed in the panel is in the entry after restart).
- [ ] **M12** Ctrl+Option+P opens the panel near the top centre of the main screen, focused; pressing it again hides it. The settings window shows no red text about the shortcut.
- [ ] **M13** Notifications: on first reminder macOS asks for permission (or the app appears in System Settings → Notifications as 打工人計時器); if denied, nothing breaks.
- [ ] **M14** Logging out / shutting down while timing keeps the unsaved title/content (OS quit path without the menu).

### Floating clock (macOS)

- [ ] **M15** After turning it on (設定 → 浮動小時鐘 → 顯示浮動小時鐘; off by default on macOS) the capsule is transparent around its rounded corners (no white box), floats above other apps, shows on every Space and over full-screen apps (note if not), and no Dock icon appears. While another app is active, a single click on the clock opens the panel (the first click is not swallowed).

### Chinese input (macOS)

- [ ] **M16** Chinese IME while timing — 注音 (Zhuyin/Bopomofo), 倉頡 (Cangjie) and 拼音 (Pinyin – Simplified or Traditional): type a sentence in 標題 and several lines in 內容 for at least a minute (past a minute change of the clock). The candidate window never closes by itself, no characters are lost or duplicated, the caret does not jump. Press ⌘S while a candidate window is open: nothing happens until you commit. 中文輸入模式下按 ⌘S (IME switched on, nothing being composed): it saves. Commit, then click STOP with the candidate window still open: the entry has the committed text.

## Windows

Test on Windows 10 or 11 (note which).

### Install, names and uninstall

- [ ] **W1** Run `partimer_<version>_windows_x64-setup.exe`. SmartScreen's 「Windows 已保護您的電腦」 → 其他資訊 → 仍要執行 lets it run. The installer's title and pages show 打工人計時器; note whether it asked for administrator rights and where it installed (default folder name).
- [ ] **W2** Start menu entry and Settings → Apps → Installed apps show 「打工人計時器」. Record the exact Start-menu name (and folder, if the shortcut is in one) and the name in Installed apps.
- [ ] **W3** Uninstall from Settings → Apps → Installed apps → 打工人計時器 → 解除安裝: the Start-menu entry and the install folder are gone, the app no longer starts at login (if 開機自動啟動 was on), and `%APPDATA%\partimer\` is still there.

### Start, tray and shortcut

- [ ] **W4** Tray icon is clear in the Windows system tray (light and dark taskbar, 16px and scaled).
- [ ] **W5** Launching the app opens no window and no taskbar button; the process keeps running (Task Manager shows `partimer.exe`).
- [ ] **W6** Launching the app a second time shows the punch panel near the top centre of the main screen, on top and focused, with no taskbar button; no second process stays running.
- [ ] **W7** Alt+F4 on the panel only hides it.
- [ ] **W8** Tray icon: resting plant when not timing; awake plant swaying (4 frames, 0.5 s) while timing; resting plant while paused. Clear at 100%, 125%, 150% and 200% display scaling.
- [ ] **W9** Hover tooltip reads exactly 「計時中 1:23・專案名」 while timing, 「暫停中 ⏸ 0:47・專案名」 while paused, 「打工人計時器」 otherwise, and updates each minute.
- [ ] **W10** Left-click on the icon opens the panel just above the icon (taskbar at the bottom); a second left-click hides it. Also check with the icon in the overflow area (^) and with the taskbar on another edge.
- [ ] **W11** Right-click shows 打開面板／紀錄／設定／結束; each item works.
- [ ] **W12** Ctrl+Alt+P opens / hides the panel. If another app owns Ctrl+Alt+P, the settings window explains it in red.
- [ ] **W13** Hourly and 12-hour notifications appear in the Action Center as 打工人計時器 with the texts 「已計時 N 小時・專案名」 / 「是不是忘了停？已計時 12 小時・專案名」, once each (rules: N1–N2 below). With notifications turned off for the app, nothing breaks.
- [ ] **W19** Signing out of Windows, and separately shutting down / restarting, while timing with unsaved text typed in the panel (not saved with Ctrl+S): after signing in again the unsaved title/content is in the running entry (README says it is saved on Windows). Note what you saw for both.

### Floating clock (Windows)

- [ ] **W14** The clock is a translucent rounded capsule: the desktop / windows behind show through, with no white or black box around the capsule and no window shadow or border. It has no taskbar button and is not in Alt+Tab.
- [ ] **W15** It stays on top of other windows, also after clicking into another app. Note whether it is visible over a full-screen video (browser full screen on YouTube, the Movies & TV app) and over a PowerPoint slide show (also with the presenter view on a second screen). Exclusive full-screen games covering it is a known limit.
- [ ] **W16** Starting a timer from the global shortcut while typing in another app (e.g. Notepad) makes the clock appear without taking the caret away; typing continues in that app.
- [ ] **W17** At 125 %, 150 % and 200 % display scaling the capsule is 130×40 (scaled), crisp, text not cut off. With two screens at different scaling, drag it onto the second screen, STOP and START again: it reappears at the same spot.

### Chinese input (Windows)

- [ ] **W18** Chinese IME while timing — Microsoft Bopomofo (注音), Microsoft Changjie (倉頡) and Microsoft Pinyin (拼音): type a sentence in 標題 and several lines in 內容 for at least a minute (past a minute change of the clock). The candidate window never closes by itself, no characters are lost or duplicated, the caret does not jump. Press Ctrl+S while composing: nothing happens until you commit. 中文輸入模式下按 Ctrl+S (IME switched on, nothing being composed): it saves. Click STOP with an uncommitted composition: the entry has the committed text.

### Portable exe (Windows)

- [ ] **W20** Run `partimer_<version>_windows_x64_portable.exe` from Downloads without installing (quit the installed app first — only one can run). SmartScreen may ask once. 設定 → 資料 shows `%APPDATA%\partimer`, and the installed app's clients, projects and records are there.
- [ ] **W21** Turn on 開機自動啟動 in the portable exe, sign out and in: it starts from where the exe is.

## Ubuntu

Test on Ubuntu 24.04 or later in the default (Wayland) session, and the X11 items in an 「Ubuntu on Xorg」 session.

### Install, names and uninstall

- [ ] **U1** `sudo apt install ./partimer_<version>_linux_amd64.deb` installs without errors or warnings about the package name; `dpkg -s partimer` shows it.
- [ ] **U2** The app list shows 「打工人計時器」; the terminal command is `partimer`.
- [ ] **U3** The AppImage: `chmod +x partimer_<version>_linux_amd64.AppImage` and run it; the tray icon appears and the app works like the `.deb` one (do this with the `.deb` version removed or not running). Note the name shown in the dock / app switcher.
- [ ] **U4** The 「Ubuntu AppIndicators」 extension is on (Extensions app, or `gnome-extensions list --enabled | grep -i appindicator`); with it off, note what happens (no icon is expected; the app keeps running).
- [ ] **U5** With 開機自動啟動 on, record the name shown in 「初始啟動程式」 / `~/.config/autostart/` (on Linux the internal name `partimer` is expected there).
- [ ] **U6** `sudo apt remove partimer` removes the app-list entry and the `partimer` command; `~/.local/share/partimer/` is still there.
- [ ] **U18** `grep Categories /usr/share/applications/partimer.desktop` shows a non-empty value (e.g. `Categories=Office;` from the bundle category Productivity), and the app appears under a sensible category in the app list.

### Start, tray and shortcut

- [ ] **U7** Tray icon is clear in the Ubuntu top bar / indicator area (light and dark themes).
- [ ] **U8** Launching `partimer` opens no window; the process keeps running (`pgrep -x partimer`).
- [ ] **U9** Running `partimer` again shows the punch panel near the top centre of the main screen, on top and focused (checked headless under Xvfb only: panel 360 wide at 20% from the top, centred; its height follows the content, e.g. 360×595 with one entry today).
- [ ] **U10** Tray (AppIndicator): resting plant when not timing; awake plant swaying while timing; resting plant while paused. (Checked headless only: the item is exported on D-Bus and sends a new icon every 0.5 s while running, none while paused.)
- [ ] **U11** Next to the icon the time shows as `1:23` / `⏸ 0:47` where the desktop supports indicator labels (GNOME's AppIndicator extension may not; note what you see). (Checked headless only: label `  11:59` → `  12:00`, `  ⏸ 0:47`.)
- [ ] **U12** Clicking the icon (left or right) always shows the menu 打開面板／紀錄／設定／結束; each item works. 打開面板 opens the panel in the top-right corner under the top bar (checked headless only: 360×480 at the top-right of the Xvfb screen; 結束 exited with code 0).
- [ ] **U13** X11 session: Ctrl+Alt+P opens / hides the panel, and the settings window shows no red text.
- [ ] **U14** Wayland session (Ubuntu default): the settings window shows the red text about Wayland and the custom-shortcut steps; a custom shortcut in 設定 → 鍵盤 → 檢視及自訂快捷鍵 → 自訂快捷鍵 running `partimer` opens the panel.
- [ ] **U15** Hourly and 12-hour notifications appear in GNOME's notification list as 打工人計時器 with the texts 「已計時 N 小時・專案名」 / 「是不是忘了停？已計時 12 小時・專案名」, once each (rules: N1–N2 below). (Checked headless only: one D-Bus Notify call each, title 打工人計時器, bodies 「是不是忘了停？已計時 12 小時・官網改版」 and 「已計時 12 小時・官網改版」.)
- [ ] **U19** Logging out, and separately shutting down, while timing with unsaved text typed in the panel: note whether the unsaved title/content survives (README warns it may be lost on Ubuntu, because the session's SIGTERM is not handled). Then quit from the tray menu (結束) with unsaved text and log out: the text is in the entry after logging in again.
- [ ] **U20** Default (Wayland) session: 設定 →「打開資料夾」 opens `~/.local/share/partimer/` in Files as a normal Wayland window (Files looks and scales like when opened from the dock; `xprop` clicking it reports that the window is not an X window). Start partimer from a terminal with `GDK_BACKEND=x11 partimer` and repeat: the user's choice is passed on (Files may then run through XWayland).

### Floating clock (Ubuntu)

- [ ] **U16** Default session (the app runs through XWayland): the clock stays on top of other windows, appears on every workspace, its position is remembered after STOP/START and after restarting the app. Note whether the corners are transparent (GNOME's compositor) or black. It cannot be resized with window-manager shortcuts (Alt+F8 / Super+middle-drag). (Checked headless only: 130×40 at the top-right, 24 px from the right and 64 px from the top of a 1280×1024 screen.)

### Chinese input (Ubuntu)

- [ ] **U17** Chinese IME while timing — IBus (or Fcitx 5) with 注音 (Chewing), 倉頡 (Cangjie table) and 拼音 (libpinyin): type a sentence in 標題 and several lines in 內容 for at least a minute (past a minute change of the clock). The candidate window never closes by itself, no characters are lost or duplicated, the caret does not jump. Press Ctrl+S while composing: nothing happens until you commit. 中文輸入模式下按 Ctrl+S (IME switched on, nothing being composed): it saves. Click STOP with an uncommitted composition: the entry has the committed text. Note whether the app runs under XWayland (default) and whether the IME works there.

## Common: app and names (each item on macOS, Windows and Ubuntu)

- **A1** Closing the records / settings window leaves the app running.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **A2** Window titles read 打工人計時器 (panel), 紀錄 — 打工人計時器 and 設定 — 打工人計時器 wherever the system shows them (title bar, Alt+Tab / Mission Control / the dock or app switcher). Note any place that shows `partimer` instead.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu

- **A3** Data file from a newer version: quit, edit `data.json` and set `"version": 2`, start the app. The panel shows the red notice 「資料檔是由較新版本的打工人計時器建立的，為避免覆蓋，這次開啟期間的變更不會存檔。請改用較新的版本。」 without a close button; start/stop a timer, quit: `data.json` is unchanged (same bytes, still `"version": 2`) and no new file appeared in `backups/` today. Set the version back to 1 afterwards.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **A4** Broken item in the data file: quit, in `data.json` change one entry's `"start"` to `"x"`, start the app. The panel shows 「有 1 筆資料格式不正確，已另外保留在資料檔中，不會顯示在清單與合計裡。」 with a close button; the entry is not in the records window; after any change `data.json` has it under `quarantinedEntries`. `activity.log` has one `RESTORE` line with `note=quarantined=1`.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu

## Common: reminders (each item on macOS, Windows and Ubuntu)

- **N1** Hourly reminder on (設定 → 整點提醒): each full hour of running time shows 「已計時 N 小時・專案名」 once, the first at the next full hour of running time; nothing while paused; reopening the app after hours of running does not send a backlog.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **N2** 12-hour reminder: after 12 h of effective time (e.g. lid closed overnight) one notification 「是不是忘了停？已計時 12 小時・專案名」, not repeated every second; after 「沒忘，繼續計時」 the next one comes at 24 h.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu

## Common: punch panel (each item on macOS, Windows and Ubuntu)

Set-up: create at least two clients with two projects each (one with a few entries today).

- **P1** First launch with no data: the panel shows only the add form (客戶名、專案名、建立) where the project menu would be. 建立 with empty names shows the error in red under the form. Creating 「客戶甲／官網改版」 replaces the form with the menu, the colour dot left of it, and START becomes enabled.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P2** Project menu: projects are grouped under their client names; archived projects are not listed; the last item is 「＋ 新增專案…」. The colour dot sits left of the menu (never inside the options) and changes colour with the selected project. Typing an existing client name (with extra spaces) in the add form puts the new project under that client instead of a second client with the same name.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P3** Not timing, switching the project: the title changes to that project's latest title; text already typed in 內容 stays.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P4** START with a title and two content lines: the clock starts at 0:00:00 and counts every second; the save row (儲存 + 已儲存 + Ctrl+S / ⌘S) and PAUSE appear left of STOP; the new entry is at the top of 今日紀錄 with 「計時中」 and the same text (lines starting with 「- 」 shown as bullets).
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P5** While timing, type in 標題 and 內容: the label changes to 「尚未儲存」 as soon as the text differs from the saved entry and back to 「已儲存」 if you undo the change. Nothing is written to `data.json` while typing (check its modification time).
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P6** Ctrl+S (Windows/Ubuntu) or ⌘S (macOS) while typing: the label becomes 「已儲存」, the caret stays where it was and you can keep typing without clicking. The 儲存 button does the same. When not timing, Ctrl+S / ⌘S does nothing (no browser "save page" dialog). Known limit: keys typed during the split second the save runs may be dropped (the fields stop syncing while a command runs); note it if you ever notice lost characters.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P7** STOP: the entry ends and keeps exactly the text in the fields (open it in 紀錄 to compare). The title goes back to that same title, 內容 is empty. Then START again without typing: the new entry has the title and **empty** content — no text of the previous entry.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P8** Double-click STOP as fast as you can (also: press STOP and the global shortcut together): only one STOP line in `activity.log`, no extra or empty entry, the next draft is intact.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P9** PAUSE: the clock stops and greys out with ⏸ in front (check the ⏸ glyph renders, not a box or bar); the entry shows 「暫停中」; RESUME continues from the same value. STOP while paused ends the entry at the moment PAUSE was pressed.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P10** Switching the project while timing: the question 「結束目前這筆，改記另一個專案？」 appears **in front of** the panel. 取消 → the menu shows the old project again and nothing changes. 結束並切換 → the old entry ends with the text from the fields, the new project starts immediately with its latest title and empty content. Same while paused (old entry ends at the pause moment). Choosing 「＋ 新增專案…」 and creating a project while timing asks the same question.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P11** Height: the panel grows when the save row appears, when a notice appears and with each entry in 今日紀錄, and shrinks after STOP / dismissing a notice. Buttons and the footer (紀錄、設定、結束 app) are never cut off. With many entries today the panel stops 40 px short of the screen's usable height and only 今日紀錄 scrolls (今日總計 and the footer stay visible). The panel never opens as 0×0 or as a sliver.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P12** Opened from the tray icon at the bottom of the screen (Windows taskbar; elsewhere a bottom panel/dock if you have one): when the panel grows it moves up instead of disappearing behind the taskbar.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P13** Hide and show again (Esc, the global shortcut, the tray icon): the half-typed title and content are still there, and an IME candidate window does not hide the panel. Clicking another app does not hide the panel (no auto-hide by design).
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P14** Esc while an IME is composing (candidate window open) only cancels the composition; the panel stays. A second Esc hides it.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P15** 今日紀錄: entries started today, newest first, plus the running one even if it started yesterday (start a timer before midnight and check after midnight). Each shows start–end (or 計時中／暫停中), title (or （無標題）), 客戶・專案, duration; content lines starting with 「- 」 are bullets. 今日總計 includes the running entry and counts up every second. Clicking an entry opens the records window on that entry.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P16** Leave the panel open (not timing) across midnight, or hide it before midnight and open it after: 今日紀錄 shows the new day's entries and 今日總計 restarts.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P17** 12-hour prompt (e.g. set the clock or run a timer overnight): the panel shows 「已經計時超過 12 小時，是不是忘了停？」 with a date-time field preset to start + 12 h; 用這個時間結束 ends the entry at that time (check in 紀錄) with the text from the fields; a time in the future shows 「結束時間不能晚於現在」. 沒忘，繼續計時 hides the prompt until 24 h.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P18** Notices: orange notices (e.g. restored from backup, 「這筆已在紀錄視窗被修改…」) have a × that removes them. Save errors / unreadable or corrupt data file are red and have no ×. (Unreadable checked headless only: red, no ×.)
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P19** Footer: 紀錄 and 設定 open those windows; 結束 app quits, and text typed but not saved while timing is in the entry after restart.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **P20** Dark mode (system setting): the panel switches to the dark palette and all text stays readable.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu

## Common: records window (each item on macOS, Windows and Ubuntu)

Set-up: a few entries this month and last month across two clients, one entry that crosses midnight, one with a pause, one running.

- **R1** Opening 紀錄 (tray menu, panel footer) shows this month, 全部客戶, 全部專案. ‹ › and 本月 change the month. Picking a client empties the project menu to that client's projects and resets it to 全部專案. Deleting the client/project you filter on in 設定 while 紀錄 is open resets the filter instead of showing an empty list. (Checked headless only: default view and layout on Xvfb.)
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R2** 左欄 shows the month total and every project subtotal both as 「1h 10m」 and 「1.17 小時」; the running entry is not included until it ends (same numbers as the CSV).
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R3** 右欄: newest day first, newest entry first within a day. An entry from 23:30 to 00:30 sits under the day it started. Paused-before entries show small 「另暫停 20m」; the running entry shows 「計時中」 (「暫停中」 when paused) and its duration counts up every second. Content is complete (no cut-off) and lines starting with 「- 」 are bullets. (Checked headless only: all of this, with seeded data.)
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R4** Click an entry: the editor opens with that entry's values. Changing 專案, 開始, 結束, 暫停分鐘數, 標題, 內容 and 儲存 updates the list and the totals. Esc or 取消 closes without saving.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R5** Editor validation: an end before the start, a negative pause, a pause longer than the entry, and a start/end that does not exist (spring-forward gap) each show the Chinese message inside the dialog in red; the dialog stays open with everything you typed.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R6** A running entry shows 「仍在計時」 in the editor (other entries do not). With it on, 結束 is greyed out; turning it off fills in the current time.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R7** 刪除 asks 「確定刪除這筆紀錄？」 in front of the window; 取消 keeps the entry, 確定 removes it.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R8** 新增紀錄 opens an empty editor (start one hour ago, end now, the selected project); a saved entry appears under the right day.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R9** While the editor is open, start/stop a timer from the panel: the list behind refreshes, the text you were typing in the editor is untouched.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R10** Click an entry in the panel's 今日紀錄: the records window opens (also when it was not open yet) on that entry's month with the editor showing it. Do it again with the records window already open on another month.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R11** 匯出 CSV: a save dialog proposes partimer-YYYY-MM.csv; the file contains the filtered month. `head -c 3 <file> | xxd` shows `efbb bf`. Opening it in Excel (Windows and macOS) shows the Chinese correctly. Cancelling the dialog leaves no message.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R12** Dark mode: the window and the editor stay readable.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R13** CSV and Excel formulas: export a month containing an entry whose 內容 has 「- 」 bullet lines (e.g. 「- 改 banner」 / 「- 修 RWD」) and whose 標題 starts with 「=」 (e.g. 「=1+1」), plus a project named 「+專案」. Opening the CSV in Excel (Windows and macOS; on Ubuntu LibreOffice Calc) shows all of them as text exactly as typed — no `#NAME?`, no computed `2`, no visible extra character in front.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R14** In the editor, type in 標題 with a Chinese IME and press Enter to confirm the composition: the text is committed and the dialog stays open (not saved, not closed). A second Enter with nothing being composed saves.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R15** In the editor, type 1.5 (or -1) in 暫停分鐘數 and 儲存: the dialog shows 「暫停分鐘請填整數」 in red and stays open; nothing is saved. An empty field saves as 0.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R16** While timing, type unsaved text in the panel's 內容 (do not press Ctrl+S), then in the records window turn 仍在計時 off for that entry and 儲存. The panel goes idle with the title kept and 內容 empty, and shows a notice with a close button quoting the unsaved text (「…原文：標題「…」，內容：…（請到紀錄視窗補寫）」). Press START: the new entry does not contain the old text.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R17** While timing, type unsaved text in the panel, then edit the same entry's 內容 in the records window and 儲存: the panel shows 「這筆已在紀錄視窗被修改，面板上的文字尚未儲存（按儲存才會寫入，結束 app 時不會自動存入）」. Quit from the tray menu without pressing 儲存 and start again: the entry has the records-window text, not the panel text.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R18** Same as R17, but close the notice with its × first, then quit from the tray menu without pressing 儲存: after starting again the entry still has the records-window text. Repeat, but press 儲存 (or Ctrl+S) in the panel before quitting: the entry has the panel text.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **R19** 新增紀錄 with 結束 one hour later than now: 儲存 shows 「結束時間不能晚於現在」 and nothing is added; with 開始 in the future: 「開始時間不能晚於現在」. Editing an existing entry's 結束 to a future time shows the same message. The default form (last hour, ending now) saves fine.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu

## Common: settings window (each item on macOS, Windows and Ubuntu)

Set-up: two clients with two projects each, one archived project, one project with a few entries (one of them running).

- **S1** 開機自動啟動: turning it on makes the app start at the next login (macOS 登入項目, Windows startup, Ubuntu autostart entry); turning it off removes it. When the system refuses, a red message with the reason and the platform's manual steps appears (macOS 「系統設定 → 一般 → 登入項目…」, Windows 「Win+R shell:startup…」, Ubuntu 「初始啟動程式…」). The toggle shows the real state after reopening the window.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **S2** 浮動小時鐘: the switch shows/hides the clock at once; the opacity slider (40%–100%) changes its opacity after release and the % label follows while dragging; 「沒在計時時也顯示」 shows the resting plant while idle. All three survive restarting the app.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **S3** 全域快捷鍵: shows Ctrl+Alt+P (⌃⌥P on macOS). On Ubuntu Wayland a red text with the 自訂快捷鍵 workaround shows; when another program already owns the shortcut, red 「快捷鍵被其他程式占用」 shows; otherwise no red text.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **S4** Rename: typing in a name box shows 儲存 and 還原; typing the original name back (or only spaces around it) hides them. 儲存 or Enter saves; Enter while an IME candidate window is open only confirms the composition. 還原 restores the old name.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **S5** Rename rejections show the Chinese reason right under that row and keep what you typed: empty name, a client named like another client, a project named like another project of the same client (same name under a different client is fine).
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **S6** While you type a new name in one row, start/stop a timer from the panel (or rename something in another row): the half-typed text stays, other rows update.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **S7** 色塊: the 8 swatches match the dots in the panel menu and 紀錄; the current colour has an outline; clicking another colour changes it at once (also in the panel menu dot and in 紀錄). Tab reaches each swatch.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **S8** 封存: an archived project (or a whole archived client) disappears from the panel's project menu but its entries still show in 紀錄; 取消封存 brings it back. Archiving the running project does not stop the timer.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **S9** 刪除: a project/client with entries asks 「會刪掉 N 筆紀錄、共 Xh Ym，確定刪除？」 in front of the window and the numbers match 紀錄 (check one with exactly 1h 0m and one under an hour: 「0h」 never shows, e.g. 「45m」); one without entries asks 「確定刪除？」. Deleting a client removes its projects and their entries. If the running entry is included, the dialog adds 「其中包含正在計時的紀錄，計時會一併停止。」 and after 確定 the timer is gone. 取消 changes nothing.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **S10** 打開資料夾 opens the data folder in Finder / Explorer / the file manager; the path shown above it is the same folder (macOS `~/Library/Application Support/partimer`, Windows `%APPDATA%\partimer`, Ubuntu `~/.local/share/partimer`).
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **S11** Dark mode: the window stays readable, the swatch outline is visible on both themes.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu

## Common: floating clock (each item on macOS, Windows and Ubuntu)

Set-up: a project to time. On macOS the clock is off by default: turn on 設定 → 浮動小時鐘 → 顯示浮動小時鐘 first. Platform-specific clock items are in the platform lists above. Headless runs (Xvfb, no window manager, no compositor) only checked placement, size, contents, click/drag/stop wiring and runtime switching; **transparency itself could not be seen there** (the corners outside the capsule render black without a compositor).

- **C1** A click on the clock (without moving the mouse) opens the punch panel near the top centre of the main screen; another click hides it. (Checked headless only.)
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **C2** Dragging the clock moves it and does not open or hide the panel; the very next click after a drag toggles the panel at the first try. After a drag, STOP and START (or restart the app): it comes back where it was dropped. (Checked headless only: dragged, the new physical x/y was in data.json, the next click hid the panel.) Known limit: a drag that ends less than 0.5 s before the clock disappears (e.g. STOP via the shortcut right after dropping it) may not be remembered.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **C3** Position reset: with the clock on a second screen, quit, unplug that screen, start again while timing (or edit `widget.x/y` in data.json to e.g. 5000/5000): it appears at the top-right of the main screen, 24 px from the right and 64 px from the top. A position partly off the screen edge also resets. (Checked headless only.)
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **C4** Running: awake plant with leaves swaying, time as `1:23` that changes with the minute (same minute as the panel's clock). Paused: resting plant with `⏸ 0:47`, no sway; check the ⏸ glyph renders (Xvfb's fonts showed a thin bar). (Checked headless only.)
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **C5** Not timing: the clock disappears after STOP. With 沒在計時時也顯示 on it stays and shows the resting plant centred, without a time. Turning 顯示浮動小時鐘 off removes it at once and on brings it back. (Checked headless only.)
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **C6** Opacity: 60 % by default; moving the mouse over the clock makes it fully opaque, leaving makes it translucent again; the 透明度 slider changes it after release. (Checked headless only: 60 % vs 100 % and hover.)
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **C7** With the system's reduce-motion setting on (macOS 減少動態效果, Windows 動畫效果 off, GNOME 減少動畫), the leaves do not sway.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **C8** Quit (結束 app) while timing and start the app again: the clock appears by itself (no other window opens). Quit while not timing: nothing appears. (Checked headless only, with seeded data: running → clock only; idle → no window.)
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **C9** Right-click on the clock shows no browser context menu.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
- **C10** Fractional display scaling (Windows 設定 → 顯示器 → 縮放 125 % and then 150 %; macOS a scaled external display if available; Ubuntu fractional scaling if enabled): drag the clock somewhere, quit from the tray menu and start again, three times. The clock comes back on exactly the same spot each time (note any drift, even 1 px, and the scale). Then change 透明度 in the settings window: the clock follows at once.
  - [ ] macOS
  - [ ] Windows
  - [ ] Ubuntu
