# assets/panel/ — Frontend Architecture

The frontend is a SolidJS 1.x single-file application that communicates with the Rust backend via IPC. It presents 13 screens that guide the user through enrollment, testing, and active filtering.

---

## Tech Stack

| Layer | Tech | Version | Notes |
|-------|------|---------|-------|
| **Framework** | SolidJS | 1.9+ | Reactive signals; no virtual DOM |
| **UI Components** | ArkUI (@ark-ui/solid) | 5.37+ | Unstyled, accessible components |
| **Styling** | TailwindCSS | 4.3+ | Utility-first CSS |
| **Icons** | lucide-solid | 1.17+ | SVG icon library |
| **Build** | Vite | 8.0+ | Fast bundler, HMR dev server |
| **Single File** | vite-plugin-singlefile | 2.3+ | Builds to single `dist/index.html` for embedding |
| **Plugin** | vite-plugin-solid | 2.11+ | SolidJS JSX compiler |
| **Language** | TypeScript | 6.0+ | Strict type checking |

---

## Source Layout

```
assets/panel/src/
├── index.tsx           # App mount point
├── App.tsx             # Top-level router (screen switching)
├── types.ts            # Type definitions (Screen enum, IPC types)
├── components/
│   ├── Loading.tsx     # Spinner during model download
│   ├── DriverSetup.tsx # Driver installation + device setup
│   ├── Onboarding.tsx  # Enrollment flow (recording 1/2, adapting)
│   ├── Test.tsx        # Test mode (filter validation)
│   ├── Active.tsx      # Running filter (filter stats, playback)
│   ├── Settings.tsx    # User preferences
│   ├── Footer.tsx      # Playback controls (replay, stop)
│   └── ToggleSwitch.tsx # Reusable switch component
├── hooks/
│   ├── useAppState.ts  # Reactive state + window.__voce_update listener
│   └── useIpc.ts       # IPC sender methods (window.ipc.send wrapper)
└── assets/
    └── logo-on.svg     # App icon source (white on dark background)

dist/
└── index.html          # Vite build output (committed to repo)
```

---

## Screen → Component Mapping

The `Screen` type is a union of 13 screen names. `App.tsx` routes based on `state.screen()`:

| Screen | Component | State | Purpose |
|--------|-----------|-------|---------|
| `loading` | `Loading` | `IDLE` / `MODEL_LOADING` | Spinner; downloading ONNX models |
| `driver-setup` | `DriverSetup` | `ACTIVE_STANDBY` (conditional) | Driver installation + device setup |
| `onboarding-ready` | `Onboarding` | `ONBOARDING_READY` | "Ready to record sample 1" |
| `recording-1` | `Onboarding` | `RECORDING_1` | Recording enrollment sample 1 |
| `recording-1-invalid` | `Onboarding` | — (custom) | "Insufficient speech; try again" |
| `recording-1-complete` | `Onboarding` | — (custom) | "Sample 1 saved; ready for sample 2" |
| `recording-2` | `Onboarding` | `RECORDING_2` | Recording enrollment sample 2 |
| `recording-2-invalid` | `Onboarding` | — (custom) | "Insufficient speech; try again" |
| `adapting` | `Onboarding` | `ADAPTING` | "Processing embeddings..." |
| `test-ready` | `Test` | `TEST_READY` | "Ready to test filter" (first time) |
| `testing` | `Test` | `TESTING` | Recording test audio (microphone active) |
| `playback-filtered` | `Test` | `PLAYING_BACK` | Playing test audio through filter |
| `playback-raw` | `Test` | `PLAYING_BACK_RAW` | Playing test audio unfiltered |
| `test-complete` | `Test` | `TEST_READY` (after test) | "Test complete; results shown" |
| `active` | `Active` | `ACTIVE_STANDBY` / `FILTERING` | Running filter; showing live stats |
| `settings` | `Settings` | (any) | User preferences, BlackHole setup, info |

**Route Logic (App.tsx):**
```typescript
ONBOARDING_SCREENS.includes(screen) ? Onboarding :
TEST_SCREENS.includes(screen) ? Test :
screen === 'active' ? Active :
screen === 'settings' ? Settings :
Loading
```

---

## Reactive State API (useAppState.ts)

The `useAppState()` hook exports a single state object with signals and setters:

| Signal | Type | Purpose |
|--------|------|---------|
| `screen()` / `setScreen()` | `Screen` | Current UI screen |
| `blackholeFound()` / `setBlackholeFound()` | `boolean` | Virtual mic detected? |
| `hasTestedOnce()` / `setHasTestedOnce()` | `boolean` | Test run completed? (changes `TEST_READY` → `test-complete` screen) |
| `rec1Elapsed()` / `setRec1Elapsed()` | `number` | Recording 1 elapsed seconds (live meter) |
| `rec2Elapsed()` / `setRec2Elapsed()` | `number` | Recording 2 elapsed seconds (live meter) |
| `testElapsed()` / `setTestElapsed()` | `number` | Test recording elapsed seconds (live meter) |
| `filterPaused()` / `setFilterPaused()` | `boolean` | Filter pause state (Phase 8) |
| `similarity()` / `setSimilarity()` | `number \| null` | Real-time filter similarity (0.0–1.0) |
| `isPassing()` / `setIsPassing()` | `boolean` | Filter decision (passing or not) |
| `voicePct()` / `setVoicePct()` | `number` | Test recording voice % (0–100) |
| `noiseSuppression()` / `setNoiseSuppression()` | `boolean` | Denoiser enabled? |
| `downloadFraction()` / `setDownloadFraction()` | `number` | Model download progress (0.0–1.0) |
| `isDownloading()` / `setIsDownloading()` | `boolean` | Model download in progress? |
| `driverInstalled()` / `setDriverInstalled()` | `boolean` | Driver installation status |
| `voceDeviceFound()` / `setVoceDeviceFound()` | `boolean` | Voce Microphone device detected? |
| `hasSeenDriverSetup()` / `setHasSeenDriverSetup()` | `boolean` | Session flag: skip driver-setup on re-entry? |
| `resetSuccess()` / `setResetSuccess()` | `boolean \| null` | Full reset operation result |

### window.__voce_update Handler

The hook registers `window.__voce_update(jsonStr)` to receive events from Rust:

```typescript
(window as VoceWindow).__voce_update = (jsonStr: string) => {
  const msg = JSON.parse(jsonStr);
  switch (msg.event) {
    case 'state_changed':
      handleStateChanged(msg.state);  // Updates screen + dependent signals
      break;
    case 'recording_progress':
      // Update rec1Elapsed or rec2Elapsed
      break;
    case 'filter_stats':
      setSimilarity(msg.similarity);
      setIsPassing(msg.is_passing);
      break;
    // ... etc
  }
};
```

When Rust emits `PanelEvent::StateChanged{state: "RECORDING_1"}`, the handler calls `handleStateChanged("RECORDING_1")` which sets both `setScreen("recording-1")` and any dependent signals (e.g., `setRec1Elapsed(0)`).

---

## IPC Sender API (useIpc.ts)

The `useIpc()` hook returns an object with methods to send commands to Rust:

| Method | JSON Command | Payload | Purpose |
|--------|--------------|---------|---------|
| `startRecording(index)` | `start_recording` | `index: 1\|2` | Start enrollment recording |
| `startTest()` | `start_test` | — | Start test-mode recording |
| `stopTest()` | `stop_test` | — | Stop test recording |
| `replayTest()` | `replay_test` | — | Play back filtered test audio |
| `replayTestRaw()` | `replay_test_raw` | — | Play back unfiltered test audio |
| `stopPlayback()` | `stop_playback` | — | Stop playback |
| `confirmEnrollment()` | `confirm_enrollment` | — | Finalize enrollment, start filter |
| `reenroll()` | `reenroll` | — | Discard profile, restart |
| `openBlackholeLink()` | `open_blackhole_link` | — | Open BlackHole download in browser |
| `toggleFilter()` | `toggle_filter` | — | Pause/resume filter (Phase 8) |
| `setNoiseSuppression(enabled)` | `set_noise_suppression` | `enabled: bool` | Enable/disable denoiser |
| `installDriver()` | `install_driver` | — | Background driver installation |
| `fullReset()` | `full_reset` | — | Full reset (wipe config + driver) |
| `openSystemSound()` | `open_system_sound` | — | Open macOS Sound preferences |

### Typical Usage

```typescript
const ipc = useIpc();
<button onclick={() => ipc.startRecording(1)}>Record Sample 1</button>
```

Implementation wraps `window.ipc.send()` which sends JSON to the Rust webview IPC handler.

---

## Build Commands

From the repo root:

| Command | Purpose | Output |
|---------|---------|--------|
| `pnpm --dir assets/panel install` | Install npm dependencies | `assets/panel/node_modules/` |
| `cd assets/panel && pnpm exec vite build` | Production build (single file) | `assets/panel/dist/index.html` |
| `cd assets/panel && pnpm exec vite` | Dev server with HMR | `http://localhost:5173` |
| `just build-panel` | Justfile recipe: install + build | `assets/panel/dist/index.html` |
| `just dev-panel` | Justfile recipe: install + dev server | Dev server runs |

### Vite Config (vite.config.ts)

```typescript
import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';
import tailwindcss from '@tailwindcss/vite';
import singleFile from 'vite-plugin-singlefile';

export default defineConfig({
  plugins: [solid(), tailwindcss(), singleFile()],
});
```

- `vite-plugin-solid`: Compiles SolidJS JSX to fine-grained reactivity
- `@tailwindcss/vite`: Inlines Tailwind CSS
- `vite-plugin-singlefile`: Bundles HTML + CSS + JS into a single `index.html`

### Distribution

The built `assets/panel/dist/index.html` is **committed to git** and embedded in the Rust binary:

```rust
// src/panel/mod.rs or src/main.rs
const PANEL_HTML: &str = include_str!("../assets/panel/dist/index.html");
```

---

## How to Add a New Screen

### 6-Step Checklist

1. **Define screen name** in `src/types.ts`:
   ```typescript
   export type Screen = 
     | 'loading' | ... | 'new-screen'  // Add here
   ```

2. **Create component** in `src/components/NewScreen.tsx`:
   ```typescript
   import type { Component } from 'solid-js';
   import type { AppState } from '../hooks/useAppState';
   import type { IpcMethods } from '../hooks/useIpc';

   interface Props {
     state: ReturnType<typeof useAppState>;
     ipc: ReturnType<typeof useIpc>;
   }

   const NewScreen: Component<Props> = (props) => {
     return <div class="p-4"> ... </div>;
   };

   export default NewScreen;
   ```

3. **Add routing case** in `src/App.tsx`:
   ```typescript
   <Match when={state.screen() === 'new-screen'}>
     <NewScreen state={state} ipc={ipc} />
   </Match>
   ```

4. **Update state transitions** in `src/hooks/useAppState.ts` if needed:
   ```typescript
   function handleStateChanged(state: string) {
     switch (state) {
       case 'NEW_STATE':
         setScreen('new-screen');
         break;
     }
   }
   ```

5. **Add signals** to `useAppState.ts` if the screen needs live data (e.g., a progress meter):
   ```typescript
   const [newMetric, setNewMetric] = createSignal(0);
   // ... in window.__voce_update handler:
   case 'new_event':
     setNewMetric(msg.value);
     break;
   ```

6. **Test locally**:
   ```bash
   cd assets/panel
   pnpm exec vite  # Dev server
   # Browser: http://localhost:5173
   # Edit files; HMR refreshes
   ```

---

## Key Constraints & Patterns

### SolidJS Reactivity Rules

- **Signals are functions**, not values: `state.screen()` not `state.screen`
- **Effects must be inside components**: `createEffect(() => { ... })`
- **No virtual DOM**: Updates are surgical; don't mutate state directly
- **One-way data flow**: Parent → child props, child calls parent callback

### No External URLs

All content (CSS, fonts, icons) must be bundled:

- ❌ Don't link `<link rel="stylesheet" href="https://...">` 
- ✅ Use local `import` statements and bundled npm packages
- ✅ Use `lucide-solid` for icons (bundled)
- ✅ Use Tailwind CSS (inlined via `@tailwindcss/vite`)

### window.ipc Guard

Always check if `window.ipc` exists before calling (for dev/test environments):

```typescript
export function useIpc() {
  return {
    startRecording: (index: number) => {
      if (window.ipc) {
        window.ipc.send('start_recording', { index });
      } else {
        console.warn('IPC not available');
      }
    },
  };
}
```

### TypeScript Strictness

- Strict mode enabled; no implicit `any`
- Props must be typed (`interface Props`, `<Component<Props>`)
- Event handlers must be `(e: Event) => void` or `(index: number) => void`

---

## Common Patterns

### Conditional Rendering

```typescript
<Show when={state.isPassing()}>
  <p class="text-green-500">Filter passing!</p>
</Show>

<Switch>
  <Match when={state.screen() === 'loading'}>
    <Spinner />
  </Match>
  <Match when={state.screen() === 'active'}>
    <FilterStats />
  </Match>
</Switch>
```

### Live Meters

```typescript
<Progress value={state.rec1Elapsed()} max={10} />
<p>{state.rec1Elapsed()}s elapsed</p>
```

When Rust emits `RecordingProgress{elapsed_s: 2}`, the signal updates and the UI re-renders.

### Button Handlers

```typescript
<button
  class="px-4 py-2 bg-blue-600 text-white rounded"
  onclick={() => ipc.startRecording(1)}
>
  Record
</button>
```

### Route Navigation

```typescript
<button onclick={() => state.setScreen('settings')}>
  Settings
</button>
```

---

## Testing

### Dev Server (HMR)

```bash
just dev-panel
# Browser: http://localhost:5173
# Edit src/; page auto-refreshes
```

### Build & Embed

```bash
just build-panel
just build-rust  # Embeds dist/index.html
just dev-app     # Run in voce.app bundle
```

### TypeScript Check

```bash
cd assets/panel
pnpm exec tsc --noEmit  # Type-check without emit
```

---

## Integration with Rust

The webview is created once in `src/main.rs`:

```rust
let webview = WebviewBuilder::new(&window)
    .with_url(&PANEL_HTML)
    .with_ipc_handler(|_window, request| {
        // Handle window.ipc.send(cmd, payload)
    })
    .build()?;
```

When the app needs to send an event to the panel:

```rust
let event = PanelEvent::StateChanged { state: "FILTERING" };
let js_call = event.to_js_call()?;
webview.evaluate_script(&js_call)?;  // JS: window.__voce_update("...")
```

See `src/panel/AGENTS.md` for the full IPC contract.
