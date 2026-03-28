pub mod download;
pub mod embedder;
pub mod vad;

use anyhow::Result;
use std::path::Path;
use tracing::info;

pub use embedder::{cosine_similarity, l2_normalize, EmbedderWrapper};
pub use vad::VadWrapper;

/// Holds initialised model wrappers.
pub struct ModelSet {
    pub vad: VadWrapper,
    pub embedder: EmbedderWrapper,
}

impl ModelSet {
    /// Download (if needed) and load both ONNX models.
    /// Calls `on_progress(0.0–1.0)` during download.
    pub async fn load(
        models_dir: &Path,
        on_progress: impl Fn(f32) + Send + Sync + 'static,
    ) -> Result<Self> {
        download::ensure_models(models_dir, on_progress).await?;

        info!("Loading VAD model…");
        let vad = VadWrapper::new(&models_dir.join("voice_activity_detector.onnx"))?;
        info!("VAD model ready");

        info!("Loading speaker embedding model…");
        let embedder =
            EmbedderWrapper::new(&models_dir.join("speaker_embedding_extractor.onnx"))?;
        info!("Speaker embedding model ready");

        Ok(ModelSet { vad, embedder })
    }
}
