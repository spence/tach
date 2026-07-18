//! Runtime wall-clock selection for Windows x86 and x86_64 `Instant`.
//!
//! `RDTSC` is ring-3-legal on Windows: user mode never sets `CR4.TSD`, and
//! Windows exposes no per-thread counter-disable analogue to Linux's
//! `PR_SET_TSC`, so the read cannot trap. Tach reads a bare `RDTSC` for
//! `Instant` only when the counter is rate-stable — CPUID advertises the
//! timestamp counter (leaf 1 `EDX[4]`) and an invariant TSC
//! (`CPUID.8000_0007H:EDX[8]`), the same rate-stability gate the Linux x86,
//! Apple, and FreeBSD providers require. That gate survives ADR-0007, which
//! relaxes only the cross-core value-consistency requirement for `Instant`, not
//! rate stability. The read is scaled by the calibrated counter frequency.
//!
//! `OrderedInstant` is **not** handled here. Its cross-core-consistent,
//! happens-before contract stays on `QueryPerformanceCounter` for every Windows
//! architecture (`src/arch/fallback.rs`), so this module never touches an
//! ordered-reachable path.
//!
//! When the counter is ineligible the `Instant` read degrades to the same
//! `QueryPerformanceCounter` timeline the ordered contract uses, read through
//! [`super::fallback::qpc_ticks`] and scaled by `QueryPerformanceFrequency`.
//! Because that frequency is authoritative, the calibration below references
//! QPC — Windows has no `clock_gettime` — rather than a monotonic syscall.

use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};

const PROVIDER_UNKNOWN: u8 = 0;
const PROVIDER_SELECTING: u8 = 1;
const PROVIDER_TSC: u8 = 2;
const PROVIDER_QPC: u8 = 3;

static INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static TSC_FREQUENCY: AtomicU64 = AtomicU64::new(0);

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  let provider = INSTANT_PROVIDER.load(Ordering::Relaxed);
  if provider == PROVIDER_TSC { read_tsc() } else { read_outlined_instant_provider(provider) }
}

#[inline(never)]
fn read_outlined_instant_provider(provider: u8) -> u64 {
  match provider {
    PROVIDER_TSC => read_tsc(),
    PROVIDER_QPC => super::fallback::qpc_ticks(),
    _ => ticks_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_after_selection() -> u64 {
  match selected_instant_provider() {
    PROVIDER_TSC => read_tsc(),
    _ => super::fallback::qpc_ticks(),
  }
}

#[inline(always)]
fn read_tsc() -> u64 {
  #[cfg(target_arch = "x86_64")]
  {
    // SAFETY: `_rdtsc` emits the timestamp-counter read and has no Rust memory
    // safety preconditions. RDTSC is ring-3-legal on Windows (user mode never
    // sets CR4.TSD), so it cannot trap.
    unsafe { core::arch::x86_64::_rdtsc() }
  }
  #[cfg(target_arch = "x86")]
  {
    // SAFETY: as above; i686 Windows leaves RDTSC enabled for ring 3.
    unsafe { core::arch::x86::_rdtsc() }
  }
}

#[inline]
pub fn instant_frequency() -> u64 {
  frequency_for(selected_instant_provider())
}

#[inline]
pub(crate) fn instant_uses_tsc() -> bool {
  selected_instant_provider() == PROVIDER_TSC
}

fn frequency_for(provider: u8) -> u64 {
  if provider != PROVIDER_TSC {
    // The QPC fallback reads counts in the QueryPerformanceFrequency domain, so
    // its scale divides by that authoritative rate, not 1 GHz.
    return super::fallback::qpc_frequency();
  }
  let cached = TSC_FREQUENCY.load(Ordering::Relaxed);
  if cached != 0 {
    return cached;
  }
  let hz = calibrate_tsc_frequency().max(1);
  TSC_FREQUENCY.store(hz, Ordering::Relaxed);
  hz
}

/// Re-measure the TSC rate against QPC and return the fresh Q32 scale, or
/// `None` when the QPC fallback is selected (its `QueryPerformanceFrequency` is
/// authoritative and never drifts). Ordered is untouched: it never recalibrates.
pub(crate) fn recalibrate_instant_scale() -> Option<u64> {
  if selected_instant_provider() != PROVIDER_TSC {
    return None;
  }
  let hz = calibrate_tsc_frequency();
  if hz == 0 {
    return None;
  }
  Some(super::scale_from_ratio(1_000_000_000, hz))
}

// Mirrors `linux_x86_wall::calibrate_tsc_frequency`: seven 100 ms windows, each
// rejected if preemption stretched it past 1.5x, then the median of the
// accepted samples (a lone accepted sample is its own median). The wall
// reference is QPC scaled by QueryPerformanceFrequency because Windows exposes
// no `clock_gettime`.
fn calibrate_tsc_frequency() -> u64 {
  const WINDOW_NS: u64 = 100_000_000;
  const MAX_OVERRUN_NS: u64 = WINDOW_NS * 3 / 2;
  const SAMPLE_COUNT: usize = 7;

  let mut samples = [0_u64; SAMPLE_COUNT];
  let mut accepted = 0;
  for _ in 0..SAMPLE_COUNT {
    let wall_start = calibration_wall_nanos();
    let tick_start = read_tsc();
    let mut wall_end;
    loop {
      wall_end = calibration_wall_nanos();
      if wall_end.saturating_sub(wall_start) >= WINDOW_NS {
        break;
      }
      core::hint::spin_loop();
    }
    let tick_end = read_tsc();
    let wall_elapsed = wall_end.saturating_sub(wall_start);
    if wall_elapsed == 0 || wall_elapsed > MAX_OVERRUN_NS {
      continue;
    }
    let ticks = tick_end.saturating_sub(tick_start);
    samples[accepted] =
      u64::try_from(u128::from(ticks).saturating_mul(1_000_000_000) / u128::from(wall_elapsed))
        .unwrap_or(u64::MAX);
    accepted += 1;
  }
  if accepted == 0 {
    return 1_000_000_000;
  }
  samples[..accepted].sort_unstable();
  samples[accepted / 2]
}

#[inline]
fn calibration_wall_nanos() -> u64 {
  let ticks = super::fallback::qpc_ticks();
  let frequency = super::fallback::qpc_frequency().max(1);
  u64::try_from(u128::from(ticks).saturating_mul(1_000_000_000) / u128::from(frequency))
    .unwrap_or(u64::MAX)
}

fn selected_instant_provider() -> u8 {
  loop {
    match INSTANT_PROVIDER.load(Ordering::Acquire) {
      PROVIDER_UNKNOWN => {
        if INSTANT_PROVIDER
          .compare_exchange(
            PROVIDER_UNKNOWN,
            PROVIDER_SELECTING,
            Ordering::AcqRel,
            Ordering::Acquire,
          )
          .is_ok()
        {
          let provider = detect_instant_provider();
          INSTANT_PROVIDER.store(provider, Ordering::Release);
          return provider;
        }
      }
      // A concurrent first reader waits for the winner to publish rather than
      // reading a different tick domain: TSC and QPC counts are not
      // interchangeable, so `now()` and a later `elapsed()` must agree.
      PROVIDER_SELECTING => core::hint::spin_loop(),
      provider => return provider,
    }
  }
}

#[cold]
#[inline(never)]
fn detect_instant_provider() -> u8 {
  let (has_tsc, invariant_tsc) = tsc_capabilities();
  if has_tsc && invariant_tsc { PROVIDER_TSC } else { PROVIDER_QPC }
}

#[allow(unused_unsafe)] // supported rustc versions differ on whether __cpuid is unsafe
fn tsc_capabilities() -> (bool, bool) {
  #[cfg(target_arch = "x86")]
  use core::arch::x86::__cpuid;
  #[cfg(target_arch = "x86_64")]
  use core::arch::x86_64::__cpuid;

  // SAFETY: supported x86 targets guarantee CPUID leaf zero.
  let basic = unsafe { __cpuid(0) };
  if basic.eax < 1 {
    return (false, false);
  }
  // SAFETY: the maximum basic leaf includes leaf one.
  let has_tsc = unsafe { __cpuid(1) }.edx & (1 << 4) != 0;
  // SAFETY: the extended maximum-leaf query is defined on CPUID systems.
  let extended = unsafe { __cpuid(0x8000_0000) };
  // SAFETY: the maximum extended leaf includes invariant-TSC metadata.
  let invariant_tsc =
    extended.eax >= 0x8000_0007 && unsafe { __cpuid(0x8000_0007) }.edx & (1 << 8) != 0;
  (has_tsc, invariant_tsc)
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_provider() -> &'static str {
  match selected_instant_provider() {
    PROVIDER_TSC => "windows_tsc",
    _ => "windows_qpc",
  }
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_tsc() -> u64 {
  read_tsc()
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_tsc_frequency() -> u64 {
  frequency_for(PROVIDER_TSC)
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_tsc_nanos_per_tick_q32() -> u64 {
  super::scale_from_ratio(1_000_000_000, frequency_for(PROVIDER_TSC))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn selected_instant_domain_is_monotonic_and_has_a_rate() {
    let before = ticks();
    assert!(instant_frequency() > 0);
    assert!(ticks() >= before);
  }

  #[test]
  fn tsc_selection_agrees_with_the_invariant_tsc_gate() {
    // The bare RDTSC path is chosen only where CPUID advertises a rate-stable
    // invariant TSC; otherwise the QPC fallback is selected. This proves
    // `tsc_capabilities` is a load-bearing eligibility gate, not tournament
    // cruft that ADR-0007's cross-core relaxation could drop.
    let (has_tsc, invariant_tsc) = tsc_capabilities();
    if selected_instant_provider() == PROVIDER_TSC {
      assert!(has_tsc && invariant_tsc);
      assert!(instant_uses_tsc());
    } else {
      assert!(!(has_tsc && invariant_tsc));
    }
  }

  #[test]
  fn recalibration_only_touches_a_selected_tsc_scale() {
    match recalibrate_instant_scale() {
      Some(scale) => {
        assert!(scale > 0);
        assert_eq!(selected_instant_provider(), PROVIDER_TSC);
      }
      None => assert_ne!(selected_instant_provider(), PROVIDER_TSC),
    }
  }
}
