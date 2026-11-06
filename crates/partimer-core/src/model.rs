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
    /// Entries that failed to parse; kept verbatim so no user data is lost.
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
}

fn take_array(obj: &mut Map<String, Value>, key: &str) -> Vec<Value> {
    match obj.remove(key) {
        Some(Value::Array(a)) => a,
        _ => Vec::new(),
    }
}

/// Parse items one by one, silently dropping the ones that do not fit.
fn parse_lenient<T: serde::de::DeserializeOwned>(items: Vec<Value>) -> Vec<T> {
    items
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect()
}

/// Lenient parser: unknown/missing optional fields get defaults, broken
/// clients/projects are skipped, broken entries are quarantined.
pub fn parse_data(bytes: &[u8]) -> Result<Data, ParseError> {
    let Value::Object(mut obj) = serde_json::from_slice::<Value>(bytes)? else {
        return Err(ParseError::NotObject);
    };

    let version = obj
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(DATA_VERSION);
    let clients = parse_lenient(take_array(&mut obj, "clients"));
    let projects = parse_lenient(take_array(&mut obj, "projects"));

    let mut entries = Vec::new();
    let mut quarantined_entries = take_array(&mut obj, "quarantinedEntries");
    for raw in take_array(&mut obj, "entries") {
        match serde_json::from_value::<Entry>(raw.clone()) {
            Ok(e) => entries.push(e),
            Err(_) => quarantined_entries.push(raw),
        }
    }

    let settings = obj
        .remove("settings")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    Ok(Data {
        version,
        clients,
        projects,
        entries,
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
        assert_eq!(again.quarantined_entries.len(), 1);
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
