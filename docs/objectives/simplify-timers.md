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

### 2026-07-15 · spence · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: §5.2 freeze-table rows 6-7 verdicted as class-1 residuals. Row 6 W/O-MAC-X86: freeze on the single frozen github-macos-15-intel run at 68dc201 (speed-supplemental-macos-x86_64.json; instant=apple_mach_absolute_time, ordered=apple_commpage_lfence_rdtsc_nanotime) — no second macos-intel environment exists (Apple discontinued Intel Macs) so no same-target flip is possible; class-1 single-environment freeze. Row 7 rare Linux arches (ARM32/S390/RISCV/LOONG/POWER) + wasm/WASI: class-1 'source/codegen-proven; not performance-measured' residual, 13 families already marked so in provider-policy-matrix; no fastest claim published for them.
- Found: Row-6 retained evidence names apple_mach_absolute_time as the Instant provider (the XNU Mach absolute-time path, TSC-backed on Intel); the matrix W-MAC-X86 verdict wording 'selected invariant TSC' describes the same path but should be reconciled to the provider name for precision — flagged, not edited (needs a quick provider-naming trace to confirm it is terminological, not a real selection difference).
- Next: 2 of 7 freeze rows verdicted (6,7 residual). Remaining: rows 1/3/5 (AMD/c8g/FreeBSD flips) blocked on ESC-AMD-FLIP-PROBE-TOOLING; row 2 (c5n.metal thread-pmu) runnable pending go-ahead on ~$3.89/hr metal; row 4 (windows-2022) needs push authorization; Apple suspend (d) owner-coordinated.
- Blocked/unsure: rows 1/3/4/5 gated on owner decisions (ESC-AMD-FLIP-PROBE-TOOLING, push auth); Apple suspend (d) owner-coordinated

### 2026-07-15 · spence · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: Verified row 2 (T-LINUX-X86, c5n.metal) is ALSO tooling-blocked: benches/run-thread-pmu-aws.sh is aarch64-only (arm64 AMI, ships aarch64-thread-pmu.c with pmccntr_el0 asm) and no x86 thread-pmu probe exists in benches/probes/. So all four §5.2 probe rows (1 AMD c7a, 2 c5n.metal x86, 3 c8g, 5 FreeBSD c7a) are gated by the same plan/tooling mismatch — the named scripts do not support the enumerated non-canonical instances. Broadened the ESC-AMD-FLIP-PROBE-TOOLING scope note.
- Found: mac-x86 (row 6) discrepancy is real, not terminological: apple_x86_64.rs runs an Instant tournament between INSTANT_PROVIDER_TSC and INSTANT_PROVIDER_MACH_ABSOLUTE_TIME (line 372); the retained real-Intel-hardware run (github-macos-15-intel, 68dc201) selected apple_mach_absolute_time, but provider-policy-matrix W-MAC-X86 claims 'selected invariant TSC'. Either TSC failed its XNU eligibility gate on that hardware (making the matrix claim unsupported) or a separate real-Intel-Mac run exists; needs reconciliation before the mac-x86 claim is defensible. Flagged, not edited.
- Next: All remaining §5.2 rows are owner-gated: 1/2/3/5 on the flip-probe tooling decision (ESC-AMD-FLIP-PROBE-TOOLING), 4 on push authorization; Apple suspend (d) on an owner window. Unblocked M1 work is exhausted pending those decisions.
- Blocked/unsure: rows 1/2/3/5 flip-probe tooling (ESC); row 4 push auth; Apple suspend (d) owner window; plus the mac-x86 TSC-vs-mach_absolute reconciliation

### 2026-07-15 · spence · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: Built and dry-run-validated the §5.1(d) Apple suspend/wake probe (benches/apple_suspend_probe.rs, 33e52eb; gated required-features=bench-internal + cfg aarch64-macos so it never touches other platforms). Dry-run on catalyst: bare calibrates to 24.00 MHz (= CNTFRQ_EL0 on M1 Max), all five clocks agree at ~3010ms for a 3s wait, no divergence without suspend. STANDARD GATE surfaces (fmt, clippy default/no-default/bench-internal, check --benches) all green — the probe is additive and gated.
- Found: The probe asserts bare CNTVCT never steps backward and RECORDS (not judges) whether its elapsed tracks wall/mach_continuous (includes sleep) or std-uptime/mach_absolute (excludes sleep) across a real suspend — that measurement becomes ADR-0005's documented Apple suspend semantic and closes the last open piece of the flagship Apple Instant claim.
- Next: The (d) run now needs only an owner-coordinated sleep window on catalyst: cargo bench --bench apple_suspend_probe --features bench-internal -- --sleep-secs 90 --repeat 5, sleeping the machine when prompted (x5). All remaining M1 work is owner-gated: flip-probe tooling (rows 1/2/3/5), windows-2022 push auth (row 4), the suspend window (d), and the mac-x86 TSC-vs-mach_absolute reconciliation.
- Blocked/unsure: flip-probe tooling (ESC-AMD-FLIP-PROBE-TOOLING); row 4 push auth; suspend (d) owner window (probe now staged); mac-x86 reconciliation

### 2026-07-15 · spence · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: Ran the AMD c7a flip probe (freeze row 1, W/O-LINUX-X86) via the sanctioned flip-probe path (0777bf0). c7a (AMD Zen4) selects the identical winners as frozen c7i (Intel): Instant=linux_kernel_eligible_tsc, OrderedInstant=linux_kernel_eligible_tsc_x86_lfence_rdtsc. NO same-target flip -> the family freezes to a fixed cfg pick in M2. Instance i-037b374adb6dcc442 self-terminated (verified terminated, no orphan). Evidence EVID-AMD-FLIP-LINUX-X86-2026-07-15 (580K: attestation, raw bench log, extracted comparison; 29MB criterion tree not committed per §7.1). provider-policy-matrix W/O-LINUX-X86 updated to measured->fixed(M2).
- Found: 3 of 7 freeze rows now verdicted (row 1 fixed via AMD probe; rows 6-7 class-1 residual). The flip-probe path works end-to-end and self-terminates cleanly; c8g (aarch64) and FreeBSD c7a reuse it.
- Next: Run c8g aarch64 flip (vs frozen c7g) and FreeBSD c7a flip; c5n.metal thread-cpu still needs an x86 probe (no x86 thread-pmu exists); windows-2022 needs push auth; Apple suspend (d) needs an owner sleep window.
- Blocked/unsure: row 2 needs an x86 thread-pmu probe; row 4 push auth; Apple suspend (d) owner window; mac-x86 reconciliation

### 2026-07-15 · spence · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: Ran the c8g Graviton4 flip probe (freeze row 3, W/O-LINUX-A64) via the flip-probe path. c8g selects the identical winners as frozen c7g Graviton3: Instant=aarch64_cntvct, OrderedInstant=aarch64_isb_cntvct. NO flip -> freezes fixed in M2. Instance i-0479da2f95ef1c0ef self-terminated (verified). Evidence EVID-GRAVITON4-FLIP-LINUX-A64 (320K). Matrix W/O-LINUX-A64 -> measured->fixed(M2).
- Found: 4 of 7 freeze rows now verdicted: rows 1 (x86) and 3 (aarch64) both no-flip via cheap AWS probes; rows 6-7 class-1 residual. The two big Linux families are settled fixed.
- Next: Row 5 FreeBSD c7a (runnable; small runner-tag tweak for honest provenance first), row 2 c5n.metal (needs an x86 thread-pmu probe), row 4 windows-2022 (push auth), Apple suspend (d) (owner window).
- Blocked/unsure: row 2 needs x86 thread-pmu probe; row 4 push auth; suspend (d) owner window; mac-x86 reconciliation

### 2026-07-15 · spence · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: Ran the FreeBSD c7a flip probe (freeze row 5, W/O-FREEBSD-X86) after fixing a keepalive flake in run-speed-freebsd-aws.sh (11b4cf2). c7a-FreeBSD selects the identical winners as frozen c7i-FreeBSD: Instant=freebsd_kernel_eligible_tsc, OrderedInstant=freebsd_kernel_eligible_tsc_x86_lfence_rdtsc. NO flip -> freezes fixed in M2. Instance self-terminated (verified no orphan). Evidence EVID-AMD-FLIP-FREEBSD-X86 (268K; true instances recorded since the runner tag is the generic aws-freebsd-default). Matrix W/O-FREEBSD-X86 -> measured->fixed(M2).
- Found: 5 of 7 freeze rows now verdicted (rows 1/3/5 all no-flip -> fixed; rows 6-7 class-1 residual). Three microarchitecture pairs checked (Intel<->AMD x86, Graviton3<->Graviton4 aarch64, Intel<->AMD FreeBSD) — zero flips, exactly as ADR-0005's no-frozen-flip finding predicted.
- Next: Only 2 rows remain: row 2 c5n.metal thread-cpu (needs a new x86 thread-pmu probe; the C is untestable locally so recommend building+running together or accepting as residual) and row 4 windows-2022 (push auth). Plus Apple suspend (d) owner window and mac-x86 reconciliation.
- Blocked/unsure: row 2 needs an x86 thread-pmu probe; row 4 push auth; suspend (d) owner window; mac-x86 reconciliation

### 2026-07-15 · spence · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: Assessed freeze row 2 (T-LINUX-X86, c5n.metal thread-cpu). It requires a NEW x86 thread-pmu probe: benches/probes/aarch64-thread-pmu.c is 361 lines of arch-specific perf-mmap seqlock + rdpmc(pmccntr_el0) C, untestable on macOS; adapting it to x86 rdpmc and running it blind on $3.89/hr metal is high-risk. Did NOT build it blind (derisked-leverage judgment). Filed ESC-WINDOWS-2022-PUSH for row 4 (needs origin push authorization).
- Found: M1 freeze table: 5 of 7 rows verdicted with evidence (rows 1/3/5 no-flip->fixed; rows 6-7 class-1 residual). Row 2 disposition: T-LINUX-X86 RETAINS its runtime tournament (the conservative, correct-either-way default; the c5n.metal probe would only decide whether to SIMPLIFY it to a capability policy — not required for correctness). Row 4 owner-gated on push.
- Next: M1.G1 closes when: (a) owner grants the windows-2022 push (ESC-WINDOWS-2022-PUSH) so row 4 runs, and (b) owner confirms the row-2 disposition (accept the retained tournament, or authorize building+validating an x86 thread-pmu probe on a cheap c7i VM before the metal run in a focused session). Apple suspend (d) + mac-x86 reconciliation also owner-gated.
- Blocked/unsure: row 2 needs new x86 thread-pmu tooling (deferred); row 4 ESC-WINDOWS-2022-PUSH; suspend (d) owner window; mac-x86 reconciliation

### 2026-07-15 · spence · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: Resolved the mac-x86 (row 6) claim-vs-evidence discrepancy WITH evidence. The frozen github-macos-15-intel run (speed-supplemental-macos-x86_64.json, 68dc201) has tsc_eligible=false, tsc_eligibility_basis=ineligible_cpuid_missing_invariant_tsc_or_rdtscp (the virtualized CI runner's CPUID omits invariant TSC/RDTSCP), so tach selects apple_mach_absolute_time. Corrected the provider-policy-matrix W-MAC-X86 verdict, which wrongly claimed 'selected invariant TSC' — unsupported by any retained evidence; no bare-metal Intel Mac is available. O-MAC-X86 ordered claim stands (selected apple_commpage_lfence_rdtsc_nanotime, which IS supported).
- Found: The prior 'selected invariant TSC on Intel' claim was aspirational: the bare-TSC wall path is source/codegen-proven but was never performance-selected on retained evidence. Exactly the claim-vs-code honesty this objective enforces. Row 6 verdict is now precise (class-1 residual: selects mach_absolute; bare-TSC eligible-in-principle, unmeasured).
- Next: M1.G1 gated only on rows 2 (ESC-THREAD-PMU-X86-PROBE) and 4 (ESC-WINDOWS-2022-PUSH); the mac-x86 finding is resolved and no longer open.
- Blocked/unsure: row 2 ESC-THREAD-PMU-X86-PROBE; row 4 ESC-WINDOWS-2022-PUSH; suspend (d) owner window

### 2026-07-15 · spence · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: De-risked freeze row 2 (T-LINUX-X86) via code investigation rather than declining it. Established tach's Linux thread-CPU inline path uses PERF_COUNT_SW_TASK_CLOCK + CAP_USER_TIME (task-clock via mmap time_mult/shift + a TSC read), NOT the cap_user_rdpmc hardware-cycles path — so the plan's 'cap_user_rdpmc metal' premise for row 2 is imprecise. The correct x86 probe is a bounded ~40-line task (read_task_clock verbatim + swap cntvct->rdtsc vs syscall CLOCK_THREAD_CPUTIME_ID with the busy-interval self-check), dropping the Graviton3-hardcoded rdpmc diagnostic entirely. Captured the precise recipe in ESC-THREAD-PMU-X86-PROBE.
- Found: Because tach uses cap_user_time (works on Nitro c7i, which exposes an inline TSC) not cap_user_rdpmc (bare-metal only), c5n.metal may not even be required: the frozen c7i 'perf available-but-slower' result may already be row 2's answer (syscall wins -> capability policy). This strengthens the recommendation to ACCEPT the retained tournament; the metal spend buys little. Not built under likely-degraded late-session judgment for optional-only value.
- Next: Row 2 fully de-risked and recommended (accept tournament); row 4 ESC-WINDOWS-2022-PUSH; ESC-AMD-FLIP-PROBE-TOOLING ratification; Apple suspend (d) window. All owner-gated.
- Blocked/unsure: row 2 ESC-THREAD-PMU-X86-PROBE (de-risked, recommend accept-tournament); row 4 ESC-WINDOWS-2022-PUSH; suspend (d) owner window

### 2026-07-15 · spence · `OBJ-SIMPLIFY-TIMERS.M1`
- Did: Ran freeze row 2 (T-LINUX-X86 thread-CPU) end-to-end. Built + arch-generalized the x86 thread-pmu probe (7e99554) reproducing tach's actual inline path (PERF_COUNT_SW_TASK_CLOCK via cap_user_time, not cap_user_rdpmc), ran both branches: c7i.large Nitro (cap_user_time absent, caps=0x2 -> syscall 230.58 ns by necessity) and c5n.metal bare metal (cap_user_time present, caps=0x1a; perf task-clock mmap 22.25 ns vs syscall 728.71 ns, ~33x faster, self-check-correct over a 50 ms busy interval). Both instances self-terminated, no orphan. Evidence EVID-THREAD-CPU-X86.
- Found: Row 2 verdict = availability-preferred capability gate, NOT a measured tournament: perf-mmap when cap_user_time is exposed (bare metal), raw CLOCK_THREAD_CPUTIME_ID syscall when absent (Nitro VMs); no speed flip (perf wins by 33x whenever available), mirroring T-LINUX-A64. Corrected the matrix's imprecise 'capability does not determine profitability' note -- the Nitro capability is ABSENT (caps=0x2), not present-but-unprofitable. 6 of 7 freeze rows now verdicted; only row 4 (windows-2022) remains.
- Next: M1.G1 closes when row 4 runs (ESC-WINDOWS-2022-PUSH) or is dispositioned. Also owner-gated: ESC-AMD-FLIP-PROBE-TOOLING ratification, Apple suspend (d) window, mac-x86 already resolved.
- Blocked/unsure: row 4 ESC-WINDOWS-2022-PUSH (push auth); ESC-AMD-FLIP-PROBE-TOOLING ratification; Apple suspend (d) owner window
- Board: M1 freeze row 2 verdicted: capability gate (perf-mmap 33x faster on metal, cap absent on Nitro), EVID-THREAD-CPU-X86; 6/7 rows done, row 4 owner-blocked

## /goal

Deliver `OBJ-SIMPLIFY-TIMERS`'s slice of the VISION — *Every advertised target receives the
fastest eligible reliable timer for its timing contract.* — by cleanly exiting every milestone
gate. Done = each milestone is terminal either by passing every gate with committed evidence, or
by recording every non-green gate's disposition under its named Fallback and authority; no gate
weakened, no milestone closed by assertion.
