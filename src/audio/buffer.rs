use std::collections::VecDeque;

/// A single 512-sample chunk of mono f32 audio at 16000 Hz.
#[derive(Clone)]
pub struct AudioChunk {
    pub samples: Box<[f32]>,
    pub seq: u64,
}

/// Accumulates incoming audio chunks and emits 3-second windows at 1.5-second hops.
///
/// Window size: 48000 samples (3 s at 16000 Hz) — 3 s gives reliable speaker embeddings
/// Hop size:    24000 samples (1.5 s — 50% overlap)
pub struct EmbeddingWindowAccumulator {
    ring: VecDeque<f32>,
    samples_since_last_emit: usize,
    pub window_size: usize,
    pub hop_size: usize,
}

impl EmbeddingWindowAccumulator {
    pub fn new() -> Self {
        Self {
            ring: VecDeque::with_capacity(48000 * 2),
            samples_since_last_emit: 0,
            window_size: 48000,
            hop_size: 24000,
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

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: usize = 48000;
    const HOP: usize = 24000;
    const CHUNK: usize = 512;

    #[test]
    fn no_window_before_first_hop_fills() {
        let mut acc = EmbeddingWindowAccumulator::new();
        // Push just under one hop worth of data
        let chunk = vec![0.0f32; CHUNK];
        let mut emitted = 0usize;
        let mut total = 0usize;
        while total + CHUNK <= HOP - CHUNK {
            if acc.push_chunk(&chunk).is_some() { emitted += 1; }
            total += CHUNK;
        }
        assert_eq!(emitted, 0, "no window should emit before first hop fills");
    }

    #[test]
    fn emits_window_after_one_hop_with_enough_data() {
        let mut acc = EmbeddingWindowAccumulator::new();
        let chunk = vec![0.5f32; CHUNK];
        let mut window = None;
        // Push enough for a full window (WINDOW samples) in CHUNK-sized pieces
        let chunks_needed = WINDOW / CHUNK + 1;
        for _ in 0..chunks_needed {
            if let Some(w) = acc.push_chunk(&chunk) {
                window = Some(w);
                break;
            }
        }
        let w = window.expect("should emit a window once WINDOW samples are available");
        assert_eq!(w.len(), WINDOW);
    }

    #[test]
    fn emits_second_window_after_one_hop() {
        let mut acc = EmbeddingWindowAccumulator::new();
        let chunk = vec![0.1f32; CHUNK];
        let mut windows = 0usize;
        // Push 2 * WINDOW samples → expect at least 2 windows
        let chunks_needed = (2 * WINDOW) / CHUNK + 2;
        for _ in 0..chunks_needed {
            if acc.push_chunk(&chunk).is_some() {
                windows += 1;
            }
        }
        assert!(windows >= 2, "expected at least 2 windows, got {windows}");
    }
}
