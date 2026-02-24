//! Smoke test: loads models and transcribes a test WAV file.
//! Run: cargo run -p drift-commander --example smoke_test -- /path/to/test.wav

use std::path::PathBuf;

fn read_wav_pcm16(path: &std::path::Path) -> anyhow::Result<Vec<f32>> {
    let data = std::fs::read(path)?;
    // Skip 44-byte WAV header, read i16 samples, convert to f32
    if data.len() < 44 {
        anyhow::bail!("WAV file too small");
    }
    let samples: Vec<f32> = data[44..]
        .chunks_exact(2)
        .map(|chunk| {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            sample as f32 / 32768.0
        })
        .collect();
    Ok(samples)
}

fn main() -> anyhow::Result<()> {
    let wav_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/test_wavs/0.wav".into());

    let models_dir = drift_core::paths::models_dir();

    // Test 1: Load VAD
    println!("=== Test 1: Silero VAD ===");
    let vad_path = models_dir.join("silero_vad.onnx");
    println!("Loading VAD from: {}", vad_path.display());
    let vad_config = drift_commander::vad::VadConfig::default();
    let mut vad = drift_commander::vad::SileroVad::new(vad_config, &vad_path)?;
    println!("VAD loaded OK");

    // Test 2: Load STT
    println!("\n=== Test 2: Parakeet STT ===");
    let stt_dir = models_dir.join("parakeet-tdt-0.6b-v3");
    let mut stt = drift_commander::stt::transcribe::SttEngine::load(&stt_dir)?;
    println!("STT loaded OK");

    // Test 3: Read WAV and transcribe
    println!("\n=== Test 3: Transcribe ===");
    let wav = PathBuf::from(&wav_path);
    println!("Reading: {}", wav.display());
    let samples = read_wav_pcm16(&wav)?;
    println!("Samples: {} ({:.1}s)", samples.len(), samples.len() as f64 / 16000.0);

    let text = stt.transcribe(&samples)?;
    let text = drift_commander::post_process::post_process(&text);
    println!("Transcript: \"{text}\"");

    // Test 4: Feed audio through VAD
    println!("\n=== Test 4: VAD processing ===");
    let segments = vad.process_audio(&samples)?;
    println!("VAD returned {} segments", segments.len());
    for (i, seg) in segments.iter().enumerate() {
        println!(
            "  Segment {}: {:.2}s - {:.2}s ({} samples)",
            i,
            seg.start_time,
            seg.end_time,
            seg.samples.len()
        );
    }
    println!("VAD speaking: {}", vad.is_speaking());

    println!("\n=== All tests passed! ===");
    Ok(())
}
