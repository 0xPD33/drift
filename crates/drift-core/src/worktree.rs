use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

/// Path where task worktrees are created
pub fn worktree_base_dir(repo_path: &Path) -> PathBuf {
    repo_path.join(".drift-worktrees")
}

pub fn worktree_path(repo_path: &Path, task_id: &str) -> PathBuf {
    worktree_base_dir(repo_path).join(task_id)
}

/// Create a git worktree for a task, branching from current HEAD
pub fn create_task_worktree(repo_path: &Path, task_id: &str) -> Result<PathBuf> {
    let wt_path = worktree_path(repo_path, task_id);
    let branch = format!("drift/{}", task_id);

    std::fs::create_dir_all(worktree_base_dir(repo_path))?;

    let output = Command::new("git")
        .args(["worktree", "add", &wt_path.to_string_lossy(), "-b", &branch])
        .current_dir(repo_path)
        .output()
        .context("failed to run git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree add failed: {}", stderr.trim());
    }

    Ok(wt_path)
}

/// Remove a task's worktree and delete its branch
pub fn remove_task_worktree(repo_path: &Path, task_id: &str) -> Result<()> {
    let wt_path = worktree_path(repo_path, task_id);
    let branch = format!("drift/{}", task_id);

    if wt_path.exists() {
        let _ = Command::new("git")
            .args(["worktree", "remove", &wt_path.to_string_lossy(), "--force"])
            .current_dir(repo_path)
            .output();
    }

    // Clean up branch
    let _ = Command::new("git")
        .args(["branch", "-D", &branch])
        .current_dir(repo_path)
        .output();

    Ok(())
}

/// List active drift worktrees for a repo
pub fn list_task_worktrees(repo_path: &Path) -> Result<Vec<WorktreeInfo>> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .context("failed to list worktrees")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut result = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_branch: Option<String> = None;

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = Some(path.to_string());
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/drift/") {
            current_branch = Some(branch.to_string());
        } else if line.is_empty() {
            if let (Some(path), Some(task_id)) = (current_path.take(), current_branch.take()) {
                result.push(WorktreeInfo {
                    task_id,
                    path: PathBuf::from(path),
                });
            }
            current_path = None;
            current_branch = None;
        }
    }

    Ok(result)
}

pub struct WorktreeInfo {
    pub task_id: String,
    pub path: PathBuf,
}
