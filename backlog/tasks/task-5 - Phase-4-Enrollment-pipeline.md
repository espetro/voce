---
id: TASK-5
title: 'Phase 4: Enrollment pipeline'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-03-27 21:06'
updated_date: '2026-03-27 21:48'
labels:
  - phase-4
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two 20-second timed recordings with auto-stop and VAD speech validation. Embedding extraction+averaging over recording windows. Save enrolled_embedding.json to ~/.voce/.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Recording auto-stops at exactly 20 seconds
- [ ] #2 Invalid recording detected when speech < 10s and retry offered
- [ ] #3 enrolled_embedding.json saved with 256-element array after both recordings
- [ ] #4 Cosine similarity between the two recording embeddings is > 0.8
- [ ] #5 Panel UI reflects recording progress, adapting state, and test-ready state
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Key design decisions:
- InferenceCmd channel (crossbeam unbounded) from main → inference task
- Second audio channel from capture → inference task (clone per chunk)
- Inference task: tokio::spawn, polls both channels via try_recv + 5ms sleep
- Enrollment: per-chunk silence check (fast path), accumulate in EnrollmentSession
- Profile computation: async inline in inference task after both buffers collected
- If enrolled_embedding.json exists at startup → skip to ActiveStandby

Files:
1. src/events.rs: AppEvent + InferenceCmd (avoid circular dep)
2. src/inference.rs: run_inference_task
3. capture.rs: add optional inference_tx second sender
4. enrollment/profile.rs: compute_profile async fn
5. main.rs: wire inference task, PanelCmd::StartRecording, startup check
<!-- SECTION:PLAN:END -->
