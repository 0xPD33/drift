use drift_core::{config, kdl, paths, registry};

pub fn run() -> anyhow::Result<()> {
    let projects = registry::list_projects()?;
    let global = config::load_global_config()?;
    kdl::write_niri_rules(&projects, &global)?;

    let path = paths::niri_rules_path();
    println!("Wrote niri rules to {}", path.display());
    Ok(())
}
