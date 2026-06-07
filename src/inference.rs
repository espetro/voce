//! Long-running inference task.
//!
//! Runs as a `tokio::spawn`'d async task that owns the `ModelSet` and
//! processes audio chunks for enrollment and real-time filtering.

use crate::{
    app_state::AppState,
    audio::buffer::{AudioChunk, EmbeddingWindowAccumulator},
    config,
    enrollment::{
        profile::compute_profile,
        recorder::{EnrollmentSession, EnrollmentStatus},
    },
    events::{AppEvent, InferenceCmd},
    filter::gate::SlidingVoteGate,
    model::{ModelSet, VadWrapper, cosine_similarity},
};
use anyhow::Result;
use crossbeam_channel::Receiver;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::{debug, error, info, warn};
use winit::event_loop::EventLoopProxy;

/// Result of processing one 16000-sample window through the full pipeline.
pub struct FrameResult {
    /// Cosine similarity vs. enrolled embedding (0.0 if non-speech or silent).
    pub similarity: f32,
    /// Whether the gate passed this frame.
    pub passed: bool,
    /// True when VAD classified the window as non-speech (similarity is 0.0).
    pub is_nonspeech: bool,
}

/// Run one pre-accumulated 16000-sample window through VAD → embedder → gate.
///
/// Returns `Ok(None)` when the window is below the fast-silence energy threshold
/// (gate is not updated; caller should treat as pass).  Returns `Ok(Some(r))`
/// for every audible window with the similarity score and gate decision.
///
/// Both the live filtering loop and `--eval` offline mode call this function,
/// ensuring zero pipeline divergence between the two paths.
pub async fn process_window(
    window: &[f32],
    models: &mut ModelSet,
    enrolled: &[f32; 256],
    gate: &mut SlidingVoteGate,
) -> Result<Option<FrameResult>> {
    // Fast RMS energy check — skip neural models for clearly silent windows
    if VadWrapper::is_silence_fast(window) {
        return Ok(None);
    }

    // Neural VAD: probability that this window contains speech
    let speech_prob = models.vad.speech_probability_16000(window).await.unwrap_or(0.0);
    if speech_prob < 0.5 {
        return Ok(Some(FrameResult { similarity: 0.0, passed: true, is_nonspeech: true }));
    }

    // Speaker embedding + cosine similarity
    let emb = models.embedder.extract_embedding(window).await?;
    let similarity = cosine_similarity(&emb, enrolled);
    let passed = gate.update(similarity);

    Ok(Some(FrameResult { similarity, passed, is_nonspeech: false }))
}

// Accumulation buffer for a 10-second test recording.
struct TestCapture {
    buffer: Vec<f32>,
    elapsed_samples: usize,
}

/// State the inference task can be in.
enum Mode {
    /// Waiting for a command.
    Idle,
    /// Accumulating a 20-second enrollment recording.
    Enrolling {
        index: u8,
        session: EnrollmentSession,
    },
    /// Both recordings complete; computing the profile (no audio consumed).
    ComputingProfile,
    /// Real-time speaker-gate filtering.
    /// `test_capture` is `Some` while a 10-second test recording is active.
    Filtering {
        enrolled: [f32; 256],
        gate: SlidingVoteGate,
        accumulator: EmbeddingWindowAccumulator,
        test_capture: Option<TestCapture>,
    },
}

/// Start the inference task. Runs until the process exits.
pub async fn run(
    mut models: ModelSet,
    audio_rx: Receiver<AudioChunk>,
    cmd_rx: Receiver<InferenceCmd>,
    gate_state: Arc<AtomicBool>,
    proxy: EventLoopProxy<AppEvent>,
) {
    info!("Inference task started");

    let mut mode = Mode::Idle;
    let mut enrollment_buffers: Vec<Vec<f32>> = Vec::new();
    let mut stats_sample_counter: u32 = 0;

    loop {
        // ---- Process commands (non-blocking) ----
        while let Ok(cmd) = cmd_rx.try_recv() {
            debug!("InferenceCmd: {:?}", cmd);
            match cmd {
                InferenceCmd::StartEnrollment { index } => {
                    info!("Starting enrollment recording {index}");
                    mode = Mode::Enrolling {
                        index,
                        session: EnrollmentSession::new(),
                    };
                    let _ = proxy.send_event(AppEvent::StateChanged(
                        if index == 1 { AppState::Recording { index: 1 } }
                        else          { AppState::Recording { index: 2 } }
                    ));
                }

                InferenceCmd::StartFilter { enrolled } => {
                    info!("Starting real-time filter");
                    let threshold = config::Config::load(&config::config_path())
                        .unwrap_or_default()
                        .threshold;
                    mode = Mode::Filtering {
                        enrolled: *enrolled,
                        gate: SlidingVoteGate::new(threshold, 3),
                        accumulator: EmbeddingWindowAccumulator::new(),
                        test_capture: None,
                    };
                    gate_state.store(true, Ordering::Relaxed);
                    let _ = proxy.send_event(AppEvent::StateChanged(AppState::Filtering));
                }

                InferenceCmd::StopFilter => {
                    info!("Stopping filter");
                    gate_state.store(true, Ordering::Relaxed); // fail-open
                    mode = Mode::Idle;
                    let _ = proxy.send_event(AppEvent::StateChanged(AppState::ActiveStandby));
                }

                InferenceCmd::StartTest => {
                    // Test capture runs *inside* the filtering loop so the gate
                    // continues to update while we record.
                    if let Mode::Filtering { test_capture, .. } = &mut mode {
                        info!("Starting test capture (filter continues running)");
                        *test_capture = Some(TestCapture {
                            buffer: Vec::with_capacity(16000 * 30),
                            elapsed_samples: 0,
                        });
                        let _ = proxy.send_event(AppEvent::StateChanged(AppState::Testing));
                    } else {
                        warn!("StartTest received while not in Filtering mode — ignored");
                    }
                }

                InferenceCmd::StopTest => {
                    if let Mode::Filtering { test_capture, .. } = &mut mode {
                        if let Some(tc) = test_capture.take() {
                            info!("Test capture stopped ({} samples)", tc.buffer.len());
                            let _ = proxy.send_event(AppEvent::TestCaptureComplete { samples: tc.buffer });
                        }
                    }
                }
            }
        }

        // ---- Process audio chunks (drain up to 20 per iteration) ----
        let mut chunks_processed = 0;
        while let Ok(chunk) = audio_rx.try_recv() {
            process_chunk(
                &chunk,
                &mut mode,
                &mut models,
                &mut enrollment_buffers,
                &gate_state,
                &proxy,
                &mut stats_sample_counter,
            )
            .await;

            chunks_processed += 1;
            if chunks_processed >= 20 { break; }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
    }
}

async fn process_chunk(
    chunk: &AudioChunk,
    mode: &mut Mode,
    models: &mut ModelSet,
    enrollment_buffers: &mut Vec<Vec<f32>>,
    gate_state: &Arc<AtomicBool>,
    proxy: &EventLoopProxy<AppEvent>,
    stats_counter: &mut u32,
) {
    match mode {
        // ---- Idle / ComputingProfile: discard audio ----
        Mode::Idle | Mode::ComputingProfile => {}

        // ---- Enrollment recording ----
        Mode::Enrolling { index, session } => {
            let is_speech = !VadWrapper::is_silence_fast(&chunk.samples);
            let idx = *index;

            match session.push_chunk(&chunk.samples, is_speech) {
                EnrollmentStatus::InProgress { elapsed_s, speech_s } => {
                    if chunk.seq % 11 == 0 {
                        let _ = proxy.send_event(AppEvent::RecordingProgress {
                            index: idx,
                            elapsed_s,
                            speech_s,
                        });
                    }
                }

                EnrollmentStatus::Complete { speech_s } => {
                    info!("Enrollment recording {idx} complete ({speech_s}s speech)");
                    let buffer = std::mem::replace(session, EnrollmentSession::new()).take_buffer();
                    enrollment_buffers.push(buffer);

                    let _ = proxy.send_event(AppEvent::RecordingComplete { index: idx, speech_s });

                    if enrollment_buffers.len() >= 2 {
                        *mode = Mode::ComputingProfile;
                        let _ = proxy.send_event(AppEvent::StateChanged(AppState::Adapting));
                        compute_and_save_profile(models, enrollment_buffers, gate_state, proxy).await;
                    } else {
                        *mode = Mode::Idle;
                    }
                }

                EnrollmentStatus::Invalid { reason } => {
                    warn!("Enrollment recording {idx} invalid: {reason}");
                    let _ = proxy.send_event(AppEvent::RecordingInvalid { index: idx });
                    *mode = Mode::Idle;
                }
            }
        }

        // ---- Real-time filtering (+ optional test capture) ----
        Mode::Filtering { enrolled, gate, accumulator, test_capture } => {
            // Compute whether this chunk passes the gate.
            // We avoid early `return` here so test_capture always gets updated.
            let pass: bool;
            let is_silence = VadWrapper::is_silence_fast(&chunk.samples);

            if is_silence {
                gate_state.store(true, Ordering::Relaxed);
                pass = true;
            } else if let Some(window) = accumulator.push_chunk(&chunk.samples) {
                // Full embedding window available — delegate to the shared pipeline
                match process_window(&window, models, enrolled, gate).await {
                    Ok(None) => {
                        // Silent window — fail-open
                        gate_state.store(true, Ordering::Relaxed);
                        pass = true;
                    }
                    Ok(Some(result)) => {
                        gate_state.store(result.passed, Ordering::Relaxed);
                        pass = result.passed;

                        if !result.is_nonspeech {
                            *stats_counter += 1;
                            if *stats_counter >= 22 {
                                *stats_counter = 0;
                                let _ = proxy.send_event(AppEvent::FilterStats {
                                    similarity: result.similarity,
                                    passing: result.passed,
                                });
                            }
                        }
                    }
                    Err(e) => {
                        warn!("process_window error: {e}");
                        gate_state.store(true, Ordering::Relaxed);
                        pass = true;
                    }
                }
            } else {
                // Window not yet full — read current gate state without changing it
                pass = gate_state.load(Ordering::Relaxed);
            }

            // ---- Test capture: record gated audio alongside live filter ----
            if let Some(tc) = test_capture {
                if pass {
                    tc.buffer.extend_from_slice(&chunk.samples);
                } else {
                    tc.buffer.extend(std::iter::repeat(0.0f32).take(chunk.samples.len()));
                }
                tc.elapsed_samples += chunk.samples.len();

                if chunk.seq % 22 == 0 {
                    let elapsed_s = (tc.elapsed_samples / 16000) as u32;
                    let _ = proxy.send_event(AppEvent::TestProgress { elapsed_s });
                }

                if tc.elapsed_samples >= 16000 * 10 {
                    // 10-second safety cap — stop even if user forgets to press Stop
                    let captured = std::mem::take(&mut tc.buffer);
                    *test_capture = None;
                    let _ = proxy.send_event(AppEvent::TestCaptureComplete { samples: captured });
                }
            }
        }
    }
}

async fn compute_and_save_profile(
    models: &mut ModelSet,
    buffers: &mut Vec<Vec<f32>>,
    gate_state: &Arc<AtomicBool>,
    proxy: &EventLoopProxy<AppEvent>,
) {
    info!("Computing voice profile from {} recordings…", buffers.len());

    let result = compute_profile(&mut models.embedder, buffers).await;
    buffers.clear();

    match result {
        Ok((profile, cross_sim)) => {
            if let Some(sim) = cross_sim {
                info!("enrollment cross-similarity logged: {sim:.3}");
            }
            let path = config::enrolled_embedding_path();
            match profile.save(&path) {
                Ok(()) => {
                    info!("Voice profile saved to {}", path.display());
                    let _ = proxy.send_event(AppEvent::StateChanged(AppState::TestReady));

                    if let Some(arr) = profile.as_array() {
                        gate_state.store(true, Ordering::Relaxed);
                        let _ = proxy.send_event(AppEvent::EnrolledProfileReady(Box::new(arr)));
                    }
                }
                Err(e) => error!("Failed to save voice profile: {e}"),
            }
        }
        Err(e) => {
            error!("Profile computation failed: {e}");
            let _ = proxy.send_event(AppEvent::RecordingInvalid { index: 255 });
        }
    }
}
