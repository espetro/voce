//! Gate 1 reference runner: reproduces the desktop app's production eval
//! pipeline (`src/eval.rs`, `src/inference.rs`, `src/enrollment/profile.rs`,
//! `src/model/wespeaker.rs`) on a directory of per-speaker WAV fixtures.
//!
//! Pipeline per the app:
//! - enrollment (`compute_profile`): non-overlapping 3 s windows at 1.5 s hop
//!   (`start += HOP`), RMS silence check, mean of L2-normalized embeddings,
//!   re-normalized (app averages the *normalized* embeddings and normalizes
//!   the profile; the `voce_native` shim averages normalized embeddings too).
//! - eval (`run_eval`): `EmbeddingWindowAccumulator` emits overlapping 3 s
//!   windows at 1.5 s hop (first window after 1.5 s of audio), each window
//!   goes through `process_window`: fast RMS silence check (-40 dBFS), neural
//!   VAD (not available on Linux — see docs/research/gate1-native-eval.md),
//!   cosine similarity, `SlidingVoteGate` with vote_window=3.
//!
//! The `voce_native` backend is exercised through the same C ABI functions the
//! probe uses, on the identical window grid.
//!
//! Usage:
//!   voce-gate1-runner --enroll spk_A.wav [--enroll spk_B.wav]
//!        --inputs spk_A.wav spk_B.wav ... [--threshold 0.75]
//!        [--model /path/to.onnx] --out report.json

use clap::Parser;
use serde_json::{json, Value};
use voce_native::constants::{HOP_SAMPLES, SAMPLE_RATE_HZ, WINDOW_SAMPLES};
use voce_native::wav;
use gate::SlidingVoteGate;

const CHUNK_SIZE: usize = 512;
const VOTE_WINDOW: usize = 3;

/// Sliding vote gate, transcribed from `src/filter/gate.rs`.
mod gate {
    use std::collections::VecDeque;

    pub struct SlidingVoteGate {
        history: VecDeque<bool>,
        vote_window: usize,
        pub threshold: f32,
        pub current_pass: bool,
    }

    impl SlidingVoteGate {
        pub fn new(threshold: f32, vote_window: usize) -> Self {
            Self {
                history: VecDeque::with_capacity(vote_window),
                vote_window,
                threshold,
                current_pass: true,
            }
        }

        pub fn update(&mut self, similarity: f32) -> bool {
            let frame_pass = similarity >= self.threshold;
            self.history.push_back(frame_pass);
            if self.history.len() > self.vote_window {
                self.history.pop_front();
            }
            let all_failed = self.history.len() == self.vote_window && self.history.iter().all(|&p| !p);
            self.current_pass = !all_failed;
            self.current_pass
        }
    }
}


#[derive(Parser, Debug)]
#[command(name = "voce-gate1-runner")]
struct Args {
    /// Enrollment WAV file(s); embeddings are pooled like compute_profile.
    #[arg(long = "enroll", required = true)]
    enroll: Vec<String>,
    /// Target/other WAV files scored against the enrollment.
    #[arg(long = "inputs", required = true, num_args = 1.., value_delimiter = ' ')]
    inputs: Vec<String>,
    /// Gate threshold (app default 0.75).
    #[arg(long, default_value_t = 0.75)]
    threshold: f32,
    /// ONNX embedder model path.
    #[arg(long)]
    model: String,
    /// Output JSON report path.
    #[arg(long)]
    out: String,
    /// Evaluate this many thresholds from 0.60 to 0.90 (sweep mode).
    #[arg(long, default_value_t = 0)]
    sweep: u32,
}

fn load_pcm(path: &str) -> Result<Vec<f32>, String> {
    let (samples, rate, _channels) = wav::read_wav(path)?;
    Ok(wav::resample_linear(&samples, rate, SAMPLE_RATE_HZ))
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

fn is_silent(samples: &[f32]) -> bool {
    if samples.is_empty() {
        return true;
    }
    let r = rms(samples);
    r == 0.0 || 20.0 * r.log10() < -40.0
}

fn enrollment_windows(pcm: &[f32]) -> Vec<&[f32]> {
    let mut out = Vec::new();
    let mut start = 0usize;
    while start + WINDOW_SAMPLES <= pcm.len() {
        out.push(&pcm[start..start + WINDOW_SAMPLES]);
        start += HOP_SAMPLES;
    }
    out
}

/// Accumulator window grid identical to `EmbeddingWindowAccumulator::push_chunk`
/// over CHUNK_SIZE pieces: emits every HOP_SAMPLES once ring holds a full window.
fn eval_windows(pcm: &[f32]) -> Vec<(usize, &[f32])> {
    let mut out = Vec::new();
    if pcm.len() < WINDOW_SAMPLES {
        return out;
    }
    let mut emitted_samples = 0usize;
    let mut consumed = 0usize;
    while consumed < pcm.len() {
        let take = CHUNK_SIZE.min(pcm.len() - consumed);
        consumed += take;
        emitted_samples += take;
        if emitted_samples >= HOP_SAMPLES && consumed >= WINDOW_SAMPLES {
            emitted_samples = 0;
            let start = consumed - WINDOW_SAMPLES;
            out.push((start, &pcm[start..start + WINDOW_SAMPLES]));
        }
    }
    out
}

struct Session {
    session: ort::session::Session,
}
fn embed(session: &mut ort::session::Session, window: &[f32]) -> Result<[f32; 256], String> {
    use ndarray::{Array2, Axis};
    use ort::value::Tensor;
    let fbank = compute_fbank80(window)?;
    let input = fbank.insert_axis(Axis(0));
    let tensor = Tensor::from_array((input.shape().to_vec(), input.into_raw_vec()))
        .map_err(|e| format!("input tensor: {e}"))?;
    let outputs = session
        .run(ort::inputs! { "feats" => tensor })
        .map_err(|e| format!("ONNX inference: {e}"))?;
    let (_shape, slice) = outputs["embs"]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("output tensor: {e}"))?;
    if slice.len() != 256 {
        return Err(format!("expected 256-dim embedding, got {}", slice.len()));
    }
    let mut v = [0f32; 256];
    v.copy_from_slice(slice);
    l2_normalize(&mut v);
    Ok(v)
}

fn l2_normalize(v: &mut [f32; 256]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-8);
    for x in v.iter_mut() {
        *x /= norm;
    }
}

fn cosine(a: &[f32; 256], b: &[f32; 256]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn compute_fbank80(samples: &[f32]) -> Result<ndarray::Array2<f32>, String> {
    use kaldi_native_fbank::online::FeatureComputer;
    use kaldi_native_fbank::{FbankComputer, FbankOptions, OnlineFeature};
    let mut opts = FbankOptions::default();
    opts.mel_opts.num_bins = 80;
    opts.use_energy = false;
    opts.frame_opts.window_type = "hamming".to_string();
    opts.frame_opts.dither = 0.0;
    let computer = FbankComputer::new(opts).map_err(|e| format!("fbank: {e}"))?;
    let dim = computer.dim();
    let mut online = OnlineFeature::new(FeatureComputer::Fbank(computer));
    online.accept_waveform(16000.0, samples);
    online.input_finished();
    let frames = online.num_frames_ready();
    if frames == 0 {
        return Err("no fbank frames".into());
    }
    let mut fbank = ndarray::Array2::zeros((frames, dim));
    for i in 0..frames {
        let frame = online.get_frame(i).unwrap();
        for (j, &val) in frame.iter().enumerate() {
            fbank[[i, j]] = val;
        }
    }
    let mean = fbank.mean_axis(ndarray::Axis(0)).unwrap();
    Ok(&fbank - &mean)
}

fn now_ms() -> f64 {
    std::time::Instant::now().elapsed().as_secs_f64() * 1000.0
}

fn main() {
    if let Err(err) = run() {
        eprintln!("voce-gate1-runner: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse();
    let started = std::time::Instant::now();

    let mut session = ort::session::Session::builder()
            .map_err(|e| format!("builder: {e}"))?
            .commit_from_file(&args.model)
            .map_err(|e| format!("load {}: {e}", args.model))?;
    let model_load_ms = started.elapsed().as_secs_f64() * 1000.0;

    // --- Enrollment exactly like compute_profile ---
    let enroll_start = std::time::Instant::now();
    let mut all_embeddings: Vec<[f32; 256]> = Vec::new();
    for path in &args.enroll {
        let pcm = load_pcm(path)?;
        for window in enrollment_windows(&pcm) {
            if !is_silent(window) {
                all_embeddings.push(embed(&mut session, window)?);
            }
        }
    }
    if all_embeddings.is_empty() {
        return Err("no valid enrollment windows".into());
    }
    let mut profile = [0f32; 256];
    let count = all_embeddings.len() as f32;
    for (slot, _) in profile.iter_mut().zip(&all_embeddings) {
        *slot = 0.0;
    }
    for emb in &all_embeddings {
        for (slot, val) in profile.iter_mut().zip(emb.iter()) {
            *slot += val;
        }
    }
    for slot in profile.iter_mut() {
        *slot /= count;
    }
    l2_normalize(&mut profile);
    let enroll_ms = enroll_start.elapsed().as_secs_f64() * 1000.0;

    let thresholds: Vec<f32> = if args.sweep > 0 {
        (0..=args.sweep)
            .map(|i| 0.60 + (0.30 * i as f32 / args.sweep as f32))
            .collect()
    } else {
        vec![args.threshold]
    };

    // Pre-compute similarities per input once (threshold-independent).
    let mut input_reports: Vec<Value> = Vec::new();
    let mut per_threshold_pass: std::collections::HashMap<String, usize> = Default::default();
    let mut runtime_total = 0.0f64;
    let mut window_total = 0usize;

    for path in &args.inputs {
        let pcm = load_pcm(path)?;
        let windows = eval_windows(&pcm);
        let mut sims: Vec<(usize, f32, bool)> = Vec::new(); // (start, sim, silent)
        let mut runtime_ms = 0.0f64;
        for (start, window) in &windows {
            let t0 = now_ms();
            if is_silent(window) {
                sims.push((*start, 0.0, true));
                runtime_ms += now_ms() - t0;
                continue;
            }
            let emb = embed(&mut session, window)?;
            let sim = cosine(&emb, &profile);
            runtime_ms += now_ms() - t0;
            sims.push((*start, sim, false));
        }
        runtime_total += runtime_ms;
        window_total += sims.len();

        let windows_json: Vec<Value> = sims
            .iter()
            .map(|(start, sim, silent)| {
                json!({
                    "start_sample": start,
                    "t_s": *start as f64 / SAMPLE_RATE_HZ as f64,
                    "similarity": sim,
                    "silent": silent,
                })
            })
            .collect();

        for thr in &thresholds {
            let mut gate = SlidingVoteGate::new(*thr, VOTE_WINDOW);
            let mut passed = 0usize;
            let mut decisions = Vec::new();
            for (start, sim, silent) in &sims {
                let decision = if *silent {
                    gate.current_pass // non-speech: gate unchanged, similarity 0
                } else {
                    gate.update(*sim)
                };
                if decision {
                    passed += 1;
                }
                decisions.push(decision);
            }
            *per_threshold_pass
                .entry(format!("{thr:.2}"))
                .or_insert(0usize) += passed;
            if *thr == thresholds[0] {
                input_reports.push(json!({
                    "path": path,
                    "sample_count": pcm.len(),
                    "duration_s": pcm.len() as f64 / SAMPLE_RATE_HZ as f64,
                    "window_count": sims.len(),
                    "passed_window_count_at_primary": passed,
                    "decisions_at_primary": decisions,
                    "runtime_ms_total": runtime_ms,
                    "runtime_ms_per_window": if sims.is_empty() { 0.0 } else { runtime_ms / sims.len() as f64 },
                    "windows": windows_json,
                }));
            }
        }
    }

    let report = json!({
        "runner": "voce-gate1-reference",
        "model": args.model,
        "model_load_ms": model_load_ms,
        "enroll_ms": enroll_ms,
        "enroll_window_count": all_embeddings.len(),
        "enroll_files": args.enroll,
        "sample_rate_hz": SAMPLE_RATE_HZ,
        "window_samples": WINDOW_SAMPLES,
        "hop_samples": HOP_SAMPLES,
        "vote_window": VOTE_WINDOW,
        "thresholds": thresholds,
        "passed_windows_per_threshold": per_threshold_pass,
        "window_count_total": window_total,
        "runtime_ms_total": runtime_total,
        "runtime_ms_per_window": if window_total == 0 { 0.0 } else { runtime_total / window_total as f64 },
        "rss_peak_bytes": voce_native::metrics::peak_rss_bytes(),
        "inputs": input_reports,
        "wall_ms_total": started.elapsed().as_secs_f64() * 1000.0,
    });
    let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(&args.out, format!("{text}\n")).map_err(|e| format!("write {}: {e}", args.out))?;
    println!("wrote {} ({} inputs, {} windows)", args.out, args.inputs.len(), window_total);
    Ok(())
}
