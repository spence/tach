#![cfg(target_os = "emscripten")]

use std::hint::black_box;
use std::time::Duration;

use serde_json::{Map, Value, json};
use tach::{Instant, OrderedInstant, ThreadCpuInstant, ThreadCpuProvider, ThreadCpuReadCost};

const ITERATIONS: usize = 10_000;
const SAMPLES: usize = 31;
const WARMUP_ITERATIONS: usize = 100_000;

#[derive(Clone, Copy)]
struct CostSamples {
  samples: [f64; SAMPLES],
}

#[derive(Clone, Copy)]
struct WallCosts {
  now: CostSamples,
  elapsed: CostSamples,
}

#[derive(Clone, Copy)]
struct BehaviorSample {
  wall_delta_ns: u64,
  public_delta_ns: u64,
  direct_delta_ns: u64,
}

fn main() {
  let observation = run_observation().unwrap_or_else(|error| panic!("{error}"));
  println!("{}", serde_json::to_string(&observation).expect("serialize observation"));
}

fn run_observation() -> Result<Value, String> {
  let runtime_attestation = runtime_attestation()?;
  fastant::Instant::now();
  minstant::Instant::now();

  let provider = ThreadCpuInstant::provider();
  if provider != ThreadCpuProvider::NodeThreadCpuUsage {
    return Err(format!("Node host did not expose current-thread CPU time: {provider:?}"));
  }
  if ThreadCpuInstant::read_cost_hint() != ThreadCpuReadCost::HostCall {
    return Err("Node thread CPU provider changed its read-cost class".into());
  }

  let wall_selection = wall_selection();
  let selected_local = wall_selection["selected_provider"]["instant"]
    .as_str()
    .ok_or("missing selected Instant provider")?;
  let selected_ordered = wall_selection["selected_provider"]["ordered"]
    .as_str()
    .ok_or("missing selected OrderedInstant provider")?;
  let selected_local_read = exact_wall_reader(selected_local)?;
  let selected_ordered_read = exact_wall_reader(selected_ordered)?;
  let (instant_now, direct_instant_now) =
    paired_median_cost(|| black_box(Instant::now()), || black_box(selected_local_read()));
  let (instant_elapsed, direct_instant_elapsed) = paired_median_cost(
    || {
      let start = Instant::now();
      black_box(start.elapsed())
    },
    || {
      let start = selected_local_read();
      black_box(Duration::from_nanos(selected_local_read().saturating_sub(start)))
    },
  );
  let (ordered_now, direct_ordered_now) =
    paired_median_cost(|| black_box(OrderedInstant::now()), || black_box(selected_ordered_read()));
  let (ordered_elapsed, direct_ordered_elapsed) = paired_median_cost(
    || {
      let start = OrderedInstant::now();
      black_box(start.elapsed())
    },
    || {
      let start = selected_ordered_read();
      black_box(Duration::from_nanos(selected_ordered_read().saturating_sub(start)))
    },
  );
  let instant_pair_id = "emscripten-wall-instant-public-exact-v1";
  let ordered_pair_id = "emscripten-wall-ordered-public-exact-v1";
  let direct_instant = WallCosts { now: direct_instant_now, elapsed: direct_instant_elapsed };
  let direct_ordered = WallCosts { now: direct_ordered_now, elapsed: direct_ordered_elapsed };
  let mut rows = Map::new();

  rows.insert(
    "tach".into(),
    paired_row(
      typed_row(instant_now, instant_elapsed, selected_local, "host call", "instant wall", None),
      instant_pair_id,
    ),
  );
  rows.insert(
    "tach_ordered".into(),
    paired_row(
      typed_row(ordered_now, ordered_elapsed, selected_ordered, "host call", "ordered wall", None),
      ordered_pair_id,
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
  insert_wall_candidates(
    &mut rows,
    &wall_selection,
    selected_local,
    direct_instant,
    instant_pair_id,
    selected_ordered,
    direct_ordered,
    ordered_pair_id,
  )?;
  rows.insert("direct_selected_wall".into(), selected_wall_row(&rows, "instant", selected_local)?);
  rows.insert(
    "direct_selected_ordered_wall".into(),
    selected_wall_row(&rows, "ordered", selected_ordered)?,
  );

  let (public_thread_now, direct_thread_now) = paired_median_cost(
    || black_box(ThreadCpuInstant::now()),
    || black_box(exact_node_thread_cpu()),
  );
  let (public_thread_elapsed, direct_thread_elapsed) = paired_median_cost(
    || {
      let start = ThreadCpuInstant::now();
      black_box(start.elapsed())
    },
    || {
      let start = exact_node_thread_cpu();
      black_box(Duration::from_nanos(exact_node_thread_cpu().saturating_sub(start)))
    },
  );
  let thread_pair_id = "emscripten-thread-public-exact-v1";
  rows.insert(
    "tach_thread_cpu".into(),
    paired_row(
      typed_row(
        public_thread_now,
        public_thread_elapsed,
        "Node thread CPU usage",
        "host call",
        "thread CPU",
        None,
      ),
      thread_pair_id,
    ),
  );
  let native_benchmark = "native_thread_cpu__process_thread_cpu_usage";
  let direct_thread_row = paired_row(
    typed_row(
      direct_thread_now,
      direct_thread_elapsed,
      "process.threadCpuUsage()",
      "host call",
      "thread CPU",
      Some(native_benchmark),
    ),
    thread_pair_id,
  );
  rows.insert("native_thread_cpu".into(), direct_thread_row.clone());
  let direct_thread_benchmark = "direct_thread_cpu__node_thread_cpu_usage";
  let mut direct_candidate_row = direct_thread_row.clone();
  let direct_candidate = direct_candidate_row.as_object_mut().expect("thread CPU row");
  direct_candidate.insert("provider".into(), json!("node_thread_cpu_usage"));
  direct_candidate.insert("benchmark".into(), json!(direct_thread_benchmark));
  rows.insert(direct_thread_benchmark.into(), direct_candidate_row);
  let mut selected_thread_row = direct_thread_row;
  let selected_thread = selected_thread_row.as_object_mut().expect("thread CPU row");
  selected_thread.insert("provider".into(), json!("node_thread_cpu_usage"));
  selected_thread
    .insert("benchmark".into(), json!("direct_selected_thread_cpu__node_thread_cpu_usage"));
  rows.insert("direct_selected_thread_cpu".into(), selected_thread_row);

  let mut result = rows;
  result.insert("runtime_attestation".into(), runtime_attestation.clone());
  result.insert("wall_selection".into(), wall_selection);
  result.insert("thread_cpu_selection".into(), thread_cpu_selection());
  result.insert(
    "thread_cpu_behavior".into(),
    thread_cpu_behavior(&runtime_attestation, native_benchmark),
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
  let mut features = vec!["bench-internal"];
  if cfg!(feature = "emscripten-pthreads") {
    features.push("emscripten-pthreads");
  }
  if cfg!(feature = "tach-default") {
    features.push("thread-cpu-inline");
  }
  let build_mode = if cfg!(feature = "emscripten-pthreads") {
    "emscripten-pthreads"
  } else if cfg!(feature = "tach-default") {
    "default"
  } else {
    "no-default"
  };
  Ok(json!({
    "schema": "tach-benchmark-runtime-v2",
    "invocation_id": invocation_id,
    "harness": "emcc-node",
    "target": {"arch": "wasm32", "os": "emscripten", "env": ""},
    "features": features,
    "build_mode": build_mode,
    "build_profile": if cfg!(debug_assertions) { "debug" } else { "optimized" },
    "source_revision": source_revision,
    "runner": runner,
    "output_isolated": true,
  }))
}

fn wall_selection() -> Value {
  let evidence = tach::bench::emscripten_local_selection_evidence();
  let instant_candidates = [
    (evidence.performance_eligible, "direct_wall__performance.now"),
    (evidence.hrtime_eligible, "direct_wall__process.hrtime.bigint"),
  ]
  .into_iter()
  .filter_map(|(eligible, name)| eligible.then_some(name))
  .collect::<Vec<_>>();
  let ordered_candidates = instant_candidates
    .iter()
    .map(|name| name.replacen("direct_wall__", "direct_ordered_wall__", 1))
    .collect::<Vec<_>>();
  json!({
    "architecture": "emscripten-host",
    "selected_provider": {
      "instant": evidence.selected_provider,
      "ordered": evidence.selected_provider,
    },
    "selected_native_benchmark": {
      "instant": format!("direct_selected_wall__{}", evidence.selected_provider),
      "ordered": format!("direct_selected_ordered_wall__{}", evidence.selected_provider),
    },
    "eligible_direct_candidates": {
      "instant": instant_candidates,
      "ordered": ordered_candidates,
    },
    "probe": {
      "reads_per_batch": evidence.reads_per_batch,
      "required_decisive_wins": evidence.required_decisive_wins,
      "instant": {
        "performance_eligible": evidence.performance_eligible,
        "hrtime_eligible": evidence.hrtime_eligible,
        "performance_batches_ns": evidence.performance_samples_ns,
        "hrtime_batches_ns": evidence.hrtime_samples_ns,
        "allowance_ns": evidence.allowance_ns,
        "hrtime_decisive_wins": evidence.hrtime_decisive_wins,
      },
      "ordered": {
        "performance_eligible": evidence.performance_eligible,
        "hrtime_eligible": evidence.hrtime_eligible,
        "performance_batches_ns": evidence.performance_samples_ns,
        "hrtime_batches_ns": evidence.hrtime_samples_ns,
        "allowance_ns": evidence.allowance_ns,
        "hrtime_decisive_wins": evidence.hrtime_decisive_wins,
      },
    },
  })
}

fn thread_cpu_selection() -> Value {
  json!({
    "selection_kind": "availability_fallback",
    "selected_provider": "node_thread_cpu_usage",
    "selected_mechanism": "node_thread_cpu_usage",
    "selected_read_cost": "host call",
    "selected_native_benchmark": "direct_selected_thread_cpu__node_thread_cpu_usage",
    "fallback_provider": "monotonic_wall_clock",
    "fallback_mechanism": "selected_emscripten_wall_fallback",
    "fallback_read_cost": "host call",
    "fallback_native_benchmark": null,
    "eligible_direct_candidates": ["direct_thread_cpu__node_thread_cpu_usage"],
    "failure_fallback": {
      "trigger": "process.threadCpuUsage is missing, throws, or returns an invalid value",
      "time_domain": "monotonic wall fallback",
      "reported_by_provider": true,
    },
  })
}

fn insert_wall_candidates(
  rows: &mut Map<String, Value>,
  selection: &Value,
  selected_instant: &str,
  direct_instant: WallCosts,
  instant_pair_id: &str,
  selected_ordered: &str,
  direct_ordered: WallCosts,
  ordered_pair_id: &str,
) -> Result<(), String> {
  for domain in ["instant", "ordered"] {
    let candidates = selection["eligible_direct_candidates"][domain]
      .as_array()
      .ok_or("Emscripten selector omitted eligible wall candidates")?;
    for candidate in candidates {
      let benchmark = candidate.as_str().ok_or("invalid Emscripten wall candidate")?;
      let provider = benchmark
        .split_once("__")
        .map(|(_, provider)| provider)
        .ok_or("invalid Emscripten wall candidate key")?;
      let selected_costs = match domain {
        "instant" if provider == selected_instant => Some((direct_instant, instant_pair_id)),
        "ordered" if provider == selected_ordered => Some((direct_ordered, ordered_pair_id)),
        _ => None,
      };
      let row = match selected_costs {
        Some((costs, pair_id)) => {
          paired_row(exact_wall_row_from_costs(domain, provider, costs), pair_id)
        }
        None => exact_wall_row(domain, provider)?,
      };
      rows.insert(benchmark.into(), row);
    }
  }
  Ok(())
}

fn selected_wall_row(
  rows: &Map<String, Value>,
  domain: &str,
  provider: &str,
) -> Result<Value, String> {
  let candidate = if domain == "instant" {
    format!("direct_wall__{provider}")
  } else {
    format!("direct_ordered_wall__{provider}")
  };
  let mut row = rows
    .get(&candidate)
    .cloned()
    .ok_or_else(|| format!("selected Emscripten wall row is missing: {candidate}"))?;
  row.as_object_mut().expect("wall row").insert(
    "benchmark".into(),
    json!(if domain == "instant" {
      format!("direct_selected_wall__{provider}")
    } else {
      format!("direct_selected_ordered_wall__{provider}")
    }),
  );
  Ok(row)
}

fn exact_wall_reader(provider: &str) -> Result<fn() -> u64, String> {
  match provider {
    "performance.now" => Ok(tach::bench::emscripten_exact_performance_ticks),
    "process.hrtime.bigint" => Ok(tach::bench::emscripten_exact_hrtime_ticks),
    _ => Err(format!("unsupported Emscripten wall provider {provider}")),
  }
}

fn exact_wall_row(domain: &str, provider: &str) -> Result<Value, String> {
  let read = exact_wall_reader(provider)?;
  let (now, elapsed) = cost_pair(read);
  Ok(exact_wall_row_from_costs(domain, provider, WallCosts { now, elapsed }))
}

fn exact_wall_row_from_costs(domain: &str, provider: &str, costs: WallCosts) -> Value {
  let benchmark = if domain == "instant" {
    format!("direct_wall__{provider}")
  } else {
    format!("direct_ordered_wall__{provider}")
  };
  typed_row(
    costs.now,
    costs.elapsed,
    provider,
    "host call",
    &format!("{domain} wall"),
    Some(&benchmark),
  )
}

fn cost_pair(read: fn() -> u64) -> (CostSamples, CostSamples) {
  (
    median_cost(|| black_box(read())),
    median_cost(|| {
      let start = read();
      black_box(Duration::from_nanos(read().saturating_sub(start)))
    }),
  )
}

fn median_cost<T>(mut sample: impl FnMut() -> T) -> CostSamples {
  for _ in 0..WARMUP_ITERATIONS {
    black_box(sample());
  }
  let mut samples = [0.0; SAMPLES];
  for value in &mut samples {
    let start = host_now_nanos();
    for _ in 0..ITERATIONS {
      black_box(sample());
    }
    *value = (host_now_nanos() - start) / ITERATIONS as f64;
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
  (host_now_nanos() - start) / ITERATIONS as f64
}

fn host_now_nanos() -> f64 {
  let value = tach_host_emscripten_shims::benchmark_now_nanos();
  assert!(value.is_finite() && value >= 0.0, "invalid host benchmark clock");
  value
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

fn exact_node_thread_cpu() -> u64 {
  tach::bench::emscripten_exact_node_thread_cpu_nanos().expect("Node thread CPU clock disappeared")
}

fn duration_nanos(value: Duration) -> u64 {
  value.as_nanos().try_into().expect("duration exceeded u64 nanoseconds")
}

fn behavior_sample(operation: impl FnOnce()) -> BehaviorSample {
  let wall_start = host_now_nanos() as u64;
  let public_start = ThreadCpuInstant::now();
  let direct_start = exact_node_thread_cpu();
  operation();
  let direct_end = exact_node_thread_cpu();
  let public_delta = ThreadCpuInstant::now()
    .checked_duration_since(public_start)
    .expect("thread CPU provider changed during semantic probe");
  BehaviorSample {
    wall_delta_ns: (host_now_nanos() as u64).saturating_sub(wall_start),
    public_delta_ns: duration_nanos(public_delta),
    direct_delta_ns: direct_end.saturating_sub(direct_start),
  }
}

fn behavior_phase(operation: impl Fn() + Copy) -> Value {
  let samples =
    [behavior_sample(operation), behavior_sample(operation), behavior_sample(operation)];
  json!({
    "wall_delta_ns": median_u64(samples.map(|sample| sample.wall_delta_ns)),
    "public_delta_ns": median_u64(samples.map(|sample| sample.public_delta_ns)),
    "direct_delta_ns": median_u64(samples.map(|sample| sample.direct_delta_ns)),
    "samples": samples.map(|sample| json!({
      "wall_delta_ns": sample.wall_delta_ns,
      "public_delta_ns": sample.public_delta_ns,
      "direct_delta_ns": sample.direct_delta_ns,
    })),
  })
}

fn thread_cpu_behavior(runtime_attestation: &Value, direct_benchmark: &str) -> Value {
  json!({
    "schema": "tach-thread-cpu-behavior-v2",
    "runtime_attestation": runtime_attestation,
    "direct_benchmark": direct_benchmark,
    "sample_count": 3,
    "busy": behavior_phase(|| busy_work_millis(20)),
    "sleep": behavior_phase(|| {
      tach_host_emscripten_shims::sleep_millis(25)
    }),
    "sibling_isolation": behavior_phase(|| {
      tach_host_emscripten_shims::sibling_work_millis(50)
    }),
  })
}

fn busy_work_millis(millis: u64) {
  let start = host_now_nanos() as u64;
  let duration = millis * 1_000_000;
  let mut state = 0_u64;
  while (host_now_nanos() as u64).saturating_sub(start) < duration {
    state = state
      .wrapping_mul(6_364_136_223_846_793_005)
      .wrapping_add(1_442_695_040_888_963_407);
  }
  black_box(state);
}

fn median_u64(mut values: [u64; 3]) -> u64 {
  values.sort_unstable();
  values[1]
}
