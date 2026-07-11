#![allow(clippy::cast_precision_loss)]

use std::hint::black_box;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::mem::MaybeUninit;
use std::time::{Duration, Instant as StdInstant};

use criterion::{Criterion, criterion_group, criterion_main};
#[cfg(all(
  feature = "bench-internal",
  feature = "thread-cpu-inline",
  target_os = "linux",
  any(target_arch = "x86_64", target_arch = "aarch64"),
))]
use tach::bench::ThreadCpuPerfHandle;
use tach::{Instant, OrderedInstant, ThreadCpuInstant, ThreadCpuProvider, ThreadCpuReadCost};

fn bench_now(c: &mut Criterion) {
  // Prime the lazy frequency calibration so it doesn't land in the first
  // measured sample.
  quanta::Instant::now();
  fastant::Instant::now();
  minstant::Instant::now();

  let mut g = c.benchmark_group("Instant::now()");
  g.bench_function("tach", |b| b.iter(|| black_box(Instant::now())));
  g.bench_function("tach_ordered", |b| b.iter(|| black_box(OrderedInstant::now())));
  g.bench_function("quanta", |b| b.iter(|| black_box(quanta::Instant::now())));
  g.bench_function("fastant", |b| b.iter(|| black_box(fastant::Instant::now())));
  g.bench_function("minstant", |b| b.iter(|| black_box(minstant::Instant::now())));
  g.bench_function("std", |b| b.iter(|| black_box(StdInstant::now())));
  g.finish();
}

fn bench_elapsed(c: &mut Criterion) {
  quanta::Instant::now();
  fastant::Instant::now();
  minstant::Instant::now();

  let mut g = c.benchmark_group("Instant::now() + elapsed()");
  g.bench_function("tach", |b| {
    b.iter(|| {
      let start = Instant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("tach_ordered", |b| {
    b.iter(|| {
      let start = OrderedInstant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("quanta", |b| {
    b.iter(|| {
      let start = quanta::Instant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("fastant", |b| {
    b.iter(|| {
      let start = fastant::Instant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("minstant", |b| {
    b.iter(|| {
      let start = minstant::Instant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("std", |b| {
    b.iter(|| {
      let start = StdInstant::now();
      black_box(start.elapsed())
    });
  });
  g.finish();
}

fn thread_cpu_bench_id() -> &'static str {
  match (ThreadCpuInstant::provider(), ThreadCpuInstant::read_cost_hint()) {
    (ThreadCpuProvider::LinuxPerfMmap, ThreadCpuReadCost::Inline) => {
      "tach_thread_cpu__linux_perf_mmap__inline"
    }
    (ThreadCpuProvider::PosixThreadCpuClock, ThreadCpuReadCost::SystemCall) => {
      "tach_thread_cpu__posix_thread_cpu_clock__system_call"
    }
    (ThreadCpuProvider::WindowsThreadTimes, ThreadCpuReadCost::SystemCall) => {
      "tach_thread_cpu__windows_thread_times__system_call"
    }
    (ThreadCpuProvider::WasiThreadCpuClock, ThreadCpuReadCost::HostCall) => {
      "tach_thread_cpu__wasi_thread_cpu_clock__host_call"
    }
    (ThreadCpuProvider::PerformanceNow, ThreadCpuReadCost::HostCall) => {
      "tach_thread_cpu__performance_now__host_call"
    }
    (ThreadCpuProvider::MonotonicWallClock, ThreadCpuReadCost::Inline) => {
      "tach_thread_cpu__monotonic_wall_clock__inline"
    }
    (ThreadCpuProvider::MonotonicWallClock, ThreadCpuReadCost::SystemCall) => {
      "tach_thread_cpu__monotonic_wall_clock__system_call"
    }
    (ThreadCpuProvider::MonotonicWallClock, ThreadCpuReadCost::HostCall) => {
      "tach_thread_cpu__monotonic_wall_clock__host_call"
    }
    (_, ThreadCpuReadCost::Inline) => "tach_thread_cpu__other__inline",
    (_, ThreadCpuReadCost::SystemCall) => "tach_thread_cpu__other__system_call",
    (_, ThreadCpuReadCost::HostCall) => "tach_thread_cpu__other__host_call",
    (_, _) => "tach_thread_cpu__other__unknown_cost",
  }
}

fn bench_thread_cpu_now(c: &mut Criterion) {
  // Provider selection is intentionally outside the measured loop: the API's
  // contract is steady-state `now()`, after its once-per-process assessment.
  let tach_id = thread_cpu_bench_id();
  let mut g = c.benchmark_group("ThreadCpuInstant::now()");
  g.bench_function(tach_id, |b| b.iter(|| black_box(ThreadCpuInstant::now())));
  g.bench_function(NATIVE_THREAD_CPU_BENCH_ID, |b| {
    b.iter(|| black_box(native_thread_cpu_now()));
  });
  #[cfg(all(
    feature = "bench-internal",
    feature = "thread-cpu-inline",
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64"),
  ))]
  if let Some(direct) = ThreadCpuPerfHandle::try_for_current_thread() {
    g.bench_function("direct_thread_cpu__linux_perf_mmap", |b| {
      b.iter(|| black_box(direct.now_nanos()));
    });
  }
  g.finish();
  write_thread_cpu_selection();
  assert_eq!(tach_id, thread_cpu_bench_id(), "thread-CPU provider changed during now() bench");
}

fn bench_thread_cpu_elapsed(c: &mut Criterion) {
  let tach_id = thread_cpu_bench_id();
  let mut g = c.benchmark_group("ThreadCpuInstant::now() + elapsed()");
  g.bench_function(tach_id, |b| {
    b.iter(|| {
      let start = ThreadCpuInstant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function(NATIVE_THREAD_CPU_BENCH_ID, |b| {
    b.iter(|| {
      let start = native_thread_cpu_now();
      black_box(Duration::from_nanos(native_thread_cpu_now().saturating_sub(start)))
    });
  });
  #[cfg(all(
    feature = "bench-internal",
    feature = "thread-cpu-inline",
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64"),
  ))]
  if let Some(direct) = ThreadCpuPerfHandle::try_for_current_thread() {
    g.bench_function("direct_thread_cpu__linux_perf_mmap", |b| {
      b.iter(|| {
        let start = direct.now_nanos();
        black_box(Duration::from_nanos(direct.now_nanos().saturating_sub(start)))
      });
    });
  }
  g.finish();
  assert_eq!(tach_id, thread_cpu_bench_id(), "thread-CPU provider changed during elapsed() bench");
}

#[cfg(all(
  feature = "bench-internal",
  feature = "thread-cpu-inline",
  target_os = "linux",
  any(target_arch = "x86_64", target_arch = "aarch64"),
))]
fn write_thread_cpu_selection() {
  use std::fs;
  use std::path::PathBuf;

  let measurement = tach::bench::thread_cpu_selection_measurements();
  let provider = format!("{:?}", ThreadCpuInstant::provider());
  let payload = match measurement {
    Some((mut perf_samples, mut syscall_samples, iterations)) => {
      let perf_raw_samples = perf_samples;
      let syscall_raw_samples = syscall_samples;
      perf_samples.sort_unstable();
      syscall_samples.sort_unstable();
      let perf_total_ns = perf_samples[perf_samples.len() / 2];
      let syscall_total_ns = syscall_samples[syscall_samples.len() / 2];
      let allowance_total_ns = (iterations as u64).max(syscall_total_ns / 20);
      let decisive_wins = perf_raw_samples
        .iter()
        .zip(syscall_raw_samples)
        .filter(|(perf, syscall)| (**perf).saturating_add(allowance_total_ns) < *syscall)
        .count();
      let decision = if perf_total_ns.saturating_add(allowance_total_ns) < syscall_total_ns
        && decisive_wins >= 8
      {
        "perf"
      } else {
        "syscall"
      };
      serde_json::json!({
        "available": true,
        "iterations": iterations,
        "perf_total_ns_samples": perf_raw_samples,
        "syscall_total_ns_samples": syscall_raw_samples,
        "perf_median_total_ns": perf_total_ns,
        "syscall_median_total_ns": syscall_total_ns,
        "perf_estimated_ns": perf_total_ns as f64 / iterations as f64,
        "syscall_estimated_ns": syscall_total_ns as f64 / iterations as f64,
        "equivalence_allowance_total_ns": allowance_total_ns,
        "equivalence_allowance_ns": allowance_total_ns as f64 / iterations as f64,
        "decisive_paired_wins": decisive_wins,
        "required_decisive_paired_wins": 8,
        "decision_rule": "median advantage > max(1 ns/read, 5%) and >=8/9 decisive paired wins",
        "selected_provider": if provider == "LinuxPerfMmap" { "perf" } else { "syscall" },
        "decision": decision,
      })
    }
    None => serde_json::json!({
      "available": false,
      "selected_provider": "syscall",
      "decision": "syscall",
      "reason": "perf task-clock mmap was unavailable or became unreadable",
    }),
  };
  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create criterion output directory");
  fs::write(
    directory.join("thread-cpu-selection.json"),
    serde_json::to_vec_pretty(&payload).expect("serialize thread CPU selection"),
  )
  .expect("write thread CPU selection evidence");
}

#[cfg(not(all(
  feature = "bench-internal",
  feature = "thread-cpu-inline",
  target_os = "linux",
  any(target_arch = "x86_64", target_arch = "aarch64"),
)))]
fn write_thread_cpu_selection() {
  use std::fs;
  use std::path::PathBuf;

  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let _ = fs::remove_file(target.join("criterion/thread-cpu-selection.json"));
}

#[cfg(target_os = "linux")]
const NATIVE_THREAD_CPU_BENCH_ID: &str = "native_thread_cpu__clock_gettime_clock_thread_cputime_id";

#[cfg(target_os = "linux")]
#[inline(always)]
fn native_thread_cpu_now() -> u64 {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: the pointer is writable timespec storage and the clock ID names
  // the calling thread's CPU-time clock.
  if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, value.as_mut_ptr()) } != 0 {
    return 0;
  }
  // SAFETY: clock_gettime initialized the timespec on success.
  let value = unsafe { value.assume_init() };
  let Ok(seconds) = u64::try_from(value.tv_sec) else {
    return 0;
  };
  let Ok(nanos) = u32::try_from(value.tv_nsec) else {
    return 0;
  };
  seconds
    .checked_mul(1_000_000_000)
    .and_then(|base| base.checked_add(u64::from(nanos)))
    .unwrap_or(0)
}

#[cfg(target_os = "macos")]
const NATIVE_THREAD_CPU_BENCH_ID: &str =
  "native_thread_cpu__clock_gettime_nsec_np_clock_thread_cputime_id";

#[cfg(target_os = "macos")]
#[inline(always)]
fn native_thread_cpu_now() -> u64 {
  unsafe extern "C" {
    fn clock_gettime_nsec_np(clock_id: libc::clockid_t) -> u64;
  }

  // SAFETY: CLOCK_THREAD_CPUTIME_ID is a valid clock identifier on macOS.
  unsafe { clock_gettime_nsec_np(libc::CLOCK_THREAD_CPUTIME_ID) }
}

#[cfg(target_os = "windows")]
const NATIVE_THREAD_CPU_BENCH_ID: &str = "native_thread_cpu__get_thread_times";

#[cfg(target_os = "windows")]
#[inline(always)]
fn native_thread_cpu_now() -> u64 {
  use std::ffi::c_void;

  #[repr(C)]
  struct FileTime {
    low: u32,
    high: u32,
  }

  #[link(name = "kernel32")]
  unsafe extern "system" {
    fn GetCurrentThread() -> *mut c_void;
    fn GetThreadTimes(
      thread: *mut c_void,
      creation_time: *mut FileTime,
      exit_time: *mut FileTime,
      kernel_time: *mut FileTime,
      user_time: *mut FileTime,
    ) -> i32;
  }

  let mut creation = MaybeUninit::<FileTime>::uninit();
  let mut exit = MaybeUninit::<FileTime>::uninit();
  let mut kernel = MaybeUninit::<FileTime>::uninit();
  let mut user = MaybeUninit::<FileTime>::uninit();
  // SAFETY: the current-thread pseudo-handle is valid and all outputs point to
  // writable FILETIME storage.
  let status = unsafe {
    GetThreadTimes(
      GetCurrentThread(),
      creation.as_mut_ptr(),
      exit.as_mut_ptr(),
      kernel.as_mut_ptr(),
      user.as_mut_ptr(),
    )
  };
  if status == 0 {
    return 0;
  }
  // SAFETY: GetThreadTimes initialized both values on success.
  let kernel = unsafe { kernel.assume_init() };
  // SAFETY: GetThreadTimes initialized both values on success.
  let user = unsafe { user.assume_init() };
  let kernel_100ns = (u64::from(kernel.high) << 32) | u64::from(kernel.low);
  let user_100ns = (u64::from(user.high) << 32) | u64::from(user.low);
  kernel_100ns.saturating_add(user_100ns).saturating_mul(100)
}

fn bench_ordered(c: &mut Criterion) {
  let mut g = c.benchmark_group("Ordered Instant::now()");
  g.bench_function("tach::OrderedInstant", |b| {
    b.iter(|| black_box(OrderedInstant::now()));
  });
  g.bench_function("tach::OrderedInstant (now + elapsed)", |b| {
    b.iter(|| {
      let start = OrderedInstant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("tach::Instant (unordered reference)", |b| {
    b.iter(|| black_box(Instant::now()));
  });
  g.bench_function("std::time::Instant", |b| {
    b.iter(|| black_box(StdInstant::now()));
  });
  g.bench_function("std::time::Instant (now + elapsed)", |b| {
    b.iter(|| {
      let start = StdInstant::now();
      black_box(start.elapsed())
    });
  });
  g.finish();
}

// Isolates `elapsed()` alone (one counter read + the subtraction + conversion),
// holding `start` outside the loop so the second `now()` of the combined bench
// doesn't dilute the signal. This is the group that exposes the saturating_sub
// cost most directly.
fn bench_elapsed_only(c: &mut Criterion) {
  let mut g = c.benchmark_group("elapsed() only");
  let tach_start = Instant::now();
  g.bench_function("tach::Instant", |b| {
    b.iter(|| black_box(black_box(tach_start).elapsed()));
  });
  let ordered_start = OrderedInstant::now();
  g.bench_function("tach::OrderedInstant", |b| {
    b.iter(|| black_box(black_box(ordered_start).elapsed()));
  });
  let thread_cpu_start = ThreadCpuInstant::now();
  g.bench_function("tach::ThreadCpuInstant", |b| {
    b.iter(|| black_box(black_box(thread_cpu_start).elapsed()));
  });
  let std_start = StdInstant::now();
  g.bench_function("std::time::Instant", |b| {
    b.iter(|| black_box(black_box(std_start).elapsed()));
  });
  g.finish();
}

criterion_group!(
  benches,
  bench_now,
  bench_elapsed,
  bench_thread_cpu_now,
  bench_thread_cpu_elapsed,
  bench_elapsed_only,
  bench_ordered,
);
criterion_main!(benches);
