//! ABI integration tests through the reviewed C adapter.
//!
//! These call the same three exported symbols scriptc and pyscriptc bind. The
//! flows mirror native/tests/abi.rs and the Gate 1/Gate 2 probe sequences.

use voce_native::adapter::frame;
use voce_native::adapter::op;
use voce_native::adapter::{
    voce_adapter_create, voce_adapter_destroy, voce_adapter_frame,
    EMBEDDING_F32, I64_PREFIX_BYTES, WINDOW_RESULT_BYTES,
};
use voce_native::constants::{HOP_SAMPLES, WINDOW_SAMPLES};

const OK: i32 = 0;
const INVALID_STATE: i32 = -3;
const BUFFER_TOO_SMALL: i32 = -4;
const UNSUPPORTED: i32 = -7;

struct Session(i32);

impl Session {
    fn new() -> Self {
        let id = unsafe { voce_adapter_create() };
        assert!(id >= 1, "create failed with {id}");
        Self(id)
    }

    fn id(&self) -> i32 {
        self.0
    }

    fn call(&self, op: i32, frame: &mut [u8]) -> i32 {
        unsafe { voce_adapter_frame(self.id(), op, frame.as_mut_ptr(), frame.len() as i64) }
    }

    fn call_len(&self, op: i32, frame: &mut [u8], len: i64) -> i32 {
        unsafe { voce_adapter_frame(self.id(), op, frame.as_mut_ptr(), len) }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { voce_adapter_destroy(self.0) }
    }
}

fn tone(frequency: f32, samples: usize) -> Vec<f32> {
    (0..samples)
        .map(|i| (2.0 * std::f32::consts::PI * frequency * i as f32 / 16_000.0).sin() * 0.2)
        .collect()
}

fn call(session: i32, op: i32, frame: &mut [u8]) -> i32 {
    unsafe { voce_adapter_frame(session, op, frame.as_mut_ptr(), frame.len() as i64) }
}

fn enrolled(session: &Session, audio: &[f32]) -> Vec<f32> {
    let mut path = b"mock://builtin\0".to_vec();
    assert_eq!(session.call(op::LOAD_MODELS, &mut path), OK);
    assert_eq!(session.call(op::SET_THRESHOLD, &mut 0.75f32.to_le_bytes()), OK);
    assert_eq!(session.call(op::RESET_ENROLLMENT, &mut []), OK);
    assert_eq!(session.call(op::ENROLL_PCM, &mut frame::encode_pcm(audio)), OK);

    let mut query = vec![0u8; I64_PREFIX_BYTES];
    assert_eq!(session.call(op::FINISH_QUERY, &mut query), OK);
    assert_eq!(frame::get_i64(&query), EMBEDDING_F32 as i64);

    let mut embedding = frame::embedding_frame(EMBEDDING_F32);
    assert_eq!(session.call(op::FINISH_ENROLLMENT, &mut embedding), OK);
    assert_eq!(frame::get_i64(&embedding), EMBEDDING_F32 as i64);
    let profile = frame::decode_f32(&embedding[I64_PREFIX_BYTES..]);
    assert_eq!(profile.len(), EMBEDDING_F32);
    assert!(profile.iter().any(|value| *value != 0.0));
    profile
}

fn score(session: &Session, audio: &[f32]) -> (f64, f32, f32, i32) {
    let mut bytes = frame::encode_pcm(audio);
    bytes.resize(bytes.len() + WINDOW_RESULT_BYTES, 0);
    assert_eq!(session.call(op::SCORE_WINDOW, &mut bytes), OK);
    frame::decode_result(&bytes[bytes.len() - WINDOW_RESULT_BYTES..])
}

#[test]
fn lifecycle_rejects_dead_sessions_and_unknown_ops() {
    let session = Session::new();
    let mut frame = vec![0u8; I64_PREFIX_BYTES];
    assert_eq!(session.call(99, &mut frame), UNSUPPORTED);
    let id = session.id();
    unsafe { voce_adapter_destroy(id) };
    assert_eq!(call(id, op::EMBEDDING_DIM, &mut frame), INVALID_STATE);
    unsafe { voce_adapter_destroy(id) }; // idempotent, no double free
}

#[test]
fn create_load_enroll_score_process_and_roundtrip_profile() {
    let session = Session::new();
    let audio = tone(220.0, WINDOW_SAMPLES);
    let profile = enrolled(&session, &audio);

    let mut dim = vec![0u8; I64_PREFIX_BYTES];
    assert_eq!(session.call(op::EMBEDDING_DIM, &mut dim), OK);
    assert_eq!(frame::get_i64(&dim), EMBEDDING_F32 as i64);

    let (runtime, similarity, speech, decision) = score(&session, &audio);
    assert!(runtime >= 0.0);
    assert!(similarity > 0.9);
    assert_eq!(speech, 1.0);
    assert_eq!(decision, 1);

    let other = tone(900.0, WINDOW_SAMPLES);
    let (_, other_similarity, _, other_decision) = score(&session, &other);
    assert!(other_similarity < similarity - 0.3);
    assert_eq!(other_decision, 0);

    // Capacity query must carry the real PCM; the native ABI rejects NULL
    // input for a nonzero sample count.
    let batch = tone(220.0, WINDOW_SAMPLES + HOP_SAMPLES);
    let mut query = frame::process_frame(0, &batch);
    assert_eq!(session.call(op::PROCESS_QUERY, &mut query), OK);
    let count = frame::get_i64(&query);
    assert_eq!(count, 2);

    let mut too_small = frame::process_frame(1, &batch);
    assert_eq!(session.call(op::PROCESS_PCM, &mut too_small), BUFFER_TOO_SMALL);
    assert_eq!(frame::get_i64(&too_small), count);

    let mut results = frame::process_frame(count as usize, &batch);
    assert_eq!(session.call(op::PROCESS_PCM, &mut results), OK);
    assert_eq!(frame::get_i64(&results), count);
    let pcm_bytes = batch.len() * 4;
    for window in 0..count as usize {
        let offset = I64_PREFIX_BYTES + pcm_bytes + window * WINDOW_RESULT_BYTES;
        let (_, similarity, _, decision) =
            frame::decode_result(&results[offset..offset + WINDOW_RESULT_BYTES]);
        assert!(similarity > 0.9);
        assert_eq!(decision, 1);
    }

    let mut json_query = vec![0u8; I64_PREFIX_BYTES];
    assert_eq!(session.call(op::SERIALIZE_QUERY, &mut json_query), OK);
    let required = frame::get_i64(&json_query);
    assert!(required > 0);
    let mut json_frame = vec![0u8; I64_PREFIX_BYTES + required as usize];
    frame::put_i64(&mut json_frame, required);
    assert_eq!(session.call(op::SERIALIZE_PROFILE_JSON, &mut json_frame), OK);
    let written = frame::get_i64(&json_frame);
    assert!(written > 0 && written < required);
    let json = &json_frame[I64_PREFIX_BYTES..I64_PREFIX_BYTES + written as usize];
    assert_eq!(json_frame[I64_PREFIX_BYTES + written as usize], 0);
    let parsed: serde_json::Value = serde_json::from_slice(json).unwrap();
    assert!(parsed.is_object());

    let restored = Session::new();
    let mut path = b"mock://builtin\0".to_vec();
    assert_eq!(restored.call(op::LOAD_MODELS, &mut path), OK);
    assert_eq!(restored.call(op::DESERIALIZE_PROFILE_JSON, &mut json.to_vec()), OK);
    let mut restored_dim = vec![0u8; I64_PREFIX_BYTES];
    assert_eq!(restored.call(op::FINISH_QUERY, &mut restored_dim), OK);
    assert_eq!(frame::get_i64(&restored_dim), EMBEDDING_F32 as i64);
    let (_, restored_similarity, _, restored_decision) = score(&restored, &audio);
    assert_eq!(restored_similarity, similarity);
    assert_eq!(restored_decision, decision);
    assert_eq!(profile.len(), EMBEDDING_F32);
}

#[test]
fn malformed_frames_are_rejected_without_panics() {
    let session = Session::new();
    assert_eq!(session.call_len(op::ENROLL_PCM, &mut [0u8; 4], -1), -2);
    let mut short = vec![0u8; 4];
    assert_eq!(session.call(op::FINISH_QUERY, &mut short), -2);
    assert_eq!(session.call_len(op::LOAD_MODELS, &mut [], 0), -2);
    let mut unknown = vec![0u8; I64_PREFIX_BYTES];
    assert_eq!(session.call(0, &mut unknown), UNSUPPORTED);
}
