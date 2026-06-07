# Screen 08 — Adapting (ADAPTING)

Shown while the enrollment pipeline averages embeddings and saves the profile.
Typically only a few seconds.

## Current

```
┌──────────────────────────────────┐
│ ◑  Adapting to your voice…       │
│ ░░░░░░░██░░░░░░░░░░░░░░░░░░░░░░ │  ← indeterminate
│                                  │
│ Voce is building your voice      │
│ profile. This takes just a       │
│ moment.                          │
└──────────────────────────────────┘
```

No footer currently.

## Proposed

Show footer in an intermediate "both 1 and 2 done" state while step 3 is not
yet reachable.

```
┌──────────────────────────────────┐
│ ◑  Building your voice profile…  │
│ ░░░░░░░██░░░░░░░░░░░░░░░░░░░░░░ │
│                                  │
│ This takes just a moment.        │
│                                  │
│ ────────────────────────────── │
│    ✓  ✓  ○    Almost there…     │
└──────────────────────────────────┘
```

**Changes:**
- Footer shows steps 1 and 2 done; step 3 pending
- Footer label "Almost there…" instead of "Step N of 3" (no active step)
- Body copy shortened
