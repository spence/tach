use std::arch::asm;
use std::hint::black_box;
use std::mem::MaybeUninit;
use std::time::{Duration, Instant as WallInstant};

use lambda_runtime::{Error, LambdaEvent, service_fn};
use serde_json::{Value, json};
use tach::{Instant, OrderedInstant, ThreadCpuInstant, ThreadCpuProvider, ThreadCpuReadCost};

const ITERATIONS: usize = 100_000;
const SAMPLES: usize = 31;
const WARMUP_ITERATIONS: usize = 10_000;

struct CostSamples {
  median: f64,
  samples: [f64; SAMPLES],
}

type ClockRows = serde_json::Map<String, Value>;
type ExactThreadCpuEvidence = (ClockRows, Value, Option<(String, Value)>);

#[tokio::main]
async fn main() -> Result<(), Error> {
  lambda_runtime::run(service_fn(handler)).await
}

async fn handler(_event: LambdaEvent<Value>) -> Result<Value, Error> {
  quanta::Instant::now();
  fastant::Instant::now();
  minstant::Instant::now();

  let provider_kind = ThreadCpuInstant::provider();
  if !provider_kind.measures_thread_cpu_time() {
    return Err("Lambda benchmark requires a native current-thread CPU clock".into());
  }
  let provider = provider_label(provider_kind);
  let read_cost = read_cost_label(ThreadCpuInstant::read_cost_hint());
  let time_domain = time_domain_label(provider_kind);

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
  let thread_selection = tach::bench::thread_cpu_native64_selection_measurements();
  let thread_selection_json = thread_cpu_selection_json(&thread_selection);
  let (thread_exact_rows, selected_thread, fallback_thread) =
    exact_thread_cpu_evidence(&thread_selection, &thread_selection_json)?;
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
  let selected_instant = tach::bench::linux_x86_selected_instant_primitive();
  let selected_instant_provider = selected_instant.provider();
  let (selected_instant_now, selected_instant_elapsed) = exact_instant_wall_cost(
    selected_instant_provider,
    selected_instant.nanos_per_tick_q32(),
  );
  let selected_ordered = tach::bench::linux_x86_selected_ordered_primitive();
  let selected_ordered_provider = selected_ordered.provider();
  let (selected_ordered_now, selected_ordered_elapsed) = exact_ordered_wall_cost(
    selected_ordered_provider,
    selected_ordered.nanos_per_tick_q32(),
  );
  let instant_wall_rows = exact_instant_wall_rows();
  let ordered_wall_rows = exact_ordered_wall_rows();
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

  let mut result = json!({
    "tach": clock_json(tach_now, tach_elapsed),
    "tach_ordered": clock_json(tach_ordered_now, tach_ordered_elapsed),
    "direct_selected_wall": exact_wall_clock_json(
      selected_instant_now,
      selected_instant_elapsed,
      selected_instant_provider,
      "instant wall",
      &format!("direct_selected_wall__{selected_instant_provider}"),
    ),
    "direct_selected_ordered_wall": exact_wall_clock_json(
      selected_ordered_now,
      selected_ordered_elapsed,
      selected_ordered_provider,
      "ordered wall",
      &format!("direct_selected_ordered_wall__{selected_ordered_provider}"),
    ),
    "quanta": clock_json(quanta_now, quanta_elapsed),
    "fastant": clock_json(fastant_now, fastant_elapsed),
    "minstant": clock_json(minstant_now, minstant_elapsed),
    "std": clock_json(std_now, std_elapsed),
    "tach_thread_cpu": thread_clock_json(
      thread_tach_now,
      thread_tach_elapsed,
      provider,
      read_cost,
      time_domain,
    ),
    "native_thread_cpu": thread_clock_json(
      native_now,
      native_elapsed,
      "inline syscall(CLOCK_THREAD_CPUTIME_ID)",
      "system call",
      "thread CPU",
    ),
    "direct_selected_thread_cpu": selected_thread,
    "thread_cpu_selection": thread_selection_json,
    "wall_selection": linux_x86_wall_selection_json(),
  });
  let result_object = result.as_object_mut().expect("Lambda result is an object");
  if let Some((benchmark, row)) = fallback_thread {
    result_object.insert("direct_fallback_thread_cpu".into(), row);
    result_object
      .get_mut("direct_fallback_thread_cpu")
      .and_then(Value::as_object_mut)
      .expect("fallback thread-CPU row")
      .insert("benchmark".into(), json!(benchmark));
  }
  result_object.extend(thread_exact_rows);
  result_object.extend(instant_wall_rows);
  result_object.extend(ordered_wall_rows);
  Ok(result)
}

fn exact_thread_cpu_row(
  read: impl Fn() -> u64 + Copy,
  provider: &str,
  read_cost: &str,
  benchmark: &str,
) -> Value {
  let now = median_cost(|| black_box(read()));
  let elapsed = median_cost(|| {
    let start = read();
    black_box(Duration::from_nanos(read().saturating_sub(start)))
  });
  let mut row = thread_clock_json(now, elapsed, provider, read_cost, "thread CPU");
  row
    .as_object_mut()
    .expect("exact thread-CPU row")
    .insert("benchmark".into(), json!(benchmark));
  row
}

fn exact_thread_cpu_evidence(
  native: &tach::bench::ThreadCpuNative64SelectionMeasurements,
  selection: &Value,
) -> Result<ExactThreadCpuEvidence, Error> {
  let mut rows = serde_json::Map::new();
  if native.libc_available {
    let benchmark = format!("direct_thread_cpu__{}", native.libc_provider);
    rows.insert(
      benchmark.clone(),
      exact_thread_cpu_row(
        tach::bench::thread_cpu_native64_exact_libc_nanos,
        native.libc_provider,
        "system call",
        &benchmark,
      ),
    );
  }
  if native.raw_available {
    let benchmark = format!("direct_thread_cpu__{}", native.raw_provider);
    rows.insert(
      benchmark.clone(),
      exact_thread_cpu_row(
        tach::bench::thread_cpu_native64_exact_raw_nanos,
        native.raw_provider,
        "system call",
        &benchmark,
      ),
    );
  }
  if let Some(perf) = tach::bench::ThreadCpuPerfHandle::try_for_current_thread() {
    for candidate in 0..perf.candidate_count() {
      let Some(name) = perf.candidate_name(candidate) else {
        continue;
      };
      if !perf.select_candidate(candidate) {
        continue;
      }
      let mechanism = format!("linux_perf_mmap__{name}");
      let benchmark = format!("direct_thread_cpu__{mechanism}");
      rows.insert(
        benchmark.clone(),
        exact_thread_cpu_row(|| perf.now_nanos(), &mechanism, "inline", &benchmark),
      );
    }
  }
  if let Some(perf) = tach::bench::ThreadCpuPerfReadHandle::try_for_current_thread() {
    for candidate in 0..perf.candidate_count() {
      let Some(name) = perf.candidate_name(candidate) else {
        continue;
      };
      if !perf.select_candidate(candidate) {
        continue;
      }
      let mechanism = format!("linux_perf_read__{name}");
      let benchmark = format!("direct_thread_cpu__{mechanism}");
      rows.insert(
        benchmark.clone(),
        exact_thread_cpu_row(|| perf.direct_nanos(), &mechanism, "system call", &benchmark),
      );
    }
  }

  let selected_mechanism = selection["selected_mechanism"]
    .as_str()
    .ok_or("selected thread-CPU mechanism is missing")?;
  let selected_cost = selection["selected_read_cost"]
    .as_str()
    .ok_or("selected thread-CPU read cost is missing")?;
  let selected_benchmark = selection["selected_native_benchmark"]
    .as_str()
    .ok_or("selected thread-CPU benchmark is missing")?;
  let path_evidence = tach::bench::thread_cpu_perf_path_evidence();
  if let Some(path_evidence) = path_evidence {
    let paths = tach::bench::ThreadCpuPerfPathHandle::try_for_current_thread()
      .ok_or("measured thread-CPU paths became unavailable")?;
    let mut selected = None;
    let mut fallback = None;
    let fallback_mechanism = selection["fallback_mechanism"]
      .as_str()
      .ok_or("measured fallback mechanism is missing")?;
    let fallback_cost = selection["fallback_read_cost"]
      .as_str()
      .ok_or("measured fallback read cost is missing")?;
    let fallback_benchmark = selection["fallback_native_benchmark"]
      .as_str()
      .ok_or("measured fallback benchmark is missing")?;
    for (is_selected, path, mechanism, cost, benchmark) in [
      (true, path_evidence.selected_path, selected_mechanism, selected_cost, selected_benchmark),
      (false, path_evidence.fallback_path, fallback_mechanism, fallback_cost, fallback_benchmark),
    ] {
      for candidate in 0..paths.candidate_count() {
        if paths.candidate_name(candidate) == Some(path)
          && paths.candidate_available(candidate)
          && paths.select_candidate(candidate)
        {
          let row = exact_thread_cpu_row(|| paths.now_nanos(), mechanism, cost, benchmark);
          if is_selected {
            selected = Some(row);
          } else {
            fallback = Some((benchmark.to_owned(), row));
          }
          break;
        }
      }
    }
    Ok((rows, selected.ok_or("selected measured path was not benchmarked")?, fallback))
  } else {
    let selected = if native.selected_provider == native.libc_provider {
      exact_thread_cpu_row(
        tach::bench::thread_cpu_native64_exact_libc_nanos,
        selected_mechanism,
        selected_cost,
        selected_benchmark,
      )
    } else {
      exact_thread_cpu_row(
        tach::bench::thread_cpu_native64_exact_raw_nanos,
        selected_mechanism,
        selected_cost,
        selected_benchmark,
      )
    };
    Ok((rows, selected, None))
  }
}

fn thread_cpu_selection_json(
  evidence: &tach::bench::ThreadCpuNative64SelectionMeasurements,
) -> Value {
  let mut candidates = Vec::with_capacity(2);
  if evidence.libc_available {
    candidates.push(format!("direct_thread_cpu__{}", evidence.libc_provider));
  }
  if evidence.raw_available {
    candidates.push(format!("direct_thread_cpu__{}", evidence.raw_provider));
  }
  let path = tach::bench::thread_cpu_perf_path_evidence();
  let mmap_available = path.as_ref().is_some_and(|probe| probe.mmap_batches_ns.is_some());
  let counter = tach::bench::thread_cpu_perf_counter_evidence();
  let counter_probe = counter.as_ref().filter(|_| mmap_available).map(|probe| {
    let count = probe.candidate_count;
    json!({
      "selection_kind": "tournament",
      "candidate_names": &probe.candidate_names[..count],
      "candidate_eligible": &probe.candidate_eligible[..count],
      "candidate_batches_ns": &probe.candidate_batches_ns[..count],
      "selected_candidate": probe.selected_candidate,
      "reads_per_batch": probe.reads_per_batch,
      "required_decisive_wins": probe.required_decisive_wins,
      "equivalence_band": {"floor_ns_per_read": 1, "relative_denominator": 20},
    })
  });
  let mmap_mechanism = counter
    .as_ref()
    .filter(|_| mmap_available)
    .map(|probe| format!("linux_perf_mmap__{}", probe.selected_candidate));
  let mmap_candidates: Vec<_> = counter
    .as_ref()
    .filter(|_| mmap_available)
    .into_iter()
    .flat_map(|probe| {
      probe.candidate_names[..probe.candidate_count]
        .iter()
        .zip(&probe.candidate_eligible[..probe.candidate_count])
        .filter(|(_, eligible)| **eligible)
        .map(|(name, _)| format!("direct_thread_cpu__linux_perf_mmap__{name}"))
    })
    .collect();
  let read_entry = path.as_ref().and_then(|_| tach::bench::thread_cpu_perf_read_entry_evidence());
  let read_mechanism = read_entry
    .as_ref()
    .map(|probe| format!("linux_perf_read__{}", probe.selected_candidate));
  let read_candidates: Vec<_> = read_entry
    .as_ref()
    .into_iter()
    .flat_map(|probe| {
      probe.candidate_names[..probe.candidate_count]
        .iter()
        .zip(&probe.candidate_eligible[..probe.candidate_count])
        .filter(|(_, eligible)| **eligible)
        .map(|(name, _)| format!("direct_thread_cpu__linux_perf_read__{name}"))
    })
    .collect();
  let read_entry_probe = read_entry.as_ref().map(|probe| {
    let count = probe.candidate_count;
    json!({
      "selection_kind": "fixed_candidate",
      "candidate_names": &probe.candidate_names[..count],
      "candidate_eligible": &probe.candidate_eligible[..count],
      "candidate_measured": &probe.candidate_measured[..count],
      "candidate_batches_ns": null,
      "selected_candidate": probe.selected_candidate,
      "reads_per_batch": probe.reads_per_batch,
      "required_decisive_wins": probe.required_decisive_wins,
      "equivalence_band": {"floor_ns_per_read": 1, "relative_denominator": 20},
    })
  });
  candidates.extend(mmap_candidates.iter().cloned());
  candidates.extend(read_candidates.iter().cloned());
  let mechanism_for_path = |name: &str| match name {
    "linux_perf_mmap" => mmap_mechanism.clone().expect("mmap mechanism"),
    "linux_perf_read" => read_mechanism.clone().expect("read mechanism"),
    "posix_thread_cpu" => evidence.selected_provider.to_owned(),
    _ => panic!("unknown thread-CPU path"),
  };
  let provider_for_path = |name: &str| match name {
    "linux_perf_mmap" => "linux_perf_mmap",
    "linux_perf_read" => "linux_perf_read",
    "posix_thread_cpu" => "posix_thread_cpu_clock",
    _ => panic!("unknown thread-CPU path"),
  };
  let cost_for_path = |name: &str| match name {
    "linux_perf_mmap" => "inline",
    "linux_perf_read" | "posix_thread_cpu" => "system call",
    _ => panic!("unknown thread-CPU path"),
  };
  let selected_path = path.as_ref().map_or("posix_thread_cpu", |probe| probe.selected_path);
  let selected_mechanism = mechanism_for_path(selected_path);
  let fallback_path = path.as_ref().map(|probe| probe.fallback_path);
  let fallback_mechanism = fallback_path.map(mechanism_for_path);
  let path_probe = path.as_ref().map(|probe| {
    json!({
      "selection_kind": "tournament_with_measured_runner_up",
      "candidate_names": ["posix_thread_cpu", "linux_perf_mmap", "linux_perf_read"],
      "candidate_eligible": [true, probe.mmap_batches_ns.is_some(), true],
      "candidate_batches_ns": [
        probe.posix_batches_ns,
        probe.mmap_batches_ns.unwrap_or([0; 9]),
        probe.read_batches_ns,
      ],
      "selected_candidate": probe.selected_path,
      "fallback_candidate": probe.fallback_path,
      "reads_per_batch": probe.reads_per_batch,
      "required_decisive_wins": probe.required_decisive_wins,
      "equivalence_band": {"floor_ns_per_read": 1, "relative_denominator": 20},
      "capability_was_not_profitable": probe.mmap_batches_ns.is_some()
        && probe.selected_path != "linux_perf_mmap",
    })
  });
  json!({
    "selected_provider": provider_for_path(selected_path),
    "selected_mechanism": selected_mechanism,
    "selected_read_cost": cost_for_path(selected_path),
    "selected_native_benchmark": format!("direct_selected_thread_cpu__{selected_mechanism}"),
    "fallback_provider": fallback_path.map(provider_for_path),
    "fallback_mechanism": fallback_mechanism,
    "fallback_read_cost": fallback_path.map(cost_for_path),
    "fallback_native_benchmark": fallback_mechanism
      .as_ref()
      .map(|mechanism| format!("direct_fallback_thread_cpu__{mechanism}")),
    "eligible_direct_candidates": candidates,
    "native_entry_probe": evidence,
    "perf": {
      "event_available": path.is_some(),
      "path_probe": path_probe,
      "mmap": {
        "supported_on_target": true,
        "available": mmap_available,
        "read_cost": "inline",
        "selected_mechanism": mmap_mechanism,
        "selected_candidate_benchmark": mmap_mechanism
          .as_ref()
          .map(|mechanism| format!("direct_thread_cpu__{mechanism}")),
        "eligible_benchmarks": mmap_candidates,
        "counter_probe": counter_probe,
      },
      "read": {
        "supported_on_target": true,
        "available": read_entry.is_some(),
        "read_cost": "system call",
        "selected_mechanism": read_mechanism,
        "selected_candidate_benchmark": read_mechanism
          .as_ref()
          .map(|mechanism| format!("direct_thread_cpu__{mechanism}")),
        "eligible_benchmarks": read_candidates,
        "entry_probe": read_entry_probe,
      },
      "measurement_clock": "raw SYS_clock_gettime(CLOCK_MONOTONIC_RAW), never libc/vDSO or the candidate under test",
      "decision_rule": "POSIX, eligible perf-mmap, and persistent perf-read complete public-dispatch paths compete by repeatable material wins; the same tournament excluding the winner selects the measured fallback",
    },
    "read_cost_basis": "perf mmap is Inline; persistent perf read and CLOCK_THREAD_CPUTIME_ID entries are SystemCall",
  })
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

fn exact_wall_clock_json(
  now: CostSamples,
  elapsed: CostSamples,
  provider: &str,
  time_domain: &str,
  benchmark: &str,
) -> Value {
  let mut row = json!({
    "now": now.median,
    "elapsed": elapsed.median,
    "now_samples": now.samples,
    "elapsed_samples": elapsed.samples,
    "provider": provider,
    "read_cost": if provider.contains("vdso_direct") {
      "direct vDSO call"
    } else if provider.contains("syscall") {
      "system call"
    } else if provider.contains("_libc") {
      "platform call"
    } else {
      "inline"
    },
    "time_domain": time_domain,
  });
  row
    .as_object_mut()
    .expect("exact wall row")
    .insert("benchmark".into(), json!(benchmark));
  row
}

macro_rules! with_lambda_linux_x86_instant_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "linux_kernel_eligible_tsc" => $callback!($($arguments)*, tach::bench::linux_x86_exact_tsc),
      "linux_clock_monotonic_libc" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic),
      "linux_clock_monotonic_raw_libc" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw),
      "linux_clock_boottime_libc" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime),
      "linux_clock_monotonic_vdso_direct" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic),
      "linux_clock_monotonic_raw_vdso_direct" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw),
      "linux_clock_boottime_vdso_direct" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime),
      "linux_clock_monotonic_syscall_x86_64" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic),
      "linux_clock_monotonic_raw_syscall_x86_64" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw),
      "linux_clock_boottime_syscall_x86_64" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime),
      _ => panic!("unsupported Lambda Linux x86 Instant provider: {}", $provider),
    }
  }};
}
macro_rules! with_lambda_linux_x86_ordered_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "linux_kernel_eligible_tsc_x86_lfence_rdtsc" => $callback!($($arguments)*, tach::bench::linux_x86_exact_tsc_lfence),
      "linux_kernel_eligible_tsc_x86_mfence_rdtsc" => $callback!($($arguments)*, tach::bench::linux_x86_exact_tsc_mfence),
      "linux_kernel_eligible_tsc_x86_rdtscp" => $callback!($($arguments)*, tach::bench::linux_x86_exact_tsc_rdtscp),
      "linux_kernel_eligible_tsc_x86_cpuid_rdtsc" => $callback!($($arguments)*, tach::bench::linux_x86_exact_tsc_cpuid),
      "linux_kernel_eligible_tsc_x86_serialize_rdtsc" => $callback!($($arguments)*, tach::bench::linux_x86_exact_tsc_serialize),
      "linux_clock_monotonic_libc_x86_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic_lfence),
      "linux_clock_monotonic_libc_os_owned" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic_os_owned),
      "linux_clock_monotonic_libc_x86_rdtscp_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic_rdtscp),
      "linux_clock_monotonic_libc_x86_mfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic_mfence),
      "linux_clock_monotonic_libc_x86_cpuid" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic_cpuid),
      "linux_clock_monotonic_libc_x86_serialize" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic_serialize),
      "linux_clock_monotonic_raw_libc_x86_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw_lfence),
      "linux_clock_monotonic_raw_libc_os_owned" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw_os_owned),
      "linux_clock_monotonic_raw_libc_x86_rdtscp_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw_rdtscp),
      "linux_clock_monotonic_raw_libc_x86_mfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw_mfence),
      "linux_clock_monotonic_raw_libc_x86_cpuid" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw_cpuid),
      "linux_clock_monotonic_raw_libc_x86_serialize" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw_serialize),
      "linux_clock_monotonic_vdso_direct_os_owned" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic_os_owned),
      "linux_clock_monotonic_vdso_direct_x86_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic_lfence),
      "linux_clock_monotonic_vdso_direct_x86_rdtscp_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic_rdtscp),
      "linux_clock_monotonic_vdso_direct_x86_mfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic_mfence),
      "linux_clock_monotonic_vdso_direct_x86_cpuid" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic_cpuid),
      "linux_clock_monotonic_vdso_direct_x86_serialize" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic_serialize),
      "linux_clock_monotonic_raw_vdso_direct_os_owned" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw_os_owned),
      "linux_clock_monotonic_raw_vdso_direct_x86_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw_lfence),
      "linux_clock_monotonic_raw_vdso_direct_x86_rdtscp_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw_rdtscp),
      "linux_clock_monotonic_raw_vdso_direct_x86_mfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw_mfence),
      "linux_clock_monotonic_raw_vdso_direct_x86_cpuid" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw_cpuid),
      "linux_clock_monotonic_raw_vdso_direct_x86_serialize" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw_serialize),
      "linux_clock_monotonic_syscall_x86_64_x86_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic_lfence),
      "linux_clock_monotonic_syscall_x86_64_os_owned" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic_os_owned),
      "linux_clock_monotonic_syscall_x86_64_x86_rdtscp_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic_rdtscp),
      "linux_clock_monotonic_syscall_x86_64_x86_mfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic_mfence),
      "linux_clock_monotonic_syscall_x86_64_x86_cpuid" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic_cpuid),
      "linux_clock_monotonic_syscall_x86_64_x86_serialize" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic_serialize),
      "linux_clock_monotonic_raw_syscall_x86_64_x86_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw_lfence),
      "linux_clock_monotonic_raw_syscall_x86_64_os_owned" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw_os_owned),
      "linux_clock_monotonic_raw_syscall_x86_64_x86_rdtscp_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw_rdtscp),
      "linux_clock_monotonic_raw_syscall_x86_64_x86_mfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw_mfence),
      "linux_clock_monotonic_raw_syscall_x86_64_x86_cpuid" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw_cpuid),
      "linux_clock_monotonic_raw_syscall_x86_64_x86_serialize" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw_serialize),
      "linux_clock_boottime_libc_os_owned" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime_os_owned),
      "linux_clock_boottime_libc_x86_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime_lfence),
      "linux_clock_boottime_libc_x86_rdtscp_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime_rdtscp),
      "linux_clock_boottime_libc_x86_mfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime_mfence),
      "linux_clock_boottime_libc_x86_cpuid" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime_cpuid),
      "linux_clock_boottime_libc_x86_serialize" => $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime_serialize),
      "linux_clock_boottime_vdso_direct_os_owned" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime_os_owned),
      "linux_clock_boottime_vdso_direct_x86_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime_lfence),
      "linux_clock_boottime_vdso_direct_x86_rdtscp_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime_rdtscp),
      "linux_clock_boottime_vdso_direct_x86_mfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime_mfence),
      "linux_clock_boottime_vdso_direct_x86_cpuid" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime_cpuid),
      "linux_clock_boottime_vdso_direct_x86_serialize" => $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime_serialize),
      "linux_clock_boottime_syscall_x86_64_os_owned" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime_os_owned),
      "linux_clock_boottime_syscall_x86_64_x86_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime_lfence),
      "linux_clock_boottime_syscall_x86_64_x86_rdtscp_lfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime_rdtscp),
      "linux_clock_boottime_syscall_x86_64_x86_mfence" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime_mfence),
      "linux_clock_boottime_syscall_x86_64_x86_cpuid" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime_cpuid),
      "linux_clock_boottime_syscall_x86_64_x86_serialize" => $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime_serialize),
      _ => panic!("unsupported Lambda Linux x86 OrderedInstant provider: {}", $provider),
    }
  }};
}

macro_rules! exact_instant_wall_cost {
  ($nanos_per_tick_q32:expr, $read:path) => {{
    let now = median_cost(|| black_box($read()));
    let elapsed = median_cost(|| {
      let start = $read();
      let elapsed = $read().saturating_sub(start);
      black_box(tach::bench::exact_ticks_to_duration_with_scale(
        elapsed,
        $nanos_per_tick_q32,
      ))
    });
    (now, elapsed)
  }};
}

macro_rules! exact_ordered_wall_cost {
  ($nanos_per_tick_q32:expr, $read:path) => {{
    let now = median_cost(|| black_box($read()));
    let elapsed = median_cost(|| {
      let start = $read();
      let elapsed = $read().saturating_sub(start);
      black_box(tach::bench::exact_ticks_to_duration_with_scale(
        elapsed,
        $nanos_per_tick_q32,
      ))
    });
    (now, elapsed)
  }};
}

fn exact_instant_wall_cost(
  provider: &str,
  nanos_per_tick_q32: u64,
) -> (CostSamples, CostSamples) {
  with_lambda_linux_x86_instant_read!(
    provider,
    exact_instant_wall_cost,
    nanos_per_tick_q32
  )
}

fn exact_ordered_wall_cost(
  provider: &str,
  nanos_per_tick_q32: u64,
) -> (CostSamples, CostSamples) {
  with_lambda_linux_x86_ordered_read!(
    provider,
    exact_ordered_wall_cost,
    nanos_per_tick_q32
  )
}

fn exact_instant_wall_rows() -> serde_json::Map<String, Value> {
  let mut rows = serde_json::Map::new();
  for primitive in tach::bench::linux_x86_instant_candidate_primitives() {
    let provider = primitive.provider();
    let benchmark = format!("direct_wall__{provider}");
    let (now, elapsed) = exact_instant_wall_cost(provider, primitive.nanos_per_tick_q32());
    rows.insert(
      benchmark.clone(),
      exact_wall_clock_json(now, elapsed, provider, "instant wall", &benchmark),
    );
  }
  rows
}

fn exact_ordered_wall_rows() -> serde_json::Map<String, Value> {
  let mut rows = serde_json::Map::new();
  for primitive in tach::bench::linux_x86_ordered_candidate_primitives() {
    let provider = primitive.provider();
    let benchmark = format!("direct_ordered_wall__{provider}");
    let (now, elapsed) = exact_ordered_wall_cost(provider, primitive.nanos_per_tick_q32());
    rows.insert(
      benchmark.clone(),
      exact_wall_clock_json(now, elapsed, provider, "ordered wall", &benchmark),
    );
  }
  rows
}

fn linux_x86_wall_selection_json() -> Value {
  let instant = tach::bench::linux_x86_selected_instant_primitive();
  let ordered = tach::bench::linux_x86_selected_ordered_primitive();
  let instant_candidates: Vec<_> = tach::bench::linux_x86_instant_candidate_primitives()
    .iter()
    .map(|candidate| format!("direct_wall__{}", candidate.provider()))
    .collect();
  let ordered_candidates: Vec<_> = tach::bench::linux_x86_ordered_candidate_primitives()
    .iter()
    .map(|candidate| format!("direct_ordered_wall__{}", candidate.provider()))
    .collect();
  json!({
    "selected_provider": {
      "instant": instant.provider(),
      "ordered": ordered.provider(),
    },
    "selected_native_benchmark": {
      "instant": format!("direct_selected_wall__{}", instant.provider()),
      "ordered": format!("direct_selected_ordered_wall__{}", ordered.provider()),
    },
    "eligible_direct_candidates": {
      "instant": instant_candidates,
      "ordered": ordered_candidates,
    },
    "decision_rule": "each contract independently tournaments every eligible complete clock-id, entry-ABI, ordering-barrier, and direct-TSC path; a challenger wins only by > max(1 ns/read, 5%) in >=8/9 paired batches",
    "probe": tach::bench::linux_x86_wall_selection_measurements(),
    "post_init_boundary": "PR_SET_TSC(PR_TSC_SIGSEGV) must not revoke TSC access after direct-provider selection",
  })
}

fn provider_label(provider: ThreadCpuProvider) -> &'static str {
  match provider {
    ThreadCpuProvider::LinuxPerfMmap => "Linux perf task-clock mmap",
    ThreadCpuProvider::LinuxPerfRead => "Linux perf task-clock read",
    ThreadCpuProvider::PosixThreadCpuClock => "POSIX thread CPU clock",
    ThreadCpuProvider::WindowsThreadTimes => "Windows GetThreadTimes",
    ThreadCpuProvider::WasiThreadCpuClock => "WASI thread CPU clock",
    ThreadCpuProvider::NodeThreadCpuUsage => "Node thread CPU usage",
    ThreadCpuProvider::PerformanceNow => "performance.now",
    ThreadCpuProvider::NodeHrtime => "process.hrtime.bigint",
    ThreadCpuProvider::MonotonicWallClock => "monotonic wall clock",
    ThreadCpuProvider::Unavailable => "unavailable",
    _ => "other",
  }
}

fn read_cost_label(cost: ThreadCpuReadCost) -> &'static str {
  match cost {
    ThreadCpuReadCost::Inline => "inline",
    ThreadCpuReadCost::SystemCall => "system call",
    ThreadCpuReadCost::HostCall => "host call",
    ThreadCpuReadCost::Unavailable => "unavailable",
    _ => "unknown",
  }
}

fn time_domain_label(provider: ThreadCpuProvider) -> &'static str {
  if provider.measures_thread_cpu_time() { "thread CPU" } else { "monotonic wall fallback" }
}

#[inline(always)]
fn native_thread_cpu_now() -> u64 {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  let mut status = libc::SYS_clock_gettime;
  // SAFETY: this is the Linux x86_64 syscall ABI and value is writable
  // timespec storage.
  unsafe {
    asm!(
      "syscall",
      inlateout("rax") status,
      in("rdi") libc::CLOCK_THREAD_CPUTIME_ID,
      in("rsi") value.as_mut_ptr(),
      lateout("rcx") _,
      lateout("r11") _,
      options(nostack),
    );
  }
  assert_eq!(status, 0, "CLOCK_THREAD_CPUTIME_ID syscall failed");
  // SAFETY: clock_gettime initialized the timespec on success.
  let value = unsafe { value.assume_init() };
  let seconds = u64::try_from(value.tv_sec).expect("thread CPU seconds were negative");
  let nanos = u32::try_from(value.tv_nsec).expect("thread CPU nanoseconds were invalid");
  seconds
    .checked_mul(1_000_000_000)
    .and_then(|base| base.checked_add(u64::from(nanos)))
    .expect("thread CPU time overflowed u64 nanoseconds")
}
