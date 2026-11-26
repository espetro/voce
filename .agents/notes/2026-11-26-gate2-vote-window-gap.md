# Gate 2 vote-window semantics gap (2026-11-26)

Durable finding from lane G2P. The pyscriptc real/native CLI accepts `--vote-window 3` and reports it in its JSON, but in `--backend real` and `--backend native` paths it emits raw per-window ABI decisions (`native_decisions()`); the majority-of-3 vote logic in `spike.py` is only used by the mock path. The Rust `voce-native-probe` has no vote flag at all and also emits raw decisions.

Consequence for Gate 2 parity: compare raw decisions first, then apply the same documented majority-of-3 rule to both outputs in measurement code. `backends/pyscriptc/gate2_normalize.py` does this without changing either CLI. On the standardized 100 MB `voxblink2_samresnet34_ft.onnx` fixture set, raw target recall is 30/39 = 76.9% while vote-gated target recall is 33/39 = 84.6%; other-speaker FPR is 0% in both views. Both backends produce the same numbers.

This is a semantic fence for production gating, not a correctness-blocking compiler/runtime fence: ABI v1 is unchanged and no inference divergence was observed.
