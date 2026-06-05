use anyhow::{Context, Result};
use std::path::Path;

use crate::model::wespeaker::WeSpeakerEmbedder;

pub struct EmbedderWrapper {
    inner: WeSpeakerEmbedder,
}

impl EmbedderWrapper {
    pub fn new(model_path: &Path) -> Result<Self> {
        let inner = WeSpeakerEmbedder::new(model_path)
            .context("Failed to initialize WeSpeaker embedder")?;
        Ok(Self { inner })
    }

    pub async fn extract_embedding(&mut self, window_22050: &[f32]) -> Result<[f32; 256]> {
        self.inner.extract(window_22050).await
    }
}

pub use crate::model::wespeaker::{cosine_similarity, l2_normalize};
