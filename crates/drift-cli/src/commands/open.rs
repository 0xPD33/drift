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
    // Regenerate niri-rules.kdl for persistence across niri restarts
    let all_projects = registry::list_projects()?;
    kdl::write_niri_rules(&all_projects, &global)?;

    // Create a named workspace dynamically via IPC
    niri_client.create_named_workspace(name)?;

    // Build environment
    let env_vars = env::build_env(&project)?;

    // Port conflict detection
    if let Some(ports) = &project.ports {
        check_port_conflicts(name, ports, &mut niri_client);
    }

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

    // Partition windows into tmux and normal
    let (tmux_windows, normal_windows): (Vec<_>, Vec<_>) = project
        .windows
        .iter()
        .partition(|w| w.tmux == Some(true));

    // Collect (title, width) pairs for windows that need sizing after spawn
    let mut width_requests: Vec<(String, niri_ipc::SizeChange)> = Vec::new();

    if normal_windows.is_empty() && tmux_windows.is_empty() {
        let args = build_terminal_args(terminal, name, None, &export_str, &repo_str, None);
        niri_client.spawn(args)?;
        println!("  Spawned default terminal window");
    } else {
        // Spawn normal windows
        for window in &normal_windows {
            let cmd = window.command.as_deref().filter(|c| !c.is_empty());
            let wn = window.name.as_deref();
            let args = build_terminal_args(terminal, name, wn, &export_str, &repo_str, cmd);
            niri_client.spawn(args)?;

            let label = wn
                .or(window.command.as_deref())
                .unwrap_or("shell");
            println!("  Spawned window '{label}'");

            if let (Some(wn), Some(width_str)) = (wn, window.width.as_deref()) {
                if let Some(change) = niri::parse_width(width_str) {
                    let title = format!("drift:{name}/{wn}");
                    width_requests.push((title, change));
                }
            }
        }

        // Spawn tmux windows
        if !tmux_windows.is_empty() {
            spawn_tmux_windows(
                name,
                terminal,
                &export_str,
                &repo_str,
                &tmux_windows,
                &mut niri_client,
            )?;
        }
    }

    // Pre-trust the repo for Claude Code so interactive agents skip the trust dialog
    if let Err(e) = drift_core::claude_trust::ensure_claude_trust(&repo_path) {
        eprintln!("Warning: failed to pre-trust repo for Claude Code: {e}");
    }

    // Spawn interactive agents as terminal windows
    if let Some(ref services) = project.services {
        for svc in &services.processes {
            if drift_core::agent::is_interactive_agent(svc) {
                let agent_cmd = drift_core::agent::build_agent_command(svc, name);
                let args = build_terminal_args(
                    terminal,
                    name,
                    Some(&svc.name),
                    &export_str,
                    &repo_str,
                    Some(&agent_cmd),
                );
                niri_client.spawn(args)?;
                println!(
                    "  Spawned interactive agent '{}' ({})",
                    svc.name,
                    svc.agent.as_deref().unwrap_or("unknown")
                );

                if let Some(width_str) = svc.width.as_deref() {
                    if let Some(change) = niri::parse_width(width_str) {
                        let title = format!("drift:{name}/{}", svc.name);
                        width_requests.push((title, change));
                    }
                }
            }
        }
    }

    // Apply window widths via IPC (windows need time to register with niri)
    if !width_requests.is_empty() {
        apply_window_widths(&mut niri_client, &width_requests);
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

    if let Err(e) = drift_core::session::add_project(name) {
        eprintln!("  Warning: could not update session: {e}");
    }

    println!("Opened project '{name}'");
    Ok(())
}

fn tmux_session_name(project: &str) -> String {
    format!("drift:{project}")
}

fn tmux_session_exists(session: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", session])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn spawn_tmux_windows(
    project_name: &str,
    terminal: &str,
    export_str: &str,
    repo_path: &str,
    tmux_windows: &[&config::WindowConfig],
    niri_client: &mut niri::NiriClient,
) -> anyhow::Result<()> {
    let session = tmux_session_name(project_name);

    if tmux_session_exists(&session) {
        // Hot restore: session already exists, just attach
        let args = build_terminal_args(terminal, project_name, Some("tmux"), export_str, repo_path, Some(&format!("tmux attach -t '{session}'")));
        niri_client.spawn(args)?;
        println!("  Attached to existing tmux session '{session}'");
        return Ok(());
    }

    // Cold boot: create new session
    let first_window = &tmux_windows[0];
    let first_cmd = first_window.command.as_deref().unwrap_or("$SHELL");
    let first_name = first_window.name.as_deref().unwrap_or("shell");

    // Build inner script for first window
    let inner_script = format!("{export_str}\ncd {repo_path}\nexec {first_cmd}");

    // Create session with first window
    Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            &session,
            "-n",
            first_name,
            "sh",
            "-c",
            &inner_script,
        ])
        .status()
        .context("creating tmux session")?;

    // Add remaining windows
    for window in &tmux_windows[1..] {
        let cmd = window.command.as_deref().unwrap_or("$SHELL");
        let name = window.name.as_deref().unwrap_or("shell");
        let inner_script = format!("{export_str}\ncd {repo_path}\nexec {cmd}");

        Command::new("tmux")
            .args([
                "new-window",
                "-t",
                &session,
                "-n",
                name,
                "sh",
                "-c",
                &inner_script,
            ])
            .status()
            .context("creating tmux window")?;
    }

    // Spawn niri terminal that attaches to the session
    let args = build_terminal_args(terminal, project_name, Some("tmux"), export_str, repo_path, Some(&format!("tmux attach -t '{session}'")));
    niri_client.spawn(args)?;
    println!("  Created tmux session '{session}' with {} window(s)", tmux_windows.len());

    Ok(())
}

fn build_terminal_args(
    terminal: &str,
    project_name: &str,
    window_name: Option<&str>,
    export_str: &str,
    repo_path: &str,
    command: Option<&str>,
) -> Vec<String> {
    let title = match window_name {
        Some(wn) => format!("drift:{project_name}/{wn}"),
        None => format!("drift:{project_name}"),
    };
    let title_flag = format!("--title={title}");

    // Build the shell script that runs inside the terminal.
    // This ensures env vars, cwd, and the command all run in a proper shell.
    let inner_script = match command {
        Some(cmd) => format!("{export_str}\ncd {repo_path}\nexec {cmd}"),
        None => format!("{export_str}\ncd {repo_path}\nexec $SHELL"),
    };

    vec![
        terminal.into(),
        title_flag,
        "-e".into(),
        "sh".into(),
        "-c".into(),
        inner_script,
    ]
}

fn check_port_conflicts(
    project_name: &str,
    ports: &drift_core::config::ProjectPorts,
    niri_client: &mut drift_core::niri::NiriClient,
) {
    let other_projects = match registry::list_projects() {
        Ok(projects) => projects,
        Err(_) => return,
    };

    let our_ports: std::collections::HashSet<u16> = collect_ports(ports);
    if our_ports.is_empty() {
        return;
    }

    for other in &other_projects {
        if other.project.name == project_name {
            continue;
        }
        let Some(other_ports) = &other.ports else {
            continue;
        };
        // Only check projects that have an open workspace
        if niri_client
            .find_workspace_by_name(&other.project.name)
            .ok()
            .flatten()
            .is_none()
        {
            continue;
        }

        let their_ports = collect_ports(other_ports);
        for port in our_ports.intersection(&their_ports) {
            eprintln!(
                "Warning: port {port} conflicts with project '{}'",
                other.project.name
            );
        }
    }
}

fn apply_window_widths(
    niri_client: &mut niri::NiriClient,
    requests: &[(String, niri_ipc::SizeChange)],
) {
    // Wait for windows to register with niri
    std::thread::sleep(Duration::from_millis(500));

    let mut pending: Vec<&(String, niri_ipc::SizeChange)> = requests.iter().collect();

    for attempt in 0..5 {
        if pending.is_empty() {
            break;
        }
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(300));
        }

        let mut still_pending = Vec::new();
        for req in &pending {
            match niri_client.find_window_by_title(&req.0) {
                Ok(Some(window)) => {
                    if let Err(e) = niri_client.set_window_width(window.id, req.1) {
                        eprintln!("  Warning: failed to set width for '{}': {e}", req.0);
                    }
                }
                Ok(None) => {
                    still_pending.push(*req);
                }
                Err(e) => {
                    eprintln!("  Warning: failed to find window '{}': {e}", req.0);
                }
            }
        }
        pending = still_pending;
    }

    for req in &pending {
        eprintln!("  Warning: window '{}' not found for width setting", req.0);
    }
}

fn collect_ports(ports: &drift_core::config::ProjectPorts) -> std::collections::HashSet<u16> {
    let mut set = std::collections::HashSet::new();
    if let Some([start, end]) = ports.range {
        for p in start..=end {
            set.insert(p);
        }
    }
    for port in ports.named.values() {
        set.insert(*port);
    }
    set
}

