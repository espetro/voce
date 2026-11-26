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
