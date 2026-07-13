# 0002 — Retained evidence admission and full-release claim gates

- Status: Accepted
- Date: 2026-07-12
- Source: executed ruling
- Related: ADR-0001; `OBJ-FASTEST-TIMERS`; `OBJ-PROVE-TIMERS`; `OBJ-RELEASE-0-2`; commit `2e14e50`

## Decision

tach admits a speed or correctness claim only through a complete retained release-evidence
snapshot. The snapshot binds every document and route requirement to the campaign's literal source
revision, rejects mutable or ambiguous inputs, and is the only source from which CI claim output
and charts may render.

## Why this exists (pressure / failure mode)

The old six-cell report could be structurally valid while its source revision was stale, its
collector data was mutable, its route manifest was replaced after validation, or a chart reopened
a different JSON file after a green-looking check. Capability-shaped and primary-only evidence
also allowed a small subset of the advertised matrix to sound like a platform-wide claim. These are
integrity failures, not formatting defects: they can turn unmeasured or substituted work into a
release statement.

## Required invariants (what future work must preserve)

- The campaign source is addressed by its literal Git object with replacement refs disabled; a
  later checkout, ref replacement, or live `route-coverage.toml` edit cannot redefine the claim.
- Evidence is fresh output from the sealed source path: retained collector bundles, source seals,
  raw samples, and exact artifact hashes are admission inputs, not optional diagnostics.
- JSON parsing is strict and duplicate-free. Artifact IDs, route identities, and observations are
  one-to-one; a duplicate or caller-shaped label is a failure.
- Primary and supplemental documents are opened once as regular-file byte snapshots. Validation,
  hashing, route binding, and chart input derive from those bytes, never a later pathname read.
- The route matrix is parsed from the campaign commit, not the live worktree. Mode equivalence is
  prohibited until a canonical producer-bound closure digest exists.
- CI's claim validator and both claim charts require the same full release gate. A primary-only
  result, stale chart, tagged fallback, or smoke record cannot pass as full-speed evidence.

## Operational consequences

Commit `2e14e50` begins the retained-evidence mechanism; subsequent hardening must preserve the
same admission boundary rather than reintroducing a convenient live-file path. `OBJ-FASTEST-TIMERS`
owns declarative route and producer completeness, `OBJ-PROVE-TIMERS` owns retained runtime matrix
collection, and `OBJ-RELEASE-0-2` owns the public surface only after those gates pass. Existing
v0.2.0 copy, 23-target statements, legacy SHA `89b42f1`, and untracked charts are held as
non-authoritative until a current full snapshot admits them.

## Rejected alternatives

- Trust the current checkout or a capability bit: both can differ from the measured source or from
  the complete-path provider actually selected.
- Validate primary cells, then reopen JSON or live TOML for rendering/admission: this recreates a
  time-of-check/time-of-use substitution path.
- Treat smoke, tagged fallback, or a build-mode equivalence as a speed measurement: each weakens
  the contract being claimed.
- Preserve an old chart because it looks plausible: public output without a current frozen
  provenance chain is not evidence.

## Verification

The release validator's snapshot, route-commit, duplicate-identity, and chart-handoff regressions
run in the Python evidence suite. `OBJ-PROVE-TIMERS.M2.G1` remains open until a complete retained
matrix passes and the generated output is byte-clean from that same snapshot.
