use drift_core::{niri, workspace};

pub fn run(name: &str) -> anyhow::Result<()> {
    if let Some(current) = detect_current_project() {
        match workspace::save_workspace(&current) {
            Ok(()) => {
                if let Ok(Some(snapshot)) = workspace::load_workspace_snapshot(&current) {
                    println!("Saved {} windows for '{current}'", snapshot.windows.len());
                }
            }
            Err(e) => eprintln!("Warning: could not save workspace for '{current}': {e}"),
        }
    }

    super::open::run(name)
}

fn detect_current_project() -> Option<String> {
    if let Ok(project) = std::env::var("DRIFT_PROJECT") {
        if !project.is_empty() {
            return Some(project);
        }
    }

    let mut client = niri::NiriClient::connect().ok()?;
    let win = client.focused_window().ok()??;
    let ws_id = win.workspace_id?;
    let workspaces = client.workspaces().ok()?;
    for ws in &workspaces {
        if ws.id == ws_id {
            return ws.name.clone();
        }
    }
    None
}
