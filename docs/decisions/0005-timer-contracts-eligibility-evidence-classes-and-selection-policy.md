# 0005 — Timer contracts, eligibility evidence classes, and selection policy

- Status: Accepted
- Date: 2026-07-15
- Source: owner ruling
- Related: ADR-0001; ADR-0003 (superseded in part); ADR-0004 (narrowed); `OBJ-SIMPLIFY-TIMERS`;
  [`PLAN-SIMPLIFY-AND-VERIFY`](../plans/simplify-and-verify.md)

## Decision

**The three contracts** (normative: public docs must not promise more, and eligibility rulings
must not demand more):

- `Instant` — monotonic, wall-rate elapsed time between samples; never moves backward on one
  thread; high-resolution; carries no cross-thread ordering promise. Behavior across whole-system
  suspend is platform-defined and documented per platform — matching `std::time::Instant`'s
  non-promise — and is NOT normalized across platforms.
- `OrderedInstant` — the `Instant` contract plus the synchronization edge: a sample taken after an
  `Acquire` observation of another thread's published sample compares greater-or-equal to that
  sample. Cross-thread comparability additionally requires one consistent counter domain across
  cores.
- `ThreadCpuInstant` — scheduled CPU time on the calling OS thread at the platform clock's full
  precision; freezes while descheduled; targets without a native source report an explicit wall
  fallback, never a silent substitution.

**Eligibility evidence classes.** A clock may be excluded from candidacy for a contract only on:

1. **Architected or vendor-documented userspace behavior** — e.g. an ISA-architected
   counter-plus-frequency register pair, or a vendor statement that an API's values are not time.
2. **Frozen measured evidence** of a contract violation.

Class 3 — an inferred, stronger-than-published contract — is **inadmissible**.

**Selection policy.**

- Default: a compile-time `cfg` pick per target, guarded by runtime capability/availability gates
  wherever a mechanism can be absent or denied (kernel clocksource eligibility, perf mmap
  handshake, hwprobe, dynamic API resolution, JS host detection). Gates decide availability,
  never latency.
- A production measured tournament may exist for a family only with frozen evidence of two
  environments on the same maximally-specific Rust target selecting **different clocks among
  production candidates**. Bench-side audits may detect would-be flips and reopen selection; they
  never select in production.
- Inline constraint: after any one-time selection, public `now()` and `now() + elapsed()` must
  measure within `max(1 ns, 5%)` of the exact selected mechanism (paired public/exact probes).

## Why this exists (pressure / failure mode)

The Apple `Instant` ruling (ADR-0003) demanded XNU wake-domain correction that the published
contract never required, excluded the faster bare counter for lacking it, and then claimed
"fastest eligible" — a circular claim the owner rejected on 2026-07-15. Separately, runtime
measured tournaments spread to nearly every family although no frozen evidence anywhere shows two
environments on one target selecting different clocks; the only multi-environment survey
(Linux aarch64 thread-CPU) found no flip and replaced its tournament with a capability policy.
Machinery without evidence and claims without honest contracts are the failure modes this ADR
closes.

## Required invariants (what future work must preserve)

- Public claims and eligibility rulings reference exactly the three contracts above; any exclusion
  cites class 1 or class 2, never an inferred requirement.
- No production measured tournament without a frozen same-target two-environment flip among
  production candidates; capability gates never compare latency; audits detect, never select.
- The inline constraint holds for every selected route on every measured environment.
- An unbarriered counter read is never advertised as synchronization-ordered (ADR-0003's ordering
  half, which stands: zero inversions in ~10.9 billion reads were achieved only with the barrier).

## Operational consequences

- Bare `CNTVCT_EL0` re-enters as an Apple `Instant` candidate (barriered form for
  `OrderedInstant`), decided by the `OBJ-SIMPLIFY-TIMERS.M1` correctness battery on two local
  machines — ADR-0003 is superseded for `Instant` candidacy, unchanged for ordering.
- ADR-0004's clause "the same Linux target can have different complete-path winners, so runtime
  tournaments remain necessary there" is narrowed to families with a frozen flip; its provenance,
  audit, and evidence invariants stand.
- Windows wall exclusion of bare TSC stands under class 1 (Windows documents no userspace TSC
  invariance; QPC is the vendor-designated monotonic source). `QueryThreadCycleTime` exclusion
  stands under class 1. Deliberately coarsened clocks stay excluded under class 1.
- Every "ineligible" footnote is re-audited during `OBJ-SIMPLIFY-TIMERS.M1`; exclusions citing
  neither class dissolve into candidacy.
- The public "fastest tested" claim for Apple `Instant`/`OrderedInstant` is not defensible until
  re-adjudication completes; `OBJ-SIMPLIFY-TIMERS.M3` rewrites claims from its outcome. No publish
  until the owner confirms fastest per instant type per advertised architecture.

## Rejected alternatives

- Keeping wake-domain correction inside the `Instant` contract: unproven as a user requirement,
  costs measured speed, and manufactured a circular fastest claim.
- Selecting by capability bits as a performance verdict (already rejected by ADR-0004): the c7i
  thread-CPU observation shows capability can mispredict cost.
- Measured tournaments everywhere: no frozen same-target flip exists in retained evidence; that is
  unjustified machinery on every platform's hot path.

## Verification

`OBJ-SIMPLIFY-TIMERS.M1.G1` (freeze table complete with frozen evidence) and `.M2.G1` (code
matches the table on both feature surfaces; inline parity holds). The re-adjudication procedure,
environment matrix, and per-phase commands live in
[`PLAN-SIMPLIFY-AND-VERIFY`](../plans/simplify-and-verify.md).
