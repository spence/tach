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
/// The moment a timestamp crosses a thread boundary and gets *compared or
/// ordered* against another thread's — multi-thread ordered logs, distributed
/// spans, lock-free version numbers, a deadline read after an `Acquire`-load —
/// reach for [`OrderedInstant`] instead, which is monotonic across threads by
/// construction.
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
/// For cross-thread synchronization-order monotonicity, use [`OrderedInstant`].
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
  /// Drop-in equivalent for [`std::time::Instant::elapsed()`]. Saturates to
  /// zero rather than wrapping if the current read lands before `self` — the
  /// only way that happens is a cross-thread read landing inside the counter's
  /// sub-microsecond sync window; see [`crate::OrderedInstant`] for a
  /// value that is monotone for cross-thread *ordering*, not just subtraction.
  #[inline]
  #[must_use]
  pub fn elapsed(&self) -> Duration {
    let delta = arch::ticks().saturating_sub(self.0);
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

/// The architectural counter read, **ordered against memory** — sampled at its
/// true position in program order, never earlier. This is the timestamp to use
/// the moment a value crosses a thread boundary and gets compared or ordered.
///
/// `OrderedInstant` is **monotonic across threads**: a timestamp taken on any
/// thread, after an `Acquire`-load that observed another thread's published
/// `OrderedInstant`, is guaranteed `>=` that published value. Verified at
/// **0 backward steps in ~10.9 billion reads** across Apple Silicon, AWS
/// Graviton 3, Intel and AMD x86 (single- and dual-socket NUMA) — see
/// `benches/ORDERED-VERIFICATION.md`. On the same hardware, a bare [`Instant`]
/// inverts from sub-ppm (Nitro VMs) to ~12% (Apple Silicon) of cross-thread
/// reads.
///
/// ```ignore
/// // Thread A:                          // Thread B:
/// let t = OrderedInstant::now();         let seen = shared.load(Ordering::Acquire);
/// shared.store(t, Ordering::Release);    let now = OrderedInstant::now();
///                                        // now >= t, always.
/// ```
///
/// # Why this exists
///
/// [`Instant::now()`] is one counter-read instruction (`rdtsc` / `mrs
/// cntvct_el0`). It is not a memory operation, and an out-of-order CPU can
/// sample it *before* a prior `Acquire`-load completes. Two consequences, one
/// cause: on the same thread the read can land before the load you meant to
/// time after; across threads it can land before the load that joins you to
/// another thread, inverting two timestamps across a happens-before edge.
/// `OrderedInstant` emits the arch barrier that pins the read after prior
/// memory operations, closing both at once.
///
/// # Per-architecture barrier
///
/// - **x86 / x86_64**: `rdtscp` — Intel SDM Vol 2B: "waits until all previous
///   instructions have executed and all previous loads are globally visible."
///   One instruction, unconditionally serializing for prior loads.
/// - **aarch64**: `isb sy` before `mrs cntvct_el0` — drains the pipeline so the
///   system-register read cannot be hoisted past prior memory access.
/// - **riscv64**: `fence iorw, iorw` before `rdtime`. **Best-effort**: whether a
///   memory fence constrains the `rdtime` CSR read is implementation-defined in
///   the RISC-V spec, and tach's cross-thread guarantee is **not yet verified on
///   native RISC-V hardware**. Use `std` there if you need a bench-proven
///   ordering guarantee today.
/// - **loongarch64**: `dbar 0` before `rdtime.d` — same best-effort caveat and
///   the same "not yet verified on native hardware" status as riscv64.
/// - **wasm / WASI / fallback paths**: the kernel / runtime / JS boundary
///   already serializes the call site.
///
/// # Cost
///
/// Roughly 5–20 ns more than [`Instant::now()`] depending on architecture:
/// 18.5 ns on Apple Silicon M1, 26.1 ns on Graviton 3, 16.8 ns on Intel bare
/// metal, 25.4 ns on Windows, 40 ns on AWS Lambda Firecracker (per-thread).
/// Still 1.5–4× faster than [`std::time::Instant::now()`], which reaches its
/// own cross-thread correctness only by paying the vDSO / syscall path. And
/// unlike an atomic-based approach, `OrderedInstant` holds **no shared state**,
/// so its per-call cost is flat regardless of thread count — there is no
/// contention cliff. (See `docs/WHY-NOT-AN-ATOMIC.md`.)
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
    let delta = arch::ticks_ordered().saturating_sub(self.0);
    ticks_to_duration(delta)
  }

  /// Returns the elapsed duration with an *unordered* end read. Use this
  /// only when the start of the measurement needed ordering (e.g. anchored
  /// to a published deadline) but the end is only for logging or coarse
  /// reporting where pre-acquire drift is harmless.
  #[inline]
  #[must_use]
  pub fn elapsed_unordered(&self) -> Duration {
    let delta = arch::ticks().saturating_sub(self.0);
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
