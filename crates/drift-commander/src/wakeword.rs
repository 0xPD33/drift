use std::path::Path;

use rustpotter::{AudioFmt, Rustpotter, RustpotterConfig};

pub struct WakeWordDetector {
    rustpotter: Rustpotter,
    samples_per_frame: usize,
    buffer: Vec<f32>,
}

impl WakeWordDetector {
    pub fn new(wakeword_path: &Path) -> anyhow::Result<Self> {
        if !wakeword_path.exists() {
            anyhow::bail!("wake word file not found: {}", wakeword_path.display());
        }

        let config = RustpotterConfig {
            fmt: AudioFmt {
                sample_rate: 16000,
                channels: 1,
                ..Default::default()
            },
            detector: rustpotter::DetectorConfig {
                min_scores: 1,
                eager: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut rustpotter = Rustpotter::new(&config)
            .map_err(|e| anyhow::anyhow!("failed to create rustpotter: {e}"))?;

        let name = wakeword_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("wakeword");

        rustpotter
            .add_wakeword_from_file(name, wakeword_path.to_str().unwrap_or_default())
            .map_err(|e| anyhow::anyhow!("failed to load wake word: {e}"))?;

        let samples_per_frame = rustpotter.get_samples_per_frame();

        Ok(Self {
            rustpotter,
            samples_per_frame,
            buffer: Vec::with_capacity(samples_per_frame),
        })
    }

    /// Feed f32 audio samples (from PortAudio). Returns true if wake word detected.
    pub fn process_audio(&mut self, samples: &[f32]) -> bool {
        self.buffer.extend_from_slice(samples);

        while self.buffer.len() >= self.samples_per_frame {
            let frame: Vec<f32> = self.buffer.drain(..self.samples_per_frame).collect();
            if let Some(detection) = self.rustpotter.process_samples(frame) {
                eprintln!(
                    "wake word detected: {} (score: {:.2})",
                    detection.name, detection.score
                );
                self.buffer.clear();
                return true;
            }
        }
        false
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}
