//! Debug VAD probabilities to understand detection behavior.
//! Run: cargo run -p drift-commander --example vad_debug

use ndarray::{s, Array, Array2, ArrayD};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::{Session, SessionInputs};
use ort::value::Tensor;

fn read_wav_pcm16(path: &std::path::Path) -> anyhow::Result<Vec<f32>> {
    let data = std::fs::read(path)?;
    Ok(data[44..]
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
        .collect())
}

fn main() -> anyhow::Result<()> {
    let models_dir = drift_core::paths::models_dir();
    let vad_path = models_dir.join("silero_vad.onnx");

    let mut session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(1)?
        .with_inter_threads(1)?
        .commit_from_file(&vad_path)?;

    println!("VAD model loaded ({} inputs, {} outputs)",
        session.inputs().len(), session.outputs().len());

    // Print input/output info
    for (i, input) in session.inputs().iter().enumerate() {
        println!("  Input {i}: name={:?}", input.name());
    }
    for (i, output) in session.outputs().iter().enumerate() {
        println!("  Output {i}: name={:?}", output.name());
    }

    // Read test WAV
    let wav_path = std::env::args().nth(1).unwrap_or_else(|| {
        "/tmp/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8/test_wavs/0.wav".into()
    });
    let speech = read_wav_pcm16(std::path::Path::new(&wav_path))?;

    // Audio stats
    let min = speech.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = speech.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let rms = (speech.iter().map(|s| s * s).sum::<f32>() / speech.len() as f32).sqrt();
    println!("\nSpeech: {} samples ({:.1}s), min={min:.4}, max={max:.4}, rms={rms:.4}",
        speech.len(), speech.len() as f64 / 16000.0);

    // silence + speech + silence
    let mut audio = vec![0.0f32; 8000];
    audio.extend_from_slice(&speech);
    audio.extend_from_slice(&vec![0.0f32; 16000]);

    println!("Total audio: {:.1}s\n", audio.len() as f64 / 16000.0);

    // Process frame by frame - try different frame sizes
    for &frame_size in &[512usize, 1536] {
        println!("=== Frame size: {frame_size} ===");
        let hop = frame_size; // non-overlapping for simplicity
        let mut state = ArrayD::<f32>::zeros([2, 1, 128].as_slice());
        let sample_rate_arr = Array::from_shape_vec([1], vec![16000i64]).unwrap();

        let mut frame_buffer = Array2::<f32>::zeros((1, frame_size));
        let mut pos = 0;
        let mut frame_idx = 0;
        let mut max_prob: f32 = 0.0;

        while pos + frame_size <= audio.len() {
            let frame = &audio[pos..pos + frame_size];
            for i in 0..frame_size {
                frame_buffer[[0, i]] = frame[i];
            }

            let frame_slice = frame_buffer.slice(s![.., ..frame_size]);
            let frame_tensor = Tensor::from_array(frame_slice.to_owned())?;
            let state_tensor = Tensor::from_array(std::mem::take(&mut state))?;
            let sr_tensor = Tensor::from_array(sample_rate_arr.to_owned())?;

            let inps = ort::inputs![frame_tensor, state_tensor, sr_tensor];
            let res = session.run(SessionInputs::ValueSlice::<3>(&inps))?;

            state = res["stateN"].try_extract_array()?.to_owned();
            let output = res["output"].try_extract_tensor::<f32>()?;
            let prob = output.1[0];
            max_prob = max_prob.max(prob);

            let time = pos as f64 / 16000.0;
            if frame_idx % 5 == 0 || prob > 0.1 {
                let frame_rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
                println!("t={time:.2}s  prob={prob:.4}  rms={frame_rms:.4}  {}",
                    if prob > 0.2 { "<<< SPEECH" } else { "" });
            }

            pos += hop;
            frame_idx += 1;
        }
        println!("Max prob: {max_prob:.4}\n");

        // Reset state for next frame size
        state = ArrayD::<f32>::zeros([2, 1, 128].as_slice());
    }

    // Also test with the SileroVad wrapper
    println!("=== Using SileroVad wrapper ===");
    let vad_config = drift_commander::vad::VadConfig::default();
    let mut vad = drift_commander::vad::SileroVad::new(vad_config, &vad_path)?;

    let chunk_size = 1024;
    let mut total_segments = 0;
    for chunk in audio.chunks(chunk_size) {
        match vad.process_audio(chunk) {
            Ok(segments) => {
                for seg in &segments {
                    println!("  Segment: {:.2}s - {:.2}s ({} samples)",
                        seg.start_time, seg.end_time, seg.samples.len());
                    total_segments += 1;
                }
            }
            Err(e) => eprintln!("VAD error: {e}"),
        }
    }
    if let Some(seg) = vad.flush() {
        println!("  Flush segment: {:.2}s - {:.2}s ({} samples)",
            seg.start_time, seg.end_time, seg.samples.len());
        total_segments += 1;
    }
    println!("Total segments: {total_segments}");
    println!("Speaking: {}", vad.is_speaking());

    Ok(())
}
