use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;

use drift_core::niri::NiriClient;
use drift_core::supervisor::{ServiceStatus, ServicesState};
#[cfg(feature = "dispatch")]
use drift_core::tasks::{TaskQueue, TaskStatus};
use drift_core::{config, paths, project_state, registry};
use nix::sys::signal;
use nix::unistd::Pid;
use serde::Serialize;

#[derive(Serialize)]
struct FocusInfo {
    mode: String,
    active_project: Option<String>,
    niri_workspace_id: Option<u64>,
}

#[derive(Serialize)]
struct ShellData {
    daemon_running: bool,
    active_project: Option<String>,
    focus: FocusInfo,
    workspaces: Vec<WorkspaceInfo>,
    folders: BTreeMap<String, Vec<ProjectData>>,
    #[cfg(feature = "dispatch")]
    review_queue: Vec<ReviewItem>,
    global: GlobalSummary,
}

#[derive(Serialize)]
struct WorkspaceInfo {
    id: u64,
    idx: u8,
    name: Option<String>,
    is_focused: bool,
    window_count: u32,
    project: Option<WorkspaceProjectInfo>,
}

#[derive(Serialize)]
struct WorkspaceProjectInfo {
    name: String,
    icon: Option<String>,
    folder: Option<String>,
    services: Vec<ServiceData>,
    #[cfg(feature = "dispatch")]
    tasks: Option<TaskSummary>,
}

#[derive(Serialize)]
struct ProjectData {
    name: String,
    icon: Option<String>,
    is_active: bool,
    workspaces: Vec<WorkspaceData>,
    services: Vec<ServiceData>,
    project_state: Option<ProjectStateData>,
    #[cfg(feature = "dispatch")]
    tasks: Option<TaskSummary>,
}

#[derive(Serialize)]
struct ProjectStateData {
    status: String,
    priority: u8,
    last_agent_action: Option<String>,
    blocked_by: Option<String>,
    component_counts: HashMap<String, usize>,
}

#[derive(Serialize)]
struct WorkspaceData {
    id: u64,
    name: String,
    is_focused: bool,
    window_count: u32,
}

#[derive(Serialize)]
struct ServiceData {
    name: String,
    status: String,
    is_agent: bool,
}

#[cfg(feature = "dispatch")]
#[derive(Serialize)]
struct TaskSummary {
    queued: usize,
    running: usize,
    needs_review: usize,
    completed: usize,
    failed: usize,
    running_tasks: Vec<RunningTaskInfo>,
}

#[cfg(feature = "dispatch")]
#[derive(Serialize)]
struct RunningTaskInfo {
    id: String,
    description: String,
    agent: Option<String>,
    started_at: Option<String>,
}

#[cfg(feature = "dispatch")]
#[derive(Serialize)]
struct ReviewItem {
    task_id: String,
    project: String,
    description: String,
    agent: Option<String>,
}

#[derive(Serialize)]
struct GlobalSummary {
    #[cfg(feature = "dispatch")]
    total_agents_running: usize,
    #[cfg(feature = "dispatch")]
    total_tasks_queued: usize,
    #[cfg(feature = "dispatch")]
    total_reviews_pending: usize,
}

pub fn run() -> anyhow::Result<()> {
    let projects = registry::list_projects()?;
    let known_project_names: HashSet<String> = projects
        .iter()
        .map(|p| p.project.name.clone())
        .collect();

    // Daemon is optional — only used for recent_events buffer
    let daemon_state = read_daemon_state();
    let daemon_running = daemon_state
        .as_ref()
        .map(|s| signal::kill(Pid::from_raw(s.pid as i32), None).is_ok())
        .unwrap_or(false);

    // Query niri directly for the authoritative workspace + window state
    let (mut niri_workspaces, niri_windows) = query_niri();
    niri_workspaces.sort_by_key(|ws| ws.idx);

    // Compute window count per workspace
    let mut window_counts: HashMap<u64, u32> = HashMap::new();
    for w in &niri_windows {
        if let Some(ws_id) = w.workspace_id {
            *window_counts.entry(ws_id).or_default() += 1;
        }
    }

    // Determine which workspace is focused, derive active project
    let focused_ws_id = niri_workspaces
        .iter()
        .find(|ws| ws.is_focused)
        .map(|ws| ws.id);
    let active_project = focused_ws_id
        .and_then(|id| niri_workspaces.iter().find(|ws| ws.id == id))
        .and_then(|ws| ws.name.as_ref())
        .and_then(|name| project_name_from_workspace(name, &known_project_names));

    let focus = FocusInfo {
        mode: if active_project.is_some() {
            "workspace".into()
        } else {
            "overview".into()
        },
        active_project: active_project.clone(),
        niri_workspace_id: focused_ws_id,
    };

    // Build workspace list directly from niri (the authoritative source)
    let workspaces: Vec<WorkspaceInfo> = niri_workspaces
        .iter()
        .map(|ws| {
            let window_count = window_counts.get(&ws.id).copied().unwrap_or(0);
            // Match workspace to a drift project by name
            let project_name = ws
                .name
                .as_ref()
                .and_then(|name| project_name_from_workspace(name, &known_project_names));

            let project_info = project_name.as_ref().map(|proj_name| {
                let (icon, folder) = config::load_project_config(proj_name)
                    .ok()
                    .map(|cfg| (cfg.project.icon, cfg.project.folder))
                    .unwrap_or((None, None));
                let services = read_services(proj_name);
                #[cfg(feature = "dispatch")]
                let tasks = build_task_summary(proj_name);
                WorkspaceProjectInfo {
                    name: proj_name.clone(),
                    icon,
                    folder,
                    services,
                    #[cfg(feature = "dispatch")]
                    tasks,
                }
            });
            WorkspaceInfo {
                id: ws.id,
                idx: ws.idx,
                name: ws.name.clone(),
                is_focused: ws.is_focused,
                window_count,
                project: project_info,
            }
        })
        .collect();

    // Group projects by folder
    let mut folders: BTreeMap<String, Vec<ProjectData>> = BTreeMap::new();
    #[cfg(feature = "dispatch")]
    let mut review_queue: Vec<ReviewItem> = Vec::new();
    #[cfg(feature = "dispatch")]
    let mut total_agents_running: usize = 0;
    #[cfg(feature = "dispatch")]
    let mut total_tasks_queued: usize = 0;
    #[cfg(feature = "dispatch")]
    let mut total_reviews_pending: usize = 0;

    for project in &projects {
        let name = &project.project.name;
        let folder_key = project
            .project
            .folder
            .as_deref()
            .unwrap_or("_ungrouped")
            .to_string();

        let is_active = active_project.as_deref() == Some(name);

        // Workspaces matching this project (from niri)
        let workspaces: Vec<WorkspaceData> = niri_workspaces
            .iter()
            .filter(|ws| {
                ws.name
                    .as_ref()
                    .and_then(|n| project_name_from_workspace(n, &known_project_names))
                    .as_deref()
                    == Some(name.as_str())
            })
            .map(|ws| WorkspaceData {
                id: ws.id,
                name: ws.name.clone().unwrap_or_default(),
                is_focused: ws.is_focused,
                window_count: window_counts.get(&ws.id).copied().unwrap_or(0),
            })
            .collect();

        // Services from services.json
        let services = read_services(name);

        // Project state from PROJECT.md
        let ps = config::resolve_repo_path(&project.project.repo)
            .ok()
            .and_then(|repo| project_state::read_project_state(&repo))
            .map(|state| {
                let mut component_counts: HashMap<String, usize> = HashMap::new();
                for comp in &state.components {
                    *component_counts.entry(comp.status.clone()).or_default() += 1;
                }
                ProjectStateData {
                    status: state.status,
                    priority: state.priority.unwrap_or(3),
                    last_agent_action: state.last_agent_action,
                    blocked_by: state.blocked_by,
                    component_counts,
                }
            });

        // Task queue
        #[cfg(feature = "dispatch")]
        let task_summary = build_task_summary(name);

        // Collect review items and global totals
        #[cfg(feature = "dispatch")]
        if let Ok(queue) = TaskQueue::load(name) {
            for task in &queue.tasks {
                match task.status {
                    TaskStatus::Running => total_agents_running += 1,
                    TaskStatus::Queued => total_tasks_queued += 1,
                    TaskStatus::NeedsReview => {
                        total_reviews_pending += 1;
                        review_queue.push(ReviewItem {
                            task_id: task.id.clone(),
                            project: name.clone(),
                            description: truncate(&task.description, 80),
                            agent: task.assigned_agent.clone(),
                        });
                    }
                    _ => {}
                }
            }
        }

        folders.entry(folder_key).or_default().push(ProjectData {
            name: name.clone(),
            icon: project.project.icon.clone(),
            is_active,
            workspaces,
            services,
            project_state: ps,
            #[cfg(feature = "dispatch")]
            tasks: task_summary,
        });
    }

    let data = ShellData {
        daemon_running,
        active_project,
        focus,
        workspaces,
        folders,
        #[cfg(feature = "dispatch")]
        review_queue,
        global: GlobalSummary {
            #[cfg(feature = "dispatch")]
            total_agents_running,
            #[cfg(feature = "dispatch")]
            total_tasks_queued,
            #[cfg(feature = "dispatch")]
            total_reviews_pending,
        },
    };

    println!("{}", serde_json::to_string(&data)?);
    Ok(())
}

fn read_daemon_state() -> Option<drift_daemon::state::DaemonState> {
    let path = paths::daemon_state_path();
    let json = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&json).ok()
}

/// Query niri directly for workspaces and windows. Returns empty vecs if niri isn't available.
fn query_niri() -> (Vec<niri_ipc::Workspace>, Vec<niri_ipc::Window>) {
    let mut client = match NiriClient::connect() {
        Ok(c) => c,
        Err(_) => return (Vec::new(), Vec::new()),
    };
    let workspaces = client.workspaces().unwrap_or_default();
    let windows = client.windows().unwrap_or_default();
    (workspaces, windows)
}

/// Try to derive a drift project name from a niri workspace name.
/// Workspace names may be the bare project name ("scratch") or a formatted
/// dynamic name ("scratch · idle · 2 queued"). Match the prefix against known projects.
fn project_name_from_workspace(ws_name: &str, known: &HashSet<String>) -> Option<String> {
    let trimmed = ws_name.trim();
    // Exact match
    if known.contains(trimmed) {
        return Some(trimmed.to_string());
    }
    // Prefix match: "project · ..."
    if let Some(idx) = trimmed.find(" \u{00b7} ") {
        let prefix = &trimmed[..idx];
        if known.contains(prefix) {
            return Some(prefix.to_string());
        }
    }
    None
}

#[cfg(feature = "dispatch")]
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max.min(s.len());
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

#[cfg(feature = "dispatch")]
fn build_task_summary(project: &str) -> Option<TaskSummary> {
    let queue = TaskQueue::load(project).ok()?;
    let mut queued = 0;
    let mut running = 0;
    let mut needs_review = 0;
    let mut completed = 0;
    let mut failed = 0;
    let mut running_tasks = Vec::new();

    for task in &queue.tasks {
        match task.status {
            TaskStatus::Queued => queued += 1,
            TaskStatus::Running => {
                running += 1;
                running_tasks.push(RunningTaskInfo {
                    id: task.id.clone(),
                    description: truncate(&task.description, 80),
                    agent: task.assigned_agent.clone(),
                    started_at: task.started_at.clone(),
                });
            }
            TaskStatus::Completed => completed += 1,
            TaskStatus::Failed => failed += 1,
            TaskStatus::NeedsReview => needs_review += 1,
        }
    }

    Some(TaskSummary {
        queued,
        running,
        needs_review,
        completed,
        failed,
        running_tasks,
    })
}

fn read_services(project: &str) -> Vec<ServiceData> {
    let path = paths::services_state_path(project);
    let json = match fs::read_to_string(&path) {
        Ok(j) => j,
        Err(_) => return Vec::new(),
    };
    let state: ServicesState = match serde_json::from_str(&json) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    state
        .services
        .iter()
        .map(|svc| ServiceData {
            name: svc.name.clone(),
            status: match svc.status {
                ServiceStatus::Running => "running".into(),
                ServiceStatus::Stopped => "stopped".into(),
                ServiceStatus::Failed => "failed".into(),
                ServiceStatus::Backoff => "backoff".into(),
            },
            is_agent: svc.is_agent,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn project_name_exact_match() {
        let projects = known(&["scratch", "drift", "webapp"]);
        assert_eq!(
            project_name_from_workspace("scratch", &projects),
            Some("scratch".into())
        );
    }

    #[test]
    fn project_name_with_dynamic_suffix() {
        let projects = known(&["scratch", "drift"]);
        assert_eq!(
            project_name_from_workspace("scratch · idle", &projects),
            Some("scratch".into())
        );
        assert_eq!(
            project_name_from_workspace("scratch · 2 queued", &projects),
            Some("scratch".into())
        );
        assert_eq!(
            project_name_from_workspace("drift · claude-code ▸ fix bug", &projects),
            Some("drift".into())
        );
    }

    #[test]
    fn project_name_unknown_returns_none() {
        let projects = known(&["scratch", "drift"]);
        assert_eq!(project_name_from_workspace("random-workspace", &projects), None);
        assert_eq!(project_name_from_workspace("1", &projects), None);
        assert_eq!(project_name_from_workspace("", &projects), None);
    }

    #[test]
    fn project_name_partial_prefix_no_match() {
        // "scratchpad" should NOT match "scratch" — the separator is required
        let projects = known(&["scratch"]);
        assert_eq!(project_name_from_workspace("scratchpad", &projects), None);
        // No middle dot separator → no prefix match
        assert_eq!(project_name_from_workspace("scratch idle", &projects), None);
    }

    #[test]
    fn project_name_with_leading_whitespace() {
        let projects = known(&["scratch"]);
        assert_eq!(
            project_name_from_workspace("  scratch  ", &projects),
            Some("scratch".into())
        );
    }

    #[test]
    fn project_name_empty_known_set() {
        let projects = HashSet::new();
        assert_eq!(project_name_from_workspace("scratch", &projects), None);
        assert_eq!(project_name_from_workspace("scratch · idle", &projects), None);
    }

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string_appends_ellipsis() {
        assert_eq!(truncate("hello world", 5), "hello...");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        // Multi-byte char shouldn't be split
        let result = truncate("héllo world", 4);
        assert!(result.ends_with("..."));
        assert!(result.is_char_boundary(result.len() - 3));
    }
}
