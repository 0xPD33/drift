use anyhow::bail;
use clap::Subcommand;
use drift_core::config::{
    self, ProjectPorts, RestartPolicy, ServiceProcess, ServicesConfig, WindowConfig,
};

#[derive(Subcommand)]
pub enum AddCommand {
    /// Add a service process
    Service {
        name: String,
        command: String,
        #[arg(long, default_value = "never")]
        restart: String,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        project: Option<String>,
    },
    /// Add an AI agent service
    Agent {
        name: String,
        /// Agent type (claude, codex)
        agent: String,
        prompt: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value = "oneshot")]
        mode: String,
        #[arg(long, default_value = "full")]
        permissions: String,
        #[arg(long, default_value = "on-failure")]
        restart: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Add a terminal window
    Window {
        name: String,
        /// Command to run (empty for shell)
        command: Option<String>,
        #[arg(long)]
        tmux: bool,
        #[arg(long)]
        project: Option<String>,
    },
    /// Add an environment variable
    Env {
        key: String,
        value: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Add a named port
    Port {
        name: String,
        port: u16,
        #[arg(long)]
        project: Option<String>,
    },
    /// Set the port range
    PortRange {
        start: u16,
        end: u16,
        #[arg(long)]
        project: Option<String>,
    },
}

fn parse_restart(s: &str) -> anyhow::Result<RestartPolicy> {
    match s {
        "never" => Ok(RestartPolicy::Never),
        "on-failure" => Ok(RestartPolicy::OnFailure),
        "always" => Ok(RestartPolicy::Always),
        _ => bail!("Invalid restart policy '{s}'. Use: never, on-failure, always"),
    }
}

pub fn run(cmd: AddCommand) -> anyhow::Result<()> {
    match cmd {
        AddCommand::Service { name, command, restart, cwd, project } => {
            let proj = config::resolve_current_project(project.as_deref())?;
            let mut cfg = config::load_project_config(&proj)?;
            let services = cfg.services.get_or_insert_with(|| ServicesConfig { processes: vec![] });
            if services.processes.iter().any(|p| p.name == name) {
                bail!("Service '{name}' already exists in project '{proj}'");
            }
            services.processes.push(ServiceProcess {
                name: name.clone(),
                command,
                cwd: cwd.unwrap_or_else(|| ".".into()),
                restart: parse_restart(&restart)?,
                stop_command: None,
                agent: None,
                prompt: None,
                agent_mode: "oneshot".into(),
                agent_model: None,
                agent_permissions: "full".into(),
                width: None,
            });
            config::save_project_config(&proj, &cfg)?;
            println!("Added service '{name}' to project '{proj}'");
            Ok(())
        }
        AddCommand::Agent { name, agent, prompt, model, mode, permissions, restart, project } => {
            let proj = config::resolve_current_project(project.as_deref())?;
            let mut cfg = config::load_project_config(&proj)?;
            let services = cfg.services.get_or_insert_with(|| ServicesConfig { processes: vec![] });
            if services.processes.iter().any(|p| p.name == name) {
                bail!("Service '{name}' already exists in project '{proj}'");
            }
            services.processes.push(ServiceProcess {
                name: name.clone(),
                command: String::new(),
                cwd: ".".into(),
                restart: parse_restart(&restart)?,
                stop_command: None,
                agent: Some(agent),
                prompt: Some(prompt),
                agent_mode: mode,
                agent_model: model,
                agent_permissions: permissions,
                width: None,
            });
            config::save_project_config(&proj, &cfg)?;
            println!("Added agent '{name}' to project '{proj}'");
            Ok(())
        }
        AddCommand::Window { name, command, tmux, project } => {
            let proj = config::resolve_current_project(project.as_deref())?;
            let mut cfg = config::load_project_config(&proj)?;
            if cfg.windows.iter().any(|w| w.name.as_deref() == Some(&name)) {
                bail!("Window '{name}' already exists in project '{proj}'");
            }
            cfg.windows.push(WindowConfig {
                name: Some(name.clone()),
                command,
                width: None,
                tmux: if tmux { Some(true) } else { None },
                app_id: None,
            });
            config::save_project_config(&proj, &cfg)?;
            println!("Added window '{name}' to project '{proj}'");
            Ok(())
        }
        AddCommand::Env { key, value, project } => {
            let proj = config::resolve_current_project(project.as_deref())?;
            let mut cfg = config::load_project_config(&proj)?;
            cfg.env.vars.insert(key.clone(), value);
            config::save_project_config(&proj, &cfg)?;
            println!("Set env '{key}' in project '{proj}'");
            Ok(())
        }
        AddCommand::Port { name, port, project } => {
            let proj = config::resolve_current_project(project.as_deref())?;
            let mut cfg = config::load_project_config(&proj)?;
            let ports = cfg.ports.get_or_insert_with(|| ProjectPorts {
                range: None,
                named: std::collections::HashMap::new(),
            });
            ports.named.insert(name.clone(), port);
            config::save_project_config(&proj, &cfg)?;
            println!("Added port '{name}={port}' to project '{proj}'");
            Ok(())
        }
        AddCommand::PortRange { start, end, project } => {
            let proj = config::resolve_current_project(project.as_deref())?;
            let mut cfg = config::load_project_config(&proj)?;
            let ports = cfg.ports.get_or_insert_with(|| ProjectPorts {
                range: None,
                named: std::collections::HashMap::new(),
            });
            ports.range = Some([start, end]);
            config::save_project_config(&proj, &cfg)?;
            println!("Set port range {start}-{end} in project '{proj}'");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drift_core::config::{EnvConfig, ProjectConfig, ProjectMeta};

    #[test]
    fn parse_restart_never() {
        assert!(matches!(parse_restart("never").unwrap(), RestartPolicy::Never));
    }

    #[test]
    fn parse_restart_on_failure() {
        assert!(matches!(parse_restart("on-failure").unwrap(), RestartPolicy::OnFailure));
    }

    #[test]
    fn parse_restart_always() {
        assert!(matches!(parse_restart("always").unwrap(), RestartPolicy::Always));
    }

    #[test]
    fn parse_restart_invalid() {
        assert!(parse_restart("bogus").is_err());
    }

    fn minimal_config(name: &str) -> ProjectConfig {
        ProjectConfig {
            project: ProjectMeta {
                name: name.into(),
                repo: "/tmp".into(),
                folder: None,
                icon: None,
            },
            env: EnvConfig::default(),
            git: None,
            ports: None,
            services: None,
            windows: vec![],
            tmux: None,
            scratchpad: None,
        }
    }

    #[test]
    fn add_service_to_empty_config() {
        let mut cfg = minimal_config("test");
        let services = cfg.services.get_or_insert_with(|| ServicesConfig { processes: vec![] });
        services.processes.push(ServiceProcess {
            name: "api".into(),
            command: "npm start".into(),
            cwd: ".".into(),
            restart: RestartPolicy::Never,
            stop_command: None,
            agent: None,
            prompt: None,
            agent_mode: "oneshot".into(),
            agent_model: None,
            agent_permissions: "full".into(),
            width: None,
        });
        assert_eq!(cfg.services.as_ref().unwrap().processes.len(), 1);
        assert_eq!(cfg.services.as_ref().unwrap().processes[0].name, "api");
    }

    #[test]
    fn add_service_duplicate_detection() {
        let mut cfg = minimal_config("test");
        cfg.services = Some(ServicesConfig {
            processes: vec![ServiceProcess {
                name: "api".into(),
                command: "npm start".into(),
                cwd: ".".into(),
                restart: RestartPolicy::Never,
                stop_command: None,
                agent: None,
                prompt: None,
                agent_mode: "oneshot".into(),
                agent_model: None,
                agent_permissions: "full".into(),
                width: None,
            }],
        });
        let has_dup = cfg.services.as_ref().unwrap().processes.iter().any(|p| p.name == "api");
        assert!(has_dup);
    }

    #[test]
    fn add_agent_populates_fields() {
        let mut cfg = minimal_config("test");
        let services = cfg.services.get_or_insert_with(|| ServicesConfig { processes: vec![] });
        services.processes.push(ServiceProcess {
            name: "reviewer".into(),
            command: String::new(),
            cwd: ".".into(),
            restart: parse_restart("on-failure").unwrap(),
            stop_command: None,
            agent: Some("claude".into()),
            prompt: Some("Review code".into()),
            agent_mode: "interactive".into(),
            agent_model: Some("opus".into()),
            agent_permissions: "safe".into(),
            width: None,
        });
        let svc = &cfg.services.as_ref().unwrap().processes[0];
        assert_eq!(svc.agent.as_deref(), Some("claude"));
        assert_eq!(svc.prompt.as_deref(), Some("Review code"));
        assert_eq!(svc.agent_mode, "interactive");
        assert_eq!(svc.agent_model.as_deref(), Some("opus"));
        assert_eq!(svc.agent_permissions, "safe");
    }

    #[test]
    fn add_window_duplicate_detection() {
        let mut cfg = minimal_config("test");
        cfg.windows.push(WindowConfig { name: Some("editor".into()), command: Some("nvim .".into()), width: None, tmux: None, app_id: None });
        let has_dup = cfg.windows.iter().any(|w| w.name.as_deref() == Some("editor"));
        assert!(has_dup);
    }

    #[test]
    fn add_env_var() {
        let mut cfg = minimal_config("test");
        cfg.env.vars.insert("NODE_ENV".into(), "development".into());
        assert_eq!(cfg.env.vars.get("NODE_ENV").unwrap(), "development");
    }

    #[test]
    fn add_env_var_overwrites() {
        let mut cfg = minimal_config("test");
        cfg.env.vars.insert("PORT".into(), "3000".into());
        cfg.env.vars.insert("PORT".into(), "8080".into());
        assert_eq!(cfg.env.vars.get("PORT").unwrap(), "8080");
    }

    #[test]
    fn add_port_to_empty_config() {
        let mut cfg = minimal_config("test");
        let ports = cfg.ports.get_or_insert_with(|| ProjectPorts {
            range: None,
            named: std::collections::HashMap::new(),
        });
        ports.named.insert("api".into(), 3001);
        assert_eq!(*cfg.ports.as_ref().unwrap().named.get("api").unwrap(), 3001);
    }

    #[test]
    fn add_port_range() {
        let mut cfg = minimal_config("test");
        let ports = cfg.ports.get_or_insert_with(|| ProjectPorts {
            range: None,
            named: std::collections::HashMap::new(),
        });
        ports.range = Some([3000, 3010]);
        assert_eq!(cfg.ports.as_ref().unwrap().range, Some([3000, 3010]));
    }
}
