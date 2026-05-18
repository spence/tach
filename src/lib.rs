#![no_std]
#![warn(clippy::undocumented_unsafe_blocks)]
#![warn(rustdoc::broken_intra_doc_links)]

//! Ultra-fast drop-in replacement for [`std::time::Instant`].
//!
//! Each supported target compiles [`Instant::now()`] to a single architectural
//! counter read — RDTSC on x86 / x86_64, CNTVCT_EL0 on aarch64, rdtime on
//! riscv64 / loongarch64 — and falls back to the platform monotonic clock
//! everywhere else. No runtime dispatch on the hot path.
//!
//! # Quick start
//!
//! ```
//! use tach::Instant;
//!
//! let start = Instant::now();
//! // ... work ...
//! let elapsed = start.elapsed();
//! println!("{elapsed:?}");
//! ```
//!
//! # Timing contract
//!
//! `Instant` is wall-clock-rate: keeps ticking through park, suspension, and
//! descheduling. Same source across every thread in the process. **Not strictly
//! cross-thread monotonic** — raw hardware counters can disagree across CPUs by
//! sub-microsecond sync slop on most hosts. For strict cross-thread monotonicity,
//! use [`std::time::Instant`].
//!
//! # Ordering against atomics: [`OrderedInstant`]
//!
//! Plain [`Instant::now()`] is intentionally minimal — one counter instruction
//! with no synchronization barrier. That's a hazard if you correlate timestamps
//! with atomic loads:
//!
//! ```ignore
//! let deadline = scheduler.load(Ordering::Acquire);
//! let now = tach::Instant::now();    // ← may be sampled BEFORE `deadline` is observed
//! ```
//!
//! On aarch64 `mrs cntvct_el0` is a system-register read; on x86 `rdtsc` is not
//! serializing. Memory fences alone don't constrain when those execute, so the
//! timestamp can drift earlier than the synchronization point. Use
//! [`OrderedInstant`] when you need *"my timestamp is sampled after any prior
//! `Acquire`-or-stronger observation"*:
//!
//! ```ignore
//! let deadline = scheduler.load(Ordering::Acquire);
//! let now = tach::OrderedInstant::now();   // safe to correlate with `deadline`
//! ```
//!
//! [`OrderedInstant::now()`] emits the arch-appropriate barrier before the
//! counter read (`isb sy` on aarch64, `lfence` on x86; best-effort
//! `fence iorw, iorw` on riscv64 and `dbar 0` on loongarch64 — CSR-vs-memory
//! ordering is implementation-defined on those archs). Cost is ~5–20 ns more
//! than [`Instant::now()`] depending on architecture, still substantially
//! faster than [`std::time::Instant::now()`] on Linux and macOS (which use the
//! vDSO / libsystem path but do not themselves guarantee this ordering).

mod arch;
// Calibration is needed wherever the architectural counter doesn't self-report
// an NTP-corrected rate. That's: x86 / x86_64 on non-macOS (CPUID 15h is
// nominal, kernel doesn't continuously correct), aarch64 Linux (cntfrq_el0 is
// firmware-published nominal), and riscv64 / loongarch64. NOT needed on:
// macOS (mach_timebase_info is measured per-die), Windows aarch64
// (cntfrq_el0 is QPF-calibrated), wasm/WASI (host clock is the source).
#[cfg(any(
  all(any(target_arch = "x86_64", target_arch = "x86"), not(target_os = "macos")),
  all(target_arch = "aarch64", target_os = "linux"),
  target_arch = "riscv64",
  target_arch = "loongarch64",
))]
mod calibration;
mod instant;

pub use instant::{Instant, OrderedInstant};

// The crate is strictly `#![no_std]` by default. Two opt-in features bring std
// in: `recalibrate-background` (for the periodic-recalibration thread) and
// `bench-internal` (for the monotonicity + skew measurement primitives used by
// benches/ and the out-of-repo Lambda handler). The single `extern crate std`
// below covers both, plus `cfg(test)` for unit tests.
#[cfg(any(test, feature = "recalibrate-background", feature = "bench-internal"))]
extern crate std;

#[cfg(feature = "recalibrate-background")]
mod background;

#[cfg(feature = "recalibrate-background")]
pub use background::set_recalibration_interval;

#[cfg(feature = "bench-internal")]
#[doc(hidden)]
pub mod bench;

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::Duration;

  #[test]
  fn instant_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Instant>();
    assert_send_sync::<OrderedInstant>();
  }

  #[test]
  fn now_advances() {
    let mut previous = Instant::now();
    for _ in 0..10_000 {
      let current = Instant::now();
      assert!(current >= previous, "counter moved backward");
      previous = current;
    }
  }

  #[test]
  fn elapsed_after_sleep() {
    let start = Instant::now();
    std::thread::sleep(Duration::from_millis(10));
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() >= 9, "elapsed too short: {elapsed:?}");
    assert!(elapsed.as_millis() < 200, "elapsed too long: {elapsed:?}");
  }

  #[test]
  fn ordered_now_advances() {
    let mut previous = OrderedInstant::now();
    for _ in 0..10_000 {
      let current = OrderedInstant::now();
      assert!(current >= previous, "ordered counter moved backward");
      previous = current;
    }
  }

  #[test]
  fn ordered_elapsed_after_sleep() {
    let start = OrderedInstant::now();
    std::thread::sleep(Duration::from_millis(10));
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() >= 9, "ordered elapsed too short: {elapsed:?}");
    assert!(elapsed.as_millis() < 200, "ordered elapsed too long: {elapsed:?}");
  }

  // `as_unordered()` shares the same underlying tick value, so an elapsed
  // measurement from the converted unordered handle should match an elapsed
  // measurement from the original within bench-runtime noise.
  #[test]
  fn ordered_as_unordered_preserves_tick_value() {
    let ordered = OrderedInstant::now();
    let unordered = ordered.as_unordered();
    let elapsed_from_ordered = ordered.elapsed_unordered();
    let elapsed_from_unordered = unordered.elapsed();
    let diff = elapsed_from_ordered.abs_diff(elapsed_from_unordered);
    // The two .elapsed*() calls happen back-to-back; diff is whatever a
    // single counter read costs. 1ms is generous noise budget.
    assert!(diff.as_millis() < 1, "elapsed diverged after as_unordered: {diff:?}");
  }

  // Pairing OrderedInstant start with elapsed_unordered() end: end timestamp
  // is unordered but should still come after the ordered start (sleep is well
  // longer than any reordering window).
  #[test]
  fn ordered_elapsed_unordered_after_sleep() {
    let start = OrderedInstant::now();
    std::thread::sleep(Duration::from_millis(10));
    let elapsed = start.elapsed_unordered();
    assert!(elapsed.as_millis() >= 9, "elapsed_unordered too short: {elapsed:?}");
    assert!(elapsed.as_millis() < 200, "elapsed_unordered too long: {elapsed:?}");
  }

  #[test]
  fn instant_duration_since_saturates_when_earlier_is_later() {
    let early = Instant::now();
    std::thread::sleep(Duration::from_millis(5));
    let late = Instant::now();
    assert_eq!(early.duration_since(late), Duration::ZERO);
    assert!(late.duration_since(early) >= Duration::from_millis(4));
  }

  #[test]
  fn instant_checked_duration_since_returns_none_when_earlier_is_later() {
    let early = Instant::now();
    std::thread::sleep(Duration::from_millis(5));
    let late = Instant::now();
    assert!(early.checked_duration_since(late).is_none());
    assert!(late.checked_duration_since(early).is_some());
  }

  #[test]
  fn instant_sub_instant_returns_elapsed() {
    let a = Instant::now();
    std::thread::sleep(Duration::from_millis(5));
    let b = Instant::now();
    let diff: Duration = b - a;
    assert!(diff.as_millis() >= 4 && diff.as_millis() < 200, "unexpected diff: {diff:?}");
  }

  #[test]
  fn instant_add_duration_advances_time() {
    let now = Instant::now();
    let later = now + Duration::from_secs(1);
    let diff = later.duration_since(now);
    let drift = diff.abs_diff(Duration::from_secs(1));
    // Q32 reciprocal round-trip; sub-microsecond drift is the tolerance.
    assert!(drift < Duration::from_micros(1), "round-trip drift: {drift:?}");
  }

  #[test]
  fn instant_sub_duration_and_add_assign() {
    let now = Instant::now();
    let earlier = now - Duration::from_millis(100);
    let diff = now.duration_since(earlier);
    assert!(diff.as_millis() >= 99 && diff.as_millis() <= 101, "expected ~100ms, got {diff:?}",);

    let mut t = now;
    t += Duration::from_secs(1);
    t -= Duration::from_millis(500);
    let delta = t.duration_since(now);
    let drift = delta.abs_diff(Duration::from_millis(500));
    assert!(drift < Duration::from_micros(2), "round-trip drift: {drift:?}");
  }

  #[test]
  fn ordered_instant_arithmetic_mirrors_instant() {
    let a = OrderedInstant::now();
    std::thread::sleep(Duration::from_millis(5));
    let b = OrderedInstant::now();
    let diff: Duration = b - a;
    assert!(diff.as_millis() >= 4 && diff.as_millis() < 200, "diff: {diff:?}");
    assert_eq!(a.duration_since(b), Duration::ZERO);
    assert!(b.checked_duration_since(a).is_some());

    let later = a + Duration::from_secs(1);
    let drift = later.duration_since(a).abs_diff(Duration::from_secs(1));
    assert!(drift < Duration::from_micros(1), "drift: {drift:?}");
  }

  #[test]
  fn recalibrate_does_not_perturb_elapsed() {
    let start = Instant::now();
    std::thread::sleep(Duration::from_millis(20));
    Instant::recalibrate();
    let elapsed = start.elapsed();
    // Recalibration itself spins for up to ~700 ms on platforms where it
    // actually measures (Linux/Windows x86, aarch64 Linux); no-op on macOS
    // and Windows aarch64 where cntfrq_el0 / mach_timebase_info are
    // authoritative. The upper bound here is a sanity check that a buggy
    // recalibration didn't jump the scaling so far that elapsed jumps to
    // multi-second values, not an assertion about the recalibrate cost
    // itself.
    assert!(
      elapsed.as_millis() >= 19 && elapsed.as_millis() < 2_000,
      "unexpected elapsed across recalibration: {elapsed:?}",
    );
  }

  #[test]
  fn recalibrate_is_safe_to_call_repeatedly() {
    for _ in 0..3 {
      Instant::recalibrate();
    }
    let _ = Instant::now().elapsed();
  }

  #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
  #[test]
  fn cpuid_15h_returns_something_or_none() {
    #[cfg(target_arch = "x86_64")]
    let _ = crate::arch::x86_64::cpuid_tsc_hz();
    #[cfg(target_arch = "x86")]
    let _ = crate::arch::x86::cpuid_tsc_hz();
  }

  // Proving test: tach's elapsed() must track std's elapsed() within ±5%
  // across a 100ms sleep. Catches:
  //  - Windows freq-vs-counter mismatch (was ~300× off; QPF Hz on RDTSC ticks)
  //  - macOS mach_timebase_info numerator/denominator transposition
  //  - CPUID 15h numerator/denominator transposition
  //  - aarch64 cntfrq misreporting (e.g. exposing crystal Hz instead of Hz)
  // ±5% is a generous noise budget over a 100 ms window — schedule jitter
  // and elapsed-call overhead are << 1%; any real scaling bug blows past it.
  #[test]
  fn elapsed_tracks_std_within_5_percent() {
    let ts = std::time::Instant::now();
    let tt = Instant::now();
    std::thread::sleep(Duration::from_millis(100));
    let s_ns = ts.elapsed().as_nanos() as f64;
    let t_ns = tt.elapsed().as_nanos() as f64;
    let ratio = t_ns / s_ns;
    assert!(
      ratio > 0.95 && ratio < 1.05,
      "tach/std ratio = {ratio} (std={s_ns} ns, tach={t_ns} ns)",
    );
  }
}
