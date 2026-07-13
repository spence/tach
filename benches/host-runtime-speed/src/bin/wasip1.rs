use std::hint::black_box;
use std::time::Duration;

use serde_json::{Map, Value, json};
use tach::{Instant, OrderedInstant, ThreadCpuInstant, ThreadCpuProvider, ThreadCpuReadCost};

const ITERATIONS: usize = 10_000;
const SAMPLES: usize = 31;
const WARMUP_ITERATIONS: usize = 100_000;
const INSTANT_PAIR: &str = "wasi-instant-v1";
const ORDERED_PAIR: &str = "wasi-ordered-v1";
const THREAD_PAIR: &str = "wasi-thread-v1";

#[cfg(feature = "wasip1-node-host")]
#[link(wasm_import_module = "tach_host")]
unsafe extern "C" {
  fn benchmark_now_nanos() -> f64;
  fn sleep_millis(millis: u32);
  fn sibling_work_millis(millis: u32);
}

#[derive(Clone, Copy)]
struct CostSamples {
  samples: [f64; SAMPLES],
}

#[derive(Clone, Copy)]
struct Costs {
  now: CostSamples,
  elapsed: CostSamples,
}

#[derive(Clone, Copy)]
struct BehaviorSample {
  wall_delta_ns: u64,
  public_delta_ns: u64,
  direct_delta_ns: u64,
}

#[derive(Clone, Copy)]
enum ThreadRoute {
  #[cfg(target_env = "p1")]
  Native,
  WallFallback,
}

impl ThreadRoute {
  fn detect() -> Result<Self, String> {
    if ThreadCpuInstant::read_cost_hint() != ThreadCpuReadCost::HostCall {
      return Err("WASI thread route changed its read-cost class".into());
    }
    let provider = ThreadCpuInstant::provider();
    #[cfg(target_env = "p1")]
    match provider {
      ThreadCpuProvider::WasiThreadCpuClock => Ok(Self::Native),
      ThreadCpuProvider::MonotonicWallClock => Ok(Self::WallFallback),
      provider => Err(format!("unexpected WASI thread provider: {provider:?}")),
    }
    #[cfg(target_env = "p2")]
    match provider {
      ThreadCpuProvider::MonotonicWallClock => Ok(Self::WallFallback),
      provider => Err(format!("unexpected WASI Preview 2 thread provider: {provider:?}")),
    }
  }

  const fn provider_name(self) -> &'static str {
    match self {
      #[cfg(target_env = "p1")]
      Self::Native => "WASI thread CPU clock",
      Self::WallFallback => "monotonic wall clock",
    }
  }

  const fn mechanism(self) -> &'static str {
    match self {
      #[cfg(target_env = "p1")]
      Self::Native => "wasi_thread_cpu_clock",
      Self::WallFallback => "wasi_clock_monotonic",
    }
  }

  const fn time_domain(self) -> &'static str {
    match self {
      #[cfg(target_env = "p1")]
      Self::Native => "thread CPU",
      Self::WallFallback => "monotonic wall fallback",
    }
  }

  fn read(self) -> u64 {
    match self {
      #[cfg(target_env = "p1")]
      Self::Native => tach::bench::wasi_exact_thread_cpu_nanos()
        .expect("WASI thread CPU clock disappeared after selection"),
      Self::WallFallback => tach::bench::wasi_exact_wall_ticks(),
    }
  }
}

fn main() {
  let observation = run_observation().unwrap_or_else(|error| panic!("{error}"));
  println!("{}", serde_json::to_string(&observation).expect("serialize observation"));
}

fn run_observation() -> Result<Value, String> {
  let runtime_attestation = runtime_attestation()?;
  quanta::Instant::now();
  fastant::Instant::now();
  minstant::Instant::now();

  let thread_route = ThreadRoute::detect()?;
  let wall_read = tach::bench::wasi_exact_wall_ticks;
  let (instant_now, direct_instant_now) =
    paired_median_cost(|| black_box(Instant::now()), || black_box(wall_read()));
  let (instant_elapsed, direct_instant_elapsed) = paired_median_cost(
    || {
      let start = Instant::now();
      black_box(start.elapsed())
    },
    || {
      let start = wall_read();
      black_box(Duration::from_nanos(wall_read().saturating_sub(start)))
    },
  );
  let (ordered_now, direct_ordered_now) =
    paired_median_cost(|| black_box(OrderedInstant::now()), || black_box(wall_read()));
  let (ordered_elapsed, direct_ordered_elapsed) = paired_median_cost(
    || {
      let start = OrderedInstant::now();
      black_box(start.elapsed())
    },
    || {
      let start = wall_read();
      black_box(Duration::from_nanos(wall_read().saturating_sub(start)))
    },
  );
  let direct_instant = Costs { now: direct_instant_now, elapsed: direct_instant_elapsed };
  let direct_ordered = Costs { now: direct_ordered_now, elapsed: direct_ordered_elapsed };

  let mut rows = Map::new();
  rows.insert(
    "tach".into(),
    paired_row(
      typed_row(
        instant_now,
        instant_elapsed,
        "wasi_clock_monotonic",
        "host call",
        "instant wall",
        None,
      ),
      INSTANT_PAIR,
    ),
  );
  rows.insert(
    "tach_ordered".into(),
    paired_row(
      typed_row(
        ordered_now,
        ordered_elapsed,
        "wasi_clock_monotonic",
        "host call",
        "ordered wall",
        None,
      ),
      ORDERED_PAIR,
    ),
  );
  rows.insert(
    "quanta".into(),
    clock_row(
      median_cost(|| black_box(quanta::Instant::now())),
      median_cost(|| {
        let start = quanta::Instant::now();
        black_box(start.elapsed())
      }),
    ),
  );
  rows.insert(
    "fastant".into(),
    clock_row(
      median_cost(|| black_box(fastant::Instant::now())),
      median_cost(|| {
        let start = fastant::Instant::now();
        black_box(start.elapsed())
      }),
    ),
  );
  rows.insert(
    "minstant".into(),
    clock_row(
      median_cost(|| black_box(minstant::Instant::now())),
      median_cost(|| {
        let start = minstant::Instant::now();
        black_box(start.elapsed())
      }),
    ),
  );
  rows.insert(
    "std".into(),
    clock_row(
      median_cost(|| black_box(std::time::Instant::now())),
      median_cost(|| {
        let start = std::time::Instant::now();
        black_box(start.elapsed())
      }),
    ),
  );
  insert_wall_rows(&mut rows, direct_instant, direct_ordered);

  let (thread_now, direct_thread_now) =
    paired_median_cost(|| black_box(ThreadCpuInstant::now()), || black_box(thread_route.read()));
  let (thread_elapsed, direct_thread_elapsed) = paired_median_cost(
    || {
      let start = ThreadCpuInstant::now();
      black_box(start.elapsed())
    },
    || {
      let start = thread_route.read();
      black_box(Duration::from_nanos(thread_route.read().saturating_sub(start)))
    },
  );
  let direct_thread = Costs { now: direct_thread_now, elapsed: direct_thread_elapsed };
  let native_benchmark = format!("native_thread_cpu__{}", thread_route.mechanism());
  rows.insert(
    "tach_thread_cpu".into(),
    paired_row(
      typed_row(
        thread_now,
        thread_elapsed,
        thread_route.provider_name(),
        "host call",
        thread_route.time_domain(),
        None,
      ),
      THREAD_PAIR,
    ),
  );
  rows.insert(
    "native_thread_cpu".into(),
    paired_row(
      typed_row(
        direct_thread.now,
        direct_thread.elapsed,
        thread_route.provider_name(),
        "host call",
        thread_route.time_domain(),
        Some(&native_benchmark),
      ),
      THREAD_PAIR,
    ),
  );
  let direct_benchmark = format!("direct_thread_cpu__{}", thread_route.mechanism());
  rows.insert(
    direct_benchmark.clone(),
    paired_row(
      typed_row(
        direct_thread.now,
        direct_thread.elapsed,
        thread_route.mechanism(),
        "host call",
        thread_route.time_domain(),
        Some(&direct_benchmark),
      ),
      THREAD_PAIR,
    ),
  );
  let selected_benchmark = format!("direct_selected_thread_cpu__{}", thread_route.mechanism());
  rows.insert(
    "direct_selected_thread_cpu".into(),
    paired_row(
      typed_row(
        direct_thread.now,
        direct_thread.elapsed,
        thread_route.mechanism(),
        "host call",
        thread_route.time_domain(),
        Some(&selected_benchmark),
      ),
      THREAD_PAIR,
    ),
  );

  let mut result = rows;
  result.insert("runtime_attestation".into(), runtime_attestation.clone());
  result.insert("wall_selection".into(), wall_selection());
  result.insert("thread_cpu_selection".into(), thread_cpu_selection(thread_route));
  result.insert(
    "thread_cpu_behavior".into(),
    thread_cpu_behavior(&runtime_attestation, &native_benchmark, thread_route),
  );
  Ok(Value::Object(result))
}

fn runtime_attestation() -> Result<Value, String> {
  let source_revision = option_env!("TACH_BENCH_SOURCE_REVISION")
    .filter(|value| {
      matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
    .ok_or("host-runtime benchmark build omitted a valid source revision")?;
  let invocation_id = option_env!("TACH_BENCH_INVOCATION_ID")
    .filter(|value| !value.is_empty())
    .ok_or("host-runtime benchmark build omitted its invocation ID")?;
  let runner = option_env!("TACH_BENCH_RUNNER")
    .filter(|value| !value.is_empty())
    .ok_or("host-runtime benchmark build omitted its runner identity")?;
  Ok(json!({
    "schema": "tach-benchmark-runtime-v2",
    "invocation_id": invocation_id,
    "harness": if cfg!(feature = "wasip1-node-host") {
      "node-uvwasi"
    } else if cfg!(target_env = "p2") {
      "wasmtime-component"
    } else {
      "wasmtime"
    },
    "target": {
      "arch": "wasm32",
      "os": "wasi",
      "env": if cfg!(target_env = "p2") { "p2" } else { "p1" },
    },
    "features": ["bench-internal", "thread-cpu-inline"],
    "build_mode": "default",
    "build_profile": if cfg!(debug_assertions) { "debug" } else { "optimized" },
    "source_revision": source_revision,
    "runner": runner,
    "output_isolated": true,
  }))
}

fn wall_selection() -> Value {
  let native_primitive = if cfg!(target_env = "p2") {
    "wasi:clocks/monotonic-clock.now"
  } else {
    "wasi_snapshot_preview1.clock_time_get(CLOCK_MONOTONIC)"
  };
  let instant_basis = if cfg!(target_env = "p2") {
    "WASI Preview 2 defines one native monotonic wall-clock interface"
  } else {
    "WASI Preview 1 defines one native monotonic wall clock"
  };
  json!({
    "selection_kind": "fixed_native",
    "selected_provider": {
      "instant": "wasi_clock_monotonic",
      "ordered": "wasi_clock_monotonic",
    },
    "selected_native_benchmark": {
      "instant": "direct_selected_wall__wasi_clock_monotonic",
      "ordered": "direct_selected_ordered_wall__wasi_clock_monotonic",
    },
    "eligible_direct_candidates": {
      "instant": ["direct_wall__wasi_clock_monotonic"],
      "ordered": ["direct_ordered_wall__wasi_clock_monotonic"],
    },
    "fixed_provider": {
      "instant": {
        "candidate": "wasi_clock_monotonic",
        "time_domain": "instant wall",
        "native_primitive": native_primitive,
        "selection_basis": instant_basis,
      },
      "ordered": {
        "candidate": "wasi_clock_monotonic",
        "time_domain": "ordered wall",
        "native_primitive": native_primitive,
        "selection_basis": "the WASI host call is the cross-thread ordering boundary",
      },
    },
  })
}

fn thread_cpu_selection(route: ThreadRoute) -> Value {
  match route {
    #[cfg(target_env = "p1")]
    ThreadRoute::Native => json!({
      "selection_kind": "availability_fallback",
      "selected_provider": "wasi_thread_cpu_clock",
      "selected_mechanism": "wasi_thread_cpu_clock",
      "selected_read_cost": "host call",
      "selected_native_benchmark": "direct_selected_thread_cpu__wasi_thread_cpu_clock",
      "fallback_provider": "monotonic_wall_clock",
      "fallback_mechanism": "wasi_clock_monotonic",
      "fallback_read_cost": "host call",
      "fallback_native_benchmark": null,
      "eligible_direct_candidates": ["direct_thread_cpu__wasi_thread_cpu_clock"],
      "failure_fallback": {
        "trigger": "WASI clock ID 3 returns a nonzero errno",
        "time_domain": "monotonic wall fallback",
        "reported_by_provider": true,
      },
    }),
    ThreadRoute::WallFallback => json!({
      "selection_kind": "fallback_only",
      "selected_provider": "monotonic_wall_clock",
      "selected_mechanism": "wasi_clock_monotonic",
      "selected_read_cost": "host call",
      "selected_native_benchmark": "direct_selected_thread_cpu__wasi_clock_monotonic",
      "eligible_direct_candidates": ["direct_thread_cpu__wasi_clock_monotonic"],
      "time_domain": "monotonic wall fallback",
    }),
  }
}

fn insert_wall_rows(rows: &mut Map<String, Value>, instant: Costs, ordered: Costs) {
  rows.insert(
    "direct_wall__wasi_clock_monotonic".into(),
    paired_row(
      typed_row(
        instant.now,
        instant.elapsed,
        "wasi_clock_monotonic",
        "host call",
        "instant wall",
        Some("direct_wall__wasi_clock_monotonic"),
      ),
      INSTANT_PAIR,
    ),
  );
  rows.insert(
    "direct_ordered_wall__wasi_clock_monotonic".into(),
    paired_row(
      typed_row(
        ordered.now,
        ordered.elapsed,
        "wasi_clock_monotonic",
        "host call",
        "ordered wall",
        Some("direct_ordered_wall__wasi_clock_monotonic"),
      ),
      ORDERED_PAIR,
    ),
  );
  rows.insert(
    "direct_selected_wall".into(),
    paired_row(
      typed_row(
        instant.now,
        instant.elapsed,
        "wasi_clock_monotonic",
        "host call",
        "instant wall",
        Some("direct_selected_wall__wasi_clock_monotonic"),
      ),
      INSTANT_PAIR,
    ),
  );
  rows.insert(
    "direct_selected_ordered_wall".into(),
    paired_row(
      typed_row(
        ordered.now,
        ordered.elapsed,
        "wasi_clock_monotonic",
        "host call",
        "ordered wall",
        Some("direct_selected_ordered_wall__wasi_clock_monotonic"),
      ),
      ORDERED_PAIR,
    ),
  );
}

fn median_cost<T>(mut sample: impl FnMut() -> T) -> CostSamples {
  for _ in 0..WARMUP_ITERATIONS {
    black_box(sample());
  }
  let mut samples = [0.0; SAMPLES];
  for value in &mut samples {
    *value = measure_cost(&mut sample);
  }
  CostSamples { samples }
}

fn paired_median_cost<T, U>(
  mut subject: impl FnMut() -> T,
  mut reference: impl FnMut() -> U,
) -> (CostSamples, CostSamples) {
  for iteration in 0..WARMUP_ITERATIONS {
    if iteration & 1 == 0 {
      black_box(subject());
      black_box(reference());
    } else {
      black_box(reference());
      black_box(subject());
    }
  }
  let mut subject_samples = [0.0; SAMPLES];
  let mut reference_samples = [0.0; SAMPLES];
  for index in 0..SAMPLES {
    if index & 1 == 0 {
      subject_samples[index] = measure_cost(&mut subject);
      reference_samples[index] = measure_cost(&mut reference);
    } else {
      reference_samples[index] = measure_cost(&mut reference);
      subject_samples[index] = measure_cost(&mut subject);
    }
  }
  (CostSamples { samples: subject_samples }, CostSamples { samples: reference_samples })
}

fn measure_cost<T>(sample: &mut impl FnMut() -> T) -> f64 {
  let start = host_now_nanos();
  for _ in 0..ITERATIONS {
    black_box(sample());
  }
  (host_now_nanos() - start) as f64 / ITERATIONS as f64
}

fn host_now_nanos() -> u64 {
  #[cfg(feature = "wasip1-node-host")]
  {
    // SAFETY: the source-sealed Node host returns a finite nonnegative number.
    let value = unsafe { benchmark_now_nanos() };
    assert!(value.is_finite() && value >= 0.0, "invalid host benchmark clock");
    value as u64
  }
  #[cfg(not(feature = "wasip1-node-host"))]
  {
    tach::bench::wasi_exact_wall_ticks()
  }
}

fn clock_row(now: CostSamples, elapsed: CostSamples) -> Value {
  json!({"now_samples": now.samples, "elapsed_samples": elapsed.samples})
}

fn typed_row(
  now: CostSamples,
  elapsed: CostSamples,
  provider: &str,
  read_cost: &str,
  time_domain: &str,
  benchmark: Option<&str>,
) -> Value {
  let mut row = clock_row(now, elapsed);
  let object = row.as_object_mut().expect("clock row");
  object.insert("provider".into(), json!(provider));
  object.insert("read_cost".into(), json!(read_cost));
  object.insert("time_domain".into(), json!(time_domain));
  if let Some(benchmark) = benchmark {
    object.insert("benchmark".into(), json!(benchmark));
  }
  row
}

fn paired_row(mut row: Value, pair_id: &str) -> Value {
  row
    .as_object_mut()
    .expect("clock row")
    .insert("paired_sample_id".into(), json!(pair_id));
  row
}

fn duration_nanos(value: Duration) -> u64 {
  value.as_nanos().try_into().expect("duration exceeded u64 nanoseconds")
}

fn behavior_sample(route: ThreadRoute, operation: impl FnOnce()) -> BehaviorSample {
  let wall_start = host_now_nanos();
  let public_start = ThreadCpuInstant::now();
  let direct_start = route.read();
  operation();
  let direct_end = route.read();
  let public_delta = ThreadCpuInstant::now()
    .checked_duration_since(public_start)
    .expect("WASI thread provider changed during semantic probe");
  BehaviorSample {
    wall_delta_ns: host_now_nanos().saturating_sub(wall_start),
    public_delta_ns: duration_nanos(public_delta),
    direct_delta_ns: direct_end.saturating_sub(direct_start),
  }
}

fn behavior_phase(route: ThreadRoute, operation: impl Fn() + Copy) -> Value {
  let samples = [
    behavior_sample(route, operation),
    behavior_sample(route, operation),
    behavior_sample(route, operation),
  ];
  let median = |mut values: [u64; 3]| {
    values.sort_unstable();
    values[1]
  };
  json!({
    "wall_delta_ns": median(samples.map(|sample| sample.wall_delta_ns)),
    "public_delta_ns": median(samples.map(|sample| sample.public_delta_ns)),
    "direct_delta_ns": median(samples.map(|sample| sample.direct_delta_ns)),
    "samples": samples.map(|sample| json!({
      "wall_delta_ns": sample.wall_delta_ns,
      "public_delta_ns": sample.public_delta_ns,
      "direct_delta_ns": sample.direct_delta_ns,
    })),
  })
}

fn thread_cpu_behavior(
  runtime_attestation: &Value,
  direct_benchmark: &str,
  route: ThreadRoute,
) -> Value {
  json!({
    "schema": "tach-thread-cpu-behavior-v2",
    "runtime_attestation": runtime_attestation,
    "direct_benchmark": direct_benchmark,
    "sample_count": 3,
    "busy": behavior_phase(route, || busy_work_millis(20)),
    "sleep": behavior_phase(route, || pause_millis(25)),
    "sibling_isolation": behavior_phase(route, || sibling_millis(50)),
  })
}

fn busy_work_millis(millis: u64) {
  let start = host_now_nanos();
  let duration = millis * 1_000_000;
  let mut state = 0_u64;
  while host_now_nanos().saturating_sub(start) < duration {
    state = state
      .wrapping_mul(6_364_136_223_846_793_005)
      .wrapping_add(1_442_695_040_888_963_407);
  }
  black_box(state);
}

fn pause_millis(millis: u32) {
  #[cfg(feature = "wasip1-node-host")]
  {
    // SAFETY: the source-sealed Node import blocks only the current host thread.
    unsafe { sleep_millis(millis) };
  }
  #[cfg(not(feature = "wasip1-node-host"))]
  {
    std::thread::sleep(Duration::from_millis(u64::from(millis)));
  }
}

fn sibling_millis(millis: u32) {
  #[cfg(feature = "wasip1-node-host")]
  {
    // SAFETY: the source-sealed Node import waits for an isolated worker thread.
    unsafe { sibling_work_millis(millis) };
  }
  #[cfg(not(feature = "wasip1-node-host"))]
  {
    std::thread::sleep(Duration::from_millis(u64::from(millis)));
  }
}
