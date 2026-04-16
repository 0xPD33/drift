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

    fn state_dir(&self, project: &str) -> PathBuf {
        self.state_dir.path().join("drift").join(project)
    }

    fn task_queue_path(&self, project: &str) -> PathBuf {
        self.state_dir(project).join("tasks.json")
    }

    fn handoff_dir(&self, project: &str) -> PathBuf {
        self.state_dir(project).join("handoffs")
    }

    fn handoff_path(&self, project: &str, task_id: &str) -> PathBuf {
        self.handoff_dir(project).join(format!("{}.md", task_id))
    }

    fn read_tasks_json(&self, project: &str) -> String {
        std::fs::read_to_string(self.task_queue_path(project)).unwrap()
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

// ── Task Queue ──

fn extract_task_id(output: &str) -> String {
    output
        .lines()
        .flat_map(|l| l.split_whitespace())
        .find(|w| w.starts_with("tsk-"))
        .expect("no task ID found in output")
        .to_string()
}

#[test]
fn task_add_creates_task() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let out = t.stdout(&["task", "add", "myapp", "Fix the login bug"]);
    assert!(out.contains("tsk-"));

    let list = t.stdout(&["task", "list", "myapp"]);
    assert!(list.contains("Fix the login bug"));
    assert!(list.contains("queued"));
}

#[test]
fn task_add_with_priority_and_component() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&[
        "task", "add", "myapp", "High priority task", "--priority", "1", "--component", "auth",
    ]);
    let list = t.stdout(&["task", "list", "myapp"]);
    assert!(list.contains("High priority task"));
    assert!(list.contains("1"));
}

#[test]
fn task_list_empty() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let out = t.stdout(&["task", "list", "myapp"]);
    assert!(out.contains("No tasks"));
}

#[test]
fn task_next_shows_highest_priority() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["task", "add", "myapp", "Low priority", "--priority", "5"]);
    t.run_ok(&["task", "add", "myapp", "High priority", "--priority", "1"]);
    let out = t.stdout(&["task", "next", "myapp"]);
    assert!(out.contains("High priority"));
}

#[test]
fn task_next_empty() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let out = t.stdout(&["task", "next", "myapp"]);
    assert!(out.contains("No tasks pending"));
}

#[test]
fn task_cancel_removes_task() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let out = t.stdout(&["task", "add", "myapp", "To be cancelled"]);
    let task_id = extract_task_id(&out);

    t.run_ok(&["task", "cancel", &task_id]);
    let list = t.stdout(&["task", "list", "myapp"]);
    assert!(!list.contains("To be cancelled"));
}

#[test]
fn task_complete_marks_done() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let out = t.stdout(&["task", "add", "myapp", "To complete"]);
    let task_id = extract_task_id(&out);

    // complete requires Running status, so set it manually
    let json = t.read_tasks_json("myapp");
    let updated = json.replace("\"queued\"", "\"running\"");
    std::fs::write(t.task_queue_path("myapp"), &updated).unwrap();

    t.run_ok(&["task", "complete", &task_id]);
    let list = t.stdout(&["task", "list", "--all", "myapp"]);
    assert!(list.contains("completed"));
}

#[test]
fn task_complete_rejects_non_running() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let out = t.stdout(&["task", "add", "myapp", "Still queued"]);
    let task_id = extract_task_id(&out);

    // task is queued, complete should fail
    t.run_fail(&["task", "complete", &task_id]);
}

#[test]
fn task_fail_with_reason() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let out = t.stdout(&["task", "add", "myapp", "Will fail"]);
    let task_id = extract_task_id(&out);

    // fail requires Running status
    let json = t.read_tasks_json("myapp");
    let updated = json.replace("\"queued\"", "\"running\"");
    std::fs::write(t.task_queue_path("myapp"), &updated).unwrap();

    t.run_ok(&["task", "fail", &task_id, "--reason", "broken"]);
    let list = t.stdout(&["task", "list", "--all", "myapp"]);
    assert!(list.contains("failed"));
}

#[test]
fn task_fail_rejects_non_running() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let out = t.stdout(&["task", "add", "myapp", "Still queued"]);
    let task_id = extract_task_id(&out);

    t.run_fail(&["task", "fail", &task_id]);
}

#[test]
fn task_list_json_output() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["task", "add", "myapp", "JSON task"]);
    let out = t.stdout(&["task", "list", "myapp", "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["description"].as_str().unwrap(), "JSON task");
    assert_eq!(arr[0]["status"].as_str().unwrap(), "queued");
}

#[test]
fn task_list_status_filter() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["task", "add", "myapp", "Queued task"]);
    let out2 = t.stdout(&["task", "add", "myapp", "Running task"]);
    let running_id = extract_task_id(&out2);

    // Set second task to running
    let json = t.read_tasks_json("myapp");
    let mut tasks: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    for task in &mut tasks {
        if task["id"].as_str() == Some(&running_id) {
            task["status"] = serde_json::json!("running");
        }
    }
    std::fs::write(
        t.task_queue_path("myapp"),
        serde_json::to_string_pretty(&tasks).unwrap(),
    )
    .unwrap();

    let out = t.stdout(&["task", "list", "myapp", "--status", "running"]);
    assert!(out.contains("Running task"));
    assert!(!out.contains("Queued task"));
}

// ── Task Chain ──

#[test]
fn task_chain_creates_linked_tasks() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let out = t.stdout(&[
        "task", "chain", "myapp", "Step one", "Step two", "Step three",
    ]);
    assert!(out.contains("→"));
    assert!(out.contains("Step one"));
    assert!(out.contains("Step two"));
    assert!(out.contains("Step three"));

    let json = t.read_tasks_json("myapp");
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    assert_eq!(tasks.len(), 3);
    assert!(tasks[0]["parent_task"].is_null());
    assert!(!tasks[1]["parent_task"].is_null());
    assert!(!tasks[2]["parent_task"].is_null());
    assert_eq!(
        tasks[1]["parent_task"].as_str().unwrap(),
        tasks[0]["id"].as_str().unwrap()
    );
    assert_eq!(
        tasks[2]["parent_task"].as_str().unwrap(),
        tasks[1]["id"].as_str().unwrap()
    );
}

#[test]
fn task_chain_with_component() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&[
        "task", "chain", "myapp", "Step one", "Step two", "--component", "auth",
    ]);
    let json = t.read_tasks_json("myapp");
    let count = json.matches("\"auth\"").count();
    assert!(count >= 2);
}

#[test]
fn task_chain_empty_fails() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_fail(&["task", "chain", "myapp"]);
}

#[test]
fn task_chain_next_respects_parent_dependency() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["task", "chain", "myapp", "First", "Second"]);

    // next should return First (Second is blocked by parent)
    let out = t.stdout(&["task", "next", "myapp"]);
    assert!(out.contains("First"));
    assert!(!out.contains("Second"));
}

// ── Review CLI ──

#[test]
fn review_list_empty() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let out = t.stdout(&["review", "list"]);
    assert!(out.contains("No tasks pending review"));
}

#[test]
fn review_approve_completes_task() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);

    let out = t.stdout(&["task", "add", "myapp", "Reviewable task"]);
    let task_id = extract_task_id(&out);

    // Set task to needs-review (must go through running first for the queue methods,
    // but we can write JSON directly)
    let json = t.read_tasks_json("myapp");
    let updated = json.replace("\"queued\"", "\"needs-review\"");
    std::fs::write(t.task_queue_path("myapp"), &updated).unwrap();

    t.run_ok(&["review", "approve", &task_id]);

    let json_after = t.read_tasks_json("myapp");
    assert!(json_after.contains("\"completed\""));
}

#[test]
fn review_reject_fails_task() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);

    let out = t.stdout(&["task", "add", "myapp", "Rejectable task"]);
    let task_id = extract_task_id(&out);

    let json = t.read_tasks_json("myapp");
    let updated = json.replace("\"queued\"", "\"needs-review\"");
    std::fs::write(t.task_queue_path("myapp"), &updated).unwrap();

    t.run_ok(&["review", "reject", &task_id, "--reason", "Not good enough"]);

    let json_after = t.read_tasks_json("myapp");
    assert!(json_after.contains("\"failed\""));
}

#[test]
fn review_approve_rejects_non_review_status() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);

    let out = t.stdout(&["task", "add", "myapp", "Queued task"]);
    let task_id = extract_task_id(&out);

    // Task is queued, not needs-review
    t.run_fail(&["review", "approve", &task_id]);
}

// ── Post-dispatch / Handoff ──

#[test]
fn post_dispatch_fails_on_missing_handoff() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);

    let out = t.stdout(&["task", "add", "myapp", "No handoff task"]);
    let task_id = extract_task_id(&out);

    // Set to running
    let json = t.read_tasks_json("myapp");
    let updated = json.replace("\"queued\"", "\"running\"");
    std::fs::write(t.task_queue_path("myapp"), &updated).unwrap();

    // Run post-dispatch without writing a handoff file
    t.run_ok(&["_post-dispatch", "myapp", &task_id]);

    // Task should be failed with "no-handoff" reason
    let json = t.read_tasks_json("myapp");
    assert!(json.contains("\"failed\""));
    assert!(json.contains("no-handoff"));
}

#[test]
fn post_dispatch_needs_review_handoff() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);

    let out = t.stdout(&["task", "add", "myapp", "Review task"]);
    let task_id = extract_task_id(&out);

    let json = t.read_tasks_json("myapp");
    let updated = json.replace("\"queued\"", "\"running\"");
    std::fs::write(t.task_queue_path("myapp"), &updated).unwrap();

    // Write handoff with needs-review status
    let handoff_dir = t.handoff_dir("myapp");
    std::fs::create_dir_all(&handoff_dir).unwrap();
    let handoff_content = format!(
        "---\ntask_id: {task_id}\nstatus: needs-review\nagent: test-agent\nfiles_changed: []\n---\n\n## What was done\nChanged the API surface, needs human review.\n"
    );
    std::fs::write(t.handoff_path("myapp", &task_id), &handoff_content).unwrap();

    t.run_ok(&["_post-dispatch", "myapp", &task_id]);

    let json = t.read_tasks_json("myapp");
    assert!(json.contains("\"needs-review\""));
}

#[test]
fn post_dispatch_failed_handoff() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);

    let out = t.stdout(&["task", "add", "myapp", "Failing task"]);
    let task_id = extract_task_id(&out);

    let json = t.read_tasks_json("myapp");
    let updated = json.replace("\"queued\"", "\"running\"");
    std::fs::write(t.task_queue_path("myapp"), &updated).unwrap();

    let handoff_dir = t.handoff_dir("myapp");
    std::fs::create_dir_all(&handoff_dir).unwrap();
    let handoff_content = format!(
        "---\ntask_id: {task_id}\nstatus: failed\nagent: test-agent\nfiles_changed: []\n---\n\n## What was done\nCould not complete the task.\n"
    );
    std::fs::write(t.handoff_path("myapp", &task_id), &handoff_content).unwrap();

    t.run_ok(&["_post-dispatch", "myapp", &task_id]);

    let json = t.read_tasks_json("myapp");
    assert!(json.contains("\"failed\""));
}

#[test]
fn post_dispatch_completed_handoff() {
    let t = TestEnv::new();
    // Use a tempdir for repo so resolve_repo_path works
    let repo_dir = tempfile::tempdir().unwrap();
    t.run_ok(&["init", "myapp", repo_dir.path().to_str().unwrap()]);

    let out = t.stdout(&["task", "add", "myapp", "Test task"]);
    let task_id = extract_task_id(&out);

    let json = t.read_tasks_json("myapp");
    let updated = json.replace("\"queued\"", "\"running\"");
    std::fs::write(t.task_queue_path("myapp"), &updated).unwrap();

    let handoff_dir = t.handoff_dir("myapp");
    std::fs::create_dir_all(&handoff_dir).unwrap();
    let handoff_content = format!(
        "---\ntask_id: {task_id}\nstatus: completed\nagent: test-agent\nfiles_changed:\n  - src/main.rs\ntests_run: 5\ntests_passed: 5\ntests_failed: 0\n---\n\n## What was done\nFixed the bug.\n"
    );
    std::fs::write(t.handoff_path("myapp", &task_id), &handoff_content).unwrap();

    t.run_ok(&["_post-dispatch", "myapp", &task_id]);

    let json = t.read_tasks_json("myapp");
    assert!(json.contains("\"completed\""));
    assert!(!json.contains("\"running\""));
}

// ── Dispatch dry-run ──

#[test]
fn dispatch_dry_run_shows_prompt() {
    let t = TestEnv::new();
    let repo_dir = tempfile::tempdir().unwrap();
    t.run_ok(&["init", "myapp", repo_dir.path().to_str().unwrap()]);
    t.run_ok(&["task", "add", "myapp", "Build the feature"]);

    let out = t.stdout(&["dispatch", "myapp", "--dry-run"]);
    assert!(out.contains("Build the feature"));
    assert!(out.contains("Dry Run"));
}

#[test]
fn dispatch_no_tasks_reports_empty() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let out = t.stdout(&["dispatch", "myapp", "--dry-run"]);
    assert!(out.contains("No tasks pending"));
}

// ── Task Assign ──

#[test]
fn task_assign_dry_run_shows_prompt() {
    let t = TestEnv::new();
    let repo_dir = tempfile::tempdir().unwrap();
    t.run_ok(&["init", "myapp", repo_dir.path().to_str().unwrap()]);
    let out = t.stdout(&["task", "add", "myapp", "Specific task to assign"]);
    let task_id = extract_task_id(&out);

    let assign_out = t.stdout(&["task", "assign", &task_id, "--dry-run"]);
    assert!(assign_out.contains("Specific task to assign"));
    assert!(assign_out.contains(&task_id));
}

#[test]
fn task_assign_unknown_id_fails() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let err = t.stderr_fail(&["task", "assign", "tsk-doesnotexist", "--dry-run"]);
    assert!(err.contains("not found") || err.to_lowercase().contains("not found"));
}

#[test]
fn task_assign_rejects_non_queued() {
    let t = TestEnv::new();
    let repo_dir = tempfile::tempdir().unwrap();
    t.run_ok(&["init", "myapp", repo_dir.path().to_str().unwrap()]);
    let out = t.stdout(&["task", "add", "myapp", "Already running"]);
    let task_id = extract_task_id(&out);

    // Manually set the task to running
    let json = t.read_tasks_json("myapp");
    let updated = json.replace("\"queued\"", "\"running\"");
    std::fs::write(t.task_queue_path("myapp"), &updated).unwrap();

    let err = t.stderr_fail(&["task", "assign", &task_id, "--dry-run"]);
    assert!(
        err.contains("queued") || err.contains("running") || err.contains("not"),
        "expected status guard error, got: {err}"
    );
}

#[test]
fn task_assign_with_agent_override() {
    let t = TestEnv::new();
    let repo_dir = tempfile::tempdir().unwrap();
    t.run_ok(&["init", "myapp", repo_dir.path().to_str().unwrap()]);
    let out = t.stdout(&["task", "add", "myapp", "Use specific agent"]);
    let task_id = extract_task_id(&out);

    let assign_out = t.stdout(&["task", "assign", &task_id, "--agent", "codex", "--dry-run"]);
    assert!(assign_out.contains("codex"));
}

// ── Worktree commands ──

#[test]
fn task_worktrees_empty() {
    let t = TestEnv::new();
    let repo_dir = tempfile::tempdir().unwrap();
    // Initialize a git repo so worktree commands have something to scan
    std::process::Command::new("git")
        .args(["init", repo_dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    t.run_ok(&["init", "myapp", repo_dir.path().to_str().unwrap()]);
    let out = t.stdout(&["task", "worktrees", "myapp"]);
    // Empty list — should run without error and report no worktrees
    assert!(
        out.contains("No worktrees")
            || out.contains("no worktrees")
            || out.trim().is_empty()
            || !out.contains("tsk-"),
        "unexpected output: {out}"
    );
}

#[test]
fn task_clean_worktrees_empty() {
    let t = TestEnv::new();
    let repo_dir = tempfile::tempdir().unwrap();
    std::process::Command::new("git")
        .args(["init", repo_dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    t.run_ok(&["init", "myapp", repo_dir.path().to_str().unwrap()]);
    // Should not fail even with no worktrees to clean
    t.run_ok(&["task", "clean-worktrees", "myapp"]);
}

// ── Shell-data JSON shape ──

#[test]
fn shell_data_outputs_workspaces_field() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);

    let out = t.stdout(&["shell-data"]);
    let parsed: serde_json::Value = serde_json::from_str(&out)
        .expect("shell-data should produce valid JSON");

    // Verify all the new top-level fields exist
    assert!(parsed.get("workspaces").is_some(), "missing workspaces field");
    assert!(parsed.get("focus").is_some(), "missing focus field");
    assert!(parsed.get("review_queue").is_some(), "missing review_queue field");
    assert!(parsed.get("global").is_some(), "missing global field");
    assert!(parsed.get("folders").is_some(), "missing folders field");

    // workspaces should be an array (possibly empty if niri isn't available)
    assert!(parsed["workspaces"].is_array());

    // global should have the count fields
    let global = &parsed["global"];
    assert!(global.get("total_agents_running").is_some());
    assert!(global.get("total_tasks_queued").is_some());
    assert!(global.get("total_reviews_pending").is_some());

    // focus should have mode field
    assert!(parsed["focus"].get("mode").is_some());
}

#[test]
fn shell_data_includes_task_counts_per_project() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    t.run_ok(&["task", "add", "myapp", "Task one"]);
    t.run_ok(&["task", "add", "myapp", "Task two"]);

    let out = t.stdout(&["shell-data"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    // global.total_tasks_queued should be 2
    assert_eq!(parsed["global"]["total_tasks_queued"], 2);

    // The folder containing myapp should have task counts
    let folders = &parsed["folders"];
    let ungrouped = &folders["_ungrouped"];
    assert!(ungrouped.is_array());
    let myapp = ungrouped
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "myapp")
        .expect("myapp not in folders");
    assert_eq!(myapp["tasks"]["queued"], 2);
}

#[test]
fn shell_data_review_queue_includes_needs_review_tasks() {
    let t = TestEnv::new();
    t.run_ok(&["init", "myapp"]);
    let out = t.stdout(&["task", "add", "myapp", "Will need review"]);
    let task_id = extract_task_id(&out);

    // Set to needs-review
    let json = t.read_tasks_json("myapp");
    let updated = json.replace("\"queued\"", "\"needs-review\"");
    std::fs::write(t.task_queue_path("myapp"), &updated).unwrap();

    let shell_out = t.stdout(&["shell-data"]);
    let parsed: serde_json::Value = serde_json::from_str(&shell_out).unwrap();

    let reviews = parsed["review_queue"].as_array().unwrap();
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0]["task_id"], task_id);
    assert_eq!(reviews[0]["project"], "myapp");
    assert_eq!(parsed["global"]["total_reviews_pending"], 1);
}
