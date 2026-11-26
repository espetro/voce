//! Plain C ABI implementation.
//!
//! Function signatures match `voce_native.h` exactly. Output buffers are
//! caller-owned; this module copies into them synchronously and retains no
//! pointers. Variable-length outputs use the query/required-capacity protocol
//! documented in the header.

use crate::constants::{ABI_VERSION, EMBEDDING_DIM};
use crate::engine::{MockEngine, WindowScore};
use crate::real::RealEngine;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::ptr;

/// Mirrors `VoceStatus` in `voce_native.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    NullArgument,
    InvalidArgument,
    InvalidState,
    BufferTooSmall,
    NotEnrolled,
    Io,
    Unsupported,
}

impl Status {
    pub fn code(self) -> c_int {
        match self {
            Status::Ok => 0,
            Status::NullArgument => -1,
            Status::InvalidArgument => -2,
            Status::InvalidState => -3,
            Status::BufferTooSmall => -4,
            Status::NotEnrolled => -5,
            Status::Io => -6,
            Status::Unsupported => -7,
        }
    }
}

/// Mirrors `VoceWindowResult` in `voce_native.h` (24 bytes on common ABIs).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VoceWindowResult {
    pub runtime_ms: f64,
    pub similarity: f32,
    pub speech_probability: f32,
    pub decision: i32,
}

impl From<WindowScore> for VoceWindowResult {
    fn from(score: WindowScore) -> Self {
        Self {
            runtime_ms: score.runtime_ms,
            similarity: score.similarity,
            speech_probability: score.speech_probability,
            decision: score.decision,
        }
    }
}

/// Backend selected by `voce_engine_load_models`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Deterministic spectral/envelope mock, used by ABI tests. Also chosen
    /// by the legacy sentinel path "mock://builtin".
    Mock,
    /// Real ONNX WeSpeaker inference, chosen when `load_models` resolves an
    /// ONNX model file (directly or inside a directory).
    Real,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Mock => "mock",
            Backend::Real => "real",
        }
    }
}

/// Active backend state behind one handle.
pub enum BackendState {
    Mock(MockEngine),
    Real(RealEngine),
}

/// Opaque engine handle. Callers only ever hold a pointer to this type.
pub struct VoceEngine {
    state: BackendState,
}

impl VoceEngine {
    pub fn backend(&self) -> Backend {
        match &self.state {
            BackendState::Mock(_) => Backend::Mock,
            BackendState::Real(_) => Backend::Real,
        }
    }

    pub fn mock(&self) -> Option<&MockEngine> {
        match &self.state {
            BackendState::Mock(engine) => Some(engine),
            BackendState::Real(_) => None,
        }
    }

    pub fn real(&self) -> Option<&RealEngine> {
        match &self.state {
            BackendState::Real(engine) => Some(engine),
            BackendState::Mock(_) => None,
        }
    }

    pub fn mock_mut(&mut self) -> Option<&mut MockEngine> {
        match &mut self.state {
            BackendState::Mock(engine) => Some(engine),
            BackendState::Real(_) => None,
        }
    }

    pub fn real_mut(&mut self) -> Option<&mut RealEngine> {
        match &mut self.state {
            BackendState::Real(engine) => Some(engine),
            BackendState::Mock(_) => None,
        }
    }

    fn score_window(&mut self, pcm: &[f32]) -> Result<WindowScore, String> {
        match &mut self.state {
            BackendState::Mock(engine) => engine.score_window(pcm),
            BackendState::Real(engine) => engine.score_window(pcm),
        }
    }

    fn process_pcm(&mut self, pcm: &[f32]) -> Result<Vec<WindowScore>, String> {
        match &mut self.state {
            BackendState::Mock(engine) => engine.process_pcm(pcm),
            BackendState::Real(engine) => engine.process_pcm(pcm),
        }
    }

    fn window_count(&self, n_samples: usize) -> usize {
        match &self.state {
            BackendState::Mock(engine) => engine.window_count(n_samples),
            BackendState::Real(engine) => engine.window_count(n_samples),
        }
    }

    fn is_enrolled(&self) -> bool {
        match &self.state {
            BackendState::Mock(engine) => engine.is_enrolled(),
            BackendState::Real(engine) => engine.is_enrolled(),
        }
    }

    fn set_threshold(&mut self, threshold: f32) -> Result<(), String> {
        match &mut self.state {
            BackendState::Mock(engine) => engine.set_threshold(threshold),
            BackendState::Real(engine) => engine.set_threshold(threshold),
        }
    }

    fn reset_enrollment(&mut self) {
        match &mut self.state {
            BackendState::Mock(engine) => engine.reset_enrollment(),
            BackendState::Real(engine) => engine.reset_enrollment(),
        }
    }

    fn enroll_pcm(&mut self, pcm: &[f32]) -> Result<(), String> {
        match &mut self.state {
            BackendState::Mock(engine) => engine.enroll_pcm(pcm),
            BackendState::Real(engine) => engine.enroll_pcm(pcm),
        }
    }

    fn finish_enrollment(&mut self) -> Result<[f32; EMBEDDING_DIM], String> {
        match &mut self.state {
            BackendState::Mock(engine) => engine.finish_enrollment(),
            BackendState::Real(engine) => engine.finish_enrollment(),
        }
    }

    fn serialize_profile_json(&self) -> Result<String, String> {
        match &self.state {
            BackendState::Mock(engine) => engine.serialize_profile_json(),
            BackendState::Real(engine) => engine.serialize_profile_json(),
        }
    }

    fn deserialize_profile_json(&mut self, bytes: &[u8]) -> Result<(), String> {
        match &mut self.state {
            BackendState::Mock(engine) => engine.deserialize_profile_json(bytes),
            BackendState::Real(engine) => engine.deserialize_profile_json(bytes),
        }
    }
}

unsafe fn engine_mut<'a>(handle: *mut VoceEngine) -> Result<&'a mut VoceEngine, Status> {
    if handle.is_null() {
        Err(Status::NullArgument)
    } else {
        Ok(&mut *handle)
    }
}

unsafe fn pcm_slice<'a>(pcm: *const f32, n_samples: i64) -> Result<&'a [f32], Status> {
    if n_samples < 0 {
        return Err(Status::InvalidArgument);
    }
    if n_samples == 0 {
        return Ok(&[]);
    }
    if pcm.is_null() {
        return Err(Status::NullArgument);
    }
    Ok(std::slice::from_raw_parts(pcm, n_samples as usize))
}

/// Copies a NUL-terminated JSON string. Required capacity includes the NUL,
/// while the success value in `*len` counts bytes excluding the NUL.
unsafe fn write_json(text: &str, out: *mut c_char, len: *mut i64) -> Status {
    if len.is_null() {
        return Status::NullArgument;
    }
    let capacity = *len;
    if capacity < 0 {
        return Status::InvalidArgument;
    }
    let bytes = text.len() as i64;
    let required = bytes + 1;
    *len = required;
    if out.is_null() {
        return Status::Ok;
    }
    if capacity < required {
        return Status::BufferTooSmall;
    }
    ptr::copy_nonoverlapping(text.as_ptr() as *const c_char, out, text.len());
    *out.add(text.len()) = 0;
    *len = bytes;
    Status::Ok
}

#[no_mangle]
pub unsafe extern "C" fn voce_abi_version() -> c_int {
    ABI_VERSION
}

#[no_mangle]
pub unsafe extern "C" fn voce_engine_create() -> *mut VoceEngine {
    Box::into_raw(Box::new(VoceEngine {
        state: BackendState::Mock(MockEngine::new()),
    }))
}

#[no_mangle]
pub unsafe extern "C" fn voce_engine_destroy(handle: *mut VoceEngine) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle));
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn voce_engine_load_models(
    handle: *mut VoceEngine,
    models_dir: *const c_char,
) -> c_int {
    let engine = match unsafe { engine_mut(handle) } {
        Ok(engine) => engine,
        Err(status) => return status.code(),
    };
    if models_dir.is_null() {
        return Status::NullArgument.code();
    }
    let path = unsafe { CStr::from_ptr(models_dir) }.to_bytes();
    if path.is_empty() {
        return Status::InvalidArgument.code();
    }
    let path = String::from_utf8_lossy(path).into_owned();

    // Backend selection (explicit, ABI v1 compatible):
    //   "mock://builtin" or any path under a directory named `mock` -> mock
    //   path resolving to an ONNX model file / dir -> real backend
    // Failure to load the real model returns VOCE_ERR_IO and keeps the
    // engine in the mock state so callers can fall back.
    let wants_mock = path == "mock://builtin"
        || path
            .rsplit('/')
            .find(|segment| !segment.is_empty())
            .map_or(false, |segment| segment == "mock");
    if wants_mock {
        match engine.mock_mut() {
            Some(mock) => {
                mock.reset_enrollment();
                return Status::Ok.code();
            }
            None => {
                engine.state = BackendState::Mock(MockEngine::new());
                return Status::Ok.code();
            }
        }
    }

    let mut real = match engine.real_mut() {
        Some(real) => std::mem::take(real),
        None => RealEngine::new(),
    };
    match real.load_models(&path) {
        Ok(()) => {
            engine.state = BackendState::Real(real);
            Status::Ok.code()
        }
        Err(_) => Status::Io.code(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn voce_engine_set_threshold(handle: *mut VoceEngine, threshold: f32) -> c_int {
    let engine = match unsafe { engine_mut(handle) } {
        Ok(engine) => engine,
        Err(status) => return status.code(),
    };
    match engine.set_threshold(threshold) {
        Ok(()) => Status::Ok.code(),
        Err(_) => Status::InvalidArgument.code(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn voce_engine_reset_enrollment(handle: *mut VoceEngine) -> c_int {
    let engine = match unsafe { engine_mut(handle) } {
        Ok(engine) => engine,
        Err(status) => return status.code(),
    };
    engine.reset_enrollment();
    Status::Ok.code()
}

#[no_mangle]
pub unsafe extern "C" fn voce_engine_enroll_pcm(
    handle: *mut VoceEngine,
    pcm: *const f32,
    n_samples: i64,
) -> c_int {
    let engine = match unsafe { engine_mut(handle) } {
        Ok(engine) => engine,
        Err(status) => return status.code(),
    };
    // Header contract: a zero sample count is a valid no-op with NULL PCM.
    if n_samples == 0 {
        return Status::Ok.code();
    }
    let samples = match unsafe { pcm_slice(pcm, n_samples) } {
        Ok(samples) => samples,
        Err(status) => return status.code(),
    };
    match engine.enroll_pcm(samples) {
        Ok(()) => Status::Ok.code(),
        Err(_) => Status::InvalidArgument.code(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn voce_engine_finish_enrollment(
    handle: *mut VoceEngine,
    out_embedding: *mut f32,
    in_out_embedding_len: *mut i64,
) -> c_int {
    let engine = match unsafe { engine_mut(handle) } {
        Ok(engine) => engine,
        Err(status) => return status.code(),
    };
    if in_out_embedding_len.is_null() {
        return Status::NullArgument.code();
    }
    let capacity = unsafe { *in_out_embedding_len };
    if capacity < 0 {
        return Status::InvalidArgument.code();
    }
    // A size query must not finalize enrollment: callers always query first.
    unsafe { *in_out_embedding_len = EMBEDDING_DIM as i64 };
    if out_embedding.is_null() {
        return Status::Ok.code();
    }
    if capacity < EMBEDDING_DIM as i64 {
        return Status::BufferTooSmall.code();
    }
    if engine.is_enrolled() {
        return Status::InvalidState.code();
    }
    let profile = match engine.finish_enrollment() {
        Ok(profile) => profile,
        Err(_) => return Status::InvalidState.code(),
    };
    unsafe { ptr::copy_nonoverlapping(profile.as_ptr(), out_embedding, profile.len()) };
    Status::Ok.code()
}

#[no_mangle]
pub unsafe extern "C" fn voce_engine_embedding_dim(handle: *mut VoceEngine, out_dim: *mut i64) -> c_int {
    if unsafe { engine_mut(handle) }.is_err() {
        return Status::NullArgument.code();
    }
    if out_dim.is_null() {
        return Status::NullArgument.code();
    }
    unsafe { *out_dim = EMBEDDING_DIM as i64 };
    Status::Ok.code()
}

#[no_mangle]
pub unsafe extern "C" fn voce_engine_score_window(
    handle: *mut VoceEngine,
    pcm: *const f32,
    n_samples: i64,
    out_result: *mut VoceWindowResult,
) -> c_int {
    let engine = match unsafe { engine_mut(handle) } {
        Ok(engine) => engine,
        Err(status) => return status.code(),
    };
    if out_result.is_null() {
        return Status::NullArgument.code();
    }
    if !engine.is_enrolled() {
        return Status::NotEnrolled.code();
    }
    let samples = match unsafe { pcm_slice(pcm, n_samples) } {
        Ok(samples) => samples,
        Err(status) => return status.code(),
    };
    let score = match engine.score_window(samples) {
        Ok(score) => score,
        Err(_) => return Status::InvalidArgument.code(),
    };
    unsafe { ptr::write(out_result, VoceWindowResult::from(score)) };
    Status::Ok.code()
}

#[no_mangle]
pub unsafe extern "C" fn voce_engine_process_pcm(
    handle: *mut VoceEngine,
    pcm: *const f32,
    n_samples: i64,
    out_results: *mut VoceWindowResult,
    in_out_results_len: *mut i64,
    out_windows_total: *mut i64,
) -> c_int {
    let engine = match unsafe { engine_mut(handle) } {
        Ok(engine) => engine,
        Err(status) => return status.code(),
    };
    if in_out_results_len.is_null() {
        return Status::NullArgument.code();
    }
    if !engine.is_enrolled() {
        return Status::NotEnrolled.code();
    }
    let samples = match unsafe { pcm_slice(pcm, n_samples) } {
        Ok(samples) => samples,
        Err(status) => return status.code(),
    };
    let capacity = unsafe { *in_out_results_len };
    if capacity < 0 {
        return Status::InvalidArgument.code();
    }
    let total = engine.window_count(samples.len()) as i64;
    if !out_windows_total.is_null() {
        unsafe { *out_windows_total = total };
    }
    unsafe { *in_out_results_len = total };
    if out_results.is_null() {
        return Status::Ok.code();
    }
    if capacity < total {
        return Status::BufferTooSmall.code();
    }
    let scores = match engine.process_pcm(samples) {
        Ok(scores) => scores,
        Err(_) => return Status::InvalidArgument.code(),
    };
    for (index, score) in scores.iter().enumerate() {
        unsafe { ptr::write(out_results.add(index), VoceWindowResult::from(*score)) };
    }
    Status::Ok.code()
}

#[no_mangle]
pub unsafe extern "C" fn voce_engine_serialize_profile_json(
    handle: *mut VoceEngine,
    out_json: *mut c_char,
    in_out_json_capacity_bytes: *mut i64,
) -> c_int {
    let engine = match unsafe { engine_mut(handle) } {
        Ok(engine) => engine,
        Err(status) => return status.code(),
    };
    let text = match engine.serialize_profile_json() {
        Ok(text) => text,
        Err(_) => return Status::InvalidState.code(),
    };
    unsafe { write_json(&text, out_json, in_out_json_capacity_bytes).code() }
}

#[no_mangle]
pub unsafe extern "C" fn voce_engine_deserialize_profile_json(
    handle: *mut VoceEngine,
    json: *const c_char,
    json_len_bytes: i64,
) -> c_int {
    let engine = match unsafe { engine_mut(handle) } {
        Ok(engine) => engine,
        Err(status) => return status.code(),
    };
    if json.is_null() {
        return Status::NullArgument.code();
    }
    if json_len_bytes < 0 {
        return Status::InvalidArgument.code();
    }
    let bytes: &[u8] = if json_len_bytes == 0 {
        unsafe { CStr::from_ptr(json) }.to_bytes()
    } else {
        unsafe { std::slice::from_raw_parts(json as *const u8, json_len_bytes as usize) }
    };
    if bytes.contains(&0) {
        return Status::InvalidArgument.code();
    }
    match engine.deserialize_profile_json(bytes) {
        Ok(()) => Status::Ok.code(),
        Err(_) => Status::InvalidArgument.code(),
    }
}
