# Voce PRD — Roadmap

> Last updated: 2026-03-28

## Product Overview

**Name**: Voce
**Tagline**: Your voice. Only your voice. Even in the loudest café.
**Type**: macOS menubar app (POC phase)
**Goal**: A working proof-of-concept demonstrating end-to-end speaker-identity filtering on-device.

## Problem

Apple Voice Isolation and tools like Krisp suppress ambient noise but do NOT distinguish between human voices. A colleague speaking during a call bleeds through. The only solution is speaker-identity filtering.

## Roadmap

| Quarter | Milestone | Phases | Goal |
|---------|-----------|--------|------|
| **2026 Q1** | POC — Core Flow | 0–3 | Menubar app with onboarding, audio passthrough, model loading |
| **2026 Q1** | POC — Enrollment & Filter | 4–5 | Voice enrollment, real-time cosine similarity filter |
| **2026 Q2** | POC — Test & Polish | 6–7 | Test mode, playback, menubar polish |

### Phase Details

#### ✅ Phase 0 — Skeleton & Compilation Baseline
- **Effort**: S
- **Status**: Done (2026-03-27)
- **Goal**: Bootstrap Rust project with Cargo.toml, build.rs, menubar icon

#### ✅ Phase 1 — Audio I/O and BlackHole Passthrough
- **Effort**: M
- **Status**: Done (2026-03-27)
- **Goal**: cpal mic input at 22050 Hz mono f32, BlackHole 2ch output, pass-through

#### ✅ Phase 2 — ONNX Model Download and voxudio Wrappers
- **Effort**: M
- **Status**: Done (2026-03-27)
- **Goal**: Download silero-vad + speaker embedding ONNX models, wrap VAD + embedder

#### ✅ Phase 3 — Panel WebView UI and IPC Skeleton
- **Effort**: M
- **Status**: Done (2026-03-27)
- **Goal**: wry WebView panel anchored to tray icon, full onboarding HTML/CSS/JS

#### 🚧 Phase 4 — Enrollment Pipeline
- **Effort**: L
- **Status**: In Progress
- **Goal**: Two 20-second recordings with VAD validation, embedding extraction/averaging, save profile
- **Acceptance Criteria**:
  - [ ] Recording auto-stops at 20 seconds
  - [ ] Invalid recording detected when speech < 10s
  - [ ] enrolled_embedding.json saved with 256-dim array
  - [ ] Cosine similarity between recording pairs > 0.8
  - [ ] Panel UI reflects all enrollment states

#### 📋 Phase 5 — Real-time Cosine Similarity Filter
- **Effort**: M
- **Status**: To Do
- **Goal**: Sliding vote gate (3-frame history), AtomicBool gate_state, fail-open policy
- **Acceptance Criteria**:
  - [ ] Enrolled voice passes with similarity > 0.85
  - [ ] Different speaker is silenced
  - [ ] CPU < 15% on M1 during filtering
  - [ ] Output callback is allocation-free and non-blocking
  - [ ] Fail-open when model not loaded

#### 📋 Phase 6 — Test Mode and Filtered Audio Playback
- **Effort**: S
- **Status**: To Do
- **Goal**: 10-second test recording with filter live, playback via rodio
- **Acceptance Criteria**:
  - [ ] Test My Voice starts 10s gated capture
  - [ ] Panel shows 10s countdown
  - [ ] Filtered audio plays back through default output
  - [ ] Confirm/Re-enroll buttons work

#### 📋 Phase 7 — Menubar Polish and Live Stats
- **Effort**: S
- **Status**: To Do
- **Goal**: Dynamic icon swap, live similarity readout, re-enroll/quit menu
- **Acceptance Criteria**:
  - [ ] Three icon states swap correctly
  - [ ] Similarity updates in tray menu every ~500ms
  - [ ] Re-enroll restarts onboarding
  - [ ] Quit exits cleanly

## Definition of Done (POC)

1. `cargo run` on Apple Silicon Mac with BlackHole 2ch installed
2. Menubar icon appears (loading state)
3. Onboarding panel opens automatically
4. Record 2×20-second voice samples through panel UI
5. See "Adapting" then "Test Ready" state
6. Click "Test My Voice", speak 10 seconds, hear playback where only your voice is audible
7. Set BlackHole 2ch as mic in Zoom, confirm voice passes but others are filtered
8. Menubar icon pulses green while filter is active

## Out of Scope (POC)

- Cross-platform support (Windows/Linux)
- App Store distribution
- Model fine-tuning
- Automatic call detection
- Speaker diarization (multiple speakers)
- Preferences window
- Auto-update / telemetry

## Key Constraints

1. **Privacy**: All processing on-device. No audio/embeddings leave the machine.
2. **Latency**: Pipeline must add < 80ms end-to-end.
3. **Fail-open**: If inference fails, audio passes through unfiltered.
4. **No GPU assumption**: CPU + optional CoreML EP on Apple Silicon.
