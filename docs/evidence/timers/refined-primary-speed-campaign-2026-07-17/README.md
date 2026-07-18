# `EVID-SPEED-CAMPAIGN-REFINED` — Refined three-tier contract primary four-cell speed campaign (2026-07-17)

**Status: GATE CLOSED 🟢 — `validate_campaign_for_checkout` PASSED at `4259e92`, all four cells bound, zero failures.**

Fresh steady-state read/elapsed numbers for all three public timer contracts across the four
primary target identities, re-measured after the ADR-0007 refinement moved x86 Windows `Instant`
from QueryPerformanceCounter to a calibrated invariant TSC. Bound to source revision
`4259e92a4f84b83bd4a051a81f89e9be7669c45c`.

## Provenance

- Repo SHA: `4259e92a4f84b83bd4a051a81f89e9be7669c45c` (`feat(windows): read a calibrated invariant TSC for x86 Instant`).
- Substrate: four primary target identities (below), each source-sealed to that revision.
- Command surface: `run-speed-local.sh` (Apple), `run-speed-aws.sh` (c7g, inteln), `bench.yml` (Windows CI run 29625920608).
- Retained collector bundles (7–9 MB each) held out-of-repo per the evidence-size discipline; the composed cells plus `campaign-report.json` are the committed proof.

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-SIMPLIFY-TIMERS.M4.G1` | 🟢 all four cells validate at one revision, checkout-bound, zero failures | [`campaign-report.json`](campaign-report.json) (`passed: true`) · source `4259e92` |

## Cells and producers

| Cell | Target | Host | Producer |
|---|---|---|---|
| [`speed-0-apple.json`](speed-0-apple.json) | aarch64-apple-darwin | catalyst (Apple M1 Max) | `run-speed-local.sh` |
| [`speed-1-c7g.json`](speed-1-c7g.json) | aarch64-unknown-linux-gnu | AWS c7g.large (Graviton3) | `run-speed-aws.sh` (self-terminating) |
| [`speed-2-inteln.json`](speed-2-inteln.json) | x86_64-unknown-linux-gnu | AWS c7i.large | `run-speed-aws.sh` (self-terminating) |
| [`speed-4-windows.json`](speed-4-windows.json) | x86_64-pc-windows-msvc | GitHub `windows-2025` | `bench.yml` (CI run 29625920608) |

## Findings resolved here

**The refined `Instant` contract makes tach the fastest read on every primary cell — no eligibility caveat.**
Under ADR-0007, `Instant` is the fastest same-core clock, so x86 Windows `Instant` reads a bare
invariant TSC (`windows_tsc`) instead of QPC. The Windows CI collector confirms the TSC path
selected at runtime (`direct_selected_wall__windows_tsc`) while `OrderedInstant` stayed QPC
(`direct_selected_ordered_wall__windows_qpc_call_boundary`).

Headline steady-state reads (ns, lower is better):

| Cell | `Instant::now` tach / fastest eligible reference | `OrderedInstant::now` tach / std |
|---|---|---|
| apple M1 Max | **0.65** / quanta 3.35 | **7.66** / 20.18 |
| c7g Graviton3 | **6.67** / quanta 6.78 | **20.38** / 32.24 |
| inteln c7i | **14.56** / minstant 14.72 | **22.17** / 25.90 |
| windows 2025 | **9.29** (invariant TSC) / quanta 11.91 | **16.03** / 29.30 |

tach `Instant` is the fastest read in every primary environment, and `OrderedInstant` beats `std`
in every one. On Windows the prior campaign measured `Instant` at 25.27 ns (QPC) — slower than
quanta 11.46 and admitted only on the eligibility gate; the calibrated invariant TSC now reads at
**9.29 ns, faster than quanta outright**, so that caveat is retired.

**The c7g barrier-exposed `OrderedInstant` disposition (`fbe6e8b`) is reproduced and stands.**
On Graviton3 the ordered pick is `isb; cntvct` (`aarch64_isb_cntvct`). The mandatory `isb` context
synchronization forbids the out-of-order overlap that hides the SIGILL-safe provider dispatch on
the barrier-free `Instant` path, so the public ordered read sits above a compile-time-specialized
`isb; cntvct` that pays no per-call dispatch — a read tach cannot ship (hardcoding the pick SIGILLs
a counter-disabled thread; ADR-0003 mandates the `isb`). Per the mis-modeled-gate correction the
exact route is retained as a diagnostic dispatch lower bound (disclosed, not hidden) and the cell
gates on the usable public reference: tach_ordered 20.38 ns < std 32.24 ns. Adjudicated in
`docs/ESCALATIONS.md` → `ESC-APPLE-ELAPSED-DISPATCH`.

## Open

None. All four cells passed; the campaign binds to a single revision with a clean checkout.

## Reproduce

```
# Apple (catalyst, M1 Max):
benches/run-speed-local.sh .tach-bench-out/4259e92/speed-0-apple.json
# AWS c7g + inteln (self-terminating, cost-disciplined wrapper):
bash /tmp/aws-campaign.sh          # -> speed-1-c7g.json, speed-2-inteln.json
# Windows (GitHub Actions):
gh workflow run bench --ref main   # -> artifact tach-speed-windows-2025-<sha>/speed-4-windows.json
# Validate the assembled four-cell dir (each cell beside its .collector.bundle):
python3 -c "import json,sys; sys.path.insert(0,'benches'); import speed_evidence as se; \
from pathlib import Path; d=Path('.tach-bench-out/4259e92'); \
cells={n:d/n for n in ('speed-0-apple.json','speed-1-c7g.json','speed-2-inteln.json','speed-4-windows.json')}; \
docs={k:json.loads(v.read_text()) for k,v in cells.items()}; \
r=se.validate_campaign_for_checkout(docs,Path('.'),cells); print('passed',r['passed'],'failures',r['failures'])"
```

## Raw artifacts

- [`campaign-report.json`](campaign-report.json) — full `validate_campaign_for_checkout` report (`passed: true`, per-cell bound observations, checkout binding at `4259e92`).
- [`speed-0-apple.json`](speed-0-apple.json) · [`speed-1-c7g.json`](speed-1-c7g.json) · [`speed-2-inteln.json`](speed-2-inteln.json) · [`speed-4-windows.json`](speed-4-windows.json) — the four composed primary cells, each source-sealed to `4259e92`.
