use ndarray::{s, ArrayD, IxDyn};
use ort::session::Session;
use ort::value::Tensor;

struct DecoderState {
    /// [B, 640, 1]
    output: ArrayD<f32>,
    /// [2, B, 640]
    states: ArrayD<f32>,
    /// [2, 1, 640]
    concat_state: ArrayD<f32>,
}

fn run_decoder(
    decoder: &mut Session,
    token_id: u32,
    prev_state: &DecoderState,
) -> anyhow::Result<DecoderState> {
    let targets = ndarray::Array2::from_shape_vec((1, 1), vec![token_id as i32])?;
    let targets_tensor = Tensor::from_array(targets)?;

    let target_length = ndarray::Array1::from(vec![1i32]);
    let target_length_tensor = Tensor::from_array(target_length)?;

    let states_tensor = Tensor::from_array(prev_state.states.clone())?;
    let concat_tensor = Tensor::from_array(prev_state.concat_state.clone())?;

    let outputs = decoder.run(ort::inputs! {
        "targets" => targets_tensor,
        "target_length" => target_length_tensor,
        "states.1" => states_tensor,
        "onnx::Slice_3" => concat_tensor
    })?;

    let dec_output = outputs
        .get("outputs")
        .ok_or_else(|| anyhow::anyhow!("missing decoder 'outputs'"))?
        .try_extract_array::<f32>()
        .map(|a| a.to_owned())?;

    let new_states = outputs
        .get("states")
        .ok_or_else(|| anyhow::anyhow!("missing decoder 'states'"))?
        .try_extract_array::<f32>()
        .map(|a| a.to_owned())?;

    let new_concat = outputs
        .get("162")
        .ok_or_else(|| anyhow::anyhow!("missing decoder '162'"))?
        .try_extract_array::<f32>()
        .map(|a| a.to_owned())?;

    Ok(DecoderState {
        output: dec_output,
        states: new_states,
        concat_state: new_concat,
    })
}

fn run_joiner(
    joiner: &mut Session,
    enc_frame: ArrayD<f32>,
    decoder_out: &ArrayD<f32>,
) -> anyhow::Result<Vec<f32>> {
    let enc_tensor = Tensor::from_array(enc_frame)?;
    let dec_tensor = Tensor::from_array(decoder_out.clone())?;

    let outputs = joiner.run(ort::inputs! {
        "encoder_outputs" => enc_tensor,
        "decoder_outputs" => dec_tensor
    })?;

    let logits = outputs
        .get("outputs")
        .ok_or_else(|| anyhow::anyhow!("missing joiner 'outputs'"))?
        .try_extract_array::<f32>()
        .map(|a| a.to_owned())?;

    Ok(logits.iter().copied().collect())
}

pub fn greedy_decode_tdt(
    encoder_output: &ArrayD<f32>,
    encoded_length: usize,
    decoder: &mut Session,
    joiner: &mut Session,
    vocab_size: usize,
    blank_id: u32,
    max_steps: usize,
) -> anyhow::Result<Vec<u32>> {
    let num_frames = encoded_length;
    let mut tokens: Vec<u32> = Vec::new();
    let mut t: usize = 0;

    let initial_state = DecoderState {
        output: ArrayD::zeros(IxDyn(&[1, 640, 1])),
        states: ArrayD::zeros(IxDyn(&[2, 1, 640])),
        concat_state: ArrayD::zeros(IxDyn(&[2, 1, 640])),
    };

    let mut dec_state = run_decoder(decoder, blank_id, &initial_state)?;

    let mut step_count = 0;
    while t < num_frames && step_count < max_steps {
        step_count += 1;

        let enc_frame = encoder_output
            .slice(s![.., .., t..t + 1])
            .to_owned()
            .into_dyn();

        let logits_flat = run_joiner(joiner, enc_frame, &dec_state.output)?;

        if logits_flat.len() < vocab_size + 5 {
            anyhow::bail!(
                "joiner output too small: {} (expected at least {})",
                logits_flat.len(),
                vocab_size + 5
            );
        }

        let token_logits = &logits_flat[..vocab_size];
        let duration_logits = &logits_flat[vocab_size..vocab_size + 5];

        let token = argmax(token_logits) as u32;
        let duration = argmax(duration_logits);

        if token != blank_id {
            tokens.push(token);
            dec_state = run_decoder(decoder, token, &dec_state)?;
        }

        t += duration.max(1);
    }

    Ok(tokens)
}

fn argmax(slice: &[f32]) -> usize {
    slice
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}
