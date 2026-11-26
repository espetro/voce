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

## Swap to real voce_native

Implement `NativeShim` (src/types.ts) over the real shim ABI (via `scriptc build --ffi`)
and replace `new MockShim()` in `src/cli.ts`. Nothing in engine/audio_io/CLI changes.
