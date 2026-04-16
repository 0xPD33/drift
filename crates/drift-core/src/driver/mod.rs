use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;

#[cfg(feature = "drivers-claude")]
pub mod claude_code;
#[cfg(feature = "drivers-codex")]
pub mod codex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    Starting,
    Working,
    Blocked,
    NeedsReview,
    Completed,
    Errored,
    Idle,
}

#[derive(Debug, Clone)]
pub struct AgentSpec {
    pub name: String,
    pub driver: String,
    pub cwd: PathBuf,
    pub flags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LaunchCtx {
    pub tmux_session: String,
    pub pane_id: Option<String>,
    pub project: String,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct AgentHandle {
    pub pid: Option<u32>,
    pub session_id: Option<String>,
    pub driver_data: HashMap<String, String>,
}

pub trait AgentDriver: Send + Sync {
    fn name(&self) -> &'static str;
    fn launch(&self, spec: &AgentSpec, ctx: &LaunchCtx) -> Result<AgentHandle>;
    fn poll_state(&self, handle: &AgentHandle) -> Result<AgentState>;
}
