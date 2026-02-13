use std::collections::BTreeMap;
use std::fs;

use drift_core::supervisor::{ServiceStatus, ServicesState};
use drift_core::{paths, registry};
use nix::sys::signal;
use nix::unistd::Pid;
use serde::Serialize;

#[derive(Serialize)]
struct ShellData {
    daemon_running: bool,
    active_project: Option<String>,
    folders: BTreeMap<String, Vec<ProjectData>>,
}

#[derive(Serialize)]
struct ProjectData {
    name: String,
    icon: Option<String>,
    is_active: bool,
    workspaces: Vec<WorkspaceData>,
    services: Vec<ServiceData>,
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

pub fn run() -> anyhow::Result<()> {
    let projects = registry::list_projects()?;

    // Read daemon state
    let daemon_state = read_daemon_state();
    let daemon_running = daemon_state
        .as_ref()
        .map(|s| signal::kill(Pid::from_raw(s.pid as i32), None).is_ok())
        .unwrap_or(false);
    let active_project = daemon_state
        .as_ref()
        .and_then(|s| s.active_project.clone());

    // Group projects by folder
    let mut folders: BTreeMap<String, Vec<ProjectData>> = BTreeMap::new();

    for project in &projects {
        let name = &project.project.name;
        let folder_key = project
            .project
            .folder
            .as_deref()
            .unwrap_or("_ungrouped")
            .to_string();

        let is_active = active_project.as_deref() == Some(name);

        // Workspaces from daemon state
        let workspaces: Vec<WorkspaceData> = daemon_state
            .as_ref()
            .map(|s| {
                s.workspace_projects
                    .iter()
                    .filter(|wp| &wp.project == name)
                    .map(|wp| WorkspaceData {
                        id: wp.workspace_id,
                        name: wp.workspace_name.clone(),
                        is_focused: wp.is_focused,
                        window_count: wp.window_count,
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Services from services.json
        let services = read_services(name);

        folders.entry(folder_key).or_default().push(ProjectData {
            name: name.clone(),
            icon: project.project.icon.clone(),
            is_active,
            workspaces,
            services,
        });
    }

    let data = ShellData {
        daemon_running,
        active_project,
        folders,
    };

    println!("{}", serde_json::to_string(&data)?);
    Ok(())
}

fn read_daemon_state() -> Option<drift_daemon::state::DaemonState> {
    let path = paths::daemon_state_path();
    let json = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&json).ok()
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
