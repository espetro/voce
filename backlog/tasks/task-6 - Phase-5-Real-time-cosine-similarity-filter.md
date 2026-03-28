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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Enrolled user voice passes with similarity > 0.85 in tracing logs
- [ ] #2 Different speaker is silenced (gate mutes output frames)
- [ ] #3 CPU usage < 15% on M1 during active filtering
- [ ] #4 Output callback is allocation-free and non-blocking (no channel recv in callback)
- [ ] #5 Fail-open: audio passes if model not loaded or inference errors
<!-- AC:END -->
