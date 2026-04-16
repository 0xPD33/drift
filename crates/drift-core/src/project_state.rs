use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::events::iso_now;
use crate::paths::project_state_path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectState {
    pub project: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_agent_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<String>,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub docs: Vec<DocPointer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub name: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocPointer {
    pub path: String,
    pub description: String,
}

/// Parse YAML frontmatter from PROJECT.md content.
/// Returns (ProjectState, markdown_body) or None if parsing fails.
fn parse_frontmatter(content: &str) -> Option<(ProjectState, String)> {
    let (yaml, body) = crate::parse_yaml_frontmatter(content)?;

    let state: ProjectState = match serde_yaml::from_str(yaml) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Warning: failed to parse PROJECT.md YAML: {e}");
            return None;
        }
    };

    Some((state, body.to_string()))
}

pub fn read_project_state(repo_path: &Path) -> Option<ProjectState> {
    read_project_state_full(repo_path).map(|(state, _)| state)
}

pub fn read_project_state_full(repo_path: &Path) -> Option<(ProjectState, String)> {
    let path = project_state_path(repo_path);
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return None,
    };
    parse_frontmatter(&content)
}

pub fn write_project_state(
    repo_path: &Path,
    state: &ProjectState,
    body: &str,
) -> std::io::Result<()> {
    let yaml = serde_yaml::to_string(state).map_err(|e| {
        std::io::Error::other(e)
    })?;
    let content = format!("---\n{yaml}---\n{body}");
    let path = project_state_path(repo_path);
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, &content)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn update_from_handoff(
    state: &mut ProjectState,
    agent: &str,
    action: &str,
    component_name: Option<&str>,
    component_status: Option<&str>,
) {
    state.last_agent = Some(agent.to_string());
    state.last_agent_action = Some(action.to_string());
    state.last_updated = Some(iso_now());

    if let (Some(name), Some(status)) = (component_name, component_status) {
        if let Some(comp) = state.components.iter_mut().find(|c| c.name == name) {
            comp.status = status.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_frontmatter() {
        let content = r#"---
project: myapp
status: active
priority: 1
components:
  - name: auth
    status: complete
  - name: api
    status: in-progress
    next_task: "Add validation"
constraints:
  - "No async"
docs:
  - path: docs/arch.md
    description: "Architecture overview"
---
# My App

Some markdown body here.
"#;
        let (state, body) = parse_frontmatter(content).unwrap();
        assert_eq!(state.project, "myapp");
        assert_eq!(state.status, "active");
        assert_eq!(state.priority, Some(1));
        assert_eq!(state.components.len(), 2);
        assert_eq!(state.components[0].name, "auth");
        assert_eq!(state.components[0].status, "complete");
        assert_eq!(state.components[1].next_task.as_deref(), Some("Add validation"));
        assert_eq!(state.constraints.len(), 1);
        assert_eq!(state.docs.len(), 1);
        assert!(body.contains("# My App"));
    }

    #[test]
    fn parse_minimal_frontmatter() {
        let content = "---\nproject: test\nstatus: paused\n---\n";
        let (state, body) = parse_frontmatter(content).unwrap();
        assert_eq!(state.project, "test");
        assert_eq!(state.status, "paused");
        assert!(state.priority.is_none());
        assert!(state.components.is_empty());
        assert!(body.is_empty());
    }

    #[test]
    fn parse_no_frontmatter() {
        assert!(parse_frontmatter("# Just markdown").is_none());
    }

    #[test]
    fn parse_invalid_yaml() {
        let content = "---\n: invalid: yaml: [[\n---\n";
        assert!(parse_frontmatter(content).is_none());
    }

    #[test]
    fn roundtrip_write_read() {
        let dir = tempfile::tempdir().unwrap();
        let state = ProjectState {
            project: "roundtrip".into(),
            status: "active".into(),
            priority: Some(2),
            last_agent: Some("claude".into()),
            last_agent_action: Some("Fixed bug".into()),
            last_updated: Some("2026-03-28T10:00:00Z".into()),
            blocked_by: None,
            components: vec![Component {
                name: "core".into(),
                status: "in-progress".into(),
                next_task: Some("Add tests".into()),
                notes: None,
            }],
            constraints: vec!["No async".into()],
            docs: vec![DocPointer {
                path: "docs/readme.md".into(),
                description: "Main docs".into(),
            }],
        };
        let body = "# Roundtrip\n\nTest body.\n";

        write_project_state(dir.path(), &state, body).unwrap();
        let (loaded, loaded_body) = read_project_state_full(dir.path()).unwrap();

        assert_eq!(loaded.project, "roundtrip");
        assert_eq!(loaded.status, "active");
        assert_eq!(loaded.priority, Some(2));
        assert_eq!(loaded.last_agent.as_deref(), Some("claude"));
        assert_eq!(loaded.components.len(), 1);
        assert_eq!(loaded.constraints.len(), 1);
        assert_eq!(loaded.docs.len(), 1);
        assert!(loaded_body.contains("# Roundtrip"));
    }

    #[test]
    fn update_from_handoff_updates_fields() {
        let mut state = ProjectState {
            project: "test".into(),
            status: "active".into(),
            priority: None,
            last_agent: None,
            last_agent_action: None,
            last_updated: None,
            blocked_by: None,
            components: vec![Component {
                name: "auth".into(),
                status: "in-progress".into(),
                next_task: None,
                notes: None,
            }],
            constraints: vec![],
            docs: vec![],
        };

        update_from_handoff(&mut state, "claude-code", "Implemented auth", Some("auth"), Some("complete"));

        assert_eq!(state.last_agent.as_deref(), Some("claude-code"));
        assert_eq!(state.last_agent_action.as_deref(), Some("Implemented auth"));
        assert!(state.last_updated.is_some());
        assert_eq!(state.components[0].status, "complete");
    }

    #[test]
    fn update_from_handoff_no_component() {
        let mut state = ProjectState {
            project: "test".into(),
            status: "active".into(),
            priority: None,
            last_agent: None,
            last_agent_action: None,
            last_updated: None,
            blocked_by: None,
            components: vec![],
            constraints: vec![],
            docs: vec![],
        };

        update_from_handoff(&mut state, "agent", "Did stuff", None, None);

        assert_eq!(state.last_agent.as_deref(), Some("agent"));
        assert_eq!(state.last_agent_action.as_deref(), Some("Did stuff"));
    }

    #[test]
    fn read_missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_project_state(dir.path()).is_none());
    }
}
