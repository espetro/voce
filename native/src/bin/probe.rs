//! CLI probe / fixture runner for the Voce native shim.
//!
//! Example:
//!   cargo run -p voce-native --bin voce-native-probe -- \
//!       --fixture-dir fixtures --repeat 2 --out /tmp/voce-probe.json
//!
//! All scoring goes through the exported C ABI, so this binary doubles as an
//! FFI smoke test. JSON reports embedding dimension, per-window scores and
//! decisions, per-window runtime, RSS, and repeated-run determinism.

use serde_json::{json, Value};
use std::env;
use std::ffi::CString;
use std::fs;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::ptr;
use voce_native::abi;
use voce_native::abi::{VoceEngine, VoceWindowResult};
use voce_native::constants::{HOP_SAMPLES, SAMPLE_RATE_HZ, WINDOW_SAMPLES};
use voce_native::metrics::{current_rss_bytes, digest_results, peak_rss_bytes};
use voce_native::wav;

#[derive(Debug, Clone, Copy, PartialEq)]
enum BackendArg {
    Mock,
    Real,
}

impl BackendArg {
    fn as_str(self) -> &'static str {
        match self {
            BackendArg::Mock => "mock",
            BackendArg::Real => "real",
        }
    }
}

use abi::{
    voce_abi_version, voce_engine_create, voce_engine_deserialize_profile_json,
    voce_engine_destroy, voce_engine_embedding_dim, voce_engine_enroll_pcm,
    voce_engine_finish_enrollment, voce_engine_load_models,
    voce_engine_process_pcm, voce_engine_reset_enrollment,
    voce_engine_serialize_profile_json, voce_engine_set_threshold,
};

thread_local! {
    static MODEL_LOAD_MS: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

struct Engine(*mut VoceEngine);

impl Engine {
    fn new() -> Result<Self, String> {
        let ptr = unsafe { voce_engine_create() };
        if ptr.is_null() {
            return Err("voce_engine_create returned NULL".into());
        }
        Ok(Self(ptr))
    }

    fn ptr(&self) -> *mut VoceEngine {
        self.0
    }

    fn check(code: i32, operation: &str) -> Result<(), String> {
        if code == 0 {
            Ok(())
        } else {
            Err(format!("{operation} failed with status {code}"))
        }
    }

    fn configure(
        &self,
        threshold: f32,
        backend: BackendArg,
        models_dir: Option<&str>,
    ) -> Result<(), String> {
        let models_dir = match backend {
            BackendArg::Mock => "mock://builtin".to_string(),
            BackendArg::Real => match models_dir {
                Some(dir) => dir.to_string(),
                None => return Err("--backend real requires --models-dir".to_string()),
            },
        };
        let models = CString::new(models_dir.as_str()).map_err(|err| err.to_string())?;
        let load_started = std::time::Instant::now();
        Self::check(
            unsafe { voce_engine_load_models(self.ptr(), models.as_ptr()) },
            "voce_engine_load_models",
        )?;
        MODEL_LOAD_MS
            .with(|cell| cell.set(load_started.elapsed().as_secs_f64() * 1000.0));
        Self::check(
            unsafe { voce_engine_set_threshold(self.ptr(), threshold) },
            "voce_engine_set_threshold",
        )?;
        Self::check(
            unsafe { voce_engine_reset_enrollment(self.ptr()) },
            "voce_engine_reset_enrollment",
        )
    }

    fn enroll(&self, samples: &[f32]) -> Result<(), String> {
        Self::check(
            unsafe {
                voce_engine_enroll_pcm(self.ptr(), samples.as_ptr(), samples.len() as i64)
            },
            "voce_engine_enroll_pcm",
        )
    }

    fn finish_profile(&self) -> Result<Vec<f32>, String> {
        let mut len = 0i64;
        Self::check(
            unsafe { voce_engine_finish_enrollment(self.ptr(), ptr::null_mut(), &mut len) },
            "voce_engine_finish_enrollment size query",
        )?;
        let mut profile = vec![0.0f32; len.max(0) as usize];
        Self::check(
            unsafe { voce_engine_finish_enrollment(self.ptr(), profile.as_mut_ptr(), &mut len) },
            "voce_engine_finish_enrollment",
        )?;
        profile.truncate(len.max(0) as usize);
        Ok(profile)
    }

    fn embedding_dim(&self) -> Result<i64, String> {
        let mut dim = 0i64;
        Self::check(
            unsafe { voce_engine_embedding_dim(self.ptr(), &mut dim) },
            "voce_engine_embedding_dim",
        )?;
        Ok(dim)
    }

    fn profile_json(&self) -> Result<String, String> {
        let mut capacity = 0i64;
        Self::check(
            unsafe {
                voce_engine_serialize_profile_json(self.ptr(), ptr::null_mut(), &mut capacity)
            },
            "voce_engine_serialize_profile_json size query",
        )?;
        let mut buffer = vec![0u8; capacity.max(0) as usize];
        let mut len = capacity;
        Self::check(
            unsafe {
                voce_engine_serialize_profile_json(
                    self.ptr(),
                    buffer.as_mut_ptr() as *mut c_char,
                    &mut len,
                )
            },
            "voce_engine_serialize_profile_json",
        )?;
        buffer.truncate(len.max(0) as usize);
        String::from_utf8(buffer).map_err(|err| format!("profile JSON was not UTF-8: {err}"))
    }

    fn restore_profile_json(&self, json: &str) -> Result<(), String> {
        Self::check(
            unsafe {
                voce_engine_deserialize_profile_json(
                    self.ptr(),
                    json.as_ptr() as *const c_char,
                    json.len() as i64,
                )
            },
            "voce_engine_deserialize_profile_json",
        )
    }

    fn batch(&self, samples: &[f32]) -> Result<Vec<VoceWindowResult>, String> {
        let mut capacity = 0i64;
        let mut total = 0i64;
        Self::check(
            unsafe {
                voce_engine_process_pcm(
                    self.ptr(),
                    samples.as_ptr(),
                    samples.len() as i64,
                    ptr::null_mut(),
                    &mut capacity,
                    &mut total,
                )
            },
            "voce_engine_process_pcm size query",
        )?;
        if total != capacity {
            return Err(format!("batch size query mismatch: {capacity} vs {total}"));
        }
        let mut results = vec![VoceWindowResult::default(); capacity.max(0) as usize];
        Self::check(
            unsafe {
                voce_engine_process_pcm(
                    self.ptr(),
                    samples.as_ptr(),
                    samples.len() as i64,
                    results.as_mut_ptr(),
                    &mut capacity,
                    &mut total,
                )
            },
            "voce_engine_process_pcm",
        )?;
        results.truncate(capacity.max(0) as usize);
        Ok(results)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe { voce_engine_destroy(self.ptr()) }
    }
}

#[derive(Debug)]
struct Options {
    enroll: Vec<String>,
    inputs: Vec<String>,
    fixture_dir: Option<String>,
    threshold: f32,
    repeats: usize,
    out: Option<String>,
    backend: BackendArg,
    models_dir: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enroll: Vec::new(),
            inputs: Vec::new(),
            fixture_dir: None,
            threshold: 0.75,
            repeats: 2,
            out: None,
            backend: BackendArg::Mock,
            models_dir: None,
        }
    }
}

fn parse_args() -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        let (key, inline_value) = match arg.split_once('=') {
            Some((key, value)) => (key.to_string(), Some(value.to_string())),
            None => (arg, None),
        };
        let value = match inline_value {
            Some(value) => value,
            None => args
                .next()
                .ok_or_else(|| format!("{key} requires a value"))?,
        };
        match key.as_str() {
            "--enroll" => options.enroll.push(value),
            "--input" => options.inputs.push(value),
            "--fixture-dir" => options.fixture_dir = Some(value),
            "--out" => options.out = Some(value),
            "--threshold" => {
                options.threshold = value
                    .parse::<f32>()
                    .map_err(|err| format!("invalid --threshold: {err}"))?;
            }
            "--repeat" => {
                options.repeats = value
                    .parse::<usize>()
                    .map_err(|err| format!("invalid --repeat: {err}"))?;
                if options.repeats == 0 || options.repeats > 16 {
                    return Err("--repeat must be between 1 and 16".into());
                }
            }
            "--backend" => {
                options.backend = match value.as_str() {
                    "mock" => BackendArg::Mock,
                    "real" => BackendArg::Real,
                    other => return Err(format!("unknown --backend: {other} (mock|real)")),
                };
            }
            "--models-dir" => options.models_dir = Some(value),
            _ => return Err(format!("unknown option: {key}")),
        }
    }
    Ok(options)
}

fn resolve_fixture_dir(dir: &str) -> Result<(Vec<String>, Vec<String>), String> {
    let root = Path::new(dir);
    let manifest = root.join("manifest.json");
    if manifest.exists() {
        let text = fs::read_to_string(&manifest)
            .map_err(|err| format!("cannot read {}: {err}", manifest.display()))?;
        let value: Value =
            serde_json::from_str(&text).map_err(|err| format!("invalid manifest: {err}"))?;
        let resolve = |name: &str| root.join(name).to_string_lossy().into_owned();
        let enroll = match value.get("enroll") {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(resolve)
                .collect(),
            Some(Value::String(item)) => vec![resolve(item)],
            _ => Vec::new(),
        };
        let inputs = match value.get("inputs") {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(resolve)
                .collect(),
            Some(Value::String(item)) => vec![resolve(item)],
            _ => Vec::new(),
        };
        return Ok((enroll, inputs));
    }

    let enroll = root.join("enroll.wav");
    if !enroll.exists() {
        return Err(format!(
            "fixture directory {} needs manifest.json or enroll.wav",
            root.display()
        ));
    }
    let mut inputs: Vec<PathBuf> = fs::read_dir(root)
        .map_err(|err| format!("cannot read {}: {err}", root.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().map_or(false, |ext| ext == "wav")
                && path.file_name().map_or(false, |name| name != "enroll.wav")
        })
        .collect();
    inputs.sort();
    Ok((
        vec![enroll.to_string_lossy().into_owned()],
        inputs
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
    ))
}

fn load_pcm(path: &str) -> Result<Vec<f32>, String> {
    let (samples, rate, _channels) = wav::read_wav(path)?;
    Ok(wav::resample_linear(&samples, rate, SAMPLE_RATE_HZ))
}

fn run_once(
    options: &Options,
    enroll_files: &[String],
    input_files: &[String],
) -> Result<Value, String> {
    let rss_start = current_rss_bytes();
    let engine = Engine::new()?;
    engine.configure(options.threshold, options.backend, options.models_dir.as_deref())?;
    let model_load_ms = MODEL_LOAD_MS.with(|cell| cell.get());

    let mut enrolled_samples = 0usize;
    for path in enroll_files {
        let samples = load_pcm(path)?;
        enrolled_samples += samples.len();
        engine.enroll(&samples)?;
    }
    let profile = engine.finish_profile()?;
    let embedding_dim = engine.embedding_dim()?;
    let profile_json = engine.profile_json()?;

    let restored = Engine::new()?;
    restored.configure(options.threshold, options.backend, options.models_dir.as_deref())?;
    restored.restore_profile_json(&profile_json)?;

    let mut inputs = Vec::new();
    let mut digests = Vec::new();
    let mut runtime_total = 0.0f64;
    let mut window_total = 0usize;
    let mut passed_total = 0usize;

    for path in input_files {
        let samples = load_pcm(path)?;
        let results = engine.batch(&samples)?;
        let restored_results = restored.batch(&samples)?;
        let roundtrip_ok = results.len() == restored_results.len()
            && results.iter().zip(restored_results.iter()).all(|(a, b)| {
                a.similarity.to_bits() == b.similarity.to_bits()
                    && a.decision == b.decision
            });
        let digest = digest_results(&results);
        digests.push(digest.clone());
        let windows: Vec<Value> = results
            .iter()
            .enumerate()
            .map(|(index, result)| {
                json!({
                    "index": index,
                    "t_s": (index * HOP_SAMPLES) as f64 / SAMPLE_RATE_HZ as f64,
                    "similarity": result.similarity,
                    "speech_probability": result.speech_probability,
                    "decision": result.decision,
                    "runtime_ms": result.runtime_ms,
                })
            })
            .collect();
        let runtime_ms: f64 = results.iter().map(|result| result.runtime_ms).sum();
        let passed = results
            .iter()
            .filter(|result| result.decision == 1)
            .count();
        runtime_total += runtime_ms;
        window_total += results.len();
        passed_total += passed;
        inputs.push(json!({
            "path": path,
            "sample_count": samples.len(),
            "duration_s": samples.len() as f64 / SAMPLE_RATE_HZ as f64,
            "window_count": results.len(),
            "passed_window_count": passed,
            "runtime_ms_total": runtime_ms,
            "runtime_ms_per_window": if results.is_empty() { 0.0 } else { runtime_ms / results.len() as f64 },
            "digest": digest,
            "profile_roundtrip_ok": roundtrip_ok,
            "windows": windows,
        }));
    }

    let rss_end = current_rss_bytes();
    let run_digest = digests.join("|");
    Ok(json!({
        "digest": run_digest,
        "backend": options.backend.as_str(),
        "model_load_ms": model_load_ms,
        "embedding_dimension": embedding_dim,
        "profile_dimension": profile.len(),
        "profile_json_bytes": profile_json.len(),
        "enroll_sample_count": enrolled_samples,
        "input_count": input_files.len(),
        "window_count": window_total,
        "passed_window_count": passed_total,
        "runtime_ms_total": runtime_total,
        "runtime_ms_per_window": if window_total == 0 { 0.0 } else { runtime_total / window_total as f64 },
        "rss_start_bytes": rss_start,
        "rss_end_bytes": rss_end,
        "inputs": inputs,
    }))
}

fn run() -> Result<(), String> {
    let options = parse_args()?;
    let (enroll_files, input_files) = match &options.fixture_dir {
        Some(dir) => resolve_fixture_dir(dir)?,
        None => (options.enroll.clone(), options.inputs.clone()),
    };
    if enroll_files.is_empty() {
        return Err("at least one --enroll file (or fixture enroll.wav) is required".into());
    }
    if input_files.is_empty() {
        return Err("at least one --input file (or fixture WAV) is required".into());
    }

    let mut runs = Vec::new();
    for _ in 0..options.repeats {
        runs.push(run_once(&options, &enroll_files, &input_files)?);
    }
    let reference = runs[0]["digest"].as_str().unwrap_or("");
    let deterministic = runs
        .iter()
        .all(|run| run["digest"].as_str().unwrap_or("") == reference);

    let report = json!({
        "abi_version": unsafe { voce_abi_version() },
        "backend": options.backend.as_str(),
        "sample_rate_hz": SAMPLE_RATE_HZ,
        "window_samples": WINDOW_SAMPLES,
        "hop_samples": HOP_SAMPLES,
        "embedding_dimension": runs[0]["embedding_dimension"],
        "threshold": options.threshold,
        "enroll_files": enroll_files,
        "input_files": input_files,
        "repeat_count": options.repeats,
        "deterministic": deterministic,
        "rss_peak_bytes": peak_rss_bytes(),
        "runs": runs,
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("cannot serialize report: {err}"))?;
    match &options.out {
        Some(path) => fs::write(path, format!("{text}\n"))
            .map_err(|err| format!("cannot write {path}: {err}"))?,
        None => println!("{text}"),
    }
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("voce-native-probe: {err}");
        std::process::exit(2);
    }
}
