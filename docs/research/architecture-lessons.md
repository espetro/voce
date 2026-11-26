# Reusable architecture lessons: ClearMic Studio and Krisp-style voice isolation

Date: 2026-11-26 · Lane: backend spike 4 (model and DSP research)

Two reference products are examined for **architecture patterns Voce can reuse**. ClearMic Studio (macOS voice isolation studio app) is a public, inspectable design. Krisp is fully proprietary: this document uses it strictly as a description of product behavior and integration contract observed from public documentation, with **no implication that Krisp code or models are available to Voce**.

---

## 1. ClearMic Studio (macOS)

Sources: public write-up and repo discussion of the architecture — [dev.to: "My macOS virtual mic driver is Apple's sample code"](https://dev.to/nikolai_stepanov_d02669aa/my-macos-virtual-mic-driver-is-apples-sample-code-the-hard-part-was-a-slider-that-did-000-db-fcc) (2026-10); the "ClearMic" name also appears as a browser-based cleanup tool ([megaconfidence/clearmic](https://github.com/megaconfidence/clearmic)) which is a different project; the lessons below are from the macOS studio write-up only.

### Lesson 1: shell/daemon split with a deliberately dumb kernel-adjacent layer

The write-up splits the system into three processes:

1. **GUI** (SwiftUI) — owns no audio at all.
2. **Engine agent** under launchd (`RunAtLoad`, `KeepAlive`) — owns the physical mic, runs realtime DSP, writes to the virtual device; keeps running when the window closes.
3. **HAL plug-in** — the virtual mic. Loads no models, touches no network, reads no user files: "everything interesting happens in a normal user process, where a crash is my problem and not the system audio daemon's."

**Voce transfer:** Voce already has the same shape (Rust shell + realtime core + HAL driver in `audio-driver/`). The reusable discipline is the rule, not the shape: the component loaded into `coreaudiod` must stay model-free and allocation-free; all inference lives in a killable user process. This directly supports the spike plan's `shell → realtime core → backend engine → shim` layering — the shim must never be reached from the HAL plug-in.

GUI↔agent communication is a **narrow XPC protocol (13 methods)**: ping, configure, start, stop, apply-preset, snapshot, monitor, playback, calibration; presets travel as JSON. This matches Voce's small `PanelCmd`/`PanelEvent` surface; the lesson is to keep the control protocol count that small and resist accretion.

### Lesson 2: exact PCM transport

Concrete transport decisions worth copying or deliberately diverging from:

- **One canonical internal format:** 48 kHz Float32 mono inside the agent, preferred HAL buffer 128 frames. Every buffer allocated up front, including the monitor delay line sized for worst-case stage latency — "growing it later would mean allocating on the render thread."
- **HAL as transport, not IPC:** the engine plays into the virtual device's output side as an ordinary Core Audio client; the consumer records from the input side. "No shared memory, socket or XPC between the engine and the driver. The HAL itself carries the audio."
- **Clock-drift handling:** mic and virtual device run on different clocks, so a lock-free SPSC ring (262 144 frames) with a reader reserve of 256 frames ±64 tolerance; when fill drifts outside the window the reader stretches one frame to catch up. The point: **the ring is the contract**, and drift correction is a measured, bounded policy — never unbounded buffering.
- **Driver-side latency budget:** loopback write→read measured at 512 frames (10.667 ms) with an automated verifier (impulse at frame 4800 + 997 Hz tone) and an acceptance failure above 20 ms.

**Voce transfer:** Voce's invariant set (16 kHz mono f32 pipeline, 512-sample chunks, 3 s ring @ 48 kHz, `docs/architecture.md`) serves the same purpose — a single exact transport description that any backend spike can be tested against. The new lessons are (a) the **verifier-as-acceptance-test** pattern (synthetic impulse/tone fixtures with a hard latency threshold), which should be added to the eval harness as a smoke tier, and (b) explicit **drift policy on the output ring** (Voce's 48 kHz output ring should document its underrun behavior as precisely as the ClearMic ±64-frame reserve).

### Lesson 3: measurement discipline

The write-up reports numbers, not adjectives: 10.667 ms loopback latency, 512-frame ring, 16384-frame driver ring, 262144-frame SPSC ring, 256±64 reserve, 20 ms acceptance ceiling. Every claim is tied to a measurement fixture that can fail.

**Voce transfer:** this is exactly the spike spec's Gate 1/Gate 2 methodology (embedding drift, per-window runtime, RSS, determinism). The additional discipline to adopt: every architecture claim in Voce docs should carry either a number from a repeatable fixture or an explicit "unmeasured" tag.

### What does *not* transfer

The ClearMic (Studio) write-up's host-side distribution assumptions (host-Python analysis tooling, launchd-agent packaging choices) are explicitly out of scope per the spike design: "ClearMic Studio contributes the hybrid shell/inference-daemon shape, exact PCM transport, and latency measurement discipline, but not its host-Python distribution assumptions." Voce's backend-shim layering replaces that portion.

## 2. Krisp (product behavior only)

Sources: public product/SDK documentation only — [RTC NC/BVC overview](https://developers-docs.krisp.ai/docs/krisp-rtc-bvc-nc), [Krisp help: Noise Cancellation](https://help.krisp.ai/hc/en-us/articles/30533083171868-Noise-Cancellation-with-Krisp-AI-Meeting-Assistant), [Developer docs / getting started](https://developers-docs.krisp.ai/docs/getting-started), [Desktop SDK reference](https://developers-docs.krisp.ai/docs/desktop-sdk-bvc). Krisp's models and code are proprietary and are not assumed available.

Observable product/integration behavior and what it teaches:

1. **Tiered capability, not one model.** Krisp ships Noise Cancellation (noise only, inbound + outbound, works down to 8 kHz) as a separate product from Background Voice Cancellation (removes other *voices* too, requires ≥32 kHz processing rate, >8 kHz stream). The docs explicitly blocklist narrow-band devices from BVC because quality drops and the user's voice gets suppressed.
   → *Lesson:* speaker-gating quality is device- and bandwidth-sensitive; a fallback tier (pure suppression) must exist for bad capture paths. Voce's suppression-plus-gate arm of the eval protocol should include a degraded-device fixture tier before enabling gating-only behavior.
2. **Frame-synced, fixed-frame API.** Each call takes one audio frame and returns a clean frame of the same size; documented algorithmic latency ≈25 ms for 10 ms frames at 16 kHz; supported frame durations 10/15/20/30/32 ms.
   → *Lesson:* the "same-size PCM out per chunk in" contract Voce's backend spec defines is the industry-standard integration shape; keep chunk sizes to this family (Voce's 32 ms sits within it).
3. **On-device CPU processing as a product guarantee.** All processing local ("no audio sent to external servers"), models "extremely small, run on CPU," small/big model variants with a documented ~7× CPU ratio.
   → *Lesson:* publish Voce's CPU cost per tier (gate / gate+suppression / extraction) as a first-class metric, and keep a small-model fallback. Latency numbers in marketing are backed by measured per-frame cost — mirrors ClearMic lesson 3.
4. **Bypass semantics.** VoiceFilter-Lite-style products (and Krisp Voice Isolation) emphasize graceful bypass when the target speaker is not present; see also Google's VoiceFilter-Lite design ([research blog](https://research.google/blog/improving-on-device-speech-recognition-with-voicefilter-lite/): plug-and-play bypass when unenrolled).
   → *Lesson:* Voce's unenrolled state must be a defined passthrough mode with measured behavior, not undefined behavior — the eval protocol should include an unenrolled-arm measurement.
5. **AEC placement rules.** AEC goes before NC on the capture path and after NC on the render path.
   → *Lesson:* any future echo handling in Voce must be placed relative to the suppression/gate stages, and the backend contract should reserve ordering semantics now.

## 3. Synthesis: what Voce should adopt

| Adopt | From | Concretely |
|---|---|---|
| Model-free HAL plug-in rule | ClearMic | Codify in `src/AGENTS.md`/`audio-driver` docs: no inference, no allocation, no locks in IO callback |
| Verifier-as-acceptance-test | ClearMic | Impulse + tone fixtures with hard latency thresholds in eval smoke tier |
| Drift policy on output ring | ClearMic | Document reserve/tolerance behavior of the 48 kHz output ring |
| Frame-synced PCM contract | Krisp (behavior) / Voce spec | Keep 32 ms chunk in/out; reject variable-output-size APIs in the shim |
| Tiered capability + fallback | Krisp (behavior) | Gate vs gate+suppression vs extraction as measured tiers with CPU cost published |
| Bypass semantics | Krisp/VoiceFilter-Lite | Unenrolled passthrough is a specified, measured mode |
| Measurement discipline | ClearMic | Numbers-or-"unmeasured" tagging in docs; fixture parity as the only accepted evidence |

## Sources

- https://dev.to/nikolai_stepanov_d02669aa/my-macos-virtual-mic-driver-is-apples-sample-code-the-hard-part-was-a-slider-that-did-000-db-fcc
- https://github.com/megaconfidence/clearmic (browser tool — distinct project, cited only to disambiguate)
- https://developers-docs.krisp.ai/docs/krisp-rtc-bvc-nc
- https://developers-docs.krisp.ai/docs/getting-started
- https://developers-docs.krisp.ai/docs/desktop-sdk-bvc
- https://help.krisp.ai/hc/en-us/articles/30533083171868-Noise-Cancellation-with-Krisp-AI-Meeting-Assistant
- https://research.google/blog/improving-on-device-speech-recognition-with-voicefilter-lite/
