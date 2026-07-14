# `OBJ-PROVE-TIMERS` — Prove the fastest eligible timer route

**VISION slice.** On every advertised target, each default timer reaches the fastest eligible
reliable provider supported by the target's selection policy. Route availability and semantic
correctness are universal target claims; speed is a named-environment measurement unless a platform
contract itself determines the winner.

This objective proves one frozen shipping implementation without multiplying targets by build modes.
Static target proof establishes that every API reaches its intended candidate routes; it does not, by
itself, prove a speed ranking. Runtime selection is required where the same compiled target has shown
a provider-profitability reversal or the platform contract explicitly makes the winning cost
runtime-variable. An availability-preferred policy may remain deterministic when a bounded
same-binary survey finds no reversal and every observed win is material; that is measured evidence,
not a universal speed theorem.

## What is known versus what is retained

Route coverage is closed for the current implementation: M0.G1 proves that all 24 advertised target
identities and every supported feature configuration reach an eligible implementation for all three
public timers. M0.G2 separately requires an exhaustive provider-policy matrix and an evidence-backed
reason for each production policy. Linux AArch64 `ThreadCpuInstant` is availability-preferred by
design: the same frozen binary found perf-mmap materially faster than POSIX on c6g, c7g, c8g, and
t4g, and found no same-target profitability reversal that would justify a production tournament.
Benchmark builds retain the audit, and any observed loss invalidates this policy.

M1 is release corroboration and bookkeeping. Its artifact count does **not** mean that only that
fraction of the architecture matrix is understood. The retained set observes runtime-variable
selection and fallback boundaries plus representative native fixed routes; it is not a second
target-by-target provider-discovery exercise.

After M0.G2 closes, the remaining work is artifact admission, publication-surface consistency, and
release verification—not further architecture/provider discovery unless a retained run falsifies a
matrix entry.

## Success contract

At one reviewed candidate revision:

1. all 24 advertised target identities and all 49 supported feature configurations compile and close
   their `Instant`, `OrderedInstant`, and `ThreadCpuInstant` candidate routes;
2. a tracked matrix accounts for every eligible provider and records whether each production route
   is fixed by contract, selected by complete-path measurement, or availability-preferred under a
   retained same-binary no-reversal survey;
3. all 15 distinct runtime decision boundaries below have retained, source-sealed evidence;
4. the measured shipping-code closure is unchanged from the frozen implementation; and
5. validators, charts, README, and BENCHMARKS agree on what is universal proof and what is host
   measurement.

`--no-default-features` is a compatibility and correctness configuration. The performance promise is
for tach's default configuration, so no-default speed duplicates are not release gates.

## Milestones

| ID | Milestone | Status | Description | Context |
|---|---|---|---|---|
| `OBJ-PROVE-TIMERS.M0` | Provider matrix and selection policy | ✅ | G1 closes candidate routes; G2 proves the production policy selects the fastest eligible complete path | inline · G1🟢 · G2🟢 |
| `OBJ-PROVE-TIMERS.M1` | Retained runtime corroboration | 🚧 | Retain one source-sealed artifact for each of 15 runtime-variable or representative native boundaries | inline · G1⚪ |
| `OBJ-PROVE-TIMERS.M2` | Release-claim closure | ⚪ | Bind shipping code, validators, charts, and public wording to the accepted evidence | inline · G1⚪ |

---

## `OBJ-PROVE-TIMERS.M0` — Provider matrix and selection policy

**Description.** First prove the shipping implementation at `d9da626` across the public APIs and
optimized candidate routes for all advertised targets. Then account for every eligible provider and
prove that the production policy chooses among complete public paths at the correct boundary. The
route verifier must cover 24 target identities, 49
feature configurations, 98 warning-strict API checks, 294 optimized provider routes, 294
`now`/elapsed closures, and 18 vDSO resolver routes. Runtime-self-selecting targets must enumerate
all eligible providers and close the selected public/direct hot route; unique-provider and fallback
targets are proved by source plus code generation. No-default configurations receive the same build
and semantic coverage but do not create separate performance cells.

### Gate `OBJ-PROVE-TIMERS.M0.G1` — all advertised target routes close at the frozen implementation

Pass only when `benches/verify-target-providers.py` succeeds at literal commit `d9da626`, its report
contains exactly the counts above, optimized code generation contains no missing or unsupported
route, and selector tests cover candidate enumeration, complete-path measurement, and public/direct
route closure. The retained report must hash to
`61e68ba234c3b454926a468989d93d5b2ea9f5b093d6b3b7874d7fe3d2efb8a6`.
- **Fallback.** Fix the missing target, feature configuration, or codegen closure and rerun the full
  static verifier before collecting more runtime evidence.

### Gate `OBJ-PROVE-TIMERS.M0.G2` — every production selection policy has the required evidence

Pass only when a tracked 24-target-by-three-timer matrix names every eligible OS/architecture
provider considered, its semantic eligibility rule, and its production selection policy; official
OS or architecture sources and optimized code generation account for the candidate set; a fixed
route has exactly one eligible candidate or a documented invariant that determines the winner; and a
runtime-measured route proves that initialization measures the same complete public/direct paths
later installed as hot routes. An availability-preferred route additionally requires one frozen
binary across representative generations or environments, a material win in every observation, no
observed profitability reversal, a retained benchmark-only audit, and public wording that scopes the
speed result to those environments. A same-target reversal immediately requires complete-path
runtime selection.
- **Fallback.** Add the missing candidate, correct the fixed policy, or replace a falsified
  availability-preferred policy with a complete-path runtime selector. Rerun only the affected native
  boundary plus the full static route verifier; escalate only if no reliable eligible policy exists.

---

## `OBJ-PROVE-TIMERS.M1` — Retained runtime corroboration

**Description.** Runtime corroboration samples mechanisms, not the Cartesian product of targets, libc
variants, and feature modes. A boundary is required when the default provider is selected by
measurement, when host availability changes the route, or when a runtime supplies a distinct clock
implementation. Fixed OS primitives need one representative native runtime plus M0's target proof.
Charts may include optional corroborating environments, but optional evidence cannot block release.

### Required runtime set

| # | Decision boundary | Required environment / mode | Frozen artifact | State |
|---:|---|---|---|---|
| 1 | Apple AArch64 native wall and thread clocks | macOS AArch64, default | `speed-0-apple.json` | retained at current closure |
| 2 | Apple x86 native wall and thread clocks | macOS x86_64, default | `speed-supplemental-macos-x86_64.json` | retained at current closure; run `29359437933` |
| 3 | Linux x86 runtime tournament | Linux x86_64 GNU, default | `speed-2-inteln.json` | older result retained; recollect after Windows route freeze |
| 4 | Linux AArch64 availability policy and profitability audit | Linux AArch64 GNU, default | `speed-1-c7g.json` | older result retained; recollect after Windows route freeze |
| 5 | Windows native wall and thread clocks | Windows x86_64 MSVC, default | `speed-4-windows.json` | **failing performance admission in run `29359437933`** |
| 6 | FreeBSD native wall and thread clocks | FreeBSD x86_64, default | `speed-supplemental-freebsd-x86_64.json` | older result retained; recollect after Windows route freeze |
| 7 | JavaScript host clock | `wasm32-unknown-unknown` on Node, default | `speed-supplemental-wasm-node.json` | retained at current closure |
| 8 | Browser fallback without native thread CPU clock | browser, default negative environment | `speed-supplemental-browser-negative.json` | retained at current closure |
| 9 | Emscripten host clock | Emscripten on Node, default | `speed-supplemental-emscripten-node.json` | retained at current closure |
| 10 | Emscripten pthread clock path | Emscripten pthreads | `speed-supplemental-emscripten-pthreads.json` | retained at current closure |
| 11 | WASI Preview 1 positive host clock | WASI p1 on Node, default | `speed-supplemental-wasi-p1-node.json` | retained at current closure |
| 12 | WASI Preview 1 fallback boundary | WASI p1 on Wasmtime, default | `speed-supplemental-wasi-p1-wasmtime.json` | retained at current closure |
| 13 | WASI Preview 2 fallback boundary | WASI p2 on Wasmtime, default | `speed-supplemental-wasi-p2-wasmtime.json` | retained at current closure |
| 14 | WASI threads route availability | `wasip1-threads`, default smoke | `speed-supplemental-wasip1-threads-smoke.json` | retained at current closure |
| 15 | Minimal Wasm route availability | `wasm32v1-none`, default smoke | `speed-supplemental-wasm32v1-none-smoke.json` | retained at current closure |

The retained Linux x86_64 musl default artifact is optional corroboration of the same Linux x86
selector boundary. Lambda and no-default artifacts are diagnostic only. Linux x86 has an observed
same-target capability/profitability reversal and therefore selects from measured complete paths;
Linux AArch64 has a retained four-family no-reversal survey and keeps its simpler audited
availability policy.

The current shipping-code closure has 11/15 rows: rows 1-2 and 7-15. The older Linux and FreeBSD
results remain useful corroboration but are not admissible for M1.G1 until recollected after the
Windows route is frozen. Any shipping-code change made to close the Windows performance failure
creates a new closure and requires the affected evidence set to be regenerated.

The local x86_64 macOS bundle identifies its runner as Rosetta. It is compatibility evidence, not
native Intel speed evidence, and does not satisfy row 2.

### Gate `OBJ-PROVE-TIMERS.M1.G1` — every runtime-variable or representative native boundary is retained

Pass only when all 15 rows are present at the frozen shipping-code closure; every measured artifact
is source-sealed, replay-bound, and passes timer semantics; every runtime tournament reports the
eligible candidates, selected provider, public route, selected-exact route, and native comparison;
and no required selector, fallback, or runtime-availability branch is absent. Public and
selected-exact measurements must be materially tied when the exact route is caller-selectable. For
a runtime-dispatched public API, a private statically bound route may instead remain an explicit
diagnostic lower bound when the selector reproduces and the public API beats every caller-usable
reference for its contract.
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

### 2026-07-14 · codex · `OBJ-PROVE-TIMERS.M0`
- Did: Admitted the frozen d0fa731 universal provider proof: 24/24 targets, 49/49 feature configurations, 294/294 optimized clock routes, 294/294 now-plus-elapsed closures, and 18/18 vDSO resolver routes.; OBJ-PROVE-TIMERS.M0.G1 🟢 at SHA `d0fa731`.
- Board: OBJ-PROVE-TIMERS.M0 G1 🟢 — evidence docs/evidence/timers/provider-proof-d0fa731/target-provider-proof.txt.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Re-proved the universal target contract at ceb69c1 after adding Intel macOS invariant-TSC selection: 24/24 targets, 294/294 optimized clock routes, 294/294 elapsed closures, and 18/18 vDSO routes.
- Found: Rosetta x86_64 now selects apple_invariant_rdtsc at about 10.49 ns versus quanta at about 15.82 ns, while OrderedInstant remains on the XNU Mach path.
- Next: Retain the source-sealed macOS x86_64 runtime artifact at ceb69c1, then resolve cross-revision admission for unaffected runtime boundaries without recollecting unchanged target code.
- Board: M0 remains green at superseding shipping commit ceb69c1; M1 remains 12/15 until the passing macOS x86_64 artifact is retained.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M1`
- Did: Separated provider knowledge from release artifact retention: M0 already closes all 24 advertised target routes, while M1's 12/15 count describes retained corroboration artifacts only.
- Found: The prior milestone name made three absent host artifacts sound like three unresolved clock decisions; macOS x86_64 is already decided and measured, while Windows and FreeBSD need source-sealed native corroboration rather than architecture discovery.
- Next: Retain only the three named host artifacts, admit unchanged prior evidence by target-scoped shipping-code equivalence, then close publication consistency.
- Board: Provider/route knowledge is 24/24 complete; runtime release evidence is 12/15 retained; no architecture-wide clock search remains.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Separated completed route closure from the still-open fastest-selection policy and added an explicit profitability gate.
- Found: Linux AArch64 ThreadCpuInstant audits perf-mmap versus POSIX only in benchmark builds; production selects perf-mmap by capability, so one c7g win cannot establish fastest selection on every AArch64 host.
- Next: Complete the provider-policy matrix, then either prove the AArch64 capability rule determines profitability or make production select from measured complete paths.
- Board: M0.G1 remains green for 24-target route closure; M0 is reopened on new G2 for fastest-provider selection policy. M1 is corroboration only and cannot substitute for G2.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M0`
- Did: Bound fastest-provider evidence to the actual decision boundary: universal target routing remains separate from named-environment speed, and an availability-preferred policy now requires a retained same-binary material no-reversal survey plus an audit path.
- Found: Linux AArch64 ThreadCpuInstant has that evidence across c6g, c7g, c8g, and t4g; no same-target flip justifies restoring a production profitability tournament, while any future audit loss now invalidates the deterministic policy.
- Next: Finish native FreeBSD public-versus-installed-route validation at d9da626, then admit M0.G2 if the complete matrix has no remaining selection or hot-path defect.
- Board: M0.G1 remains green at 24/24 targets and 294/294 optimized routes at d9da626; M0.G2 waits only on the active FreeBSD native rerun.

### 2026-07-14 · codex · `OBJ-PROVE-TIMERS.M0`
- Did: Completed native FreeBSD closure at source-sealed revision 8968b16: the selector reproduces, all usable public-reference gates pass, ThreadCpuInstant matches the selected raw syscall, and thread-CPU semantics pass.
- Found: The remaining ordered public/exact gap is dispatch cost against a private static lower bound, not a caller-usable competing clock; the validator retains it diagnostically and still fails any public-reference loss.
- Next: Commit the tracked provider-policy evidence package, then admit M0.G2 against that evidence commit.
- Board: M0.G1 remains green; M0.G2 has passing 72-cell policy evidence ready for admission; M1 is 13/15 with only native Intel macOS and Windows x86_64 missing.

### 2026-07-14 · codex · `OBJ-PROVE-TIMERS.M0`
- Did: Admitted all 72 target/timer production policies: 53 runtime-measured, 9 fixed-contract, 8 availability-fallback, 1 fallback-only, and 1 availability-preferred with retained audit; native FreeBSD selector, public-reference, thread-CPU, and semantics evidence passes at 8968b16.; OBJ-PROVE-TIMERS.M0.G2 🟢 at evidence SHA `9a3c48f`.
- Board: OBJ-PROVE-TIMERS.M0 G2 🟢 — evidence docs/evidence/timers/provider-policy-closure-2026-07-14/README.md.

### 2026-07-14 · codex · `OBJ-PROVE-TIMERS.M1`
- Did: Closed M0.G2 at evidence SHA 9a3c48f and retained the passing FreeBSD boundary, bringing runtime corroboration to 13/15.
- Found: The local macOS x86_64 bundle is Rosetta rather than native Intel, and the older Windows JSON has no retained collector bundle; neither can satisfy the release gate.
- Next: After ESC-HOSTED-EVIDENCE-PUSH is approved, push the frozen branch, dispatch boundary=release-missing, and retain only the native macOS Intel and Windows x86_64 artifacts.
- Blocked/unsure: ESC-HOSTED-EVIDENCE-PUSH
- Board: M0 is complete; M1 is active at 13/15 and blocked only on approval for the two-job hosted evidence dispatch; M2 can continue with publication-surface audit.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M2`
- Did: Fixed and committed the no-default benchmark attestation compile failure at f8b3628; default and no-default tests plus warning-denied all-target clippy now pass.
- Found: The first pulled-forward release check found an empty cfg-gated feature array with no inferable element type under --no-default-features; the defect was confined to benches/instant.rs and did not change shipping provider code.
- Next: Run the two missing hosted native boundaries from the corrected evidence-harness revision, then bind charts and public claims to their retained artifacts.
- Blocked/unsure: ESC-HOSTED-EVIDENCE-PUSH
- Board: M0 shipping-code proof remains closed; M1 stays 13/15; M2 has an early release-check defect resolved at f8b3628.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M1`
- Did: Advanced the hosted evidence freeze to f8b3628 after verifying that the harness-only fix leaves shipping providers unchanged and passes both feature surfaces.
- Found: The two missing native runs must bind to f8b3628 so their runtime attestations are produced by the corrected no-default-capable harness; M0 remains closed at the unchanged shipping implementation.
- Next: After ESC-HOSTED-EVIDENCE-PUSH is approved, push f8b3628 and dispatch boundary=release-missing for native macOS Intel and Windows x86_64.
- Blocked/unsure: ESC-HOSTED-EVIDENCE-PUSH
- Board: M1 remains active at 13/15 and has one bounded owner-only dispatch blocker; M2 retains the completed early release-check result.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M1`
- Did: Passed the complete 207-test evidence suite (one expected skip) and defined hosted dispatch against the current branch head containing the verified f8b3628 harness tree.
- Found: A later documentation-only commit necessarily advances the repository SHA; runtime admission must bind the dispatched revision and prove shipping-code plus harness-tree equivalence rather than chase an ever-changing documentation HEAD.
- Next: After ESC-HOSTED-EVIDENCE-PUSH is approved, push the current branch and dispatch boundary=release-missing for native macOS Intel and Windows x86_64.
- Blocked/unsure: ESC-HOSTED-EVIDENCE-PUSH
- Board: All available local Rust and evidence checks pass; M1 remains 13/15 with only the two hosted native artifacts outstanding.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M1`
- Did: Pushed bench/six-clock-speed at eb1ff16 and dispatched release-missing run 29356740765; only native macOS Intel and Windows x86_64 jobs are active.
- Found: The existing remote branch contained one historical Windows evidence commit whose resulting file was byte-identical to the local tree; a no-content merge preserved it without force-pushing. All unrelated matrix jobs were skipped.
- Next: When run 29356740765 completes, retain and validate its two source-sealed artifacts, then close M1 at 15/15 if both pass.
- Board: The owner escalation is accepted and cleared; M1 remains active at 13/15 while the two final hosted native jobs run.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M1`
- Did: Diagnosed hosted run 29356740765: Windows benchmark completed but source sealing rejected equivalent path/descriptor metadata; macOS Intel completed but public wall elapsed failed exact-route parity and the paired thread-CPU probe showed temporal bias despite identical optimized codegen.
- Found: M1 remains 13/15 and is not owner-blocked. Windows needs a no-reparse descriptor snapshot with Windows-compatible metadata comparison; macOS x86 elapsed paid redundant scale loads, while the 65,536-read all-at-once paired probe was too coarse to distinguish code overhead from time-varying host cost.
- Next: Verify the Windows seal repair and macOS x86 provider-owned elapsed path locally, push the focused fixes, then redispatch boundary=release-missing.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M1`
- Did: Verified the Windows source-seal repair and macOS Intel fused elapsed path: Rust release tests passed with all/default and no-default features, clippy is warning-clean, 210 Python evidence tests passed, and provider proof passed 24/24 targets with 294/294 public routes and phase closures.
- Found: The macOS ThreadCpuInstant public probe compiles to the same single clock_gettime_nsec_np call as the native reference; short-chunk interleaving is required to prevent temporal host drift from masquerading as public API overhead.
- Next: Push the four focused fix commits and redispatch boundary=release-missing for the two remaining native artifacts.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M1`
- Did: Pushed the verified boundary repairs through 91f7ca0 and dispatched release-missing run 29358355745; the macOS Intel and Windows x86_64 jobs are in progress.
- Found: No owner decision is open; M1 remains 13/15 until both source-sealed hosted artifacts pass and are retained.
- Next: Retain and validate the two artifacts from run 29358355745, then close M1 at 15/15 or fix any concrete boundary failure.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M1`
- Did: Completed hosted run 29358355745: both native benchmarks ran successfully, but neither evidence artifact was retained because composition failed.
- Found: macOS Intel hit four parity-validator failures after successful measurement; Windows hit a source-seal metadata mismatch while reopening tach-speed-source-seal.json. M1 remains 13/15 and is not owner-blocked.
- Next: Repair the two evidence-harness defects, verify them locally, then rerun only macOS Intel and Windows x86_64 and retain the passing artifacts.
- Board: M0 is complete; M1 is active at 13/15 with two concrete harness defects and no owner decision pending; M2 has not started.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M1`
- Did: Fixed both hosted composition defects at 85e19ed, passed all 212 Python tests with one expected skip, pushed through a35bbf1, and dispatched release-missing run 29359437933.
- Found: The Windows failure was path-stat versus handle-stat representation drift in the collector; the macOS failures came from validator layers bypassing the retained paired public/exact proof. Neither was a provider-selection regression.
- Next: Let run 29359437933 complete, retain its macOS Intel and Windows x86_64 artifacts if both validate, and close M1 at 15/15.
- Board: M0 is complete; M1 is active at 13/15 with the focused repair rerun in progress and no owner decision pending; M2 remains not started.

### 2026-07-14 · codex · `OBJ-PROVE-TIMERS.M1`
- Did: Completed hosted run 29359437933, retained the passing native Intel macOS artifact, and reproduced it byte-for-byte from its collector bundle (SHA-256 759d09a78a20705342f86d534fcc564d726dd74a915b925a2e161ec5d85120d4).
- Found: The infrastructure and source-seal defects are closed. Windows now reaches the substantive gate and fails it: OrderedInstant now/elapsed are materially slower than std, and ThreadCpuInstant is materially slower than its direct GetThreadTimes route and native reference. The current shipping-code closure has 11/15 admissible rows; older Linux x86_64, Linux AArch64, and FreeBSD results require recollection after the Windows route freezes.
- Next: Resolve the Windows OrderedInstant eligibility/ordering contract and remove or disprove ThreadCpuInstant public-path overhead; freeze shipping code, rerun Windows, then recollect only the closure-invalidated native rows and execute M2 release validation.
- Board: M0 is complete. M1 is active at 11/15 on the current closure with one concrete Windows performance defect and three deferred native recollections; no owner decision or external resource blocks progress. M2 remains not started.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M1`
- Did: Replaced the Windows raw-counter ordering machinery with independently selected Windows OS-call paths at 5a2eb05; completed exhaustive static provider verification across 24/24 targets; fixed Windows benchmark compilation and the collector's Windows platform-call identity classification; pushed 68dc201 and dispatched retained native rerun 29369713783.
- Found: Run 29369043533 compiled, tested, and completed the Windows benchmark; composition failed only because the extractor mislabeled Windows platform calls as inline, now covered by a regression test. The objective is actionable, not blocked.
- Next: Admit or diagnose retained Windows run 29369713783, rerun retained macOS x86_64 parity, then freeze the source closure and collect only the remaining release-missing native boundaries.
- Board: M1 remains in progress; no owner decision is required.

### 2026-07-14 · spence · `OBJ-PROVE-TIMERS.M1`
- Did: Admitted the source-sealed Windows x86_64 artifact and repaired supplemental validation so the retained native Intel macOS paired public/exact proof is preserved through full-cell admission; all 12 collected decision-boundary artifacts now validate at shipping revision 68dc201.
- Found: The objective was not blocked: the Intel macOS clock path passed its paired parity probe, but _selector_free_clocks discarded that proof before the generic native comparison. Commit 33cd5d0 fixes the validator without changing Cargo/src shipping code; the release validator now reports only Linux x86_64, Linux AArch64, and FreeBSD x86_64 artifacts missing.
- Next: Collect the three missing native artifacts at 68dc201, compose route-observations-v1.json, run the complete M1/M2 release gate, then align README and BENCHMARKS before requesting explicit publication approval.
- Board: M0 is complete. M1 is active at 12/15 with exactly three native recollections remaining and no owner or external blocker. M2 follows with release validation, charts, and public wording.

### 2026-07-14 · codex · `OBJ-PROVE-TIMERS.M1`
- Did: Collected the final Linux AArch64, Linux x86_64, and native FreeBSD artifacts and composed route-observations-v1; the complete 15-boundary release evidence validator passes with zero failures across source revisions 68dc201 and c64dcb7, whose shipping Cargo/src digest is identical.
- Found: Runtime proof is complete in the sealed scratch snapshot: all 15 required boundaries admit. M1 remains open only until that snapshot is imported into a tracked evidence package and committed, because NSR gates close against durable evidence rather than /tmp output.
- Next: Import the validated snapshot without overwriting unrelated dirty benchmark work, commit the evidence package, close M1.G1, then finish M2 charts, public wording, full checks, and independent review.
- Board: M0 is complete. M1 has 15/15 passing artifacts and no research or external blocker; durable evidence admission is the sole remaining M1 step. M2 is the release-packaging path.

## /goal

At one reviewed release candidate, prove all 24 advertised target routes statically, all 15 distinct
runtime decision boundaries empirically, and every public performance claim from the same immutable
shipping-code closure. Done = M0, M1, and M2 are 🟢 with committed evidence and byte-reproducible
publication artifacts; publishing remains a separate explicit owner decision.
