//! Tray menu model: what the tray shows for the current data, free of any UI toolkit.

use crate::catalog::{self, PIN_LIMIT};
use crate::model::{Data, Id};
use crate::timer;

const NAME_MAX: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinItem {
    pub project_id: Id,
    /// `{client}・{project}` with each part truncated.
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayMenu {
    /// No running entry; empty `pins` means show a hint row.
    Idle { pins: Vec<PinItem> },
    Active {
        paused: bool,
        /// `{client}・{project}` of the running entry.
        current: String,
        /// Effective pins other than the running project.
        others: Vec<PinItem>,
    },
}

/// Names over 20 chars become their first 19 chars plus `…` (chars, not bytes).
pub fn truncate_name(name: &str) -> String {
    if name.chars().count() <= NAME_MAX {
        return name.to_string();
    }
    let mut out: String = name.chars().take(NAME_MAX - 1).collect();
    out.push('…');
    out
}

fn label(client: &str, project: &str) -> String {
    format!("{}・{}", truncate_name(client), truncate_name(project))
}

pub fn tray_menu(data: &Data) -> TrayMenu {
    let client_name = |client_id: &str| {
        data.clients
            .iter()
            .find(|c| c.id == client_id)
            .map_or("?", |c| c.name.as_str())
    };
    let pins: Vec<PinItem> = catalog::effective_pins(data)
        .into_iter()
        .take(PIN_LIMIT)
        .map(|p| PinItem {
            project_id: p.id.clone(),
            label: label(client_name(&p.client_id), &p.name),
        })
        .collect();
    let Some(entry) = timer::running(data) else {
        return TrayMenu::Idle { pins };
    };
    let current = match data.projects.iter().find(|p| p.id == entry.project_id) {
        Some(p) => label(client_name(&p.client_id), &p.name),
        None => label("?", "?"),
    };
    TrayMenu::Active {
        paused: entry.pause_started_at.is_some(),
        current,
        others: pins
            .into_iter()
            .filter(|i| i.project_id != entry.project_id)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Client, Entry, Project};
    use chrono::{TimeZone, Utc};

    fn client(id: &str, name: &str, archived: bool) -> Client {
        Client {
            extra: Default::default(),
            id: id.into(),
            name: name.into(),
            archived,
        }
    }

    fn project(id: &str, cid: &str, name: &str, pinned: bool, archived: bool) -> Project {
        Project {
            extra: Default::default(),
            id: id.into(),
            client_id: cid.into(),
            name: name.into(),
            color: 0,
            archived,
            pinned,
        }
    }

    fn data() -> Data {
        let mut d = Data::default();
        d.clients.push(client("c1", "王小明", false));
        d.projects
            .push(project("p1", "c1", "官網改版", true, false));
        d.projects.push(project("p2", "c1", "設計", true, false));
        d
    }

    fn run(d: &mut Data, pid: &str, paused: bool) {
        let t = Utc.with_ymd_and_hms(2026, 10, 5, 1, 0, 0).unwrap();
        d.entries.push(Entry {
            id: "e".into(),
            project_id: pid.into(),
            start: t,
            end: None,
            title: String::new(),
            content: String::new(),
            paused_seconds: 0,
            pause_started_at: paused.then_some(t),
            extra: Default::default(),
        });
    }

    fn ids(items: &[PinItem]) -> Vec<&str> {
        items.iter().map(|i| i.project_id.as_str()).collect()
    }

    #[test]
    fn idle_lists_effective_pins_in_order() {
        let TrayMenu::Idle { pins } = tray_menu(&data()) else {
            panic!("not idle")
        };
        assert_eq!(ids(&pins), ["p1", "p2"]);
        // "官網改版" < "設計" by name order
        assert_eq!(pins[0].label, "王小明・官網改版");
    }

    #[test]
    fn idle_without_pins_is_empty() {
        let mut d = data();
        d.projects.iter_mut().for_each(|p| p.pinned = false);
        assert_eq!(tray_menu(&d), TrayMenu::Idle { pins: vec![] });
    }

    #[test]
    fn idle_caps_at_five() {
        let mut d = data();
        for i in 0..5 {
            d.projects.push(project(
                &format!("x{i}"),
                "c1",
                &format!("a{i}"),
                true,
                false,
            ));
        }
        let TrayMenu::Idle { pins } = tray_menu(&d) else {
            panic!("not idle")
        };
        assert_eq!(pins.len(), 5);
    }

    #[test]
    fn running_excludes_current_project() {
        let mut d = data();
        run(&mut d, "p1", false);
        let item = PinItem {
            project_id: "p2".into(),
            label: "王小明・設計".into(),
        };
        assert_eq!(
            tray_menu(&d),
            TrayMenu::Active {
                paused: false,
                current: "王小明・官網改版".into(),
                others: vec![item],
            }
        );
    }

    #[test]
    fn paused_sets_flag() {
        let mut d = data();
        run(&mut d, "p1", true);
        let TrayMenu::Active { paused, .. } = tray_menu(&d) else {
            panic!("not active")
        };
        assert!(paused);
    }

    #[test]
    fn running_unpinned_archived_project_still_named() {
        let mut d = data();
        d.projects[0].pinned = false;
        d.projects[0].archived = true;
        run(&mut d, "p1", false);
        let TrayMenu::Active {
            current, others, ..
        } = tray_menu(&d)
        else {
            panic!("not active")
        };
        assert_eq!(current, "王小明・官網改版");
        assert_eq!(ids(&others), ["p2"]);
    }

    #[test]
    fn truncate_long_names() {
        let long = "a".repeat(21);
        assert_eq!(truncate_name(&long), format!("{}…", "a".repeat(19)));
        let exact = "b".repeat(20);
        assert_eq!(truncate_name(&exact), exact);
        let zh = "字".repeat(21);
        assert_eq!(truncate_name(&zh), format!("{}…", "字".repeat(19)));
        assert_eq!(truncate_name(&"字".repeat(20)), "字".repeat(20));
    }

    #[test]
    fn archived_pinned_never_listed() {
        let mut d = data();
        d.projects[1].archived = true;
        d.clients.push(client("c2", "乙", true));
        d.projects.push(project("p3", "c2", "x", true, false));
        let TrayMenu::Idle { pins } = tray_menu(&d) else {
            panic!("not idle")
        };
        assert_eq!(ids(&pins), ["p1"]);
    }
}
