<p align="center"><img src="docs/assets/voce-icon.png" width="128" alt="Voce icon"></p>

<h1 align="center">Voce</h1>

<p align="center">Your voice. Only your voice. Even in the loudest café.</p>

<p align="center">
<a href="LICENSE"><img alt="License" src="https://img.shields.io/badge/license-AGPL--3.0-blue"></a>
<img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20Apple%20Silicon-black">
<img alt="Status" src="https://img.shields.io/badge/status-proof%20of%20concept-orange">
</p>

Voce is a privacy-first macOS menu bar app that learns what you sound like, then turns that voice profile into a virtual microphone for Zoom, Meet, Teams, and any other call app.

Noise cancellation removes machines. Voce removes other people.

Apple Voice Isolation, Krisp, and most call noise suppression are built for fans, keyboards, traffic, and café equipment. They do not know whether a human voice belongs to you or to the person sitting two metres away. Voce closes that gap with speaker-identity filtering: enroll your voice once, then pass speech that matches you and silence speech that does not.

## What it does

- Enrolls your voice locally from two short recordings.
- Runs speaker verification in real time on overlapping speech.
- Exposes a virtual input device named **Voce Microphone** through a macOS audio driver.
- Passes your voice through while muting colleagues, baristas, roommates, and nearby conversations.
- Adds optional noise suppression for non-speech background noise.
- Keeps audio and voice embeddings on the device.
- Lives in the menu bar, with a small panel for enrollment, testing, settings, and live filter state.

Voce is a proof of concept under active development. The core pipeline and evaluation harness are real; packaging, onboarding, model choice, and backend architecture are still being hardened.

## How it works

```text
microphone
  → 16 kHz mono PCM
  → voice activity detection
  → speaker embedding
  → cosine-similarity gate
  → Voce Microphone virtual device
  → your call app
```

Enrollment is not model training. Voce records two 20-second samples, extracts speaker embeddings, averages them into a local voice profile, and compares live audio against that profile. The current pipeline uses WeSpeaker-style 256-dimensional embeddings and a sliding vote gate so one noisy frame does not cut off a sentence.

The default gate threshold is `0.75`. If inference fails, Voce fails open and passes audio rather than silently muting the call.

## Install and run

Voce currently targets macOS on Apple Silicon. There is no signed public release yet, so the supported path is a local build.

Prerequisites:

- macOS on Apple Silicon
- Rust installed through `rustup`
- Xcode command line tools
- [`mise`](https://mise.jdx.dev/) for the pinned Python, pnpm, and just tools
- microphone permission for the app
- administrator permission when installing the virtual audio driver

Build and install locally:

```bash
git clone https://github.com/espetro/voce.git
cd voce
mise install
just install-local
open ~/Applications/Voce.app
```

First run:

1. Let Voce download the local inference models.
2. Record two voice samples in a quiet room.
3. Test the filter and compare raw versus filtered playback.
4. Allow Voce to install the **Voce Microphone** driver when prompted.
5. Select **Voce Microphone** as the input device in Zoom, Meet, Teams, or another call app.

A BlackHole fallback path still exists for compatibility, but the primary output path is the bundled CoreAudio HAL driver.

## Evaluation

Voce has a repeatable offline evaluation harness for the same inference path used by the app. It measures false positives, recall, F1, threshold sensitivity, and audible filtered-output artifacts across LibriSpeech, AliMeeting, and meeting-speech fixtures.

The acceptance targets are:

| Metric | Target | Meaning |
| --- | ---: | --- |
| False positive rate | under 5% | non-enrolled speech incorrectly passed |
| Recall | over 85% | enrolled speech correctly passed |

The documented AliMeeting overlapping-speech example at threshold `0.75` reports `FPR=4.8%`, `Recall=97.9%`, and `F1=0.989`. Treat that as a harness result for the current pipeline, not as a finished-product guarantee across every microphone, room, language, or voice.

Run the main harness:

```bash
just eval::setup
just eval::run
just eval::threshold
```

Run the overlapping-speech harness:

```bash
just eval::alimeeting-setup
just eval::alimeeting-run
just eval::alimeeting-sweep
```

More detail: [docs/eval-harness.md](docs/eval-harness.md) and [eval/README.md](eval/README.md).

## Why on-device

A speaker gate sits directly in the microphone path. Sending that audio or the voice profile to a cloud service would defeat the product. Voce therefore keeps enrollment, inference, filtering, and test playback local. Network access is needed for initial model download and ordinary development workflows, not for processing call audio.

## Architecture at a glance

| Area | Role |
| --- | --- |
| Rust app | menu bar lifecycle, microphone capture, inference orchestration, filter state, IPC |
| SolidJS panel | onboarding, enrollment, test playback, settings, live status |
| ONNX models | voice activity detection and speaker embeddings |
| CoreAudio HAL driver | virtual `Voce Microphone` input backed by a shared-memory ring buffer |
| Python eval harness | reproducible speaker-gate metrics across speech datasets |

Deep technical documentation lives in [docs/architecture.md](docs/architecture.md), [docs/prd.md](docs/prd.md), and the area-specific `AGENTS.md` files.

## Current limits

- macOS only, with Apple Silicon as the primary target.
- One enrolled speaker per profile.
- Similarity gating rather than full target-speaker extraction.
- Manual activation and device selection.
- Local development build rather than a notarized consumer installer.
- Windows, Linux, mobile, server batch processing, and automatic call detection are roadmap concerns, not current product claims.

## Roadmap

The near-term work is to harden the backend contract, improve model and noise-suppression choices, package the app as a reliable macOS product, and prove whether explicit target-speaker extraction beats the current speaker-similarity gate.

The public roadmap is tracked in [GitHub Project #12](https://github.com/users/espetro/projects/12/views/1). Product history and acceptance criteria are in [docs/prd.md](docs/prd.md).

## Contributing

Voce is not yet a stable open-source product, but focused issues and pull requests are welcome. Read [AGENTS.md](AGENTS.md) before making changes; the repository has strict invariants around sample rates, committed panel output, IPC synchronization, and evaluation behavior.

## License

GNU Affero General Public License 3.0. See [LICENSE](LICENSE).
