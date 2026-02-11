use drift_core::{config, kdl, registry};

pub fn archive(name: &str) -> anyhow::Result<()> {
    let _project = config::load_project_config(name)?;
    let global = config::load_global_config()?;

    // Close workspace if open
    if let Ok(mut niri_client) = drift_core::niri::NiriClient::connect() {
        if niri_client.find_workspace_by_name(name)?.is_some() {
            super::close::close_project(name)?;
            println!("  Closed workspace");
        }
    }

    registry::archive_project(name)?;

    // Regenerate niri-rules
    let all_projects = registry::list_projects()?;
    kdl::write_niri_rules(&all_projects, &global)?;

    drift_core::events::try_emit_event(&drift_core::events::Event {
        event_type: "drift.project.archived".into(),
        project: name.to_string(),
        source: "drift".into(),
        ts: drift_core::events::iso_now(),
        level: Some("info".into()),
        title: Some(format!("Archived project '{name}'")),
        body: None,
        meta: None,
        priority: None,
    });

    println!("Archived project '{name}'");
    Ok(())
}

pub fn unarchive(name: &str) -> anyhow::Result<()> {
    let global = config::load_global_config()?;

    registry::unarchive_project(name)?;

    // Regenerate niri-rules
    let all_projects = registry::list_projects()?;
    kdl::write_niri_rules(&all_projects, &global)?;

    drift_core::events::try_emit_event(&drift_core::events::Event {
        event_type: "drift.project.unarchived".into(),
        project: name.to_string(),
        source: "drift".into(),
        ts: drift_core::events::iso_now(),
        level: Some("info".into()),
        title: Some(format!("Unarchived project '{name}'")),
        body: None,
        meta: None,
        priority: None,
    });

    println!("Unarchived project '{name}'");
    Ok(())
}
