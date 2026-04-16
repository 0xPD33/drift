use std::path::Path;
use std::process::Command;

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HandoffStatus {
    Completed,
    Failed,
    NeedsReview,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub task_id: String,
    pub status: HandoffStatus,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub files_changed: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests_run: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests_passed: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests_failed: Option<u32>,
}

pub struct VerificationResult {
    pub passed: bool,
    pub output: String,
    pub exit_code: i32,
}

/// Parse a handoff file (YAML frontmatter between --- delimiters + markdown body)
pub fn read_handoff(path: &Path) -> anyhow::Result<(Handoff, String)> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading handoff file {}", path.display()))?;

    let (yaml_str, body) = crate::parse_yaml_frontmatter(&content)
        .ok_or_else(|| anyhow::anyhow!("handoff file missing YAML frontmatter delimiters"))?;

    let handoff: Handoff = serde_yaml::from_str(yaml_str)
        .with_context(|| format!("parsing YAML frontmatter in {}", path.display()))?;

    Ok((handoff, body.to_string()))
}

/// Generate the handoff instruction text to append to agent prompts
pub fn handoff_template(task_id: &str, handoff_path: &Path) -> String {
    format!(
        r#"## When You Finish

Before ending your session, write a handoff file at the following path:
{path}

Use this exact format:

---
task_id: {task_id}
status: completed  # or: failed, needs-review, blocked
agent: your-agent-name
completed_at: current ISO 8601 timestamp
files_changed:
  - path/to/file1
  - path/to/file2
tests_run: 0
tests_passed: 0
tests_failed: 0
---

## What was done
Describe what you accomplished.

## Concerns
Any issues the reviewer should know about.

## Deviations from task
If you did something different from the task description, explain why.

## Next steps
What should happen next.

IMPORTANT: Write this file as your LAST action before finishing. If you cannot
complete the task, still write the handoff with status: failed or status: blocked
and explain what went wrong in the body."#,
        path = handoff_path.display(),
        task_id = task_id,
    )
}

/// Run a verification command and capture result, with a timeout.
pub fn run_verification(command: &str, cwd: &Path, timeout_sec: u64) -> anyhow::Result<VerificationResult> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("running verification command: {command}"))?;

    let timeout = std::time::Duration::from_secs(timeout_sec);
    let start = std::time::Instant::now();

    loop {
        match child.try_wait()? {
            Some(status) => {
                let stdout = child.stdout.take().map(|mut s| {
                    let mut buf = Vec::new();
                    std::io::Read::read_to_end(&mut s, &mut buf).ok();
                    String::from_utf8_lossy(&buf).to_string()
                }).unwrap_or_default();
                let stderr = child.stderr.take().map(|mut s| {
                    let mut buf = Vec::new();
                    std::io::Read::read_to_end(&mut s, &mut buf).ok();
                    String::from_utf8_lossy(&buf).to_string()
                }).unwrap_or_default();
                let combined = if stderr.is_empty() {
                    stdout
                } else {
                    format!("{stdout}\n{stderr}")
                };
                let exit_code = status.code().unwrap_or(-1);
                return Ok(VerificationResult {
                    passed: exit_code == 0,
                    output: combined,
                    exit_code,
                });
            }
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(VerificationResult {
                        passed: false,
                        output: format!("Verification timed out after {timeout_sec}s"),
                        exit_code: -1,
                    });
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_valid_handoff() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.md");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, r#"---
task_id: tsk-abc123
status: completed
agent: claude-code
model: sonnet
completed_at: "2026-03-28T14:45:00Z"
files_changed:
  - src/main.rs
  - src/lib.rs
tests_run: 5
tests_passed: 5
tests_failed: 0
---

## What was done
Fixed the bug.

## Concerns
None.
"#).unwrap();

        let (handoff, body) = read_handoff(&path).unwrap();
        assert_eq!(handoff.task_id, "tsk-abc123");
        assert_eq!(handoff.status, HandoffStatus::Completed);
        assert_eq!(handoff.agent, "claude-code");
        assert_eq!(handoff.model.as_deref(), Some("sonnet"));
        assert_eq!(handoff.files_changed.len(), 2);
        assert_eq!(handoff.tests_run, Some(5));
        assert_eq!(handoff.tests_passed, Some(5));
        assert_eq!(handoff.tests_failed, Some(0));
        assert!(body.contains("Fixed the bug"));
    }

    #[test]
    fn parse_handoff_no_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.md");
        std::fs::write(&path, "no frontmatter here").unwrap();
        assert!(read_handoff(&path).is_err());
    }

    #[test]
    fn parse_handoff_missing_close() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.md");
        std::fs::write(&path, "---\ntask_id: x\nstatus: completed\nagent: a\n").unwrap();
        assert!(read_handoff(&path).is_err());
    }

    #[test]
    fn handoff_template_contains_placeholders() {
        let tmpl = handoff_template("tsk-123", Path::new("/tmp/handoff.md"));
        assert!(tmpl.contains("tsk-123"));
        assert!(tmpl.contains("/tmp/handoff.md"));
        assert!(tmpl.contains("status: completed"));
    }

    #[test]
    fn run_verification_success() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_verification("echo ok", dir.path(), 30).unwrap();
        assert!(result.passed);
        assert_eq!(result.exit_code, 0);
        assert!(result.output.contains("ok"));
    }

    #[test]
    fn run_verification_failure() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_verification("exit 1", dir.path(), 30).unwrap();
        assert!(!result.passed);
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn handoff_status_serde_kebab_case() {
        let yaml = "task_id: x\nstatus: needs-review\nagent: a\n";
        let h: Handoff = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(h.status, HandoffStatus::NeedsReview);
    }
}
