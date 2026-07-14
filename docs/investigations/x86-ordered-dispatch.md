# `INV-X86-ORDERED-DISPATCH` — Linux x86 OrderedInstant dispatch parity

Status: INVESTIGATION, 2026-07-13 — active; the retained-state design is rejected and the
8-byte hot-path correction remains open. This is a frozen record of one effort; never renamed,
pruned, or rolled-up once concluded. Read
[`../STATUS.md`](../STATUS.md) and [`../README.md`](../README.md) first.

## Context

The frozen Linux x86 producer at `1182d7a5e73dece6e1d2b7c8f5cea35f51d40778` exposed a
repeatable mismatch between public `OrderedInstant::now() + elapsed()` and its exact selected
LFENCE+RDTSC route. The retained diagnostic bundle measured 45.35 ns for the public operation and
43.28 ns for the exact route. The `now()`-only rows remained equivalent. The public API still beat
the only eligible public comparison, `std::time::Instant`, but tach's release contract also requires
the public operation to remain within the declared 5% equivalence band of the exact mechanism it
claims to select.

The implementation loaded and dispatched the ordered provider once in `OrderedInstant::now()`,
then loaded the combined provider/scale state and dispatched it again in `elapsed()`. The exact
benchmark called one fixed reader twice, so the second public dispatch was the leading hypothesis.

## Rejected retained-state experiment

Experimental commit `afab34df833d73304afc85714c381acebee88773` retained the selected provider
and Q32 scale in each Linux x86 `OrderedInstant`. That removed the second global dispatch but made
the type 16 bytes on Linux x86 instead of 8 bytes. The experiment passed 86 optimized unit tests,
the four thread-CPU integration tests, and both initialization/reentry tests on a real
`c7i.large` before Criterion began.

The source-sealed partial Criterion run was stopped once both decisive operations had completed:

| Operation / route | Median |
|---|---:|
| public `OrderedInstant::now()` | 25.793 ns |
| exact selected LFENCE+RDTSC `now()` | 21.408 ns |
| `std::time::Instant::now()` | 24.744 ns |
| public `OrderedInstant::now() + elapsed()` | 43.158 ns |

The representation change removed the elapsed regression but transferred a larger regression to
`now()`: public `now()` was about 20.5% slower than its exact selected route and about 4.2% slower
than `std`. This violates both the direct-route equivalence requirement and the crate's fastest
eligible-provider purpose. The experiment was therefore interrupted instead of being admitted as
release evidence. AWS instance `i-0a7f4d44e5e84d106` was terminated, its ephemeral key was deleted,
and a post-run query found no live `tach-bench-*` instance or `tach-speed-*` key.

## Conclusion and next constraint

Retaining selector state by doubling the public sample representation is rejected. The correction
must keep the 8-byte `OrderedInstant` representation and remove or amortize the repeated dynamic
dispatch without slowing `now()`. Weakening the public/exact equivalence band, accepting a slower
public `now()`, or treating the rejected partial run as canonical evidence are not valid fallbacks.

## Verification

At one clean frozen revision on the affected Intel producer:

1. optimized Rust correctness, fork/reentry, no-default, lint, and x86_64/i686 target checks pass;
2. public `OrderedInstant::now()` and `now() + elapsed()` each fall within the existing paired 5%
   equivalence contract of the exact selected route;
3. public `OrderedInstant` remains faster than every eligible comparison in the same process; and
4. the retained collector bundle independently re-extracts with zero validation failures.
