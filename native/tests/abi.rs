//! Integration tests that exercise only the exported C ABI.

use std::ffi::CString;
use std::os::raw::c_char;
use std::ptr;
use voce_native::abi::{
    voce_abi_version, voce_engine_create, voce_engine_deserialize_profile_json,
    voce_engine_destroy, voce_engine_embedding_dim, voce_engine_enroll_pcm,
    voce_engine_finish_enrollment, voce_engine_load_models,
    voce_engine_process_pcm, voce_engine_reset_enrollment,
    voce_engine_serialize_profile_json, voce_engine_score_window,
    voce_engine_set_threshold, VoceWindowResult,
};
use voce_native::constants::{EMBEDDING_DIM, HOP_SAMPLES, WINDOW_SAMPLES};

const OK: i32 = 0;
const BUFFER_TOO_SMALL: i32 = -4;
const NOT_ENROLLED: i32 = -5;
const INVALID_ARGUMENT: i32 = -2;
const GATE_PASS: i32 = 1;
const GATE_BLOCK: i32 = 0;

struct Engine(*mut voce_native::abi::VoceEngine);

impl Engine {
    fn new() -> Self {
        Self(unsafe { voce_engine_create() })
    }

    fn ptr(&self) -> *mut voce_native::abi::VoceEngine {
        self.0
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe { voce_engine_destroy(self.ptr()) }
    }
}

fn tone(frequency: f32, samples: usize) -> Vec<f32> {
    (0..samples)
        .map(|index| {
            (2.0 * std::f32::consts::PI * frequency * index as f32 / 16_000.0).sin() * 0.2
        })
        .collect()
}

fn enrolled_engine(audio: &[f32]) -> Engine {
    let engine = Engine::new();
    assert!(!engine.ptr().is_null());
    let models = CString::new("mock://builtin").unwrap();
    assert_eq!(
        unsafe { voce_engine_load_models(engine.ptr(), models.as_ptr()) },
        OK
    );
    assert_eq!(
        unsafe { voce_engine_enroll_pcm(engine.ptr(), audio.as_ptr(), audio.len() as i64) },
        OK
    );
    let mut len = 0i64;
    assert_eq!(
        unsafe { voce_engine_finish_enrollment(engine.ptr(), ptr::null_mut(), &mut len) },
        OK
    );
    let mut profile = vec![0.0f32; len as usize];
    assert_eq!(
        unsafe { voce_engine_finish_enrollment(engine.ptr(), profile.as_mut_ptr(), &mut len) },
        OK
    );
    assert_eq!(len as usize, EMBEDDING_DIM);
    engine
}

#[test]
fn abi_version_and_embedding_dim_are_reported() {
    assert_eq!(unsafe { voce_abi_version() }, 1);
    let engine = Engine::new();
    let mut dim = 0i64;
    assert_eq!(
        unsafe { voce_engine_embedding_dim(engine.ptr(), &mut dim) },
        OK
    );
    assert_eq!(dim, EMBEDDING_DIM as i64);
    unsafe { voce_engine_destroy(ptr::null_mut()) };
}

#[test]
fn scoring_requires_enrollment() {
    let audio = tone(220.0, WINDOW_SAMPLES);
    let engine = Engine::new();
    let mut result = VoceWindowResult::default();
    assert_eq!(
        unsafe {
            voce_engine_score_window(
                engine.ptr(),
                audio.as_ptr(),
                audio.len() as i64,
                &mut result,
            )
        },
        NOT_ENROLLED
    );
}

#[test]
fn scores_are_compared_against_the_enrolled_profile() {
    let enrolled = tone(220.0, WINDOW_SAMPLES);
    let other = tone(900.0, WINDOW_SAMPLES);
    let engine = enrolled_engine(&enrolled);

    let mut same = VoceWindowResult::default();
    assert_eq!(
        unsafe {
            voce_engine_score_window(
                engine.ptr(),
                enrolled.as_ptr(),
                enrolled.len() as i64,
                &mut same,
            )
        },
        OK
    );
    let mut different = VoceWindowResult::default();
    assert_eq!(
        unsafe {
            voce_engine_score_window(
                engine.ptr(),
                other.as_ptr(),
                other.len() as i64,
                &mut different,
            )
        },
        OK
    );
    assert!(same.similarity > different.similarity + 0.3);
    assert_eq!(same.decision, GATE_PASS);
    assert_eq!(different.decision, GATE_BLOCK);
    assert!(same.runtime_ms >= 0.0);

    let silence = vec![0.0; WINDOW_SAMPLES];
    let mut silent = VoceWindowResult::default();
    assert_eq!(
        unsafe {
            voce_engine_score_window(
                engine.ptr(),
                silence.as_ptr(),
                silence.len() as i64,
                &mut silent,
            )
        },
        OK
    );
    assert_eq!(silent.similarity, 0.0);
    assert_eq!(silent.decision, GATE_PASS);
}

#[test]
fn process_pcm_reports_total_and_writes_results() {
    let audio = tone(220.0, WINDOW_SAMPLES + HOP_SAMPLES);
    let engine = enrolled_engine(&audio[..WINDOW_SAMPLES]);
    let mut capacity = 0i64;
    let mut total = 0i64;
    assert_eq!(
        unsafe {
            voce_engine_process_pcm(
                engine.ptr(),
                audio.as_ptr(),
                audio.len() as i64,
                ptr::null_mut(),
                &mut capacity,
                &mut total,
            )
        },
        OK
    );
    assert_eq!(capacity, 2);
    assert_eq!(total, 2);

    let mut too_small = vec![VoceWindowResult::default(); 1];
    let mut small_capacity = 1i64;
    assert_eq!(
        unsafe {
            voce_engine_process_pcm(
                engine.ptr(),
                audio.as_ptr(),
                audio.len() as i64,
                too_small.as_mut_ptr(),
                &mut small_capacity,
                ptr::null_mut(),
            )
        },
        BUFFER_TOO_SMALL
    );
    assert_eq!(small_capacity, 2);
    assert_eq!(too_small[0], VoceWindowResult::default());

    let mut results = vec![VoceWindowResult::default(); capacity as usize];
    assert_eq!(
        unsafe {
            voce_engine_process_pcm(
                engine.ptr(),
                audio.as_ptr(),
                audio.len() as i64,
                results.as_mut_ptr(),
                &mut capacity,
                ptr::null_mut(),
            )
        },
        OK
    );
    assert_eq!(capacity, 2);
    assert!(results.iter().all(|result| result.decision == GATE_PASS));
}

#[test]
fn profile_json_roundtrips_and_rejects_bad_input() {
    let audio = tone(330.0, WINDOW_SAMPLES);
    let other = tone(700.0, WINDOW_SAMPLES);
    let engine = enrolled_engine(&audio);

    let mut capacity = 0i64;
    assert_eq!(
        unsafe {
            voce_engine_serialize_profile_json(engine.ptr(), ptr::null_mut(), &mut capacity)
        },
        OK
    );
    assert!(capacity > 0);

    let mut buffer = vec![0u8; capacity as usize];
    let mut written = capacity;
    assert_eq!(
        unsafe {
            voce_engine_serialize_profile_json(
                engine.ptr(),
                buffer.as_mut_ptr() as *mut c_char,
                &mut written,
            )
        },
        OK
    );
    assert_eq!(written + 1, capacity);
    assert_eq!(buffer[written as usize], 0);

    let mut small_capacity = 4i64;
    let mut small = vec![0u8; 4];
    assert_eq!(
        unsafe {
            voce_engine_serialize_profile_json(
                engine.ptr(),
                small.as_mut_ptr() as *mut c_char,
                &mut small_capacity,
            )
        },
        BUFFER_TOO_SMALL
    );
    assert_eq!(small_capacity, capacity);
    assert_eq!(small, vec![0u8; 4]);

    let restored = Engine::new();
    assert_eq!(
        unsafe {
            voce_engine_deserialize_profile_json(
                restored.ptr(),
                buffer.as_ptr() as *const c_char,
                written,
            )
        },
        OK
    );
    let mut original = VoceWindowResult::default();
    let mut copy = VoceWindowResult::default();
    assert_eq!(
        unsafe {
            voce_engine_score_window(
                engine.ptr(),
                other.as_ptr(),
                other.len() as i64,
                &mut original,
            )
        },
        OK
    );
    assert_eq!(
        unsafe {
            voce_engine_score_window(
                restored.ptr(),
                other.as_ptr(),
                other.len() as i64,
                &mut copy,
            )
        },
        OK
    );
    assert_eq!(original.similarity.to_bits(), copy.similarity.to_bits());
    assert_eq!(original.decision, copy.decision);

    let bad = CString::new("{}").unwrap();
    assert_eq!(
        unsafe {
            voce_engine_deserialize_profile_json(
                restored.ptr(),
                bad.as_ptr(),
                bad.as_bytes().len() as i64,
            )
        },
        INVALID_ARGUMENT
    );
}

#[test]
fn zero_length_enrollment_is_a_noop() {
    let engine = Engine::new();
    assert_eq!(
        unsafe { voce_engine_enroll_pcm(engine.ptr(), ptr::null(), 0) },
        OK
    );
    assert_eq!(
        unsafe { voce_engine_enroll_pcm(engine.ptr(), ptr::null(), -1) },
        INVALID_ARGUMENT
    );
}

#[test]
fn reset_and_threshold_are_enforced() {
    let audio = tone(220.0, WINDOW_SAMPLES);
    let engine = enrolled_engine(&audio);
    assert_eq!(
        unsafe { voce_engine_set_threshold(engine.ptr(), 1.5) },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { voce_engine_set_threshold(engine.ptr(), 0.9) },
        OK
    );
    let mut result = VoceWindowResult::default();
    assert_eq!(
        unsafe {
            voce_engine_score_window(
                engine.ptr(),
                audio.as_ptr(),
                audio.len() as i64,
                &mut result,
            )
        },
        OK
    );
    assert_eq!(result.decision, GATE_PASS);

    assert_eq!(
        unsafe { voce_engine_reset_enrollment(engine.ptr()) },
        OK
    );
    assert_eq!(
        unsafe {
            voce_engine_score_window(
                engine.ptr(),
                audio.as_ptr(),
                audio.len() as i64,
                &mut result,
            )
        },
        NOT_ENROLLED
    );
}
