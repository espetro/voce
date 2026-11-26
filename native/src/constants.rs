//! Constants shared by the ABI and the mock backend.
//!
//! All audio is 16 kHz mono f32. The 3 s / 1.5 s window and hop match the
//! desktop app's inference contract (48 000 samples and 24 000 samples).

pub const ABI_VERSION: i32 = 1;
pub const SAMPLE_RATE_HZ: u32 = 16_000;
pub const WINDOW_SAMPLES: usize = 48_000;
pub const HOP_SAMPLES: usize = 24_000;
pub const EMBEDDING_DIM: usize = 256;
pub const PROFILE_VERSION: u32 = 1;
pub const DEFAULT_THRESHOLD: f32 = 0.75;
