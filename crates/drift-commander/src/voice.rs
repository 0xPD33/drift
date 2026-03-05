use std::io::Write;
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

    let vad_model_path = match models::ensure_vad_model() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("commander: VAD unavailable: {e}");
            return Ok(());
        }
    };

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

    let vad_config = VadConfig {
        threshold: 0.10,
        speech_end_threshold: 0.08,
        hangbefore_frames: 5,
        hangover_frames: 30,
        silence_tolerance_frames: 8,
        max_buffer_duration: 320000,
        max_segment_count: 10,
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
    let mut recording_samples: Vec<f32> = Vec::new();

    // Ring buffer: last 1s of audio for VAD LSTM warmup on wake word
    let ring_max = 16000; // 1s at 16kHz
    let mut ring_buffer: Vec<f32> = Vec::with_capacity(ring_max);

    while !shutdown.load(Ordering::Relaxed) {
        let samples = match audio_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(s) => s,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let VoiceState::Recording { started } = &state {
                    if started.elapsed() >= max_listen {
                        eprintln!("commander: max listen time reached, transcribing");
                        transcribe_and_handle(
                            &mut recording_samples,
                            &mut vad,
                            &mut stt,
                            config,
                            &speech_tx,
                        );
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
                // Maintain ring buffer for VAD warmup
                ring_buffer.extend_from_slice(&samples);
                if ring_buffer.len() > ring_max {
                    let excess = ring_buffer.len() - ring_max;
                    ring_buffer.drain(..excess);
                }

                if detector.process_audio(&samples) {
                    eprintln!("commander: wake word detected, recording...");
                    emit_voice_event("voice.wake", "wake word detected");

                    // Create fresh VAD, feed ring buffer to warm up LSTM,
                    // then reset segment tracking (keeping LSTM warm).
                    match SileroVad::new(vad_config.clone(), &vad_model_path) {
                        Ok(mut v) => {
                            // Warm up: feed 1s of audio history through the LSTM
                            let _ = v.process_audio(&ring_buffer);
                            // Clear segment tracking but keep warm LSTM state
                            v.warm_reset();
                            vad = Some(v);
                        }
                        Err(e) => {
                            eprintln!("commander: failed to create VAD: {e}");
                            detector.reset();
                            continue;
                        }
                    }

                    recording_samples.clear();
                    ring_buffer.clear();

                    // Feed current chunk to VAD so command audio
                    // overlapping the wake word chunk isn't lost
                    if let Some(ref mut v) = vad {
                        let _ = v.process_audio(&samples);
                    }

                    state = VoiceState::Recording {
                        started: Instant::now(),
                    };
                }
            }
            VoiceState::Recording { started } => {
                if started.elapsed() >= max_listen {
                    eprintln!("commander: max listen time reached, transcribing");
                    transcribe_and_handle(
                        &mut recording_samples,
                        &mut vad,
                        &mut stt,
                        config,
                        &speech_tx,
                    );
                    state = VoiceState::Listening;
                    detector.reset();
                    continue;
                }

                if let Some(ref mut v) = vad {
                    match v.process_audio(&samples) {
                        Ok(segments) => {
                            if !segments.is_empty() {
                                for seg in &segments {
                                    eprintln!(
                                        "commander: VAD segment {:.2}s-{:.2}s ({} samples)",
                                        seg.start_time, seg.end_time, seg.samples.len()
                                    );
                                }
                                recording_samples
                                    .extend(segments.into_iter().flat_map(|s| s.samples));

                                transcribe_and_handle(
                                    &mut recording_samples,
                                    &mut vad,
                                    &mut stt,
                                    config,
                                    &speech_tx,
                                );
                                state = VoiceState::Listening;
                                detector.reset();
                            }
                        }
                        Err(e) => {
                            eprintln!("commander: VAD error: {e}");
                            state = VoiceState::Listening;
                            vad = None;
                            recording_samples.clear();
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

fn transcribe_and_handle(
    samples: &mut Vec<f32>,
    vad: &mut Option<SileroVad>,
    stt: &mut SttEngine,
    config: &CommanderConfig,
    speech_tx: &Option<mpsc::Sender<crate::SpeechMessage>>,
) {
    // Flush any remaining speech from VAD
    if let Some(ref mut v) = vad {
        if let Some(seg) = v.flush() {
            eprintln!(
                "commander: VAD flush segment {:.2}s-{:.2}s ({} samples)",
                seg.start_time, seg.end_time, seg.samples.len()
            );
            samples.extend(seg.samples);
        }
    }
    *vad = None;

    if samples.is_empty() {
        eprintln!("commander: no speech captured");
        return;
    }

    let duration = samples.len() as f64 / 16000.0;
    eprintln!(
        "commander: transcribing {:.1}s of audio ({} samples)",
        duration,
        samples.len()
    );

    debug_save_wav("/tmp/drift-stt-raw.wav", samples, 16000);

    // Prepend 150ms of silence as preroll for mel spectrogram context
    const PREROLL_SAMPLES: usize = 16000 * 150 / 1000;
    let mut with_preroll = vec![0.0f32; PREROLL_SAMPLES];
    with_preroll.extend_from_slice(samples);

    let stt_start = Instant::now();
    match stt.transcribe(&with_preroll) {
        Ok(text) => {
            let stt_elapsed = stt_start.elapsed();
            let text = post_process(&text);
            eprintln!("commander: STT took {:.1}s", stt_elapsed.as_secs_f64());
            handle_transcript(&text, config, speech_tx);
        }
        Err(e) => {
            eprintln!("commander: STT error: {e}");
        }
    }

    samples.clear();
}

fn debug_save_wav(path: &str, samples: &[f32], sample_rate: u32) {
    let num_samples = samples.len() as u32;
    let bytes_per_sample = 2u16;
    let num_channels = 1u16;
    let data_size = num_samples * bytes_per_sample as u32;
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(44 + data_size as usize);

    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(
        &(sample_rate * bytes_per_sample as u32 * num_channels as u32).to_le_bytes(),
    );
    buf.extend_from_slice(&(bytes_per_sample * num_channels).to_le_bytes());
    buf.extend_from_slice(&(bytes_per_sample * 8).to_le_bytes());

    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let i = (clamped * 32767.0) as i16;
        buf.extend_from_slice(&i.to_le_bytes());
    }

    match std::fs::File::create(path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(&buf) {
                eprintln!("commander: failed to write debug WAV {path}: {e}");
            } else {
                eprintln!("commander: saved debug WAV {path} ({num_samples} samples)");
            }
        }
        Err(e) => eprintln!("commander: failed to create debug WAV {path}: {e}"),
    }
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

fn speak_feedback(
    speech_tx: &Option<mpsc::Sender<crate::SpeechMessage>>,
    text: &str,
    instruct: &str,
) {
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

    let cmd = crate::command_llm::parse_command_llm(text, config)
        .unwrap_or_else(|| crate::command::parse_command(text));
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
