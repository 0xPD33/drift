use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;

use anyhow::bail;
use drift_core::events::Event;
use drift_core::paths;

pub fn run(
    type_filter: Option<&str>,
    last: usize,
    all: bool,
    follow: bool,
    project: Option<&str>,
) -> anyhow::Result<()> {
    if follow {
        return follow_events(type_filter);
    }

    let project_name = if all {
        None
    } else {
        Some(resolve_project(project)?)
    };

    let state_path = paths::daemon_state_path();
    if !state_path.exists() {
        bail!("Daemon not running (no state file). Start it with `drift daemon`.");
    }

    let contents = std::fs::read_to_string(&state_path)?;
    let state: DaemonStateCompat = serde_json::from_str(&contents)?;

    let mut events: Vec<Event> = if let Some(ref name) = project_name {
        state
            .recent_events
            .get(name)
            .cloned()
            .unwrap_or_default()
    } else {
        state
            .recent_events
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect()
    };

    // Apply type filter
    if let Some(filter) = type_filter {
        events.retain(|e| matches_type_filter(&e.event_type, filter));
    }

    // Sort by timestamp and take last N
    events.sort_by(|a, b| a.ts.cmp(&b.ts));
    let start = events.len().saturating_sub(last);
    let events = &events[start..];

    if events.is_empty() {
        if let Some(name) = &project_name {
            println!("No events for project '{name}'.");
        } else {
            println!("No events.");
        }
        return Ok(());
    }

    for event in events {
        print_event(event);
    }

    Ok(())
}

fn follow_events(type_filter: Option<&str>) -> anyhow::Result<()> {
    let socket_path = paths::subscribe_socket_path();
    if !socket_path.exists() {
        bail!("Daemon not running (no subscribe socket). Start it with `drift daemon`.");
    }

    let stream = UnixStream::connect(&socket_path)?;
    let reader = BufReader::new(stream);

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<Event>(&line) {
            if let Some(filter) = type_filter {
                if !matches_type_filter(&event.event_type, filter) {
                    continue;
                }
            }
            print_event(&event);
        }
    }

    Ok(())
}

fn print_event(event: &Event) {
    // Extract time portion from ISO timestamp (HH:MM:SS)
    let time = if event.ts.len() >= 19 {
        &event.ts[11..19]
    } else {
        &event.ts
    };

    let title = event
        .title
        .as_deref()
        .unwrap_or("");

    let project = &event.project;
    let etype = &event.event_type;
    let source = &event.source;

    if title.is_empty() {
        println!("{time}  {etype:<25} {project:<12} {source}");
    } else {
        println!("{time}  {etype:<25} {project:<12} \"{title}\"");
    }
}

fn matches_type_filter(event_type: &str, filter: &str) -> bool {
    if filter.contains('*') {
        // Simple glob: "agent.*" matches "agent.completed", "agent.error", etc.
        let parts: Vec<&str> = filter.split('*').collect();
        if parts.len() == 2 {
            let (prefix, suffix) = (parts[0], parts[1]);
            event_type.starts_with(prefix) && event_type.ends_with(suffix)
        } else {
            event_type == filter
        }
    } else {
        event_type == filter
    }
}

fn resolve_project(name: Option<&str>) -> anyhow::Result<String> {
    if let Some(n) = name {
        return Ok(n.to_string());
    }
    if let Ok(project) = std::env::var("DRIFT_PROJECT") {
        if !project.is_empty() {
            return Ok(project);
        }
    }
    if let Ok(mut client) = drift_core::niri::NiriClient::connect() {
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
    bail!("Could not determine project name. Use --project or --all.")
}

/// Minimal struct to read daemon.json (only the fields we need)
#[derive(serde::Deserialize)]
struct DaemonStateCompat {
    #[serde(default)]
    recent_events: std::collections::HashMap<String, Vec<Event>>,
}
