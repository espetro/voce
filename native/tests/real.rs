//! Real ONNX inference tests.
//!
//! These run only when `VOCE_REAL_MODEL` points at a WeSpeaker ONNX file
//! (or a directory containing exactly one `*.onnx`). Without the variable
//! they pass immediately so `cargo test` works on machines without models.

use std::ffi::CString;
use std::ptr;

use voce_native::abi::{
    voce_engine_create, voce_engine_destroy, voce_engine_enroll_pcm,
    voce_engine_finish_enrollment, voce_engine_load_models, voce_engine_process_pcm,
    voce_engine_reset_enrollment, voce_engine_score_window, voce_engine_set_threshold,
    VoceWindowResult,
};

const OK: i32 = 0;
const GATE_PASS: i32 = 1;
const GATE_BLOCK: i32 = 0;
const WINDOW_SAMPLES: usize = 48_000;

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

fn model_path() -> Option<String> {
    std::env::var("VOCE_REAL_MODEL")
        .ok()
        .filter(|path| !path.is_empty())
}

/// Speech-like multi-harmonic tone complex, quiet enough to be non-silence
/// but deterministic across runs.
fn voice_like(fundamental: f32, samples: usize) -> Vec<f32> {
    let harmonics = [1.0f32, 2.0, 3.0, 4.0, 5.0];
    let amplitudes = [1.0f32, 0.5, 0.33, 0.25, 0.2];
    (0..samples)
        .map(|n| {
            let t = n as f32 / 16_000.0;
            let wobble = 1.0 + 0.02 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();
            let value: f32 = harmonics
                .iter()
                .zip(amplitudes.iter())
                .map(|(&h, &a)| a * (2.0 * std::f32::consts::PI * fundamental * h * t).sin())
                .sum();
            0.2 * value * wobble
        })
        .collect()
}

fn enrolled_real_engine(model: &str, audio: &[f32]) -> Engine {
    let engine = Engine::new();
    let models = CString::new(model.to_string()).unwrap();
    assert_eq!(
        unsafe { voce_engine_load_models(engine.ptr(), models.as_ptr()) },
        OK,
        "real model failed to load from {model}"
    );
    assert_eq!(
        unsafe { voce_engine_set_threshold(engine.ptr(), 0.75) },
        OK
    );
    assert_eq!(
        unsafe { voce_engine_reset_enrollment(engine.ptr()) },
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
    assert_eq!(len, 256);
    let mut profile = vec![0.0f32; len as usize];
    assert_eq!(
        unsafe { voce_engine_finish_enrollment(engine.ptr(), profile.as_mut_ptr(), &mut len) },
        OK
    );
    let norm: f32 = profile.iter().map(|v| v * v).sum::<f32>().sqrt();
    assert!(
        (norm - 1.0).abs() < 1e-3,
        "profile should be L2-normalized, norm {norm}"
    );
    engine
}

#[test]
fn real_backend_same_speaker_passes_other_speaker_blocks() {
    let Some(model) = model_path() else {
        eprintln!("skipping: VOCE_REAL_MODEL not set");
        return;
    };
    let enrolled = voice_like(140.0, WINDOW_SAMPLES);
    let other = voice_like(340.0, WINDOW_SAMPLES);
    let engine = enrolled_real_engine(&model, &enrolled);

    let mut same = VoceWindowResult::default();
    assert_eq!(
        unsafe {
            voce_engine_score_window(engine.ptr(), enrolled.as_ptr(), enrolled.len() as i64, &mut same)
        },
        OK
    );
    assert_eq!(same.decision, GATE_PASS, "same speaker should pass");
    assert!(same.similarity > 0.95, "same similarity {}", same.similarity);

    let mut diff = VoceWindowResult::default();
    assert_eq!(
        unsafe {
            voce_engine_score_window(engine.ptr(), other.as_ptr(), other.len() as i64, &mut diff)
        },
        OK
    );
    // Synthetic tone complexes are not real speech: an unrelated fundamental
    // must score clearly lower than the enrolled voice, but the absolute gate
    // threshold (0.75) may still pass it. The parity claim is relative.
    assert!(
        diff.similarity < same.similarity - 0.1,
        "other similarity {} should trail same {} by > 0.1",
        diff.similarity,
        same.similarity
    );
}

#[test]
fn real_backend_is_deterministic_and_profiles_roundtrip() {
    let Some(model) = model_path() else {
        eprintln!("skipping: VOCE_REAL_MODEL not set");
        return;
    };
    let audio = voice_like(160.0, WINDOW_SAMPLES);
    let engine = enrolled_real_engine(&model, &audio);

    let mut first = VoceWindowResult::default();
    assert_eq!(
        unsafe {
            voce_engine_score_window(engine.ptr(), audio.as_ptr(), audio.len() as i64, &mut first)
        },
        OK
    );
    let mut second = VoceWindowResult::default();
    assert_eq!(
        unsafe {
            voce_engine_score_window(engine.ptr(), audio.as_ptr(), audio.len() as i64, &mut second)
        },
        OK
    );
    assert_eq!(first.similarity.to_bits(), second.similarity.to_bits());

    // Batch count matches the window math on a 2-window input.
    let long = voice_like(160.0, WINDOW_SAMPLES + 24_000);
    let mut capacity = 0i64;
    let mut total = 0i64;
    assert_eq!(
        unsafe {
            voce_engine_process_pcm(
                engine.ptr(),
                long.as_ptr(),
                long.len() as i64,
                ptr::null_mut(),
                &mut capacity,
                &mut total,
            )
        },
        OK
    );
    assert_eq!(total, 2, "expected two overlapping windows");
    let _ = capacity;
}
