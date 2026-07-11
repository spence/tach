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
clocks="$(mktemp -t tach-speed-clocks.XXXXXX).json"
trap 'rm -f "$clocks"' EXIT

cd "$repo_root"
cargo bench --bench instant --features bench-internal,thread-cpu-inline -- \
  --warm-up-time 1 --measurement-time 3
python3 benches/extract_speed.py target/criterion > "$clocks"
python3 benches/compose-speed.py "$clocks" "$output" \
  --title "$title" --instance "$instance" --triple "$triple" --order "$order" \
  --source-revision "$source_revision" --rustc-version "$rustc_version" \
  --harness criterion --cargo-profile bench
echo "wrote $output"
