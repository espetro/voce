---
id: TASK-3
title: 'Phase 2: ONNX model download and voxudio wrappers'
status: Done
assignee:
  - '@claude'
created_date: '2026-03-27 21:06'
updated_date: '2026-03-27 21:36'
labels:
  - phase-2
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Download silero-vad and speaker embedding ONNX models to ~/.voce/models/ on first launch. Wrap voxudio VoiceActivityDetector and SpeakerEmbeddingExtractor with thin typed wrappers.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Models downloaded to ~/.voce/models/ with progress reporting
- [x] #2 Download skipped if files already exist
- [x] #3 VadWrapper.detect_chunk returns speech probability 0-1
- [x] #4 EmbedderWrapper.extract_embedding returns 256-dim L2-normalised [f32;256]
- [x] #5 ModelReady event sent to winit event loop after successful init
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
VAD is async; needs 16000Hz (not 22050Hz); embedder needs 22050Hz.
Strategy: capture at 22050Hz. Per embedding window (22050 samples):
  - Resample 22050→16000 via voxudio::resample::<22050,16000>
  - Run vad.detect::<16000> on each 512-sample sub-chunk → mean prob
  - If speech: run embedder on original 22050Hz window
Model URLs (voxudio GitHub releases):
  - VAD: https://github.com/mzdk100/voxudio/releases/download/model/voice_activity_detector.onnx
  - Embedder: https://github.com/mzdk100/voxudio/releases/download/model/speaker_embedding_extractor.onnx

1. Implement download.rs with reqwest streaming + retry
2. Implement vad.rs wrapping VoiceActivityDetector (async detect::<16000>)
3. Implement embedder.rs wrapping SpeakerEmbeddingExtractor (async extract)
4. Wire tokio runtime into main.rs
5. Start model loading task on StartCause::Init
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase 2 complete.

Key API findings from voxudio source:
- VAD detect is ASYNC: pub async fn detect::<const SR: usize>
- VAD only supports SR=8000 or 16000 (or multiples of 16000) — 22050 fails validation
- Embedder extract is ASYNC: pub async fn extract(&mut self, audio: &[f32], channels: usize)
- voxudio exports resample::<SSR, TSR>() via rodio — no rubato needed
- Both model files on GitHub releases: github.com/mzdk100/voxudio/releases/tag/model

Impl:
- download.rs: streams both ONNX files from GitHub with 3-retry backoff, progress callbacks
- vad.rs: resamples 22050Hz window→16000Hz via voxudio::resample, then detect::<16000> on 512-sample sub-chunks, returns mean speech probability
- embedder.rs: thin wrapper around SpeakerEmbeddingExtractor, L2-normalises output
- main.rs: tokio multi-thread runtime (2 workers) started before event loop; model loading spawned as tokio task; models stored in SharedState

On first open voce.app: downloads ~2.2MB VAD + ~17MB embedder to ~/.voce/models/ — progress visible in ~/.voce/voce.log
<!-- SECTION:NOTES:END -->
