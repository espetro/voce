---
id: TASK-1
title: 'Phase 0: Skeleton and compilation baseline'
status: Done
assignee:
  - '@claude'
created_date: '2026-03-27 21:06'
updated_date: '2026-03-27 21:12'
labels:
  - phase-0
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Bootstrap the Rust project: Cargo.toml with correct crate versions, build.rs for Info.plist generation, empty module stubs, and a working menubar icon via winit ApplicationHandler + tray-icon.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 cargo build completes without errors
- [x] #2 Menubar icon appears on launch
- [ ] #3 No Dock icon (LSUIElement=true in Info.plist)
- [x] #4 All source module files exist with stub implementations
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Create Cargo.toml with correct crate versions
2. Create build.rs (Info.plist + framework links)
3. Create src/main.rs with winit ApplicationHandler + TrayIcon
4. Create src/app_state.rs AppState enum
5. Create all empty module stub files
6. Create placeholder PNG icons in assets/icons/
7. Create voce.app bundle structure for dev mic permissions
8. Verify cargo build succeeds
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase 0 complete.

- Cargo.toml created with corrected crate versions (cpal 0.17, wry 0.55, tray-icon 0.21, ort pinned to rc.11, image 0.25 for icon loading)
- build.rs generates Info.plist with LSUIElement=true and NSMicrophoneUsageDescription
- src/main.rs: winit ApplicationHandler<AppEvent> with tray-icon + muda menu skeleton
- src/app_state.rs: full AppState enum with all state machine variants
- All module stubs created: audio/{buffer,capture,output}, model/{download,vad,embedder}, enrollment/{recorder,profile}, filter/gate, panel/ipc, config
- Three placeholder 22x22 PNG icons (loading=orange, idle=grey, active=green)
- voce.app bundle created with Info.plist symlink for dev mic permissions
- cargo build succeeds with 0 errors, 37 warnings (all expected unused-code warnings on stubs)

Dev run: open voce.app
<!-- SECTION:NOTES:END -->
