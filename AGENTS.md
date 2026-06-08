# Voce Agent Harness

## Project Overview

Voce is a speaker-gate filter that continuously monitors microphone input, enrolls a user's voice, and suppresses audio from non-target speakers in real-time. The system comprises:

| Area | Purpose | Key Files |
|------|---------|-----------|
| `src/` | Rust backend (audio capture, inference, IPC server) | `main.rs`, `audio/`, `ipc.rs`, `events.rs` |
| `src/panel/` | IPC bridge (Rust↔JS command/event marshalling) | `ipc.rs`, module tree |
| `assets/panel/` | SolidJS frontend (12 screens, ArkUI + Tailwind) | `src/`, `components/`, `hooks/`, `dist/index.html` |
| `audio-driver/` | macOS HAL plugin (virtual mic for pass-through output) | `VoceAudio.c`, `Info.plist` |
| `eval/` | Python evaluation harness (LibriSpeech, AliMeeting, MSC benchmarks) | `scripts/`, `fixtures/`, `pyproject.toml` |
| `docs/` | Architecture, roadmap, design decisions | `prd.md`, `architecture.md` |
| `.agents/` | Task tracking, backlog policy, implementation plans | `plans/`, `backlog-policy.md` |
| `scripts/` | Release automation, validation utilities | `release.sh`, `validate-ac.sh` |

## Sub-Area Agent Guides

Each area has its own `AGENTS.md` with area-specific conventions and critical paths:

- **`src/AGENTS.md`** — Rust backend: module map, state machine, concurrency model, IPC contract
- **`src/panel/AGENTS.md`** — IPC marshalling: command/event tables, `to_js_call()` escaping rules, sync invariants
- **`eval/AGENTS.md`** — Evaluation: pass criteria, fixture structure, threshold sweeps, adding datasets
- **`assets/panel/AGENTS.md`** — Frontend: tech stack, screen→component map, `useIpc`/`useAppState` API

Links: [[src/AGENTS.md|src]] | [[src/panel/AGENTS.md|src/panel]] | [[eval/AGENTS.md|eval]] | [[assets/panel/AGENTS.md|assets/panel]]

---

## Audio Architecture

All audio throughout the signal chain is **16 000 Hz mono f32**, except where explicitly noted.

- **Capture** (`src/audio/capture.rs`): decimates any device sample rate → 16 kHz.
- **Processing chunks**: 512 samples = 32 ms (tick rate for state transitions).
- **Inference windows**: 48 000 samples = 3 s (ONNX model window).
- **Output ring buffer**: 48 000 Hz nominal rate (cpal heartbeat only; actual audio goes to ring buffer, not cpal output).
  - **Why 48 kHz?** Ring buffer lives in a separate memory arena from the 16 kHz inference pipeline; 48 kHz avoids aliasing when upsampling the 16 kHz filtered signal back to 48 kHz for macOS and BlackHole.
- **BlackHole fallback**: 16 kHz (matches mic data rate for simplicity; user can upsample externally if needed).

**Key invariant:** Do not introduce any other sample rate in the processing pipeline. Changes to any of these rates ripple through capture, crossbeam queues, ring buffer layout, and output drivers.

---

## Tooling

All build, test, and eval commands are defined in justfiles. Rust toolchain is managed by `rustup`, not `mise`.

### Commands

| Command | Purpose |
|---------|---------|
| `just build-panel` | Build SolidJS frontend (Vite → `assets/panel/dist/index.html`) |
| `just build-rust` | Build Rust debug binary |
| `just build-rust-release` | Build release binary with optimizations |
| `just build-icons` | Regenerate `.icns` file from `assets/panel/src/assets/logo-on.svg` |
| `just dev-app` | Run debug binary in `voce.app` bundle (ad-hoc signed, opens) |
| `just install-local` | Install release binary to `~/Applications/Voce.app` |
| `just build-all` | Alias for `build-panel build-rust-release` |
| `just release [FLAGS]` | Run `scripts/release.sh` for distribution builds |
| `just eval::setup` | Download samples, build fixtures, compile release binary |
| `just eval::run` | Run evaluation (uses threshold from `~/.voce/config.json`) |
| `just eval::threshold` | Sweep threshold 0.60→0.90, restore config |
| `just eval::alimeeting-run` | Run AliMeeting audio evaluation |
| `just eval::alimeeting-sweep` | Sweep threshold for AliMeeting |
| `just driver::build` | Build `VoceAudio.driver` (universal arm64e + x86_64) |
| `just driver::install` | Install driver to `~/Library/Audio/Plug-Ins/HAL/` |
| `just driver::reload` | Install + restart coreaudiod |

### Tool Versions

Managed via `mise.toml` at the repo root:

```toml
[tools]
python = "3.12"
pnpm = "11"
just = "1"
# Rust is managed by rustup, not mise
```

Verify with:
```bash
mise install && mise current
```

### Python Environment

Eval uses `uv` with a `pyproject.toml`-based environment. Never activate `.venv` manually.

```bash
# From repo root:
uv run --project eval python eval/scripts/evaluate.py
```

### Panel Build

Frontend uses `pnpm` for package management (see `packageManager` in `package.json`).

```bash
cd assets/panel && pnpm install && pnpm exec vite build
```

---

## Config Files

Runtime configuration lives in `~/.voce/`:

| File | Purpose | Format |
|------|---------|--------|
| `config.json` | Threshold (0.0–1.0), vote window (frames) | JSON: `{"threshold":0.75,"vote_window":3}` |
| `enrolled_embedding.json` | Speaker embedding from enrollment (if enrolled) | JSON: `{"embedding": [float, ...]}` |
| `models/` | ONNX model cache (downloaded on first run) | Binary ONNX files |

If `config.json` is missing, the app uses `{"threshold": 0.75, "vote_window": 3}`.

---

## Task Management

All work is tracked in **GitHub Project #12** (voce): https://github.com/users/espetro/projects/12/views/1

### Refinement Criteria

A task is **refined** (ready to start) when it has:
- **Iteration/Quarter** set (maps to `.agents/backlog-policy.md`)
- **Effort** estimate (S/M/L/XL) including testing and contact-surface risk
- **Start date + Target date** (scheduled in roadmap)
- **Classification** label (`feature`/`bug`/`cosmetic`/`infra`)

See `.agents/backlog-policy.md` for the full policy.

### Project Management Workflow

1. Refine tasks in GitHub Project #12 before starting.
2. Link implementation plans to tasks (e.g., `.agents/plans/2026-06-07-agent-harness.md` → GitHub Issue #10).
3. Use `ghx` CLI to manage the project:
   ```bash
   ghx issue create --title "..." --label infra
   ghx issue status 123 --status "WIP"
   ```

---

## Script Policy

Two scripts remain as shell (`.sh`) due to heavy subprocess use:

| Script | Purpose | How to Run |
|--------|---------|-----------|
| `scripts/release.sh` | Build & codesign release `.app` bundle | `just release [FLAGS]` |
| `eval/validate-ac.sh` | Validate speaker-gate with user recordings | `just eval::validate-ac ENROLL1 OTHER [ENROLL2]` |

Both are POSIX-compatible (no bash-isms) for portability.

---

## Key Invariants

- **dist/ committed**: SolidJS build output (`assets/panel/dist/index.html`) is committed. The Rust binary embeds it via `include_str!("../assets/panel/dist/index.html")` at build time.
- **Panel↔IPC sync**: Changes to `PanelCmd`/`PanelEvent` enums in `src/panel/ipc.rs` must update `ipc.rs` *and* `assets/panel/src/hooks/useIpc.ts` and `useAppState.ts` (see `src/panel/AGENTS.md`).
- **Ring buffer layout**: Output ring buffer is 3 s @ 48 kHz = 144k samples. Changes to sample rate or duration require updates to `src/audio/output.rs` and Rust app tests.
- **ONNX model window**: 48 000 samples = 3 s @ 16 kHz. The model expects exactly this window; changes require retraining.

---

## Quick Links

- **GitHub Project**: https://github.com/users/espetro/projects/12/views/1
- **Backlog Policy**: [`.agents/backlog-policy.md`](.agents/backlog-policy.md)
- **PRD / Roadmap**: [`docs/prd.md`](docs/prd.md)
- **Architecture Guide**: [`docs/architecture.md`](docs/architecture.md) (cross-cutting reference)
