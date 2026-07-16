# `OBJ-SIMPLIFY-TIMERS` — Simplify to verified fastest per-target clocks

**VISION slice.** Every advertised target receives the fastest eligible reliable timer for its
timing contract.

Owner direction (2026-07-15): the prior campaign inflated the `Instant` contract on Apple to
exclude a faster clock — "fastest eligible" became circular, so every eligibility ruling and every
runtime-selection decision must be re-derived from honest contracts and frozen evidence. There is
**no publish authorization** until the owner can confirm the fastest timer per instant type per
advertised architecture. Runtime selection survives only where a frozen same-target flip justifies
it. The executor follows [`PLAN-SIMPLIFY-AND-VERIFY`](../plans/simplify-and-verify.md) without
inventing new contracts or making unenumerated decisions — anything outside the plan's branches is
an `nsr escalate`, not a judgment call.

Read [`../STATUS.md`](../STATUS.md) and [`../README.md`](../README.md) first. The milestone table
below is the current-status surface; the `## Working Log` is the append-only audit + trajectory
home. **Definition of Done:** every milestone row below is terminal: either every gate passed with
committed evidence, or every non-green gate has a recorded disposition under its named Fallback and
authority. The closure is owner-accepted; nothing closes by assertion.

- **Status (work):** 🚧 in progress · 🟣 next candidate · ⚪ not started · ⛔️ blocked · ✅ completed · ⚫️ out of scope
- **Checks (gates):** 🟢 passed · 🟡 warnings · 🔴 failed · ⚪ declared, not yet run

## Milestones

| ID | Milestone | Status | Description | Context |
|---|---|---|---|---|
| `OBJ-SIMPLIFY-TIMERS.M0` | Honest contracts and selection policy | ✅ | ADR records the three contracts, evidence classes, and selection rule; base lands on main; plan in place | inline · G1🟢 |
| `OBJ-SIMPLIFY-TIMERS.M1` | Eligibility re-adjudication and flip verification | 🚧 | Every provider family gets a freeze verdict from retained or new frozen evidence; Apple bare-counter candidacy re-adjudicated | inline · G1⚪ |
| `OBJ-SIMPLIFY-TIMERS.M2` | Fixed-pick conversion with inline parity | ⚪ | `src/` converts to compile-time picks + capability gates per the freeze table; tournaments only where a flip is frozen | inline · G1⚪ |
| `OBJ-SIMPLIFY-TIMERS.M3` | Apparatus diet and truthful claims | ⚪ | Release-forensics tooling leaves the live tree; CI slims; claims trace to live evidence with fresh six-cell numbers | inline · G1⚪ |

---

## `OBJ-SIMPLIFY-TIMERS.M0` — Honest contracts and selection policy

**Description.** The normative timer contracts (including the suspend stance), the eligibility
evidence classes, and the selection rule are ratified as an ADR; the working tree lands on `main`;
the execution plan is complete and prescriptive enough for a less-capable executor.

### Gate `OBJ-SIMPLIFY-TIMERS.M0.G1` — Contracts, evidence classes, and policy ADR ratified
Pass: the contracts/policy ADR exists and is Accepted; `docs/plans/simplify-and-verify.md` is
complete with the freeze table, environment matrix, and per-phase commands; `main` is
fast-forwarded to the working tip; `nsr render` then `nsr check` pass.
- **Fallback.** escalate to user. Never weaken the gate.

---

## `OBJ-SIMPLIFY-TIMERS.M1` — Eligibility re-adjudication and flip verification

**Description.** Reuse retained evidence first, then run only the missing probes: the Apple
bare-counter re-adjudication on the two local machines, and the enumerated same-target
second-environment runs. Every one of the 72 target/timer cells ends with a freeze verdict —
fixed pick, capability gate, or measured (flip frozen) — or an explicitly documented residual.

### Gate `OBJ-SIMPLIFY-TIMERS.M1.G1` — Freeze table complete with frozen evidence per family
Pass: every family row in the plan's freeze table carries a verdict plus retained evidence under
`docs/evidence/` bound to a source SHA, or a documented class-1 residual; the Apple `Instant` and
`OrderedInstant` re-adjudication has correctness and speed results from both local machines.
- **Fallback.** an environment that cannot be provisioned gets a recorded residual and a class-1
  documentation freeze; a flip outcome the plan does not already branch on → escalate to user.
  Never weaken the gate.

---

## `OBJ-SIMPLIFY-TIMERS.M2` — Fixed-pick conversion with inline parity

**Description.** Convert each family to its frozen verdict: compile-time `cfg` picks plus
capability gates; delete tournament machinery everywhere no flip is frozen; relocate embedded test
modules; hold the inline-performance constraint.

### Gate `OBJ-SIMPLIFY-TIMERS.M2.G1` — Code matches freeze table on both feature surfaces with inline parity
Pass: fmt, clippy `-D warnings`, and tests pass on default and `--no-default-features`; a grep for
the tournament/selector symbols returns hits only inside families the freeze table retains as
measured; paired public/exact probes stay within `max(1 ns, 5%)` for every converted family
runnable locally or in CI; relocated test counts reconcile with the pre-move total.
- **Fallback.** revert the failing family's conversion, keep its prior mechanism, retain the
  failure as evidence, and continue with the remaining families. Never weaken the gate.

---

## `OBJ-SIMPLIFY-TIMERS.M3` — Apparatus diet and truthful claims

**Description.** Delete the release-forensics validators, tooling self-tests, and sealed bundles
from the live tree (the archive branch retains them); slim CI to the retained jobs; rewrite README
and BENCHMARKS so every claim traces to evidence that still exists, including fresh six-cell
numbers measured on the converted tree.

### Gate `OBJ-SIMPLIFY-TIMERS.M3.G1` — Slim apparatus and claims tracing to live evidence
Pass: the plan's deletion list is gone from the live tree and reachable on the archive branch; no
workflow references a deleted path; README/BENCHMARKS contain no claim referencing deleted
provenance and carry the fresh six-cell numbers; the plan's consistency greps return empty.
- **Fallback.** restore the specific provenance from the archive branch or correct the claim;
  never leave a public claim pointing at nothing. Never weaken the gate.

---

## Working Log (append-only audit + trajectory)

### 2026-07-15 · claude · `OBJ-SIMPLIFY-TIMERS.M0` direction reset
- Did: minted this objective; rejected ESC-PUBLISH-TACH-0-2-0-76FD4B1 as the owner's explicit
  ruling; updated the AGENTS.md mission to the all-architecture three-instant wording at 60b82eb.
- Found: no frozen two-environment same-target selection flip exists anywhere in retained
  evidence; the Apple `Instant` bare-counter exclusion rested on an inferred contract (owner-
  endorsed critique), so the Apple fastest claim is not presently defensible.
- Next: land the contracts/policy ADR and PLAN-SIMPLIFY-AND-VERIFY, fast-forward main, close
  M0.G1.
- Blocked/unsure: none.
- Board: M0 🚧 with G1⚪; M1–M3 ⚪.

### 2026-07-15 · claude · `OBJ-SIMPLIFY-TIMERS.M0`
- Did: ADR-0005 accepted, plan simplify-and-verify landed, publish escalation rejected by owner, main fast-forwarded to working tip; OBJ-SIMPLIFY-TIMERS.M0.G1 🟢 at evidence SHA `0ab9614`.
- Board: OBJ-SIMPLIFY-TIMERS.M0 G1 🟢 — evidence docs/plans/simplify-and-verify.md.

### 2026-07-15 · claude · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: Re-adjudicated Apple Instant per ADR-0005 and adopted bare CNTVCT_EL0 at def4b87: public now() 0.93 ns vs quanta 3.30 ns on M1 Max (was 7.79); gates green on both feature surfaces; 216 tooling tests pass
- Found: CNTFRQ_EL0 is 24 MHz on M1/M2 but 1 GHz on M3/M4, so the instant scale must follow the selected provider; OrderedInstant keeps the self-sync route (isb+cntvct ~2x slower); evidence EVID-APPLE-BARE-CNTVCT
- Next: Remaining M1 freeze rows: AMD ordered probe, metal thread-cpu probe, c8g aarch64, windows-2022, AMD FreeBSD, suspend documentation run, mini full-crate battery
- Board: M0 ✅; M1 🚧 with the Apple Instant row adopted — evidence EVID-APPLE-BARE-CNTVCT

### 2026-07-15 · claude · HANDOFF → executor
- State: M0 ✅ closed with evidence; M1 🚧 — Apple Instant re-adjudicated and adopted at def4b87 (public now() 0.93 ns vs quanta 3.30 on M1 Max); all gates green on both feature surfaces; 216 tooling tests pass; publish escalation REJECTED and OBJ-RELEASE-0-2.M2 deferred
- Next: Run plan §5.0a green baseline, then the §5.2 probe table top to bottom (AMD ordered probe first); complete the freeze table before touching src in M2
- Traps: Plan §1.1 lists the verified traps: pushes need owner grant (escalate first); Apple scale follows the provider (1 GHz on M4); tooling accepts both Apple candidate sets; quanta now eligible on Apple; retry flaky timing tests serially; mini disk nearly full

### 2026-07-15 · spence · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: Ran §5.0a green baseline on catalyst (M1 Max) at 64c6141: fmt + clippy(default) + clippy(no-default) + test(no-default) + check-benches + doc all green; full default test suite green serially (36/36 lib incl elapsed_tracks_std_within_5_percent, plus all integration and doctests). src=42,269 lines (plan expected ~42,162).
- Found: elapsed_tracks_std_within_5_percent is ~3% intermittent under concurrent load (1/30 serial). Both failures coincide with std overshooting to ~110ms for a 100ms sleep: a preemption in the std::now()/tach::now() bracket, NOT a scale bug (tach absolute reads 102-105ms are correct; an M1 24MHz mis-scale would be orders of magnitude off). Per plan §1.1 an intermittent timing failure reran serially is not treated as a real §5.0a failure.
- Next: §5.2 same-target flip probes, starting with AMD c7a.large x86_64. Note: benches/run-speed-aws.sh refuses the amd/c7a cell (no canonical artifact declared) — resolve probe mechanics before launch.

### 2026-07-15 · spence · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: §5.3 exclusion re-audit in provider-policy-matrix.md: dissolved the Apple W-MAC-A64 bare-CNTVCT exclusion (an inadmissible Class-3 inferred 'XNU wake correction' requirement the published Instant contract never made; bare re-admitted+selected on both M1 Max and M4 Pro per EVID-APPLE-BARE-CNTVCT), and added the class-1 citation to the O-WINDOWS raw-TSC/redundant-fence exclusion (upheld: Windows designates QPC, documents no userspace TSC invariance). Both 'ineligible' footnotes now map to an admissible class; pre-decided upheld exclusions (Windows bare TSC, QueryThreadCycleTime, coarse clocks) recorded in Closure note 6.
- Found: The matrix carried exactly two 'ineligible' footnotes — Apple (dissolved) and Windows (upheld class-1); the other 70 cells already carry class-1/class-2/measured verdicts from OBJ-PROVE-TIMERS. Family verdicts stay provisional until the Apple §5.1(d) suspend measurement and the §5.2 same-target flip probes land.
- Next: Author mac-x86 (row 6) and wasm/rare-arch (row 7) class-1 residual verdicts; §5.2 flip rows await ESC-AMD-FLIP-PROBE-TOOLING and windows-2022 push authorization.
- Blocked/unsure: rows 1/3/5 gated on ESC-AMD-FLIP-PROBE-TOOLING; row 4 needs push authorization; Apple suspend (d) is owner-coordinated

## /goal

Deliver `OBJ-SIMPLIFY-TIMERS`'s slice of the VISION — *Every advertised target receives the
fastest eligible reliable timer for its timing contract.* — by cleanly exiting every milestone
gate. Done = each milestone is terminal either by passing every gate with committed evidence, or
by recording every non-green gate's disposition under its named Fallback and authority; no gate
weakened, no milestone closed by assertion.
