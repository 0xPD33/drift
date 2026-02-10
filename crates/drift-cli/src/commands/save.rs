use anyhow::bail;
use drift_core::{niri, workspace};

pub fn run(name: Option<&str>) -> anyhow::Result<()> {
    let project_name = resolve_project_name(name)?;
    workspace::save_workspace(&project_name)?;

    if let Some(snapshot) = workspace::load_workspace_snapshot(&project_name)? {
        println!("Saved {} windows for '{project_name}'", snapshot.windows.len());
    }

    drift_core::events::try_emit_event(&drift_core::events::Event {
        event_type: "drift.save.completed".into(),
        project: project_name.clone(),
        source: "drift".into(),
        ts: drift_core::events::iso_now(),
        level: Some("info".into()),
        title: Some(format!("Saved workspace for '{project_name}'")),
        body: None,
        meta: None,
        priority: None,
    });

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

    bail!("Could not determine project name. Provide it as an argument, set $DRIFT_PROJECT, or run from a drift workspace.")
}
