use std::fs;

use clap::Subcommand;
use drift_core::{config, dispatch, worktree};
use drift_core::events::{self, Event};
use drift_core::paths;
use drift_core::tasks::{self, Task, TaskQueue, TaskStatus};

#[derive(Subcommand)]
pub enum TaskCommand {
    /// Add a new task to the queue
    Add {
        /// Project name
        project: String,
        /// Task description
        description: String,
        /// Component name
        #[arg(long)]
        component: Option<String>,
        /// Priority (1=highest, 5=lowest)
        #[arg(long, default_value = "3")]
        priority: u8,
        /// Agent type to use
        #[arg(long)]
        agent: Option<String>,
        /// Parent task ID
        #[arg(long)]
        parent: Option<String>,
    },
    /// List tasks
    List {
        /// Project name (optional, lists all if omitted)
        project: Option<String>,
        /// Show all tasks including completed/failed
        #[arg(long)]
        all: bool,
        /// Filter by status
        #[arg(long)]
        status: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show next dispatchable task
    Next {
        /// Project name
        project: String,
    },
    /// Mark a task as completed
    Complete {
        /// Task ID
        task_id: String,
    },
    /// Mark a task as failed
    Fail {
        /// Task ID
        task_id: String,
        /// Failure reason
        #[arg(long)]
        reason: Option<String>,
    },
    /// Cancel a queued task
    Cancel {
        /// Task ID
        task_id: String,
    },
    /// Launch an agent to work on a specific task
    Assign {
        /// Task ID
        task_id: String,
        /// Override agent type
        #[arg(long)]
        agent: Option<String>,
        /// Override model
        #[arg(long)]
        model: Option<String>,
        /// Show prompt and command without launching
        #[arg(long)]
        dry_run: bool,
    },
    /// Create a chain of dependent tasks (each depends on the previous)
    Chain {
        /// Project name
        project: String,
        /// Task descriptions in sequence order
        descriptions: Vec<String>,
        /// Component for all tasks in the chain
        #[arg(long)]
        component: Option<String>,
        /// Priority for all tasks (1=highest, 5=lowest)
        #[arg(long, default_value = "3")]
        priority: u8,
        /// Agent type for all tasks
        #[arg(long)]
        agent: Option<String>,
    },
    /// List active worktrees for a project
    Worktrees {
        /// Project name
        project: String,
    },
    /// Clean up worktrees for completed/failed tasks
    CleanWorktrees {
        /// Project name
        project: String,
    },
}

pub fn run(cmd: TaskCommand) -> anyhow::Result<()> {
    match cmd {
        TaskCommand::Add {
            project,
            description,
            component,
            priority,
            agent,
            parent,
        } => {
            let task = Task {
                id: tasks::generate_task_id(),
                project: project.clone(),
                component,
                description: description.clone(),
                priority,
                status: TaskStatus::Queued,
                assigned_agent: None,
                agent_type: agent,
                created_at: events::iso_now(),
                started_at: None,
                completed_at: None,
                handoff_path: None,
                parent_task: parent,
                verification_passed: None,
                failure_reason: None,
            };
            let task_id = task.id.clone();
            let mut queue = TaskQueue::load(&project)?;
            queue.add(task);
            queue.save(&project)?;

            events::try_emit_event(&Event {
                event_type: "task.queued".into(),
                project: project.clone(),
                source: "cli".into(),
                ts: events::iso_now(),
                level: Some("info".into()),
                title: Some("Task queued".into()),
                body: Some(format!("{task_id}: {}", truncate(&description, 60))),
                meta: Some(serde_json::json!({
                    "task_id": task_id,
                    "priority": priority,
                })),
                priority: None,
            });

            println!("{task_id}");
            Ok(())
        }

        TaskCommand::List {
            project,
            all,
            status,
            json,
        } => {
            let status_filter: Option<TaskStatus> = status
                .as_deref()
                .map(|s| s.parse())
                .transpose()?;

            let projects = match project {
                Some(p) => vec![p],
                None => list_project_dirs()?,
            };

            let mut all_tasks: Vec<Task> = Vec::new();
            for proj in &projects {
                let queue = TaskQueue::load(proj)?;
                all_tasks.extend(queue.tasks);
            }

            // Filter
            let filtered: Vec<&Task> = all_tasks
                .iter()
                .filter(|t| {
                    if let Some(ref sf) = status_filter {
                        return &t.status == sf;
                    }
                    if all {
                        return true;
                    }
                    !matches!(t.status, TaskStatus::Completed | TaskStatus::Failed)
                })
                .collect();

            if json {
                println!("{}", serde_json::to_string_pretty(&filtered)?);
                return Ok(());
            }

            if filtered.is_empty() {
                println!("No tasks found.");
                return Ok(());
            }

            println!(
                "{:<12} {:<4} {:<13} {:<40} AGENT",
                "ID", "PRI", "STATUS", "DESCRIPTION"
            );
            for task in &filtered {
                let desc = truncate(&task.description, 38);
                let agent = task.assigned_agent.as_deref().unwrap_or("-");
                println!(
                    "{:<12} {:<4} {:<13} {:<40} {}",
                    task.id, task.priority, task.status, desc, agent
                );
            }
            Ok(())
        }

        TaskCommand::Next { project } => {
            let queue = TaskQueue::load(&project)?;
            match queue.next() {
                Some(task) => {
                    println!("ID:          {}", task.id);
                    println!("Priority:    {}", task.priority);
                    println!("Description: {}", task.description);
                    if let Some(ref comp) = task.component {
                        println!("Component:   {comp}");
                    }
                    if let Some(ref agent) = task.agent_type {
                        println!("Agent type:  {agent}");
                    }
                }
                None => println!("No tasks pending."),
            }
            Ok(())
        }

        TaskCommand::Complete { task_id } => {
            let (project, mut queue) = find_task_queue(&task_id)?;
            queue.complete(&task_id, None, None)?;
            queue.save(&project)?;

            events::try_emit_event(&Event {
                event_type: "task.completed".into(),
                project: project.clone(),
                source: "cli".into(),
                ts: events::iso_now(),
                level: Some("info".into()),
                title: Some("Task completed".into()),
                body: Some(task_id.clone()),
                meta: Some(serde_json::json!({ "task_id": task_id })),
                priority: None,
            });

            println!("Task {task_id} marked as completed");
            Ok(())
        }

        TaskCommand::Fail { task_id, reason } => {
            let (project, mut queue) = find_task_queue(&task_id)?;
            queue.fail(&task_id, reason.as_deref())?;
            queue.save(&project)?;

            events::try_emit_event(&Event {
                event_type: "task.failed".into(),
                project: project.clone(),
                source: "cli".into(),
                ts: events::iso_now(),
                level: Some("warning".into()),
                title: Some("Task failed".into()),
                body: Some(task_id.clone()),
                meta: Some(serde_json::json!({ "task_id": task_id })),
                priority: None,
            });

            println!("Task {task_id} marked as failed");
            Ok(())
        }

        TaskCommand::Cancel { task_id } => {
            let (project, mut queue) = find_task_queue(&task_id)?;
            queue.cancel(&task_id)?;
            queue.save(&project)?;
            println!("Task {task_id} cancelled");
            Ok(())
        }

        TaskCommand::Assign {
            task_id,
            agent,
            model,
            dry_run,
        } => {
            let (project_name, task) = find_task_queue(&task_id)
                .map(|(p, q)| {
                    let t = q.find(&task_id).cloned()
                        .ok_or_else(|| anyhow::anyhow!("Task {task_id} not found"));
                    (p, t)
                })
                .and_then(|(p, t)| t.map(|t| (p, t)))?;

            if task.status != TaskStatus::Queued {
                anyhow::bail!(
                    "Cannot assign task {}: status is {}, expected queued",
                    task_id,
                    task.status
                );
            }

            let plan = dispatch::prepare_dispatch(
                &project_name,
                &task,
                agent.as_deref(),
                model.as_deref(),
            )?;

            if dry_run {
                println!("=== Task Assign Dry Run ===\n");
                println!("Project: {project_name}");
                println!("Task: {} (priority {})", task.id, task.priority);
                println!("Description: {}", task.description);
                println!("Agent: {}", plan.agent_type);
                if let Some(ref m) = plan.model {
                    println!("Model: {m}");
                }
                println!("\n=== Prompt ===\n");
                println!("{}", plan.prompt);
                println!("\n=== Agent Command ===\n");
                println!("{}", plan.wrapped_cmd);
                return Ok(());
            }

            let log_path = dispatch::execute_dispatch(&plan)?;

            println!(
                "Assigned task {} to {} for project '{project_name}'",
                task.id, plan.agent_type
            );
            println!("  Log: {}", log_path.display());
            Ok(())
        }

        TaskCommand::Chain {
            project,
            descriptions,
            component,
            priority,
            agent,
        } => {
            if descriptions.is_empty() {
                anyhow::bail!("At least one task description is required");
            }

            let mut queue = TaskQueue::load(&project)?;
            let mut prev_id: Option<String> = None;
            let mut ids = Vec::new();

            for desc in &descriptions {
                let task = Task {
                    id: tasks::generate_task_id(),
                    project: project.clone(),
                    component: component.clone(),
                    description: desc.clone(),
                    priority,
                    status: TaskStatus::Queued,
                    assigned_agent: None,
                    agent_type: agent.clone(),
                    created_at: events::iso_now(),
                    started_at: None,
                    completed_at: None,
                    handoff_path: None,
                    parent_task: prev_id.clone(),
                    verification_passed: None,
                    failure_reason: None,
                };

                prev_id = Some(task.id.clone());
                ids.push(task.id.clone());

                events::try_emit_event(&Event {
                    event_type: "task.queued".into(),
                    project: project.clone(),
                    source: "cli".into(),
                    ts: events::iso_now(),
                    level: Some("info".into()),
                    title: Some("Task queued".into()),
                    body: Some(format!("{}: {}", task.id, truncate(desc, 60))),
                    meta: Some(serde_json::json!({
                        "task_id": &task.id,
                        "priority": priority,
                    })),
                    priority: None,
                });

                queue.add(task);
            }

            queue.save(&project)?;

            let chain_str = ids.join(" → ");
            println!("Created task chain: {chain_str}");
            for (i, (id, desc)) in ids.iter().zip(descriptions.iter()).enumerate() {
                let dep = if i == 0 {
                    String::new()
                } else {
                    format!(" (after {})", ids[i - 1])
                };
                println!("  {id}{dep}: {desc}");
            }
            Ok(())
        }

        TaskCommand::Worktrees { project } => {
            let project_config = config::load_project_config(&project)?;
            let repo_path = config::resolve_repo_path(&project_config.project.repo)?;
            let worktrees = worktree::list_task_worktrees(&repo_path)?;

            if worktrees.is_empty() {
                println!("No active worktrees for project '{project}'");
                return Ok(());
            }

            println!("{:<12} PATH", "TASK");
            for wt in &worktrees {
                println!("{:<12} {}", wt.task_id, wt.path.display());
            }
            Ok(())
        }

        TaskCommand::CleanWorktrees { project } => {
            let project_config = config::load_project_config(&project)?;
            let repo_path = config::resolve_repo_path(&project_config.project.repo)?;
            let worktrees = worktree::list_task_worktrees(&repo_path)?;

            if worktrees.is_empty() {
                println!("No worktrees to clean");
                return Ok(());
            }

            let queue = TaskQueue::load(&project)?;
            let mut removed = 0;

            for wt in &worktrees {
                let should_remove = match queue.find(&wt.task_id) {
                    Some(task) => matches!(task.status, TaskStatus::Completed | TaskStatus::Failed),
                    None => true, // Task not found, safe to clean up
                };

                if should_remove {
                    worktree::remove_task_worktree(&repo_path, &wt.task_id)?;
                    println!("Removed worktree for task {}", wt.task_id);
                    removed += 1;
                }
            }

            if removed == 0 {
                println!("No completed/failed worktrees to clean");
            } else {
                println!("Cleaned {removed} worktree(s)");
            }
            Ok(())
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

fn list_project_dirs() -> anyhow::Result<Vec<String>> {
    let base = paths::state_base_dir();
    let mut projects = Vec::new();
    if base.exists() {
        for entry in fs::read_dir(&base)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    let task_path = paths::task_queue_path(name);
                    if task_path.exists() {
                        projects.push(name.to_string());
                    }
                }
            }
        }
    }
    Ok(projects)
}

fn find_task_queue(task_id: &str) -> anyhow::Result<(String, TaskQueue)> {
    let base = paths::state_base_dir();
    if base.exists() {
        for entry in fs::read_dir(&base)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    let queue = TaskQueue::load(name)?;
                    if queue.find(task_id).is_some() {
                        return Ok((name.to_string(), queue));
                    }
                }
            }
        }
    }
    anyhow::bail!("Task {task_id} not found in any project")
}
