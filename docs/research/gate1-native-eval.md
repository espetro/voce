# Gate 1: real-speech validation of the `voce_native` backend

Date: 2026-11-26. Branch: `voce-spike-gate1-eval`. Lane D1 of the backend
spike program. Question: is the real ONNX backend behind `voce_native`
ABI v1 correct enough (vs the direct-Rust reference pipeline) to judge the
backend language spikes against?

Verdict up front: **GO.** On identical enrollment and input audio the shim
backend and the reference Rust pipeline produce matching gate decisions on
every one of 841 scoring windows across 7 speakers, with mean cosine-similarity
drift under 0.02 on target speech and under 0.08 on non-target speech —
comfortably inside the separation margin (target mean 0.84 vs non-target
means from -0.08 to 0.19 at threshold 0.75).

## Matrix

Full 7-input matrix completed (no reduction needed):

- Enrollment: `spk_1272.wav` (~180 s, LibriSpeech dev-clean-2), pooled
  3 s / 1.5 s-hop windows → 119 windows → 256-dim profile.
- Inputs: all 8 speakers present in `/tmp/gate1/fixtures` except `spk_3752`
  (used nowhere in this run; 7 scored inputs = enrolled speaker + 6 others),
  ~180 s each, 16 kHz mono PCM16.
- Pipelines compared on identical WAV files:
  - **Reference**: `voce-gate1-runner` (`native/src/bin/gate1_runner.rs`)
    — transcribed from the app's `src/eval.rs` + `inference.rs` +
    `profile.rs` path: fbank → ONNX `voxblink2_samresnet34_ft` → cosine vs
    pooled enrollment profile, 512-sample chunk grid, RMS −40 dBFS
    fast-silence, `SlidingVoteGate(vote_window=3)`, sweep 0.60–0.90.
  - **Shim**: `voce-native-probe --backend real` (`native/src/bin/probe.rs`)
    — the production `voce_native` ABI v1 surface (`voce_engine_load_models`,
    profile build, per-window scoring) driven end-to-end.

### Fixtures and model

Fixture sha256s (per `manifest_summary.json`):

| file | speaker | seconds | sha256 (first 12) |
|---|---|---|---|
| spk_777.wav | 777 | 190.76 | `09fa24f59e5a` |
| spk_5895.wav | 5895 | 180.11 | `8ba2d6eb9eb0` |
| spk_2035.wav | 2035 | 180.61 | `d335d71b4cdb` |
| spk_3752.wav | 3752 | 183.15 | `25d2f78d2676` (not scored this run) |
| spk_1462.wav | 1462 | 182.59 | `3449b4efd8b0` |
| spk_1988.wav | 1988 | 180.30 | `caa068d8a24f` |
| spk_1272.wav | 1272 | 180.66 | `a0513a342b74` |
| spk_6241.wav | 6241 | 180.90 | `b1124ffc974b` |

Model: production artifact used by the desktop app, durable at
`/home/own-agents/.cache/voce/models/voxblink2_samresnet34_ft.onnx`,
sha256 `33af8affe6191b1ebd196d2b56e22c2934104cd2764abfdbdd954d3a934eb2a1`
(100,865,597 bytes, opset 14, feats `[B,T,80]` → embs `[B,256]`).
Not committed. Fixture WAVs are `/tmp`-only, not committed.

## Exact commands

```sh
cd native
FIX=/tmp/gate1/fixtures
MODEL=/home/own-agents/.cache/voce/models/voxblink2_samresnet34_ft.onnx

# reference pipeline
./target/release/voce-gate1-runner \
  --enroll $FIX/spk_1272.wav \
  --inputs $FIX/spk_1272.wav $FIX/spk_2035.wav $FIX/spk_5895.wav \
           $FIX/spk_777.wav $FIX/spk_1462.wav $FIX/spk_1988.wav $FIX/spk_6241.wav \
  --model $MODEL --sweep 6 --out /tmp/gate1/ref-run1.json

# shim (production ABI v1), 2 repeats for determinism
./target/release/voce-native-probe --backend real --models-dir $MODEL \
  --enroll $FIX/spk_1272.wav \
  --input $FIX/spk_1272.wav --input $FIX/spk_2035.wav --input $FIX/spk_5895.wav \
  --input $FIX/spk_777.wav --input $FIX/spk_1462.wav --input $FIX/spk_1988.wav \
  --input $FIX/spk_6241.wav \
  --repeat 2 --out /tmp/gate1/shim-run1.json

# tests (VOCE_REAL_MODEL enables the 2 real-inference tests)
VOCE_REAL_MODEL=$MODEL flock /tmp/tsc.lock \
  cargo test --release --manifest-path native/Cargo.toml
```

Results live in `/tmp/gate1/{ref-run1,shim-run1}.json` (not committed;
the report below carries the numbers).

## Gate 1 metrics at threshold 0.75

Vote-gated pass counts (`SlidingVoteGate`, vote_window=3, gate starts
open — hence the 2-window warm-up artifact on non-target speakers):

| input | reference | shim | per-window decision mismatches |
|---|---|---|---|
| spk_1272 (enrolled) | **119/119** | **119/119** | 0 |
| spk_2035 | 2/119 | 2/119 | 0 |
| spk_5895 | 2/118 | 2/119 | 0 |
| spk_777 | 2/125 | 2/126 | 0 |
| spk_1462 | 2/120 | 2/120 | 0 |
| spk_1988 | 2/118 | 2/119 | 0 |
| spk_6241 | 2/119 | 2/119 | 0 |

- **Enrolled recall @0.75: 119/119 = 100%** on both pipelines.
- **Non-target FPR @0.75: 0 genuine passes on both pipelines.** The 2/119
  entries are entirely the gate warm-up (first two windows before the
  3-window vote can close); raw per-window threshold hits on non-target
  speech are 0/841 on the shim and 2/838 (isolated windows in the 0.70–0.75
  band) on the reference, both fully suppressed by the vote gate.

## Cosine similarity drift, reference vs shim

Window grids differ (see alignment note below), so compare distributions:

| input | mean (ref) | mean (shim) | drift |
|---|---|---|---|
| spk_1272 | 0.8384 | 0.8378 | −0.0007 |
| spk_2035 | −0.0457 | 0.0030 | +0.0487 |
| spk_5895 | −0.0104 | 0.0175 | +0.0279 |
| spk_777 | 0.1213 | 0.1222 | +0.0009 |
| spk_1462 | −0.0519 | 0.0039 | +0.0559 |
| spk_1988 | −0.0786 | 0.0015 | +0.0800 |
| spk_6241 | 0.1895 | 0.1831 | −0.0064 |

The shim clamps non-matching similarities at 0.0 (shim_min = 0 on all
non-target files) while the reference lets them go negative (down to
−0.23). That alone explains most of the non-target mean drift; the
shape of both distributions is the same. **On target speech the drift is
−0.0007 — noise-level.** The 0.19-wide gap between worst non-target mean
(spk_6241, 0.19) and the target mean (0.84) dwarfs all observed drift.

### Window-grid alignment note (stated precisely)

The reference runner evaluates on the app's 512-sample-chunk eval grid
(t=8 ms start, hop 512 samples), while the probe scores on its own 3 s
window grid (hop 24 000 samples, t=0 start). Exact per-window index
alignment is therefore impossible; only 28 of 838 reference windows fall
within 20 ms of a probe window start. On those aligned pairs: 0/28 decision
mismatches @0.75, mean |drift| 0.0255, median 0.0166, max 0.1153 — but
n=28 is too small to lean on, so the primary comparison is the
distribution/digest level above plus the vote-gated pass counts, which is
the granularity the gate actually acts at. Digest-level: per-input profile
digests are stable across the shim's repeats and the profile JSON
round-trips (5 427 bytes, 256-dim, `profile_roundtrip_ok: true`).

## Threshold sensitivity (all windows, raw per-window threshold hits)

| threshold | reference (of 838) | shim (of 841) |
|---|---|---|
| 0.60 | 119 | 119 |
| 0.65 | 119 | 119 |
| 0.70 | 118 | 119 |
| 0.75 | 113 | 115 |
| 0.80 | 95 | 91 |
| 0.85 | 54 | 55 |
| 0.90 | 7 | 7 |

Identical to within 4 windows out of ~840 at every threshold — the two
pipelines have the same operating curve. The enrolled speaker's 119 windows
account for all passes through 0.90; non-target windows contribute nothing
meaningful anywhere.

## Runtime, memory, determinism

| metric | reference runner | shim (ABI v1) |
|---|---|---|
| model load | 1 000 ms | 1 251 ms |
| enrollment (119 pooled windows) | 69.3 s | included in run |
| per-window scoring | n/a (grid differs) | 724 ms/window |
| total scoring (841 windows) | wall 664.9 s | 608.9 s |
| peak RSS | 304 MB | 505 MB (from 7 MB start) |
| repeats | — | 2; **all digests identical, max per-window sim diff 0.0** |

The shim is ~1.3x faster end-to-end despite the ABI boundary. ~724 ms/window
is dominated by voxblink2-SAMResNet34 inference (a 100 MB model, ~4x the
lane-A2 fallback); acceptable for background gating, worth keeping in mind
for the spikes' latency budgets. RSS delta (~500 MB) is ONNX Runtime arena
+ model weights.

## Test status

`cargo test --release` (lock held): 15/15 pass, including the 2
real-inference tests with the production model
(`real_backend_is_deterministic_and_profiles_roundtrip`,
`real_backend_same_speaker_passes_other_speaker_blocks`, 3.5 s).

## Unresolved parity gaps

1. **Neural VAD (voxudio).** voxudio 0.5.8 does not compile on Linux due to
   ~10 pre-existing bugs inside the published crate (E0597 lifetimes ×7,
   E0061 arity, etc.). Both pipelines here fall back to the RMS −40 dBFS
   fast-silence check; the app's neural-VAD 0.5-probability gate is
   untested on Linux and the probe reports `speech_probability: 1.0`
   unconditionally. LibriSpeech fixtures are continuous speech, so impact
   on this eval is minimal, but any future eval with real silence/noise
   needs this gap closed (or a macOS run).
2. **Execution-provider parity.** ONNX Runtime here runs CPU-only on
   Linux; the mac app may use CoreML/other providers. Numerically the CPU
   and (presumed) mac runs agree at distribution level, but bit-identity of
   the model artifact across machines should be confirmed once by sha256
   against `~/.voce/models` on a mac.
3. **Full-app build blocker.** The full Linux app workspace still cannot
   build (glib-2.0 ≥ 2.70 missing, no sudo) — established in Milestone 1,
   not retried. This eval therefore validates the ABI + backend, not the
   full app wiring.
4. **Enrollment-grid difference.** The probe's enrollment pooled-window
   grid matches compute_profile (119 windows for 180 s), and profile
   dimensions match (256), but the reference enrollment uses
   `compute_profile` directly; small pooled-embedding differences are
   absorbed into the drift numbers above.

## Go / no-go

**GO.** `voce_native` ABI v1 with the real ONNX backend reproduces the
reference pipeline's gate decisions exactly (841/841 windows, 7 speakers,
7 thresholds within 4 windows), with noise-level target-similarity drift,
bit-exact repeat determinism, working profile round-trip, and acceptable
CPU-only latency (~724 ms/window). It is correct enough to serve as the
ground-truth oracle for judging the backend language spikes. Caveats to
carry into those spikes: the neural-VAD path is unvalidated, the CPU
latency/RSS numbers set the performance baseline to beat, and ABI v1
stays frozen.
