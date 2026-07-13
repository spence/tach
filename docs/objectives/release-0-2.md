# `OBJ-RELEASE-0-2` — Release the proven three-timer crate

**VISION slice.** Users can choose and ship tach from one clear, verified release story.

This objective starts after the frozen proof package exists. Publishing remains an explicit owner
decision; the objective records everything needed to make that decision safely.

## Milestones

| ID | Milestone | Status | Description | Context |
|---|---|---|---|---|
| `OBJ-RELEASE-0-2.M0` | Public truth | ⚪ | Align README, benchmark report, crate metadata, and platform claims | inline · G1⚪ |
| `OBJ-RELEASE-0-2.M1` | Release candidate | ⚪ | Verify archive, docs, MSRV, package, and publish dry run | inline · G1⚪ |
| `OBJ-RELEASE-0-2.M2` | Owner-approved publish | ⚪ | Publish only after the complete approval packet is accepted | inline · G1⚪ |

---

## `OBJ-RELEASE-0-2.M0` — Public truth

**Description.** Replace stale two-timer and old-revision copy with the approved three-contract
mental model, final provider matrix, evidence SHA, and current toolchain facts. The checked-in
v0.2.0 wording, 23-target availability claims, legacy `89b42f1` report, and untracked charts are
held pending current retained proof; they are not release-ready public evidence.

### Gate `OBJ-RELEASE-0-2.M0.G1` — every public claim traces to frozen proof

Pass: README, BENCHMARKS, crate docs, examples, package metadata, and charts agree with the final
source revision and complete retained evidence package; no public surface carries a stale target
count, legacy SHA, v0.2.0 claim, or untracked chart.
- **Fallback.** Correct or remove the stale claim and rerun the claim audit; never preserve a
  marketing statement that the evidence cannot support.

---

## `OBJ-RELEASE-0-2.M1` — Release candidate

**Description.** Build the exact crate users will receive and check its normal feature surfaces,
archive contents, generated documentation, and publication dry run from a clean revision.

### Gate `OBJ-RELEASE-0-2.M1.G1` — release candidate checks pass

Pass: format, lint, tests, target proof, full release-evidence validation, `cargo package --locked`,
and `cargo publish --dry-run --locked` all pass against one candidate SHA. Generated claim output
must derive from the admitted snapshot rather than a mutable worktree path.
- **Fallback.** Repair the failing candidate surface and rebuild the packet; do not publish a
  different SHA from the one reviewed.

---

## `OBJ-RELEASE-0-2.M2` — Owner-approved publish

**Description.** Present the candidate SHA, final README, evidence report, chart, archive list,
and dry-run result. Publishing is intentionally outside unattended authority.

### Gate `OBJ-RELEASE-0-2.M2.G1` — explicit owner approval is recorded

Pass: the owner explicitly approves publication of the reviewed candidate SHA, then the immutable
tag and crate publication succeed and a fresh consumer verifies the published crate.
- **Fallback.** Leave the candidate unpublished, record the open decision, and continue only on a
  new owner instruction.

## Working Log
### 2026-07-12 · codex · `OBJ-RELEASE-0-2.M0`
- Did: held v0.2.0 copy, 23-target availability wording, legacy SHA, and untracked charts outside the release claim surface.
- Found: public output cannot lead retained full-matrix proof.
- Next: rewrite public documents only from the admitted candidate snapshot.
- Board: M0 remains ⚪ with G1⚪; public truth is intentionally unpromoted.

## /goal

Deliver `OBJ-RELEASE-0-2`'s slice of the VISION — *Users can choose and ship tach from one clear, verified release story.* — by cleanly exiting every milestone gate. Done = every gate 🟢 with committed evidence at a recorded SHA; no gate weakened, no milestone closed by assertion.
