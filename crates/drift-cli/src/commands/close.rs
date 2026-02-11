use std::fs;

use anyhow::bail;
use drift_core::{niri, paths};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

/// Stop supervisor, close windows, unset workspace name, clean up state.
/// Does NOT emit events or print summary — callers handle that.
pub fn close_project(project_name: &str) -> anyhow::Result<()> {
    let mut niri_client = niri::NiriClient::connect()?;

    // Stop supervisor (which stops all services)
    let supervisor_pid_path = paths::supervisor_pid_path(project_name);
    if supervisor_pid_path.exists() {
        if let Ok(pid_str) = fs::read_to_string(&supervisor_pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                if signal::kill(Pid::from_raw(pid), None).is_ok() {
                    let _ = signal::kill(Pid::from_raw(pid), Signal::SIGTERM);

                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    while std::time::Instant::now() < deadline {
                        if signal::kill(Pid::from_raw(pid), None).is_err() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }

                    if signal::kill(Pid::from_raw(pid), None).is_ok() {
                        let _ = signal::kill(Pid::from_raw(pid), Signal::SIGKILL);
                    }

                    println!("  Stopped supervisor (PID {pid})");
                } else {
                    println!("  Supervisor already stopped");
                }
            }
        }
        let _ = fs::remove_file(&supervisor_pid_path);
    }

    // Clean up state file
    let _ = fs::remove_file(paths::services_state_path(project_name));

    // Close all windows on the workspace
    if let Some(ws) = niri_client.find_workspace_by_name(project_name)? {
        let ws_id = ws.id;
        let windows = niri_client.windows()?;
        for win in &windows {
            if win.workspace_id == Some(ws_id) {
                niri_client.close_window(win.id)?;
            }
        }
    }

    // Unset workspace name so it becomes dynamic and gets auto-removed
    let _ = niri_client.unset_workspace_name(project_name);

    Ok(())
}

pub fn run(name: Option<&str>) -> anyhow::Result<()> {
    let project_name = resolve_project_name(name)?;

    close_project(&project_name)?;

    drift_core::events::try_emit_event(&drift_core::events::Event {
        event_type: "drift.project.closed".into(),
        project: project_name.clone(),
        source: "drift".into(),
        ts: drift_core::events::iso_now(),
        level: Some("info".into()),
        title: Some(format!("Closed project '{project_name}'")),
        body: None,
        meta: None,
        priority: None,
    });

    println!("Closed project '{project_name}'");
    Ok(())
}

fn resolve_project_name(name: Option<&str>) -> anyhow::Result<String> {
    if let Some(n) = name {
        return Ok(n.to_string());
    }

    if let Ok(project) = std::env::var("DRIFT_PROJECT") {
        if !project.is_empty() {
            return Ok(project);
        }
    }

    // Try to detect from focused workspace
    if let Ok(mut client) = niri::NiriClient::connect() {
        if let Ok(Some(win)) = client.focused_window() {
            if let Some(ws_id) = win.workspace_id {
                if let Ok(workspaces) = client.workspaces() {
                    for ws in &workspaces {
                        if ws.id == ws_id {
                            if let Some(ws_name) = &ws.name {
                                return Ok(ws_name.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    bail!("Could not determine project name. Provide it as an argument, set $DRIFT_PROJECT, or run from a drift workspace.")
}
