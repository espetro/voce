# Gate 2 standardized bakeoff: scriptc backend

Date: 2026-11-26 · Branch: `voce-spike-scriptc-backend` · scriptc 0.4.16 · ABI v1 unchanged · model `voxblink2_samresnet34_ft.onnx`.

Question: does the real-backend scriptc CLI meet the standardized Gate 2 pass criteria on the staged 60-second speaker fixtures, using the Rust native probe as the reference?

Verdict: **fail**. Criterion 2 fails narrowly: vote-gated other-speaker FPR is 6/117 = 5.128%, above the required strictly below 5%. The mismatch is identical on the Rust native probe, so it is model/fixture behavior, not a scriptc parity defect. Raw per-window recall is also 60/78 = 76.9%, below 85%, if the vote gate is not applied. All other seven criteria pass.

## Gate results

| # | Criterion | Result | Evidence |
|---|---|---|---|
| 1 | 100% vote-gated decision parity with Rust native probe at 0.75 | PASS | 312/312 aligned windows match on raw similarity bits, speech probability bits and decision; 312/312 vote-gated decisions match; 8/8 per-input FNV-1a digests match exactly. |
| 2 | Target recall >= 85% and aggregate other FPR < 5% | FAIL | Vote-gated recall 78/78 = 100%, other FPR 6/117 = 5.128%; raw recall 60/78 = 76.9%, raw other FPR 0/117. Both interpretations fail one limb. |
| 3 | Deterministic repeated runs | PASS | Native probe `deterministic: true`; all 10 scriptc `run` invocations produced identical profile JSON and window digests; `bench --repeat 2` reports `deterministic: true`; profile roundtrip `score_digest_equal: true`. |
| 4 | No correctness-blocking compiler/runtime fence | PASS | No new fence. Existing C frame wrapper remains mandatory for pointer-to-length and element-count arguments; native tests pass with the same baseline and no native source changed. |
| 5 | Self-contained with respect to ONNX Runtime | PASS | `ldd` on the built executable shows no `libonnxruntime.so` and no `libvoce_native.so`; only `libm`, `libstdc++`, `libc`, `libgcc_s` and the loader. |
| 6 | Mean window < 1 s and <= 1.25x native | PASS | Scriptc mean 719.22 ms/window, native mean 730.59 ms/window, ratio 0.984x. |
| 7 | Peak RSS <= max(1.25x native, native + 128 MiB) | PASS | Scriptc peak 332,439,552 B (317.1 MiB), native peak 554,663,936 B (529.0 MiB), ratio 0.599x. |
| 8 | Artifact < 150 MB | PASS | 68,271,528 B (65.1 MiB). |

## Standardized inputs

All inputs are 16 kHz mono PCM16 WAV, 60 seconds each. SHA256 values were verified against the staged manifest and model hash.

| Role | File | SHA256 |
|---|---|---|
| enrollment | `enroll_1272.wav` | `0d65773a9d754cf629c246ef028cd508a203d05f6930cd2a8bf214a6b9f37316` |
| target | `target_1272.wav` | `51c82c77bb221f6be6acf5867bca4f48e3fde163336079ae54e96f820ca6cdd4` |
| other | `other_2035.wav` | `f1f563d5e8280eee76f2db53dde491c3d0b7b4f016133eacc9195cac7a2922cc` |
| other | `other_5895.wav` | `0e9435c02a65fb7de2695ead133598d7448392b6d3ca3b95ae2578018e198744` |
| other | `other_777.wav` | `ac5ee3d4c7bbc9d465797faefc88e0d00203e2e734a2548fdcdb30f5946c8163` |
| model | `voxblink2_samresnet34_ft.onnx` | `33af8affe6191b1ebd196d2b56e22c2934104cd2764abfdbdd954d3a934eb2a1` |

## Exact commands

Baseline native tests, before any source change:

```sh
flock -w 300 /tmp/tsc.lock env \
  VOCE_REAL_MODEL=/home/own-agents/.cache/voce/models/wespeaker_en_voxceleb_resnet34.onnx \
  cargo test --manifest-path native/Cargo.toml
```

Result: 6 unit + 7 ABI + 2 gated real tests passed, 0 failed. Native source was not changed during this lane, only `backends/scriptc/src/cli_native.ts` to emit `model_load_ms` in existing JSON output.

Incremental managed rebuild, Rust and native caches retained:

```sh
flock -w 300 /tmp/tsc.lock sh backends/scriptc/ffi/build-wrapper.sh
flock -w 300 /tmp/tsc.lock scriptc build backends/scriptc/src/cli_native.ts \
  --ffi backends/scriptc/ffi/voce_native_scriptc.json \
  -o /tmp/voce-gate2-scriptc-native --no-keep-llvm
```

Measured 424.0 ms for the wrapper and 9,528.1 ms for scriptc. A second unchanged incremental rebuild took 387.4 ms and 4,470.4 ms. Both produced a 68,271,528-byte executable. No full cold rebuild was attempted because the six fixture measurements took priority.

Rust native probe, one process with two repeats over all four inputs:

```sh
flock -w 600 /tmp/tsc.lock native/target/debug/voce-native-probe \
  --backend real \
  --models-dir /home/own-agents/.cache/voce/models/voxblink2_samresnet34_ft.onnx \
  --enroll /home/own-agents/.cache/voce/gate2-fixtures/enroll_1272.wav \
  --threshold 0.75 --repeat 2 \
  --input /home/own-agents/.cache/voce/gate2-fixtures/target_1272.wav \
  --input /home/own-agents/.cache/voce/gate2-fixtures/other_2035.wav \
  --input /home/own-agents/.cache/voce/gate2-fixtures/other_5895.wav \
  --input /home/own-agents/.cache/voce/gate2-fixtures/other_777.wav \
  --out /tmp/gate2-evidence/native-probe.json
```

Scriptc real-backend CLI, two separate processes per input:

```sh
for rep in 1 2; do
  for f in target_1272.wav other_2035.wav other_5895.wav other_777.wav; do
    /tmp/voce-gate2-scriptc-native run --backend real \
      --model /home/own-agents/.cache/voce/models/voxblink2_samresnet34_ft.onnx \
      --enroll /home/own-agents/.cache/voce/gate2-fixtures/enroll_1272.wav \
      --input /home/own-agents/.cache/voce/gate2-fixtures/$f \
      --threshold 0.75
  done
done
```

Additional scriptc digest and roundtrip evidence:

```sh
flock -w 600 /tmp/tsc.lock /tmp/voce-gate2-scriptc-native bench \
  --backend real \
  --model /home/own-agents/.cache/voce/models/voxblink2_samresnet34_ft.onnx \
  --enroll /home/own-agents/.cache/voce/gate2-fixtures/enroll_1272.wav \
  --input /home/own-agents/.cache/voce/gate2-fixtures/target_1272.wav \
  --other /home/own-agents/.cache/voce/gate2-fixtures/other_2035.wav \
  --repeat 2

flock -w 600 /tmp/tsc.lock /tmp/voce-gate2-scriptc-native roundtrip \
  --backend real \
  --model /home/own-agents/.cache/voce/models/voxblink2_samresnet34_ft.onnx \
  --enroll /home/own-agents/.cache/voce/gate2-fixtures/enroll_1272.wav \
  --input /home/own-agents/.cache/voce/gate2-fixtures/target_1272.wav
```

After the final rebuild, the target probe was rerun twice with the same `run --backend real --model ... --enroll ... --input target_1272.wav --threshold 0.75` command, under `/tmp/tsc.lock`, writing `/tmp/gate2-evidence/final-target-run-1.stdout` and `final-target-run-2.stdout`.

All exact stdout JSON, stderr and parsed aggregate data are in `/tmp/gate2-evidence/`; none of these artifacts, WAVs, models or downloads are committed. `/tmp/voce-gate2-scriptc-summary.json` is the machine-readable summary.

## Comparison table

| Metric | scriptc | Rust native probe |
|---|---|---|
| Backend | real ONNX through staticlib | real ONNX through rlib |
| Threshold | 0.75 | 0.75 |
| Embedding dimension | 256 | 256 |
| Windows per 60 s input | 39 | 39 |
| Target raw pass | 60/78 (76.9%) | 60/78 (76.9%) |
| Other raw pass | 0/117 (0%) | 0/117 (0%) |
| Target vote-gated pass | 78/78 (100%) | 78/78 (100%) |
| Other vote-gated pass | 6/117 (5.128%) | 6/117 (5.128%) |
| Mean runtime per window | 719.22 ms | 730.59 ms |
| Runtime ratio scriptc/native | 0.984x | 1.000x |
| Process wall, full two-repeat probe | 448,059.7 ms summed over 10 single-input scriptc runs, 154,836.7 ms bench, 73,265.7 ms roundtrip | 499,096.5 ms |
| Model-load time | 948.63 ms mean per scriptc process | 918.68 ms and 872.67 ms |
| Peak RSS | 332,439,552 B | 554,663,936 B |
| Profile JSON bytes | 5,412 | 5,412 |
| Deterministic | yes across all runs | yes |

The process wall figures use a nanosecond monotonic clock around each command. Native probe wall includes two complete repeats plus a restored-profile engine. Scriptc `run` wall includes one model load, enrollment, one input and process startup. Per-window runtime is the engine-reported `runtime_ms` summed and divided by window count; it excludes process startup and model load.

## Decision and digest alignment

Both probes process 39 overlapping windows per 60-second file at a 1.5-second hop. The native CLI does not implement the app-level sliding vote gate, so the same `SlidingVoteGate` semantics from `src/filter/gate.rs` were applied to both similarity streams: fail-open, mute only when the last three similarities are all below 0.75. This is the closest meaningful decision alignment; no per-window offset needed to be applied.

| Input | Native FNV-1a digest | Scriptc FNV-1a digest | Vote-gated pass native/scriptc | Raw pass native/scriptc |
|---|---|---|---|---|
| `target_1272.wav` | `43c86ebaccf85fb4` | `43c86ebaccf85fb4` | 39/39, 39/39 | 30/39, 30/39 |
| `other_2035.wav` | `ac679c690c4a45e5` | `ac679c690c4a45e5` | 2/39, 2/39 | 0/39, 0/39 |
| `other_5895.wav` | `dc9ec0614ffb8948` | `dc9ec0614ffb8948` | 2/39, 2/39 | 0/39, 0/39 |
| `other_777.wav` | `dd60f839095113d0` | `dd60f839095113d0` | 2/39, 2/39 | 0/39, 0/39 |

The scriptc `bench` profile JSON digest has SHA256 `8305564d6cf0a5d5b0ec026b1ec4d650626fabf3bc8eab05392c3a730120dd5e` over 5,412 bytes. Target and other stable-window digest SHA256 values are `5b2085a15b5f819e16264d1932ca9ad0960969a6a6463ef4da77268030509c54` and `e4a2ed5d664349a2479f8218394026a3af5be66157675a26e34dd1173c898997`. Native and scriptc FNV-1a digests match for all eight input-repeat pairs.

## Fences and constraints

- The C frame wrapper remains mandatory. Scriptc FFI `mutable-bytes` lowers to `(ptr, byte_length)`, so native `int64_t *` outputs cannot be expressed directly.
- Byte-length versus element-count conversion is handled in wrapper ops 4, 8, 9 and 10. Direct FFI previously passed 192,000 instead of 48,000 samples and returned `-2`.
- The real staticlib requires `stdc++` in the FFI manifest. Without it the link fails on Abseil and C++ runtime symbols.
- The FFI manifest contains an absolute path to the debug staticlib in this worktree. A rebuild in another worktree needs that path updated. This is a build portability constraint, not a runtime dependency.
- `ort`'s `download-binaries` feature acquires ONNX Runtime at build time. Offline builds need a prepopulated cache or vendored ORT. The model itself is passed at runtime and is not embedded.
- The executable is Linux x86-64/glibc-specific and still depends on host `libm`, `libstdc++`, `libc` and `libgcc_s`.
- Neither probe implements the app's vote gate. Applying `src/filter/gate.rs` semantics externally is a measurement choice, documented above.
- The 5.128% vote-gated other FPR comes from the fail-open first two windows of each 60-second other file. It is identical on the native probe, so it is not caused by scriptc.
- `/usr/bin/time` is unavailable on this host. Wall times use a monotonic clock and RSS is read from `/proc/self/status` and `/proc/self/statm`.
- Build and probe commands use `/tmp/tsc.lock` because other lanes share it.

## Final verdict

Gate 2 is **fail** under the stated pass criteria. The scriptc backend has exact native parity, deterministic behavior, acceptable speed, memory and artifact size, and no new correctness fence. The only failing criterion is the strict quality threshold: aggregate other-speaker FPR is 6/117 = 5.128% with the app's vote gate, and raw target recall is below 85% without it. Because the native reference has the same numbers, this is a model or fixture-level outcome rather than a scriptc implementation defect, but it still fails the lane as written.

## Hardening amendment (2026-11-26, criteria v2)

This amendment supersedes the v1 verdict above. Criteria v2 reclassifies recall/FPR and RSS as informational, raises the native runtime-ratio cap to 1.5x, and requires vote-count normalization plus release-mode re-measurement. The amended verdict is **pass with constraints**. The constraints from the original report still stand: the C frame wrapper is mandatory, the FFI manifest carries an absolute staticlib path that must be rewritten for a different checkout, ORT binaries must be cached or vendored for offline builds, and the artifact is Linux x86-64/glibc only.

### Vote-window counting normalization

The 78 versus 39 target-window disagreement was caused by two independent harness differences, not by backend behavior. The scriptc helper ran eight separate `run` processes, one per input and repeat, reset vote history naturally at each process boundary, and then summed both repeats into one denominator. The 78 figure therefore double-counted the same 39 unique target windows as two repeat observations; both counts are internally consistent but they answer different questions. Its vote rule was the app rule from `src/filter/gate.rs`: `SlidingVoteGate` starts in the fail-open passing state, pushes `similarity >= threshold` into a history capped at `vote_window`, and mutes only when the history is full and every frame failed. The pyscriptc helper used a different rule, passing a frame when at least two of the last three raw decisions passed, and its aggregator kept only repeat 1 (`first = {name: ps[name][0] ...}`), discarding repeat 2. Pyscriptc's CLI is one WAV per invocation, so its vote state also reset per file and invocation. Native probe normalization created a fresh vote history for each input/repeat even though all inputs were processed in one process.

The committed harness `backends/scriptc/gate2_hardening.py` implements one method for both lanes: vote state is fresh for every `(input, repeat)`, each 3 s window is counted exactly once in that file/repeat, and per-repeat results are aggregated separately instead of concatenating repeats into one vote stream. Repeats are separate observations of the same fixture content, so they are not counted as new unique audio, but both repeats contribute to the combined denominator. Native raw decisions and app-vote decisions are compared against scriptc on the same per-file/repeat basis. The old majority-of-3 rule is retained only as a diagnostic so the pyscriptc numbers can be reproduced from the standardized fixtures.

Using the corrected method, each repeat contributes 39 target windows and 117 other-speaker windows. The combined denominators are 78 target and 234 other windows. Per repeat: target raw 30/39, app-vote 39/39, majority-vote 33/39; other raw 0/117, app-vote 6/117, majority-vote 0/117. Combined: target raw 60/78 = 76.9%, app-vote 78/78 = 100%, majority-vote 66/78 = 84.6%; other raw 0/234 = 0%, app-vote 12/234 = 5.128%, majority-vote 0/234 = 0%. The app-vote rule is canonical because it matches the shipped `SlidingVoteGate`. The pyscriptc 84.6% figure is explained entirely by the majority rule plus first-repeat-only counting, not by a backend decision mismatch.

Vote-gated FPR can exceed raw FPR because the two measurements ask different questions. Raw FPR counts per-window decisions: no other-speaker window clears 0.75. The app gate is a fail-open temporal smoother: the first two windows of each 60 s other file pass while the history fills, and muting starts only after three consecutive failures. That contributes two passing windows per other file, hence 6/117 in one repeat and 12/234 in two. The majority rule requires at least two passes in three and therefore reports zero. This is intentional smoothing lag in the app gate, not a scoring error.

### Release-mode measurements

All build and probe commands in this amendment were individually wrapped in `flock -w 1800 /tmp/tsc.lock`. The release staticlib and native probe were rebuilt with `cargo build --release` in 39.37 s (`/tmp/opencode/g2srel/build.log`), the C wrapper was recompiled against it, and the scriptc artifact was re-linked through `backends/scriptc/ffi/voce_native_scriptc.release.json`. The native probe was also the release binary.

| Metric | scriptc (release) | native probe (release) | Ratio |
| --- | --- | --- | --- |
| Mean 3 s window runtime | 513.51 ms | 365.48 ms | 1.405x |
| Peak RSS | 290,533,376 B | 524,771,328 B | 0.554x |
| Artifact size | 31,010,200 B | n/a | n/a |
| Model load | 1053.25 ms mean | 865.18, 608.77 ms | n/a |

**Release measurement result:** all measured commands acquired `/tmp/tsc.lock` under `flock -w 1800` and completed. The standardized sweep on `target_1272.wav` plus `other_2035.wav` ran with `repeat 2` and threshold 0.75, and its per-command outputs are in `/tmp/gate2-evidence/hardening/`. The scriptc artifact is 31,010,200 B, below the 150,000,000 B cap. `ldd` confirms no `libonnxruntime.so` runtime path; the artifact needs only host libm, libstdc++, libc and libgcc_s on Linux x86-64/glibc. Runtime ratio is 1.405x against the release native probe, within the criteria v2 1.5x cap and below the absolute 1000 ms/window limit. RSS is informational under criteria v2.

### Corrected sweep and amended verdict

The standardized sweep was rerun to completion with release artifacts (enrollment plus `target_1272.wav` and `other_2035.wav`, `repeat 2`, threshold 0.75), reusing the release native probe output captured earlier in the same evidence directory. Exact native parity holds in release mode: 156 windows compared, 0 raw mismatches, 0 vote mismatches and 4/4 FNV window digests matching. Repeated scriptc runs are deterministic across repeats, the native probe reports `deterministic: true`, and the profile/enrollment/roundtrip path is unchanged. With one target and one other-speaker fixture, the corrected counts are 78/78 target and 4/78 other under the app vote gate, with raw 60/78 and 0/78.

Amended Gate 2 scriptc verdict under criteria v2: **pass with constraints, release-mode verification complete**. The constraints are the build-time absolute staticlib path, the mandatory C frame wrapper for the `mutable-bytes` length mismatch, the need for a cached or vendored ORT binary when building offline, and the Linux x86-64/glibc target restriction. Release build evidence is `/tmp/opencode/g2srel/build.log`, the release artifact is `/tmp/voce-gate2-scriptc-native-release` and the release native probe is `native/target/release/voce-native-probe`; the completed sweep and all per-command outputs are in `/tmp/gate2-evidence/hardening/`. The machine summary is `/tmp/voce-gate2-scriptc-hardening-summary.json`.
