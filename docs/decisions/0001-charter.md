# 0001 — Three timer contracts with evidence-backed fastest selection

- Status: Accepted
- Date: 2026-07-12
- Source: owner ruling
- Related: `OBJ-FASTEST-TIMERS`, `OBJ-PROVE-TIMERS`, `OBJ-RELEASE-0-2`

## Decision

tach exposes three `Instant`-shaped values—`Instant`, `OrderedInstant`, and
`ThreadCpuInstant`—that all report elapsed time as `Duration` but deliberately
have different advancement and comparison contracts. Each default configuration
selects the fastest eligible reliable provider available on its OS/architecture.

## What this project is and why it exists

Users need a fast timer without having to learn each kernel's counter access,
virtualization behavior, ordering rules, and fallback path. A single bare counter
read cannot honestly cover local wall time, synchronization-ordered wall time,
and scheduled CPU time. The crate exists to make each of those contracts easy to
choose and fast to execute.

## Charter invariants any future ADR must preserve

- `Instant` is a local wall-rate elapsed-time timer; it does not claim
  synchronization ordering.
- `OrderedInstant` preserves its documented happens-before ordering contract.
- `ThreadCpuInstant` never disguises a wall fallback as CPU time and remains
  same-thread-only.
- Runtime capability is not enough to select a provider: selection must respect
  the timer contract and measured complete-path cost where more than one route is
  eligible.
- "Fastest" is a scoped, reproducible claim tied to a provider set, a frozen
  source revision, and evidence; failing evidence is a finding, never a prompt to
  weaken the claim.

## Operational consequences

Provider work, harness work, evidence collection, public documentation, and
release preparation are separate milestones. A release does not publish merely
because code compiles: the target matrix, runtime evidence, claim copy, crate
archive, and owner approval each have an explicit gate.

## Rejected alternatives

- One universal `Instant` contract: it hides the difference between local timing,
  cross-thread ordering, and thread CPU accounting.
- Selecting a direct route solely from a capability flag: virtualization can make a
  nominally inline route slower than a syscall or host route.
- A global atomic maximum for ordered timestamps: it creates shared contention
  without improving the documented ordering contract; the historical experiment
  remains in `docs/WHY-NOT-AN-ATOMIC.md` until its separate migration is complete.

## Verification

`OBJ-FASTEST-TIMERS` owns provider correctness and target-route proof.
`OBJ-PROVE-TIMERS` owns frozen performance and semantic evidence.
`OBJ-RELEASE-0-2` owns public claims, package verification, and approval.
