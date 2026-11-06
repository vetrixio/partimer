use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const DATA_VERSION: u32 = 1;

pub type Id = String;

pub fn new_id() -> Id {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Client {
    pub id: Id,
    pub name: String,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: Id,
    pub client_id: Id,
    pub name: String,
    #[serde(default)]
    pub color: u8,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: Id,
    pub project_id: Id,
    pub start: DateTime<Utc>,
    #[serde(default)]
    pub end: Option<DateTime<Utc>>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub paused_seconds: i64,
    #[serde(default)]
    pub pause_started_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WidgetSettings {
    /// `None` means "use the platform default".
    pub enabled: Option<bool>,
    pub opacity: f64,
    pub show_when_idle: bool,
    pub x: Option<i32>,
    pub y: Option<i32>,
}

impl Default for WidgetSettings {
    fn default() -> Self {
        Self {
            enabled: None,
            opacity: 0.6,
            show_when_idle: false,
            x: None,
            y: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub last_project_id: Option<Id>,
    pub widget: WidgetSettings,
    pub hourly_reminder: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Data {
    pub version: u32,
    pub clients: Vec<Client>,
    pub projects: Vec<Project>,
    pub entries: Vec<Entry>,
    /// Items that failed to parse; kept verbatim so no user data is lost.
    pub quarantined_clients: Vec<Value>,
    pub quarantined_projects: Vec<Value>,
    pub quarantined_entries: Vec<Value>,
    pub settings: Settings,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            version: DATA_VERSION,
            clients: Vec::new(),
            projects: Vec::new(),
            entries: Vec::new(),
            quarantined_clients: Vec::new(),
            quarantined_projects: Vec::new(),
            quarantined_entries: Vec::new(),
            settings: Settings::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("top-level JSON value is not an object")]
    NotObject,
    #[error("`{0}` is present but not an array")]
    NotArray(String),
}

fn take_array(obj: &mut Map<String, Value>, key: &str) -> Result<Vec<Value>, ParseError> {
    match obj.remove(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(a)) => Ok(a),
        Some(_) => Err(ParseError::NotArray(key.to_string())),
    }
}

/// Parse items one by one; the ones that do not fit are pushed (verbatim)
/// into `quarantine` so nothing is lost on the next save.
fn parse_or_quarantine<T: serde::de::DeserializeOwned>(
    items: Vec<Value>,
    quarantine: &mut Vec<Value>,
) -> Vec<T> {
    let mut ok = Vec::new();
    for raw in items {
        match serde_json::from_value::<T>(raw.clone()) {
            Ok(v) => ok.push(v),
            Err(_) => quarantine.push(raw),
        }
    }
    ok
}

/// Lenient parser: missing optional fields get defaults, broken
/// clients/projects/entries are quarantined. A list key that is present but
/// not an array is an error (the file is treated as corrupt).
pub fn parse_data(bytes: &[u8]) -> Result<Data, ParseError> {
    let Value::Object(mut obj) = serde_json::from_slice::<Value>(bytes)? else {
        return Err(ParseError::NotObject);
    };

    let version = obj
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(DATA_VERSION);

    let mut quarantined_clients = take_array(&mut obj, "quarantinedClients")?;
    let mut quarantined_projects = take_array(&mut obj, "quarantinedProjects")?;
    let mut quarantined_entries = take_array(&mut obj, "quarantinedEntries")?;
    let clients = parse_or_quarantine(take_array(&mut obj, "clients")?, &mut quarantined_clients);
    let projects =
        parse_or_quarantine(take_array(&mut obj, "projects")?, &mut quarantined_projects);
    let entries = parse_or_quarantine(take_array(&mut obj, "entries")?, &mut quarantined_entries);

    let settings = obj
        .remove("settings")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    Ok(Data {
        version,
        clients,
        projects,
        entries,
        quarantined_clients,
        quarantined_projects,
        quarantined_entries,
        settings,
    })
}

pub fn to_json(data: &Data) -> Vec<u8> {
    serde_json::to_vec_pretty(data).expect("Data is always serializable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_old_file_missing_pause_and_settings() {
        let json = r#"{"clients":[{"id":"c1","name":"王小明"}],
      "projects":[{"id":"p1","clientId":"c1","name":"官網改版"}],
      "entries":[{"id":"e1","projectId":"p1","start":"2026-10-06T01:30:00Z","end":"2026-10-06T03:00:00Z","title":"t","content":""}]}"#;
        let d = parse_data(json.as_bytes()).unwrap();
        assert_eq!(d.entries[0].paused_seconds, 0);
        assert_eq!(d.entries[0].pause_started_at, None);
        assert!(!d.projects[0].archived);
        assert_eq!(d.projects[0].color, 0);
        assert_eq!(d.settings.widget.opacity, 0.6);
        assert_eq!(d.version, DATA_VERSION);
    }

    #[test]
    fn entry_missing_required_field_is_quarantined_not_fatal() {
        // entries[1] has no start
        let d = parse_data(
            br#"{"entries":[{"id":"e1","projectId":"p1","start":"2026-10-06T01:30:00Z"},{"id":"e2","projectId":"p1"}]}"#,
        )
        .unwrap();
        assert_eq!(d.entries.len(), 1);
        assert_eq!(d.quarantined_entries.len(), 1);
        // saved and re-read, the quarantined one survives
        let again = parse_data(&to_json(&d)).unwrap();
        assert_eq!(again.quarantined_entries, d.quarantined_entries);
    }

    #[test]
    fn bad_client_and_project_are_quarantined_and_survive_round_trip() {
        let d = parse_data(
            br#"{"clients":[{"id":"c1","name":"ok"},{"id":"c2","name":null}],
                 "projects":[{"id":"p1","clientId":"c1","name":"ok"},
                             {"id":"p2","clientId":"c1","name":"x","color":300}]}"#,
        )
        .unwrap();
        assert_eq!(d.clients.len(), 1);
        assert_eq!(d.projects.len(), 1);
        assert_eq!(d.quarantined_clients.len(), 1);
        assert_eq!(d.quarantined_projects.len(), 1);
        let again = parse_data(&to_json(&d)).unwrap();
        assert_eq!(again.quarantined_clients, d.quarantined_clients);
        assert_eq!(again.quarantined_projects, d.quarantined_projects);
        assert_eq!(again.clients.len(), 1);
        assert_eq!(again.projects.len(), 1);
    }

    #[test]
    fn list_key_not_an_array_is_error() {
        for key in ["entries", "clients", "projects", "quarantinedEntries"] {
            let json = format!(r#"{{"{key}":{{}}}}"#);
            assert!(parse_data(json.as_bytes()).is_err(), "{key}");
        }
    }

    #[test]
    fn invalid_json_is_error() {
        assert!(parse_data(b"{not json").is_err());
    }

    #[test]
    fn top_level_not_object_is_error() {
        assert!(parse_data(b"[]").is_err());
    }
}
