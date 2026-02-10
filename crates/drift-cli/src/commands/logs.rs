use std::fs;
use std::io::{self, BufRead};
use std::process::Command;

use anyhow::{bail, Context};
use drift_core::{niri, paths};

pub fn run(service: Option<&str>, follow: bool, project: Option<&str>) -> anyhow::Result<()> {
    let project_name = resolve_project_name(project)?;
    let logs_dir = paths::logs_dir(&project_name);

    if !logs_dir.exists() {
        bail!("No logs directory for project '{project_name}'");
    }

    match (service, follow) {
        (None, false) => list_logs(&logs_dir),
        (Some(svc), false) => show_log(&logs_dir, svc),
        (Some(svc), true) => follow_log(&logs_dir, svc),
        (None, true) => follow_all(&logs_dir),
    }
}

fn list_logs(logs_dir: &std::path::Path) -> anyhow::Result<()> {
    let mut entries: Vec<String> = Vec::new();
    for entry in fs::read_dir(logs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "log") {
            if let Some(stem) = path.file_stem() {
                entries.push(stem.to_string_lossy().to_string());
            }
        }
    }

    if entries.is_empty() {
        println!("No log files found");
        return Ok(());
    }

    entries.sort();
    for name in &entries {
        println!("{name}");
    }
    Ok(())
}

fn show_log(logs_dir: &std::path::Path, service: &str) -> anyhow::Result<()> {
    let log_path = logs_dir.join(format!("{service}.log"));
    if !log_path.exists() {
        bail!("No log file for service '{service}'");
    }

    let file = fs::File::open(&log_path)
        .with_context(|| format!("opening {}", log_path.display()))?;
    let lines: Vec<String> = io::BufReader::new(file)
        .lines()
        .collect::<Result<_, _>>()?;

    let start = lines.len().saturating_sub(50);
    for line in &lines[start..] {
        println!("{line}");
    }
    Ok(())
}

fn follow_log(logs_dir: &std::path::Path, service: &str) -> anyhow::Result<()> {
    let log_path = logs_dir.join(format!("{service}.log"));
    if !log_path.exists() {
        bail!("No log file for service '{service}'");
    }

    let status = Command::new("tail")
        .args(["-f", &log_path.to_string_lossy()])
        .status()
        .context("running tail -f")?;

    if !status.success() {
        bail!("tail exited with {status}");
    }
    Ok(())
}

fn follow_all(logs_dir: &std::path::Path) -> anyhow::Result<()> {
    let pattern = format!("{}/*.log", logs_dir.to_string_lossy());

    let status = Command::new("sh")
        .args(["-c", &format!("tail -f {pattern}")])
        .status()
        .context("running tail -f on all logs")?;

    if !status.success() {
        bail!("tail exited with {status}");
    }
    Ok(())
}

fn resolve_project_name(name: Option<&str>) -> anyhow::Result<String> {
    if let Some(n) = name {
        return Ok(n.to_string());
    }

    if let Ok(project) = std::env::var("DRIFT_PROJECT") {
        if !project.is_empty() {
            return Ok(project);
        }
    }

    if let Ok(mut client) = niri::NiriClient::connect() {
        if let Ok(Some(win)) = client.focused_window() {
            if let Some(ws_id) = win.workspace_id {
                if let Ok(workspaces) = client.workspaces() {
                    for ws in &workspaces {
                        if ws.id == ws_id {
                            if let Some(ws_name) = &ws.name {
                                return Ok(ws_name.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    bail!("Could not determine project name. Use --project, set $DRIFT_PROJECT, or run from a drift workspace.")
}
