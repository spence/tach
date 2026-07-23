# tach

[![docs.rs](https://docs.rs/tach/badge.svg)](https://docs.rs/tach)
[![crates.io](https://img.shields.io/crates/v/tach.svg)](https://crates.io/crates/tach)

`tach` provides three Instant-shaped timers for three timing contracts. On four primary
environments, each has the fastest tested steady-state read and elapsed bracket among providers
eligible for its contract, treating differences within `max(1 ns, 5%)` as a material tie. A
separate 24-target build matrix proves API availability and provider routing, not speed on
unbenchmarked hardware.

All three APIs compute elapsed time as `Duration`. What changes is the quantity that advances
and where two samples may be compared safely.

| You're timing | Reach for | Instead of | Contract, and how tach compares |
|---|---|---|---|
| An elapsed bracket on one thread | `Instant` | `quanta`, `std::time::Instant` | Wall-rate, endpoints stay local. Fastest tested: 0.65 ns on Apple; ties or beats `quanta` on every primary cell. |
| Timestamps compared across threads | `GlobalInstant` | `std::time::Instant` | Same elapsed domain, ordered across a happens-before edge. 1.2–2.6× faster than `std`, and still cross-core-safe. |
| A thread's own CPU time | `ThreadCpuInstant` | `clock_gettime` / `GetThreadTimes` | Scheduled CPU time, or an explicit wall fallback. As fast as the OS call; up to 4.6× on Linux. |

That is the mental model: two elapsed-time clocks with different ordering contracts, plus one
thread-time clock. They share an API shape and units, not a time domain.

And the same three types as a matrix of guarantees — what each contract promises, and what it
deliberately doesn't:

| Guarantee | `Instant` | `GlobalInstant` | `ThreadCpuInstant` |
|---|---|---|---|
| Measures | Wall-clock elapsed | Wall-clock elapsed | Thread CPU time † |
| Advances while the thread is parked or descheduled | Yes | Yes | No — freezes |
| `elapsed()` is monotonic and never negative | Yes | Yes | Yes |
| Two samples comparable across threads | No — same-thread only | Yes — cross-core value-consistent | No — `!Send` by design |
| Read ordered after a prior `Acquire` (happens-before edge) | No | Yes | n/a |
| `Send + Sync` | Yes | Yes | No — neither |

† Native thread CPU time on every OS target: Linux, macOS, Windows, Android, and FreeBSD, across
all architectures. The monotonic-wall fallback is confined to WASM/WASI hosts that expose no
per-thread clock (browsers; Node uses native thread CPU), never a silent substitution. Check
`ThreadCpuInstant::measures_thread_cpu_time()`.

## Usage

```rust
use tach::{Instant, GlobalInstant, ThreadCpuInstant};

// Wall time for a bracket whose endpoints remain on this thread.
let local_start = Instant::now();
do_local_work();
println!("local elapsed: {:?}", local_start.elapsed());

// Use ordered samples when timestamps participate in cross-thread synchronization.
let published = GlobalInstant::now();
publish_to_another_thread(published);

// CPU delivered to this OS thread, excluding sleep and descheduling.
let cpu_start = ThreadCpuInstant::now();
do_cpu_work();
if cpu_start.measures_thread_cpu_time() {
  println!("thread CPU: {:?}", cpu_start.elapsed());
}
```

`Instant` and `GlobalInstant` are `Send + Sync`. `ThreadCpuInstant` is deliberately neither:
moving a sample would make it too easy to compare unrelated thread-CPU timelines.

## Fastest tested for all three contracts

![tach steady-state speed across three timing contracts](https://raw.githubusercontent.com/spence/tach/v0.2.0/benches/summary-use-cases.png)

The chart shows the four primary cells admitted by `EVID-PRIMARY-SPEED-CAMPAIGN`: Apple M1 Max,
AWS Graviton 3, AWS Intel Linux, and GitHub Windows 2025. Dark bars are `now()`; light bars are
the full `now() + elapsed()` roundtrip.

The renderer refuses to render unless every eligible comparison below passes for both operations
on all four primary environments.

| Contract | Audited references | Primary result |
|---|---|---|
| Same-thread elapsed | `quanta`, `fastant`, `minstant`, `std` (eligibility is platform-specific) | 4/4 pass |
| Synchronization-ordered elapsed | `std` | 4/4 pass |
| Current-thread CPU usage | Direct OS primitive; direct cached perf mapping when selected | 4/4 pass |

`Instant` is fastest outright on Apple M1 Max and AWS Graviton 3, and materially tied — within
`max(1 ns, 5%)` — with the fastest same-tier library on AWS Intel Linux and GitHub Windows.
`GlobalInstant` beats `std` on every primary cell.

A shorter bar can still be ineligible or unshippable: on Graviton 3 the exact `isb; cntvct` route
under `GlobalInstant` is retained as a disclosed diagnostic dispatch lower bound — the mandatory
`isb` barrier exposes a SIGILL-safe provider dispatch tach cannot skip and still ship, so the
chart's public comparison gates on the shippable read (`tach::GlobalInstant` 20.38 ns < `std`
32.23 ns), not the idealized diagnostic. The chart keeps such diagnostics visible; the validator
does not call them competitors.

The predeclared material-tie rule requires tach's point estimate and the conservative edges of
the two 95% confidence intervals to fit within `max(1 ns, 5%)` of every eligible reference. A
fraction-of-a-nanosecond wobble is therefore a tie, not a product claim. Linux provider selection
uses a separate paired rule: the inline path must win by that same margin in at least eight of
nine alternating batches.

These are steady-state results. Linux's one-time provider setup and measurement are deliberately
outside the hot-path benchmark.

See the [full values and methodology](https://github.com/spence/tach/blob/v0.2.0/BENCHMARKS.md),
the [machine-readable claim report](https://github.com/spence/tach/blob/v0.2.0/docs/evidence/timers/primary-speed-campaign-2026-07-18/campaign-report.json),
and the [retained evidence package](https://github.com/spence/tach/tree/v0.2.0/docs/evidence/timers/primary-speed-campaign-2026-07-18).

## How each timer works

### `Instant`: same-thread elapsed time

`Instant::now()` reads the architecture's monotonic counter directly where one is available:
RDTSC on x86, CNTVCT_EL0 on aarch64, and `rdtime` on RISC-V/LoongArch. It omits the
instruction-ordering barrier, making it the lowest-cost choice when both ends of a measurement
stay local to one thread.

The value is process-wide and can be moved, but the read is not ordered after prior memory
operations. If a timestamp participates in a cross-thread happens-before relationship, use
`GlobalInstant`.

### `GlobalInstant`: synchronization-ordered elapsed time

`GlobalInstant::now()` reads the elapsed-time counter behind the architecture's ordering
primitive: `lfence; rdtsc` on x86 and `isb sy` before CNTVCT_EL0 on aarch64, except Windows, whose
`GlobalInstant` stays on the OS-owned `QueryPerformanceCounter` call boundary for its cross-core
guarantee. A sample taken after an `Acquire` observation cannot be pulled in front of that
observation.

The load-then-now-then-check contract produced zero inversions in about 10.9 billion reads on the
tested x86 and aarch64 systems. RISC-V and LoongArch use their strongest available barriers, but
their ISA specifications do not guarantee that those barriers order the time CSR. Use
`std::time::Instant` there when a hardware-verified cross-thread guarantee is required. The
[ordered-clock evidence](https://github.com/spence/tach/blob/v0.2.0/benches/ORDERED-VERIFICATION.md)
records the exact placements and results.

### `ThreadCpuInstant`: current-thread CPU usage

On targets with a native thread clock, `ThreadCpuInstant::now()` measures CPU time delivered to
the calling OS thread. It advances while that thread executes and freezes while it sleeps, parks,
waits, or is descheduled. Native values are normalized to nanoseconds, so `elapsed()` returns an
ordinary `Duration`.

Provider setup and selection stay behind `ThreadCpuInstant::now()`; there is no separate public
clock handle. Introspection reports the selected mechanism and its cost class:

```rust
use tach::ThreadCpuInstant;

let provider = ThreadCpuInstant::provider();
let cost = ThreadCpuInstant::read_cost_hint();
let sample = ThreadCpuInstant::now();

println!("provider={provider:?}, cost={cost:?}");
println!("sample is thread CPU={}", sample.measures_thread_cpu_time());
```

Linux AArch64 has a deterministic provider policy: when the kernel exposes complete perf
task-clock mmap conversion metadata and permits the architectural counter, the default build uses
that inline path. If the capability is unavailable or a read fails, it falls back to the inlined
raw `CLOCK_THREAD_CPUTIME_ID` syscall. Benchmark builds audit both paths, but the measurements do
not override this capability policy.

Linux x86 and other runtime-tournament routes measure the complete eligible perf-mmap,
persistent perf-read, and native thread-clock paths during the thread's first read. A challenger
replaces the current winner only when it wins by more than `max(1 ns/read, 5%)` in at least eight
of nine paired 4,096-read batches. The retained Linux x86 host exposed perf access but selected the
raw `CLOCK_THREAD_CPUTIME_ID` syscall because the perf-read path was slower. The raw syscall is the
native primitive on Linux x86_64 and AArch64; libc remains only its read-failure fallback.

Other native targets use the OS primitive identified at build time:
`clock_gettime(CLOCK_THREAD_CPUTIME_ID)`, macOS `clock_gettime_nsec_np`, Windows
`GetThreadTimes`, or the WASI host thread clock.

`ThreadCpuInstant::now()` never fails. Targets without a portable current-thread clock use their
fastest monotonic elapsed-time source, such as `Performance.now()`. That fallback is explicit via
`provider()` and `measures_thread_cpu_time()`; tach never silently labels it CPU time. Durations
spanning a CPU/wall domain change return zero, `checked_duration_since` returns `None`, and partial
comparison is unordered.

## Platform support

| Platform / target | `Instant` | `GlobalInstant` | `ThreadCpuInstant` |
|---|---|---|---|
| Linux x86 / x86_64 | Measured kernel-eligible RDTSC or OS monotonic route | Independently measured ordered counter or OS monotonic route | Measured perf task-clock mmap/read or native thread-clock route |
| Linux AArch64 | Measured CNTVCT or OS monotonic route | Independently measured ordered CNTVCT or OS monotonic route | Complete inline perf capability when available; native raw syscall fallback |
| Linux RISC-V / LoongArch | Measured architecture counter or OS monotonic route | Independently measured ordered counter or OS monotonic route † | Measured perf task-clock mmap/read or native thread-clock route |
| Linux armv7 / s390x / powerpc64 | Measured architecture or OS monotonic route | Independently measured barrier/exception-ordered route | Measured perf task-clock mmap/read or native thread-clock route |
| macOS AArch64 | Bare architectural counter (Apple Silicon; ADR-0005) | Independently measured ordered XNU Mach/commpage route | `clock_gettime_nsec_np` thread clock |
| macOS x86_64 | Fixed `mach_absolute_time` read (no runtime tournament) | Independently measured ordered XNU Mach/commpage route | `clock_gettime_nsec_np` thread clock |
| Windows x86 / x86_64 | Calibrated invariant TSC behind a CPUID rate-stability gate (ADR-0007); degrades to QPC when ineligible | Windows-owned QPC ordered route (fixed) | `GetThreadTimes`; explicit QPC wall fallback on failure |
| Windows AArch64 | Windows-owned QPC monotonic route | Windows-owned QPC ordered route (fixed) | `GetThreadTimes`; explicit QPC wall fallback on failure |
| Android x86_64 / AArch64 | Measured architecture counter or OS monotonic route | Independently measured ordered counter or OS monotonic route | Measured perf task-clock mmap/read or native thread-clock route |
| FreeBSD x86_64 | Measured kernel-eligible TSC, libc, or raw clock route | Independently measured ordered TSC or OS clock route | Measured libc or raw native thread-clock route |
| WASI preview 1 / 2 | Host monotonic clock | Host monotonic clock | Host thread clock where exposed; otherwise explicit wall fallback |
| wasm-bindgen JavaScript host | Measured `Performance.now()` or Node `hrtime` route | Worker-comparable host route | Node thread CPU where exposed; otherwise explicit wall fallback |
| Emscripten | Measured local JavaScript host route | Local host route, or worker-comparable route with pthread support | Node thread CPU where exposed; otherwise explicit wall fallback |

The provider proof compiles all three public APIs with warnings denied in default and
`--no-default-features` modes for 24 target triples, then inspects optimized LLVM IR for each
provider route. This establishes support and routing. The primary speed campaign provides runtime
corroboration on four native environments; its public charts show those primary cells.
Unbenchmarked hardware is not represented as measured.

The `wasm32-unknown-unknown` and `wasm32v1-none` routes require a wasm-bindgen JavaScript host
that exposes `globalThis.performance`. A standalone wasm module without that host has no clock to
read.

† Best-effort cross-thread ordering by ISA specification, as described above.

## `no_std`

The crate root is `#![no_std]`. The default `thread-cpu-inline` feature links `std` on Linux x86,
x86_64, AArch64, armv7, RISC-V, s390x, LoongArch64, and PowerPC64 and on Android x86_64 and
AArch64 because the perf provider owns a native TLS mapping. Use `--no-default-features` for a
strict `no_std` dependency surface; those targets then use the native thread-CPU syscall tier.
Other supported targets retain their compile-time provider without pulling in `std`.

## Accuracy and drift

`Instant` and `GlobalInstant` convert architectural ticks with a cached fixed-point scale. For
wall-correlated accuracy over long intervals, call `Instant::recalibrate()` or enable the
`recalibrate-background` feature. The background feature requires `std`; manual recalibration does
not.

tach assumes a coherent, monotonic hardware counter. If the OS marks a TSC clocksource unstable
because cores are genuinely desynchronized, use the OS clock instead.

## Non-goals

- Calendar time or cross-machine synchronization.
- Sleeping, waking, or scheduling timers; use the OS or an async runtime.
- Comparing `ThreadCpuInstant` values from different OS threads.

## MSRV

Rust 1.95.

## License

MIT OR Apache-2.0.
