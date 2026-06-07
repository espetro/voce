---
id: TASK-6
title: 'Phase 5: Real-time cosine similarity filter'
status: To Do
assignee: []
created_date: '2026-03-27 21:06'
labels:
  - phase-5
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace audio passthrough with speaker identity gate. SlidingVoteGate (3-frame history). AtomicBool gate_state read by cpal output callback. Fail-open policy.
<!-- SECTION:DESCRIPTION:END -->

## Plan
<!-- SECTION:PLAN:BEGIN -->
- [x] SlidingVoteGate (3-frame window, fail-open) — filter/gate.rs
- [x] process_window() pipeline (silence → VAD → embedding → cosine → gate) — inference.rs
- [x] AtomicBool gate_state updated per frame; output callback reads it (no lock/alloc) — audio/output.rs
- [x] Fix default threshold 0.65 → 0.75 — config.rs
- [ ] Validate AC#1–3 end-to-end — `./eval/validate-ac.sh <enroll1.wav> [enroll2.wav] <other.wav>`
<!-- SECTION:PLAN:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Enrolled user voice passes with similarity > 0.85 in tracing logs
- [ ] #2 Different speaker is silenced (gate mutes output frames)
- [ ] #3 CPU usage < 15% on M1 during active filtering
- [x] #4 Output callback is allocation-free and non-blocking (no channel recv in callback)
- [x] #5 Fail-open: audio passes if model not loaded or inference errors
<!-- AC:END -->
