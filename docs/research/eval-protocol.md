# Eval protocol: similarity gate vs suppression+gate vs target speaker extraction

Date: 2026-11-26 · Lane: backend spike 4 (model and DSP research)

Goal: produce evidence that can trigger (or reject) a change of model direction per Gate 3 of the spike design ("Default remains the current similarity gate. Move toward target-speaker extraction only if quality improves materially without unacceptable CPU, latency, size, or licensing cost"), using the existing Voce fixture infrastructure (`eval/scripts/evaluate.py`, `eval_audio.py`, LibriSpeech/AliMeeting/MSC fixtures per `eval/AGENTS.md` and `docs/eval-harness.md`).

## Arms under comparison

| Arm | Pipeline | Model set |
|---|---|---|
| A — baseline similarity gate (current) | 16 kHz f32 → 512-sample chunks → WeSpeaker ResNet34 embedding per 3 s window → cosine vs enrolled → threshold + vote window | current ONNX embedding only |
| B — suppression + gate | same as A, with GTCRN (or DFN3 as fallback) applied to the 3 s window (or 512-sample stream) before embedding | gtcrn_simple.onnx + current embedding |
| C — target speaker extraction | streaming target-conditioned mask (VoiceFilter-Lite reimplementation artifact) conditioned on enrollment embedding; gate decision derived from extraction output (e.g., extracted-speech energy ratio, or gate re-run on extracted audio) | voicefilter_lite.onnx + embedder.int8.onnx |
| A′ — embedding swap (optional sub-arm) | identical to A with ECAPA512-LM or CAM++ | one-file swap |

All arms consume the **same enrollment PCM and same fixtures**. Arm B and C processing happens offline on fixture WAVs first; realtime integration is only attempted for an arm that wins offline.

## Fixtures and tiers

Reuse existing fixture generators (`make_fixtures.py`, `make_audio_tests.py`, `make_msc_tests.py`, AliMeeting download). Structure:

1. **Clean single-speaker (LibriSpeech enrolled + held-out same speaker):** sanity tier; all arms must score enrolled windows passed, and same-speaker held-out windows passed.
2. **Non-target interference (MSC / LibriSpeech other speakers at matched level):** measures suppression of other-speaker speech with noise-free interference — the discriminator between A (fails by design) and B/C.
3. **Noise, no interference (stationary + babble, SNR sweep 20/10/5/0 dB):** measures whether suppression helps or hurts the gate when there is only noise. Key risk: GTCRN nonlinear distortion shifting embeddings; this tier quantifies that.
4. **Overlap (AliMeeting segments):** target + interferer + noise. This is the tier where extraction (C) is expected to win and where the direction decision actually lives.
5. **Degraded capture tier** (after Krisp lesson 1): narrowband-simulated (band-limited to 8 kHz then upsampled) and level-mismatched variants of tiers 2–4, to test fallback-tier behavior.
6. **Unenrolled tier:** no enrollment loaded; all arms must exhibit defined passthrough/reject behavior (arm C passthrough per VoiceFilter-Lite design).

Each fixture keeps per-window ground truth (`t_s`, `is_enrolled`) in the existing format consumed by `match_predictions` in `eval/scripts/evaluate.py`.

## Metrics (exact)

All metrics computed per fixture tier and aggregated; per-window decisions use the existing ±tolerance matching (`match_predictions`).

Gate/decision quality (primary):
- **Recall (TPR)** on enrolled-target windows; **FPR** on non-target windows; **precision**, **F1** (as already implemented in `evaluate.py:metrics`).
- **EER-style separation:** cosine distributions of enrolled-vs-other windows; report equal-error point and d-prime. This is threshold-free, so it compares arms without threshold tuning artifacts.
- **Threshold sweep:** FPR/recall curve over threshold 0.60→0.90 (existing `just eval::threshold` machinery) per arm.
- **Decision latency:** windows from first target speech to first passed decision (gate reaction time) and from interferer-only to first rejected decision (leak time), in windows (32 ms units).

Suppression/extraction quality (secondary, only for arms B/C):
- **SI-SDR improvement** (dB) of processed vs mixture against target clean reference, computed on the overlap tier with available clean references; if no clean reference exists for a fixture (AliMeeting), report **DNSMOS OVRL/P.808** instead, using an ONNX DNSMOS model.
- **Interference attenuation:** band-energy ratio of interferer-only segments before/after processing (dB), so suppression of *other speech* is measured separately from noise suppression.

Cost (always reported, all arms):
- **RTF** on one CPU core (1 intra-op thread) for each added model stage.
- **Added latency:** algorithmic lookahead + chunk quantization, ms.
- **Model size on disk**, RSS delta, cold-start time.
- **Determinism:** bitwise-identical per-window decisions across two runs (Gate 1 requirement).

## Acceptance thresholds for each arm

- Arm A baseline is recorded as-is; its numbers are the reference.
- Arm B (suppression+gate) is a candidate improvement if, on tiers 2+4, **FPR decreases by ≥30 % relative at ≤2 % absolute recall loss**, with RTF ≤0.3 for the added stage and ≤40 ms added latency. If tier 3 shows recall loss >2 %, suppression must be gated by VAD/noise-classifier state (suppression only when noise detected) and re-measured.
- Arm C (extraction) is a candidate direction change only if, on tier 4 (overlap), **FPR decreases by ≥50 % relative** vs A at ≤2 % recall loss, **and** RTF ≤0.5 total, **and** added model size ≤50 MB, **and** license is Apache-2.0/MIT-class. Failing any one → stay with A (or B if B passed).
- Arm A′ (embedding swap) replaces the baseline embedding only if EER-style separation improves and FPR/recall both improve at the existing operating threshold with no pipeline change.

## What evidence justifies changing direction

Per Gate 3, direction changes require **artifacts and measurements**, not claims:

1. **Stay with A** if arms B/C fail their acceptance thresholds above, or if their gains appear only in tiers where A already performs acceptably (tier 1/3).
2. **Adopt B** if it passes its threshold and its gain is concentrated in tier 2/3 (noise and interference) — cheapest change, GTCRN artifact already meets license/size/rate constraints.
3. **Move toward C** only on tier-4 overlap evidence. Additional required evidence before any commitment: (a) the extraction artifact reproduces its published quality on Voce fixtures offline; (b) a viable C/C++ runtime path exists (today the VoiceFilter-Lite artifact is Python/onnxruntime — the port/FFI cost must be measured, e.g. via the voce_native shim's process-boundary mode, before deciding); (c) WeSep has shipped ONNX pretrained models — re-check the [WeSep README status table](https://github.com/wenet-e2e/wesep) at decision time, since a supported ONNX TSE artifact would supersede the reimplementation.
4. **Any direction change requires a repeat run:** two independent fixture runs with identical outputs, plus the cost table (RTF/latency/size/RSS), committed alongside the eval report.

## Execution order (cheapest evidence first)

1. Run arm A baseline on all tiers (existing harness, no code change) — 1 day.
2. Arm B offline: produce GTCRN-enhanced fixture audio via `sherpa-onnx-offline-denoiser` (no Rust integration), re-run arm A evaluator on enhanced audio — measures the entire hypothesis without touching the backend.
3. Arm C offline: same trick with the VoiceFilter-Lite reimplementation in a Python sidecar script (eval-only; not shipped).
4. Arm A′ embedding swaps: swap ONNX file, re-run.
5. Only for a passing arm: measure realtime RTF/latency on the target machine and evaluate shim integration cost.

This ordering means the whole comparison is executable in days, and nothing lands in the Rust backend until an arm has won offline with the thresholds above.

## Open items

- Clean-reference availability for AliMeeting overlap tier (affects SI-SDR vs DNSMOS choice).
- GTCRN chunk alignment with Voce's 512-sample chunks: GTCRN consumes 256-sample hops (512 FFT); the shim must define rechunking behavior. Add to fixture harness as a chunk-boundary test.
- VoiceFilter-Lite reimplementation provenance: third-party reimplementation, not Google's model — verify quality on Voce fixtures before citing its numbers anywhere user-facing.
