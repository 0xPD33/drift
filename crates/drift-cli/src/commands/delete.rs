use anyhow::bail;
use drift_core::{config, kdl, registry};

pub fn run(name: &str, yes: bool) -> anyhow::Result<()> {
    // Verify project exists
    let _project = config::load_project_config(name)?;
    let global = config::load_global_config()?;

    if !yes {
        bail!("Deleting '{name}' will remove its config and all state/logs.\nRe-run with --yes to confirm.");
    }

    // Close workspace if open
    if let Ok(mut niri_client) = drift_core::niri::NiriClient::connect() {
        if niri_client.find_workspace_by_name(name)?.is_some() {
            super::close::close_project(name)?;
            println!("  Closed workspace");
        }
    }

    // Remove config + state
    registry::delete_project(name)?;
    println!("  Removed config and state");

    // Regenerate niri-rules
    let all_projects = registry::list_projects()?;
    kdl::write_niri_rules(&all_projects, &global)?;

    drift_core::events::try_emit_event(&drift_core::events::Event {
        event_type: "drift.project.deleted".into(),
        project: name.to_string(),
        source: "drift".into(),
        ts: drift_core::events::iso_now(),
        level: Some("info".into()),
        title: Some(format!("Deleted project '{name}'")),
        body: None,
        meta: None,
        priority: None,
    });

    println!("Deleted project '{name}'");
    Ok(())
}
