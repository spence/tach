# `OBJ-PROVE-TIMERS` — Prove fastest eligible routes

**VISION slice.** Every public timing claim is reproducible from frozen evidence.

This objective starts only after `OBJ-FASTEST-TIMERS` freezes the implementation and route
contract. Its milestone table is the current-status surface; the Working Log is append-only.

## Milestones

| ID | Milestone | Status | Description | Context |
|---|---|---|---|---|
| `OBJ-PROVE-TIMERS.M0` | Canonical runtime cells | 🚧 | Re-run the six primary environments against one frozen revision | inline · G1⚪ |
| `OBJ-PROVE-TIMERS.M1` | Supplemental platforms | ⚪ | Produce the required native, Wasm, and negative-environment artifacts | inline · G1⚪ |
| `OBJ-PROVE-TIMERS.M2` | Release evidence | ⚪ | Validate performance, semantics, provenance, and regenerated charts | inline · G1⚪ |

---

## `OBJ-PROVE-TIMERS.M0` — Canonical runtime cells

**Description.** Capture the six primary environments with exhaustive exact and public routes for
all three timer contracts. Every artifact must name the same frozen literal source revision and
bind its serialized result to retained collector/source-seal material. The legacy `89b42f1`
campaign is historical input, not a current primary proof.

### Gate `OBJ-PROVE-TIMERS.M0.G1` — primary cells are complete and source-bound

Pass: the canonical six artifacts validate against the frozen implementation with replacement refs
disabled, include the declared `now` and elapsed comparisons, reproduce from retained collectors,
and contain no failed provider-selection reproduction.
- **Fallback.** Rebuild the failing runner or cell-specific extractor and recollect that cell; do
  not mix revisions or hand-edit a result.

---

## `OBJ-PROVE-TIMERS.M1` — Supplemental platforms

**Description.** Measure the remaining advertised platforms and explicit negative environments,
including thread-CPU sleep, busy, and isolation semantics where native CPU accounting is claimed.
Each producer must retain its own host attestation; a runnable host gap remains open rather than
being filled by a primary-cell label.

### Gate `OBJ-PROVE-TIMERS.M1.G1` — required supplemental artifacts are complete

Pass: every required supplemental artifact is present, schema-valid, source-bound, and clearly
labels measured, smoke, and negative evidence classes.
- **Fallback.** Add or repair the missing producer and rerun only the affected target; leave the
  claim scoped until the real artifact exists.

---

## `OBJ-PROVE-TIMERS.M2` — Release evidence

**Description.** The release report must calculate the fastest/material-tie verdict from the
frozen artifacts and regenerate charts only after the complete validation gate passes. It binds
strict, duplicate-free document snapshots, the campaign-commit route matrix, and the exact primary
bytes handed to the chart renderer.

### Gate `OBJ-PROVE-TIMERS.M2.G1` — release validators and charts agree

Pass: the full retained release-evidence validator passes, all performance and semantic verdicts
are honest, and the checked-in PNG/SVG summaries regenerate byte-clean from that same successful
snapshot. Untracked charts, a primary-only report, a smoke record, or a tagged fallback cannot
enter this gate as speed evidence.
- **Fallback.** Preserve the failing comparison as a finding, investigate the selector or claim,
  and narrow no contract without owner direction.

## Working Log
### 2026-07-12 · codex · `OBJ-PROVE-TIMERS.M0`
- Did: bound future primary proof to a retained full release matrix rather than the legacy 89b42f1 campaign.
- Found: the old six-cell JSON lacks the current source-seal, collector, route-commit, and supplemental provenance required for a release claim.
- Next: collect fresh primary cells only after the source and route contract freeze.
- Board: M0 remains ⚪ with G1⚪; no legacy evidence was promoted.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Opened canonical runtime collection after closing provider and target-route proof at source revision 463faa04cde78f4eef35129df866cfb76e7e785b.
- Found: The exact runtime classification declares 23 artifact identities and 32 open artifact-binding gaps; final release admission remains red until source-consistent primary and supplemental artifacts are retained.
- Next: Collect the six canonical primary cells at 463faa0, beginning with local Apple and serial AWS Graviton/Intel cells while the hosted Windows producer runs from the same revision.
- Board: OBJ-PROVE-TIMERS.M0 is active on one-revision canonical collection at 463faa0.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Froze the complete runtime artifact contract at cd598b9: all 55 runtime identities now have exact source-sealed producer and artifact bindings, with the 192-test evidence suite green in a detached worktree.
- Found: Artifact readiness is complete but runtime proof is not: the cd598b9 campaign still has 0/6 canonical and 0/49 supplemental artifacts, so no OBJ-PROVE-TIMERS gate closes.
- Next: Collect the locally runnable cd598b9 host-runtime, runtime-smoke, and Apple cells, then run serial AWS and hosted producers without mixing source revisions.
- Board: OBJ-PROVE-TIMERS.M0 remains active at frozen revision cd598b9; 55/55 artifact contracts are ready and runtime collection is next.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Corrected route-observation admission at a152e0a so source-bound tagged wall fallbacks replay through the same retained-bundle path as full-speed cells; the complete 194-test evidence suite is green.
- Found: The 19-artifact 6b4c1ed local tranche exposed the defect and is diagnostic only: the source revision advanced, so every promotable runtime artifact must be recollected at a152e0a.
- Next: Recollect and replay-compose the 19 locally runnable artifacts at a152e0a, then begin the source-identical canonical AWS and hosted cells.
- Board: OBJ-PROVE-TIMERS.M0 remains active with no gate closed; a152e0a is the corrected campaign revision and local recollection is in progress.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Collected and replay-composed the complete locally runnable tranche at source a152e0a: 19 retained artifacts bind to 19 committed route requirements as 9 full-speed, 6 tagged-wall-fallback, and 4 runtime-smoke observations.
- Found: Local proof now covers 1 of 6 canonical and 18 of 49 supplemental artifacts at one revision; these are campaign work products, not a gate closure, until the remaining 36 runtime identities join the same source-bound snapshot.
- Next: Run the source-sealed Graviton, Intel GNU, Intel musl, Lambda, and FreeBSD AWS producers at a152e0a, then add hosted Windows/macOS-x86 and native rare-architecture cells.
- Board: OBJ-PROVE-TIMERS.M0 remains active at a152e0a with 19/55 runtime artifacts replay-bound; M0.G1, M1.G1, and M2.G1 remain open.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Fixed the EC2 and FreeBSD producer layout at 2ea11ee so every cloud artifact retains an artifact-specific collector-bundle path that can coexist in the single release-evidence directory; 195 evidence tests pass.
- Found: The a152e0a local tranche proved its 19 routes but is diagnostic after the source revision advanced; the bundle collision was caught before any AWS instance launched.
- Next: Recollect and replay-compose the local 19 at 2ea11ee, then run AWS producers whose outputs can now assemble without path collisions.
- Board: OBJ-PROVE-TIMERS.M0 remains active with all gates open; 2ea11ee is the current campaign revision and no cloud spend was wasted on the superseded layout.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Recollected and replay-composed all 19 locally runnable artifacts at final producer-layout revision 2ea11ee; every artifact is source-identical and the manifest binds 9 full-speed, 6 tagged-wall-fallback, and 4 runtime-smoke observations.
- Found: The local tranche now proves 1/6 canonical and 18/49 supplemental identities at 2ea11ee; all 36 remaining artifacts can coexist in the same evidence directory because every retained bundle path is artifact-specific.
- Next: Collect the canonical Graviton, Intel GNU, Intel musl, Lambda, and FreeBSD AWS cells at 2ea11ee, then merge hosted and rare-native evidence.
- Board: OBJ-PROVE-TIMERS.M0 remains active at 2ea11ee with 19/55 runtime artifacts replay-bound and all three objective gates still open.

## /goal

Deliver `OBJ-PROVE-TIMERS`'s slice of the VISION — *Every public timing claim is reproducible from frozen evidence.* — by cleanly exiting every milestone gate. Done = every gate 🟢 with committed evidence at a recorded SHA; no gate weakened, no milestone closed by assertion.
