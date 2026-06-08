# Voce Architecture Overview

This document is a **cross-cutting reference** for agents making changes that span multiple areas. For area-specific detail, see:

- **`src/AGENTS.md`** — Rust backend module map, state machine, concurrency model
- **`src/panel/AGENTS.md`** — IPC contract (PanelCmd, PanelEvent, escaping rules)
- **`assets/panel/AGENTS.md`** — Frontend tech stack, screens, hooks, components
- **`eval/AGENTS.md`** — Evaluation harness, datasets, benchmarks
- **Root `AGENTS.md`** — Project overview, tooling, task management

---

## Data Flow: Microphone → Output

The signal path through Voce:

```mermaid
graph TD
    A["🎤 Microphone Input<br/>(any sample rate)"] --> B["Capture Thread cpal<br/>• Decimate to 16 kHz<br/>• 512-sample chunks<br/>• Push to inference queue"]
    
    B --> C["Inference Task<br/>• Read 16 kHz<br/>• Collect 48k samples 3s<br/>• Run ONNX model<br/>• Cosine similarity<br/>• Emit FilterStats"]
    B --> D["Output Ring Buffer<br/>• Write 16 kHz<br/>• Upsample to 48 kHz<br/>• Output thread drains"]
    
    C --> E["FilterStats AppEvent<br/>similarity, is_passing"] --> F["📊 Panel Update<br/>Real-time filter stats"]
    D --> G["📢 Output Device<br/>macOS CoreAudio / BlackHole"]
    
    style A fill:#e1f5ff
    style B fill:#fff3e0
    style C fill:#f3e5f5
    style D fill:#f3e5f5
    style E fill:#e8f5e9
    style F fill:#e8f5e9
    style G fill:#e1f5ff
```

**Key rates:**
- **Input capture**: 16 kHz (decimated from device rate)
- **Processing chunks**: 512 samples = 32 ms (tick rate)
- **ONNX window**: 48 000 samples = 3 s @ 16 kHz
- **Output ring buffer**: 48 kHz nominal (for upsampling to macOS/BlackHole)

---

## Thread & Runtime Model

Voce uses a hybrid model:

```mermaid
graph TB
    subgraph Main["🔄 Main Thread (winit event loop)"]
        dispatch["AppEvent Dispatch<br/>StateChanged → PanelEvent<br/>PanelCommand → match<br/>FilterStats → Panel"]
    end
    
    subgraph Workers["Worker Threads & Runtimes"]
        capture["🎤 cpal capture thread<br/>Read mic @ 16 kHz<br/>→ 512-sample chunks<br/>→ inference queue"]
        tokio["⚡ tokio runtime<br/>Model downloads<br/>HTTP requests<br/>async spawn tasks"]
        inference["🧠 crossbeam inference task<br/>Read 16 kHz samples<br/>Run ONNX model<br/>Emit FilterStats"]
        output["📢 cpal output thread<br/>Drain ring buffer<br/>@ 48 kHz<br/>→ device"]
    end
    
    Main --> Workers
    capture -->|audio samples| inference
    inference -->|FilterStats| dispatch
    dispatch -->|panelEvent| capture
    dispatch -->|InferenceCmd| inference
    
    style Main fill:#e3f2fd
    style Workers fill:#f5f5f5
    style capture fill:#fff3e0
    style tokio fill:#f3e5f5
    style inference fill:#f3e5f5
    style output fill:#e8f5e9
```

### Synchronization

- **Main → Inference**: `crossbeam::channel::Sender<InferenceCmd>` (blocking send)
- **Inference → Main**: `AppEvent` enum pushed to event loop (non-blocking)
- **Capture ↔ Inference**: Audio samples via crossbeam queue; samples enqueued as chunks arrive
- **Inference ↔ Output**: Ring buffer (circular buffer shared by two threads, no mutex)

### AtomicBool Gate

A shared `AtomicBool` controls immediate filter muting:

```rust
let gate = Arc::new(AtomicBool::new(false));
let gate_clone = gate.clone();

// Inference task
while gate_clone.load(Ordering::Relaxed) {
    // Read from audio queue, process, write to ring buffer
}

// Main thread (on PanelCmd::ToggleFilter)
gate.store(!gate.load(...), Ordering::Relaxed);
```

This allows sub-32ms muting without stopping the task or ring buffer drain.

---

## IPC Bridge (Rust ↔ JavaScript)

Two-way message passing via webview:

```mermaid
sequenceDiagram
    participant rust as Rust Main
    participant js as JS Panel
    participant ipc as IPC Server
    
    rect rgb(220, 240, 255)
    Note over rust: Emit Event Path
    rust->>rust: AppEvent emitted
    rust->>rust: PanelEvent::from(AppEvent)
    rust->>rust: Serialize to JSON
    rust->>rust: Escape backslashes, then quotes
    rust->>js: webview.evaluate_script()<br/>window.__voce_update("JSON")
    js->>js: JSON.parse(jsonStr)
    js->>js: useAppState handlers
    js->>js: Update Solid signals
    js->>js: Reactive UI re-render
    end
    
    rect rgb(255, 240, 220)
    Note over js: User Action Path
    js->>js: User clicks button
    js->>js: useIpc().startRecording(1)
    js->>ipc: window.ipc.send('start_recording', {index: 1})
    ipc->>ipc: parse_cmd(&request.body)
    ipc->>ipc: PanelCmd::StartRecording{index}
    ipc->>ipc: Wrap in AppEvent::PanelCommand()
    ipc->>rust: tx.send(AppEvent)
    rust->>rust: Match AppEvent in main loop
    rust->>rust: emit InferenceCmd::StartEnrollment{index}
    end
```

See `src/panel/AGENTS.md` for PanelCmd/PanelEvent tables and escaping rules.

---

## Build Pipeline

### Rust → Binary

```mermaid
graph LR
    A["src/*.rs<br/>+ include_str!()"] --> B["just build-panel"]
    B --> C["assets/panel/<br/>dist/index.html<br/>Vite single-file"]
    C --> D["Committed to git"]
    C --> E["just build-rust"]
    E --> F["voce binary<br/>Embedded HTML<br/>macOS .app"]
    F --> G["Webview displays HTML<br/>IPC bridge active"]
    
    style A fill:#e8f5e9
    style C fill:#fff9c4
    style D fill:#fff9c4
    style F fill:#c8e6c9
    style G fill:#a5d6a7
```

Steps:
1. `just build-panel` → `assets/panel/dist/index.html` (Vite single-file output)
2. `just build-rust` → Compiles Rust, embeds HTML via `include_str!()`
3. HTML is **committed to git** (dist/index.html in repo)

### Audio Driver → Bundle

```mermaid
graph LR
    A["audio-driver/<br/>VoceAudio.c<br/>Info.plist"] --> B["just driver::build"]
    B --> C["audio-driver/<br/>VoceAudio.driver/"]
    C --> D["target/release/<br/>VoceAudio.driver/<br/>on first launch"]
    
    style A fill:#ffe0b2
    style C fill:#ffe0b2
    style D fill:#ffcc80
```

### Release Bundle

```mermaid
graph TD
    A["cargo build --release"] --> B["target/release/voce<br/>Rust binary + panel"]
    A --> C["target/release/<br/>VoceAudio.driver/"]
    B --> D["scripts/release.sh"]
    C --> D
    D --> E["voce.app/Contents/MacOS/voce"]
    D --> F["voce.app/Audio/VoceAudio.driver/"]
    E --> G["codesign --deep"]
    F --> G
    G --> H["voce-v*.zip<br/>Release artifact"]
    
    style A fill:#e3f2fd
    style B fill:#c8e6c9
    style C fill:#c8e6c9
    style G fill:#fff9c4
    style H fill:#ffccbc
```

---

## Runtime Data Files

User configuration and models live in `~/.voce/`:

```
~/.voce/
├── config.json                 # User threshold + vote window
│                               # {"threshold": 0.75, "vote_window": 3}
├── enrolled_embedding.json     # Speaker embedding (if enrolled)
│                               # {"embedding": [f32, ..., f32]} (256 floats)
└── models/
    ├── wespeaker-...onnx       # Speaker embedding model (downloaded)
    ├── vad-...onnx             # Voice activity detector (downloaded)
    └── ...
```

Models are downloaded on first run and cached. Embeddings are extracted and saved post-enrollment.

---

## Key Rate Boundaries & Reasoning

| Boundary | Value | Used For | Reasoning |
|----------|-------|----------|-----------|
| **Mic input** | Any | Capture | Device-dependent; decimated to 16 kHz |
| **Capture output** | 16 kHz | Processing, inference | ONNX model trained on 16 kHz speech |
| **Processing chunk** | 512 samples (32 ms) | Tick rate, state checks | Balance: low latency vs. processing overhead |
| **ONNX window** | 48 000 samples (3 s) @ 16 kHz | Embedding extraction | Model trained on 3 s windows; higher context for speaker identification |
| **Ring buffer nominal** | 48 kHz | Output device | Standard macOS/CoreAudio rate; upsamples 16 kHz filtered audio |
| **Output chunk** | Varies (cpal-dependent) | macOS I/O | CoreAudio callback size; 512–4096 samples typical |

**Why 48 kHz output when input is 16 kHz?**

The ring buffer uses 48 kHz as a nominal rate because:
1. macOS CoreAudio primarily works at 48 kHz (or 44.1 kHz on some devices)
2. BlackHole (virtual mic) expects 48 kHz
3. The filtered 16 kHz audio is resampled to 48 kHz before output to avoid aliasing artifacts
4. The 16 kHz inference pipeline is decoupled from the output sample rate via the ring buffer

---

## State Machine (Quick Reference)

For detailed state machine diagram, see `src/AGENTS.md`.

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> ModelLoading: download models
    ModelLoading --> OnboardingReady: models ready
    
    OnboardingReady --> Recording1: start enrollment 1
    Recording1 --> Recording1: insufficient speech
    Recording1 --> Recording2: done with 1
    Recording2 --> Recording2: insufficient speech
    Recording2 --> Adapting: 2 valid recordings
    Adapting --> TestReady: profile ready
    
    TestReady --> Testing: StartTest
    TestReady --> ActiveStandby: StartFilter
    
    Testing --> PlayingBackFiltered: finish recording
    Testing --> PlayingBackRaw: finish recording
    PlayingBackFiltered --> TestReady: StopPlayback
    PlayingBackRaw --> TestReady: StopPlayback
    
    ActiveStandby --> Filtering: mic active + speech
    Filtering --> ActiveStandby: no speech
    
    ActiveStandby --> [*]
    Filtering --> [*]
    
    note right of Recording1
        Enrollment Path
    end note
    
    note right of Testing
        Test Mode Path
    end note
    
    note right of Filtering
        Production Path
    end note
```

---

## Contact Surface Checklist

When making changes, check each area's invariants:

- **Audio rates** (16 kHz, 48 kHz)? → Update `src/audio/`, docs, eval
- **State transitions**? → Update `src/app_state.rs`, panel screens
- **New IPC message**? → Sync `src/panel/ipc.rs`, JS hooks, UI components
- **New eval dataset**? → Add to `eval/scripts/`, update `eval/justfile`, sync `eval/AGENTS.md`
- **Frontend component** → Add screen type, routing, signals, styles
- **Audio driver** → Test with `just driver::build`, verify install

See individual AGENTS.md files for step-by-step checklists.
