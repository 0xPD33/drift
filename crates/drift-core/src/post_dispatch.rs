use anyhow::Result;

use crate::{config, events, handoff, paths, project_state, tasks};
use crate::handoff::HandoffStatus;

struct FileConflict {
    other_task_id: String,
    overlapping_files: Vec<String>,
}

fn check_file_conflicts(project: &str, task_id: &str, files_changed: &[String]) -> Vec<FileConflict> {
    if files_changed.is_empty() {
        return vec![];
    }

    let queue = match tasks::TaskQueue::load(project) {
        Ok(q) => q,
        Err(_) => return vec![],
    };

    let mut conflicts = Vec::new();

    // Only check last 20 completed tasks to keep it lightweight
    let completed: Vec<&tasks::Task> = queue.tasks.iter()
        .filter(|t| t.id != task_id && t.status == tasks::TaskStatus::Completed)
        .rev()
        .take(20)
        .collect();

    for other_task in completed {
        let other_handoff_path = paths::handoff_path(project, &other_task.id);
        if let Ok((other_handoff, _)) = handoff::read_handoff(&other_handoff_path) {
            let overlapping: Vec<String> = files_changed.iter()
                .filter(|f| other_handoff.files_changed.contains(f))
                .cloned()
                .collect();

            if !overlapping.is_empty() {
                conflicts.push(FileConflict {
                    other_task_id: other_task.id.clone(),
                    overlapping_files: overlapping,
                });
            }
        }
    }

    conflicts
}

/// Process a completed dispatch: read handoff, verify, update state, emit events.
pub fn process_completed_dispatch(project: &str, task_id: &str) -> Result<()> {
    let mut queue = tasks::TaskQueue::load(project)?;
    let task = queue
        .find(task_id)
        .ok_or_else(|| anyhow::anyhow!("Task {} not found", task_id))?;

    // Check task is in a processable state
    let processable = task.status == tasks::TaskStatus::Running;

    // Check for handoff file
    let handoff_file = paths::handoff_path(project, task_id);
    if !handoff_file.exists() {
        if processable {
            queue.fail(task_id, Some("no-handoff: agent exited without writing handoff file"))?;
            queue.save(project)?;
        }
        emit_task_event("task.failed", project, task_id, "no-handoff");
        return Ok(());
    }

    // Parse handoff
    let (ho, body) = match handoff::read_handoff(&handoff_file) {
        Ok(h) => h,
        Err(e) => {
            if processable {
                queue.fail(task_id, Some(&format!("invalid-handoff: {}", e)))?;
                queue.save(project)?;
            }
            emit_task_event("task.failed", project, task_id, "invalid-handoff");
            return Ok(());
        }
    };

    // Check agent's self-assessment
    match ho.status {
        HandoffStatus::Failed => {
            let reason = extract_section(&body, "## What was done")
                .unwrap_or_else(|| "agent reported failure".to_string());
            if processable {
                queue.fail(task_id, Some(&reason))?;
                queue.save(project)?;
            }
            emit_task_event("task.failed", project, task_id, "agent-failed");
            return Ok(());
        }
        HandoffStatus::NeedsReview => {
            if processable {
                queue.needs_review(task_id)?;
                queue.save(project)?;
            }
            emit_task_event("task.needs_review", project, task_id, "agent-requested-review");
            return Ok(());
        }
        HandoffStatus::Blocked => {
            if processable {
                queue.fail(task_id, Some("agent reported blocked state"))?;
                queue.save(project)?;
            }
            emit_task_event("task.failed", project, task_id, "agent-blocked");
            return Ok(());
        }
        HandoffStatus::Completed => {
            // Continue to verification
        }
    }

    // File conflict detection
    let conflicts = check_file_conflicts(project, task_id, &ho.files_changed);
    if !conflicts.is_empty() {
        let conflict_details: Vec<String> = conflicts.iter()
            .map(|c| format!("{}: {}", c.other_task_id, c.overlapping_files.join(", ")))
            .collect();

        if processable {
            queue.needs_review(task_id)?;
            queue.save(project)?;
        }

        events::try_emit_event(&events::Event {
            event_type: "task.needs_review".into(),
            project: project.into(),
            source: "post-dispatch".into(),
            ts: events::iso_now(),
            level: Some("warning".into()),
            title: Some(format!("Task {} has file conflicts", task_id)),
            body: Some(format!("Overlapping files with: {}", conflict_details.join("; "))),
            meta: Some(serde_json::json!({
                "task_id": task_id,
                "reason": "file-conflict",
                "conflicts": conflicts.iter().map(|c| serde_json::json!({
                    "task": c.other_task_id,
                    "files": c.overlapping_files,
                })).collect::<Vec<_>>(),
            })),
            priority: None,
        });
        return Ok(());
    }

    // Task should already be Running at this point

    // Run verification gate (if configured)
    let project_config = config::load_project_config(project)?;
    let verification_passed = if let Some(ref verify) = project_config.verification {
        let repo_path = config::resolve_repo_path(&project_config.project.repo)?;
        let timeout = verify.timeout_sec.unwrap_or(300);
        match handoff::run_verification(&verify.command, &repo_path, timeout) {
            Ok(result) => {
                if !result.passed {
                    queue.needs_review(task_id)?;
                    queue.save(project)?;
                    events::try_emit_event(&events::Event {
                        event_type: "task.needs_review".into(),
                        project: project.into(),
                        source: "post-dispatch".into(),
                        ts: events::iso_now(),
                        level: Some("warning".into()),
                        title: Some(format!("Task {} verification failed", task_id)),
                        body: Some(result.output),
                        meta: Some(serde_json::json!({"task_id": task_id, "reason": "verification-failed"})),
                        priority: None,
                    });
                    return Ok(());
                }
                Some(true)
            }
            Err(e) => {
                eprintln!("Verification error: {e}");
                Some(false)
            }
        }
    } else {
        None
    };

    // Update PROJECT.md (only after verification passes)
    let repo_path = config::resolve_repo_path(&project_config.project.repo)?;
    if let Some((mut state, md_body)) = project_state::read_project_state_full(&repo_path) {
        let action = extract_section(&body, "## What was done")
            .unwrap_or_else(|| "completed task".to_string());
        let action_summary = action.lines().next().unwrap_or("completed task").trim();
        let component = task_component(project, task_id);
        let has_children = queue.tasks.iter().any(|t| t.parent_task.as_deref() == Some(task_id));
        let component_status = if has_children { "in-progress" } else { "complete" };
        project_state::update_from_handoff(
            &mut state,
            &ho.agent,
            action_summary,
            component.as_deref(),
            Some(component_status),
        );
        let _ = project_state::write_project_state(&repo_path, &state, &md_body);
    }

    // Mark task completed
    let handoff_path_str = handoff_file.to_string_lossy().to_string();
    queue.complete(task_id, Some(handoff_path_str), verification_passed)?;
    queue.save(project)?;

    // Emit completion event
    events::try_emit_event(&events::Event {
        event_type: "task.completed".into(),
        project: project.into(),
        source: "post-dispatch".into(),
        ts: events::iso_now(),
        level: Some("success".into()),
        title: Some(format!("Task {} completed", task_id)),
        body: None,
        meta: Some(serde_json::json!({
            "task_id": task_id,
            "agent": ho.agent,
            "files_changed": ho.files_changed.len(),
            "tests_passed": ho.tests_passed,
            "verification_passed": verification_passed,
        })),
        priority: None,
    });

    Ok(())
}

fn extract_section(body: &str, heading: &str) -> Option<String> {
    let mut capturing = false;
    let mut result = String::new();
    for line in body.lines() {
        if line.starts_with(heading) {
            capturing = true;
            continue;
        } else if capturing && line.starts_with("## ") {
            break;
        } else if capturing {
            result.push_str(line);
            result.push('\n');
        }
    }
    let trimmed = result.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

fn task_component(project: &str, task_id: &str) -> Option<String> {
    let queue = tasks::TaskQueue::load(project).ok()?;
    queue.find(task_id)?.component.clone()
}

fn emit_task_event(event_type: &str, project: &str, task_id: &str, reason: &str) {
    events::try_emit_event(&events::Event {
        event_type: event_type.into(),
        project: project.into(),
        source: "post-dispatch".into(),
        ts: events::iso_now(),
        level: Some(if event_type.contains("failed") { "error" } else { "warning" }.into()),
        title: Some(format!("Task {}", task_id)),
        body: None,
        meta: Some(serde_json::json!({"task_id": task_id, "reason": reason})),
        priority: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Use the real state dir with a unique project name to avoid env var races in parallel tests.
    fn unique_project(prefix: &str) -> String {
        use rand::Rng;
        let n: u32 = rand::thread_rng().gen_range(0..0xFFFFFF);
        format!("{prefix}-{n:06x}")
    }

    struct TestProject {
        name: String,
    }

    impl TestProject {
        fn new(prefix: &str) -> Self {
            let name = unique_project(prefix);
            let state = paths::state_dir(&name);
            std::fs::create_dir_all(state.join("handoffs")).unwrap();
            TestProject { name }
        }

        fn write_task_queue(&self, tasks_json: &str) {
            let path = paths::task_queue_path(&self.name);
            std::fs::write(&path, tasks_json).unwrap();
        }

        fn write_handoff_file(&self, task_id: &str, files: &[&str]) {
            let path = paths::handoff_path(&self.name, task_id);
            let files_yaml: String = files.iter()
                .map(|f| format!("  - {}", f))
                .collect::<Vec<_>>()
                .join("\n");
            let content = format!(
                "---\ntask_id: {}\nstatus: completed\nagent: test-agent\nfiles_changed:\n{}\n---\n\n## What was done\nTest.\n",
                task_id, files_yaml
            );
            let mut f = std::fs::File::create(&path).unwrap();
            write!(f, "{}", content).unwrap();
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(paths::state_dir(&self.name));
        }
    }

    #[test]
    fn check_file_conflicts_detects_overlap() {
        let tp = TestProject::new("conflict");

        let tasks_json = serde_json::json!([
            {
                "id": "tsk-aaa",
                "project": tp.name,
                "description": "Task A",
                "priority": 3,
                "status": "completed",
                "created_at": "2026-03-28T00:00:00Z",
                "completed_at": "2026-03-28T01:00:00Z"
            },
            {
                "id": "tsk-bbb",
                "project": tp.name,
                "description": "Task B",
                "priority": 3,
                "status": "running",
                "created_at": "2026-03-28T00:00:00Z"
            }
        ]);
        tp.write_task_queue(&tasks_json.to_string());
        tp.write_handoff_file("tsk-aaa", &["src/main.rs", "src/lib.rs"]);

        let conflicts = check_file_conflicts(&tp.name, "tsk-bbb", &["src/main.rs".to_string(), "src/new.rs".to_string()]);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].other_task_id, "tsk-aaa");
        assert_eq!(conflicts[0].overlapping_files, vec!["src/main.rs"]);
    }

    #[test]
    fn check_file_conflicts_no_overlap() {
        let tp = TestProject::new("no-conflict");

        let tasks_json = serde_json::json!([
            {
                "id": "tsk-aaa",
                "project": tp.name,
                "description": "Task A",
                "priority": 3,
                "status": "completed",
                "created_at": "2026-03-28T00:00:00Z",
                "completed_at": "2026-03-28T01:00:00Z"
            }
        ]);
        tp.write_task_queue(&tasks_json.to_string());
        tp.write_handoff_file("tsk-aaa", &["src/other.rs"]);

        let conflicts = check_file_conflicts(&tp.name, "tsk-bbb", &["src/main.rs".to_string()]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn check_file_conflicts_empty_files() {
        let conflicts = check_file_conflicts("nonexistent-project", "tsk-x", &[]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn check_file_conflicts_skips_self() {
        let tp = TestProject::new("self");

        let tasks_json = serde_json::json!([
            {
                "id": "tsk-aaa",
                "project": tp.name,
                "description": "Task A",
                "priority": 3,
                "status": "completed",
                "created_at": "2026-03-28T00:00:00Z",
                "completed_at": "2026-03-28T01:00:00Z"
            }
        ]);
        tp.write_task_queue(&tasks_json.to_string());
        tp.write_handoff_file("tsk-aaa", &["src/main.rs"]);

        let conflicts = check_file_conflicts(&tp.name, "tsk-aaa", &["src/main.rs".to_string()]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn check_file_conflicts_skips_non_completed() {
        let tp = TestProject::new("status");

        let tasks_json = serde_json::json!([
            {
                "id": "tsk-running",
                "project": tp.name,
                "description": "Running task",
                "priority": 3,
                "status": "running",
                "created_at": "2026-03-28T00:00:00Z"
            }
        ]);
        tp.write_task_queue(&tasks_json.to_string());
        tp.write_handoff_file("tsk-running", &["src/main.rs"]);

        let conflicts = check_file_conflicts(&tp.name, "tsk-bbb", &["src/main.rs".to_string()]);
        assert!(conflicts.is_empty());
    }
}
