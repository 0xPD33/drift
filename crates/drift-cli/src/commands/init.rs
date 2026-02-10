use anyhow::bail;
use drift_core::config::{
    EnvConfig, ProjectConfig, ProjectMeta, WindowConfig,
};
use drift_core::{config, kdl, paths, registry};

pub fn run(name: &str, repo: Option<&str>, folder: Option<&str>) -> anyhow::Result<()> {
    let config_path = paths::project_config_path(name);
    if config_path.exists() {
        bail!("Project '{}' already exists at {}", name, config_path.display());
    }

    let repo_path = match repo {
        Some(r) => r.to_string(),
        None => std::env::current_dir()?
            .to_string_lossy()
            .to_string(),
    };

    let project = ProjectConfig {
        project: ProjectMeta {
            name: name.to_string(),
            repo: repo_path,
            folder: folder.map(|f| f.to_string()),
            icon: None,
        },
        env: EnvConfig::default(),
        git: None,
        ports: None,
        services: None,
        windows: vec![WindowConfig {
            name: None,
            command: None,
        }],
        scratchpad: None,
    };

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_str = toml::to_string_pretty(&project)?;
    std::fs::write(&config_path, toml_str)?;

    std::fs::create_dir_all(paths::state_dir(name))?;
    std::fs::create_dir_all(paths::logs_dir(name))?;

    let projects = registry::list_projects()?;
    let global = config::load_global_config()?;
    kdl::write_niri_rules(&projects, &global)?;

    println!("Initialized project '{name}' at {}", config_path.display());
    Ok(())
}
