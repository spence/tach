[![docs.rs](https://docs.rs/tach/badge.svg)](https://docs.rs/tach)
[![crates.io](https://img.shields.io/crates/v/tach.svg)](https://crates.io/crates/tach)

# tach

**The fastest `Instant` for every architecture — and the only one that stays correct across threads.**

`tach::Instant` reads the CPU's architectural counter directly (`RDTSC` on x86,
`CNTVCT_EL0` on aarch64, `rdtime` on RISC-V / LoongArch) — a few nanoseconds,
versus the ~20–60 ns a syscall/vDSO-backed `std::time::Instant` costs. Two types,
one decision:

- **`Instant`** — the fastest timestamp. Monotonic on a thread, and in wall-clock
  order across threads. For timing, profiling, latency, local elapsed — anything
  not compared against another thread's timestamp through shared state.
- **`OrderedInstant`** — the same read, *ordered against memory*. **Monotonic
  across threads by construction**, verified at **0 backward steps in ~10.9 billion
  reads** across Apple Silicon, AWS Graviton 3, Intel and AMD x86 (single- and
  dual-socket NUMA). For the moment a timestamp crosses a thread boundary and gets
  compared or ordered — trace timelines, timestamp-as-version, a deadline read
  after an `Acquire`-load.

> If a timestamp ever leaves the thread that made it and gets compared, reach for
> `OrderedInstant`. Otherwise `Instant`.

## usage

```rust
use tach::{Instant, OrderedInstant};

// Fastest read. Use for local timing — start and stop on one thread.
let start = Instant::now();
// ... work ...
let elapsed = start.elapsed();

// Ordered read. Monotonic across threads: a timestamp taken after observing
// another thread's published OrderedInstant is never smaller than it.
let t = OrderedInstant::now();
let elapsed = t.elapsed();
```

## the ecosystem gap

Every fast Rust clock reads the architectural counter bare for speed. That makes
the read reorderable: an out-of-order CPU can sample it *before* a prior
`Acquire`-load completes, so two threads' timestamps can invert across a
happens-before edge. `std::time::Instant` avoids this only by going through the
kernel — which is why it is slow. **`tach::OrderedInstant` is the only one that is
both fast and cross-thread-correct.**

| Crate | Fast read (no syscall) | Cross-thread ordered |
|---|:---:|:---:|
| `tach::Instant` | ✓ 3.5–21 ns | ✗ *(by design — bare read)* |
| **`tach::OrderedInstant`** | **✓ ordered read** | **✓ 0 / 10.9B verified** |
| `quanta::Instant` | ✓ | ✗ inverts on every cell tested |
| `minstant::Instant` | ✓ | ✗ inverts on bare-metal x86 |
| `fastant::Instant` | ✓ | ✗ inverts on bare-metal x86 |
| `std::time::Instant` | ✗ ~20–60 ns | ✓ *(but slow)* |

This is measured, not asserted. On a 2-socket AMD EPYC box, under identical
cross-socket conditions, `quanta`, `minstant`, and `fastant` each inverted **over
1.5 billion times** while `tach::OrderedInstant` inverted **zero** times — same
hardware, same threads, same instant. The fast crates fall back to `std` on aarch64
(and pass there, by not actually being fast), and none of the three exposes an
*ordered* read variant — there is no knob to make them correct on x86. Full
side-by-side per-cell numbers in
[benches/ORDERED-VERIFICATION.md](benches/ORDERED-VERIFICATION.md) and
[BENCHMARKS.md](BENCHMARKS.md).

## why OrderedInstant exists: one hazard, two faces

`Instant::now()` is one counter-read instruction. It is *not* a memory operation,
and an out-of-order CPU can sample it before a prior `Acquire`-load completes. That
single fact has two consequences:

```rust
// Same thread — the read can land before the load you meant to time after:
let flag = ready.load(Ordering::Acquire);
let now = tach::Instant::now();   // may be sampled BEFORE `ready` was observed

// Across threads — the read can land before the load that joins you to another
// thread, inverting two timestamps across a happens-before edge.
```

`OrderedInstant` emits the arch barrier that pins the read *after* prior memory
operations, closing both faces at once. From that one property you get same-thread
acquire-correlation *and* strict cross-thread monotonicity:

```rust
let flag = ready.load(Ordering::Acquire);
let now = tach::OrderedInstant::now();   // sampled after `ready` — safe to compare
```

`Instant` lacks the barrier on purpose: most timestamps never cross threads, and the
bare read is the fastest the hardware allows.

## the guarantee, precisely

> A timestamp taken on any thread, after an `Acquire`-load that observed another
> thread's published `OrderedInstant`, is guaranteed `>=` that published value.

This is the **load-then-now-then-check** contract. We verified it directly: **0
violations in ~10.9 billion reads** across x86 and aarch64, including 2-socket Intel
(Xeon 8488C) and AMD (EPYC 9R14) NUMA boxes where a bare `Instant` inverts from
sub-ppm (Nitro VMs) to ~12% of cross-thread reads (Apple Silicon). On the
multi-socket boxes the bare counter went backward by up to ~97 µs under contention;
`OrderedInstant` held at 0 throughout. Full methodology and per-cell data in
[benches/ORDERED-VERIFICATION.md](benches/ORDERED-VERIFICATION.md).

The barrier is `rdtscp` on x86 (Intel SDM: "waits until all previous instructions
have executed and all previous loads are globally visible") and `isb sy` before
`mrs cntvct_el0` on aarch64.

**riscv64 and loongarch64** are supported and compiled, and `OrderedInstant` emits
the strongest ordering barrier each ISA offers (`fence iorw, iorw` / `dbar 0`). But
whether a memory fence constrains the time-CSR read is implementation-defined in
those specs, and tach's cross-thread ordering is **not yet verified on native
RISC-V / LoongArch hardware** — use `std::time::Instant` there if you need a
bench-proven guarantee today.

## the one assumption

tach assumes a coherent, monotonic hardware counter. On a host where the OS marks
the TSC clocksource unstable (genuinely desynchronized cores — exotic or broken
multi-socket firmware), tach is off-contract for *every* type; use
`std::time::Instant` on such hosts. This is the same assumption every fast TSC clock
makes, stated plainly.

## the process-clock idiom

A common need is "microseconds since process start," correct across every thread.
The canonical pattern:

```rust
use std::sync::LazyLock;
use tach::OrderedInstant;

static CLOCK_START: LazyLock<OrderedInstant> = LazyLock::new(OrderedInstant::now);

pub fn wall_time_us() -> u64 {
    CLOCK_START.elapsed().as_micros() as u64
}
```

`LazyLock` gives a single shared process epoch initialized once; `OrderedInstant`
gives the cross-thread ordering. Every `elapsed()` does an *ordered* `now()` read
internally, so process-relative timestamps stay correct no matter which thread asks.

## performance

Per-thread call cost (single-thread tight loop, no contention):

| Platform | `Instant` | `OrderedInstant` | `std::Instant` |
|---|---|---|---|
| Apple Silicon M1 (aarch64 macOS) | **3.5 ns** | 18.5 ns | 28.0 ns |
| Graviton 3 (aarch64 Linux, `c7g.4xlarge`) | **7.3 ns** | 26.1 ns | 37.7 ns |
| Intel Nitro VM (x86 Linux, `t3.medium`) | **14.4 ns** | 27.1 ns | 36.3 ns |
| Intel bare metal (x86 Linux, `m7i.metal-24xl`) | **8.5 ns** | 16.8 ns | 19.5 ns |
| AWS Lambda Firecracker (x86) | **21.2 ns** | 40.0 ns | 59.5 ns |
| Windows Server 2025 (x86) | **9.6 ns** | 25.4 ns | 36.3 ns |

`Instant` is **1.5–8× faster than `std::time::Instant`**; `OrderedInstant` is still
1.5–4× faster than std while adding the cross-thread guarantee std reaches only via
the slow path. Crucially, `OrderedInstant` holds **no shared state**, so its
per-call cost is flat regardless of thread count — there is no contention cliff (see
*why not an atomic?* below).

Cross-cell empirical medians; methodology and per-cell breakdown in
[BENCHMARKS.md](BENCHMARKS.md).

## platform support

| Platform / target               | `Instant` clock                  |
|---------------------------------|----------------------------------|
| Linux (x86_64)                  | RDTSC                            |
| Linux (x86)                     | RDTSC                            |
| Linux (aarch64)                 | CNTVCT_EL0                       |
| Linux (riscv64)                 | rdtime †                        |
| Linux (loongarch64)             | rdtime.d †                      |
| macOS (aarch64)                 | CNTVCT_EL0                       |
| macOS (x86_64)                  | RDTSC                            |
| Windows (x86_64)                | RDTSC                            |
| Windows (aarch64)               | CNTVCT_EL0                       |
| wasm32 (browser / Node host)    | `Performance.now()`              |
| WASI (wasm32-wasip{1,2})        | `clock_time_get(MONOTONIC)`      |
| Unix / other                    | `clock_gettime(CLOCK_MONOTONIC)` |

† riscv64 / loongarch64: supported and compiled; `OrderedInstant`'s cross-thread
ordering is best-effort by ISA spec and not yet verified on native hardware (see
*the guarantee, precisely*).

The crate is `#![no_std]`. `wasm-bindgen` is the only dependency, pulled in only for
`wasm32-unknown-unknown` and `wasm32v1-none` (the targets that go through
`Performance.now()`).

## drift

`elapsed()` can diverge from true wall-clock time over long intervals. Drift is
*per-interval* — a 1-minute measurement made 5 seconds into the process has the same
drift as one made 100 days in. Numbers below assume room-temperature operation.

| Crate | 1-sec | 1-min | 1-hr | 1-day |
|---|---|---|---|---|
| `tach::Instant` (default, `#![no_std]`) | 1.6 µs | 11.1 µs | 668.9 µs | 16.1 ms |
| `tach::Instant` + `recalibrate-background` (**requires `std`**) | 1.4 µs | 13.9 µs | 13.9 µs | 13.9 µs |
| `tach::OrderedInstant` (default, `#![no_std]`) | 1.4 µs | 9.9 µs | 593.9 µs | 14.3 ms |
| `quanta::Instant` | 1.5 µs | 67.4 µs | 4.0 ms | 97.1 ms |
| `minstant::Instant` | 1.8 µs | 9.9 µs | 595.7 µs | 14.3 ms |
| `fastant::Instant` | 2.0 µs | 18.4 µs | 1.1 ms | 26.5 ms |
| `std::time::Instant` | 373 ns | 511 ns | 511 ns | 511 ns |

For long-running services that need wall-clock-correlated accuracy:

- **`tach::Instant::recalibrate()`** — manual, `#![no_std]`-compatible. Re-derives
  scaling against the platform monotonic clock. Works on every supported target.
- **`recalibrate-background` Cargo feature** — spawns a thread that re-measures the
  frequency every 60 s (configurable via `tach::set_recalibration_interval`) and
  EMA-blends it in. **Requires `std`.**

Methodology and per-cell breakdown in [BENCHMARKS.md](BENCHMARKS.md).

## why not just wrap it in an atomic?

A reasonable skeptic: *`OrderedInstant` trusts a CPU barrier — why not guarantee
monotonicity the obvious way, with a process-global `AtomicU64::fetch_max` on every
read?*

We built exactly that and measured both head-to-head. It buys **zero correctness**:
both the atomic and the barrier hit 0 cross-thread violations across ~10.9 billion
reads. And it is strictly worse on cost — a LOCKed atomic on every call, plus a
single shared cache line that collapses under contention (the atomic stops scaling
at 2 threads and is slower than `std` beyond that; the barrier holds flat per-call
cost at any thread count because it has no shared state). The barrier is as correct,
faster, and scales. Full data in [docs/WHY-NOT-AN-ATOMIC.md](docs/WHY-NOT-AN-ATOMIC.md).

## non-goals

- Clock-skew correction across machines. This is a per-process counter.
- Per-thread CPU time, calendar/wall time, sleeping/timers — use the OS, `std`, or
  your runtime for those. tach measures fast monotonic elapsed time, nothing else.

## msrv

Rust 1.85.

## license

MIT OR Apache-2.0.
