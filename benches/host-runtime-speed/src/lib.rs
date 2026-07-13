#![cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]

use std::hint::black_box;
use std::time::Duration;

use serde_json::{Map, Value, json};
use tach::{Instant, OrderedInstant, ThreadCpuInstant, ThreadCpuProvider, ThreadCpuReadCost};
use wasm_bindgen::prelude::*;

const ITERATIONS: usize = 10_000;
const SAMPLES: usize = 31;
const WARMUP_ITERATIONS: usize = 100_000;

#[wasm_bindgen(inline_js = r#"
export function tachHostBenchmarkNowNanos() {
  return Number(globalThis.process.hrtime.bigint());
}

export function tachHostSleepMillis(millis) {
  const signal = new Int32Array(new SharedArrayBuffer(4));
  Atomics.wait(signal, 0, 0, millis);
}

export function tachHostSiblingWorkMillis(millis) {
  const process = globalThis.process;
  const workerThreads = process.getBuiltinModule("node:worker_threads");
  const signal = new Int32Array(new SharedArrayBuffer(4));
  const worker = new workerThreads.Worker(`
    const { workerData } = require("node:worker_threads");
    const signal = new Int32Array(workerData.signal);
    Atomics.store(signal, 0, 1);
    Atomics.notify(signal, 0);
    const start = process.hrtime.bigint();
    const duration = BigInt(workerData.millis) * 1000000n;
    let state = 0n;
    while (process.hrtime.bigint() - start < duration) {
      state = state * 6364136223846793005n + 1442695040888963407n;
      state &= 0xffffffffffffffffn;
    }
    Atomics.store(signal, 0, 2);
    Atomics.notify(signal, 0);
  `, {
    eval: true,
    workerData: { signal: signal.buffer, millis },
  });
  while (Atomics.load(signal, 0) === 0) {
    Atomics.wait(signal, 0, 0);
  }
  while (Atomics.load(signal, 0) !== 2) {
    Atomics.wait(signal, 0, 1);
  }
  worker.unref();
}
"#)]
unsafe extern "C" {
  #[wasm_bindgen(js_name = tachHostBenchmarkNowNanos)]
  fn host_benchmark_now_nanos() -> f64;
  #[wasm_bindgen(js_name = tachHostSleepMillis)]
  fn host_sleep_millis(millis: u32);
  #[wasm_bindgen(js_name = tachHostSiblingWorkMillis)]
  fn host_sibling_work_millis(millis: u32);
}

struct CostSamples {
  samples: [f64; SAMPLES],
}

#[derive(Clone, Copy)]
struct BehaviorSample {
  wall_delta_ns: u64,
  public_delta_ns: u64,
  direct_delta_ns: u64,
}

#[wasm_bindgen]
pub fn run() -> Result<String, JsValue> {
  run_observation()
    .and_then(|value| serde_json::to_string(&value).map_err(|error| error.to_string()))
    .map_err(|error| JsValue::from_str(&error))
}

fn run_observation() -> Result<Value, String> {
  let runtime_attestation = runtime_attestation()?;
  quanta::Instant::now();
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
  let mut rows = Map::new();

  rows.insert(
    "tach".into(),
    typed_row(
      median_cost(|| black_box(Instant::now())),
      median_cost(|| {
        let start = Instant::now();
        black_box(start.elapsed())
      }),
      selected_local,
      "host call",
      "instant wall",
      None,
    ),
  );
  rows.insert(
    "tach_ordered".into(),
    typed_row(
      median_cost(|| black_box(OrderedInstant::now())),
      median_cost(|| {
        let start = OrderedInstant::now();
        black_box(start.elapsed())
      }),
      selected_ordered,
      "host call",
      "ordered wall",
      None,
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
  insert_wall_candidates(&mut rows);
  rows.insert("direct_selected_wall".into(), selected_wall_row("instant", selected_local)?);
  rows
    .insert("direct_selected_ordered_wall".into(), selected_wall_row("ordered", selected_ordered)?);

  let public_thread_now = median_cost(|| black_box(ThreadCpuInstant::now()));
  let public_thread_elapsed = median_cost(|| {
    let start = ThreadCpuInstant::now();
    black_box(start.elapsed())
  });
  rows.insert(
    "tach_thread_cpu".into(),
    typed_row(
      public_thread_now,
      public_thread_elapsed,
      "Node thread CPU usage",
      "host call",
      "thread CPU",
      None,
    ),
  );
  let native_now = median_cost(|| black_box(exact_node_thread_cpu()));
  let native_elapsed = median_cost(|| {
    let start = exact_node_thread_cpu();
    black_box(Duration::from_nanos(exact_node_thread_cpu().saturating_sub(start)))
  });
  let native_benchmark = "native_thread_cpu__process_thread_cpu_usage";
  rows.insert(
    "native_thread_cpu".into(),
    typed_row(
      native_now,
      native_elapsed,
      "process.threadCpuUsage()",
      "host call",
      "thread CPU",
      Some(native_benchmark),
    ),
  );
  let direct_thread_benchmark = "direct_thread_cpu__node_thread_cpu_usage";
  rows.insert(
    direct_thread_benchmark.into(),
    typed_row(
      median_cost(|| black_box(exact_node_thread_cpu())),
      median_cost(|| {
        let start = exact_node_thread_cpu();
        black_box(Duration::from_nanos(exact_node_thread_cpu().saturating_sub(start)))
      }),
      "node_thread_cpu_usage",
      "host call",
      "thread CPU",
      Some(direct_thread_benchmark),
    ),
  );
  rows.insert(
    "direct_selected_thread_cpu".into(),
    typed_row(
      median_cost(|| black_box(exact_node_thread_cpu())),
      median_cost(|| {
        let start = exact_node_thread_cpu();
        black_box(Duration::from_nanos(exact_node_thread_cpu().saturating_sub(start)))
      }),
      "node_thread_cpu_usage",
      "host call",
      "thread CPU",
      Some("direct_selected_thread_cpu__node_thread_cpu_usage"),
    ),
  );

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
  Ok(json!({
    "schema": "tach-benchmark-runtime-v2",
    "invocation_id": invocation_id,
    "harness": "node-wasm-bindgen",
    "target": {"arch": "wasm32", "os": "unknown", "env": ""},
    "features": ["bench-internal", "thread-cpu-inline"],
    "build_mode": "default",
    "build_profile": if cfg!(debug_assertions) { "debug" } else { "optimized" },
    "source_revision": source_revision,
    "runner": runner,
    "output_isolated": true,
  }))
}

fn wall_selection() -> Value {
  let evidence = tach::bench::wasm_wall_selection_evidence();
  let instant_candidates = [
    (evidence.performance_median_ns > 0, "direct_wall__performance.now"),
    (evidence.hrtime_median_ns > 0, "direct_wall__process.hrtime.bigint"),
  ]
  .into_iter()
  .filter_map(|(eligible, name)| eligible.then_some(name))
  .collect::<Vec<_>>();
  let ordered_candidates = [
    (evidence.ordered_performance_median_ns > 0, "direct_ordered_wall__performance.now"),
    (evidence.ordered_hrtime_median_ns > 0, "direct_ordered_wall__process.hrtime.bigint"),
  ]
  .into_iter()
  .filter_map(|(eligible, name)| eligible.then_some(name))
  .collect::<Vec<_>>();
  json!({
    "architecture": "wasm32-host",
    "selected_provider": {
      "instant": evidence.local_provider,
      "ordered": evidence.ordered_provider,
    },
    "selected_native_benchmark": {
      "instant": format!("direct_selected_wall__{}", evidence.local_provider),
      "ordered": format!("direct_selected_ordered_wall__{}", evidence.ordered_provider),
    },
    "eligible_direct_candidates": {
      "instant": instant_candidates,
      "ordered": ordered_candidates,
    },
    "probe": {
      "reads_per_batch": evidence.reads_per_batch,
      "required_decisive_wins": evidence.required_decisive_wins,
      "instant": {
        "performance_median_ns": evidence.performance_median_ns,
        "hrtime_median_ns": evidence.hrtime_median_ns,
        "performance_batches_ns": evidence.performance_batches_ns,
        "hrtime_batches_ns": evidence.hrtime_batches_ns,
        "allowance_ns": evidence.allowance_ns,
        "hrtime_decisive_wins": evidence.hrtime_decisive_wins,
      },
      "ordered": {
        "performance_median_ns": evidence.ordered_performance_median_ns,
        "hrtime_median_ns": evidence.ordered_hrtime_median_ns,
        "performance_batches_ns": evidence.ordered_performance_batches_ns,
        "hrtime_batches_ns": evidence.ordered_hrtime_batches_ns,
        "allowance_ns": evidence.ordered_allowance_ns,
        "hrtime_decisive_wins": evidence.ordered_hrtime_decisive_wins,
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
    "fallback_mechanism": "selected_wasm_wall_fallback",
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

fn insert_wall_candidates(rows: &mut Map<String, Value>) {
  for (domain, provider) in [
    ("instant", "performance.now"),
    ("instant", "process.hrtime.bigint"),
    ("ordered", "performance.now"),
    ("ordered", "process.hrtime.bigint"),
  ] {
    let benchmark = if domain == "instant" {
      format!("direct_wall__{provider}")
    } else {
      format!("direct_ordered_wall__{provider}")
    };
    let row = exact_wall_row(domain, provider).expect("Node wall candidate");
    rows.insert(benchmark, row);
  }
}

fn selected_wall_row(domain: &str, provider: &str) -> Result<Value, String> {
  let mut row = exact_wall_row(domain, provider)?;
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

fn exact_wall_row(domain: &str, provider: &str) -> Result<Value, String> {
  let (now, elapsed) = match (domain, provider) {
    ("instant", "performance.now") => cost_pair(tach::bench::wasm_exact_performance_ticks),
    ("instant", "process.hrtime.bigint") => cost_pair(tach::bench::wasm_exact_hrtime_ticks),
    ("ordered", "performance.now") => cost_pair(tach::bench::wasm_exact_ordered_performance_ticks),
    ("ordered", "process.hrtime.bigint") => cost_pair(tach::bench::wasm_exact_ordered_hrtime_ticks),
    _ => return Err(format!("unsupported exact {domain} wall provider {provider}")),
  };
  let benchmark = if domain == "instant" {
    format!("direct_wall__{provider}")
  } else {
    format!("direct_ordered_wall__{provider}")
  };
  Ok(typed_row(now, elapsed, provider, "host call", &format!("{domain} wall"), Some(&benchmark)))
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

fn host_now_nanos() -> f64 {
  let value = host_benchmark_now_nanos();
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

fn exact_node_thread_cpu() -> u64 {
  tach::bench::wasm_exact_node_thread_cpu_nanos().expect("Node thread CPU clock disappeared")
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

#[inline(never)]
fn consume_cpu_for(millis: u32) {
  let start = host_now_nanos();
  let target = f64::from(millis) * 1_000_000.0;
  let mut state = 0_u64;
  while host_now_nanos() - start < target {
    state = state
      .wrapping_mul(6_364_136_223_846_793_005)
      .wrapping_add(1_442_695_040_888_963_407);
    black_box(state);
  }
  black_box(state);
}

fn summarize(samples: [BehaviorSample; 3]) -> Value {
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

fn thread_cpu_behavior(runtime_attestation: &Value, direct_benchmark: &str) -> Value {
  let busy = [
    behavior_sample(|| consume_cpu_for(20)),
    behavior_sample(|| consume_cpu_for(20)),
    behavior_sample(|| consume_cpu_for(20)),
  ];
  let sleep = [
    behavior_sample(|| host_sleep_millis(20)),
    behavior_sample(|| host_sleep_millis(20)),
    behavior_sample(|| host_sleep_millis(20)),
  ];
  let sibling_isolation = [
    behavior_sample(|| host_sibling_work_millis(40)),
    behavior_sample(|| host_sibling_work_millis(40)),
    behavior_sample(|| host_sibling_work_millis(40)),
  ];
  json!({
    "schema": "tach-thread-cpu-behavior-v2",
    "runtime_attestation": runtime_attestation,
    "direct_benchmark": direct_benchmark,
    "sample_count": 3,
    "busy": summarize(busy),
    "sleep": summarize(sleep),
    "sibling_isolation": summarize(sibling_isolation),
  })
}
