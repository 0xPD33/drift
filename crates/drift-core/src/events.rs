use std::io::Write;
use std::os::unix::net::UnixStream;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    #[serde(rename = "type")]
    pub event_type: String,
    pub project: String,
    pub source: String,
    pub ts: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
}

pub fn iso_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| String::new())
}

pub fn emit_event(event: &Event) -> anyhow::Result<()> {
    let path = paths::emit_socket_path();
    let mut stream = UnixStream::connect(&path)?;
    let json = serde_json::to_string(event)?;
    writeln!(stream, "{json}")?;
    Ok(())
}

pub fn try_emit_event(event: &Event) {
    let _ = emit_event(event);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_event() -> Event {
        Event {
            event_type: "build.complete".into(),
            project: "myapp".into(),
            source: "ci".into(),
            ts: "2026-01-15T10:30:00Z".into(),
            level: None,
            title: None,
            body: None,
            meta: None,
            priority: None,
        }
    }

    fn full_event() -> Event {
        Event {
            event_type: "build.complete".into(),
            project: "myapp".into(),
            source: "ci".into(),
            ts: "2026-01-15T10:30:00Z".into(),
            level: Some("info".into()),
            title: Some("Build succeeded".into()),
            body: Some("All 42 tests passed".into()),
            meta: Some(serde_json::json!({"duration_ms": 1234})),
            priority: Some("high".into()),
        }
    }

    #[test]
    fn event_serialization_roundtrip() {
        let event = full_event();
        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event_type, "build.complete");
        assert_eq!(parsed.project, "myapp");
        assert_eq!(parsed.source, "ci");
        assert_eq!(parsed.ts, "2026-01-15T10:30:00Z");
        assert_eq!(parsed.level.as_deref(), Some("info"));
        assert_eq!(parsed.title.as_deref(), Some("Build succeeded"));
        assert_eq!(parsed.body.as_deref(), Some("All 42 tests passed"));
        assert_eq!(parsed.meta.as_ref().unwrap()["duration_ms"], 1234);
        assert_eq!(parsed.priority.as_deref(), Some("high"));
    }

    #[test]
    fn event_serde_rename_type_key() {
        let event = minimal_event();
        let json = serde_json::to_string(&event).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(val.get("type").is_some(), "JSON should have 'type' key");
        assert!(val.get("event_type").is_none(), "JSON should NOT have 'event_type' key");
    }

    #[test]
    fn event_optional_fields_omitted() {
        let event = minimal_event();
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("\"level\""), "level should be omitted");
        assert!(!json.contains("\"title\""), "title should be omitted");
        assert!(!json.contains("\"body\""), "body should be omitted");
        assert!(!json.contains("\"meta\""), "meta should be omitted");
        assert!(!json.contains("\"priority\""), "priority should be omitted");
    }

    #[test]
    fn event_deserialization_from_type_key() {
        let json = r#"{
            "type": "deploy.started",
            "project": "webapp",
            "source": "cd",
            "ts": "2026-02-01T12:00:00Z",
            "level": "warning"
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "deploy.started");
        assert_eq!(event.project, "webapp");
        assert_eq!(event.level.as_deref(), Some("warning"));
        assert!(event.title.is_none());
    }

    #[test]
    fn iso_now_returns_valid_timestamp() {
        let ts = iso_now();
        assert!(!ts.is_empty(), "iso_now should return a non-empty string");
        assert!(ts.contains('T'), "ISO 8601 timestamp should contain 'T'");
        assert!(
            ts.contains('Z') || ts.contains('+') || ts.contains('-'),
            "ISO 8601 timestamp should contain timezone info"
        );
    }

    #[test]
    fn event_with_meta_json_object() {
        let event = Event {
            meta: Some(serde_json::json!({
                "commit": "abc123",
                "branch": "main",
                "tags": ["v1.0", "latest"]
            })),
            ..minimal_event()
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        let meta = parsed.meta.unwrap();
        assert_eq!(meta["commit"], "abc123");
        assert_eq!(meta["branch"], "main");
        assert_eq!(meta["tags"][0], "v1.0");
        assert_eq!(meta["tags"][1], "latest");
    }
}
