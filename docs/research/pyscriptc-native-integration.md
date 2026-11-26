# pyscriptc native ABI integration findings

Date: 2026-11-26. Lane: C2 of the Voce backend spike program. Branch: `voce-spike-pyscriptc-backend`. Scope was `native/Cargo.toml`, `backends/pyscriptc/`, and this document. No macOS HAL driver code and no Rust app logic outside `native/` was touched. The native lane merge `112aad2` was already present locally and was not merged again.

## Verdict

Verdict: viable with constraints.

pyscriptc extern FFI can drive the real stateful `voce_native` C ABI v1 end to end. A typed adapter creates and destroys engines, loads models, sets threshold, resets and enrolls, queries embedding capacity, finishes enrollment, scores one window, processes a batch with caller-sized output buffers, and serializes/deserializes profile JSON. The native-backed probe matches the Rust `voce-native-probe` exactly on similarity and decision values for the same fixture, and it is deterministic across repeated runs and across an enrolled-profile roundtrip on a second engine.

It is not production-viable as a drop-in replacement yet. The working route needs a Rust staticlib, a C thunk layer that converts pyscriptc `list[float]` cells to native `f32`, a staged copy of the archive beside the manifest, and explicit ownership discipline for opaque handles. pyscriptc currently compiles a single closed module, so adapter classes had to live in the entry file. All measurements here are debug-build measurements against the native lane's mock embedding engine, not a real ONNX backend. The next gate is release-build timing and a process-boundary integration against a real model-backed shim.

## Build and link route

The smallest working route is staticlib plus C thunks, not a runtime cdylib load.

- `native/Cargo.toml` now declares `crate-type = ["cdylib", "rlib", "staticlib"]`. The existing cdylib and rlib outputs are unchanged, and native tests still pass.
- `backends/pyscriptc/build.sh` runs `cargo build` under `flock /tmp/tsc.lock`, copies `native/target/debug/libvoce_native.a` to `backends/pyscriptc/lib/libvoce_native.a`, then compiles the Python entry under the same lock.
- `backends/pyscriptc/lib/` is gitignored because it is a build staging directory. The copy exists because the pyscriptc manifest validator rejects paths containing `..`, so the manifest cannot point directly at `native/target/`.
- `backends/pyscriptc/voce_native.toml` declares `objects = ["lib/libvoce_native.a"]`, `source = "voce_native_ffi.c"`, and `libs = ["pthread", "dl"]`. The compiler link line consumes the archive at binary link time, so no rpath, `dlopen`, or exported-symbol visibility work is needed.
- `backends/pyscriptc/voce_native_ffi.c` is the only thunk layer. It includes `<ps_rt.h>` and `../../native/voce_native.h`, converts `PsSeq` f64 cells to native f32 PCM, copies strings into NUL-terminated buffers, writes results back with `ps_seq_set`/`ps_buf_copy`, and tracks engine open/close counts.
- `backends/pyscriptc/voce_native.pyi` gives every C thunk a typed Python signature, including `class Engine` for the opaque handle.

The handle declaration is `[[handle]] name = "Engine"`, `c_type = "VoceEngine *"`, `frees = "ps_ffi_voce_close"`. Borrowed engine arguments declare `borrows = ["engine"]`, and destroy declares `consumes = ["engine"]`. The final probe reports `open_count = 2` and `close_count = 2`, so both created engines were destroyed through the ABI.

## ABI surface covered by the adapter

`VoceNativeAdapter` lives in `backends/pyscriptc/spike.py` behind the existing adapter seam. `make_adapter()` still returns the deterministic mock, and `make_native_adapter()` wraps a caller-owned engine handle for the `--backend native` path.

| ABI operation | Python surface | Final probe evidence |
| --- | --- | --- |
| create and destroy | `voce_native.create()`, `voce_native.close(engine)` | `abi_version = 1`, `open_count = 2`, `close_count = 2` |
| load_models | adapter constructor | `status = 0` |
| set_threshold and reset_enrollment | adapter constructor, `reset()` | `status = 0`, `enrollment_status = 0` |
| enroll_pcm | `enroll([enroll_pcm])` | 48,000 samples accepted, `enrollment_status = 0` |
| embedding_dim | `dimension()` | 256 |
| finish_enrollment | capacity query, then sized `list[float]` | `profile_dimension = 256` |
| score_window | direct 4-cell result buffer | `score_window_status = 0`, similarity `0.9999999403953552`, decision 1, matches first `process_pcm` window |
| process_pcm | capacity query, then `total * 4` result cells | `process_status = 3`, 3 windows, similarities `0.9999999403953552`, `0.10259371995925903`, `0.005075631197541952` |
| serialize_profile_json | NULL capacity query, then `PsBuf` copy | `profile_json_capacity = 5770` including NUL, `profile_json_bytes = 5769` excluding NUL |
| deserialize_profile_json | explicit byte length | `restore_status = 0`, `profile_round_trip = true` |
| RSS | `/proc/self/statm` and `/proc/self/status VmHWM` | start 8364 KB, end 10640 KB, peak 10764 KB |

The capacity protocol is respected exactly. Variable-size outputs first query with a NULL data pointer and zero-ish length, the caller allocates the reported capacity, and the real call writes the count or bytes excluding NUL. PCM input is different: native rejects a NULL PCM pointer for nonzero sample counts, so batch output capacity is derived from `VOCE_WINDOW_SAMPLES = 48000` and `VOCE_HOP_SAMPLES = 24000` in the C thunk.

## Exact commands

Environment on this host uses explicit tool paths because the mise shims have no version set. All heavy compiles are under `flock /tmp/tsc.lock`.

```bash
python3 backends/pyscriptc/fixtures/make_fixtures.py /tmp/opencode/voce-fixtures
flock /tmp/tsc.lock sh -c 'cd native && cargo test && cargo build && cargo run --bin voce-native-probe -- --fixture-dir /tmp/opencode/voce-fixtures --repeat 2 --out /tmp/voce-native-probe.json'
sh backends/pyscriptc/build.sh --timings --debug=fences
backends/pyscriptc/voce_spike --input /tmp/opencode/voce-fixtures/test.wav --enroll /tmp/opencode/voce-fixtures/enroll.wav --backend native --repeat 2 > /tmp/voce-pyscriptc-native-probe.json
backends/pyscriptc/voce_spike --input /tmp/opencode/voce-fixtures/test.wav --enroll /tmp/opencode/voce-fixtures/enroll.wav --backend mock --repeat 2 > /tmp/voce-pyscriptc-mock-probe.json
```

Final `cargo test` output: 6 unit tests passed, 7 ABI integration tests passed, 0 failed, no warnings. The binary probe result is in `/tmp/voce-native-probe.json`, the native-backed pyscriptc result is in `/tmp/voce-pyscriptc-native-probe.json`, and the mock result is in `/tmp/voce-pyscriptc-mock-probe.json`.

Final `build.sh --timings --debug=fences` reported `capability: 50 native, 0 fenced, 0 island definitions of 50` and `fences: 0`.

## Compile time and artifact size

| Measurement | Result |
| --- | --- |
| Cold native cargo build, after `cargo clean` | 9,073 ms |
| Incremental native cargo build | 0.03 s to 0.04 s |
| Final pyscriptc compile total with checker | 2,133.5 ms |
| pyscriptc parse / lower / validate | 11.0 / 502.0 / 4.8 ms |
| pyscriptc emit_llvm / cc_compile / cc_link | 56.9 / 963.3 / 479.5 ms |
| pyscriptc link / check | 1,490.6 / 1,587.3 ms |
| Earlier full `build.sh` wall time, cached native build | 2.753 s |
| `backends/pyscriptc/voce_spike` | 9,382,032 bytes |
| `native/target/debug/libvoce_native.a` | 31,003,886 bytes |
| `native/target/debug/libvoce_native.so` | 7,373,944 bytes |
| `native/target/debug/voce-native-probe` | 8,615,696 bytes |
| Baseline mock-only pyscriptc binary | 558,184 bytes |

The native-backed pyscriptc binary is about 17 times the mock-only binary because it statically links the Rust shim, serde_json, and the C thunks in a debug build. It is only about 0.77 MB larger than the Rust native probe binary. Release-build size and latency were not measured in this lane.

## Behavior comparison on the same fixture

The fixture is generated by `backends/pyscriptc/fixtures/make_fixtures.py` in `/tmp/opencode/voce-fixtures`, with a 48,000-sample enroll WAV and a 96,000-sample test WAV at 16 kHz mono.

| Metric | native probe | pyscriptc native backend | pyscriptc mock backend |
| --- | --- | --- | --- |
| ABI version | 1 | 1 | not ABI-backed |
| Embedding dimension | 256 | 256 | 16 |
| Window / hop samples | 48000 / 24000 | 48000 / 24000 | 48000 / 16000 |
| Window count | 3 | 3 | 4 |
| Similarities | `0.9999999403953552`, `0.10259371995925903`, `0.005075631197541952` | identical to native probe | `1.0`, `0.9568655446368548`, `0.35462800437259673`, `0.036655150358607044` |
| Decisions | 1 / 0 / 0 | 1 / 0 / 0 | pass 1 / 1 / 0 / 0, gated false / true / true / false |
| Passed windows | 1 | 1 | 2 by raw threshold |
| ABI runtime total | 16.023045 ms, then 9.15987 ms over two probe runs | 14.735477 ms from ABI result cells | not applicable, Python-side timing only |
| Wall total | not reported by native probe | 56.426 ms | 59.917 ms |
| RSS | start 3,088,384 B, end 4,227,072 B or 4,169,728 B, peak 4,804,608 B | start 8364 KB, end 10640 KB, peak 10764 KB | 8608 KB, baseline mock binary 8124 KB |
| Profile JSON bytes | 5769 | 5769 | different shim-specific envelope |
| Profile roundtrip | true | true | true |
| Determinism | true over 2 runs | true over repeated in-process runs | true over repeated in-process runs |
| Decision digest | `3c9487fc2e9202b8` | `1960184478` | `4042626515` |
| Artifact size | 8,615,696 B | 9,382,032 B | 9,382,032 B in the new binary, 558,184 B baseline |

The native probe digest is a bit-level digest of all window output, while the pyscriptc digest is an FNV hash over the decision string, so the digest values are not directly comparable. Similarities, decisions, dimensions, profile bytes, and roundtrip status are directly comparable and match between native probe and native-backed pyscriptc.

Runtime numbers are not apples-to-apples. Native probe timing is Rust-side ABI work. The pyscriptc `runtime_ms_total` is the sum of `VoceWindowResult.runtime_ms` from the ABI, while `wall_ms_total` also includes list conversion and profile work. Mock timing is Python-side scoring with its own 16-dimensional mock algorithm.

Memory overhead is visible but bounded. The pyscriptc native peak was about 10.5 MB versus about 4.7 MB for the Rust probe. The difference is expected from the pyscriptc runtime, statically linked code, and f64-to-f32 PCM copies in the thunk.

## Fences, failures, and minimal reproducers

These were observed against pyscriptc 0.9.0 on Python 3.14.9. The final C2 source compiles with zero fences.

### PS2002 and PS1106: local module import

Reproducer: put typed `VoceNativeAdapter` and `WindowScore` in `backends/pyscriptc/voce_native_adapter.py`, then add `from voce_native_adapter import VoceNativeAdapter, WindowScore` to the entry file and compile `spike.py`.

Observed: `PS2002: voce_native_adapter is not in the closed import table`, followed by `PS1106: the annotation VoceNativeAdapter names a type that is not lowered` and 14 more annotation errors.

Fix: define both classes directly in `spike.py`. No multi-module compile path was attempted, so a single entry module remains a constraint for this spike.

### PS4011: consumes must own a local handle

Reproducer: `voce_native.close(adapter.engine)` where `adapter` holds the engine in a struct field.

Observed: `PS4011: voce_native.close consumes engine; the argument must be a handle variable this function owns, so the compiler can prove it is dead afterwards`.

Fix: create `engine` and `restored_engine` as locals in `run_native`, pass them into adapters, and call `close` on the locals after their last use. The final probe closes both before printing and reports `close_count = 2`.

### Manifest path rejection and staticlib staging

Reproducer: set `objects = ["../../native/target/debug/libvoce_native.a"]` in `voce_native.toml`.

Observed: manifest validation rejects `..` paths.

Fix: `build.sh` copies the archive into `backends/pyscriptc/lib/` and the manifest uses `objects = ["lib/libvoce_native.a"]`. The directory is gitignored and recreated by the build.

### NULL query misuse for PCM input

Reproducer: first thunk implementation called `voce_engine_process_pcm(engine, NULL, 96000, NULL, &total)`. Native `pcm_slice(NULL, 96000)` returned `VOCE_ERR_NULL_ARGUMENT`, the thunk returned `1`, the adapter allocated only 4 result cells, and the real call returned `-4` (`VOCE_ERR_BUFFER_TOO_SMALL`).

Observed JSON before fix: `process_pcm` returned an error and no windows were emitted.

Fix: the C thunk derives batch window count from `VOCE_WINDOW_SAMPLES` and `VOCE_HOP_SAMPLES`. NULL queries are valid for variable-size outputs such as embedding and JSON, not for PCM input.

### Wrong RSS source

Reproducer: call the original `getrusage(RUSAGE_SELF).ru_maxrss` thunk as the first statement of a probe on this host.

Observed: 509,544 KB at process start, inherited across `execve` and not representative of the probe.

Fix: read current RSS from `/proc/self/statm` and peak RSS from `/proc/self/status VmHWM`, matching the native lane metrics.

### Toolchain invocation

Reproducer: `flock /tmp/tsc.lock PYREFLY_BIN=/path/to/pyrefly pyscriptc compile ...`.

Observed: `flock: failed to execute PYREFLY_BIN=/path/to/pyrefly: No such file or directory`. Without `PYREFLY_BIN`, the checker prints `No module named pyrefly`.

Fix: export `PYREFLY_BIN` before invoking `flock`, as `build.sh` does.

### Carried over from the mock lane

These were recorded in `docs/research/pyscriptc-backend-spike.md` and did not reappear in C2 after the documented workarounds.

| Construct | Failure | Workaround |
| --- | --- | --- |
| `import resource` | PS2002 closed import table | RSS moved to FFI thunk |
| `Any` or `object` annotations | PS1104 | use `dict[str, float]` and `list[float]` |
| `struct.unpack` into `tuple[float, float, float]` | PS1003, got `list[float]` | decode integer bytes arithmetically |
| subclass returned from a base-annotated function | PS1003 | return the concrete type |
| `text.find(",", start)` | PS2001 attribute call not lowered | `find_from()` loop with string slices |
| `for value in list[bool]` | PS2002 | indexed `while` loop |
| slicing `bytes` with a non-ASCII byte | runtime divergence, not a fence | convert to `list[int]` before parsing |

The bytes-slicing divergence is the strongest remaining upstream report candidate. A minimal reproducer from the mock lane is `b"RIFF$\xee\x02\x00WAVEfmt "`: CPython returns `b"$\xee\x02\x00"` for `data[4:8]`, while pyscriptc returned six bytes for the same slice.

## Constraints and follow-ups

1. Keep adapter classes in the entry module until pyscriptc supports a multi-module compile workflow.
2. Keep opaque handle ownership in the entry function and pass borrowed handles into adapters. The compiler must be able to prove a consumed handle is dead.
3. Respect the ABI capacity protocol per output kind. Use NULL queries for embedding and JSON capacity, and derive PCM batch output count from window and hop constants.
4. Keep the staticlib staging step in the build. A cdylib route would need a runtime loader path and symbol visibility decisions, which this spike did not need.
5. Repeat the comparison with release builds, a real model-backed native shim, and the process boundary required by `docs/superpowers/specs/2026-11-25-backend-spikes-design.md` before claiming Gate 1 or production parity.
6. Do not share an engine across threads. The native ABI documents handles as not thread-safe.
7. Validate profile compatibility if the real embedding dimension differs from 256, either by projecting to 256 or by versioning the ABI.
