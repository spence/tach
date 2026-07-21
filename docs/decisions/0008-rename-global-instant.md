# 0008 — Rename OrderedInstant to GlobalInstant

- Status: Accepted
- Date: 2026-07-20
- Source: owner ruling (final name set committed 2026-07-20)
- Related: [ADR-0007](0007-instant-contract-refinement.md) (the three-tier contract; the naming
  below supersedes ADR-0007's `OrderedInstant` label, the contract itself is unchanged),
  [ADR-0005](0005-timer-contracts-eligibility-evidence-classes-and-selection-policy.md),
  [ADR-0006](0006-apple-ordered-selects-self-synchronizing-counter.md)

## Decision

The public type `OrderedInstant` is renamed `GlobalInstant`. `Instant` and `ThreadCpuInstant` keep
their names. The final public type set is `Instant`, `GlobalInstant`, `ThreadCpuInstant`. This is a
pure identifier change: no contract, provider selection, or measured behavior changes.

## Why this exists (pressure / failure mode)

`OrderedInstant` named the mechanism — the ordering barrier that makes the timestamp comparable —
rather than the situation a caller reaches for it in. The three types are clearest named by their
*scope of comparability*, which is the question a caller actually asks ("can I trust a comparison
between two of these taken on different threads?"):

- `Instant` — a timestamp valid for an elapsed bracket local to one thread.
- `GlobalInstant` — a timestamp comparable across the whole process (every thread and core).
- `ThreadCpuInstant` — a different axis (CPU time consumed by one thread), so it keeps its
  measure-based name rather than a false scope parallel.

`Local`/`Global` states the scope in the name; `Ordered` described the barrier. The rename lands
pre-publish, so nothing external breaks and nothing is republished.

## Required invariants (what future work must preserve)

- The [ADR-0007](0007-instant-contract-refinement.md) contract is unchanged. `GlobalInstant` is
  still the fastest eligible cross-core-reliable clock: value-consistent across cores and carrying
  the documented happens-before edge. Renaming the type never weakens that contract.
- The public type name and the internal mechanism names may differ. Internal `ordered` identifiers
  are retained, not renamed.

## Operational consequences

- Renamed: the public type across `src/`, `tests/`, `examples/`, `benches/*.rs`, the bench tooling
  (`*.py`/`*.sh`/`*.md`), `Cargo.toml`, and the user-facing docs (`README.md`, `BENCHMARKS.md`,
  `AGENTS.md`, the `lib.rs` crate doc). The `summary-use-cases` chart panel is relabeled
  `CROSS-THREAD ELAPSED TIME` with the `tach::GlobalInstant` series name.
- Retained: `tach_ordered` (evidence schema / criterion bench IDs), `tach_probe_ordered_instant_*`
  (route-proof probe symbols), and provider/selector names. Renaming them would break the checkout
  binding of every frozen evidence cell and the route proof's symbol matching.
- The rename edits sealed source, so the primary speed campaign is re-measured at the renamed
  revision to re-bind its evidence; the numbers reproduce because behavior is unchanged.

## Rejected alternatives

- `CoreInstant` for the local type: `core` is a loaded word in a `#![no_std]` crate (reads as "the
  `core`-crate Instant"), and a CPU core is a boundary application code cannot pin.
- `ProcessInstant` for the cross-thread type: accurate, but next to `ThreadCpuInstant` it invites a
  "process CPU time" misreading; `Global` pairs more cleanly with the plain `Instant`.
- `ThreadInstant` for the CPU type: drops the defining `Cpu`, and would collide on the scope axis
  with the local `Instant`.

## Verification

Frozen evidence packages, superseded plans, historical ADRs, and investigations keep `OrderedInstant`
as the name in effect when written; this ADR is the bridge that explains those references. A repo
grep for `OrderedInstant` should return only those historical records plus the retained internal
`ordered` identifiers documented above — no live public surface.
