# Gate 2 standardized bakeoff: pyscriptc backend

Date: 2026-11-26. Lane: G2P. Branch: `voce-spike-pyscriptc-backend`. Scope: standardized Gate 2 measurement of the existing pyscriptc real-backend CLI against the Rust native probe, with no backend redesign, no ABI change, and no fixture/model/build output committed. Measurement helpers only: `backends/pyscriptc/gate2_normalize.py` and `backends/pyscriptc/gate2_aggregate.py`.

## Final verdict

VERDICT: fail

The pyscriptc CLI drives the frozen `voce_native` ABI v1 staticlib against the standardized 100 MB ONNX model. Decisions, similarities, and recomputed native FNV digests match the Rust release probe exactly on all five standardized WAVs, runs are deterministic, the artifact is 74,273,784 B before the incremental rebuild and 75,865,744 B after it, with no `libonnxruntime.so` dependency, and mean per-window runtime is below 1 s. Two criteria fail: target recall on the standardized fixture is below the 85% floor for both backends (76.9% raw, 84.6% vote-gated), and the pyscriptc/native mean-window ratio is 1.271x against a 1.25x budget. Recall is a fidelity failure of the shared real backend on this fixture/model pair, and the ratio is a debug-staticlib cost, not a pyscriptc-vs-native divergence.

## Standardized inputs

| Item | Value |
| --- | --- |
| Model | `/home/own-agents/.cache/voce/models/voxblink2_samresnet34_ft.onnx`, 100,865,597 B, sha256 `33af8affe6191b1ebd196d2b56e22c2934104cd2764abfdbdd954d3a934eb2a1` |
| Enrollment | `/home/own-agents/.cache/voce/gate2-fixtures/enroll_1272.wav`, 1,920,044 B, 60 s, sha256 `0d65773a9d754cf629c246ef028cd508a203d05f6930cd2a8bf214a6b9f37316` |
| Target | `target_1272.wav`, 1,920,044 B, 60 s, sha256 `51c82c77bb221f6be6acf5867bca4f48e3fde163336079ae54e96f820ca6cdd4` |
| Others | `other_2035.wav`, `other_5895.wav`, `other_777.wav`, each 1,920,044 B, 60 s (hashes in the fixture manifest) |
| Format | 16 kHz mono PCM16 WAV; 48,000-sample (3 s) windows, 24,000-sample (1.5 s) hop in the native ABI; 39 windows per 60 s file |
| Threshold | 0.75 for every run; `--vote-window 3` passed to the pyscriptc CLI |

All hashes were checked against `/home/own-agents/.cache/voce/gate2-fixtures/manifest.json` before measurement. No fixture, model, WAV, or download was copied into the repo.

## Exact commands

```bash
# From the spike worktree; build.sh takes /tmp/tsc.lock internally.
sh backends/pyscriptc/build.sh --timings --debug=fences

# Native release probe build (direct heavy cargo command under the lock).
flock /tmp/tsc.lock sh -c 'cd native && cargo build --release'

MODEL=/home/own-agents/.cache/voce/models/voxblink2_samresnet34_ft.onnx
FIX=/home/own-agents/.cache/voce/gate2-fixtures

# Rust native probe, twice, all five standardized WAVs in one process per run.
NATIVE="native/target/release/voce-native-probe"
for n in 1 2; do
  flock /tmp/tsc.lock "$NATIVE" --backend real --models-dir "$MODEL" \
    --threshold 0.75 --repeat 1 \
    --enroll "$FIX/enroll_1272.wav" \
    --input "$FIX/target_1272.wav" --input "$FIX/other_2035.wav" \
    --input "$FIX/other_5895.wav" --input "$FIX/other_777.wav" \
    --out /tmp/opencode/g2ps/native-run-$n.json
done

# pyscriptc real-backend CLI, one WAV per invocation, each invocation twice.
for input in target_1272 other_2035 other_5895 other_777; do
  for n in 1 2; do
    flock /tmp/tsc.lock backends/pyscriptc/voce_spike \
      --input "$FIX/$input.wav" --enroll "$FIX/enroll_1272.wav" \
      --backend real --models-dir "$MODEL" --threshold 0.75 \
      --vote-window 3 --repeat 1 \
      > /tmp/opencode/g2ps/ps-$input-$n.json
  done
done

# Normalization and aggregation (measurement-only, no CLI changes).
python3 backends/pyscriptc/gate2_normalize.py \
  --native /tmp/opencode/g2ps/native-run-1.json \
  --pyscriptc /tmp/opencode/g2ps/ps-target_1272-1.json \
  --input target_1272.wav
python3 backends/pyscriptc/gate2_aggregate.py --dir /tmp/opencode/g2ps

# Artifact / dynamic dependency checks.
ldd backends/pyscriptc/voce_spike
stat -c '%n %s' backends/pyscriptc/voce_spike
```

`--repeat 1` keeps each invocation to four inference passes (scored windows, repeated windows, restored-profile windows, and one `score_window` probe). Two full invocations per WAV provide the required repeated-measurement evidence and cross-invocation determinism check. Timing from the CLI is the summed native `runtime_ms` over the first scored pass, excluding process startup and model load; wall time was measured separately with `date +%s%N` around each invocation.

## Build and iteration evidence

| Measurement | Result |
| --- | --- |
| Native release build, warm cargo cache (`cargo build --release` under lock) | 60,592 ms |
| Incremental managed rebuild (`sh backends/pyscriptc/build.sh --timings --debug=fences`, no clean) | 215,731 ms wall including lock wait behind the running fixture sweep; warm cargo dev profile finished in 0.23 s and pyscriptc total was 9,005.6 ms (parse 25.1, lower 871.2, validate 8.0, emit_llvm 897.3, cc_compile 1,304.5, cc_link 5,644.2); capability 50 native / 0 fenced |
| pyscriptc artifact `backends/pyscriptc/voce_spike` | 74,273,784 B before incremental rebuild; 75,865,744 B after (both below 150 MB) |
| Native debug staticlib staged as `backends/pyscriptc/lib/libvoce_native.a` | 195,262,032 B |
| Native release probe / release staticlib | 26,523,904 B / 122,129,422 B |
| Cold rebuild | Not measured: budget prioritized the standardized fixture measurements |

## Probe outputs and comparison

Native probe runs (release, 5 inputs x 39 windows = 156 windows per run):

| Run | Wall (ms) | Mean per window (ms) | Model load (ms) | Peak RSS (B) | Digests (target / 2035 / 5895 / 777) |
| --- | --- | --- | --- | --- | --- |
| 1 | 294,640 | 847.29 | 909.55 | 467,144,704 | `43c86ebaccf85fb4` / `ac679c690c4a45e5` / `dc9ec0614ffb8948` / `dd60f839095113d0` |
| 2 | 259,044 | 705.77 | 880.10 | 501,362,688 | identical |

pyscriptc CLI runs (real backend, one WAV per invocation, two invocations each):

| Input | Run | Wall (ms) | Mean per window (ms) | Model load (ms) | Peak RSS (B) | Passed / windows | Decision digest |
| --- | --- | --- | --- | --- | --- | --- | --- |
| target_1272 | 1 | 147,512 | 981.89 | 915.53 | 608,395,264 | 30 / 39 | `1136148149` |
| target_1272 | 2 | 166,384 | 1054.35 | 982.47 | 597,962,752 | 30 / 39 | `1136148149` |
| other_2035 | 1 | 144,089 | 938.19 | 838.43 | 613,326,848 | 0 / 39 | `179093175` |
| other_2035 | 2 | 150,925 | 878.52 | 909.19 | 580,354,048 | 0 / 39 | `179093175` |
| other_5895 | 1 | 151,026 | 963.56 | 923.75 | 598,409,216 | 0 / 39 | `179093175` |
| other_5895 | 2 | 171,081 | 1075.91 | 917.32 | 565,342,208 | 0 / 39 | `179093175` |
| other_777 | 1 | 186,666 | 1118.68 | 943.77 | 609,366,016 | 0 / 39 | `179093175` |
| other_777 | 2 | 144,378 | 881.96 | 963.28 | 572,416,000 | 0 / 39 | `179093175` |

Comparison summary:

| Metric | Native | pyscriptc |
| --- | --- | --- |
| Windows per 60 s input | 39 | 39 |
| Embedding dimension | 256 | 256 |
| Raw decisions | 30/39 target pass, 0/117 other pass | identical |
| Majority-of-3 (vote-gated) decisions | 33/39 target, 0/117 others | 33/39 target, 0/117 others |
| Mean per-window runtime | 776.53 ms | 986.63 ms |
| Peak RSS | 467,144,704 to 501,362,688 B | 613,326,848 B |
| Model load | 909.55 ms (run 1) | 915.53 ms (first target run) |
| Recomputed FNV digest (target) | `43c86ebaccf85fb4` | `43c86ebaccf85fb4` |
| Profile JSON roundtrip | true on all 4 inputs in both runs | true on all 4 inputs in every invocation |
| Determinism | true in both runs | true across both invocations of every input |

Decision parity is bit-for-bit: per-window similarity f32 patterns, speech probability, and raw decisions agree on all 156 windows per run; applying the documented majority-of-3 rule to both sides leaves zero mismatches. Recomputing the Rust FNV-1a digest from pyscriptc window fields reproduces all four native digests exactly. The pyscriptc CLI does not apply the vote rule inside the native path (it reports the configured `--vote-window` but emits raw ABI decisions); the Rust probe similarly has no vote flag. The vote-gated numbers in this report are produced by the measurement-only normalizer applied identically to both outputs.

## Gate 2 criteria

| # | Criterion | Result | Evidence |
| --- | --- | --- | --- |
| 1 | 100% vote-gated decision parity with native at 0.75 | pass | 0 raw and 0 vote-gated mismatches over 156 windows; recomputed FNV digests identical on all 5 WAVs |
| 2 | Target recall >= 85% and aggregate other FPR < 5% | fail | target 30/39 = 76.9% raw (84.6% vote-gated); others 0/117 = 0% raw (0.0% vote-gated), identical in both backends |
| 3 | Determinism across repeated runs | pass | both native runs and all 8 pyscriptc invocations deterministic; identical windows/digests across invocations |
| 4 | No correctness-blocking compiler/runtime fence | pass | real backend compiles and runs; all status codes 0 except `process_status = 39`, the documented window-count return; see fences below |
| 5 | Self-contained wrt ONNX Runtime | pass | `ldd backends/pyscriptc/voce_spike` lists only vdso, libstdc++, libm, libgcc_s, libc, ld-linux; no `libonnxruntime.so` |
| 6 | Mean < 1 s/window and <= 1.25x native | fail | 986.63 ms vs native 776.53 ms, ratio 1.271x |
| 7 | Peak RSS <= 1.25x native or native +128 MiB | pass | pyscriptc 584.9 MiB vs native 445.5-478.1 MiB; native max +128 MiB = 606.1 MiB |
| 8 | Artifact < 150 MB | pass | 74,273,784 B (~70.8 MiB) initially; 75,865,744 B (~72.3 MiB) after incremental rebuild |

## Fences and constraints

- Non-reentrant lock: `build.sh` takes `/tmp/tsc.lock` internally, so wrapping it in another `flock` deadlocks. Direct cargo/test/probe commands take the lock once.
- C++ runtime link: the real staticlib needs `libs = ["pthread", "dl", "stdc++", "m"]` in `voce_native.toml`; without `stdc++`/`m` the link fails with about 98.6k undefined C++ symbols. Already present in this branch.
- Manifest rejects `..` paths: `build.sh` stages `libvoce_native.a` beside the manifest before compiling.
- Local module fence: adapter classes must live in `spike.py` (PS2002/PS1106 for local imports); the normalizers are standalone scripts and are not imported by the compiled spike.
- Handle ownership: `consumes` arguments must be local handle variables (PS4011); creation and closing stay inside `run_native`.
- Capacity protocol: `process_pcm` capacity is derived from `WINDOW_SAMPLES / HOP_SAMPLES` instead of a NULL PCM query; `process_status = 39` is the 39-window count, not an error code.
- Vote-window gap: the native path emits raw ABI decisions and does not apply majority voting; vote-gated parity is computed externally on both sides, so the CLI's `--vote-window` value is metadata in this path. This is a semantic fence for production gating, not a correctness-blocking compiler/runtime fence.
- Model is an external runtime file: ONNX Runtime is statically embedded, but the 100 MB ONNX model must be provisioned on disk via `--models-dir`; it is not committed and not embedded in the artifact.
- Host contention: other agents were compiling on this VPS during the sweep. Both probes were run under the same lock and the same host, but wall time varies; per-window ABI timings from both runs are reported rather than a single favorable number.

No native source changed in this lane, so native tests were not rerun; the last recorded run under `flock /tmp/tsc.lock` (prior lane) passed 6 unit, 7 ABI integration, and 2 real-backend tests. `git diff --check` was clean at commit time.

## Follow-ups

The recall shortfall is shared by both backends on this model/fixture pair. The next step is model/threshold work in the native lane, not a pyscriptc orchestration change; pyscriptc is useful as a typed verification harness across the frozen ABI but does not alter inference fidelity or latency.
