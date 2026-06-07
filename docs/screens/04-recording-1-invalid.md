# Screen 04 — Recording 1 Invalid

Shown when first recording is rejected (not enough speech detected).

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

Add footer. Update error copy to explain why (VAD needs clear speech).

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
│    ●  ○  ○    Step 1 of 3       │
└──────────────────────────────────┘
```

**Changes:**
- Footer added
- Error body more specific (mentions 10 s threshold)
- CTA relabelled "Try again" (more natural than "Retry")
