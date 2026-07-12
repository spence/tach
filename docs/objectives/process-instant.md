# `OBJ-PROCESS-INSTANT` — Add process-start and atomic deadline ergonomics

**VISION slice.** Future timer ergonomics extend these contracts without weakening them.

This is a separate planned 0.3.0 objective, not release work for the current three-timer crate.
The retained design proposal is [`../PLAN-0.3.0-process-instant-atomic-deadline.md`](../PLAN-0.3.0-process-instant-atomic-deadline.md); it must be reconciled with the active contract and evidence model before implementation begins.

## Milestones

| ID | Milestone | Status | Description | Context |
|---|---|---|---|---|
| `OBJ-PROCESS-INSTANT.M0` | Revalidate the 0.3 contract | ⚪ | Confirm the proposal still fits the three-timer model and current source | inline · G1⚪ |
| `OBJ-PROCESS-INSTANT.M1` | Implement scoped ergonomics | ⚪ | Add only the approved process-start and deadline storage APIs | inline · G1⚪ |
| `OBJ-PROCESS-INSTANT.M2` | Prove non-regression | ⚪ | Verify contracts and speed remain intact across advertised targets | inline · G1⚪ |

---

## `OBJ-PROCESS-INSTANT.M0` — Revalidate the 0.3 contract

**Description.** Reopen the proposal only after 0.2.0 is settled; record any changed invariants as
an ADR rather than treating the scratch plan as automatically authoritative.

### Gate `OBJ-PROCESS-INSTANT.M0.G1` — 0.3 design is ratified against current contracts

Pass: an accepted ADR names the preserved timer contracts, API ownership rules, and verification
plan for the proposed additions.
- **Fallback.** Keep the proposal planned and unimplemented; escalate to user if the desired API
  would weaken an established contract.

---

## `OBJ-PROCESS-INSTANT.M1` — Implement scoped ergonomics

**Description.** Implement the ratified `ProcessInstant` and `AtomicDeadline` scope without
turning tach into a generic atomic timestamp abstraction.

### Gate `OBJ-PROCESS-INSTANT.M1.G1` — API and semantic tests pass

Pass: the approved API compiles on all supported targets and its semantic tests meet the ratified
contract.
- **Fallback.** Remove the unproven addition from the release branch and return to the ADR.

---

## `OBJ-PROCESS-INSTANT.M2` — Prove non-regression

**Description.** Extend the target and runtime proof so the new storage ergonomics do not blur or
slow the existing timer contracts.

### Gate `OBJ-PROCESS-INSTANT.M2.G1` — existing contracts retain their proof

Pass: the frozen target, semantic, and performance gates for the three existing timers remain green after the addition.
- **Fallback.** Treat the regression as a finding and revert to the last proven contract.

## Working Log

## /goal

Deliver `OBJ-PROCESS-INSTANT`'s slice of the VISION — *Future timer ergonomics extend these contracts without weakening them.* — by cleanly exiting every milestone gate. Done = every gate 🟢 with committed evidence at a recorded SHA; no gate weakened, no milestone closed by assertion.
