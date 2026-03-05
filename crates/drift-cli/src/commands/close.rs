use std::fs;

use anyhow::bail;
use drift_core::{niri, paths};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

/// Stop supervisor, close windows, unset workspace name, clean up state.
/// Does NOT emit events or print summary — callers handle that.
pub fn close_project(project_name: &str) -> anyhow::Result<()> {
    // Read supervisor PID before teardown (teardown removes the PID file)
    let supervisor_pid = read_supervisor_pid(project_name);

    // Non-blocking teardown: save workspace, kill tmux, SIGTERM supervisor,
    // remove state files, remove from session
    drift_core::lifecycle::teardown_project(project_name);

    // Wait for supervisor to actually die (blocking)
    if let Some(pid) = supervisor_pid {
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
    }

    // Close all windows on the workspace
    let mut niri_client = niri::NiriClient::connect()?;
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

fn read_supervisor_pid(project_name: &str) -> Option<i32> {
    let pid_path = paths::supervisor_pid_path(project_name);
    let pid_str = fs::read_to_string(&pid_path).ok()?;
    let pid: i32 = pid_str.trim().parse().ok()?;
    // Check if the process is actually alive
    if signal::kill(Pid::from_raw(pid), None).is_ok() {
        Some(pid)
    } else {
        None
    }
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
