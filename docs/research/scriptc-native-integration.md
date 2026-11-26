# Scriptc backend against the real voce_native ABI (lane B2)

Date: 2026-11-26 · Branch: `voce-spike-scriptc-backend` · scriptc **v0.4.16** · native C ABI v1

Question: can the lane B scriptc backend drive the *stateful* `voce_native`
C ABI (create / enroll / finish / score / batch / JSON profile) instead of the
minimal cosine stub used to prove FFI viability in lane B?

Status: in progress. Verdict is recorded at the end when all deliverables land.

## Baseline

`cd native && cargo test`: 6 unit + 7 ABI integration tests pass, 0 failures.
`cd native && cargo build`: succeeds; `libvoce_native.so` exports all 13
`voce_*` symbols (`nm -D --defined-only`).

## Experiment 1: scriptc FFI links the Rust cdylib directly

Files: `backends/scriptc/src/probe_direct_cdylib.ts`,
`backends/scriptc/ffi/direct_cdylib.json`.

```sh
cd native && cargo build
cd ../backends/scriptc
flock /tmp/tsc.lock scriptc build src/probe_direct_cdylib.ts \
  --ffi ffi/direct_cdylib.json -o probe-direct-cdylib --no-keep-llvm
./probe-direct-cdylib
```

Output: `{"abi_version":1}`. `ldd ./probe-direct-cdylib` shows
`libvoce_native.so` loaded from the absolute build-worktree path, so direct
cdylib FFI works but bakes an absolute path into the executable; it is not a
self-contained artifact. Relative `libraries` paths are resolved from the
process CWD, not the manifest directory: `../../native/...` failed with
`ENOENT ... /backends/native/target/debug/libvoce_native.so`; an absolute path
worked.

## Experiment 2: direct stateful ABI calls, one and two byte spans

Files: `src/probe_direct_abi.ts`, `src/probe_direct_2span.ts`,
`ffi/direct_abi.json`; logs `/tmp/voce-direct-abi-build2.log`,
`/tmp/voce-direct-abi-run2.log`, `/tmp/voce-direct-2span-build.log`.

- `voce_engine_create(): pointer` (TypeScript `bigint`),
  `voce_engine_destroy(pointer): void`, and
  `voce_engine_embedding_dim(pointer, mutable-bytes): i32` all work. The
  dimension query wrote `256` into an 8-byte span; status `0`. Extra C
  arguments from scriptc's `(ptr, byte_length)` lowering are ignored by
  simple C functions under the SysV ABI.
- `voce_engine_deserialize_profile_json(pointer, bytes): i32` works: the
  scriptc byte-span length is passed as `json_len_bytes`, which matches the
  C signature.
- `voce_engine_score_window(pointer, bytes, mutable-bytes): i32` works with
  **two** byte spans in one call on the real cdylib: status `0`, and the
  24-byte result was written into the second span. This *contradicts* the
  lane B stub-based finding that a second byte span always segfaults; the
  fence was an artifact of the stub experiment or its pinned-pointer code,
  not of scriptc's FFI lowering.
- `voce_engine_enroll_pcm(pointer, bytes): i32` reveals a marshalling
  mismatch: scriptc passes the byte-span length (192000 for 48000 f32
  samples) as C `n_samples`, which the native engine reads as 192000 floats,
  runs past the buffer, and returns `VOCE_ERR_INVALID_ARGUMENT` (-2) for
  the non-finite garbage. With an 8-byte span it returns 0. There is no way
  to pass a separate element count after a byte span: the count lands in
  the argument slot C already uses for the span length.
- Every function in an FFI manifest must have a matching ambient
  `declare function` in the program, even if unused. Omitting three unused
  declarations produced `SC5002: FFI binding '<name>' cannot be resolved`.

## Experiment 3: capacity/query protocol is unrepresentable through direct FFI

Files: `src/probe_direct_lenptr.ts`, `ffi/direct_abi_lenptr.json`; logs
`/tmp/voce-direct-lenptr-build.log`.

```text
enroll: 0
before finish_enrollment
exit=139  (SIGSEGV)
```

C takes `voce_engine_finish_enrollment(handle, float *out, int64_t *len)`.
scriptc's `mutable-bytes` lowers to `(ptr, byte_length)`, so the call becomes
`(handle, out_ptr, out_byte_length, len_ptr, len_byte_length)`. The native
callee reads its `int64_t *len` argument from the integer `out_byte_length`
(1024), dereferences address 1024, and crashes. The same shape blocks
`voce_engine_process_pcm` (`in_out_results_len`, `out_windows_total`) and
`voce_engine_serialize_profile_json` (`in_out_json_capacity_bytes`). Those
functions cannot be called faithfully without an adaptation layer.

## Minimal adaptation decision

The cdylib links, so no loader workaround is needed. The blocker is
*marshalling of pointer-to-length output parameters*, plus the byte-length vs
element-count mismatch. The smallest adaptation is a thin C wrapper that
presents a scriptc-friendly `(session, op, frame)` API, uses a session table
for opaque engine handles, converts byte lengths to element counts, and
implements the native capacity/query protocol internally. Build the shim as
a `staticlib` as well so the final executable does not depend on the absolute
worktree path of a `.so`. Details and measurements follow.

## Adaptation implementation

`native/voce_native_scriptc.c` is a 248-line C11 adapter with three exports, `vws_create`, `vws_destroy` and `vws_call(session, op, frame)`. The Rust crate now also builds as a `staticlib`, so `backends/scriptc/ffi/voce_native_scriptc.json` links `libvoce_native_scriptc.a` plus `libvoce_native.a` into the final executable and no runtime `.so` path is baked in. All 13 ABI operations are reachable through the frame protocol. Queries use native's documented NULL output protocol, writes allocate caller-sized buffers, and a too-small capacity returns `-4` without writing.

`backends/scriptc/src/voce_native_adapter.ts` implements the full stateful surface behind the existing `NativeShim` seam. It caches the queried embedding dimension, performs size queries before each variable-length write, retries on `-4`, and verifies written counts. `backends/scriptc/src/cli_native.ts` exposes `run`, `bench` and `roundtrip` with the same WAV/PCM fixture loader as the mock CLI and emits mock-compatible window fields plus native runtime, dimension, decisions, RSS and artifact size.

## Comparison on the shared fixture

The fixture is three deterministic WAV files generated by `/tmp/voce-scriptc-fixtures/gen.py`: `enroll.wav` (6 s, seed 1, 210 Hz, 192,044 B), `target.wav` (10 s, seed 1, 210 Hz, 320,044 B) and `other.wav` (10 s, seed 7, 950 Hz, 320,044 B). All three backends consumed the same `enroll.wav` and `target.wav`, unless noted.

| Backend | Build | Artifact | Runtime on target | Peak RSS | Windows |
| ------- | ----- | -------- | ----------------- | -------- | ------- |
| scriptc mock (`cli.ts`) | 970 ms | 1,195,488 B | 259 ms wall | ~4.4 MiB (lane B measurement) | 3 non-overlapping, 64-dim |
| scriptc native adapter (`cli_native.ts`) | 2,188 ms | 8,123,912 B | 57 ms wall, 17.97 ms score | 7,152 KiB | 5 overlapping, 256-dim |
| native probe (`voce-native-probe`) | cargo dev, incremental | 8,615,696 B | 16.24 ms / 15.80 ms score | 5,758,976 B | 5 overlapping, 256-dim |

Build times were measured with a nanosecond wall clock around `flock /tmp/tsc.lock scriptc build ... --no-keep-llvm`; the wrapper rebuild itself took 289 ms (cargo incremental 0.02 s plus clang and ar). The large native artifact is a debug staticlib plus scriptc runtime; a release staticlib was not measured in this spike.

Per-window similarities from the native probe and the scriptc native adapter are bit-for-bit identical: 0.9981677532196045, 0.9999999403953552, 0.998175323009491, 0.9927635192871094 and 0.9838584065437317. All five windows have `decision=1` and `speech_probability=1`; native probe digest `40d7c9375eccb2a5` matches across its two repeats. The mock CLI is intentionally different: 64-dim embedding, three non-overlapping windows, similarities 0.9999562990437472 / 0.9999562193901794 / 0.999695333076635 and passed flags false / false / true. Field-for-field parity with the mock is neither expected nor a gate.

Repeated-run determinism of the scriptc native adapter was checked with `./voce-scriptc-native bench --enroll ... --input ... --other ... --repeat 2`: `deterministic=true`, both runs produced identical profile digests and identical target/other window digests. Target passed 5/5 windows and other passed 0/5 (similarities 0.06874308735132217, then 0). Profile roundtrip was checked with `./voce-scriptc-native roundtrip --enroll ... --input ...`: exported 5,585 JSON bytes, `score_digest_equal=true`, and before/after similarities and decisions matched exactly.

## FFI fences and minimal reproducers

Direct cdylib loading works but is not self-contained. `probe-direct-cdylib` prints `{"abi_version":1}`, while `ldd` reports the absolute worktree path to `libvoce_native.so`. Relative manifest library paths resolve from the process CWD: using `../../native/target/debug/libvoce_native.so` in `ffi/direct_cdylib.json` and running from `backends/scriptc` failed with `ENOENT ... /backends/native/target/debug/libvoce_native.so`; an absolute path worked. Staticlib linking removes this dependency.

Byte-span length versus element count: `voce_engine_enroll_pcm(handle, bytes): i32` passes 192000 instead of 48000 as `n_samples`, the native engine reads past the buffer and returns `-2`. Minimal reproducer: `probe_direct_abi.ts` with 48000 f32 samples of PCM. The wrapper converts `frame_len / 4` before calling native.

Pointer-to-length output parameters cannot be expressed: `mutable-bytes` lowers to `(ptr, byte_length)`, so a native `int64_t *len` receives the integer byte length as its address. Minimal reproducer: `probe-direct-lenptr.ts` prints `enroll: 0` then `before finish_enrollment` and exits `139` (SIGSEGV) at `finish_enrollment`. The same shape blocks `process_pcm`, `serialize_profile_json` and all query-capacity calls. The wrapper converts the prefix frame into a real local `int64_t`.

Missing ambient declarations are a compile-time fence: omitting an unused manifest function from `probe_direct_abi.ts` produced `SC5002: FFI binding '<name>' cannot be resolved`. Every FFI function needs `declare function`.

The `process_pcm` size query cannot pass a NULL PCM pointer with a nonzero sample count. Minimal reproducer: the first native CLI run with the shared `target.wav` failed with `process_pcm size query failed with status -1` because native `pcm_slice` returns `VOCE_ERR_NULL_ARGUMENT`. Fix: wrapper op 9 carries the PCM bytes in the frame and calls native with the real PCM pointer and NULL results.

The lane B stub finding that a second byte span always segfaults did not reproduce on the real cdylib: `voce_engine_score_window` with two spans works. It should be treated as a stub-specific artifact, not a scriptc lowering fence.

CLI artifact sizing via `statSync(process.argv[0])` returned null because the built executable prepends an extra `scriptc` argv element. The CLI now uses `statSync(readlinkSync("/proc/self/exe")).size`. This is a Linux probe detail, not an ABI issue.

## Verdict

Verdict: **viable with constraints**. The scriptc backend can drive the real stateful `voce_native` ABI through the thin C wrapper with exact native behavior, deterministic repeated runs and working profile serialization. It is not production-viable as a direct binding: the ABI's `int64_t *` outputs and native element counts are unrepresentable in scriptc FFI v0.4.16, the current artifact is a large debug staticlib, and the wrapper adds a second interface that must be kept in sync with `voce_native.h`. For Gate 2 this is a sound process-boundary comparison; a production design should either keep the wrapper as a small reviewed compatibility layer or change the ABI to use fixed-size frame structs with explicit element counts.
