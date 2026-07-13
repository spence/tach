#!/usr/bin/env bash
# Build one source-sealed host-runtime benchmark, retain five raw observations,
# and compose its supplemental speed cell.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <output-dir>/speed-supplemental-wasm-node.json" >&2
  exit 2
fi

output_input="$1"
output_name="$(basename "$output_input")"
if [ "$output_name" != speed-supplemental-wasm-node.json ]; then
  echo "host-runtime runner currently accepts only speed-supplemental-wasm-node.json" >&2
  exit 2
fi

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
invocation_id="wasm-node-${source_revision:0:12}-$(date -u +%Y%m%dT%H%M%SZ)-$$"
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
env \
  CARGO_TARGET_DIR="$target_dir" \
  TACH_BENCH_SOURCE_REVISION="$source_revision" \
  TACH_BENCH_INVOCATION_ID="$invocation_id" \
  TACH_BENCH_RUNNER="node-wasm-bindgen" \
  cargo +1.95 build --locked --release --manifest-path "$manifest" \
    --target wasm32-unknown-unknown

wasm-bindgen \
  "$target_dir/wasm32-unknown-unknown/release/tach_host_runtime_speed.wasm" \
  --target nodejs \
  --out-dir "$generated_dir"

for run in 1 2 3 4 5; do
  node - "$generated_dir/tach_host_runtime_speed.js" > "$host_dir/run-$run.json" <<'NODE'
const modulePath = process.argv[2];
const benchmark = require(modulePath);
process.stdout.write(benchmark.run() + "\n");
NODE
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
  --instant-profile runtime_tournament \
  --ordered-profile runtime_tournament \
  --thread-cpu-profile availability_fallback

echo "wrote $output with retained collector bundle $bundle_dir"
