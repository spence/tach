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
| `OBJ-FASTEST-TIMERS.M0` | Provider correctness | 🚧 | Close known selector, reentry, and retiering correctness gaps | inline · G1⚪ |
| `OBJ-FASTEST-TIMERS.M1` | Benchmark contract | ⚪ | Make every eligible and public route measurable and schema-checked | inline · G1⚪ |
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

---

## `OBJ-FASTEST-TIMERS.M1` — Benchmark contract

**Description.** The harness must enumerate every eligible exact route and every selected public
route for both `now()` and `now() + elapsed()`. Candidate counts, read-cost labels, and provider
identities must reproduce the implementation rather than hand-wave over a faster path.

**Tasks.**

- [ ] `OBJ-FASTEST-TIMERS.M1.T1` Repair FreeBSD and LoongArch route-schema drift.
- [ ] `OBJ-FASTEST-TIMERS.M1.T2` Add missing Apple, Windows, Lambda, and thread-CPU public/direct rows.
- [ ] `OBJ-FASTEST-TIMERS.M1.T3` Remove generated evidence from source-binding inputs.

### Gate `OBJ-FASTEST-TIMERS.M1.G1` — exhaustive route schema passes

Pass: `python3 -m unittest discover -s benches -p 'test_speed_evidence.py'` exits zero and its
route-identity checks cover every declared eligible provider plus selected public `now` and elapsed
paths.
- **Fallback.** Correct the provider map, extractor, or fixture before running any expensive
campaign; do not substitute an incomplete evidence document.

---

## `OBJ-FASTEST-TIMERS.M2` — Target-route proof

**Description.** Compile and inspect every advertised target identity after the source and harness
are frozen. The proof must distinguish availability/codegen from measured speed and cover public
elapsed behavior as well as raw reads.

### Gate `OBJ-FASTEST-TIMERS.M2.G1` — advertised target matrix verifies complete routes

Pass: the clean-source target verifier succeeds for every advertised target identity and verifies
the selected and public `now` plus elapsed route closures without conflating codegen with runtime
speed evidence.
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

## /goal

Deliver `OBJ-FASTEST-TIMERS`'s slice of the VISION — *Every advertised target receives the fastest
eligible reliable timer for its timing contract.* — by cleanly exiting every milestone gate. Done =
every gate 🟢 with committed evidence at a recorded SHA; no gate weakened, no milestone closed by
assertion.
