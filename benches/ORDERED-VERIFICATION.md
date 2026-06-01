# OrderedInstant cross-thread monotonicity — verification

**Question:** does `OrderedInstant` ever go backward across threads — can a read on
thread B, sequenced after a synchronization edge from thread A, return a value less
than A's? `OrderedInstant` defends against this with an instruction-ordering barrier
(`rdtscp` on x86, `isb sy` on aarch64) that pins the counter read after prior loads
are globally visible. This file records that it holds, and that the fast comparison
crates — which lack the barrier — do not.

## Method

The `measure_synchronization_order` test (load-then-now-then-check): thread B
Acquire-loads a value published by thread A, reads the clock, checks
`now >= published`, then Release-publishes its own read. A violation is a genuine
synchronization-order inversion.

Two ways it's run:
- **unpinned**, across all 6 production skewmono cells (`benches/skewmono-*.json`,
  `synchronization_order` field) — for `tach`, `tach_ordered`, `std`, `quanta`,
  `minstant`, `fastant`.
- **pinned** (`--mode ordered-verify`, `benches/run-ordered-verify-aws.sh`): worker
  threads bound to specific cores to *force* the cross-domain read pairs an unpinned
  scheduler might never produce — adversarial cross-socket pair, full-span (one
  thread/core), and oversubscribed-2x placements.

**Positive control.** Every placement also runs bare `tach` (no barrier). It MUST
show violations, or the placement was inert and the result is *inconclusive*, not a
pass. This is what makes a "0" meaningful.

## Results — OrderedInstant held at 0 on every machine tested

| Cell | Arch / topology | `tach_ordered` viol | reads | bare-`tach` control |
|---|---|---:|---:|---:|
| apple-silicon-m1 | aarch64, Apple M1, 1 socket | **0** | 125.0M | 17,366,168 ✓ fired |
| c7g-4xlarge | aarch64, Graviton 3, 1 socket | **0** | 138.1M | 1,827 ✓ fired |
| t3-medium | x86_64, Intel Nitro VM | **0** | 551.7M | 42 ✓ fired |
| m7i-metal-24xl | x86_64, Intel, 1-socket metal | **0** | 151.6M | 9,629,641 ✓ fired |
| lambda-x86_64 | x86_64, Firecracker | **0** | 80.2M | 13,351 ✓ fired |
| github-windows-x86_64 | x86_64, Windows, 1 socket | **0** | 336.1M | 1,678,700 ✓ fired |
| M4 Pro (pinned) | aarch64, Apple M4 Pro, 1 die | **0** | 194.0M | 4.2K / 2.9M / 2.6M ✓ fired |
| **intel-2s-m7i (pinned)** | **x86_64, Xeon 8488C, 2 sockets** | **0** | 3.6B | **18K / 96.9M / 112M, max 55µs** ✓ fired |
| **amd-2s-m7a (pinned)** | **x86_64, EPYC 9R14, 2 sockets** | **0** | 5.7B | **399M / 2.1B / 2.2B, max 97µs** ✓ fired |

**Total: 0 backward steps in ~10.9 billion `OrderedInstant` cross-thread reads.**

The two-socket runs are the decisive ones. On a genuinely multi-socket box bare
`tach` inverted enormously — up to **55 µs on Intel** and **97 µs on AMD** (on AMD,
~8% of all cross-socket reads on the adversarial pair). That is exactly the regime
where a read-ordering barrier is most stressed, and `OrderedInstant` held at **0
across every placement on both vendors**. Cross-socket pin pairs were verified to
straddle sockets (cpu0/cpu48 on Intel, cpu0/cpu96 on AMD) before trusting the result.

## The fast comparison crates invert; OrderedInstant and std don't

The headline result. The same pinned cross-socket test, run side-by-side against the
whole ecosystem on the two-socket boxes (`benches/ordered-verify-{intel,amd}-*.json`).
Identical placement, identical hardware, identical moment — violation counts:

**Intel Xeon 8488C, 2-socket (cross-socket pinned):**

| placement | `tach::Instant` | **`tach::OrderedInstant`** | `std` | `quanta` | `minstant` | `fastant` |
|---|--:|--:|--:|--:|--:|--:|
| adversarial-pair | 63,638 | **0** | 0 | 16,166 | 4,797 | 5,785 |
| full-span (192t) | 48,099,661 | **0** | 0 | 46,299,832 | 44,214,260 | 54,422,121 |
| oversubscribed-2x | 87,131,497 | **0** | 0 | 49,260,027 | 49,441,901 | 51,173,254 |

**AMD EPYC 9R14, 2-socket (cross-socket pinned):**

| placement | `tach::Instant` | **`tach::OrderedInstant`** | `std` | `quanta` | `minstant` | `fastant` |
|---|--:|--:|--:|--:|--:|--:|
| adversarial-pair | 257,284,001 | **0** | 0 | 121,250,245 | 253,498,378 | 237,108,349 |
| full-span (192t) | 1,529,678,258 | **0** | 0 | 1,595,243,184 | 1,536,214,626 | 1,597,493,438 |
| oversubscribed-2x | 1,610,301,168 | **0** | 0 | 1,615,712,350 | 1,627,973,714 | 1,463,503,959 |

`tach::Instant` is in the table on purpose — it is the **positive control**, and it
inverts right alongside the fast competitors (AMD full-span: Instant 1.53B, quanta
1.60B, minstant 1.54B, fastant 1.60B). That's the point: **every bare architectural-
counter read fails this contract the same way** — tach's included. The barrier is the
only thing that changes the outcome, and `tach::OrderedInstant` is the one clock here
that has it. `std` also passes, but via its vDSO / syscall path, at 2–8× the per-call
cost.

(`tach::Instant`'s inversions are *not* a defect — it's the fast read, documented to
trade cross-thread ordering for speed. Reach for `OrderedInstant` exactly when this
table matters to you.)

None of `quanta` / `minstant` / `fastant` exposes an *ordered* read variant — there is
no API knob to make them cross-thread-correct short of dropping to `std`.
`OrderedInstant` is the only fast clock in the set that is correct by construction.

(On aarch64, `minstant` / `fastant` fall back to `std` rather than reading the TSC, so
they pass there — honest nuance, but it means they're not actually fast on aarch64.)

## What the bare-counter violations actually are

Worth stating precisely, because it sharpens the claim. The bare-`tach` violation
*magnitude scales with thread count* (Intel: ~5.7 µs at 2 threads → ~55 µs at 384;
AMD up to ~97 µs) — the signature of **read-reorder amplified by cross-socket
cache-coherence latency**, not a fixed counter offset. AWS Nitro keeps the invariant
TSC synchronized across sockets, so the counters themselves are coherent; what we
measured is the bare read being sampled before the Acquire-load retires, with the
stall lengthened by the contended cross-socket cache line. Wherever a bare
cross-thread read appears to invert — whatever the underlying cause —
`OrderedInstant`'s barrier prevents it (0 / 10.9B). We did **not** isolate a
deliberately-desynchronized counter; that case is out of contract for *every* tach
type (see the "one assumption" note in the README).

## Boundary table (arch × topology)

| | single-socket / coherent counter | multi-socket / NUMA |
|---|---|---|
| **aarch64** | ✅ verified (M1, M4, Graviton 3) | not tested — ARM system counter is one SoC-broadcast IP; skew has no analog |
| **x86_64** | ✅ verified (t3, m7i-24xl, lambda, windows) | ✅ **verified — Intel Xeon 8488C + AMD EPYC 9R14, control fired to 97µs, ordered=0** |
| **riscv64 / loongarch64** | ⚠️ best-effort by ISA spec; **not verified on native hardware** | — |

## Honest limitations

- A "0" means **not observed at ~1/total_reads sensitivity**, never proof of
  impossibility. The multi-socket runs reach billions of ordered reads each, so the
  sensitivity is well below 1 ppb on the topology that matters most.
- Tested: single-socket x86 + aarch64, and 2-socket x86 (both vendors). Not tested:
  >2-socket (4/8-socket) systems, non-AWS firmware, and aarch64 multi-socket (low
  risk — the ARM system counter is a single broadcast IP). The barrier mechanism is
  socket-count-independent, so these are coverage gaps, not known risks.
- riscv64 / loongarch64 **cannot be closed by a single hardware run** — a board that
  doesn't reorder in practice proves nothing about the ISA's implementation-defined
  fence-vs-CSR ordering. Honest status: supported and compiled, barrier emitted, but
  **not verified on native hardware** — use `std::time::Instant` there if you need a
  bench-proven guarantee.

## Bottom line

On every modern architecture we tested — Apple Silicon (M1/M4), AWS Graviton 3,
Intel x86 and AMD x86 (virtualized, single-socket bare-metal, Firecracker, and
2-socket NUMA bare-metal), Windows x86 — `OrderedInstant` does not go backward across
threads: **0 violations in ~10.9 billion reads**, with the bare-counter control
firing on every cell to prove each test was live, and with the fast comparison crates
inverting under the same conditions. The one topology that could have broken it —
multi-socket NUMA, where the bare counter inverts by tens of microseconds — is the
one that confirmed it.
