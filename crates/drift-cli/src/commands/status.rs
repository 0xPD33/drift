use std::fs;

use drift_core::{config, niri, paths};
use nix::sys::signal;
use nix::unistd::Pid;

pub fn run() -> anyhow::Result<()> {
    show_daemon_status();

    let project_name = detect_project();

    let Some(project_name) = project_name else {
        println!("No active drift project detected.");
        println!("Set $DRIFT_PROJECT or run from a drift workspace.");
        return Ok(());
    };

    let project = config::load_project_config(&project_name)?;
    let mut niri_client = niri::NiriClient::connect()?;

    // Project info
    let repo_path = config::resolve_repo_path(&project.project.repo);
    println!("Project: {project_name}");
    println!("  Repo: {}", repo_path.display());
    if let Some(folder) = &project.project.folder {
        println!("  Folder: {folder}");
    }

    // Workspace status
    let ws = niri_client.find_workspace_by_name(&project_name)?;
    match &ws {
        Some(ws) => {
            let focused = if ws.is_focused { " (focused)" } else { "" };
            println!("  Workspace: active{focused}");
        }
        None => {
            println!("  Workspace: not open");
        }
    }

    // Services status from state file
    let state_path = paths::services_state_path(&project_name);
    if state_path.exists() {
        match fs::read_to_string(&state_path) {
            Ok(json) => {
                match serde_json::from_str::<drift_core::supervisor::ServicesState>(&json) {
                    Ok(state) => {
                        let sup_alive = signal::kill(
                            Pid::from_raw(state.supervisor_pid as i32),
                            None,
                        ).is_ok();
                        println!(
                            "  Supervisor: {} (PID {})",
                            if sup_alive { "running" } else { "stopped" },
                            state.supervisor_pid
                        );

                        if !state.services.is_empty() {
                            println!("  Services:");
                            for svc in &state.services {
                                let pid_info = svc.pid
                                    .map(|p| format!(" (PID {p})"))
                                    .unwrap_or_default();
                                let restart_info = if svc.restart_count > 0 {
                                    format!(" [{} restarts]", svc.restart_count)
                                } else {
                                    String::new()
                                };
                                let status_str = match svc.status {
                                    drift_core::supervisor::ServiceStatus::Running => "running",
                                    drift_core::supervisor::ServiceStatus::Stopped => "stopped",
                                    drift_core::supervisor::ServiceStatus::Failed => "failed",
                                    drift_core::supervisor::ServiceStatus::Backoff => "restarting",
                                };
                                println!("    {}: {status_str}{pid_info}{restart_info}", svc.name);
                            }
                        }
                    }
                    Err(_) => println!("  Services: (corrupt state file)"),
                }
            }
            Err(_) => println!("  Services: (unreadable state file)"),
        }
    } else if let Some(services) = &project.services {
        // Fallback: no state file, check PID files directly
        if !services.processes.is_empty() {
            println!("  Services:");
            for service in &services.processes {
                let pid_path = paths::pid_file(&project_name, &service.name);
                let status = if pid_path.exists() {
                    match fs::read_to_string(&pid_path) {
                        Ok(pid_str) => match pid_str.trim().parse::<i32>() {
                            Ok(pid) => {
                                if signal::kill(Pid::from_raw(pid), None).is_ok() {
                                    format!("running (PID {pid})")
                                } else {
                                    "stopped (stale PID)".into()
                                }
                            }
                            Err(_) => "stopped (bad PID file)".into(),
                        },
                        Err(_) => "stopped (unreadable PID file)".into(),
                    }
                } else {
                    "stopped".into()
                };
                println!("    {}: {status}", service.name);
            }
        }
    }

    // Windows on the workspace
    if let Some(ws) = &ws {
        let ws_id = ws.id;
        let windows = niri_client.windows()?;
        let ws_windows: Vec<_> = windows
            .iter()
            .filter(|w| w.workspace_id == Some(ws_id))
            .collect();
        println!("  Windows: {}", ws_windows.len());
        for win in &ws_windows {
            let app_id = win.app_id.as_deref().unwrap_or("unknown");
            let title = win.title.as_deref().unwrap_or("");
            println!("    {app_id}: {title}");
        }
    }

    Ok(())
}

fn show_daemon_status() {
    let daemon_state_path = paths::daemon_state_path();
    if daemon_state_path.exists() {
        if let Ok(json) = fs::read_to_string(&daemon_state_path) {
            if let Ok(state) =
                serde_json::from_str::<drift_daemon::state::DaemonState>(&json)
            {
                let alive =
                    signal::kill(Pid::from_raw(state.pid as i32), None).is_ok();
                println!(
                    "Daemon: {} (PID {})",
                    if alive { "running" } else { "stopped" },
                    state.pid
                );
                if let Some(active) = &state.active_project {
                    println!("  Active project: {active}");
                }
                if !state.workspace_projects.is_empty() {
                    println!(
                        "  Tracked workspaces: {}",
                        state.workspace_projects.len()
                    );
                }
                println!();
            }
        }
    }
}

fn detect_project() -> Option<String> {
    if let Ok(project) = std::env::var("DRIFT_PROJECT") {
        if !project.is_empty() {
            return Some(project);
        }
    }

    // Try to detect from focused workspace
    let mut client = niri::NiriClient::connect().ok()?;
    let win = client.focused_window().ok()??;
    let ws_id = win.workspace_id?;
    let workspaces = client.workspaces().ok()?;
    workspaces
        .into_iter()
        .find(|ws| ws.id == ws_id)
        .and_then(|ws| ws.name)
}
