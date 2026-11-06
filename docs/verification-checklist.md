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
- [ ] Chinese IME while timing — 注音 (Zhuyin/Bopomofo), 倉頡 (Cangjie) and 拼音 (Pinyin – Simplified or Traditional): type a sentence in 標題 and several lines in 內容 for at least a minute (past a minute change of the clock). The candidate window never closes by itself, no characters are lost or duplicated, the caret does not jump. Press ⌘S while a candidate window is open: nothing happens until you commit. 中文輸入模式下按 ⌘S (IME switched on, nothing being composed): it saves. Commit, then click STOP with the candidate window still open: the entry has the committed text.

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
- [ ] Chinese IME while timing — Microsoft Bopomofo (注音), Microsoft Changjie (倉頡) and Microsoft Pinyin (拼音): type a sentence in 標題 and several lines in 內容 for at least a minute (past a minute change of the clock). The candidate window never closes by itself, no characters are lost or duplicated, the caret does not jump. Press Ctrl+S while composing: nothing happens until you commit. 中文輸入模式下按 Ctrl+S (IME switched on, nothing being composed): it saves. Click STOP with an uncommitted composition: the entry has the committed text.

## Ubuntu

- [ ] Tray icon is clear in the Ubuntu top bar / indicator area (light and dark themes).
- [ ] Launching `partimer` opens no window; the process keeps running (`pgrep -x partimer`).
- [ ] The app list shows 「打工人計時器」; the terminal command is `partimer`.
- [ ] Running `partimer` again shows the punch panel near the top centre of the main screen, on top and focused (checked headless under Xvfb only: panel 360 wide at 20% from the top, centred; its height follows the content, e.g. 360×595 with one entry today).
- [ ] Closing the records / settings window leaves the app running.
- [ ] Tray (AppIndicator): resting plant when not timing; awake plant swaying while timing; resting plant while paused. (Checked headless only: the item is exported on D-Bus and sends a new icon every 0.5 s while running, none while paused.)
- [ ] Next to the icon the time shows as `1:23` / `⏸ 0:47` where the desktop supports indicator labels (GNOME's AppIndicator extension may not; note what you see). (Checked headless only: label `  11:59` → `  12:00`, `  ⏸ 0:47`.)
- [ ] Clicking the icon (left or right) always shows the menu 打開面板／紀錄／設定／結束; each item works. 打開面板 opens the panel in the top-right corner under the top bar (checked headless only: 360×480 at the top-right of the Xvfb screen; 結束 exited with code 0).
- [ ] X11 session: Ctrl+Alt+P opens / hides the panel, and the settings window shows no red text.
- [ ] Wayland session (Ubuntu default): the settings window shows the red text about Wayland and the custom-shortcut steps; a custom shortcut in 設定 → 鍵盤 → 檢視及自訂快捷鍵 → 自訂快捷鍵 running `partimer` opens the panel.
- [ ] Hourly and 12-hour notifications appear in GNOME's notification list as 打工人計時器 with the texts above, once each. (Checked headless only: one D-Bus Notify call each, title 打工人計時器, bodies 「是不是忘了停？已計時 12 小時・官網改版」 and 「已計時 12 小時・官網改版」.)
- [ ] Chinese IME while timing — IBus (or Fcitx 5) with 注音 (Chewing), 倉頡 (Cangjie table) and 拼音 (libpinyin): type a sentence in 標題 and several lines in 內容 for at least a minute (past a minute change of the clock). The candidate window never closes by itself, no characters are lost or duplicated, the caret does not jump. Press Ctrl+S while composing: nothing happens until you commit. 中文輸入模式下按 Ctrl+S (IME switched on, nothing being composed): it saves. Click STOP with an uncommitted composition: the entry has the committed text. Note whether the app runs under XWayland (default) and whether the IME works there.

## Punch panel (do every item on macOS, Windows and Ubuntu)

Set-up: create at least two clients with two projects each (one with a few entries today). Write the platform next to any item that fails.

- [ ] First launch with no data: the panel shows only the add form (客戶名、專案名、建立) where the project menu would be. 建立 with empty names shows the error in red under the form. Creating 「客戶甲／官網改版」 replaces the form with the menu, the colour dot left of it, and START becomes enabled.
- [ ] Project menu: projects are grouped under their client names; archived projects are not listed; the last item is 「＋ 新增專案…」. The colour dot sits left of the menu (never inside the options) and changes colour with the selected project. Typing an existing client name (with extra spaces) in the add form puts the new project under that client instead of a second client with the same name.
- [ ] Not timing, switching the project: the title changes to that project's latest title; text already typed in 內容 stays.
- [ ] START with a title and two content lines: the clock starts at 0:00:00 and counts every second; the save row (儲存 + 已儲存 + Ctrl+S / ⌘S) and PAUSE appear left of STOP; the new entry is at the top of 今日紀錄 with 「計時中」 and the same text (lines starting with 「- 」 shown as bullets).
- [ ] While timing, type in 標題 and 內容: the label changes to 「尚未儲存」 as soon as the text differs from the saved entry and back to 「已儲存」 if you undo the change. Nothing is written to `data.json` while typing (check its modification time).
- [ ] Ctrl+S (Windows/Ubuntu) or ⌘S (macOS) while typing: the label becomes 「已儲存」, the caret stays where it was and you can keep typing without clicking. The 儲存 button does the same. When not timing, Ctrl+S / ⌘S does nothing (no browser "save page" dialog). Known limit: keys typed during the split second the save runs may be dropped (the fields stop syncing while a command runs); note it if you ever notice lost characters.
- [ ] STOP: the entry ends and keeps exactly the text in the fields (open it in 紀錄 to compare). The title goes back to that same title, 內容 is empty. Then START again without typing: the new entry has the title and **empty** content — no text of the previous entry.
- [ ] Double-click STOP as fast as you can (also: press STOP and the global shortcut together): only one STOP line in `activity.log`, no extra or empty entry, the next draft is intact.
- [ ] PAUSE: the clock stops and greys out with ⏸ in front (check the ⏸ glyph renders, not a box or bar); the entry shows 「暫停中」; RESUME continues from the same value. STOP while paused ends the entry at the moment PAUSE was pressed.
- [ ] Switching the project while timing: the question 「結束目前這筆，改記另一個專案？」 appears **in front of** the panel. 取消 → the menu shows the old project again and nothing changes. 結束並切換 → the old entry ends with the text from the fields, the new project starts immediately with its latest title and empty content. Same while paused (old entry ends at the pause moment). Choosing 「＋ 新增專案…」 and creating a project while timing asks the same question.
- [ ] Height: the panel grows when the save row appears, when a notice appears and with each entry in 今日紀錄, and shrinks after STOP / dismissing a notice. Buttons and the footer (紀錄、設定、結束 app) are never cut off. With many entries today the panel stops 40 px short of the screen's usable height and only 今日紀錄 scrolls (今日總計 and the footer stay visible). The panel never opens as 0×0 or as a sliver.
- [ ] Opened from the tray icon at the bottom of the screen (Windows taskbar): when the panel grows it moves up instead of disappearing behind the taskbar.
- [ ] Hide and show again (Esc, the global shortcut, the tray icon): the half-typed title and content are still there, and an IME candidate window does not hide the panel. Clicking another app does not hide the panel (no auto-hide by design).
- [ ] Esc while an IME is composing (candidate window open) only cancels the composition; the panel stays. A second Esc hides it.
- [ ] 今日紀錄: entries started today, newest first, plus the running one even if it started yesterday (start a timer before midnight and check after midnight). Each shows start–end (or 計時中／暫停中), title (or （無標題）), 客戶・專案, duration; content lines starting with 「- 」 are bullets. 今日總計 includes the running entry and counts up every second. Clicking an entry opens the records window on that entry.
- [ ] Leave the panel open (not timing) across midnight, or hide it before midnight and open it after: 今日紀錄 shows the new day's entries and 今日總計 restarts.
- [ ] 12-hour prompt (e.g. set the clock or run a timer overnight): the panel shows 「已經計時超過 12 小時，是不是忘了停？」 with a date-time field preset to start + 12 h; 用這個時間結束 ends the entry at that time (check in 紀錄) with the text from the fields; a time in the future shows 「結束時間不能晚於現在」. 沒忘，繼續計時 hides the prompt until 24 h.
- [ ] Notices: orange notices (e.g. restored from backup, 「這筆已在紀錄視窗被修改…」) have a × that removes them. Save errors / unreadable or corrupt data file are red and have no ×. (Unreadable checked headless only: red, no ×.)
- [ ] Footer: 紀錄 and 設定 open those windows; 結束 app quits, and text typed but not saved while timing is in the entry after restart.
- [ ] Dark mode (system setting): the panel switches to the dark palette and all text stays readable.

## Records window (do every item on macOS, Windows and Ubuntu)

Set-up: a few entries this month and last month across two clients, one entry that crosses midnight, one with a pause, one running.

- [ ] Opening 紀錄 (tray menu, panel footer) shows this month, 全部客戶, 全部專案. ‹ › and 本月 change the month. Picking a client empties the project menu to that client's projects and resets it to 全部專案. Deleting the client/project you filter on in 設定 while 紀錄 is open resets the filter instead of showing an empty list. (Checked headless only: default view and layout on Xvfb.)
- [ ] 左欄 shows the month total and every project subtotal both as 「1h 10m」 and 「1.17 小時」; the running entry is not included until it ends (same numbers as the CSV).
- [ ] 右欄: newest day first, newest entry first within a day. An entry from 23:30 to 00:30 sits under the day it started. Paused-before entries show small 「另暫停 20m」; the running entry shows 「計時中」 (「暫停中」 when paused) and its duration counts up every second. Content is complete (no cut-off) and lines starting with 「- 」 are bullets. (Checked headless only: all of this, with seeded data.)
- [ ] Click an entry: the editor opens with that entry's values. Changing 專案, 開始, 結束, 暫停分鐘數, 標題, 內容 and 儲存 updates the list and the totals. Esc or 取消 closes without saving.
- [ ] Editor validation: an end before the start, a negative pause, a pause longer than the entry, and a start/end that does not exist (spring-forward gap) each show the Chinese message inside the dialog in red; the dialog stays open with everything you typed.
- [ ] A running entry shows 「仍在計時」 in the editor (other entries do not). With it on, 結束 is greyed out; turning it off fills in the current time.
- [ ] 刪除 asks 「確定刪除這筆紀錄？」 in front of the window; 取消 keeps the entry, 確定 removes it.
- [ ] 新增紀錄 opens an empty editor (start one hour ago, end now, the selected project); a saved entry appears under the right day.
- [ ] While the editor is open, start/stop a timer from the panel: the list behind refreshes, the text you were typing in the editor is untouched.
- [ ] Click an entry in the panel's 今日紀錄: the records window opens (also when it was not open yet) on that entry's month with the editor showing it. Do it again with the records window already open on another month.
- [ ] 匯出 CSV: a save dialog proposes partimer-YYYY-MM.csv; the file contains the filtered month. `head -c 3 <file> | xxd` shows `efbb bf`. Opening it in Excel (Windows and macOS) shows the Chinese correctly. Cancelling the dialog leaves no message.
- [ ] Dark mode: the window and the editor stay readable.

## Settings window (do every item on macOS, Windows and Ubuntu)

Set-up: two clients with two projects each, one archived project, one project with a few entries (one of them running).

- [ ] 開機自動啟動 (do on all three platforms): turning it on makes the app start at the next login (macOS 登入項目, Windows startup, Ubuntu autostart entry); turning it off removes it. When the system refuses, a red message with the reason and the platform's manual steps appears (macOS 「系統設定 → 一般 → 登入項目…」, Windows 「Win+R shell:startup…」, Ubuntu 「初始啟動程式…」). The toggle shows the real state after reopening the window.
- [ ] 浮動小時鐘: the switch shows/hides the clock at once; the opacity slider (40%–100%) changes its opacity after release and the % label follows while dragging; 「沒在計時時也顯示」 shows the resting plant while idle. All three survive restarting the app.
- [ ] 整點提醒 switch: on, a notification arrives at the next full hour while timing.
- [ ] 全域快捷鍵: shows Ctrl+Alt+P (⌃⌥P on macOS). On Ubuntu Wayland a red text with the 自訂快捷鍵 workaround shows; when another program already owns the shortcut, red 「快捷鍵被其他程式占用」 shows; otherwise no red text.
- [ ] Rename: typing in a name box shows 儲存 and 還原; typing the original name back (or only spaces around it) hides them. 儲存 or Enter saves; Enter while an IME candidate window is open only confirms the composition. 還原 restores the old name.
- [ ] Rename rejections show the Chinese reason right under that row and keep what you typed: empty name, a client named like another client, a project named like another project of the same client (same name under a different client is fine).
- [ ] While you type a new name in one row, start/stop a timer from the panel (or rename something in another row): the half-typed text stays, other rows update.
- [ ] 色塊: the 8 swatches match the dots in the panel menu and 紀錄; the current colour has an outline; clicking another colour changes it at once (also in the panel menu dot and in 紀錄). Tab reaches each swatch.
- [ ] 封存: an archived project (or a whole archived client) disappears from the panel's project menu but its entries still show in 紀錄; 取消封存 brings it back. Archiving the running project does not stop the timer.
- [ ] 刪除: a project/client with entries asks 「會刪掉 N 筆紀錄、共 Xh Ym，確定刪除？」 in front of the window and the numbers match 紀錄 (check one with exactly 1h 0m and one under an hour: 「0h」 never shows, e.g. 「45m」); one without entries asks 「確定刪除？」. Deleting a client removes its projects and their entries. If the running entry is included, the dialog adds 「其中包含正在計時的紀錄，計時會一併停止。」 and after 確定 the timer is gone. 取消 changes nothing.
- [ ] 打開資料夾 opens the data folder in Finder / Explorer / the file manager; the path shown above it is the same folder.
- [ ] Dark mode: the window stays readable, the swatch outline is visible on both themes.
