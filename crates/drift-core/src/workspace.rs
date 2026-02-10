use std::fs;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::{niri::NiriClient, paths};

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub project: String,
    pub saved_at: String,
    pub windows: Vec<SavedWindow>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedWindow {
    pub app_id: Option<String>,
    pub title: Option<String>,
}

pub fn save_workspace(project: &str) -> anyhow::Result<()> {
    let mut client = NiriClient::connect()?;
    let ws = client.find_workspace_by_name(project)?;
    let ws_id = match ws {
        Some(ws) => ws.id,
        None => anyhow::bail!("workspace '{project}' not found"),
    };

    let all_windows = client.windows()?;
    let windows: Vec<SavedWindow> = all_windows
        .into_iter()
        .filter(|w| w.workspace_id == Some(ws_id))
        .map(|w| SavedWindow {
            app_id: w.app_id.clone(),
            title: w.title.clone(),
        })
        .collect();

    write_snapshot(project, windows)
}

pub fn load_workspace_snapshot(project: &str) -> anyhow::Result<Option<WorkspaceSnapshot>> {
    let path = paths::workspace_state_path(project);
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path)?;
    let snapshot: WorkspaceSnapshot = serde_json::from_str(&data)?;
    Ok(Some(snapshot))
}

pub fn write_snapshot(project: &str, windows: Vec<SavedWindow>) -> anyhow::Result<()> {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let snapshot = WorkspaceSnapshot {
        project: project.to_string(),
        saved_at: format!("{secs}"),
        windows,
    };

    let path = paths::workspace_state_path(project);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(&snapshot)?;
    fs::write(&tmp, &json)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_snapshot_serialization_roundtrip() {
        let snapshot = WorkspaceSnapshot {
            project: "myapp".into(),
            saved_at: "1700000000".into(),
            windows: vec![
                SavedWindow {
                    app_id: Some("org.mozilla.firefox".into()),
                    title: Some("Home - Firefox".into()),
                },
                SavedWindow {
                    app_id: Some("com.mitchellh.ghostty".into()),
                    title: Some("~/code/myapp".into()),
                },
            ],
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let parsed: WorkspaceSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.project, "myapp");
        assert_eq!(parsed.saved_at, "1700000000");
        assert_eq!(parsed.windows.len(), 2);
        assert_eq!(parsed.windows[0].app_id.as_deref(), Some("org.mozilla.firefox"));
        assert_eq!(parsed.windows[1].title.as_deref(), Some("~/code/myapp"));
    }

    #[test]
    fn saved_window_both_fields_none() {
        let window = SavedWindow {
            app_id: None,
            title: None,
        };
        let json = serde_json::to_string(&window).unwrap();
        let parsed: SavedWindow = serde_json::from_str(&json).unwrap();
        assert!(parsed.app_id.is_none());
        assert!(parsed.title.is_none());
    }

    #[test]
    fn saved_window_both_fields_some() {
        let window = SavedWindow {
            app_id: Some("kitty".into()),
            title: Some("terminal".into()),
        };
        let json = serde_json::to_string(&window).unwrap();
        let parsed: SavedWindow = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.app_id.as_deref(), Some("kitty"));
        assert_eq!(parsed.title.as_deref(), Some("terminal"));
    }

    #[test]
    fn load_workspace_snapshot_nonexistent_path() {
        let result = load_workspace_snapshot("nonexistent_project_that_does_not_exist_xyz_12345").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn workspace_snapshot_write_and_load_roundtrip() {
        let snapshot = WorkspaceSnapshot {
            project: "roundtrip-test".into(),
            saved_at: "9999999".into(),
            windows: vec![
                SavedWindow {
                    app_id: Some("app1".into()),
                    title: Some("Win 1".into()),
                },
            ],
        };
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        let parsed: WorkspaceSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.project, "roundtrip-test");
        assert_eq!(parsed.saved_at, "9999999");
        assert_eq!(parsed.windows.len(), 1);
        assert_eq!(parsed.windows[0].app_id.as_deref(), Some("app1"));
    }
}
