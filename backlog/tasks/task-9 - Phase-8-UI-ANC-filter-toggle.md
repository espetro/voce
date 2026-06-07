---
id: TASK-9
title: 'Phase 8: UI Overhaul, ANC, and Filter Toggle'
status: In Progress
priority: high
labels:
  - phase-8
  - enhancement
dependencies: [TASK-7, TASK-8]
---

SECTION:DESCRIPTION:BEGIN
Redesign the panel UI with SolidJS + ArkUI, add nnnoiseless noise suppression
to the capture pipeline, and implement a live filter toggle (panel + tray).

Problem areas being addressed:
- Filter cannot be paused/resumed without re-enrolling
- Test UX uses ad-hoc vanilla JS state machine that is hard to maintain
- Users test with background noise not knowing the filter does speaker identity
  gating, not ANC — filter feels like it "does nothing"
- No noise suppression means the product thesis is only half-true with ambient noise
SECTION:DESCRIPTION:END

SECTION:PLAN:BEGIN
See .agents/plans/2026-06-07-ui-anc-filter-toggle.md for full plan.

Step 0: ASCII screen documentation (docs/screens/) — prerequisite, hard stop for review
Step 1: SolidJS + ArkUI migration (assets/panel/)
Step 2: ANC integration (nnnoiseless + rubato)
Step 3: Filter toggle (InferenceCmd::PauseFilter/ResumeFilter + tray)

GitHub Issue: https://github.com/espetro/voce/issues/9
SECTION:PLAN:END

AC:BEGIN
- [ ] Step 0: docs/screens/*.md wireframes reviewed and approved
- [ ] Step 1: SolidJS migration passes all 14+ state transitions
- [ ] Step 2: cargo build --release clean; ANC functional
- [ ] Step 3: Pause/resume works from panel and tray; states synced
- [ ] Quality alert shows correct tier (green/amber/red) based on voice_pct
- [ ] Settings screen accessible via gear icon from active screen
AC:END

SECTION:NOTES:BEGIN
SECTION:NOTES:END
