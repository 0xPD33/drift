use drift_core::{config, session, workspace};

pub fn run(name: Option<&str>) -> anyhow::Result<()> {
    match name {
        Some(name) => restore_single(name),
        None => restore_session(),
    }
}

fn restore_single(name: &str) -> anyhow::Result<()> {
    // Verify project config exists
    config::load_project_config(name)?;

    // Show snapshot info if available
    if let Ok(Some(snapshot)) = workspace::load_workspace_snapshot(name) {
        println!("Restoring '{name}' (last saved: {} windows)", snapshot.windows.len());
    } else {
        println!("Restoring '{name}' (no saved snapshot)");
    }

    super::open::run(name)
}

fn restore_session() -> anyhow::Result<()> {
    let session = match session::load_session()? {
        Some(s) => s,
        None => {
            println!("No session to restore");
            return Ok(());
        }
    };

    if session.projects.is_empty() {
        println!("No projects in session to restore");
        return Ok(());
    }

    println!("Restoring {} projects from session...", session.projects.len());

    let mut failures = Vec::new();
    for project in &session.projects {
        if let Err(e) = restore_single(project) {
            eprintln!("  Failed to restore '{project}': {e}");
            failures.push(project.clone());
        }
    }

    let restored = session.projects.len() - failures.len();
    if failures.is_empty() {
        println!("Restored all {} projects", restored);
    } else {
        println!("Restored {restored} projects, {} failed", failures.len());
    }

    Ok(())
}
