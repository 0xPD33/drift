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
    pub command: String,
    #[serde(default = "default_cwd")]
    pub cwd: String,
    #[serde(default)]
    pub restart: RestartPolicy,
    pub stop_command: Option<String>,
}

fn default_cwd() -> String {
    ".".into()
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
}
