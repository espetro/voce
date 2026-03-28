---
id: TASK-8
title: 'Phase 7: Menubar polish and live stats'
status: To Do
assignee: []
created_date: '2026-03-27 21:06'
labels:
  - phase-7
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Dynamic tray icon swap across loading/active/idle states. Live cosine similarity readout in tray menu. Re-enroll and Quit menu items. Tooltip after enrollment.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Three icon states (loading/active/idle) swap correctly on state transitions
- [ ] #2 Similarity value updates in tray menu every ~500ms during active filtering
- [ ] #3 Re-enroll deletes enrolled_embedding.json and restarts onboarding
- [ ] #4 Quit Voce exits the process cleanly
- [ ] #5 Tooltip set to BlackHole mic selection reminder after enrollment
<!-- AC:END -->
