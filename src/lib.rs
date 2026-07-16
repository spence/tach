#![no_std]
#![warn(clippy::undocumented_unsafe_blocks)]
#![warn(rustdoc::broken_intra_doc_links)]

//! Three Instant-shaped timers for three timing contracts.
//!
//! All three APIs compute elapsed time as [`core::time::Duration`]. They differ
//! in the quantity that advances and where samples may be compared safely:
//!
//! | Job | Type | Contract |
//! |---|---|---|
//! | Same-thread elapsed | [`Instant`] | Wall-rate time; endpoints stay local |
//! | Ordered elapsed | [`OrderedInstant`] | Wall time ordered after memory observations |
//! | Thread CPU, where native | [`ThreadCpuInstant`] | CPU time or explicit wall fallback |
//!
//! With default features, each type selects the fastest eligible provider for
//! its contract. Eligible wall providers must be monotonic, wall-rate, and
//! high-resolution; deliberately coarsened clocks are a different contract.
//! Disabling default features requests the syscall-only `no_std` thread-CPU
//! implementation and can therefore trade speed for dependency surface.
//!
//! On six benchmark environments, each type has the fastest tested
//! steady-state read and elapsed bracket for its contract, with
//! `max(1 ns, 5%)` treated as a practical material tie. A separate 24-target
//! compile and codegen matrix proves API availability and provider routing; it
//! does not claim measured speed on unbenchmarked hardware.
//!
//! # Quick start
//!
//! ```
//! use tach::{Instant, ThreadCpuInstant};
//!
//! let wall_start = Instant::now();
//! let wall_elapsed = wall_start.elapsed();
//!
//! let cpu_start = ThreadCpuInstant::now();
//! let cpu_elapsed = cpu_start.elapsed();
//!
//! assert!(wall_elapsed <= wall_start.elapsed());
//! assert!(cpu_elapsed <= cpu_start.elapsed());
//! ```
//!
//! # Choosing a type
//!
//! Use [`Instant`] when both endpoints of an elapsed-time bracket stay local to
//! one thread. Direct-counter providers do not order the sample after prior
//! memory operations.
//!
//! Use [`OrderedInstant`] when a timestamp participates in a cross-thread
//! happens-before relationship. Direct-counter targets select an architecture
//! barrier; Windows and Intel macOS fence before their reliable platform
//! clock. The load-then-now-then-check contract produced zero inversions in
//! about 10.9 billion tested x86 and aarch64 reads. RISC-V's ratified Zicsr
//! ordering rules cover `fence r, i; rdtime`. LoongArch Linux uses a raw
//! system-call exception boundary before `rdtime.d`; Linux armv7 and s390x
//! fence before `CLOCK_MONOTONIC`; Linux powerpc64 GNU uses `sync; mftb`.
//!
//! Use [`ThreadCpuInstant`] for CPU delivered to the calling OS thread. Native
//! providers freeze while the thread is sleeping or descheduled. Targets with
//! no portable thread clock use an explicitly reported monotonic-wall fallback;
//! check [`ThreadCpuInstant::measures_thread_cpu_time`] when that distinction is
//! correctness-sensitive. Native candidates must retain the platform clock's
//! full scheduled-runtime precision; a cheaper coarsened accounting API is not
//! eligible for selection.
//!
//! Linux providers use the native `CLOCK_THREAD_CPUTIME_ID` timeline. Targets
//! with multiple equivalent syscall entry paths measure those paths once per
//! process and retain the fastest reliable route on the hot path.
//!
//! # Hardware assumption
//!
//! Direct-counter providers assume a coherent, monotonic architectural
//! counter. Windows uses QPC because raw TSC/CNTVCT cost and frequency probes
//! cannot establish Windows' cross-core, sleep, and VM-migration guarantees.
//! Intel macOS admits a bare invariant TSC only for same-thread `Instant`
//! after runtime eligibility and complete-path cost checks; its ordered timer
//! stays on the platform-owned reliable timeline.
//! on other hosts whose OS marks a TSC clocksource unstable because cores are
//! genuinely desynchronized, use the platform clock instead.
//!
//! Linux can explicitly configure architectural counter reads to fault on a
//! per-thread basis. Initial selection fails closed when its calling thread is
//! denied, but a process-wide direct or vDSO winner requires every reading
//! thread to retain counter permission. Calling `PR_SET_TSC` to request a fault
//! is an external fault boundary for tach, libc's vDSO clocks, and other direct
//! counter readers.

mod arch;
// Calibration is needed wherever the selected architectural counter doesn't
// self-report an NTP-corrected rate: x86 outside Windows/macOS/FreeBSD,
// aarch64 Linux, and riscv64 / loongarch64. FreeBSD uses the authoritative
// kernel TSC rate or nanosecond CLOCK_MONOTONIC; Windows uses QPC/QPF, Intel
// macOS uses the Mach timebase, and Apple Silicon uses cntfrq_el0.
#[cfg(any(
  all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(any(target_os = "windows", target_os = "macos", target_os = "freebsd")),
  ),
  all(target_arch = "aarch64", target_os = "linux"),
  target_arch = "riscv64",
  target_arch = "loongarch64",
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
))]
mod calibration;
mod instant;
mod thread_cpu;

pub use instant::{Instant, OrderedInstant};
pub use thread_cpu::{ThreadCpuInstant, ThreadCpuProvider, ThreadCpuReadCost};

// `#![no_std]` remains the crate root. The default thread-cpu-inline feature
// links std only on the listed Linux-kernel perf targets for native TLS;
// --no-default-features preserves the strict no_std dependency surface.
// Recalibration, benchmark support, and tests also link std.
#[cfg(any(
  test,
  feature = "recalibrate-background",
  feature = "bench-internal",
  all(
    feature = "thread-cpu-inline",
    any(
      all(
        target_os = "linux",
        any(
          target_arch = "x86",
          target_arch = "x86_64",
          target_arch = "aarch64",
          target_arch = "arm",
          target_arch = "riscv64",
          target_arch = "s390x",
          target_arch = "loongarch64",
          target_arch = "powerpc64",
        ),
      ),
      all(target_os = "android", any(target_arch = "x86_64", target_arch = "aarch64"),),
    ),
  ),
))]
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

  // Upper bound for sleep-based elapsed checks: a garbage guard, not a precision
  // bound. Hosted CI oversleeps short sleeps heavily (a 10 ms sleep measured
  // ~400 ms on a loaded windows-2022 runner), so a tight bound flakes without any
  // clock defect. Precision is covered by `elapsed_tracks_std_within_5_percent`
  // and overflow-to-garbage by `elapsed_saturates_when_self_is_in_the_future`.
  const SLEEP_ELAPSED_MAX_MS: u128 = 60_000;

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
    assert!(elapsed.as_millis() < SLEEP_ELAPSED_MAX_MS, "elapsed too long: {elapsed:?}");
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
    assert!(elapsed.as_millis() < SLEEP_ELAPSED_MAX_MS, "ordered elapsed too long: {elapsed:?}");
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
    assert!(elapsed.as_millis() < SLEEP_ELAPSED_MAX_MS, "elapsed_unordered too long: {elapsed:?}");
  }

  #[test]
  fn instant_duration_since_saturates_when_earlier_is_later() {
    let early = Instant::now();
    std::thread::sleep(Duration::from_millis(5));
    let late = Instant::now();
    assert_eq!(early.duration_since(late), Duration::ZERO);
    assert!(late.duration_since(early) >= Duration::from_millis(4));
  }

  // elapsed() must saturate to zero when `self` is in the future rather than
  // wrapping to a ~580-year garbage Duration. The future instant is an hour
  // ahead, so the current read always lands before it. Covers both types
  // plus OrderedInstant's unordered end read.
  #[test]
  fn elapsed_saturates_when_self_is_in_the_future() {
    let one_hour = Duration::from_secs(3600);
    assert_eq!((Instant::now() + one_hour).elapsed(), Duration::ZERO);
    assert_eq!((OrderedInstant::now() + one_hour).elapsed(), Duration::ZERO);
    assert_eq!((OrderedInstant::now() + one_hour).elapsed_unordered(), Duration::ZERO);
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
    assert!(
      diff.as_millis() >= 4 && diff.as_millis() < SLEEP_ELAPSED_MAX_MS,
      "unexpected diff: {diff:?}"
    );
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
    assert!(diff.as_millis() >= 4 && diff.as_millis() < SLEEP_ELAPSED_MAX_MS, "diff: {diff:?}");
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
    // actually measures (direct-TSC x86 and aarch64 Linux); no-op on macOS
    // and Windows where the selected platform clock has an authoritative
    // scale. The upper bound here is a sanity check that a buggy
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

  #[cfg(all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(any(target_os = "windows", target_os = "macos")),
  ))]
  #[test]
  fn cpuid_15h_returns_something_or_none() {
    #[cfg(target_arch = "x86_64")]
    let _ = crate::arch::x86_64::cpuid_tsc_hz();
    #[cfg(target_arch = "x86")]
    let _ = crate::arch::x86::cpuid_tsc_hz();
  }

  // Proving test: tach's elapsed() must track std's elapsed() within ±5%
  // across a 100ms sleep. Catches:
  //  - Windows QPC/QPF counter-frequency mismatch
  //  - Darwin timebase numerator/denominator transposition
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

  // Synchronization-order monotonicity, directly validating the happens-before
  // contract: "after observing a value via Acquire-load, a subsequent
  // OrderedInstant::now() must return a value >= what was observed." This is the
  // in-crate version of the `measure_synchronization_order` bench; it must hold
  // with 0 violations across N racing threads.
  //
  // The contract:
  //   - Thread P does now(); publishes its value via Release-store.
  //   - Thread O does Acquire-load to observe P's value; then now().
  //   - O's now() must return >= what O observed.
  //
  // Plain Instant FAILS this — on x86 (~10 µs hardware sync slop) and on Apple
  // Silicon (~12% of reads on M1) the bare counter read can be sampled before
  // the Acquire-load retires. OrderedInstant passes because its barrier
  // (`rdtscp` / `isb sy`) pins the read after prior loads are globally visible,
  // verified at 0 violations across ~10.9B reads incl. 2-socket NUMA.
  #[test]
  fn ordered_honors_happens_before() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::vec::Vec;

    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
    let anchor = OrderedInstant::now();
    let published_ns = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let barrier = Arc::new(std::sync::Barrier::new(threads + 1));

    let handles: Vec<_> = (0..threads)
      .map(|_| {
        let published_ns = Arc::clone(&published_ns);
        let stop = Arc::clone(&stop);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
          let mut violations: u64 = 0;
          barrier.wait();
          while !stop.load(Ordering::Relaxed) {
            // Step 1: Acquire-load the latest published nanosecond value,
            //         synchronizing-with the prior thread's Release on this
            //         atomic.
            let observed = published_ns.load(Ordering::Acquire);
            // Step 2: Call now(). Its barrier pins the counter read after the
            //         Acquire-load above, so the return must be >= the tick
            //         for the observed ns.
            let t = OrderedInstant::now();
            let ns = t.duration_since(anchor).as_nanos() as u64;
            // Step 3: Check the contract: ns >= observed.
            if ns < observed {
              violations += 1;
            }
            // Step 4: Publish our value via Release fetch_max so future
            //         readers can observe us.
            published_ns.fetch_max(ns, Ordering::Release);
          }
          violations
        })
      })
      .collect();

    barrier.wait();
    std::thread::sleep(Duration::from_millis(500));
    stop.store(true, Ordering::Relaxed);

    let total_violations: u64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
    assert_eq!(
      total_violations, 0,
      "OrderedInstant showed {total_violations} happens-before cross-thread monotonicity \
       violations (expected 0); the ordering barrier appears to be broken",
    );
  }
}
