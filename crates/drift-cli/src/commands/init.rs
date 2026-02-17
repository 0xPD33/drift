use anyhow::bail;
use drift_core::config::{EnvConfig, ProjectConfig, ProjectMeta};
use drift_core::{config, kdl, paths, registry};

fn load_template(template_name: &str) -> anyhow::Result<ProjectConfig> {
    let template_path = paths::templates_dir().join(format!("{template_name}.toml"));
    if !template_path.exists() {
        bail!("Template '{}' not found at {}", template_name, template_path.display());
    }
    let content = std::fs::read_to_string(&template_path)?;
    let config: ProjectConfig = toml::from_str(&content)?;
    Ok(config)
}

pub fn run(name: &str, repo: Option<&str>, folder: Option<&str>, template: Option<&str>) -> anyhow::Result<()> {
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

    let project = match template {
        Some(tmpl) => {
            let mut config = load_template(tmpl)?;
            config.project.name = name.to_string();
            config.project.repo = repo_path;
            if let Some(f) = folder {
                config.project.folder = Some(f.to_string());
            }
            config
        }
        None => ProjectConfig {
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
            windows: vec![],
            scratchpad: None,
            tmux: None,
        },
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
