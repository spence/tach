# ROADMAP — tach

The portfolio is ordered by derisked leverage. [STATUS.md](STATUS.md) shows only the
current objective and next handoff; this document owns the whole remaining program.

**Conflict rule.** ROADMAP owns objective ordering. The active objective's own
milestone table owns its live state.

## Remaining objectives

| # | ID | Objective | VISION slice | Unblocks | Context |
|---|---|---|---|---|---|
| 3 | `OBJ-SIMPLIFY-TIMERS` | Simplify to verified fastest per-target clocks | Every advertised target receives the fastest eligible reliable timer for its timing contract | Defensible fastest claims; reopens the release decision | [objective](objectives/simplify-timers.md) |
| 4 | `OBJ-RELEASE-0-2` | Align the product story and build a release candidate (M2 deferred on `OBJ-SIMPLIFY-TIMERS`) | Users can choose and ship tach from one clear, verified release story | Explicit owner approval to publish 0.2.0 | [objective](objectives/release-0-2.md) |
| 5 | `OBJ-PROCESS-INSTANT` | Add planned 0.3 ergonomics without contract drift | Future timer ergonomics extend these contracts without weakening them | A separately scoped post-release capability | [objective](objectives/process-instant.md) |

## Past objectives

Owner-accepted closures only. An objective appears here after its close is recorded with retained evidence.

| ID | Objective | Closed |
|---|---|---|
| `OBJ-PROVE-TIMERS` | Prove default timer selection by decision boundary | ✅ 2026-07-14 · evidence SHA `d0f5da8` |
