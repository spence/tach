# Simplify and verify: fastest honest clock per target

Status: PLAN v1.0, 2026-07-15 — execution plan for `OBJ-SIMPLIFY-TIMERS`. Read
[`../STATUS.md`](../STATUS.md),
[`ADR-0005`](../decisions/0005-timer-contracts-eligibility-evidence-classes-and-selection-policy.md),
and [`../objectives/simplify-timers.md`](../objectives/simplify-timers.md) first. Supersedes the
withdrawn reset-based draft (its reset-point analysis is preserved in Appendix A).

## Context

The 2026-07 campaign left tach over-engineered (42K-line `src/`, ~26K-line Python proof ecosystem,
runtime tournaments on nearly every family) and its flagship Apple claim circular (an inferred
wake-correction requirement excluded a faster clock; the owner rejected that reasoning on
2026-07-15). This plan converts the crate to honest contracts, evidence-backed fixed picks with
capability gates, tournaments only where a frozen same-target flip exists, and a slim reproducible
benchmark story — on all 24 advertised targets, with no history reset and no publish until the
owner confirms fastest-per-type-per-architecture from fresh evidence.

This plan is written for a **less-capable executor agent**. Every step names concrete files,
commands, decision branches, and acceptance checks. If a situation is not covered by an enumerated
branch: `nsr escalate` and continue with other unblocked work. Do not invent or reinterpret
contracts, do not weaken gates, do not make unenumerated judgment calls.

## 0. Owner direction (2026-07-15, authoritative)

1. No history reset. Land the current tree on `main` and clean forward with ordinary commits.
2. No platform reduction. All 24 advertised targets stay, including the WASM runtimes
   (AGENTS.md Mission updated at `60b82eb`).
3. Honest contracts (ADR-0005): bare `CNTVCT_EL0` re-enters Apple `Instant` candidacy; every
   eligibility exclusion is re-audited against the evidence classes.
4. Runtime selection only where a frozen same-target two-environment flip among production
   candidates justifies it. Capability gates stay everywhere they guard availability.
5. Inline constraint: post-selection public reads within `max(1 ns, 5%)` of the exact mechanism.
6. **No publish authorization.** `ESC-PUBLISH-TACH-0-2-0-76FD4B1` is REJECTED;
   `OBJ-RELEASE-0-2.M2` is deferred on `OBJ-SIMPLIFY-TIMERS`.
7. Reuse retained benchmark evidence wherever it already answers a question; run new probes only
   for genuinely missing cells.

## 1. Executor discipline (read before every work session)

- Work on `main` (after §4 step 2) or short-lived branches off it. Never touch `backup/*` or
  `archive/*`, never force-push, never `cargo publish`, never push tags.
- Conventional commits; keep `src/`-only and `docs/`-only changes in separate commits.
- `nsr` owns objective/gate/escalation state: `nsr log` for entries, `nsr close-gate` with
  `--evidence` for gate flips, `nsr render` + `nsr check` in the same pass. Never hand-edit a
  `render:` region.
- Tooling gotchas: a shell hook proxies commands through `rtk` — `git log` output is capped at
  ~50 entries (use `git rev-list` or `rtk proxy git ...`); use `/usr/bin/find` for `-exec`.
- **STANDARD GATE** (after every code phase; all must pass):
  `cargo fmt --all -- --check` · `cargo clippy --all-targets -- -D warnings` ·
  `cargo clippy --no-default-features --all-targets -- -D warnings` · `cargo test` ·
  `cargo test --no-default-features` · `cargo check --benches` · `cargo doc --no-deps` · the
  phase's consistency greps (zero live references to anything the phase removed).
- AWS discipline: add the current IP to bench SG `sg-05e99abafa54936d3` before SSH; always launch
  with `--instance-initiated-shutdown-behavior terminate`; terminate instances explicitly after
  runs and verify none remain:
  `aws ec2 describe-instances --filters Name=tag:Name,Values=tach-bench-* --query "Reservations[].Instances[?State.Name!='terminated'].[InstanceId,InstanceType,State.Name]" --output text --region us-east-2`.

### 1.1 Handoff traps (verified 2026-07-15 — read every line before starting)

- **Remote pushes need owner authorization.** The windows-2022 probe (§5.2) and CI verification
  require pushing to origin. Until the owner records that grant (here or in an escalation
  resolution), the first push is an `nsr escalate`, not a judgment call. Force-push, tags, and
  publish remain forbidden regardless.
- **Apple scale follows the provider.** `CNTFRQ_EL0` is 24 MHz on M1/M2 and 1 GHz on M3/M4; the
  bare counter is NOT in the Mach tick domain on M3/M4. Never freeze the instant scale to a
  constant or the Mach timebase (`apple_aarch64::instant_nanos_per_tick_q32` is the pattern).
- **The tooling accepts both Apple candidate sets.** `benches/speed_evidence.py` validates frozen
  pre-adoption artifacts (4 candidates) and post-`def4b87` collections (5, bare first).
  `verify-target-providers.py` carries no Apple candidate-name coupling, but its first CI run
  after the adoption is the real proof — if `provider-proof` fails, register the new route where
  its failure message points; do not revert the adoption.
- **quanta is now an ELIGIBLE Apple reference** (same mechanism as the adopted provider). The M3
  claims rewrite treats it as a competitor — tach beats it 0.93 ns vs 3.30 ns on the M1 Max — not
  as an ineligible diagnostic. The harness tag
  `apple_bare_cntvct_omits_xnu_wake_correction_and_may_suspend_diverge` describes the dissolved
  exclusion and must not survive the M3 rewrite.
- **Timing-sensitive tests that fail intermittently**: rerun serially
  (`cargo test -- --test-threads=1`) before treating the failure as real; a persistent failure is
  a finding — never weaken or skip the test.
- **`catalyst-mini` disk is nearly full** (~1.5 GiB free of 460 after a `reap` sweep). Anything
  needing space on the mini (full-crate battery builds) should use a small scratch or wait for the
  owner to clear space; `reap sweep --apply` is pre-authorized for cargo target dirs.
- `python3 -m unittest discover -s benches -p 'test_*.py'` leaves `benches/__pycache__/` — remove
  it before committing.

## 2. The contracts and pre-decided rulings (ADR-0005 — never reinterpret)

Contracts: see ADR-0005 §Decision (Instant / OrderedInstant / ThreadCpuInstant, including the
platform-defined suspend stance). Copy them into `lib.rs`/README wording during §7, verbatim in
meaning.

Pre-decided rulings the executor applies without revisiting:
- Apple bare `CNTVCT_EL0` IS an `Instant` candidate; its barriered form IS an `OrderedInstant`
  candidate (§5.1 decides by evidence).
- Windows bare TSC stays excluded (class 1); `QueryThreadCycleTime` stays excluded (class 1);
  deliberately coarsened clocks stay excluded (class 1).
- An unbarriered read is never advertised as synchronization-ordered.

## 3. Selection rule

- Fixed `cfg` pick + capability gate is the default for every family.
- A measured tournament survives only with a frozen same-target two-environment flip among
  production candidates. Expected survivor: `T-LINUX-X86`, pending §5.2's metal probe.
- Bench-side audits (the `T-LINUX-A64` pattern from
  [`aarch64-thread-cpu-runtime-selection`](../investigations/aarch64-thread-cpu-runtime-selection.md))
  detect future flips and reopen selection; they never select in production.
- Inline constraint verified with the existing paired public/exact probes.

## 4. M0 — Honest contracts and selection policy

1. Safety anchors: `git branch archive/pre-simplify-2026-07-15 44010d4` and
   `git tag pre-simplify-44010d4 44010d4` (local only; do not push).
2. `git checkout main && git merge --ff-only bench/six-clock-speed` — `main` becomes the working
   line for every later phase.
3. `nsr render` then `nsr check` pass; close the gate:
   `nsr close-gate OBJ-SIMPLIFY-TIMERS.M0.G1 --result pass --evidence docs/plans/simplify-and-verify.md --did "<summary>"`.

(Steps 1–3 were executed on 2026-07-15; verify the anchors resolve before starting M1.)

## 5. M1 — Eligibility re-adjudication and flip verification

### 5.0a Session start — green baseline before anything else

Run the STANDARD GATE once on the untouched tree and record
`/usr/bin/find src -name '*.rs' | xargs wc -l | tail -1` (expect ~42,162). CI proved this tree at
`76fd4b1` (27/27 jobs); re-prove locally once before any phase changes code. A failure here is a
finding — `nsr escalate` it; do not start M2 on a red baseline.

### 5.0 Mine retained evidence FIRST (no new runs for answered questions)

- `benches/speed-0-apple.json` + `docs/evidence/timers/release-speed-closure-2026-07-14/`:
  extract every Apple candidate row — ineligible-diagnostic rows were retained. Record bare
  `CNTVCT_EL0` read cost vs the commpage route vs quanta.
- `benches/speed-2-inteln.json`: frozen Linux x86_64 winners (`linux_kernel_eligible_tsc`,
  `…_x86_lfence_rdtsc`) and the fallback ranking (libc `CLOCK_MONOTONIC` first).
- `benches/speed-1-c7g.json`: frozen aarch64 winners (`aarch64_cntvct`, `aarch64_isb_cntvct`).
- [`aarch64-thread-cpu-runtime-selection`](../investigations/aarch64-thread-cpu-runtime-selection.md):
  the no-flip survey is complete — do NOT rerun.
- `benches/ORDERED-VERIFICATION.md` + `benches/ordered-verify-*.json`: ordering evidence — keep,
  do NOT rerun.

### 5.1 Apple re-adjudication (two local machines; the flagship question)

Machines: `catalyst` (M1 Max MacBook Pro, this repo) and `catalyst-mini` (M4 Pro, `ssh macmini`,
non-interactive SSH gets a bare PATH — prepend `/opt/homebrew/bin` or use absolute paths). Same
target `aarch64-apple-darwin`, two environments.

Correctness battery per machine (build the probe under `benches/`; `bench-internal` exposes raw
counter accessors):
- (a) Same-thread monotonicity: ≥1e9 paired bare-`CNTVCT_EL0` reads asserting never-backward,
  cycling QoS classes to force P→E core migrations. Expect 0 violations.
- (b) Wall-rate: bare-counter elapsed vs `std::time::Instant` over 100 ms sleeps ×100; every ratio
  within ±5%.
- (c) Frequency sanity: `CNTFRQ_EL0` matches the measured rate against `mach_absolute_time` over
  ≥10 s.
- (d) Suspend/wake — **catalyst ONLY; never sleep the headless mini; coordinate the window with
  the owner first**: sample, `sudo pmset sleepnow`, wake after ≥60 s, resample; repeat ×5. Assert
  no backward step; RECORD (do not judge) whether elapsed includes the sleep interval — that
  becomes the documented platform suspend semantic.
- Speed: paired probes bare vs commpage route vs quanta on both machines.

Decision table (mechanical):
- (a)–(d) pass on both machines AND bare wins by more than `max(1 ns, 5%)` → bare `CNTVCT_EL0`
  becomes Apple `Instant`'s fixed pick; the barriered bare form enters the `OrderedInstant` freeze
  the same way (the ordering harness must still show 0 violations); document the measured suspend
  semantic; retain a new investigation + evidence.
- ANY correctness failure → keep the commpage route; retain the failing run as frozen class-2
  evidence (the exclusion becomes legitimate); rewrite the public claim with §7.3 template B.
- Mixed or unclear → `nsr escalate`; continue elsewhere.

**Executed 2026-07-15** (`EVID-APPLE-BARE-CNTVCT`): (a)–(c) plus speed passed on both machines —
0 violations in ~2.8e9 paired reads, wall rate 0.99997, bare read 0.33/0.44 ns vs 5.4/3.8 ns for
the prior route. Bare `CNTVCT_EL0` is adopted as an `Instant`-only candidate and the live
tournament selects it: public `Instant::now()` 0.93 ns (was 7.79), roundtrip 2.27 ns (was 15.47),
vs quanta 3.30/7.22 in the same run; inline parity holds. `OrderedInstant` is unchanged —
`isb+cntvct` measured ~2× slower than the selected self-synchronizing route on both machines.
Critical implementation fact: the bare counter is its own tick domain (`CNTFRQ_EL0` = 24 MHz on
M1/M2 but **1 GHz on M3/M4**) — the instant scale follows the selected provider and must never be
frozen to the Mach timebase or a constant. Remaining from this section: the item (d) suspend
documentation run (owner-coordinated) and the full-crate battery on `catalyst-mini`.

### 5.2 Same-target flip runs (new evidence; cheap; enumerated)

| Family | Frozen env #1 | New env #2 | Recipe | Branch on outcome |
|---|---|---|---|---|
| `W/O-LINUX-X86` | c7i (TSC; LFENCE+RDTSC) | `c7a.large` (AMD Zen4, ~$0.09/hr) | `benches/run-speed-aws.sh` paired probes | Same winners → freeze fixed. Different ordered winner → first real flip: `O-LINUX-X86` stays measured; retain evidence |
| `T-LINUX-X86` | c7i (syscall won; perf available-but-slower) | `c5n.metal` (~$3.89/hr, minutes; `cap_user_rdpmc=true`) | `benches/run-thread-pmu-aws.sh` | perf mmap wins on metal → flip frozen: tournament stays WITH evidence. Syscall wins there too → convert to capability policy (mmap handshake → else raw syscall) |
| `W/O-LINUX-A64` | c7g | `c8g.large` (~$0.07/hr) | `run-speed-aws.sh` wall/ordered probes | Same winners → freeze fixed. Flip → stays measured; retain |
| `W/O-WINDOWS` | windows-2025 runner (QPC) | `windows-2022` runner | dispatch the existing Windows bench workflow on the second runner label | Same winner → freeze QPC fixed. Flip → `nsr escalate` (unexpected) |
| `W/O-FREEBSD-X86` | c7i FreeBSD 15 (TSC; LFENCE+TSC) | `c7a.large` FreeBSD | `benches/run-speed-freebsd-aws.sh` | Same → freeze fixed. Flip → stays measured; retain |
| `W/O-MAC-X86` | macos-15-intel runner | none available | — | Freeze on the single frozen run + class-1 documentation; record residual |
| Rare Linux arches, wasm/WASI | none | none | — | Freeze on class-1 architecture documentation; record residual "source/codegen-proven; not performance-measured"; publish no fastest claim for them |

Retain every run via `nsr new evidence --topic timers …` with source SHA, instance identity, and
raw output. Terminate instances; verify no orphans.

### 5.3 Exclusion re-audit

Produce one table (updating [`provider-policy-matrix`](provider-policy-matrix.md)): every
"ineligible" footnote → its class-1 citation or class-2 frozen evidence. An exclusion citing
neither dissolves into candidacy and joins its family's freeze procedure. Known dissolution:
Apple bare counter (§5.1). Known upheld (pre-decided): Windows bare TSC, `QueryThreadCycleTime`,
coarse clocks.

Close `M1.G1` when all 72 target/timer cells carry a verdict plus evidence or documented residual.

## 6. M2 — Fixed-pick conversion with inline parity

1. **Relocate embedded tests first** (~171 `#[test]` fns in `src/`): integration-shaped tests move
   to `tests/`; genuinely private unit blocks stay minimal. Record before/after totals; they must
   reconcile. STANDARD GATE.
2. **Convert families per the frozen table, one family per commit:**
   - Install the frozen winner as the `cfg`-selected implementation; keep its capability gate and
     failure fallback; delete candidate enums, selection stopwatches, alternating-batch selectors,
     and selector state that existed only to measure. Keep fork-safety state only where a
     surviving gate installs process-wide state.
   - Keep `T-LINUX-A64` (capability policy + bench-only audit) unchanged. Keep the `T-LINUX-X86`
     tournament only if §5.2 froze its flip.
   - Linux wall fallback = libc `CLOCK_MONOTONIC` (the frozen fallback ranking). Delete the
     direct-vDSO resolution layer (`src/arch/linux_vdso.rs`) unless a kept family's frozen
     evidence shows a direct-vDSO route winning — the c7i/c7g data does not.
   - After each family: grep the deleted provider names, symbols, and cfg strings across `src/`,
     `benches/`, `docs/`, `.github/`, `Cargo.toml`, README, BENCHMARKS; read every hit; zero live
     references remain.
3. **Shrink `src/bench.rs`** and the `#[cfg(feature = "bench-internal")]` blocks in arch files to
   the retained surface: paired public/exact probes, retained audits, six-cell harness accessors.
   `bench-quanta` stays (`benches/skew.rs` is the ordering harness).
4. **Inline parity:** run the paired public/exact probe for every converted family runnable
   locally or in CI; each within `max(1 ns, 5%)`. The six native cells re-verify fully in M3.

Close `M2.G1` on: STANDARD GATE green on both feature surfaces; tournament-symbol grep clean
outside retained families; parity results recorded; test counts reconciled.

## 7. M3 — Apparatus diet and truthful claims

### 7.1 Delete from the live tree (the archive branch retains everything)

- Sealed bundles: `docs/evidence/timers/release-speed-closure-2026-07-14/collector-bundles.tgz`
  (10.9 MB), `docs/evidence/timers/provider-policy-closure-2026-07-14/freebsd-collector-bundle-*.tgz`,
  and dated evidence dirs that exist only to prove the 15-boundary campaign (keep dirs M1
  created).
- Forensics Python + self-tests: `validate-release-evidence.py`, `validate-speed-evidence.py`,
  `validate-supplemental-thread-cpu.py`, `seal-speed-source.py`,
  `require-clean-benchmark-source.sh`, `verify-skewmono.sh`, `release_matrix.py`, all
  `benches/test_*.py`, and `speed_evidence.py` once `grep -rn 'speed_evidence' benches/` shows no
  surviving importer.
- Supplemental orchestrators and their data: `run-speed-lambda.sh`, `run-skewmono-lambda.sh`,
  `run-skewmono-aws.sh`, `run-skewmono-local.sh`, `run-emscripten-reentry.sh`,
  `run-runtime-smoke.sh`, `run-browser-host-runtime.mjs`, `aggregate-skewmono.py`,
  `benches/lambda-speed/`, `benches/host-runtime-speed/`, `benches/emscripten-reentry/`,
  `benches/probes/`, `benches/runtime-smoke/`, `benches/skewmono-*.json`,
  `benches/report-skewmono-*.svg`.
- KEEP: `verify-target-providers.py` (the 24-target availability/routing proof backing the
  all-architectures vision), `collect-speed-bundle.py`, `bench_data.py`, `compose-speed.py`,
  `run-speed-aws.sh`, `run-speed-freebsd-aws.sh`, `run-speed-local.sh`, `run-speed-criterion.sh`,
  `run-thread-pmu-aws.sh`, `run-ordered-verify-aws.sh`, chart scripts (`summary*.py`,
  `report.py`), `ORDERED-VERIFICATION.md`, `ordered-verify-*.json`, `benches/instant.rs`,
  `benches/skew.rs`. `extract_speed.py`, `release_chart.py`, `release-boundaries.toml`,
  `route-coverage.toml`: keep ONLY if a kept script imports/reads them — check with grep, delete
  if orphaned.

### 7.2 CI

- `ci.yml`: keep `native` (6-way), `cross-check` (all-target `cargo check`), one `provider-proof`
  job, `package-check`, `msrv`, `benchmark-check`. Delete `claim-evidence`.
- `bench-speed-windows.yml`: delete the supplemental jobs; keep one canonical Windows bench
  dispatch (fold into `bench.yml` if simpler). `bench-skew.yml`: keep the
  `windows-ordered-contract` job (ordering evidence); delete the `full` skew campaign. Grep all
  workflows for deleted paths.

### 7.3 Claims rewrite (mechanical)

- Re-measure the six native cells on the converted tree (`run-speed-local.sh` for the Macs,
  `run-speed-aws.sh` for c7i/c7g, the Windows dispatch, `run-speed-freebsd-aws.sh`); regenerate
  charts; replace the BENCHMARKS numbers; bind them to the new source SHA.
- Template A (bare counter won §5.1): "`Instant` reads the architectural counter directly on
  Apple Silicon; suspend semantics are documented per platform (Platform notes); measured against
  quanta/fastant/minstant/std on <environments> at <SHA>."
- Template B (bare counter failed §5.1): "On Apple Silicon `Instant` uses the XNU-corrected
  route: the bare counter measured faster but violated <contract clause> in <frozen evidence>;
  quanta's bare read is faster and carries that documented behavior."
- Global: remove 24-target/15-boundary campaign framing; fix the stale "RDTSCP on x86" prose to
  the frozen winner; every named provider/environment maps to a kept code path or retained
  evidence file; eligibility footnotes cite ADR-0005 classes; copy the ADR-0005 contracts into
  `lib.rs`/README wording.

Close `M3.G1` on the gate's grep battery plus the fresh numbers.

## Verification

The objective's four gates are the end-to-end confirmation, in order: M0.G1 (base landed, policy
ratified, green baseline) → M1.G1 (72/72 freeze verdicts with evidence/residual) → M2.G1
(converted code matches the table on both feature surfaces with inline parity and reconciled test
counts) → M3.G1 (slim tree, slim CI, claims tracing to fresh live evidence). Definition of done
additionally requires: safety anchors resolve; no push, tag, or publish happened;
`OBJ-RELEASE-0-2.M2` remains deferred until the owner reviews the fresh claims.

## Appendix A — withdrawn reset-point analysis (reference only)

| Candidate | SHA | Date | Has | Lacks |
|---|---|---|---|---|
| main (pre-blitz) | `43146bf` | 05-31 | Two types, honest two-type README, NUMA verification | No `ThreadCpuInstant`, no per-arch providers; re-landing meant re-deriving an uncherry-pickable +37K-line commit |
| `6a80582` | — | 07-12 | Three timers functional, pre-apparatus | README omitted the third timer; thin provider tests |
| `76fd4b1` | HEAD~2 | 07-14 | Three timers, honest README, digest-identical to frozen bench revisions | Full apparatus in tree |

"Functional three timers plus honest docs" and "before the evidence apparatus" never coexisted in
history; the owner chose to work in place instead of resetting.

## Suggested /goal

Anchor: **the fastest honest clock on every surface we advertise.** tach promises three timer
contracts, each reading the fastest eligible clock wherever the crate runs, with hot reads at
inline cost, eligibility decided by documented behavior or frozen evidence — never by inferred
requirements — and selection machinery existing only where frozen evidence proves a real choice.
Strip everything that does not serve that promise, and keep every public claim pointing at
evidence that describes the code as shipped. Fast, correct, cross-thread safe, never crashing.
