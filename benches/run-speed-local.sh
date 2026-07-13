#!/usr/bin/env bash
# Run the complete wall/thread-CPU Criterion campaign on this Apple Silicon
# machine and write one source-bound primary speed cell plus its retained bundle.
#
# Usage:
#   benches/run-speed-local.sh <output-dir>/speed-0-apple.json
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <output-dir>/speed-0-apple.json" >&2
  exit 2
fi

output_input="$1"
output_name="$(basename "$output_input")"
case "$output_name" in
  speed-0-apple.json) runner="local-macos-criterion" ;;
  *)
    echo "local Criterion runner only composes the canonical speed-0-apple.json artifact" >&2
    exit 2
    ;;
esac
host_triple="$(rustc -vV | sed -n 's/^host: //p')"
if [ "$host_triple" != "aarch64-apple-darwin" ]; then
  echo "speed-0-apple.json requires an aarch64-apple-darwin host, got ${host_triple:-unknown}" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
output_dir="$(cd "$(dirname "$output_input")" && pwd)"
output="$output_dir/$output_name"
bundle_dir="$output_dir/${output_name%.json}.collector.bundle"
if [ -e "$output" ] || [ -L "$output" ]; then
  echo "refusing to overwrite speed cell: $output" >&2
  exit 1
fi
if [ -e "$bundle_dir" ] || [ -L "$bundle_dir" ]; then
  echo "collector bundle destination already exists: $bundle_dir" >&2
  exit 1
fi

source_revision="$(bash "$repo_root/benches/require-clean-benchmark-source.sh")"
source_dir=""
target_dir=""

cleanup() {
  if [ -n "${source_dir:-}" ] && [ -d "$source_dir" ]; then
    rm -rf -- "$source_dir" || true
  fi
  if [ -n "${target_dir:-}" ] && [ -d "$target_dir" ]; then
    rm -rf -- "$target_dir" || true
  fi
}
trap cleanup EXIT

source_dir="$(mktemp -d -t tach-speed-local-source.XXXXXX)"
# Build and compose from the commit object that passed the clean-source gate,
# never from a checkout that can change while this campaign is running.
git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision" | \
  tar -xf - -C "$source_dir"
target_dir="$(mktemp -d -t tach-speed-local-target.XXXXXX)"
criterion_dir="$target_dir/criterion"

cd "$source_dir"
if [ -e "$criterion_dir" ] || [ -L "$criterion_dir" ]; then
  echo "fresh local target directory already contains Criterion output: $criterion_dir" >&2
  exit 1
fi
python3 benches/seal-speed-source.py "$criterion_dir" -- \
  env \
    CARGO_TARGET_DIR="$target_dir" \
    TACH_BENCH_EVIDENCE=1 \
    TACH_BENCH_SOURCE_REVISION="$source_revision" \
    TACH_BENCH_RUNNER="$runner" \
    cargo bench --locked --bench instant --features bench-internal,thread-cpu-inline -- \
      --warm-up-time 1 --measurement-time 3
python3 benches/collect-speed-bundle.py "$criterion_dir" "$bundle_dir"
python3 benches/compose-speed.py "$output" --collector-bundle "$bundle_dir"
echo "wrote $output with retained collector bundle $bundle_dir"
