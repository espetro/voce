# Screen 07 — Recording 2 Invalid

Shown when second recording is rejected (not enough speech detected).

## Current

```
┌──────────────────────────────────┐
│ ○  Not enough speech detected    │
│                                  │
│ ✗ Please try again in a          │
│   quieter environment.           │
│                                  │
│ ┌────────────────────────────┐   │
│ │           Retry            │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

## Proposed

```
┌──────────────────────────────────┐
│ ○  Not enough speech detected    │
│                                  │
│ ✗ Less than 10 s of clear        │
│   speech found. Try again in     │
│   a quieter spot, or speak       │
│   a bit louder.                  │
│                                  │
│ ┌────────────────────────────┐   │
│ │       Try again            │   │
│ └────────────────────────────┘   │
│                                  │
│ ────────────────────────────── │
│    ✓  ●  ○    Step 2 of 3       │
└──────────────────────────────────┘
```

**Changes:**
- Footer added showing step 2 active
- Same copy improvements as screen 04
