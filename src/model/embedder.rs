use anyhow::{Context, Result};
use std::path::Path;
use voxudio::SpeakerEmbeddingExtractor;

/// Wraps voxudio's SpeakerEmbeddingExtractor.
///
/// Expects mono audio at 22050 Hz. Produces a 256-dim L2-normalised embedding.
pub struct EmbedderWrapper {
    inner: SpeakerEmbeddingExtractor,
}

impl EmbedderWrapper {
    pub fn new(model_path: &Path) -> Result<Self> {
        let inner = SpeakerEmbeddingExtractor::new(model_path)
            .context("failed to load speaker embedding model")?;
        Ok(Self { inner })
    }

    /// Extract a 256-dim L2-normalised embedding from a 22050-sample (1 s) mono window.
    pub async fn extract_embedding(&mut self, window_22050: &[f32]) -> Result<[f32; 256]> {
        // channels=1 for mono; returns Vec<[f32; 256]> with one entry
        let result = self
            .inner
            .extract(window_22050, 1)
            .await
            .context("embedding extraction failed")?;

        let raw = result
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("embedder returned empty result"))?;

        Ok(l2_normalize(raw))
    }
}

/// L2-normalise a 256-dim vector.
pub fn l2_normalize(mut v: [f32; 256]) -> [f32; 256] {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm = norm.max(1e-8);
    for x in &mut v {
        *x /= norm;
    }
    v
}

/// Cosine similarity of two L2-normalised 256-dim vectors (= dot product).
pub fn cosine_similarity(a: &[f32; 256], b: &[f32; 256]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}
