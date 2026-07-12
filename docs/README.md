# Documentation — tach's operating record

[STATUS.md](STATUS.md) is the thin live board. This file says where truth lives; it does not
duplicate live milestone state.

## Document homes

| Need | Home | Rule |
|---|---|---|
| Current work and next objective | `docs/STATUS.md` | Rendered from the active objective; never hand-edit a `render:` region |
| Vision-slice, milestones, and gates | `docs/objectives/<slug>.md` | The top table is live truth; gates need evidence and SHA to turn green |
| Program ordering | `docs/ROADMAP.md` | ROADMAP owns objective order, not live milestone state |
| Durable ruling | `docs/decisions/NNNN-*.md` | ADRs explain pressure, invariant, rejected alternatives, and verification |
| Active forward design | `docs/plans/` | Mutable plan/SPEC; do not confuse it with an ADR or evidence |
| Frozen inquiry or negative result | `docs/investigations/` | Immutable once concluded; it informs an ADR but never decides |
| Evidence of a gate result | `docs/evidence/<topic>/<event>-<date>/` | Tracked manifest plus trimmed deciding artifacts; never link durable docs to scratch output |
| Public crate explanation | root `README.md` and `BENCHMARKS.md` | Rewrite only from frozen implementation and evidence |

## IDs and status

`OBJ-<SLUG>` is an objective; `OBJ-<SLUG>.M<n>` is its milestone; `.G<n>` is its gate;
`ADR-NNNN` is a decision; `EVID-<SLUG>` is a proof package. IDs are permanent.

- Work: 🚧 in progress · 🟣 next · ⚪ not started · ✅ complete · ⛔️ blocked · ⚫️ out of scope.
- Gate: 🟢 passed · 🟡 passed with warnings · 🔴 failed · ⚪ declared, not yet run.

Nothing closes by assertion. A gate result needs a machine-checkable pass condition, a named
fallback, tracked evidence, a recorded SHA, and a Working Log entry. An objective additionally
needs owner acceptance.

## Evidence discipline

Raw benchmark output is working material, not release proof. Before a gate closes, promote the
deciding result into a small tracked `docs/evidence/` package with provenance, command, substrate,
verdict, honest open items, and reproduction instructions. Later measurements supersede rather
than delete old packages. Until that package exists, status remains open.

The existing `benches/speed-*.json` and charts are campaign artifacts whose old source binding is
not valid for the active provider revision. They are not closure evidence for this objective.

## Current documentation migration

The root `README.md` and `BENCHMARKS.md` are the only canonical public documents. Their current
revisions are held until final evidence is frozen. The former `README_AGENT.md`,
`README_STRUCTURE.md`, `README_TOPICS.md`, and `tmp-plan-delete-me.md` were stale scratch
material; their surviving contract and release criteria now live in ADR-0001 and
`OBJ-RELEASE-0-2`. Do not recreate a second public README.

`WHY-NOT-AN-ATOMIC.md` is retained historical decision evidence until it is classified and linked
from a dedicated ADR or frozen investigation. `PLAN-0.3.0-process-instant-atomic-deadline.md`
belongs to `OBJ-PROCESS-INSTANT`, not the 0.2.0 release.

## Maintenance

Author objective, ADR, and evidence sources, run `nsr render`, then `nsr check`. A rendered status
or index must never be edited by hand. On a gate flip, update the objective's table and append its
Working Log entry in the same change.
