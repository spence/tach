//! Background recalibration thread. Compiled only with the
//! `recalibrate-background` Cargo feature, which **requires `std`** —
//! everything in this file uses `std::thread`, `std::sync::OnceLock`, and
//! `std::time::Duration`. The rest of the crate stays `#![no_std]`.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

const DEFAULT_INTERVAL_SECS: u64 = 60;

// EMA coefficient: blended = (51 * new + 205 * prev) >> 8, i.e. alpha = 51/256
// ≈ 0.199. Effective averaging window ≈ 1/alpha ≈ 5 samples → 5 min at the
// 60s default interval. Short enough to track real thermal-drift trends in
// the underlying crystal, long enough that a single preempted calibration
// window can't jolt either scale by more than ~20% of its noise.
const ALPHA_NUM: u128 = 51;
const ONE_MINUS_ALPHA_NUM: u128 = 205;
const ALPHA_DEN_SHIFT: u32 = 8;

static INTERVAL_SECS: AtomicU64 = AtomicU64::new(DEFAULT_INTERVAL_SECS);
static THREAD: OnceLock<()> = OnceLock::new();

/// Configure the interval at which the background recalibration thread
/// runs. Takes effect on the next sleep cycle (so up to one current-interval
/// worth of delay before the change is observed).
///
/// Minimum is 1 second; smaller values are clamped up. Default is 60 seconds.
///
/// Available only with the `recalibrate-background` Cargo feature, which
/// **requires `std`**. The default tach build is `#![no_std]`; enabling this
/// feature is the only thing that promotes the crate to `std`.
pub fn set_recalibration_interval(interval: Duration) {
  let secs = interval.as_secs().max(1);
  INTERVAL_SECS.store(secs, Ordering::Relaxed);
}

pub(crate) fn ensure_thread() {
  THREAD.get_or_init(|| {
    let _ = thread::Builder::new().name("tach-recalibrate".into()).spawn(|| {
      loop {
        let secs = INTERVAL_SECS.load(Ordering::Relaxed);
        thread::sleep(Duration::from_secs(secs));
        let update = crate::arch::recalibrate_measure();
        blend_scale(&crate::arch::NANOS_PER_TICK_Q32, update.local);
        blend_scale(&crate::arch::ORDERED_NANOS_PER_TICK_Q32, update.ordered);
      }
    });
  });
}

#[inline]
fn blend_scale(cache: &AtomicU64, update: Option<u64>) {
  let Some(new_q32) = update else {
    return;
  };
  let previous = cache.load(Ordering::Acquire);
  let blended = if previous == 0 { new_q32 } else { ema_blend_q32(previous, new_q32) };
  cache.store(blended, Ordering::Release);
}

// Integer-only EMA: blended = (51 * new + 205 * prev) >> 8. With
// prev, new ≤ u64::MAX the numerator stays under 256 * u64::MAX < 2^72,
// which fits in u128; the shift-right brings the result back inside u64.
#[inline]
fn ema_blend_q32(prev: u64, new: u64) -> u64 {
  ((ALPHA_NUM * u128::from(new) + ONE_MINUS_ALPHA_NUM * u128::from(prev)) >> ALPHA_DEN_SHIFT) as u64
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn late_initialized_scale_starts_from_its_first_measurement() {
    let cache = AtomicU64::new(0);
    blend_scale(&cache, None);
    assert_eq!(cache.load(Ordering::Relaxed), 0);

    blend_scale(&cache, Some(5_000_000_000));
    assert_eq!(cache.load(Ordering::Relaxed), 5_000_000_000);

    blend_scale(&cache, Some(6_000_000_000));
    assert_eq!(cache.load(Ordering::Relaxed), ema_blend_q32(5_000_000_000, 6_000_000_000),);
  }

  // Step-response convergence: starting from 0, feeding a constant target
  // value, the EMA should reach the target within 1 ppm in well under
  // 200 iterations. (1 - 51/256)^n < 1e-6 at n ≈ 63 in exact arithmetic;
  // we leave headroom for rounding.
  #[test]
  fn ema_converges_to_constant_input() {
    let target: u64 = 5_000_000_000;
    let mut blended: u64 = 0;
    for _ in 0..200 {
      blended = ema_blend_q32(blended, target);
    }
    let diff = blended.abs_diff(target);
    let ppm = diff as u128 * 1_000_000 / target as u128;
    assert!(ppm <= 1, "after 200 iters, blended={blended} target={target} drift={ppm} ppm");
  }

  // Stability: starting at the target, feeding the target, the value must
  // remain at the target. Catches rounding bias that would slowly drift
  // the EMA away from its input even in steady state.
  #[test]
  fn ema_stable_at_target() {
    let target: u64 = 5_000_000_000;
    let mut blended = target;
    for _ in 0..10_000 {
      blended = ema_blend_q32(blended, target);
    }
    assert_eq!(blended, target, "EMA drifted from steady-state input over 10k iters");
  }

  // Smoothing factor: a single noisy sample shouldn't move the blended
  // value by more than the EMA weight (~20%) of the step size, regardless
  // of how large the noise is.
  #[test]
  fn ema_single_outlier_bounded_by_alpha() {
    let baseline: u64 = 5_000_000_000;
    let outlier: u64 = baseline * 2;
    let blended = ema_blend_q32(baseline, outlier);
    let step = blended - baseline;
    let expected_max_step = (outlier - baseline) / 4; // < 25 % of step
    assert!(
      step < expected_max_step,
      "single outlier moved EMA by {step}, expected < {expected_max_step}",
    );
  }
}
