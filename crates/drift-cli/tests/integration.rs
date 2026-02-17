use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

struct TestEnv {
    config_dir: TempDir,
    state_dir: TempDir,
}

impl TestEnv {
    fn new() -> Self {
        Self {
            config_dir: TempDir::new().unwrap(),
            state_dir: TempDir::new().unwrap(),
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_drift"));
        cmd.env("XDG_CONFIG_HOME", self.config_dir.path());
        cmd.env("XDG_STATE_HOME", self.state_dir.path());
        cmd.env_remove("DRIFT_PROJECT");
        cmd
    }

    fn project_config_path(&self, name: &str) -> PathBuf {
        self.config_dir
            .path()
            .join("drift")
            .join("projects")
            .join(format!("{name}.toml"))
    }

    fn read_config(&self, name: &str) -> String {
        std::fs::read_to_string(self.project_config_path(name)).unwrap()
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        self.cmd().args(args).output().unwrap()
    }

    fn run_ok(&self, args: &[&str]) -> std::process::Output {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "Command {:?} failed.\nstdout: {}\nstderr: {}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        output
    }

    fn run_fail(&self, args: &[&str]) -> std::process::Output {
        let output = self.run(args);
        assert!(
            !output.status.success(),
            "Command {:?} should have failed but succeeded.\nstdout: {}",
            args,
            String::from_utf8_lossy(&output.stdout),
        );
        output
    }

    fn stdout(&self, args: &[&str]) -> String {
        let output = self.run_ok(args);
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn stderr_fail(&self, args: &[&str]) -> String {
        let output = self.run_fail(args);
        String::from_utf8_lossy(&output.stderr).to_string()
    }
}

// ── Init ──

#[test]
fn init_creates_project() {
    let t = TestEnv::new();
    let out = t.stdout(&["init", "myapp"]);
    assert!(out.contains("Initialized project 'myapp'"));
    assert!(t.project_config_path("myapp").exists());

    let cfg = t.read_config("myapp");
    assert!(cfg.contains("name = \"myapp\""));
}

#[test]
fn init_with_explicit_repo() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp", "/tmp/myrepo"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("repo = \"/tmp/myrepo\""));
}

#[test]
fn init_with_folder() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp", "--folder", "work"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("folder = \"work\""));
}

#[test]
fn init_duplicate_fails() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let err = t.stderr_fail(&["init", "myapp"]);
    assert!(err.contains("already exists"));
}

#[test]
fn init_with_template() {
    let t = TestEnv::new();
    let templates_dir = t.config_dir.path().join("drift").join("templates");
    std::fs::create_dir_all(&templates_dir).unwrap();
    std::fs::write(
        templates_dir.join("rust.toml"),
        r#"
[project]
name = "placeholder"
repo = "/placeholder"

[env]
RUST_LOG = "debug"

[[windows]]
name = "editor"
command = "nvim ."

[[windows]]
name = "shell"
"#,
    )
    .unwrap();

    t.run_ok(&["init", "myapp", "/tmp/myrepo", "--template", "rust"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("name = \"myapp\""));
    assert!(cfg.contains("repo = \"/tmp/myrepo\""));
    assert!(cfg.contains("RUST_LOG = \"debug\""));
    assert!(cfg.contains("name = \"editor\""));
}

#[test]
fn init_template_overrides_folder() {
    let t = TestEnv::new();
    let templates_dir = t.config_dir.path().join("drift").join("templates");
    std::fs::create_dir_all(&templates_dir).unwrap();
    std::fs::write(
        templates_dir.join("base.toml"),
        r#"
[project]
name = "placeholder"
repo = "/placeholder"
folder = "template-folder"
"#,
    )
    .unwrap();

    t.run_ok(&[
        "init", "myapp", "/tmp/repo", "--template", "base", "--folder", "override",
    ]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("folder = \"override\""));
    assert!(!cfg.contains("template-folder"));
}

#[test]
fn init_template_preserves_folder_when_not_specified() {
    let t = TestEnv::new();
    let templates_dir = t.config_dir.path().join("drift").join("templates");
    std::fs::create_dir_all(&templates_dir).unwrap();
    std::fs::write(
        templates_dir.join("base.toml"),
        r#"
[project]
name = "placeholder"
repo = "/placeholder"
folder = "template-folder"
"#,
    )
    .unwrap();

    t.run_ok(&["init", "myapp", "/tmp/repo", "--template", "base"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("folder = \"template-folder\""));
}

#[test]
fn init_missing_template_fails() {
    let t = TestEnv::new();
    let err = t.stderr_fail(&["init", "myapp", "--template", "nonexistent"]);
    assert!(err.contains("not found"));
}

// ── Add service ──

#[test]
fn add_service() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&[
        "add", "service", "api", "npm start", "--restart", "on-failure", "--project", "myapp",
    ]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("name = \"api\""));
    assert!(cfg.contains("command = \"npm start\""));
    assert!(cfg.contains("restart = \"on-failure\""));
}

#[test]
fn add_service_with_cwd() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&[
        "add", "service", "api", "npm start", "--cwd", "./backend", "--project", "myapp",
    ]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("cwd = \"./backend\""));
}

#[test]
fn add_duplicate_service_fails() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "service", "api", "npm start", "--project", "myapp"]);
    let err = t.stderr_fail(&["add", "service", "api", "npm run dev", "--project", "myapp"]);
    assert!(err.contains("already exists"));
}

// ── Add agent ──

#[test]
fn add_agent() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&[
        "add", "agent", "reviewer", "claude", "Review code", "--model", "opus", "--mode",
        "interactive", "--permissions", "safe", "--project", "myapp",
    ]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("name = \"reviewer\""));
    assert!(cfg.contains("agent = \"claude\""));
    assert!(cfg.contains("prompt = \"Review code\""));
    assert!(cfg.contains("agent_model = \"opus\""));
    assert!(cfg.contains("agent_mode = \"interactive\""));
    assert!(cfg.contains("agent_permissions = \"safe\""));
}

// ── Add window ──

#[test]
fn add_window_with_command() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "window", "editor", "nvim .", "--project", "myapp"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("name = \"editor\""));
    assert!(cfg.contains("command = \"nvim .\""));
}

#[test]
fn add_window_without_command() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "window", "shell", "--project", "myapp"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("name = \"shell\""));
}

#[test]
fn add_duplicate_window_fails() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "window", "editor", "nvim .", "--project", "myapp"]);
    let err = t.stderr_fail(&["add", "window", "editor", "--project", "myapp"]);
    assert!(err.contains("already exists"));
}

// ── Add env ──

#[test]
fn add_env() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "env", "NODE_ENV", "development", "--project", "myapp"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("NODE_ENV = \"development\""));
}

#[test]
fn add_env_overwrites() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "env", "PORT", "3000", "--project", "myapp"]);
    t.run_ok(&["add", "env", "PORT", "8080", "--project", "myapp"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("PORT = \"8080\""));
    assert!(!cfg.contains("PORT = \"3000\""));
}

// ── Add port ──

#[test]
fn add_port() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "port", "api", "3001", "--project", "myapp"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("api = 3001"));
}

#[test]
fn add_port_range() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "port-range", "3000", "3010", "--project", "myapp"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("range = [") && cfg.contains("3000") && cfg.contains("3010"));
}

// ── Remove service ──

#[test]
fn remove_service() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "service", "api", "npm start", "--project", "myapp"]);
    t.run_ok(&["add", "service", "worker", "npm run worker", "--project", "myapp"]);
    t.run_ok(&["remove", "service", "api", "--project", "myapp"]);

    let cfg = t.read_config("myapp");
    assert!(!cfg.contains("name = \"api\""));
    assert!(cfg.contains("name = \"worker\""));
}

#[test]
fn remove_last_service_clears_section() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "service", "api", "npm start", "--project", "myapp"]);
    t.run_ok(&["remove", "service", "api", "--project", "myapp"]);

    let cfg = t.read_config("myapp");
    assert!(!cfg.contains("[services]"));
    assert!(!cfg.contains("[[services"));
}

#[test]
fn remove_nonexistent_service_fails() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let err = t.stderr_fail(&["remove", "service", "ghost", "--project", "myapp"]);
    assert!(err.contains("No services") || err.contains("not found"));
}

// ── Remove agent (same as service) ──

#[test]
fn remove_agent_removes_service() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&[
        "add", "agent", "reviewer", "claude", "Review code", "--project", "myapp",
    ]);
    t.run_ok(&["remove", "agent", "reviewer", "--project", "myapp"]);

    let cfg = t.read_config("myapp");
    assert!(!cfg.contains("name = \"reviewer\""));
}

// ── Remove window ──

#[test]
fn remove_window() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "window", "editor", "nvim .", "--project", "myapp"]);
    t.run_ok(&["remove", "window", "editor", "--project", "myapp"]);

    let cfg = t.read_config("myapp");
    assert!(!cfg.contains("name = \"editor\""));
}

#[test]
fn remove_nonexistent_window_fails() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let err = t.stderr_fail(&["remove", "window", "ghost", "--project", "myapp"]);
    assert!(err.contains("not found"));
}

// ── Remove env ──

#[test]
fn remove_env() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "env", "NODE_ENV", "development", "--project", "myapp"]);
    t.run_ok(&["remove", "env", "NODE_ENV", "--project", "myapp"]);

    let cfg = t.read_config("myapp");
    assert!(!cfg.contains("NODE_ENV"));
}

#[test]
fn remove_nonexistent_env_fails() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let err = t.stderr_fail(&["remove", "env", "GHOST", "--project", "myapp"]);
    assert!(err.contains("not found"));
}

// ── Remove port ──

#[test]
fn remove_port() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "port", "api", "3001", "--project", "myapp"]);
    t.run_ok(&["add", "port", "web", "3002", "--project", "myapp"]);
    t.run_ok(&["remove", "port", "api", "--project", "myapp"]);

    let cfg = t.read_config("myapp");
    assert!(!cfg.contains("api = 3001"));
    assert!(cfg.contains("web = 3002"));
}

#[test]
fn remove_last_port_no_range_clears_section() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "port", "api", "3001", "--project", "myapp"]);
    t.run_ok(&["remove", "port", "api", "--project", "myapp"]);

    let cfg = t.read_config("myapp");
    assert!(!cfg.contains("[ports]"));
}

#[test]
fn remove_port_range() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "port-range", "3000", "3010", "--project", "myapp"]);
    t.run_ok(&["remove", "port-range", "--project", "myapp"]);

    let cfg = t.read_config("myapp");
    assert!(!cfg.contains("range"));
}

#[test]
fn remove_port_range_keeps_named() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "port", "api", "3001", "--project", "myapp"]);
    t.run_ok(&["add", "port-range", "3000", "3010", "--project", "myapp"]);
    t.run_ok(&["remove", "port-range", "--project", "myapp"]);

    let cfg = t.read_config("myapp");
    assert!(!cfg.contains("range"));
    assert!(cfg.contains("api = 3001"));
}

// ── Full lifecycle ──

#[test]
fn full_lifecycle() {
    let t = TestEnv::new();

    // Init with folder
    t.run_ok(&["init", "webapp", "/tmp/webapp", "--folder", "work"]);

    // Add services
    t.run_ok(&["add", "service", "api", "npm start", "--restart", "on-failure", "--project", "webapp"]);
    t.run_ok(&["add", "service", "worker", "npm run worker", "--restart", "always", "--project", "webapp"]);

    // Add agent
    t.run_ok(&["add", "agent", "reviewer", "claude", "Review PRs", "--model", "opus", "--project", "webapp"]);

    // Add windows
    t.run_ok(&["add", "window", "editor", "nvim .", "--project", "webapp"]);
    t.run_ok(&["add", "window", "logs", "tail -f app.log", "--project", "webapp"]);

    // Add env
    t.run_ok(&["add", "env", "NODE_ENV", "development", "--project", "webapp"]);
    t.run_ok(&["add", "env", "PORT", "3000", "--project", "webapp"]);

    // Add ports
    t.run_ok(&["add", "port", "api", "3001", "--project", "webapp"]);
    t.run_ok(&["add", "port-range", "3000", "3010", "--project", "webapp"]);

    // Verify full config
    let cfg = t.read_config("webapp");
    assert!(cfg.contains("name = \"webapp\""));
    assert!(cfg.contains("repo = \"/tmp/webapp\""));
    assert!(cfg.contains("folder = \"work\""));
    assert!(cfg.contains("name = \"api\""));
    assert!(cfg.contains("name = \"worker\""));
    assert!(cfg.contains("name = \"reviewer\""));
    assert!(cfg.contains("agent = \"claude\""));
    assert!(cfg.contains("name = \"editor\""));
    assert!(cfg.contains("NODE_ENV = \"development\""));
    assert!(cfg.contains("PORT = \"3000\""));
    assert!(cfg.contains("api = 3001"));
    assert!(cfg.contains("range = [") && cfg.contains("3000") && cfg.contains("3010"));

    // Remove some items
    t.run_ok(&["remove", "service", "worker", "--project", "webapp"]);
    t.run_ok(&["remove", "window", "logs", "--project", "webapp"]);
    t.run_ok(&["remove", "env", "PORT", "--project", "webapp"]);
    t.run_ok(&["remove", "port", "api", "--project", "webapp"]);

    // Verify removals and retained items
    let cfg = t.read_config("webapp");
    assert!(!cfg.contains("name = \"worker\""));
    assert!(!cfg.contains("name = \"logs\""));
    assert!(!cfg.contains("PORT"));
    assert!(!cfg.contains("api = 3001"));
    assert!(cfg.contains("name = \"api\""));
    assert!(cfg.contains("name = \"reviewer\""));
    assert!(cfg.contains("name = \"editor\""));
    assert!(cfg.contains("NODE_ENV = \"development\""));
    assert!(cfg.contains("range = [") && cfg.contains("3000") && cfg.contains("3010"));
}

// ── Tmux ──

#[test]
fn add_window_with_tmux() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "window", "editor", "nvim .", "--tmux", "--project", "myapp"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("name = \"editor\""));
    assert!(cfg.contains("command = \"nvim .\""));
    assert!(cfg.contains("tmux = true"));
}

#[test]
fn add_window_without_tmux_has_no_tmux_field() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["add", "window", "shell", "--project", "myapp"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("name = \"shell\""));
    assert!(!cfg.contains("tmux"));
}

#[test]
fn tmux_config_roundtrip() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    // Write a config with [tmux] section manually
    let config_path = t.project_config_path("myapp");
    let content = std::fs::read_to_string(&config_path).unwrap();
    let new_content = format!("{content}\n[tmux]\nkill_on_close = true\n");
    std::fs::write(&config_path, new_content).unwrap();
    // Verify it parses by adding a window (which loads + saves config)
    t.run_ok(&["add", "window", "editor", "--tmux", "--project", "myapp"]);
    let cfg = t.read_config("myapp");
    assert!(cfg.contains("kill_on_close = true"));
    assert!(cfg.contains("tmux = true"));
}
