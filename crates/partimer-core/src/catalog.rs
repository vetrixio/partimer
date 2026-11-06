use chrono::{DateTime, Utc};

use crate::duration::{effective_seconds, round_minutes};
use crate::model::{Client, Data, Entry, Id, Project, new_id};

pub const COLOR_COUNT: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    #[error("名稱不能空白")]
    EmptyName,
    #[error("已經有同名的客戶")]
    DuplicateClient,
    #[error("這個客戶底下已經有同名的專案")]
    DuplicateProject,
    #[error("顏色不正確")]
    InvalidColor,
    #[error("找不到這個項目")]
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeletionImpact {
    pub entry_count: usize,
    pub total_minutes: i64,
    pub includes_running: bool,
}

fn clean_name(name: &str) -> Result<&str, CatalogError> {
    let name = name.trim();
    if name.is_empty() {
        Err(CatalogError::EmptyName)
    } else {
        Ok(name)
    }
}

/// Adds a project, creating the client when no client has the same trimmed name.
/// Returns the new project id.
pub fn add_project(
    data: &mut Data,
    client_name: &str,
    project_name: &str,
) -> Result<Id, CatalogError> {
    let client_name = clean_name(client_name)?;
    let project_name = clean_name(project_name)?;
    let existing = data
        .clients
        .iter()
        .find(|c| c.name == client_name)
        .map(|c| c.id.clone());
    let (client_id, color) = match existing {
        Some(id) => {
            let siblings = data.projects.iter().filter(|p| p.client_id == id);
            if siblings.clone().any(|p| p.name == project_name) {
                return Err(CatalogError::DuplicateProject);
            }
            let count = siblings.count();
            // Unarchive the merged client so the new project shows in `active_groups`.
            if let Some(c) = data.clients.iter_mut().find(|c| c.id == id) {
                c.archived = false;
            }
            (id, (count % COLOR_COUNT as usize) as u8)
        }
        None => {
            let id = new_id();
            data.clients.push(Client {
                id: id.clone(),
                name: client_name.to_string(),
                archived: false,
            });
            (id, 0)
        }
    };
    let id = new_id();
    data.projects.push(Project {
        id: id.clone(),
        client_id,
        name: project_name.to_string(),
        color,
        archived: false,
    });
    Ok(id)
}

pub fn rename_client(data: &mut Data, id: &str, name: &str) -> Result<(), CatalogError> {
    let name = clean_name(name)?;
    if !data.clients.iter().any(|c| c.id == id) {
        return Err(CatalogError::NotFound);
    }
    if data.clients.iter().any(|c| c.id != id && c.name == name) {
        return Err(CatalogError::DuplicateClient);
    }
    if let Some(c) = data.clients.iter_mut().find(|c| c.id == id) {
        c.name = name.to_string();
    }
    Ok(())
}

pub fn rename_project(data: &mut Data, id: &str, name: &str) -> Result<(), CatalogError> {
    let name = clean_name(name)?;
    let client_id = data
        .projects
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.client_id.clone())
        .ok_or(CatalogError::NotFound)?;
    if data
        .projects
        .iter()
        .any(|p| p.id != id && p.client_id == client_id && p.name == name)
    {
        return Err(CatalogError::DuplicateProject);
    }
    if let Some(p) = data.projects.iter_mut().find(|p| p.id == id) {
        p.name = name.to_string();
    }
    Ok(())
}

pub fn set_project_color(data: &mut Data, id: &str, color: u8) -> Result<(), CatalogError> {
    if color >= COLOR_COUNT {
        return Err(CatalogError::InvalidColor);
    }
    let p = data
        .projects
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or(CatalogError::NotFound)?;
    p.color = color;
    Ok(())
}

pub fn set_client_archived(data: &mut Data, id: &str, archived: bool) -> Result<(), CatalogError> {
    let c = data
        .clients
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or(CatalogError::NotFound)?;
    c.archived = archived;
    Ok(())
}

pub fn set_project_archived(data: &mut Data, id: &str, archived: bool) -> Result<(), CatalogError> {
    let p = data
        .projects
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or(CatalogError::NotFound)?;
    p.archived = archived;
    Ok(())
}

fn impact_of<'a>(entries: impl Iterator<Item = &'a Entry>, now: DateTime<Utc>) -> DeletionImpact {
    let mut impact = DeletionImpact {
        entry_count: 0,
        total_minutes: 0,
        includes_running: false,
    };
    for e in entries {
        impact.entry_count += 1;
        impact.total_minutes += round_minutes(effective_seconds(e, now));
        impact.includes_running |= e.end.is_none();
    }
    impact
}

pub fn project_deletion_impact(
    data: &Data,
    id: &str,
    now: DateTime<Utc>,
) -> Result<DeletionImpact, CatalogError> {
    if !data.projects.iter().any(|p| p.id == id) {
        return Err(CatalogError::NotFound);
    }
    Ok(impact_of(
        data.entries.iter().filter(|e| e.project_id == id),
        now,
    ))
}

pub fn client_deletion_impact(
    data: &Data,
    id: &str,
    now: DateTime<Utc>,
) -> Result<DeletionImpact, CatalogError> {
    if !data.clients.iter().any(|c| c.id == id) {
        return Err(CatalogError::NotFound);
    }
    let in_client = |project_id: &str| {
        data.projects
            .iter()
            .any(|p| p.id == project_id && p.client_id == id)
    };
    Ok(impact_of(
        data.entries.iter().filter(|e| in_client(&e.project_id)),
        now,
    ))
}

/// Removes the projects with the given ids and their entries; returns the removed entries.
fn remove_projects(data: &mut Data, ids: &[Id]) -> Vec<Entry> {
    data.projects.retain(|p| !ids.contains(&p.id));
    let (removed, kept): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut data.entries)
        .into_iter()
        .partition(|e| ids.contains(&e.project_id));
    data.entries = kept;
    if data
        .settings
        .last_project_id
        .as_ref()
        .is_some_and(|id| ids.contains(id))
    {
        data.settings.last_project_id = None;
    }
    removed
}

pub fn delete_project(data: &mut Data, id: &str) -> Result<Vec<Entry>, CatalogError> {
    if !data.projects.iter().any(|p| p.id == id) {
        return Err(CatalogError::NotFound);
    }
    Ok(remove_projects(data, &[id.to_string()]))
}

pub fn delete_client(data: &mut Data, id: &str) -> Result<Vec<Entry>, CatalogError> {
    if !data.clients.iter().any(|c| c.id == id) {
        return Err(CatalogError::NotFound);
    }
    let project_ids: Vec<Id> = data
        .projects
        .iter()
        .filter(|p| p.client_id == id)
        .map(|p| p.id.clone())
        .collect();
    data.clients.retain(|c| c.id != id);
    Ok(remove_projects(data, &project_ids))
}

/// Clients (sorted by name) with their projects (sorted by name), hiding archived
/// clients, archived projects, and clients left with no project.
pub fn active_groups(data: &Data) -> Vec<(&Client, Vec<&Project>)> {
    let mut groups: Vec<(&Client, Vec<&Project>)> = data
        .clients
        .iter()
        .filter(|c| !c.archived)
        .map(|c| {
            let mut ps: Vec<&Project> = data
                .projects
                .iter()
                .filter(|p| p.client_id == c.id && !p.archived)
                .collect();
            ps.sort_by(|a, b| a.name.cmp(&b.name));
            (c, ps)
        })
        .filter(|(_, ps)| !ps.is_empty())
        .collect();
    groups.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::utc;
    use chrono::{DateTime, Utc};

    fn entry(project_id: &str, start: &str, end: Option<&str>) -> Entry {
        Entry {
            id: new_id(),
            project_id: project_id.to_string(),
            start: utc(start),
            end: end.map(utc),
            title: String::new(),
            content: String::new(),
            paused_seconds: 0,
            pause_started_at: None,
        }
    }

    fn now() -> DateTime<Utc> {
        utc("2026-11-06T12:00:00Z")
    }

    #[test]
    fn add_project_creates_client() {
        let mut d = Data::default();
        let pid = add_project(&mut d, "王小明", "網站").unwrap();
        assert_eq!(d.clients.len(), 1);
        assert_eq!(d.clients[0].name, "王小明");
        assert_eq!(d.projects[0].id, pid);
        assert_eq!(d.projects[0].client_id, d.clients[0].id);
        assert_eq!(d.projects[0].color, 0);
    }

    #[test]
    fn add_project_merges_same_client_name() {
        let mut d = Data::default();
        add_project(&mut d, "王小明", "網站").unwrap();
        add_project(&mut d, " 王小明 ", " App ").unwrap();
        assert_eq!(d.clients.len(), 1);
        assert_eq!(d.projects.len(), 2);
        assert_eq!(d.projects[1].name, "App");
        assert_eq!(d.projects[1].color, 1);
    }

    #[test]
    fn add_project_rejects_empty() {
        let mut d = Data::default();
        assert_eq!(add_project(&mut d, "  ", "x"), Err(CatalogError::EmptyName));
        assert_eq!(add_project(&mut d, "x", " "), Err(CatalogError::EmptyName));
        assert!(d.clients.is_empty() && d.projects.is_empty());
    }

    #[test]
    fn add_project_rejects_duplicate_in_same_client() {
        let mut d = Data::default();
        add_project(&mut d, "A", "P").unwrap();
        assert_eq!(
            add_project(&mut d, " A", "P "),
            Err(CatalogError::DuplicateProject)
        );
        let id = d.projects[0].id.clone();
        set_project_archived(&mut d, &id, true).unwrap();
        assert_eq!(
            add_project(&mut d, "A", "P"),
            Err(CatalogError::DuplicateProject)
        );
        assert_eq!(d.projects.len(), 1);
    }

    #[test]
    fn same_project_name_allowed_in_other_client() {
        let mut d = Data::default();
        add_project(&mut d, "A", "P").unwrap();
        add_project(&mut d, "B", "P").unwrap();
        assert_eq!(d.projects.len(), 2);
    }

    #[test]
    fn rename_client_rejects_blank_and_duplicate() {
        let mut d = Data::default();
        add_project(&mut d, "A", "P").unwrap();
        add_project(&mut d, "B", "P").unwrap();
        let a = d.clients[0].id.clone();
        assert_eq!(
            rename_client(&mut d, &a, "  "),
            Err(CatalogError::EmptyName)
        );
        assert_eq!(
            rename_client(&mut d, &a, " B "),
            Err(CatalogError::DuplicateClient)
        );
        assert_eq!(
            rename_client(&mut d, "nope", "C"),
            Err(CatalogError::NotFound)
        );
        rename_client(&mut d, &a, " C ").unwrap();
        assert_eq!(d.clients[0].name, "C");
    }

    #[test]
    fn rename_project_checks_within_client() {
        let mut d = Data::default();
        let p1 = add_project(&mut d, "A", "P1").unwrap();
        add_project(&mut d, "A", "P2").unwrap();
        let other = add_project(&mut d, "B", "P2").unwrap();
        assert_eq!(
            rename_project(&mut d, &p1, "P2"),
            Err(CatalogError::DuplicateProject)
        );
        assert_eq!(
            rename_project(&mut d, &p1, " "),
            Err(CatalogError::EmptyName)
        );
        rename_project(&mut d, &other, "P1").unwrap();
        rename_project(&mut d, &p1, " P3 ").unwrap();
        assert_eq!(d.projects[0].name, "P3");
    }

    #[test]
    fn rename_to_same_name_ok() {
        let mut d = Data::default();
        let p = add_project(&mut d, "A", "P").unwrap();
        let c = d.clients[0].id.clone();
        rename_client(&mut d, &c, "A").unwrap();
        rename_project(&mut d, &p, " P ").unwrap();
    }

    #[test]
    fn color_out_of_range_rejected() {
        let mut d = Data::default();
        let p = add_project(&mut d, "A", "P").unwrap();
        assert_eq!(
            set_project_color(&mut d, &p, 8),
            Err(CatalogError::InvalidColor)
        );
        set_project_color(&mut d, &p, 7).unwrap();
        assert_eq!(d.projects[0].color, 7);
        assert_eq!(
            set_project_color(&mut d, "nope", 1),
            Err(CatalogError::NotFound)
        );
    }

    #[test]
    fn archived_hidden_from_active_groups() {
        let mut d = Data::default();
        let _b1 = add_project(&mut d, "B", "b1").unwrap();
        let a1 = add_project(&mut d, "A", "z").unwrap();
        add_project(&mut d, "A", "y").unwrap();
        let c1 = add_project(&mut d, "C", "c1").unwrap();
        let g = active_groups(&d);
        let names: Vec<_> = g.iter().map(|(c, _)| c.name.as_str()).collect();
        assert_eq!(names, ["A", "B", "C"]);
        let a_projects: Vec<_> = g[0].1.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(a_projects, ["y", "z"]);

        set_project_archived(&mut d, &a1, true).unwrap();
        let g = active_groups(&d);
        assert_eq!(g[0].1.len(), 1);

        set_project_archived(&mut d, &c1, true).unwrap();
        assert_eq!(active_groups(&d).len(), 2); // C has no active projects

        let b = d.clients.iter().find(|c| c.name == "B").unwrap().id.clone();
        set_client_archived(&mut d, &b, true).unwrap();
        let g = active_groups(&d);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].0.name, "A");
        assert_eq!(
            set_client_archived(&mut d, "nope", true),
            Err(CatalogError::NotFound)
        );
    }

    #[test]
    fn impact_counts_entries_and_minutes() {
        let mut d = Data::default();
        let p = add_project(&mut d, "A", "P").unwrap();
        d.entries.push(entry(
            &p,
            "2026-11-01T01:00:00Z",
            Some("2026-11-01T02:30:00Z"),
        ));
        d.entries.push(entry(
            &p,
            "2026-11-02T01:00:00Z",
            Some("2026-11-02T01:30:00Z"),
        ));
        let i = project_deletion_impact(&d, &p, now()).unwrap();
        assert_eq!(
            i,
            DeletionImpact {
                entry_count: 2,
                total_minutes: 120,
                includes_running: false
            }
        );
        let c = d.clients[0].id.clone();
        assert_eq!(client_deletion_impact(&d, &c, now()).unwrap(), i);
        assert_eq!(
            project_deletion_impact(&d, "nope", now()),
            Err(CatalogError::NotFound)
        );
        assert_eq!(
            client_deletion_impact(&d, "nope", now()),
            Err(CatalogError::NotFound)
        );
    }

    #[test]
    fn impact_flags_running_entry() {
        let mut d = Data::default();
        let p = add_project(&mut d, "A", "P").unwrap();
        d.entries.push(entry(&p, "2026-11-06T11:00:00Z", None));
        let i = project_deletion_impact(&d, &p, now()).unwrap();
        assert!(i.includes_running);
        assert_eq!(i.entry_count, 1);
        assert_eq!(i.total_minutes, 60);
    }

    #[test]
    fn delete_project_cascades_entries() {
        let mut d = Data::default();
        let p1 = add_project(&mut d, "A", "P1").unwrap();
        let p2 = add_project(&mut d, "A", "P2").unwrap();
        d.entries.push(entry(
            &p1,
            "2026-11-01T01:00:00Z",
            Some("2026-11-01T02:00:00Z"),
        ));
        d.entries.push(entry(&p1, "2026-11-06T11:00:00Z", None));
        d.entries.push(entry(
            &p2,
            "2026-11-01T01:00:00Z",
            Some("2026-11-01T02:00:00Z"),
        ));
        let removed = delete_project(&mut d, &p1).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(removed.iter().any(|e| e.end.is_none()));
        assert_eq!(d.projects.len(), 1);
        assert_eq!(d.entries.len(), 1);
        assert_eq!(d.clients.len(), 1);
        assert_eq!(delete_project(&mut d, &p1), Err(CatalogError::NotFound));
    }

    #[test]
    fn delete_client_cascades_projects_and_entries() {
        let mut d = Data::default();
        let p1 = add_project(&mut d, "A", "P1").unwrap();
        let p2 = add_project(&mut d, "A", "P2").unwrap();
        let other = add_project(&mut d, "B", "P").unwrap();
        for p in [&p1, &p2, &other] {
            d.entries.push(entry(
                p,
                "2026-11-01T01:00:00Z",
                Some("2026-11-01T02:00:00Z"),
            ));
        }
        let a = d.clients[0].id.clone();
        let removed = delete_client(&mut d, &a).unwrap();
        assert_eq!(removed.len(), 2);
        assert_eq!(d.clients.len(), 1);
        assert_eq!(d.projects.len(), 1);
        assert_eq!(d.projects[0].id, other);
        assert_eq!(d.entries.len(), 1);
        assert_eq!(delete_client(&mut d, &a), Err(CatalogError::NotFound));
    }

    #[test]
    fn delete_clears_last_project_id() {
        let mut d = Data::default();
        let p1 = add_project(&mut d, "A", "P1").unwrap();
        let p2 = add_project(&mut d, "B", "P2").unwrap();
        d.settings.last_project_id = Some(p1.clone());
        delete_project(&mut d, &p1).unwrap();
        assert_eq!(d.settings.last_project_id, None);

        d.settings.last_project_id = Some(p2.clone());
        let b = d.clients[1].id.clone();
        delete_client(&mut d, &b).unwrap();
        assert_eq!(d.settings.last_project_id, None);
    }

    #[test]
    fn delete_keeps_unrelated_last_project_id() {
        let mut d = Data::default();
        let p1 = add_project(&mut d, "A", "P1").unwrap();
        let p2 = add_project(&mut d, "B", "P2").unwrap();
        d.settings.last_project_id = Some(p2.clone());
        delete_project(&mut d, &p1).unwrap();
        assert_eq!(d.settings.last_project_id, Some(p2));
    }

    #[test]
    fn add_project_unarchives_merged_client() {
        let mut d = Data::default();
        add_project(&mut d, "王小明", "舊案").unwrap();
        let c = d.clients[0].id.clone();
        set_client_archived(&mut d, &c, true).unwrap();
        let p = add_project(&mut d, " 王小明 ", "新案").unwrap();
        assert_eq!(d.clients.len(), 1);
        assert!(!d.clients[0].archived);
        let g = active_groups(&d);
        assert!(g[0].1.iter().any(|x| x.id == p));
    }

    #[test]
    fn color_wraps_after_eight_projects() {
        let mut d = Data::default();
        let mut last = String::new();
        for i in 0..9 {
            last = add_project(&mut d, "A", &format!("P{i}")).unwrap();
        }
        let p = d.projects.iter().find(|p| p.id == last).unwrap();
        assert_eq!(p.color, 0);
        assert_eq!(d.projects[7].color, 7);
    }

    #[test]
    fn client_impact_flags_running_entry() {
        let mut d = Data::default();
        let p = add_project(&mut d, "A", "P").unwrap();
        d.entries.push(entry(&p, "2026-11-06T11:00:00Z", None));
        let c = d.clients[0].id.clone();
        assert!(
            client_deletion_impact(&d, &c, now())
                .unwrap()
                .includes_running
        );
    }

    #[test]
    fn error_display_is_chinese() {
        assert_eq!(CatalogError::EmptyName.to_string(), "名稱不能空白");
        assert_eq!(
            CatalogError::DuplicateClient.to_string(),
            "已經有同名的客戶"
        );
        assert_eq!(
            CatalogError::DuplicateProject.to_string(),
            "這個客戶底下已經有同名的專案"
        );
        assert_eq!(CatalogError::InvalidColor.to_string(), "顏色不正確");
        assert_eq!(CatalogError::NotFound.to_string(), "找不到這個項目");
    }
}
