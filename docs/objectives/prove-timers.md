# `OBJ-PROVE-TIMERS` — Prove default timer selection by decision boundary

**VISION slice.** On every advertised target, each default timer uses the fastest eligible reliable
provider available to the running program, and every public claim states whether it is proved
universally or measured on a named environment.

This objective proves one frozen shipping implementation without multiplying targets by build modes.
Static target proof establishes universal route coverage. Runtime artifacts are required only where a
selector decision, host capability, or execution environment can change the winning provider.

## Success contract

At one reviewed candidate revision:

1. all 24 advertised target identities and all 49 supported feature configurations compile and close
   their `Instant`, `OrderedInstant`, and `ThreadCpuInstant` routes;
2. all 15 distinct runtime decision boundaries below have retained, source-sealed evidence;
3. the measured shipping-code closure is unchanged from the frozen implementation; and
4. validators, charts, README, and BENCHMARKS agree on what is universal proof and what is host
   measurement.

`--no-default-features` is a compatibility and correctness configuration. The performance promise is
for tach's default configuration, so no-default speed duplicates are not release gates.

## Milestones

| ID | Milestone | Status | Description | Context |
|---|---|---|---|---|
| `OBJ-PROVE-TIMERS.M0` | Universal target contract | 🚧 | Prove every advertised target and feature route at frozen commit `d0fa731` | inline · G1⚪ |
| `OBJ-PROVE-TIMERS.M1` | Runtime decision boundaries | 🟣 | Retain the exact 15 selector, host, and runtime boundaries listed below | inline · G1⚪ · 12/15 required retained |
| `OBJ-PROVE-TIMERS.M2` | Release-claim closure | ⚪ | Bind shipping code, validators, charts, and public wording to the accepted evidence | inline · G1⚪ |

---

## `OBJ-PROVE-TIMERS.M0` — Universal target contract

**Description.** Freeze the shipping implementation at `d0fa731` and prove the public APIs and
optimized routes for all advertised targets. The verifier must cover 24 target identities, 49
feature configurations, 98 warning-strict API checks, 294 optimized provider routes, 294
`now`/elapsed closures, and 18 vDSO resolver routes. Runtime-self-selecting targets must enumerate
all eligible providers and close the selected public/direct hot route; unique-provider and fallback
targets are proved by source plus code generation. No-default configurations receive the same build
and semantic coverage but do not create separate performance cells.

### Gate `OBJ-PROVE-TIMERS.M0.G1` — all advertised target routes close at the frozen implementation

Pass only when `benches/verify-target-providers.py` succeeds at literal commit `d0fa731`, its report
contains exactly the counts above, optimized code generation contains no missing or unsupported
route, and selector tests cover candidate enumeration, complete-path measurement, and public/direct
route closure. The retained report must hash to
`e2233386f3d7c64fff705f75ad089ee0dd8717809ed5eadca1b85b1547a579ba`.
- **Fallback.** Fix the missing target, feature configuration, or codegen closure and rerun the full
  static verifier before collecting more runtime evidence.

---

## `OBJ-PROVE-TIMERS.M1` — Runtime decision boundaries

**Description.** Runtime proof samples mechanisms, not the Cartesian product of targets, libc
variants, and feature modes. A boundary is required when the default provider is selected by
measurement, when host availability changes the route, or when a runtime supplies a distinct clock
implementation. Fixed OS primitives need one representative native runtime plus M0's target proof.
Charts may include optional corroborating environments, but optional evidence cannot block release.

### Required runtime set

| # | Decision boundary | Required environment / mode | Frozen artifact | State |
|---:|---|---|---|---|
| 1 | Apple AArch64 native wall and thread clocks | macOS AArch64, default | `speed-0-apple.json` | retained |
| 2 | Apple x86 native wall and thread clocks | macOS x86_64, default | `speed-apple-x86_64.json` | **missing** |
| 3 | Linux x86 runtime tournament | Linux x86_64 GNU, default | `speed-2-inteln.json` | retained |
| 4 | Linux AArch64 runtime tournament | Linux AArch64 GNU, default | `speed-1-c7g.json` | retained |
| 5 | Windows native wall and thread clocks | Windows x86_64 MSVC, default | `speed-4-windows.json` | **missing** |
| 6 | FreeBSD native wall and thread clocks | FreeBSD x86_64, default | `speed-freebsd-x86_64.json` | **missing** |
| 7 | JavaScript host clock | `wasm32-unknown-unknown` on Node, default | `speed-supplemental-wasm-node.json` | retained |
| 8 | Browser fallback without native thread CPU clock | browser, default negative environment | `speed-supplemental-browser-negative.json` | retained |
| 9 | Emscripten host clock | Emscripten on Node, default | `speed-supplemental-emscripten-node.json` | retained |
| 10 | Emscripten pthread clock path | Emscripten pthreads | `speed-supplemental-emscripten-pthreads.json` | retained |
| 11 | WASI Preview 1 positive host clock | WASI p1 on Node, default | `speed-supplemental-wasi-p1-node.json` | retained |
| 12 | WASI Preview 1 fallback boundary | WASI p1 on Wasmtime, default | `speed-supplemental-wasi-p1-wasmtime.json` | retained |
| 13 | WASI Preview 2 fallback boundary | WASI p2 on Wasmtime, default | `speed-supplemental-wasi-p2-wasmtime.json` | retained |
| 14 | WASI threads route availability | `wasip1-threads`, default smoke | `speed-supplemental-wasip1-threads-smoke.json` | retained |
| 15 | Minimal Wasm route availability | `wasm32v1-none`, default smoke | `speed-supplemental-wasm32v1-none-smoke.json` | retained |

The retained Linux x86_64 musl default artifact is optional corroboration of the same Linux x86
selector boundary. Lambda and no-default artifacts are diagnostic only: c7g already proves a
perf-mmap win, c7i proves a raw-syscall win, and the selector makes that decision from measured
complete-path cost rather than instance labels or capability bits.

### Gate `OBJ-PROVE-TIMERS.M1.G1` — every distinct runtime decision is observed

Pass only when all 15 rows are present at the frozen shipping-code closure; every measured artifact
is source-sealed, replay-bound, and passes timer semantics; every runtime tournament reports the
eligible candidates, selected provider, public route, selected-exact route, and native comparison;
and no required selector, fallback, or runtime-availability branch is absent. Public and
selected-exact measurements must be materially tied or the discrepancy must be resolved in code.
- **Fallback.** Recollect only the missing or failing boundary. Add a new row only when a provider
  decision or host availability boundary is demonstrably distinct from all 15 listed rows.

---

## `OBJ-PROVE-TIMERS.M2` — Release-claim closure

**Description.** Replace the 55-cell Cartesian validator with the exact decision-boundary manifest,
then generate release output from one immutable snapshot. Evidence-only or documentation commits may
follow `d0fa731` without recollection only when a deterministic digest proves that the shipping Cargo
configuration and `src/` closure are byte-identical to the measured implementation.

### Gate `OBJ-PROVE-TIMERS.M2.G1` — release validators and publication surfaces agree

Pass only when the release validator requires exactly the 15 M1 rows; the M0 report and every M1
artifact are duplicate-free, source-sealed, replay-bound, and bound to the same shipping-code closure;
the complete Rust, Python, formatting, lint, and target checks pass; checked-in PNG/SVG charts
regenerate byte-clean from accepted measured environments; and README/BENCHMARKS explicitly separate
universal target guarantees from named-host measurements. Smoke and fallback records may prove route
availability but may not be rendered as speed wins.
- **Fallback.** Preserve the failing evidence, repair the validator, route, chart, or claim, and rerun
  only the affected runtime boundary plus the complete release gate.

## Working Log

### 2026-07-12 · codex · `OBJ-PROVE-TIMERS.M0`
- Did: bound future primary proof to a retained full release matrix rather than the legacy 89b42f1 campaign.
- Found: the old six-cell JSON lacks the current source-seal, collector, route-commit, and supplemental provenance required for a release claim.
- Next: collect fresh primary cells only after the source and route contract freeze.
- Board: M0 remains ⚪ with G1⚪; no legacy evidence was promoted.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Opened canonical runtime collection after closing provider and target-route proof at source revision 463faa04cde78f4eef35129df866cfb76e7e785b.
- Found: The exact runtime classification declares 23 artifact identities and 32 open artifact-binding gaps; final release admission remains red until source-consistent primary and supplemental artifacts are retained.
- Next: Collect the six canonical primary cells at 463faa0, beginning with local Apple and serial AWS Graviton/Intel cells while the hosted Windows producer runs from the same revision.
- Board: OBJ-PROVE-TIMERS.M0 is active on one-revision canonical collection at 463faa0.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Froze the complete runtime artifact contract at cd598b9: all 55 runtime identities now have exact source-sealed producer and artifact bindings, with the 192-test evidence suite green in a detached worktree.
- Found: Artifact readiness is complete but runtime proof is not: the cd598b9 campaign still has 0/6 canonical and 0/49 supplemental artifacts, so no OBJ-PROVE-TIMERS gate closes.
- Next: Collect the locally runnable cd598b9 host-runtime, runtime-smoke, and Apple cells, then run serial AWS and hosted producers without mixing source revisions.
- Board: OBJ-PROVE-TIMERS.M0 remains active at frozen revision cd598b9; 55/55 artifact contracts are ready and runtime collection is next.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Corrected route-observation admission at a152e0a so source-bound tagged wall fallbacks replay through the same retained-bundle path as full-speed cells; the complete 194-test evidence suite is green.
- Found: The 19-artifact 6b4c1ed local tranche exposed the defect and is diagnostic only: the source revision advanced, so every promotable runtime artifact must be recollected at a152e0a.
- Next: Recollect and replay-compose the 19 locally runnable artifacts at a152e0a, then begin the source-identical canonical AWS and hosted cells.
- Board: OBJ-PROVE-TIMERS.M0 remains active with no gate closed; a152e0a is the corrected campaign revision and local recollection is in progress.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Collected and replay-composed the complete locally runnable tranche at source a152e0a: 19 retained artifacts bind to 19 committed route requirements as 9 full-speed, 6 tagged-wall-fallback, and 4 runtime-smoke observations.
- Found: Local proof now covers 1 of 6 canonical and 18 of 49 supplemental artifacts at one revision; these are campaign work products, not a gate closure, until the remaining 36 runtime identities join the same source-bound snapshot.
- Next: Run the source-sealed Graviton, Intel GNU, Intel musl, Lambda, and FreeBSD AWS producers at a152e0a, then add hosted Windows/macOS-x86 and native rare-architecture cells.
- Board: OBJ-PROVE-TIMERS.M0 remains active at a152e0a with 19/55 runtime artifacts replay-bound; M0.G1, M1.G1, and M2.G1 remain open.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Fixed the EC2 and FreeBSD producer layout at 2ea11ee so every cloud artifact retains an artifact-specific collector-bundle path that can coexist in the single release-evidence directory; 195 evidence tests pass.
- Found: The a152e0a local tranche proved its 19 routes but is diagnostic after the source revision advanced; the bundle collision was caught before any AWS instance launched.
- Next: Recollect and replay-compose the local 19 at 2ea11ee, then run AWS producers whose outputs can now assemble without path collisions.
- Board: OBJ-PROVE-TIMERS.M0 remains active with all gates open; 2ea11ee is the current campaign revision and no cloud spend was wasted on the superseded layout.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Recollected and replay-composed all 19 locally runnable artifacts at final producer-layout revision 2ea11ee; every artifact is source-identical and the manifest binds 9 full-speed, 6 tagged-wall-fallback, and 4 runtime-smoke observations.
- Found: The local tranche now proves 1/6 canonical and 18/49 supplemental identities at 2ea11ee; all 36 remaining artifacts can coexist in the same evidence directory because every retained bundle path is artifact-specific.
- Next: Collect the canonical Graviton, Intel GNU, Intel musl, Lambda, and FreeBSD cells at 2ea11ee, then merge hosted and rare-native evidence.
- Board: OBJ-PROVE-TIMERS.M0 remains active at 2ea11ee with 19/55 runtime artifacts replay-bound and all three objective gates still open.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Committed d0fa731 to unify Linux x86 OrderedInstant now and elapsed on one packed hot-state selector, proved x86_64/i686 optimized route closures, and source-sealed plus byte-for-byte replay-validated the canonical c7i GNU cell.
- Found: At d0fa731 all 92 remote correctness and thread-semantics tests passed; the retained alternating paired probe measured ordered elapsed at about 40.4 ns public versus 39.9 ns selected-exact without a repeatable material loss, and the canonical composer accepted the cell. The previous 21 cells at 2ea11ee remain diagnostic only because the source revision advanced.
- Next: Treat d0fa731 as the new campaign freeze; recollect the other 5 canonical and all 49 supplemental identities at this exact revision, then run unified validators and regenerate the performance charts.
- Board: The c7i OrderedInstant release regression is resolved at d0fa731; M0 remains active on the final one-revision 55-cell campaign (1/55 currently retained at the new freeze).

### 2026-07-14 · codex · `OBJ-PROVE-TIMERS.M0`
- Did: Retained and replay-composed the exact d0fa731 Graviton default and no-default pair; the campaign manifest now binds 22 of 55 required runtime identities at one frozen source revision.
- Found: The default c7g build selected perf-mmap thread CPU at about 58 ns versus about 260 ns for the syscall runner-up, while no-default selected the raw syscall at about 259 ns and matched its selected-exact route; all remote correctness, semantic, and initialization tests passed and both EC2 instances terminated.
- Next: Collect Intel GNU no-default, Intel musl default/no-default, Lambda, and FreeBSD cells at d0fa731, then merge hosted Windows/macOS-x86 and rare-native identities before running the unified release validators.
- Board: OBJ-PROVE-TIMERS.M0 remains active at d0fa731 with 3/6 canonical and 19/49 supplemental identities retained (22/55 total); all three proof gates remain open.

### 2026-07-14 · codex · `OBJ-PROVE-TIMERS.M1`
- Did: Replaced the 55 target-by-mode runtime campaign with a three-layer proof contract: 24-target static closure, 15 runtime decision boundaries, and one publication closure.
- Found: The Cartesian campaign duplicated no-default and host identities even though the provider proof already classifies fixed, fallback-only, codegen-proven, and runtime-self-selecting routes; 12 of the 15 decision boundaries are already retained at d0fa731.
- Next: Update the release validator to enforce this exact manifest and shipping-code closure, then collect only macOS x86_64, Windows x86_64, and FreeBSD x86_64.
- Board: M0 has complete evidence awaiting formal gate admission; M1 is 12/15; M2 begins with the validator rewrite; there is no owner blocker.

### 2026-07-14 · codex · `OBJ-PROVE-TIMERS.M1`
- Did: Committed the decision-boundary proof contract and ADR at c0c0032.
- Found: The release path is now mechanically bounded to 24 static target identities, 15 runtime boundaries with 12 retained, three named missing hosts, and one publication closure gate.
- Next: Update the release validator to enforce the 15-row manifest and shipping-code closure before launching another benchmark host.
- Board: M0 awaits retained-report admission; M1 is next at 12/15; M2 remains unopened; no owner decision is pending.

## /goal

At one reviewed release candidate, prove all 24 advertised target routes statically, all 15 distinct
runtime decision boundaries empirically, and every public performance claim from the same immutable
shipping-code closure. Done = M0, M1, and M2 are 🟢 with committed evidence and byte-reproducible
publication artifacts; publishing remains a separate explicit owner decision.
