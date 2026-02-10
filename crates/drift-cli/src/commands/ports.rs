use anyhow::bail;
use drift_core::{config, niri};

pub fn run(project: Option<&str>) -> anyhow::Result<()> {
    let project_name = resolve_project_name(project)?;
    let project_config = config::load_project_config(&project_name)?;

    let ports = match &project_config.ports {
        Some(p) => p,
        None => {
            println!("No ports configured for '{project_name}'");
            return Ok(());
        }
    };

    if let Some([start, end]) = ports.range {
        println!("Port range: {start}-{end}");
    }

    if !ports.named.is_empty() {
        let mut names: Vec<(&String, &u16)> = ports.named.iter().collect();
        names.sort_by_key(|(name, _)| name.to_string());
        for (name, port) in names {
            println!("  {name}: {port}");
        }
    }

    if ports.range.is_none() && ports.named.is_empty() {
        println!("No ports configured for '{project_name}'");
    }

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

    bail!("Could not determine project name. Use --project, set $DRIFT_PROJECT, or run from a drift workspace.")
}
