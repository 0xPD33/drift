//! Live mic test: records for a few seconds and transcribes.
//! Bypasses wake word — directly tests audio capture → VAD → STT.
//! Run: cargo run -p drift-commander --example mic_test

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use drift_commander::audio::AudioCapture;
use drift_commander::post_process::post_process;
use drift_commander::stt::transcribe::SttEngine;
use drift_commander::vad::{SileroVad, VadConfig};

fn main() -> anyhow::Result<()> {
    let models_dir = drift_core::paths::models_dir();

    // Load models
    eprintln!("Loading VAD...");
    let vad_config = VadConfig::default();
    let mut vad = SileroVad::new(vad_config, models_dir.join("silero_vad.onnx"))?;

    eprintln!("Loading STT...");
    let mut stt = SttEngine::load(&models_dir.join("parakeet-tdt-0.6b-v3"))?;

    // Start audio capture
    eprintln!("Starting mic capture... Speak a command (recording for 5 seconds)");
    let running = Arc::new(AtomicBool::new(true));
    let (audio_tx, audio_rx) = mpsc::sync_channel::<Vec<f32>>(64);

    let mut capture = AudioCapture::new();
    capture.start(audio_tx, running.clone())?;

    let start = Instant::now();
    let max_duration = Duration::from_secs(5);
    let mut all_samples = Vec::new();
    let mut segment_found = false;

    while start.elapsed() < max_duration {
        match audio_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(samples) => {
                all_samples.extend_from_slice(&samples);

                match vad.process_audio(&samples) {
                    Ok(segments) => {
                        if !segments.is_empty() {
                            for seg in &segments {
                                eprintln!(
                                    "VAD segment: {:.2}s - {:.2}s ({} samples)",
                                    seg.start_time,
                                    seg.end_time,
                                    seg.samples.len()
                                );
                                match stt.transcribe(&seg.samples) {
                                    Ok(text) => {
                                        let text = post_process(&text);
                                        println!("Transcript: \"{text}\"");
                                    }
                                    Err(e) => eprintln!("STT error: {e}"),
                                }
                            }
                            segment_found = true;
                        }
                    }
                    Err(e) => eprintln!("VAD error: {e}"),
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    running.store(false, Ordering::Relaxed);
    capture.stop();

    if !segment_found {
        eprintln!(
            "\nNo speech segments detected by VAD. Transcribing all captured audio ({:.1}s)...",
            all_samples.len() as f64 / 16000.0
        );
        match stt.transcribe(&all_samples) {
            Ok(text) => {
                let text = post_process(&text);
                println!("Full transcript: \"{text}\"");
            }
            Err(e) => eprintln!("STT error: {e}"),
        }
    }

    eprintln!("Done.");
    Ok(())
}
