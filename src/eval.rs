//! Offline evaluation mode: `voce --eval <wav_path>`
//!
//! Loads the enrolled embedding and ONNX models, then feeds a WAV file through
//! the exact same VAD → embedder → cosine-similarity → gate pipeline used in
//! live filtering.  Emits one JSONL line per embedding window to stdout.
//!
//! Exit codes: 0 on success, 1 on any error (missing file, bad WAV, model failure).

use anyhow::{Context, Result, bail};
use std::path::PathBuf;

use crate::{
    audio::buffer::EmbeddingWindowAccumulator,
    config,
    enrollment::profile::VoiceProfile,
    filter::gate::SlidingVoteGate,
    inference::process_window,
    model::ModelSet,
};

const CHUNK_SIZE: usize = 512;
const HOP: usize = 11025;
const SAMPLE_RATE: f32 = 22050.0;

/// Entry point for `--eval` mode.  Returns 0 on success, 1 on any error.
pub async fn run_eval(wav_path: PathBuf) -> Result<i32> {
    // --- Validate path early for a clean error message ---
    if !wav_path.exists() {
        eprintln!("error: file not found: {}", wav_path.display());
        return Ok(1);
    }

    // --- Load enrolled embedding ---
    let profile_path = config::enrolled_embedding_path();
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

    // --- Read WAV and normalise to mono f32 at 22050 Hz ---
    let samples = load_wav_as_f32_22050(&wav_path)?;

    // --- Load threshold / vote_window from config ---
    let cfg = config::Config::load(&config::config_path()).unwrap_or_default();

    // --- Run pipeline ---
    let mut accumulator = EmbeddingWindowAccumulator::new();
    let mut gate = SlidingVoteGate::new(cfg.threshold, cfg.vote_window);
    let mut window_index: u64 = 0;

    for chunk in samples.chunks(CHUNK_SIZE) {
        if let Some(window) = accumulator.push_chunk(chunk) {
            let t_s = window_index as f32 * HOP as f32 / SAMPLE_RATE;

            match process_window(&window, &mut models, &enrolled, &mut gate).await {
                Ok(None) => {
                    // Silent window — pass through, emit with similarity 0
                    println!(
                        r#"{{"t_s":{t_s:.3},"similarity":0.000,"passed":true}}"#
                    );
                }
                Ok(Some(r)) => {
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
    }

    Ok(0)
}

/// Load a WAV file as mono f32 samples at 22050 Hz.
///
/// Handles 16-bit PCM int (LibriSpeech native), 32-bit PCM int, and 32-bit float.
/// Collapses stereo to mono by averaging channels.
/// Resamples to 22050 Hz with rubato if the source rate differs.
fn load_wav_as_f32_22050(path: &PathBuf) -> Result<Vec<f32>> {
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

    if spec.sample_rate == 22050 {
        Ok(samples_mono)
    } else {
        resample_to_22050(&samples_mono, spec.sample_rate as usize).with_context(|| {
            format!(
                "resampling from {} Hz to 22050 Hz failed",
                spec.sample_rate
            )
        })
    }
}

/// Resample mono f32 audio from `from_rate` Hz to 22050 Hz using rubato FftFixedIn.
///
/// Processes in 4096-sample chunks for memory efficiency.  The output is trimmed
/// to the expected length (extra samples arise from zero-padding the final chunk).
fn resample_to_22050(input: &[f32], from_rate: usize) -> Result<Vec<f32>> {
    use rubato::{FftFixedIn, Resampler};

    if input.is_empty() {
        return Ok(Vec::new());
    }

    const CHUNK: usize = 4096;
    let mut resampler = FftFixedIn::<f32>::new(from_rate, 22050, CHUNK, 2, 1)
        .context("failed to construct FftFixedIn resampler")?;

    let expected_out_len =
        (input.len() as f64 * 22050.0 / from_rate as f64).ceil() as usize;

    // Pad input to a multiple of CHUNK
    let remainder = input.len() % CHUNK;
    let pad = if remainder == 0 { 0 } else { CHUNK - remainder };

    let padded: Vec<f32> = input
        .iter()
        .copied()
        .chain(std::iter::repeat(0.0f32).take(pad))
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
