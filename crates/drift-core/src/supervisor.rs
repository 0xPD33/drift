use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};

use crate::config::{self, RestartPolicy, ServiceProcess};
use crate::events::{self, Event};
use crate::{agent, env, paths};

// --- Public types (serialized to services.json) ---

#[derive(Debug, Serialize, Deserialize)]
pub struct ServicesState {
    pub supervisor_pid: u32,
    pub project: String,
    pub services: Vec<ServiceState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceState {
    pub name: String,
    pub pid: Option<u32>,
    pub status: ServiceStatus,
    pub restart_count: u32,
    pub started_at: Option<String>,
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub is_agent: bool,
    pub agent_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceStatus {
    Running,
    Stopped,
    Failed,
    Backoff,
}

// --- Internal types ---

struct ManagedService {
    config: ServiceProcess,
    child: Option<Child>,
    pid: Option<u32>,
    status: ServiceStatus,
    restart_count: u32,
    started_at: Option<Instant>,
    started_at_system: Option<SystemTime>,
    last_exit: Option<Instant>,
    exit_code: Option<i32>,
    backoff: Duration,
}

// --- Signal handling ---

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_signal(_: libc::c_int) {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

fn install_signal_handlers() {
    unsafe {
        let action = SigAction::new(
            SigHandler::Handler(handle_signal),
            SaFlags::SA_RESTART,
            SigSet::empty(),
        );
        signal::sigaction(Signal::SIGTERM, &action).expect("install SIGTERM handler");
        signal::sigaction(Signal::SIGINT, &action).expect("install SIGINT handler");
    }
}

// --- Main entry point ---

pub fn run_supervisor(project_name: &str) -> anyhow::Result<()> {
    let project_config = config::load_project_config(project_name)?;
    let env_vars = env::build_env(&project_config)?;
    let repo_path = config::resolve_repo_path(&project_config.project.repo);

    install_signal_handlers();

    let logs_dir = paths::logs_dir(project_name);
    let state_dir = paths::state_dir(project_name);
    fs::create_dir_all(&logs_dir)?;
    fs::create_dir_all(&state_dir)?;

    fs::write(
        paths::supervisor_pid_path(project_name),
        std::process::id().to_string(),
    )?;

    let processes = match project_config.services {
        Some(svc_config) if !svc_config.processes.is_empty() => svc_config.processes,
        _ => return Ok(()),
    };

    // Filter out interactive agents (they get spawned as windows, not services)
    let processes: Vec<_> = processes.into_iter()
        .filter(|s| !agent::is_interactive_agent(s))
        .collect();

    if processes.is_empty() {
        return Ok(());
    }

    let mut services: Vec<ManagedService> = Vec::with_capacity(processes.len());
    for proc in processes {
        match spawn_service(&proc, &env_vars, &repo_path, project_name) {
            Ok(child) => {
                let pid = child.id();
                events::try_emit_event(&Event {
                    event_type: "service.started".into(),
                    project: project_name.to_string(),
                    source: proc.name.clone(),
                    ts: events::iso_now(),
                    level: Some("info".into()),
                    title: Some(format!("Service '{}' started", proc.name)),
                    body: None,
                    meta: Some(serde_json::json!({ "pid": pid })),
                    priority: None,
                });
                services.push(ManagedService {
                    config: proc,
                    pid: Some(pid),
                    child: Some(child),
                    status: ServiceStatus::Running,
                    restart_count: 0,
                    started_at: Some(Instant::now()),
                    started_at_system: Some(SystemTime::now()),
                    last_exit: None,
                    exit_code: None,
                    backoff: Duration::ZERO,
                });
            }
            Err(e) => {
                eprintln!("failed to spawn service '{}': {e}", proc.name);
                services.push(ManagedService {
                    config: proc,
                    child: None,
                    pid: None,
                    status: ServiceStatus::Failed,
                    restart_count: 0,
                    started_at: None,
                    started_at_system: None,
                    last_exit: None,
                    exit_code: None,
                    backoff: Duration::ZERO,
                });
            }
        }
    }

    write_state(&services, project_name);

    loop {
        if SHUTDOWN.load(Ordering::Relaxed) {
            graceful_shutdown(&mut services, &env_vars, &repo_path, project_name);
            break;
        }

        let mut state_changed = false;

        for svc in &mut services {
            match svc.status {
                ServiceStatus::Running => {
                    if let Some(child) = svc.child.as_mut() {
                        match child.try_wait() {
                            Ok(Some(exit_status)) => {
                                svc.exit_code = exit_status.code();
                                svc.child = None;
                                svc.pid = None;
                                svc.last_exit = Some(Instant::now());

                                let should_restart = match svc.config.restart {
                                    RestartPolicy::Always => true,
                                    RestartPolicy::OnFailure => !exit_status.success(),
                                    RestartPolicy::Never => false,
                                };

                                if should_restart {
                                    let ran_for = svc
                                        .started_at
                                        .map(|s| s.elapsed())
                                        .unwrap_or(Duration::ZERO);
                                    if ran_for < Duration::from_secs(5) {
                                        svc.backoff = (svc.backoff * 2)
                                            .max(Duration::from_secs(1))
                                            .min(Duration::from_secs(30));
                                        svc.status = ServiceStatus::Backoff;
                                    } else {
                                        svc.backoff = Duration::ZERO;
                                        match spawn_service(
                                            &svc.config,
                                            &env_vars,
                                            &repo_path,
                                            project_name,
                                        ) {
                                            Ok(child) => {
                                                svc.restart_count += 1;
                                                let new_pid = child.id();
                                                svc.pid = Some(new_pid);
                                                svc.child = Some(child);
                                                svc.status = ServiceStatus::Running;
                                                svc.started_at = Some(Instant::now());
                                                svc.started_at_system = Some(SystemTime::now());
                                                events::try_emit_event(&Event {
                                                    event_type: "service.restarted".into(),
                                                    project: project_name.to_string(),
                                                    source: svc.config.name.clone(),
                                                    ts: events::iso_now(),
                                                    level: Some("warning".into()),
                                                    title: Some(format!("Service '{}' restarted", svc.config.name)),
                                                    body: None,
                                                    meta: Some(serde_json::json!({ "pid": new_pid, "restart_count": svc.restart_count })),
                                                    priority: None,
                                                });
                                            }
                                            Err(_) => {
                                                svc.status = ServiceStatus::Failed;
                                            }
                                        }
                                    }
                                } else if exit_status.success() {
                                    svc.status = ServiceStatus::Stopped;
                                    events::try_emit_event(&Event {
                                        event_type: "service.stopped".into(),
                                        project: project_name.to_string(),
                                        source: svc.config.name.clone(),
                                        ts: events::iso_now(),
                                        level: Some("info".into()),
                                        title: Some(format!("Service '{}' stopped", svc.config.name)),
                                        body: None,
                                        meta: Some(serde_json::json!({ "exit_code": 0 })),
                                        priority: None,
                                    });
                                } else {
                                    svc.status = ServiceStatus::Failed;
                                    events::try_emit_event(&Event {
                                        event_type: "service.crashed".into(),
                                        project: project_name.to_string(),
                                        source: svc.config.name.clone(),
                                        ts: events::iso_now(),
                                        level: Some("error".into()),
                                        title: Some(format!("Service '{}' crashed", svc.config.name)),
                                        body: None,
                                        meta: Some(serde_json::json!({ "exit_code": svc.exit_code })),
                                        priority: None,
                                    });
                                }
                                state_changed = true;
                            }
                            Ok(None) => {}
                            Err(_) => {
                                svc.child = None;
                                svc.pid = None;
                                svc.status = ServiceStatus::Failed;
                                state_changed = true;
                            }
                        }
                    }
                }
                ServiceStatus::Backoff => {
                    if svc
                        .last_exit
                        .map(|t| t.elapsed() >= svc.backoff)
                        .unwrap_or(true)
                    {
                        match spawn_service(&svc.config, &env_vars, &repo_path, project_name) {
                            Ok(child) => {
                                svc.restart_count += 1;
                                let new_pid = child.id();
                                svc.pid = Some(new_pid);
                                svc.child = Some(child);
                                svc.status = ServiceStatus::Running;
                                svc.started_at = Some(Instant::now());
                                svc.started_at_system = Some(SystemTime::now());
                                events::try_emit_event(&Event {
                                    event_type: "service.restarted".into(),
                                    project: project_name.to_string(),
                                    source: svc.config.name.clone(),
                                    ts: events::iso_now(),
                                    level: Some("warning".into()),
                                    title: Some(format!("Service '{}' restarted", svc.config.name)),
                                    body: None,
                                    meta: Some(serde_json::json!({ "pid": new_pid, "restart_count": svc.restart_count })),
                                    priority: None,
                                });
                            }
                            Err(_) => {
                                svc.status = ServiceStatus::Failed;
                            }
                        }
                        state_changed = true;
                    }
                }
                ServiceStatus::Stopped | ServiceStatus::Failed => {}
            }
        }

        if state_changed {
            write_state(&services, project_name);
        }

        if services
            .iter()
            .all(|s| matches!(s.status, ServiceStatus::Stopped | ServiceStatus::Failed))
        {
            break;
        }

        thread::sleep(Duration::from_millis(500));
    }

    Ok(())
}

// --- Spawn ---

fn spawn_service(
    svc: &ServiceProcess,
    env_vars: &HashMap<String, String>,
    repo_path: &Path,
    project: &str,
) -> anyhow::Result<Child> {
    let svc_cwd = if svc.cwd.is_empty() || svc.cwd == "." {
        repo_path.to_path_buf()
    } else {
        repo_path.join(&svc.cwd)
    };

    let logs_dir = paths::logs_dir(project);
    let log_path = logs_dir.join(format!("{}.log", svc.name));

    let command = if svc.agent.is_some() {
        agent::build_agent_command(svc, project)
    } else {
        svc.command.clone()
    };

    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    writeln!(log_file, "\n--- service '{}' started at {now} ---", svc.name)?;

    let stderr_file = log_file.try_clone()?;

    let child = unsafe {
        Command::new("sh")
            .args(["-c", &command])
            .envs(env_vars)
            .current_dir(&svc_cwd)
            .stdout(log_file)
            .stderr(stderr_file)
            .stdin(Stdio::null())
            .pre_exec(|| {
                libc::setsid();
                Ok(())
            })
            .spawn()?
    };

    Ok(child)
}

// --- Graceful shutdown ---

fn graceful_shutdown(
    services: &mut [ManagedService],
    env_vars: &HashMap<String, String>,
    repo_path: &Path,
    project: &str,
) {
    // Phase 1: SIGTERM or stop_command
    for svc in services.iter_mut() {
        if svc.child.is_some() {
            if let Some(stop_cmd) = &svc.config.stop_command {
                let _ = Command::new("sh")
                    .args(["-c", stop_cmd])
                    .current_dir(repo_path)
                    .envs(env_vars)
                    .status();
            } else if let Some(pid) = svc.pid {
                let _ = signal::kill(Pid::from_raw(-(pid as i32)), Signal::SIGTERM);
            }
        }
    }

    // Phase 2: Wait up to 5 seconds
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let all_exited = services.iter_mut().all(|svc| {
            if let Some(child) = svc.child.as_mut() {
                matches!(child.try_wait(), Ok(Some(_)))
            } else {
                true
            }
        });
        if all_exited || Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    // Phase 3: SIGKILL stragglers
    for svc in services.iter_mut() {
        if svc.child.is_some() {
            if let Some(pid) = svc.pid {
                let _ = signal::kill(Pid::from_raw(-(pid as i32)), Signal::SIGKILL);
            }
            if let Some(child) = svc.child.as_mut() {
                let _ = child.wait();
            }
            svc.child = None;
        }
        svc.status = ServiceStatus::Stopped;
    }

    write_state(services, project);
    let _ = fs::remove_file(paths::supervisor_pid_path(project));
}

// --- State writing (atomic) ---

fn write_state(services: &[ManagedService], project: &str) {
    let state = ServicesState {
        supervisor_pid: std::process::id(),
        project: project.to_string(),
        services: services
            .iter()
            .map(|s| ServiceState {
                name: s.config.name.clone(),
                pid: s.pid,
                status: s.status.clone(),
                restart_count: s.restart_count,
                started_at: s.started_at_system.map(format_time),
                exit_code: s.exit_code,
                is_agent: s.config.agent.is_some(),
                agent_type: s.config.agent.clone(),
            })
            .collect(),
    };
    let path = paths::services_state_path(project);
    let tmp = path.with_extension("json.tmp");
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        let _ = fs::write(&tmp, &json);
        let _ = fs::rename(&tmp, &path);
    }
}

fn format_time(time: SystemTime) -> String {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    format!("{secs}")
}
