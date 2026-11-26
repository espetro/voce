# Gate 3: Model-direction evaluation (suppression + gate vs similarity gate)

**Date:** 2026-11-26 · **Branch:** `voce-spike-gate3-model` · **Lane:** G3

## Decision: STAY WITH SIMILARITY GATE (Arm A)

Arm B (GTCRN suppression + gate) **fails** acceptance. Arm C (target-speaker
extraction) is rejected on artifact/runtime constraints today. Arm A remains the
default; the similarity gate keeps clean-speech parity (Gate 1: 841/841) and
shows no interference/overlap condition where a suppression front-end would
clear the required 30% relative FPR reduction at ≤2% absolute recall loss.

## Arm B acceptance check (threshold 0.75, vote_window 3, same grid both arms)

Acceptance requires: FPR ↓ ≥30% relative, recall loss ≤2% absolute on
interference/overlap tiers, added-stage RTF ≤0.3, added latency ≤40 ms.

| Criterion | Required | Measured | Pass |
|---|---|---|---|
| FPR reduction (interference/overlap) | ≥30% rel | 0% (frame FPR identical: 14.5%→14.5% overlap tier, 0% noise tier both arms) | ✗ |
| Target recall loss | ≤2% abs | −26.3% abs on babble10 (47.4%→36.8%... gate pass 18→14/38), −28.9% abs on noise10 (13→2/38 pass) | ✗ |
| Added-stage RTF | ≤0.3 | 0.237 aggregate (0.196–0.276 per file, 1 thread) | ✓ |
| Added latency | ≤40 ms | n/a in offline protocol; 0.24 RTF ≈ +77 ms per 32 ms chunk if chunked → exceeds budget | ✗ (marginal) |

**Verdict: Arm B fails.** GTCRN degraded target similarity under every noisy
condition (mean similarity: babble10 0.668→0.615, noise10 0.646→0.607,
noise5 0.557→0.450, overlap +0.071 but still far below threshold) and provided
zero FPR improvement on non-target tiers.

### Full matrix (38 windows/file, 60 s each; reduced matrix, precisely labeled)

Enrollment: Gate 2 clean `enroll_1272.wav` (Arm B: its GTCRN-enhanced copy).
Similarity = cosine vs pooled profile; decision = frame pass ≥0.75 AND vote
gate (window 3). Arm A = raw fixtures; Arm B = all fixtures through GTCRN first.

| Fixture (tier) | Arm A pass | Arm B pass | Arm A meanSim | Arm B meanSim |
|---|---|---|---|---|
| target_1272 (clean target) | 37/38 (97.4%) | 36/38 (94.7%) | 0.781 | 0.771 |
| other_2035 / 5895 / 777 (clean non-target) | 2/38 each (5.3%) | 2/38 each (5.3%) | ~0.0–0.13 | ~−0.06–0.15 |
| target_noise10 | 13/38 (34.2%) | 2/38 (5.3%) | 0.646 | 0.607 |
| target_noise5 | 2/38 (5.3%) | 2/38 (5.3%) | 0.557 | 0.450 |
| target_babble10 | 18/38 (47.4%) | 14/38 (36.8%) | 0.668 | 0.615 |
| overlap target+other2035 @0 dB | 8/38 (21.1%) | 8/38 (21.1%) | 0.357 | 0.428 |
| overlap target+other2035, other 2× | 2/38 (5.3%) | 2/38 (5.3%) | 0.110 | 0.122 |

Notes:
- Enrolled recall on clean target falls 37→36/38 (−2.6% abs, already over the
  2% budget before any noise tier).
- Gate-pass on non-targets is the vote gate's tail behavior (identical counts
  across arms); frame-level FPR is 0% on all clean non-targets in both arms.
- Decision flips Arm A→B per file: target clean 3, babble10 4, noise10 5,
  overlap 3 — all degradations.
- RSS ~286–305 MB (embedder dominated, both arms similar). Embedding runtime
  ~0.00005–0.0001 ms/window on this host (not a bottleneck; GTCRN is).

### Determinism
Two identical Arm A clean runs produced byte-identical similarity vectors
(6-decimal rounded comparison): **deterministic ✓**.

## Deliverable 1: GTCRN artifact

| Item | Value |
|---|---|
| Model | `gtcrn_simple.onnx` (sherpa-onnx re-export of Xiaobin-Rong/gtcrn `stream/onnx_models/gtcrn_simple.onnx`; the upstream ONNX lacks the `model_type` metadata sherpa requires) |
| Source | https://github.com/k2-fsa/sherpa-onnx/releases/tag/speech-enhancement-models |
| License | MIT (GTCRN, © 2024 Rong Xiaobin; sherpa-onnx Apache-2.0) |
| Size | 523 100 B (0.5 MB) |
| sha256 | `e77603ac0c23dac3227dd2d7135b3a585cbee2679048aecfa886657d3ae1b534` |
| Runtime | sherpa-onnx 2.0.3 Python `OfflineSpeechDenoiser` (production path would be the sherpa-onnx online denoiser C API) |
| Note | NOT committed to the repo (downloaded artifact); files in `/tmp/gate3/` |

Exact commands (see also `/tmp/gate3/synth.py`, `/tmp/gate3/enhance.py`):

```bash
curl -sL -o gtcrn_simple_sherpa.onnx \
  https://github.com/k2-fsa/sherpa-onnx/releases/download/speech-enhancement-models/gtcrn_simple.onnx
pip install sherpa-onnx==2.0.3 numpy
python3 synth.py   # synthesize noisy/overlap mixes into /tmp/gate3/noisy
python3 enhance.py --model gtcrn_simple_sherpa.onnx --out-dir enhanced/noisy --inputs noisy/*.wav
python3 enhance.py --model gtcrn_simple_sherpa.onnx --out-dir enhanced/clean \
  --inputs /home/own-agents/.cache/voce/gate2-fixtures/*.wav
./native/target/release/voce-gate1-runner --enroll <enroll> --inputs <set> \
  --model ~/.cache/voce/models/voxblink2_samresnet34_ft.onnx --out report.json
```

**Enhancement throughput:** aggregate RTF **0.237** (clean) / **0.241** (noisy),
60 s of audio in ~14.3 s wall, single thread. Chunked at the app's 32 ms tick
this implies ~7.7 ms compute per chunk → ~+38% CPU per chunk and, with the
offline denoiser's internal buffering, added latency likely **>40 ms** — fails
the latency budget without a streaming/chunk-aligned integration (unmeasured;
sherpa's *online* denoiser was not benchmarked in this pass).

## Deliverable 3: Arm C audit

Requirements: overlap FPR ↓≥50% rel, recall loss ≤2%, total RTF ≤0.5,
added model ≤50 MB, Apache/MIT license, viable runtime path.

| Constraint | Status |
|---|---|
| Artifact | ✗ No production-ready target-speaker-extraction ONNX artifact exists (per Gate model-research: WeSep artifacts not production-ready; VoiceFilter-Lite is a Python-only third-party reimplementation) |
| Runtime path | ✗ No sherpa-onnx (or equivalent C-API) extraction runtime available |
| License | Licensing of viable research artifacts is unclear/✗ |
| RTF / size | Unmeasurable without an artifact |

Arm C **fails the hard artifact/runtime/license constraints today**; no smoke
test was run (it could not be run without an artifact). Direction change is not
supported.

## Unmeasured / caveats

- Sherpa **online** (streaming) GTCRN RTF and chunk-aligned latency — the
  Arm B latency criterion was estimated, not measured end-to-end.
- Only 1 enrolled speaker and 3 non-targets; synthetic stationary/babble/overlap
  noise, not real-device recordings. No VAD on Linux (Gate 1 known gap), so
  gate-pass tails on non-targets reflect RMS-only silence gating.
- The vote gate (window 3) compresses differences; frame-level similarity
  statistics above are the sensitive metric.
- Multi-thread sherpa (num_threads>1) RTF was not swept.

## Bottom line

The similarity gate alone is already at 0% frame FPR on non-targets, so there
is no FPR headroom for suppression to claim; meanwhile GTCRN actively destroys
up to 29% of gate-pass recall on noise tiers. **Gate 3 outcome: stay with
similarity gate.** Revisit suppression only if real-device noisy captures show
non-target FPR > 0 on the Arm A pipeline.
