#!/usr/bin/env bash
# Run the complete wall/thread-CPU criterion campaign on the current machine
# and write one validated speed cell.
#
# Usage:
#   benches/run-speed-local.sh <output> <order> <title> <instance> <triple>
set -euo pipefail

output="${1:?output JSON path required}"
order="${2:?cell order required}"
title="${3:?title required}"
instance="${4:?instance label required}"
triple="${5:?target triple required}"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
source_revision="$(bash "$repo_root/benches/require-clean-benchmark-source.sh")"
rustc_version="$(rustc --version)"
clocks=""
target_dir=""

cleanup() {
  if [ -n "${clocks:-}" ]; then
    rm -f -- "$clocks" || true
  fi
  if [ -n "${target_dir:-}" ] && [ -d "$target_dir" ]; then
    rm -rf -- "$target_dir" || true
  fi
}
trap cleanup EXIT

clocks="$(mktemp -t tach-speed-clocks.XXXXXX)"
target_dir="$(mktemp -d -t tach-speed-local-target.XXXXXX)"
criterion_dir="$target_dir/criterion"

cd "$repo_root"
if [ -e "$criterion_dir" ]; then
  echo "fresh local target directory already contains Criterion output: $criterion_dir" >&2
  exit 1
fi
CARGO_TARGET_DIR="$target_dir" cargo bench --bench instant --features bench-internal,thread-cpu-inline -- \
  --warm-up-time 1 --measurement-time 3
if [ ! -d "$criterion_dir" ]; then
  echo "local benchmark did not create Criterion output: $criterion_dir" >&2
  exit 1
fi
python3 benches/extract_speed.py "$criterion_dir" > "$clocks"
python3 benches/compose-speed.py "$clocks" "$output" \
  --title "$title" --instance "$instance" --triple "$triple" --order "$order" \
  --source-revision "$source_revision" --rustc-version "$rustc_version" \
  --harness criterion --cargo-profile bench
echo "wrote $output"
