use std::hint::black_box;
use std::mem::MaybeUninit;
use std::time::{Duration, Instant as WallInstant};

use lambda_runtime::{Error, LambdaEvent, service_fn};
use serde_json::{Value, json};
#[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))]
use tach::bench::ThreadCpuPerfHandle;
use tach::{Instant, OrderedInstant, ThreadCpuInstant, ThreadCpuProvider, ThreadCpuReadCost};

const ITERATIONS: usize = 100_000;
const SAMPLES: usize = 31;
const WARMUP_ITERATIONS: usize = 10_000;

struct CostSamples {
  median: f64,
  samples: [f64; SAMPLES],
}

#[tokio::main]
async fn main() -> Result<(), Error> {
  lambda_runtime::run(service_fn(handler)).await
}

async fn handler(_event: LambdaEvent<Value>) -> Result<Value, Error> {
  quanta::Instant::now();
  fastant::Instant::now();
  minstant::Instant::now();

  let provider = provider_label(ThreadCpuInstant::provider());
  let read_cost = read_cost_label(ThreadCpuInstant::read_cost_hint());
  let time_domain = time_domain_label(ThreadCpuInstant::provider());

  let thread_tach_now = median_cost(|| black_box(ThreadCpuInstant::now()));
  let thread_tach_elapsed = median_cost(|| {
    let start = ThreadCpuInstant::now();
    black_box(start.elapsed())
  });
  let native_now = median_cost(|| black_box(native_thread_cpu_now()));
  let native_elapsed = median_cost(|| {
    let start = native_thread_cpu_now();
    black_box(Duration::from_nanos(native_thread_cpu_now().saturating_sub(start)))
  });
  #[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))]
  let direct_perf = ThreadCpuPerfHandle::try_for_current_thread().map(|direct| {
    let now = median_cost(|| black_box(direct.now_nanos()));
    let elapsed = median_cost(|| {
      let start = direct.now_nanos();
      black_box(Duration::from_nanos(direct.now_nanos().saturating_sub(start)))
    });
    thread_clock_json(
      now,
      elapsed,
      "Linux perf mmap (cached direct handle)",
      "inline",
      "thread CPU",
    )
  });
  #[cfg(not(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
  let direct_perf: Option<Value> = None;
  let tach_now = median_cost(|| black_box(Instant::now()));
  let tach_elapsed = median_cost(|| {
    let start = Instant::now();
    black_box(start.elapsed())
  });
  let tach_ordered_now = median_cost(|| black_box(OrderedInstant::now()));
  let tach_ordered_elapsed = median_cost(|| {
    let start = OrderedInstant::now();
    black_box(start.elapsed())
  });
  let quanta_now = median_cost(|| black_box(quanta::Instant::now()));
  let quanta_elapsed = median_cost(|| {
    let start = quanta::Instant::now();
    black_box(start.elapsed())
  });
  let fastant_now = median_cost(|| black_box(fastant::Instant::now()));
  let fastant_elapsed = median_cost(|| {
    let start = fastant::Instant::now();
    black_box(start.elapsed())
  });
  let minstant_now = median_cost(|| black_box(minstant::Instant::now()));
  let minstant_elapsed = median_cost(|| {
    let start = minstant::Instant::now();
    black_box(start.elapsed())
  });
  let std_now = median_cost(|| black_box(WallInstant::now()));
  let std_elapsed = median_cost(|| {
    let start = WallInstant::now();
    black_box(start.elapsed())
  });
  if provider != provider_label(ThreadCpuInstant::provider())
    || read_cost != read_cost_label(ThreadCpuInstant::read_cost_hint())
  {
    return Err("thread-CPU provider changed during Lambda benchmark".into());
  }

  let mut tach_thread =
    thread_clock_json(thread_tach_now, thread_tach_elapsed, provider, read_cost, time_domain);
  tach_thread["selection"] = selection_json(provider);
  let mut result = json!({
    "tach": clock_json(tach_now, tach_elapsed),
    "tach_ordered": clock_json(tach_ordered_now, tach_ordered_elapsed),
    "quanta": clock_json(quanta_now, quanta_elapsed),
    "fastant": clock_json(fastant_now, fastant_elapsed),
    "minstant": clock_json(minstant_now, minstant_elapsed),
    "std": clock_json(std_now, std_elapsed),
    "tach_thread_cpu": tach_thread,
    "native_thread_cpu": thread_clock_json(
      native_now,
      native_elapsed,
      "clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
      "system call",
      "thread CPU",
    ),
  });
  if let Some(direct_perf) = direct_perf {
    result["direct_thread_cpu"] = direct_perf;
  }
  Ok(result)
}

fn median_cost<T>(mut sample: impl FnMut() -> T) -> CostSamples {
  for _ in 0..WARMUP_ITERATIONS {
    black_box(sample());
  }

  let mut values = [0.0; SAMPLES];
  for value in &mut values {
    let start = WallInstant::now();
    for _ in 0..ITERATIONS {
      black_box(sample());
    }
    *value = start.elapsed().as_nanos() as f64 / ITERATIONS as f64;
  }
  values.sort_unstable_by(f64::total_cmp);
  CostSamples { median: values[SAMPLES / 2], samples: values }
}

fn clock_json(now: CostSamples, elapsed: CostSamples) -> Value {
  json!({
    "now": now.median,
    "elapsed": elapsed.median,
    "now_samples": now.samples,
    "elapsed_samples": elapsed.samples,
  })
}

fn thread_clock_json(
  now: CostSamples,
  elapsed: CostSamples,
  provider: &str,
  read_cost: &str,
  time_domain: &str,
) -> Value {
  json!({
    "now": now.median,
    "elapsed": elapsed.median,
    "now_samples": now.samples,
    "elapsed_samples": elapsed.samples,
    "provider": provider,
    "read_cost": read_cost,
    "time_domain": time_domain,
  })
}

fn provider_label(provider: ThreadCpuProvider) -> &'static str {
  match provider {
    ThreadCpuProvider::LinuxPerfMmap => "Linux perf mmap",
    ThreadCpuProvider::PosixThreadCpuClock => "POSIX thread CPU clock",
    ThreadCpuProvider::WindowsThreadTimes => "Windows GetThreadTimes",
    _ => "other",
  }
}

fn read_cost_label(cost: ThreadCpuReadCost) -> &'static str {
  match cost {
    ThreadCpuReadCost::Inline => "inline",
    ThreadCpuReadCost::SystemCall => "system call",
    _ => "unknown",
  }
}

fn time_domain_label(provider: ThreadCpuProvider) -> &'static str {
  if provider.measures_thread_cpu_time() { "thread CPU" } else { "monotonic wall fallback" }
}

#[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))]
fn selection_json(provider: &str) -> Value {
  let Some((mut perf_samples, mut syscall_samples, iterations)) =
    tach::bench::thread_cpu_selection_measurements()
  else {
    return json!({
      "available": false,
      "selected_provider": "syscall",
      "decision": "syscall",
      "reason": "perf task-clock mmap was unavailable or became unreadable",
    });
  };
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
  let decision =
    if perf_total_ns.saturating_add(allowance_total_ns) < syscall_total_ns && decisive_wins >= 8 {
      "perf"
    } else {
      "syscall"
    };
  json!({
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
    "selected_provider": if provider == "Linux perf mmap" { "perf" } else { "syscall" },
    "decision": decision,
  })
}

#[cfg(not(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
fn selection_json(_provider: &str) -> Value {
  json!({
    "available": false,
    "selected_provider": "syscall",
    "decision": "syscall",
    "reason": "Linux inline selector is not compiled for this target",
  })
}

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
