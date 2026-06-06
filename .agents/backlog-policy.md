# Voce Backlog Policy

## Task Structure

All work must be tracked as GitHub Issues in the **voce** Project (GitHub ProjectV2, number 12).

### Required Fields

Every task must have:
- **Title**: Descriptive, action-oriented
- **Status**: For Refinement | Scheduled | WIP | Blocked | Completed
- **Priority**: P0 (critical/blocker) | P1 (high) | P2 (medium) | P3 (low)
- **Effort**: S (< 1 day) | M (1–3 days) | L (3–5 days) | XL (> 1 week)
- **Quarter**: Iteration (Q1 2026 | Q2 2026 | ...)
- **Labels**: `phase-N`, `bug`, `enhancement`, `documentation`, `infra`

### Refinement Criteria

A task is **refined** when it has:
- [ ] Iteration/Quarter set → maps to a roadmap milestone in `docs/prd.md`
- [ ] Effort estimate (S/M/L/XL) → includes testing + bug potential per contact surface
- [ ] Start date + Target date → scheduled in the roadmap
- [ ] Classification label (`feature` / `bug` / `cosmetic` / `infra`)

### Phase Tasks

The POC is broken into 8 phases tracked as individual issues:
- Issues #1–8 in the `voce` project represent Phase 0 through Phase 7
- Each phase has a corresponding file in `backlog/tasks/task-N - <name>.md`
- Phase task files use this frontmatter format:

```yaml
---
id: TASK-N
title: 'Phase N: <name>'
status: Done | In Progress | To Do
priority: high | medium
labels:
  - phase-N
dependencies: [TASK-N-1, TASK-N-2]
---
```

### No Orphan Work

**All plans and implementations must be linked to a refined task in the Project.**
- Never start implementation without a linked issue
- Never commit without closing the loop on the issue

### GitHub Project Fields

| Field | Values |
|-------|--------|
| Status | For Refinement, Scheduled, WIP, Blocked, Completed |
| Priority | P0, P1, P2, P3 |
| Effort | S, M, L, XL |
| Quarter | Quarter 1, Quarter 2, ... |
| Labels | phase-0–7, bug, enhancement, documentation, infra |

## Phase Backlog Files

Tasks are decomposed in `backlog/tasks/` with:
- `SECTION:DESCRIPTION:BEGIN` / `SECTION:DESCRIPTION:END` — task description
- `SECTION:PLAN:BEGIN` / `SECTION:PLAN:END` — implementation plan
- `SECTION:NOTES:BEGIN` / `SECTION:NOTES:END` — completion notes
- `AC:BEGIN` / `AC:END` — acceptance criteria checklist

## Definition of Done

A phase is complete when all acceptance criteria are checked off in both:
1. The GitHub Issue (all ACs checked)
2. The `backlog/tasks/task-N.md` file (status: Done)
