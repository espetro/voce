// Cosine similarity gate with sliding window vote — Phase 5
// Stub for Phase 0 compilation.

use std::collections::VecDeque;

/// Applies a sliding-window vote over recent cosine similarity scores.
///
/// Only mutes if ALL frames in the window are below threshold, preventing
/// mid-sentence cuts. Fail-open: starts in pass state.
pub struct SlidingVoteGate {
    history: VecDeque<bool>,
    vote_window: usize,
    pub threshold: f32,
    pub current_pass: bool,
    pub last_similarity: f32,
}

impl SlidingVoteGate {
    pub fn new(threshold: f32, vote_window: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(vote_window),
            vote_window,
            threshold,
            current_pass: true, // fail-open
            last_similarity: 0.0,
        }
    }

    /// Update gate with a new similarity score. Returns whether audio should pass.
    pub fn update(&mut self, similarity: f32) -> bool {
        let frame_pass = similarity >= self.threshold;
        self.history.push_back(frame_pass);
        if self.history.len() > self.vote_window {
            self.history.pop_front();
        }
        self.last_similarity = similarity;

        // Mute only if ALL recent frames failed (conservative: prefer false-pass over false-mute)
        let all_failed = !self.history.is_empty() && self.history.iter().all(|&p| !p);
        self.current_pass = !all_failed;
        self.current_pass
    }

    pub fn reset(&mut self) {
        self.history.clear();
        self.current_pass = true;
        self.last_similarity = 0.0;
    }
}
