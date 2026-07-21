# `EVID-PRIMARY-SPEED-CAMPAIGN` — Four-cell primary speed campaign, re-bound at `e35ec98` (2026-07-20)

**Status: GATE-SUPPORTING 🟢 — `validate_campaign_for_checkout` PASSED at `e35ec98`, all four cells bound, zero failures.**

Fresh steady-state read/elapsed numbers for all three public timer contracts across the four
primary target identities. This package was first established 2026-07-18 at `f6df5df` and is
re-bound here at `e35ec98`, re-measured to follow the `OrderedInstant` → `GlobalInstant` rename
([ADR-0008](../../../decisions/0008-rename-global-instant.md)). The rename is a pure identifier
change, so the numbers reproduce the `f6df5df` measurement within noise; that prior measurement is
preserved in this repository's git history. Bound to source revision
`e35ec986c0a797f2291908b91863e374dd352824`.

## Provenance

- Repo SHA: `e35ec986c0a797f2291908b91863e374dd352824` (`refactor(api)!: rename OrderedInstant to
  GlobalInstant`), which includes `f6df5df` (the prior release revision) plus the type rename.
- Substrate: four primary target identities (below), each source-sealed to that revision.
- Command surface: `run-speed-local.sh` (Apple), `run-speed-aws.sh` (c7g, inteln), `bench.yml`
  (Windows CI run 29801858831).
- The `ci.yml` 24-target route-proof passed at `e35ec98` (run 29801712705) — the sealed bench
  tooling rename is codegen-clean on every target.

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-SIMPLIFY-TIMERS.M4.G1` | 🟢 four cells validate at one revision, checkout-bound, zero failures | [`campaign-report.json`](campaign-report.json) (`passed: true`) · source `e35ec98` |
| `OBJ-SIMPLIFY-TIMERS.M5.G1` | 🟢 runtime-selection audit holds at the renamed revision; no wall tournament remains in-tree | [`campaign-report.json`](campaign-report.json) · source `e35ec98` |

## Cells and producers

| Cell | Target | Host | Producer |
|---|---|---|---|
| [`speed-0-apple.json`](speed-0-apple.json) | aarch64-apple-darwin | catalyst (Apple M1 Max) | `run-speed-local.sh` |
| [`speed-1-c7g.json`](speed-1-c7g.json) | aarch64-unknown-linux-gnu | AWS c7g.large (Graviton3) | `run-speed-aws.sh` (self-terminating) |
| [`speed-2-inteln.json`](speed-2-inteln.json) | x86_64-unknown-linux-gnu | AWS c7i.large | `run-speed-aws.sh` (self-terminating) |
| [`speed-4-windows.json`](speed-4-windows.json) | x86_64-pc-windows-msvc | GitHub `windows-2025` | `bench.yml` (CI run 29801858831) |

## Findings

**tach `Instant` is the fastest or materially-tied read on every primary cell, and `GlobalInstant`
beats `std` on every one.** Under the material-tie rule (point estimate plus the conservative edges
of both 95% CIs within `max(1 ns, 5%)`), the honest per-cell result at `e35ec98`
(ns, `now / now+elapsed`, lower is better):

| Cell | `Instant` tach / fastest eligible reference | verdict | `GlobalInstant` tach / std |
|---|---|---|---|
| apple M1 Max | **0.65 / 1.59** / quanta 3.34 | fastest outright | **7.65 / 15.07** / 19.84 |
| c7g Graviton3 | **6.68 / 13.35** / quanta 6.83 | fastest (within margin) | **20.38 / 40.05** / 32.23 |
| inteln c7i | **13.94 / 28.78** / minstant 13.96, fastant 13.98 | material tie; beats quanta 16.28 | **21.26 / 41.34** / 24.61 |
| windows 2025 | **11.48 / 22.76** / quanta 11.44 | material tie | **25.29 / 53.37** / 37.72 |

**The numbers reproduce the `f6df5df` release measurement within noise.** apple, c7g, and windows
land within a fraction of a nanosecond of the prior run; the `c7i.large` runner drew a ~6% faster
instance this time (inteln `Instant` 13.94 vs 14.85), and its references moved proportionally, so
every material-tie and `std`-beat verdict is unchanged. Per the no-cherry-pick rule this valid run
stands; the relative claims are identical to the prior binding.

**The c7g barrier-exposed `GlobalInstant` disposition is reproduced and stands.** On Graviton3 the
ordered pick is `isb; cntvct`; the mandatory `isb` barrier forbids the out-of-order overlap that
hides the SIGILL-safe provider dispatch, so the public ordered read gates on the shippable
reference (tach_ordered 20.38 ns < std 32.23 ns), with the compile-time-specialized route retained
as a disclosed diagnostic lower bound. Adjudicated in `docs/ESCALATIONS.md` →
`ESC-APPLE-ELAPSED-DISPATCH`.

## Open

None for the campaign: all four cells passed and bind to a single revision with a clean checkout.

## Reproduce

```
# Apple (catalyst, M1 Max):
benches/run-speed-local.sh .tach-bench-out/e35ec98/speed-0-apple.json
# AWS c7g + inteln (self-terminating; add current IP to SG sg-05e99abafa54936d3 first):
benches/run-speed-aws.sh c7g    c7g.large     # -> speed-1-c7g.json
benches/run-speed-aws.sh inteln c7i.large     # -> speed-2-inteln.json (retry serially on the known signal-reentry harness flake)
# Windows (GitHub Actions):
gh workflow run bench --ref main              # -> artifact tach-speed-windows-2025-<sha>/speed-4-windows.json
# Validate the assembled four-cell dir (each cell beside its .collector.bundle):
python3 -c "import json,sys; sys.path.insert(0,'benches'); import speed_evidence as se; \
from pathlib import Path; d=Path('.tach-bench-out/e35ec98'); \
cells={n:d/n for n in ('speed-0-apple.json','speed-1-c7g.json','speed-2-inteln.json','speed-4-windows.json')}; \
docs={k:json.loads(v.read_text()) for k,v in cells.items()}; \
r=se.validate_campaign_for_checkout(docs,Path('.'),cells); print('passed',r['passed'],'failures',r['failures'])"
```

## Raw artifacts

- [`campaign-report.json`](campaign-report.json) — full `validate_campaign_for_checkout` report (`passed: true`, per-cell bound observations, checkout binding at `e35ec98`).
- [`speed-0-apple.json`](speed-0-apple.json) · [`speed-1-c7g.json`](speed-1-c7g.json) · [`speed-2-inteln.json`](speed-2-inteln.json) · [`speed-4-windows.json`](speed-4-windows.json) — the four composed primary cells, each source-sealed to `e35ec98`.
