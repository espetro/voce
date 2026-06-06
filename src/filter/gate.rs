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

        // Mute only when the window is full AND every frame failed; a partial window stays open.
        let all_failed = self.history.len() == self.vote_window && self.history.iter().all(|&p| !p);
        self.current_pass = !all_failed;
        self.current_pass
    }

    pub fn reset(&mut self) {
        self.history.clear();
        self.current_pass = true;
        self.last_similarity = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fail_open_initial_state() {
        let gate = SlidingVoteGate::new(0.8, 3);
        assert!(gate.current_pass, "gate must start in pass (fail-open) state");
    }

    #[test]
    fn mutes_only_when_all_window_frames_fail() {
        let mut gate = SlidingVoteGate::new(0.8, 3);
        gate.update(0.5); // fail
        assert!(gate.current_pass, "one fail in window of 3 should not mute");
        gate.update(0.5); // fail
        assert!(gate.current_pass, "two fails in window of 3 should not mute");
        gate.update(0.5); // fail — now all 3 frames below threshold
        assert!(!gate.current_pass, "three consecutive fails should mute");
    }

    #[test]
    fn one_passing_frame_reopens() {
        let mut gate = SlidingVoteGate::new(0.8, 3);
        gate.update(0.5);
        gate.update(0.5);
        gate.update(0.5); // all failed → muted
        assert!(!gate.current_pass);
        gate.update(0.9); // one passing frame displaces oldest fail
        assert!(gate.current_pass, "a passing frame should reopen the gate");
    }

    #[test]
    fn reset_restores_fail_open() {
        let mut gate = SlidingVoteGate::new(0.8, 3);
        gate.update(0.1);
        gate.update(0.1);
        gate.update(0.1); // muted
        gate.reset();
        assert!(gate.current_pass, "reset must restore fail-open state");
        assert_eq!(gate.last_similarity, 0.0);
    }
}
