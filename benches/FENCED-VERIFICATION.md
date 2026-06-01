# FencedInstant cross-thread monotonicity — verification

**Question:** does `FencedInstant` ever go backward across threads — i.e. can a
read on thread B, sequenced after a synchronization edge from thread A, return a
value less than A's? `FencedInstant` defends against this with a pipeline barrier
(`rdtscp` on x86, `isb sy` on aarch64) that pins the read in time; it then relies
on the underlying counter being globally monotonic across cores. This file records
where that holds.

## Method

The `measure_synchronization_order` test (load-then-now-then-check): thread B
Acquire-loads a value published by thread A, reads the clock, and checks
`now >= published`, then Release-publishes its own read. A violation is a genuine
synchronization-order inversion.

Two ways it's run:
- **unpinned**, across all 6 production skewmono cells (`benches/skewmono-*.json`,
  `synchronization_order` field).
- **pinned** (`--mode fenced-verify`, `benches/run-fenced-verify-aws.sh`): worker
  threads bound to specific cores to *force* cross-domain read pairs an unpinned
  scheduler might never produce — adversarial cross-socket pair, full-span
  (one thread/core), and oversubscribed-2x placements.

**Positive control.** Every placement also runs bare `tach` (no barrier). It MUST
show violations, or the placement was inert and the `FencedInstant` result is
*inconclusive*, not a pass. This is what makes a "0" meaningful.

## Results — FencedInstant held at 0 on every machine tested

| Cell | Arch / topology | Fenced viol | Fenced reads | Bare-tach control |
|---|---|---:|---:|---:|
| apple-silicon-m1 | aarch64, Apple M1, 1 socket | **0** | 125.0M | 17,366,168 ✓ fired |
| c7g-4xlarge | aarch64, Graviton 3, 1 socket | **0** | 138.1M | 1,827 ✓ fired |
| t3-medium | x86_64, Intel Nitro VM | **0** | 551.7M | 42 ✓ fired |
| m7i-metal-24xl | x86_64, Intel, 1-socket metal | **0** | 151.6M | 9,629,641 ✓ fired |
| lambda-x86_64 | x86_64, Firecracker | **0** | 80.2M | 13,351 ✓ fired |
| github-windows-x86_64 | x86_64, Windows, 1 socket | **0** | 336.1M | 1,678,700 ✓ fired |
| local M4 Pro (pinned) | aarch64, Apple M4 Pro, 1 die | **0** | 194.0M | 4.2K / 2.9M / 2.6M ✓ fired |

**Total: 0 backward steps in 1,576,780,486 FencedInstant cross-thread reads.**

On the M4 Pro the pinned study fired the control on all three placements
(adversarial-pair, full-span, oversubscribed-2x) — 4.2K to 2.9M bare violations,
max ~800 ns — while both `FencedInstant` and `SyncedInstant` stayed at 0. So the
0 is interpretable, not a placement that simply never raced.

## Boundary table (arch × topology)

| | single-socket / coherent counter | multi-socket / NUMA |
|---|---|---|
| **aarch64** | ✅ Fenced sufficient (M1, M4, Graviton 3) | not tested — ARM system counter is one SoC-broadcast IP; skew unlikely |
| **x86_64** | ✅ Fenced sufficient (t3, m7i-24xl, lambda, windows) | ⏳ **OPEN — decisive test, run pending** |
| **riscv64 / loongarch64** | ⚠️ best-effort by spec (fence-vs-CSR ordering implementation-defined); unverified | — |

## Honest limitations

- A "0" means **not observed at ~1/total_reads sensitivity**, never proof of
  impossibility.
- Every cell verified so far is **single-socket / coherent-counter**. The one
  topology that could genuinely make `FencedInstant` go backward — where the
  barrier correctly pins the read in time but the *other socket's TSC physically
  lags* — is **multi-socket / NUMA x86**, which is **not yet tested**. A true
  cross-socket test requires a bare-metal instance (`m7i.metal-48xl`, 2-socket,
  192 vCPU); a virtualized instance shares one hypervisor-synced TSC and can't
  exercise the cross-socket case. The harness and runner
  (`run-fenced-verify-aws.sh`) are ready and the AWS account can launch it
  (on-demand standard vCPU quota is 592). Only `SyncedInstant`'s value-floor
  would catch a cross-socket inversion if one occurs.
- riscv64 / loongarch64 **cannot be closed by hardware runs** — a board that
  doesn't reorder in practice proves nothing about the ISA's best-effort
  guarantee. Their honest status is "use `SyncedInstant` for a
  guaranteed-by-construction ordering on those targets."

## Bottom line

On every modern single-socket architecture we care about — Apple Silicon
(M1/M4), AWS Graviton 3, Intel x86 (virtualized, bare-metal, Firecracker),
Windows x86 — `FencedInstant` does not go backward across threads: 0 violations
in ~1.58 billion reads, with the bare-counter control firing to prove the test
was live. The remaining gap is multi-socket NUMA x86 — the only place the
underlying counter itself could be incoherent. The harness and AWS runner are in
place to settle it; the run is pending.
