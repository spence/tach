# tach

A replacement for `std::time::Instant` that reads the architectural counter directly: RDTSC on x86, CNTVCT_EL0 on aarch64, rdtime on riscv64 / loongarch64.

[![docs.rs](https://docs.rs/tach/badge.svg)](https://docs.rs/tach)
[![crates.io](https://img.shields.io/crates/v/tach.svg)](https://crates.io/crates/tach)

## usage

```rust
use tach::{Instant, SyncedInstant, FencedInstant};

// Default. Drop-in for std::time::Instant — fastest read
// (3.5–21 ns across platforms; 1.5–8× faster than std).
// Monotonic per-thread and in wall-clock order across
// threads. The right answer for almost all timing,
// tracing, profiling, and logging.
let start = Instant::now();
let elapsed = start.elapsed();

// Adds synchronization-order monotonicity. If Thread B's
// read happens-after Thread A's via Acquire/Release on
// shared state, B's value is guaranteed ≥ A's. Use for
// timestamps participating in happens-before edges:
// distributed-style tracing spans handed across threads,
// lock-free queues with monotonic-stamp invariants,
// lock-free version numbers keyed on timestamps.
let sync = SyncedInstant::now();
let elapsed = sync.elapsed();

// The read is sampled after any prior Acquire-loads in
// the SAME thread. Use when you have your own atomic
// synchronization and need a timestamp paired with it
// (e.g. "the time at which this thread observed flag X").
let fenced = FencedInstant::now();
let elapsed = fenced.elapsed();
```

## choosing the right type

| Use case | Reach for |
|---|---|
| Anything that doesn't fall into the rows below — timing, profiling, latency, request budgets, tracing spans on a single thread, independent per-thread logging | **`Instant`** |
| Cross-thread timestamps that participate in a happens-before relationship via shared state — tracing spans handed across threads, lock-free queues with monotonic-stamp invariants, lock-free version numbers, multi-thread merged logs that must be non-decreasing | **`SyncedInstant`** |
| Same-thread acquire-load correlation — you've just done `flag.load(Ordering::Acquire)` and need the next timestamp sampled *after* that load completes | **`FencedInstant`** |

`Instant` covers the multi-thread case for almost every real use case, because the underlying counter is shared and monotonic on every supported architecture. Two threads each reading `Instant::now()` see values that are monotonic in wall-clock order — what most users mean by "monotonic across threads." `SyncedInstant` is only needed when your code uses memory synchronization (Acquire/Release on shared atomics) to express a happens-before edge AND the timestamps participate in that edge. That's a narrower set than "anything multi-threaded."

Cost ordering, per-thread (no contention), as of 2026-05: **`Instant` < `SyncedInstant` < `FencedInstant`**. Counterintuitive — `SyncedInstant`'s LOCK fetch_max on an uncontended cache line is cheaper than `FencedInstant`'s pipeline-drain barrier — but consistent across every cell we measure.

**`FencedInstant` also provides synchronization-order monotonicity** as a structural consequence on x86 (`rdtscp` waits for prior loads to be globally visible — Intel SDM Vol 2B) and aarch64 (`isb sy` drains the pipeline before `mrs cntvct_el0`). On RISC-V and LoongArch it's best-effort (whether memory fences constrain CSR reads is implementation-defined). So if you happen to need both same-thread acquire-correlation and synchronization-order monotonicity on x86/aarch64, `FencedInstant` covers both. If you only need synchronization-order monotonicity, `SyncedInstant` is cheaper and portable across every ISA.

## benchmark

![benchmark](https://raw.githubusercontent.com/spence/tach/main/benches/summary.png)

Methodology and per-target reports: [BENCHMARKS.md](https://github.com/spence/tach/blob/main/BENCHMARKS.md).

## semantics

`Instant` is the fast read — `RDTSC` on x86, `mrs cntvct_el0` on aarch64, `rdtime` on RISC-V, `rdtime.d` on LoongArch, `Performance.now()` in wasm, `clock_gettime(CLOCK_MONOTONIC)` everywhere else. Wall-clock-rate, keeps ticking through park / suspension / descheduling. Same source across every thread in the process. The whole counter read is one instruction on every native target; no runtime dispatch.

**Per-thread monotonicity is hardware-guaranteed and empirically verified.** A thread reading `Instant::now()` in a tight loop sees a strictly non-decreasing sequence. Measured: **0 backward jumps across 7.6 billion bare `Instant` reads** on our 6-cell bench matrix ([per-cell `skewmono-*.json` files](https://github.com/spence/tach/tree/main/benches)).

**Cross-thread monotonicity in wall-clock order is hardware-guaranteed too.** Every supported architecture exposes a single shared system counter — `CNTVCT_EL0` per ARMv8 §D11.1.1 ("monotonically increasing"), invariant TSC per Intel SDM Vol 3 §17.17 on modern x86, `mtime` per RISC-V Priv Spec §3.1.10, and equivalents on LoongArch and the OS fallback paths. All cores read the same counter; the counter only moves forward. So any two reads from any two threads, compared in wall-clock order, return monotonically increasing values. This is what most users mean by "monotonic across threads," and `Instant` provides it on every supported platform.

What `Instant` does *not* provide is **synchronization-order monotonicity** — the guarantee that when Thread B's read happens-after Thread A's via a memory-ordering edge (Acquire-load on a value Thread A Released), B's timestamp is ≥ A's. The counter is monotonic, but the *read instruction* (`mrs cntvct_el0`, `rdtsc`) is not ordered against memory operations — the CPU can speculatively schedule it before a prior Acquire-load completes, sampling the counter at a wall-clock instant earlier than the synchronization point. In code that doesn't use cross-thread memory synchronization to enforce timestamp ordering, this never surfaces. In code that does, see `## synchronization-order monotonicity` below.

Cost on the read: 3.5–21 ns across our test matrix vs `std::time::Instant::now()`'s ~20–60 ns (1.5–8× faster). Full per-platform breakdown in `## performance` below.

## synchronization-order monotonicity

Bare `Instant` is monotonic per-thread and in wall-clock order across threads (see `## semantics`). What it does *not* guarantee is that a timestamp on Thread B, read after a synchronization edge from Thread A, will be ≥ Thread A's published timestamp. Concretely:

```rust
// Thread A:
let stamp = Instant::now();
queue.push(Item { data, stamp });          // Release-publish, synchronizes-with the pop

// Thread B:
let item = queue.pop();                    // Acquire-load
let arrival = Instant::now();
let latency = arrival - item.stamp;        // expected: latency >= 0
```

Under the C++ memory model, Thread B's `now()` happens-after Thread A's `now()` via the queue's Acquire/Release pair. But the hardware-level read instruction (`mrs cntvct_el0`, `rdtsc`) isn't ordered against memory operations — the CPU can speculatively issue Thread B's read at a wall-clock instant *before* the Acquire-load retires, sampling the counter earlier than the synchronization edge would suggest. Result: `arrival < stamp` is possible, even though the counter never moved backward. The window is small (tens to hundreds of ns on aarch64, up to ~µs on x86) but real — empirically: 17M+ violations on Apple Silicon M1 under the canonical load-then-now-then-check protocol (`measure_synchronization_order`). See `BENCHMARKS.md` "Synchronization-order monotonicity (contract validation)" for the per-cell × per-clock data.

`SyncedInstant` closes this gap by routing every read through an `AtomicU64::fetch_max` against a process-global last-seen tick:

```rust
let stamp = tach::SyncedInstant::now();   // publishes stamp via fetch_max
// ... cross-thread work via channels, mutexes, atomics ...
let arrival = tach::SyncedInstant::now(); // observes >= stamp via fetch_max
assert!(arrival >= stamp);                       // always; by construction
```

**Algorithm**: every `now()` does the bare counter read followed by `AtomicU64::fetch_max(tsc, AcqRel)` against a process-global last-seen tick. The `fetch_max` is ordered against itself across threads (the modification order on the atomic is total), and its value is monotonically non-decreasing by construction. So Thread B's read returns ≥ every value previously published by any thread, including Thread A's. On wasm32 (single-threaded JS realm with W3C HRT strict-monotonic spec) and WASI (single-threaded execution model with strict-monotonic spec), the enforcement is skipped — `SyncedInstant::now()` compiles to **the same instruction as `Instant::now()`**, free of cost.

**Cost where enforcement applies**: ~+2–14 ns per call uncontended (one LOCK CMPXCHG-class atomic on a hot cache line). Under heavy contention (many threads simultaneously hammering `now()`) it can degrade to 100+ ns per call as the cache line bounces between cores. Plain `Instant` stays untouched for callers who don't need synchronization-order monotonicity.

**Counterintuitive performance note**: on every platform we measure, `SyncedInstant::now()` is FASTER than `FencedInstant::now()` per-thread (e.g. Apple Silicon: 7.0 ns vs 18.5 ns; Windows: 10.7 ns vs 25.4 ns). The pipeline-drain barrier `FencedInstant` uses costs more than a LOCK fetch_max on an uncontended cache line. Atomics-should-be-expensive is the wrong intuition for this regime.

**Comparison crates**: `std::time::Instant`, `quanta`, `minstant`, `fastant` all read the same underlying hardware counter (or a kernel-mediated wrapper thereof). On the synchronization-order test (`BENCHMARKS.md`), `std` happens to pass on every cell (the vDSO/syscall path serializes the read internally); `quanta` fails on every cell (bare counter read); `minstant` and `fastant` show mixed results by platform. Notably, `fastant` falls back to wall-clock `SystemTime` on non-Linux targets — this is structurally non-monotonic (NTP corrections can move it backward) and would break tracing-style code on macOS / Windows. `SyncedInstant` is the only type in the comparison set that gives synchronization-order monotonicity by construction across every supported architecture.

## fenced reads

A plain counter read can be reordered earlier than a preceding `Acquire` load in the same thread:

```rust
let deadline = scheduler.load(Ordering::Acquire);
let now = tach::Instant::now();   // may be sampled before `deadline` is observed
```

`mrs cntvct_el0` is a system-register read; `rdtsc` is not a serializing instruction. Memory fences don't constrain when either executes. `FencedInstant` emits the per-arch barrier (`isb sy` on aarch64, `rdtscp` on x86 — Intel SDM Vol 2B specifies that `rdtscp` "waits until all previous instructions have executed and all previous loads are globally visible") before / as the counter read, restoring the order:

```rust
let deadline = scheduler.load(Ordering::Acquire);
let now = tach::FencedInstant::now();   // sampled after `deadline`
```

Cost is ~5–20 ns more than `Instant::now()`. `FencedInstant::as_unfenced()` downgrades to a plain `Instant` for storage; the reverse is not provided.

On riscv64 (`fence iorw, iorw`) and loongarch64 (`dbar 0`) the strongest available memory barrier is used; whether memory fences constrain CSR reads is implementation-defined on those targets, so the guarantee is best-effort.

**Bonus on x86 / aarch64: synchronization-order monotonicity by construction.** Because `rdtscp` waits for prior loads to be globally visible and `isb sy` drains the pipeline before `mrs`, a `FencedInstant::now()` reading after an Acquire-load is structurally ordered after that load — which closes the same window `SyncedInstant` closes via `fetch_max`. So on x86 and aarch64, `FencedInstant` also satisfies synchronization-order monotonicity. Empirically confirmed: 0 violations on every cell tested. On RISC-V / LoongArch this remains best-effort (fence semantics over CSR reads aren't pinned down by the ISA). If you specifically need synchronization-order monotonicity portable across every ISA we support, `SyncedInstant` is the explicit guarantee (and cheaper); if you happen to need both same-thread acquire-correlation and synchronization-order monotonicity on x86/aarch64, `FencedInstant` covers both.

## platform support

| Platform / target               | `Instant` clock                  |
|---------------------------------|----------------------------------|
| Linux (x86_64)                  | RDTSC                            |
| Linux (x86)                     | RDTSC                            |
| Linux (aarch64)                 | CNTVCT_EL0                       |
| Linux (riscv64)                 | rdtime                           |
| Linux (loongarch64)             | rdtime.d                         |
| macOS (aarch64)                 | CNTVCT_EL0                       |
| macOS (x86_64)                  | RDTSC                            |
| Windows (x86_64)                | RDTSC                            |
| Windows (aarch64)               | CNTVCT_EL0                       |
| wasm32 (browser / Node host)    | `Performance.now()`              |
| WASI (wasm32-wasip{1,2})        | `clock_time_get(MONOTONIC)`      |
| Unix / other                    | `clock_gettime(CLOCK_MONOTONIC)` |

The crate is `#![no_std]`. `wasm-bindgen` is the only dependency, pulled in only for `wasm32-unknown-unknown` and `wasm32v1-none` (the targets that go through `Performance.now()`).

## drift

`elapsed()` can diverge from true wall-clock time over long intervals. Drift is *per-interval* — a 1-minute measurement made 5 seconds into the process has the same drift as one made 100 days in. Numbers below assume room-temperature operation; rows marked kernel-corrected assume no NTP, with active discipline they drop another order of magnitude.

| Crate | 1-sec interval | 1-min interval | 1-hr interval | 1-day interval |
|---|---|---|---|---|
| `tach::Instant` (default, `#![no_std]`) | 1.6 µs | 11.1 µs | 668.9 µs | 16.1 ms |
| `tach::Instant` + `recalibrate-background` (**requires `std`**) | 1.4 µs | 13.9 µs | 13.9 µs | 13.9 µs |
| `tach::FencedInstant` (default, `#![no_std]`) | 1.4 µs | 9.9 µs | 593.9 µs | 14.3 ms |
| `tach::SyncedInstant` (default, `#![no_std]`) | 1.4 µs | 8.6 µs | 518.4 µs | 12.4 ms |
| `quanta::Instant` | 1.5 µs | 67.4 µs | 4.0 ms | 97.1 ms |
| `minstant::Instant` | 1.8 µs | 9.9 µs | 595.7 µs | 14.3 ms |
| `fastant::Instant` | 2.0 µs | 18.4 µs | 1.1 ms | 26.5 ms |
| `std::time::Instant` | 373 ns | 511 ns | 511 ns | 511 ns |

Numbers are cross-cell empirical medians measured on 6 platforms (Apple Silicon M1 MBP, AWS Graviton 3, AWS Intel t3.medium, AWS Intel m7i.metal-24xl bare-metal, AWS Lambda x86_64, GitHub Actions windows-2025). Per-cell breakdown and methodology in [BENCHMARKS.md](https://github.com/spence/tach/blob/main/BENCHMARKS.md). On Intel x86 the architectural TSC frequency comes from CPUID leaf 15h when the host exposes it (Skylake+ Intel, Zen2+ AMD bare metal); on hosts that zero the leaf (Firecracker, Azure VMs, GitHub Windows runners) tach falls back to a 100 ms × 7-sample spin-loop calibration with hypervisor-preemption discard. On Linux aarch64 (Graviton 3 and similar) `cntfrq_el0` is firmware-published nominal — the underlying crystal can be 10–30 ppm off and the kernel never folds the NTP-corrected scaling factor back into it. Tach calibrates `cntvct_el0` against `clock_gettime(CLOCK_MONOTONIC)` at startup, which inherits the kernel's NTP-corrected vDSO scaling, so drift lands sub-ppm regardless of the underlying chip's crystal offset. Apple Silicon (macOS aarch64) reads `mach_timebase_info` directly — Apple measures the timebase per-die at manufacture, so no calibration is needed.

For long-running services that need wall-clock-correlated accuracy:

- **`tach::Instant::recalibrate()`** — manual, `#![no_std]`-compatible. Call from your own scheduler to re-derive scaling against the platform monotonic clock (`clock_gettime(CLOCK_MONOTONIC)` on Unix, `QueryPerformanceCounter` on Windows). Costs ~700 ms of spin-loop time per call (7 × 100 ms samples, preempted samples discarded). Works on every supported target including embedded and SGX.
- **`recalibrate-background` Cargo feature** — automatic. Spawns a background thread that re-measures the frequency every 60 seconds (configurable via `tach::set_recalibration_interval`) and EMA-blends the result into the cached scale (α ≈ 0.2 ≈ 5-sample averaging window), so a single noisy calibration window can't jolt the scale on virtualized hosts. **Requires `std`; incompatible with `#![no_std]` targets** (pulls in `std::thread` and `std::sync::OnceLock`). Active on every target that calibrates at startup: Intel x86 (Linux / Windows) and aarch64 Linux. Empirically improves drift where startup calibration accumulates error — AWS Lambda goes from 0.75 ppm baseline to 0.58 ppm with recal, m7i.metal-24xl bare metal goes from -3.25 ppm to -0.34 ppm. No-op on macOS (Apple writes the per-die timebase) and Windows aarch64 (`cntfrq_el0` is QPF-calibrated). On cells where startup calibration was already sub-ppm (t3.medium burst VM, c7g.4xlarge Graviton) the EMA's residual stays within noise of baseline.

Within a single process, two tach measurements are mutually consistent — drift only shows up when comparing against an external reference (NTP-disciplined wall clock, another process, etc.).

## performance

Per-thread call cost across our 6-cell bench matrix (single-thread tight loop, no contention):

| Platform | `Instant` | `SyncedInstant` | `FencedInstant` | `std::Instant` |
|---|---|---|---|---|
| Apple Silicon M1 (aarch64 macOS) | **3.5 ns** | 7.0 ns | 18.5 ns | 28.0 ns |
| Graviton 3 (aarch64 Linux, `c7g.4xlarge`) | **7.3 ns** | 10.6 ns | 26.1 ns | 37.7 ns |
| Intel Nitro VM (x86 Linux, `t3.medium`) | **14.4 ns** | 25.4 ns | 27.1 ns | 36.3 ns |
| Intel bare metal (x86 Linux, `m7i.metal-24xl`) | **8.5 ns** | 14.4 ns | 16.8 ns | 19.5 ns |
| AWS Lambda Firecracker (x86) | **21.2 ns** | 35.7 ns | 40.0 ns | 59.5 ns |
| Windows Server 2025 (x86) | **9.6 ns** | 10.7 ns | 25.4 ns | 36.3 ns |

`Instant` is **1.5–8× faster than `std::time::Instant`** on every platform. `SyncedInstant` adds 2–14 ns over `Instant`; `FencedInstant` adds 8–19 ns. Notably `SyncedInstant` is **faster than `FencedInstant`** on every cell — the LOCK fetch_max on an uncontended cache line is cheaper than the pipeline-drain barrier `FencedInstant` uses. Counterintuitive but consistent.

These are uncontended per-thread costs. Under heavy contention (many threads simultaneously hammering `now()`), `SyncedInstant`'s fetch_max can degrade to 100+ ns as the cache line bounces between cores; `Instant` and `FencedInstant` keep their per-thread cost regardless of contention. If your use case has hundreds of threads emitting timestamps in tight loops, plain `Instant` is the right choice for the hot path; reserve `SyncedInstant` for cases where synchronization-order monotonicity (timestamps participating in happens-before edges) is load-bearing.

Source: [per-cell `skewmono-*.json` files](https://github.com/spence/tach/tree/main/benches) (regenerable via `bash benches/run-skewmono-aws.sh <cell> <instance-type>`).

## non-goals

- Clock-skew correction across machines. This is a per-process counter.

## msrv

Rust 1.85.

## license

MIT OR Apache-2.0.
