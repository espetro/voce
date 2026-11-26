# Gate 2 native probe baseline (2026-11-26)

Durable measurement record for lane G2P. Model: `/home/own-agents/.cache/voce/models/voxblink2_samresnet34_ft.onnx`, sha256 `33af8affe6191b1ebd196d2b56e22c2934104cd2764abfdbdd954d3a934eb2a1`, 100,865,597 B. Fixtures: `/home/own-agents/.cache/voce/gate2-fixtures/*.wav`, 60 s each, 16 kHz mono PCM16. Threshold 0.75, `--repeat 1`, real backend, release probe.

```bash
flock /tmp/tsc.lock sh -c 'cd native && cargo build --release'
NATIVE=native/target/release/voce-native-probe
for n in 1 2; do
  flock /tmp/tsc.lock "$NATIVE" --backend real --models-dir "$MODEL" \
    --threshold 0.75 --repeat 1 --enroll "$FIX/enroll_1272.wav" \
    --input "$FIX/target_1272.wav" --input "$FIX/other_2035.wav" \
    --input "$FIX/other_5895.wav" --input "$FIX/other_777.wav" \
    --out /tmp/opencode/g2ps/native-run-$n.json
done
```

Native release build: 60,592 ms. Probe run 1: 294,640 ms wall, 847.29 ms per window, model load 909.55 ms, peak RSS 467,144,704 B. Probe run 2: 259,044 ms wall, 705.77 ms per window, peak RSS 501,362,688 B. Both runs: 156 windows (5 inputs x 39), 30 passed (all target), deterministic true, profile roundtrip true on all inputs. Target recall 30/39 = 76.9%; other FPR 0/117 = 0%.

Per-input FNV digests (target, other_2035, other_5895, other_777): `43c86ebaccf85fb4`, `ac679c690c4a45e5`, `dc9ec0614ffb8948`, `dd60f839095113d0`. The pyscriptc smoke run on target reproduced the native digest exactly with zero raw/similarity/vote mismatches via `backends/pyscriptc/gate2_normalize.py`.

Constraint: this model/fixture pair yields target recall below the Gate 2 85% floor on the native backend itself. The shortfall is not a pyscriptc divergence.
