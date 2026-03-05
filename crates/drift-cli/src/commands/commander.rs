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

    let state_dir = paths::state_base_dir();
    fs::create_dir_all(&state_dir)?;

    let log_path = state_dir.join("commander.log");
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .context("creating commander log")?;
    let stderr_file = log_file.try_clone()?;

    Command::new("drift-commander")
        .stdout(log_file)
        .stderr(stderr_file)
        .stdin(Stdio::null())
        .spawn()
        .context("spawning drift-commander (is it installed?)")?;

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
    println!("Voice control: {}", if config.commander.voice_enabled { "enabled" } else { "disabled" });
    if config.commander.voice_enabled {
        println!("Wake word: {}", config.commander.wake_word);
        println!("STT model: {}", config.commander.stt_model);

        let models_dir = drift_core::paths::models_dir();
        let vad_ok = models_dir.join("silero_vad.onnx").exists();
        let stt_ok = models_dir.join(&config.commander.stt_model).join("encoder.int8.onnx").exists();
        let wakeword_ok = models_dir.join(format!("{}.rpw", config.commander.wake_word)).exists();

        if !vad_ok || !stt_ok || !wakeword_ok {
            println!("Models: incomplete (run 'drift commander setup')");
        } else {
            println!("Models: ready");
        }
    }
    Ok(())
}

pub fn setup() -> anyhow::Result<()> {
    let models_dir = drift_core::paths::models_dir();
    fs::create_dir_all(&models_dir)?;
    println!("Models directory: {}", models_dir.display());

    let mut all_ready = true;

    let vad_path = models_dir.join("silero_vad.onnx");
    if vad_path.exists() {
        println!("[ok] Silero VAD model");
    } else {
        println!("[missing] Silero VAD model");
        println!("  curl -L -o {} https://github.com/snakers4/silero-vad/raw/v6.0/src/silero_vad/data/silero_vad.onnx", vad_path.display());
        println!("  Place at: {}", vad_path.display());
        all_ready = false;
    }

    let config = drift_core::config::load_global_config().unwrap_or_default();
    let stt_dir = models_dir.join(&config.commander.stt_model);
    let stt_files = ["encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx", "tokens.txt"];
    let stt_ok = stt_files.iter().all(|f| stt_dir.join(f).exists());
    if stt_ok {
        println!("[ok] Parakeet STT model ({})", config.commander.stt_model);
    } else {
        println!("[missing] Parakeet STT model ({})", config.commander.stt_model);
        println!("  Download from HuggingFace: https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/tree/main");
        println!("  Extract to: {}", stt_dir.display());
        all_ready = false;
    }

    let wakeword_path = models_dir.join(format!("{}.rpw", config.commander.wake_word));
    if wakeword_path.exists() {
        println!("[ok] Wake word model ({})", config.commander.wake_word);
    } else {
        println!("[missing] Wake word model ({})", config.commander.wake_word);
        println!("  Run: drift commander train");
        all_ready = false;
    }

    if all_ready {
        println!("\nAll voice models are ready!");
    } else {
        println!("\nSome models are missing. Download them to enable voice control.");
    }

    Ok(())
}

pub fn train(word: Option<&str>) -> anyhow::Result<()> {
    let config = drift_core::config::load_global_config().unwrap_or_default();
    let wake_word = word.unwrap_or(&config.commander.wake_word);
    let models_dir = drift_core::paths::models_dir();
    fs::create_dir_all(&models_dir)?;

    let rpw_path = models_dir.join(format!("{wake_word}.rpw"));

    println!("Wake word training: \"{wake_word}\"");
    println!("Output: {}", rpw_path.display());
    println!();
    println!("To create a wake word model:");
    println!("  1. Install rustpotter-cli: cargo install rustpotter-cli");
    println!("  2. Record samples (say \"{wake_word}\" 3-5 times):");
    println!("     rustpotter-cli build-model --model-name {wake_word} \\");
    println!("       --model-path {} \\", rpw_path.display());
    println!("       --sample-rate 16000 --channels 1");
    println!("  3. Restart commander: drift commander stop && drift commander start");

    if rpw_path.exists() {
        println!();
        println!("Note: existing model will be overwritten");
    }

    Ok(())
}

pub fn say(text: &str) -> anyhow::Result<()> {
    let status = Command::new("drift-commander")
        .args(["--say", text])
        .status()
        .context("running drift-commander (is it installed?)")?;
    if !status.success() {
        anyhow::bail!("drift-commander exited with {status}");
    }
    Ok(())
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
