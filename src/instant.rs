use core::ops::{Add, AddAssign, Sub, SubAssign};
use core::time::Duration;

use crate::arch;

/// A sampled point in tach's local monotonic elapsed-time domain.
///
/// `Instant` is wall-clock-rate: it keeps ticking while the calling thread is
/// parked or descheduled. Whether it advances during whole-system suspend
/// follows the target's selected monotonic clock. Tach uses a direct
/// architectural counter where that is the fastest reliable timeline — including
/// a calibrated invariant TSC on x86 Windows — the OS-vetted QPC timeline on
/// aarch64 Windows and as the x86 Windows fallback, a measured invariant TSC or
/// XNU Mach absolute-time implementation on Intel macOS, a kernel-eligible and
/// measured TSC on FreeBSD/amd64, and the platform monotonic clock on fallback
/// targets.
///
/// Eligible providers expose a high-resolution timeline. Deliberately
/// coarsened or approximate clocks such as Linux `CLOCK_MONOTONIC_COARSE` do
/// not satisfy this contract even when their calls are cheaper.
///
/// # When to use this
///
/// Use `Instant` when both endpoints of an elapsed-time bracket remain local to
/// one thread. If a timestamp participates in a cross-thread happens-before
/// relationship, use [`OrderedInstant`] instead.
///
/// # Monotonicity contract
///
/// Samples are non-decreasing on one thread. Direct counter reads are not
/// ordered after prior memory operations, so they do not provide
/// [`OrderedInstant`]'s synchronization-order guarantee.
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
  /// Reads the current value of the target's local tick counter.
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now() -> Self {
    Self(arch::ticks())
  }

  /// Returns the duration that has elapsed since `self` was sampled.
  ///
  /// Saturates to zero rather than wrapping if the current read lands before
  /// `self`. See [`crate::OrderedInstant`] when the endpoint must be ordered
  /// after a cross-thread synchronization observation.
  #[inline(always)]
  #[allow(clippy::inline_always)]
  #[must_use]
  pub fn elapsed(&self) -> Duration {
    let (ticks, scale) = arch::ticks_with_scale();
    let delta = ticks.saturating_sub(self.0);
    ticks_to_duration_with_scale(delta, scale)
  }

  /// Returns the duration elapsed from `earlier` to `self`, or zero if
  /// `earlier` is later. This matches modern `std::time::Instant` behavior.
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
  /// No-op on platforms where the selected clock has an authoritative rate:
  /// aarch64 Windows QPC/QPF, Windows `OrderedInstant` on every architecture,
  /// the Darwin timebase, FreeBSD's selected wall provider, aarch64 `cntfrq_el0`,
  /// WASI, and the wasm host. Direct-TSC x86 targets re-measure the actual
  /// counter rate against the platform monotonic clock — `clock_gettime` on
  /// Unix, `QueryPerformanceCounter` on x86 Windows.
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
  /// Affects both [`Instant`] and [`crate::OrderedInstant`]. Each type owns
  /// the scale for its selected provider, so independently selected wall
  /// clocks remain in their own raw tick domains.
  pub fn recalibrate() {
    arch::recalibrate();
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

/// A monotonic elapsed-time sample ordered after prior memory observations.
///
/// On x86 and aarch64, a sample taken after an `Acquire` load that observed a
/// published `OrderedInstant` is at least that published value. The documented
/// load-then-now-then-check harness produced zero inversions in about 10.9
/// billion reads across the tested systems.
///
/// # Why this exists
///
/// A direct counter read is not a memory operation, so an out-of-order CPU can
/// sample [`Instant::now()`] before a prior `Acquire` load completes.
/// `OrderedInstant` uses the target's ordering boundary before or as part of
/// reading its selected wall-time domain. That provider can differ from
/// [`Instant`]'s provider when the fastest ordered and unordered reads differ
/// on a host.
///
/// # Per-architecture barrier
///
/// - **Windows x86, x86_64, and aarch64**: an opaque call to the independently
///   selected Windows-owned high-resolution monotonic source. The call boundary
///   prevents compiler motion across the read, while Windows owns cross-core
///   timeline ordering; tach never substitutes a raw TSC or CNTVCT read.
/// - **Intel macOS**: XNU's `lfence; rdtsc; lfence` Mach absolute-time protocol,
///   either inlined from the commpage data or reached through the system
///   function according to an Ordered-specific runtime cost probe.
/// - **Linux and Android x86 / x86_64**: an independently measured ordered TSC
///   path or the platform monotonic clock. Other x86 targets use `lfence;
///   rdtsc` on Intel, gated `rdtscp` elsewhere, and `cpuid; rdtsc` when
///   `rdtscp` is unavailable.
/// - **Linux and Android aarch64**: an independently measured `isb sy;
///   cntvct_el0`, self-synchronizing `cntvctss_el0`, or platform monotonic
///   provider. Other aarch64 targets use their OS-approved ordered counter.
/// - **riscv64**: `fence r, i` before `rdtime`; ratified Zicsr classifies the
///   observed memory read as `R` and the timer CSR read as device input `I`.
/// - **loongarch64 Linux**: a raw `clock_gettime` syscall as a synchronous
///   exception boundary, followed by `rdtime.d` in the same raw tick domain.
/// - **Linux armv7 and s390x**: respectively `dmb ish; isb` and `bcr 15,0`
///   immediately before `clock_gettime(CLOCK_MONOTONIC)`.
/// - **Linux powerpc64 GNU**: heavyweight `sync` immediately before the
///   64-bit Time Base read.
/// - **JavaScript-hosted wasm**: non-threaded Emscripten uses its direct
///   `performance.now()` host boundary. Emscripten pthread builds enabling the
///   `emscripten-pthreads` feature use the selected performance-epoch or
///   synchronized host timeline followed by a module-shared sequentially
///   consistent atomic maximum.
/// - **WASI and remaining fallback paths**: the host or OS clock boundary.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct OrderedInstant(u64);

impl OrderedInstant {
  /// Reads the counter through the target's ordering boundary so the timestamp
  /// is sampled *after* any prior `Acquire`-or-stronger observation.
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now() -> Self {
    Self(arch::ticks_ordered())
  }

  /// Returns the duration that has elapsed since `self` was sampled, with
  /// the end read also ordered. Use this when the elapsed end must come
  /// after some downstream synchronization point.
  #[inline(always)]
  #[allow(clippy::inline_always)]
  #[must_use]
  pub fn elapsed(&self) -> Duration {
    let (ticks, scale) = arch::ordered_ticks_with_scale();
    let delta = ticks.saturating_sub(self.0);
    ticks_to_duration_with_scale(delta, scale)
  }

  /// Returns the elapsed duration with an *unordered* end read. Use this
  /// only when the start of the measurement needed ordering (e.g. anchored
  /// to a published deadline) but the end is only for logging or coarse
  /// reporting where pre-acquire drift is harmless.
  #[inline]
  #[must_use]
  pub fn elapsed_unordered(&self) -> Duration {
    let delta = arch::ticks_ordered_unordered().saturating_sub(self.0);
    ordered_ticks_to_duration(delta)
  }

  /// See [`Instant::duration_since`]. Cross-type calls against a plain
  /// [`Instant`] are deliberately not provided because the two types may use
  /// different providers and raw tick domains.
  #[inline]
  #[must_use]
  pub fn duration_since(&self, earlier: OrderedInstant) -> Duration {
    self.checked_duration_since(earlier).unwrap_or_default()
  }

  /// See [`Instant::checked_duration_since`].
  #[inline]
  #[must_use]
  pub fn checked_duration_since(&self, earlier: OrderedInstant) -> Option<Duration> {
    self.0.checked_sub(earlier.0).map(ordered_ticks_to_duration)
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
    let delta = duration_to_ordered_ticks(duration)?;
    self.0.checked_add(delta).map(Self)
  }

  /// See [`Instant::checked_sub`].
  #[inline]
  #[must_use]
  pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
    let delta = duration_to_ordered_ticks(duration)?;
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
pub(crate) fn ticks_to_duration(ticks: u64) -> Duration {
  ticks_to_duration_with_scale(ticks, arch::nanos_per_tick_q32())
}

#[inline]
pub(crate) fn ordered_ticks_to_duration(ticks: u64) -> Duration {
  ticks_to_duration_with_scale(ticks, arch::ordered_nanos_per_tick_q32())
}

#[inline]
pub(crate) fn ticks_to_duration_with_scale(ticks: u64, scale: u64) -> Duration {
  let nanos = if scale == 1_u64 << 32 {
    ticks
  } else {
    let product = u128::from(ticks) * u128::from(scale);
    u64::try_from(product >> 32).unwrap_or(u64::MAX)
  };
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
pub(crate) fn duration_to_ticks(d: Duration) -> Option<u64> {
  duration_to_ticks_with_scale(d, arch::nanos_per_tick_q32())
}

#[inline]
fn duration_to_ordered_ticks(d: Duration) -> Option<u64> {
  duration_to_ticks_with_scale(d, arch::ordered_nanos_per_tick_q32())
}

#[inline]
fn duration_to_ticks_with_scale(d: Duration, q32: u64) -> Option<u64> {
  let nanos = d.as_nanos();
  if q32 == 0 {
    return None;
  }
  // nanos = (ticks * q32) >> 32  ⇒  ticks = (nanos << 32) / q32
  nanos.checked_shl(32)?.checked_div(u128::from(q32))?.try_into().ok()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn conversion_helpers_keep_tick_scales_independent() {
    let one_ns_per_tick = 1_u64 << 32;
    let two_ns_per_tick = 2_u64 << 32;

    assert_eq!(ticks_to_duration_with_scale(50, one_ns_per_tick), Duration::from_nanos(50));
    assert_eq!(ticks_to_duration_with_scale(50, two_ns_per_tick), Duration::from_nanos(100));
    assert_eq!(duration_to_ticks_with_scale(Duration::from_nanos(100), one_ns_per_tick), Some(100));
    assert_eq!(duration_to_ticks_with_scale(Duration::from_nanos(100), two_ns_per_tick), Some(50));
  }
}
