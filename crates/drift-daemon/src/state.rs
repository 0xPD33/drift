use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use drift_core::events::Event;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DaemonState {
    pub pid: u32,
    pub active_project: Option<String>,
    pub workspace_projects: Vec<WorkspaceProject>,
    pub recent_events: HashMap<String, Vec<Event>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceProject {
    pub workspace_id: u64,
    pub workspace_name: String,
    pub project: String,
    pub is_active: bool,
    pub is_focused: bool,
    pub window_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_state_default() {
        let state = DaemonState::default();
        assert_eq!(state.pid, 0);
        assert!(state.active_project.is_none());
        assert!(state.workspace_projects.is_empty());
        assert!(state.recent_events.is_empty());
    }

    #[test]
    fn daemon_state_roundtrip() {
        let mut events = HashMap::new();
        events.insert(
            "myapp".into(),
            vec![Event {
                event_type: "notification".into(),
                project: "myapp".into(),
                source: "build".into(),
                ts: "2024-01-01T00:00:00Z".into(),
                level: Some("info".into()),
                title: Some("Build succeeded".into()),
                body: None,
                meta: None,
                priority: None,
            }],
        );

        let state = DaemonState {
            pid: 12345,
            active_project: Some("myapp".into()),
            workspace_projects: vec![
                WorkspaceProject {
                    workspace_id: 1,
                    workspace_name: "myapp".into(),
                    project: "myapp".into(),
                    is_active: true,
                    is_focused: true,
                    window_count: 3,
                },
                WorkspaceProject {
                    workspace_id: 2,
                    workspace_name: "other".into(),
                    project: "other".into(),
                    is_active: false,
                    is_focused: false,
                    window_count: 1,
                },
            ],
            recent_events: events,
        };

        let json = serde_json::to_string_pretty(&state).unwrap();
        let parsed: DaemonState = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.pid, 12345);
        assert_eq!(parsed.active_project.as_deref(), Some("myapp"));
        assert_eq!(parsed.workspace_projects.len(), 2);
        assert_eq!(parsed.workspace_projects[0].workspace_id, 1);
        assert_eq!(parsed.workspace_projects[0].workspace_name, "myapp");
        assert!(parsed.workspace_projects[0].is_active);
        assert!(parsed.workspace_projects[0].is_focused);
        assert_eq!(parsed.workspace_projects[0].window_count, 3);
        assert_eq!(parsed.workspace_projects[1].workspace_id, 2);
        assert!(!parsed.workspace_projects[1].is_active);
        assert!(!parsed.workspace_projects[1].is_focused);
        let evts = parsed.recent_events.get("myapp").unwrap();
        assert_eq!(evts.len(), 1);
        assert_eq!(evts[0].title.as_deref(), Some("Build succeeded"));
    }

    #[test]
    fn daemon_state_no_active_project() {
        let state = DaemonState {
            pid: 999,
            active_project: None,
            workspace_projects: vec![],
            recent_events: HashMap::new(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: DaemonState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.pid, 999);
        assert!(parsed.active_project.is_none());
    }

    #[test]
    fn workspace_project_roundtrip() {
        let wp = WorkspaceProject {
            workspace_id: 42,
            workspace_name: "test-ws".into(),
            project: "test-proj".into(),
            is_active: true,
            is_focused: false,
            window_count: 5,
        };
        let json = serde_json::to_string(&wp).unwrap();
        let parsed: WorkspaceProject = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.workspace_id, 42);
        assert_eq!(parsed.workspace_name, "test-ws");
        assert_eq!(parsed.project, "test-proj");
        assert!(parsed.is_active);
        assert!(!parsed.is_focused);
        assert_eq!(parsed.window_count, 5);
    }

    #[test]
    fn daemon_state_with_events() {
        let mut events = HashMap::new();
        events.insert("proj-a".into(), vec![
            Event {
                event_type: "build.started".into(),
                project: "proj-a".into(),
                source: "ci".into(),
                ts: "2026-01-01T00:00:00Z".into(),
                level: Some("info".into()),
                title: Some("Build started".into()),
                body: None,
                meta: None,
                priority: Some("low".into()),
            },
            Event {
                event_type: "build.complete".into(),
                project: "proj-a".into(),
                source: "ci".into(),
                ts: "2026-01-01T00:01:00Z".into(),
                level: Some("success".into()),
                title: Some("Build succeeded".into()),
                body: Some("42 tests passed".into()),
                meta: Some(serde_json::json!({"duration_ms": 5000})),
                priority: Some("high".into()),
            },
        ]);
        events.insert("proj-b".into(), vec![
            Event {
                event_type: "deploy.failed".into(),
                project: "proj-b".into(),
                source: "cd".into(),
                ts: "2026-01-01T00:02:00Z".into(),
                level: Some("error".into()),
                title: Some("Deploy failed".into()),
                body: Some("Connection timeout".into()),
                meta: None,
                priority: Some("critical".into()),
            },
        ]);

        let state = DaemonState {
            pid: 54321,
            active_project: Some("proj-a".into()),
            workspace_projects: vec![],
            recent_events: events,
        };

        let json = serde_json::to_string_pretty(&state).unwrap();
        let parsed: DaemonState = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.pid, 54321);
        assert_eq!(parsed.active_project.as_deref(), Some("proj-a"));

        let a_events = parsed.recent_events.get("proj-a").unwrap();
        assert_eq!(a_events.len(), 2);
        assert_eq!(a_events[0].event_type, "build.started");
        assert_eq!(a_events[1].event_type, "build.complete");
        assert_eq!(a_events[1].meta.as_ref().unwrap()["duration_ms"], 5000);

        let b_events = parsed.recent_events.get("proj-b").unwrap();
        assert_eq!(b_events.len(), 1);
        assert_eq!(b_events[0].level.as_deref(), Some("error"));
        assert_eq!(b_events[0].priority.as_deref(), Some("critical"));
    }
}
