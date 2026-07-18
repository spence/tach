# 0007 — Refined three-tier timer contract: same-core Instant, cross-core OrderedInstant

- Status: Accepted
- Date: 2026-07-17
- Source: owner ruling
- Related: [ADR-0005](0005-*.md) (eligibility classes — this refines its `Instant` reading), `ESC-M3-CLAIMS-REMEASURE`, `ESC-APPLE-ELAPSED-DISPATCH`
- Provenance: owner reframing of the three contracts, adopted as option A over "keep conservative Instant + apologetic claim".

## Decision

The three timer contracts are sharpened by the guarantee each makes, and each selects the
**fastest clock eligible for that exact guarantee**:

- **`Instant` — fastest same-core clock.** For same-thread elapsed timing where the caller can
  keep the read on one core. It does **not** promise cross-core value consistency. Its only
  cross-core safety guarantee is that `elapsed()` never returns a negative `Duration`: on a thread
  migrated to a core whose counter is unsynchronized, `elapsed()` **saturates to zero** rather than
  going negative (its wall-rate accuracy across that migration is not guaranteed). This is the
  absolute-performance tier.
- **`OrderedInstant` — fastest cross-core-reliable clock.** For reading a timer across threads or a
  migrating thread. It is both cross-core value-consistent **and** carries the documented
  happens-before synchronization edge (ADR-0003). This is where the cross-core guarantee lives.
- **`ThreadCpuInstant` — fastest reliable per-thread time**, with the documented explicit
  monotonic-wall fallback where a platform cannot expose thread CPU time.

Eligibility rule: a clock that is faster but does not carry a contract's guarantee is **ineligible
for that contract** — excluding it is not a weakening of the speed claim.

> Saturation default: `elapsed()` saturates to **zero** (tach's existing `saturating_sub`). Owner
> may overturn to "last observed value"; that would require per-value state and is not adopted here.

## Why this exists (pressure / failure mode)

The prior reading treated `Instant` as needing cross-core value consistency. That forced Windows
`Instant` onto `QueryPerformanceCounter` (QPC, ~25 ns) instead of a raw TSC read (~11 ns), because
Windows exposes no userspace TSC-invariance signal — making tach's `Instant` look ~2× slower than
the same-tier fast-clock libraries (`quanta`/`minstant`/`fastant`, which read a raw counter) and
forcing an apologetic public claim. The cross-core guarantee belongs to `OrderedInstant`: most
`Instant` use is same-core, and callers who cross cores/threads have `OrderedInstant`. Conflating
the two penalized the common case and mispresented tach against its own tier.

## Required invariants (what future work must preserve)

- The three contracts stay distinct (P2), though all return `Duration`.
- `Instant::elapsed()` never returns a negative `Duration` (saturates to zero on a backward read).
- `OrderedInstant` retains cross-core value consistency **and** the happens-before edge on every
  advertised target — never silently downgraded to a same-core-only or unbarriered read.
- No read that can crash (SIGILL/SIGSEGV) on an advertised target is ever selected; the SIGILL-safe
  dispatch stays. A wall/CPU fallback stays explicit; no silent substitution.

## Operational consequences

- Windows `Instant`: `windows_qpc` → a raw-TSC provider. Windows `OrderedInstant` stays
  `windows_qpc_call_boundary`. Apple (`apple_bare_cntvct`) and Graviton (`aarch64_cntvct`) already
  read a system-wide counter (fast **and** cross-core); Linux x86 already reads raw TSC. An
  eligibility gate that existed only to establish cross-core invariance for `Instant` may relax; a
  readability/existence gate may remain.
- Only the Windows `Instant` speed cell re-measures; every `OrderedInstant` cell and the other
  `Instant` cells are unaffected — so the current campaign is not discarded.
- Public claims reframe: `Instant` compared to the same-tier fast libraries (competitive),
  `OrderedInstant` to `std` (the only cross-thread public reference); no "slower but safer" caveat.
- The c7g `OrderedInstant` dispatch disposition (`fbe6e8b`, `ESC-APPLE-ELAPSED-DISPATCH`) is
  reinforced, not changed — `OrderedInstant` is explicitly the cross-core tier.

## Rejected alternatives

- **Keep conservative `Instant` (Windows = QPC) + apologetic claim.** Penalizes the common
  same-core case and misrepresents tach against its own tier for a guarantee that belongs to
  `OrderedInstant`.
- **Opt-in fast path, QPC default.** Keeps the common case slow; the fast read is the correct
  `Instant` default. A future opt-in cross-core-consistent `Instant` variant remains possible but is
  out of scope here.

## Verification

- Machine-checked by the speed campaign (`validate_campaign_for_checkout`): `Instant` cells
  competitive with the same-tier references; `OrderedInstant` cells beat `std` and carry the
  0-inversion ordering evidence (`benches/ORDERED-VERIFICATION.md`).
- The `elapsed()` non-negative invariant is covered by the saturating-subtraction path and its
  tests.
- Implementation and re-measurement tracked under `OBJ-SIMPLIFY-TIMERS.M4`.
