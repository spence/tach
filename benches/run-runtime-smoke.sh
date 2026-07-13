#!/usr/bin/env bash
# Build and execute one source-sealed runtime-smoke producer, then compose its
# supplemental evidence cell from the runtime-emitted attestation.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <output-dir>/<runtime-smoke-artifact>.json" >&2
  exit 2
fi

output_input="$1"
output_name="$(basename "$output_input")"
case "$output_name" in
  speed-supplemental-wasip1-threads-smoke.json)
    invocation_prefix="wasi-p1-threads-smoke"
    runner="wasmtime-wasi-threads"
    target="wasm32-wasip1-threads"
    runtime_kind="wasip1-threads"
    ;;
  speed-supplemental-wasm32v1-none-smoke.json)
    invocation_prefix="wasm32v1-none-smoke"
    runner="node-wasm-bindgen-none"
    target="wasm32v1-none"
    runtime_kind="wasm32v1-none"
    ;;
  *)
    echo "unsupported runtime-smoke artifact: $output_name" >&2
    exit 2
    ;;
esac

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
output_dir="$(cd "$(dirname "$output_input")" && pwd)"
output="$output_dir/$output_name"
if [ -e "$output" ] || [ -L "$output" ]; then
  echo "refusing to overwrite runtime-smoke evidence: $output" >&2
  exit 1
fi

source_revision="$(bash "$repo_root/benches/require-clean-benchmark-source.sh")"
invocation_id="$invocation_prefix-${source_revision:0:12}-$(date -u +%Y%m%dT%H%M%SZ)-$$"
source_dir="$(mktemp -d -t tach-runtime-smoke-source.XXXXXX)"
target_dir="$(mktemp -d -t tach-runtime-smoke-target.XXXXXX)"
generated_dir="$(mktemp -d -t tach-runtime-smoke-generated.XXXXXX)"
smoke_attestation="$(mktemp -t tach-runtime-smoke-attestation.XXXXXX.json)"
cleanup() {
  rm -rf -- "$source_dir" "$target_dir" "$generated_dir" "$smoke_attestation"
}
trap cleanup EXIT

git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision" | \
  tar -xf - -C "$source_dir"

manifest="$source_dir/benches/runtime-smoke/Cargo.toml"
common_env=(
  CARGO_TARGET_DIR="$target_dir"
  TACH_BENCH_SOURCE_REVISION="$source_revision"
  TACH_BENCH_INVOCATION_ID="$invocation_id"
  TACH_BENCH_RUNNER="$runner"
)

if [ "$runtime_kind" = "wasip1-threads" ]; then
  env "${common_env[@]}" cargo +1.95 build --locked --release \
    --manifest-path "$manifest" \
    --target "$target" \
    --bin tach-runtime-smoke-wasip1-threads \
    --features wasip1-threads
  wasmtime run -W threads=y -W shared-memory=y -S threads=y \
    "$target_dir/$target/release/tach-runtime-smoke-wasip1-threads.wasm" \
    > "$smoke_attestation"
else
  env "${common_env[@]}" cargo +1.95 build --locked --release \
    --manifest-path "$manifest" \
    --target "$target" \
    --lib
  wasm-bindgen \
    "$target_dir/$target/release/tach_runtime_smoke.wasm" \
    --target nodejs \
    --out-dir "$generated_dir"
  node - "$generated_dir/tach_runtime_smoke.js" "$runner" > "$smoke_attestation" <<'NODE'
const modulePath = process.argv[2];
const runner = process.argv[3];
const smoke = require(modulePath);
const readString = (lengthName, byteName) => {
  const length = smoke[lengthName]();
  return String.fromCharCode(...Array.from(
    { length },
    (_, index) => smoke[byteName](index),
  ));
};
const mask = smoke.tach_runtime_smoke();
if (mask !== 0x0f) {
  throw new Error(`runtime smoke assertion mask was 0x${mask.toString(16)}`);
}
const providerCodes = {
  1: "NodeThreadCpuUsage",
  2: "PerformanceNow",
  3: "NodeHrtime",
  4: "MonotonicWallClock",
  5: "OtherEligibleProvider",
};
const providerCode = smoke.tach_runtime_smoke_provider();
const provider = providerCodes[providerCode];
if (provider === undefined) {
  throw new Error(`runtime selected unavailable provider code ${providerCode}`);
}
const measuresThreadCpu = smoke.tach_runtime_smoke_measures_thread_cpu();
const sourceRevision = readString(
  "tach_runtime_smoke_revision_len",
  "tach_runtime_smoke_revision_byte",
);
const invocationId = readString(
  "tach_runtime_smoke_invocation_len",
  "tach_runtime_smoke_invocation_byte",
);
process.stdout.write(JSON.stringify({
  schema: "tach-runtime-smoke-attestation-v1",
  runtime_attestation: {
    schema: "tach-benchmark-runtime-v2",
    invocation_id: invocationId,
    harness: "wasm32v1-none-smoke",
    target: { arch: "wasm32", os: "none", env: "" },
    features: ["thread-cpu-inline"],
    build_mode: "default",
    build_profile: "optimized",
    source_revision: sourceRevision,
    runner,
    output_isolated: true,
  },
  assertions: [
    "Instant advanced monotonically in the executed wasm32v1-none module",
    "OrderedInstant advanced monotonically in the executed wasm32v1-none module",
    "ThreadCpuInstant advanced monotonically in the executed wasm32v1-none module",
    `ThreadCpuInstant selected ${provider}; measures_thread_cpu_time=${measuresThreadCpu}`,
  ],
}, null, 2) + "\n");
NODE
fi

python3 "$source_dir/benches/compose-supplemental-speed.py" \
  --artifact "$output_name" \
  --output "$output" \
  --source-revision "$source_revision" \
  --smoke-attestation "$smoke_attestation"

echo "wrote $output"
