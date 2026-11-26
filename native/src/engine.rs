//! Deterministic mock inference backend.
//!
//! This is not a model: it derives a 256-dimensional embedding from a PCM
//! window using a fixed spectral/envelope feature map and compares it with
//! the enrolled profile by cosine similarity. The same input always produces
//! the same result, and different audio produces a different embedding, so
//! the ABI can be exercised end to end without downloading ONNX models.

use crate::constants::{
    DEFAULT_THRESHOLD, EMBEDDING_DIM, HOP_SAMPLES, PROFILE_VERSION, WINDOW_SAMPLES,
};
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Instant;

/// Mock gate decision values mirror `VOCE_GATE_PASS` / `VOCE_GATE_BLOCK`.
pub const GATE_PASS: i32 = 1;
pub const GATE_BLOCK: i32 = 0;

/// Number of points in the deterministic decimated feature grid.
const FEATURE_POINTS: usize = 512;
const SPECTRAL_DIMS: usize = 128;

/// One scored window from the mock backend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowScore {
    pub similarity: f32,
    pub speech_probability: f32,
    pub decision: i32,
    pub runtime_ms: f64,
}

/// State behind one opaque `VoceEngine` handle.
#[derive(Debug, Clone)]
pub struct MockEngine {
    profile: [f32; EMBEDDING_DIM],
    enrolled: bool,
    threshold: f32,
    enrollment_audio: Vec<f32>,
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockEngine {
    pub fn new() -> Self {
        Self {
            profile: [0.0; EMBEDDING_DIM],
            enrolled: false,
            threshold: DEFAULT_THRESHOLD,
            enrollment_audio: Vec::new(),
        }
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

    /// Borrows PCM for the duration of this call only; never retained.
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

    /// Averages the embeddings of all complete enrollment windows (or the
    /// whole chunk when shorter than one window) into an L2-normalized profile.
    pub fn finish_enrollment(&mut self) -> Result<[f32; EMBEDDING_DIM], String> {
        if self.enrollment_audio.is_empty() {
            return Err("no enrollment audio; call enroll_pcm first".into());
        }

        let windows = enrollment_windows(&self.enrollment_audio);
        let mut sum = [0.0f64; EMBEDDING_DIM];
        for window in windows {
            let embedding = embed_window(window);
            for (total, value) in sum.iter_mut().zip(embedding.iter()) {
                *total += f64::from(*value);
            }
        }
        let count = enrollment_window_count(self.enrollment_audio.len()) as f64;
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

    /// Scores one window against the enrolled profile. Silence passes with
    /// similarity 0; speech is passed when cosine similarity >= threshold.
    pub fn score_window(&self, pcm: &[f32]) -> Result<WindowScore, String> {
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
            cosine_similarity(&embed_window(pcm), &self.profile)
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

    /// Number of complete overlapping windows in a chunk.
    pub fn window_count(&self, n_samples: usize) -> usize {
        if n_samples < WINDOW_SAMPLES {
            0
        } else {
            1 + (n_samples - WINDOW_SAMPLES) / HOP_SAMPLES
        }
    }

    /// Scores all complete overlapping windows in a chunk.
    pub fn process_pcm(&self, pcm: &[f32]) -> Result<Vec<WindowScore>, String> {
        (0..self.window_count(pcm.len()))
            .map(|index| {
                let start = index * HOP_SAMPLES;
                self.score_window(&pcm[start..start + WINDOW_SAMPLES])
            })
            .collect()
    }

    /// Serializes profile state as JSON. Missing fields are rejected on load.
    pub fn serialize_profile_json(&self) -> Result<String, String> {
        let value = json!({
            "version": PROFILE_VERSION,
            "backend": "mock",
            "dim": EMBEDDING_DIM,
            "enrolled": self.enrolled,
            "threshold": self.threshold,
            "embedding": self.profile.as_slice(),
        });
        serde_json::to_string(&value).map_err(|err| err.to_string())
    }

    /// Restores a profile produced by `serialize_profile_json`.
    pub fn deserialize_profile_json(&mut self, json_bytes: &[u8]) -> Result<(), String> {
        let text = std::str::from_utf8(json_bytes).map_err(|err| err.to_string())?;
        if text.as_bytes().contains(&0) {
            return Err("profile JSON contains a NUL byte".into());
        }
        let value: Value =
            serde_json::from_str(text).map_err(|err| err.to_string())?;
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
        // Profiles produced by this engine are already normalized. Keep their
        // exact bits on roundtrip; only repair externally supplied profiles.
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
}

fn enrollment_window_count(n_samples: usize) -> usize {
    if n_samples < WINDOW_SAMPLES {
        1
    } else {
        1 + (n_samples - WINDOW_SAMPLES) / HOP_SAMPLES
    }
}

fn enrollment_windows(pcm: &[f32]) -> Vec<&[f32]> {
    if pcm.len() < WINDOW_SAMPLES {
        return vec![pcm];
    }
    let count = enrollment_window_count(pcm.len());
    (0..count)
        .map(|index| {
            let start = index * HOP_SAMPLES;
            &pcm[start..start + WINDOW_SAMPLES]
        })
        .collect()
}

pub fn rms_energy(pcm: &[f32]) -> f64 {
    if pcm.is_empty() {
        return 0.0;
    }
    let sum: f64 = pcm.iter().map(|sample| f64::from(*sample) * f64::from(*sample)).sum();
    (sum / pcm.len() as f64).sqrt()
}

fn normalize(vector: &mut [f32; EMBEDDING_DIM]) {
    let norm: f32 = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm.is_finite() && norm > 1e-8 {
        for value in vector.iter_mut() {
            *value /= norm;
        }
    } else {
        vector.fill(0.0);
    }
}

fn cosine_similarity(left: &[f32; EMBEDDING_DIM], right: &[f32; EMBEDDING_DIM]) -> f32 {
    let dot: f32 = left.iter().zip(right).map(|(a, b)| a * b).sum();
    dot.clamp(0.0, 1.0)
}

/// Fixed sin/cos grid for the deterministic mock feature extractor.
/// Computing it once keeps repeated windows cheap and bit-identical.
fn trig_table() -> &'static [(f32, f32)] {
    static TABLE: OnceLock<Vec<(f32, f32)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = Vec::with_capacity(SPECTRAL_DIMS * FEATURE_POINTS);
        for k in 1..=SPECTRAL_DIMS {
            for n in 0..FEATURE_POINTS {
                let phase = 2.0 * std::f64::consts::PI * k as f64 * n as f64
                    / FEATURE_POINTS as f64;
                table.push((phase.cos() as f32, phase.sin() as f32));
            }
        }
        table
    })
}

/// Deterministic 256-dim mock embedding: 128 log spectral magnitudes plus 128
/// log temporal envelope values, L2-normalized.
pub fn embed_window(pcm: &[f32]) -> [f32; EMBEDDING_DIM] {
    let mut embedding = [0.0f32; EMBEDDING_DIM];
    if pcm.is_empty() {
        return embedding;
    }

    let stride = pcm.len() as f64 / FEATURE_POINTS as f64;
    let mut decimated = [0.0f32; FEATURE_POINTS];
    for (index, slot) in decimated.iter_mut().enumerate() {
        let start = (index as f64 * stride) as usize;
        let mut end = ((index + 1) as f64 * stride) as usize;
        if end <= start {
            end = start + 1;
        }
        if end > pcm.len() {
            end = pcm.len();
        }
        let sum: f64 = pcm[start..end].iter().map(|value| f64::from(*value)).sum();
        *slot = (sum / (end - start) as f64) as f32;
    }

    let table = trig_table();
    let mut spectral_mean = 0.0f32;
    for k in 0..SPECTRAL_DIMS {
        let mut re = 0.0f64;
        let mut im = 0.0f64;
        let offset = k * FEATURE_POINTS;
        for (n, &sample) in decimated.iter().enumerate() {
            let (cos, sin) = table[offset + n];
            re += f64::from(sample) * f64::from(cos);
            im -= f64::from(sample) * f64::from(sin);
        }
        let magnitude = (re * re + im * im).sqrt() as f32 / FEATURE_POINTS as f32;
        // Mean subtraction removes broadband level so similarity reflects the
        // spectral shape instead of loudness.
        embedding[k] = 100.0 * magnitude;
        spectral_mean += embedding[k];
    }
    spectral_mean /= SPECTRAL_DIMS as f32;
    for value in embedding[..SPECTRAL_DIMS].iter_mut() {
        *value -= spectral_mean;
    }

    let mut envelope_mean = 0.0f32;
    for band in 0..SPECTRAL_DIMS {
        let start = band * 4;
        let energy: f32 = decimated[start..start + 4]
            .iter()
            .map(|value| value * value)
            .sum();
        embedding[SPECTRAL_DIMS + band] = 100.0 * (energy / 4.0).sqrt();
        envelope_mean += embedding[SPECTRAL_DIMS + band];
    }
    envelope_mean /= SPECTRAL_DIMS as f32;
    for value in embedding[SPECTRAL_DIMS..].iter_mut() {
        *value -= envelope_mean;
    }

    normalize(&mut embedding);
    embedding
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::SAMPLE_RATE_HZ;

    fn tone(frequency: f32, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|n| {
                (2.0 * std::f32::consts::PI * frequency * n as f32 / SAMPLE_RATE_HZ as f32).sin()
                    * 0.2
            })
            .collect()
    }

    #[test]
    fn same_audio_scores_above_threshold() {
        let audio = tone(220.0, WINDOW_SAMPLES);
        let mut engine = MockEngine::new();
        engine.enroll_pcm(&audio).unwrap();
        engine.finish_enrollment().unwrap();
        let score = engine.score_window(&audio).unwrap();
        assert!(score.similarity > 0.95, "similarity {}", score.similarity);
        assert_eq!(score.decision, GATE_PASS);
    }

    #[test]
    fn different_audio_scores_below_threshold() {
        let enrolled = tone(220.0, WINDOW_SAMPLES);
        let other = tone(1200.0, WINDOW_SAMPLES);
        let mut engine = MockEngine::new();
        engine.enroll_pcm(&enrolled).unwrap();
        engine.finish_enrollment().unwrap();
        let score = engine.score_window(&other).unwrap();
        assert!(score.similarity < 0.6, "similarity {}", score.similarity);
        assert_eq!(score.decision, GATE_BLOCK);
    }

    #[test]
    fn silence_passes_with_zero_similarity() {
        let audio = tone(220.0, WINDOW_SAMPLES);
        let mut engine = MockEngine::new();
        engine.enroll_pcm(&audio).unwrap();
        engine.finish_enrollment().unwrap();
        let score = engine.score_window(&[0.0; WINDOW_SAMPLES]).unwrap();
        assert_eq!(score.similarity, 0.0);
        assert_eq!(score.decision, GATE_PASS);
    }

    #[test]
    fn profile_json_roundtrip_preserves_decisions() {
        let audio = tone(330.0, WINDOW_SAMPLES);
        let other = tone(900.0, WINDOW_SAMPLES);
        let mut engine = MockEngine::new();
        engine.enroll_pcm(&audio).unwrap();
        engine.finish_enrollment().unwrap();
        let json = engine.serialize_profile_json().unwrap();

        let mut restored = MockEngine::new();
        restored.deserialize_profile_json(json.as_bytes()).unwrap();
        let before = engine.score_window(&other).unwrap();
        let after = restored.score_window(&other).unwrap();
        assert_eq!(before.similarity.to_bits(), after.similarity.to_bits());
        assert_eq!(before.decision, after.decision);
    }

    #[test]
    fn process_pcm_uses_overlapping_windows() {
        let audio = tone(220.0, WINDOW_SAMPLES + HOP_SAMPLES * 2);
        let mut engine = MockEngine::new();
        engine.enroll_pcm(&audio[..WINDOW_SAMPLES]).unwrap();
        engine.finish_enrollment().unwrap();
        assert_eq!(engine.window_count(audio.len()), 3);
        let results = engine.process_pcm(&audio).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|score| score.decision == GATE_PASS));
    }
}
