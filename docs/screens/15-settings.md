# Screen 15 — Settings (new)

Accessible via gear icon on the Active screen. Not part of the onboarding flow.
Allows adjusting noise suppression and returning to the active view.

## Current

Does not exist.

## Proposed

```
┌──────────────────────────────────┐
│ ←  Settings                      │
│ ────────────────────────────── │
│                                  │
│ AUDIO                            │
│ Noise Suppression  [On ●──────]  │
│ Reduces background noise…        │
│                                  │
│ Voice Filter       [On ●──────]  │
│ Filters other speakers…          │
│ ────────────────────────────── │
│ VOCE MICROPHONE                  │
│ ✓ Device active                  │  ← CheckCircle green OR grey dot
│                                  │
│ ────────────────────────────── │
│ ENROLLMENT                       │
│ ┌────────────────────────────┐   │
│ │     Re-enroll voice…       │   │
│ └────────────────────────────┘   │
│                                  │
│ ────────────────────────────── │
│ DATA                             │
│   Reset everything…              │  ← danger button
│ ────────────────────────────── │
│  [logo]  ● Voce Microphone  ▾   │  ← status bar (bottom, sticky)
│          ┌──────────────────┐    │  ← tooltip (shown when ▾ clicked)
│          │ ● Driver installed│   │
│          │ ● Device active   │   │
│          └──────────────────┘    │
└──────────────────────────────────┘
```

**Implementation:**
- Settings component converted from arrow function to function body to support `createSignal` for dialog/tooltip state
- New "Voce Microphone" status section with CheckCircle (green) or grey dot icon
- New "Data" section with "Reset everything…" button (danger style)
- Status bar (sticky bottom) shows logo + "Voce Microphone" label + collapsible tooltip button (▾)
- Tooltip displays two rows: driver installation status + device active status
- Reset button opens confirmation dialog listing what will be deleted:
  - Voice enrollment profile
  - App configuration
  - Downloaded models
  - Voce Microphone audio driver

**Signals:**
- `driverInstalled()` — driver installation status
- `voceDeviceFound()` — device detection status
- `showStatusTooltip` (local createSignal) — tooltip visibility
- `showResetDialog` (local createSignal) — reset confirmation dialog visibility

**IPC Methods:**
- `setNoiseSuppression(enabled)` — enable/disable denoiser
- `toggleFilter()` — pause/resume speaker filter
- `reenroll()` — restart enrollment flow
- `fullReset()` — trigger full reset (config + driver)
