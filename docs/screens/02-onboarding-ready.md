# Screen 02 — Onboarding Ready (ONBOARDING_READY)

First screen the user sees after models load. Starts the enrollment flow.

## Current

```
┌──────────────────────────────────┐
│ ●  Model ready                   │
│                                  │
│ STEP 1 OF 2 — RECORD YOUR VOICE  │
│ Speak naturally for 20 seconds.  │
│ Read anything aloud — an         │
│ article, your emails, count      │
│ numbers. The app needs to        │
│ hear you clearly.                │
│                                  │
│           Recording 1 of 2       │
│ ┌────────────────────────────┐   │
│ │       Start Recording      │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

No footer. "Step 1 of 2" counts only recording steps, ignoring test step.

## Proposed

Add footer progress dots. Re-label step count to 1 of 3 (test is step 3).

```
┌──────────────────────────────────┐
│ ●  Model ready                   │
│                                  │
│ STEP 1 OF 3 — RECORD SAMPLE 1   │
│ Speak naturally for 20 seconds.  │
│ Read anything aloud — an         │
│ article, your emails, count      │
│ numbers.                         │
│                                  │
│ ┌────────────────────────────┐   │
│ │       Start Recording      │   │
│ └────────────────────────────┘   │
│                                  │
│ ────────────────────────────── │
│    ●  ○  ○    Step 1 of 3       │
└──────────────────────────────────┘
```

**Changes:**
- Step count updated to "1 of 3" (test filter is step 3)
- Footer: `● ○ ○  Step 1 of 3` — filled dot = current step
- "Recording N of 2" badge removed (redundant with footer)
