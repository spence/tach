use core::ops::{Add, AddAssign, Sub, SubAssign};
use core::time::Duration;

use crate::arch;

/// A sampled point in the process-wide counter timeline. The fast read.
///
/// Drop-in replacement for [`std::time::Instant`] backed by the architectural
/// wall-clock counter (RDTSC, CNTVCT_EL0, rdtime, rdtime.d). One instruction
/// on every native target; no runtime dispatch. **1.5–8× faster than
/// `std::time::Instant`** on every supported platform.
///
/// `Instant` is wall-clock-rate: it keeps ticking through park, suspension, and
/// descheduling. The same source is used across every thread in the process.
///
/// # When to use this
///
/// Reach for `Instant` for the common case: pinned threads, or any code
/// where ≤10 µs cross-thread sync slop is acceptable. That covers almost
/// any tracing, profiling, latency measurement, or request-budget use case.
///
/// For uses where strict cross-thread monotonicity matters (multi-thread
/// ordered logs, distributed spans, lock-free version numbers), reach for
/// [`MonotonicInstant`] instead — it adds a process-global atomic to
/// guarantee strict ordering across threads. For acquire-load correlation
/// against user atomics, reach for [`OrderedInstant`].
///
/// # Monotonicity contract
///
/// **Per-thread**: strictly non-decreasing by hardware on every supported
/// target. A pinned thread (or any thread the OS doesn't migrate between
/// cores) reads a monotonically increasing sequence. Measured: 0 backward
/// jumps across 33 billion reads in the 6-cell bench matrix.
///
/// **Cross-thread**: bounded by the hardware's per-core sync floor (≤10 µs
/// on every tested cell, matching [`std::time::Instant`] on the same
/// hardware — both read the same underlying counter). Empirically the bare
/// counter shows non-zero contract violations on the strict
/// load-then-now-then-check test on every multi-threaded platform tested.
/// For strict cross-thread monotonicity, use [`MonotonicInstant`].
///
/// # Cost
///
/// 3.5 ns on Apple Silicon M1, 7.3 ns on Graviton 3, 8.5 ns on Intel bare
/// metal, 9.6 ns on Windows, 14.4 ns on Intel virtualized (Nitro), 21.2 ns
/// on AWS Lambda Firecracker. Per-thread, single-thread tight loop. Full
/// per-platform table in `BENCHMARKS.md`.
///
/// # Example
///
/// ```
/// use tach::Instant;
///
/// let start = Instant::now();
/// // ... work ...
/// let elapsed = start.elapsed();
/// println!("{elapsed:?}");
/// ```
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Instant(u64);

impl Instant {
  /// Reads the current value of the process-wide tick counter.
  ///
  /// Compiles to a single architectural counter read on every supported target.
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now() -> Self {
    Self(arch::ticks())
  }

  /// Returns the duration that has elapsed since `self` was sampled.
  ///
  /// Drop-in equivalent for [`std::time::Instant::elapsed()`].
  #[inline]
  #[must_use]
  pub fn elapsed(&self) -> Duration {
    let delta = arch::ticks().wrapping_sub(self.0);
    ticks_to_duration(delta)
  }

  /// Returns the duration elapsed from `earlier` to `self`, or zero if
  /// `earlier` is later. Matches modern [`std::time::Instant::duration_since`]
  /// (which saturates rather than panicking).
  #[inline]
  #[must_use]
  pub fn duration_since(&self, earlier: Instant) -> Duration {
    self.checked_duration_since(earlier).unwrap_or_default()
  }

  /// Returns the duration elapsed from `earlier` to `self`, or `None` if
  /// `earlier` is later than `self`.
  #[inline]
  #[must_use]
  pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
    self.0.checked_sub(earlier.0).map(ticks_to_duration)
  }

  /// Saturating equivalent of [`Self::duration_since`] — same behavior in
  /// modern std. Returns zero if `earlier` is later than `self`.
  #[inline]
  #[must_use]
  pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
    self.duration_since(earlier)
  }

  /// Returns `Some(self + duration)` if it can be represented as an
  /// `Instant`, otherwise `None`. Headroom is ~580 years on a 1 GHz
  /// counter — overflow is theoretical, not practical.
  #[inline]
  #[must_use]
  pub fn checked_add(&self, duration: Duration) -> Option<Self> {
    let delta = duration_to_ticks(duration)?;
    self.0.checked_add(delta).map(Self)
  }

  /// Returns `Some(self - duration)` if it can be represented as an
  /// `Instant`, otherwise `None`.
  #[inline]
  #[must_use]
  pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
    let delta = duration_to_ticks(duration)?;
    self.0.checked_sub(delta).map(Self)
  }

  /// Re-derive the tick-to-nanosecond scaling against the platform
  /// monotonic clock to correct for crystal drift over long uptime.
  ///
  /// Call from a background scheduler or after a long sleep. Cost is
  /// ~700 ms of spin-loop time per call (7 × 100 ms calibration
  /// windows; preempted samples are discarded and don't contribute
  /// to the median); do not invoke from a hot path.
  ///
  /// No-op on platforms where the frequency is exact: aarch64
  /// (`cntfrq_el0`), macOS (`mach_timebase_info`), WASI, and the wasm host.
  /// On x86 / x86_64 (Linux, other Unixes, and Windows) the kernel doesn't
  /// continuously correct crystal drift, so recalibration measures the
  /// actual rate against the platform monotonic clock
  /// (`clock_gettime(CLOCK_MONOTONIC)` on Unix, `QueryPerformanceCounter`
  /// on Windows).
  ///
  /// `recalibrate` itself is `#![no_std]`-compatible — it uses the same
  /// platform-monotonic spin-loop path the crate already calls during
  /// startup calibration. Safe from embedded, kernel, and SGX targets.
  ///
  /// For services that prefer not to drive this themselves, enabling the
  /// `recalibrate-background` Cargo feature spawns a background thread
  /// that calls this every 60 seconds (interval configurable via
  /// `tach::set_recalibration_interval`). **That feature requires `std`
  /// and is incompatible with `#![no_std]` targets** — it pulls in
  /// `std::thread` and `std::sync::OnceLock`.
  ///
  /// Affects both [`Instant`] and [`crate::OrderedInstant`]; they share
  /// the same scaling cache.
  pub fn recalibrate() {
    arch::recalibrate();
  }

  #[inline(always)]
  pub(crate) fn from_raw_ticks(ticks: u64) -> Self {
    Self(ticks)
  }
}

impl Add<Duration> for Instant {
  type Output = Instant;
  fn add(self, rhs: Duration) -> Instant {
    self.checked_add(rhs).expect("overflow when adding duration to instant")
  }
}

impl AddAssign<Duration> for Instant {
  fn add_assign(&mut self, rhs: Duration) {
    *self = *self + rhs;
  }
}

impl Sub<Duration> for Instant {
  type Output = Instant;
  fn sub(self, rhs: Duration) -> Instant {
    self.checked_sub(rhs).expect("overflow when subtracting duration from instant")
  }
}

impl SubAssign<Duration> for Instant {
  fn sub_assign(&mut self, rhs: Duration) {
    *self = *self - rhs;
  }
}

impl Sub<Instant> for Instant {
  type Output = Duration;
  fn sub(self, rhs: Instant) -> Duration {
    self.duration_since(rhs)
  }
}

/// An [`Instant`] sampled with an instruction-ordering barrier so the
/// timestamp cannot be reordered before any prior `Acquire`-or-stronger
/// observation.
///
/// Use this when correlating a timestamp with synchronization state — e.g.
/// reading a deadline or yielding signal from another thread and needing
/// the timestamp to reflect time *after* the observation:
///
/// ```ignore
/// let deadline = scheduler_state.load(Ordering::Acquire);
/// let now = OrderedInstant::now();
/// // `now` is guaranteed to be sampled after `deadline` was observed.
/// ```
///
/// With plain [`Instant`] the counter read can be hoisted earlier than the
/// acquire-load completes (on aarch64 `mrs cntvct_el0` is a system-register
/// access that memory fences do not constrain; on x86 `rdtsc` is not a
/// serializing instruction). [`OrderedInstant::now()`] emits the
/// arch-appropriate barrier before the counter read.
///
/// # Per-architecture barrier
///
/// - **aarch64**: `isb sy` before `mrs cntvct_el0` — the established pattern,
///   documented to order the system-register read.
/// - **x86 / x86_64**: `lfence` before `rdtsc` — Intel-documented; AMD honors
///   it when the kernel sets `DE_CFG[1]` (Linux does so by default for
///   Spectre v1 mitigation).
/// - **riscv64**: `fence iorw, iorw` before `rdtime`. **Best-effort**: whether
///   memory fences constrain CSR access (`rdtime` reads the Zicntr time CSR,
///   not memory) is implementation-defined in the RISC-V spec. Weaker
///   contract than aarch64 / x86.
/// - **loongarch64**: `dbar 0` before `rdtime.d`. **Best-effort**: same
///   caveat — `dbar 0` is a memory barrier; its constraint on CSR access is
///   implementation-defined.
/// - **wasm / WASI / fallback paths**: kernel / runtime / JS boundary already
///   serializes naturally.
///
/// # Cost
///
/// Roughly 5–20 ns more than [`Instant::now()`] depending on architecture;
/// per-platform measurements: 18.5 ns on Apple Silicon M1, 26.1 ns on
/// Graviton 3, 27.1 ns on Intel virtualized, 16.8 ns on Intel bare metal,
/// 25.4 ns on Windows, 40 ns on AWS Lambda Firecracker (per-thread, no
/// contention). Still substantially faster than [`std::time::Instant::now()`]
/// on Linux and macOS (which call into the vDSO / libsystem path but do not
/// themselves guarantee this ordering against atomics).
///
/// Note: on every cell we measure, `OrderedInstant::now()` is **slower**
/// than [`crate::MonotonicInstant::now()`] per-thread. The pipeline-drain
/// barrier costs more than a LOCK fetch_max on an uncontended cache line.
/// Counterintuitive — atomics should be expensive! — but consistent across
/// our 6-cell bench matrix.
///
/// # Empirical bonus: strict cross-thread monotonicity
///
/// Across every cell we test, `OrderedInstant::now()` also passes the
/// strict cross-thread monotonicity test (`monotonic_strict_cross_thread`
/// in the unit tests; per-cell data in `BENCHMARKS.md`). The pipeline-
/// drain barrier serializes enough state that the cross-core race window
/// closes. **This is an empirical observation, not an ISA guarantee** —
/// Intel/ARM specs for `rdtscp` / `isb sy` do not promise cross-core TSC
/// synchronization, only per-core pipeline serialization.
///
/// If you need *guaranteed* strict cross-thread monotonicity by
/// construction, use [`crate::MonotonicInstant`] (which is also cheaper
/// per-thread). If you've measured your target hardware and confirmed
/// `OrderedInstant` passes the strict test there, you can rely on it for
/// both acquire-ordering and cross-thread strictness on that hardware.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct OrderedInstant(u64);

impl OrderedInstant {
  /// Reads the counter with an instruction-ordering barrier so the
  /// timestamp is sampled *after* any prior `Acquire`-or-stronger
  /// observation.
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now() -> Self {
    Self(arch::ticks_ordered())
  }

  /// Returns the duration that has elapsed since `self` was sampled, with
  /// the end read also ordered. Use this when the elapsed end must come
  /// after some downstream synchronization point.
  #[inline]
  #[must_use]
  pub fn elapsed(&self) -> Duration {
    let delta = arch::ticks_ordered().wrapping_sub(self.0);
    ticks_to_duration(delta)
  }

  /// Returns the elapsed duration with an *unordered* end read. Use this
  /// only when the start of the measurement needed ordering (e.g. anchored
  /// to a published deadline) but the end is only for logging or coarse
  /// reporting where pre-acquire drift is harmless.
  #[inline]
  #[must_use]
  pub fn elapsed_unordered(&self) -> Duration {
    let delta = arch::ticks().wrapping_sub(self.0);
    ticks_to_duration(delta)
  }

  /// Discards the ordering guarantee and returns a plain [`Instant`] with
  /// the same tick value. Useful when storing the timestamp in a struct
  /// field typed as [`Instant`]. There is no inverse — an unordered
  /// [`Instant`] cannot be promoted because the original read was not
  /// ordered.
  #[inline]
  pub fn as_unordered(&self) -> Instant {
    Instant::from_raw_ticks(self.0)
  }

  /// See [`Instant::duration_since`]. Cross-type calls (against a plain
  /// `Instant`) are deliberately not provided — downgrade with
  /// [`Self::as_unordered`] first if you need to compare.
  #[inline]
  #[must_use]
  pub fn duration_since(&self, earlier: OrderedInstant) -> Duration {
    self.checked_duration_since(earlier).unwrap_or_default()
  }

  /// See [`Instant::checked_duration_since`].
  #[inline]
  #[must_use]
  pub fn checked_duration_since(&self, earlier: OrderedInstant) -> Option<Duration> {
    self.0.checked_sub(earlier.0).map(ticks_to_duration)
  }

  /// See [`Instant::saturating_duration_since`].
  #[inline]
  #[must_use]
  pub fn saturating_duration_since(&self, earlier: OrderedInstant) -> Duration {
    self.duration_since(earlier)
  }

  /// See [`Instant::checked_add`]. The returned `OrderedInstant` is a
  /// synthetic point in the timeline; no architectural fence runs (fences
  /// only matter for *reads*).
  #[inline]
  #[must_use]
  pub fn checked_add(&self, duration: Duration) -> Option<Self> {
    let delta = duration_to_ticks(duration)?;
    self.0.checked_add(delta).map(Self)
  }

  /// See [`Instant::checked_sub`].
  #[inline]
  #[must_use]
  pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
    let delta = duration_to_ticks(duration)?;
    self.0.checked_sub(delta).map(Self)
  }
}

impl Add<Duration> for OrderedInstant {
  type Output = OrderedInstant;
  fn add(self, rhs: Duration) -> OrderedInstant {
    self.checked_add(rhs).expect("overflow when adding duration to ordered instant")
  }
}

impl AddAssign<Duration> for OrderedInstant {
  fn add_assign(&mut self, rhs: Duration) {
    *self = *self + rhs;
  }
}

impl Sub<Duration> for OrderedInstant {
  type Output = OrderedInstant;
  fn sub(self, rhs: Duration) -> OrderedInstant {
    self
      .checked_sub(rhs)
      .expect("overflow when subtracting duration from ordered instant")
  }
}

impl SubAssign<Duration> for OrderedInstant {
  fn sub_assign(&mut self, rhs: Duration) {
    *self = *self - rhs;
  }
}

impl Sub<OrderedInstant> for OrderedInstant {
  type Output = Duration;
  fn sub(self, rhs: OrderedInstant) -> Duration {
    self.duration_since(rhs)
  }
}

/// An [`Instant`] sampled with a guarantee of **strict cross-thread
/// monotonicity**: every call from any thread returns a value greater than
/// or equal to every prior call from any thread.
///
/// # When to use this
///
/// Reach for `MonotonicInstant` when you need timestamps from multiple
/// threads to be strictly ordered — for example, distributed tracing spans
/// whose start/end events come from different threads but must form a
/// non-decreasing timeline, lock-free data structures that use timestamps
/// as monotonic version numbers, or any code where a "later" timestamp
/// observed via cross-thread synchronization must not compare less than an
/// "earlier" one.
///
/// Plain [`Instant`] is bounded by the underlying counter's cross-thread
/// synchronization. On x86 the per-core TSC is firmware-synchronized but
/// not architecturally so, and even on aarch64 — where ARMv8 specifies
/// `cntvct_el0` as a single global counter — measurable per-core slop has
/// been observed in practice (sub-microsecond on Apple Silicon M1). For
/// timestamps that must be strictly ordered across threads, `Instant`'s
/// hardware-floor monotonicity isn't enough.
///
/// `MonotonicInstant` adds a process-global `AtomicU64::fetch_max` to
/// every read, applied uniformly across every supported target. The
/// fetch_max forces every return value to be `>=` every previously
/// published value, by construction.
///
/// # Cost
///
/// The enforcement is applied empirically — `MonotonicInstant::now()` pays
/// the cost of `AtomicU64::fetch_max(AcqRel)` only on platforms where
/// `measure_strict_cross_thread` (see `BENCHMARKS.md` "Strict cross-thread
/// monotonicity (contract validation)") shows the bare arch counter fails
/// the strict happens-before contract. That covers every multi-threaded
/// platform tach supports: x86 (Linux / macOS / Windows), aarch64 (Linux /
/// macOS / Windows), riscv64, loongarch64.
///
/// Per-platform uncontended cost (per-thread tight loop, no contention):
/// 7.0 ns on Apple Silicon M1, 10.6 ns on Graviton 3, 14.4 ns on Intel
/// bare metal, 10.7 ns on Windows, 25.4 ns on Intel virtualized (Nitro),
/// 35.7 ns on AWS Lambda Firecracker. That's 2–14 ns more than
/// [`Instant::now()`] on the same hardware.
///
/// **Counterintuitive finding**: `MonotonicInstant::now()` is FASTER than
/// [`OrderedInstant::now()`] per-thread on every cell we measure (e.g. Apple
/// Silicon: 7.0 vs 18.5 ns; Windows: 10.7 vs 25.4 ns). The LOCK fetch_max
/// on an uncontended cache line is cheaper than the pipeline-drain barrier
/// `OrderedInstant` uses. Atomics-should-be-expensive is the wrong
/// intuition for this regime.
///
/// Under heavy contention (many threads simultaneously hammering `now()`),
/// the `fetch_max` can degrade to 100+ ns per call as the cache line
/// bounces between cores. If your workload only needs per-thread
/// monotonicity, prefer plain [`Instant`] to avoid this cost.
///
/// On wasm32 (single-threaded JS realm with W3C HRT strict `performance.now()`)
/// and WASI (single-threaded execution with strict-monotonic spec),
/// `MonotonicInstant::now()` compiles to **the same instruction as
/// [`Instant::now()`]** — there is no concurrency for the enforcement
/// to enforce against, so it is elided at compile time.
///
/// # Comparison to `std::time::Instant`
///
/// `std::time::Instant::now()` on Unix is just `clock_gettime(CLOCK_MONOTONIC)`
/// — it reads the same underlying hardware counter and performs no software
/// cross-thread enforcement. On x86 it inherits the same sub-microsecond
/// hardware sync slop that plain tach `Instant` does. `MonotonicInstant` is
/// **strictly stronger than `std::time::Instant`** on x86, and matches its
/// guarantee at zero cost on every other architecture.
///
/// # Example
///
/// ```
/// use tach::MonotonicInstant;
///
/// let t1 = MonotonicInstant::now();
/// // ... cross-thread work, channel sends, mutex unlocks, etc. ...
/// let t2 = MonotonicInstant::now();
/// assert!(t2 >= t1);  // always; never returns Instants out of order
/// ```
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct MonotonicInstant(u64);

impl MonotonicInstant {
  /// Reads a strictly-cross-thread-monotonic timestamp.
  ///
  /// On architectures where the underlying counter is already
  /// architecturally globally synchronized (aarch64, RISC-V, LoongArch,
  /// WASI, wasm) or where the OS clock guarantees monotonicity, this
  /// compiles to the same instruction as [`Instant::now()`]. On x86, it
  /// performs the bare counter read plus an `AtomicU64::fetch_max` against
  /// a process-global last-seen tick.
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now() -> Self {
    Self(arch::ticks_monotonic())
  }

  /// Returns the duration that has elapsed since `self` was sampled, with
  /// the end read also strictly cross-thread monotonic.
  #[inline]
  #[must_use]
  pub fn elapsed(&self) -> Duration {
    let delta = arch::ticks_monotonic().wrapping_sub(self.0);
    ticks_to_duration(delta)
  }

  /// Returns the duration elapsed from `earlier` to `self`, or zero if
  /// `earlier` is later. Matches modern [`std::time::Instant::duration_since`].
  #[inline]
  #[must_use]
  pub fn duration_since(&self, earlier: MonotonicInstant) -> Duration {
    self.checked_duration_since(earlier).unwrap_or_default()
  }

  /// Returns the duration elapsed from `earlier` to `self`, or `None` if
  /// `earlier` is later than `self`. Because `MonotonicInstant::now()` is
  /// strictly cross-thread monotonic, this only returns `None` if the
  /// caller passes the arguments in the wrong order.
  #[inline]
  #[must_use]
  pub fn checked_duration_since(&self, earlier: MonotonicInstant) -> Option<Duration> {
    self.0.checked_sub(earlier.0).map(ticks_to_duration)
  }

  /// Saturating equivalent of [`Self::duration_since`].
  #[inline]
  #[must_use]
  pub fn saturating_duration_since(&self, earlier: MonotonicInstant) -> Duration {
    self.duration_since(earlier)
  }

  /// Returns `Some(self + duration)` if it can be represented, otherwise `None`.
  #[inline]
  #[must_use]
  pub fn checked_add(&self, duration: Duration) -> Option<Self> {
    let delta = duration_to_ticks(duration)?;
    self.0.checked_add(delta).map(Self)
  }

  /// Returns `Some(self - duration)` if it can be represented, otherwise `None`.
  #[inline]
  #[must_use]
  pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
    let delta = duration_to_ticks(duration)?;
    self.0.checked_sub(delta).map(Self)
  }

  #[inline(always)]
  #[allow(dead_code)]
  pub(crate) fn from_raw_ticks(ticks: u64) -> Self {
    Self(ticks)
  }
}

impl Add<Duration> for MonotonicInstant {
  type Output = MonotonicInstant;
  fn add(self, rhs: Duration) -> MonotonicInstant {
    self.checked_add(rhs).expect("overflow when adding duration to instant")
  }
}

impl AddAssign<Duration> for MonotonicInstant {
  fn add_assign(&mut self, rhs: Duration) {
    *self = *self + rhs;
  }
}

impl Sub<Duration> for MonotonicInstant {
  type Output = MonotonicInstant;
  fn sub(self, rhs: Duration) -> MonotonicInstant {
    self.checked_sub(rhs).expect("overflow when subtracting duration from instant")
  }
}

impl SubAssign<Duration> for MonotonicInstant {
  fn sub_assign(&mut self, rhs: Duration) {
    *self = *self - rhs;
  }
}

impl Sub<MonotonicInstant> for MonotonicInstant {
  type Output = Duration;
  fn sub(self, rhs: MonotonicInstant) -> Duration {
    self.duration_since(rhs)
  }
}

// Q32 fixed-point conversion: nanos = (ticks * scale) >> 32 where
// scale = (1e9 << 32) / frequency. Avoids the per-call u128 division
// which is slow on virtualized x86 (Nitro burst VMs, Firecracker on
// Lambda) — typical savings on those targets is 15-25 ns/call.
#[inline]
fn ticks_to_duration(ticks: u64) -> Duration {
  let product = u128::from(ticks) * u128::from(arch::nanos_per_tick_q32());
  let nanos = u64::try_from(product >> 32).unwrap_or(u64::MAX);
  // Common case for elapsed (< 1 second): build Duration directly from
  // secs=0 + subsec_nanos. The compiler can prove `nanos_u32 < 1e9` from
  // the branch and elide the internal divide in Duration::new. Avoids
  // a divide by 1e9 on the hot path (~10 ns on virtualized x86).
  if nanos < 1_000_000_000 { Duration::new(0, nanos as u32) } else { Duration::from_nanos(nanos) }
}

// Inverse of `ticks_to_duration`. Returns None when the Duration is large
// enough to overflow either the u128 intermediate (shift-left by 32) or the
// final u64 tick count. Headroom: at 1 GHz, u64 ticks represent ~580 years;
// overflow is theoretical, not practical.
#[inline]
fn duration_to_ticks(d: Duration) -> Option<u64> {
  let nanos = d.as_nanos();
  let q32 = arch::nanos_per_tick_q32();
  if q32 == 0 {
    return None;
  }
  // nanos = (ticks * q32) >> 32  ⇒  ticks = (nanos << 32) / q32
  nanos.checked_shl(32)?.checked_div(u128::from(q32))?.try_into().ok()
}
