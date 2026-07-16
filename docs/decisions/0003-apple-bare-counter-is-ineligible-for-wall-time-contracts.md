# 0003 — Apple bare counter is ineligible for wall-time contracts

- Status: Accepted — superseded in part by ADR-0005 (2026-07-15): bare `CNTVCT_EL0` re-enters
  `Instant` candidacy under the honest-contract evidence classes; the ordering requirement below
  stands unchanged.
- Date: 2026-07-12
- Source: executed ruling
- Related: ADR-0001; ADR-0005; `OBJ-FASTEST-TIMERS`; `OBJ-PROVE-TIMERS`; commit `2e14e50`
- Provenance: INV-APPLE-BARE-CNTVCT

## Decision

Bare `CNTVCT_EL0` is not an eligible selectable Apple provider for either `Instant` or
`OrderedInstant`. Apple wall-time providers must implement the commpage-selected XNU protocol,
including its wake correction; ordered generic-CNTVCT reads must also carry the XNU instruction
barrier.

## Why this exists (pressure / failure mode)

The raw instruction is an attractive benchmark lower bound, and an awake M1 Max probe found close
short-interval agreement with Mach time. But short awake agreement does not prove the system's
monotonic timeline through wake transitions or prove a post-Acquire sample. Selecting the bare
instruction would make tach fast by silently omitting the parts of Apple's contract that make the
time value honest.

## Required invariants (what future work must preserve)

- Apple `Instant` uses only XNU-shaped, mode-eligible counter paths with the stable offset/base
  protocol; bare `CNTVCT_EL0` cannot bypass wake correction.
- Apple `OrderedInstant` uses the corresponding ordered path. A generic `CNTVCT_EL0` read includes
  `ISB`; an unbarriered bare counter is never advertised as synchronization ordered.
- A microbenchmark may report a bare counter as an explicitly ineligible diagnostic, but it cannot
  nominate it as a default provider or use it to support a fastest claim.
- Any reconsideration begins with a new frozen investigation and retained suspend/resume plus
  ordering evidence; it does not reinterpret the awake-only result.

## Operational consequences

Apple candidate enumeration, benchmark eligibility, and target-route proof must measure complete
XNU-equivalent providers. `OBJ-FASTEST-TIMERS` keeps its Apple route work open until it proves
those selected paths; `OBJ-PROVE-TIMERS` collects performance evidence only after that contract is
frozen. The decision leaves the wall-clock and thread-CPU contracts distinct.

## Rejected alternatives

- Bare `CNTVCT_EL0` for local `Instant`: it lacks XNU's offset/retry wake protocol.
- Bare `CNTVCT_EL0` for `OrderedInstant`: it also permits the sample before the required
  synchronization edge.
- Treat one awake Apple device as platform-wide proof: it does not cover sleep/wake, other commpage
  modes, virtualized hosts, or the ordered contract.

## Verification

INV-APPLE-BARE-CNTVCT records the XNU source, awake-only probe limit, and ordering evidence.
[`src/arch/apple_aarch64.rs`](../../src/arch/apple_aarch64.rs) is the implementation reference;
[`benches/ORDERED-VERIFICATION.md`](../../benches/ORDERED-VERIFICATION.md) is the retained
cross-thread control. Future Apple candidates must pass the applicable route and runtime gates,
not this ADR by assertion.
