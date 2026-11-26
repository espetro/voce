# Voce shell direction: GPUIX

Date: 2026-11-26
Status: direction (M1 of the GPUIX prototype program, in flight)
Depends on: GPUIX hands-on validation (`/Users/josocjoq/voce-shell-validation/gpuix/REPORT.md`,
gates 1-4 of the spike program complete)
Reference: https://cap.so/blog/rebuilding-cap-in-gpui

## Direction statement

Voce's long-term desktop shell is GPUIX (espetro/gpuix, a thin JS/React layer over a Zed-fork
gpui renderer): a tray-first, GPU-drawn, Rust-native shell with the existing voce Rust core
(`src/`, scriptc-or-pyscriptc sidecar behind the `voce_native` C ABI) kept as a decoupled child
process via stdio IPC, or later compiled as a napi addon following gpuix-native's build pipeline.
The SolidJS/webview panel is retained for parity until the GPUIX panel reaches feature equivalence
(M3), after which GPUIX becomes the only shell.

The model direction is unchanged: speaker similarity gate (see docs/architecture.md), independent
of shell choice.

### Why (validated evidence)

- Tray creation via NSStatusItem works on macOS arm64; the tray architecture (id-in/id-out
  channel, no JS closures in native callbacks, AppKit-thread discipline) matches the design agreed
  in remorses/gpuix#109.
- Clone-to-running on this Mac: ~6-7 min cold, seconds warm; Bun-first toolchain; `bun --hot`
  live reload works.
- AccessKit accessibility is built in (far beyond the current SolidJS panel).
- Overlay/HUD needs are covered by `WindowKind::PopUp/AnchoredPopup/Floating` (NSPanel) and the
  `<anchored>` element; true multi-window is not needed for M1-M3.
- Cap's production migration corroborates the stack at scale: 77% less CPU and 88% less memory
  than the Tauri build, ~49% smaller macOS download, and one-implementation cross-platform
  rendering. Cap's lessons that apply to a menu-bar-first app like Voce:
  - Keep the engine independent of the shell; in-process or sidecar, not bridged through JSON in
    a web view. Voce already has this via the voce_native C ABI sidecar.
  - GPUI redraws like a game but must idle properly; patch/limit wake-ups when windows are static
    (Cap: 121 -> 1 wake-ups/s). Relevant for Voce's always-on tray/HUD.
  - Budget for rebuilding browser-freebies: text fields, menus, focus/keyboard navigation are
    hand-built in GPUI. Carry this in M3 (panel parity) estimates.
  - Expect to carry small framework patches; having the framework source in-tree is what makes
    OS-level breakage (macOS 26 visibility flag) fixable. Our fork strategy below formalizes this.
  - Ship the native shell as an opt-in preview first, then flip the default only after
    interaction-by-interaction parity (Cap: 0.6.0 experimental -> 0.6.1 default). Mirrored by our
    M3 gate.
  - No hot reload parity with Vite; restart loop is seconds, not minutes. Plan QA accordingly.
  - GPUI is young, Zed-first, small ecosystem; prefer plain composition over library pulls.

### Known constraints accepted for now

- Single window per renderer (README.md:831). Voce's panel + HUD live in one window via
  `<anchored>` or a focus:false window. True multi-window is zed-fork-scale; not a blocker.
- Tray: flat menu only; click vs menu mutual exclusivity; no `setTrayIcon`. See gap list.

## Upstream contribution strategy (standing policy)

Any GPUIX blocker follows this sequence, in order:

1. Check gpuix docs and GPUI support first; the gap may already be solvable in our layer.
2. If the gap is real (reproduced, not speculative): file a reproducer issue in our fork
   (espetro/gpuix), implement the fix on a branch in the fork, merge and release in the fork.
3. Contribute both the issue and the PR upstream to remorses/gpuix.

Rules: never approve deployments; opening PRs is fine. Fixes should be small, located, and match
designs already agreed upstream (e.g. remorses/gpuix#109 for tray).

## Milestone plan (GPUIX prototype program)

| Milestone | Scope | Exit criteria |
|---|---|---|
| M1 | Scaffold + tray: `/Users/josocjoq/voce-gpuix-prototype`, tray-first app skeleton (status line, mute toggle placeholder, quit), opens window from tray, trivial sidecar with restart-on-crash, screencapture evidence | builds + runs on this Mac, evidence/ captured, clean quit, no stray processes |
| M2 | Sidecar audio integration: real voce_native sidecar, stdio IPC, state -> tray title/icon updates | audio events drive tray state end-to-end |
| M3 | Panel parity: settings panel screens against assets/panel parity list, `<anchored>` HUD, AccessKit | interaction-by-interaction parity review, opt-in switch shipped |
| M4 | Distribution packaging: fix standalone-compile native binding path, cargo-packager/compile script, bundle sidecar + Info.plist (LSUIElement, NSMicrophoneUsageDescription) | signed/notarized .app runs mic-gated on a clean Mac |
| M5 | Upstream-contribution completion: (issue, PR) pairs for fork gaps merged in espetro/gpuix and contributed to remorses/gpuix | PRs open upstream, fork releases carry the fixes |

## Validated gap list (espetro/gpuix issues)

- #1 Reproducer: `bun run compile` standalone binary cannot load the native binding (blocks M4)
  https://github.com/espetro/gpuix/issues/1
- #2 Reproducer: tray title/tooltip/menu interactions are not verifiable from an automated test
  https://github.com/espetro/gpuix/issues/2
- #3 Reproducer: tray menu events only dispatch after window activation; flat menu; click-only
  https://github.com/espetro/gpuix/issues/3

Additional small fixes identified but not blocking M1: `setTrayIcon` exposure (~25 lines),
click-vs-menu coexistence (~50 lines, matches remorses/gpuix#109), `NSMicrophoneUsageDescription`
in the compile script's Info.plist (one key).
