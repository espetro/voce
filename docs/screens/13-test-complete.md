# Screen 13 — Test Complete

Shown after playback ends (or when returning to TEST_READY after a prior test).
Central decision point: continue, re-test, or re-enroll.

## Current

```
┌──────────────────────────────────┐
│ ●  How did it sound?             │
│ ────────────────────────────── │
│            [ quality label ]     │  ← static blue, no logic
│                                  │
│ ┌────────────────────────────┐   │
│ │       Replay filtered      │   │
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │       Play original        │   │
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │          Re-test           │   │
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │  Looks good — I'm done     │   │  ← primary
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │       Re-enroll voice…     │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

## Proposed

Three conditional tiers based on `voice_pct` from `test_stats` event.

### Tier 1 — Good (voice_pct ≥ 60)

```
┌──────────────────────────────────┐
│ ●  How did it sound?             │
│ ────────────────────────────── │
│  ✓  Crystal clear                │  ← green label (≥80)
│     or "Crisp and defined" (≥60) │
│                                  │
│ ┌────────────────────────────┐   │
│ │  Looks good — I'm done     │   │  ← primary
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │       Replay filtered      │   │
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │       Play original        │   │
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │          Re-test           │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

### Tier 2 — Marginal (voice_pct 40–59)

```
┌──────────────────────────────────┐
│ ●  How did it sound?             │
│ ────────────────────────────── │
│  ⚠  Coming through               │  ← amber label
│                                  │
│  Consider re-testing in a        │
│  quieter room.                   │
│                                  │
│ ┌────────────────────────────┐   │
│ │  Looks good — I'm done     │   │
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │       Replay filtered      │   │
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │       Play original        │   │
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │          Re-test           │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

### Tier 3 — Poor (voice_pct < 40)

```
┌──────────────────────────────────┐
│ ○  How did it sound?             │
│ ────────────────────────────── │
│  ✗  Low match detected           │  ← red label
│                                  │
│  Try re-enrolling or speak       │
│  closer to the mic.              │
│                                  │
│ ┌────────────────────────────┐   │
│ │       Re-enroll voice…     │   │  ← primary for this tier
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │          Re-test           │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

**Changes vs current:**
- Quality label is colour-coded and tier-conditional (green / amber / red)
- Tier 3 promotes "Re-enroll" as the primary CTA instead of "Looks good"
- No raw numeric similarity exposed here (only label)
- Footer not shown (onboarding is complete; user is making a decision)
