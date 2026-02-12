use std::fs;

use serde::{Deserialize, Serialize};

use crate::{events::iso_now, paths};

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub projects: Vec<String>,
    pub saved_at: String,
}

pub fn load_session() -> anyhow::Result<Option<Session>> {
    let path = paths::session_path();
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path)?;
    let session: Session = serde_json::from_str(&data)?;
    Ok(Some(session))
}

pub fn add_project(name: &str) -> anyhow::Result<()> {
    let mut session = load_session()?.unwrap_or_else(|| Session {
        projects: Vec::new(),
        saved_at: String::new(),
    });

    if !session.projects.iter().any(|p| p == name) {
        session.projects.push(name.to_string());
    }

    session.saved_at = iso_now();
    write_session(&session)
}

pub fn remove_project(name: &str) -> anyhow::Result<()> {
    let mut session = match load_session()? {
        Some(s) => s,
        None => return Ok(()),
    };

    session.projects.retain(|p| p != name);
    session.saved_at = iso_now();
    write_session(&session)
}

fn write_session(session: &Session) -> anyhow::Result<()> {
    let path = paths::session_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(session)?;
    fs::write(&tmp, &json)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_serialization_roundtrip() {
        let session = Session {
            projects: vec!["alpha".into(), "beta".into(), "gamma".into()],
            saved_at: "2026-01-15T10:30:00Z".into(),
        };
        let json = serde_json::to_string(&session).unwrap();
        let parsed: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.projects, vec!["alpha", "beta", "gamma"]);
        assert_eq!(parsed.saved_at, "2026-01-15T10:30:00Z");
    }

    #[test]
    fn session_empty_projects() {
        let session = Session {
            projects: vec![],
            saved_at: "2026-02-01T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&session).unwrap();
        let parsed: Session = serde_json::from_str(&json).unwrap();
        assert!(parsed.projects.is_empty());
    }
}
