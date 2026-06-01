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
| **intel-2s-m7i (pinned)** | **x86_64, Xeon 8488C, 2 sockets** | **0** | 3.6B | **18K / 96.9M / 112M, max 55µs** ✓ fired |
| **amd-2s-m7a (pinned)** | **x86_64, EPYC 9R14, 2 sockets** | **0** | 5.7B | **399M / 2.1B / 2.2B, max 97µs** ✓ fired |

**Total: 0 backward steps in ~10.9 billion FencedInstant cross-thread reads.**

On the M4 Pro the pinned study fired the control on all three placements
(adversarial-pair, full-span, oversubscribed-2x) — 4.2K to 2.9M bare violations,
max ~800 ns — while both `FencedInstant` and `SyncedInstant` stayed at 0. So the
0 is interpretable, not a placement that simply never raced.

**The two-socket runs are the decisive ones.** On a genuinely multi-socket box
the per-socket TSCs are *not* coherent: bare `tach` went backward up to **55 µs
on Intel** and **97 µs on AMD** — orders of magnitude beyond the sub-µs
single-socket read-reorder window, i.e. real cross-socket counter skew. On AMD,
~8% of all cross-socket reads inverted (399M on the adversarial pair alone).
This is exactly the topology where a barrier that only pins the read *in time*
could still hand back a lagging counter value. It did not: `FencedInstant` held
at **0 across every placement on both Intel and AMD** (`rdtscp` waits for prior
loads to be globally visible, which is sufficient even when the counters
themselves disagree by tens of µs). `SyncedInstant`'s value-floor also held at 0,
as expected by construction. Cross-socket pin pairs were verified to straddle
sockets (cpu0/cpu48 on Intel, cpu0/cpu96 on AMD) before trusting the result.

The Intel oversubscribed-2x row records `tach`+`tach_fenced` only — the instance
was terminated on request seconds before its `tach_synced`/`std` clocks recorded
(noted in `fenced-verify-intel-2s-m7i.json`). The decisive control and subject
are complete for all three placements; AMD pulled its full JSON normally.

## Boundary table (arch × topology)

| | single-socket / coherent counter | multi-socket / NUMA |
|---|---|---|
| **aarch64** | ✅ Fenced sufficient (M1, M4, Graviton 3) | not tested — ARM system counter is one SoC-broadcast IP; skew unlikely |
| **x86_64** | ✅ Fenced sufficient (t3, m7i-24xl, lambda, windows) | ✅ **Fenced sufficient — Intel Xeon 8488C + AMD EPYC 9R14, control fired to 97µs, fenced=0** |
| **riscv64 / loongarch64** | ⚠️ best-effort by spec (fence-vs-CSR ordering implementation-defined); unverified | — |

## Honest limitations

- A "0" means **not observed at ~1/total_reads sensitivity**, never proof of
  impossibility. The multi-socket runs reach ~3.6B (Intel) and ~5.7B (AMD)
  fenced reads each, so the sensitivity is well below 1 ppb on the topology that
  matters most.
- The decisive **multi-socket / NUMA x86** case is now tested on both vendors
  (Intel Xeon 8488C, AMD EPYC 9R14), each a true 2-socket box with verified
  cross-socket pin pairs. These are bare-metal — a virtualized instance shares
  one hypervisor-synced TSC and can't exercise the cross-socket case. Not tested:
  >2-socket (4/8-socket) systems and non-AWS firmware; the mechanism (`rdtscp`
  waiting for prior loads to be globally visible) is socket-count-independent, so
  this is a coverage gap, not a known risk.
- aarch64 multi-socket (e.g. 2-socket Ampere Altra) is untested but low-risk —
  the ARMv8 system counter is a single SoC-broadcast IP, so cross-socket skew of
  the kind x86 TSCs exhibit doesn't have an analog.
- riscv64 / loongarch64 **cannot be closed by hardware runs** — a board that
  doesn't reorder in practice proves nothing about the ISA's best-effort
  guarantee. Their honest status is "use `SyncedInstant` for a
  guaranteed-by-construction ordering on those targets."

## Bottom line

On every modern architecture we care about — Apple Silicon (M1/M4), AWS
Graviton 3, Intel x86 and AMD x86 (virtualized, single-socket bare-metal,
Firecracker, **and 2-socket NUMA bare-metal**), Windows x86 — `FencedInstant`
does not go backward across threads: **0 violations in ~10.9 billion reads**,
with the bare-counter control firing on every cell to prove each test was live.

The decisive case was multi-socket NUMA x86, where the per-socket TSCs are
genuinely incoherent — the bare counter went backward up to **55 µs on Intel**
and **97 µs on AMD**, with up to ~8% of cross-socket reads inverting. Even there,
`rdtscp`'s "wait for prior loads to be globally visible" semantics held the
fenced read at 0 across billions of reads on both vendors. The one topology that
could have broken `FencedInstant` is the one that confirmed it.
