# scriptc backend spike (lane B)

Typed TypeScript implementation of the Voce backend orchestration contract,
compiled to a native Linux executable with scriptc (shim-first strategy).

## Layout

- `src/types.ts` — shared contract types (16 kHz mono f32, 512-sample chunks, 48 000-sample windows)
- `src/mock_shim.ts` — deterministic mock shim behind the `NativeShim` adapter contract
- `src/engine.ts` — enrollment state, windowing, scoring orchestration, vote-window gate, profile serialization
- `src/audio_io.ts` — WAV (16-bit PCM) / raw f32 PCM loading, deterministic fixture synthesis
- `src/cli.ts` — executable entrypoint (bench / enroll / run)

## Build

```sh
cd backends/scriptc
flock /tmp/tsc.lock scriptc build src/cli.ts -o voce-scriptc --no-keep-llvm
```

## Run

```sh
./voce-scriptc bench --seconds 20
./voce-scriptc enroll --out /tmp/profile.json
./voce-scriptc run --input fixture.wav --profile /tmp/profile.json
```

## Native ABI probe

`src/voce_native_adapter.ts` drives the full stateful `voce_native` ABI through a
thin C wrapper (`native/voce_native_scriptc.c`) so scriptc FFI never has to pass
pointer-to-length outputs. Build it and the native CLI with:

```sh
flock /tmp/tsc.lock sh ffi/build-wrapper.sh
flock /tmp/tsc.lock scriptc build src/cli_native.ts \
  --ffi ffi/voce_native_scriptc.json -o voce-scriptc-native --no-keep-llvm
```

`ffi/voce_native_scriptc.json` links the C wrapper plus the Rust `staticlib`;
`stdc++` is required by the ONNX Runtime C++ objects. No runtime `libvoce_native.so`
path is baked into the executable.

Backend selection uses the unchanged v1 `load_models` path:

```sh
./voce-scriptc-native bench --backend mock --repeat 2
./voce-scriptc-native bench --backend real \
  --model /path/to/wespeaker.onnx \
  --enroll enroll.wav --input target.wav --other other.wav
# --models-dir also accepts a directory with one embedder.onnx / *.onnx
```

`run` and `roundtrip` accept the same `--backend`, `--model` and `--models-dir`
options. All ABIs, window sizes and profile formats remain v1.
