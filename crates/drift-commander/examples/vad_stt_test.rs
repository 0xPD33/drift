//! Test VAD → STT pipeline with a WAV file.
//! Pads silence around speech so VAD can detect boundaries.
//! Run: cargo run -p drift-commander --example vad_stt_test

use drift_commander::post_process::post_process;
use drift_commander::stt::transcribe::SttEngine;
use drift_commander::vad::{SileroVad, VadConfig};

fn read_wav_pcm16(path: &std::path::Path) -> anyhow::Result<Vec<f32>> {
    let data = std::fs::read(path)?;
    if data.len() < 44 {
        anyhow::bail!("WAV file too small");
    }
    Ok(data[44..]
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
        .collect())
}

fn main() -> anyhow::Result<()> {
    let models_dir = drift_core::paths::models_dir();

    // Load models
    println!("Loading VAD...");
    let vad_config = VadConfig::default();
    let mut vad = SileroVad::new(vad_config, models_dir.join("silero_vad.onnx"))?;

    println!("Loading STT...");
    let mut stt = SttEngine::load(&models_dir.join("parakeet-tdt-0.6b-v3"))?;

    // Read test WAV
    let wav_path = std::env::args().nth(1).unwrap_or_else(|| {
        "/tmp/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/test_wavs/0.wav".into()
    });
    println!("Reading: {wav_path}");
    let speech = read_wav_pcm16(std::path::Path::new(&wav_path))?;
    println!("Speech: {} samples ({:.1}s)", speech.len(), speech.len() as f64 / 16000.0);

    // Create test audio: 0.5s silence + speech + 1s silence
    let silence_before = vec![0.0f32; 8000]; // 0.5s
    let silence_after = vec![0.0f32; 16000]; // 1s
    let mut audio = Vec::with_capacity(silence_before.len() + speech.len() + silence_after.len());
    audio.extend_from_slice(&silence_before);
    audio.extend_from_slice(&speech);
    audio.extend_from_slice(&silence_after);
    println!("Total audio: {:.1}s (with silence padding)", audio.len() as f64 / 16000.0);

    // Feed through VAD in chunks (simulating real-time mic input)
    let chunk_size = 1024;
    let mut all_segments = Vec::new();

    for chunk in audio.chunks(chunk_size) {
        match vad.process_audio(chunk) {
            Ok(segments) => {
                for seg in segments {
                    println!(
                        "  VAD segment: {:.2}s - {:.2}s ({} samples)",
                        seg.start_time, seg.end_time, seg.samples.len()
                    );
                    all_segments.push(seg);
                }
            }
            Err(e) => eprintln!("VAD error: {e}"),
        }
    }

    // Also flush any in-progress speech
    if let Some(seg) = vad.flush() {
        println!(
            "  VAD flush segment: {:.2}s - {:.2}s ({} samples)",
            seg.start_time, seg.end_time, seg.samples.len()
        );
        all_segments.push(seg);
    }

    println!("\nVAD detected {} segment(s)", all_segments.len());

    // Transcribe each segment
    for (i, seg) in all_segments.iter().enumerate() {
        match stt.transcribe(&seg.samples) {
            Ok(text) => {
                let text = post_process(&text);
                println!("  Segment {i}: \"{text}\"");
            }
            Err(e) => eprintln!("  Segment {i} STT error: {e}"),
        }
    }

    // Also transcribe the raw speech directly for comparison
    println!("\nDirect STT (no VAD):");
    let text = stt.transcribe(&speech)?;
    let text = post_process(&text);
    println!("  \"{text}\"");

    Ok(())
}
