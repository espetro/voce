# Gate 4 shell-direction evaluation: Rust shell, Electrobun, GPUIX

Date: 2026-11-26. Research and one throwaway proof only. No production code, dependencies, or build outputs were changed in this worktree.

Verdict: `defer with gpuix monitored`. Keep the existing Rust shell for the next delivery increment. Do not adopt Electrobun now, because seven hard requirements are not all met in a way that justifies migration cost. GPUIX is a credible monitored alternative, not an equal implementation arm today. The parallel lane's evidence is in [docs/research/gpuix-tray-landscape.md](gpuix-tray-landscape.md).

## Scope and hard constraints

Realtime audio capture, 32 ms state ticks, 3 s inference windows, the 48 kHz ring buffer, and the macOS HAL driver must remain native. Electrobun or GPUIX may own only tray, panel, updater, and sidecar supervision. The current Rust shell is `tao` 0.35 plus `wry` 0.55 plus `tray-icon` 0.21 and `muda` 0.17 in `Cargo.toml`, with the panel bridge in `src/panel/ipc.rs` and the app state machine in `src/main.rs`. `scripts/release.sh` already builds, signs, notarizes, and assembles a universal `Voce.app` with the bundled HAL driver.

The audit target was the live project at [blackboardsh/electrobun](https://github.com/blackboardsh/electrobun). The `blackfishstudio/electrobun` URL in the task context returns 404 from the GitHub API, so all evidence below uses the live `blackboardsh` organization.

## Artifact and API audit

| Item | Verified value and source | Reading |
| --- | --- | --- |
| npm package | `electrobun@2.0.7`, published 2026-11-20T03:30:38Z, npm dist-tags `latest: 2.0.7`, `beta: 2.0.8-beta.5` ([registry metadata](https://registry.npmjs.org/electrobun)) | Current npm artifact is only a bootstrap. Its `lib/moved.cjs` throws and says APIs come from the Hutch devkit. |
| GitHub release | [v2.0.7](https://github.com/blackboardsh/electrobun/releases/tag/v2.0.7), published 2026-11-20T03:26:15Z, non-prerelease | Release artifacts include `electrobun-core-darwin-arm64.tar.gz` at 7.1 MB, `electrobun-core-darwin-x64.tar.gz` at 7.6 MB, and optional `electrobun-cef-darwin-arm64.tar.gz` at 147.7 MB. System-webview builds stay small; bundled CEF is heavy. |
| Repo activity | [GitHub API](https://api.github.com/repos/blackboardsh/electrobun): 13,115 stars, 387 forks, 180 open issues, pushed 2026-11-23, updated 2026-11-26, default branch `main`, MIT license | Active upstream with meaningful community size. Open-issue count is non-trivial but not a red flag by itself. |
| Toolchain artifact | `electrobun-hutch-linux-x64.tar.gz` v2.0.7 contains `hutch` 0.30.0 with revision `f371047548abcbaadec69064b8ab9680bc0557de`; local run of `hutch electrobun --help` works and downloads Bun 1.4.4 | Real CLI exists and orchestrates init, sync, prepare, build, run, dev, upload, and publish. |
| License | npm package `license: "MIT"`; GitHub repo license `MIT`; [LICENSE](https://github.com/blackboardsh/electrobun/blob/main/LICENSE) | Acceptable for a local-first product. Optional CEF and platform webview licenses still need a distribution review if bundled, but no blocking license fact was found. |
| Docs coverage | [Electrobun docs](https://docs.electrobun.dev/electrobun/) with API pages for [Tray](https://docs.electrobun.dev/electrobun/apis/tray/), [BrowserWindow](https://docs.electrobun.dev/electrobun/apis/browser-window/), [BrowserView](https://docs.electrobun.dev/electrobun/apis/browser-view/), [Bundled assets](https://docs.electrobun.dev/electrobun/apis/bundled-assets/), [Updater](https://docs.electrobun.dev/electrobun/apis/updater/), [Packaging](https://docs.electrobun.dev/electrobun/guides/bundling-and-distribution/), and [Code signing](https://docs.electrobun.dev/electrobun/guides/code-signing/) | Documentation is specific enough to audit implementation, not just a marketing page. Homepage claims about signed releases and small updates are backed by these API and build docs plus release artifacts. |

## Requirement-by-requirement evidence matrix

| Hard requirement | Evidence | Status | Product implication |
| --- | --- | --- | --- |
| 1. macOS tray and menu ownership documented or demonstrable | [Tray API](https://docs.electrobun.dev/electrobun/apis/tray/) defines `new Tray({ title, image, template, width, height })`, `setMenu`, `tray-clicked`, `setTitle`, `setImage`, `setVisible`, `getBounds`, lookup by id, and `remove`. Images accept `views://` URLs or absolute paths, and `template` is explicitly macOS template-image behavior. [Main API coverage table](https://docs.electrobun.dev/electrobun/apis/main/) shows tray create, show, title and events in Bun and Cottontail, with a documented native-SDK click gap for Zig, Rust, and Odin. [Utils](https://docs.electrobun.dev/electrobun/apis/utils/) also exposes `Utils.setDockIconVisible(false)`, which matters for a menu-bar resident app. | PASS for TypeScript main process | A Voce shell could own the status item and menu without writing AppKit code. If the shell main process were Rust through Electrobun's Rust SDK, tray clicks are not surfaced and menus are TypeScript-only, so the Bun main process would be required for shell-owned tray behavior. |
| 2. Panel webview can host existing SolidJS static UI | [Bundled assets](https://docs.electrobun.dev/electrobun/apis/bundled-assets/) maps `Resources/app/views` to `views://`, and `build.copy` copies HTML, CSS, and images unmodified. [Creating UI](https://docs.electrobun.dev/electrobun/guides/creating-ui/) shows `BrowserWindow({ url: "views://mainview/index.html" })`. The local `assets/panel/dist/index.html` is one 292,531-byte static HTML file with SHA-256 `eabc477afce033f13852e35017b56ad571102564d9aea7210c6ffbbc3c2e5ca0`. | PASS | The existing build artifact can be copied to `views://panel/index.html` without rewriting the panel. [BrowserView](https://docs.electrobun.dev/electrobun/apis/browser-view/) provides typed `defineRPC` as a cleaner replacement for the current `webview.evaluate_script` event path, but that is migration work, not a free reuse. |
| 3. Sidecar spawn, supervision, restart, lifecycle practical | Docs do not define an Electrobun-specific supervised-process API. A grep over all `docs/src/content/docs/electrobun/**/*.mdx` in release v2.0.7 found no app-level `spawn`, `child_process`, `supervise`, or restart API. The Bun main process can use `Bun.spawn`, and [Paths](https://docs.electrobun.dev/electrobun/apis/paths/) exposes `PATHS.RESOURCES_FOLDER` and `Utils.paths.userData` so a bundled native core can be located and managed from the shell. [Utils](https://docs.electrobun.dev/electrobun/apis/utils/) provides `quit` and `before-quit` handling for coordinated shutdown. The local proof below implemented and tested a restart supervisor in 61 lines of TypeScript. | PARTIAL, practical but not first-class | Spawn and supervision are feasible with framework primitives, but Voce owns crash policy, backoff, shutdown handoff, and child-lifecycle testing. This is real work that the Rust shell does not need today because audio and UI currently share one process. |
| 4. Credible updater, signing, packaging, macOS distribution | [Updater](https://docs.electrobun.dev/electrobun/apis/updater/) documents `checkForUpdate`, `downloadUpdate`, `applyUpdate`, transactional replacement, rollback, and delta patches from 2.0.6. [Packaging](https://docs.electrobun.dev/electrobun/guides/bundling-and-distribution/) documents DMG plus `.app.tar.zst` on macOS, stable and canary channels, static hosts, and six target combinations. [Code signing](https://docs.electrobun.dev/electrobun/guides/code-signing/) documents `codesign`, notarize, staple, `ELECTROBUN_DEVELOPER_ID`, App Store Connect API credentials, and custom entitlements including microphone. The v2.0.7 release assets listed above are concrete evidence that the pipeline runs. | PASS | Electrobun's update and signing story is stronger and more generic than Voce's custom `scripts/release.sh`. It does not solve the separate HAL-driver co-install and `coreaudiod` restart path already implemented in this repo. |
| 5. Native realtime audio and HAL driver stay outside the shell with narrow IPC | [Main API](https://docs.electrobun.dev/electrobun/apis/main/) supports six main-process runtimes, including a documented [Rust hello world](https://docs.electrobun.dev/electrobun/guides/hello-world-rust/) where the native core and webview creation are separate calls. [BrowserView](https://docs.electrobun.dev/electrobun/apis/browser-view/) documents typed RPC and notes that native SDKs use a raw JSON bridge with no typed RPC layer. No Electrobun API touches audio capture, ONNX inference, ring buffers, or CoreAudio HAL plugins. | PASS with a design caveat | The architecture constraint is compatible with Electrobun. The narrow IPC still has to be designed and tested, and the existing `PanelCmd`/`PanelEvent` contract would need a second transport or an RPC rewrite. |
| 6. License and dependency posture acceptable | npm and GitHub both report MIT. Release artifacts show a 7.1 MB darwin-arm64 core and a separate 147.7 MB CEF artifact, so system webview is the default path and CEF is opt-in. Hutch downloads pinned toolchains from release infrastructure, which adds a supply-chain dependency on Blackboard infrastructure. | PASS with caveats | Acceptable for evaluation. Distribution should pin and verify toolchain and devkit artifacts, and legal review should cover the chosen webview and CEF if enabled. |
| 7. Migration cost justified by concrete benefits over the current Rust shell | Current shell already has working tao/wry/tray, SolidJS panel, `PanelCmd`/`PanelEvent`, cpal/ort audio, HAL driver install, and signed notarized release script. Migration would add Hutch, devkit projection, a Bun main process, a new process model, a sidecar protocol, new packaging hooks for the driver, and a new IPC bridge. Benefits are a maintained cross-platform shell API, better updater, and typed RPC. No measured benefit on Voce's single macOS target outweighs those costs at this gate. | FAIL | This is the decisive requirement. Strong framework capability does not equal product-level justification. |

The hard-requirement result is six pass or partial and one fail, so `electrobun_hard_requirements_pass` is false.

## Minimal proof spike

The proof lives outside the worktree at `/tmp/electrobun-g4-proof` and is intentionally not committed. It contains a Hutch config that copies the real SolidJS `index.html` into `views/panel/index.html`, an Electrobun shell scaffold using `Tray`, `BrowserWindow`, and a `SupervisedProcess`, a Bun child-process restart supervisor, and two Bun tests. A real Hutch 0.30.0 Linux binary was downloaded to `/tmp/electrobun-g4-hutch` and its CLI help ran successfully. No `node_modules`, build output, or binary was added to git.

Final validation output:

```text
bun test v1.4.3 (c6da4a4d3)

 2 pass
 0 fail
 9 expect() calls
Ran 2 tests across 1 file. [91.00ms]
EXIT_STATUS=0
```

The first test run failed because the assertion raced the third spawn after two restarts. The test was corrected to wait for starts explicitly and the rerun above is green. The proof establishes static-bundle placement and Bun-level crash restart, not real webview rendering, macOS menu-bar behavior, or packaged macOS distribution.

Exact blocker for a fuller proof on this VPS: `DISPLAY` is empty and `ldconfig -p` reports no WebKitGTK, GTK-3, or Ayatana AppIndicator libraries, while macOS `NSStatusItem` cannot exist on Linux. Xvfb is installed, but the native webview stack is absent, so a real Electrobun window and tray could not be launched here. Installing system GUI dependencies was out of scope for this 45-minute direction gate.

## GPUIX as a third direction

The parallel lane found real macOS `NSStatusItem` tray code in GPUIX's `examples/todo-validation` branch at commit `a5def75331efe2ea5c4d0bad673cba0e77bf6c33`, with `createTray`, `setTrayMenu`, `setTrayTitle`, `setTrayTooltip`, `removeTray`, and `onTrayEvent`, plus a stateful todo example and CI. It also found that the tray API is fork-only, roughly one day old, absent from GPUIX `main` and upstream `remorses/gpuix` `main`, has no automated tray integration test, and has no proven microphone, TCC, HAL driver, or Voce IPC bridge. The lane's recommendation is monitored alternative rather than equal third arm.

GPUIX is more interesting than a theoretical option because it removes the "writes its own webview shell" objection and offers a Solid adapter that matches the panel language. It does not change the hard constraint that audio and inference stay native, and it adds a three-layer fork dependency plus a prebuilt native addon and new notarization flow. It should be revisited only when tray lands on an immutable release or `main`, an automated macOS tray integration test exists, and a throwaway signed-app spike proves mic permission plus native audio sidecar plus a panel/tray state round trip.

## Migration risks

- Process model risk: Voce currently runs UI, audio, state, and driver control in one Rust process. A shell plus sidecar split introduces lifecycle, restart, backpressure, and crash-isolation behavior that has not been exercised in this repo.
- IPC risk: `PanelEvent::to_js_call()` uses serde double-encoding over `evaluate_script`, as documented in `src/panel/AGENTS.md`. Electrobun's typed RPC is better, but porting every command and event is a separate project with its own regression surface.
- Packaging risk: the HAL driver must be co-installed and `coreaudiod` restarted. Electrobun's generic DMG and updater flow does not demonstrate this path, and modifying packaged resources invalidates code signatures.
- Runtime risk: Bun or Cottontail becomes part of the shipped shell. Bun 1.4.4 was downloaded by Hutch during this audit, which shows the toolchain works but also adds a new runtime to sign, update, and debug.
- Dependency risk: MIT license is acceptable, yet the v2 stack depends on Hutch devkit projection and optional CEF. Release builds would need pinned verification of those artifacts.
- Opportunity cost: Gate 4's remaining product risk is audio reliability, driver behavior, and real-speech evaluation, not shell framework novelty. Switching shells does not reduce those risks.

## Decision and revisit criteria

Verdict: `defer with gpuix monitored`.

Keep the Rust shell. Do not adopt Electrobun in this cycle because hard requirement 7 fails and hard requirement 3 is only partial. Continue to monitor GPUIX under the three promotion conditions above. Reopen the shell decision if one of the following becomes true: a measured macOS packaging or tray maintenance problem in the Rust shell; a shipped Electrobun app spike that includes the HAL driver, microphone permission, native sidecar, signed notarized `.app`, and update round trip; or GPUIX meeting its promotion conditions.

The narrow next step after this gate is to keep shell work out of the audio path and invest in the native backend and driver validation already planned. No Electrobun or GPUIX code should be merged into production as a result of this evaluation.

## Sources consulted

- [Electrobun repository](https://github.com/blackboardsh/electrobun), [repo API](https://api.github.com/repos/blackboardsh/electrobun), [v2.0.7 release](https://github.com/blackboardsh/electrobun/releases/tag/v2.0.7), [npm registry](https://registry.npmjs.org/electrobun)
- [Tray API](https://docs.electrobun.dev/electrobun/apis/tray/), [Main API](https://docs.electrobun.dev/electrobun/apis/main/), [BrowserWindow](https://docs.electrobun.dev/electrobun/apis/browser-window/), [BrowserView](https://docs.electrobun.dev/electrobun/apis/browser-view/), [Bundled assets](https://docs.electrobun.dev/electrobun/apis/bundled-assets/), [Paths](https://docs.electrobun.dev/electrobun/apis/paths/), [Utils](https://docs.electrobun.dev/electrobun/apis/utils/), [Updater](https://docs.electrobun.dev/electrobun/apis/updater/), [Packaging](https://docs.electrobun.dev/electrobun/guides/bundling-and-distribution/), [Code signing](https://docs.electrobun.dev/electrobun/guides/code-signing/), [Rust hello world](https://docs.electrobun.dev/electrobun/guides/hello-world-rust/)
- Local sources: `Cargo.toml`, `src/main.rs`, `src/panel/ipc.rs`, `src/panel/AGENTS.md`, `scripts/release.sh`, `assets/panel/dist/index.html`, `docs/research/gpuix-tray-landscape.md`
