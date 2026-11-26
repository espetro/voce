# Scriptc backend against the real ONNX backend (lane B3)

Date: 2026-11-26 · Branch: `voce-spike-scriptc-backend` · scriptc **v0.4.16** ·
Rust 1.99.1 · `voce_native.h` **ABI v1, unchanged**.

Question: after lane B2 proved the scriptc backend can drive the full stateful
ABI through the thin staticlib wrapper, does the same path work against lane A2's
real WeSpeaker/ONNX backend, and is scriptc viable as the production
orchestration backend under the process/sidecar-first strategy?

Status: **landed**. Verdict at the end.

## Deliverable 1: merge and build

```sh
cd /home/own-agents/.worktrees/voce-backend-spikes/scriptc-backend
git merge --no-ff origin/voce-spike-native-shim \
  -m "Merge remote-tracking branch 'origin/voce-spike-native-shim' into voce-spike-scriptc-backend"
```

The auto-merge was clean, with **no conflicts**. `a001580` merged lane A2 at
`0497688` into lane B2 at `3bc61ba`. The frozen header, the wrapper
`native/voce_native_scriptc.c`, the `staticlib` crate type and all scriptc FFI
files were preserved. `native/Cargo.toml` keeps:

```toml
crate-type = ["cdylib", "rlib", "staticlib"]
```

and gains `ndarray`, `kaldi-native-fbank` and pinned `ort =2.0.0-rc.11`
(`std`, `download-binaries`, `tls-rustls`). No ABI symbol or signature changed;
backend selection is encoded in the existing `voce_engine_load_models(path)`
string argument.

Baseline before the merge (`/tmp/voce-scriptc-b3-baseline-tests.log`):

```sh
flock /tmp/tsc.lock cargo test --manifest-path native/Cargo.toml
```

6 unit + 7 ABI integration tests passed, 0 failed.

Merged tests with the real model (`/tmp/voce-scriptc-b3-native-tests-merged.log`):

```sh
flock /tmp/tsc.lock env \
  VOCE_REAL_MODEL=/home/own-agents/.cache/voce/models/wespeaker_en_voxceleb_resnet34.onnx \
  cargo test --manifest-path native/Cargo.toml
```

6 unit + 7 ABI + 2 gated real-inference tests passed, 0 failed. The only warning
is pre-existing `unused const GATE_BLOCK` in `native/tests/real.rs`. The real
model checksum matches the assignment:
`5ef208a9da1453335308a6b6f4e6dfbd7e183a38b604de0a57664f45d257fe94` (sha256).

### Does the real backend work through the static archive?

Yes. `backends/scriptc/ffi/build-wrapper.sh` keeps the C11 wrapper and the Rust
`staticlib` as separate archives, and `scriptc build` links both into one
executable. The scriptc probe loaded the ONNX file through wrapper op 1
(`LOAD_MODELS`), enrolled, scored and exported a profile whose JSON says
`"backend":"real"` with `dim: 256`. No `libvoce_native.so` path is baked into
the executable. The mock backend still works with the same binary via
`--backend mock` (5/5 target pass, 0/5 other pass on tone fixtures), so lane B2
parity and mock support are preserved.

### Artifacts and build times

| item | value |
|---|---|
| `native/target/debug/libvoce_native.a` | 195,261,712 bytes (186 MiB) |
| `backends/scriptc/ffi/libvoce_native_scriptc.a` | ~5.9 KiB |
| `scriptc build` executable | 68,271,176 bytes (65.1 MiB) |
| cargo cold compile + all tests (merged) | 34,113 ms |
| wrapper rebuild (cargo dev 0.11 s + clang + ar) | 31,094 ms |
| scriptc build, first attempt, failed link | 40,562 ms |
| scriptc build after manifest fix | 4,884 ms |
| scriptc bench process on real-speech fixtures (`wall_ms`) | 8,395 ms |

The first scriptc link failed with undefined `std::chrono`, `operator new` and
Abseil C++ symbols. Adding `stdc++` to `system_libraries` in
`backends/scriptc/ffi/voce_native_scriptc.json` fixed it (commit `51aa80b`).

### Dynamic-linking requirements and ONNX Runtime deployment

`ldd /tmp/voce-scriptc-native-real`:

```text
linux-vdso.so.1
libm.so.6 => /lib/x86_64-linux-gnu/libm.so.6
libstdc++.so.6 => /lib/x86_64-linux-gnu/libstdc++.so.6
libc.so.6 => /lib/x86_64-linux-gnu/libc.so.6
/lib64/ld-linux-x86-64.so.2
libgcc_s.so.1 => /lib/x86_64-linux-gnu/libgcc_s.so.1
```

There is **no** runtime `libvoce_native.so` and **no** `libonnxruntime.so`:
ONNX Runtime and its C++ objects are inside the static archive. Deployment
still needs the host's `libstdc++.so.6` and `libgcc_s.so.1`, and the binary is
Linux x86_64/glibc-specific. `ort`'s `download-binaries` feature fetches the
prebuilt ORT binary at **build time**, so an offline build needs a pre-populated
ORT cache or a vendored native library. The ONNX model is *not* embedded; it is
passed at run time as a file or directory.

## Deliverable 2: real-backend scriptc probe

`backends/scriptc/src/cli_native.ts` now accepts backend selection on every
command and forwards it to `VoceNativeAdapter(modelsPath)`; the ABI and wrapper
are untouched:

```sh
./voce-scriptc-native bench --backend mock --repeat 2
./voce-scriptc-native bench --backend real \
  --model /home/own-agents/.cache/voce/models/wespeaker_en_voxceleb_resnet34.onnx \
  --enroll enroll.wav --input target.wav --other other.wav --repeat 2
./voce-scriptc-native run --backend real \
  --models-dir /tmp/voce-b3-model-dir --enroll enroll.wav --input target.wav
```

`--model` takes the ONNX file directly; `--models-dir` takes a directory with
one `embedder.onnx` or one `*.onnx`. Both were exercised. Defaults remain mock
(`mock://builtin`), and invalid combinations (`real` without a path, `mock` with
a path, both flags) are rejected before any FFI call.

Fixture sets, all under `/tmp` (no fixtures committed):

1. `/tmp/voce-scriptc-fixtures`: deterministic tones from lane B2, 6 s enroll,
   10 s target and other, 192,044 / 320,044 / 320,044 bytes.
2. `/tmp/voce-b3-speech`: 6 s enrollment and 10 s target sliced from
   `spk_5895.wav` at 10 s / 30 s, and 10 s other speaker sliced from
   `spk_777.wav` at 20 s (existing local speech WAVs, 16 kHz mono PCM16,
   same sizes as set 1). Manifest:
   `{"enroll":["enroll.wav"],"inputs":["target.wav","other.wav"]}`.

Exact real probe commands:

```sh
flock /tmp/tsc.lock sh backends/scriptc/ffi/build-wrapper.sh
flock /tmp/tsc.lock scriptc build backends/scriptc/src/cli_native.ts \
  --ffi backends/scriptc/ffi/voce_native_scriptc.json \
  -o /tmp/voce-scriptc-native-real --no-keep-llvm

flock /tmp/tsc.lock /tmp/voce-scriptc-native-real bench \
  --enroll /tmp/voce-b3-speech/enroll.wav \
  --input /tmp/voce-b3-speech/target.wav \
  --other /tmp/voce-b3-speech/other.wav \
  --backend real \
  --model /home/own-agents/.cache/voce/models/wespeaker_en_voxceleb_resnet34.onnx \
  --repeat 2

flock /tmp/tsc.lock native/target/debug/voce-native-probe \
  --backend real \
  --models-dir /home/own-agents/.cache/voce/models/wespeaker_en_voxceleb_resnet34.onnx \
  --fixture-dir /tmp/voce-b3-speech --repeat 2 \
  --out /tmp/voce-scriptc-b3-native-speech-probe.json
```

### Validation matrix

| check | evidence |
|---|---|
| create/destroy | `vwsCreate` returns a session id, `vwsDestroy` clears the wrapper slot; `bench` creates and closes one adapter per repeat and `roundtrip` closes two; all runs exit 0. Wrapper capacity is 16 concurrent sessions. |
| `load_models` | Real file and `--models-dir` symlink directory both returned 0 and produced 256-dim profiles tagged `"backend":"real"`. Mock sentinel still returns 0. |
| enrollment | 6 s audio -> 3 overlapping 3 s windows at 1.5 s hop; profile JSON is 5,414 bytes with 256 floats. |
| `score_window` | Adapter enforces exactly 48,000 samples; the real ABI path is exercised through wrapper op 8 in both `run` and `bench`. |
| `process_pcm` capacity protocol | Adapter queries total with NULL results (op 9), allocates caller-sized result frames (op 10), retries on `-4`, and checks written == capacity. 10 s of audio yields 5 windows. |
| profile JSON roundtrip | `roundtrip --backend real ...` reports `score_digest_equal: true`; before/after target both pass 4/5 with identical similarities. Native ABI tests also cover roundtrip. |
| handle close | Every command calls `adapter.close()` exactly once per created adapter; no stale-session errors across repeated commands in the same probe binary. |
| repeat determinism | `bench --repeat 2` reports `deterministic: true`; profile JSON and both window digests match across repeats. Two separate process runs were also compared (see final verification). |
| target pass | Real speech: 4/5 windows pass at threshold 0.75 (similarities 0.680606, 0.783674, 0.793200, 0.791887, 0.800050). Tone fixture: 5/5 pass. |
| other-speaker block | Real speech: 0/5 windows pass, all similarities 0. Tone fixture: 2/5 block; the synthetic 950 Hz tone is confused with the 210 Hz enrollment (0.67-0.80 similarity), so real-speech fixtures are the gate evidence. |

### Parity with the Rust native probe on identical fixtures

Similarities, speech probabilities, decisions and FNV-1a digests are bit-for-bit
identical between the scriptc staticlib path and the Rust `rlib` probe. Runtime
is excluded from digests, as in the native probe.

| input | scriptc target | native target | scriptc other | native other |
|---|---|---|---|---|
| window count | 5 | 5 | 5 | 5 |
| passed windows | 4 | 4 | 0 | 0 |
| similarities | 0.680606, 0.783674, 0.793200, 0.791887, 0.800050 | identical | 0, 0, 0, 0, 0 | identical |
| FNV digest | `41cc167339af6ea9` | `41cc167339af6ea9` | `d936097bb929952b` | `d936097bb929952b` |
| mean ms/window | 286.29 / 291.56 | 364.99 / 336.42 | 329.94 / 305.18 | 410.75 / 335.99 |
| RSS | 107,472 KiB | 164,638,720 bytes | same process | same process |
| profile JSON bytes | 5,414 | 5,414 | - | - |

Scriptc process wall for the two-repeat bench was 8,395 ms; native probe RSS
peak was 164,638,720 bytes (~157 MiB). The extra native RSS is expected from
the separate `rlib`/test-linked process; both use the same ONNX session and
embedding code.

## Fences and workarounds

1. **Pointer-to-length outputs** (from B2, still binding): scriptc `mutable-bytes`
   lowers to `(ptr, byte_length)`, so native `int64_t *` outputs receive the byte
   length as an address and segfault. The C frame wrapper remains mandatory.
2. **Byte length vs element count**: `frame_len / 4` is converted to `n_samples`
   in wrapper ops 4, 8, 9 and 10. Direct FFI passed 192,000 instead of 48,000
   and returned `-2`.
3. **C++ runtime link** (new in B3): the real staticlib pulls in Abseil/ORT C++
   objects; `stdc++` must be in the FFI manifest. Symptom: undefined
   `std::chrono`, `operator new`, `std::__cxx11::basic_string` symbols.
4. **ABI v1 sentinel selection**: a path whose last segment is `mock`, or
   `mock://builtin`, selects the mock; any other path must resolve to exactly
   one ONNX file. A failed real load returns `VOCE_ERR_IO` and leaves the
   engine usable, so callers can fall back.
5. **Debug artifact size**: 186 MiB staticlib and 65 MiB executable. A release
   staticlib was not measured here; this is a packaging cost, not a correctness
   issue.
6. **ORT deployment**: static embedding removes runtime `.so` path problems but
   requires build-time download/cached binaries and a target-matched glibc
   build; `libstdc++.so.6` / `libgcc_s.so.1` remain dynamic requirements.
7. **Synthetic tone confusion**: the 210 Hz / 950 Hz tone fixtures do not give
   clean other-speaker blocking with ResNet34-VoxCeleb (3/5 pass at 0.75).
   Real-speech excerpts give 0/5; use those for gate decisions.
8. **Shared build lock**: `/tmp/tsc.lock` is also used by other lanes. A first
   native-probe attempt spent 600 s waiting for another session's cargo test;
   all final runs below were timed *after* acquiring the lock.
9. **Probe details**: `/usr/bin/time` is absent on this host, so elapsed times
   use a nanosecond `date` clock; the built executable prepends a `scriptc`
   argv marker, which the CLI strips; artifact size is read via
   `/proc/self/exe`.

## Revised Gate 2 verdict

**Viable as a process/sidecar orchestration backend, with a thin reviewed C
adapter; not yet a drop-in in-process production backend.** The real ONNX
engine works through the static archive with exact native parity, deterministic
repeats, functioning profile roundtrip, target pass and real-speaker block
behavior, and no ABI v1 change. Those are the properties Gate 2 needs from a
sidecar: scriptc can own enrollment/window orchestration and the stateful call
sequence while the heavy inference stays in one reviewed native library.

The remaining constraints are packaging and fidelity, not control flow: a 65 MiB
debug executable and C++ runtime dependency, build-time ORT binary acquisition,
the mandatory wrapper for unrepresentable pointer-to-length and element-count
arguments, no neural VAD in v1, and a different embedding model from the
desktop app. Production adoption should keep the sidecar process boundary,
measure a release staticlib, vendor or pin the ORT binary per target platform,
audit the 13-op frame wrapper against `voce_native.h` in CI, and defer any claim
of gate-decision parity with the app until VAD and embedding-model parity are
addressed.
