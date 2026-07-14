use std::hint::black_box;
use std::time::Duration;

use serde_json::json;
use tach::{Instant, OrderedInstant, ThreadCpuInstant, ThreadCpuProvider};

fn advances_instant() -> bool {
  let start = Instant::now();
  (0..1_000_000).any(|value| {
    black_box(value);
    start.elapsed().as_nanos() > 0
  })
}

fn advances_ordered() -> bool {
  let start = OrderedInstant::now();
  (0..1_000_000).any(|value| {
    black_box(value);
    start.elapsed().as_nanos() > 0
  })
}

fn advances_thread_cpu() -> bool {
  let start = ThreadCpuInstant::now();
  (0..1_000_000).any(|value| {
    black_box(value);
    start.elapsed().as_nanos() > 0
  })
}

fn main() {
  assert!(advances_instant(), "Instant remained frozen");
  assert!(advances_ordered(), "OrderedInstant remained frozen");
  assert!(advances_thread_cpu(), "ThreadCpuInstant remained frozen");

  let provider = ThreadCpuInstant::provider();
  assert!(!matches!(provider, ThreadCpuProvider::Unavailable));
  let measures_thread_cpu = ThreadCpuInstant::now().measures_thread_cpu_time();
  assert_eq!(measures_thread_cpu, provider.measures_thread_cpu_time());

  let child = std::thread::spawn(|| {
    assert!(advances_instant());
    assert!(advances_ordered());
    assert!(advances_thread_cpu());
    ThreadCpuInstant::provider()
  });
  let child_provider = child.join().expect("WASI child thread trapped");

  let sleep_start = ThreadCpuInstant::now();
  std::thread::sleep(Duration::from_millis(2));
  let sleep_advanced = sleep_start.elapsed().as_nanos() > 100_000;
  assert_eq!(sleep_advanced, !measures_thread_cpu);

  let source_revision = env!("TACH_BENCH_SOURCE_REVISION");
  let invocation_id = env!("TACH_BENCH_INVOCATION_ID");
  let runner = env!("TACH_BENCH_RUNNER");
  let features =
    if cfg!(feature = "tach-default") { vec!["thread-cpu-inline"] } else { Vec::new() };
  let observation = json!({
    "schema": "tach-runtime-smoke-attestation-v1",
    "runtime_attestation": {
      "schema": "tach-benchmark-runtime-v2",
      "invocation_id": invocation_id,
      "harness": "wasi-threads-smoke",
      "target": {"arch": "wasm32", "os": "wasi", "env": "p1"},
      "features": features,
      "build_mode": if cfg!(feature = "tach-default") { "default" } else { "no-default" },
      "build_profile": if cfg!(debug_assertions) { "debug" } else { "optimized" },
      "source_revision": source_revision,
      "runner": runner,
      "output_isolated": true,
    },
    "assertions": [
      "Instant advanced monotonically in the main thread",
      "OrderedInstant advanced monotonically in the main thread",
      "ThreadCpuInstant advanced monotonically and its domain tag matched provider introspection",
      "all three timers executed successfully in a spawned WASI thread",
      format!("main provider: {provider:?}; child provider: {child_provider:?}"),
      format!("sleep behavior matched the selected domain: advanced={sleep_advanced}"),
    ],
  });
  println!("{}", serde_json::to_string(&observation).expect("serialize runtime smoke"));
}
