use nnnoiseless::DenoiseState;
use rubato::{FftFixedIn, Resampler};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

// 10 ms frame: 160 samples at 16 kHz, 480 samples at 48 kHz
const FRAME_16K: usize = 160;
const FRAME_48K: usize = 480;
// nnnoiseless expects PCM range [-32768, 32767], not [-1, 1]
const PCM_SCALE: f32 = 32767.0;

/// Streaming noise suppressor for 16 kHz mono f32 audio.
///
/// Internally resamples each 10 ms frame to 48 kHz, runs nnnoiseless,
/// then resamples back. Variable-length chunks are supported; the denoiser
/// returns all currently processed output (may be shorter than input during
/// the first call while the pipeline warms up).
pub struct Denoiser {
    input_buf: VecDeque<f32>,
    output_buf: VecDeque<f32>,
    up: FftFixedIn<f32>,
    down: FftFixedIn<f32>,
    state: Box<DenoiseState<'static>>,
    enabled: Arc<AtomicBool>,
}

impl Denoiser {
    pub fn new(enabled: Arc<AtomicBool>) -> anyhow::Result<Self> {
        let up = FftFixedIn::<f32>::new(16_000, 48_000, FRAME_16K, 2, 1)
            .map_err(|e| anyhow::anyhow!("denoiser up-resampler: {e}"))?;
        let down = FftFixedIn::<f32>::new(48_000, 16_000, FRAME_48K, 2, 1)
            .map_err(|e| anyhow::anyhow!("denoiser down-resampler: {e}"))?;
        Ok(Self {
            input_buf: VecDeque::new(),
            output_buf: VecDeque::new(),
            up,
            down,
            state: DenoiseState::new(),
            enabled,
        })
    }

    /// Process a chunk of 16 kHz mono f32 samples. Returns all denoised
    /// samples currently available (same average rate as input, but length
    /// may vary between calls). Returns the input unchanged when disabled.
    pub fn process_chunk(&mut self, chunk: &[f32]) -> Vec<f32> {
        if !self.enabled.load(Ordering::Relaxed) {
            return chunk.to_vec();
        }

        self.input_buf.extend(chunk.iter().copied());

        while self.input_buf.len() >= FRAME_16K {
            let frame: Vec<f32> = self.input_buf.drain(..FRAME_16K).collect();

            // Scale from normalised float to PCM range for nnnoiseless
            let pcm: Vec<f32> = frame.iter().map(|&s| s * PCM_SCALE).collect();

            let up = match self.up.process(&[pcm], None) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let mut out_pcm = vec![0.0f32; FRAME_48K];
            self.state.process_frame(&mut out_pcm, &up[0]);

            let down = match self.down.process(&[out_pcm], None) {
                Ok(v) => v,
                Err(_) => continue,
            };

            self.output_buf
                .extend(down[0].iter().map(|&s| s / PCM_SCALE));
        }

        self.output_buf.drain(..).collect()
    }
}
