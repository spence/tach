# Thread CPU route coverage

Status: PLAN v0.1, 2026-07-12. Serves `OBJ-FASTEST-TIMERS.M1.G1` and
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

## Ordered approach

1. Establish one schema profile per selection shape in the extractor and validator:
   `runtime_tournament`, `fixed_native`, `availability_fallback`, and `fallback_only`.
   Each profile must identify its selected direct row, cost class, time domain, and both measured
   operations. A tagged wall fallback is never an eligible CPU-time runner-up.
2. Finish fixed native routes first. macOS needs a direct
   `clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)` selected row and metadata; FreeBSD's existing
   native64 route remains covered. Windows is separate because a `GetThreadTimes` failure selects
   wall time at runtime.
3. Model Windows as `availability_fallback`: emit direct `GetThreadTimes` rows when native setup
   succeeds and an explicit selected-wall row/metadata when it fails. A campaign that claims CPU
   time must fail closed if it observes the wall branch.
4. Add host-aware supplemental harnesses rather than forcing browser, Node, Emscripten, or WASI
   behavior through Criterion. Node and Emscripten need exact `threadCpuUsage` and wall fallback
   readers; WASI p1/pthreads need optional host-clock plus wall-fallback records; WASI p2 and
   `wasm32v1-none` need tagged fallback-only/smoke records.
5. Bind every emitted record to the frozen benchmark source, then run the target-route verifier
   and the evidence schema suite before collecting runtime artifacts.

## Coverage map

| Target family | Required profile | M1 source state | Runtime proof owner |
|---|---|---|---|
| Linux and Android | `runtime_tournament` | existing route family | `OBJ-PROVE-TIMERS.M0/M1` |
| FreeBSD x86_64 | `fixed_native` | existing native64 route | `OBJ-PROVE-TIMERS.M1` |
| macOS x86_64/aarch64 | `fixed_native` | direct selected rows and metadata pending | `OBJ-PROVE-TIMERS.M0/M1` |
| Windows i686/x86_64/aarch64 | `availability_fallback` | direct/metadata pending | `OBJ-PROVE-TIMERS.M0/M1` |
| Lambda x86_64 | `runtime_tournament` | custom rows complete; arm64 harness pending | `OBJ-PROVE-TIMERS.M0` |
| wasm32 unknown / Emscripten | dynamic native-or-wall | host-aware direct rows pending | `OBJ-PROVE-TIMERS.M1` |
| WASI p1 / p1-threads | `availability_fallback` | host-aware direct rows pending | `OBJ-PROVE-TIMERS.M1` |
| WASI p2 / wasm32v1-none | `fallback_only` / smoke | explicit records pending | `OBJ-PROVE-TIMERS.M1` |

## Verification

- `python3 -m unittest discover -s benches -p 'test_speed_evidence.py'` exercises every schema
  profile and rejects a missing selected or elapsed row.
- `cargo check --bench instant --features bench-internal,thread-cpu-inline` and applicable
  Windows/macOS target checks compile the Criterion producers.
- `python3 benches/verify-target-providers.py` proves each public `now` and elapsed closure after
  the benchmark source is clean and frozen.
- `OBJ-FASTEST-TIMERS.M1.G1` stays open until the exhaustive route schema passes; runtime evidence
  and chart generation remain gates of `OBJ-PROVE-TIMERS`, not substitutes for this contract.
