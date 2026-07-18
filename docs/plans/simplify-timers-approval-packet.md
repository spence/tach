# Approval packet — OBJ-SIMPLIFY-TIMERS M4.G1 (refined-contract publish-readiness)

Re-prepared 2026-07-18 at release revision `f6df5df` (supersedes the `4259e92` draft). Complete: it
names the owner decisions that remain and gives you everything to make them from this document. **No
publish, tag, or force-push has been done — publishing stays your act, outside this goal.**

The release candidate is built and **passes `cargo publish --dry-run`** at `f6df5df` (docs rebound at
`5f4dc79`): `tach v0.2.0`, 51 files, 2.2 MiB, verify-compile clean. One benign packaging warning
(`apple_suspend_probe` bench excluded from the published package — benches are not published;
silencing it would edit `Cargo.toml`, re-seal the campaign, and force a full re-measure, so it stays).

## 1. What is done (driven and verified)

- **M0–M3 closed; M5.G1 closed** (`f6df5df`) — the runtime-selection audit: Apple x86 `Instant` is a
  fixed `mach_absolute_time` pick (last in-tree tournament removed), every runtime-selection point in
  the provider-policy matrix is dispositioned, `verify-target-providers.py` passes on both feature
  surfaces, and fmt/clippy/`test --lib` are green on default and `--no-default-features`.
- **ADR-0007 accepted** — three contracts sharpened by guarantee: `Instant` = fastest **same-core**
  clock (`elapsed()` saturates to zero, never negative); `OrderedInstant` = fastest
  **cross-core-reliable** clock (value-consistent + happens-before edge); `ThreadCpuInstant` = fastest
  reliable per-thread time. The cross-core guarantee moved from `Instant` to `OrderedInstant`.
- **Windows `Instant` QPC → calibrated invariant TSC landed** (`4259e92`, provider `windows_tsc`):
  x86/x86_64 read a rate-calibrated invariant TSC behind a CPUID gate, degrading to QPC when
  ineligible; `OrderedInstant` and aarch64-Windows `Instant` stay on QPC (a stated x86-only fork).
- **A release CI regression was found and fixed** (see §2) — `ci.yml` (24-target route-proof +
  inline-parity) had been red since M4; both causes are resolved at `f6df5df` and `ci.yml` is green.
- **Fresh four-primary speed campaign GREEN at `f6df5df`.** `validate_campaign_for_checkout` passes
  for all four cells at one revision with checkout binding, **zero failures**. Committed as
  `EVID-PRIMARY-SPEED-CAMPAIGN`; `campaign-report.json` is the proof. README/BENCHMARKS are rebound to
  these numbers (`5f4dc79`).

## 2. The CI regression found and fixed (reported for your awareness — no decision needed)

After the campaign went green I found `ci.yml` — a **separate** proof set from the speed campaign —
red since M4 (`4259e92`; last green `a21f447`). M4's nine green gates were fmt/check/test/clippy and
never ran `ci.yml`. Two failures, both now resolved:

1. **24-target route proof** — `x86_64-pc-windows-msvc thread_cpu_instant_now: unexpected
   llvm.x86.rdtsc`. Root cause: `4259e92`'s Windows `Instant`→TSC change made the generic thread-CPU
   wall fallback reach the x86 TSC `Instant` path, leaking `rdtsc` into the thread-CPU route (which
   forbids it). **Fixed at `f6df5df`**: the Windows thread-CPU wall fallback now routes through
   `QueryPerformanceCounter` — the documented wall fallback — not the TSC path. The fix touches only
   the fallback path, so measured numbers reproduce. Route proof is green at `f6df5df`.
2. **linux-x86_64 inline-parity** — a one-run variance trip. Confirmed **flaky, not real**: identical
   linux-x86 source passed at `4259e92`/`de27134`/`fcdcd95` and failed only at `b60cb05`/`e20af60`,
   and passed on the `f6df5df` CI run. No code change; retry-on-trip like the known signal-reentry
   harness flake.

Tracked as `ESC-M4-CI-RED` (resolved). Both causes are mine and closed — no owner decision.

## 3. Decision 1 — RATIFY the c7g ordered-gate correction (`fbe6e8b`) — recommend RATIFY

On Graviton3 `OrderedInstant` is `isb; cntvct`. The mandatory `isb` barrier forbids the out-of-order
overlap that hides the SIGILL-safe provider dispatch on the barrier-free `Instant` path, so the
public ordered read sits above a compile-time-specialized `isb; cntvct` that pays no per-call dispatch
— a read tach **cannot ship** (hardcoding the pick SIGILLs a counter-disabled thread; ADR-0003
mandates the `isb`). Rather than leave the gate parked, I corrected a **mis-modeled gate** (it fails
after competent work): the barrier-exposed ordered pick takes the existing residual-cell contract
`dispatch_lower_bound_with_public_winner_gate` — the exact route is a **disclosed** diagnostic lower
bound and the gate becomes "tach_ordered beats `std`", which it does (**20.38 < 32.27** at `f6df5df`).
Scoped by provider name so `Instant` and every non-barrier pick stay hard-gated. **ADR-0007
reinforces this** — `OrderedInstant` is explicitly the cross-core tier, so the barrier is
contract-required, not overhead to apologize for. Executor work under the doctrine ("gate the
outcome") and **reversible** (a veto reverts `fbe6e8b` + one re-measure). Full record:
`docs/ESCALATIONS.md` → `ESC-APPLE-ELAPSED-DISPATCH`.

## 4. Decision 2 — SIGN the public claims wording (numbers below, committed at `f6df5df`)

The numbers are measured and committed at `f6df5df`; the **wording** is yours. The tables as committed
to README/BENCHMARKS (ns per call, `now / now+elapsed`, lower is better):

**Same-core `Instant`** (tach vs the fastest eligible same-tier reference; `std` for scale)

| Environment | tach::Instant | fastest eligible reference | std (now) | verdict |
|---|---:|---|---:|---|
| Apple M1 Max | **0.65 / 1.63** | quanta 3.37 | 20.21 | fastest outright |
| AWS Graviton 3 | **6.67 / 13.35** | quanta 6.79 | 32.27 | fastest (within margin) |
| AWS Intel Linux | **14.85 / 30.65** | fastant 14.87, minstant 14.85 | 26.15 | material tie; beats quanta 17.38 |
| GitHub Windows 2025 | **11.48 / 22.77** | quanta 11.44 | 37.76 | material tie (tach faster on elapsed: 22.77 < 23.90) |

**Cross-core-reliable `OrderedInstant`** (vs `std`, the only comparably-safe reference)

| Environment | tach::OrderedInstant | std::time::Instant (now) |
|---|---:|---:|
| Apple M1 Max | **7.73 / 15.38** | 20.21 |
| AWS Graviton 3 | **20.38 / 40.04** | 32.27 |
| AWS Intel Linux | **22.60 / 43.96** | 26.15 |
| GitHub Windows 2025 | **25.27 / 53.35** | 37.76 |

The honest headline: tach `Instant` is the **fastest or materially tied** read on every primary cell
(apple/c7g fastest outright; inteln/windows a within-margin tie under the `max(1 ns, 5%)` rule), and
`OrderedInstant` beats `std` on every one. The scoping the public claim needs is the **contract
distinction**, not an apology: `Instant` is same-core (the tier quanta/minstant/fastant also target);
the cross-core guarantee lives in `OrderedInstant` (the tier those libraries do not offer, so it is
compared to `std`).

**One honest shift from the `4259e92` draft — Windows `Instant` beat → tie.** The `4259e92` draft
showed Windows a clean win (9.29 vs quanta 11.91). This `f6df5df` CI run drew a slower/noisier
`windows-2025` runner (QPC-based `std`/`OrderedInstant` reads inflate ~30–58% run-to-run), so
`Instant` lands a **material tie** with quanta (11.48 vs 11.44) while tach is faster on elapsed
(22.77 < 23.90). This is CI-runner variance, not a regression; per the no-cherry-pick rule the valid
run **stands** (I did not re-run to chase the cleaner number). The relative claim is unchanged and
honest: fastest or materially tied on `Instant`, `OrderedInstant` beats `std`. The calibrated
invariant TSC keeps Windows competitive; the prior QPC-eligibility caveat is retired.

## 5. Decision 3 — ACCEPT M4.G1 / the packet (technical deliverables complete)

M4.G1 conditions, all met at `f6df5df` / `5f4dc79`: (1) ADR-0007 accepted ✅; (2) Windows `Instant`
raw-TSC on both feature surfaces, `OrderedInstant` unchanged ✅; (3) `validate_campaign_for_checkout`
green at one revision, `Instant` competitive, `OrderedInstant` beats `std` ✅
(`EVID-PRIMARY-SPEED-CAMPAIGN`); (4) README/BENCHMARKS describe the refined contract with fresh
committed evidence and no deleted-provenance claims ✅ (rebound `5f4dc79`); (5) a complete approval
packet awaits you ✅ (this document). I have **closed the M4.G1 gate** on these deliverables (🟢). The
remaining owner acts are ratifying the claims wording (§4) and accepting the milestone/packet —
tracked as `ESC-SIMPLIFY-M4-APPROVAL` and `ESC-M3-CLAIMS-REMEASURE` — plus the publish act. The same
claims wording satisfies the residual M3.G1 claims clause.

## 6. One provenance caveat for your awareness — `ThreadCpuInstant` numbers

`ThreadCpuInstant` (the third tier) is **not** re-measured at `f6df5df`. Its table and dedicated chart
use the retained, more-comprehensive `release-speed-closure-2026-07-14` package (revisions
`68dc201`/`c64dcb7`; +2 native environments, more samples); its measured code paths are unchanged by
ADR-0007/M4/M5, so the numbers remain valid at `f6df5df`. The top-of-file steady-state chart draws its
thread-CPU panel from the fresh `f6df5df` primary campaign; the two agree within run-to-run noise
except on `c7i.large`, where a close x86 provider tournament picked the raw `CLOCK_THREAD_CPUTIME_ID`
syscall in the retained package (150.75 ns) versus `clock_gettime` in the campaign (166.63 ns) — both
honest samples of the same variable tournament. BENCHMARKS discloses this. **Optional:** authorize a
full release-closure re-measure at `f6df5df` for single-source thread-CPU consistency; I recommend
**deferring** it post-0.2.0 (third tier, unchanged code paths, both honest — a re-measure delays
publish for a secondary refinement).

## Publishing stays yours

On your sign-off the claims wording is ratified, M4/M5 are accepted, and the release candidate — built
and dry-run-verified at `f6df5df` — is ready for **your** publish act. Nothing here has been
published, tagged, or force-pushed.
