# `OBJ-FASTEST-TIMERS` — Close provider correctness and route proof

**VISION slice.** Every advertised target receives the fastest eligible reliable timer for its timing contract.

Read [`../STATUS.md`](../STATUS.md) and [`../README.md`](../README.md) first. The milestone table
is the current-status surface; the Working Log is append-only trajectory and audit. **Definition of
Done:** every gate is green with committed evidence at a recorded SHA, then the owner accepts the
objective. A failed gate is a finding, never permission to weaken it.

- **Status (work):** 🚧 in progress · 🟣 next candidate · ⚪ not started · ✅ completed · ⛔️ blocked · ⚫️ out of scope
- **Checks (gates):** 🟢 passed · 🟡 warnings · 🔴 failed · ⚪ declared, not yet run

## Milestones

| ID | Milestone | Status | Description | Context |
|---|---|---|---|---|
| `OBJ-FASTEST-TIMERS.M0` | Provider correctness | ✅ | Close known selector, reentry, and retiering correctness gaps | inline · G1🟢 |
| `OBJ-FASTEST-TIMERS.M1` | Benchmark contract | 🚧 | Make every eligible and public route measurable and schema-checked | inline · G1⚪ |
| `OBJ-FASTEST-TIMERS.M2` | Target-route proof | ⚪ | Prove all advertised target identities and complete public paths | inline · G1⚪ |

---

## `OBJ-FASTEST-TIMERS.M0` — Provider correctness

**Description.** Resolve the known Emscripten same-thread selection reentry risk and FreeBSD
post-selection retiering/introspection drift. Recheck provider totality, fallback-domain truth,
and regression behavior before treating the adaptive layer as frozen.

**Tasks.**

- [x] `OBJ-FASTEST-TIMERS.M0.T1` Make Emscripten local selection reentrant without spinning.
- [x] `OBJ-FASTEST-TIMERS.M0.T2` Make a FreeBSD `AT_TIMEKEEP` transition retier reads and introspection.
- [x] `OBJ-FASTEST-TIMERS.M0.T3` Add target-specific regressions for every fixed provider fault.

### Gate `OBJ-FASTEST-TIMERS.M0.G1` — production providers pass their correctness suite

Pass: the fixed source revision passes strict default and `--no-default-features` Rust tests,
Clippy, and target-specific regression tests with no known selector, domain, or stale-provider
failure remaining.
- **Fallback.** Preserve the failing reproducer, force the affected provider onto its safe eligible
  fallback, and continue diagnosis; if no safe fallback exists, escalate to user. Never weaken the
  contract.
- **Evidence.** [`EVID-PROVIDER-CORRECTNESS`](../evidence/providers/remediation-2026-07-12/README.md).

---

## `OBJ-FASTEST-TIMERS.M1` — Benchmark contract

**Description.** The harness must enumerate every eligible exact route and every selected public
route for both `now()` and `now() + elapsed()`: 24 advertised targets expand to 49 target-mode and
55 target-mode-runtime identities. Candidate counts, read-cost labels, and provider identities must
reproduce the implementation rather than hand-wave over a faster path.
[`../plans/thread-cpu-route-coverage.md`](../plans/thread-cpu-route-coverage.md) owns the
selection-profile and target-family breakdown for `ThreadCpuInstant`.

**Tasks.**

- [x] `OBJ-FASTEST-TIMERS.M1.T1` Repair FreeBSD and LoongArch route-schema drift.
- [ ] `OBJ-FASTEST-TIMERS.M1.T2` Add missing Apple, Windows, Lambda, and thread-CPU public/direct rows.
- [x] `OBJ-FASTEST-TIMERS.M1.T3` Remove generated evidence from source-binding inputs.
- [x] `OBJ-FASTEST-TIMERS.M1.T4` Make every exact candidate row statically direct and
  provider-scale-correct for both operations.
- [ ] `OBJ-FASTEST-TIMERS.M1.T5` Bind every exact row to its identity, raw samples, and one
  isolated benchmark invocation.
- [x] `OBJ-FASTEST-TIMERS.M1.T6` Declare a producer and typed route profile for the 24 advertised
  targets, 49 target-mode identities, and 55 target-mode-runtime identities.
- [x] `OBJ-FASTEST-TIMERS.M1.T7` Bind admission to literal source, retained evidence snapshots,
  committed route requirements, and the full release/chart gate.

### Gate `OBJ-FASTEST-TIMERS.M1.G1` — exhaustive route schema passes

Pass: `python3 -m unittest discover -s benches -p 'test_speed_evidence.py'` exits zero, a
declarative coverage manifest expands to all 24 targets, 49 target-mode identities, and 55
target-mode-runtime identities, and each measured case declares a real producer, selected direct
`now` and elapsed rows, exact candidate identities, and a typed selection profile. The static
contract and retained-admission mechanism never substitute for runnable producer or runtime speed
evidence.
- **Fallback.** Correct the provider map, extractor, or fixture before running any expensive
campaign; do not substitute an incomplete evidence document.

---

## `OBJ-FASTEST-TIMERS.M2` — Target-route proof

**Description.** Compile and inspect every advertised target identity after the source and harness
are frozen. The proof must distinguish codegen closure, a runnable producer, measured speed,
runtime smoke, and tagged wall fallback; no weaker category may be relabeled as a runtime speed
claim. It covers public elapsed behavior as well as raw reads.

**Tasks.**

- [ ] `OBJ-FASTEST-TIMERS.M2.T1` Prove optimized codegen and public route closure for every
  advertised target-mode identity.
- [ ] `OBJ-FASTEST-TIMERS.M2.T2` Establish a runnable producer or explicit unavailable-host record
  for every target-mode-runtime identity.
- [ ] `OBJ-FASTEST-TIMERS.M2.T3` Collect measured speed only where a latency-capable runtime
  producer exists; retain smoke and tagged fallback as their own evidence classes.

### Gate `OBJ-FASTEST-TIMERS.M2.G1` — advertised target matrix verifies complete routes

Pass: the clean-source target verifier succeeds for every advertised target identity and verifies
the selected and public `now` plus elapsed route closures; each 55-way runtime identity is then
classified as runnable measured speed, runtime smoke, tagged wall fallback, or an explicitly open
producer gap without conflating codegen with runtime speed evidence.
- **Fallback.** Fix the target provider or correct the advertised support set with an owner-ratified
decision; never silently omit a failing target.

## Working Log
### 2026-07-12 · codex · `OBJ-FASTEST-TIMERS.M0`
- Did: committed adaptive provider implementation at 0df505b and installed the NSR objective system.
- Found: Emscripten same-thread selection reentry and FreeBSD post-selection retiering remain open provider risks.
- Next: make the Emscripten selector reentrant, then retier FreeBSD AT_TIMEKEEP transitions with truthful introspection.
- Board: M0 remains 🚧; G1 is declared ⚪ and no historical evidence has been promoted.

### 2026-07-12 · codex · `OBJ-FASTEST-TIMERS.M0`
- Did: committed Emscripten reentry and FreeBSD AT_TIMEKEEP retiering fixes at 6a80582.
- Found: The Emscripten target test is blocked before tach by quanta lacking an Emscripten Monotonic implementation; normal target builds and Clippy pass.
- Next: run a real Emscripten callback regression outside quanta, then complete the remaining provider correctness review.
- Board: M0 remains 🚧; G1 stays declared ⚪ until target-specific runtime proof is complete.

### 2026-07-12 · codex · `OBJ-FASTEST-TIMERS.M0`
- Did: committed a standalone Emscripten reentry runtime probe at 4751a18.
- Found: The probe passes under Rust 1.95, Emscripten 6.0.2, and Node 26 without quanta dev dependencies.
- Next: finish the remaining provider correctness review, including the native FreeBSD retiering path.
- Board: M0 remains 🚧; all remediation tasks are complete, but G1 stays declared ⚪ pending the full provider review.

### 2026-07-12 · codex · `OBJ-FASTEST-TIMERS.M0`
- Did: attempted the native FreeBSD retiering test and terminated the ephemeral instance when no retained pass log was produced.
- Found: An unretained remote command result is not evidence; the next FreeBSD probe must copy its targeted test log before cleanup.
- Next: make a retained-log FreeBSD test runner and execute the targeted AT_TIMEKEEP retiering tests.
- Board: M0 remains 🚧; G1 stays declared ⚪ and the native FreeBSD result is still unproven.

### 2026-07-12 · codex · `OBJ-FASTEST-TIMERS.M0`
- Did: closed provider correctness with EVID-PROVIDER-CORRECTNESS after Emscripten and native FreeBSD regressions.; OBJ-FASTEST-TIMERS.M0.G1 🟢 at SHA `ea0db88`.
- Board: OBJ-FASTEST-TIMERS.M0 G1 🟢 — evidence EVID-PROVIDER-CORRECTNESS.

### 2026-07-12 · codex · `OBJ-FASTEST-TIMERS.M1`
- Did: opened the benchmark-contract milestone after provider correctness closed with EVID-PROVIDER-CORRECTNESS.
- Found: The evidence unit suite is 34/34 green, but Apple, Windows, Lambda, and thread-CPU route coverage still needs exhaustive public and exact rows.
- Next: audit Apple aarch64 candidates and add every missing exact plus public now-and-elapsed benchmark route.
- Board: M1 is 🚧 with G1 declared ⚪; M0 is ✅ with EVID-PROVIDER-CORRECTNESS.

### 2026-07-12 · codex · `OBJ-FASTEST-TIMERS.M1`
- Did: recorded the target-by-target `ThreadCpuInstant` route contract in the coverage plan.
- Found: fixed-native, availability-fallback, runtime-tournament, and fallback-only providers need distinct exact-row and validation shapes; Windows is the active fixed-platform implementation slice.
- Next: complete Windows `GetThreadTimes` availability-fallback rows and metadata, then reuse the fixed-native profile for macOS.
- Board: M1 remains 🚧 with G1 declared ⚪; runtime speed proof is still deferred to OBJ-PROVE-TIMERS.

### 2026-07-12 · codex · `OBJ-FASTEST-TIMERS.M1`
- Did: reviewed the route harness against direct-path, identity, sample-provenance, and stale-run failure modes.
- Found: a green synthetic schema suite did not prove every advertised build identity had a producer; generic candidate primitives also risked indirect reads and selected-provider conversion scales.
- Next: finish the static direct/provider-scale repair, then add the declarative target/mode/runtime producer manifest before considering G1.
- Board: M1 remains 🚧 with G1 declared ⚪; the coverage manifest and its route-profile fixtures are mandatory gate inputs.

### 2026-07-12 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: committed static direct-provider benchmark readers and provider-local tick conversion at aa86c72; marked T4 complete.
- Found: macOS thread-CPU benchmark rows need a distinct fixed-native selector profile, not Linux perf tournament metadata.
- Next: finish the macOS fixed-native rows and schema profile, then validate the declarative target/mode/runtime producer manifest.
- Board: M1 remains 🚧 and G1 ⚪; T4 is complete at aa86c72 while T2, T3, T5, and T6 remain open.

### 2026-07-12 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: committed the exhaustive route-evidence contract at 6f7f646; marked source binding (T3) and the 49-identity typed producer declaration (T6) complete.
- Found: admission rejects ten explicitly planned producers; static route coverage and selector extraction do not substitute for runnable target producers or runtime speed evidence.
- Next: use the shared supplemental-composer design to turn the highest-leverage planned/hosted routes into real producers, beginning with the remaining Linux and hosted-native gaps.
- Board: M1 remains 🚧 and G1 ⚪; T3, T4, and T6 are complete, while T2 and T5 plus ten producer admissions remain open.

### 2026-07-12 · codex · `OBJ-FASTEST-TIMERS.M1`
- Did: recorded retained release-admission mechanism at 2e14e50: literal no-replace source, sealed collectors, strict snapshot/route binding, and full CI/chart gate; 132 Python tests pass.
- Found: the 24-target contract expands to 49 target-mode and 55 target-mode-runtime identities; Windows evidence remains stale and user-owned, Lambda is retired pending a host protocol, and Wasm/WASI host producers remain open.
- Next: turn the runnable Linux/macOS/FreeBSD producers into retained artifacts, then resolve Windows, Lambda, and host-bound runtime gaps without relabeling smoke or fallback as speed.
- Board: M1 remains 🚧 with G1⚪; retained evidence mechanism T7 is done while producer and runtime proof remain open.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: promoted EVID-LINUX-PRIMARY-SPEED for final source 136d12c: both default Linux primary cells independently admit and the clean 24-target provider proof passes
- Found: the aggregate release gate remains red from 58 missing-campaign failures per isolated cell; Intel ThreadCpuInstant is a material tie rather than the absolute minimum at 173.563 ns versus a 166.464 ns raw-syscall candidate
- Next: collect and admit the Linux musl and current macOS primary cells, then FreeBSD and the remaining hosted runtime gaps without treating codegen as speed
- Board: M1 remains 🚧 and G1 stays ⚪; EVID-LINUX-PRIMARY-SPEED is partial evidence, not a gate closure.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed the retained Lambda x86_64 host-observation producer at a646063: the runtime emits its frozen build identity, five raw AWS payloads and invocation records are digest-bound, and aggregation reproduces only from the retained bundle; 165 Python evidence tests and a real cargo-lambda Linux package build pass.
- Found: The 136d12c AWS measurements remain useful provider diagnostics but are not final campaign evidence after benchmark-source changes; Lambda Arm64 plus Wasm/WASI producers remain unready, so M1 cannot close.
- Next: Implement the shared Wasm/WASI host-runtime producer and Lambda Arm64 route without weakening tagged-fallback or runtime-smoke evidence classes.
- Board: M1 remains 🚧 and G1 ⚪; Lambda x86_64 now has a retained host producer at a646063, while final measurements wait for one frozen producer revision.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed the source-sealed Node/Wasm producer at c50d2b4 and ran its exact archived revision through five fresh Node processes; supplemental validation passed with zero failures for Instant, OrderedInstant, and native Node thread CPU.
- Found: The guarded performance.now path is materially tied with its exact guarded provider (31.00 ns public versus 30.06 ns selected direct), all five 9-batch tournaments reproducibly selected performance.now, and Node thread CPU passed busy, sleep, and sibling-isolation semantics. Quanta's slightly cheaper bare binding is ineligible for tach's reliable contract because it traps when the host performance object is absent; fastant/minstant use non-monotonic low-resolution Date.now on this target.
- Next: Extend the sealed host-runtime producer through Emscripten and WASI Preview 1/2, then rerun every admitted producer at one frozen source revision.
- Board: Node/Wasm route sealed and green at c50d2b4; Emscripten and WASI hosted routes remain.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed the source-sealed Emscripten/Node producer at f0bcc59 and removed its measured Wasm indirect-dispatch loss at 679db65; the public/exact harness now uses paired alternating batches.
- Found: The exact archived revision 679db6514b759471b372d72cc1689f468d6515b7 produced a bundle-bound cell with zero validation failures: Instant 33.0458 ns versus selected performance.now 34.5166 ns, OrderedInstant 32.975 ns versus 33.7042 ns, and ThreadCpuInstant 1087.4459 ns versus native 1091.9792 ns. Busy, 25 ms sleep, and 50 ms sibling-isolation semantics all passed; 170 Python tests, Rust tests/clippy, three Emscripten feature checks, and the synchronous-reentry probe were green.
- Next: Implement and seal the WASI Preview 1 Node producer, then extend the same source-sealed contract to Wasmtime Preview 1/2 and the remaining smoke/negative routes.
- Board: Node/Wasm and Emscripten/Node are proven producers; WASI remains on the M1 critical path.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed the adaptive WASI Preview 1 Node/Wasmtime producer at 15efe24 and the paired host-comparison protocol at a5b48d5/e82d980, then sealed both hosts from the exact archived revision e82d98063e0e15b2176058905c657200273ab09d.
- Found: Both retained bundles re-extracted with zero failures. Node exposes WASI clock ID 3: Instant 40.7208 ns, OrderedInstant 38.7292 ns, and ThreadCpuInstant 598.7583 ns versus native 594.9667 ns; busy advanced about 20.007 ms while 25 ms sleep and 50 ms sibling work consumed only 33 us and 327 us of thread CPU. Wasmtime rejects clock ID 3, so the same API truthfully reports monotonic wall fallback; its Instant, OrderedInstant, and fallback reads passed exact-route parity and the fallback advanced across sleep/sibling delay.
- Next: Implement and seal the WASI Preview 2 Wasmtime component producer, then complete the browser negative and remaining runtime-smoke routes before freezing the matrix revision.
- Board: Node/Wasm, Emscripten/Node, WASI P1/Node, and WASI P1/Wasmtime are proven; Preview 2 and smoke/negative routes remain.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Extended the shared WASI harness to a real Preview 2 component at 5e1588e and sealed Wasmtime's five-process producer from exact revision 5e1588e02383e1d674420bb9c3ae822ce9d968b1.
- Found: The retained Preview 2 bundle re-extracted with zero failures. Instant measured 113.5875 ns versus std 120.2083 ns and quanta 126.7666 ns; OrderedInstant measured 112.3834 ns. Preview 2 exposes no current-thread CPU interface, so ThreadCpuInstant truthfully reported monotonic wall fallback at 112.425 ns versus its exact wall route at 112.2 ns; busy, sleep, and sibling-delay fallback semantics passed.
- Next: Complete the browser negative producer and the wasm32-wasip1-threads/wasm32v1-none smoke producers, then freeze one source revision and rerun every admitted producer.
- Board: All Node/Emscripten/WASI hosted routes are proven; browser negative and two runtime-smoke routes remain before matrix freeze.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Added source-frozen runtime-smoke producers for wasm32-wasip1-threads and wasm32v1-none; sealed both at 478178438d1051e3cf8652c8c566587cfd5be0e6 and independently validated both supplemental cells with zero failures.
- Found: Wasmtime 46 executes the real WASI threads target with shared-memory enabled and selects the explicit MonotonicWallClock fallback; the wasm32v1-none module executed under Node and selected NodeThreadCpuUsage. Runtime-smoke feature attestations must describe public tach features only because bench-internal requires std and is ineligible on wasm32v1-none.
- Next: Complete and seal the real browser fallback producer, then freeze the full admitted source revision and rerun every producer.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Added and sealed a real Chromium browser producer at 8171221d3a9cc6ae967975827775fe7c43a402e8. Five isolated browser observations passed direct-route parity for Instant, OrderedInstant, and the tagged ThreadCpuInstant performance.now fallback; c0a05ea made the release wrapper re-extract tagged fallbacks from their retained digest-bound bundles.
- Found: Browser current-thread CPU time is genuinely unavailable, so the observed thread selector is fallback_only rather than availability_fallback. Sealed medians were 60.50 ns Instant now, 61.50 ns OrderedInstant now, and 60.00 ns ThreadCpuInstant fallback now; every public route was materially equivalent to its exact selected route and wall-fallback semantics passed busy, sleep, and sibling probes.
- Next: Freeze one common source revision, rerun every admitted producer at that revision, and close the remaining benchmark/target-route evidence gates.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed paired Wasm public/exact route evidence at 73c1f32 and the nanosecond host ThreadCpuInstant elapsed specialization at 1a7808b; the benchmark source guard accepts 1a7808b as the common revision.
- Found: Independent Wasm sampling had falsely rejected Node and Chromium, while paired WASI P1 Wasmtime samples exposed a real roughly 20 ns public elapsed overhead. Pairing and the host-duration specialization remove both defects; fresh WASI P1 Node, P1 Wasmtime, and P2 Wasmtime cells compose successfully at 1a7808b.
- Next: Recollect Apple, Node/Wasm, Chromium, Emscripten, and runtime-smoke cells at 1a7808b; then collect every native/cloud cell, validate the unified matrix, and generate the final PNG.
- Board: Frozen 1a7808b; three WASI hosted cells pass, with the remaining local/browser and native/cloud matrix still to collect.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed 1047ca0 to let wall-tagged host ThreadCpuInstant elapsed reads use the sticky selected wall timeline directly, without a second generic CPU-provider dispatch.
- Found: The full Chromium campaign at 1a7808b showed a repeatable 4-7.5 ns paired elapsed overhead despite tied single reads. A source-local Chromium proof after the fix measured 123.5 ns public versus 121.5 ns exact with a 2.5 ns paired delta, inside the equivalence allowance; Rust tests, Clippy, 175 evidence tests, and all affected host target builds pass.
- Next: Recollect every admitted local, hosted, native, and cloud producer at frozen revision 1047ca0, then validate the unified matrix and generate the final PNG.
- Board: Frozen 1047ca0 closes the residual browser fallback elapsed overhead; full source-consistent recollection is underway.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed 4d79311 so supplemental native thread-CPU identities are keyed by target, harness, and build mode; 176 evidence tests and real Graviton no-default bundle composition pass.
- Found: The 1047ca0 Graviton no-default run exposed that a target-only identity table conflated Criterion libc reference measurements with Lambda raw-syscall measurements on AArch64.
- Next: Refreeze at 4d79311 and recollect every admitted producer before unified validation and chart generation.
- Board: Frozen 4d79311; the source-consistent runtime campaign restarts after the evidence contract rejected an honestly different producer identity.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed e40d744 to preserve Emscripten paired public/exact wall and thread-CPU samples; a clean real Emscripten producer and 176 evidence tests pass.
- Found: Fresh 4d79311 Emscripten evidence exposed that paired measurements were serialized without their pairing identity, forcing noisy unpaired confidence intervals to reject materially tied public routes.
- Next: Recollect every admitted producer at frozen revision e40d744, then validate the unified route matrix and generate the final PNG.
- Board: Frozen e40d744; real Emscripten public/exact parity and selector reproduction are green, and source-consistent recollection restarts from this revision.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed 20bd114 so the native 64-bit thread-CPU tournament accepts a material 7-of-9 win while rejecting three noisy pairs; all 176 evidence tests, local Rust tests, formatting, and an x86_64 Linux all-target cross-build pass.
- Found: A real c7i no-default run at e40d744 measured raw SYS_clock_gettime faster in 7 of 9 paired batches, but the 8-of-9 threshold selected libc and the retained Criterion bundle then proved public ThreadCpuInstant elapsed slower than the eligible raw exact route.
- Next: Freeze at 20bd114 and rerun the c7i no-default cell first; if it passes, recollect every admitted producer at that revision before unified validation and chart generation.
- Board: Refroze at 20bd114 after real c7i evidence exposed an over-conservative native thread-CPU selector threshold.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed 8105273 adding retained paired public-versus-selected-exact now and elapsed proof for Linux x86; 178 evidence tests, Rust default/no-default tests, clippy with warnings denied, and x86_64/i686 Linux benchmark cross-builds pass.
- Found: The c7i musl no-default selector chose LFENCE-RDTSC correctly, but the independent Criterion CI-edge gate false-failed even though the public roundtrip median remained within the declared 5% band; the retained bundle made the drift visible.
- Next: Freeze at 8105273 and rerun the c7i musl no-default cell first; if its retained paired proof passes, recollect every admitted producer at that revision.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Reran c7i musl no-default at 8105273 with retained paired public-versus-selected-exact now and elapsed proof; the selector chose LFENCE+RDTSC correctly, but public OrderedInstant elapsed was repeatably slower than the exact route and composition failed. Independently recollected and bundle-validated Apple Silicon primary evidence at the same revision.
- Found: The paired gate exposed a real hot-path gap rather than Criterion interval drift: nine public OrderedInstant elapsed batches were about 2.97-3.01 ms per 65,536 brackets versus 2.78-2.86 ms exact, so the runtime provider/state reads compound beyond the 5% equivalence band. 8105273 cannot be the release freeze.
- Next: Inspect the emitted Linux x86 public ordered hot path and the preserved self-patching implementation, close the dispatch gap without weakening the evidence contract, then refreeze and rerun the decisive c7i musl no-default producer before recollecting the matrix.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed 1182d7a making the paired public-versus-exact probe cross one shared opaque FnMut call boundary, and made that symmetry a validator-required evidence field.
- Found: Emitted optimized x86_64-musl assembly now contains one shared measure_wall_read_batch body with the same indirect call for both closures. The 178-test evidence suite, Rust default/no-default tests, warning-strict library/test clippy, and optimized x86_64/i686 musl benchmark builds pass.
- Next: Freeze at 1182d7a and rerun c7i musl no-default. Admit it only if retained paired proof and the independent Criterion route estimates both validate, then recollect Apple and the remaining matrix at that exact revision.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: The decisive c7i musl no-default producer passed at 1182d7a after the symmetric probe correction; its retained bundle was recomposed at the final path and independently replay-validated. Apple primary plus eight browser/Wasm/WASI supplemental cells were also recollected and individually validated at the same revision.
- Found: Criterion measured OrderedInstant public elapsed at 45.82 ns versus 44.97 ns selected exact; the symmetric paired probe put the public median about 4.2% above exact, inside the declared 5% band. The prior 6.9% result was entirely the probe's asymmetric closure inlining, not a runtime dispatch defect.
- Next: Collect the Graviton primary producer now, then continue the serial AWS default/no-default glibc/musl and FreeBSD matrix at 1182d7a while preserving each retained bundle at its final path.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Recollected the Graviton primary producer at frozen source 1182d7a5e73dece6e1d2b7c8f5cea35f51d40778, preserved its retained collector bundle at the final artifact path, and replay-validated the recomposed cell.
- Found: The aarch64 Linux primary cell passes all three public-clock proofs: Instant 6.67 ns, OrderedInstant 20.64 ns, and ThreadCpuInstant 57.39 ns; the selected perf-mmap thread CPU path matches its direct mechanism and beats the measured raw-syscall runner-up.
- Next: Continue the serial AWS matrix at the same frozen revision: Intel glibc primary, Intel musl primary, Graviton and Intel glibc no-default, then FreeBSD.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Ran one frozen aarch64-unknown-linux-gnu selector binary across c6g.large, c7g.large, c8g.large, and t4g.small with both production providers eligible, then recorded the raw paired batches and cleanup evidence.
- Found: All four Arm environments selected perf task-clock mmap by wide margins; no same-target cost-driven provider flip was observed. The repository therefore has evidence for runtime capability fallback but not for an aarch64 profitability tournament.
- Next: Replace the aarch64 perf/native cost tournament with capability-based perf-mmap selection plus native failure fallback, preserve other architectures until their own evidence is resolved, then re-freeze and revalidate.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed e7cb1d0 to replace Linux AArch64 provider-cost selection with capability-preferred perf task-clock mmap and native thread-CPU failure fallback, while retaining paired path measurements only in benchmark evidence builds.
- Found: A fresh same-binary c7g.large run selected LinuxPerfMmap/Inline at 57.13 ns per audited read when the perf handshake was enabled and PosixThreadCpuClock/SystemCall when both controls were denied; the enabled native and perf-read audit paths were 250.86 ns and 380.79 ns.
- Next: Freeze e7cb1d0 plus this evidence update, rerun the canonical Graviton producer against the capability-policy schema, then resolve the retained Intel ordered-parity failure before recollecting the remaining matrix.
- Board: M1 remains active until the revised source-sealed primary cells pass the route and parity validators.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Preserved the passing e1f38c6 canonical Graviton bundle, used its exact native rows to identify a libc fallback mismatch, then committed ed1d017 so Linux AArch64 deterministically prefers the inlined raw thread-clock syscall with libc only as failure fallback.
- Found: The canonical rows measured raw at 259.81 ns versus libc at 278.86 ns. A corrected same-binary c7g run selected raw with perf enabled and denied; its paired native audits measured raw 4-5% faster in both processes, and the denied public provider remained PosixThreadCpuClock/SystemCall.
- Next: Freeze ed1d017 plus this evidence update and rerun the canonical Graviton producer; admit the resulting cell only if capability policy, raw native fallback, public/exact parity, and all release tests pass together.
- Board: M1 remains active; the final revised Graviton source seal is the next admission gate.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Sealed and independently validated the canonical c7g producer at 5dfd158: all 79 remote release/integration tests passed, the retained bundle had zero validation failures, public ThreadCpuInstant measured 57.74 ns now and 116.27 ns elapsed versus 260.46 ns and 522.87 ns for native, and the raw syscall fallback beat libc.
- Found: Linux AArch64 no longer needs provider-profitability selection: capability chooses perf mmap, denial/failure chooses raw CLOCK_THREAD_CPUTIME_ID, and the public path matches its exact mechanism. This cell is proof of the implementation but will need rerunning after any later source change for the one-revision release campaign.
- Next: Diagnose and correct the retained Linux x86 OrderedInstant public/exact elapsed parity failure before spending on another Intel producer; then rerun the affected canonical cells at one frozen revision.
- Board: Canonical Graviton provider/fallback proof is green at 5dfd158; M1 remains open on the Intel OrderedInstant mismatch and remaining one-revision campaign.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Ran and rejected the 16-byte Linux x86 OrderedInstant retained-state experiment at afab34d; optimized correctness suites passed and the source-sealed c7i Criterion run reached both decisive public operations before cleanup.
- Found: Retaining provider and scale fixed public elapsed at 43.16 ns, but regressed public now to 25.79 ns versus 21.41 ns exact and 24.74 ns std; the design moves rather than removes the dispatch cost and violates the fastest/public-exact contract.
- Next: Keep OrderedInstant at 8 bytes and correct the repeated Linux x86 ordered dispatch without slowing now; rerun the affected canonical Intel producer only after targeted public/exact proof passes.
- Board: Intel OrderedInstant remains the release-critical gap; the 16-byte state-retention design is dispositioned no-go and AWS cleanup is complete.

### 2026-07-13 · spence · `OBJ-FASTEST-TIMERS.M1`
- Did: Committed the 8-byte Linux x86 LFENCE hot-path specialization at 1edcd01, corrected the signal-regression TLS teardown race at 3954caa, and source-sealed plus independently replay-validated the canonical c7i default bundle.
- Found: OrderedInstant measured 20.313 ns now and 40.372 ns elapsed versus 20.312/40.069 ns for its exact selected route and 23.507/50.659 ns for std; all optimized Linux correctness tests and the retained bundle passed with zero validation failures.
- Next: Freeze 3954caa and recollect the remaining one-revision native and hosted matrix, beginning with the canonical Graviton primary cell and the Intel musl/no-default identities.
- Board: The Intel ordered-dispatch investigation is frozen as resolved and the c7i default cell is green at 3954caa; M1 remains open until every admitted producer is recollected and unified validation passes.

## /goal

Deliver `OBJ-FASTEST-TIMERS`'s slice of the VISION — *Every advertised target receives the fastest
eligible reliable timer for its timing contract.* — by cleanly exiting every milestone gate. Done =
every gate 🟢 with committed evidence at a recorded SHA; no gate weakened, no milestone closed by
assertion.
