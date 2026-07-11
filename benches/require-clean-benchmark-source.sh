#!/usr/bin/env bash
# Print the exact source revision only when every file that affects the
# benchmark artifact, extraction, or claim gate matches that revision.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
paths=(
  Cargo.lock
  Cargo.toml
  src
  tests
  benches/bench_data.py
  benches/compose-speed.py
  benches/extract_speed.py
  benches/instant.rs
  benches/lambda-speed/Cargo.lock
  benches/lambda-speed/Cargo.toml
  benches/lambda-speed/src
  benches/require-clean-benchmark-source.sh
  benches/run-speed-aws.sh
  benches/run-speed-lambda.sh
  benches/run-speed-local.sh
  benches/speed_evidence.py
  benches/summary.py
  benches/summary-thread-cpu.py
  benches/summary-use-cases.py
  benches/validate-speed-evidence.py
  benches/verify-target-providers.py
)
dirty="$(git -C "$repo_root" status --porcelain=v1 --untracked-files=all -- "${paths[@]}")"
if [ -n "$dirty" ]; then
  echo "refusing to benchmark source that differs from HEAD:" >&2
  echo "$dirty" >&2
  exit 1
fi
git -C "$repo_root" rev-parse HEAD
