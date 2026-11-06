//! The once-a-second tick: save retry, reminders, tray update, `state-changed` broadcast.
//! The same thread also steps the tray's awake animation every 500 ms, so the icon is only
//! ever changed from one thread.

use std::thread;
use std::time::Duration;

use chrono::Utc;
use partimer_core::{Notice, Reminder, TimerStatus};
use tauri::plugin::PermissionState;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::{AppState, commands, lock, tray, windows};

const FRAME_INTERVAL: Duration = Duration::from_millis(500);
/// The session ticks every second frame, i.e. once a second.
const FRAMES_PER_TICK: u64 = 2;

/// What the open windows show that changes with time alone. Broadcasting only when it
/// changes keeps the records window from re-rendering every second.
#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    status: TimerStatus,
    /// Elapsed whole minutes (the windows show `H:MM`).
    minute: i64,
    /// Save errors appear / disappear here when a retried save fails or succeeds.
    notices: Vec<Notice>,
}

fn should_emit(prev: Option<&Snapshot>, next: &Snapshot, reminded: bool) -> bool {
    reminded || prev != Some(next)
}

/// The floating clock depends on the status only (and on settings, which update it
/// themselves), so the first tick and every status change re-check it.
fn status_changed(prev: Option<&Snapshot>, next: &Snapshot) -> bool {
    prev.map(|p| p.status) != Some(next.status)
}

fn reminder_text(reminder: &Reminder) -> String {
    match reminder {
        Reminder::LongRunning {
            project_name,
            hours,
        } => format!("是不是忘了停？已計時 {hours} 小時・{project_name}"),
        Reminder::Hourly {
            project_name,
            hours,
        } => format!("已計時 {hours} 小時・{project_name}"),
    }
}

pub fn spawn(app: &AppHandle) {
    let app = app.clone();
    let started = thread::Builder::new()
        .name("partimer-ticker".into())
        .spawn(move || {
            let mut last: Option<Snapshot> = None;
            for frame in (0..FRAMES_PER_TICK).cycle() {
                // Animate before ticking: a timer that just started shows frame 0 first.
                tray::advance_frame(&app);
                if frame == 0
                    && let Some(snapshot) = tick(&app, last.as_ref())
                {
                    last = Some(snapshot);
                }
                thread::sleep(FRAME_INTERVAL);
            }
        });
    if let Err(e) = started {
        eprintln!("partimer: cannot start the ticker: {e}");
    }
}

fn tick(app: &AppHandle, last: Option<&Snapshot>) -> Option<Snapshot> {
    let state = app.try_state::<AppState>()?;
    let now = Utc::now();
    // Lock → read/mutate → drop; notifications, tray and events come after.
    let (reminders, panel) = {
        let mut session = lock(&state.session);
        let reminders = session.tick(now);
        (reminders, session.panel_state(now))
    };

    for reminder in &reminders {
        notify(app, &reminder_text(reminder));
    }
    let project_name = panel.running.as_ref().map(|e| e.project_name.as_str());
    tray::update(app, panel.status, panel.elapsed_seconds, project_name);

    let snapshot = Snapshot {
        status: panel.status,
        minute: panel.elapsed_seconds / 60,
        notices: panel.notices,
    };
    if status_changed(last, &snapshot) {
        windows::ensure_widget(app);
    }
    if should_emit(last, &snapshot, !reminders.is_empty()) {
        commands::changed(app);
    }
    Some(snapshot)
}

/// Show a system notification, asking for permission the first time it is needed. When the
/// user said no, the reminder is silently skipped (the panel still shows the 12-hour prompt).
fn notify(app: &AppHandle, body: &str) {
    let notification = app.notification();
    let allowed = match notification.permission_state() {
        Ok(PermissionState::Granted) => true,
        Ok(PermissionState::Prompt | PermissionState::PromptWithRationale) => matches!(
            notification.request_permission(),
            Ok(PermissionState::Granted)
        ),
        _ => false,
    };
    if !allowed {
        return;
    }
    if let Err(e) = notification
        .builder()
        .title("打工人計時器")
        .body(body)
        .show()
    {
        eprintln!("partimer: notification: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use partimer_core::{Notice, NoticeKind};

    fn snap(status: TimerStatus, elapsed: i64) -> Snapshot {
        Snapshot {
            status,
            minute: elapsed / 60,
            notices: Vec::new(),
        }
    }

    #[test]
    fn widget_is_updated_on_first_tick_and_status_changes_only() {
        let running = snap(TimerStatus::Running, 60);
        assert!(status_changed(None, &running));
        assert!(!status_changed(
            Some(&running),
            &snap(TimerStatus::Running, 600)
        ));
        assert!(status_changed(
            Some(&running),
            &snap(TimerStatus::Paused, 60)
        ));
        assert!(status_changed(Some(&running), &snap(TimerStatus::Idle, 0)));
    }

    #[test]
    fn long_running_reminder_text() {
        let r = Reminder::LongRunning {
            project_name: "官網改版".into(),
            hours: 12,
        };
        assert_eq!(reminder_text(&r), "是不是忘了停？已計時 12 小時・官網改版");
    }

    #[test]
    fn hourly_reminder_text() {
        let r = Reminder::Hourly {
            project_name: "官網改版".into(),
            hours: 3,
        };
        assert_eq!(reminder_text(&r), "已計時 3 小時・官網改版");
    }

    #[test]
    fn first_tick_always_broadcasts() {
        assert!(should_emit(None, &snap(TimerStatus::Idle, 0), false));
    }

    #[test]
    fn same_status_and_minute_does_not_broadcast() {
        let prev = snap(TimerStatus::Running, 61);
        assert!(!should_emit(
            Some(&prev),
            &snap(TimerStatus::Running, 119),
            false
        ));
    }

    #[test]
    fn minute_status_reminder_or_notice_change_broadcasts() {
        let prev = snap(TimerStatus::Running, 119);
        assert!(should_emit(
            Some(&prev),
            &snap(TimerStatus::Running, 120),
            false
        ));
        assert!(should_emit(
            Some(&prev),
            &snap(TimerStatus::Paused, 119),
            false
        ));
        assert!(should_emit(
            Some(&prev),
            &snap(TimerStatus::Running, 119),
            true
        ));
        let mut failed = snap(TimerStatus::Running, 119);
        failed.notices.push(Notice {
            id: 1,
            kind: NoticeKind::SaveError,
            message: "存檔失敗，每秒自動重試：磁碟已滿".into(),
            dismissable: false,
        });
        assert!(should_emit(Some(&prev), &failed, false));
        assert!(should_emit(Some(&failed), &prev, false));
    }
}
