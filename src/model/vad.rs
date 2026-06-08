use anyhow::{Context, Result};
use std::path::Path;
use voxudio::VoiceActivityDetector;

/// Wraps voxudio's VoiceActivityDetector.
pub struct VadWrapper {
    inner: VoiceActivityDetector,
}

impl VadWrapper {
    pub fn new(model_path: &Path) -> Result<Self> {
        let inner = VoiceActivityDetector::new(model_path).context("failed to load VAD model")?;
        Ok(Self { inner })
    }

    /// Compute mean speech probability for a 16000 Hz window.
    ///
    /// Runs detect::<16000> on each 512-sample sub-chunk, returning the mean
    /// probability (0.0–1.0).
    ///
    /// Returns 0.0 if the window is empty.
    pub async fn speech_probability_16000(&mut self, window: &[f32]) -> Result<f32> {
        // VAD requires exactly 512-sample chunks at 16000 Hz
        const CHUNK: usize = 512;
        let chunks: Vec<&[f32]> = window.chunks(CHUNK).collect();

        let mut sum = 0.0f32;
        let mut count = 0usize;

        for chunk in chunks {
            if chunk.len() < CHUNK {
                // Pad the last under-sized chunk
                let mut padded = vec![0.0f32; CHUNK];
                padded[..chunk.len()].copy_from_slice(chunk);
                match self.inner.detect::<16000>(&padded).await {
                    Ok(p) => {
                        sum += p;
                        count += 1;
                    }
                    Err(e) => tracing::warn!("VAD detect error: {e}"),
                }
            } else {
                match self.inner.detect::<16000>(chunk).await {
                    Ok(p) => {
                        sum += p;
                        count += 1;
                    }
                    Err(e) => tracing::warn!("VAD detect error: {e}"),
                }
            }
        }

        if count == 0 {
            Ok(0.0)
        } else {
            Ok(sum / count as f32)
        }
    }

    /// Fast silence check using RMS energy.
    /// Returns true if the chunk is below -40 dBFS (no need to run neural VAD).
    pub fn is_silence_fast(chunk: &[f32]) -> bool {
        if chunk.is_empty() {
            return true;
        }
        let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
        if rms == 0.0 {
            return true;
        }
        20.0 * rms.log10() < -40.0
    }
}
