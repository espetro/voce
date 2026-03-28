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
