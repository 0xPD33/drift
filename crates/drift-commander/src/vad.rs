use ndarray::{s, Array, Array2, ArrayD, Dim, IxDynImpl, OwnedRepr};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::{Session, SessionInputs};
use ort::value::Tensor;
use std::collections::VecDeque;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VadState {
    Silence,
    PossibleSpeech,
    Speech,
    PossibleSilence,
}

#[derive(Debug, Clone)]
pub struct AudioSegment {
    pub samples: Vec<f32>,
    pub start_time: f64,
    pub end_time: f64,
    pub sample_rate: usize,
}

#[derive(Debug, Clone)]
pub struct VadConfig {
    pub threshold: f32,
    pub frame_size: usize,
    pub sample_rate: usize,
    pub hangbefore_frames: usize,
    pub hangover_frames: usize,
    pub hop_samples: usize,
    pub max_buffer_duration: usize,
    pub max_segment_count: usize,
    pub silence_tolerance_frames: usize,
    pub speech_end_threshold: f32,
    pub speech_prob_smoothing: f32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            threshold: 0.2,
            frame_size: 512,
            sample_rate: 16000,
            hangbefore_frames: 3,
            hangover_frames: 20,
            hop_samples: 160,
            max_buffer_duration: 160000, // 10s at 16kHz
            max_segment_count: 5,
            silence_tolerance_frames: 5,
            speech_end_threshold: 0.15,
            speech_prob_smoothing: 0.3,
        }
    }
}

#[derive(Debug)]
pub struct SileroVad {
    session: Session,
    sample_rate: ndarray::ArrayBase<OwnedRepr<i64>, Dim<[usize; 1]>>,
    state: ndarray::ArrayBase<OwnedRepr<f32>, Dim<IxDynImpl>>,
    config: VadConfig,
    buffer: VecDeque<f32>,
    speeches: Vec<AudioSegment>,
    current_state: VadState,
    frames_in_state: usize,
    silence_frames: usize,
    current_time: f64,
    time_offset: f64,
    speech_start_time: Option<f64>,
    smoothed_prob: f32,
    sample_buffer: Vec<f32>,
    frame_buffer: Array2<f32>,
    sample_rate_f64: f64,
    frame_counter: usize,
    buffer_check_interval: usize,
    samples_since_trim: usize,
    trim_threshold: usize,
}

impl SileroVad {
    pub fn new(config: VadConfig, model_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let frame_size = config.frame_size;

        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(1)?
            .with_inter_threads(1)?
            .commit_from_file(model_path)?;

        let state = ArrayD::<f32>::zeros([2, 1, 128].as_slice());
        let sample_rate_arr = Array::from_shape_vec([1], vec![config.sample_rate as i64])?;
        let frame_buffer = Array2::<f32>::zeros((1, frame_size));
        let sample_rate_f64 = config.sample_rate as f64;
        let max_buffer_duration = config.max_buffer_duration;
        let max_segment_count = config.max_segment_count;

        let buffer_check_interval = 30;
        let trim_threshold = frame_size * 60;

        Ok(Self {
            session,
            sample_rate: sample_rate_arr,
            state,
            config,
            buffer: VecDeque::with_capacity(frame_size * 2),
            speeches: Vec::with_capacity(max_segment_count),
            current_state: VadState::Silence,
            frames_in_state: 0,
            silence_frames: 0,
            current_time: 0.0,
            time_offset: 0.0,
            speech_start_time: None,
            smoothed_prob: 0.0,
            sample_buffer: Vec::with_capacity(max_buffer_duration),
            frame_buffer,
            sample_rate_f64,
            frame_counter: 0,
            buffer_check_interval,
            samples_since_trim: 0,
            trim_threshold,
        })
    }

    pub fn reset(&mut self) {
        self.state = ArrayD::<f32>::zeros([2, 1, 128].as_slice());
        self.buffer.clear();
        self.speeches.clear();
        self.current_state = VadState::Silence;
        self.frames_in_state = 0;
        self.silence_frames = 0;
        self.current_time = 0.0;
        self.time_offset = 0.0;
        self.speech_start_time = None;
        self.smoothed_prob = 0.0;
        self.sample_buffer.clear();
        self.frame_counter = 0;
        self.samples_since_trim = 0;
    }

    fn calc_speech_prob(&mut self, audio_frame: &[f32]) -> Result<f32, ort::Error> {
        let frame_len = audio_frame.len().min(512);

        for i in 0..frame_len {
            self.frame_buffer[[0, i]] = if i < audio_frame.len() {
                audio_frame[i]
            } else {
                0.0
            };
        }

        let frame = self.frame_buffer.slice(s![.., ..frame_len]);

        let frame_tensor = Tensor::from_array(frame.to_owned())?;
        let state_tensor = Tensor::from_array(std::mem::take(&mut self.state))?;
        let sample_rate_tensor = Tensor::from_array(self.sample_rate.to_owned())?;

        let inps = ort::inputs![frame_tensor, state_tensor, sample_rate_tensor,];
        let res = self.session.run(SessionInputs::ValueSlice::<3>(&inps))?;

        self.state = res["stateN"].try_extract_array()?.to_owned();
        let output_tensor = res["output"].try_extract_tensor::<f32>()?;
        Ok(output_tensor.1[0])
    }

    fn process_frame(&mut self, frame: &[f32], hop_len: usize) -> Result<VadState, ort::Error> {
        let raw_prob = self.calc_speech_prob(frame)?;

        let alpha = self.config.speech_prob_smoothing;
        self.smoothed_prob = alpha * raw_prob + (1.0 - alpha) * self.smoothed_prob;

        self.update_vad_state(raw_prob, self.smoothed_prob);

        let effective_hop = if self.sample_buffer.is_empty() {
            frame.len()
        } else {
            hop_len.min(frame.len())
        };

        let time_increment = effective_hop as f64 / self.sample_rate_f64;
        self.current_time += time_increment;

        let start_idx = frame.len().saturating_sub(effective_hop);
        self.sample_buffer.extend_from_slice(&frame[start_idx..]);

        self.samples_since_trim += effective_hop;

        self.frame_counter += 1;
        if self.frame_counter >= self.buffer_check_interval {
            self.frame_counter = 0;
            self.trim_buffer_if_needed();
        }

        Ok(self.current_state)
    }

    fn trim_buffer_if_needed(&mut self) {
        if self.sample_buffer.len() <= self.config.max_buffer_duration {
            return;
        }

        let excess = self.sample_buffer.len() - self.config.max_buffer_duration;
        let min_trim = self.min_trim_samples();
        let trim_samples = excess.max(min_trim).min(self.sample_buffer.len());

        let time_trimmed = trim_samples as f64 / self.sample_rate_f64;
        let new_time_offset = self.time_offset + time_trimmed;

        self.trim_buffer(trim_samples, new_time_offset);
        self.samples_since_trim = 0;
    }

    fn trim_buffer(&mut self, trim_samples: usize, new_time_offset: f64) {
        if trim_samples == 0 {
            return;
        }

        self.time_offset = new_time_offset;

        if let Some(start_time) = self.speech_start_time {
            if start_time < new_time_offset {
                let segment = AudioSegment {
                    samples: self.extract_speech_segment(start_time, new_time_offset),
                    start_time,
                    end_time: new_time_offset,
                    sample_rate: self.config.sample_rate,
                };

                if !segment.samples.is_empty() {
                    self.speeches.push(segment);
                    if self.speeches.len() > self.config.max_segment_count {
                        self.speeches.remove(0);
                    }
                }

                self.speech_start_time = Some(new_time_offset);
            }
        }

        self.sample_buffer.drain(0..trim_samples);
    }

    fn update_vad_state(&mut self, raw_prob: f32, smoothed_prob: f32) {
        let threshold = self.config.threshold;
        let speech_end_threshold = self.config.speech_end_threshold;
        let hangbefore_frames = self.config.hangbefore_frames;
        let hangover_frames = self.config.hangover_frames;
        let silence_tolerance_frames = self.config.silence_tolerance_frames;

        let detection_prob = if self.current_state == VadState::Silence {
            raw_prob
        } else {
            smoothed_prob
        };

        let is_starting_speech = detection_prob > threshold;
        let is_continuing_speech = detection_prob > speech_end_threshold;

        match self.current_state {
            VadState::Silence => {
                if is_starting_speech {
                    self.current_state = VadState::PossibleSpeech;
                    self.frames_in_state = 1;
                }
            }
            VadState::PossibleSpeech => {
                if is_starting_speech {
                    self.frames_in_state += 1;
                    self.silence_frames = 0;

                    if self.frames_in_state >= hangbefore_frames {
                        let hop = self.config.hop_samples.max(1);
                        let frame_samples = self.config.frame_size;

                        let lookback_samples = if hangbefore_frames == 0 {
                            0
                        } else {
                            frame_samples + (hangbefore_frames - 1) * hop
                        };
                        let lookback_time = lookback_samples as f64 / self.sample_rate_f64;

                        self.speech_start_time =
                            Some((self.current_time - lookback_time).max(0.0));
                        self.current_state = VadState::Speech;
                        self.frames_in_state = 0;
                    }
                } else if is_continuing_speech {
                    self.silence_frames = 0;
                } else {
                    self.silence_frames += 1;
                    if self.silence_frames >= silence_tolerance_frames {
                        self.current_state = VadState::Silence;
                        self.frames_in_state = 0;
                        self.silence_frames = 0;
                    }
                }
            }
            VadState::Speech => {
                if !is_continuing_speech {
                    self.current_state = VadState::PossibleSilence;
                    self.frames_in_state = 1;
                }
            }
            VadState::PossibleSilence => {
                if !is_continuing_speech {
                    self.frames_in_state += 1;
                    if self.frames_in_state >= hangover_frames {
                        self.current_state = VadState::Silence;
                        self.frames_in_state = 0;
                        self.finalize_speech_segment();
                    }
                } else {
                    self.current_state = VadState::Speech;
                    self.frames_in_state = 0;
                }
            }
        }
    }

    fn finalize_speech_segment(&mut self) {
        if let Some(start_time) = self.speech_start_time.take() {
            let segment = AudioSegment {
                samples: self.extract_speech_segment(start_time, self.current_time),
                start_time,
                end_time: self.current_time,
                sample_rate: self.config.sample_rate,
            };

            if !segment.samples.is_empty() {
                self.speeches.push(segment);
                if self.speeches.len() > self.config.max_segment_count {
                    self.speeches.remove(0);
                }
            }
        }
    }

    fn extract_speech_segment(&mut self, start_time: f64, end_time: f64) -> Vec<f32> {
        let context_duration = 0.1;

        let adjusted_start = (start_time - self.time_offset - context_duration).max(0.0);
        let adjusted_end = (end_time - self.time_offset).max(0.0);

        let to_idx = |time: f64| -> usize { (time * self.sample_rate_f64) as usize };

        let start_idx = to_idx(adjusted_start).min(self.sample_buffer.len());
        let end_idx = to_idx(adjusted_end).min(self.sample_buffer.len());

        if start_idx >= end_idx || start_idx >= self.sample_buffer.len() {
            return Vec::new();
        }

        self.sample_buffer[start_idx..end_idx].to_vec()
    }

    fn min_trim_samples(&self) -> usize {
        let max_buffer = self.config.max_buffer_duration.max(1);
        let sample_rate = self.config.sample_rate.max(1);
        let frame_size = self.config.frame_size.max(1);

        let fraction = (max_buffer / 6).max(frame_size);
        let half_second = (sample_rate / 2).max(frame_size);

        fraction.max(half_second).min(max_buffer).max(1)
    }

    pub fn process_audio(&mut self, samples: &[f32]) -> Result<Vec<AudioSegment>, ort::Error> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }

        let frame_size = self.config.frame_size;
        let hop_samples = self.config.hop_samples.max(1);
        let mut frame = Vec::with_capacity(frame_size);

        self.buffer.extend(samples);

        while self.buffer.len() >= frame_size {
            frame.clear();
            frame.extend(self.buffer.iter().take(frame_size).copied());

            let hop = hop_samples.min(frame.len());
            self.process_frame(&frame, hop)?;

            let drain = hop.min(self.buffer.len());
            self.buffer.drain(0..drain);
        }

        let partial_threshold = frame_size / 8;
        if !self.buffer.is_empty() && self.buffer.len() >= partial_threshold {
            frame.clear();
            frame.resize(frame_size, 0.0);

            let remaining = self.buffer.len();
            {
                let contiguous = self.buffer.make_contiguous();
                frame[0..remaining].copy_from_slice(&contiguous[0..remaining]);
            }

            self.process_frame(&frame, remaining)?;
            self.buffer.clear();
        }

        if self.samples_since_trim >= self.trim_threshold {
            self.samples_since_trim = 0;

            let max_buffer = self.config.max_buffer_duration;
            let current_size = self.sample_buffer.len();

            if current_size > max_buffer * 3 / 4 {
                let target_size = max_buffer / 2;
                let excess = current_size - target_size;

                let time_trimmed = excess as f64 / self.sample_rate_f64;
                let new_time_offset = self.time_offset + time_trimmed;

                self.trim_buffer(excess, new_time_offset);
            }
        }

        if self.speeches.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(std::mem::take(&mut self.speeches))
        }
    }

    pub fn is_speaking(&self) -> bool {
        self.current_state == VadState::Speech || self.current_state == VadState::PossibleSpeech
    }

    /// Force-finalize any in-progress speech and return all collected samples.
    pub fn flush(&mut self) -> Option<AudioSegment> {
        if let Some(start_time) = self.speech_start_time.take() {
            let segment = AudioSegment {
                samples: self.extract_speech_segment(start_time, self.current_time),
                start_time,
                end_time: self.current_time,
                sample_rate: self.config.sample_rate,
            };
            self.current_state = VadState::Silence;
            self.frames_in_state = 0;
            if !segment.samples.is_empty() {
                return Some(segment);
            }
        }
        None
    }
}
