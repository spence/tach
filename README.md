# tach

A replacement for `std::time::Instant` that reads the architectural counter directly: RDTSC on x86, CNTVCT_EL0 on aarch64, rdtime on riscv64 / loongarch64.

[![docs.rs](https://docs.rs/tach/badge.svg)](https://docs.rs/tach)
[![crates.io](https://img.shields.io/crates/v/tach.svg)](https://crates.io/crates/tach)

## usage

```rust
use tach::{Instant, OrderedInstant};

// drop-in for std::time::Instant
let start = Instant::now();
let elapsed = start.elapsed();

// same API, sampled after prior Acquire loads
let ordered = OrderedInstant::now();
let elapsed = ordered.elapsed();
```

## benchmark

![benchmark](benches/summary-wide.png)

Methodology and per-target reports: [BENCHMARKS.md](BENCHMARKS.md).

## semantics

The counter is wall-clock-rate. It keeps ticking through park, suspension, and descheduling. All threads in the process read the same source. Per-thread monotonicity is verified empirically (0 backward jumps over billions of consecutive reads on every benchmark cell). Cross-thread observation consistency is measured at ≤10 µs across all tested cells — matching `std::time::Instant` within measurement noise on the same hardware. AMD Zen4 CCX boundaries are not in the tested set; if you correlate timestamps across CCXes or sockets, prefer `std::time::Instant`. Cost difference: tach's counter read is ~0.35 ns vs `std::time::Instant::now()`'s ~20 ns.

## ordered reads

A plain counter read can be reordered earlier than a preceding `Acquire` load:

```rust
let deadline = scheduler.load(Ordering::Acquire);
let now = tach::Instant::now();   // may be sampled before `deadline` is observed
```

`mrs cntvct_el0` is a system-register read; `rdtsc` is not a serializing instruction. Memory fences don't constrain when either executes. `OrderedInstant` emits the per-arch barrier (`isb sy` on aarch64, `lfence` on x86) before the counter read, restoring the order:

```rust
let deadline = scheduler.load(Ordering::Acquire);
let now = tach::OrderedInstant::now();   // sampled after `deadline`
```

Cost is ~5–20 ns more than `Instant::now()`. `OrderedInstant::as_unordered()` downgrades to a plain `Instant` for storage; the reverse is not provided.

On riscv64 (`fence iorw, iorw`) and loongarch64 (`dbar 0`) the strongest available memory barrier is used; whether memory fences constrain CSR reads is implementation-defined on those targets, so the guarantee is best-effort.

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
| `tach::Instant` (default, `#![no_std]`) | 1.2 µs | 13.1 µs | 785.8 µs | 18.9 ms |
| `tach::Instant` + `recalibrate-background` (**requires `std`**) | 934 ns | 24.1 µs | 24.1 µs | 24.1 µs |
| `tach::OrderedInstant` (default, `#![no_std]`) | 1.6 µs | 10.3 µs | 615.5 µs | 14.8 ms |
| `quanta::Instant` | 6.3 µs | 411.6 µs | 24.7 ms | 592.7 ms |
| `minstant::Instant` | 2.3 µs | 37.9 µs | 2.3 ms | 54.5 ms |
| `fastant::Instant` | 2.2 µs | 27.8 µs | 1.7 ms | 40.0 ms |
| `std::time::Instant` | 346 ns | 457 ns | 457 ns | 457 ns |

Numbers are cross-cell empirical medians measured on 6 platforms (Apple Silicon M1 MBP, AWS Graviton 3, AWS Intel t3.medium, AWS Intel m7i.metal-24xl bare-metal, AWS Lambda x86_64, GitHub Actions windows-2025). Per-cell breakdown and methodology in [BENCHMARKS.md](BENCHMARKS.md). On Intel x86 the architectural TSC frequency comes from CPUID leaf 15h when the host exposes it (Skylake+ Intel, Zen2+ AMD bare metal); on hosts that zero the leaf (Firecracker, Azure VMs, GitHub Windows runners) tach falls back to a 100 ms × 7-sample spin-loop calibration with hypervisor-preemption discard. On Linux aarch64 (Graviton 3 and similar) `cntfrq_el0` is firmware-published nominal — the underlying crystal can be 10–30 ppm off and the kernel never folds the NTP-corrected scaling factor back into it. Tach calibrates `cntvct_el0` against `clock_gettime(CLOCK_MONOTONIC)` at startup, which inherits the kernel's NTP-corrected vDSO scaling, so drift lands sub-ppm regardless of the underlying chip's crystal offset. Apple Silicon (macOS aarch64) reads `mach_timebase_info` directly — Apple measures the timebase per-die at manufacture, so no calibration is needed.

For long-running services that need wall-clock-correlated accuracy:

- **`tach::Instant::recalibrate()`** — manual, `#![no_std]`-compatible. Call from your own scheduler to re-derive scaling against the platform monotonic clock (`clock_gettime(CLOCK_MONOTONIC)` on Unix, `QueryPerformanceCounter` on Windows). Costs ~700 ms of spin-loop time per call (7 × 100 ms samples, preempted samples discarded). Works on every supported target including embedded and SGX.
- **`recalibrate-background` Cargo feature** — automatic. Spawns a background thread that re-measures the frequency every 60 seconds (configurable via `tach::set_recalibration_interval`) and EMA-blends the result into the cached scale (α ≈ 0.2 ≈ 5-sample averaging window), so a single noisy calibration window can't jolt the scale on virtualized hosts. **Requires `std`; incompatible with `#![no_std]` targets** (pulls in `std::thread` and `std::sync::OnceLock`). Active on every target that calibrates at startup: Intel x86 (Linux / Windows) and aarch64 Linux. Empirically improves drift where startup calibration accumulates error — AWS Lambda goes from 0.75 ppm baseline to 0.58 ppm with recal, m7i.metal-24xl bare metal goes from -3.25 ppm to -0.34 ppm. No-op on macOS (Apple writes the per-die timebase) and Windows aarch64 (`cntfrq_el0` is QPF-calibrated). On cells where startup calibration was already sub-ppm (t3.medium burst VM, c7g.4xlarge Graviton) the EMA's residual stays within noise of baseline.

Within a single process, two tach measurements are mutually consistent — drift only shows up when comparing against an external reference (NTP-disciplined wall clock, another process, etc.).

## non-goals

- Strict cross-thread monotonicity. Use `std::time::Instant`.
- Clock-skew correction across machines. This is a per-process counter.

## msrv

Rust 1.85.

## license

MIT OR Apache-2.0.
