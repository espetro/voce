# Voce backend spike plan

Date: 2026-11-25
Status: approved for shim-first spike execution

## Goal

Extract Voce's inference/backend behavior behind a stable PCM-first contract, prove a reusable native inference shim, then compare scriptc TypeScript and pyscriptc Python backend implementations without rewriting the working realtime shell.

Desktop remains first priority, server second, mobile third. GUI, PyObjC, microphone capture, virtual-device ownership, and tray behavior are outside the backend contract.

## Architecture contract

The backend consumes and produces 16 kHz mono f32 PCM supplied by the host. Control and metadata may use JSON or typed native values. The semantic API is:

```text
create engine
load models
reset enrollment
add enrollment PCM
finish enrollment
score PCM window against profile
process PCM chunk or file
serialize/deserialize profile state
close engine
```

The production target is layered:

```text
desktop shell -> realtime native core -> backend engine -> native inference shim
```

The first integration uses process boundaries. Library embedding is an optimization after fixture parity is proven.

## Workstreams

### 0. Baseline extraction

Deliverables:

- stable backend contract
- fixture-driven acceptance harness
- current Rust eval baseline
- comparable output format for every spike

### 1. voce_native shim

Build a narrow C ABI over the inference stack. Preserve the current WeSpeaker/fbank/VAD behavior first. Evaluate sherpa-onnx as the packaging/runtime layer when it can either load the current model or match/exceed baseline behavior with a supported alternative.

Deliverables:

- `voce_native.h` with explicit ownership rules
- Linux implementation and CLI probe
- fixture runner
- latency, memory, and determinism measurements
- notes on whether sherpa-onnx can preserve the current model behavior

### 2. scriptc backend spike

Implement enrollment, voting gate, orchestration, and batch processing in typed TypeScript. Consume `voce_native` through direct FFI in executable mode. Separately assess whether library-mode host callbacks can support embedded use without FFI-in-library changes.

Deliverables:

- scriptc executable and FFI manifest
- fixture runner matching the baseline output format
- compile/iteration/artifact/RSS measurements
- exact compiler or runtime fences

### 3. pyscriptc backend spike

Implement the same behavior in typed pyscriptc-compatible Python. Consume the same `voce_native` shim through extern FFI. No GUI, PyObjC, sounddevice, NumPy, scipy, or PyTorch.

Deliverables:

- pyscriptc executable and extern FFI manifest
- fixture runner matching the baseline output format
- compile/iteration/artifact/RSS measurements
- exact subset/compiler failures and upstream issue candidates

### 4. Model and DSP research lane

Assess reusable architecture and model directions. ClearMic Studio contributes the hybrid shell/inference-daemon shape, exact PCM transport, and latency measurement discipline, but not its host-Python distribution assumptions. Krisp is proprietary and contributes product lessons only, not code or models.

Evaluate artifacts, not claims. Candidates include Silero/Ten VAD, current WeSpeaker embeddings, sherpa-supported embedding alternatives, RNNoise, DeepFilterNet-family suppression, GTCRN-like low-latency suppression, and WeSep or VoiceFilter/SpEx+-style target speaker extraction. General separation models such as TF-GridNet/SepFormer are references unless an extraction-conditioned artifact is practical.

Deliverables:

- license/export/runtime matrix
- model size and CPU cost assessment
- eval protocol comparing similarity gate, suppression plus gate, and explicit target extraction
- recommendation only where artifacts and measurements support it

### 5. Electrobun shell spike

Deferred until the backend contract is frozen. Test TypeScript main-process shell ownership of tray, panel, updater, and sidecar supervision while keeping realtime audio in native processes. Electrobun is not allowed to own audio callbacks or model inference.

## Decision gates

### Gate 1: shim correctness

Compare fixture outputs against current Rust implementation. Record embedding drift, VAD probability difference, gate decision parity, runtime per window, RSS, startup time, and repeated-run determinism.

### Gate 2: backend comparison

Compare scriptc and pyscriptc only against the same shim and fixtures. Measure compile time, iteration time, artifact size, memory, throughput, eval parity, FFI complexity, failure modes, and server packaging difficulty.

### Gate 3: model direction

Default remains the current similarity gate. Move toward target-speaker extraction only if quality improves materially without unacceptable CPU, latency, size, or licensing cost.

### Gate 4: shell direction

Choose among existing Rust shell, Electrobun shell, or deferral only after backend fixture parity.

## Delegation model

Use T3 for steerable spike supervision and coordinate for durable queued execution on the VPS. Each job gets a scoped goal, a worktree under `$HOME/.worktrees/voce-backend-spikes/<lane>`, a monitor file, and a time box. Jobs must commit at least every 20 minutes and write `/tmp/<lane>.md` before exiting.

Initial lanes are shim-first:

1. native shim implementation
2. model research matrix
3. search-setup audit for devin/opencode/agy

scriptc and pyscriptc lanes start after the shim ABI and fixture contract land, unless they are explicitly limited to fixture harness/mock preparation.
