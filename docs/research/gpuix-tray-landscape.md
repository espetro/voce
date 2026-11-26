# GPUIX tray landscape: input to Gate 4

Date: 2026-11-26. Research only; no product code changed.

Source cloned successfully with `git clone --depth 1 --branch examples/todo-validation https://github.com/espetro/gpuix.git /tmp/gpuix-explore`. The default branch (`main`, `0ce4f41`, 2026-11-01), the upstream `remorses/gpuix` `main` (same commit), and the pinned Zed/GPUI submodule commit (`ebcbb242b0bc01c08a1cbde7e241b0794e0bd96e`) were fetched separately for comparison. Network and auth worked; there were no blockers.

Explored commit: `a5def75331efe2ea5c4d0bad673cba0e77bf6c33` on `examples/todo-validation` (2026-11-26 17:46 UTC, `fix(examples): make todo tray interactions observable`). All paths below are relative to that checkout unless prefixed with `zed/`.

## (a) What the tray support actually is, with evidence

GPUIX is not a Rust-only framework: it is a React/Solid renderer in TypeScript over a Node-API native addon (`packages/native`, Rust crate `gpuix-native`) built on a pinned fork of Zed's GPUI (`zed/` submodule from `remorses/zed`, branch `gpuix`). It paints with Metal/DirectX/Vulkan and explicitly avoids Electron and webviews (`README.md` lines 1-10).

The tray feature is a process-wide status item exposed to JS by the native renderer:

| JS API (`packages/native/js/host.ts`, `packages/native/index.d.ts`) | Native implementation | GPUI layer |
| --- | --- | --- |
| `renderer.createTray({ icon, title, tooltip }): boolean` | `GpuixRenderer::create_tray` in `packages/native/src/renderer.rs` (~2055-2120) | `App::create_tray` in `zed/crates/gpui/src/app.rs:1576` |
| `renderer.setTrayMenu(items)` with `{ id, label, enabled?, checked? }` | `set_tray_menu` (~2129) | `TrayHandle::set_menu`, `zed/crates/gpui/src/platform/tray.rs` |
| `renderer.setTrayTooltip(value)`, `renderer.setTrayTitle(value)` (macOS only) | `set_tray_tooltip` / `set_tray_title` (~2147, ~2162) | `PlatformTray` trait |
| `renderer.removeTray()` | `remove_tray` (~2177) | drop of `TrayHandle` |
| `renderer.onTrayEvent(cb)` receiving `{ type: "click" }` or `{ type: "menuItem", id }` | `on_tray_event` (~2196) | `App::on_tray_event`, `futures::mpsc` channel |
| `render(..., { quitMode: "explicit" })` plus `renderer.quit()` and `renderer.activateWindow()` | quit handling in `renderer.rs` (~1337, ~1501, ~7896) | `QuitMode::Explicit` in the GPUI fork |

Concrete platform code exists, not just a stub:

- macOS: `zed/crates/gpui_macos/src/tray.rs` builds an `NSStatusItem` / `NSStatusBarButton` with `NSMenu` / `NSMenuItem`, loads `.icns`/`.png` fitted to an 18 pt menu-bar icon height, and implements `PlatformTray`. `zed/crates/gpui_macos/src/platform.rs:1255` wires `create_tray` and `set_tray_event_sender`. AppKit actions only push into a channel (`TrayActionTarget::menu_item_selected`, `trayButtonClicked:`); they never touch GPUI state during menu tracking.
- Windows: `zed/crates/gpui_windows/src/tray.rs` uses `Shell_NotifyIconW` and a hidden message window; `platform.rs:744` wires it.
- Linux: `zed/crates/gpui_linux/src/linux/tray.rs` uses `ksni` / StatusNotifierItem and DBus menus; `platform.rs:599` wires it. README notes a StatusNotifier host is required.

Threading is explicit: `packages/native/src/tray.rs` header and `renderer.rs:222-230` state that `TrayHandle` lives in an app-thread local (`TRAY_HANDLE`) and all calls route through `update_app`; the Node thread never holds the handle. GPUI tray events carry no tray identity, so there is exactly one tray per process; the menu is flat (no submenus, separators, or accelerators), and `click` fires only when the menu is empty.

Examples and documentation:

- `examples/tray.tsx` (86 lines): creates a tray, sets a three-item menu, logs/prints the last event, and quits from a menu item with `quitMode: "explicit"`.
- `examples/todo.tsx` (433 lines): the named validation example. Its header says it validates keyboard input, pointer events, conditional rendering, scrolling layout, and repeated state updates. Lines 211-284 create the tray, log `[todo tray event]`, expose the last event in the UI, call `activateWindow()` on `open`, complete todos from `complete-<id>` menu items, update `setTrayTitle` / `setTrayTooltip` with active/completed counts, and build a dynamic flat menu (disabled headings, up to six active tasks, "more active tasks", Quit). Line 432 sets `quitMode: "explicit"`.
- `README.md:1061-1134` is the Tray section with a platform support table; `.changeset/tray.md` describes the API and "Fixes #109"; `BETA.md` documents a fork-only rolling `tray-beta` prerelease.

## (b) Maturity assessment

Maturity: **early / validation-branch only**. Evidence:

- Tray exists only on this fork branch. Neither `main` (`0ce4f41`) nor upstream `remorses/gpuix` `main` contains any tray file or README tray section. GitHub API confirms `espetro/gpuix` is a fork of `remorses/gpuix` with 0 stars; upstream issue `remorses/gpuix#109` ("[feat]: tray / status-bar item") was still open on 2026-11-25.
- Branch history is roughly one day old: `9213665 feat(native): tray API and explicit quit mode` (2026-11-25), `4ee625f` docs/example/changeset, `e49e768` fork-only beta release CI, `8961a87` zed submodule bump to the tray-capable GPUI, `12db940 fix(native): create trays directly in macOS embedded mode`, then `654fe7f`, `55be94d`, `a5def75` todo-validation commits on 2026-11-26. Distribution path is `beta/tray` plus a rolling `tray-beta` GitHub prerelease whose assets are replaced in place.
- `BETA.md` explicitly says the tray feature is still in progress, the public API may change between builds without notice, there are no universal macOS binaries, and nothing is published to npm. It is a fork prerelease, not an upstream release.
- Tests: `packages/native/src/tray.rs` has 3 unit tests (options conversion, menu-item defaults, event mapping). The pinned GPUI fork's `zed/crates/gpui/src/platform/tray.rs` has fake-tray tests including deferred event delivery. CI (`.github/workflows/ci.yml`) builds native bindings on macOS/Linux/Windows and runs React, Solid, native, example, typecheck, and CLI tests on macOS (plus Windows example tests). However, `rg 'createTray'` across `packages`, `examples`, `cli`, and `example-app` finds only `examples/todo.tsx`, `examples/tray.tsx`, `index.d.ts`, `js/host.ts`, and `renderer.rs`: there is **no automated tray integration test**. The todo example's latest commit only makes tray state visible in the UI/console for manual observation.
- Known gaps documented in the source: one tray per process; no Dock hiding / Accessory activation policy (`README.md:1117`, `.changeset/tray.md`); icon must be a file path; `setTrayTitle` is a no-op off macOS; Linux click semantics vary by desktop environment.

Positive maturity signals: real native backends on all three platforms, careful AppKit threading notes, a changeset, explicit docs, CI, a beta artifact pipeline, and a stateful validation example rather than a static screenshot.

## (c) How this changes the desktop shell landscape for Voce

| Candidate | What Voce gets | Cost / risk for Voce |
| --- | --- | --- |
| **Current Rust shell + SolidJS panel** (tao main loop, `wry` webview, `tray-icon` + `muda`, `src/panel/ipc.rs` `PanelCmd`/`PanelEvent`, cpal/ort/HAL driver) | Already proven menu-bar lifecycle, permissions/driver paths, 16 kHz capture, 48 kHz ring buffer, and a shipped IPC contract. Lowest delivery risk. | Two UI worlds (Rust state machine vs DOM panel), double JSON encoding over `evaluate_script`, webview binary weight, panel↔IPC sync burden documented in `src/panel/AGENTS.md`. |
| **Electrobun** | Web-standard UI with a maintained shell and tray conventions; would keep the panel in HTML/JS and could reuse much of the SolidJS work. | Still a webview shell; native audio/ONNX/HAL work remains a sidecar or Rust bridge. New process model, packaging, permission, and driver co-install paths to validate. Not explored in this spike; comparison is directional. |
| **Unified GPUIX shell** | One renderer for panel and tray, Solid adapter already matches Voce's panel language, no DOM/webview, GPUI handles windows/input/accessibility (AccessKit), native Rust addon can in principle host audio/inference, `quitMode: "explicit"` matches a menu-bar resident app. | Tray is fork-only and pre-1.0 on a validation branch; no upstream merge; no proven microphone/TCC or HAL driver story; no Voce IPC bridge; custom Rust code replaces the prebuilt addon (`README.md:3350+`); bus factor is a fork of a fork. |

The change is real but narrow: GPUIX removes the "writes its own webview shell" objection to a GPU-native Rust-adjacent UI and closes the specific tray gap that would otherwise block a menu-bar-first app. It does **not** remove the need for a native backend for audio and the driver, nor does it make the shell choice safe today. For Voce it is a credible direction to monitor, not a drop-in replacement for the current shell.

## (d) What a GPUIX-based Voce architecture would look like

```text
compiled GPUIX app (Bun or Hermes + @gpuix/solid + beta @gpuix/native)
  ├─ panel: SolidJS tree rendered by GPUI; same screens as assets/panel/src
  ├─ tray: renderer.createTray + setTrayMenu/onTrayEvent
  │    (state, pause toggle, test, quit; quitMode: "explicit")
  └─ bridge: localhost IPC/fetch or custom Node-API element
             ↕ commands/events mirroring PanelCmd/PanelEvent
native Rust backend sidecar
  ├─ cpal capture @16 kHz -> 512-sample ticks -> ort inference
  ├─ gate, enrollment, config in ~/.voce, ring buffer @48 kHz
  ├─ macOS TCC / permission + BlackHole status
  └─ VoceAudio.driver install/load and output device setup
app bundle: cargo-packager .app containing GPUIX binary + native sidecar + driver
```

The panel could keep its Solid signal model via `@gpuix/solid` (`README.md` Solid quick start), though DOM-only components, CSS, and browser APIs would need rewriting to GPUIX host nodes. Tray menu updates would be driven from the same app state that currently emits `PanelEvent`s; `activateWindow()` replaces the current `OpenPanel` tray action. The audio path must remain in native Rust: 32 ms ticks and the 3 s inference window must not depend on JS scheduling, and the 48 kHz ring buffer stays where it is now.

Two bridge options, neither built today:

1. Native sidecar with a local socket/HTTP endpoint; GPUIX JS uses `fetch` (available per the Hermes guide) and a typed command/event protocol. Keeps the prebuilt addon and isolates audio crashes, but adds a new transport to design, backpressure, and secure locally.
2. Custom Rust element/addon (`example-rust-native-element/`, `README.md:3350+`) that links `gpuix-native` and registers GPUI components; this fuses UI and backend but requires rebuilding/replacing the prebuilt `@gpuix/native` binary and carrying the Zed fork submodule in Voce's build.

`VoceAudio.driver` remains a separately packaged HAL plugin copied into the `.app`; GPUIX has no driver or audio-device APIs. macOS microphone/accessibility permission prompts would also need native code. The tray beta has no Dock-hiding option, so a menu-bar-only Voce would still show a Dock icon unless extra native work is done.

## (e) Risks and unknowns

- **Upstream risk:** tray is not on GPUIX `main` or upstream `remorses/gpuix` `main`; issue #109 is open. A beta API change could invalidate any spike, and the rolling `tray-beta` tag is replaced in place (no immutable version pin beyond contents carrying a build SHA).
- **Bus factor:** GPUIX is a fork of `remorses/gpuix` (0 stars on the espetro fork) which itself pins a custom `remorses/zed` GPUI fork, including an embedded macOS run-loop extension. Three layers of fork-specific maintenance for a menu-bar-critical component.
- **Unproven Voce surfaces:** no evidence of microphone/TCC permission helpers, BlackHole/HAL driver handling, or a production IPC bridge in GPUIX. Accessibility in README means AccessKit semantics for the UI tree, not OS permission flows.
- **Testing gap:** no automated tray test; validation is a manual example. No evidence of real-world menu-bar behavior over long sessions, on multiple macOS versions, or with VoiceOver.
- **Runtime unknowns:** GPU vs CPU usage of an idle GPUI app on a laptop (the README warns about a 73% CPU idle loop if `tick()` is misused), tray menu rebuild cadence at 32 ms state-update rates, window restoration/activation semantics, and notarization of a compiled Bun/Hermes binary plus native sidecar.
- **Licensing:** GPUIX root and `gpuix-native` are Apache-2.0; pinned `crates/gpui/Cargo.toml` declares Apache-2.0, while the Zed repo root ships both `LICENSE-APACHE` and `LICENSE-GPL` and GPUIX `THIRD_PARTY_NOTICES.md` records ported code. Legal review is still needed for the exact bundled fork combination, but nothing found here is a red flag for a closed-source Voce binary.
- **Distribution mismatch:** beta artifacts have no universal macOS binary; Voce currently ships one release with a bundled driver. A source-built native addon would require a Rust toolchain in release CI and a new codesigning/notarization flow.

## (f) Recommendation for Gate 4

**Treat GPUIX as a documented monitored alternative, not a full third arm and not out of scope.** It is more than theoretical now (real macOS NSStatusItem code, a stateful example, docs, CI, and a beta release path), so it should be recorded in the Gate 4 decision with the evidence above. But it is not ready to be compared as an equal implementation arm: the tray API is fork-only, one day old on a validation branch, untested automatically, and there is no Voce audio/driver/permission bridge.

Promote it to a full third arm only if, before the Gate 4 decision is locked: (1) tray lands on GPUIX `main` or an immutable published release, (2) an automated macOS tray integration test exists, and (3) a throwaway spike proves mic permission + a native audio sidecar + a panel/tray state round trip in a signed `.app`. Otherwise revisit after a defined interval or at the next gate.

## (g) Impact on the Gate 4 evaluation criteria

- Add an explicit criterion: "shell owns or cleanly integrates a first-class menu-bar tray with a maintained upstream path." GPUIX scores well on capability, poorly on maintenance; the current Rust shell scores well on both; Electrobun scores well on capability and needs its own verification.
- Weight **upstream/mainline status and release stability** separately from raw feature coverage. A feature on a validation branch with a rolling prerelease should not receive the same score as a shipped API.
- Keep "audio/driver/permission ownership stays native and testable" as a hard constraint. GPUIX does not change where the 16 kHz pipeline or HAL driver must live, so sidecar design quality should be scored for both Electrobun and GPUIX.
- Add "UI/native state duplication and IPC surface" as a differentiator: a unified GPUIX panel/tray could remove the `PanelCmd`/`PanelEvent` duplication, but only if the proposed bridge is proven; do not credit it on the basis of the todo example alone.
- Keep distribution/complexity criteria (codesigning, driver co-install, toolchain in release CI, bus factor) prominent. The GPUIX beta adds a prebuilt-addon constraint that makes the current single Rust build simpler by comparison.

Machine summary: `/tmp/voce-gpuix-tray-summary.json`.
