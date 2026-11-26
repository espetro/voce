# pyscriptc backend spike

This lane proves the Voce backend contract in typed Python compiled by pyscriptc. It depends on no native shim artifacts: `spike.py` owns enrollment, windowing, scoring, gating, profile serialization, and batch PCM/WAV processing, while all inference-shaped work sits behind `MockNativeAdapter` in the same file. `make_adapter()` is the single swap point for a future `voce_native` FFI adapter.

The adapter contract is two methods: `embed_window(window: list[float]) -> list[float]` returns a unit-length embedding for one 48 000-sample window, and `similarity(profile: list[float], window: list[float]) -> float` compares a window embedding to the enrolled profile. The mock computes normalised 16-band magnitudes from the decimated window and scores each window with a dot product against the enrolled profile, so decisions depend on the profile rather than a per-window hash.

Input is 16 kHz mono f32 raw PCM or 16-bit PCM / 32-bit float mono WAV at 16 kHz. Windows are 48 000 samples (3 s) with a 16 000-sample (1 s) hop, and enrollment averages at most three windows. Output is one JSON object with embedding dimension, per-window similarity and pass/gated decisions, per-window runtime, peak RSS from the FFI thunk, artifact size, repeat count, decision digest, and determinism.

Build and run:

```bash
python3 backends/pyscriptc/fixtures/make_fixtures.py /tmp/opencode/voce-fixtures
sh backends/pyscriptc/build.sh --timings
backends/pyscriptc/voce_spike --input /tmp/opencode/voce-fixtures/test.wav --enroll /tmp/opencode/voce-fixtures/enroll.wav --repeat 2
```

`build.sh` sets `PYSC` and `PYREFLY_BIN` to the installed toolchain paths when they are not already set, then runs the compile under `flock /tmp/tsc.lock`. See `docs/research/pyscriptc-backend-spike.md` for exact measurements, compiler fences, and the FFI assessment.
