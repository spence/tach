#![cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]

use std::hint::black_box;
use std::time::{Duration, Instant as WallInstant};

use tach::ThreadCpuInstant;

#[test]
fn thread_cpu_time_freezes_during_sleep() {
  let start = ThreadCpuInstant::now();
  std::thread::sleep(Duration::from_millis(50));
  let elapsed = start.elapsed();

  assert!(elapsed < Duration::from_millis(20), "sleep consumed {elapsed:?} of thread CPU");
}

#[test]
fn thread_cpu_time_advances_during_work() {
  let start = ThreadCpuInstant::now();
  let wall_start = WallInstant::now();
  let mut value = 1_u64;

  while start.elapsed() < Duration::from_millis(5) && wall_start.elapsed() < Duration::from_secs(2)
  {
    for _ in 0..10_000 {
      value = black_box(value.wrapping_mul(6364136223846793005).wrapping_add(1));
    }
  }

  assert!(start.elapsed() >= Duration::from_millis(5), "thread CPU clock did not advance");
  black_box(value);
}

#[test]
fn thread_cpu_timelines_are_independent() {
  let sleeping = std::thread::spawn(|| {
    let start = ThreadCpuInstant::now();
    std::thread::sleep(Duration::from_millis(100));
    start.elapsed()
  });
  let working = std::thread::spawn(|| {
    let start = ThreadCpuInstant::now();
    let wall_start = WallInstant::now();
    let mut value = 1_u64;
    while start.elapsed() < Duration::from_millis(10)
      && wall_start.elapsed() < Duration::from_secs(2)
    {
      for _ in 0..10_000 {
        value = black_box(value.rotate_left(7).wrapping_add(0x9e3779b97f4a7c15));
      }
    }
    black_box(value);
    start.elapsed()
  });

  let sleeping = sleeping.join().unwrap();
  let working = working.join().unwrap();
  assert!(sleeping < Duration::from_millis(20), "sleeping thread consumed {sleeping:?}");
  assert!(
    working > sleeping + Duration::from_millis(5),
    "working thread CPU {working:?} did not exceed sleeping thread CPU {sleeping:?}",
  );
}

#[cfg(target_os = "linux")]
#[test]
fn selected_provider_tracks_native_thread_cpu_time() {
  let provider = ThreadCpuInstant::provider();
  assert!(provider.measures_thread_cpu_time(), "unexpected provider: {provider:?}");

  let native_start = native_thread_cpu_nanos();
  let tach_start = ThreadCpuInstant::now();
  burn_cpu(Duration::from_millis(30));
  let tach_delta = ThreadCpuInstant::now().duration_since(tach_start);
  let native_delta = Duration::from_nanos(native_thread_cpu_nanos().saturating_sub(native_start));
  let tolerance = Duration::from_micros(250).max(native_delta / 20);
  assert!(
    tach_delta.abs_diff(native_delta) <= tolerance,
    "{provider:?} diverged from CLOCK_THREAD_CPUTIME_ID: tach={tach_delta:?}, \
     native={native_delta:?}, tolerance={tolerance:?}"
  );
}

#[cfg(target_os = "linux")]
fn native_thread_cpu_nanos() -> u64 {
  let mut value = core::mem::MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: value is writable timespec storage and the clock ID selects the
  // calling thread's CPU-time clock.
  let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, value.as_mut_ptr()) };
  assert_eq!(status, 0, "CLOCK_THREAD_CPUTIME_ID unavailable");
  // SAFETY: clock_gettime initialized the value on success.
  let value = unsafe { value.assume_init() };
  u64::try_from(value.tv_sec)
    .expect("non-negative thread CPU seconds")
    .saturating_mul(1_000_000_000)
    .saturating_add(u64::try_from(value.tv_nsec).expect("non-negative thread CPU nanoseconds"))
}

#[cfg(target_os = "linux")]
fn burn_cpu(duration: Duration) {
  let start = WallInstant::now();
  let mut value = 1_u64;
  while start.elapsed() < duration {
    for _ in 0..10_000 {
      value = black_box(value.rotate_left(11).wrapping_add(0xd6e8_feb8_6659_fd93));
    }
  }
  black_box(value);
}
