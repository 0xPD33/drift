use anyhow::{bail, Context};
use drift_core::events::{iso_now, try_emit_event, Event};
use drift_core::handoff::read_handoff;
use drift_core::paths;
use drift_core::registry::list_projects;
use drift_core::tasks::{TaskQueue, TaskStatus};

#[derive(clap::Subcommand)]
pub enum ReviewCommand {
    /// List tasks needing review
    List {
        /// Filter by project
        project: Option<String>,
    },
    /// Show handoff details for a task
    Show {
        /// Task ID
        task_id: String,
    },
    /// Approve a reviewed task
    Approve {
        /// Task ID
        task_id: String,
    },
    /// Reject a reviewed task
    Reject {
        /// Task ID
        task_id: String,
        /// Rejection reason
        #[arg(long)]
        reason: Option<String>,
    },
}

pub fn run(cmd: ReviewCommand) -> anyhow::Result<()> {
    match cmd {
        ReviewCommand::List { project } => list(project.as_deref()),
        ReviewCommand::Show { task_id } => show(&task_id),
        ReviewCommand::Approve { task_id } => approve(&task_id),
        ReviewCommand::Reject { task_id, reason } => reject(&task_id, reason.as_deref()),
    }
}

fn list(project_filter: Option<&str>) -> anyhow::Result<()> {
    let projects = if let Some(name) = project_filter {
        vec![name.to_string()]
    } else {
        list_projects()?
            .iter()
            .map(|p| p.project.name.clone())
            .collect()
    };

    let mut found = false;
    for project in &projects {
        let queue = TaskQueue::load(project)?;
        let reviews = queue.pending_reviews();
        for task in reviews {
            if !found {
                println!("{:<14} {:<14} {:<12} DESCRIPTION", "TASK", "PROJECT", "AGENT");
                found = true;
            }
            let desc = if task.description.len() > 50 {
                format!("{}...", &task.description[..47])
            } else {
                task.description.clone()
            };
            let agent = task.assigned_agent.as_deref().unwrap_or("-");
            println!("{:<14} {:<14} {:<12} {}", task.id, project, agent, desc);
        }
    }

    if !found {
        println!("No tasks pending review.");
    }

    Ok(())
}

fn show(task_id: &str) -> anyhow::Result<()> {
    let (project, queue) = find_task_project(task_id)?;
    let task = queue.find(task_id).unwrap();

    let handoff_path = paths::handoff_path(&project, task_id);
    if !handoff_path.exists() {
        println!("Task: {} ({})", task_id, project);
        println!("Status: {}", task.status);
        println!("Description: {}", task.description);
        if let Some(ref reason) = task.failure_reason {
            println!("Failure reason: {reason}");
        }
        println!();
        println!("No handoff file found at {}", handoff_path.display());
        return Ok(());
    }

    let (handoff, body) = read_handoff(&handoff_path)
        .with_context(|| format!("reading handoff for task {task_id}"))?;

    println!("Task: {} ({})", handoff.task_id, project);
    println!("Status: {:?}", handoff.status);
    println!("Agent: {}", handoff.agent);
    if let Some(ref model) = handoff.model {
        println!("Model: {model}");
    }
    if let Some(ref started) = handoff.started_at {
        println!("Started: {started}");
    }
    if let Some(ref completed) = handoff.completed_at {
        println!("Completed: {completed}");
    }
    if !handoff.files_changed.is_empty() {
        println!("Files changed: {}", handoff.files_changed.len());
        for f in &handoff.files_changed {
            println!("  {f}");
        }
    }
    if let Some(run) = handoff.tests_run {
        let passed = handoff.tests_passed.unwrap_or(0);
        let failed = handoff.tests_failed.unwrap_or(0);
        println!("Tests: {run} run, {passed} passed, {failed} failed");
    }

    if !body.is_empty() {
        println!();
        println!("{body}");
    }

    Ok(())
}

fn approve(task_id: &str) -> anyhow::Result<()> {
    let (project, mut queue) = find_task_project(task_id)?;
    let task = queue.find(task_id)
        .with_context(|| format!("task {task_id} not found"))?;

    if task.status != TaskStatus::NeedsReview {
        bail!("Task {} is not pending review (status: {})", task_id, task.status);
    }

    queue.complete(task_id, None, None)?;
    queue.save(&project)?;

    try_emit_event(&Event {
        event_type: "task.completed".into(),
        project: project.clone(),
        source: "review".into(),
        ts: iso_now(),
        level: Some("success".into()),
        title: Some(format!("Task {task_id} approved")),
        body: None,
        meta: None,
        priority: None,
    });

    println!("Approved task {task_id} in project {project}");
    Ok(())
}

fn reject(task_id: &str, reason: Option<&str>) -> anyhow::Result<()> {
    let (project, mut queue) = find_task_project(task_id)?;
    let task = queue.find(task_id)
        .with_context(|| format!("task {task_id} not found"))?;

    if task.status != TaskStatus::NeedsReview {
        bail!("Task {} is not pending review (status: {})", task_id, task.status);
    }

    queue.fail(task_id, reason)?;
    queue.save(&project)?;

    try_emit_event(&Event {
        event_type: "task.failed".into(),
        project: project.clone(),
        source: "review".into(),
        ts: iso_now(),
        level: Some("error".into()),
        title: Some(format!("Task {task_id} rejected")),
        body: reason.map(|s| s.to_string()),
        meta: None,
        priority: None,
    });

    println!("Rejected task {task_id} in project {project}");
    if let Some(r) = reason {
        println!("Reason: {r}");
    }
    Ok(())
}

fn find_task_project(task_id: &str) -> anyhow::Result<(String, TaskQueue)> {
    let projects = list_projects()?;
    for project in &projects {
        let name = &project.project.name;
        let queue = TaskQueue::load(name)?;
        if queue.find(task_id).is_some() {
            return Ok((name.clone(), queue));
        }
    }
    bail!("Task {task_id} not found in any project")
}
