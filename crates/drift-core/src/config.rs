use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Deserialize, Serialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub ports: PortDefaults,
    #[serde(default)]
    pub events: EventsConfig,
    #[serde(default)]
    pub commander: CommanderConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Defaults {
    #[serde(default = "default_terminal")]
    pub terminal: String,
    #[serde(default = "default_editor")]
    pub editor: String,
    #[serde(default = "default_shell")]
    pub shell: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            terminal: default_terminal(),
            editor: default_editor(),
            shell: default_shell(),
        }
    }
}

fn default_terminal() -> String {
    "ghostty".into()
}
fn default_editor() -> String {
    "nvim".into()
}
fn default_shell() -> String {
    "zsh".into()
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PortDefaults {
    #[serde(default = "default_port_base")]
    pub base: u16,
    #[serde(default = "default_range_size")]
    pub range_size: u16,
}

fn default_port_base() -> u16 {
    3000
}
fn default_range_size() -> u16 {
    10
}

impl Default for PortDefaults {
    fn default() -> Self {
        Self {
            base: default_port_base(),
            range_size: default_range_size(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EventsConfig {
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    #[serde(default = "default_replay_on_subscribe")]
    pub replay_on_subscribe: usize,
}

fn default_buffer_size() -> usize { 200 }
fn default_replay_on_subscribe() -> usize { 20 }

impl Default for EventsConfig {
    fn default() -> Self {
        Self {
            buffer_size: default_buffer_size(),
            replay_on_subscribe: default_replay_on_subscribe(),
        }
    }
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            defaults: Defaults::default(),
            ports: PortDefaults::default(),
            events: EventsConfig::default(),
            commander: CommanderConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommanderConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_voice")]
    pub voice: String,
    #[serde(default)]
    pub instruct: String,
    #[serde(default)]
    pub fallback_engine: Option<String>,
    #[serde(default)]
    pub fallback_voice: Option<String>,
    #[serde(default)]
    pub fallback_command: Option<String>,
    #[serde(default)]
    pub audio_filter: Option<String>,
    #[serde(default)]
    pub speak_background_only: bool,
    #[serde(default = "default_cooldown_sec")]
    pub cooldown_sec: u64,
    #[serde(default = "default_max_queue")]
    pub max_queue: usize,
    #[serde(default)]
    pub event_instructs: HashMap<String, String>,
}

fn default_endpoint() -> String { "http://localhost:8880".into() }
fn default_voice() -> String { "Vivian".into() }
fn default_cooldown_sec() -> u64 { 5 }
fn default_max_queue() -> usize { 3 }

impl Default for CommanderConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: default_endpoint(),
            voice: default_voice(),
            instruct: String::new(),
            fallback_engine: None,
            fallback_voice: None,
            fallback_command: None,
            audio_filter: None,
            speak_background_only: false,
            cooldown_sec: default_cooldown_sec(),
            max_queue: default_max_queue(),
            event_instructs: HashMap::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub project: ProjectMeta,
    #[serde(default)]
    pub env: EnvConfig,
    #[serde(default)]
    pub git: Option<GitConfig>,
    #[serde(default)]
    pub ports: Option<ProjectPorts>,
    #[serde(default)]
    pub services: Option<ServicesConfig>,
    #[serde(default)]
    pub windows: Vec<WindowConfig>,
    #[serde(default)]
    pub scratchpad: Option<ScratchpadConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProjectMeta {
    pub name: String,
    pub repo: String,
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct EnvConfig {
    #[serde(default)]
    pub env_file: Option<String>,
    #[serde(flatten)]
    pub vars: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitConfig {
    pub user_name: Option<String>,
    pub user_email: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProjectPorts {
    pub range: Option<[u16; 2]>,
    #[serde(flatten)]
    pub named: HashMap<String, u16>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServicesConfig {
    #[serde(default)]
    pub processes: Vec<ServiceProcess>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceProcess {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default = "default_cwd", skip_serializing_if = "is_default_cwd")]
    pub cwd: String,
    #[serde(default, skip_serializing_if = "is_default_restart")]
    pub restart: RestartPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default = "default_agent_mode", skip_serializing_if = "is_default_agent_mode")]
    pub agent_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_model: Option<String>,
    #[serde(default = "default_agent_permissions", skip_serializing_if = "is_default_agent_permissions")]
    pub agent_permissions: String,
}

fn default_cwd() -> String {
    ".".into()
}

fn default_agent_mode() -> String {
    "oneshot".into()
}

fn default_agent_permissions() -> String {
    "full".into()
}

fn is_default_cwd(s: &str) -> bool {
    s == "."
}

fn is_default_restart(r: &RestartPolicy) -> bool {
    matches!(r, RestartPolicy::Never)
}

fn is_default_agent_mode(s: &str) -> bool {
    s == "oneshot"
}

fn is_default_agent_permissions(s: &str) -> bool {
    s == "full"
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    #[default]
    Never,
    OnFailure,
    Always,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WindowConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScratchpadConfig {
    pub file: String,
}

pub fn load_global_config() -> anyhow::Result<GlobalConfig> {
    let path = paths::global_config_path();
    if !path.exists() {
        return Ok(GlobalConfig::default());
    }
    let contents = std::fs::read_to_string(&path)?;
    let config: GlobalConfig = toml::from_str(&contents)?;
    Ok(config)
}

pub fn load_project_config(name: &str) -> anyhow::Result<ProjectConfig> {
    let path = paths::project_config_path(name);
    let contents = std::fs::read_to_string(&path)?;
    let config: ProjectConfig = toml::from_str(&contents)?;
    Ok(config)
}

pub fn save_project_config(name: &str, config: &ProjectConfig) -> anyhow::Result<()> {
    let path = paths::project_config_path(name);
    let toml_str = toml::to_string_pretty(config)?;
    std::fs::write(&path, toml_str)?;
    Ok(())
}

pub fn resolve_current_project(explicit: Option<&str>) -> anyhow::Result<String> {
    if let Some(n) = explicit {
        return Ok(n.to_string());
    }

    if let Ok(project) = std::env::var("DRIFT_PROJECT") {
        if !project.is_empty() {
            return Ok(project);
        }
    }

    if let Ok(mut client) = crate::niri::NiriClient::connect() {
        if let Ok(Some(win)) = client.focused_window() {
            if let Some(ws_id) = win.workspace_id {
                if let Ok(workspaces) = client.workspaces() {
                    for ws in &workspaces {
                        if ws.id == ws_id {
                            if let Some(ws_name) = &ws.name {
                                return Ok(ws_name.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    anyhow::bail!("Could not determine project name. Use --project, set $DRIFT_PROJECT, or run from a drift workspace.")
}

pub fn resolve_repo_path(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        dirs::home_dir()
            .expect("could not determine home directory")
            .join(rest)
    } else if raw == "~" {
        dirs::home_dir().expect("could not determine home directory")
    } else {
        PathBuf::from(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_project_config() {
        let toml_str = r#"
[project]
name = "myapp"
repo = "~/code/myapp"
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.project.name, "myapp");
        assert_eq!(config.project.repo, "~/code/myapp");
        assert!(config.project.folder.is_none());
        assert!(config.project.icon.is_none());
        assert!(config.env.vars.is_empty());
        assert!(config.env.env_file.is_none());
        assert!(config.git.is_none());
        assert!(config.ports.is_none());
        assert!(config.services.is_none());
        assert!(config.windows.is_empty());
        assert!(config.scratchpad.is_none());
    }

    #[test]
    fn parse_full_project_config() {
        let toml_str = r#"
[project]
name = "webapp"
repo = "~/code/webapp"
folder = "web"
icon = "🌐"

[env]
env_file = ".env"
NODE_ENV = "development"
PORT = "3000"

[git]
user_name = "Dev User"
user_email = "dev@example.com"

[ports]
range = [3000, 3010]
api = 3001
frontend = 3002

[services]
processes = [
    { name = "api", command = "npm run api", restart = "on-failure" },
    { name = "worker", command = "npm run worker", restart = "always", stop_command = "kill -TERM $PID" },
]

[[windows]]
name = "editor"
command = "nvim ."

[[windows]]
name = "shell"

[scratchpad]
file = "notes.md"
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.project.name, "webapp");
        assert_eq!(config.project.folder.as_deref(), Some("web"));
        assert_eq!(config.project.icon.as_deref(), Some("🌐"));
        assert_eq!(config.env.env_file.as_deref(), Some(".env"));
        assert_eq!(config.env.vars.get("NODE_ENV").unwrap(), "development");
        assert_eq!(config.env.vars.get("PORT").unwrap(), "3000");
        let git = config.git.unwrap();
        assert_eq!(git.user_name.as_deref(), Some("Dev User"));
        assert_eq!(git.user_email.as_deref(), Some("dev@example.com"));
        let ports = config.ports.unwrap();
        assert_eq!(ports.range, Some([3000, 3010]));
        assert_eq!(*ports.named.get("api").unwrap(), 3001);
        assert_eq!(*ports.named.get("frontend").unwrap(), 3002);
        let services = config.services.unwrap();
        assert_eq!(services.processes.len(), 2);
        assert_eq!(services.processes[0].name, "api");
        assert!(matches!(services.processes[0].restart, RestartPolicy::OnFailure));
        assert!(matches!(services.processes[1].restart, RestartPolicy::Always));
        assert_eq!(services.processes[1].stop_command.as_deref(), Some("kill -TERM $PID"));
        assert_eq!(config.windows.len(), 2);
        assert_eq!(config.windows[0].name.as_deref(), Some("editor"));
        assert_eq!(config.windows[0].command.as_deref(), Some("nvim ."));
        assert_eq!(config.windows[1].name.as_deref(), Some("shell"));
        assert!(config.windows[1].command.is_none());
        assert_eq!(config.scratchpad.unwrap().file, "notes.md");
    }

    #[test]
    fn parse_global_config_with_values() {
        let toml_str = r#"
[defaults]
terminal = "alacritty"
editor = "code"
shell = "bash"

[ports]
base = 8000
range_size = 20
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.defaults.terminal, "alacritty");
        assert_eq!(config.defaults.editor, "code");
        assert_eq!(config.defaults.shell, "bash");
        assert_eq!(config.ports.base, 8000);
        assert_eq!(config.ports.range_size, 20);
    }

    #[test]
    fn parse_global_config_empty_uses_defaults() {
        let config: GlobalConfig = toml::from_str("").unwrap();
        assert_eq!(config.defaults.terminal, "ghostty");
        assert_eq!(config.defaults.editor, "nvim");
        assert_eq!(config.defaults.shell, "zsh");
        assert_eq!(config.ports.base, 3000);
        assert_eq!(config.ports.range_size, 10);
    }

    #[test]
    fn restart_policy_serde_kebab_case() {
        #[derive(Debug, Deserialize, Serialize)]
        struct Wrapper {
            policy: RestartPolicy,
        }

        let cases = [
            (r#"policy = "on-failure""#, RestartPolicy::OnFailure),
            (r#"policy = "always""#, RestartPolicy::Always),
            (r#"policy = "never""#, RestartPolicy::Never),
        ];

        for (toml_str, expected) in &cases {
            let w: Wrapper = toml::from_str(toml_str).unwrap();
            assert_eq!(std::mem::discriminant(&w.policy), std::mem::discriminant(expected));
        }

        let w = Wrapper { policy: RestartPolicy::OnFailure };
        let serialized = toml::to_string(&w).unwrap();
        assert!(serialized.contains("on-failure"), "serialized: {serialized}");

        let w = Wrapper { policy: RestartPolicy::Always };
        let serialized = toml::to_string(&w).unwrap();
        assert!(serialized.contains("always"), "serialized: {serialized}");

        let w = Wrapper { policy: RestartPolicy::Never };
        let serialized = toml::to_string(&w).unwrap();
        assert!(serialized.contains("never"), "serialized: {serialized}");
    }

    #[test]
    fn resolve_repo_path_tilde_expansion() {
        let home = dirs::home_dir().unwrap();

        let result = resolve_repo_path("~/code/myapp");
        assert_eq!(result, home.join("code/myapp"));

        let result = resolve_repo_path("~");
        assert_eq!(result, home);
    }

    #[test]
    fn resolve_repo_path_absolute() {
        let result = resolve_repo_path("/opt/repos/myapp");
        assert_eq!(result, PathBuf::from("/opt/repos/myapp"));
    }

    #[test]
    fn resolve_repo_path_relative() {
        let result = resolve_repo_path("repos/myapp");
        assert_eq!(result, PathBuf::from("repos/myapp"));
    }

    #[test]
    fn service_process_default_cwd() {
        let toml_str = r#"
[project]
name = "test"
repo = "/tmp/test"

[services]
processes = [
    { name = "svc", command = "echo hi" },
]
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        let svc = &config.services.unwrap().processes[0];
        assert_eq!(svc.cwd, ".");
        assert!(matches!(svc.restart, RestartPolicy::Never));
        assert!(svc.stop_command.is_none());
    }

    #[test]
    fn global_config_default_trait() {
        let config = GlobalConfig::default();
        assert_eq!(config.defaults.terminal, "ghostty");
        assert_eq!(config.defaults.editor, "nvim");
        assert_eq!(config.defaults.shell, "zsh");
        assert_eq!(config.ports.base, 3000);
        assert_eq!(config.ports.range_size, 10);
    }

    #[test]
    fn events_config_defaults() {
        let config = EventsConfig::default();
        assert_eq!(config.buffer_size, 200);
        assert_eq!(config.replay_on_subscribe, 20);
    }

    #[test]
    fn events_config_serde() {
        let toml_str = r#"
buffer_size = 500
replay_on_subscribe = 50
"#;
        let config: EventsConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.buffer_size, 500);
        assert_eq!(config.replay_on_subscribe, 50);
    }

    #[test]
    fn global_config_with_events() {
        let toml_str = r#"
[events]
buffer_size = 100
replay_on_subscribe = 10
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.events.buffer_size, 100);
        assert_eq!(config.events.replay_on_subscribe, 10);
        assert_eq!(config.defaults.terminal, "ghostty");
        assert_eq!(config.ports.base, 3000);
    }

    #[test]
    fn save_and_load_project_config_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let name = "roundtrip-test";
        let config_path = dir.path().join(format!("{name}.toml"));

        let config = ProjectConfig {
            project: ProjectMeta {
                name: name.into(),
                repo: "/tmp/test".into(),
                folder: Some("dev".into()),
                icon: None,
            },
            env: EnvConfig::default(),
            git: None,
            ports: None,
            services: Some(ServicesConfig {
                processes: vec![ServiceProcess {
                    name: "api".into(),
                    command: "npm start".into(),
                    cwd: ".".into(),
                    restart: RestartPolicy::OnFailure,
                    stop_command: None,
                    agent: None,
                    prompt: None,
                    agent_mode: "oneshot".into(),
                    agent_model: None,
                    agent_permissions: "full".into(),
                }],
            }),
            windows: vec![WindowConfig { name: Some("editor".into()), command: Some("nvim .".into()) }],
            scratchpad: None,
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&config_path, &toml_str).unwrap();
        let loaded: ProjectConfig = toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();

        assert_eq!(loaded.project.name, name);
        assert_eq!(loaded.project.repo, "/tmp/test");
        assert_eq!(loaded.project.folder.as_deref(), Some("dev"));
        assert_eq!(loaded.services.unwrap().processes[0].name, "api");
        assert_eq!(loaded.windows[0].name.as_deref(), Some("editor"));
    }

    #[test]
    fn config_with_env_vars_roundtrip() {
        let mut config = ProjectConfig {
            project: ProjectMeta {
                name: "env-test".into(),
                repo: "/tmp".into(),
                folder: None,
                icon: None,
            },
            env: EnvConfig::default(),
            git: None,
            ports: None,
            services: None,
            windows: vec![],
            scratchpad: None,
        };
        config.env.vars.insert("NODE_ENV".into(), "development".into());
        config.env.vars.insert("PORT".into(), "3000".into());

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let loaded: ProjectConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(loaded.env.vars.get("NODE_ENV").unwrap(), "development");
        assert_eq!(loaded.env.vars.get("PORT").unwrap(), "3000");
    }

    #[test]
    fn config_with_ports_roundtrip() {
        let config = ProjectConfig {
            project: ProjectMeta {
                name: "ports-test".into(),
                repo: "/tmp".into(),
                folder: None,
                icon: None,
            },
            env: EnvConfig::default(),
            git: None,
            ports: Some(ProjectPorts {
                range: Some([3000, 3010]),
                named: [("api".into(), 3001), ("web".into(), 3002)].into_iter().collect(),
            }),
            services: None,
            windows: vec![],
            scratchpad: None,
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let loaded: ProjectConfig = toml::from_str(&toml_str).unwrap();

        let ports = loaded.ports.unwrap();
        assert_eq!(ports.range, Some([3000, 3010]));
        assert_eq!(*ports.named.get("api").unwrap(), 3001);
        assert_eq!(*ports.named.get("web").unwrap(), 3002);
    }

    #[test]
    fn resolve_current_project_explicit() {
        let result = resolve_current_project(Some("myapp")).unwrap();
        assert_eq!(result, "myapp");
    }

    #[test]
    fn config_services_none_when_empty_processes() {
        let mut config = ProjectConfig {
            project: ProjectMeta {
                name: "svc-test".into(),
                repo: "/tmp".into(),
                folder: None,
                icon: None,
            },
            env: EnvConfig::default(),
            git: None,
            ports: None,
            services: Some(ServicesConfig {
                processes: vec![ServiceProcess {
                    name: "api".into(),
                    command: "run".into(),
                    cwd: ".".into(),
                    restart: RestartPolicy::Never,
                    stop_command: None,
                    agent: None,
                    prompt: None,
                    agent_mode: "oneshot".into(),
                    agent_model: None,
                    agent_permissions: "full".into(),
                }],
            }),
            windows: vec![],
            scratchpad: None,
        };

        // Remove the service
        if let Some(services) = &mut config.services {
            services.processes.retain(|p| p.name != "api");
            if services.processes.is_empty() {
                config.services = None;
            }
        }
        assert!(config.services.is_none());
    }

    #[test]
    fn agent_service_roundtrip() {
        let config = ProjectConfig {
            project: ProjectMeta {
                name: "agent-test".into(),
                repo: "/tmp".into(),
                folder: None,
                icon: None,
            },
            env: EnvConfig::default(),
            git: None,
            ports: None,
            services: Some(ServicesConfig {
                processes: vec![ServiceProcess {
                    name: "reviewer".into(),
                    command: String::new(),
                    cwd: ".".into(),
                    restart: RestartPolicy::OnFailure,
                    stop_command: None,
                    agent: Some("claude".into()),
                    prompt: Some("Review code".into()),
                    agent_mode: "oneshot".into(),
                    agent_model: Some("opus".into()),
                    agent_permissions: "safe".into(),
                }],
            }),
            windows: vec![],
            scratchpad: None,
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let loaded: ProjectConfig = toml::from_str(&toml_str).unwrap();

        let svc = &loaded.services.unwrap().processes[0];
        assert_eq!(svc.name, "reviewer");
        assert_eq!(svc.agent.as_deref(), Some("claude"));
        assert_eq!(svc.prompt.as_deref(), Some("Review code"));
        assert_eq!(svc.agent_model.as_deref(), Some("opus"));
        assert_eq!(svc.agent_permissions, "safe");
        assert!(matches!(svc.restart, RestartPolicy::OnFailure));
    }
}
