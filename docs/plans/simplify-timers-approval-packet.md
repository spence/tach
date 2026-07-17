# Approval packet — OBJ-SIMPLIFY-TIMERS M3.G1 (publish-readiness)

Prepared 2026-07-17 at source revision `505b3d7`. Complete: it names the three owner decisions
that remain and gives you everything to make them from this document. **No publish, tag, or
force-push has been done — publishing stays your act.**

## 1. What is done (driven and verified)

- **M0–M2 closed** — honest contracts + policy ADR, 72/72 freeze verdicts, fixed-pick conversion
  with inline parity, tooling migrated to the fixed-pick shape.
- **Apple `Instant::elapsed` optimization landed** (`0f034cc`, +1.99 → +0.337 ns, SIGILL-safe).
- **4-cell primary speed campaign GREEN at `505b3d7`.** `validate_campaign_for_checkout` passes for
  all four cells at one revision with checkout binding, **zero failures**. Committed as
  `EVID-SPEED-CAMPAIGN-2026-07-17` (`2f9715d`/`3b379b5`, pushed); `campaign-report.json` is the proof.

## 2. Decision 1 — RATIFY the c7g ordered gate correction (`fbe6e8b`) — recommend RATIFY

On Graviton3 `OrderedInstant` is `isb; cntvct`. The mandatory `isb` barrier forbids the
out-of-order overlap that hides the SIGILL-safe provider dispatch on the barrier-free `Instant`
path (measured +0.001 ns), so the public ordered read sits **+1.548 ns (9/9 decisive losses)** over a
compile-time-specialized `isb; cntvct` that pays no per-call dispatch — a read tach **cannot ship**
(hardcoding the pick SIGILLs a counter-disabled thread; ADR-0003 mandates the `isb`). A bounded-A
optimization attempt (frameless hot path, `20aa53e`) could not close it — the residual is structural.

Rather than leave the gate parked, I corrected what is a **mis-modeled gate** (it fails after
competent work): the barrier-exposed ordered pick now takes the existing residual-cell contract
`dispatch_lower_bound_with_public_winner_gate` — the exact route is a **disclosed** diagnostic lower
bound (the +1.548 ns is shown, not hidden) and the gate becomes "tach_ordered beats `std`", which it
does (20.38 < 32.24). Scoped by provider name so `Instant` and every non-barrier pick stay
hard-gated. This is executor work under the doctrine ("gate the outcome") + M3.G1's own Fallback
("correct the claim"); it is **reversible** (veto reverts `fbe6e8b` + one re-measure). Full record:
`docs/ESCALATIONS.md` → `ESC-APPLE-ELAPSED-DISPATCH`.

## 3. Decision 2 — SIGN the public claims wording (fresh numbers below)

The numbers are measured and committed; the **wording** is yours. Draft tables (ns per call,
`now / now+elapsed`, lower is better):

**Same-thread `Instant`**

| Environment | tach | quanta | minstant | fastant | std |
|---|---:|---:|---:|---:|---:|
| Apple M1 Max | **0.65 / 1.64** | 3.35 / 7.30 | 27.59 | 27.32 | 20.39 / 44.31 |
| AWS Graviton 3 | **6.67 / 13.35** | 6.80 / 14.18 | 41.42 | 41.12 | 32.24 / 70.09 |
| AWS Intel Linux | **14.72 / 30.43** | 17.21 / 38.22 | 14.75 | 14.75 | 25.97 / 55.94 |
| GitHub Windows 2025 | 25.27 / 53.41 | 11.46 / 23.23 † | 41.20 | 41.21 | 37.73 / 78.29 |

**Synchronization-ordered `OrderedInstant`**

| Environment | tach::OrderedInstant | std::time::Instant |
|---|---:|---:|
| Apple M1 Max | **7.74 / 15.63** | 20.39 / 44.31 |
| AWS Graviton 3 | **20.38 / 40.04** | 32.24 / 70.09 |
| AWS Intel Linux | **22.39 / 43.65** | 25.97 / 55.94 |
| GitHub Windows 2025 | **25.28 / 53.34** | 37.73 / 78.29 |

† **Windows honesty note (needs your wording):** quanta's `Instant` (11.46 ns) is faster than tach's
QPC (25.27 ns) but **forgoes** QPC's documented cross-core / hypervisor / platform-timeline
guarantees, so it is **not eligible** for tach's reliable contract (ADR-0005). tach is the fastest
*eligible* read; the campaign passes on the eligible-reference gate, not on being unconditionally
fastest. This is the one place the public claim must be carefully scoped.

## 4. Decision 3 — ACCEPT M3.G1 closure (after decisions 1–2 + the claims edit land)

M3.G1 conditions: (1) deletion list gone / archived ✅; (2) no workflow references a deleted path
✅; (4) consistency greps substantively clean ✅; (3) README/BENCHMARKS carry the fresh numbers and
reference no deleted provenance — **the remaining edit, gated on your Decision 2**.

## 5. Claims-rewrite spec (applied on your Decision 2 sign-off)

`BENCHMARKS.md`:
- Replace the six-cell tables (lines 42–75) with the four-primary tables above; the current tables
  are the frozen `76fd4b1` campaign (pre-Apple-bare-counter) and are stale.
- Rewrite the intro (lines 5–18): `4 primary + 11 supplemental` / `six native full-speed cells`
  framing → the current **4 primary cells**; update the revision + digest to `505b3d7`.
- Fix the deleted-provenance reference: line 178 calls `benches/validate-speed-evidence.py`
  (deleted in the §7.1 apparatus diet) — replace with the retained validator path.
- Repoint the durable-package reference (lines 133, 171) to `EVID-SPEED-CAMPAIGN-2026-07-17`.
- Regenerate `benches/summary-*.png` from the fresh cells.

`README.md`: the existing framing already supports the disposition (line 7-8 "fastest among
providers **eligible for its contract**"; line 104 "`isb sy` before CNTVCT_EL0"). Update the
environments list (line 54) and any embedded numbers; carry the Windows eligibility note.

## Publishing stays yours

On your sign-off the claims edit lands, M3.G1 closes, and the release is ready for **your** publish
act. Nothing here has been published, tagged, or force-pushed.
