# Screen 14b: Driver Setup

**State(s):** ACTIVE_STANDBY / FILTERING (with device not detected)

**Trigger Condition:** When app transitions to `ACTIVE_STANDBY` or `FILTERING` state and:
- `voceDeviceFound()` is `false` (Voce Microphone not detected in system)
- `hasSeenDriverSetup()` is `false` (first time in session)

Once the user clicks "Already set up" or "Continue", `hasSeenDriverSetup` is set to `true` for the session, so subsequent state transitions skip this screen and go directly to `active`.

---

## Layout: Installing

```
┌──────────────────────────────────┐
│                                  │
│ ◑ Setting up Voce Microphone…   │  ← indeterminate progress bar
│                                  │
│ ─────────────────────────────── │
│   Already set up                 │  ← caption; onclick → setScreen('active')
└──────────────────────────────────┘
```

Shown when `!driverInstalled()`. Auto-triggers `ipc.installDriver()` on mount via `onMount()` hook. Once the Rust background task completes and emits `PanelEvent::DriverStatus{installed: true, ...}`, the UI re-renders to the next state.

---

## Layout: Ready to Configure (Device Not Yet Found)

```
┌──────────────────────────────────┐
│                                  │
│ One more step                    │
│                                  │
│ In Zoom, Meet, or Teams, set     │
│ the microphone to                │
│ Voce Microphone.                 │
│                                  │
│ ┌────────────────────────────┐   │
│ │   Open Sound Settings      │   │  ← ipc.openSystemSound()
│ └────────────────────────────┘   │
│   Already set up                  │  ← caption secondary
└──────────────────────────────────┘
```

Shown when `driverInstalled()` is true but `voceDeviceFound()` is false. The "Open Sound Settings" button calls `ipc.openSystemSound()` to launch macOS Sound preferences. The user can then select "Voce Microphone" in their call app.

---

## Layout: Device Found

```
┌──────────────────────────────────┐
│                                  │
│ ✓ Voce Microphone is ready       │  ← CheckCircle green
│                                  │
│ You're all set! Use Voce         │
│ Microphone in your call app.     │
│                                  │
│ ┌────────────────────────────┐   │
│ │         Continue →         │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

Shown when `driverInstalled()` and `voceDeviceFound()` are both true. Clicking "Continue →" sets `hasSeenDriverSetup(true)` and navigates to `active`.

---

## State Transitions

| From | Trigger | To |
|------|---------|-----|
| ACTIVE_STANDBY / FILTERING | Device not found + !hasSeenDriverSetup | `driver-setup` |
| `driver-setup` | `installDriver` completes | Re-render (same screen) |
| `driver-setup` | `driverInstalled()` → true | Re-render to "ready to configure" |
| `driver-setup` | `voceDeviceFound()` → true | Re-render to "device found" |
| `driver-setup` | "Already set up" / "Continue" clicked | `active` |

---

## Component Implementation

File: `assets/panel/src/components/DriverSetup.tsx`

```typescript
const DriverSetup: Component<Props> = (props) => {
  onMount(() => {
    if (!props.state.driverInstalled()) {
      props.ipc.installDriver();
    }
  });

  const handleContinue = () => {
    props.state.setHasSeenDriverSetup(true);
    props.state.setScreen('active');
  };

  // Conditional rendering:
  // if (!driverInstalled()) → installing state
  // else if (!deviceFound()) → ready to configure
  // else → device found
};
```

---

## IPC Methods Used

| Method | Payload | Effect |
|--------|---------|--------|
| `ipc.installDriver()` | — | Spawns background thread to install driver; emits `DriverStatus` when done |
| `ipc.openSystemSound()` | — | Opens macOS Sound preferences (`x-apple.systempreferences:com.apple.preference.sound`) |

---

## Signals Used

| Signal | Purpose |
|--------|---------|
| `driverInstalled()` | True if driver installation completed |
| `voceDeviceFound()` | True if "Voce Microphone" detected in system |
| `hasSeenDriverSetup()` | Session flag; once true, bypass this screen on state transitions |

---

## Edge Cases

1. **User opens Sound Preferences but does not select Voce Microphone:** User can return to this screen by navigating away and back (e.g., going to Settings, then returning to active). The app will detect device absence on next state transition.

2. **User clicks "Already set up" without actually setting up:** Session flag prevents re-showing this screen. User must call app support or manually uncheck `hasSeenDriverSetup` (not exposed in UI).

3. **Driver installation fails:** The background task emits `DriverStatus{installed: false, ...}`. The screen shows "Already set up" as a secondary action; user can retry via panel reload or app restart.

