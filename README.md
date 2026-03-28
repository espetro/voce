# Claude Code — Plan Mode Prompt: Voce POC (Rust, macOS)

> **Mode**: Plan first. Do not write any code until the full plan is approved.
> **Target**: macOS, Apple Silicon (M1/M2/M3), 8 GB RAM minimum.
> **Language**: Rust (2021 edition), no Python, no Node.js.
> **Goal**: A working proof-of-concept that demonstrates the full user flow end-to-end.

***

## 1. Product Requirements Document (PRD)

### 1.1 Product Overview

**Name**: Voce
**Tagline**: Your voice. Only your voice. Even in the loudest café.
**Type**: macOS menubar app (POC phase)
**Core Capability**: Enrolls a user's voice biometric on-device, then acts as a real-time virtual microphone that passes only the enrolled user's voice and silences all others — colleagues, baristas, background conversations.

### 1.2 Problem Statement

Apple Voice Isolation and tools like Krisp suppress ambient noise (fans, keyboards,
espresso machines), but they do NOT distinguish between human voices. When a colleague
sitting two metres away speaks during a call, it bleeds through. The user has no recourse
short of muting themselves or moving.

The only solution is **speaker-identity filtering**: learn what the user sounds like, then
gate all audio that does not match their voice profile.

### 1.3 Design Principles

1. **Privacy-first**: All processing is on-device. No audio or embeddings leave the machine.
2. **Zero friction**: The app lives in the menubar. No dock icon. No full-screen window.
3. **Opinionated UX**: The algorithm is completely hidden. The user records their voice,
   the app says "ready", and it just works.
4. **Reliable over clever**: Conservative cosine-similarity threshold. False silences are
   worse than occasional bleed-through for the POC.

### 1.4 Non-Goals (POC Phase)

- Cross-platform support (Windows/Linux — roadmap only)
- App Store distribution
- Fine-tuning / retraining model weights (enrollment = embedding extraction only)
- Background call detection (POC: the user manually activates the filter)
- Speaker diarization of multiple distinct people (POC: one enrolled speaker)

***

## 2. Technical Architecture

### 2.1 Pipeline Overview

```
[Physical Mic]
      │  (cpal: raw PCM f32 @ 16 kHz, mono)
      ▼
[Chunk Buffer]  ←── 512-sample ring buffer with 50% overlap
      │
      ▼
[VAD Gate]  ←── Silence below -40 dBFS is zeroed immediately (fast path)
      │  if speech energy detected:
      ▼
[Speaker Embedding Extractor]  ←── voxudio SpeakerEmbeddingExtractor (ONNX)
      │  1-second window → 192-dim L2-normalised embedding vector
      ▼
[Cosine Similarity]  ←── vs. stored enrolled_embedding (loaded from disk)
      │  similarity >= THRESHOLD (0.75 default) → PASS
      │  similarity <  THRESHOLD              → MUTE (zero the frame)
      ▼
[Virtual Mic Output]  ←── cpal output to BlackHole 2ch virtual device
      │  (user selects BlackHole as mic in Zoom / Meet / Teams)
      ▼
[Call App]
```

### 2.2 Speaker Enrollment (Onboarding)

Enrollment is NOT model fine-tuning. It is:

1. Record N seconds of the user's clean speech.
2. Extract a speaker embedding per 1-second window using `voxudio::SpeakerEmbeddingExtractor`.
3. Average all valid window embeddings → mean embedding vector.
4. L2-normalise the mean vector.
5. Serialize to `~/.voce/enrolled_embedding.json`.

At runtime, each incoming audio chunk's embedding is compared against this stored vector
via cosine similarity. The user experience describes this as "fine-tuning the model with
your voice" — which is accurate in spirit (the model adapts to their voice profile).

### 2.3 Recording Parameters

| Parameter | Value | Rationale |
|---|---|---|
| Recordings required | 2 | Minimum for reliable average embedding |
| Duration per recording | 20 seconds | 20 × 1s windows = 20 embedding samples; enough for a stable mean at low memory |
| Sample rate | 16,000 Hz | Required by speaker embedding models |
| Channels | Mono | Embedding models expect mono |
| Format | f32 PCM | Native cpal format, no conversion needed |
| Auto-stop | Yes, at 20s | No manual stop button needed |
| Minimum speech detected | 10s of VAD-positive frames | If user is silent, recording is invalid |

### 2.4 Crate Stack

| Component | Crate | Version | Notes |
|---|---|---|---|
| Audio I/O | `cpal` | `0.15` | Cross-platform; BlackHole shows as standard audio device |
| Speaker embedding + VAD | `voxudio` | latest | ONNX-backed; `SpeakerEmbeddingExtractor` + `VoiceActivityDetector` |
| ONNX runtime (transitive) | `ort` | `2.x` | Pulled in by voxudio; CoreML EP for Apple Silicon |
| Menubar icon | `tray-icon` | `0.19` | Tauri-family crate; macOS menubar, supports dynamic icon swap |
| Menubar menu | `muda` | latest | Companion to tray-icon |
| Panel window | `wry` | `0.47` | WebView2 / WKWebView; renders HTML/JS onboarding panel |
| Event loop | `winit` | `0.30` | Drives tray-icon and wry together |
| Async runtime | `tokio` | `1.x` | Full features; audio callbacks are sync, embedding inference is async |
| Serialization | `serde` + `serde_json` | `1.x` | Embedding storage |
| Logging | `tracing` + `tracing-subscriber` | `0.1` | Structured logs to stderr |
| Config/paths | `dirs` | `5.x` | `~/.voce/` data directory |
| Error handling | `anyhow` | `1.x` | POC-grade error propagation |

### 2.5 ONNX Model Files

The voxudio crate requires two ONNX checkpoint files at runtime:

- `~/.voce/models/voice_activity_detector.onnx`
- `~/.voce/models/speaker_embedding_extractor.onnx`

**POC strategy**: On first launch, if model files are absent, the app must download them
from a known public URL (HuggingFace hub or bundled in the binary via `include_bytes!`).
For the POC, downloading from HuggingFace at first launch is acceptable. Show a progress
state in the onboarding panel.

Recommended models:
- VAD: silero-vad ONNX (small, ~1 MB)
- Speaker embedding: pyannote embedding ONNX from `deepghs/pyannote-embedding-onnx`
  on HuggingFace (~17 MB), or WeSpeaker ResNet34 ONNX

### 2.6 BlackHole Dependency

The POC requires BlackHole 2ch to be installed by the user for the virtual mic output.
- If BlackHole is not detected at startup, the onboarding panel shows an inline warning
  with a direct link to `https://existential.audio/blackhole/`.
- BlackHole detection: enumerate cpal output devices and check for a device named
  containing "BlackHole".
- POC does NOT bundle or auto-install BlackHole.

### 2.7 Cosine Similarity Threshold

Default threshold: `0.75` (configurable via `~/.voce/config.json`).

At this threshold:
- Same speaker, different sessions: typically 0.85–0.95
- Different speakers: typically 0.20–0.60
- Same speaker, heavy background noise: typically 0.65–0.80

The POC uses a **sliding window vote**: the last 3 consecutive 1-second windows must
*all* be below threshold before muting, to avoid cutting mid-sentence.

***

## 3. User Flow (Detailed)

### State Machine

```
IDLE
  │  (app launches)
  ▼
MODEL_LOADING
  │  (models downloaded/verified; ~2–5 seconds cold start)
  ▼
ONBOARDING_READY
  │  (user sees green dot; BlackHole check passes)
  ▼
RECORDING_1         ← user clicks "Start Recording"
  │  (20 seconds, auto-stops)
  ▼
RECORDING_2         ← user clicks "Record Again"
  │  (20 seconds, auto-stops)
  ▼
ADAPTING            ← "fine-tuning" (embedding extraction + averaging; ~2–3 seconds)
  │
  ▼
TEST_READY          ← "Test Your Voice" button unlocked
  │  (user clicks; app runs pipeline for 10 seconds on live mic; plays back filtered audio)
  ▼
ACTIVE_STANDBY      ← panel closes, app watches mic activity
  │  (user joins a call; mic becomes active)
  ▼
FILTERING           ← dot pulses green; filter is live
  │  (call ends; mic goes idle)
  ▼
ACTIVE_STANDBY      ← back to standby
```

### 3.1 Step-by-Step UX

#### Step 1 — Install & Launch

The user downloads a `.app` bundle or runs `cargo install voce` (POC: `cargo run`).
No dock icon appears. The menubar icon appears immediately (a microphone glyph with an
orange dot indicating "loading").

#### Step 2 — Panel Opens on First Launch

A floating panel (not a window with a title bar) opens anchored below the menubar icon.
The panel is 320px wide, auto-height. It cannot be resized. It closes when the user
clicks outside it (like a system popover).

**Panel content during MODEL_LOADING state:**
```
  ● (orange dot, static)  Loading voice model…
  [progress bar, indeterminate]
  
  Voce learns to recognise your voice and filters
  out everyone else's on your calls.
```

If BlackHole is not installed, an inline warning appears below the progress bar:
```
  ⚠  BlackHole 2ch is required.  Install →
```

#### Step 3 — Onboarding: Voice Recording

Once MODEL_LOADING → ONBOARDING_READY:

```
  ● (green dot)  Model ready
  
  ─────────────────────────────
  Step 1 of 2 — Record Your Voice
  
  Speak naturally for 20 seconds in a quiet place.
  Read anything aloud: an article, your emails, 
  count numbers. The app needs to hear you clearly.
  
  [  Start Recording  ]   ← primary button, enabled
  
  Recording 1 of 2
```

When recording is active (RECORDING_1):
```
  ● (orange dot, pulsing)  Recording…  14s remaining
  
  [████████░░░░░░░░░░░░]   14 / 20s
  
  Keep talking naturally.
```

The recording stops automatically at 20 seconds. If less than 10 seconds of VAD-positive
speech was detected, show:
```
  ✗  Not enough speech detected. Please try again
     in a quieter environment.
  [  Retry  ]
```

Otherwise, transition to RECORDING_2:
```
  ✓  Recording 1 complete.
  
  Step 2 of 2 — One More Recording
  [  Start Recording  ]
```

Same flow. After both recordings are successful:

#### Step 4 — Adapting (Embedding Extraction)

```
  ● (orange dot, pulsing)  Adapting to your voice…
  
  Voce is building your voice profile.
  This takes just a moment.
```

Duration: 2–5 seconds on M1/M2/M3 (embedding inference on 40 × 1s windows).

#### Step 5 — Test Mode

```
  ● (green dot)  Ready — your voice profile is saved.
  
  ─────────────────────────────
  Test it out
  
  Click the button below, then speak for 10 seconds.
  You'll hear your voice played back with the filter 
  applied — only your voice should come through.
  
  [  Test My Voice  ]   ← enabled
```

During test:
```
  ● (green dot, pulsing)  Listening…  8s remaining
  
  [████████████░░░░░░░░]  12 / 20s
  
  Speak naturally — try having someone else talk too.
```

After 10 seconds: captured audio is played back via system audio output. No loopback
needed — the app just plays the filtered audio buffer back to the default output device.

```
  How did it sound?
  [  Looks good — I'm done  ]   [  Re-enroll  ]
```

If the user clicks "Looks good", the panel closes. If "Re-enroll", wipe the saved
embedding and restart from ONBOARDING_READY.

#### Step 6 — Standby & Active Filtering

After onboarding, the panel closes. The menubar icon shows:
- **Grey microphone** — standby (mic not in use)
- **Green pulsing dot** — filtering is active (mic in use, e.g., a call is happening)

The app detects mic activity by monitoring whether any other application has claimed
the audio input device via CoreAudio's `kAudioHardwarePropertyDefaultInputDevice`
listener, or by detecting non-silence on the input stream.

**POC simplification**: the filter is ALWAYS running on the virtual BlackHole device.
The icon reflects whether audio is being processed (i.e., whether the BlackHole input
device is receiving non-silent frames). The user must manually set BlackHole as their
mic in Zoom/Meet — a tooltip on the icon reminds them of this after onboarding.

Clicking the menubar icon in standby shows a minimal menu:
```
  ● Active — BlackHole 2ch
    Similarity: 0.87 (last 3s)   ← live readout in POC
  ─────────────────────────
    Re-enroll voice…
    Settings…
    Quit Voce
```

***

## 4. File & Project Structure

```
voce/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── build.rs                         ← macOS bundle metadata
├── assets/
│   ├── icons/
│   │   ├── voce_idle.png            ← grey mic, for menubar (22×22 @2x)
│   │   ├── voce_active.png          ← green mic, for menubar
│   │   └── voce_loading.png         ← orange mic, for menubar
│   └── panel/
│       ├── index.html               ← onboarding panel HTML
│       ├── style.css
│       └── panel.js                 ← communicates with Rust via wry IPC
├── src/
│   ├── main.rs                      ← entry point; event loop; tray-icon setup
│   ├── app_state.rs                 ← AppState enum + shared Arc<Mutex<State>>
│   ├── audio/
│   │   ├── mod.rs
│   │   ├── capture.rs               ← cpal mic input stream
│   │   ├── output.rs                ← cpal BlackHole output stream
│   │   └── buffer.rs                ← ring buffer + chunk splitter
│   ├── model/
│   │   ├── mod.rs
│   │   ├── download.rs              ← HuggingFace model download + integrity check
│   │   ├── vad.rs                   ← wraps voxudio VoiceActivityDetector
│   │   └── embedder.rs              ← wraps voxudio SpeakerEmbeddingExtractor
│   ├── enrollment/
│   │   ├── mod.rs
│   │   ├── recorder.rs              ← timed recording session (20s, auto-stop)
│   │   └── profile.rs               ← mean embedding computation + JSON serialisation
│   ├── filter/
│   │   └── gate.rs                  ← cosine similarity gate + sliding window vote
│   ├── panel/
│   │   └── ipc.rs                   ← Rust ↔ JS message types (serde JSON)
│   └── config.rs                    ← ~/.voce/config.json (threshold, paths)
```

***

## 5. IPC Contract (Rust ↔ Panel WebView)

The panel HTML/JS communicates with Rust using wry's `evaluate_script` (Rust → JS) and
`ipc_handler` (JS → Rust). All messages are JSON strings.

### JS → Rust (user actions)

```json
{ "cmd": "start_recording", "index": 1 }
{ "cmd": "start_recording", "index": 2 }
{ "cmd": "start_test" }
{ "cmd": "confirm_enrollment" }
{ "cmd": "reenroll" }
{ "cmd": "open_blackhole_link" }
```

### Rust → JS (state updates)

```json
{ "event": "state_changed", "state": "MODEL_LOADING" }
{ "event": "state_changed", "state": "ONBOARDING_READY" }
{ "event": "recording_progress", "index": 1, "elapsed_s": 7, "speech_s": 5 }
{ "event": "recording_complete", "index": 1, "speech_s": 14 }
{ "event": "recording_invalid", "index": 1, "reason": "insufficient_speech" }
{ "event": "state_changed", "state": "ADAPTING" }
{ "event": "state_changed", "state": "TEST_READY" }
{ "event": "test_progress", "elapsed_s": 3 }
{ "event": "state_changed", "state": "ACTIVE_STANDBY" }
{ "event": "filter_stats", "similarity": 0.87, "is_passing": true }
{ "event": "blackhole_status", "found": false }
```

***

## 6. Cargo.toml Dependencies (Starter)

```toml
[package]
name = "voce"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "voce"
path = "src/main.rs"

[dependencies]
# Audio
cpal = "0.15"

# Speaker embedding + VAD
voxudio = { version = "*", features = ["async"] }

# ONNX Runtime (pulled by voxudio; pin for CoreML EP)
ort = { version = "2", features = ["coreml"] }

# Menubar
tray-icon = "0.19"
muda = "*"

# Panel webview
wry = "0.47"
winit = "0.30"

# Async
tokio = { version = "1", features = ["full"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Utils
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
dirs = "5"
reqwest = { version = "0.12", features = ["stream"] }  # model download

[target.'cfg(target_os = "macos")'.dependencies]
# CoreAudio bindings for mic-in-use detection
coreaudio-sys = "0.2"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
```

***

## 7. Implementation Phases (for the Plan)

### Phase 0 — Skeleton & Compilation Baseline
- Create workspace, `Cargo.toml`, empty modules, `main.rs` that compiles.
- Verify all crates resolve on Apple Silicon.
- Display a static menubar icon (any PNG). App does not crash.

### Phase 1 — Audio I/O
- Enumerate audio input/output devices via cpal.
- Open mic input stream at 16 kHz mono f32.
- Detect BlackHole 2ch in output device list; log warning if absent.
- Open BlackHole output stream; pass mic audio through verbatim (no filter).
- Verify audio passthrough with a call app.

### Phase 2 — Model Loading
- Implement `model::download` to fetch ONNX files from HuggingFace if absent.
- Implement `model::vad` wrapping `voxudio::VoiceActivityDetector`.
- Implement `model::embedder` wrapping `voxudio::SpeakerEmbeddingExtractor`.
- Run inference on a static 1-second test WAV file; print embedding dimensions.

### Phase 3 — Panel UI
- Implement `wry` WebView panel anchored to menubar icon.
- Render `assets/panel/index.html` with full onboarding UI.
- Implement IPC: JS sends messages, Rust logs them. Rust sends dummy state events, JS
  updates the UI.

### Phase 4 — Enrollment Pipeline
- Implement `enrollment::recorder`: 20-second timed recording, auto-stop, VAD speech
  duration measurement.
- Implement `enrollment::profile`: extract embeddings from recorded buffers, average,
  normalise, save to `~/.voce/enrolled_embedding.json`.
- Wire Phase 3 panel UI to Phase 4 enrollment pipeline via IPC.

### Phase 5 — Real-time Filter
- Implement `filter::gate`: cosine similarity function, sliding window vote (3 frames).
- Replace passthrough in Phase 1 with gated output.
- Verify: enrolled user's voice passes; a different voice is silenced.

### Phase 6 — Test Mode & Playback
- Implement test recording (10s) and local playback via cpal default output.
- Wire "Test My Voice" button in panel to test pipeline.

### Phase 7 — Menubar State & Polish
- Dynamic menubar icon swapping (idle / active / loading).
- Live similarity readout in tray menu.
- "Re-enroll" and "Quit" menu items.
- Panel closes on outside click (winit focus-lost event).

***

## 8. Key Constraints & Decisions

1. **No GPU assumption.** All inference must run on CPU via ONNX Runtime with optional
   CoreML EP acceleration. Target: <15% CPU on M2 during active filtering.

2. **No microphone permission prompt bypass.** The app must declare
   `NSMicrophoneUsageDescription` in `Info.plist`. In the POC (non-App-Store), this is
   handled via `build.rs` generating a minimal `Info.plist`.

3. **Latency budget.** The end-to-end pipeline (mic → embedding → gate → BlackHole output)
   must introduce less than 80ms added latency. The 1-second embedding window means the
   gate decision lags by 1 second — this is acceptable for calls but must be documented
   clearly in the README.

4. **Thread model.** Audio callbacks are real-time threads (cpal). Embedding inference
   runs on a dedicated tokio blocking thread. Communication via `crossbeam-channel`
   (bounded, non-blocking send). If the inference thread is busy, the previous gate
   decision is reused (fail-open: pass audio through).

5. **Fail-open policy.** If embedding inference fails or the model is not loaded, audio
   PASSES through unfiltered. Never silence the user's mic silently on error.

6. **POC data directory.** All state lives in `~/.voce/`:
   - `models/` — ONNX files
   - `enrolled_embedding.json` — serialised mean embedding
   - `config.json` — threshold, version

7. **Panel is stateless HTML.** The WebView panel is destroyed and recreated on each
   open. State is always read from `AppState` (Rust-side), not from JS variables.

***

## 9. Out of Scope (Explicitly)

Do NOT implement the following in the POC:
- Automatic call detection (CoreAudio kAudioHardwarePropertyDefaultInputDevice hooking)
- Multiple enrolled speakers
- Model weight fine-tuning (LoRA, adapter layers, etc.)
- App Store entitlements / notarisation
- Preferences window
- Auto-update mechanism
- Telemetry of any kind

***

## 10. Definition of Done (POC)

The POC is complete when a developer can:

1. `cargo run` on an Apple Silicon Mac with BlackHole 2ch installed.
2. See the menubar icon appear (loading state).
3. See the onboarding panel open automatically.
4. Record 2 × 20-second voice samples through the panel UI.
5. See the "Adapting" state and then "Test Ready".
6. Click "Test My Voice", speak for 10 seconds (optionally with a second person present),
   and hear the playback where only their voice is audible.
7. Set BlackHole 2ch as the microphone in Zoom, join a call, and confirm that their voice
   passes through but a colleague's voice is filtered out.
8. See the menubar icon pulse green while the filter is active.

***

*End of prompt. Begin by stating your full plan across all 8 phases before writing any code.*
