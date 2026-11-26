# Native shim real backend - lane A2

Date: 2026-11-26. Branch: `voce-spike-native-shim`. C ABI version 1, unchanged.

## What landed

- `native/src/real.rs`: a real WeSpeaker speaker-embedding backend through
  ONNX Runtime (`ort =2.0.0-rc.11`, features `std`, `download-binaries`,
  `tls-rustls`) with the same 80-bin kaldi-native-fbank pipeline as the
  desktop app's `src/model/wespeaker.rs`: hamming window, no dither, no
  energy, 16 kHz, per-utterance mean subtraction, 256-dim output,
  L2-normalized. Enrollment averages overlapping 3 s windows at 1.5 s hop
  exactly like the mock; silence passes with similarity 0; cosine scoring
  against the threshold is unchanged.
- Backend selection is explicit through the unchanged `voce_engine_load_models`:
  - `mock://builtin` or any path whose last non-empty segment is `mock` keeps
    the deterministic mock backend (all existing ABI tests use this).
  - Any other path is resolved to an ONNX file: a file is used directly; a
    directory must contain exactly one `embedder.onnx` or one `*.onnx`.
  - Real load failure returns `VOCE_ERR_IO` and leaves the engine usable.
- `voce-native-probe` gained `--backend real|mock` and `--models-dir`; every
  report is tagged with the backend and includes `model_load_ms`.
- `native/tests/real.rs` adds real-inference tests gated on `VOCE_REAL_MODEL`;
  they skip (pass) when the variable is unset so model-less CI stays green.

No ABI change was required: the mock remains fully available for ABI tests
and the real backend preserves every documented v1 semantic.

## Model facts

| fact | value |
|---|---|
| file | `wespeaker_en_voxceleb_resnet34.onnx` (25.3 MB) |
| source | https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-recongition-models |
| architecture | WeSpeaker ResNet34, VoxCeleb, 16 kHz, 80-bin fbank |
| embedding | 256-dim, L2-normalized by the shim |
| license | Apache-2.0 (wespeaker upstream; sherpa-onnx redistribution) |
| ONNX input | `feats`, shape `[1, T, 80]`, dynamic frame count |
| ONNX output | `embs`, 256 floats |

The desktop app's model (`voxblink2_samresnet34_ft.onnx` from
`wenet.org.cn/downloads`) was NOT obtainable from this environment: the URL
returns an HTML single-page app instead of the file, and TLS certificate
verification fails. HuggingFace requires a token (401). The sherpa-onnx
GitHub release asset worked without authentication.

sherpa-onnx can load this model class directly (these exports are produced
for it); this shim calls ONNX Runtime directly with the same fbank front end
instead, which keeps the dependency tree small and preserves the app's exact
feature pipeline. A sherpa-onnx C API integration remains an option if
packaging simplicity outweighs direct control.

## Exact commands

From `native/`:

```sh
# everything except real-inference tests (no model needed)
cargo test

# with the real model
curl -L -o /tmp/wespeaker_en_voxceleb_resnet34.onnx \
  https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/wespeaker_en_voxceleb_resnet34.onnx
VOCE_REAL_MODEL=/tmp/wespeaker_en_voxceleb_resnet34.onnx cargo test

# probe, mock vs real on the same fixtures
cargo build --release
./target/release/voce-native-probe --backend mock \
  --fixture-dir /tmp/voce-real-fixture --repeat 2 --out /tmp/voce-probe-mock.json
./target/release/voce-native-probe --backend real \
  --models-dir /tmp/wespeaker_en_voxceleb_resnet34.onnx \
  --fixture-dir /tmp/voce-real-fixture --repeat 2 --out /tmp/voce-probe-real.json
```

Fixture contract is unchanged: a directory with `enroll.wav` plus other
`*.wav` files, or a `manifest.json`.

## Measurements (release build, 6-core Linux x86_64, synthetic 3-4 s tone fixtures)

| metric | mock | real |
|---|---|---|
| embedding dimension | 256 | 256 |
| same-speaker similarity | 1.0 | 1.0 |
| other-speaker similarity (tones) | 0.004 | 0.62-0.82 |
| per-3 s-window runtime | 0.28 ms | 77-83 ms (~39x realtime) |
| peak RSS | 9.2 MB | 153-166 MB |
| process wall time (probe) | 9 ms | 640 ms |
| `voce_engine_load_models` | ~0 | 177-235 ms |
| repeated-run determinism | bit-identical | bit-identical |
| profile JSON roundtrip | ok | ok |
| debug build per-window | ~5 ms | ~280 ms |

ORT is loaded once per process; the reported model-load time includes session
creation and optimization. RSS is dominated by the ONNX Runtime arena.

## Remaining parity gaps against the current Rust eval path

1. **VAD.** The app uses voxudio's neural VAD (512-sample chunks, mean speech
   probability). The shim uses the RMS heuristic for `speech_probability`.
   Probabilities are not comparable; gate decisions on real speech may differ
   near silence boundaries. Loading the voxudio VAD behind the same ABI is
   the natural next step if numbers must match.
2. **Embedding model.** ResNet34-VoxCeleb vs voxblink2-SimAMResNet34-ft:
   similarity distributions differ, so the 0.75 threshold needs a re-sweep on
   real speech before any gate-decision parity claim. Embedding drift vs the
   app cannot be measured until the voxblink2 ONNX is obtainable.
3. **Execution provider.** The app uses CoreML on macOS; the shim uses CPU EP
   on Linux. Runtime numbers are not comparable across platforms.

## Next Gate 1 command

After LibriSpeech fixtures are staged (eval lane), run the shim against the
same enrollment/test utterances the Rust eval uses and diff per-window
decisions:

```sh
cd native && cargo run --release --bin voce-native-probe -- \
  --backend real --models-dir /path/to/wespeaker_en_voxceleb_resnet34.onnx \
  --fixture-dir ../eval/fixtures/librispeech-smoke --repeat 2 \
  --out /tmp/gate1-real.json
```

Gate 1 parity is claimed only when: per-window gate decisions match the Rust
eval on the same utterances (or differ only inside a documented threshold
re-sweep), VAD probabilities match (or the gap is documented), and runtime /
RSS / determinism numbers above are reproduced on real speech.
