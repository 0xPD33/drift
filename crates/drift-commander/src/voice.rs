use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use drift_core::config::CommanderConfig;

use crate::audio::AudioCapture;
use crate::models;
use crate::post_process::post_process;
use crate::stt::transcribe::SttEngine;
use crate::vad::{SileroVad, VadConfig};
use crate::wakeword::WakeWordDetector;

enum VoiceState {
    Listening,
    Recording { started: Instant },
}

pub(crate) fn run_voice_loop(
    config: &CommanderConfig,
    shutdown: &AtomicBool,
    speech_tx: Option<mpsc::Sender<crate::SpeechMessage>>,
) -> anyhow::Result<()> {
    if !config.voice_enabled {
        eprintln!("commander: voice control disabled");
        return Ok(());
    }

    // Load wake word model
    let models_dir = models::models_dir();
    let wakeword_path = models_dir.join(format!("{}.rpw", config.wake_word));

    let mut detector = match WakeWordDetector::new(&wakeword_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("commander: voice control unavailable: {e}");
            eprintln!("commander: run 'drift commander train' to create a wake word model");
            return Ok(());
        }
    };

    // Load VAD model
    let vad_model_path = match models::ensure_vad_model() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("commander: VAD unavailable: {e}");
            return Ok(());
        }
    };

    // Load STT engine
    let stt_model_dir = match models::ensure_stt_model(&config.stt_model) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("commander: STT unavailable: {e}");
            return Ok(());
        }
    };

    let mut stt = match SttEngine::load(&stt_model_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("commander: failed to load STT engine: {e}");
            return Ok(());
        }
    };

    // Create VAD with config threshold
    let vad_config = VadConfig {
        threshold: config.vad_threshold,
        ..VadConfig::default()
    };

    eprintln!(
        "commander: voice control active, listening for '{}'",
        config.wake_word
    );

    let running = Arc::new(AtomicBool::new(true));
    let (audio_tx, audio_rx) = mpsc::sync_channel::<Vec<f32>>(64);

    let mut capture = AudioCapture::new();
    capture.start(audio_tx, running.clone())?;

    let max_listen = Duration::from_secs(config.max_listen_sec);
    let mut state = VoiceState::Listening;
    let mut vad: Option<SileroVad> = None;

    while !shutdown.load(Ordering::Relaxed) {
        let samples = match audio_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(s) => s,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Check timeout in Recording state even without new audio
                if let VoiceState::Recording { started } = &state {
                    if started.elapsed() >= max_listen {
                        eprintln!("commander: max listen time reached, transcribing");
                        let transcript = finish_recording(&mut vad, &mut stt);
                        handle_transcript(&transcript, config, &speech_tx);
                        state = VoiceState::Listening;
                        detector.reset();
                    }
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        match state {
            VoiceState::Listening => {
                if detector.process_audio(&samples) {
                    eprintln!("commander: wake word detected, recording...");
                    emit_voice_event("voice.wake", "wake word detected");

                    // Initialize fresh VAD
                    match SileroVad::new(vad_config.clone(), &vad_model_path) {
                        Ok(v) => vad = Some(v),
                        Err(e) => {
                            eprintln!("commander: failed to create VAD: {e}");
                            detector.reset();
                            continue;
                        }
                    }

                    state = VoiceState::Recording {
                        started: Instant::now(),
                    };
                }
            }
            VoiceState::Recording { started } => {
                if started.elapsed() >= max_listen {
                    eprintln!("commander: max listen time reached, transcribing");
                    let transcript = finish_recording(&mut vad, &mut stt);
                    handle_transcript(&transcript, config, &speech_tx);
                    state = VoiceState::Listening;
                    detector.reset();
                    continue;
                }

                if let Some(ref mut v) = vad {
                    match v.process_audio(&samples) {
                        Ok(segments) => {
                            if !segments.is_empty() {
                                // Speech segment completed — VAD detected silence after speech
                                let all_samples: Vec<f32> = segments
                                    .into_iter()
                                    .flat_map(|s| s.samples)
                                    .collect();

                                if !all_samples.is_empty() {
                                    match stt.transcribe(&all_samples) {
                                        Ok(text) => {
                                            let text = post_process(&text);
                                            handle_transcript(&text, config, &speech_tx);
                                        }
                                        Err(e) => {
                                            eprintln!("commander: STT error: {e}");
                                        }
                                    }
                                }

                                state = VoiceState::Listening;
                                vad = None;
                                detector.reset();
                            }
                        }
                        Err(e) => {
                            eprintln!("commander: VAD error: {e}");
                            state = VoiceState::Listening;
                            vad = None;
                            detector.reset();
                        }
                    }
                }
            }
        }
    }

    running.store(false, Ordering::Relaxed);
    capture.stop();
    eprintln!("commander: voice control stopped");
    Ok(())
}

/// Force-flush the VAD and transcribe whatever was collected.
fn finish_recording(vad: &mut Option<SileroVad>, stt: &mut SttEngine) -> String {
    if let Some(ref mut v) = vad {
        if let Some(segment) = v.flush() {
            if !segment.samples.is_empty() {
                match stt.transcribe(&segment.samples) {
                    Ok(text) => return post_process(&text),
                    Err(e) => {
                        eprintln!("commander: STT error on flush: {e}");
                    }
                }
            }
        }
    }
    *vad = None;
    String::new()
}

fn emit_voice_event(event_type: &str, title: &str) {
    use drift_core::events::{self, Event};
    events::try_emit_event(&Event {
        event_type: event_type.into(),
        project: String::new(),
        source: "commander".into(),
        ts: events::iso_now(),
        level: Some("info".into()),
        title: Some(title.into()),
        body: None,
        meta: None,
        priority: Some("low".into()),
    });
}

fn speak_feedback(speech_tx: &Option<mpsc::Sender<crate::SpeechMessage>>, text: &str, instruct: &str) {
    if let Some(tx) = speech_tx {
        let _ = tx.send(crate::SpeechMessage {
            text: text.into(),
            instruct: instruct.into(),
            _critical: false,
        });
    }
}

fn handle_transcript(
    text: &str,
    config: &CommanderConfig,
    speech_tx: &Option<mpsc::Sender<crate::SpeechMessage>>,
) {
    if text.is_empty() {
        eprintln!("commander: (empty transcript, ignoring)");
        return;
    }
    eprintln!("commander: transcript: \"{text}\"");

    let cmd = crate::command::parse_command(text);
    eprintln!("commander: parsed command: {cmd:?}");

    let result = crate::action::execute(&cmd);
    if result.success {
        eprintln!("commander: action ok: {}", result.message);
        emit_voice_event("voice.command", &result.message);
        if config.speak_feedback {
            speak_feedback(speech_tx, &result.message, &config.instruct);
        }
    } else {
        eprintln!("commander: action failed: {}", result.message);
        emit_voice_event("voice.error", &result.message);
        if config.speak_feedback {
            speak_feedback(speech_tx, &result.message, &config.instruct);
        }
    }
}
