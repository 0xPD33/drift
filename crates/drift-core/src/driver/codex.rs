use std::process::Command;

use anyhow::{bail, Result};

use super::{AgentDriver, AgentHandle, AgentSpec, AgentState, LaunchCtx};

pub struct CodexDriver;

impl CodexDriver {
    fn process_alive(pid: u32) -> bool {
        use nix::sys::signal;
        use nix::unistd::Pid;
        signal::kill(Pid::from_raw(pid as i32), None).is_ok()
    }

    fn capture_pane(session: &str, pane_id: &str) -> Option<String> {
        let target = format!("{session}:{pane_id}");
        let output = Command::new("tmux")
            .args(["capture-pane", "-p", "-t", &target])
            .output()
            .ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            None
        }
    }

    fn state_from_pane_output(output: &str) -> AgentState {
        let last = output.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
        if last.contains("thinking") || last.contains("Thinking") || last.contains("...") {
            AgentState::Working
        } else if last.contains("Error") || last.contains("error") || last.contains("fatal") {
            AgentState::Errored
        } else if last.contains('$') || last.contains('%') || last.contains("❯") {
            // Shell prompt — codex has handed back control
            AgentState::Idle
        } else if last.contains("press") || last.contains("Press") || last.contains("(y/n)") {
            AgentState::Blocked
        } else {
            AgentState::Working
        }
    }
}

impl AgentDriver for CodexDriver {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn launch(&self, spec: &AgentSpec, ctx: &LaunchCtx) -> Result<AgentHandle> {
        let cwd_str = spec.cwd.to_string_lossy().to_string();
        let mut cmd_args = vec!["new-window", "-t", &ctx.tmux_session, "-c", &cwd_str, "--"];
        cmd_args.push("codex");
        let extra: Vec<&str> = spec.flags.iter().map(|s| s.as_str()).collect();
        let all_args: Vec<&str> = cmd_args.into_iter().chain(extra.iter().copied()).collect();

        let output = Command::new("tmux").args(&all_args).output()?;
        if !output.status.success() {
            bail!("tmux new-window failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        // Get the pane id of the newly created window
        let pane_output = Command::new("tmux")
            .args(["display-message", "-t", &ctx.tmux_session, "-p", "#{pane_id}"])
            .output()?;
        let pane_id = String::from_utf8_lossy(&pane_output.stdout).trim().to_string();

        Ok(AgentHandle {
            pid: None,
            session_id: None,
            driver_data: std::collections::HashMap::from([
                ("tmux_session".into(), ctx.tmux_session.clone()),
                ("pane_id".into(), pane_id),
            ]),
        })
    }

    fn poll_state(&self, handle: &AgentHandle) -> Result<AgentState> {
        if let Some(pid) = handle.pid {
            if !Self::process_alive(pid) {
                return Ok(AgentState::Completed);
            }
        }

        let session = handle.driver_data.get("tmux_session").map(|s| s.as_str()).unwrap_or("");
        let pane_id = handle.driver_data.get("pane_id").map(|s| s.as_str()).unwrap_or("");

        if session.is_empty() || pane_id.is_empty() {
            return Ok(AgentState::Idle);
        }

        match Self::capture_pane(session, pane_id) {
            Some(output) => Ok(Self::state_from_pane_output(&output)),
            None => Ok(AgentState::Completed),
        }
    }
}
