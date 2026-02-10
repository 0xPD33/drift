use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use std::{fs, io, thread};

use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};

use crate::config::{self, CommanderConfig};
use crate::events::Event;
use crate::paths;

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

// --- Speakable event types ---

const SPEAKABLE_EVENTS: &[&str] = &[
    "agent.completed",
    "agent.error",
    "agent.needs_review",
    "service.crashed",
    "build.failed",
];

fn is_speakable_event(event_type: &str) -> bool {
    SPEAKABLE_EVENTS.contains(&event_type)
}

fn should_speak(event: &Event) -> bool {
    is_speakable_event(&event.event_type)
}

fn is_critical(event: &Event) -> bool {
    event.priority.as_deref() == Some("critical")
}

// --- Speech rendering ---

fn title_or_type(event: &Event) -> &str {
    event
        .title
        .as_deref()
        .filter(|t| !t.is_empty())
        .unwrap_or(&event.event_type)
}

fn render_speech(event: &Event) -> String {
    match event.event_type.as_str() {
        "agent.completed" => format!("{}: agent finished — {}", event.project, title_or_type(event)),
        "agent.error" => format!("{}: agent error — {}", event.project, title_or_type(event)),
        "agent.needs_review" => {
            format!("{}: agent needs review — {}", event.project, title_or_type(event))
        }
        "service.crashed" => format!("{}: {} crashed", event.project, event.source),
        "build.failed" => format!("{}: build failed — {}", event.project, title_or_type(event)),
        _ => format!("{}: {}", event.project, title_or_type(event)),
    }
}

// --- Cooldown batching ---

struct CooldownEntry {
    count: u32,
    expires: Instant,
}

struct CooldownTracker {
    entries: HashMap<(String, String), CooldownEntry>,
    cooldown: Duration,
}

impl CooldownTracker {
    fn new(cooldown_sec: u64) -> Self {
        Self {
            entries: HashMap::new(),
            cooldown: Duration::from_secs(cooldown_sec),
        }
    }

    /// Returns None if this event should be spoken normally,
    /// or Some(count) if the cooldown window expired and a batch summary should be spoken.
    fn check(&mut self, project: &str, event_type: &str) -> CooldownAction {
        let key = (project.to_string(), event_type.to_string());
        let now = Instant::now();

        if let Some(entry) = self.entries.get_mut(&key) {
            if now < entry.expires {
                entry.count += 1;
                return CooldownAction::Suppress;
            }
            // Window expired
            let count = entry.count;
            entry.count = 1;
            entry.expires = now + self.cooldown;
            if count > 1 {
                return CooldownAction::Batch(count);
            }
            CooldownAction::Speak
        } else {
            self.entries.insert(
                key,
                CooldownEntry {
                    count: 1,
                    expires: now + self.cooldown,
                },
            );
            CooldownAction::Speak
        }
    }

    fn flush_expired(&mut self) -> Vec<(String, String, u32)> {
        let now = Instant::now();
        let mut batches = Vec::new();
        self.entries.retain(|key, entry| {
            if now >= entry.expires && entry.count > 1 {
                batches.push((key.0.clone(), key.1.clone(), entry.count));
                false
            } else if now >= entry.expires {
                false
            } else {
                true
            }
        });
        batches
    }
}

enum CooldownAction {
    Speak,
    Suppress,
    Batch(u32),
}

// --- TTS engine ---

#[derive(Clone, Copy, PartialEq)]
enum TtsEngine {
    Http,
    Fallback,
    None,
}

struct SpeechMessage {
    text: String,
    instruct: String,
    _critical: bool,
}

struct TtsState {
    engine: TtsEngine,
    config: CommanderConfig,
    last_http_check: Instant,
}

impl TtsState {
    fn new(config: CommanderConfig) -> Self {
        let engine = if check_http_tts(&config.endpoint) {
            eprintln!("commander: using HTTP TTS at {}", config.endpoint);
            TtsEngine::Http
        } else if config.fallback_engine.is_some() || config.fallback_command.is_some() {
            eprintln!("commander: HTTP TTS unavailable, using fallback");
            TtsEngine::Fallback
        } else {
            eprintln!("commander: no TTS engine available");
            TtsEngine::None
        };

        Self {
            engine,
            config,
            last_http_check: Instant::now(),
        }
    }

    fn maybe_recheck_http(&mut self) {
        if self.engine == TtsEngine::Http {
            return;
        }
        if self.last_http_check.elapsed() >= Duration::from_secs(60) {
            self.last_http_check = Instant::now();
            if check_http_tts(&self.config.endpoint) {
                eprintln!("commander: HTTP TTS recovered");
                self.engine = TtsEngine::Http;
            }
        }
    }

    fn speak(&mut self, msg: &SpeechMessage) {
        self.maybe_recheck_http();

        match self.engine {
            TtsEngine::Http => {
                if let Err(e) = speak_http(&self.config, &msg.text, &msg.instruct) {
                    eprintln!("commander: HTTP TTS failed: {e}, trying fallback");
                    self.engine = TtsEngine::Fallback;
                    self.last_http_check = Instant::now();
                    let _ = speak_fallback(&self.config, &msg.text);
                }
            }
            TtsEngine::Fallback => {
                if let Err(e) = speak_fallback(&self.config, &msg.text) {
                    eprintln!("commander: fallback TTS failed: {e}");
                }
            }
            TtsEngine::None => {}
        }
    }
}

fn make_agent(timeout_secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .build()
        .new_agent()
}

fn check_http_tts(endpoint: &str) -> bool {
    let url = format!("{endpoint}/v1/audio/speech");
    let body = serde_json::json!({
        "model": "qwen3-tts",
        "voice": "Vivian",
        "input": "test",
        "response_format": "wav",
    });

    let agent = make_agent(5);
    match agent.post(&url).send_json(&body) {
        Ok(resp) => resp.status() == 200,
        Err(_) => false,
    }
}

fn speak_http(config: &CommanderConfig, text: &str, instruct: &str) -> anyhow::Result<()> {
    let url = format!("{}/v1/audio/speech", config.endpoint);
    let mut body = serde_json::json!({
        "model": "qwen3-tts",
        "voice": config.voice,
        "input": text,
        "response_format": "wav",
    });
    if !instruct.is_empty() {
        body["instruct"] = serde_json::Value::String(instruct.to_string());
    }

    let agent = make_agent(30);
    let mut resp = agent.post(&url).send_json(&body)?;

    if resp.status() != 200 {
        anyhow::bail!("HTTP TTS returned status {}", resp.status());
    }

    let audio_data = resp.body_mut().read_to_vec()?;

    play_audio(&audio_data, config.audio_filter.as_deref())
}

fn speak_fallback(config: &CommanderConfig, text: &str) -> anyhow::Result<()> {
    let cmd = if let Some(custom) = &config.fallback_command {
        custom.replace("{text}", text)
    } else {
        match config.fallback_engine.as_deref() {
            Some("piper") => {
                let voice = config
                    .fallback_voice
                    .as_deref()
                    .unwrap_or("en_US-lessac-medium");
                if let Some(filter) = &config.audio_filter {
                    format!(
                        "echo {} | piper --model {} --output-raw | {} | aplay -r 22050 -f S16_LE",
                        shell_escape(text),
                        shell_escape(voice),
                        filter,
                    )
                } else {
                    format!(
                        "echo {} | piper --model {} --output-raw | aplay -r 22050 -f S16_LE",
                        shell_escape(text),
                        shell_escape(voice),
                    )
                }
            }
            Some("espeak-ng") | Some("espeak") => {
                format!("echo {} | espeak-ng", shell_escape(text))
            }
            _ => {
                anyhow::bail!("no fallback engine configured");
            }
        }
    };

    let status = unsafe {
        Command::new("sh")
            .args(["-c", &cmd])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .pre_exec(|| {
                libc::setsid();
                Ok(())
            })
            .status()?
    };

    if !status.success() {
        anyhow::bail!("fallback TTS exited with {status}");
    }
    Ok(())
}

fn play_audio(data: &[u8], audio_filter: Option<&str>) -> anyhow::Result<()> {
    let cmd = if let Some(filter) = audio_filter {
        format!("{filter} | aplay -r 22050 -f S16_LE")
    } else {
        "aplay -r 22050 -f S16_LE".into()
    };

    let mut child = unsafe {
        Command::new("sh")
            .args(["-c", &cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .pre_exec(|| {
                libc::setsid();
                Ok(())
            })
            .spawn()?
    };

    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        let _ = stdin.write_all(data);
    }
    drop(child.stdin.take());
    child.wait()?;
    Ok(())
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// --- Speech worker thread ---

fn speech_worker(
    rx: mpsc::Receiver<SpeechMessage>,
    interrupt: &AtomicBool,
    config: CommanderConfig,
) {
    let mut tts = TtsState::new(config);

    while !SHUTDOWN.load(Ordering::Relaxed) {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(msg) => {
                interrupt.store(false, Ordering::Relaxed);
                tts.speak(&msg);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

// --- Main commander process ---

pub fn run_commander() -> anyhow::Result<()> {
    install_signal_handlers();

    let global_config = config::load_global_config().unwrap_or_default();
    let commander_config = global_config.commander;

    // Write PID file
    let pid_path = paths::commander_pid_path();
    if let Some(parent) = pid_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&pid_path, std::process::id().to_string())?;

    eprintln!("commander started (PID {})", std::process::id());

    // Speech worker channel
    let (speech_tx, speech_rx) = mpsc::channel::<SpeechMessage>();
    let interrupt = std::sync::Arc::new(AtomicBool::new(false));

    let worker_interrupt = std::sync::Arc::clone(&interrupt);
    let worker_config = CommanderConfig {
        enabled: commander_config.enabled,
        endpoint: commander_config.endpoint.clone(),
        voice: commander_config.voice.clone(),
        instruct: commander_config.instruct.clone(),
        fallback_engine: commander_config.fallback_engine.clone(),
        fallback_voice: commander_config.fallback_voice.clone(),
        fallback_command: commander_config.fallback_command.clone(),
        audio_filter: commander_config.audio_filter.clone(),
        speak_background_only: commander_config.speak_background_only,
        cooldown_sec: commander_config.cooldown_sec,
        max_queue: commander_config.max_queue,
        event_instructs: commander_config.event_instructs.clone(),
    };

    let speech_thread = thread::Builder::new()
        .name("speech-worker".into())
        .spawn(move || speech_worker(speech_rx, &worker_interrupt, worker_config))?;

    // Connect to subscribe.sock
    let sock_path = paths::subscribe_socket_path();
    let mut cooldown = CooldownTracker::new(commander_config.cooldown_sec);

    'outer: loop {
        if SHUTDOWN.load(Ordering::Relaxed) {
            break;
        }

        let stream = match UnixStream::connect(&sock_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("commander: cannot connect to subscribe.sock: {e}");
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        stream.set_read_timeout(Some(Duration::from_millis(500)))?;

        let mut reader = io::BufReader::new(stream);
        let mut line_buf = String::new();

        loop {
            if SHUTDOWN.load(Ordering::Relaxed) {
                break 'outer;
            }

            // Check muted
            if paths::commander_muted_path().exists() {
                line_buf.clear();
                let _ = io::BufRead::read_line(&mut reader, &mut line_buf);
                continue;
            }

            // Flush expired cooldowns
            for (project, event_type, count) in cooldown.flush_expired() {
                let batch_text = format!("{project}: {count} more {event_type} events");
                let _ = speech_tx.send(SpeechMessage {
                    text: batch_text,
                    instruct: commander_config.instruct.clone(),
                    _critical: false,
                });
            }

            line_buf.clear();
            match io::BufRead::read_line(&mut reader, &mut line_buf) {
                Ok(0) => {
                    // EOF — socket closed, reconnect
                    eprintln!("commander: subscribe.sock closed, reconnecting");
                    thread::sleep(Duration::from_secs(1));
                    break;
                }
                Ok(_) => {
                    let line = line_buf.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let event: Event = match serde_json::from_str(line) {
                        Ok(e) => e,
                        Err(e) => {
                            eprintln!("commander: failed to parse event: {e}");
                            continue;
                        }
                    };

                    if !should_speak(&event) {
                        continue;
                    }

                    let critical = is_critical(&event);

                    // Cooldown check
                    match cooldown.check(&event.project, &event.event_type) {
                        CooldownAction::Suppress => continue,
                        CooldownAction::Batch(count) => {
                            let batch_text = format!(
                                "{}: {} more {} events",
                                event.project, count, event.event_type
                            );
                            let _ = speech_tx.send(SpeechMessage {
                                text: batch_text,
                                instruct: commander_config.instruct.clone(),
                                _critical: false,
                            });
                        }
                        CooldownAction::Speak => {}
                    }

                    let text = render_speech(&event);
                    let instruct = commander_config
                        .event_instructs
                        .get(&event.event_type)
                        .cloned()
                        .unwrap_or_else(|| commander_config.instruct.clone());

                    if critical {
                        interrupt.store(true, Ordering::Relaxed);
                    }

                    let _ = speech_tx.send(SpeechMessage {
                        text,
                        instruct,
                        _critical: critical,
                    });
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
                    continue;
                }
                Err(e) => {
                    eprintln!("commander: read error: {e}, reconnecting");
                    thread::sleep(Duration::from_secs(1));
                    break;
                }
            }
        }
    }

    // Cleanup
    let _ = fs::remove_file(&pid_path);
    drop(speech_tx);
    let _ = speech_thread.join();

    eprintln!("commander shutting down");
    Ok(())
}

/// One-shot TTS: speak a single text string using the configured engine.
pub fn say_text(text: &str) -> anyhow::Result<()> {
    let global_config = config::load_global_config().unwrap_or_default();
    let config = global_config.commander;

    let mut tts = TtsState::new(config.clone());
    tts.speak(&SpeechMessage {
        text: text.to_string(),
        instruct: config.instruct.clone(),
        _critical: false,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_speech_agent_completed() {
        let event = Event {
            event_type: "agent.completed".into(),
            project: "myapp".into(),
            source: "claude".into(),
            ts: String::new(),
            level: None,
            title: Some("Implemented auth".into()),
            body: None,
            meta: None,
            priority: None,
        };
        assert_eq!(render_speech(&event), "myapp: agent finished — Implemented auth");
    }

    #[test]
    fn render_speech_service_crashed() {
        let event = Event {
            event_type: "service.crashed".into(),
            project: "myapp".into(),
            source: "api-server".into(),
            ts: String::new(),
            level: None,
            title: None,
            body: None,
            meta: None,
            priority: None,
        };
        assert_eq!(render_speech(&event), "myapp: api-server crashed");
    }

    #[test]
    fn render_speech_fallback_to_type() {
        let event = Event {
            event_type: "build.failed".into(),
            project: "myapp".into(),
            source: "ci".into(),
            ts: String::new(),
            level: None,
            title: None,
            body: None,
            meta: None,
            priority: None,
        };
        assert_eq!(render_speech(&event), "myapp: build failed — build.failed");
    }

    #[test]
    fn speakable_events() {
        assert!(is_speakable_event("agent.completed"));
        assert!(is_speakable_event("service.crashed"));
        assert!(!is_speakable_event("workspace.created"));
        assert!(!is_speakable_event("random.event"));
    }

    #[test]
    fn should_speak_filters_by_event_type() {
        let speakable = Event {
            event_type: "agent.error".into(),
            project: "p".into(),
            source: "s".into(),
            ts: String::new(),
            level: None,
            title: None,
            body: None,
            meta: None,
            priority: None,
        };
        assert!(should_speak(&speakable));

        let not_speakable = Event {
            event_type: "workspace.created".into(),
            ..speakable.clone()
        };
        assert!(!should_speak(&not_speakable));
    }

    #[test]
    fn cooldown_first_event_speaks() {
        let mut tracker = CooldownTracker::new(5);
        assert!(matches!(
            tracker.check("proj", "agent.completed"),
            CooldownAction::Speak
        ));
    }

    #[test]
    fn cooldown_second_event_suppressed() {
        let mut tracker = CooldownTracker::new(5);
        tracker.check("proj", "agent.completed");
        assert!(matches!(
            tracker.check("proj", "agent.completed"),
            CooldownAction::Suppress
        ));
    }

    #[test]
    fn cooldown_different_types_independent() {
        let mut tracker = CooldownTracker::new(5);
        tracker.check("proj", "agent.completed");
        assert!(matches!(
            tracker.check("proj", "agent.error"),
            CooldownAction::Speak
        ));
    }

    #[test]
    fn shell_escape_basic() {
        assert_eq!(shell_escape("hello"), "'hello'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }
}
