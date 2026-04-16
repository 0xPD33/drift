use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::{ProjectConfig, RestartPolicy, ServiceProcess};
use crate::handoff::Handoff;
use crate::project_state::ProjectState;
use crate::registry;
use crate::tasks::{Task, TaskQueue, TaskStatus};
use crate::{agent, config, env, handoff, paths, project_state};

/// Indicates the source of a previous handoff for prompt construction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HandoffSource {
    /// Handoff from a parent task in a chain
    Parent,
    /// Handoff from previous work on the same component
    Component,
    /// No previous handoff
    None,
}

/// Build the full prompt to send to an agent for a dispatched task.
pub fn build_dispatch_prompt(
    project_state: Option<&(ProjectState, String)>,
    previous_handoff: Option<&(Handoff, String)>,
    handoff_source: HandoffSource,
    task: &Task,
    constraints: &[String],
    handoff_path: &Path,
) -> String {
    let mut sections = Vec::new();

    if let Some((state, body)) = project_state {
        let yaml = serde_yaml::to_string(state).unwrap_or_default();
        let mut section = format!("# Project State\n\n---\n{yaml}---\n");
        if !body.is_empty() {
            section.push_str(body);
        }
        sections.push(section);
    }

    if let Some((_handoff, body)) = previous_handoff {
        let header = match handoff_source {
            HandoffSource::Parent => "# Previous Work (from parent task)\n\n",
            HandoffSource::Component => "# Previous Work (from previous work on this component)\n\n",
            HandoffSource::None => "# Previous Work\n\n",
        };
        let mut section = String::from(header);
        section.push_str(&extract_handoff_sections(body));
        sections.push(section);
    }

    sections.push(format!("# Your Task\n\n{}", task.description));

    if !constraints.is_empty() {
        let mut section = String::from("# Constraints\n\n");
        for c in constraints {
            section.push_str(&format!("- {c}\n"));
        }
        sections.push(section);
    }

    sections.push(format!(
        "# Handoff Instructions\n\n{}",
        crate::handoff::handoff_template(&task.id, handoff_path)
    ));

    sections.join("\n\n")
}

/// Extract "What was done" and "Next steps" sections from a handoff body.
fn extract_handoff_sections(body: &str) -> String {
    let mut result = String::new();
    let mut capturing = false;

    for line in body.lines() {
        if line.starts_with("## What was done") || line.starts_with("## Next steps") {
            capturing = true;
            result.push_str(line);
            result.push('\n');
        } else if line.starts_with("## ") {
            capturing = false;
        } else if capturing {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

/// Select which agent type to use for a task.
pub fn select_agent_type(task: &Task, project_config: &ProjectConfig) -> String {
    if let Some(ref at) = task.agent_type {
        return at.clone();
    }
    if let Some(ref d) = project_config.dispatcher {
        if let Some(ref pa) = d.preferred_agent {
            return pa.clone();
        }
    }
    "claude".into()
}

/// Select which model to use.
pub fn select_model(task: &Task, project_config: &ProjectConfig) -> Option<String> {
    // Task doesn't have a model field, so check project config
    let _ = task;
    project_config
        .dispatcher
        .as_ref()
        .and_then(|d| d.preferred_model.clone())
}

/// Everything needed to dispatch a task to an agent.
pub struct DispatchPlan {
    pub project_name: String,
    pub task: Task,
    pub agent_type: String,
    pub model: Option<String>,
    pub prompt: String,
    pub handoff_path: PathBuf,
    pub repo_path: PathBuf,
    pub wrapped_cmd: String,
    pub env_vars: HashMap<String, String>,
}

/// Prepare everything needed to dispatch a task without actually spawning it.
pub fn prepare_dispatch(
    project_name: &str,
    task: &Task,
    agent_override: Option<&str>,
    model_override: Option<&str>,
) -> anyhow::Result<DispatchPlan> {
    let project_config = config::load_project_config(project_name)?;
    let repo_path = config::resolve_repo_path(&project_config.project.repo)?;

    let project_state = project_state::read_project_state_full(&repo_path);

    let handoff_file_path = paths::handoff_path(project_name, &task.id);
    let previous_handoff = if let Some(ref parent_id) = task.parent_task {
        let parent_handoff_file = paths::handoff_path(project_name, parent_id);
        handoff::read_handoff(&parent_handoff_file).ok()
    } else if let Some(ref component) = task.component {
        let queue = TaskQueue::load(project_name)?;
        let completed_for_component = queue
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed)
            .filter(|t| t.component.as_deref() == Some(component))
            .filter(|t| t.completed_at.is_some())
            .max_by_key(|t| t.completed_at.clone().unwrap());

        if let Some(prev_task) = completed_for_component {
            let prev_handoff_file = paths::handoff_path(project_name, &prev_task.id);
            handoff::read_handoff(&prev_handoff_file).ok()
        } else {
            None
        }
    } else {
        None
    };

    let handoff_source = if task.parent_task.is_some() {
        HandoffSource::Parent
    } else if task.component.is_some() && previous_handoff.is_some() {
        HandoffSource::Component
    } else {
        HandoffSource::None
    };

    let agent_type = agent_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| select_agent_type(task, &project_config));
    let model = model_override
        .map(|s| s.to_string())
        .or_else(|| select_model(task, &project_config));

    let constraints = project_state
        .as_ref()
        .map(|(s, _)| s.constraints.clone())
        .unwrap_or_default();

    let dispatch_prompt = build_dispatch_prompt(
        project_state.as_ref(),
        previous_handoff.as_ref(),
        handoff_source,
        task,
        &constraints,
        &handoff_file_path,
    );

    let svc = ServiceProcess {
        name: format!("dispatch-{}", task.id),
        command: String::new(),
        cwd: ".".into(),
        restart: RestartPolicy::Never,
        stop_command: None,
        agent: Some(agent_type.clone()),
        prompt: Some(dispatch_prompt.clone()),
        agent_mode: "oneshot".into(),
        agent_model: model.clone(),
        agent_permissions: "full".into(),
        width: None,
    };

    let agent_cmd = agent::build_agent_command(&svc, project_name);
    let wrapped_cmd = format!(
        "{}; drift _post-dispatch '{}' '{}'",
        agent_cmd,
        project_name.replace('\'', "'\\''"),
        task.id.replace('\'', "'\\''"),
    );

    let mut env_vars = env::build_env(&project_config)?;
    let task_env = env::dispatch_env(&task.id, &handoff_file_path);
    env_vars.extend(task_env);

    Ok(DispatchPlan {
        project_name: project_name.to_string(),
        task: task.clone(),
        agent_type,
        model,
        prompt: dispatch_prompt,
        handoff_path: handoff_file_path,
        repo_path,
        wrapped_cmd,
        env_vars,
    })
}

/// Spawn the agent process and update task status.
pub fn execute_dispatch(plan: &DispatchPlan) -> anyhow::Result<PathBuf> {
    use std::fs;
    use std::process::Command;

    let handoff_dir = paths::handoff_dir(&plan.project_name);
    fs::create_dir_all(&handoff_dir)
        .map_err(|e| anyhow::anyhow!("creating handoff directory: {e}"))?;

    let logs_dir = paths::logs_dir(&plan.project_name);
    fs::create_dir_all(&logs_dir)
        .map_err(|e| anyhow::anyhow!("creating logs directory: {e}"))?;
    let log_path = logs_dir.join(format!("dispatch-{}.log", plan.task.id));
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| anyhow::anyhow!("creating dispatch log file: {e}"))?;
    let stderr_file = log_file.try_clone()?;

    let spawn_result = Command::new("sh")
        .arg("-c")
        .arg(&plan.wrapped_cmd)
        .envs(&plan.env_vars)
        .current_dir(&plan.repo_path)
        .stdout(log_file)
        .stderr(stderr_file)
        .stdin(std::process::Stdio::null())
        .spawn();

    match spawn_result {
        Ok(_child) => {
            let mut queue = TaskQueue::load(&plan.project_name)?;
            queue.start(&plan.task.id, &plan.agent_type)?;
            queue.save(&plan.project_name)?;
        }
        Err(e) => {
            anyhow::bail!("Failed to spawn agent: {e}");
        }
    }

    crate::events::try_emit_event(&crate::events::Event {
        event_type: "task.running".into(),
        project: plan.project_name.clone(),
        source: "dispatch".into(),
        ts: crate::events::iso_now(),
        level: Some("info".into()),
        title: Some(format!("Dispatched task {}", plan.task.id)),
        body: Some(plan.task.description.clone()),
        meta: Some(serde_json::json!({
            "task_id": plan.task.id,
            "agent_type": plan.agent_type,
        })),
        priority: None,
    });

    Ok(log_path)
}

/// Find the next dispatchable task across all projects (for --next flag).
pub fn find_next_cross_project() -> anyhow::Result<Option<(String, Task)>> {
    let projects = registry::list_projects()?;

    // Collect (priority, project_name, task) tuples for active projects with queued tasks
    let mut candidates: Vec<(u8, String, Task)> = Vec::new();

    for proj in &projects {
        let repo_path = match crate::config::resolve_repo_path(&proj.project.repo) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let state = match crate::project_state::read_project_state(&repo_path) {
            Some(s) => s,
            None => continue,
        };
        if state.status != "active" {
            continue;
        }
        let priority = state.priority.unwrap_or(255);

        let queue = match TaskQueue::load(&proj.project.name) {
            Ok(q) => q,
            Err(_) => continue,
        };
        if let Some(task) = queue.next() {
            candidates.push((priority, proj.project.name.clone(), task.clone()));
        }
    }

    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    Ok(candidates.into_iter().next().map(|(_, name, task)| (name, task)))
}
