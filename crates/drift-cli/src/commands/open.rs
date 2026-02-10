use std::fs;
use std::process::Command;
use std::time::Duration;

use anyhow::Context;
use drift_core::{config, env, kdl, niri, paths, registry};

pub fn run(name: &str) -> anyhow::Result<()> {
    let project = config::load_project_config(name)?;
    let global = config::load_global_config()?;
    let mut niri_client = niri::NiriClient::connect()?;

    // Hot path: workspace already exists, just focus it
    if niri_client.find_workspace_by_name(name)?.is_some() {
        niri_client.focus_workspace(name)?;
        println!("Focused existing workspace '{name}'");
        return Ok(());
    }

    // Cold boot
    // Regenerate niri-rules.kdl so the workspace is declared statically
    let all_projects = registry::list_projects()?;
    kdl::write_niri_rules(&all_projects, &global)?;

    // Brief sleep to let niri live-reload the config
    std::thread::sleep(Duration::from_millis(200));

    // Focus the workspace
    niri_client.focus_workspace(name)?;

    // Build environment
    let env_vars = env::build_env(&project)?;

    let repo_path = config::resolve_repo_path(&project.project.repo);

    // Set git identity if configured
    if let Some(git) = &project.git {
        if let Some(user_name) = &git.user_name {
            Command::new("git")
                .args(["config", "--local", "user.name", user_name])
                .current_dir(&repo_path)
                .status()
                .context("setting git user.name")?;
        }
        if let Some(user_email) = &git.user_email {
            Command::new("git")
                .args(["config", "--local", "user.email", user_email])
                .current_dir(&repo_path)
                .status()
                .context("setting git user.email")?;
        }
    }

    // Spawn services via supervisor
    if project.services.is_some() {
        let state_dir = paths::state_dir(name);
        fs::create_dir_all(&state_dir).context("creating state directory")?;
        let logs_dir = paths::logs_dir(name);
        fs::create_dir_all(&logs_dir).context("creating logs directory")?;

        // Check for existing supervisor
        let supervisor_pid_path = paths::supervisor_pid_path(name);
        let supervisor_running = if supervisor_pid_path.exists() {
            if let Ok(pid_str) = fs::read_to_string(&supervisor_pid_path) {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        if supervisor_running {
            println!("  Supervisor already running");
        } else {
            // Clean up stale PID file
            let _ = fs::remove_file(&supervisor_pid_path);

            let drift_bin = std::env::current_exe()
                .context("determining drift binary path")?;

            let supervisor_log = logs_dir.join("supervisor.log");
            let log_file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&supervisor_log)
                .context("creating supervisor log")?;
            let stderr_file = log_file.try_clone()?;

            Command::new(&drift_bin)
                .args(["_supervisor", name])
                .stdout(log_file)
                .stderr(stderr_file)
                .stdin(std::process::Stdio::null())
                .spawn()
                .context("spawning supervisor")?;

            // Brief wait for supervisor to start
            std::thread::sleep(Duration::from_millis(300));

            if supervisor_pid_path.exists() {
                let pid = fs::read_to_string(&supervisor_pid_path)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                println!("  Started supervisor (PID {pid})");
            } else {
                eprintln!("  Warning: supervisor may not have started (check logs/supervisor.log)");
            }
        }
    }

    // Spawn terminal windows via niri
    let terminal = &global.defaults.terminal;
    let export_str = env::format_env_exports(&env_vars);
    let repo_str = repo_path.to_string_lossy();

    if project.windows.is_empty() {
        let full_cmd = build_terminal_command(terminal, name, &export_str, &repo_str, None);
        niri_client.spawn(vec!["sh".into(), "-c".into(), full_cmd])?;
        println!("  Spawned default terminal window");
    } else {
        for window in &project.windows {
            let cmd = window.command.as_deref().filter(|c| !c.is_empty());
            let full_cmd = build_terminal_command(terminal, name, &export_str, &repo_str, cmd);
            niri_client.spawn(vec!["sh".into(), "-c".into(), full_cmd])?;

            let label = window
                .name
                .as_deref()
                .or(window.command.as_deref())
                .unwrap_or("shell");
            println!("  Spawned window '{label}'");
        }
    }

    drift_core::events::try_emit_event(&drift_core::events::Event {
        event_type: "drift.project.opened".into(),
        project: name.to_string(),
        source: "drift".into(),
        ts: drift_core::events::iso_now(),
        level: Some("info".into()),
        title: Some(format!("Opened project '{name}'")),
        body: None,
        meta: None,
        priority: None,
    });

    println!("Opened project '{name}'");
    Ok(())
}

fn build_terminal_command(
    terminal: &str,
    project_name: &str,
    export_str: &str,
    repo_path: &str,
    command: Option<&str>,
) -> String {
    let title_flag = match terminal {
        "foot" => format!("--title \"drift:{project_name}\""),
        "ghostty" => format!("--title=\"drift:{project_name}\""),
        _ => format!("--title \"drift:{project_name}\""),
    };

    let exec_flag = match command {
        Some(cmd) => format!(" -e {cmd}"),
        None => String::new(),
    };

    format!(
        "{export_str} && cd {repo_path} && exec {terminal} {title_flag}{exec_flag}"
    )
}

