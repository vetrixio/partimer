use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const DATA_VERSION: u32 = 1;

pub type Id = String;

pub fn new_id() -> Id {
    uuid::Uuid::new_v4().to_string()
}

/// Keys this version does not know (e.g. written by a newer minor version), kept verbatim so
/// a save does not drop them.
pub type Extra = Map<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Client {
    pub id: Id,
    pub name: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(flatten)]
    pub extra: Extra,
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
    #[serde(flatten)]
    pub extra: Extra,
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
    #[serde(flatten)]
    pub extra: Extra,
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
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for WidgetSettings {
    fn default() -> Self {
        Self {
            enabled: None,
            opacity: 0.6,
            show_when_idle: false,
            x: None,
            y: None,
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub last_project_id: Option<Id>,
    pub widget: WidgetSettings,
    pub hourly_reminder: bool,
    #[serde(flatten)]
    pub extra: Extra,
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
    #[serde(flatten)]
    pub extra: Extra,
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
            extra: Extra::new(),
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
        match T::deserialize(&raw) {
            Ok(v) => ok.push(v),
            Err(_) => quarantine.push(raw),
        }
    }
    ok
}

/// `obj[key]` if present and of the right type; a bad value is dropped (falls back to the
/// caller's default) rather than kept, so it never clashes with the field on save.
fn take_field<T: serde::de::DeserializeOwned>(
    obj: &mut Map<String, Value>,
    key: &str,
) -> Option<T> {
    obj.remove(key).and_then(|v| T::deserialize(&v).ok())
}

/// Field by field: one bad value resets only itself.
fn parse_widget(v: Value) -> WidgetSettings {
    let d = WidgetSettings::default();
    let Value::Object(mut o) = v else {
        return d;
    };
    WidgetSettings {
        enabled: take_field(&mut o, "enabled").unwrap_or(d.enabled),
        opacity: take_field(&mut o, "opacity").unwrap_or(d.opacity),
        show_when_idle: take_field(&mut o, "showWhenIdle").unwrap_or(d.show_when_idle),
        x: take_field(&mut o, "x").unwrap_or(d.x),
        y: take_field(&mut o, "y").unwrap_or(d.y),
        extra: o,
    }
}

/// Field by field: one bad value resets only itself (e.g. never `lastProjectId` because of
/// a bad widget value).
fn parse_settings(v: Value) -> Settings {
    let Value::Object(mut o) = v else {
        return Settings::default();
    };
    Settings {
        last_project_id: take_field(&mut o, "lastProjectId").unwrap_or_default(),
        widget: o.remove("widget").map(parse_widget).unwrap_or_default(),
        hourly_reminder: take_field(&mut o, "hourlyReminder").unwrap_or_default(),
        extra: o,
    }
}

/// Lenient parser: missing optional fields get defaults, broken
/// clients/projects/entries are quarantined. A list key that is present but
/// not an array is an error (the file is treated as corrupt).
pub fn parse_data(bytes: &[u8]) -> Result<Data, ParseError> {
    let Value::Object(mut obj) = serde_json::from_slice::<Value>(bytes)? else {
        return Err(ParseError::NotObject);
    };

    let version = obj
        .remove("version")
        .as_ref()
        .and_then(Value::as_u64)
        // Too large for u32 is still "newer than us", never "current".
        .map(|v| u32::try_from(v).unwrap_or(u32::MAX))
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
        .map(parse_settings)
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
        // Whatever is left is unknown to this version.
        extra: obj,
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
    fn bad_settings_field_falls_back_alone() {
        let d = parse_data(
            br#"{"settings":{"lastProjectId":"p1","hourlyReminder":"yes",
                 "widget":{"enabled":false,"opacity":"x","showWhenIdle":true,"x":10,"y":1.5}}}"#,
        )
        .unwrap();
        let s = &d.settings;
        assert_eq!(s.last_project_id.as_deref(), Some("p1"));
        assert!(!s.hourly_reminder);
        assert_eq!(s.widget.enabled, Some(false));
        assert_eq!(s.widget.opacity, 0.6);
        assert!(s.widget.show_when_idle);
        assert_eq!(s.widget.x, Some(10));
        assert_eq!(s.widget.y, None);
        // A bad `widget` object as a whole only resets the widget.
        let d = parse_data(br#"{"settings":{"lastProjectId":"p1","widget":7}}"#).unwrap();
        assert_eq!(d.settings.last_project_id.as_deref(), Some("p1"));
        assert_eq!(d.settings.widget, WidgetSettings::default());
        // Not an object at all: defaults.
        let d = parse_data(br#"{"settings":[1]}"#).unwrap();
        assert_eq!(d.settings, Settings::default());
    }

    #[test]
    fn unknown_keys_survive_round_trip() {
        let json = br#"{"version":1,"futureRoot":{"a":1},
            "clients":[{"id":"c1","name":"ok","nick":"Ming"}],
            "projects":[{"id":"p1","clientId":"c1","name":"ok","budget":10},
                        {"id":"p2","clientId":"c1","name":"x","color":300,"budget":1}],
            "entries":[{"id":"e1","projectId":"p1","start":"2026-10-06T01:30:00Z","tags":["a"]}],
            "settings":{"theme":"dark","widget":{"corner":"tr"}}}"#;
        let d = parse_data(json).unwrap();
        assert_eq!(d.projects.len(), 1);
        assert_eq!(d.quarantined_projects.len(), 1);
        let out: Value = serde_json::from_slice(&to_json(&d)).unwrap();
        assert_eq!(out["futureRoot"], serde_json::json!({"a": 1}));
        assert_eq!(out["clients"][0]["nick"], "Ming");
        assert_eq!(out["projects"][0]["budget"], 10);
        assert_eq!(out["entries"][0]["tags"], serde_json::json!(["a"]));
        assert_eq!(out["settings"]["theme"], "dark");
        assert_eq!(out["settings"]["widget"]["corner"], "tr");
        assert_eq!(out["quarantinedProjects"][0]["budget"], 1);
        assert_eq!(out["version"], 1);
        // Known keys are not duplicated into the extras.
        let again = parse_data(&to_json(&d)).unwrap();
        assert_eq!(again, d);
    }

    #[test]
    fn list_key_not_an_array_is_error() {
        for key in ["entries", "clients", "projects", "quarantinedEntries"] {
            let json = format!(r#"{{"{key}":{{}}}}"#);
            assert!(parse_data(json.as_bytes()).is_err(), "{key}");
        }
    }

    #[test]
    fn version_too_large_for_u32_counts_as_newest() {
        let d = parse_data(br#"{"version":99999999999}"#).unwrap();
        assert_eq!(d.version, u32::MAX);
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
