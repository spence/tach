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

## /goal

Deliver `OBJ-FASTEST-TIMERS`'s slice of the VISION — *Every advertised target receives the fastest
eligible reliable timer for its timing contract.* — by cleanly exiting every milestone gate. Done =
every gate 🟢 with committed evidence at a recorded SHA; no gate weakened, no milestone closed by
assertion.
