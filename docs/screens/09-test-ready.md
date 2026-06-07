# Screen 09 — Test Ready (TEST_READY)

Shown after adapting completes and before the user starts the test.
Also shown when returning to TEST_READY after a test-complete.

## Current

```
┌──────────────────────────────────┐
│ ●  Ready — your voice profile    │
│    is saved                      │
│ ────────────────────────────── │
│ TEST IT OUT                      │
│ Click Record, speak for a few    │
│ seconds (try having someone      │
│ else talk too), then click Stop. │
│ You'll hear the filtered         │
│ playback — only your voice       │
│ should come through.             │
│                                  │
│ ┌────────────────────────────┐   │
│ │       Start Recording      │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

## Proposed

Footer at step 3. Trim body copy. Add noise context hint (filter = identity
gating, not ANC; ANC is now also running).

```
┌──────────────────────────────────┐
│ ●  Voice profile saved           │
│ ────────────────────────────── │
│ STEP 3 OF 3 — TEST THE FILTER   │
│ Speak for a few seconds —        │
│ try having a colleague talk at   │
│ the same time. Then listen to    │
│ the filtered playback.           │
│                                  │
│ ┌────────────────────────────┐   │
│ │       Start Recording      │   │
│ └────────────────────────────┘   │
│                                  │
│ ────────────────────────────── │
│    ✓  ✓  ●    Step 3 of 3       │
└──────────────────────────────────┘
```

**Changes:**
- Footer shows steps 1 and 2 done, step 3 active
- Body copy trimmed
- "Start Recording" button label retained (user expectation)
