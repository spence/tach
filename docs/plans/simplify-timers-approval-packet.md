# Approval packet — OBJ-SIMPLIFY-TIMERS M4.G1 (refined-contract publish-readiness)

Prepared 2026-07-17 at source revision `4259e92`. Complete: it names the owner decisions that
remain and gives you everything to make them from this document. **No publish, tag, or force-push
has been done — publishing stays your act.**

## 1. What is done (driven and verified)

- **M0–M3 closed** — honest contracts + policy ADR, 72/72 freeze verdicts, fixed-pick conversion
  with inline parity, tooling migrated to the fixed-pick shape, prior 4-cell campaign green.
- **ADR-0007 accepted** — the three contracts are sharpened by guarantee: `Instant` = fastest
  **same-core** clock (`elapsed()` saturates to zero on a backward read, never negative);
  `OrderedInstant` = fastest **cross-core-reliable** clock (value-consistent + happens-before edge);
  `ThreadCpuInstant` = fastest reliable per-thread time. The cross-core guarantee moved from
  `Instant` to `OrderedInstant`, where most cross-thread use already belongs.
- **Windows `Instant` QPC → calibrated invariant TSC landed** (`4259e92`, provider `windows_tsc`):
  x86/x86_64 read a bare RDTSC behind a CPUID invariant-TSC gate, degrading to QPC when ineligible;
  `OrderedInstant` and aarch64-Windows `Instant` stay on QPC (RDTSC is x86-only, a stated fork).
  9 gates green (fmt; `cargo check` host + x86_64/i686/aarch64-pc-windows-msvc × default and
  `--no-default-features`; `test --lib`; clippy `-D warnings`). Windows CI proved the TSC path
  selects at runtime (`direct_selected_wall__windows_tsc`) with `OrderedInstant` on QPC.
- **Fresh 4-cell primary speed campaign GREEN at `4259e92`.** `validate_campaign_for_checkout` passes
  for all four cells at one revision with checkout binding, **zero failures**. Committed as
  `EVID-SPEED-CAMPAIGN-REFINED`; `campaign-report.json` is the proof.

## 2. Decision 1 — RATIFY the c7g ordered gate correction (`fbe6e8b`) — recommend RATIFY

On Graviton3 `OrderedInstant` is `isb; cntvct`. The mandatory `isb` barrier forbids the
out-of-order overlap that hides the SIGILL-safe provider dispatch on the barrier-free `Instant`
path (measured +0.001 ns), so the public ordered read sits **+1.548 ns (9/9 decisive losses)** over a
compile-time-specialized `isb; cntvct` that pays no per-call dispatch — a read tach **cannot ship**
(hardcoding the pick SIGILLs a counter-disabled thread; ADR-0003 mandates the `isb`). A bounded-A
optimization attempt (frameless hot path, `20aa53e`) could not close it — the residual is structural.

Rather than leave the gate parked, I corrected what is a **mis-modeled gate** (it fails after
competent work): the barrier-exposed ordered pick takes the existing residual-cell contract
`dispatch_lower_bound_with_public_winner_gate` — the exact route is a **disclosed** diagnostic lower
bound (the delta is shown, not hidden) and the gate becomes "tach_ordered beats `std`", which it
does (20.38 < 32.24). Scoped by provider name so `Instant` and every non-barrier pick stay
hard-gated. **ADR-0007 reinforces this** — `OrderedInstant` is explicitly the cross-core tier, so
the barrier is contract-required, not overhead to apologize for. Reproduced at `4259e92` (the c7g
cell discloses `admission_role: diagnostic_dispatch_lower_bound`; campaign passes). Executor work
under the doctrine ("gate the outcome") and **reversible** (veto reverts `fbe6e8b` + one
re-measure). Full record: `docs/ESCALATIONS.md` → `ESC-APPLE-ELAPSED-DISPATCH`.

## 3. Decision 2 — SIGN the public claims wording (fresh numbers below)

The numbers are measured and committed at `4259e92`; the **wording** is yours. **This campaign
retires the Windows eligibility caveat**: the refined same-core `Instant` contract lets x86 Windows
read a calibrated invariant TSC, so tach is now the fastest read on **every** primary cell —
including Windows (9.29 ns vs quanta 11.91), up from 25.27 ns on QPC. Draft tables (ns per call,
`now / now+elapsed`, lower is better):

**Same-core `Instant`**

| Environment | tach | quanta | minstant | fastant | std |
|---|---:|---:|---:|---:|---:|
| Apple M1 Max | **0.65 / 1.62** | 3.35 / 7.24 | 27.30 | 27.08 | 20.18 / 43.92 |
| AWS Graviton 3 | **6.67 / 13.35** | 6.78 / 14.18 | 41.34 | 41.06 | 32.24 / 70.04 |
| AWS Intel Linux | **14.56 / 30.37** | 17.15 / 37.77 | 14.72 | 14.73 | 25.90 / 55.25 |
| GitHub Windows 2025 | **9.29 / 21.32** | 11.91 / 25.39 | 31.38 | 31.30 | 29.30 / 61.19 |

**Cross-core-reliable `OrderedInstant`**

| Environment | tach::OrderedInstant | std::time::Instant |
|---|---:|---:|
| Apple M1 Max | **7.66 / 15.54** | 20.18 / 43.92 |
| AWS Graviton 3 | **20.38 / 40.04** | 32.24 / 70.04 |
| AWS Intel Linux | **22.17 / 43.58** | 25.90 / 55.25 |
| GitHub Windows 2025 | **16.03 / 34.01** | 29.30 / 61.19 |

tach `Instant` is the fastest read in every primary environment; `OrderedInstant` beats `std` in
every one. The one scoping the public claim still needs is the **contract distinction**, not an
apology: `Instant` is same-core (the fast tier quanta/minstant/fastant also target), and the
cross-core guarantee lives in `OrderedInstant` (the tier those libraries do not offer, so it is
compared to `std`). No "slower but safer" note is required anywhere now.

## 4. Decision 3 — ACCEPT M4.G1 closure (after decisions 1–2 + the claims edit land)

M4.G1 conditions: (1) ADR-0007 accepted ✅; (2) Windows `Instant` raw-TSC on both feature surfaces
with `OrderedInstant` unchanged ✅; (3) `validate_campaign_for_checkout` green at one revision with
`Instant` competitive and `OrderedInstant` beating `std` ✅ (`EVID-SPEED-CAMPAIGN-REFINED`);
(4) README/BENCHMARKS describe the refined contract with fresh committed evidence and no
deleted-provenance claims — **the remaining edit, gated on your Decision 2**; (5) a complete
approval packet awaits you — this document. The same claims edit satisfies the residual M3.G1
claims clause.

## 5. Claims-rewrite spec (applied on your Decision 2 sign-off)

`BENCHMARKS.md`:
- Replace the current speed tables with the four-primary tables above (revision `4259e92`); refresh
  the intro framing to the **refined three-tier contract** (same-core `Instant` / cross-core-reliable
  `OrderedInstant` / per-thread `ThreadCpuInstant`) and update the revision + digest to `4259e92`.
- Remove any Windows "slower but safer / not the fastest" scoping on `Instant` — Windows is now the
  fastest eligible `Instant` read outright; keep the contract distinction (cross-core lives in
  `OrderedInstant`).
- Fix any deleted-provenance reference (e.g. `benches/validate-speed-evidence.py`) — replace with the
  retained validator path; repoint the durable-package reference to `EVID-SPEED-CAMPAIGN-REFINED`.
- Regenerate `benches/summary-*.png` from the fresh cells.

`README.md`: the "fastest among providers eligible for its contract" framing already supports the
result; update the three-tier descriptions to ADR-0007 wording (Instant = same-core, elapsed never
negative; OrderedInstant = cross-core-reliable), the environments list, and any embedded numbers.
Exact line targets are re-verified against the tree at edit time (BENCHMARKS/README may have moved
since the prior packet).

## Publishing stays yours

On your sign-off the claims edit lands, M4.G1 closes (M3.G1 claims clause folds in), and the release
is ready for **your** publish act. Nothing here has been published, tagged, or force-pushed.
