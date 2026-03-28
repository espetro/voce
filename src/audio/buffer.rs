use std::collections::VecDeque;

/// A single 512-sample chunk of mono f32 audio at 22050 Hz.
#[derive(Clone)]
pub struct AudioChunk {
    pub samples: Box<[f32]>,
    pub seq: u64,
}

/// Accumulates incoming audio chunks and emits 1-second windows at 0.5-second hops.
///
/// Window size: 22050 samples (1 s at 22050 Hz)
/// Hop size:    11025 samples (0.5 s — 50% overlap)
pub struct EmbeddingWindowAccumulator {
    ring: VecDeque<f32>,
    samples_since_last_emit: usize,
    pub window_size: usize,
    pub hop_size: usize,
}

impl EmbeddingWindowAccumulator {
    pub fn new() -> Self {
        Self {
            ring: VecDeque::with_capacity(22050 * 2),
            samples_since_last_emit: 0,
            window_size: 22050,
            hop_size: 11025,
        }
    }

    /// Push a chunk of samples. Returns Some(window) when enough samples have
    /// accumulated for a new hop, None otherwise.
    pub fn push_chunk(&mut self, chunk: &[f32]) -> Option<Vec<f32>> {
        self.ring.extend(chunk);
        self.samples_since_last_emit += chunk.len();

        if self.samples_since_last_emit >= self.hop_size
            && self.ring.len() >= self.window_size
        {
            self.samples_since_last_emit = 0;

            // Collect the most recent window_size samples in order
            let start = self.ring.len() - self.window_size;
            let window: Vec<f32> = self.ring.range(start..).copied().collect();

            // Trim ring so it never grows unbounded (keep one window of history)
            while self.ring.len() > self.window_size {
                self.ring.pop_front();
            }

            Some(window)
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        self.ring.clear();
        self.samples_since_last_emit = 0;
    }
}
