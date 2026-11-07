//! The once-a-second tick: save retry, reminders, tray update, `state-changed` broadcast.
//! The same thread also steps the tray's awake animation every 500 ms, so the icon is only
//! ever changed from one thread.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use partimer_core::{Notice, RealFs, Reminder, Session, TimerStatus};
use tauri::plugin::PermissionState;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::{AppState, commands, lock, tray, windows};

const FRAME_INTERVAL: Duration = Duration::from_millis(500);
/// The session ticks every second frame, i.e. once a second.
const FRAMES_PER_TICK: u64 = 2;
/// A floating clock that should be on screen but is missing (e.g. its build failed) is
/// asked for again at most this often.
const WIDGET_RETRY_INTERVAL: Duration = Duration::from_secs(5);

/// Set when the app is exiting: the loop stops instead of ticking through shutdown.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// What the last `state-changed` reflected, whether the ticker or a command sent it, so the
/// tick right after a command does not broadcast the same change again.
static LAST_EMITTED: Mutex<Option<Snapshot>> = Mutex::new(None);

/// Stop the loop (called on exit, before the final flush).
pub fn stop() {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

/// Remember the state a command is about to broadcast. Call without holding the session.
pub fn note_broadcast(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let snapshot = snapshot_of(&lock(&state.session), Utc::now());
    *lock(&LAST_EMITTED) = Some(snapshot);
}

fn snapshot_of(session: &Session<RealFs>, now: DateTime<Utc>) -> Snapshot {
    let (status, elapsed) = session.timer_status(now);
    Snapshot {
        status,
        minute: elapsed / 60,
        notices: session.notices().to_vec(),
    }
}

/// The next frame's deadline and how long to sleep for it. Deadlines are absolute, so the
/// time spent ticking does not add up into drift; after a long stall (sleep, suspend) the
/// schedule restarts from now instead of firing a burst of catch-up frames.
fn next_deadline(prev: Instant, now: Instant, interval: Duration) -> (Instant, Duration) {
    let target = prev + interval;
    if target > now {
        (target, target - now)
    } else {
        (now, Duration::ZERO)
    }
}

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

/// Ask for the floating clock again when it should be visible but no window exists and the
/// last request was at least `WIDGET_RETRY_INTERVAL` ago.
fn widget_retry_due(wanted: bool, exists: bool, last: Option<Instant>, now: Instant) -> bool {
    wanted && !exists && last.is_none_or(|t| now.duration_since(t) >= WIDGET_RETRY_INTERVAL)
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
            let mut widget_asked: Option<Instant> = None;
            let mut deadline = Instant::now();
            for frame in (0..FRAMES_PER_TICK).cycle() {
                if SHUTDOWN.load(Ordering::SeqCst) {
                    break;
                }
                // Animate before ticking: a timer that just started shows frame 0 first.
                tray::advance_frame(&app);
                if frame == 0
                    && let Some(snapshot) = tick(&app, last.as_ref(), &mut widget_asked)
                {
                    last = Some(snapshot);
                }
                let (next, wait) = next_deadline(deadline, Instant::now(), FRAME_INTERVAL);
                deadline = next;
                thread::sleep(wait);
            }
        });
    if let Err(e) = started {
        eprintln!("partimer: cannot start the ticker: {e}");
    }
}

fn tick(
    app: &AppHandle,
    last: Option<&Snapshot>,
    widget_asked: &mut Option<Instant>,
) -> Option<Snapshot> {
    let state = app.try_state::<AppState>()?;
    let now = Utc::now();
    // Lock → read/mutate → drop; notifications, tray, windows and events come after.
    let (reminders, panel, widget_wanted) = {
        let mut session = lock(&state.session);
        let reminders = session.tick(now);
        let panel = session.panel_state(now);
        let wanted = windows::widget_wanted(&session.settings().widget, panel.status);
        (reminders, panel, wanted)
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
    let instant = Instant::now();
    if status_changed(last, &snapshot)
        || widget_retry_due(
            widget_wanted,
            windows::widget_exists(app),
            *widget_asked,
            instant,
        )
    {
        *widget_asked = Some(instant);
        windows::ensure_widget(app);
    }
    let mut emitted = lock(&LAST_EMITTED);
    if should_emit(emitted.as_ref(), &snapshot, !reminders.is_empty()) {
        *emitted = Some(snapshot.clone());
        drop(emitted);
        commands::emit_state_changed(app);
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
    fn missing_widget_is_asked_for_again_at_most_every_five_seconds() {
        let t0 = Instant::now();
        let after = |secs| t0 + Duration::from_secs(secs);
        assert!(widget_retry_due(true, false, None, t0));
        assert!(!widget_retry_due(true, false, Some(t0), after(4)));
        assert!(widget_retry_due(true, false, Some(t0), after(5)));
        assert!(!widget_retry_due(true, true, Some(t0), after(60)), "exists");
        assert!(!widget_retry_due(false, false, None, t0), "not wanted");
    }

    #[test]
    fn deadlines_are_absolute_and_restart_after_a_stall() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        // The tick took 120 ms: sleep only the rest of the 500 ms frame.
        assert_eq!(
            next_deadline(t0, t0 + ms(120), ms(500)),
            (t0 + ms(500), ms(380))
        );
        // Late by 100 ms (e.g. after a slow tick): run now, no extra wait.
        assert_eq!(
            next_deadline(t0, t0 + ms(600), ms(500)),
            (t0 + ms(600), ms(0))
        );
        // Several deadlines in a row keep the 500 ms grid.
        let (d1, _) = next_deadline(t0, t0 + ms(10), ms(500));
        let (d2, w2) = next_deadline(d1, t0 + ms(530), ms(500));
        assert_eq!((d2, w2), (t0 + ms(1000), ms(470)));
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
