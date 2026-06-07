# Screen 15 — Settings (new)

Accessible via gear icon on the Active screen. Not part of the onboarding flow.
Allows adjusting noise suppression and returning to the active view.

## Current

Does not exist.

## Proposed

```
┌──────────────────────────────────┐
│ ←  Settings                      │  ← back arrow returns to screen 14
│ ────────────────────────────── │
│                                  │
│ NOISE SUPPRESSION                │
│ On  [●────────]                  │  ← ArkUI Switch
│ Reduces background noise before  │
│ speaker filtering.               │
│                                  │
│ ────────────────────────────── │
│                                  │
│ VOICE FILTER                     │
│ On  [●────────]                  │  ← mirrors active screen toggle
│ Filters out other speakers in    │
│ real time.                       │
│                                  │
│ ────────────────────────────── │
│                                  │
│ ┌────────────────────────────┐   │
│ │     Re-enroll voice…       │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

**Notes:**
- Noise Suppression toggle → dispatches `SetNoiseSuppression(bool)` IPC cmd
- Voice Filter toggle state is kept in sync with screen 14's toggle
  (both read the same `filter_paused` signal)
- Re-enroll shortcut here for discoverability
- No per-session state: both toggles persist via `config.noise_suppression`
  and `filter_paused` AtomicBool respectively
- No threshold slider in V1 — too many knobs; similarity threshold stays
  hardcoded at 0.75
