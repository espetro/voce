# pyscriptc against the real `voce_native` backend

Date: 2026-11-26. Lane: C3 of the Voce backend spike program. Branch: `voce-spike-pyscriptc-backend`. Scope: merge of the A2 real ONNX backend into the C2 pyscriptc FFI lane, real-backend probe evidence, and the revised Gate 2 verdict. No ABI change, no model binary, WAV fixture, build output, or dataset was committed.

## Verdict

Verdict: viable as a verification/orchestration probe, not as the in-process production orchestrator under the process/sidecar-first strategy.

pyscriptc drives the frozen `voce_native` ABI v1 end to end against the real ONNX backend through the same staticlib and C thunk route used for the mock backend. Every checked operation succeeded: create/destroy, load_models, threshold/reset, enrollment, embedding capacity, finish_enrollment, score_window, process_pcm capacity protocol, profile JSON roundtrip, handle close count, and repeat determinism. Window similarities and decisions match the Rust `voce-native-probe` bit-for-bit on the same fixtures, and recomputing the Rust FNV-1a digest from the pyscriptc window fields reproduces the Rust digest exactly for both fixtures.

The cost is high. The debug pyscriptc binary is 74,273,784 B and links a 195,262,032 B static archive that embeds ONNX Runtime and absl/cctz. A real 3 s window costs roughly 340 to 400 ms in the debug staticlib build, versus roughly 106 ms in the release Rust probe. The build needs explicit `stdc++` and `m` link flags, and the model is a runtime file that must be supplied on disk via `load_models`; it is not embedded in the archive. Under the process/sidecar-first strategy, the useful production shape remains a native sidecar that owns inference, with pyscriptc as a fixture harness or typed orchestrator across a process boundary. Embedding this staticlib inside a long-lived pyscriptc-owned process would buy no parity or latency advantage and would couple the shell to C++ link details.

## Merge and build

A2 tip `0497688` (`origin/voce-spike-native-shim`) was merged with `git merge --no-ff origin/voce-spike-native-shim -m "Merge remote-tracking branch 'origin/voce-spike-native-shim' into voce-spike-pyscriptc-backend"`, producing `bfeefbe`. The merge was clean. No conflict resolution was needed. `native/voce_native.h` was untouched, so ABI v1 is frozen and unchanged.

The merge preserved the C2 wiring:

- `native/Cargo.toml` keeps `crate-type = ["cdylib", "rlib", "staticlib"]`. The staticlib is required by `backends/pyscriptc/build.sh`.
- `backends/pyscriptc/voce_native_ffi.c` remains the only C thunk layer, converting pyscriptc f64 cells to native f32 PCM and converting strings and result buffers back.
- `backends/pyscriptc/voce_native.toml` still declares the opaque `Engine` handle with `frees = "ps_ffi_voce_close"` and the same extern functions. It gained only link libraries; no function signature changed.
- `make_adapter()` still returns the deterministic mock. Mock support and all native tests remain intact.

A2 contributed `native/src/real.rs`, `native/tests/real.rs`, real-backend probe flags, and `docs/research/native-shim-real.md`. Backend selection stays entirely inside the unchanged `voce_engine_load_models(path)` call: `models` or a path whose last segment is `mock` keeps the mock engine; any other file or directory selects the real ONNX engine. The pyscriptc CLI only forwards the path; it does not add a parallel ABI or a second handle type.

Native tests under the shared build lock:

```bash
flock /tmp/tsc.lock sh -c 'cd native && VOCE_REAL_MODEL=/home/own-agents/.cache/voce/models/wespeaker_en_voxceleb_resnet34.onnx cargo test'
```

Result: 6 unit tests, 7 ABI integration tests, and 2 real-backend tests passed (real tests 1.49 s). One pre-existing warning remains: `constant GATE_BLOCK is never used` in `native/tests/real.rs:19`.

The real backend works through the static archive. Both the Rust release probe and the pyscriptc binary load the model, enroll, and score with no `dlopen`, rpath, or cdylib route. The pyscriptc binary reports `status = 0` for `load_models` with an absolute ONNX file path and produces identical window outputs to the Rust probe.

## Dynamic linking and ONNX Runtime deployment

`ldd backends/pyscriptc/voce_spike` lists only:

```text
linux-vdso.so.1
libstdc++.so.6 => /lib/x86_64-linux-gnu/libstdc++.so.6
libm.so.6 => /lib/x86_64-linux-gnu/libm.so.6
libgcc_s.so.1 => /lib/x86_64-linux-gnu/libgcc_s.so.1
libc.so.6 => /lib/x86_64-linux-gnu/libc.so.6
/lib64/ld-linux-x86-64.so.2
```

ONNX Runtime itself is statically embedded in the Rust archive; there is no `libonnxruntime.so` dependency and no runtime environment variable for its loader. The `pthread` and `dl` manifest entries are folded into glibc on this host. The A2 `ort` dependency uses `download-binaries` at build time, so a build host needs the downloadable prebuilt binaries or an existing cargo cache; the resulting binary does not need them at runtime.

Deployment constraint: the model is external. `load_models` needs an ONNX file (or a directory with exactly one `*.onnx`, conventionally `embedder.onnx`) present at runtime. The probe used `/home/own-agents/.cache/voce/models/wespeaker_en_voxceleb_resnet34.onnx`, sha256 `5ef208a9da1453335308a6b6f4e6dfbd7e183a38b604de0a57664f45d257fe94`, about 26 MB. That file was not copied into the repo. The OS also needs the C++ runtime (`libstdc++.so.6`, `libgcc_s.so.1`) and `libm.so.6`; this is the new deployment requirement introduced by the real backend.

## CLI change without ABI change

`backends/pyscriptc/spike.py` gained `--backend real` and `--models-dir FILE|DIR`. `--backend native` remains the legacy ABI-mock path and still uses `models`. `--backend real` requires an explicit model path and forwards it to `voce_engine_load_models`. The report is still one JSON line with schema `voce-pyscriptc-native/1`, now with added `backend`, `models_path`, and `model_load_ms` fields. The C ABI, handle declaration, and thunk signatures are unchanged.

## Exact commands

```bash
# Merge (already applied in this branch)
git merge --no-ff origin/voce-spike-native-shim

# Native tests and release probe binary
flock /tmp/tsc.lock sh -c 'cd native && VOCE_REAL_MODEL=/home/own-agents/.cache/voce/models/wespeaker_en_voxceleb_resnet34.onnx cargo test'
flock /tmp/tsc.lock sh -c 'cd native && cargo build --release'

# pyscriptc real-backend binary (build.sh takes /tmp/tsc.lock itself)
export PYREFLY_BIN=/home/own-agents/src/pyscriptc/.venv/bin/pyrefly
sh backends/pyscriptc/build.sh --timings --debug=fences

MODEL=/home/own-agents/.cache/voce/models/wespeaker_en_voxceleb_resnet34.onnx
FIX=/tmp/voce-scriptc-fixtures

# Rust native probe on the same fixtures
flock /tmp/tsc.lock native/target/release/voce-native-probe \
  --backend real --models-dir "$MODEL" --fixture-dir "$FIX" \
  --repeat 2 --out /tmp/voce-c3-native-real.json

# pyscriptc probe, twice per fixture for determinism
for input in target other; do
  for n in 1 2; do
    flock /tmp/tsc.lock backends/pyscriptc/voce_spike \
      --input "$FIX/$input.wav" --enroll "$FIX/enroll.wav" \
      --backend real --models-dir "$MODEL" --repeat 2 \
      > "/tmp/voce-c3-ps-real-${input}-${n}.json"
  done
done
```

Shared fixtures are in `/tmp/voce-scriptc-fixtures` with manifest `{"enroll": ["enroll.wav"], "inputs": ["target.wav", "other.wav"]}`. SHA256: enroll `cfeb604a3e3c9a74800498b6566296726d4cf24816b11d3e101de2750fc6adbe`, target `4abeeec1f7d4a3efd19da62e69250c69c266734815c9ed1e4a790506788bc0cf`, other `a80054c9a2fbb2613a6dd98dd0f80293bb34495b9f90a3b4b68471ce8e9f0810`, all 16 kHz mono WAV. Enroll is 6 s, each input is 10 s, yielding 5 windows at 48,000 samples / 24,000 hop.

## Build time and artifact size

| Measurement | Result |
| --- | --- |
| Cold native debug build after `cargo clean` | 36.81 s (measured earlier in this lane, including real backend deps) |
| Cold full native + pyscriptc build after `cargo clean` | 65,125 ms (native cargo 55.00 s + pyscriptc compile/link) |
| Incremental `build.sh` wall time with `--timings --debug=fences` | 11,165 ms (shared lock contended) |
| pyscriptc parse / lower / validate | 16.5 / 637.9 / 5.8 ms |
| pyscriptc emit_llvm / cc_compile / cc_link | 83.7 / 1,282.5 / 6,475.9 ms |
| pyscriptc link / check / total | 7,818.4 / 7,947.8 / 8,656.1 ms |
| pyscriptc capability | 50 native, 0 fenced, 0 island definitions of 50 |
| `backends/pyscriptc/voce_spike` | 74,273,784 B |
| `backends/pyscriptc/lib/libvoce_native.a` (copy of debug archive) | 195,262,032 B |
| `native/target/release/libvoce_native.a` | 122,129,422 B |
| `native/target/release/voce-native-probe` | 26,523,904 B |

The pyscriptc binary is about 2.8 times the Rust release probe because it statically links the debug Rust shim, ONNX Runtime, absl/cctz, serde_json, and the pyscriptc runtime. `cc_link` dominates incremental compile time.

## Runtime and parity on identical fixtures

| Metric | Rust native probe (release) | pyscriptc real backend (debug staticlib) |
| --- | --- | --- |
| Wall time, probe process | 5.471 s including lock wait | 7.326 to 7.771 s reported internally per run; 69.142 s for four runs including lock wait |
| Model load (first engine) | not printed in this run; A2 measured 177 to 235 ms release | 238.34 to 262.19 ms |
| ABI runtime per 3 s window | 106.69 ms target, 105.62 ms other | 391.51 / 394.51 ms target (two runs), 338.99 / 397.48 ms other |
| Peak RSS | 160,944,128 B | 161,776 to 177,888 KB (about 158 to 174 MiB) |
| Embedding dimension | 256 | 256 |
| Profile JSON bytes | not printed in this report | 5,429 |
| Window count | 5 per input | 5 per input |
| Target pass behavior | 5/5 passed | 5/5 passed |
| Other-speaker behavior | 3/5 passed (tones similarity 0.67 to 0.80) | 3/5 passed, identical per-window values |
| Determinism | true over 2 runs | true over 2 runs per fixture, digest and window fields identical; only `runtime_ms` varies |
| Recomputed native FNV-1a digest | target `0268ffd8cda41a50`, other `7720ac834e25c88c` | same values, computed from pyscriptc `similarity`, `speech_probability`, and `decision` bit patterns |
| pyscriptc decision digest | not emitted | target `546870402`, other `530092816` |

Exact per-window similarities, identical in both probes:

- target.wav: `0.998910546`, `0.999825060`, `0.998821437`, `0.996697545`, `0.994030714`; decisions `1, 1, 1, 1, 1`.
- other.wav: `0.786977708`, `0.776504576`, `0.801156163`, `0.701625288`, `0.671834052`; decisions `1, 1, 1, 0, 0`.

The pyscriptc `decision_digest` is an FNV hash over the decision string only, so it is not directly comparable with the Rust digest. Recomputing the Rust algorithm from the pyscriptc window fields gives byte-identical digests, which is the parity claim used here. Cross-run equality checks must exclude `runtime_ms`, which is intentionally nondeterministic.

ABI validation evidence from each pyscriptc real run: `abi_version = 1`, `open_count = 2`, `close_count = 2`, `status = 0`, `enrollment_status = 0`, `restore_status = 0`, `process_status = 5`, `score_window_status = 0`, `score_window_matches_process = true`, `profile_dimension = 256`, `profile_round_trip = true`, `artifact_size_bytes = 74273784`. `score_window` similarity was `0.9989105463027954` on target.wav, matching the first `process_pcm` window.

The process_pcm capacity protocol remains valid with the real backend: the C thunk derives `5 * 4` result cells from `VOCE_WINDOW_SAMPLES / VOCE_HOP_SAMPLES` rather than issuing a NULL PCM query, while embedding and JSON capacity still use NULL queries.

## Fences and workarounds

- Non-reentrant lock: wrapping `build.sh` in an outer `flock /tmp/tsc.lock` times out because `build.sh` takes the same lock internally. Run `build.sh` directly, or take the lock once around manual commands. One 10-minute timeout was lost to this before the fix.
- Missing C++ symbols: the first real staticlib link failed with `PS0001` and about 98.6k undefined `std::ostream`, `std::string`, `operator new`, `__cxa_guard_*`, and `__cxa_pure_virtual` references from absl/cctz objects in `libvoce_native.a`. Fix: `libs = ["pthread", "dl", "stdc++", "m"]` in `voce_native.toml`.
- Real backend requires a path: `--backend real` without `--models-dir` is rejected with an explicit error instead of silently falling back to mock.
- Model file is external: the 26 MB ONNX file stays outside the repo and must be provisioned at runtime.
- Single closed module: local adapter classes still have to live in `spike.py` (`PS2002` and `PS1106` for local imports). Carried over from C2.
- Handle ownership: `consumes` arguments must be local handle variables, so `engine` and `restored_engine` are created and closed in `run_native` (`PS4011`). Carried over from C2.
- Manifest `..` rejection: the staticlib is staged into `backends/pyscriptc/lib/` by `build.sh`. Carried over from C2.
- NULL PCM query misuse: capacity is derived from window/hop constants. Carried over from C2.
- RSS source: current RSS comes from `/proc/self/statm` and peak from `/proc/self/status VmHWM`, not inherited `getrusage`. Carried over from C2.
- Toolchain invocation: `PYREFLY_BIN` must be exported before `flock`, not passed as an argument to it. Carried over from C2.
- Digest construction: compare native FNV-1a over f32 bit patterns, not the pyscriptc decision-string digest.
- Cargo.lock pulls `ureq` 3.5.2 even though the A2 dependency comment says it is pinned to 2.x. Noted as an observation; no dependency change was made in this lane.

## Constraints and follow-ups

1. Keep `voce_native.h` v1 frozen; backend selection must stay inside `load_models(path)`.
2. Keep the staticlib staging step and the C++ link flags in the manifest while using the real backend.
3. Do not commit the ONNX model, WAV fixtures, `lib/`, or compiled binaries.
4. Repeat gate comparisons with release pyscriptc/staticlib builds before using these latency numbers as production SLOs.
5. For the process/sidecar-first strategy, keep inference in the Rust native process and use pyscriptc for fixture-driven orchestration and ABI verification. Embedded library mode remains an optimization, not a Gate 2 recommendation.
6. Existing A2 gaps remain: RMS speech probability instead of the app's neural VAD, VoxCeleb ResNet34 instead of the app's voxblink2 embedder, and CPU execution provider instead of macOS CoreML. The other-speaker fixture is tonal, so 3/5 blocks is separation evidence, not a production false-accept claim.
