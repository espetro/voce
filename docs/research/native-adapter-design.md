# Narrow native adapter design

Date: 2026-11-26. Branch: `voce-spike-native-adapter` (from `voce-spike-gate1-eval`).

## Why this exists

The Gate 2 scriptc and pyscriptc lanes both drove the real `voce_native` C ABI,
but each had to invent its own marshalling layer:

- `voce-spike-scriptc-backend:native/voce_native_scriptc.c` introduced session
  ids, an op dispatcher, byte frames, a session table, byte-to-f32 conversion,
  and the native capacity-query protocol because scriptc `mutable-bytes`
  lowers to `(ptr, byte_length)` and passes the length as the address of an
  `int64_t *` output parameter (SIGSEGV on `finish_enrollment`).
- `voce-spike-pyscriptc-backend:backends/pyscriptc/voce_native_ffi.c`
  introduced f64 PsSeq-to-f32 copies, per-output capacity queries, a hand
  derived `process_pcm` window count, JSON PsBuf copies, and an NUL-terminated
  path allocation because pyscriptc cannot express the native output pointers.

Both wrappers encoded the same protocol decisions. This adapter moves those
decisions into one small C compilation unit with a reviewed header, so each
backend binds only three symbols and no longer reimplements native semantics.

## Surface

| Symbol / op | Inputs | Output | Contract |
| --- | --- | --- | --- |
| `voce_adapter_create()` | none | positive session id or negative status | Owns one `VoceEngine`; 16-slot table |
| `voce_adapter_destroy(session)` | session id | none | Consumes exactly once; dead/invalid id is a no-op |
| `voce_adapter_frame(session, op, frame, frame_len)` | borrowed byte frame + byte length | `VOCE_OK` or negative status | Dispatches the ops below; never retains `frame` |
| 1 `LOAD_MODELS` | NUL-terminated path bytes | status | Delegates backend selection to native |
| 2 `SET_THRESHOLD` | 4-byte f32 | status | Delegates range validation to native |
| 3 `RESET_ENROLLMENT` | empty | status | Drops profile and buffered audio |
| 4 `ENROLL_PCM` | f32 byte span | status | Converts bytes to f32 element count |
| 5 `FINISH_QUERY` | 8-byte prefix | required f32 count | Native NULL-data query; no state change |
| 6 `FINISH_ENROLLMENT` | `[capacity i64][f32 * capacity]` | prefix = f32 count written | Finalizes enrollment; `-4` updates required capacity |
| 7 `EMBEDDING_DIM` | 8-byte prefix | i64 dimension | Fixed scalar output, no query needed |
| 8 `SCORE_WINDOW` | `[f32 PCM][24-byte result]` | trailing fixed result struct | f64 runtime, f32 similarity, f32 speech, i32 decision |
| 9 `PROCESS_QUERY` | `[8-byte prefix][f32 PCM]` | prefix = complete windows | Uses real PCM; a NULL PCM query is invalid in native |
| 10 `PROCESS_PCM` | `[capacity i64][f32 PCM][24-byte result * capacity]` | prefix = written/required count | Copies into aligned native buffers and back; `-4` leaves payload untouched |
| 11 `SERIALIZE_QUERY` | 8-byte prefix | required JSON bytes incl. NUL | Native NULL-data query |
| 12 `SERIALIZE_PROFILE_JSON` | `[capacity i64][JSON bytes]` | prefix = bytes written excl. NUL | Caller capacity includes room for NUL |
| 13 `DESERIALIZE_PROFILE_JSON` | JSON bytes | status | Explicit byte length passed through, no NUL required |

The complete per-function and per-op contract is in `native/voce_adapter.h`.
Status codes are the existing `VoceStatus` values; a dead session maps to
`VOCE_ERR_INVALID_STATE` (-3) and an unknown op to `VOCE_ERR_UNSUPPORTED` (-7).

## Ownership and layout rules

- The adapter owns the engine behind an integer session id. `create`
  transfers ownership to the caller; `destroy` consumes it. No backend ever
  receives a raw `VoceEngine *`, so stale handles cannot be dereferenced.
- All frame memory is caller-owned and borrowed for one call. The adapter
  never returns a pointer, never exposes its allocator, and allocates only
  temporary aligned conversion buffers that it frees before returning.
- Variable-size outputs are length-prefixed: the first eight bytes are a
  little-endian i64 capacity on input and a count on output. The one fixed
  struct is the 24-byte `VoceWindowResult` frame.
- All lengths on the wire are bytes; all lengths crossing into native are
  f32 or result element counts. This conversion exists only in
  `native/voce_adapter.c`.
- No callbacks, no variadics, no C++ types, no platform headers. Frames are
  little-endian and the caller supplies aligned or unaligned byte storage;
  the adapter uses `memcpy` for every multi-byte field.
- Sessions are process-global and not thread-safe. Serialize calls with a
  lock or use one process per backend.

## Backend bindings

| Backend | Binding | What it still does |
| --- | --- | --- |
| scriptc | three `declare function`s mapped to `voce_adapter_create/destroy/frame`; `mutable-bytes` carries one frame per call | Builds little-endian frames, retries on `-4`, decodes result tail |
| pyscriptc | manifest with three extern functions; a thunk converts f64 PsSeq values to frame bytes and copies outputs back into PsSeq/PsBuf | Converts f64 to f32, allocates caller-owned lists/strings |

`backends/adapter-consumers/scriptc/probe.ts` and
`backends/adapter-consumers/pyscriptc/probe.py` are the minimal consumers.
They exercise create/destroy, load models, enroll, score, process with a
capacity query, and profile serialize/deserialize using only the three
symbols.

## Review checklist

- [ ] Header and C implementation agree on every op number and byte layout.
- [ ] Every `int64_t *` native output is replaced by a length-prefixed frame
      or a fixed-width struct crossing the backend boundary.
- [ ] All element-count conversions are byte-length divisions in the adapter.
- [ ] Queries use NULL native output pointers, never NULL PCM with n > 0.
- [ ] `-4` leaves the caller payload untouched and reports required capacity.
- [ ] No native pointer escapes; no caller pointer is retained after a call.
- [ ] Session ids are range-checked; destroy is idempotent; dead sessions fail.
- [ ] No callbacks, no variadics, no C++ types, no allocator API in the header.
- [ ] ABI tests cover all probe flows and both consumer bindings compile.

## Verification

`cargo test --manifest-path native/Cargo.toml --test abi --test adapter --lib`
runs the original raw-ABI suite, the new C-adapter ABI suite, and Rust frame
codec unit tests. On this host the build needs
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0` because the root filesystem is
nearly full; `--offline` avoids registry traffic. The native crate now emits a
staticlib and `native/build.rs` copies `libvoce_adapter.a` beside
`libvoce_native.a` in `target/debug`, which is the stable archive path the
consumer manifests need. The consumer scripts build with the same flags and
then compile one probe per backend.

Baseline before the change: the raw `abi` test target could not be built on
this host because the root filesystem was full; no test assertion failed.

Observed after the change with
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --manifest-path native/Cargo.toml --test abi --test adapter --lib --offline`:
lib 9 passed / 0 failed, `tests/abi.rs` 7 passed / 0 failed, `tests/adapter.rs`
3 passed / 0 failed. The C reproducer
`backends/adapter-consumers/c_reproducer.c` links only `libvoce_adapter.a` and
`libvoce_native.a` and exercises both consumption shapes in one probe flow:
`adapter C reproducer: dim=256 windows=2 json=5792`.

The two consumer scripts use `/tmp/tsc.lock`, which another agent held for its
Gate 2 hardening sweep during this spike. The probes were therefore built and
run directly with the same flags the scripts use. The scriptc probe printed
`{"dim":256,"similarity":1,"windows":2,"restored_similarity":1,"json_bytes":5792}`.
The pyscriptc probe printed `256`, `2`, `1111`, `0`, `0` for embedding dimension,
complete windows, JSON bytes, score similarity bits and restored similarity
bits. Both bind only `voce_adapter_create`, `voce_adapter_destroy` and
`voce_adapter_frame`.
