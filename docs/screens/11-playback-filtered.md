# Screen 11 — Playback: Filtered (PLAYING_BACK)

Shown while the filtered test audio is playing back.

## Current

```
┌──────────────────────────────────┐
│ ◑ ●  Playing your filtered       │
│      voice…                      │
│                                  │
│ Listen — only your voice         │
│ should come through.             │
│                                  │
│ ┌────────────────────────────┐   │
│ │           Stop             │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

## Proposed

Add footer. Add "Filtered" label to distinguish from raw playback screen.

```
┌──────────────────────────────────┐
│ ◑ ●  Playing filtered audio…     │
│                                  │
│ FILTERED                         │
│ Only your voice should come      │
│ through.                         │
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
- "FILTERED" section label so users know which mode they're hearing
