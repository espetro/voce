//! Real ONNX speaker-embedding backend.
//!
//! Loads a WeSpeaker ResNet34 ONNX export (256-dim embeddings, 16 kHz input,
//! 80-bin fbank features) through ONNX Runtime and reproduces the desktop
//! app's `src/model/wespeaker.rs` pipeline: kaldi-native-fbank 80-bin
//! features with hamming window, no dither, no energy, per-window mean
//! subtraction, then cosine scoring against the enrolled profile.
//!
//! ABI-visible semantics (window/hop, enrollment averaging, silence pass,
//! profile JSON envelope) intentionally match `MockEngine`; only the
//! embedding source differs. Neural VAD is not loaded in v1: speech
//! probability uses the same RMS heuristic as the mock and the gap is
//! recorded in `docs/research/native-shim-real.md`.

use crate::constants::{EMBEDDING_DIM, HOP_SAMPLES, PROFILE_VERSION, WINDOW_SAMPLES};
use crate::engine::WindowScore;
use crate::engine::{cosine_similarity, normalize, rms_energy, GATE_PASS};
use ndarray::{Array2, Axis};
use ort::session::Session;
use ort::value::Tensor;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Instant;

pub const GATE_BLOCK: i32 = 0;

/// One engine over one loaded ONNX session.
pub struct RealEngine {
    session: Option<Session>,
    profile: [f32; EMBEDDING_DIM],
    enrolled: bool,
    threshold: f32,
    enrollment_audio: Vec<f32>,
    model_path: Option<String>,
}

impl Default for RealEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RealEngine {
    pub fn new() -> Self {
        Self {
            session: None,
            profile: [0.0; EMBEDDING_DIM],
            enrolled: false,
            threshold: crate::constants::DEFAULT_THRESHOLD,
            enrollment_audio: Vec::new(),
            model_path: None,
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.session.is_some()
    }

    pub fn model_path(&self) -> Option<&str> {
        self.model_path.as_deref()
    }

    pub fn is_enrolled(&self) -> bool {
        self.enrolled
    }

    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    pub fn profile(&self) -> &[f32; EMBEDDING_DIM] {
        &self.profile
    }

    pub fn set_threshold(&mut self, threshold: f32) -> Result<(), String> {
        if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
            return Err("threshold must be a finite value in [0, 1]".into());
        }
        self.threshold = threshold;
        Ok(())
    }

    pub fn reset_enrollment(&mut self) {
        self.enrollment_audio.clear();
        self.profile = [0.0; EMBEDDING_DIM];
        self.enrolled = false;
    }

    /// Loads the embedder ONNX file. `path` may be the model file itself or a
    /// directory searched for `embedder.onnx`, then any single `*.onnx`.
    pub fn load_models(&mut self, path: &str) -> Result<(), String> {
        let model_path = resolve_model_path(path)?;
        let session = Session::builder()
            .map_err(|err| format!("ONNX session builder: {err}"))?
            .commit_from_file(&model_path)
            .map_err(|err| format!("cannot load {}: {err}", model_path.display()))?;
        self.session = Some(session);
        self.model_path = Some(model_path.to_string_lossy().into_owned());
        Ok(())
    }

    pub fn enroll_pcm(&mut self, pcm: &[f32]) -> Result<(), String> {
        if pcm.is_empty() {
            return Err("enrollment PCM must not be empty".into());
        }
        if pcm.iter().any(|sample| !sample.is_finite()) {
            return Err("enrollment PCM contains non-finite samples".into());
        }
        self.enrollment_audio.extend_from_slice(pcm);
        Ok(())
    }

    /// Averages L2-normalized embeddings over all overlapping enrollment
    /// windows exactly like `MockEngine::finish_enrollment`.
    pub fn finish_enrollment(&mut self) -> Result<[f32; EMBEDDING_DIM], String> {
        if self.session.is_none() {
            return Err("models not loaded; call load_models first".into());
        }
        if self.enrollment_audio.is_empty() {
            return Err("no enrollment audio; call enroll_pcm first".into());
        }

        let windows: Vec<Vec<f32>> = enrollment_windows(&self.enrollment_audio)
            .into_iter()
            .map(|w| w.to_vec())
            .collect();
        let mut sum = [0.0f64; EMBEDDING_DIM];
        for window in &windows {
            let embedding = self.embed_window(window)?;
            for (total, value) in sum.iter_mut().zip(embedding.iter()) {
                *total += f64::from(*value);
            }
        }
        let count = windows.len() as f64;
        let mut profile = [0.0f32; EMBEDDING_DIM];
        for (slot, total) in profile.iter_mut().zip(sum.iter()) {
            *slot = (total / count) as f32;
        }
        normalize(&mut profile);

        self.profile = profile;
        self.enrolled = true;
        self.enrollment_audio.clear();
        Ok(profile)
    }

    pub fn score_window(&mut self, pcm: &[f32]) -> Result<WindowScore, String> {
        let started = Instant::now();
        if !self.enrolled {
            return Err("engine is not enrolled".into());
        }
        if pcm.is_empty() {
            return Err("window PCM must not be empty".into());
        }
        if pcm.iter().any(|sample| !sample.is_finite()) {
            return Err("window PCM contains non-finite samples".into());
        }

        let rms = rms_energy(pcm);
        let speech_probability = ((rms - 0.005) / 0.02).clamp(0.0, 1.0) as f32;
        let is_speech = rms >= 0.005;
        let similarity = if is_speech {
            cosine_similarity(&self.embed_window(pcm)?, &self.profile)
        } else {
            0.0
        };
        let decision = if !is_speech || similarity >= self.threshold {
            GATE_PASS
        } else {
            GATE_BLOCK
        };

        Ok(WindowScore {
            similarity,
            speech_probability,
            decision,
            runtime_ms: started.elapsed().as_secs_f64() * 1000.0,
        })
    }

    pub fn window_count(&self, n_samples: usize) -> usize {
        if n_samples < WINDOW_SAMPLES {
            0
        } else {
            1 + (n_samples - WINDOW_SAMPLES) / HOP_SAMPLES
        }
    }

    pub fn process_pcm(&mut self, pcm: &[f32]) -> Result<Vec<WindowScore>, String> {
        (0..self.window_count(pcm.len()))
            .map(|index| {
                let start = index * HOP_SAMPLES;
                self.score_window(&pcm[start..start + WINDOW_SAMPLES])
            })
            .collect()
    }

    pub fn serialize_profile_json(&self) -> Result<String, String> {
        let value = json!({
            "version": PROFILE_VERSION,
            "backend": "real",
            "dim": EMBEDDING_DIM,
            "enrolled": self.enrolled,
            "threshold": self.threshold,
            "embedding": self.profile.as_slice(),
        });
        serde_json::to_string(&value).map_err(|err| err.to_string())
    }

    pub fn deserialize_profile_json(&mut self, json_bytes: &[u8]) -> Result<(), String> {
        let text = std::str::from_utf8(json_bytes).map_err(|err| err.to_string())?;
        if text.as_bytes().contains(&0) {
            return Err("profile JSON contains a NUL byte".into());
        }
        let value: Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
        let version = value
            .get("version")
            .and_then(Value::as_u64)
            .ok_or("profile JSON missing version")?;
        if version != u64::from(PROFILE_VERSION) {
            return Err(format!("unsupported profile version: {version}"));
        }
        let dim = value
            .get("dim")
            .and_then(Value::as_u64)
            .ok_or("profile JSON missing dim")?;
        if dim != EMBEDDING_DIM as u64 {
            return Err(format!("unsupported profile dimension: {dim}"));
        }
        let embedding = value
            .get("embedding")
            .and_then(Value::as_array)
            .ok_or("profile JSON missing embedding")?;
        if embedding.len() != EMBEDDING_DIM {
            return Err(format!(
                "expected {} embedding values, got {}",
                EMBEDDING_DIM,
                embedding.len()
            ));
        }
        let mut profile = [0.0f32; EMBEDDING_DIM];
        for (slot, item) in profile.iter_mut().zip(embedding.iter()) {
            let value = item
                .as_f64()
                .ok_or("profile embedding contains a non-number")?;
            if !value.is_finite() {
                return Err("profile embedding contains a non-finite value".into());
            }
            *slot = value as f32;
        }
        let norm_squared: f32 = profile.iter().map(|value| value * value).sum();
        if (norm_squared - 1.0).abs() > 1e-3 {
            normalize(&mut profile);
        }

        if let Some(threshold) = value.get("threshold").and_then(Value::as_f64) {
            self.set_threshold(threshold as f32)?;
        }
        self.profile = profile;
        self.enrolled = true;
        Ok(())
    }

    /// Runs fbank + ONNX inference for one window and returns the
    /// L2-normalized 256-dim embedding.
    fn embed_window(&mut self, window: &[f32]) -> Result<[f32; EMBEDDING_DIM], String> {
        let session = self
            .session
            .as_mut()
            .ok_or("models not loaded; call load_models first")?;

        let fbank = compute_fbank80(window)?;
        let input = fbank.insert_axis(Axis(0));
        let input_tensor = Tensor::from_array((input.shape().to_vec(), input.into_raw_vec()))
            .map_err(|err| format!("input tensor: {err}"))?;

        let outputs = session
            .run(ort::inputs! {"feats" => input_tensor})
            .map_err(|err| format!("ONNX inference: {err}"))?;
        let (_shape, embedding_slice) = outputs["embs"]
            .try_extract_tensor::<f32>()
            .map_err(|err| format!("output tensor: {err}"))?;

        if embedding_slice.len() != EMBEDDING_DIM {
            return Err(format!(
                "expected {}-dim embedding, got {}",
                EMBEDDING_DIM,
                embedding_slice.len()
            ));
        }
        let mut result = [0f32; EMBEDDING_DIM];
        result.copy_from_slice(embedding_slice);
        Ok({
            normalize(&mut result);
            result
        })
    }
}

/// Resolves `path` to an ONNX model file: a file is used directly; a
/// directory is searched for `embedder.onnx` and then any single `*.onnx`.
pub fn resolve_model_path(path: &str) -> Result<std::path::PathBuf, String> {
    let path = Path::new(path);
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    if path.is_dir() {
        let candidate = path.join("embedder.onnx");
        if candidate.is_file() {
            return Ok(candidate);
        }
        let mut onnx_files: Vec<std::path::PathBuf> = std::fs::read_dir(path)
            .map_err(|err| format!("cannot read {}: {err}", path.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|p| p.extension().map_or(false, |ext| ext == "onnx"))
            .collect();
        onnx_files.sort();
        if onnx_files.len() == 1 {
            return Ok(onnx_files.remove(0));
        }
        return Err(format!(
            "{} must contain exactly one embedder.onnx or one *.onnx file, found {}",
            path.display(),
            onnx_files.len()
        ));
    }
    Err(format!("model path does not exist: {}", path.display()))
}

fn enrollment_windows(pcm: &[f32]) -> Vec<&[f32]> {
    if pcm.len() < WINDOW_SAMPLES {
        return vec![pcm];
    }
    let count = 1 + (pcm.len() - WINDOW_SAMPLES) / HOP_SAMPLES;
    (0..count)
        .map(|index| {
            let start = index * HOP_SAMPLES;
            &pcm[start..start + WINDOW_SAMPLES]
        })
        .collect()
}

/// Mirrors `src/model/wespeaker.rs::compute_fbank80`: 80 mel bins, hamming
/// window, no dither, no energy, 16 kHz, per-utterance mean subtraction.
fn compute_fbank80(samples: &[f32]) -> Result<Array2<f32>, String> {
    use kaldi_native_fbank::online::FeatureComputer;
    use kaldi_native_fbank::{FbankComputer, FbankOptions, OnlineFeature};

    let mut opts = FbankOptions::default();
    opts.mel_opts.num_bins = 80;
    opts.use_energy = false;
    opts.frame_opts.window_type = "hamming".to_string();
    opts.frame_opts.dither = 0.0;

    let computer = FbankComputer::new(opts).map_err(|err| format!("FbankComputer: {err}"))?;
    let dim = computer.dim();

    let mut online = OnlineFeature::new(FeatureComputer::Fbank(computer));
    online.accept_waveform(16000.0, samples);
    online.input_finished();

    let num_frames = online.num_frames_ready();
    if num_frames == 0 {
        return Err("no fbank frames extracted from audio".into());
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
