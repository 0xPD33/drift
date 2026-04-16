use anyhow::bail;
use clap::Args;
use drift_core::{config, dispatch, worktree};
use drift_core::tasks::{self, TaskQueue};

#[derive(Args)]
pub struct DispatchArgs {
    /// Project to dispatch from (omit for cross-project --next)
    project: Option<String>,

    /// Dispatch a specific task by ID
    #[arg(long)]
    task: Option<String>,

    /// Override agent type
    #[arg(long)]
    agent: Option<String>,

    /// Override model
    #[arg(long)]
    model: Option<String>,

    /// Show what would be dispatched without launching
    #[arg(long)]
    dry_run: bool,

    /// Run agent in a git worktree (for parallel work)
    #[arg(long)]
    worktree: bool,

    /// Use next task from highest-priority project
    #[arg(long)]
    next: bool,
}

pub fn run(args: DispatchArgs) -> anyhow::Result<()> {
    // Step 1: Determine project + task
    let (project_name, task) = if args.next {
        match dispatch::find_next_cross_project()? {
            Some((name, task)) => (name, task),
            None => {
                println!("No tasks pending for dispatch across any project");
                return Ok(());
            }
        }
    } else if let Some(ref task_id) = args.task {
        // Scan all projects to find this task
        find_task_by_id(task_id)?
    } else {
        let project_name = args.project.as_deref()
            .ok_or_else(|| anyhow::anyhow!("Project name required (or use --next)"))?;
        let queue = TaskQueue::load(project_name)?;
        match queue.next() {
            Some(t) => (project_name.to_string(), t.clone()),
            None => {
                println!("No tasks pending for dispatch in project '{project_name}'");
                return Ok(());
            }
        }
    };

    let mut plan = dispatch::prepare_dispatch(
        &project_name,
        &task,
        args.agent.as_deref(),
        args.model.as_deref(),
    )?;

    // Determine if worktree mode should be used
    let use_worktree = args.worktree || {
        let project_config = config::load_project_config(&project_name)?;
        let dispatcher = project_config.dispatcher.as_ref();
        let max = dispatcher.map(|d| d.max_concurrent_agents).unwrap_or(1);
        if max > 1 {
            let queue = TaskQueue::load(&project_name)?;
            let running = queue.tasks.iter()
                .filter(|t| t.status == tasks::TaskStatus::Running)
                .count();
            running > 0
        } else {
            false
        }
    };

    if use_worktree {
        let wt_path = worktree::create_task_worktree(&plan.repo_path, &task.id)?;
        let wt_env = drift_core::env::worktree_env(&wt_path);
        plan.env_vars.extend(wt_env);
        plan.repo_path = wt_path;
    }

    if args.dry_run {
        println!("=== Dispatch Dry Run ===\n");
        println!("Project: {project_name}");
        println!("Task: {} (priority {})", task.id, task.priority);
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

    println!("Dispatched task {} to {} for project '{project_name}'", task.id, plan.agent_type);
    println!("  Log: {}", log_path.display());
    Ok(())
}

fn find_task_by_id(task_id: &str) -> anyhow::Result<(String, tasks::Task)> {
    let projects = drift_core::registry::list_projects()?;
    for proj in &projects {
        let queue = match TaskQueue::load(&proj.project.name) {
            Ok(q) => q,
            Err(_) => continue,
        };
        if let Some(task) = queue.find(task_id) {
            return Ok((proj.project.name.clone(), task.clone()));
        }
    }
    bail!("Task '{task_id}' not found in any project")
}
