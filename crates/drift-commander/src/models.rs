use std::path::PathBuf;

use drift_core::paths;

pub fn models_dir() -> PathBuf {
    paths::models_dir()
}

/// Ensure the Silero VAD model is available, returning its path.
pub fn ensure_vad_model() -> anyhow::Result<PathBuf> {
    let path = models_dir().join("silero_vad.onnx");
    if !path.exists() {
        anyhow::bail!(
            "VAD model not found at {}\n\
             Download silero_vad.onnx and place it there, or run 'drift commander setup'.",
            path.display()
        );
    }
    Ok(path)
}

/// Ensure the STT model directory is available, returning its path.
pub fn ensure_stt_model(model_name: &str) -> anyhow::Result<PathBuf> {
    let dir = models_dir().join(model_name);
    if !dir.exists() {
        anyhow::bail!(
            "STT model directory not found at {}\n\
             Download the {} model and place it there, or run 'drift commander setup'.",
            dir.display(),
            model_name
        );
    }

    let required = [
        ("encoder", &["encoder.int8.onnx", "encoder.onnx"][..]),
        ("decoder", &["decoder.int8.onnx", "decoder.onnx"]),
        ("joiner", &["joiner.int8.onnx", "joiner.onnx"]),
        ("tokens", &["tokens.txt"]),
    ];

    for (component, candidates) in &required {
        let found = candidates.iter().any(|f| dir.join(f).exists());
        if !found {
            anyhow::bail!(
                "Missing {} in {}. Expected one of: {}",
                component,
                dir.display(),
                candidates.join(", ")
            );
        }
    }

    Ok(dir)
}
