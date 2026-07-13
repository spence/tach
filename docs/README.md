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

## Markdown disposition

| Surface | Disposition | Rule |
|---|---|---|
| root `README.md` and `BENCHMARKS.md` | Retain but hold public rewrites | They remain the only canonical public documents; do not update their v0.2.0/23-target/legacy-chart claims until a full retained proof package passes `OBJ-RELEASE-0-2.M0.G1` |
| root `AGENTS.md` and `CLAUDE.md` | Retain | They are the canonical agent entry point and its shim, not public product documentation |
| `WHY-NOT-AN-ATOMIC.md` | Retain as historical decision evidence | Preserve its benchmark history and link it from a future dedicated ADR or frozen investigation; do not delete or turn it into current API authorization |
| [`plans/process-instant-atomic-deadline.md`](plans/process-instant-atomic-deadline.md) | Retain as a planned 0.3 proposal | It belongs to `OBJ-PROCESS-INSTANT`, not the 0.2 release; an accepted ADR must revalidate it before implementation |
| former `README_AGENT.md`, `README_STRUCTURE.md`, `README_TOPICS.md`, and `tmp-plan-delete-me.md` | Remain retired | Their surviving contract and release criteria live in ADR-0001 and `OBJ-RELEASE-0-2`; do not recreate a second public README |

## Maintenance

Author objective, ADR, and evidence sources, run `nsr render`, then `nsr check`. A rendered status
or index must never be edited by hand. On a gate flip, update the objective's table and append its
Working Log entry in the same change.
