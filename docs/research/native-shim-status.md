# Native shim status - lane A

Date: 2026-11-26. Branch: `voce-spike-native-shim`. C ABI version 1.

## What is real

- `native/voce_native.h` is a plain C header: 16 kHz mono f32 PCM, opaque engine handle, status codes, typed window results, and caller-owned output buffers with explicit lengths. It has no platform or C++ dependencies and can be declared from Rust, C++, TypeScript FFI, Python ctypes/cffi, Go cgo, Zig, and Odin.
- `native/src/abi.rs` implements every exported symbol in the header using the same capacity/query protocol. Size queries never mutate engine state; undersized buffers return `VOCE_ERR_BUFFER_TOO_SMALL` without writing.
- Engine lifecycle, repeated enrollment chunks, finish enrollment, scoring, overlapping-window batch processing, threshold setting, reset, and profile JSON serialize/deserialize all work through the ABI.
- `native/src/bin/probe.rs` is a CLI fixture runner that calls only the exported ABI. Its JSON reports embedding dimension, per-window similarity/decision/speech probability/runtime, aggregate RSS, profile JSON roundtrip status, and repeated-run determinism digests.
- `native/src/wav.rs` reads 16/24/32-bit PCM or float WAV files and optionally resamples to 16 kHz for probe convenience only. The ABI itself never resamples.
- Verification: `cd native && cargo test` passes 6 unit and 7 ABI integration tests with no warnings; `cargo build` succeeds; the CLI probe is deterministic across two repeated runs. A small C11 consumer and the same source compiled as C++17 both include `voce_native.h`, link `libvoce_native.so`, enroll, score, and report `decision=1`.

## What is mock

- The 256-dimensional embedding is a deterministic spectral/envelope feature map (`native/src/engine.rs`), not WeSpeaker or any ONNX model. It mean-subtracts broadband level, then uses 128 spectral magnitude bins and 128 temporal envelope values, L2-normalized.
- Voice activity detection is an RMS threshold. Non-speech windows report similarity 0 and `VOCE_GATE_PASS`; speech windows use cosine similarity against the enrolled profile with similarity >= threshold passing.
- `voce_engine_load_models` validates a non-empty path but reads no files. No model is downloaded or loaded. There is no streaming state carried between `process_pcm` calls; only complete 3 s windows at 1.5 s hops are scored.
- Profile JSON is a shim-specific envelope (`version`, `backend`, `dim`, `enrolled`, `threshold`, `embedding`), not a WeSpeaker artifact. The ABI version is 1; changing the embedding dimension or JSON schema requires a version decision.
- RSS is Linux-only (`/proc`); other platforms report null. The reported per-window runtime is a debug-build measurement of mock feature extraction, not model inference latency.

## Exact build and probe commands

From the repository root:

```sh
cd native
cargo test
cargo build
```

Fixture directory contract: either `enroll.wav` plus one or more other `*.wav` files, or a `manifest.json` containing `{"enroll": ["enroll.wav"], "inputs": ["a.wav", "b.wav"]}` with paths relative to that directory. Single-string values are also accepted.

```sh
cargo run --bin voce-native-probe -- \
  --fixture-dir /tmp/voce-native-fixtures --repeat 2 --out /tmp/voce-native-probe.json
# or: --enroll enroll.wav --input same.wav --input other.wav --threshold 0.75
```

Measured probe result with generated 4 s 16 kHz mono tones (enroll and test at 220 Hz, other at 900 Hz): embedding dimension 256, same-speaker similarity 1.0 with `decision=1`, other similarity 0.000102 with `decision=0`, about 5 ms per 3 s window in a debug build, peak RSS about 4.5 MB, `deterministic: true` across two runs, and `profile_roundtrip_ok: true` for every input. These numbers validate ABI behavior only, not speaker-gate quality.

## ABI caveats

- All buffers are caller-owned and copied synchronously. The shim never retains pointers and exposes no allocator or callback API.
- `int64_t *in_out_*_len` is capacity on input and written count on output. For f32/struct arrays the unit is elements; for JSON it is bytes excluding the trailing NUL. A size query uses a NULL data pointer and reports required capacity, including NUL bytes for JSON.
- PCM sample counts are signed 64-bit; negative values are invalid. Zero samples permit a NULL PCM pointer. Non-finite input samples are rejected.
- A handle is not thread-safe. Use one handle per thread or serialize externally. Destroy each handle exactly once.
- Batch output contains only complete 48000-sample windows spaced by 24000 samples; trailing samples are dropped in v1.
- `VOCE_ERR_NOT_ENROLLED` is returned by scoring before enrollment. Gate decisions intentionally pass silence, matching a filter that suppresses only detected non-target speech.
- ABI v1 has no host callbacks, no progress/cancellation, and no partial-window carryover. Production integration first uses a process boundary, as required by the spike design.

## Next steps for real sherpa-onnx integration

Lane A2 landed the real ONNX backend; see `native-shim-real.md` for model
facts, measurements, and remaining parity gaps. The notes below are the
original lane-A plan, kept for context.


1. Confirm whether the current WeSpeaker ONNX model can be loaded by sherpa-onnx directly, or select a supported speaker-embedding model with comparable features. Record export/license/runtime constraints in the model research lane.
2. Implement model loading behind `voce_engine_load_models`: fbank/VAD configuration at 16 kHz, 3 s windows with 1.5 s hop, embedding extraction, and cosine gate scoring. Keep the ABI surface unchanged where possible.
3. Decide profile compatibility explicitly. If the real embedding dimension differs from 256, either pad/project to 256 for ABI v1 or introduce `VOCE_NATIVE_ABI_VERSION` 2 with a typed dimension field and update every FFI consumer.
4. Replace the mock profile JSON envelope with the real runtime state (or version it) while preserving the caller-owned buffer protocol and size-query behavior.
5. Add fixture parity against the current Rust eval path: embedding drift, VAD probability difference, gate decision parity, runtime per window, RSS, startup time, and repeated-run determinism. Do not claim Gate 1 parity until those measurements exist.
6. Package the cdylib and CLI for the process-boundary integration, then measure release-build latency before considering library embedding.
