use anyhow::{Context, Result};
use ndarray::{Array2, Axis};
use std::path::Path;

use ort::ep::CoreML;
use ort::session::Session;
use ort::value::Tensor;

pub struct WeSpeakerEmbedder {
    session: Session,
}

impl WeSpeakerEmbedder {
    pub fn new(model_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("Failed to create ONNX session builder: {}", e))?
            .with_execution_providers([CoreML::default().build()])
            .map_err(|e| anyhow::anyhow!("Failed to configure CoreML: {}", e))?
            .commit_from_file(model_path)
            .context("Failed to load WeSpeaker ONNX model")?;

        Ok(Self { session })
    }

    pub async fn extract(&mut self, window: &[f32]) -> Result<[f32; 256]> {
        let fbank = compute_fbank80(window)?;
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
    use kaldi_native_fbank::online::FeatureComputer;
    use kaldi_native_fbank::{FbankComputer, FbankOptions, OnlineFeature};

    // Match the WeSpeaker training pipeline: 80 mel bins, hamming window, no dither, no energy.
    let mut opts = FbankOptions::default();
    opts.mel_opts.num_bins = 80;
    opts.use_energy = false;
    opts.frame_opts.window_type = "hamming".to_string();
    opts.frame_opts.dither = 0.0;

    let computer = FbankComputer::new(opts)
        .map_err(|e| anyhow::anyhow!("Failed to create FbankComputer: {}", e))?;
    let dim = computer.dim();

    // OnlineFeature applies extract_window() (dither, DC removal, preemphasis, windowing)
    // before calling FbankComputer::compute() — the correct usage path.
    let mut online = OnlineFeature::new(FeatureComputer::Fbank(computer));
    online.accept_waveform(16000.0, samples);
    online.input_finished();

    let num_frames = online.num_frames_ready();
    if num_frames == 0 {
        anyhow::bail!("No frames extracted from audio");
    }

    let mut fbank = Array2::zeros((num_frames, dim));
    for i in 0..num_frames {
        let frame = online.get_frame(i).unwrap();
        for (j, &val) in frame.iter().enumerate() {
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
