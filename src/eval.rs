//! Offline evaluation modes.
//!
//! `voce --eval <wav> [--output <path>] [--enrollment <path>]`
//!   Feeds a WAV through the VAD → embedder → gate pipeline; emits JSONL per
//!   window; optionally writes a filtered WAV where blocked chunks are silenced.
//!
//! `voce --eval-enroll <wav> [--eval-enroll-out <path>]`
//!   Computes a speaker embedding from a WAV and saves it as the enrolled
//!   profile (same format as the GUI enrollment flow).
//!
//! Exit codes: 0 on success, 1 on any error.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use crate::{
    audio::buffer::EmbeddingWindowAccumulator,
    config,
    enrollment::profile::{compute_profile, VoiceProfile},
    filter::gate::SlidingVoteGate,
    inference::process_window,
    model::{ModelSet, VadWrapper},
};

const CHUNK_SIZE: usize = 512;
const HOP: usize = 24000; // 1.5 s hop — matches EmbeddingWindowAccumulator
const SAMPLE_RATE: f32 = 16000.0;

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Entry point for `--eval` mode.
///
/// Runs the full pipeline on `wav_path`.  If `output_path` is given, writes a
/// filtered WAV where chunks that did not pass the gate are replaced with
/// silence.  If `enrollment_path` is given it is used instead of the default
/// `~/.voce/enrolled_embedding.json`.
pub async fn run_eval(
    wav_path: PathBuf,
    output_path: Option<PathBuf>,
    enrollment_path: Option<PathBuf>,
) -> Result<i32> {
    if !wav_path.exists() {
        eprintln!("error: file not found: {}", wav_path.display());
        return Ok(1);
    }

    // --- Load enrolled embedding ---
    let profile_path = enrollment_path.unwrap_or_else(config::enrolled_embedding_path);
    let profile = VoiceProfile::load(&profile_path).with_context(|| {
        format!(
            "failed to load enrolled embedding from {} — run enrollment in the GUI first",
            profile_path.display()
        )
    })?;
    let enrolled: [f32; 256] = profile.as_array().ok_or_else(|| {
        anyhow::anyhow!(
            "enrolled embedding has wrong dimension (got {}, expected 256)",
            profile.embedding.len()
        )
    })?;

    // --- Load ONNX models ---
    let models_dir = config::models_dir();
    let mut models = ModelSet::load(&models_dir, |_| {})
        .await
        .context("failed to load ONNX models")?;

    // --- Read WAV and normalise to mono f32 at 16000 Hz ---
    let samples = load_wav_as_f32_16000(&wav_path)?;

    // --- Load threshold / vote_window from config ---
    let cfg = config::Config::load(&config::config_path()).unwrap_or_default();

    // --- Run pipeline ---
    let mut accumulator = EmbeddingWindowAccumulator::new();
    let mut gate = SlidingVoteGate::new(cfg.threshold, cfg.vote_window);
    let mut window_index: u64 = 0;

    // Gate state between windows: fail-open (true = passing through)
    let mut current_gate_pass = true;

    // Pre-allocate filtered audio buffer only when output is requested
    let mut filtered: Vec<f32> = if output_path.is_some() {
        Vec::with_capacity(samples.len())
    } else {
        Vec::new()
    };

    for chunk in samples.chunks(CHUNK_SIZE) {
        let is_silent_chunk = VadWrapper::is_silence_fast(chunk);

        // Always push every chunk (including silent) so the accumulator's window
        // positions stay aligned with wall-clock audio time.
        if let Some(window) = accumulator.push_chunk(chunk) {
            let t_s = window_index as f32 * HOP as f32 / SAMPLE_RATE;

            match process_window(&window, &mut models, &enrolled, &mut gate).await {
                Ok(None) => {
                    // VAD classified window as non-speech (silence or noise).
                    // Preserve gate state: enrolled-speaker pauses stay open,
                    // other-speaker pauses stay closed.
                    println!(
                        r#"{{"t_s":{t_s:.3},"similarity":0.000,"passed":{current_gate_pass}}}"#
                    );
                }
                Ok(Some(r)) => {
                    current_gate_pass = r.passed;
                    println!(
                        r#"{{"t_s":{t_s:.3},"similarity":{sim:.3},"passed":{pass}}}"#,
                        sim = r.similarity,
                        pass = r.passed,
                    );
                }
                Err(e) => {
                    eprintln!("process_window error at t={t_s:.3}s: {e}");
                    return Ok(1);
                }
            }

            window_index += 1;
        }

        // Collect filtered audio: silent chunks always output silence regardless of
        // gate state; non-silent chunks follow the gate.
        if output_path.is_some() {
            if is_silent_chunk || !current_gate_pass {
                filtered.extend(std::iter::repeat_n(0.0f32, chunk.len()));
            } else {
                filtered.extend_from_slice(chunk);
            }
        }
    }

    if let Some(ref path) = output_path {
        write_wav_f32(path, &filtered, 16000)?;
    }

    Ok(0)
}

/// Entry point for `--eval-enroll` mode.
///
/// Computes a speaker embedding from `wav_path` (and optionally `wav_path2`)
/// and saves it to `out_path` (defaults to `~/.voce/enrolled_embedding.json`).
/// When two recordings are provided, prints the cross-similarity to stderr.
pub async fn run_enroll_from_wav(
    wav_path: PathBuf,
    wav_path2: Option<PathBuf>,
    out_path: Option<PathBuf>,
) -> Result<i32> {
    if !wav_path.exists() {
        eprintln!("error: file not found: {}", wav_path.display());
        return Ok(1);
    }

    let mut recordings = vec![load_wav_as_f32_16000(&wav_path)?];

    if let Some(ref p2) = wav_path2 {
        if !p2.exists() {
            eprintln!("error: file not found: {}", p2.display());
            return Ok(1);
        }
        recordings.push(load_wav_as_f32_16000(p2)?);
    }

    let models_dir = config::models_dir();
    let mut models = ModelSet::load(&models_dir, |_| {})
        .await
        .context("failed to load ONNX models")?;

    let (profile, cross_sim) = compute_profile(&mut models.embedder, &recordings)
        .await
        .context("failed to compute embedding from WAV")?;

    if let Some(sim) = cross_sim {
        eprintln!("enrollment cross-similarity: {sim:.3}");
        if sim < 0.8 {
            eprintln!("warning: cross-similarity {sim:.3} < 0.8 — recordings may be different speakers / low quality");
        }
    }

    let save_path = out_path.unwrap_or_else(config::enrolled_embedding_path);
    profile
        .save(&save_path)
        .with_context(|| format!("failed to save embedding to {}", save_path.display()))?;

    eprintln!(
        "Enrollment saved: {} (dim={})",
        save_path.display(),
        profile.dim
    );
    Ok(0)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write mono f32 samples to a WAV file.
fn write_wav_f32(path: &PathBuf, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .with_context(|| format!("cannot create output WAV: {}", path.display()))?;
    for &s in samples {
        writer.write_sample(s)?;
    }
    writer.finalize()?;
    Ok(())
}

/// Load a WAV file as mono f32 samples at 16000 Hz.
///
/// Handles 16-bit PCM int (LibriSpeech native), 32-bit PCM int, and 32-bit float.
/// Collapses stereo to mono by averaging channels.
/// Resamples to 16000 Hz with rubato if the source rate differs.
fn load_wav_as_f32_16000(path: &PathBuf) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("cannot open WAV file: {}", path.display()))?;
    let spec = reader.spec();

    let samples_raw: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / 32_768.0))
            .collect::<hound::Result<_>>()
            .context("error reading 16-bit PCM samples")?,

        (hound::SampleFormat::Int, 32) => reader
            .samples::<i32>()
            .map(|s| s.map(|v| v as f32 / 2_147_483_648.0))
            .collect::<hound::Result<_>>()
            .context("error reading 32-bit PCM samples")?,

        (hound::SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .collect::<hound::Result<_>>()
            .context("error reading 32-bit float samples")?,

        (fmt, bits) => bail!("unsupported WAV format: {:?} {}-bit", fmt, bits),
    };

    // Collapse multi-channel to mono by averaging
    let channels = spec.channels as usize;
    let samples_mono: Vec<f32> = if channels == 1 {
        samples_raw
    } else {
        samples_raw
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    if spec.sample_rate == 16000 {
        Ok(samples_mono)
    } else {
        resample_to_16000(&samples_mono, spec.sample_rate as usize)
            .with_context(|| format!("resampling from {} Hz to 16000 Hz failed", spec.sample_rate))
    }
}

/// Resample mono f32 audio from `from_rate` Hz to 16000 Hz using rubato FftFixedIn.
///
/// Processes in 4096-sample chunks for memory efficiency.  The output is trimmed
/// to the expected length (extra samples arise from zero-padding the final chunk).
fn resample_to_16000(input: &[f32], from_rate: usize) -> Result<Vec<f32>> {
    use rubato::{FftFixedIn, Resampler};

    if input.is_empty() {
        return Ok(Vec::new());
    }

    const CHUNK: usize = 4096;
    let mut resampler = FftFixedIn::<f32>::new(from_rate, 16000, CHUNK, 2, 1)
        .context("failed to construct FftFixedIn resampler")?;

    let expected_out_len = (input.len() as f64 * 16000.0 / from_rate as f64).ceil() as usize;

    // Pad input to a multiple of CHUNK
    let remainder = input.len() % CHUNK;
    let pad = if remainder == 0 { 0 } else { CHUNK - remainder };

    let padded: Vec<f32> = input
        .iter()
        .copied()
        .chain(std::iter::repeat_n(0.0f32, pad))
        .collect();

    let mut output = Vec::with_capacity(expected_out_len + CHUNK);
    for (i, chunk) in padded.chunks(CHUNK).enumerate() {
        let out = resampler
            .process(&[chunk.to_vec()], None)
            .with_context(|| format!("resampler failed at chunk {i}"))?;
        output.extend_from_slice(&out[0]);
    }

    output.truncate(expected_out_len);
    Ok(output)
}
