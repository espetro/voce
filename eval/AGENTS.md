# eval/ — Evaluation Harness

The evaluation harness benchmarks the speaker-gate filter against three datasets: **LibriSpeech** (enrollment), **AliMeeting** (multi-speaker audio), and **MSC** (meeting speech corpus). All evaluation uses the Rust release binary and measures FPR, recall, and similarity statistics.

---

## Pass Criteria

A filter configuration is **acceptable** when evaluated on LibriSpeech if:

| Metric | Threshold | Rationale |
|--------|-----------|-----------|
| **False Positive Rate (FPR)** | < 5% | Wrongly passing non-target speech |
| **Recall** | > 85% | Passing target speech (enrolled voice) |
| **Exit Code** | 0 | No runtime errors |

These are **soft targets**; final thresholds are chosen via manual sweep (see `just eval::threshold`).

---

## Directory Layout

```
eval/
├── pyproject.toml           # Python dependencies (replaces requirements.txt)
├── justfile                 # Task recipes (replaces Makefile)
├── AGENTS.md                # This guide
├── validate-ac.sh           # Manual AC validation with user recordings
├── scripts/
│   ├── evaluate.py          # Main eval harness (LibriSpeech)
│   ├── eval_audio.py        # Audio eval (AliMeeting / MSC)
│   ├── download_samples.py  # Fetch LibriSpeech test set
│   ├── make_fixtures.py     # Build enrollment fixtures from LibriSpeech
│   ├── download_alimeeting.py # Fetch AliMeeting dataset
│   ├── make_audio_tests.py  # Build audio test fixtures (AliMeeting)
│   ├── make_msc_tests.py    # Build MSC test fixtures
│   └── ...
└── fixtures/
    ├── librispeech_enroll_*.wav  # Enrollment samples (speaker IDs 84, 174, 251, 777, 1272, 1462)
    ├── librispeech_test_*.wav    # Test samples (enrollment speaker)
    ├── librispeech_other_*.wav   # Test samples (non-enrollment speakers)
    ├── mixed_*.wav               # AliMeeting mixed audio (speaker overlaps)
    ├── mixed_*.gt.jsonl          # AliMeeting ground truth (speaker timings)
    ├── msc/                      # MSC test fixtures (external dataset)
    └── audio_tests/              # Intermediate audio test files
```

---

## Python Tooling Rules

The eval environment uses `uv` with a `pyproject.toml` source of truth. **Never activate `.venv` manually.**

### Running Python Scripts

All Python scripts run via `uv`:

```bash
# From repo root:
uv run --project eval python eval/scripts/evaluate.py --binary target/release/voce
```

**Do not:**
```bash
# ❌ Don't manually activate .venv
source eval/.venv/bin/activate

# ❌ Don't use old system Python
python eval/scripts/evaluate.py

# ❌ Don't use pip directly
pip install -r eval/requirements.txt
```

### Dependency Management

All dependencies are in `eval/pyproject.toml`. To add a package:

1. Edit `eval/pyproject.toml` (`[project]dependencies` section)
2. Run `uv sync --project eval` to verify
3. Commit `pyproject.toml` (lock file is not committed)

---

## just eval::* Commands Reference

All evaluation tasks are recipes in `eval/justfile` (runs from repo root).

### LibriSpeech Evaluation (Main Pipeline)

| Command | Purpose | Time | Notes |
|---------|---------|------|-------|
| `just eval::setup` | Download samples, build fixtures, compile release binary | ~5–10 min | One-time setup; builds all needed test data |
| `just eval::run` | Evaluate release binary using `~/.voce/config.json` threshold | ~3 min | Requires valid enrollment samples |
| `just eval::run-debug` | Same as `run` but with `RUST_LOG=voce=debug` | ~3 min | Capture binary stderr for debugging |
| `just eval::threshold` | Sweep threshold 0.60 → 0.90 (step 0.05), restore config | ~30 min | Find optimal threshold for LibriSpeech |

### AliMeeting Evaluation (Multi-Speaker Audio)

| Command | Purpose | Time | Notes |
|---------|---------|------|-------|
| `just eval::alimeeting-setup` | Download AliMeeting data, build audio test fixtures | ~10 min | Large download (~5 GB) |
| `just eval::alimeeting-run` | Evaluate on AliMeeting audio at current threshold | ~5 min | Requires AliMeeting fixtures |
| `just eval::alimeeting-sweep` | Sweep threshold 0.50 → 0.95 (step 0.05) | ~45 min | Find threshold for multi-speaker |

### MSC Evaluation (Meeting Speech Corpus)

| Command | Purpose | Time | Notes |
|---------|---------|------|-------|
| `just eval::msc-setup [MSCD_DIR]` | Build MSC fixtures from external dataset | ~5 min | Requires `MSCD_DIR` path or default |
| `just eval::msc-run` | Evaluate on MSC audio at current threshold | ~5 min | Requires MSC fixtures |
| `just eval::msc-sweep` | Sweep threshold 0.50 → 0.95 (step 0.05) | ~45 min | Find threshold for MSC |

### Cleanup

| Command | Purpose |
|---------|---------|
| `just eval::alimeeting-clean` | Remove generated AliMeeting fixtures (keep downloads) |
| `just eval::msc-clean` | Remove all MSC fixtures |

### Manual Validation

| Command | Usage | Purpose |
|---------|-------|---------|
| `just eval::validate-ac ENROLL1 OTHER [ENROLL2]` | `just eval::validate-ac me.wav other.wav` | Test custom recordings |

---

## Binary Evaluation Modes

The Rust binary has two evaluation modes (set via CLI flags):

### `--eval-enroll` Mode

Used during enrollment fixture creation. Records enrollment samples and saves embeddings.

```bash
cargo build --release
./target/release/voce --eval-enroll <speaker_id> <output.wav>
# Outputs:
# - output.wav (recorded audio)
# - embedding.json (extracted [f32; 256] vector)
```

### `--eval` Mode

Main evaluation mode. Runs filter against test samples and outputs JSONL results.

```bash
./target/release/voce --eval <enroll_embedding.json> <test_sample.wav>
# Outputs (JSONL):
# {"timestamp_s": 0.0, "similarity": 0.92, "passing": true, ...}
# {"timestamp_s": 0.032, "similarity": 0.91, "passing": true, ...}
# ...
```

---

## Fixture Structure

### LibriSpeech Fixtures

Mini-LibriSpeech (6 speakers, ~30 hours each):

```
eval/fixtures/
├── librispeech_enroll_84_0.wav      # Speaker 84, enrollment sample 1
├── librispeech_enroll_84_1.wav      # Speaker 84, enrollment sample 2
├── librispeech_test_84_0.wav        # Speaker 84, test (should pass filter)
├── librispeech_test_84_1.wav
├── librispeech_other_174_0.wav      # Speaker 174, test (should fail filter)
├── librispeech_other_174_1.wav
├── ... (for speakers 251, 777, 1272, 1462)
```

**Workflow:**
1. `download_samples.py` fetches LibriSpeech dev-clean-2, extracts 6 speakers
2. `make_fixtures.py` segments into enrollment + test samples
3. Binary enrolls on `enroll_*` files, tested on `test_*` and `other_*`

### AliMeeting Fixtures

Mixed-speaker audio with ground truth speaker labels:

```
eval/fixtures/
├── mixed_1.wav               # Multi-speaker audio
├── mixed_1.gt.jsonl          # Ground truth: [{"start_s": 0, "end_s": 2.5, "speaker": "S1"}, ...]
├── mixed_2.wav
├── mixed_2.gt.jsonl
├── ... (~100 clips)
```

**Workflow:**
1. `download_alimeeting.py` fetches meeting recordings
2. `make_audio_tests.py` mixes speaker A + speaker B, labels ground truth
3. Binary enrolls on speaker A, filtered clips compared to ground truth

### MSC Fixtures

External dataset (user-supplied). Built on-demand:

```
eval/fixtures/msc/
├── ...  (structure depends on make_msc_tests.py and dataset)
```

Use `MSC_DIR` env var to specify dataset location (default: `~/Downloads/Multi-stream Spontaneous Conversation Training Dataset`).

---

## Threshold Configuration

### Config File: `~/.voce/config.json`

The threshold is stored in the user's config:

```json
{"threshold": 0.75, "vote_window": 3}
```

- **threshold** (0.0–1.0): Cosine similarity cutoff (higher = stricter filter)
- **vote_window** (frames): Number of consecutive frames to vote on (smoothing)

### Sweep Behavior

Threshold sweeps temporarily overwrite `config.json`, then restore it:

```bash
just eval::threshold
# For each threshold in [0.60, 0.65, ..., 0.90]:
#   1. Write {"threshold": t, "vote_window": 3} → ~/.voce/config.json
#   2. Run evaluation
#   3. Print FPR / Recall / Similarity stats
# 4. Restore original config
```

If `config.json` doesn't exist, sweeps assume `{"threshold": 0.75, "vote_window": 3}` as default.

---

## How to Add a New Dataset

### 5-Step Checklist

1. **Create fixture download script** in `eval/scripts/`:
   ```python
   # eval/scripts/download_mydata.py
   def download_mydata():
       # Fetch dataset, extract to eval/fixtures/mydata/
       pass
   ```

2. **Create fixture builder** in `eval/scripts/`:
   ```python
   # eval/scripts/make_mydata_tests.py
   def make_mydata_tests():
       # Load mydata, segment into enrollment + test
       # Save as WAV files or JSONL ground truth
       pass
   ```

3. **Add just recipe** in `eval/justfile`:
   ```just
   mydata-setup:
       uv run --project eval python eval/scripts/download_mydata.py
       uv run --project eval python eval/scripts/make_mydata_tests.py
       cargo build --release

   mydata-run:
       uv run --project eval python eval/scripts/eval_audio.py --binary target/release/voce --fixtures-dir eval/fixtures/mydata
   ```

4. **Add sweep recipe** (if needed):
   ```just
   mydata-sweep:
       #!/usr/bin/env bash
       # Template from eval::alimeeting-sweep
   ```

5. **Update this guide**: Add dataset to directory layout and checklist above.

---

## Common Workflows

### Initial Setup

```bash
just eval::setup
just eval::run  # Check baseline on LibriSpeech
```

### Find Optimal Threshold

```bash
just eval::threshold
# Read output, choose threshold with best FPR/Recall tradeoff
# Edit ~/.voce/config.json manually if needed
```

### Validate Against AliMeeting

```bash
just eval::alimeeting-setup
just eval::alimeeting-run
just eval::alimeeting-sweep  # Compare across thresholds
```

### Debug a Single Test

```bash
RUST_LOG=voce=debug just eval::run  # or just eval::run-debug
# Check stderr for filter decision logs
```

---

## Testing the Harness

1. **Unit tests** in Python scripts:
   ```bash
   uv run --project eval pytest eval/scripts/  # if tests exist
   ```

2. **Dry-run** (check imports without running full eval):
   ```bash
   uv run --project eval python -c "import sys; sys.path.insert(0, 'eval/scripts'); from evaluate import *; print('ok')"
   ```

3. **Manual end-to-end**:
   ```bash
   just eval::setup
   just eval::run
   ```

---

## Troubleshooting

| Issue | Solution |
|-------|----------|
| `No fixture WAVs found` | Run `just eval::setup` first |
| `.venv/bin/python: not found` | Never activate `.venv` manually; use `uv run --project eval` |
| Model download hangs | Check network; restart with `just eval::setup` |
| `threshold: no such file` | Manually create `~/.voce/config.json` with default values |
| AliMeeting download fails | Download URL may have changed; check `download_alimeeting.py` |
