# pyscriptc backend spike findings

Date: 2026-11-26. Lane C of the Voce backend spike program, branch `voce-spike-pyscriptc-backend`. Scope was `backends/pyscriptc/` and this document only. No Rust app, panel, macOS driver, eval harness, or native lane artifact was touched. The approved shim-first strategy was followed: a narrow local adapter contract with a deterministic mock, no dependency on the separate native shim worktree. Lane C2 later validated the real native ABI in this same worktree; see `docs/research/pyscriptc-native-integration.md`.

## What was built

`backends/pyscriptc/spike.py` is typed, pyscriptc-compatible Python that implements enrollment state, 48 000-sample windowing at 16 kHz with a 16 000-sample hop, scoring orchestration, raw similarity plus vote-window gate decisions, profile JSON serialization and deserialization, and batch raw f32 / WAV processing. `MockNativeAdapter` computes a repeatable 16-dimensional embedding from decimated band magnitudes and compares each window embedding to the enrolled profile with a dot product; `make_adapter()` is the only place that names the implementation, so a real `voce_native` adapter can replace it without touching orchestration.

The CLI accepts `--input`, optional `--enroll`, `--threshold`, `--vote-window`, and `--repeat`, and emits one JSON object containing embedding dimension, per-window similarity/pass/gated/runtime, total scoring time, profile and round-trip flag, peak RSS, artifact size, decision digest, and repeated-run determinism. `voce_native.toml` and `voce_native_ffi.c` are the extern FFI manifest and C thunk used for RSS; `voce_native.pyi` is the typed surface. `backends/pyscriptc/tests/voce_native.py` is a CPython-only stub used to cross-check native output against source execution.

## Exact commands

Environment on this host: the mise shim for pyscriptc has no version set, and the mise shim for zig has no version set, so the installed compiler binary and clang driver are named explicitly.

```bash
export PYSC=/opt/mise/installs/python/3.14.9/bin/pyscriptc
export PYREFLY_BIN=/home/own-agents/src/pyscriptc/.venv/bin/pyrefly
python3 backends/pyscriptc/fixtures/make_fixtures.py /tmp/opencode/voce-fixtures
sh backends/pyscriptc/build.sh --timings --debug=fences
backends/pyscriptc/voce_spike --input /tmp/opencode/voce-fixtures/test.wav --enroll /tmp/opencode/voce-fixtures/enroll.wav --repeat 2
PYTHONPATH=backends/pyscriptc:backends/pyscriptc/tests python3 backends/pyscriptc/spike.py --input /tmp/opencode/voce-fixtures/test.wav --enroll /tmp/opencode/voce-fixtures/enroll.wav --repeat 2
```

`build.sh` runs `flock /tmp/tsc.lock "$PYSC" compile backends/pyscriptc/spike.py --emit bin -o backends/pyscriptc/voce_spike --ffi backends/pyscriptc/voce_native.toml --driver clang --linker clang --call main "$@"`, so all heavy compiles share the requested lock.

## Compile and iteration time

Final build with the checker enabled reported `--timings` stages: `parse 10.3 ms`, `lower 203.4 ms`, `validate 3.0 ms`, `emit_llvm 37.3 ms`, `cc_compile 506.1 ms`, `cc_link 109.0 ms`, `link 679.3 ms`, `check 741.1 ms`, `total 976.1 ms`, and `capability: 30 native, 0 fenced, 0 island definitions of 30`. Three wall-clock rebuilds measured 1411 ms, 1512 ms, and 1680 ms with the checker, and 1315 ms, 1295 ms, and 1304 ms with `--skip-check`. The checker is the main iteration cost; source changes in this narrow spike stayed around 1.3 to 1.7 s per compile, not minutes.

Artifact size was 558 184 bytes (545 KiB) for `backends/pyscriptc/voce_spike`, measured with `stat -c '%s'`. No Python interpreter is linked: the binary uses the small pyscriptc runtime plus the one C thunk object.

## Runtime behavior

The 6 s WAV fixture contains 3 s of the enrolled 220 Hz modulated tone followed by 3 s of a different 700 Hz tone. With a 3 s window and 1 s hop it produces four windows: similarities around `1.0`, `0.9569`, `0.3546`, `0.0367`, with pass decisions `true, true, false, false` and vote-gated decisions `false, true, true, false` at `vote_window = 3`. This shows the mock comparing against enrollment rather than hashing windows independently. Raw f32 fixtures produce the same decision digest `4042626515`.

Five native runs on the WAV fixture gave peak RSS between 8056 KB and 8156 KB from the `getrusage` thunk, total scoring time between 38.9 ms and 56.4 ms, and per-window times between 9.1 ms and 14.9 ms. Three runs on the raw f32 fixture gave 10 544 to 10 732 KB RSS and total scoring time between 37.8 ms and 59.6 ms. Replacing per-sample `struct.pack`/`unpack` with direct IEEE-754 bit decoding cut raw f32 peak RSS from about 37 MB to about 10.6 MB.

Repeated runs inside one process (`--repeat 2` or `--repeat 3`) reported `deterministic: true` and identical profiles and decision digests. Two separate binary invocations produced the same digest and the same serialized profile. The compiled binary and CPython source execution produced the same profile JSON, the same decision digest, and similarities matching to six decimal places (`1.0, 0.956866, 0.354628, 0.036655` on the WAV fixture).

## Compiler subset failures and fences hit

These were observed while developing against pyscriptc 0.9.0 on Python 3.14.9, not guessed from documentation:

- `import resource` fenced with `PS2002: resource is not in the closed import table`; RSS now comes from the FFI thunk instead.
- `Any` and `object` annotations fenced with `PS1104`, which blocked `json.loads` into a dynamic dict. Profile scalars use a `dict[str, float]` and the vector uses `list[float]`, both of which lower.
- `struct.unpack` into a `tuple[float, float, float]` fenced with `PS1003: got is list[float]`; `<h` unpacking into `float` fenced in the opposite direction because it returns `int`.
- A subclass returned from a function annotated with its base class fenced with `PS1003: return is MockNativeAdapter, annotation says NativeAdapter`; the mock now exposes the contract structurally and `make_adapter()` returns the concrete type.
- `text.find(",", start)` fenced with `PS2001: call of attribute is not lowered (text.find)`; a small `find_from()` loop using string slices replaced it.
- Iterating `list[bool]` fenced with `PS2002: for over list[bool] is not lowered`; the vote count uses an indexed `while` loop.
- Runtime divergence, not a compile fence: slicing a `bytes` value that contains a non-ASCII byte produced shifted slices. For `b"RIFF$\xee\x02\x00WAVEfmt "`, native `data[4:8]` returned six bytes and `data[8:12]` returned `b"VEfm"`, while CPython returned `b"$\xee\x02\x00"` and `b"WAVE"`. Binary parsing now converts the file to `list[int]` and reconstructs integers arithmetically, which avoids the bug entirely.
- Toolchain setup fences: the mise `pyscriptc` and `zig` shims have no version set, and pyrefly is not installed into the compiler interpreter, so the commands above set explicit `PYSC`, `PYREFLY_BIN`, `--driver clang`, and `--linker clang`.

The final source compiles with zero fences, so no upstream issue was filed from this spike. The bytes-slicing divergence is the strongest candidate for an upstream report once it has a minimal reproducer outside this spike.

## Extern FFI assessment for the real voce_native integration

The manifest model is understandable and small enough for the real shim: `[manifest]` declares `module`, `ps_abi = 1`, and `format = 1`; `[[source]]` names a C source relative to the manifest; each `[[fn]]` declares a Python-visible name, a `.pyi`-shaped `py` signature, the documented `c_symbol`, and the `abi` thunk symbol that the generated LLVM calls. This spike compiled and ran a real extern call (`voce_native.rss_kb()` bound to a `getrusage` thunk), so the read path from manifest to typed Python call to linked C object is proven rather than inferred.

The future inference surface is sketched as comments in `voce_native.toml`: create, load models, reset enrollment, add enrollment PCM, finish enrollment, embed window, and close. Adding those as `[[fn]]` entries plus C thunks in `voce_native_ffi.c` is the planned swap; orchestration only needs `make_adapter()` changed. The opaque `[[handle]]` ownership model with `borrows`/`consumes` and list marshalling are documented by the compiler but were not exercised here, so handle lifetime for a stateful engine and per-window f32 list transfer remain the unverified parts of the real integration.

## Conclusion

A typed pyscriptc backend can own the Voce orchestration with a ~545 KiB native artifact, sub-2 s iteration on this host, native-versus-CPython parity on this fixture, and deterministic decisions. Peak RSS stays around 8 MB for WAV and 11 MB for raw PCM with the narrow mock. The extern FFI seam is clear enough to plan the integration, but it needs a real shim ABI to validate handles and embedding transfer before this spike can claim parity with production inference.
