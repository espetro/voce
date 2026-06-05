use anyhow::{Context, Result};
use ndarray::{Array2, Axis};
use rubato::{FftFixedIn, Resampler};
use std::path::Path;

use ort::ep::CoreML;
use ort::session::Session;
use ort::value::Tensor;

pub struct WeSpeakerEmbedder {
    session: Session,
    resampler: FftFixedIn<f32>,
}

impl WeSpeakerEmbedder {
    pub fn new(model_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("Failed to create ONNX session builder: {}", e))?
            .with_execution_providers([CoreML::default().build()])
            .map_err(|e| anyhow::anyhow!("Failed to configure CoreML: {}", e))?
            .commit_from_file(model_path)
            .context("Failed to load WeSpeaker ONNX model")?;

        let resampler =
            FftFixedIn::new(22050, 16000, 22050, 2, 1).context("Failed to create resampler")?;

        Ok(Self { session, resampler })
    }

    pub async fn extract(&mut self, window_22050: &[f32]) -> Result<[f32; 256]> {
        // Clone data for the async block
        let mut window_16k = vec![0.0f32; 16000];
        self.resampler
            .process_into_buffer(
                &[window_22050],
                std::slice::from_mut(&mut window_16k.as_mut_slice()),
                None,
            )
            .context("Resampling failed")?;

        let fbank = compute_fbank80(&window_16k)?;
        let input = fbank.insert_axis(Axis(0));

        let input_tensor = Tensor::from_array((input.shape().to_vec(), input.into_raw_vec()))
            .context("Failed to create input tensor from ndarray")?;

        let outputs = self
            .session
            .run(ort::inputs! { "feats" => input_tensor })
            .context("ONNX inference failed")?;

        let (_shape, embedding_slice) = outputs["embs"]
            .try_extract_tensor::<f32>()
            .context("Failed to extract embeddings tensor")?;

        let mut result = [0f32; 256];
        if embedding_slice.len() != 256 {
            anyhow::bail!("Expected 256-dim embedding, got {}", embedding_slice.len());
        }
        result.copy_from_slice(embedding_slice);

        Ok(l2_normalize(result))
    }
}

fn compute_fbank80(samples: &[f32]) -> Result<Array2<f32>> {
    use kaldi_native_fbank::{FbankComputer, FbankOptions};

    let mut opts = FbankOptions::default();
    opts.mel_opts.num_bins = 80;

    let mut computer = FbankComputer::new(opts)
        .map_err(|e| anyhow::anyhow!("Failed to create FbankComputer: {}", e))?;

    let frame_length_samples = 400;
    let frame_shift_samples = 160;
    let num_frames = (samples.len() - frame_length_samples) / frame_shift_samples + 1;

    if num_frames == 0 {
        anyhow::bail!("No frames extracted from audio");
    }

    let mut fbank = Array2::zeros((num_frames, 80));

    for i in 0..num_frames {
        let start = i * frame_shift_samples;
        let end = start + frame_length_samples;

        if end > samples.len() {
            break;
        }

        let mut frame = samples[start..end].to_vec();
        let mut feature = vec![0.0f32; 80];

        computer.compute(0.0, 1.0, &mut frame, &mut feature);

        for (j, &val) in feature.iter().enumerate() {
            fbank[[i, j]] = val;
        }
    }

    let mean = fbank.mean_axis(Axis(0)).unwrap();
    fbank = &fbank - &mean;

    Ok(fbank)
}

pub fn l2_normalize(mut v: [f32; 256]) -> [f32; 256] {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm = norm.max(1e-8);
    for x in &mut v {
        *x /= norm;
    }
    v
}

pub fn cosine_similarity(a: &[f32; 256], b: &[f32; 256]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}
