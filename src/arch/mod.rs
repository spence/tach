use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
pub mod fallback;
#[cfg(target_arch = "loongarch64")]
pub mod loongarch64;
// Strict-monotonicity enforcement module. Not compiled on platforms whose
// execution model already guarantees strict cross-thread monotonicity by
// structure (wasm32 single-threaded JS realm; WASI single-threaded execution
// model). Matches the cfg gate in `direct::ticks_monotonic`.
#[cfg(not(any(
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
  target_os = "wasi",
)))]
mod monotonic;
#[cfg(target_arch = "riscv64")]
pub mod riscv64;
#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
pub mod wasm;
#[cfg(target_arch = "x86")]
pub mod x86;
#[cfg(target_arch = "x86_64")]
pub mod x86_64;

mod direct;
pub use direct::{ticks, ticks_monotonic, ticks_ordered};

// Cached at first elapsed() call. Stored as fixed-point Q32:
//   nanos_per_tick_q32 = (1e9 << 32) / frequency
// Then converting ticks to nanos becomes (ticks * scale) >> 32, replacing
// the per-call u128 division with a multiply + shift.
//
// `pub(crate)` so the background-recal thread can issue its own
// Acquire/Release cycle without going through the wholesale-replace
// public `recalibrate()` path.
pub(crate) static NANOS_PER_TICK_Q32: AtomicU64 = AtomicU64::new(0);

pub(crate) const NANOS_PER_SECOND_Q32: u128 = 1_000_000_000u128 << 32;

#[inline]
#[must_use]
pub fn nanos_per_tick_q32() -> u64 {
  let cached = NANOS_PER_TICK_Q32.load(Ordering::Relaxed);
  if cached != 0 {
    return cached;
  }
  let freq = read_frequency();
  let scale = u64::try_from(NANOS_PER_SECOND_Q32 / u128::from(freq)).unwrap_or(u64::MAX);
  NANOS_PER_TICK_Q32.store(scale, Ordering::Relaxed);

  // Spawn the periodic recalibration thread on first use. Only compiled
  // when the `recalibrate-background` feature is enabled — which is the
  // only feature that pulls in `std`.
  #[cfg(feature = "recalibrate-background")]
  crate::background::ensure_thread();

  scale
}

// aarch64 on macOS / Windows / non-Linux: `cntfrq_el0` is authoritative.
// Apple writes the per-die measured timebase; Windows derives it from QPF.
#[cfg(all(target_arch = "aarch64", not(target_os = "linux")))]
#[inline]
fn read_frequency() -> u64 {
  aarch64::cntfrq()
}

// aarch64 on Linux: `cntfrq_el0` is the firmware-published nominal value
// (typically a round 1.000 GHz), not the actual crystal frequency. The
// underlying crystal can be 10-30 ppm off — on Graviton 3 specifically it's
// about -27 ppm. Calibrate against `clock_gettime(CLOCK_MONOTONIC)`, whose
// vDSO scaling factor already includes the kernel's NTP correction, so the
// measured rate ends up wall-clock-correct.
#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
#[inline]
fn read_frequency() -> u64 {
  crate::calibration::calibrate_frequency()
}

#[cfg(all(not(target_arch = "aarch64"), target_os = "macos"))]
#[inline]
fn read_frequency() -> u64 {
  // mach_timebase_info reports (numer, denom) such that
  //   nanoseconds = ticks * numer / denom
  // so the effective tick rate is 1e9 * denom / numer Hz.
  #[repr(C)]
  struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
  }
  unsafe extern "C" {
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
  }
  let mut info = MachTimebaseInfo { numer: 0, denom: 0 };
  // SAFETY: `mach_timebase_info` populates the struct.
  unsafe { mach_timebase_info(&mut info) };
  1_000_000_000u64 * u64::from(info.denom) / u64::from(info.numer)
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[inline]
fn read_frequency() -> u64 {
  // ticks() returns nanos directly; identity Q32 transform.
  1_000_000_000
}

#[cfg(target_os = "wasi")]
#[inline]
fn read_frequency() -> u64 {
  // ticks() returns nanos from clock_time_get directly; identity transform.
  1_000_000_000
}

// x86 (non-macOS): prefer CPUID leaf 15h when available — modern Intel
// (Skylake+) and AMD (Zen2+) expose the exact architectural TSC frequency,
// eliminating the ~500 ppm error baked into spin-loop calibration. Fall back
// to calibration on older / virtualized CPUs that zero the leaf (typical on
// Firecracker, Azure VMs, and some Hyper-V guests).
//
// On Windows, calibration uses QueryPerformanceCounter as the wall-clock
// reference. Reading QPF directly here would be wrong — QPF reports the QPC
// frequency (~10 MHz on modern Windows), not the RDTSC tick rate (~3 GHz);
// the two are unrelated.
#[cfg(all(any(target_arch = "x86_64", target_arch = "x86"), not(target_os = "macos")))]
#[inline]
fn read_frequency() -> u64 {
  #[cfg(target_arch = "x86_64")]
  if let Some(hz) = x86_64::cpuid_tsc_hz() {
    return hz;
  }
  #[cfg(target_arch = "x86")]
  if let Some(hz) = x86::cpuid_tsc_hz() {
    return hz;
  }
  crate::calibration::calibrate_frequency()
}

#[cfg(not(any(
  target_arch = "aarch64",
  target_arch = "x86_64",
  target_arch = "x86",
  target_os = "macos",
  target_os = "wasi",
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
)))]
#[inline]
fn read_frequency() -> u64 {
  crate::calibration::calibrate_frequency()
}

/// Re-derive the tick-to-nanosecond scaling against the platform monotonic
/// clock and atomically replace the cached Q32 reciprocal. The next
/// `ticks_to_duration` call observes the new value via the Acquire/Relaxed
/// load in `nanos_per_tick_q32`.
///
/// On platforms where the frequency comes from an authoritative register or
/// OS API (macOS `mach_timebase_info`, Windows aarch64 `cntfrq_el0`,
/// WASI / wasm fixed at 1 GHz), this is a no-op — re-reading would just
/// yield the same value. On x86 / x86_64 (Linux, other Unixes, and
/// Windows) and on aarch64 Linux, recalibration measures the actual
/// counter rate against the platform monotonic clock
/// (`clock_gettime(CLOCK_MONOTONIC)` on Unix, `QueryPerformanceCounter` on
/// Windows).
///
/// This is the manual / wholesale-replace path — the new scale takes
/// effect immediately. The background-recal thread (when the
/// `recalibrate-background` feature is enabled) feeds new measurements
/// through an EMA blender instead via [`recalibrate_measure`], so a single
/// noisy calibration window can't jolt the scale on virtualized hosts.
///
/// `recalibrate` is `#![no_std]`-compatible; it uses the same spin-loop +
/// platform-monotonic path that already runs at startup.
pub fn recalibrate() {
  if let Some(new_hz) = measure_frequency_for_recal() {
    let scale = u64::try_from(NANOS_PER_SECOND_Q32 / u128::from(new_hz)).unwrap_or(u64::MAX);
    NANOS_PER_TICK_Q32.store(scale, Ordering::Release);
  }
}

/// Re-measure the architectural counter's frequency against the platform
/// monotonic clock without committing it to the global scale. Used by the
/// background-recal thread to feed an EMA blender; callers of the public
/// [`crate::Instant::recalibrate`] get wholesale-replace semantics via
/// [`recalibrate`] above.
///
/// Returns `None` on platforms where there's nothing to measure (macOS,
/// Windows aarch64, WASI, wasm — frequency source is authoritative).
#[cfg(feature = "recalibrate-background")]
pub(crate) fn recalibrate_measure() -> Option<u64> {
  measure_frequency_for_recal()
}

// Skip CPUID 15h here: it reports *nominal* frequency, which doesn't change.
// Recalibration's job is to track *actual* frequency drift over uptime, so we
// go straight to the platform-monotonic spin-loop calibration. Returns None
// (don't update) when the rate measurement was unusable, e.g. survivor-list
// fallback returned 0, or when the platform's frequency source is already
// authoritative (macOS, Windows aarch64, wasm).
#[inline]
fn measure_frequency_for_recal() -> Option<u64> {
  #[cfg(any(
    all(any(target_arch = "x86_64", target_arch = "x86"), not(target_os = "macos")),
    all(target_arch = "aarch64", target_os = "linux"),
  ))]
  {
    let hz = crate::calibration::calibrate_frequency();
    if hz > 0 { Some(hz) } else { None }
  }
  #[cfg(not(any(
    all(any(target_arch = "x86_64", target_arch = "x86"), not(target_os = "macos")),
    all(target_arch = "aarch64", target_os = "linux"),
  )))]
  {
    None
  }
}
