//! Calibration runs on the direct-counter targets whose selected wall clock
//! does not self-report an NTP-corrected frequency: non-Windows/non-macOS x86,
//! aarch64 Linux, riscv64, loongarch64, and the pathological PowerPC64 host
//! whose glibc Time Base frequency query returned zero. It measures the
//! counter's tick rate against `clock_gettime(CLOCK_MONOTONIC)`.

use core::hint::spin_loop;

#[inline]
fn ref_ns() -> u64 {
  crate::arch::fallback::clock_monotonic()
}

// Calibrate by spinning for `CALIBRATION_TIME_NS` and observing how many
// architectural ticks elapse against the platform monotonic clock. A 100 ms
// window is long enough that per-tick wall-clock noise (~50 ns on bare metal,
// ~500 ns on Nitro VMs) doesn't dominate the rate estimate but short enough
// that the worst-case startup cost (7 × 100 ms = 700 ms when no preemption
// occurs) is acceptable for a one-shot lazy init.
//
// On virtualized hosts the spin loop can be preempted by the hypervisor. When
// that happens, `wall_elapsed` overshoots `CALIBRATION_TIME_NS` by orders of
// magnitude and the per-sample rate estimate is contaminated by the wall
// clock advancing while the vCPU was descheduled (counters keep ticking, but
// at the wrong relative rate to wall time over that interval). Any sample
// that overran by more than 50 % is dropped as preempted; the median of the
// survivors is returned. The 1.5× threshold catches preemption that's
// distinguishable from ordinary noise without flagging samples that merely
// ran for the full window plus normal scheduling jitter.
//
// If every sample gets discarded — e.g. on a host that consistently preempts
// every 100 ms window — the function falls back to one un-filtered sample
// rather than returning 0. Something within a few percent of the right
// answer is preferable to zero; the background-recal thread (when enabled)
// gets another chance every 60 s.
pub fn calibrate_frequency() -> u64 {
  calibrate_frequency_with(crate::arch::ticks)
}

/// Calibrate an explicit counter reader against the platform wall clock.
///
/// Runtime-selected platforms can expose different raw domains for local and
/// ordered wall timers. Passing the reader explicitly keeps calibration tied
/// to the domain whose fixed-point scale will consume the result.
pub(crate) fn calibrate_frequency_with(read_ticks: fn() -> u64) -> u64 {
  const CALIBRATION_TIME_NS: u64 = 100_000_000;
  const MAX_OVERRUN_NS: u64 = CALIBRATION_TIME_NS * 3 / 2;
  const NUM_SAMPLES: usize = 7;

  let mut survivors = [0u64; NUM_SAMPLES];
  let mut n = 0usize;

  for _ in 0..NUM_SAMPLES {
    let wall_start = ref_ns();
    let t0 = read_ticks();

    while ref_ns().wrapping_sub(wall_start) < CALIBRATION_TIME_NS {
      spin_loop();
    }

    let t1 = read_ticks();
    let wall_elapsed = ref_ns().wrapping_sub(wall_start);

    if wall_elapsed > MAX_OVERRUN_NS {
      continue;
    }

    let ticks = t1.wrapping_sub(t0);
    if let Some(hz) = (u128::from(ticks) * 1_000_000_000).checked_div(u128::from(wall_elapsed)) {
      survivors[n] = u64::try_from(hz).unwrap_or(u64::MAX);
      n += 1;
    }
  }

  if n == 0 {
    return single_unfiltered_sample(CALIBRATION_TIME_NS, read_ticks);
  }

  survivors[..n].sort_unstable();
  survivors[n / 2]
}

// Fallback for the pathological case where every bracketed sample overran.
// Better to return something within a few percent than zero — recal-bg gets
// another shot every 60 s if it's enabled.
#[inline]
fn single_unfiltered_sample(window_ns: u64, read_ticks: fn() -> u64) -> u64 {
  let wall_start = ref_ns();
  let t0 = read_ticks();
  while ref_ns().wrapping_sub(wall_start) < window_ns {
    spin_loop();
  }
  let t1 = read_ticks();
  let wall_elapsed = ref_ns().wrapping_sub(wall_start);
  let ticks = t1.wrapping_sub(t0);
  match (u128::from(ticks) * 1_000_000_000).checked_div(u128::from(wall_elapsed)) {
    Some(hz) => u64::try_from(hz).unwrap_or(u64::MAX),
    None => 0,
  }
}
