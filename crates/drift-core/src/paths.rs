use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .expect("could not determine config directory")
        .join("drift")
}

pub fn projects_dir() -> PathBuf {
    config_dir().join("projects")
}

pub fn archived_projects_dir() -> PathBuf {
    projects_dir().join("archived")
}

pub fn state_dir(project: &str) -> PathBuf {
    dirs::state_dir()
        .expect("could not determine state directory")
        .join("drift")
        .join(project)
}

pub fn logs_dir(project: &str) -> PathBuf {
    state_dir(project).join("logs")
}

pub fn niri_rules_path() -> PathBuf {
    config_dir().join("niri-rules.kdl")
}

pub fn pid_file(project: &str, service: &str) -> PathBuf {
    state_dir(project).join(format!("{service}.pid"))
}

pub fn templates_dir() -> PathBuf {
    config_dir().join("templates")
}

pub fn global_config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn project_config_path(name: &str) -> PathBuf {
    projects_dir().join(format!("{name}.toml"))
}

pub fn supervisor_pid_path(project: &str) -> PathBuf {
    state_dir(project).join("supervisor.pid")
}

pub fn services_state_path(project: &str) -> PathBuf {
    state_dir(project).join("services.json")
}

pub fn workspace_state_path(project: &str) -> PathBuf {
    state_dir(project).join("workspace.json")
}

pub fn state_base_dir() -> PathBuf {
    dirs::state_dir()
        .expect("could not determine state directory")
        .join("drift")
}

pub fn daemon_pid_path() -> PathBuf {
    state_base_dir().join("daemon.pid")
}

pub fn daemon_state_path() -> PathBuf {
    state_base_dir().join("daemon.json")
}

pub fn emit_socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(runtime_dir).join("drift").join("emit.sock")
}

pub fn subscribe_socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(runtime_dir).join("drift").join("subscribe.sock")
}

pub fn notify_socket_path() -> PathBuf {
    emit_socket_path()
}

pub fn commander_pid_path() -> PathBuf {
    state_base_dir().join("commander.pid")
}

pub fn commander_muted_path() -> PathBuf {
    state_base_dir().join("commander.muted")
}

pub fn commander_state_path() -> PathBuf {
    state_base_dir().join("commander.json")
}

pub fn session_path() -> PathBuf {
    state_base_dir().join("session.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_ends_in_drift() {
        let p = config_dir();
        assert_eq!(p.file_name().unwrap(), "drift");
    }

    #[test]
    fn projects_dir_is_config_dir_projects() {
        let p = projects_dir();
        assert_eq!(p, config_dir().join("projects"));
    }

    #[test]
    fn state_dir_ends_in_drift_project() {
        let p = state_dir("myapp");
        assert!(p.ends_with("drift/myapp"), "got: {}", p.display());
    }

    #[test]
    fn logs_dir_is_state_dir_logs() {
        let p = logs_dir("myapp");
        assert_eq!(p, state_dir("myapp").join("logs"));
    }

    #[test]
    fn supervisor_pid_path_is_state_dir_supervisor_pid() {
        let p = supervisor_pid_path("myapp");
        assert_eq!(p, state_dir("myapp").join("supervisor.pid"));
    }

    #[test]
    fn services_state_path_is_state_dir_services_json() {
        let p = services_state_path("myapp");
        assert_eq!(p, state_dir("myapp").join("services.json"));
    }

    #[test]
    fn workspace_state_path_is_state_dir_workspace_json() {
        let p = workspace_state_path("myapp");
        assert_eq!(p, state_dir("myapp").join("workspace.json"));
    }

    #[test]
    fn emit_socket_path_ends_in_drift_emit_sock() {
        let p = emit_socket_path();
        assert!(p.ends_with("drift/emit.sock"), "got: {}", p.display());
    }

    #[test]
    fn subscribe_socket_path_ends_in_drift_subscribe_sock() {
        let p = subscribe_socket_path();
        assert!(p.ends_with("drift/subscribe.sock"), "got: {}", p.display());
    }

    #[test]
    fn notify_socket_path_is_emit_socket_path() {
        assert_eq!(notify_socket_path(), emit_socket_path());
    }

    #[test]
    fn templates_dir_is_config_dir_templates() {
        let p = templates_dir();
        assert_eq!(p, config_dir().join("templates"));
    }
}
