use std::fs;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::Context;
use drift_core::paths;

pub fn start() -> anyhow::Result<()> {
    // Check if already running
    let pid_path = paths::commander_pid_path();
    if pid_path.exists() {
        if let Ok(pid_str) = fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                if nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok() {
                    println!("Commander already running (PID {pid})");
                    return Ok(());
                }
            }
        }
        let _ = fs::remove_file(&pid_path);
    }

    let drift_bin = std::env::current_exe().context("determining drift binary path")?;
    let state_dir = paths::state_base_dir();
    fs::create_dir_all(&state_dir)?;

    let log_path = state_dir.join("commander.log");
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .context("creating commander log")?;
    let stderr_file = log_file.try_clone()?;

    Command::new(&drift_bin)
        .args(["_commander"])
        .stdout(log_file)
        .stderr(stderr_file)
        .stdin(Stdio::null())
        .spawn()
        .context("spawning commander")?;

    std::thread::sleep(Duration::from_millis(300));

    if pid_path.exists() {
        let pid = fs::read_to_string(&pid_path)
            .unwrap_or_default()
            .trim()
            .to_string();
        println!("Commander started (PID {pid})");
    } else {
        eprintln!("Warning: commander may not have started (check commander.log)");
    }
    Ok(())
}

pub fn stop() -> anyhow::Result<()> {
    let pid_path = paths::commander_pid_path();
    if !pid_path.exists() {
        println!("Commander not running");
        return Ok(());
    }

    let pid_str = fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str.trim().parse().context("invalid PID file")?;

    match nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), nix::sys::signal::Signal::SIGTERM) {
        Ok(_) => {
            println!("Commander stopped (PID {pid})");
            let _ = fs::remove_file(&pid_path);
        }
        Err(_) => {
            println!("Commander not running (stale PID file)");
            let _ = fs::remove_file(&pid_path);
        }
    }
    Ok(())
}

pub fn status() -> anyhow::Result<()> {
    let pid_path = paths::commander_pid_path();
    let running = if pid_path.exists() {
        if let Ok(pid_str) = fs::read_to_string(&pid_path) {
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

    let muted = paths::commander_muted_path().exists();

    if running {
        let pid = fs::read_to_string(&pid_path)
            .unwrap_or_default()
            .trim()
            .to_string();
        println!("Status: running (PID {pid})");
    } else {
        println!("Status: stopped");
    }

    if muted {
        println!("Muted: yes");
    }

    let config = drift_core::config::load_global_config().unwrap_or_default();
    println!("Voice: {}", config.commander.voice);
    println!("Endpoint: {}", config.commander.endpoint);
    if let Some(fb) = &config.commander.fallback_engine {
        println!("Fallback: {fb}");
    }
    Ok(())
}

pub fn say(text: &str) -> anyhow::Result<()> {
    drift_core::commander::say_text(text)
}

pub fn mute() -> anyhow::Result<()> {
    let path = paths::commander_muted_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, "")?;
    println!("Commander muted");
    Ok(())
}

pub fn unmute() -> anyhow::Result<()> {
    let path = paths::commander_muted_path();
    if path.exists() {
        fs::remove_file(&path)?;
    }
    println!("Commander unmuted");
    Ok(())
}
