# Plan — tach → publish-ready (refined three-tier contract, honest frozen evidence, owner-approved crates.io publish)

Prepared 2026-07-18 by a Plan agent, verified against `main` (`1ad5664`) and adopted by the owner.
This is the map from the current state to a published, verified crate. Realized into the nsr
board as a new `OBJ-SIMPLIFY-TIMERS.M5` milestone plus the reactivated `OBJ-RELEASE-0-2`.

## Corrections to the pre-plan picture (verified)

- **Apple aarch64 `GlobalInstant` is NOT an open defect — it is resolved.** `ESC-APPLE-ORDERED-SELECTION`
  ✅ ACCEPTED; **ADR-0006** accepted; `EVID-APPLE-ORDERED` (two-machine happens-before survey);
  `src/arch/apple_aarch64.rs` selects deterministically on the commpage timebase *mode*
  (`NOSPEC_APPLE→acntvct`, `NOSPEC→cntvctss`, `SPEC→isb+cntvct`, else `mach`). Deterministic per
  machine; a model case of justified runtime selection. No work — board reconciliation only.
- **Two release-candidate blockers found:** (1) `Cargo.toml` `include` lists `/benches/three-clock-evidence.json`,
  which does not exist → `cargo package` breaks. (2) The packaged chart `benches/summary-use-cases.png`
  is the stale six-cell `76fd4b1` campaign; its renderer needs the full 15-boundary release snapshot and
  cannot consume the 4-cell `validate_campaign_for_checkout` shape → chart regen needs a tooling decision.
- **Board stale:** M2 🚧 despite `G1🟢`; M3 ⚪ despite §7.1/§7.2 done (`a7d77ac`) and §7.3 folded to M4.

## 1. "Publish-ready" — acceptance criteria (all hold at one frozen revision R1)

1. Every advertised target's selected clock matches its ADR-0005/0007 contract; `verify-target-providers.py`
   (24-target × both surfaces) passes; no in-tree runtime tournament survives without a frozen flip
   (grep-clean incl. `src/arch/apple_x86_64.rs`).
2. `fmt --check`, `clippy --all-targets -D warnings`, `test`, `doc --no-deps` green on default AND
   `--no-default-features`; MSRV 1.95.
3. Selection honesty closed: gap (a) landed; gaps (c)/(d)/(e) each carry a recorded disposition; gap (b)
   confirmed closed + board-reconciled.
4. Public claims honest + frozen: README/BENCHMARKS describe the refined three-tier contract; every claim
   traces to committed evidence bound to R1; zero deleted-provenance refs; zero premature `v0.2.0`-tag or
   stale-SHA links; Windows Instant reflects TSC (scoped to invariant-TSC hosts); charts regenerated + bound to R1.
5. Owner claims-wording sign-off recorded (incl. the four-primary-vs-six table decision).
6. M4 approval-packet accepted (`ESC-SIMPLIFY-M4-APPROVAL` Decisions 1–3) → `M4.G1` 🟢 → objective owner-accepted-closed.
7. Release candidate at R1: `Cargo.toml` include valid; `cargo package --locked` archive correct;
   `cargo publish --dry-run --locked` passes — all at one reviewed SHA.
8. Version chosen by owner (default `0.2.0`, unpublished/available).
9. A complete, reviewed release candidate at R1 awaits ONLY the owner's explicit publish act: `cargo publish
   --dry-run --locked` green, approval packet accepted, version chosen. Publish and tag are NOT performed —
   they stay the owner's act after full review. (The actual publish + fresh-consumer verification is
   `OBJ-RELEASE-0-2.M2.G1`, which is deliberately OUTSIDE this goal's Done.)

## 2. Remaining-work map (objective → milestone → gate). [E]=executor · [O]=owner

### Phase 0 — board reconciliation [E] (prerequisite for all closures; nothing closes over a stale board)
Complete M2 (G1🟢→✅), resolve M3 (done/§7.3-folded-to-M4), record gap (b) resolved. `nsr render` + `nsr check`.

### `OBJ-SIMPLIFY-TIMERS` — NEW milestone `M5` (runtime-selection audit closure)
- **M5.G1 — mac-x86 Instant demote-to-fixed (gap a) [E].** Pass: `apple_x86_64.rs` Instant is a fixed
  `mach_absolute_time` pick (grep for `INSTANT_PROVIDER_TSC|select_instant_provider|instant_probe|tsc_eligibility|read_tsc`
  empty); gates green both surfaces; `bench.rs`/`mod.rs`/`speed_evidence.py` mac-x86 consumers reconciled;
  matrix `W-MAC-X86` updated; `verify-target-providers.py` green. Fallback: revert to residual tournament,
  record class-1 residual.
- **M5.G2 — wall eligibility-gate dispositions (gap c) [E] + [O] for Windows claim scope.** Pass: each of
  the 4 gates carries a recorded disposition; no public speed claim rests on an unexercised path without an
  explicit documented scope; the Windows Instant claim scopes to invariant-TSC hosts with the QPC degrade
  documented. Fallback: survey the fallback env, or demote to a fixed conservative pick.
- **M5.G3 — exotic-arch residual disposition (gap d) [E].** Pass: matrix + README carry the source-proven
  documented residual for armv7/s390x/riscv64/loong64/ppc64 with NO speed claim; `verify-target-providers.py`
  green for those triples. Fallback: QEMU functional smoke (optional, post-release).
- **M5.G4 — flip-probe tooling + gap (b) confirm (gap e + b) [O]/[E].** Pass: `ESC-AMD-FLIP-PROBE-TOOLING`
  dispositioned (ratify the ~15-line branch or revert; `EVID-AMD-FLIP-LINUX-X86` already committed);
  `ESC-APPLE-ORDERED-SELECTION` confirmed closed + board reflects it. Fallback: revert the branch.

### `OBJ-SIMPLIFY-TIMERS.M4` (existing) — refined contract landed, competitive, claims honest, packet ready [O]
Pass (owner decisions at post-M5 revision R1, via `ESC-SIMPLIFY-M4-APPROVAL`): D1 ratify c7g gate correction
`fbe6e8b`; D2 sign claims wording (incl. four-primary-vs-six + Windows scope from M5.G2); D3 accept closure.
Campaign re-frozen + checkout-bound to R1; claims rewrite landed. Then `nsr close-objective` (owner-accepted)
→ unblocks `OBJ-RELEASE-0-2.M2`.

### `OBJ-RELEASE-0-2` — reactivated at R1 (prior M0/M1 🟢 were bound to the REJECTED `76fd4b1`; re-verify at R1)
- **M0.G1 — every public claim traces to frozen proof [E] draft + [O] wording (= D2).** Pass: README/BENCHMARKS/
  docs/metadata/charts agree with R1 + refined evidence; no stale count, legacy SHA, premature tag, deleted-provenance
  ref (`validate-speed-evidence.py`→campaign validator), or untracked chart; Windows = scoped TSC; four-primary (or six)
  tables; charts regenerated + bound to R1. Fallback: correct/remove the stale claim, rerun the audit.
- **M1.G1 — release-candidate checks pass [E].** Pass: fmt/clippy/test both surfaces; `verify-target-providers.py`
  24-target green; campaign green bound to R1; **`Cargo.toml` include fixed** (missing `three-clock-evidence.json`;
  packaged charts/evidence = refreshed R1 artifacts); `cargo package --locked` correct; `cargo publish --dry-run --locked`
  passes — all at one candidate SHA. Fallback: repair + rebuild the packet at the same SHA; never publish an unreviewed SHA.
- **M2.G1 — explicit owner approval + publish [O].** Pass: owner approves the reviewed SHA + version; immutable tag +
  `cargo publish` succeed; a fresh consumer verifies. Fallback: leave unpublished, record the open decision.

## 3. Sequence

```
Phase 0 board reconcile [E] ──────────────────────────────────┐ (prereq for all closures)
Phase 1 M5.G1 gap(a) [E] ┐                                     │
        M5.G2 gap(c) [E] ├─ parallel ─→ revision R1           │
        M5.G3 gap(d) [E] ┤   gap(b) confirm [E]               │
        M5.G4 gap(e) [O] ┘                                     │
Phase 2 re-freeze campaign @ R1 [E] + regenerate charts [E]  (four-primary vs six = owner sub-decision)
Phase 3 claims rewrite @ R1 [E] + M4.G1 owner D1–D3 [O] → OBJ-SIMPLIFY-TIMERS closed → unblocks RELEASE.M2
Phase 4 RELEASE M0.G1 claim audit @ R1 [E/O] → M1.G1 RC build [E]
Phase 5 RELEASE M2.G1 owner approve + publish + consumer verify [O]
```

**Must close before publish (correctness/honesty):** gap (a); public-claims honesty + Windows-TSC scope; M4
sign-off; RELEASE M1 (incl. the two packaging blockers) + M2. **Shippable as documented residuals:** gap (c)
fallback envs, gap (d) exotic arches, gap (e) flip-probe tooling. **gap (b): verify + reconcile only.**

## 4. Per-gap resolution (to close, not list)

- **(a) apple x86 Instant → DEMOTE-TO-FIXED `mach_absolute_time`** (owner-ruled). Drop the TSC branch + its
  bench/validator surface; keep ordered. Erases the unvalidatable path; removes the last in-tree tournament. [E]
- **(b) apple aarch64 ordered → ALREADY RESOLVED; accept + reconcile board** (ADR-0006, EVID-APPLE-ORDERED). [E]
- **(c) wall eligibility gates → ACCEPT-AS-DOCUMENTED (all four).** Every fallback degrades to a more-conservative
  OS clock that makes no speed claim (windows non-invariant-TSC → QPC = prior-proven universal path). Windows is the
  only public-honesty item → scope the Windows Instant claim to invariant-TSC hosts + document the QPC degrade. No new
  instance required. [E] + [O] confirm Windows scope.
- **(d) exotic arches → ACCEPT-AS-SOURCE-PROVEN documented residual.** No speed claim rides on them; 24-target proof
  covers availability + routing. Optional post-release QEMU smoke. [E]
- **(e) AMD flip-probe tooling → OWNER-DECISION (ratify or revert); not publish-critical** (`EVID-AMD-FLIP-LINUX-X86`
  already committed; families frozen-fixed independent of the branch). [O]
- **(f) T-LINUX-X86 thread-CPU → NO ACTION** (fully validated, `EVID-THREAD-CPU-X86`).

## 5. Risks + smallest decisive check

| # | Risk | Decisive check |
|---|---|---|
| R1 | gap (a) src change breaks the checkout-bound campaign binding though `apple_x86_64.rs` is cfg-isolated from the 4 cells | run `validate_campaign_for_checkout` at R1 vs retained bundles → re-bind vs re-measure |
| R2 | **Chart tooling mismatch** — `summary-use-cases.py` needs the 15-boundary release snapshot, not the 4-cell campaign | run it against the campaign dir; decide adapt-renderer vs full-release-snapshot BEFORE Phase 2 |
| R3 | four-primary vs six drives spend + gap-(a) coupling | owner decision — **recommend four-primary** (no Intel-macOS re-measure after gap (a); no FreeBSD re-run; both stay in the support table) |
| R4 | windows-x86 QPC fallback never exercised | **no new instance** — scope the claim to invariant-TSC hosts + document the QPC degrade; owner confirms in D2 |
| R5 | RELEASE M0/M1 prior 🟢 bound to the rejected `76fd4b1` | treat as fresh evidence at R1; re-run M1.G1 at the candidate SHA |
| R6 | packaging blocker: missing `three-clock-evidence.json`; stale six-cell charts | `cargo package --locked` + inspect archive; fix include + ship refreshed R1 artifacts |
| R7 | version choice | owner confirms `0.2.0` (available) vs bump; `cargo publish --dry-run` corroborates |
| R8 | board inconsistency could let a milestone close by assertion | Phase 0 reconcile + `nsr check` before any close |

## Suggested /goal

Bring tach to the verified brink of publication and stop there for the owner's full review. Every advertised
target's selected clock matches its documented contract and committed evidence; every public performance and
correctness claim is reproducible from frozen evidence bound to one release revision; the approval packet is
accepted; and a release candidate is built that passes a publish dry-run. Done when a complete, reviewed
release candidate awaits only the owner's explicit publish act — nothing tagged, nothing published, no gate
weakened, and no claim outrunning its evidence. Publishing stays the owner's act, outside this goal.
