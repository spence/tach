// Direct OS-clock fallbacks for targets without an architectural counter.
// Each submodule is cfg-gated to its platform; `direct::ticks()` selects one
// based on target_os.

#[cfg(all(
  target_os = "macos",
  not(any(
    target_arch = "x86_64",
    target_arch = "x86",
    target_arch = "aarch64",
    target_arch = "riscv64",
    target_arch = "loongarch64",
  )),
))]
mod mach {
  unsafe extern "C" {
    fn mach_absolute_time() -> u64;
  }

  #[inline(always)]
  pub fn mach_time() -> u64 {
    // SAFETY: `mach_absolute_time` takes no arguments and returns the host
    // monotonic tick value with no Rust-side aliasing requirements.
    unsafe { mach_absolute_time() }
  }
}

#[cfg(all(
  target_os = "macos",
  not(any(
    target_arch = "x86_64",
    target_arch = "x86",
    target_arch = "aarch64",
    target_arch = "riscv64",
    target_arch = "loongarch64",
  )),
))]
pub use mach::*;

#[cfg(all(
  unix,
  not(target_os = "macos"),
  not(all(target_arch = "aarch64", not(target_os = "linux"))),
))]
mod monotonic {
  #[repr(C)]
  struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
  }

  const CLOCK_MONOTONIC: i32 = 1;

  unsafe extern "C" {
    fn clock_gettime(clk_id: i32, tp: *mut Timespec) -> i32;
  }

  #[inline(always)]
  pub fn clock_monotonic() -> u64 {
    let mut ts = Timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `ts` is a valid, writable `timespec` pointer for the duration of the call.
    let rc = unsafe { clock_gettime(CLOCK_MONOTONIC, &mut ts) };
    debug_assert_eq!(rc, 0);
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
  }
}

#[cfg(all(
  unix,
  not(target_os = "macos"),
  not(all(target_arch = "aarch64", not(target_os = "linux"))),
))]
pub use monotonic::*;

#[cfg(target_os = "wasi")]
mod wasi {
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

#[cfg(target_os = "wasi")]
pub use wasi::*;

#[cfg(all(target_os = "windows", not(target_arch = "aarch64")))]
mod qpc {
  use core::sync::atomic::{AtomicI64, Ordering};

  unsafe extern "system" {
    fn QueryPerformanceCounter(c: *mut i64) -> i32;
    fn QueryPerformanceFrequency(f: *mut i64) -> i32;
  }

  // QueryPerformanceFrequency is constant for the lifetime of the system, so
  // cache it on first read. Without this, calibration's tight spin-loop polls
  // QPF on every iteration and the syscall overhead inflates the measured
  // wall-elapsed by tens of nanoseconds per call.
  static QPC_FREQ: AtomicI64 = AtomicI64::new(0);

  #[inline]
  fn freq() -> i64 {
    let cached = QPC_FREQ.load(Ordering::Relaxed);
    if cached != 0 {
      return cached;
    }
    let mut f: i64 = 0;
    // SAFETY: writes a single i64.
    unsafe {
      QueryPerformanceFrequency(&mut f);
    }
    QPC_FREQ.store(f, Ordering::Relaxed);
    f
  }

  #[inline(always)]
  pub fn qpc_now_ns() -> u64 {
    let f = freq();
    let mut c: i64 = 0;
    // SAFETY: writes a single i64.
    unsafe {
      QueryPerformanceCounter(&mut c);
    }
    ((c as u128) * 1_000_000_000u128 / (f as u128)) as u64
  }
}

#[cfg(all(target_os = "windows", not(target_arch = "aarch64")))]
pub use qpc::*;
