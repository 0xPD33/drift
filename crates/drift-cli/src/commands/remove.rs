use anyhow::bail;
use clap::Subcommand;
use drift_core::config;

#[derive(Subcommand)]
pub enum RemoveCommand {
    /// Remove a service process
    Service {
        name: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Remove an agent service
    Agent {
        name: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Remove a terminal window
    Window {
        name: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Remove an environment variable
    Env {
        key: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Remove a named port
    Port {
        name: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Remove the port range
    PortRange {
        #[arg(long)]
        project: Option<String>,
    },
}

pub fn run(cmd: RemoveCommand) -> anyhow::Result<()> {
    match cmd {
        RemoveCommand::Service { name, project } | RemoveCommand::Agent { name, project } => {
            let proj = config::resolve_current_project(project.as_deref())?;
            let mut cfg = config::load_project_config(&proj)?;
            if let Some(services) = &mut cfg.services {
                let before = services.processes.len();
                services.processes.retain(|p| p.name != name);
                if services.processes.len() == before {
                    bail!("Service '{name}' not found in project '{proj}'");
                }
                if services.processes.is_empty() {
                    cfg.services = None;
                }
            } else {
                bail!("No services in project '{proj}'");
            }
            config::save_project_config(&proj, &cfg)?;
            println!("Removed service '{name}' from project '{proj}'");
            Ok(())
        }
        RemoveCommand::Window { name, project } => {
            let proj = config::resolve_current_project(project.as_deref())?;
            let mut cfg = config::load_project_config(&proj)?;
            let before = cfg.windows.len();
            cfg.windows.retain(|w| w.name.as_deref() != Some(&name));
            if cfg.windows.len() == before {
                bail!("Window '{name}' not found in project '{proj}'");
            }
            config::save_project_config(&proj, &cfg)?;
            println!("Removed window '{name}' from project '{proj}'");
            Ok(())
        }
        RemoveCommand::Env { key, project } => {
            let proj = config::resolve_current_project(project.as_deref())?;
            let mut cfg = config::load_project_config(&proj)?;
            if cfg.env.vars.remove(&key).is_none() {
                bail!("Env var '{key}' not found in project '{proj}'");
            }
            config::save_project_config(&proj, &cfg)?;
            println!("Removed env '{key}' from project '{proj}'");
            Ok(())
        }
        RemoveCommand::Port { name, project } => {
            let proj = config::resolve_current_project(project.as_deref())?;
            let mut cfg = config::load_project_config(&proj)?;
            if let Some(ports) = &mut cfg.ports {
                if ports.named.remove(&name).is_none() {
                    bail!("Port '{name}' not found in project '{proj}'");
                }
                if ports.range.is_none() && ports.named.is_empty() {
                    cfg.ports = None;
                }
            } else {
                bail!("No ports in project '{proj}'");
            }
            config::save_project_config(&proj, &cfg)?;
            println!("Removed port '{name}' from project '{proj}'");
            Ok(())
        }
        RemoveCommand::PortRange { project } => {
            let proj = config::resolve_current_project(project.as_deref())?;
            let mut cfg = config::load_project_config(&proj)?;
            if let Some(ports) = &mut cfg.ports {
                if ports.range.is_none() {
                    bail!("No port range set in project '{proj}'");
                }
                ports.range = None;
                if ports.named.is_empty() {
                    cfg.ports = None;
                }
            } else {
                bail!("No ports in project '{proj}'");
            }
            config::save_project_config(&proj, &cfg)?;
            println!("Removed port range from project '{proj}'");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use drift_core::config::*;

    fn config_with_services() -> ProjectConfig {
        ProjectConfig {
            project: ProjectMeta {
                name: "test".into(),
                repo: "/tmp".into(),
                folder: None,
                icon: None,
            },
            env: EnvConfig::default(),
            git: None,
            ports: None,
            services: Some(ServicesConfig {
                processes: vec![
                    ServiceProcess {
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
                    },
                    ServiceProcess {
                        name: "worker".into(),
                        command: "npm run worker".into(),
                        cwd: ".".into(),
                        restart: RestartPolicy::Always,
                        stop_command: None,
                        agent: None,
                        prompt: None,
                        agent_mode: "oneshot".into(),
                        agent_model: None,
                        agent_permissions: "full".into(),
                    },
                ],
            }),
            windows: vec![
                WindowConfig { name: Some("editor".into()), command: Some("nvim .".into()) },
                WindowConfig { name: Some("shell".into()), command: None },
            ],
            scratchpad: None,
        }
    }

    #[test]
    fn remove_service_retains_others() {
        let mut cfg = config_with_services();
        if let Some(services) = &mut cfg.services {
            services.processes.retain(|p| p.name != "api");
        }
        let procs = &cfg.services.as_ref().unwrap().processes;
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0].name, "worker");
    }

    #[test]
    fn remove_last_service_clears_section() {
        let mut cfg = config_with_services();
        if let Some(services) = &mut cfg.services {
            services.processes.retain(|p| p.name != "api");
            services.processes.retain(|p| p.name != "worker");
            if services.processes.is_empty() {
                cfg.services = None;
            }
        }
        assert!(cfg.services.is_none());
    }

    #[test]
    fn remove_nonexistent_service_detected() {
        let cfg = config_with_services();
        let exists = cfg.services.as_ref().unwrap().processes.iter().any(|p| p.name == "ghost");
        assert!(!exists);
    }

    #[test]
    fn remove_window_retains_others() {
        let mut cfg = config_with_services();
        let before = cfg.windows.len();
        cfg.windows.retain(|w| w.name.as_deref() != Some("editor"));
        assert_eq!(cfg.windows.len(), before - 1);
        assert_eq!(cfg.windows[0].name.as_deref(), Some("shell"));
    }

    #[test]
    fn remove_nonexistent_window_detected() {
        let cfg = config_with_services();
        let exists = cfg.windows.iter().any(|w| w.name.as_deref() == Some("ghost"));
        assert!(!exists);
    }

    #[test]
    fn remove_env_var() {
        let mut cfg = config_with_services();
        cfg.env.vars.insert("NODE_ENV".into(), "dev".into());
        cfg.env.vars.insert("PORT".into(), "3000".into());
        assert!(cfg.env.vars.remove("NODE_ENV").is_some());
        assert_eq!(cfg.env.vars.len(), 1);
        assert!(cfg.env.vars.contains_key("PORT"));
    }

    #[test]
    fn remove_nonexistent_env_detected() {
        let cfg = config_with_services();
        assert!(!cfg.env.vars.contains_key("GHOST"));
    }

    #[test]
    fn remove_port_retains_others() {
        let mut cfg = config_with_services();
        cfg.ports = Some(ProjectPorts {
            range: Some([3000, 3010]),
            named: [("api".into(), 3001), ("web".into(), 3002)].into_iter().collect(),
        });
        if let Some(ports) = &mut cfg.ports {
            ports.named.remove("api");
        }
        let ports = cfg.ports.as_ref().unwrap();
        assert_eq!(ports.named.len(), 1);
        assert!(ports.named.contains_key("web"));
        assert!(ports.range.is_some());
    }

    #[test]
    fn remove_last_port_with_no_range_clears_section() {
        let mut cfg = config_with_services();
        cfg.ports = Some(ProjectPorts {
            range: None,
            named: [("api".into(), 3001)].into_iter().collect(),
        });
        if let Some(ports) = &mut cfg.ports {
            ports.named.remove("api");
            if ports.range.is_none() && ports.named.is_empty() {
                cfg.ports = None;
            }
        }
        assert!(cfg.ports.is_none());
    }

    #[test]
    fn remove_port_range_keeps_named() {
        let mut cfg = config_with_services();
        cfg.ports = Some(ProjectPorts {
            range: Some([3000, 3010]),
            named: [("api".into(), 3001)].into_iter().collect(),
        });
        if let Some(ports) = &mut cfg.ports {
            ports.range = None;
        }
        let ports = cfg.ports.as_ref().unwrap();
        assert!(ports.range.is_none());
        assert_eq!(ports.named.len(), 1);
    }

    #[test]
    fn remove_port_range_no_named_clears_section() {
        let mut cfg = config_with_services();
        cfg.ports = Some(ProjectPorts {
            range: Some([3000, 3010]),
            named: std::collections::HashMap::new(),
        });
        if let Some(ports) = &mut cfg.ports {
            ports.range = None;
            if ports.named.is_empty() {
                cfg.ports = None;
            }
        }
        assert!(cfg.ports.is_none());
    }
}
