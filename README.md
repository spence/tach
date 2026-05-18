# tach

A replacement for `std::time::Instant` that reads the architectural counter directly: RDTSC on x86, CNTVCT_EL0 on aarch64, rdtime on riscv64 / loongarch64.

[![docs.rs](https://docs.rs/tach/badge.svg)](https://docs.rs/tach)
[![crates.io](https://img.shields.io/crates/v/tach.svg)](https://crates.io/crates/tach)

## usage

```rust
use tach::{Instant, MonotonicInstant, OrderedInstant};

// drop-in for std::time::Instant
let start = Instant::now();
let elapsed = start.elapsed();

// same API, sampled after prior Acquire loads
let ordered = OrderedInstant::now();
let elapsed = ordered.elapsed();

// same API, strict cross-thread monotonic by construction
let monotonic = MonotonicInstant::now();
let elapsed = monotonic.elapsed();
```

## benchmark

![benchmark](benches/summary-wide.png)

Methodology and per-target reports: [BENCHMARKS.md](BENCHMARKS.md).

## semantics

**Guaranteed monotonic timer.** `Instant::now()` returns strictly non-decreasing values within a thread, by design and by architectural spec. The counter is wall-clock-rate — keeps ticking through park, suspension, and descheduling — and every thread in the process reads the same source.

The guarantee rests on three things:

1. **Hardware**: every backing counter is documented monotonic non-decreasing in its primary spec — RDTSC (Intel SDM Vol 3B §17.17 "Invariant TSC"), CNTVCT_EL0 (ARMv8 ARM §D11.1.2, "must report the same value of the global counter" across cores), RISC-V `time` (Privileged Spec §10.1), LoongArch Stable Counter, Windows QPC, `clock_gettime(CLOCK_MONOTONIC)`, WASI `clockid::monotonic`, `Performance.now()`. Survey + citations in `BENCHMARKS.md`.
2. **Design**: `Instant` stores the raw counter tick (`u64`), not a converted nanosecond value. Ordering (`<`, `>`, `cmp`) is pure tick comparison; `duration_since` is non-negative tick delta × scale; `checked_duration_since` returns `None` on argument-swap. The frequency scaling is consulted only at observation time on a non-negative delta, so a frequency *estimate* changing across calls cannot make a younger `Instant` appear older.
3. **Empirical**: 0 backward jumps measured per-thread on every cell × clock × variant we test (`benches/skewmono-*.json`, 6 cells, billions of reads). Cross-thread, every cell sits at the hardware sync-slop floor (≤10 µs) — matching `std::time::Instant` on the same hardware. On Graviton 3, tach reads `cntvct_el0` directly and reliably beats `std` on cross-thread slop because direct register reads dodge vDSO call jitter (current run: tach 3.4 µs vs std 9.4 µs; an earlier run hit a chip with perfectly-synced counters and saw 0 ns vs std's 9.4 µs — the architectural spec aspires to perfect sync but real chips vary).

Untested cross-CCX (AMD Zen4) and multi-socket NUMA boundaries are outside the verification set. `std` doesn't help there either — it reads the same hardware counter through a slower path — so measure on your specific hardware if you correlate timestamps across those boundaries.

Cost difference on the read: tach's ~0.35 ns vs `std::time::Instant::now()`'s ~20 ns.

## ordered reads

A plain counter read can be reordered earlier than a preceding `Acquire` load:

```rust
let deadline = scheduler.load(Ordering::Acquire);
let now = tach::Instant::now();   // may be sampled before `deadline` is observed
```

`mrs cntvct_el0` is a system-register read; `rdtsc` is not a serializing instruction. Memory fences don't constrain when either executes. `OrderedInstant` emits the per-arch barrier (`isb sy` on aarch64, `rdtscp` on x86 — Intel SDM Vol 2B specifies that `rdtscp` "waits until all previous instructions have executed and all previous loads are globally visible") before / as the counter read, restoring the order:

```rust
let deadline = scheduler.load(Ordering::Acquire);
let now = tach::OrderedInstant::now();   // sampled after `deadline`
```

Cost is ~5–20 ns more than `Instant::now()`. `OrderedInstant::as_unordered()` downgrades to a plain `Instant` for storage; the reverse is not provided.

On riscv64 (`fence iorw, iorw`) and loongarch64 (`dbar 0`) the strongest available memory barrier is used; whether memory fences constrain CSR reads is implementation-defined on those targets, so the guarantee is best-effort.

## strict cross-thread monotonicity

Plain `Instant` and `OrderedInstant` are bounded by the underlying counter's cross-thread synchronization. On x86 the per-core TSC is firmware-synchronized but not architecturally so — a thread that migrates between cores can observe a TSC slightly behind what it just saw. On aarch64 the ARMv8 ARM specifies `cntvct_el0` as a single global counter, but empirically Apple Silicon and Graviton 3 both show real per-core slop in practice — tach's own strict-monotonicity test (load-then-now-then-check pattern; runnable with `cargo bench --bench skew` or as the `monotonic_strict_cross_thread` unit test) fails against bare counter reads on **every multi-threaded platform tested**: 17M / 144M reads on Apple Silicon, 1.8K / 382M reads on Graviton 3, 9.6M / 154M on Intel bare metal, similar non-zero numbers on every other cell. Architectural-guarantee-says-so is not a safe basis for shipping a "strict cross-thread monotonic" claim — only software enforcement makes the test pass on every cell.

`MonotonicInstant` enforces strict cross-thread monotonicity in software where it's empirically needed, and skips the enforcement where the platform's execution model already guarantees it:

```rust
let t1 = tach::MonotonicInstant::now();
// ... cross-thread work via channels, mutexes, atomics ...
let t2 = tach::MonotonicInstant::now();
assert!(t2 >= t1);   // always; no hardware-floor sync slop leaks through
```

Algorithm: every `now()` does the bare counter read; on platforms where the bare clock empirically fails the strict contract (every multi-threaded platform — x86, aarch64, RISC-V, LoongArch), the read is followed by `AtomicU64::fetch_max(tsc, AcqRel)` against a process-global last-seen tick. The fetch_max forces the return to be `>=` every previously published value. On wasm32 (single-threaded JS realm with W3C HRT strict-monotonic spec) and WASI (single-threaded execution model with strict-monotonic spec), the enforcement is skipped — `MonotonicInstant::now()` compiles to **the same instruction as `Instant::now()`**, free of cost.

Where enforcement applies, cost is ~+10–25 cycles per call uncontended (one LOCK CMPXCHG-class atomic on a hot cache line). Under heavy contention (many threads simultaneously hammering `now()`) it can degrade to 100+ ns per call as the cache line bounces between cores. Plain `Instant` stays untouched for callers who want the speed pitch and accept hardware-floor monotonicity.

This is the only timestamp in the comparison set (vs `std::time::Instant`, `quanta`, `minstant`, `fastant`) that offers strict cross-thread monotonicity by construction — `std::time::Instant::now()` on Unix is just `clock_gettime(CLOCK_MONOTONIC)` reading the same underlying counter, and `quanta::Instant::now()` reads bare RDTSC / `mrs cntvct_el0` with no software-side enforcement. The empirical evidence (see `BENCHMARKS.md`, "Strict cross-thread monotonicity (contract validation)") shows `std` happens to pass the test on every cell (the vDSO/syscall path serializes internally), while `quanta`, `minstant`, and `fastant` show non-zero contract violations on multiple cells.

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
| `tach::OrderedInstant` (default, `#![no_std]`) | 1.4 µs | 9.9 µs | 593.9 µs | 14.3 ms |
| `tach::MonotonicInstant` (default, `#![no_std]`) | 1.4 µs | 8.6 µs | 518.4 µs | 12.4 ms |
| `quanta::Instant` | 1.5 µs | 67.4 µs | 4.0 ms | 97.1 ms |
| `minstant::Instant` | 1.8 µs | 9.9 µs | 595.7 µs | 14.3 ms |
| `fastant::Instant` | 2.0 µs | 18.4 µs | 1.1 ms | 26.5 ms |
| `std::time::Instant` | 373 ns | 511 ns | 511 ns | 511 ns |

Numbers are cross-cell empirical medians measured on 6 platforms (Apple Silicon M1 MBP, AWS Graviton 3, AWS Intel t3.medium, AWS Intel m7i.metal-24xl bare-metal, AWS Lambda x86_64, GitHub Actions windows-2025). Per-cell breakdown and methodology in [BENCHMARKS.md](BENCHMARKS.md). On Intel x86 the architectural TSC frequency comes from CPUID leaf 15h when the host exposes it (Skylake+ Intel, Zen2+ AMD bare metal); on hosts that zero the leaf (Firecracker, Azure VMs, GitHub Windows runners) tach falls back to a 100 ms × 7-sample spin-loop calibration with hypervisor-preemption discard. On Linux aarch64 (Graviton 3 and similar) `cntfrq_el0` is firmware-published nominal — the underlying crystal can be 10–30 ppm off and the kernel never folds the NTP-corrected scaling factor back into it. Tach calibrates `cntvct_el0` against `clock_gettime(CLOCK_MONOTONIC)` at startup, which inherits the kernel's NTP-corrected vDSO scaling, so drift lands sub-ppm regardless of the underlying chip's crystal offset. Apple Silicon (macOS aarch64) reads `mach_timebase_info` directly — Apple measures the timebase per-die at manufacture, so no calibration is needed.

For long-running services that need wall-clock-correlated accuracy:

- **`tach::Instant::recalibrate()`** — manual, `#![no_std]`-compatible. Call from your own scheduler to re-derive scaling against the platform monotonic clock (`clock_gettime(CLOCK_MONOTONIC)` on Unix, `QueryPerformanceCounter` on Windows). Costs ~700 ms of spin-loop time per call (7 × 100 ms samples, preempted samples discarded). Works on every supported target including embedded and SGX.
- **`recalibrate-background` Cargo feature** — automatic. Spawns a background thread that re-measures the frequency every 60 seconds (configurable via `tach::set_recalibration_interval`) and EMA-blends the result into the cached scale (α ≈ 0.2 ≈ 5-sample averaging window), so a single noisy calibration window can't jolt the scale on virtualized hosts. **Requires `std`; incompatible with `#![no_std]` targets** (pulls in `std::thread` and `std::sync::OnceLock`). Active on every target that calibrates at startup: Intel x86 (Linux / Windows) and aarch64 Linux. Empirically improves drift where startup calibration accumulates error — AWS Lambda goes from 0.75 ppm baseline to 0.58 ppm with recal, m7i.metal-24xl bare metal goes from -3.25 ppm to -0.34 ppm. No-op on macOS (Apple writes the per-die timebase) and Windows aarch64 (`cntfrq_el0` is QPF-calibrated). On cells where startup calibration was already sub-ppm (t3.medium burst VM, c7g.4xlarge Graviton) the EMA's residual stays within noise of baseline.

Within a single process, two tach measurements are mutually consistent — drift only shows up when comparing against an external reference (NTP-disciplined wall clock, another process, etc.).

## non-goals

- Clock-skew correction across machines. This is a per-process counter.

## msrv

Rust 1.85.

## license

MIT OR Apache-2.0.
