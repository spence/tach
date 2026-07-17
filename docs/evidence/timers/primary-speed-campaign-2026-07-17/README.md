# EVID-SPEED-CAMPAIGN-2026-07-17 — Primary four-cell speed campaign

Fresh steady-state read/elapsed numbers for all three public timer contracts across the four
primary target identities, measured on the converted fixed-pick tree and bound to source
revision `505b3d76a72014ce5f9e61a09dde64c003da9378`.

## What this proves

`validate_campaign_for_checkout` passes for all four cells at one revision with the checkout
binding satisfied (`campaign-report.json`, `passed: true`, zero failures). This is the
`OBJ-SIMPLIFY-TIMERS.M3.G1` "fresh numbers" evidence.

## Cells and producers

| Cell | Target | Host | Producer |
|---|---|---|---|
| `speed-0-apple.json` | aarch64-apple-darwin | catalyst (Apple M1 Max) | `run-speed-local.sh` |
| `speed-1-c7g.json` | aarch64-unknown-linux-gnu | AWS c7g.large (Graviton3) | `run-speed-aws.sh` (self-terminating) |
| `speed-2-inteln.json` | x86_64-unknown-linux-gnu | AWS c7i.large | `run-speed-aws.sh` (self-terminating) |
| `speed-4-windows.json` | x86_64-pc-windows-msvc | GitHub `windows-2025` | `bench.yml` (CI run 29577574576) |

Collector bundles (7–9 MB each) are retained out-of-repo per the evidence-size discipline; the
composed cells plus `campaign-report.json` are the committed proof.

## Headline steady-state reads (ns, lower is better)

| Cell | `Instant::now` tach / fastest competitor | `OrderedInstant::now` tach / std |
|---|---|---|
| apple M1 Max | **0.65** / quanta 3.35 | **7.74** / 20.39 |
| c7g Graviton3 | **6.67** / quanta 6.80 | **20.38** / 32.24 |
| inteln c7i | **14.72** / minstant 14.75 | **22.39** / 25.97 |
| windows 2025 | 25.27 (QPC) / quanta 11.46 † | **25.28** / 37.73 |

† On Windows, quanta's faster `Instant` forgoes QPC's documented cross-core / hypervisor /
platform-timeline guarantees and is therefore **not eligible** for tach's reliable contract
(ADR-0005); tach is the fastest *eligible* read. The campaign passes on the eligible-reference
gate, not on being unconditionally fastest.

## The c7g barrier-exposed ordered disposition (`fbe6e8b`)

On Graviton3 the `OrderedInstant` pick is `isb; cntvct`. The mandatory `isb` context
synchronization forbids the out-of-order overlap that hides the SIGILL-safe provider dispatch on
the barrier-free `Instant` path (measured at +0.001 ns here), so the public ordered read sits
**+1.548 ns (9/9 decisive losses)** over a compile-time-specialized `isb; cntvct` that pays no
per-call dispatch — a read tach cannot ship (hardcoding the pick SIGILLs a counter-disabled
thread; ADR-0003 mandates the `isb`). Per the mis-modeled-gate correction, the exact route is
retained as a **diagnostic dispatch lower bound** (the +1.548 ns is disclosed, not hidden) and the
cell gates on the usable public reference: tach_ordered 20.38 ns < std 32.24 ns. Adjudicated in
`docs/ESCALATIONS.md` → `ESC-APPLE-ELAPSED-DISPATCH`.
