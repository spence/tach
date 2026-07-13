# `INV-APPLE-BARE-CNTVCT` — Apple bare CNTVCT eligibility

Status: CONCLUDED / FROZEN, 2026-07-12 — a frozen record of one effort; never renamed, pruned, or rolled-up. Read
[`../STATUS.md`](../STATUS.md) and [`../README.md`](../README.md) first.

## Context

Could a bare `mrs CNTVCT_EL0` be admitted as the fastest Apple wall-time path for
`Instant` or `OrderedInstant`? The question matters because an awake microbenchmark can make the
single instruction look equivalent to XNU's fuller timebase path. This investigation separates
that lower bound from the timer contracts tach actually promises.

## Findings

### XNU defines a counter-plus-correction protocol

On arm64, XNU's `mach_absolute_time` wrapper reads the commpage offset, reads the selected
hardware counter, rereads the offset, retries if it changed, and adds the stable offset. XNU
updates that offset on wake; the retry is therefore part of the monotonic wall-time timeline, not
optional accounting. See XNU's [offset/retry wrapper](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/libsyscall/wrappers/mach_absolute_time.s#L227-L246).

The commpage chooses an architectural mode. Generic mode uses `ISB + CNTVCT_EL0`; Apple mode
`USER_TIMEBASE_NOSPEC_APPLE` uses `ACNTVCT_EL0`, still through the offset protocol. XNU's mode
selection is documented in the [same wrapper](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/libsyscall/wrappers/mach_absolute_time.s#L250-L278).
tach mirrors those complete paths in
[`src/arch/apple_aarch64.rs`](../../src/arch/apple_aarch64.rs): mode-specific counter selection,
the offset retry, and an `isb sy` before generic ordered `CNTVCT_EL0` reads.

### Awake parity is only an awake observation

An M1 Max/macOS 26.5 probe observed mode 3 (`USER_TIMEBASE_NOSPEC_APPLE`), a 24 MHz counter,
and a `125/3` ns Mach timebase. During that awake epoch, bare `CNTVCT_EL0` and `ACNTVCT_EL0`
tracked Mach ticks within the sampling bracket and their 500 ms deltas agreed within one tick; the
commpage offsets were zero. This does not exercise suspend/resume, a wake offset update, another
Apple mode, virtualized macOS, or other hardware. It is evidence of an awake lower bound, not
evidence that bare `CNTVCT_EL0` is XNU's wall-time ABI.

### Ordering is independently disqualifying for `OrderedInstant`

XNU calls a bare `CNTVCT_EL0` read speculative/out-of-order and requires `ISB` for a
nonspeculative sample; see [XNU machine routines](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/arm64/machine_routines.c#L2408-L2434).
The local cross-thread control agrees: on Apple M1, bare tach recorded 17,366,168
synchronization-order inversions in 144,179,634 reads while `OrderedInstant` recorded zero in
124,988,102 reads. The retained [ordered verification](../../benches/ORDERED-VERIFICATION.md)
documents the method and the broader 10.9B-read result. A bare counter therefore cannot satisfy
the ordered contract even if a future sleep test happened to show stable deltas.

## Conclusion

Bare `CNTVCT_EL0` is an ineligible Apple provider for both `Instant` and `OrderedInstant`.
For `Instant`, it lacks XNU's wake offset/retry correction and therefore does not establish the
promised wall-rate timeline. For `OrderedInstant`, it also lacks the required instruction-order
guarantee. The admissible fast candidates are complete XNU-shaped paths selected by the commpage
mode (`ACNTVCT`, `CNTVCTSS`, or `CNTVCT` with the required offset protocol and, when ordered,
`ISB`).

This is a negative result: retain bare CNTVCT only as an explicitly ineligible diagnostic lower
bound. A hypothetical revisit would require retained real suspend/resume evidence across the
supported Apple modes and machines, plus the ordered synchronization harness; that evidence alone
would still not override XNU's documented ABI.

## Verification

The source citations, the local provider implementation, and
[`benches/ORDERED-VERIFICATION.md`](../../benches/ORDERED-VERIFICATION.md) are the retained basis
for this conclusion. This investigation is frozen as concluded history; later provider work must
create new evidence rather than revise these findings.
