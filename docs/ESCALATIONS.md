# ESCALATIONS — tach

Owner-only blockers, filed by unattended agents so they can move on. When an agent hits a decision only the owner can make, it files a row here and continues with other work — it does not block and wait. The owner reviews this queue on their next interaction.

- **Lifecycle:** 🟠 OPEN → 👁 ACKED → ✅ ACCEPTED / ⛔️ REJECTED / ☑️ RESOLVED. ACCEPTED and REJECTED are **owner-only**; an agent may self-☑️ RESOLVE only when the blocker genuinely disappears.

## Open queue

| ID | Status | Description | Blocks | Context |
|---|---|---|---|---|
| `ESC-HOSTED-EVIDENCE-PUSH` | ☑️ RESOLVED | Approve pushing bench/six-clock-speed and dispatching .github/workflows/bench-speed-windows.yml with boundary=release-missing to collect only native Windows x86_64 and macOS Intel evidence. | `OBJ-PROVE-TIMERS.M1.G1` | [entry](#esc-hosted-evidence-push) |
| `ESC-PUBLISH-TACH-0-2-0-76FD4B1` | ⛔️ REJECTED | Approve publishing tach 0.2.0 from candidate 76fd4b1 after reviewing the final README and retained approval packet. | `OBJ-RELEASE-0-2.M2.G1` | [entry](#esc-publish-tach-0-2-0-76fd4b1) |
| `ESC-AMD-FLIP-PROBE-TOOLING` | 🟠 OPEN | Plan §5.2 W/O-LINUX-X86 flip probe names benches/run-speed-aws.sh for the AMD c7a.large run, but that script deliberately rejects the amd/c7a alias 'before any AWS call', enforced by test_runner_wiring.py::test_aws_rejects_unsupported_alias_before_any_aws_call. run-speed-aws.sh is canonical-primary-cell-only by design; AMD is not a canonical cell and both compose-speed.py and compose-supplemental-speed.py enforce fixed registries. The flip check only needs c7a's selected_provider, which lives in the collector bundle, so a sanctioned flip-probe branch (retain bundle + extract selection, honest aws-c7a runner tag, no canonical compose) is a ~15-line enabling change (prototyped, then reverted to keep the tree honest) — but it overrides a deliberate, tested guard, so it is an owner call, not an executor judgment. Validity is fine: the x86_64-gnu measured path is byte-identical to frozen speed-2-inteln.json at c64dcb73 (only apple_aarch64.rs/mod.rs/bench.rs changed since, all Apple-cfg-gated), so a c7a run at any current SHA is a valid same-target comparison. OPTIONS: (a) authorize the flip-probe branch + replace the guard's self-test with a flip-probe-wiring assertion (keep fail-fast on genuinely-unknown aliases); (b) point to the intended AMD mechanism; (c) accept AMD as a documented class-1 residual. RECOMMEND (a). Related: c8g (row 3) has no guard but mislabels under the c7g cell; FreeBSD c7a (row 5) runs via run-speed-freebsd-aws.sh into a fixed-name supplemental; windows-2022 (row 4) needs push authorization. | — | [entry](#esc-amd-flip-probe-tooling) |

## `ESC-HOSTED-EVIDENCE-PUSH`

- **Status:** ☑️ RESOLVED
- **Filed:** 2026-07-14 by unattended agent
- **Blocks gate:** `OBJ-PROVE-TIMERS.M1.G1`
- **Owner decision needed:** Approve pushing bench/six-clock-speed and dispatching .github/workflows/bench-speed-windows.yml with boundary=release-missing to collect only native Windows x86_64 and macOS Intel evidence.

- 2026-07-14 🟠 OPEN — filed; agent moved on to other work.
- 2026-07-14 ☑️ RESOLVED — the blocker genuinely disappeared.

## `ESC-PUBLISH-TACH-0-2-0-76FD4B1`

- **Status:** ⛔️ REJECTED
- **Filed:** 2026-07-14 by unattended agent
- **Blocks gate:** `OBJ-RELEASE-0-2.M2.G1`
- **Owner decision needed:** Approve publishing tach 0.2.0 from candidate 76fd4b1 after reviewing the final README and retained approval packet.

- 2026-07-14 🟠 OPEN — filed; agent moved on to other work.
- 2026-07-15 ⛔️ REJECTED — owner rejects; the blocker stands and the gate is not advanced.

## `ESC-AMD-FLIP-PROBE-TOOLING`

- **Status:** 🟠 OPEN
- **Filed:** 2026-07-15 by unattended agent
- **Owner decision needed:** Plan §5.2 W/O-LINUX-X86 flip probe names benches/run-speed-aws.sh for the AMD c7a.large run, but that script deliberately rejects the amd/c7a alias 'before any AWS call', enforced by test_runner_wiring.py::test_aws_rejects_unsupported_alias_before_any_aws_call. run-speed-aws.sh is canonical-primary-cell-only by design; AMD is not a canonical cell and both compose-speed.py and compose-supplemental-speed.py enforce fixed registries. The flip check only needs c7a's selected_provider, which lives in the collector bundle, so a sanctioned flip-probe branch (retain bundle + extract selection, honest aws-c7a runner tag, no canonical compose) is a ~15-line enabling change (prototyped, then reverted to keep the tree honest) — but it overrides a deliberate, tested guard, so it is an owner call, not an executor judgment. Validity is fine: the x86_64-gnu measured path is byte-identical to frozen speed-2-inteln.json at c64dcb73 (only apple_aarch64.rs/mod.rs/bench.rs changed since, all Apple-cfg-gated), so a c7a run at any current SHA is a valid same-target comparison. OPTIONS: (a) authorize the flip-probe branch + replace the guard's self-test with a flip-probe-wiring assertion (keep fail-fast on genuinely-unknown aliases); (b) point to the intended AMD mechanism; (c) accept AMD as a documented class-1 residual. RECOMMEND (a). Related: c8g (row 3) has no guard but mislabels under the c7g cell; FreeBSD c7a (row 5) runs via run-speed-freebsd-aws.sh into a fixed-name supplemental; windows-2022 (row 4) needs push authorization.

- 2026-07-15 🟠 OPEN — filed; agent moved on to other work.
- 2026-07-15 — scope: the same class of plan/tooling mismatch blocks the whole §5.2 flip-probe fleet, not just AMD. Row 2 (`T-LINUX-X86`, c5n.metal x86 thread-CPU) cannot run via `benches/run-thread-pmu-aws.sh` — it is aarch64-only (arm64 AMI at line 38, ships `aarch64-thread-pmu.c` with `pmccntr_el0` asm) and no x86 thread-pmu probe exists in `benches/probes/`. Rows 1 (AMD c7a), 2 (c5n.metal), 3 (c8g), and 5 (FreeBSD c7a) all resolve from the one owner decision on the flip-probe mechanism. Rows 6–7 are already verdicted (class-1 residuals); row 4 (windows-2022) additionally needs push authorization.
