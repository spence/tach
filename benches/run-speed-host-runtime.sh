#!/usr/bin/env bash
# Build one source-sealed host-runtime benchmark, retain five raw observations,
# and compose its supplemental speed cell.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <output-dir>/<host-runtime-speed-artifact>.json" >&2
  exit 2
fi

output_input="$1"
output_name="$(basename "$output_input")"
case "$output_name" in
  speed-supplemental-wasm-node.json)
    invocation_prefix="wasm-node"
    runner="node-wasm-bindgen"
    target="wasm32-unknown-unknown"
    runtime_kind="wasm-node"
    instant_profile="runtime_tournament"
    ordered_profile="runtime_tournament"
    thread_cpu_profile="availability_fallback"
    ;;
  speed-supplemental-emscripten-node.json)
    invocation_prefix="emscripten-node"
    runner="emcc-node"
    target="wasm32-unknown-emscripten"
    runtime_kind="emscripten-node"
    instant_profile="runtime_tournament"
    ordered_profile="runtime_tournament"
    thread_cpu_profile="availability_fallback"
    ;;
  speed-supplemental-wasi-p1-node.json)
    invocation_prefix="wasi-p1-node"
    runner="node-uvwasi"
    target="wasm32-wasip1"
    runtime_kind="wasip1-node"
    instant_profile="fixed_native"
    ordered_profile="fixed_native"
    thread_cpu_profile="availability_fallback"
    ;;
  speed-supplemental-wasi-p1-wasmtime.json)
    invocation_prefix="wasi-p1-wasmtime"
    runner="wasmtime"
    target="wasm32-wasip1"
    runtime_kind="wasip1-wasmtime"
    instant_profile="fixed_native"
    ordered_profile="fixed_native"
    thread_cpu_profile="fallback_only"
    ;;
  speed-supplemental-wasi-p2-wasmtime.json)
    invocation_prefix="wasi-p2-wasmtime"
    runner="wasmtime-component"
    target="wasm32-wasip2"
    runtime_kind="wasip2-wasmtime"
    instant_profile="fixed_native"
    ordered_profile="fixed_native"
    thread_cpu_profile="fallback_only"
    ;;
  *)
    echo "unsupported host-runtime evidence artifact: $output_name" >&2
    exit 2
    ;;
esac

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
output_dir="$(cd "$(dirname "$output_input")" && pwd)"
output="$output_dir/$output_name"
bundle_dir="$output_dir/${output_name%.json}.collector.bundle"
for destination in "$output" "$bundle_dir"; do
  if [ -e "$destination" ] || [ -L "$destination" ]; then
    echo "refusing to overwrite host-runtime evidence: $destination" >&2
    exit 1
  fi
done

source_revision="$(bash "$repo_root/benches/require-clean-benchmark-source.sh")"
invocation_id="$invocation_prefix-${source_revision:0:12}-$(date -u +%Y%m%dT%H%M%SZ)-$$"
source_dir="$(mktemp -d -t tach-host-runtime-source.XXXXXX)"
target_dir="$(mktemp -d -t tach-host-runtime-target.XXXXXX)"
host_dir="$(mktemp -d -t tach-host-runtime-observation.XXXXXX)"
generated_dir="$(mktemp -d -t tach-host-runtime-generated.XXXXXX)"
cleanup() {
  rm -rf -- "$source_dir" "$target_dir" "$host_dir" "$generated_dir"
}
trap cleanup EXIT

git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision" | \
  tar -xf - -C "$source_dir"

manifest="$source_dir/benches/host-runtime-speed/Cargo.toml"
cargo_args=(--target "$target")
case "$runtime_kind" in
  emscripten-node)
    cargo_args+=(--bin tach-host-runtime-emscripten --features emscripten-host)
    ;;
  wasip1-node)
    cargo_args+=(--bin tach-host-runtime-wasip1 --features wasip1-node-host)
    ;;
  wasip1-wasmtime)
    cargo_args+=(--bin tach-host-runtime-wasip1 --features wasip1-host)
    ;;
  wasip2-wasmtime)
    cargo_args+=(--bin tach-host-runtime-wasip2 --features wasip2-host)
    ;;
esac
env \
  CARGO_TARGET_DIR="$target_dir" \
  TACH_BENCH_SOURCE_REVISION="$source_revision" \
  TACH_BENCH_INVOCATION_ID="$invocation_id" \
  TACH_BENCH_RUNNER="$runner" \
  cargo +1.95 build --locked --release --manifest-path "$manifest" \
    "${cargo_args[@]}"

case "$runtime_kind" in
  wasm-node)
    wasm-bindgen \
      "$target_dir/$target/release/tach_host_runtime_speed.wasm" \
      --target nodejs \
      --out-dir "$generated_dir"
    runtime="$generated_dir/tach_host_runtime_speed.js"
    ;;
  emscripten-node)
    runtime="$target_dir/$target/release/tach-host-runtime-emscripten.js"
    ;;
  wasip1-node|wasip1-wasmtime)
    runtime="$target_dir/$target/release/tach-host-runtime-wasip1.wasm"
    ;;
  wasip2-wasmtime)
    runtime="$target_dir/$target/release/tach-host-runtime-wasip2.wasm"
    ;;
esac

for run in 1 2 3 4 5; do
  if [ "$runtime_kind" = wasm-node ]; then
    node - "$runtime" > "$host_dir/run-$run.json" <<'NODE'
const modulePath = process.argv[2];
const benchmark = require(modulePath);
process.stdout.write(benchmark.run() + "\n");
NODE
  elif [ "$runtime_kind" = emscripten-node ]; then
    node "$runtime" > "$host_dir/run-$run.json"
  elif [ "$runtime_kind" = wasip1-node ]; then
    node - "$runtime" > "$host_dir/run-$run.json" <<'NODE'
const { WASI } = require("node:wasi");
const fs = require("node:fs");
const { Worker } = require("node:worker_threads");
const modulePath = process.argv[2];
const wasi = new WASI({ version: "preview1" });
const tachHost = {
  benchmark_now_nanos() {
    return Number(process.hrtime.bigint());
  },
  sleep_millis(millis) {
    const signal = new Int32Array(new SharedArrayBuffer(4));
    Atomics.wait(signal, 0, 0, millis);
  },
  sibling_work_millis(millis) {
    const signal = new Int32Array(new SharedArrayBuffer(4));
    const worker = new Worker(`
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
    `, { eval: true, workerData: { signal: signal.buffer, millis } });
    while (Atomics.load(signal, 0) === 0) Atomics.wait(signal, 0, 0);
    while (Atomics.load(signal, 0) !== 2) Atomics.wait(signal, 0, 1);
    worker.unref();
  },
};
(async () => {
  const module = await WebAssembly.compile(fs.readFileSync(modulePath));
  const instance = await WebAssembly.instantiate(module, {
    ...wasi.getImportObject(),
    tach_host: tachHost,
  });
  wasi.start(instance);
})().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
NODE
  else
    wasmtime run "$runtime" > "$host_dir/run-$run.json"
  fi
done

python3 - "$host_dir" <<'PY'
import json
import os
import sys
from pathlib import Path

root = Path(sys.argv[1])
attestations = []
for run in range(1, 6):
    payload = json.loads((root / f"run-{run}.json").read_text())
    attestation = payload.get("runtime_attestation") if isinstance(payload, dict) else None
    if not isinstance(attestation, dict):
        raise SystemExit(f"host-runtime observation {run} omitted runtime attestation")
    attestations.append(attestation)
if any(value != attestations[0] for value in attestations[1:]):
    raise SystemExit("host-runtime attestation changed across observations")
destination = root / "runtime-attestation.json"
descriptor = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(descriptor, "w", encoding="utf-8") as output:
    json.dump(attestations[0], output, indent=2, sort_keys=True)
    output.write("\n")
PY

python3 "$source_dir/benches/collect-host-speed-bundle.py" "$host_dir" "$bundle_dir"
python3 "$source_dir/benches/compose-supplemental-speed.py" \
  --artifact "$output_name" \
  --output "$output" \
  --source-revision "$source_revision" \
  --collector-bundle "$bundle_dir" \
  --instant-profile "$instant_profile" \
  --ordered-profile "$ordered_profile" \
  --thread-cpu-profile "$thread_cpu_profile"

echo "wrote $output with retained collector bundle $bundle_dir"
