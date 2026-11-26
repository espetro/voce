# Voce model/DSP research index

Date: 2026-11-26 · Lane: backend spike 4

Deliverables of the model/DSP research lane:

| Document | Purpose |
|---|---|
| [model-dsp-matrix.md](model-dsp-matrix.md) | Artifact-backed comparison matrix: VAD (silero/TEN), speaker embeddings (WeSpeaker ResNet34/ECAPA512, CAM++), suppression (RNNoise, DeepFilterNet3, GTCRN), target speaker extraction (WeSep, VoiceFilter-Lite reimplementation, SpEx+). License, ONNX path, size, runtime, cost, eval feasibility per candidate. |
| [architecture-lessons.md](architecture-lessons.md) | Reusable architecture lessons from ClearMic Studio (shell/daemon split, exact PCM transport, measurement discipline) and Krisp product behavior only (tiered capability, frame-synced API, bypass semantics). No proprietary code/models assumed. |
| [eval-protocol.md](eval-protocol.md) | Protocol comparing similarity gate / suppression+gate / target extraction on Voce fixtures: tiers, exact metrics, acceptance thresholds, evidence required to change direction (Gate 3). |

Headline conclusions (details in each doc):

1. GTCRN is the only suppression candidate meeting all Voce constraints (MIT, 0.5 MB, 16 kHz native, sherpa-onnx online C API).
2. Target speaker extraction has no production-grade Apache/MIT streaming ONNX artifact today; the VoiceFilter-Lite reimplementation is the only measurable extraction artifact; WeSep ONNX is "coming soon" — monitor, don't build.
3. Default direction remains the similarity gate; the eval protocol defines what evidence would change that.
