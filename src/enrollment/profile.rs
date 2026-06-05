use crate::model::{EmbedderWrapper, cosine_similarity, l2_normalize};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

const WINDOW: usize = 22050; // 1 s at 22050 Hz
const HOP: usize = 11025;    // 0.5 s — 50% overlap

#[derive(Serialize, Deserialize)]
pub struct VoiceProfile {
    pub version: u8,
    pub dim: usize,
    pub embedding: Vec<f32>,
}

impl VoiceProfile {
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }

    pub fn as_array(&self) -> Option<[f32; 256]> {
        if self.embedding.len() != 256 { return None; }
        let mut a = [0f32; 256];
        a.copy_from_slice(&self.embedding);
        Some(a)
    }
}

/// Extract embeddings from all recordings, average them, L2-normalise, and
/// return a `VoiceProfile` plus an optional cross-similarity score.
///
/// Cross-similarity (AC #4): when exactly 2 recordings are provided, computes
/// cosine similarity between their mean embeddings. Logs a warning if < 0.8
/// but always proceeds (diagnostic only).
///
/// Skips windows below -40 dBFS. Logs a warning if fewer than 4 valid windows
/// are found (too little speech for a reliable profile).
pub async fn compute_profile(
    embedder: &mut EmbedderWrapper,
    recordings: &[Vec<f32>],
) -> Result<(VoiceProfile, Option<f32>)> {
    // Collect embeddings per recording for cross-similarity check.
    let mut per_rec: Vec<Vec<[f32; 256]>> = Vec::with_capacity(recordings.len());

    for (rec_idx, recording) in recordings.iter().enumerate() {
        let mut rec_embeddings: Vec<[f32; 256]> = Vec::new();
        let mut start = 0usize;

        while start + WINDOW <= recording.len() {
            let window = &recording[start..start + WINDOW];

            if !is_silent(window) {
                match embedder.extract_embedding(window).await {
                    Ok(emb) => rec_embeddings.push(emb),
                    Err(e) => warn!("Embedding extraction failed for rec {rec_idx} window {start}: {e}"),
                }
            }
            start += HOP;
        }

        debug!("Recording {}: {} valid windows extracted", rec_idx + 1, rec_embeddings.len());
        per_rec.push(rec_embeddings);
    }

    // Flatten for pooled mean → identical output/profile as before.
    let all_embeddings: Vec<[f32; 256]> = per_rec.iter().flatten().copied().collect();

    if all_embeddings.is_empty() {
        anyhow::bail!("No valid speech windows found in recordings — profile cannot be built");
    }

    if all_embeddings.len() < 4 {
        warn!(
            "Only {} embedding windows — profile may be unreliable (need 4+)",
            all_embeddings.len()
        );
    }

    let cross_sim = recording_cross_similarity(&per_rec);
    if let Some(sim) = cross_sim {
        info!("enrollment cross-similarity: {sim:.3}");
        if sim < 0.8 {
            warn!("enrollment cross-similarity {sim:.3} < 0.8 — recordings may be different speakers / low quality — proceeding anyway");
        }
    }

    let mean = mean_embedding(&all_embeddings);
    let normalised = l2_normalize(mean);

    Ok((VoiceProfile {
        version: 1,
        dim: 256,
        embedding: normalised.to_vec(),
    }, cross_sim))
}

/// Cosine similarity between the mean embeddings of exactly 2 recordings.
/// Returns `None` if there aren't exactly 2 recordings each with ≥1 window.
pub fn recording_cross_similarity(per_rec: &[Vec<[f32; 256]>]) -> Option<f32> {
    if per_rec.len() != 2 { return None; }
    if per_rec[0].is_empty() || per_rec[1].is_empty() { return None; }
    let a = l2_normalize(mean_embedding(&per_rec[0]));
    let b = l2_normalize(mean_embedding(&per_rec[1]));
    Some(cosine_similarity(&a, &b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_embedding_correctness() {
        let a = [1.0f32; 256];
        let b = [3.0f32; 256];
        let mean = mean_embedding(&[a, b]);
        for v in mean {
            assert!((v - 2.0).abs() < 1e-6);
        }
    }

    #[test]
    fn as_array_correct_dim() {
        let profile = VoiceProfile { version: 1, dim: 256, embedding: vec![0.0f32; 256] };
        assert!(profile.as_array().is_some());
    }

    #[test]
    fn as_array_wrong_dim() {
        let profile = VoiceProfile { version: 1, dim: 128, embedding: vec![0.0f32; 128] };
        assert!(profile.as_array().is_none());
    }

    #[test]
    fn cross_similarity_identical_recordings() {
        let emb = [0.5f32; 256];
        let per_rec = vec![vec![emb], vec![emb]];
        let sim = recording_cross_similarity(&per_rec).unwrap();
        assert!((sim - 1.0).abs() < 1e-5, "identical embeddings → sim ≈ 1.0, got {sim}");
    }

    #[test]
    fn cross_similarity_orthogonal_recordings() {
        let mut a = [0.0f32; 256];
        let mut b = [0.0f32; 256];
        a[0] = 1.0;
        b[1] = 1.0;
        let per_rec = vec![vec![a], vec![b]];
        let sim = recording_cross_similarity(&per_rec).unwrap();
        assert!(sim.abs() < 1e-5, "orthogonal embeddings → sim ≈ 0.0, got {sim}");
    }

    #[test]
    fn cross_similarity_none_for_single_recording() {
        let per_rec = vec![vec![[0.5f32; 256]]];
        assert!(recording_cross_similarity(&per_rec).is_none());
    }

    #[test]
    fn cross_similarity_none_for_empty_recording() {
        let per_rec: Vec<Vec<[f32; 256]>> = vec![vec![[0.5f32; 256]], vec![]];
        assert!(recording_cross_similarity(&per_rec).is_none());
    }
}

fn is_silent(samples: &[f32]) -> bool {
    if samples.is_empty() { return true; }
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    rms == 0.0 || 20.0 * rms.log10() < -40.0
}

pub fn mean_embedding(embeddings: &[[f32; 256]]) -> [f32; 256] {
    let n = embeddings.len() as f32;
    let mut mean = [0f32; 256];
    for emb in embeddings {
        for (i, v) in emb.iter().enumerate() {
            mean[i] += v / n;
        }
    }
    mean
}
