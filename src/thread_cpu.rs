use core::cmp::Ordering;
use core::marker::PhantomData;
use core::ops::{Add, AddAssign, Sub, SubAssign};
use core::time::Duration;

use crate::arch;

const WALL_DOMAIN_BIT: u64 = 1 << 63;
const VALUE_MASK: u64 = WALL_DOMAIN_BIT - 1;

#[inline]
#[cfg(any(not(target_os = "macos"), test))]
pub(crate) const fn encode_wall_ticks(ticks: u64) -> u64 {
  WALL_DOMAIN_BIT | (ticks & VALUE_MASK)
}

#[inline]
#[cfg(target_os = "windows")]
pub(crate) const fn is_wall_value(value: u64) -> bool {
  value & WALL_DOMAIN_BIT != 0
}

/// The native mechanism used to read current-thread CPU time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ThreadCpuProvider {
  /// Linux `perf_event_open` metadata mapped into the process and read inline.
  LinuxPerfMmap,
  /// The operating system's POSIX current-thread CPU clock.
  PosixThreadCpuClock,
  /// Windows `GetThreadTimes`, combining kernel and user execution time.
  WindowsThreadTimes,
  /// The WASI host's current-thread CPU clock.
  WasiThreadCpuClock,
  /// JavaScript `Performance.now()`, used when WebAssembly has no thread clock.
  ///
  /// This is a monotonic wall-time fallback: it advances while the thread is
  /// descheduled or sleeping.
  PerformanceNow,
  /// The target's fastest monotonic wall clock.
  ///
  /// This fallback is used when the target has no current-thread CPU clock, or
  /// if a native thread clock unexpectedly becomes unavailable. It advances
  /// while the thread is descheduled or sleeping.
  MonotonicWallClock,
}

impl ThreadCpuProvider {
  /// Returns whether this provider measures CPU time delivered to the current
  /// OS thread rather than monotonic wall time.
  #[inline]
  #[must_use]
  pub const fn measures_thread_cpu_time(self) -> bool {
    matches!(
      self,
      Self::LinuxPerfMmap
        | Self::PosixThreadCpuClock
        | Self::WindowsThreadTimes
        | Self::WasiThreadCpuClock
    )
  }
}

/// The expected steady-state cost class of the selected provider.
///
/// This is categorical because exact nanosecond cost depends on the CPU,
/// kernel, and virtualization environment. It lets applications require the
/// inline production tier without treating a machine-specific benchmark as a
/// stable API promise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ThreadCpuReadCost {
  /// An inline userspace counter read.
  Inline,
  /// A call into the operating system.
  SystemCall,
  /// A call across the WebAssembly host boundary.
  HostCall,
}

/// A sampled point in the current OS thread's scheduled CPU-time timeline.
///
/// On targets with a native current-thread CPU clock, this timeline advances
/// only while the calling thread executes on a CPU and freezes while that
/// thread is parked, descheduled, or sleeping. Values are normalized to
/// nanoseconds.
///
/// Targets without a native thread clock use their fastest monotonic wall
/// source so this API remains available everywhere tach supports. That
/// fallback advances during descheduling. Call [`Self::provider`] and
/// [`ThreadCpuProvider::measures_thread_cpu_time`] when the distinction is
/// correctness-sensitive.
///
/// Provider choice is stable in normal operation. If a native read ever fails,
/// tach returns a tagged wall sample rather than failing the call. Durations
/// spanning a CPU/wall boundary return zero (or
/// `None` from [`Self::checked_duration_since`]); partial comparisons across
/// the boundary are unordered. This prevents the two domains from being mixed.
/// The tag leaves 63 value bits: roughly 292 years of thread CPU nanoseconds;
/// wall fallbacks wrap after `2^63` source ticks (about 58 years even at
/// 5 GHz). An interval spanning that wrap fails closed in the same way.
///
/// A `ThreadCpuInstant` may only be compared with values sampled on the same
/// OS thread. The type is deliberately neither [`Send`] nor [`Sync`] so safe
/// Rust cannot move or share a sample across threads.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<tach::ThreadCpuInstant>();
/// ```
///
/// ```compile_fail
/// fn require_sync<T: Sync>() {}
/// require_sync::<tach::ThreadCpuInstant>();
/// ```
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ThreadCpuInstant {
  nanos: u64,
  not_send_or_sync: PhantomData<*mut ()>,
}

impl PartialOrd for ThreadCpuInstant {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    if (self.nanos ^ other.nanos) & WALL_DOMAIN_BIT != 0 {
      None
    } else {
      (self.nanos & VALUE_MASK).partial_cmp(&(other.nanos & VALUE_MASK))
    }
  }
}

impl ThreadCpuInstant {
  /// Reads the fastest available current-thread timeline.
  ///
  /// This is scheduled CPU time where the target provides it and an explicitly
  /// reported monotonic-wall fallback otherwise. The call never fails.
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now() -> Self {
    Self::from_nanos(arch::thread_cpu::now_nanos())
  }

  /// Reports the provider selected for the current OS thread.
  ///
  /// With the default `thread-cpu-inline` feature, Linux measures the candidates
  /// once per process. Each thread then installs the process-selected perf
  /// event or falls back locally if setup is unavailable. Calling this method
  /// initializes the calling thread if needed.
  #[inline]
  pub fn provider() -> ThreadCpuProvider {
    arch::thread_cpu::provider()
  }

  /// Reports the actual steady-state read-cost class selected for this thread.
  ///
  /// Calling this method initializes the current thread's provider if needed.
  #[inline]
  pub fn read_cost_hint() -> ThreadCpuReadCost {
    arch::thread_cpu::read_cost_hint()
  }

  /// Returns whether this sample came from a current-thread CPU-time provider.
  #[inline]
  #[must_use]
  pub const fn measures_thread_cpu_time(&self) -> bool {
    self.nanos & WALL_DOMAIN_BIT == 0
  }

  /// Returns the duration elapsed on this sample's timeline.
  ///
  /// Returns zero if the provider changed between the two reads.
  #[inline]
  #[must_use]
  pub fn elapsed(&self) -> Duration {
    Self::now().duration_since(*self)
  }

  /// Returns the duration from `earlier` to `self`, or zero if `earlier` is
  /// later.
  ///
  /// Both values must have been sampled on the same OS thread and in the same
  /// time domain. A provider-domain change returns zero.
  #[inline]
  #[must_use]
  pub fn duration_since(&self, earlier: Self) -> Duration {
    self.checked_duration_since(earlier).unwrap_or_default()
  }

  /// Returns the duration from `earlier` to `self`, or `None` if `earlier` is
  /// later or the samples use different time domains.
  ///
  /// Both values must have been sampled on the same OS thread.
  #[inline]
  #[must_use]
  pub fn checked_duration_since(&self, earlier: Self) -> Option<Duration> {
    if (self.nanos ^ earlier.nanos) & WALL_DOMAIN_BIT != 0 {
      return None;
    }
    let delta = (self.nanos & VALUE_MASK).checked_sub(earlier.nanos & VALUE_MASK)?;
    if self.measures_thread_cpu_time() {
      Some(Duration::from_nanos(delta))
    } else {
      Some(crate::instant::ticks_to_duration(delta))
    }
  }

  /// Returns the duration from `earlier` to `self`, saturating at zero.
  ///
  /// Both values must have been sampled on the same OS thread.
  #[inline]
  #[must_use]
  pub fn saturating_duration_since(&self, earlier: Self) -> Duration {
    self.duration_since(earlier)
  }

  /// Returns `Some(self + duration)` if the result is representable.
  #[inline]
  #[must_use]
  pub fn checked_add(&self, duration: Duration) -> Option<Self> {
    let domain = self.nanos & WALL_DOMAIN_BIT;
    let delta = if self.measures_thread_cpu_time() {
      duration_to_nanos(duration)?
    } else {
      crate::instant::duration_to_ticks(duration)?
    };
    let nanos = (self.nanos & VALUE_MASK).checked_add(delta)?;
    (nanos <= VALUE_MASK).then(|| Self::from_nanos(domain | nanos))
  }

  /// Returns `Some(self - duration)` if the result is representable.
  #[inline]
  #[must_use]
  pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
    let domain = self.nanos & WALL_DOMAIN_BIT;
    let delta = if self.measures_thread_cpu_time() {
      duration_to_nanos(duration)?
    } else {
      crate::instant::duration_to_ticks(duration)?
    };
    (self.nanos & VALUE_MASK)
      .checked_sub(delta)
      .map(|nanos| Self::from_nanos(domain | nanos))
  }

  #[inline]
  const fn from_nanos(nanos: u64) -> Self {
    Self { nanos, not_send_or_sync: PhantomData }
  }
}

impl Add<Duration> for ThreadCpuInstant {
  type Output = Self;

  fn add(self, rhs: Duration) -> Self::Output {
    self
      .checked_add(rhs)
      .expect("overflow when adding duration to thread CPU instant")
  }
}

impl AddAssign<Duration> for ThreadCpuInstant {
  fn add_assign(&mut self, rhs: Duration) {
    *self = *self + rhs;
  }
}

impl Sub<Duration> for ThreadCpuInstant {
  type Output = Self;

  fn sub(self, rhs: Duration) -> Self::Output {
    self
      .checked_sub(rhs)
      .expect("overflow when subtracting duration from thread CPU instant")
  }
}

impl SubAssign<Duration> for ThreadCpuInstant {
  fn sub_assign(&mut self, rhs: Duration) {
    *self = *self - rhs;
  }
}

impl Sub for ThreadCpuInstant {
  type Output = Duration;

  fn sub(self, rhs: Self) -> Self::Output {
    self.duration_since(rhs)
  }
}

#[inline]
fn duration_to_nanos(duration: Duration) -> Option<u64> {
  let nanos = duration.as_nanos().try_into().ok()?;
  (nanos <= VALUE_MASK).then_some(nanos)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn samples_are_monotonic_on_the_current_thread() {
    let mut previous = ThreadCpuInstant::now();
    for _ in 0..10_000 {
      let current = ThreadCpuInstant::now();
      assert!(current >= previous, "thread CPU clock moved backward");
      previous = current;
    }
  }

  #[test]
  fn duration_arithmetic_is_exact_in_nanoseconds() {
    let start = ThreadCpuInstant::now();
    let delta = Duration::from_nanos(1_234_567);
    let later = start.checked_add(delta).expect("small addition must fit");
    assert_eq!(later.duration_since(start), delta);
    assert_eq!(later.checked_sub(delta), Some(start));
    assert_eq!(start.duration_since(later), Duration::ZERO);
    assert!(start.checked_duration_since(later).is_none());
  }

  #[test]
  fn selected_provider_is_reported_truthfully() {
    let provider = ThreadCpuInstant::provider();
    let cost = ThreadCpuInstant::read_cost_hint();
    match provider {
      ThreadCpuProvider::LinuxPerfMmap => assert_eq!(cost, ThreadCpuReadCost::Inline),
      ThreadCpuProvider::PosixThreadCpuClock | ThreadCpuProvider::WindowsThreadTimes => {
        assert_eq!(cost, ThreadCpuReadCost::SystemCall);
      }
      ThreadCpuProvider::WasiThreadCpuClock
      | ThreadCpuProvider::PerformanceNow
      | ThreadCpuProvider::MonotonicWallClock => {}
    }

    #[cfg(all(
      any(target_os = "linux", target_os = "macos"),
      not(all(
        feature = "thread-cpu-inline",
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64"),
      )),
    ))]
    assert_eq!(provider, ThreadCpuProvider::PosixThreadCpuClock);
    #[cfg(target_os = "windows")]
    assert_eq!(provider, ThreadCpuProvider::WindowsThreadTimes);
  }

  #[test]
  fn cross_domain_durations_fail_closed() {
    let cpu = ThreadCpuInstant::from_nanos(100);
    let wall = ThreadCpuInstant::from_nanos(encode_wall_ticks(200));
    assert!(cpu.measures_thread_cpu_time());
    assert!(!wall.measures_thread_cpu_time());
    assert_eq!(wall.duration_since(cpu), Duration::ZERO);
    assert_eq!(cpu.duration_since(wall), Duration::ZERO);
    assert!(wall.checked_duration_since(cpu).is_none());
    assert_eq!(wall.partial_cmp(&cpu), None);
    assert_eq!(cpu.partial_cmp(&wall), None);
    assert_eq!(wall.checked_add(Duration::ZERO), Some(wall));
  }
}
