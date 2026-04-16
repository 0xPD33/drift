use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceDisplayState {
    pub agent_running: Option<String>,
    pub task_summary: Option<String>,
    pub queued_count: usize,
    pub needs_review: bool,
    pub error: bool,
    pub recently_completed: Option<String>,
}

/// Format a workspace name for Niri display.
pub fn format_workspace_name(project: &str, state: &WorkspaceDisplayState) -> String {
    let detail = if state.error {
        "\u{2716} agent error".to_string()
    } else if state.needs_review {
        "\u{26a0} review pending".to_string()
    } else if let Some(ref agent) = state.agent_running {
        if let Some(ref task) = state.task_summary {
            format!("{agent} \u{25b8} {task}")
        } else {
            format!("{agent} \u{25b8} working")
        }
    } else if let Some(ref done) = state.recently_completed {
        format!("\u{2713} {done}")
    } else if state.queued_count > 0 {
        format!("idle \u{00b7} {} queued", state.queued_count)
    } else {
        "idle".to_string()
    };

    let full = format!("{project} \u{00b7} {detail}");
    truncate_to_length(&full, 50)
}

fn truncate_to_length(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max.min(s.len());
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    format!("{}\u{2026}", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_no_tasks() {
        let state = WorkspaceDisplayState::default();
        assert_eq!(format_workspace_name("myapp", &state), "myapp \u{00b7} idle");
    }

    #[test]
    fn idle_with_queued() {
        let state = WorkspaceDisplayState {
            queued_count: 3,
            ..Default::default()
        };
        assert_eq!(
            format_workspace_name("myapp", &state),
            "myapp \u{00b7} idle \u{00b7} 3 queued"
        );
    }

    #[test]
    fn agent_running_with_task() {
        let state = WorkspaceDisplayState {
            agent_running: Some("claude-code".into()),
            task_summary: Some("currency validation".into()),
            ..Default::default()
        };
        assert_eq!(
            format_workspace_name("accora", &state),
            "accora \u{00b7} claude-code \u{25b8} currency validation"
        );
    }

    #[test]
    fn agent_running_no_task() {
        let state = WorkspaceDisplayState {
            agent_running: Some("claude-code".into()),
            ..Default::default()
        };
        assert_eq!(
            format_workspace_name("myapp", &state),
            "myapp \u{00b7} claude-code \u{25b8} working"
        );
    }

    #[test]
    fn needs_review() {
        let state = WorkspaceDisplayState {
            needs_review: true,
            ..Default::default()
        };
        assert_eq!(
            format_workspace_name("myapp", &state),
            "myapp \u{00b7} \u{26a0} review pending"
        );
    }

    #[test]
    fn error_state() {
        let state = WorkspaceDisplayState {
            error: true,
            ..Default::default()
        };
        assert_eq!(
            format_workspace_name("myapp", &state),
            "myapp \u{00b7} \u{2716} agent error"
        );
    }

    #[test]
    fn error_takes_priority_over_review() {
        let state = WorkspaceDisplayState {
            error: true,
            needs_review: true,
            ..Default::default()
        };
        assert_eq!(
            format_workspace_name("myapp", &state),
            "myapp \u{00b7} \u{2716} agent error"
        );
    }

    #[test]
    fn recently_completed() {
        let state = WorkspaceDisplayState {
            recently_completed: Some("validation done".into()),
            ..Default::default()
        };
        assert_eq!(
            format_workspace_name("myapp", &state),
            "myapp \u{00b7} \u{2713} validation done"
        );
    }

    #[test]
    fn truncation_at_50_chars() {
        let state = WorkspaceDisplayState {
            agent_running: Some("claude-code".into()),
            task_summary: Some("this is a very long task description that exceeds".into()),
            ..Default::default()
        };
        let name = format_workspace_name("myproject", &state);
        assert!(name.len() <= 53); // 50 + up to 3 bytes for ellipsis char
        assert!(name.ends_with('\u{2026}'));
    }

    #[test]
    fn no_truncation_when_short() {
        let state = WorkspaceDisplayState::default();
        let name = format_workspace_name("app", &state);
        assert!(!name.contains('\u{2026}'));
    }
}
