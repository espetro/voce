# Screen 14 — Active (ACTIVE_STANDBY / FILTERING)

Main screen during active use. Filter is running. User can pause/resume
without re-enrolling. Gear icon opens Settings.

## Current

```
┌──────────────────────────────────┐
│ ●  Active — Voce Microphone      │
│                                  │
│ In your call app, set your mic   │
│ input to Voce Microphone.        │
│                                  │
│ Similarity (last 3 s)      0.91  │
│ ────────────────────────────── │
│ ┌────────────────────────────┐   │
│ │     Test voice filter      │   │
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │     Re-enroll voice…       │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

No filter toggle. No settings access.

## Proposed

Add filter on/off toggle and gear icon. Show filter state in status row.

```
┌──────────────────────────────────┐
│ ●  Active — Voce Microphone   ⚙  │  ← gear → opens Settings
│                                  │
│ In your call app, set your mic   │
│ input to Voce Microphone.        │
│                                  │
│ FILTER                           │
│ On  [●────────]  (toggle)        │  ← ArkUI Switch; synced w/ tray
│                                  │
│ Similarity (last 3 s)      0.91  │
│ ────────────────────────────── │
│ ┌────────────────────────────┐   │
│ │     Test voice filter      │   │
│ └────────────────────────────┘   │
│ ┌────────────────────────────┐   │
│ │     Re-enroll voice…       │   │
│ └────────────────────────────┘   │
└──────────────────────────────────┘
```

When filter is paused:
```
│ ○  Filter paused — all audio     │  ← status row updates
│    is passing through            │
│                                  │
│ FILTER                           │
│ Off [────○  ]  (toggle)          │
│                                  │
│ Similarity (last 3 s)       —    │  ← no gating, no similarity
```

**Changes:**
- Filter toggle (ArkUI Switch) — dispatches `ToggleFilter` IPC cmd
- Status row reflects paused state (dot turns grey, label changes)
- Gear icon (⚙) top-right opens Settings screen
- Similarity shows `—` when filter is paused (no gating happening)
- Similarity row stays (useful context when active)
