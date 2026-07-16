// Direct OS clocks used when the platform owns the reliable monotonic
// timeline. Each submodule is cfg-gated to its platform; `direct::ticks()`
// selects one based on both target OS and architecture.

#[cfg(target_os = "macos")]
mod mach {
  #[repr(C)]
  struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
  }

  unsafe extern "C" {
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
  }

  #[inline(always)]
  pub fn mach_time() -> u64 {
    // SAFETY: `mach_absolute_time` takes no arguments and returns the host
    // monotonic tick value with no Rust-side aliasing requirements.
    unsafe { mach_absolute_time() }
  }

  #[inline]
  pub fn mach_timebase() -> (u32, u32) {
    let mut info = MachTimebaseInfo { numer: 0, denom: 0 };
    // SAFETY: `info` is writable storage with the documented Darwin ABI.
    let _ = unsafe { mach_timebase_info(&mut info) };

    // Darwin documents a successful, non-zero ratio. Keep the conversion
    // total even if a non-conforming host violates that contract.
    (info.numer.max(1), info.denom.max(1))
  }
}

#[cfg(target_os = "macos")]
pub use mach::*;

#[cfg_attr(
  any(
    target_os = "android",
    all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x"))
  ),
  allow(dead_code)
)]
#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten")),))]
mod monotonic {
  #[cfg(all(target_os = "linux", target_arch = "powerpc64", not(target_env = "gnu"),))]
  use core::sync::atomic::{Ordering, fence};

  #[cfg(all(target_os = "linux", target_arch = "powerpc64", not(target_env = "gnu"),))]
  #[inline(always)]
  fn ordered_clock_barrier() {
    // Acquire fences are compiler-only on s390x and lower to `lwsync` on
    // powerpc64. Neither serializes a following non-storage clock operation.
    // SeqCst is the minimum Rust primitive that lowers to the required
    // execution barrier: `bcr 15,0` on s390x and heavyweight `sync` on
    // powerpc64. Power ISA v3.1 Book II section 4.6.3 specifies that hwsync
    // completes all prior instructions before any later instruction starts,
    // which orders the vDSO's Time Base read itself rather than only its
    // surrounding memory accesses.
    fence(Ordering::SeqCst);
  }

  #[inline(always)]
  pub fn clock_monotonic() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `ts` is writable storage with the platform libc ABI, and
    // CLOCK_MONOTONIC is valid on every Unix target routed here.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    timespec_nanos(rc, ts)
  }

  #[cfg(all(target_os = "linux", target_arch = "powerpc64", not(target_env = "gnu"),))]
  #[inline(always)]
  pub fn clock_monotonic_ordered() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    ordered_clock_barrier();
    // SAFETY: `ts` is writable storage with the Linux libc ABI, and
    // CLOCK_MONOTONIC is a valid clock ID.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    timespec_nanos(rc, ts)
  }

  #[inline(always)]
  fn timespec_nanos(status: i32, value: libc::timespec) -> u64 {
    if status != 0 {
      return 0;
    }
    value.tv_sec as u64 * 1_000_000_000 + value.tv_nsec as u64
  }

  #[cfg(test)]
  mod tests {
    use super::*;

    #[test]
    fn clock_failure_has_a_total_value() {
      let value = libc::timespec { tv_sec: 123, tv_nsec: 456 };
      assert_eq!(timespec_nanos(-1, value), 0);
    }
  }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten")),))]
#[allow(unused_imports)]
pub use monotonic::*;

#[cfg(all(target_os = "wasi", target_env = "p1"))]
mod wasip1 {
  #[link(wasm_import_module = "wasi_snapshot_preview1")]
  unsafe extern "C" {
    fn clock_time_get(id: u32, precision: u64, time: *mut u64) -> u16;
  }

  const CLOCK_MONOTONIC: u32 = 1;

  #[inline(always)]
  pub fn wasi_clock_monotonic() -> u64 {
    let mut t: u64 = 0;
    // SAFETY: writes a single u64 the host fills in. CLOCK_MONOTONIC and
    // precision=0 are always-valid inputs for wasi_snapshot_preview1.
    let _ = unsafe { clock_time_get(CLOCK_MONOTONIC, 0, &mut t) };
    t
  }
}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
pub use wasip1::*;

#[cfg(all(target_os = "wasi", target_env = "p2"))]
mod wasi_p2 {
  #[inline(always)]
  pub fn wasi_clock_monotonic() -> u64 {
    wasip2::clocks::monotonic_clock::now()
  }
}

#[cfg(all(target_os = "wasi", target_env = "p2"))]
pub use wasi_p2::*;

#[cfg(target_os = "windows")]
mod qpc {
  use core::sync::atomic::{AtomicI64, Ordering};

  unsafe extern "system" {
    fn QueryPerformanceCounter(c: *mut i64) -> i32;
    fn QueryPerformanceFrequency(f: *mut i64) -> i32;
  }

  // QueryPerformanceCounter is the OS-designated high-resolution monotonic wall
  // clock on every supported Windows target, so both wall contracts read it and
  // scale by the cached QueryPerformanceFrequency. Windows owns whatever
  // cross-processor synchronization, hypervisor scaling, bias, and live-migration
  // continuity its backing source needs. A raw TSC/CNTVCT read does not inherit
  // those guarantees; the precise interrupt-time APIs expose a slower 100 ns
  // domain; and the coarse/UTC/auxiliary clocks do not satisfy the
  // high-resolution monotonic contract. `OrderedInstant` needs no separate
  // fence: the opaque QueryPerformanceCounter call boundary prevents the
  // compiler from moving the read across a prior Acquire load, and the
  // Windows-owned timeline orders events across processors.
  static QPC_FREQ: AtomicI64 = AtomicI64::new(0);

  #[inline]
  pub fn qpc_frequency() -> u64 {
    let cached = QPC_FREQ.load(Ordering::Relaxed);
    if cached != 0 {
      return cached as u64;
    }
    let mut f: i64 = 0;
    // SAFETY: writes a single i64.
    let _ = unsafe { QueryPerformanceFrequency(&mut f) };
    f = f.max(1);
    QPC_FREQ.store(f, Ordering::Relaxed);
    f as u64
  }

  /// Reads the OS-designated high-resolution monotonic wall clock for `Instant`.
  #[inline(always)]
  pub fn windows_ticks() -> u64 {
    qpc_ticks()
  }

  /// Reads the same wall clock for `OrderedInstant`, ordered after prior
  /// Acquire-or-stronger observations by the opaque QueryPerformanceCounter call
  /// boundary.
  #[inline(always)]
  pub fn windows_ticks_ordered() -> u64 {
    qpc_ticks()
  }

  /// Reads `OrderedInstant`'s numeric domain without the call-boundary ordering
  /// used by its start and ordered-end reads. QPC is a single domain, so this is
  /// the same read.
  #[inline(always)]
  pub fn windows_ticks_ordered_unordered() -> u64 {
    qpc_ticks()
  }

  #[inline]
  pub fn instant_frequency() -> u64 {
    qpc_frequency()
  }

  #[inline]
  pub fn ordered_frequency() -> u64 {
    qpc_frequency()
  }

  #[inline(always)]
  pub fn qpc_ticks() -> u64 {
    let mut c: i64 = 0;
    // SAFETY: writes a single i64.
    let _ = unsafe { QueryPerformanceCounter(&mut c) };
    c as u64
  }

  #[cfg(feature = "bench-internal")]
  pub(crate) fn bench_instant_provider() -> &'static str {
    "windows_qpc"
  }

  #[cfg(feature = "bench-internal")]
  pub(crate) fn bench_ordered_provider() -> &'static str {
    "windows_qpc_call_boundary"
  }

  #[cfg(feature = "bench-internal")]
  #[inline(always)]
  pub(crate) fn bench_instant_qpc() -> u64 {
    qpc_ticks()
  }

  #[cfg(test)]
  mod wall_tests {
    use super::*;

    #[test]
    fn selected_domains_are_monotonic_and_share_the_qpc_frequency() {
      let instant_before = windows_ticks();
      let ordered_before = windows_ticks_ordered();
      assert!(instant_frequency() > 0);
      assert!(ordered_frequency() > 0);
      assert!(windows_ticks() >= instant_before);
      assert!(windows_ticks_ordered() >= ordered_before);
      // The unordered endpoint reads the same QPC domain as the ordered read.
      assert!(windows_ticks_ordered_unordered() >= ordered_before);
      assert_eq!(instant_frequency(), qpc_frequency());
      assert_eq!(ordered_frequency(), qpc_frequency());
    }
  }
}

#[cfg(target_os = "windows")]
pub use qpc::*;
