# Screen 12 — Playback: Raw (PLAYING_BACK_RAW)

Shown while the unfiltered (original mic) test audio is playing back.
Allows A/B comparison against the filtered playback.

## Current

```
┌──────────────────────────────────┐
│ ◑ ●  Playing original mic        │
│      audio…                      │
│                                  │
│ This is the unfiltered audio —   │
│ including all voices.            │
│                                  │
│ ┌────────────────────────────┐   │
│ │           Stop             │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

## Proposed

Add footer. Add "ORIGINAL" label for clarity.

```
┌──────────────────────────────────┐
│ ◑ ●  Playing original audio…     │
│                                  │
│ ORIGINAL                         │
│ Everything captured — including  │
│ all voices and background noise. │
│                                  │
│ ┌────────────────────────────┐   │
│ │           Stop             │   │
│ └────────────────────────────┘   │
│                                  │
│ ────────────────────────────── │
│    ✓  ✓  ●    Step 3 of 3       │
└──────────────────────────────────┘
```

**Changes:**
- Footer added
- "ORIGINAL" section label mirrors screen 11's "FILTERED" label
- Body mentions noise too (sets expectation: you'll hear everything)
