use std::collections::HashMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use drift_core::events::Event;
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

    // Project header: name (folder/)
    let folder_suffix = project
        .project
        .folder
        .as_deref()
        .map(|f| format!(" ({f}/)"))
        .unwrap_or_default();
    println!("{project_name}{folder_suffix}");

    // Repo
    let repo_path = config::resolve_repo_path(&project.project.repo)?;
    let repo_display = abbreviate_home(&repo_path.to_string_lossy());
    println!("  Repo: {repo_display}");

    // Workspace status with window count
    let ws = niri_client.find_workspace_by_name(&project_name)?;
    let window_count = if let Some(ref ws) = ws {
        let ws_id = ws.id;
        let windows = niri_client.windows()?;
        windows.iter().filter(|w| w.workspace_id == Some(ws_id)).count()
    } else {
        0
    };

    match &ws {
        Some(ws) => {
            let mut parts = Vec::new();
            if ws.is_focused {
                parts.push("focused".to_string());
            }
            parts.push(format!("{window_count} window{}", if window_count == 1 { "" } else { "s" }));
            println!("  Workspace: active ({})", parts.join(", "));
        }
        None => {
            println!("  Workspace: not open");
        }
    }

    // Services
    show_services(&project_name, &project);

    // Recent events
    show_recent_events(&project_name);

    // Ports
    show_ports(&project);

    Ok(())
}

fn show_services(project_name: &str, project: &config::ProjectConfig) {
    let state_path = paths::services_state_path(project_name);
    if state_path.exists() {
        match fs::read_to_string(&state_path) {
            Ok(json) => {
                match serde_json::from_str::<drift_core::supervisor::ServicesState>(&json) {
                    Ok(state) => {
                        if state.services.is_empty() {
                            return;
                        }
                        println!();
                        println!("  Services:");
                        let now_epoch = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        for svc in &state.services {
                            print_service_line(svc, now_epoch);
                        }
                    }
                    Err(_) => println!("  Services: (corrupt state file)"),
                }
            }
            Err(_) => println!("  Services: (unreadable state file)"),
        }
    } else if let Some(services) = &project.services {
        if !services.processes.is_empty() {
            println!();
            println!("  Services:");
            for service in &services.processes {
                let pid_path = paths::pid_file(project_name, &service.name);
                let status = if pid_path.exists() {
                    match fs::read_to_string(&pid_path) {
                        Ok(pid_str) => match pid_str.trim().parse::<i32>() {
                            Ok(pid) => {
                                if signal::kill(Pid::from_raw(pid), None).is_ok() {
                                    format!("running   PID {pid}")
                                } else {
                                    "stopped (stale PID)".into()
                                }
                            }
                            Err(_) => "stopped".into(),
                        },
                        Err(_) => "stopped".into(),
                    }
                } else {
                    "stopped".into()
                };
                println!("    {:<12} {status}", service.name);
            }
        }
    }
}

fn print_service_line(svc: &drift_core::supervisor::ServiceState, now_epoch: u64) {
    use drift_core::supervisor::ServiceStatus;

    let status_str = match svc.status {
        ServiceStatus::Running => "running",
        ServiceStatus::Stopped => "stopped",
        ServiceStatus::Failed => "failed",
        ServiceStatus::Backoff => "restarting",
    };

    let mut parts = vec![format!("    {:<12} {:<10}", svc.name, status_str)];

    // PID (only for running/restarting)
    if matches!(svc.status, ServiceStatus::Running | ServiceStatus::Backoff) {
        if let Some(pid) = svc.pid {
            parts.push(format!("PID {pid}"));
        }
    }

    // Uptime (only for running with started_at)
    if svc.status == ServiceStatus::Running {
        if let Some(ref started) = svc.started_at {
            if let Ok(epoch_secs) = started.parse::<u64>() {
                let elapsed = now_epoch.saturating_sub(epoch_secs);
                parts.push(format!("uptime {}", format_duration(elapsed)));
            }
        }
    }

    // Agent info
    if svc.is_agent {
        if let Some(ref agent_type) = svc.agent_type {
            parts.push(format!("agent:{agent_type}"));
        }
    }

    // Restart count
    if svc.restart_count > 0 {
        let label = if svc.restart_count == 1 { "restart" } else { "restarts" };
        parts.push(format!("[{} {label}]", svc.restart_count));
    }

    // Join: first part is already formatted with padding, rest separated by two spaces
    let line = if parts.len() == 1 {
        parts.remove(0)
    } else {
        let first = parts.remove(0);
        format!("{}  {}", first, parts.join("  "))
    };
    println!("{line}");
}

fn show_recent_events(project_name: &str) {
    let state_path = paths::daemon_state_path();
    if !state_path.exists() {
        return;
    }

    let Ok(contents) = fs::read_to_string(&state_path) else {
        return;
    };

    #[derive(serde::Deserialize)]
    struct DaemonStateCompat {
        #[serde(default)]
        recent_events: HashMap<String, Vec<Event>>,
    }

    let Ok(state) = serde_json::from_str::<DaemonStateCompat>(&contents) else {
        return;
    };

    let Some(events) = state.recent_events.get(project_name) else {
        return;
    };

    if events.is_empty() {
        return;
    }

    // Sort by timestamp, take last 5
    let mut sorted: Vec<&Event> = events.iter().collect();
    sorted.sort_by(|a, b| a.ts.cmp(&b.ts));
    let start = sorted.len().saturating_sub(5);
    let recent = &sorted[start..];

    println!();
    println!("  Recent events:");
    for event in recent {
        let time = if event.ts.len() >= 16 {
            &event.ts[11..16] // HH:MM
        } else {
            &event.ts
        };

        let title_part = event
            .title
            .as_deref()
            .map(|t| format!("  \"{t}\""))
            .unwrap_or_default();

        println!(
            "    {time}  {:<17} {}{title_part}",
            event.event_type, event.source
        );
    }
}

fn show_ports(project: &config::ProjectConfig) {
    let Some(ref ports) = project.ports else {
        return;
    };

    let has_range = ports.range.is_some();
    let has_named = !ports.named.is_empty();

    if !has_range && !has_named {
        return;
    }

    println!();
    let mut line = String::from("  Ports: ");

    if let Some([start, end]) = ports.range {
        line.push_str(&format!("{start}-{end}"));
    }

    if has_named {
        let mut named: Vec<_> = ports.named.iter().collect();
        named.sort_by_key(|(_, v)| *v);
        let named_str: Vec<String> = named.iter().map(|(k, v)| format!("{k}={v}")).collect();
        if has_range {
            line.push_str(&format!(" ({})", named_str.join(", ")));
        } else {
            line.push_str(&named_str.join(", "));
        }
    }

    println!("{line}");
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

/// Format seconds into human-readable duration: <1m, 5m, 2h13m, 1d4h
fn format_duration(secs: u64) -> String {
    let minutes = secs / 60;
    let hours = minutes / 60;
    let days = hours / 24;

    if days > 0 {
        let rem_hours = hours % 24;
        if rem_hours > 0 {
            format!("{days}d{rem_hours}h")
        } else {
            format!("{days}d")
        }
    } else if hours > 0 {
        let rem_minutes = minutes % 60;
        if rem_minutes > 0 {
            format!("{hours}h{rem_minutes:02}m")
        } else {
            format!("{hours}h")
        }
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        "<1m".into()
    }
}

/// Replace home directory prefix with ~
fn abbreviate_home(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        if let Some(rest) = path.strip_prefix(home_str.as_ref()) {
            return format!("~{rest}");
        }
    }
    path.to_string()
}
