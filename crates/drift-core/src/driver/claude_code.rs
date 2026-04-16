use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Result};
use serde::Deserialize;

use super::{AgentDriver, AgentHandle, AgentSpec, AgentState, LaunchCtx};

pub struct ClaudeCodeDriver;

#[derive(Deserialize)]
struct SessionEntry {
    pid: Option<u32>,
    cwd: Option<String>,
}

#[derive(Deserialize)]
struct TranscriptLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
}

impl ClaudeCodeDriver {
    fn sessions_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".claude/sessions")
    }

    fn projects_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".claude/projects")
    }

    fn find_session(&self, cwd: &std::path::Path) -> Option<SessionEntry> {
        let dir = Self::sessions_dir();
        let cwd_str = cwd.to_string_lossy();
        for entry in fs::read_dir(&dir).ok()? {
            let path = entry.ok()?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let content = fs::read_to_string(&path).ok()?;
            if let Ok(sess) = serde_json::from_str::<SessionEntry>(&content) {
                if sess.cwd.as_deref() == Some(cwd_str.as_ref()) {
                    return Some(sess);
                }
            }
        }
        None
    }

    fn tail_any_transcript(&self, cwd: &std::path::Path) -> Option<String> {
        let projects_dir = Self::projects_dir();
        let cwd_str = cwd.to_string_lossy();
        // slug is cwd with '/' → '-' (leading '/' stripped)
        let slug = cwd_str.trim_start_matches('/').replace('/', "-");
        let project_path = projects_dir.join(&slug);
        if !project_path.exists() {
            return None;
        }
        let mut files: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(&project_path)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let p = e.path();
                p.extension().and_then(|x| x.to_str()) == Some("jsonl")
                    && !p.file_name().unwrap_or_default().to_string_lossy().starts_with("agent-")
            })
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                Some((meta.modified().ok()?, e.path()))
            })
            .collect();
        files.sort_by(|a, b| b.0.cmp(&a.0));

        let path = files.into_iter().next()?.1;
        let content = fs::read_to_string(&path).ok()?;
        content.lines().rev().find(|l| !l.trim().is_empty()).map(String::from)
    }

    fn state_from_last_line(&self, line: &str) -> AgentState {
        let parsed: TranscriptLine = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return AgentState::Idle,
        };
        match parsed.line_type.as_deref() {
            Some("tool_use") => AgentState::Working,
            Some("tool_result") => AgentState::Working,
            Some("assistant") => AgentState::Idle,
            Some("user") => AgentState::Working,
            Some("system") => AgentState::Idle,
            _ => AgentState::Idle,
        }
    }

    fn process_alive(pid: u32) -> bool {
        use nix::sys::signal;
        use nix::unistd::Pid;
        signal::kill(Pid::from_raw(pid as i32), None).is_ok()
    }
}

impl AgentDriver for ClaudeCodeDriver {
    fn name(&self) -> &'static str {
        "claude-code"
    }

    fn launch(&self, spec: &AgentSpec, ctx: &LaunchCtx) -> Result<AgentHandle> {
        let mut args = vec!["new-window", "-t", &ctx.tmux_session, "-c"];
        let cwd_str = spec.cwd.to_string_lossy().to_string();
        args.push(&cwd_str);
        args.push("--");
        args.push("claude");
        let extra: Vec<&str> = spec.flags.iter().map(|s| s.as_str()).collect();
        let all_args: Vec<&str> = args.into_iter().chain(extra.iter().copied()).collect();

        let output = Command::new("tmux")
            .args(&all_args)
            .output()?;
        if !output.status.success() {
            bail!("tmux new-window failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        Ok(AgentHandle {
            pid: None,
            session_id: None,
            driver_data: std::collections::HashMap::from([
                ("cwd".into(), cwd_str),
            ]),
        })
    }

    fn poll_state(&self, handle: &AgentHandle) -> Result<AgentState> {
        let cwd = match handle.driver_data.get("cwd") {
            Some(c) => PathBuf::from(c),
            None => return Ok(AgentState::Idle),
        };

        // Check if there's a live session entry
        if let Some(sess) = self.find_session(&cwd) {
            if let Some(pid) = sess.pid {
                if !Self::process_alive(pid) {
                    return Ok(AgentState::Completed);
                }
            }
            // Process alive — check transcript for current activity
            if let Some(line) = self.tail_any_transcript(&cwd) {
                return Ok(self.state_from_last_line(&line));
            }
            return Ok(AgentState::Starting);
        }

        // No live session — check transcript for historical state
        if let Some(line) = self.tail_any_transcript(&cwd) {
            let state = self.state_from_last_line(&line);
            // If transcript exists but no live session, agent has exited
            return Ok(match state {
                AgentState::Idle => AgentState::Completed,
                other => other,
            });
        }

        Ok(AgentState::Idle)
    }
}
