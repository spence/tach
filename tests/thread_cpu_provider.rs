//! Integration tests for the public `ThreadCpuInstant` provider reporting and
//! arithmetic contracts. Relocated from the `src/thread_cpu.rs` unit module
//! (OBJ-SIMPLIFY-TIMERS M2). Unlike tests/thread_cpu.rs (linux/macos/windows
//! only), these run on every target — the platform-specific assertions are
//! cfg-gated inline — so this file is intentionally not `#![cfg]`-gated.

use std::time::Duration;

use tach::{ThreadCpuInstant, ThreadCpuProvider, ThreadCpuReadCost};

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
    ThreadCpuProvider::LinuxPerfRead => {
      assert_eq!(cost, ThreadCpuReadCost::SystemCall);
    }
    ThreadCpuProvider::PosixThreadCpuClock | ThreadCpuProvider::WindowsThreadTimes => {
      assert_eq!(cost, ThreadCpuReadCost::SystemCall);
    }
    // `#[non_exhaustive]` requires a wildcard from an external crate; the
    // remaining providers (wasi/node/perf-now/hrtime/wall/unavailable) assert
    // no cost here.
    _ => {}
  }

  #[cfg(all(
    feature = "thread-cpu-inline",
    any(
      target_os = "linux",
      all(target_os = "android", any(target_arch = "x86_64", target_arch = "aarch64"),),
    ),
  ))]
  assert!(matches!(
    provider,
    ThreadCpuProvider::LinuxPerfMmap
      | ThreadCpuProvider::LinuxPerfRead
      | ThreadCpuProvider::PosixThreadCpuClock
  ));
  #[cfg(any(
    target_os = "macos",
    all(target_os = "linux", not(feature = "thread-cpu-inline")),
    all(
      target_os = "android",
      not(all(
        feature = "thread-cpu-inline",
        any(target_arch = "x86_64", target_arch = "aarch64"),
      )),
    ),
  ))]
  assert_eq!(provider, ThreadCpuProvider::PosixThreadCpuClock);
  #[cfg(target_os = "windows")]
  assert_eq!(provider, ThreadCpuProvider::WindowsThreadTimes);
}

#[test]
fn javascript_provider_domains_are_explicit() {
  assert!(ThreadCpuProvider::NodeThreadCpuUsage.measures_thread_cpu_time());
  assert!(!ThreadCpuProvider::PerformanceNow.measures_thread_cpu_time());
  assert!(!ThreadCpuProvider::NodeHrtime.measures_thread_cpu_time());
  assert!(!ThreadCpuProvider::Unavailable.measures_thread_cpu_time());
}

#[test]
fn selected_read_gap_is_positive_when_present() {
  if let Some(gap) = ThreadCpuInstant::max_read_gap() {
    assert!(!gap.is_zero());
    assert_eq!(ThreadCpuInstant::provider(), ThreadCpuProvider::LinuxPerfMmap);
  }
}
