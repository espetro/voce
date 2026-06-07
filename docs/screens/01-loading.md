# Screen 01 — Loading (MODEL_LOADING / IDLE)

Shown while models are downloading or initialising. No user action possible.

## Current

```
┌──────────────────────────────────┐
│ ◑  Loading voice model…          │
│ ░░░░░░░██░░░░░░░░░░░░░░░░░░░░░░ │  ← indeterminate animation
│                                  │
│ Voce learns to recognise your    │
│ voice and filters out everyone   │
│ else's on your calls.            │
└──────────────────────────────────┘

  [if driver missing]:
┌──────────────────────────────────┐
│ ⚠  Voce Microphone driver not    │
│    found. Run make -C            │
│    audio-driver reload.          │
└──────────────────────────────────┘
```

When a model download is in progress, the progress bar becomes determinate
and the label shows "Downloading model… N%".

## Proposed

No layout change. The loading screen is fine as-is.

```
┌──────────────────────────────────┐
│ ◑  Loading voice model…          │
│ ░░░░░░░██░░░░░░░░░░░░░░░░░░░░░░ │
│                                  │
│ Voce learns to recognise your    │
│ voice and filters out everyone   │
│ else's on your calls.            │
└──────────────────────────────────┘
```

No footer dots — user is not yet in the onboarding flow.
