---
id: TASK-2
title: 'Phase 1: Audio I/O and BlackHole passthrough'
status: Done
assignee:
  - '@claude'
created_date: '2026-03-27 21:06'
updated_date: '2026-03-27 21:18'
labels:
  - phase-1
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
cpal mic input at 22050Hz mono f32, BlackHole 2ch output device detection, 512-sample chunk pipeline, pass-through (no filtering yet).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Default mic opens at 22050 Hz mono f32 without error
- [x] #2 BlackHole 2ch detected in output device list (or warning logged if absent)
- [x] #3 Mic audio passes through to BlackHole output (verifiable in Zoom)
- [x] #4 512-sample AudioChunk structs flow through crossbeam bounded(64) channel
- [x] #5 EmbeddingWindowAccumulator implemented (22050 window, 11025 hop)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Implement capture.rs: enumerate cpal devices, open default input at 22050Hz mono f32, split into 512-sample AudioChunks, try_send to bounded(64) channel
2. Implement output.rs: enumerate cpal output devices, find BlackHole (case-insensitive), open stereo f32 output, duplicate mono→L+R, read AtomicBool gate_state
3. Wire both streams in main.rs: create channels + AtomicBool, start streams on ModelReady or at startup
4. Send BlackHoleStatus event at startup
5. Handle 22050Hz unavailability gracefully
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase 1 complete.

- capture.rs: opens default mic at 22050Hz mono f32; splits into 512-sample AudioChunks via try_send to bounded(64) channel; down-mixes stereo→mono; falls back gracefully if 22050Hz unsupported
- output.rs: finds BlackHole by case-insensitive "blackhole" substring; opens stereo f32 output at 22050Hz; VecDeque ring buffer for mic samples; reads AtomicBool gate_state (fail-open=true) in real-time callback without blocking
- main.rs: starts audio pipeline at StartCause::Init; sends BlackHoleStatus event; gate_state AtomicBool shared between output callback and future inference task
- cpal 0.17 SampleRate is plain u32 (not a newtype) — corrected from spec assumptions
- Run via: open voce.app (then set BlackHole 2ch as mic in Zoom to verify passthrough)
<!-- SECTION:NOTES:END -->
