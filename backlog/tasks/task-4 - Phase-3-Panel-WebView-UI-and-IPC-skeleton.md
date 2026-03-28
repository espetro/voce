---
id: TASK-4
title: 'Phase 3: Panel WebView UI and IPC skeleton'
status: Done
assignee:
  - '@claude'
created_date: '2026-03-27 21:06'
updated_date: '2026-03-27 21:42'
labels:
  - phase-3
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
wry WebView panel anchored below tray icon, 320px wide, shows/hides on tray click/focus-lost. Full onboarding HTML/CSS/JS with all state screens. Rust<->JS IPC wired.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Tray icon click opens 320px panel positioned below the menubar icon
- [x] #2 Clicking outside the panel hides it
- [x] #3 All onboarding states render correctly in HTML (MODEL_LOADING through ACTIVE_STANDBY)
- [x] #4 JS send() posts IPC messages visible in Rust tracing logs
- [x] #5 Rust dummy StateChanged events update panel DOM via evaluate_script
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Write full panel HTML/CSS/JS (all onboarding states, IPC bridge)
2. Update src/panel/ipc.rs with correct serde types + to_js_call helper
3. In main.rs: create panel window (decorations=false, always_on_top, visible=false)
4. Build wry WebView on that window with ipc_handler → AppEvent::PanelCommand
5. TrayIcon::Click → reposition panel below icon, set_visible(true)
6. WindowEvent::Focused(false) → set_visible(false)
7. user_event: StateChanged/BlackHoleStatus → send_to_panel via evaluate_script
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase 3 complete.

- assets/panel/index.html: all 10 onboarding screens in one document, toggled via .screen.active CSS class
- assets/panel/panel.js: window.__voce_update(json) entry point, send(obj) IPC bridge, handles all PanelEvent variants
- src/panel/ipc.rs: serde PanelCmd (JS→Rust) and PanelEvent (Rust→JS) enums; to_js_call() serialises with proper double-quote escaping for evaluate_script
- main.rs: wry WebViewBuilder on a decorations=false AlwaysOnTop window; panel.js inlined into HTML at startup; IPC handler parses JSON → AppEvent::PanelCommand; TrayIcon::Click toggles panel with position anchored below tray icon rect; WindowEvent::Focused(false) hides panel; panel auto-opens on first launch after ModelReady
- wry ipc_handler signature: Fn(Request<String>) where Request is wry::http::Request
<!-- SECTION:NOTES:END -->
