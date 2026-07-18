# `EVID-PRIMARY-SPEED-CAMPAIGN` — Refined-contract four-cell primary speed campaign at `f6df5df` (2026-07-18)

**Status: GATE-SUPPORTING 🟢 — `validate_campaign_for_checkout` PASSED at `f6df5df`, all four cells bound, zero failures.**

Fresh steady-state read/elapsed numbers for all three public timer contracts across the four
primary target identities, re-measured at the release revision `f6df5df` after the Apple x86
`Instant` fixed-pick conversion (the last in-tree wall tournament) re-sealed the benchmark source.
Supersedes the `4259e92` campaign (`EVID-SPEED-CAMPAIGN-REFINED`) for the release. Bound to source
revision `f6df5df4ce8c5b0576e42d0e7cb2bd06dbcfa37b`. Revision `f6df5df` additionally carries the
`ThreadCpuInstant`→QPC wall-fallback fix that removed a stale `rdtsc` leak from the Windows
thread-cpu route, and `ci.yml` (24-target route-proof + inline-parity) is green at this revision.

## Provenance

- Repo SHA: `f6df5df4ce8c5b0576e42d0e7cb2bd06dbcfa37b` (`fix(windows): route ThreadCpuInstant wall fallback through QPC, not the TSC Instant path`), which includes `fcdcd95` (Apple x86 `Instant` → fixed `mach_absolute_time`) and `4259e92` (Windows `Instant` → calibrated invariant TSC).
- Substrate: four primary target identities (below), each source-sealed to that revision.
- Command surface: `run-speed-local.sh` (Apple), `run-speed-aws.sh` (c7g, inteln), `bench.yml` (Windows CI run 29647697572).
- Retained collector bundles (7–9 MB each) held out-of-repo per the evidence-size discipline; the composed cells plus `campaign-report.json` are the committed proof.

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-SIMPLIFY-TIMERS.M4.G1` | 🟢 four cells validate at one revision, checkout-bound, zero failures; Windows `Instant` reads the calibrated invariant TSC | [`campaign-report.json`](campaign-report.json) (`passed: true`) · source `f6df5df` |
| `OBJ-SIMPLIFY-TIMERS.M5.G1` | 🟢 campaign re-measured green at the revision that carries the Apple x86 `Instant` fixed-pick; no wall tournament remains in-tree | [`campaign-report.json`](campaign-report.json) · source `f6df5df` |

## Cells and producers

| Cell | Target | Host | Producer |
|---|---|---|---|
| [`speed-0-apple.json`](speed-0-apple.json) | aarch64-apple-darwin | catalyst (Apple M1 Max) | `run-speed-local.sh` |
| [`speed-1-c7g.json`](speed-1-c7g.json) | aarch64-unknown-linux-gnu | AWS c7g.large (Graviton3) | `run-speed-aws.sh` (self-terminating) |
| [`speed-2-inteln.json`](speed-2-inteln.json) | x86_64-unknown-linux-gnu | AWS c7i.large | `run-speed-aws.sh` (self-terminating) |
| [`speed-4-windows.json`](speed-4-windows.json) | x86_64-pc-windows-msvc | GitHub `windows-2025` | `bench.yml` (CI run 29647697572) |

## Findings resolved here

**tach `Instant` is the fastest or materially-tied read on every primary cell, and `OrderedInstant`
beats `std` on every one.** Under the material-tie rule (point estimate plus the conservative edges
of both 95% CIs within `max(1 ns, 5%)`), the honest per-cell result:

Headline steady-state reads (ns, `now / now+elapsed`, lower is better):

| Cell | `Instant` tach / fastest eligible reference | verdict | `OrderedInstant` tach / std |
|---|---|---|---|
| apple M1 Max | **0.65 / 1.63** / quanta 3.37 | fastest outright | **7.73 / 15.38** / 20.21 |
| c7g Graviton3 | **6.67 / 13.35** / quanta 6.79 | fastest (within margin) | **20.38 / 40.04** / 32.27 |
| inteln c7i | **14.85 / 30.65** / fastant 14.87, minstant 14.85 | material tie; beats quanta 17.38 | **22.60 / 43.96** / 26.15 |
| windows 2025 | **11.48 / 22.77** / quanta 11.44 | material tie (tach faster on elapsed: 22.77 < 23.90) | **25.27 / 53.35** / 37.76 |

**The calibrated invariant TSC keeps Windows `Instant` competitive; the QPC eligibility caveat is
retired.** Windows `Instant` reads a bare invariant TSC (`windows_tsc`), landing at a material tie
with quanta (11.48 vs 11.44) — versus the prior 25.27 ns QPC read that was admitted only on the
eligibility gate. This CI run measured a slower/noisier `windows-2025` runner than the `4259e92`
campaign (QPC-based reads — `std` 37.76, `OrderedInstant` 25.27 — are inflated ~30–58% run-to-run),
so `Instant` reads as a tie here rather than the `4259e92` clean win (9.29 vs 11.91). The relative
claim is unchanged and honest: fastest or materially tied on `Instant`, and `OrderedInstant` beats
`std`. GitHub CI runner variance is inherent; a single sealed run is one honest sample.

**Apple x86 `Instant` is now a fixed pick (M5).** The revision includes `fcdcd95`, which removed the
Apple x86 TSC-vs-mach tournament in favor of a fixed `mach_absolute_time` read (XNU's x86
`mach_timebase` is 1/1). The four primary cells here reproduce the refined-contract numbers at that
revision; the Apple x86 route itself is proven by `verify-target-providers.py` (requires
`@mach_absolute_time`, forbids `rdtscp`/`llvm.x86.rdtsc`), not this speed campaign.

**The c7g barrier-exposed `OrderedInstant` disposition (`fbe6e8b`) is reproduced and stands.** On
Graviton3 the ordered pick is `isb; cntvct` (`aarch64_isb_cntvct`). The mandatory `isb` context
synchronization forbids the out-of-order overlap that hides the SIGILL-safe provider dispatch on the
barrier-free `Instant` path, so the public ordered read sits above a compile-time-specialized
`isb; cntvct` that pays no per-call dispatch — a read tach cannot ship (hardcoding the pick SIGILLs a
counter-disabled thread; ADR-0003 mandates the `isb`). Per the mis-modeled-gate correction the exact
route is retained as a disclosed diagnostic dispatch lower bound and the cell gates on the usable
public reference: tach_ordered 20.38 ns < std 32.27 ns. Adjudicated in `docs/ESCALATIONS.md` →
`ESC-APPLE-ELAPSED-DISPATCH`.

## Open

None for the campaign: all four cells passed and bind to a single revision with a clean checkout.
The Windows `Instant` beat→tie shift versus `4259e92` is CI-runner variance, not a regression, and
is folded into the claims wording (owner sign-off).

## Reproduce

```
# Apple (catalyst, M1 Max):
benches/run-speed-local.sh .tach-bench-out/f6df5df/speed-0-apple.json
# AWS c7g + inteln (self-terminating; add current IP to SG sg-05e99abafa54936d3 first):
benches/run-speed-aws.sh c7g    c7g.large     # -> speed-1-c7g.json
benches/run-speed-aws.sh inteln c7i.large     # -> speed-2-inteln.json (retry serially on the known signal-reentry harness flake)
# Windows (GitHub Actions):
gh workflow run bench --ref main              # -> artifact tach-speed-windows-2025-<sha>/speed-4-windows.json
# Validate the assembled four-cell dir (each cell beside its .collector.bundle):
python3 -c "import json,sys; sys.path.insert(0,'benches'); import speed_evidence as se; \
from pathlib import Path; d=Path('.tach-bench-out/f6df5df'); \
cells={n:d/n for n in ('speed-0-apple.json','speed-1-c7g.json','speed-2-inteln.json','speed-4-windows.json')}; \
docs={k:json.loads(v.read_text()) for k,v in cells.items()}; \
r=se.validate_campaign_for_checkout(docs,Path('.'),cells); print('passed',r['passed'],'failures',r['failures'])"
```

## Raw artifacts

- [`campaign-report.json`](campaign-report.json) — full `validate_campaign_for_checkout` report (`passed: true`, per-cell bound observations, checkout binding at `f6df5df`).
- [`speed-0-apple.json`](speed-0-apple.json) · [`speed-1-c7g.json`](speed-1-c7g.json) · [`speed-2-inteln.json`](speed-2-inteln.json) · [`speed-4-windows.json`](speed-4-windows.json) — the four composed primary cells, each source-sealed to `f6df5df`.
