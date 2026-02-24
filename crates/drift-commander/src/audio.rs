use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;

use portaudio as pa;

const SAMPLE_RATE: f64 = 16_000.0;
const DEFAULT_BUFFER_SIZE: u32 = 1024;

pub struct AudioCapture {
    pa_stream: Option<pa::Stream<pa::NonBlocking, pa::Input<f32>>>,
    pa: Option<pa::PortAudio>,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            pa_stream: None,
            pa: None,
        }
    }

    /// Start capturing audio. Sends chunks of f32 samples via the provided channel.
    /// Runs until `running` is set to false.
    pub fn start(
        &mut self,
        tx: SyncSender<Vec<f32>>,
        running: Arc<AtomicBool>,
    ) -> anyhow::Result<()> {
        let pa = pa::PortAudio::new()
            .map_err(|e| anyhow::anyhow!("failed to initialize PortAudio: {e}"))?;

        let input_params = pa
            .default_input_stream_params::<f32>(1)
            .map_err(|e| anyhow::anyhow!("failed to get default input params: {e}"))?;

        let settings = pa::InputStreamSettings::new(input_params, SAMPLE_RATE, DEFAULT_BUFFER_SIZE);

        let callback = move |pa::InputStreamCallbackArgs { buffer, .. }| {
            let _ = tx.try_send(buffer.to_vec());
            if running.load(Ordering::Relaxed) {
                pa::Continue
            } else {
                pa::Complete
            }
        };

        let mut stream = pa
            .open_non_blocking_stream(settings, callback)
            .map_err(|e| anyhow::anyhow!("failed to open audio stream: {e}"))?;

        stream
            .start()
            .map_err(|e| anyhow::anyhow!("failed to start audio stream: {e}"))?;

        self.pa = Some(pa);
        self.pa_stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(stream) = &mut self.pa_stream {
            let _ = stream.stop();
            let _ = stream.close();
        }
        self.pa_stream = None;
        self.pa = None;
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
