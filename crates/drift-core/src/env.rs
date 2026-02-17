use std::collections::HashMap;

use crate::config::{self, ProjectConfig};

pub fn build_env(project: &ProjectConfig) -> anyhow::Result<HashMap<String, String>> {
    let mut env = HashMap::new();

    let repo_path = config::resolve_repo_path(&project.project.repo);
    let repo_str = repo_path.to_string_lossy().to_string();

    env.insert("DRIFT_PROJECT".into(), project.project.name.clone());
    env.insert("DRIFT_REPO".into(), repo_str.clone());
    env.insert(
        "DRIFT_FOLDER".into(),
        project
            .project
            .folder
            .as_deref()
            .unwrap_or("")
            .to_string(),
    );

    env.insert(
        "DRIFT_NOTIFY_SOCK".into(),
        crate::paths::emit_socket_path().to_string_lossy().to_string(),
    );

    if let Some(env_file) = &project.env.env_file {
        let env_path = repo_path.join(env_file);
        if env_path.exists() {
            let contents = std::fs::read_to_string(&env_path)?;
            for line in contents.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = trimmed.split_once('=') {
                    env.insert(key.trim().to_string(), value.trim().to_string());
                }
            }
        }
    }

    for (key, value) in &project.env.vars {
        env.insert(key.clone(), value.clone());
    }

    if let Some(ports) = &project.ports {
        if let Some([start, end]) = ports.range {
            env.insert("DRIFT_PORT_RANGE_START".into(), start.to_string());
            env.insert("DRIFT_PORT_RANGE_END".into(), end.to_string());
        }
        for (name, port) in &ports.named {
            env.insert(
                format!("DRIFT_PORT_{}", name.to_uppercase()),
                port.to_string(),
            );
        }
    }

    Ok(env)
}

pub fn format_env_exports(env: &HashMap<String, String>) -> String {
    let mut keys: Vec<&String> = env.keys().collect();
    keys.sort();
    keys.iter()
        .map(|k| format!("export {}='{}'", k, env[*k].replace('\'', "'\"'\"'")))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EnvConfig, ProjectConfig, ProjectMeta};

    fn minimal_project(name: &str, repo: &str) -> ProjectConfig {
        ProjectConfig {
            project: ProjectMeta {
                name: name.into(),
                repo: repo.into(),
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
    fn build_env_minimal() {
        let project = minimal_project("myapp", "/tmp/myapp");
        let env = build_env(&project).unwrap();
        assert_eq!(env.get("DRIFT_PROJECT").unwrap(), "myapp");
        assert_eq!(env.get("DRIFT_REPO").unwrap(), "/tmp/myapp");
        assert_eq!(env.get("DRIFT_FOLDER").unwrap(), "");
        assert!(env.contains_key("DRIFT_NOTIFY_SOCK"));
    }

    #[test]
    fn build_env_with_folder() {
        let mut project = minimal_project("myapp", "/tmp/myapp");
        project.project.folder = Some("web".into());
        let env = build_env(&project).unwrap();
        assert_eq!(env.get("DRIFT_FOLDER").unwrap(), "web");
    }

    #[test]
    fn build_env_with_inline_vars() {
        let mut project = minimal_project("myapp", "/tmp/myapp");
        project.env.vars.insert("FOO".into(), "bar".into());
        project.env.vars.insert("BAZ".into(), "qux".into());
        let env = build_env(&project).unwrap();
        assert_eq!(env.get("FOO").unwrap(), "bar");
        assert_eq!(env.get("BAZ").unwrap(), "qux");
        assert_eq!(env.get("DRIFT_PROJECT").unwrap(), "myapp");
    }

    #[test]
    fn format_env_exports_sorted() {
        let mut env = HashMap::new();
        env.insert("ZZZ".into(), "last".into());
        env.insert("AAA".into(), "first".into());
        env.insert("MMM".into(), "middle".into());
        let result = format_env_exports(&env);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "export AAA='first'");
        assert_eq!(lines[1], "export MMM='middle'");
        assert_eq!(lines[2], "export ZZZ='last'");
    }

    #[test]
    fn format_env_exports_quotes_single_quotes() {
        let mut env = HashMap::new();
        env.insert("VAL".into(), "it's a test".into());
        let result = format_env_exports(&env);
        assert_eq!(result, "export VAL='it'\"'\"'s a test'");
    }

    #[test]
    fn format_env_exports_empty() {
        let env = HashMap::new();
        let result = format_env_exports(&env);
        assert_eq!(result, "");
    }

    #[test]
    fn build_env_with_ports_range() {
        use crate::config::ProjectPorts;
        let mut project = minimal_project("myapp", "/tmp/myapp");
        project.ports = Some(ProjectPorts {
            range: Some([3000, 3010]),
            named: HashMap::new(),
        });
        let env = build_env(&project).unwrap();
        assert_eq!(env.get("DRIFT_PORT_RANGE_START").unwrap(), "3000");
        assert_eq!(env.get("DRIFT_PORT_RANGE_END").unwrap(), "3010");
    }

    #[test]
    fn build_env_with_named_ports() {
        use crate::config::ProjectPorts;
        let mut project = minimal_project("myapp", "/tmp/myapp");
        let mut named = HashMap::new();
        named.insert("api".into(), 3001);
        named.insert("frontend".into(), 3002);
        project.ports = Some(ProjectPorts {
            range: None,
            named,
        });
        let env = build_env(&project).unwrap();
        assert_eq!(env.get("DRIFT_PORT_API").unwrap(), "3001");
        assert_eq!(env.get("DRIFT_PORT_FRONTEND").unwrap(), "3002");
    }

    #[test]
    fn build_env_with_ports_range_and_named() {
        use crate::config::ProjectPorts;
        let mut project = minimal_project("myapp", "/tmp/myapp");
        let mut named = HashMap::new();
        named.insert("api".into(), 3001);
        project.ports = Some(ProjectPorts {
            range: Some([3000, 3010]),
            named,
        });
        let env = build_env(&project).unwrap();
        assert_eq!(env.get("DRIFT_PORT_RANGE_START").unwrap(), "3000");
        assert_eq!(env.get("DRIFT_PORT_RANGE_END").unwrap(), "3010");
        assert_eq!(env.get("DRIFT_PORT_API").unwrap(), "3001");
    }
}
