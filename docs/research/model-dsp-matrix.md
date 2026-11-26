# Voce model/DSP comparison matrix

Date: 2026-11-26 · Lane: backend spike 4 (model and DSP research) · Branch: `voce-spike-model-research`

Scope: artifact-backed candidates for VAD, speaker embeddings, noise suppression, and target speaker extraction (TSE). Every candidate below names a concrete artifact (repo, release asset, or downloadable ONNX). General separation architectures (TF-GridNet, SepFormer) are listed as references only — no target-conditioned streaming artifact exists for them. Krisp appears only as a product-behavior reference in [architecture-lessons.md](architecture-lessons.md); nothing proprietary is assumed available.

Voce pipeline invariants these candidates must respect: 16 kHz mono f32, 512-sample processing chunks (32 ms), 3 s inference windows (48 000 samples), ONNX or C-lib inference behind a C ABI shim (see `docs/superpowers/specs/2026-11-25-backend-spikes-design.md`).

---

## 1. VAD

| | Silero VAD (v4/v5) | TEN VAD |
|---|---|---|
| Artifact | [snakers4/silero-vad](https://github.com/snakers4/silero-vad); sherpa docs: [silero-vad](https://k2-fsa.github.io/sherpa/onnx/vad/silero-vad.html) | [TEN-framework/ten-vad](https://github.com/TEN-framework/ten-vad); sherpa integration PR [#2377](https://github.com/k2-fsa/sherpa-onnx/pull/2377) |
| License | MIT (model); sherpa-onnx runtime Apache-2.0 | "modified Apache-2.0" per [sherpa VAD docs](https://k2-fsa.github.io/sherpa/onnx/vad/) — custom license text, must be read before commercial bundling |
| ONNX/export | Direct ONNX, no export needed | Direct ONNX (sherpa re-adds normalization metadata; must use sherpa's copy) |
| Size | 629 KB fp32 (16 kHz-only k2 export), 208 KB int8; v4 1.72 MB / v5 2.22 MB (8/16 kHz) | 324 KB fp32, 126 KB int8 |
| Runtime path | `SherpaOnnxVadModelConfig.silero_vad`, 512-sample window @ 16 kHz — exactly matches Voce's chunk size | `SherpaOnnxVadModelConfig.ten_vad` |
| Practicality | Trivial: drop-in via sherpa-onnx or current rustburn ONNX session; already the de-facto standard | Same runtime path; one caveat: sherpa sets TEN's pitch feature to 0 (PR #2377), which may degrade performance vs the reference implementation |
| Eval feasibility | High: run on Voce fixtures, compare per-window speech/silence decisions against current gate | Same, plus re-check whether the pitch-zeroing delta matters on fixture audio |

Status: Voce's VAD behavior is already compatible with the sherpa silero path. TEN VAD is an optional A/B only if fixture audio shows silero failing on the specific failure modes it targets (short/overlapped speech). No direction change justified by VAD alone.

## 2. Speaker embeddings

| | WeSpeaker ResNet34 (current baseline) | WeSpeaker ECAPA512-LM | CAM++ (3D-Speaker / WeSpeaker) |
|---|---|---|---|
| Artifact | [HF: Wespeaker/wespeaker-voxceleb-resnet34-LM](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM); runtime export: [wespeaker/runtime](https://wenet-e2e.github.io/wespeaker/runtime.html) | [HF: Wespeaker/wespeaker-ecapa-tdnn512-LM](https://huggingface.co/Wespeaker/wespeaker-ecapa-tdnn512-LM) | sherpa release: [speaker-recongition-models](https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-recongition-models) (`wespeaker_en_voxceleb_CAM++.onnx` 27.9 MB, `3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx` 28.2 MB) |
| License | Apache-2.0 code; VoxCeleb-trained weights CC-BY-4.0 (dataset license, per [pretrained models page](https://wenet-e2e.github.io/wespeaker/pretrained.html)) | CC-BY-4.0 weights / Apache-2.0 code | Apache-2.0 |
| ONNX/export | `wespeaker/bin/export_onnx.py`; ONNX runtime path documented | Prebuilt runtime ONNX | Prebuilt sherpa ONNX, 80-dim fbank → 192-dim embedding |
| Size | ~25 MB (6.63 M params, 4.55 GFLOPs per HF card) | ~24 MB ONNX (6.19 M params, 1.04 GFLOPs) | 27–28 MB fp32; 14 MB fp16; 8.2 MB int8 (third-party: [ldov/campplus-zh-en-onnx](https://huggingface.co/ldov/campplus-zh-en-onnx)) |
| Runtime path | Current Voce baseline (custom fbank + ONNX); also `SpeakerEmbeddingExtractor` in sherpa-onnx | sherpa-onnx `SpeakerEmbeddingExtractor` (192-dim) | sherpa-onnx `SpeakerEmbeddingExtractor` (192-dim) |
| Accuracy (published, VoxCeleb) | vox1-O-clean EER 0.867 (plain) / 0.723 (LM+AS-Norm) | 1.069 (plain) / 0.782 (LM+AS-Norm) | — (3D-Speaker reports SV benchmarks on its own eval sets) |
| Mobile/server practicality | Proven (it is the baseline) | Comparable cost to ResNet34 on paper (lower FLOPs); one third-party device benchmark (Pixel 6, [ldov card](https://huggingface.co/ldov/campplus-zh-en-onnx)) measured CAM++ fp32 at 102 ms/utt and fp16 at 70 ms/utt vs ERes2Net 256 ms/utt — single-device anecdote, not a decision basis | Same |
| Eval feasibility | Reference point | High: swap model file, re-run enrollment+eval on fixtures, compare gate FPR/recall and EER-style separation on enrolled-vs-other cosine distributions | High: same swap |

Notes for the matrix decision: embedding swaps are the cheapest experiments available — same shim, same fbank conventions (must verify CAM++/ECAPA expected fbank config matches Voce's current fbank; sherpa handles this internally via model metadata), no pipeline change. Recommendation only if fixtures show the current embedding as the bottleneck.

## 3. Noise suppression (speaker-agnostic)

| | RNNoise | DeepFilterNet3 | GTCRN |
|---|---|---|---|
| Artifact | [xiph/rnnoise](https://github.com/xiph/rnnoise) | [Rikorose/DeepFilterNet](https://github.com/Rikorose/DeepFilterNet), `models/DeepFilterNet3_onnx.tar.gz` (~8 MB) | [Xiaobin-Rong/gtcrn](https://github.com/Xiaobin-Rong/gtcrn) (ICASSP 2024); sherpa support since 2025-03 ([repo note](https://github.com/Xiaobin-Rong/gtcrn)); online denoiser PR [#3324](https://github.com/k2-fsa/sherpa-onnx/pull/3324), merged 2026-03 |
| License | BSD-3-Clause **code**; the distributed pretrained weights have no explicit license statement — open issue [#284](https://github.com/xiph/rnnoise/issues/284) (2026-09) is unanswered as of 2026-10. Distributions disagree (Debian/Gentoo treat as BSD-3, Fedora tags CC0). Legal review required before shipping | MIT OR Apache-2.0 (dual) | MIT |
| ONNX/export | No official ONNX; native C lib with ~87.5 k weights (~85 KB, int8-constrained). Third-party ONNX ports exist but are unofficial | Three-graph export (`enc.onnx` 1.95 MB + `erb_dec.onnx` 3.29 MB + `df_dec.onnx` 3.34 MB ≈ 8.5 MB); STFT/ERB framing and LSTM state management live **outside** the graphs — needs the Rust `libDF` runtime or a reimplementation. An "ll" low-latency variant (35 MB, zero lookahead) also exists ([issue #628](https://github.com/Rikorose/DeepFilterNet/issues/628)) | Streaming stateful graph `gtcrn_simple.onnx` (0.52–0.54 MB); STFT outside graph, caches in/out; sherpa-onnx wraps all of this |
| Size | ~0.09 MB | ~8 MB (or 35 MB ll) | 0.5 MB |
| Runtime path | C ABI, native | Rust crate `deep_filter` (libDF); onnxruntime possible but heavy glue | sherpa-onnx online denoiser: `SherpaOnnxOnlineSpeechDenoiser` C API, exact match to the shim's C ABI direction ([C API speech enhancement docs](https://k2-fsa.github.io/sherpa/onnx/c-api/html/speech_enhancement.html)) |
| Sample rate | 48 kHz native (Voce would need 16↔48 handling or the model resampled) | 48 kHz native, 10 ms hop, ~40 ms algorithmic latency | **16 kHz native** — matches Voce pipeline exactly |
| Cost (published) | ~40 MMAC/s whole algorithm, ~1.3 % of one Haswell x86 core (paper [arXiv:1709.08243](https://arxiv.org/pdf/1709.08243.pdf)) | Higher than RNNoise; specific RTF depends on host; not re-measured here | 23.7 k params (paper) / 48.2 k incl. ERB, 33 MMAC/s; RTF 0.07 on i5-12400 (repo); sherpa demo RTF 0.071 ([sherpa gtcrn docs](https://k2-fsa.github.io/sherpa/onnx/speech-enhancement/models.html)) |
| Quality (published) | PESQ 2.29 on VCTK-DEMAND (as reported in GTCRN repo's comparison table) | Competitive with much larger models (per [GTCRN repo table](https://github.com/Xiaobin-Rong/gtcrn)) | VCTK-DEMAND PESQ 2.91 / STOI 0.940; DNS3 DNSMOS OVRL 2.70 vs RNNoise 2.53 — "outperforms RNNoise by a substantial margin" per repo |
| Practicality | Best size, worst quality, license ambiguity on weights, no ONNX | Good quality, but 48 kHz + glue code + 3-graph runtime fights Voce's 16 kHz invariant | Best fit: artifact quality, size, 16 kHz native, and sherpa online denoiser already speaks the C ABI Voce's shim targets |
| Eval feasibility | High (C lib trivially embeddable) but see license | Medium: needs libDF integration first | Highest: sherpa-onnx binary can produce enhanced fixtures for offline eval before any Rust integration |

Recommendation supported by artifacts: if suppression enters the pipeline, GTCRN is the only candidate that satisfies license, size, sample-rate, and runtime-path constraints simultaneously. DFN3 remains the fallback if fixture eval shows GTCRN's 16 kHz quality insufficient.

## 4. Target speaker extraction

| | WeSep | VoiceFilter-Lite (reimplementation) | SpEx+ |
|---|---|---|---|
| Artifact | [wenet-e2e/wesep](https://github.com/wenet-e2e/wesep) | [PyPI open-voicefilter-lite](https://pypi.org/project/open-voicefilter-lite/) (v0.2.0) + source repo; concept: Google [VoiceFilter-Lite paper](https://arxiv.org/abs/2009.04323) / [blog](https://research.google/blog/improving-on-device-speech-recognition-with-voicefilter-lite/) | [gemengtju/SpEx_Plus](https://github.com/gemengtju/SpEx_Plus) (MIT); [xuchenglin28/speaker_extraction_SpEx](https://github.com/xuchenglin28/speaker_extraction_SpEx) (GPL-3.0) |
| License | Apache-2.0 | Apache-2.0 | MIT (gemengtju) / GPL-3.0 (xuchenglin28) |
| ONNX/export | v0.1 (2026) is a research preview: ONNX/TorchScript export and pretrained model packages are explicitly "coming soon" / "to be updated" ([README status table](https://github.com/wenet-e2e/wesep)). The 2024-era checkpoints (BSRNN/DPCCN/SpEx+/TFGridNet with WeSpeaker cues) target a legacy interface whose C++ runtime is retained "for reference only" | Bundled ONNX mask predictor + int8 d-vector encoder; torch-free runtime | No official export; PyTorch training code and checkpoints, non-causal/offline |
| Size | n/a until artifacts land | Bundled in wheel (small; Google's reference int8 TFLite target was <100 MB, this reimplementation is smaller) | Not published as a runtime artifact |
| Runtime path | PyTorch only today | Python/onnxruntime, streaming, 10 ms hop @ 16 kHz, PCM-in/PCM-out. No C/C++ runtime — Voce would need a port or Python sidecar | Offline research only |
| Streaming | Not yet supported by current release | Yes: unidirectional LSTM, per-hop state, passthrough when no enrollment set — matches Voce's enrollment-gate model | No |
| Practicality | Strongest-scoped open TSE toolkit, but no deployable artifact as of 2026-11 | The only target-conditioned, streaming, enrollment-conditioned, ONNX artifact runnable today without training. Caveat: third-party reimplementation (not Google's model), Python-only runtime | Reference only |
| Eval feasibility | Blocked pending artifacts | High for quality measurement (offline fixture runs); integration cost is the port/FFI, not the model | Offline metrics only (SI-SDR on simulated mixes) |

General separation references: TF-GridNet and SepFormer have strong published separation numbers but no target-conditioned streaming artifact meeting Voce's constraints; per the spike spec they stay out of the candidate set.

### Extraction verdict (artifact-based)

- WeSep: correct long-term direction (Apache-2.0, cue-conditioned models, ONNX on roadmap) but **no runnable artifact today** → monitor, do not build against it.
- VoiceFilter-Lite reimplementation: artifact-backed, streaming, matches Voce's enrollment shape. The eval protocol in [eval-protocol.md](eval-protocol.md) treats it as the extraction arm.
- SpEx+: GPL/MIT research code, non-streaming → reference only.

---

## 5. Decision implications (summary)

1. VAD: keep current behavior; silero-vad via sherpa is the parity path; TEN VAD only as a fixture-driven A/B.
2. Embeddings: keep WeSpeaker ResNet34 unless fixture eval implicates the embedding; CAM++ and ECAPA512 are one-file swaps behind the same shim.
3. Suppression: GTCRN is the only suppression candidate meeting all Voce constraints (license, 0.5 MB, 16 kHz native, sherpa online C API). Evidence needed from eval protocol before any integration.
4. Extraction: default remains the similarity gate (spec Gate 3). VoiceFilter-Lite reimplementation is the only extraction artifact that can be measured today; WeSep is tracked, not adopted.

## Sources

- https://github.com/k2-fsa/sherpa-onnx/
- https://k2-fsa.github.io/sherpa/onnx/vad/silero-vad.html
- https://k2-fsa.github.io/sherpa/onnx/vad/ (licensing of silero vs ten-vad)
- https://github.com/k2-fsa/sherpa-onnx/pull/2377 (ten-vad support, sizes, pitch-zero caveat)
- https://wenet-e2e.github.io/wespeaker/pretrained.html
- https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM
- https://huggingface.co/Wespeaker/wespeaker-ecapa-tdnn512-LM
- https://wenet-e2e.github.io/wespeaker/runtime.html
- https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-recongition-models
- https://huggingface.co/ldov/campplus-zh-en-onnx (CAM++ fp16/int8 device benchmark)
- https://github.com/xiph/rnnoise ; https://github.com/xiph/rnnoise/issues/284 (weights license question)
- https://arxiv.org/pdf/1709.08243.pdf (RNNoise paper)
- https://github.com/Rikorose/DeepFilterNet ; https://github.com/Rikorose/DeepFilterNet/issues/628 (ll variant)
- https://huggingface.co/Alice-Sabrina-Ivy/deepfilternet3-onnx (pinned DFN3 ONNX sizes)
- https://github.com/Xiaobin-Rong/gtcrn
- https://k2-fsa.github.io/sherpa/onnx/speech-enhancement/models.html
- https://k2-fsa.github.io/sherpa/onnx/c-api/html/speech_enhancement.html
- https://github.com/k2-fsa/sherpa-onnx/pull/3324 (online GTCRN denoiser)
- https://github.com/wenet-e2e/wesep
- https://pypi.org/project/open-voicefilter-lite/
- https://arxiv.org/abs/2009.04323 ; https://research.google/blog/improving-on-device-speech-recognition-with-voicefilter-lite/
- https://github.com/gemengtju/SpEx_Plus ; https://github.com/xuchenglin28/speaker_extraction_SpEx
