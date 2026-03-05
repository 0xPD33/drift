use std::fs;
use std::process::{Command, Stdio};

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

use crate::{config, paths};

/// Non-blocking project teardown: save workspace, kill tmux, SIGTERM supervisor,
/// clean up state files, remove from session tracking.
/// Does NOT wait for supervisor to die — callers handle that if needed.
pub fn teardown_project(project_name: &str) {
    // Best-effort workspace save
    if let Err(e) = crate::workspace::save_workspace(project_name) {
        eprintln!("  Warning: could not save workspace: {e}");
    }

    // Kill tmux session if configured
    if let Ok(cfg) = config::load_project_config(project_name) {
        if let Some(tmux_cfg) = cfg.tmux {
            if tmux_cfg.kill_on_close {
                let session_name = format!("drift:{project_name}");
                let has_session = Command::new("tmux")
                    .args(["has-session", "-t", &session_name])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);

                if has_session {
                    let _ = Command::new("tmux")
                        .args(["kill-session", "-t", &session_name])
                        .status();
                }
            }
        }
    }

    // Send SIGTERM to supervisor (non-blocking)
    let supervisor_pid_path = paths::supervisor_pid_path(project_name);
    if supervisor_pid_path.exists() {
        if let Ok(pid_str) = fs::read_to_string(&supervisor_pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                if signal::kill(Pid::from_raw(pid), None).is_ok() {
                    let _ = signal::kill(Pid::from_raw(pid), Signal::SIGTERM);
                }
            }
        }
        let _ = fs::remove_file(&supervisor_pid_path);
    }

    // Clean up state files
    let _ = fs::remove_file(paths::services_state_path(project_name));

    // Remove from session tracking
    if let Err(e) = crate::session::remove_project(project_name) {
        eprintln!("  Warning: could not update session: {e}");
    }
}
