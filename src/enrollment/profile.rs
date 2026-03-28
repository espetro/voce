use crate::model::{EmbedderWrapper, l2_normalize};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, warn};

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
/// return a `VoiceProfile` ready to be saved.
///
/// Skips windows below -40 dBFS. Logs a warning if fewer than 4 valid windows
/// are found (too little speech for a reliable profile).
pub async fn compute_profile(
    embedder: &mut EmbedderWrapper,
    recordings: &[Vec<f32>],
) -> Result<VoiceProfile> {
    let mut all_embeddings: Vec<[f32; 256]> = Vec::new();

    for (rec_idx, recording) in recordings.iter().enumerate() {
        let mut start = 0usize;
        let mut window_count = 0usize;

        while start + WINDOW <= recording.len() {
            let window = &recording[start..start + WINDOW];

            if !is_silent(window) {
                match embedder.extract_embedding(window).await {
                    Ok(emb) => {
                        all_embeddings.push(emb);
                        window_count += 1;
                    }
                    Err(e) => warn!("Embedding extraction failed for rec {rec_idx} window {start}: {e}"),
                }
            }
            start += HOP;
        }

        debug!("Recording {}: {} valid windows extracted", rec_idx + 1, window_count);
    }

    if all_embeddings.is_empty() {
        anyhow::bail!("No valid speech windows found in recordings — profile cannot be built");
    }

    if all_embeddings.len() < 4 {
        warn!(
            "Only {} embedding windows — profile may be unreliable (need 4+)",
            all_embeddings.len()
        );
    }

    let mean = mean_embedding(&all_embeddings);
    let normalised = l2_normalize(mean);

    Ok(VoiceProfile {
        version: 1,
        dim: 256,
        embedding: normalised.to_vec(),
    })
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
