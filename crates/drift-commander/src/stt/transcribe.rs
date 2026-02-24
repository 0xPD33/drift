use std::path::Path;

use ndarray::ArrayD;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

use super::decoder::greedy_decode_tdt;
use super::mel::MelSpectrogram;
use super::tokenizer::ParakeetTokenizer;

pub struct SttEngine {
    encoder: Session,
    decoder: Session,
    joiner: Session,
    mel: MelSpectrogram,
    tokenizer: ParakeetTokenizer,
}

fn load_session(path: &Path) -> anyhow::Result<Session> {
    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(1)?
        .with_inter_threads(1)?
        .commit_from_file(path)?;
    Ok(session)
}

fn resolve_model_file(model_dir: &Path, candidates: &[&str]) -> anyhow::Result<std::path::PathBuf> {
    for candidate in candidates {
        let path = model_dir.join(candidate);
        if path.exists() {
            return Ok(path);
        }
    }
    anyhow::bail!(
        "missing model file in {}. Tried: {}",
        model_dir.display(),
        candidates.join(", ")
    )
}

impl SttEngine {
    pub fn load(model_dir: &Path) -> anyhow::Result<Self> {
        let encoder_path =
            resolve_model_file(model_dir, &["encoder.int8.onnx", "encoder.onnx"])?;
        let decoder_path =
            resolve_model_file(model_dir, &["decoder.int8.onnx", "decoder.onnx"])?;
        let joiner_path =
            resolve_model_file(model_dir, &["joiner.int8.onnx", "joiner.onnx"])?;

        eprintln!("commander: loading STT encoder from {}", encoder_path.display());
        let encoder = load_session(&encoder_path)?;
        eprintln!("commander: loading STT decoder from {}", decoder_path.display());
        let decoder = load_session(&decoder_path)?;
        eprintln!("commander: loading STT joiner from {}", joiner_path.display());
        let joiner = load_session(&joiner_path)?;

        let mel = MelSpectrogram::new();
        let tokenizer = ParakeetTokenizer::from_dir(model_dir)?;

        Ok(Self {
            encoder,
            decoder,
            joiner,
            mel,
            tokenizer,
        })
    }

    pub fn transcribe(&mut self, samples: &[f32]) -> anyhow::Result<String> {
        if samples.is_empty() {
            return Ok(String::new());
        }

        // 1. Mel spectrogram -> [1, 128, T]
        let mel_features = self.mel.compute(samples);

        // 2. Encoder
        let num_frames = mel_features.shape()[2] as i64;
        let mel_tensor = Tensor::from_array(mel_features)?;
        let length = ndarray::Array1::from(vec![num_frames]);
        let length_tensor = Tensor::from_array(length)?;

        let encoder_outputs = self.encoder.run(ort::inputs! {
            "audio_signal" => mel_tensor,
            "length" => length_tensor
        })?;

        let encoder_out: ArrayD<f32> = encoder_outputs
            .get("outputs")
            .ok_or_else(|| anyhow::anyhow!("missing encoder 'outputs'"))?
            .try_extract_array::<f32>()
            .map(|a| a.to_owned())?;

        let enc_len: ArrayD<i64> = encoder_outputs
            .get("encoded_lengths")
            .ok_or_else(|| anyhow::anyhow!("missing encoder 'encoded_lengths'"))?
            .try_extract_array::<i64>()
            .map(|a| a.to_owned())?;

        let encoded_length = enc_len.iter().next().copied().unwrap_or(0) as usize;

        // 3. TDT greedy decode
        let token_ids = greedy_decode_tdt(
            &encoder_out,
            encoded_length,
            &mut self.decoder,
            &mut self.joiner,
            self.tokenizer.vocab_size(),
            self.tokenizer.blank_id(),
            10000,
        )?;

        // 4. Decode tokens
        Ok(self.tokenizer.decode(&token_ids))
    }
}
