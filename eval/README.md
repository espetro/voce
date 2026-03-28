# Voce Eval Harness

Automated evaluation of Voce's speaker-gate filter using objective metrics
(False Positive Rate, Recall, F1) computed on synthetic audio fixtures.

---

## Prerequisites

- **Rust toolchain** (`cargo build --release`)
- **uv** — Python package manager (`brew install uv` or `pip install uv`)
- **Voce enrollment done** — `~/.voce/enrolled_embedding.json` must exist
  (run the GUI, complete the two-recording enrollment flow)
- **Your voice recording** — place a WAV file of your own voice at
  `eval/fixtures/enrolled.wav` (any sample rate, mono or stereo is fine;
  the script resamples automatically)

---

## Quick Start

```bash
# From the repo root:

# 1. Record yourself speaking for ~30 seconds (any tool, e.g. QuickTime)
#    and save as eval/fixtures/enrolled.wav

# 2. Set up everything: download LibriSpeech samples, build fixtures, build binary
make -f eval/Makefile eval-setup

# 3. Run evaluation
make -f eval/Makefile eval-run
```

---

## Directory Layout

```
eval/
├── README.md              — this file
├── Makefile               — convenience targets (see below)
├── requirements.txt       — Python dependencies (managed by uv)
├── fixtures/
│   ├── enrolled.wav       — YOUR voice recording (you provide this)
│   ├── pure_enrolled.wav  — 30 s enrolled only (generated)
│   ├── pure_spk_*.wav     — 30 s each other speaker (generated)
│   ├── mixed_*.wav        — 60 s alternating segments (generated)
│   ├── *.gt.jsonl         — ground-truth labels (generated alongside WAVs)
│   └── other/
│       └── spk_*.wav      — downloaded LibriSpeech speaker WAVs
└── scripts/
    ├── download_samples.py — download mini-LibriSpeech dev-clean-2
    ├── make_fixtures.py    — generate mixed WAVs + .gt.jsonl sidecars
    └── evaluate.py         — run voce --eval, compute metrics, print report
```

---

## Makefile Targets

| Target | Description |
|---|---|
| `eval-setup` | Create venv → install deps → download LibriSpeech → build fixtures → `cargo build --release` |
| `eval-run` | Run evaluation using threshold from `~/.voce/config.json` |
| `eval-run-debug` | Same but with `RUST_LOG=voce=debug` for verbose binary output |
| `eval-threshold` | Sweep threshold 0.60–0.90 in 0.05 steps to find the sweet spot |

---

## How the Eval Mode Works

The `--eval` flag added to the Voce binary reads a WAV file and runs it through
the **identical pipeline** as live audio:

1. WAV is resampled to 22050 Hz if needed (rubato)
2. Samples are fed through `EmbeddingWindowAccumulator` in 512-sample chunks
3. Every 0.5 s (11025 samples), a 1-second window is emitted
4. Each window goes through: fast silence check → neural VAD → speaker embedder
   → cosine similarity vs. enrolled embedding → sliding-vote gate
5. One JSONL line is printed per window:
   ```json
   {"t_s": 0.000, "similarity": 0.823, "passed": true}
   ```

The `evaluate.py` script runs the binary on each fixture WAV, matches output
windows to ground-truth intervals (±0.1 s tolerance), counts TP/FP/TN/FN, and
prints a report table.

---

## Pass Criteria

| Metric | Target |
|---|---|
| FPR (False Positive Rate) | < 5 % |
| Recall | > 85 % |

`evaluate.py` exits with code **0** if both criteria are met, **1** otherwise.

---

## Tuning the Threshold

```bash
make -f eval/Makefile eval-threshold
```

This sweeps cosine similarity thresholds from 0.60 to 0.90, printing an eval
report at each step. The optimal threshold is the lowest value that keeps FPR
below 5 % while maintaining recall above 85 %.

Update `~/.voce/config.json` with your chosen threshold:
```json
{"threshold": 0.80, "vote_window": 3}
```

---

## Adding More Enrolled Clips

`make_fixtures.py` reads a single `eval/fixtures/enrolled.wav`. For better
coverage, concatenate multiple recordings before running:

```bash
python3 -c "
import soundfile as sf, numpy as np, glob
clips = [sf.read(p)[0] for p in sorted(glob.glob('my_recordings/*.wav'))]
sf.write('eval/fixtures/enrolled.wav', np.concatenate(clips), 22050)
"
```
