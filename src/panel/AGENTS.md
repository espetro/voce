# src/panel/ — IPC Bridge (Rust ↔ JavaScript)

The IPC bridge in `src/panel/ipc.rs` defines the contract between the Rust main thread and the JavaScript panel. It handles two directions: **commands** (JS → Rust) and **events** (Rust → JS).

---

## Architecture

```
┌─────────────────────────────────┐
│   JavaScript Panel              │
│ (SolidJS, assets/panel/)        │
└────────────┬────────────────────┘
             │ window.ipc.send(cmd)
             ▼
┌─────────────────────────────────┐
│  Webview IPC Server             │
│  (recv message → emit AppEvent) │
└────────────┬────────────────────┘
             │
             ▼
┌─────────────────────────────────┐
│  main.rs event loop             │
│  (match AppEvent::PanelCommand) │
└────────────┬────────────────────┘
             │
             ▼
┌─────────────────────────────────┐
│ Inference task / State machine  │
│ (emit AppEvent → PanelEvent)    │
└────────────┬────────────────────┘
             │ to_js_call()
             ▼
┌─────────────────────────────────┐
│ webview.evaluate_script()       │
│ window.__voce_update(json)      │
└────────────┬────────────────────┘
             │
             ▼
┌─────────────────────────────────┐
│ useAppState.ts signal update    │
│ (reactive panel UI)             │
└─────────────────────────────────┘
```

---

## PanelCmd Enum (JS → Rust)

Commands sent from the panel via `window.ipc.send(JSON)`. Defined in `src/panel/ipc.rs`.

| Variant | JSON `cmd` value | Payload | Meaning |
|---------|------------------|---------|---------|
| `StartRecording{index}` | `"start_recording"` | `index: u8` (1 or 2) | Begin enrollment recording |
| `StartTest` | `"start_test"` | — | Begin test-mode recording (filter off) |
| `StopTest` | `"stop_test"` | — | Stop test recording |
| `ReplayTest` | `"replay_test"` | — | Play back test recording (filtered) |
| `ReplayTestRaw` | `"replay_test_raw"` | — | Play back test recording (unfiltered) |
| `StopPlayback` | `"stop_playback"` | — | Stop any active playback |
| `ConfirmEnrollment` | `"confirm_enrollment"` | — | Finalize enrollment, start filter |
| `Reenroll` | `"reenroll"` | — | Reset to onboarding, discard profile |
| `OpenBlackholeLink` | `"open_blackhole_link"` | — | Open BlackHole download link |
| `ToggleFilter` | `"toggle_filter"` | — | Pause/resume filter (Phase 8) |
| `SetNoiseSuppression{enabled}` | `"set_noise_suppression"` | `enabled: bool` | Enable/disable denoiser |
| `InstallDriver` | `"install_driver"` | — | Background driver installation |
| `FullReset` | `"full_reset"` | — | Full reset (wipe config + driver) |
| `OpenSystemSound` | `"open_system_sound"` | — | Open macOS Sound preferences |

### JSON Format

Commands are serialized with a `cmd` tag field and flattened payload:

```json
{"cmd": "start_recording", "index": 1}
{"cmd": "start_test"}
{"cmd": "set_noise_suppression", "enabled": true}
```

---

## PanelEvent Enum (Rust → JS)

Events sent from Rust to JS via `webview.evaluate_script(window.__voce_update(...))`. Defined in `src/panel/ipc.rs`.

| Variant | JSON `event` value | Payload | Meaning |
|---------|-------------------|---------|---------|
| `StateChanged{state}` | `"state_changed"` | `state: "IDLE"\|"MODEL_LOADING"\|...` | App state changed (see `app_state.rs` for values) |
| `RecordingProgress{index, elapsed_s, speech_s}` | `"recording_progress"` | `index, elapsed_s, speech_s` (u32) | Live recording meter |
| `RecordingComplete{index, speech_s}` | `"recording_complete"` | `index, speech_s` (u32) | Recording saved |
| `RecordingInvalid{index, reason}` | `"recording_invalid"` | `index, reason` (e.g., `"insufficient_speech"`) | Recording rejected |
| `TestProgress{elapsed_s}` | `"test_progress"` | `elapsed_s: u32` | Live test recording meter |
| `FilterStats{similarity, is_passing}` | `"filter_stats"` | `similarity: f32`, `is_passing: bool` | Real-time filter decision (per-chunk) |
| `BlackholeStatus{found}` | `"blackhole_status"` | `found: bool` | Virtual mic detected? |
| `DownloadProgress{fraction}` | `"download_progress"` | `fraction: f32` (0.0–1.0) | Model download % |
| `TestStats{voice_pct}` | `"test_stats"` | `voice_pct: f32` | Test recording voice % (after playback) |
| `FilterPaused{paused}` | `"filter_paused"` | `paused: bool` | Filter toggle state (Phase 8) |
| `NoiseSuppression{enabled}` | `"noise_suppression"` | `enabled: bool` | Denoiser toggle state |
| `DriverStatus{installed, device_found}` | `"driver_status"` | `installed: bool`, `device_found: bool` | Driver installation + device detection status |
| `ResetComplete{success}` | `"reset_complete"` | `success: bool` | Full reset operation completed |

### JSON Format

Events are serialized with an `event` tag field:

```json
{"event": "state_changed", "state": "RECORDING_1"}
{"event": "filter_stats", "similarity": 0.92, "is_passing": true}
{"event": "download_progress", "fraction": 0.35}
```

---

## JavaScript Serialisation (to_js_call)

The `PanelEvent::to_js_call()` method in `src/panel/ipc.rs` wraps the JSON event inside a JavaScript function call using **serde double-encoding** — the event is serialised to JSON, then that JSON string is itself JSON-encoded to produce a valid JS string literal.

```rust
pub fn to_js_call(&self) -> anyhow::Result<String> {
    let payload = serde_json::to_string(self)?;            // inner JSON
    Ok(format!("window.__voce_update({})", serde_json::to_string(&payload)?)) // valid JS literal
}
```

**Why double-encoding?**
`serde_json::to_string(&string)` produces a properly-escaped JSON string literal, correctly handling all Unicode escapes including U+2028/U+2029 line separators that would break a naively-escaped JS string. The resulting JS call passes a single string argument that the panel parses with `JSON.parse`.

**Example:**

```rust
PanelEvent::StateChanged { state: "FILTERING" }
// Inner JSON:  {"event":"state_changed","state":"FILTERING"}
// Outer (as JS literal): "\"{ \\\"event\\\":\\\"state_changed\\\",...}\""
// Final JS call: window.__voce_update("{\"event\":\"state_changed\",\"state\":\"FILTERING\"}")
```

The panel JS receives a string and calls `JSON.parse(jsonStr)` to reconstruct the event object.

---

## How to Add a New Command/Event

### 5-Step Checklist: Adding a PanelCmd

1. **Add enum variant** to `PanelCmd` in `src/panel/ipc.rs`:
   ```rust
   pub enum PanelCmd {
       // ... existing
       NewCommand { field1: u8, field2: bool },
   }
   ```
   Serde will auto-derive the `cmd` tag as `"new_command"` (snake_case).

2. **Ensure parse_cmd handles it** in `src/panel/ipc.rs`:
   ```rust
   pub fn parse_cmd(body: &str) -> anyhow::Result<PanelCmd> {
       serde_json::from_str(body).map_err(Into::into)
   }
   ```
   (Already generic; no change needed if serde derives correctly.)

3. **Match in main.rs** event loop:
   ```rust
   AppEvent::PanelCommand(cmd) => match cmd {
       PanelCmd::NewCommand { field1, field2 } => {
           // e.g., emit InferenceCmd::* or change state
       }
   }
   ```

4. **Add JS-side handler** in `assets/panel/src/`:
   - Add button / UI trigger that calls `window.ipc.send("new_command", { field1, field2 })`
   - See `src/panel/AGENTS.md` section "Screen → Component Mapping" (in `assets/panel/AGENTS.md`)

5. **Update docs**: Link the new command in the PanelCmd table above.

### 5-Step Checklist: Adding a PanelEvent

1. **Add enum variant** to `PanelEvent` in `src/panel/ipc.rs`:
   ```rust
   pub enum PanelEvent {
       // ... existing
       NewEvent { field1: f32, field2: &'static str },
   }
   ```

2. **Emit from Rust** (main thread or inference task):
   ```rust
   let event = PanelEvent::NewEvent { field1: 0.85, field2: "ok" };
   let js_call = event.to_js_call()?;
   webview.evaluate_script(&js_call)?;
   ```

3. **Add to useAppState.ts** signal mapping (in `assets/panel/src/hooks/useAppState.ts`):
   ```typescript
   window.__voce_update = (jsonStr: string) => {
       const event = JSON.parse(jsonStr);
       if (event.event === "new_event") {
           setAppState("newEventField1", event.field1);
           // update other signals as needed
       }
   };
   ```

4. **Update UI components** to react to the signal:
   ```typescript
   const [appState] = useAppState();
   <Show when={appState.newEventField1 > 0.8}>
       <p>Event firing!</p>
   </Show>
   ```

5. **Update docs**: Add row to PanelEvent table above.

---

## Sync Invariant (Critical)

⚠️ **PanelCmd and PanelEvent changes must be coordinated across multiple files.**

If you add a new command or event, you must update:

1. **`src/panel/ipc.rs`** — enum definition + serde tags
2. **`src/main.rs`** — match on the new PanelCmd / emit PanelEvent
3. **`assets/panel/src/hooks/useIpc.ts`** — (if new command) add `send()` method
4. **`assets/panel/src/hooks/useAppState.ts`** — (if new event) add signal + window.__voce_update handler
5. **Relevant UI components** in `assets/panel/src/components/` or `assets/panel/src/screens/`

**Failing to sync one of these will result in:**
- Commands not being parsed (Rust error)
- Events not reaching JS (silent failure, no UI update)
- Type mismatches (TypeScript error in build)

Use the 5-step checklists above to avoid missing a file.

---

## IPC Server Details (src/panel/mod.rs)

The webview is initialized once, and an IPC server is spawned to receive commands:

```rust
// Pseudo-code from src/main.rs
let webview = WebviewBuilder::new(&window)
    .with_ipc_handler(move |_window, request| {
        if let Ok(cmd) = parse_cmd(&request.body) {
            tx.send(AppEvent::PanelCommand(cmd)).ok();
        }
    })
    .build()?;
```

When JS sends `window.ipc.send("start_recording", {index: 1})`:
1. Webview IPC handler receives the JSON body
2. `parse_cmd()` deserializes → `PanelCmd::StartRecording{index: 1}`
3. Wrapped in `AppEvent::PanelCommand()` → sent to main event loop via `tx`
4. Main loop matches and dispatches

---

## Testing the IPC Contract

1. **Unit tests** in `src/panel/ipc.rs`:
   ```bash
   cargo test -p voce panel::
   ```
   Tests `parse_cmd()` and `to_js_call()` round-tripping.

2. **Manual QA**:
   - Run `just dev-app`
   - Open browser DevTools, run in console:
     ```javascript
     window.ipc.send("start_recording", {index: 1})
     ```
   - Verify Rust app receives it (check terminal logs)

3. **Integration**: Run full test cycle (record → filter) via the UI to ensure commands and events flow correctly.
