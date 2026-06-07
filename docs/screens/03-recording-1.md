# Screen 03 — Recording 1 (RECORDING_1)

Active while first voice sample is being captured.

## Current

```
┌──────────────────────────────────┐
│ ◑  Recording… 18s remaining      │
│ ██████░░░░░░░░░░░░░░░░░░░░░░░░░ │  ← fills left to right
│                                  │
│ Keep talking naturally.          │
│                                  │
│           Recording 1 of 2       │
└──────────────────────────────────┘
```

Countdown (`rec1-remaining`) and bar (`rec1-bar`) update via `recording_progress`.
No button — user cannot cancel mid-recording.

## Proposed

Same layout, add footer. Remove badge.

```
┌──────────────────────────────────┐
│ ◑  Recording… 18s remaining      │
│ ██████░░░░░░░░░░░░░░░░░░░░░░░░░ │
│                                  │
│ Keep talking naturally.          │
│                                  │
│ ────────────────────────────── │
│    ●  ○  ○    Step 1 of 3       │
└──────────────────────────────────┘
```

**Changes:**
- Footer added (same as screen 02)
- "Recording N of 2" badge removed
