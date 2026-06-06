// Enrollment recording session — Phase 4
// Stub for Phase 0 compilation.

pub const TARGET_DURATION_SAMPLES: u32 = 22050 * 20; // 20 s at 22050 Hz
pub const MIN_SPEECH_SAMPLES: u32 = 22050 * 10;      // 10 s minimum speech

#[derive(Debug)]
pub enum EnrollmentStatus {
    InProgress { elapsed_s: u32, speech_s: u32 },
    Complete { speech_s: u32 },
    Invalid { reason: &'static str },
}

pub struct EnrollmentSession {
    pub buffer: Vec<f32>,
    speech_samples: u32,
    total_samples: u32,
}

impl EnrollmentSession {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(TARGET_DURATION_SAMPLES as usize),
            speech_samples: 0,
            total_samples: 0,
        }
    }

    pub fn push_chunk(&mut self, chunk: &[f32], is_speech: bool) -> EnrollmentStatus {
        self.buffer.extend_from_slice(chunk);
        if is_speech {
            self.speech_samples += chunk.len() as u32;
        }
        self.total_samples += chunk.len() as u32;

        let elapsed_s = self.total_samples / 22050;
        let speech_s = self.speech_samples / 22050;

        if self.total_samples >= TARGET_DURATION_SAMPLES {
            if self.speech_samples >= MIN_SPEECH_SAMPLES {
                EnrollmentStatus::Complete { speech_s }
            } else {
                EnrollmentStatus::Invalid { reason: "insufficient_speech" }
            }
        } else {
            EnrollmentStatus::InProgress { elapsed_s, speech_s }
        }
    }

    pub fn take_buffer(self) -> Vec<f32> {
        self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHUNK: usize = 512;

    fn push_n_chunks(session: &mut EnrollmentSession, n: usize, is_speech: bool) -> EnrollmentStatus {
        let chunk = vec![0.0f32; CHUNK];
        let mut last = EnrollmentStatus::InProgress { elapsed_s: 0, speech_s: 0 };
        for _ in 0..n {
            last = session.push_chunk(&chunk, is_speech);
        }
        last
    }

    #[test]
    fn complete_with_sufficient_speech() {
        let mut s = EnrollmentSession::new();
        // Push TARGET_DURATION_SAMPLES samples all as speech
        let total_chunks = (TARGET_DURATION_SAMPLES as usize + CHUNK - 1) / CHUNK;
        let status = push_n_chunks(&mut s, total_chunks, true);
        assert!(matches!(status, EnrollmentStatus::Complete { .. }));
    }

    #[test]
    fn invalid_with_insufficient_speech() {
        let mut s = EnrollmentSession::new();
        // Fill total duration but mark nothing as speech
        let total_chunks = (TARGET_DURATION_SAMPLES as usize + CHUNK - 1) / CHUNK;
        let status = push_n_chunks(&mut s, total_chunks, false);
        assert!(matches!(status, EnrollmentStatus::Invalid { reason: "insufficient_speech" }));
    }

    #[test]
    fn in_progress_before_target_duration() {
        let mut s = EnrollmentSession::new();
        // Push half the target
        let half_chunks = (TARGET_DURATION_SAMPLES as usize / CHUNK) / 2;
        let status = push_n_chunks(&mut s, half_chunks, true);
        assert!(matches!(status, EnrollmentStatus::InProgress { .. }));
    }

    #[test]
    fn take_buffer_length() {
        let mut s = EnrollmentSession::new();
        let n = 10;
        let chunk = vec![1.0f32; CHUNK];
        for _ in 0..n {
            s.push_chunk(&chunk, false);
        }
        assert_eq!(s.take_buffer().len(), n * CHUNK);
    }
}
