# 0004 — Runtime proof follows decision boundaries

- Status: Accepted — narrowed by ADR-0005 (2026-07-15): a production measured tournament now
  requires a frozen same-target two-environment flip; the audit and evidence invariants stand.
- Date: 2026-07-14
- Source: owner ruling and executed correction
- Related: ADR-0001; ADR-0002; ADR-0005; `OBJ-PROVE-TIMERS`; commit `d0fa731`

## Decision

Runtime performance proof follows distinct provider-selection and host-availability boundaries, not
the Cartesian product of Rust targets, libc variants, and feature modes. Universal target support is
proved by source, compilation, code generation, and selector-contract tests. Runtime artifacts are
required only when the default provider is selected by measurement, host availability changes the
route, or a runtime supplies a distinct clock implementation.

The default-configuration speed promise has exactly 15 required runtime boundaries, enumerated in
`OBJ-PROVE-TIMERS.M1`. No-default configurations remain required build and correctness coverage but
do not require duplicate speed artifacts. Evidence-only commits after the measured revision are
admissible only when a deterministic shipping-code closure proves that the Cargo configuration and
`src/` bytes are unchanged.

This decision narrows ADR-0002's "complete retained matrix" to the complete decision-boundary matrix
and supersedes its prohibition on all build-mode equivalence. ADR-0002's provenance, immutable
snapshot, duplicate rejection, replay binding, and time-of-check/time-of-use protections remain
mandatory.

## Why this exists (pressure / failure mode)
<!-- Charter (ADR-0001): use "What this project is and why it exists". -->

The prior 55-cell campaign treated every target/build-mode identity as an independent empirical
claim. That required no-default benchmarks even though tach's speed promise is for default builds,
required rare hosts for routes already proved fixed or fallback-only, and forced full recollection
after evidence-only changes. It also contradicted the static provider report's own classifications:
runtime-self-selecting routes were already closed by candidate enumeration and hot-route codegen,
while fixed and fallback routes were already source/codegen-proven.

The result was work without additional decision coverage. For example, Linux AArch64 default proved
perf-mmap winning over the syscall, Linux x86_64 default proved the raw syscall winning over a nominal
perf path, and the runtime selector already chooses between eligible complete paths by measurement.
Repeating those mechanisms across target labels cannot establish a new selector behavior.

## Required invariants (what future work must preserve)
<!-- Charter (ADR-0001): use "Charter invariants any future ADR must preserve". -->

- Every advertised target closes all three public timer contracts under every supported feature
  configuration.
- A runtime tournament measures complete public-to-provider paths and never trusts architecture,
  instance labels, or capability bits as a performance verdict.
- Every distinct selector branch, fallback branch, or host clock implementation has one retained
  source-sealed runtime observation.
- Fixed and unique providers may be proved by source plus optimized code generation; a new runtime
  cell is added only for a demonstrably new decision boundary.
- Optional measurements may strengthen charts but cannot become accidental release blockers.
- Public documentation distinguishes universal guarantees from measurements on named hosts.

## Operational consequences

`OBJ-PROVE-TIMERS` owns an exact 15-row runtime manifest rather than 55 target/mode identities. The
release validator must accept only that complete set, bind it to the frozen shipping-code closure,
and keep optional corroborating artifacts outside the blocking count. The remaining runtime work is
therefore macOS x86_64, Windows x86_64, and FreeBSD x86_64; Lambda, rare-native, and no-default speed
campaigns are removed from the release path unless future code introduces a distinct boundary.

## Rejected alternatives

- One runtime artifact per target and build mode: duplicates mechanisms and makes feature
  compatibility part of a default-performance promise.
- Only the original six benchmark hosts: misses browser, WASI, Emscripten, pthread, and fallback
  boundaries that really do change the available clock.
- Select providers from compile-time target facts or capability bits alone: the same Linux target can
  have different complete-path winners, so runtime tournaments remain necessary there.
- Publish broad claims from optional or legacy charts: measurements must remain scoped to their named
  hosts and frozen shipping-code closure.

## Verification

`OBJ-PROVE-TIMERS.M0.G1` checks the 24-target static provider report at `d0fa731`.
`OBJ-PROVE-TIMERS.M1.G1` checks the exact 15-row runtime table. `OBJ-PROVE-TIMERS.M2.G1` checks the
shipping-code digest, retained-evidence integrity, full test suite, and byte-clean publication
artifacts. Any change to provider enumeration or target routing must update the static verifier and,
only if it creates a new runtime decision, the M1 table.
