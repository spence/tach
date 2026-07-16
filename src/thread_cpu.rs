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
#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten"))))]
pub(crate) const fn is_wall_value(value: u64) -> bool {
  value & WALL_DOMAIN_BIT != 0
}

/// The native mechanism used to read current-thread CPU time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ThreadCpuProvider {
  /// Linux `perf_event_open` task-clock metadata mapped and read in userspace.
  LinuxPerfMmap,
  /// Linux `perf_event_open` task-clock read through its persistent file
  /// descriptor.
  LinuxPerfRead,
  /// The operating system's POSIX current-thread CPU clock.
  PosixThreadCpuClock,
  /// Windows `GetThreadTimes`, combining kernel and user execution time.
  WindowsThreadTimes,
  /// A WASIp1 host's optional current-thread CPU clock.
  WasiThreadCpuClock,
  /// Node.js `process.threadCpuUsage()`, combining user and system CPU time for
  /// the current main or worker thread.
  NodeThreadCpuUsage,
  /// The JavaScript host's monotonic `performance.now()` wall clock.
  ///
  /// This advances while the thread is descheduled or sleeping.
  PerformanceNow,
  /// Node.js `process.hrtime.bigint()`, selected as a monotonic wall clock.
  ///
  /// This advances while the thread is descheduled or sleeping.
  NodeHrtime,
  /// The target's fastest monotonic wall clock.
  ///
  /// This fallback is used when the target has no current-thread CPU clock, or
  /// if a native thread clock unexpectedly becomes unavailable. It advances
  /// while the thread is descheduled or sleeping.
  MonotonicWallClock,
  /// No eligible monotonic clock is exposed by the host.
  ///
  /// Reads remain total and return a frozen value rather than substituting a
  /// non-monotonic source.
  Unavailable,
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
        | Self::LinuxPerfRead
        | Self::PosixThreadCpuClock
        | Self::WindowsThreadTimes
        | Self::WasiThreadCpuClock
        | Self::NodeThreadCpuUsage
    )
  }
}

/// A conservative steady-state cost class for the selected provider.
///
/// This is categorical because exact nanosecond cost depends on the CPU,
/// kernel, and virtualization environment. It lets applications validate or
/// report the selected mechanism without treating a machine-specific
/// benchmark as a stable API promise. `Inline` means tach's selected hot path
/// directly reads mapped metadata or an architectural counter without issuing
/// an operating-system or host call. A hypervisor or compatibility kernel may
/// still trap and emulate that instruction; the initialization tournament
/// measures the resulting cost and retains the route only when its complete
/// public path wins materially. A selected libc clock is conservatively
/// classified as [`SystemCall`](Self::SystemCall) because its implementation
/// may choose either a vDSO or kernel entry at runtime. An `Inline` result can
/// describe an explicit wall-clock fallback; pair this hint with
/// [`ThreadCpuInstant::measures_thread_cpu_time`] when CPU-time semantics are
/// required.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ThreadCpuReadCost {
  /// Tach issues no operating-system or host call on the selected read path.
  ///
  /// Architectural instructions can still be virtualized. Provider selection
  /// measures that effective path rather than inferring its cost from a
  /// capability bit.
  Inline,
  /// A route that explicitly invokes an operating-system clock entry.
  ///
  /// This includes libc clocks whose runtime implementation may be either a
  /// userspace vDSO or a kernel entry.
  SystemCall,
  /// A call across the WebAssembly host boundary.
  HostCall,
  /// The host exposes no eligible provider, so reads return a frozen value.
  Unavailable,
}

/// A sampled point in the current OS thread's scheduled CPU-time timeline.
///
/// On targets with a native current-thread CPU clock, this timeline advances
/// only while the calling thread executes on a CPU and freezes while that
/// thread is parked, descheduled, or sleeping. Values are normalized to
/// nanoseconds. Eligible native candidates must preserve the platform clock's
/// native scheduled-runtime precision; tach never substitutes coarsened thread
/// accounting merely because its read is cheaper.
///
/// Targets without a native thread clock use their fastest monotonic wall
/// source so this API remains available everywhere tach supports. That
/// fallback advances during descheduling. Call [`Self::provider`] and
/// [`ThreadCpuProvider::measures_thread_cpu_time`] when the distinction is
/// correctness-sensitive.
///
/// Provider choice is stable in normal operation. If Linux's selected perf
/// mechanism becomes unreadable, tach aligns the native POSIX fallback to the
/// last published sample before changing the reported provider. If no native
/// thread clock remains available, tach returns a tagged wall sample rather
/// than failing the call. Durations spanning a CPU/wall boundary return zero (or
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
  /// Reads tach's selected current-thread timeline.
  ///
  /// This is scheduled CPU time where the target provides it and an explicitly
  /// reported monotonic-wall fallback otherwise. The call never fails.
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now() -> Self {
    Self::from_nanos(arch::thread_cpu::now_nanos())
  }

  #[cfg(all(feature = "bench-internal", target_os = "windows"))]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(crate) fn bench_windows_wall_fallback_now() -> Self {
    Self::from_nanos(encode_wall_ticks(arch::ticks()))
  }

  /// Reports the provider selected for the current OS thread.
  ///
  /// Targets with multiple semantically equivalent native entry paths measure
  /// those complete paths at initialization. Linux-kernel perf candidates are
  /// assessed separately for each OS thread because availability and cost can
  /// differ by thread or CPU. Calling this method initializes the calling
  /// thread's choice when needed.
  #[inline]
  pub fn provider() -> ThreadCpuProvider {
    arch::thread_cpu::provider()
  }

  /// Reports a conservative steady-state read-cost class for this thread.
  ///
  /// Calling this method initializes the current thread's provider if needed.
  #[inline]
  pub fn read_cost_hint() -> ThreadCpuReadCost {
    arch::thread_cpu::read_cost_hint()
  }

  /// Returns the maximum gap between reads required by a wrapping inline
  /// provider, if the selected provider has such a constraint.
  ///
  /// Linux perf metadata can expose a shortened architectural counter. Tach
  /// owns its wrap extension and returns a conservative half-window bound so a
  /// read interrupted by a signal is distinguishable from a genuine wrap.
  /// Callers must sample before that strict bound while continuously scheduled.
  /// `None` means the selected provider has no practical read-gap constraint.
  #[inline]
  #[must_use]
  pub fn max_read_gap() -> Option<Duration> {
    arch::thread_cpu::max_read_gap()
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
  #[inline(always)]
  #[allow(clippy::inline_always)]
  #[must_use]
  pub fn elapsed(&self) -> Duration {
    #[cfg(any(
      target_os = "wasi",
      target_os = "emscripten",
      all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
    ))]
    if self.nanos & WALL_DOMAIN_BIT != 0 {
      // These host wall selections are sticky and share `arch::ticks()`'s
      // nanosecond timeline, so a wall-tagged sample needs no CPU-provider dispatch.
      let now = encode_wall_ticks(arch::ticks());
      return Duration::from_nanos(now.saturating_sub(self.nanos));
    }
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
    #[cfg(any(
      target_os = "wasi",
      target_os = "emscripten",
      all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
    ))]
    {
      if (self.nanos ^ earlier.nanos) & WALL_DOMAIN_BIT != 0 {
        return Duration::ZERO;
      }
      return Duration::from_nanos(self.nanos.saturating_sub(earlier.nanos));
    }
    #[cfg(all(target_os = "macos", not(test)))]
    {
      Duration::from_nanos(self.nanos.saturating_sub(earlier.nanos))
    }
    #[cfg(all(
      any(not(target_os = "macos"), test),
      not(any(
        target_os = "wasi",
        target_os = "emscripten",
        all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
      )),
    ))]
    {
      if (self.nanos | earlier.nanos) & WALL_DOMAIN_BIT == 0 {
        return Duration::from_nanos(self.nanos.saturating_sub(earlier.nanos));
      }
      duration_since_wall_or_mixed(*self, earlier)
    }
  }

  /// Returns the duration from `earlier` to `self`, or `None` if `earlier` is
  /// later or the samples use different time domains.
  ///
  /// Both values must have been sampled on the same OS thread.
  #[inline]
  #[must_use]
  pub fn checked_duration_since(&self, earlier: Self) -> Option<Duration> {
    #[cfg(any(
      target_os = "wasi",
      target_os = "emscripten",
      all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
    ))]
    {
      if (self.nanos ^ earlier.nanos) & WALL_DOMAIN_BIT != 0 {
        return None;
      }
      return self.nanos.checked_sub(earlier.nanos).map(Duration::from_nanos);
    }
    #[cfg(all(target_os = "macos", not(test)))]
    {
      self.nanos.checked_sub(earlier.nanos).map(Duration::from_nanos)
    }
    #[cfg(all(
      any(not(target_os = "macos"), test),
      not(any(
        target_os = "wasi",
        target_os = "emscripten",
        all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
      )),
    ))]
    {
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

#[cfg(all(
  any(not(target_os = "macos"), test),
  not(any(
    target_os = "wasi",
    target_os = "emscripten",
    all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
  )),
))]
#[cold]
#[inline(never)]
fn duration_since_wall_or_mixed(later: ThreadCpuInstant, earlier: ThreadCpuInstant) -> Duration {
  later.checked_duration_since(earlier).unwrap_or_default()
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
