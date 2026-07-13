# Thread CPU route coverage

Status: PLAN v0.2, 2026-07-12. Serves `OBJ-FASTEST-TIMERS.M1.G1` and
`OBJ-FASTEST-TIMERS.M2.G1`. Read [`../STATUS.md`](../STATUS.md) and
[`../README.md`](../README.md) first.

## Context

`ThreadCpuInstant` is total, but its selected provider can be one of four different proof
shapes: a fixed native thread clock, a runtime-tournament winner, an availability fallback from
a native clock to tagged wall time, or a fixed tagged-wall fallback. A benchmark row named
"native" is not enough: every shape needs an exact selected read and a selected
`now() + elapsed()` read, with metadata that says whether it measures thread CPU or wall time.

The objective is to make the harness faithfully represent that contract on every advertised
target before any cross-platform speed campaign begins. Runtime measurement belongs to
`OBJ-PROVE-TIMERS`; this plan only makes those measurements possible and schema-checked.

## Current producer disposition

| Producer family | Current truth | Consequence |
|---|---|---|
| Criterion Linux, macOS, and FreeBSD | The source path exists for the native route shape | It still needs a fresh retained run before it is runtime proof |
| Windows | Existing material is stale and user-owned | Do not overwrite it; add a clean non-overlapping retained producer or reconcile the existing workflow later |
| Lambda | The former path is retired | It waits for a host-specific protocol rather than being called a Criterion result |
| Wasm and WASI | No retained producer exists | Keep the route identities open; smoke and tagged fallback remain distinct from speed |

## Ordered approach

1. Establish one schema profile per selection shape in the extractor and validator:
   `runtime_tournament`, `fixed_native`, `availability_fallback`, and `fallback_only`.
   Each profile must identify its selected direct row, cost class, time domain, and both measured
   operations. A tagged wall fallback is never an eligible CPU-time runner-up.
2. Re-run the existing Criterion Linux/macOS/FreeBSD producer paths as fresh retained observations.
   macOS uses the direct `clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)` shape; FreeBSD remains a
   fixed native route. Source existence is not an artifact admission.
3. Model Windows as `availability_fallback` through either a clean non-overlapping retained producer
   or a later reconciliation of the existing user-owned workflow: emit direct `GetThreadTimes` rows
   when native setup succeeds and an explicit selected-wall row/metadata when it fails. The current
   Windows material remains untouched until one of those paths exists.
4. Replace the retired Lambda approach only with a host-aware protocol. Do not reuse Criterion or
   historical samples as Lambda proof.
5. Add host-aware supplemental harnesses for the currently absent Wasm and WASI routes rather than
   forcing browser, Node, Emscripten, or WASI behavior through Criterion. Node and Emscripten need
   exact `threadCpuUsage` and wall-fallback readers; WASI p1/pthreads need optional host-clock plus
   wall-fallback records; WASI p2 and `wasm32v1-none` need tagged fallback-only/smoke records.
6. Bind every emitted record to the frozen benchmark source, then run the target-route verifier
   and the evidence schema suite before collecting runtime artifacts.

## Exact-route invariants

- A direct candidate is a statically named provider read and that provider's own tick conversion.
  Selector dispatch, a function-pointer call, or a different provider's scale may occur before
  benchmark registration, never inside the measured operation.
- Every declared exact row carries its benchmark identity, provider, cost class, and time domain.
  Candidate keys are not labels that another route may reuse.
- Lambda candidate rows retain all raw samples and reproduce their aggregate just like public and
  selected rows. Criterion extraction only consumes an isolated run directory, so `now`, elapsed,
  and selector metadata cannot be mixed from different invocations.
- The schema test contains a declarative target/profile coverage map. A new advertised target or
  fallback shape cannot make `OBJ-FASTEST-TIMERS.M1.G1` green until it has a producer, selected
  `now` and elapsed rows, and validated metadata.

## Coverage map

| Target family | Required profile | M1 source state | Runtime proof owner |
|---|---|---|---|
| Linux and Android | `runtime_tournament` | Criterion source path exists; fresh retained run pending | `OBJ-PROVE-TIMERS.M0/M1` |
| FreeBSD x86_64 | `fixed_native` | Criterion source path exists; fresh retained run pending | `OBJ-PROVE-TIMERS.M1` |
| macOS x86_64/aarch64 | `fixed_native` | Criterion source path exists; fresh retained run pending | `OBJ-PROVE-TIMERS.M0/M1` |
| Windows i686/x86_64/aarch64 | `availability_fallback` | stale user-owned material; clean non-overlapping producer or later reconciliation pending | `OBJ-PROVE-TIMERS.M0/M1` |
| Lambda x86_64/aarch64 | host-specific | retired; awaiting a host protocol | `OBJ-PROVE-TIMERS.M0` |
| wasm32 unknown / Emscripten | dynamic native-or-wall | absent; host-aware producer pending | `OBJ-PROVE-TIMERS.M1` |
| WASI p1 / p1-threads | `availability_fallback` | absent; host-aware producer pending | `OBJ-PROVE-TIMERS.M1` |
| WASI p2 / wasm32v1-none | `fallback_only` / smoke | absent; explicit records pending | `OBJ-PROVE-TIMERS.M1` |

## Verification

- `python3 -m unittest discover -s benches -p 'test_speed_evidence.py'` exercises every schema
  profile and rejects a missing selected or elapsed row.
- `cargo check --bench instant --features bench-internal,thread-cpu-inline` and applicable
  Windows/macOS target checks compile the Criterion producers.
- `python3 benches/verify-target-providers.py` proves each public `now` and elapsed closure after
  the benchmark source is clean and frozen.
- `OBJ-FASTEST-TIMERS.M1.G1` stays open until the exhaustive route schema passes; runtime evidence
  and chart generation remain gates of `OBJ-PROVE-TIMERS`, not substitutes for this contract.
