//! Rust declarations and frame helpers for the narrow C adapter.
//!
//! The adapter itself lives in `voce_adapter.c`; `build.rs` compiles it into
//! every Rust target (rlib, cdylib, staticlib and tests). These declarations
//! let ABI tests call the exact symbols scriptc and pyscriptc bind.

/// Operation codes from `voce_adapter.h`.
pub mod op {
    pub const LOAD_MODELS: i32 = 1;
    pub const SET_THRESHOLD: i32 = 2;
    pub const RESET_ENROLLMENT: i32 = 3;
    pub const ENROLL_PCM: i32 = 4;
    pub const FINISH_QUERY: i32 = 5;
    pub const FINISH_ENROLLMENT: i32 = 6;
    pub const EMBEDDING_DIM: i32 = 7;
    pub const SCORE_WINDOW: i32 = 8;
    pub const PROCESS_QUERY: i32 = 9;
    pub const PROCESS_PCM: i32 = 10;
    pub const SERIALIZE_QUERY: i32 = 11;
    pub const SERIALIZE_PROFILE_JSON: i32 = 12;
    pub const DESERIALIZE_PROFILE_JSON: i32 = 13;
}

pub const I64_PREFIX_BYTES: usize = 8;
pub const WINDOW_RESULT_BYTES: usize = 24;
pub const EMBEDDING_F32: usize = 256;

unsafe extern "C" {
    pub fn voce_adapter_create() -> i32;
    pub fn voce_adapter_destroy(session: i32);
    pub fn voce_adapter_frame(
        session: i32,
        op: i32,
        frame: *mut u8,
        frame_len: i64,
    ) -> i32;
}

/// Frame codec helpers shared by tests; both backends use the same layouts.
pub mod frame {
    use super::{I64_PREFIX_BYTES, WINDOW_RESULT_BYTES};

    pub fn put_i64(frame: &mut [u8], value: i64) {
        frame[..I64_PREFIX_BYTES].copy_from_slice(&value.to_le_bytes());
    }

    pub fn get_i64(frame: &[u8]) -> i64 {
        let mut bytes = [0u8; I64_PREFIX_BYTES];
        bytes.copy_from_slice(&frame[..I64_PREFIX_BYTES]);
        i64::from_le_bytes(bytes)
    }

    pub fn encode_pcm(pcm: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(pcm.len() * 4);
        for sample in pcm {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    pub fn decode_f32(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|chunk| {
                let mut value = [0u8; 4];
                value.copy_from_slice(chunk);
                f32::from_le_bytes(value)
            })
            .collect()
    }

    /// A window result is f64, f32, f32, i32, all little-endian.
    pub fn decode_result(bytes: &[u8]) -> (f64, f32, f32, i32) {
        let mut runtime = [0u8; 8];
        runtime.copy_from_slice(&bytes[..8]);
        let mut similarity = [0u8; 4];
        similarity.copy_from_slice(&bytes[8..12]);
        let mut speech = [0u8; 4];
        speech.copy_from_slice(&bytes[12..16]);
        let mut decision = [0u8; 4];
        decision.copy_from_slice(&bytes[16..20]);
        (
            f64::from_le_bytes(runtime),
            f32::from_le_bytes(similarity),
            f32::from_le_bytes(speech),
            i32::from_le_bytes(decision),
        )
    }

    pub fn embedding_len(count: usize) -> usize {
        I64_PREFIX_BYTES + count * 4
    }

    pub fn process_len(capacity: usize, n_samples: usize) -> usize {
        I64_PREFIX_BYTES + n_samples * 4 + capacity * WINDOW_RESULT_BYTES
    }

    pub fn embedding_frame(capacity: usize) -> Vec<u8> {
        let mut frame = vec![0u8; embedding_len(capacity)];
        put_i64(&mut frame, capacity as i64);
        frame
    }

    pub fn process_frame(capacity: usize, pcm: &[f32]) -> Vec<u8> {
        let mut frame = vec![0u8; process_len(capacity, pcm.len())];
        put_i64(&mut frame, capacity as i64);
        frame[I64_PREFIX_BYTES..I64_PREFIX_BYTES + pcm.len() * 4]
            .copy_from_slice(&encode_pcm(pcm));
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::frame;

    #[test]
    fn frame_codec_roundtrips_fixed_width_fields() {
        let mut frame = vec![0u8; 8];
        frame::put_i64(&mut frame, -4);
        assert_eq!(frame::get_i64(&frame), -4);
        let pcm = [0.5f32, -1.25, 3.5];
        let bytes = frame::encode_pcm(&pcm);
        assert_eq!(bytes.len(), 12);
        assert_eq!(frame::decode_f32(&bytes), pcm);
    }

    #[test]
    fn frame_codec_uses_documented_prefix_and_result_offsets() {
        let mut frame = frame::process_frame(2, &[0.0; 4]);
        assert_eq!(frame::get_i64(&frame), 2);
        assert_eq!(frame.len(), 8 + 16 + 2 * 24);
        let result_offset = 8 + 16;
        frame[result_offset..result_offset + 8]
            .copy_from_slice(&12.5f64.to_le_bytes());
        frame[result_offset + 8..result_offset + 12]
            .copy_from_slice(&0.75f32.to_le_bytes());
        frame[result_offset + 12..result_offset + 16]
            .copy_from_slice(&1.0f32.to_le_bytes());
        frame[result_offset + 16..result_offset + 20]
            .copy_from_slice(&1i32.to_le_bytes());
        let (runtime, similarity, speech, decision) =
            frame::decode_result(&frame[result_offset..]);
        assert_eq!((runtime, similarity, speech, decision), (12.5, 0.75, 1.0, 1));
    }

    #[test]
    fn frame_codec_embedding_size_is_fixed() {
        let frame = frame::embedding_frame(super::EMBEDDING_F32);
        assert_eq!(frame.len(), 8 + 256 * 4);
        assert_eq!(frame::get_i64(&frame), 256);
    }
}
