use std::fs;

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};

use crate::events::iso_now;
use crate::paths;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    Failed,
    NeedsReview,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Queued => write!(f, "queued"),
            TaskStatus::Running => write!(f, "running"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Failed => write!(f, "failed"),
            TaskStatus::NeedsReview => write!(f, "needs-review"),
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(TaskStatus::Queued),
            "running" => Ok(TaskStatus::Running),
            "completed" => Ok(TaskStatus::Completed),
            "failed" => Ok(TaskStatus::Failed),
            "needs-review" => Ok(TaskStatus::NeedsReview),
            _ => bail!("Unknown task status: {s}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    pub description: String,
    pub priority: u8,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_passed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

pub struct TaskQueue {
    pub tasks: Vec<Task>,
}

impl TaskQueue {
    pub fn load(project: &str) -> anyhow::Result<TaskQueue> {
        let path = paths::task_queue_path(project);
        if !path.exists() {
            return Ok(TaskQueue { tasks: vec![] });
        }
        let data = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let tasks: Vec<Task> = serde_json::from_str(&data)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(TaskQueue { tasks })
    }

    pub fn save(&self, project: &str) -> anyhow::Result<()> {
        let path = paths::task_queue_path(project);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&self.tasks)?;
        fs::write(&tmp, &json)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn add(&mut self, task: Task) {
        self.tasks.push(task);
    }

    pub fn next(&self) -> Option<&Task> {
        let mut candidates: Vec<&Task> = self.tasks.iter()
            .filter(|t| t.status == TaskStatus::Queued)
            .filter(|t| {
                // Parent dependency check
                if let Some(ref parent_id) = t.parent_task {
                    self.tasks.iter()
                        .find(|p| p.id == *parent_id)
                        .map_or(true, |p| p.status == TaskStatus::Completed)
                } else {
                    true
                }
            })
            .filter(|t| {
                // Component lock check
                if let Some(ref component) = t.component {
                    !self.tasks.iter().any(|other|
                        other.id != t.id &&
                        other.status == TaskStatus::Running &&
                        other.component.as_deref() == Some(component)
                    )
                } else {
                    true
                }
            })
            .collect();

        candidates.sort_by(|a, b| {
            a.priority.cmp(&b.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });

        candidates.first().copied()
    }

    pub fn find(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    pub fn start(&mut self, task_id: &str, agent: &str) -> anyhow::Result<()> {
        let task = self.find_mut(task_id)
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", task_id))?;
        if task.status != TaskStatus::Queued {
            anyhow::bail!("Cannot start task {}: status is {}, expected queued", task_id, task.status);
        }
        task.status = TaskStatus::Running;
        task.assigned_agent = Some(agent.to_string());
        task.started_at = Some(crate::events::iso_now());
        Ok(())
    }

    pub fn complete(
        &mut self,
        task_id: &str,
        handoff_path: Option<String>,
        verified: Option<bool>,
    ) -> anyhow::Result<()> {
        let task = self.find_mut(task_id)
            .with_context(|| format!("task {task_id} not found"))?;
        if !matches!(task.status, TaskStatus::Running | TaskStatus::NeedsReview) {
            bail!("Cannot complete task {}: status is {}, expected running or needs-review", task_id, task.status);
        }
        task.status = TaskStatus::Completed;
        task.completed_at = Some(iso_now());
        task.handoff_path = handoff_path;
        task.verification_passed = verified;
        Ok(())
    }

    pub fn fail(&mut self, task_id: &str, reason: Option<&str>) -> anyhow::Result<()> {
        let task = self.find_mut(task_id)
            .with_context(|| format!("task {task_id} not found"))?;
        if !matches!(task.status, TaskStatus::Running | TaskStatus::NeedsReview) {
            bail!("Cannot fail task {}: status is {}, expected running or needs-review", task_id, task.status);
        }
        task.status = TaskStatus::Failed;
        task.completed_at = Some(iso_now());
        task.failure_reason = reason.map(|s| s.to_string());
        Ok(())
    }

    pub fn needs_review(&mut self, task_id: &str) -> anyhow::Result<()> {
        let task = self.find_mut(task_id)
            .with_context(|| format!("task {task_id} not found"))?;
        if task.status != TaskStatus::Running {
            bail!("Cannot mark task {} as needs-review: status is {}, expected running", task_id, task.status);
        }
        task.status = TaskStatus::NeedsReview;
        Ok(())
    }

    pub fn cancel(&mut self, task_id: &str) -> anyhow::Result<()> {
        let idx = self.tasks.iter().position(|t| t.id == task_id)
            .with_context(|| format!("task {task_id} not found"))?;
        if self.tasks[idx].status != TaskStatus::Queued {
            bail!("Can only cancel queued tasks (task {} is {})", task_id, self.tasks[idx].status);
        }
        self.tasks.remove(idx);
        Ok(())
    }

    pub fn pending_reviews(&self) -> Vec<&Task> {
        self.tasks.iter().filter(|t| t.status == TaskStatus::NeedsReview).collect()
    }

    pub fn active_tasks(&self) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|t| !matches!(t.status, TaskStatus::Completed | TaskStatus::Failed))
            .collect()
    }
}

pub fn generate_task_id() -> String {
    use rand::Rng;
    let n: u32 = rand::thread_rng().gen_range(0..0xFFFFFF);
    format!("tsk-{:06x}", n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: &str, priority: u8, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            project: "test".to_string(),
            component: None,
            description: "Test task".to_string(),
            priority,
            status,
            assigned_agent: None,
            agent_type: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            started_at: None,
            completed_at: None,
            handoff_path: None,
            parent_task: None,
            verification_passed: None,
            failure_reason: None,
        }
    }

    #[test]
    fn task_status_serialization_roundtrip() {
        let task = make_task("tsk-abc123", 3, TaskStatus::Queued);
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"queued\""));
        let parsed: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, TaskStatus::Queued);
    }

    #[test]
    fn task_status_display() {
        assert_eq!(TaskStatus::Queued.to_string(), "queued");
        assert_eq!(TaskStatus::NeedsReview.to_string(), "needs-review");
    }

    #[test]
    fn task_status_from_str() {
        assert_eq!("queued".parse::<TaskStatus>().unwrap(), TaskStatus::Queued);
        assert_eq!("needs-review".parse::<TaskStatus>().unwrap(), TaskStatus::NeedsReview);
        assert!("bogus".parse::<TaskStatus>().is_err());
    }

    #[test]
    fn next_returns_highest_priority_first() {
        let mut q = TaskQueue { tasks: vec![] };
        q.add(make_task("low", 5, TaskStatus::Queued));
        q.add(make_task("high", 1, TaskStatus::Queued));
        q.add(make_task("mid", 3, TaskStatus::Queued));
        assert_eq!(q.next().unwrap().id, "high");
    }

    #[test]
    fn next_fifo_within_same_priority() {
        let mut q = TaskQueue { tasks: vec![] };
        let mut t1 = make_task("first", 3, TaskStatus::Queued);
        t1.created_at = "2026-01-01T00:00:00Z".to_string();
        let mut t2 = make_task("second", 3, TaskStatus::Queued);
        t2.created_at = "2026-01-02T00:00:00Z".to_string();
        q.add(t1);
        q.add(t2);
        assert_eq!(q.next().unwrap().id, "first");
    }

    #[test]
    fn next_skips_non_queued() {
        let mut q = TaskQueue { tasks: vec![] };
        q.add(make_task("running", 1, TaskStatus::Running));
        q.add(make_task("queued", 3, TaskStatus::Queued));
        assert_eq!(q.next().unwrap().id, "queued");
    }

    #[test]
    fn next_returns_none_when_empty() {
        let q = TaskQueue { tasks: vec![] };
        assert!(q.next().is_none());
    }

    #[test]
    fn start_sets_fields() {
        let mut q = TaskQueue { tasks: vec![make_task("t1", 3, TaskStatus::Queued)] };
        q.start("t1", "agent-1").unwrap();
        let t = q.find("t1").unwrap();
        assert_eq!(t.status, TaskStatus::Running);
        assert_eq!(t.assigned_agent.as_deref(), Some("agent-1"));
        assert!(t.started_at.is_some());
    }

    #[test]
    fn complete_sets_fields() {
        let mut q = TaskQueue { tasks: vec![make_task("t1", 3, TaskStatus::Running)] };
        q.complete("t1", Some("/tmp/handoff".into()), Some(true)).unwrap();
        let t = q.find("t1").unwrap();
        assert_eq!(t.status, TaskStatus::Completed);
        assert!(t.completed_at.is_some());
        assert_eq!(t.handoff_path.as_deref(), Some("/tmp/handoff"));
        assert_eq!(t.verification_passed, Some(true));
    }

    #[test]
    fn fail_sets_fields() {
        let mut q = TaskQueue { tasks: vec![make_task("t1", 3, TaskStatus::Running)] };
        q.fail("t1", Some("crashed")).unwrap();
        let t = q.find("t1").unwrap();
        assert_eq!(t.status, TaskStatus::Failed);
        assert!(t.completed_at.is_some());
        assert_eq!(t.failure_reason.as_deref(), Some("crashed"));
    }

    #[test]
    fn cancel_removes_queued_task() {
        let mut q = TaskQueue { tasks: vec![make_task("t1", 3, TaskStatus::Queued)] };
        q.cancel("t1").unwrap();
        assert!(q.find("t1").is_none());
    }

    #[test]
    fn cancel_rejects_non_queued() {
        let mut q = TaskQueue { tasks: vec![make_task("t1", 3, TaskStatus::Running)] };
        assert!(q.cancel("t1").is_err());
    }

    #[test]
    fn active_tasks_excludes_terminal() {
        let mut q = TaskQueue { tasks: vec![] };
        q.add(make_task("queued", 3, TaskStatus::Queued));
        q.add(make_task("running", 3, TaskStatus::Running));
        q.add(make_task("done", 3, TaskStatus::Completed));
        q.add(make_task("failed", 3, TaskStatus::Failed));
        q.add(make_task("review", 3, TaskStatus::NeedsReview));
        let active = q.active_tasks();
        assert_eq!(active.len(), 3);
    }

    #[test]
    fn pending_reviews_filters_correctly() {
        let mut q = TaskQueue { tasks: vec![] };
        q.add(make_task("queued", 3, TaskStatus::Queued));
        q.add(make_task("review", 3, TaskStatus::NeedsReview));
        assert_eq!(q.pending_reviews().len(), 1);
        assert_eq!(q.pending_reviews()[0].id, "review");
    }

    #[test]
    fn generate_task_id_format() {
        let id = generate_task_id();
        assert!(id.starts_with("tsk-"));
        assert_eq!(id.len(), 10);
    }

    #[test]
    fn next_skips_parent_not_completed() {
        let mut q = TaskQueue { tasks: vec![] };
        q.add(make_task("parent", 1, TaskStatus::Running));
        let mut child = make_task("child", 1, TaskStatus::Queued);
        child.parent_task = Some("parent".to_string());
        q.add(child);
        assert!(q.next().is_none());
    }

    #[test]
    fn next_returns_task_with_completed_parent() {
        let mut q = TaskQueue { tasks: vec![] };
        q.add(make_task("parent", 1, TaskStatus::Completed));
        let mut child = make_task("child", 1, TaskStatus::Queued);
        child.parent_task = Some("parent".to_string());
        q.add(child);
        assert_eq!(q.next().unwrap().id, "child");
    }

    #[test]
    fn next_skips_component_locked() {
        let mut q = TaskQueue { tasks: vec![] };
        let mut running = make_task("running", 1, TaskStatus::Running);
        running.component = Some("foo".to_string());
        q.add(running);
        let mut queued = make_task("queued", 1, TaskStatus::Queued);
        queued.component = Some("foo".to_string());
        q.add(queued);
        assert!(q.next().is_none());
    }

    #[test]
    fn next_returns_task_with_free_component() {
        let mut q = TaskQueue { tasks: vec![] };
        let mut running = make_task("running", 1, TaskStatus::Running);
        running.component = Some("bar".to_string());
        q.add(running);
        let mut queued = make_task("queued", 1, TaskStatus::Queued);
        queued.component = Some("foo".to_string());
        q.add(queued);
        assert_eq!(q.next().unwrap().id, "queued");
    }

    #[test]
    fn next_allows_no_component() {
        let mut q = TaskQueue { tasks: vec![] };
        let mut running = make_task("running", 1, TaskStatus::Running);
        running.component = Some("foo".to_string());
        q.add(running);
        q.add(make_task("queued", 1, TaskStatus::Queued));
        assert_eq!(q.next().unwrap().id, "queued");
    }

    #[test]
    fn task_serialization_omits_none_fields() {
        let task = make_task("t1", 3, TaskStatus::Queued);
        let json = serde_json::to_string(&task).unwrap();
        assert!(!json.contains("component"));
        assert!(!json.contains("assigned_agent"));
        assert!(!json.contains("handoff_path"));
    }
}
