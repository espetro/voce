# Scriptc Backend Spike (Lane B)

Date: 2026-11-26 · Branch: `voce-spike-scriptc-backend` · scriptc **v0.4.16**

Question: can the scriptc TypeScript→native compiler produce a deterministic,
self-contained CLI that implements the Voce shim contract (enroll / window /
score / gate / serialize) against a narrow adapter, with a clean swap path to a
real `voce_native` FFI shim?

**Answer: yes**, with one significant FFI fence (see §FFI fences). Full static
compilation was achieved; no dynamic-tier code in the output.

Lane B2 extends this to the real stateful `voce_native` ABI; see
[`scriptc-native-integration.md`](scriptc-native-integration.md) for the
verdict and comparison.

## Layout

```
backends/scriptc/
  src/types.ts            # contract: SAMPLE_RATE=16000, WINDOW_SAMPLES=48000,
                          # MOCK_EMBEDDING_DIM=64, NativeShim, Profile, EngineMetrics
  src/mock_shim.ts        # deterministic embedding (log-spaced band energies, unit-norm)
  src/engine.ts           # enroll accumulation, processPcm windowing, vote-window gate,
                          # profile serialize/load, BenchClock (const clock → determinism)
  src/audio_io.ts         # 16-bit PCM WAV parser (mono/stereo downmix), raw f32, synth fixture
  src/cli.ts              # bench | enroll | run; JSON metrics incl. RSS (VmHWM)
  src/native_ffi_shim.ts  # FFI-backed shim (cosine via native stub)
  src/cli_ffi.ts          # FFI probe: validates cosine +1/+1/−1 through the native stub
  ffi/voce_native_stub.c/.json  # minimal archive: voce_emb_dim(), voce_similarity(dim,a,b)
  voce-scriptc            # main exe   (1,195,488 B)
  voce-scriptc-ffi        # FFI probe  (611,328 B)
```

## Build commands

```sh
# static C stub for FFI probe
clang -c ffi/voce_native_stub.c -O2 -o ffi/voce_native_stub.o
ar rcs ffi/libvoce_native_stub.a ffi/voce_native_stub.o

# main CLI (no FFI)
flock /tmp/tsc.lock scriptc build src/cli.ts -o voce-scriptc --no-keep-llvm

# FFI probe
flock /tmp/tsc.lock scriptc build src/cli_ffi.ts \
  --ffi ffi/voce_native_stub.json -o voce-scriptc-ffi --no-keep-llvm

# fast iteration (no exe)
scriptc coverage src/engine.ts
```

## Measurements

| Metric | Value |
|---|---|
| First build (cold cache) | ~2 min |
| Incremental build | ~1.3 s |
| Artifact size (main) | 1,195,488 B |
| Artifact size (FFI probe) | 611,328 B |
| Wall time, 10 s bench (3 windows) | ~1.5 s |
| Per-window score time | sub-ms (integer ms clock reports 0) |
| RSS (VmHWM) | ~4.4 MiB |
| Static tier coverage | 100% (0% dynamic) |

### Determinism

Byte-identical outputs across runs. Achieved by injecting a `BenchClock`
const-clock stub into the engine instead of `Date.now()`. The only
non-deterministic field is `rss_kib` (kernel VmHWM accounting).

## FFI fences (critical)

Scriptc FFI (manifest format 6 and 7 both work) has sharp limits:

1. **Only the first `bytes` argument in a call may be dereferenced.** Deref of a
   second byte-span argument in the same call segfaults.
2. A trailing scalar passed after two byte spans arrives **corrupted** (constant
   value, not the passed value).
3. Looping over an index variable inside the native function crashes.

**Working pattern** (`src/native_ffi_shim.ts`): pack `[window ‖ profile]` into
one concatenated byte span and pass it as *both* byte args; the C side pins two
region pointers (`pa`, `pb2 = pa + half`) and computes f32 cosine across the
split. Validated: exactly +1.0 for identical unit vectors, −1.0 for opposites.

Additional notes:

- Params must be `bytes` (not `mutable-bytes`) for read-only spans.
- All manifest functions must be declared in TS even if unused.
- Type-only imports must be written `import { type Foo }` — scriptc strips
  plain imports of types at runtime.
- `scriptc run file.ts args` does not forward args; build an exe and run it.
- `process.argv` in the built exe has an extra `scriptc` prefix at argv[1] — shift it.

## Adequacy for real voce_native

- **Scoring path**: yes — the concat-span workaround suffices for cosine
  similarity against a real native implementation.
- **Embedding path**: the FFI shim's `embed()` currently throws; a real
  `voce_native` would own embedding, so the real shim's ABI would define it.
  Expect the same single-byte-span fence to apply when marshalling 3 s windows.
- Host callbacks were not needed; direct FFI was sufficient for this spike.

## Scope fences respected

No changes to the Rust app, panel, macOS driver, eval harness, or the native
lane. All code confined to `backends/scriptc/`.
