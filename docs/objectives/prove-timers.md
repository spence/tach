# `OBJ-PROVE-TIMERS` — Prove fastest eligible routes

**VISION slice.** Every public timing claim is reproducible from frozen evidence.

This objective starts only after `OBJ-FASTEST-TIMERS` freezes the implementation and route
contract. Its milestone table is the current-status surface; the Working Log is append-only.

## Milestones

| ID | Milestone | Status | Description | Context |
|---|---|---|---|---|
| `OBJ-PROVE-TIMERS.M0` | Canonical runtime cells | ⚪ | Re-run the six primary environments against one frozen revision | inline · G1⚪ |
| `OBJ-PROVE-TIMERS.M1` | Supplemental platforms | ⚪ | Produce the required native, Wasm, and negative-environment artifacts | inline · G1⚪ |
| `OBJ-PROVE-TIMERS.M2` | Release evidence | ⚪ | Validate performance, semantics, provenance, and regenerated charts | inline · G1⚪ |

---

## `OBJ-PROVE-TIMERS.M0` — Canonical runtime cells

**Description.** Capture the six primary environments with exhaustive exact and public routes for
all three timer contracts. Every artifact must name the same frozen source revision.

### Gate `OBJ-PROVE-TIMERS.M0.G1` — primary cells are complete and source-bound

Pass: the canonical six artifacts validate against the frozen implementation, include the declared
`now` and elapsed comparisons, and contain no failed provider-selection reproduction.
- **Fallback.** Rebuild the failing runner or cell-specific extractor and recollect that cell; do
  not mix revisions or hand-edit a result.

---

## `OBJ-PROVE-TIMERS.M1` — Supplemental platforms

**Description.** Measure the remaining advertised platforms and explicit negative environments,
including thread-CPU sleep, busy, and isolation semantics where native CPU accounting is claimed.

### Gate `OBJ-PROVE-TIMERS.M1.G1` — required supplemental artifacts are complete

Pass: every required supplemental artifact is present, schema-valid, source-bound, and clearly
labels measured, smoke, and negative evidence classes.
- **Fallback.** Add or repair the missing producer and rerun only the affected target; leave the
  claim scoped until the real artifact exists.

---

## `OBJ-PROVE-TIMERS.M2` — Release evidence

**Description.** The release report must calculate the fastest/material-tie verdict from the
frozen artifacts and regenerate charts only after the complete validation gate passes.

### Gate `OBJ-PROVE-TIMERS.M2.G1` — release validators and charts agree

Pass: the release evidence validator passes, all performance and semantic verdicts are honest, and
the checked-in PNG/SVG summaries regenerate byte-clean from that evidence.
- **Fallback.** Preserve the failing comparison as a finding, investigate the selector or claim,
  and narrow no contract without owner direction.

## Working Log

## /goal

Deliver `OBJ-PROVE-TIMERS`'s slice of the VISION — *Every public timing claim is reproducible from frozen evidence.* — by cleanly exiting every milestone gate. Done = every gate 🟢 with committed evidence at a recorded SHA; no gate weakened, no milestone closed by assertion.
