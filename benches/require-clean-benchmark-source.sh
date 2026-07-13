#!/usr/bin/env bash
# Print the exact source revision only when every file that affects the
# benchmark artifact, extraction, or claim gate matches that revision.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
paths=(
  .github/workflows/bench-speed-windows.yml
  Cargo.lock
  Cargo.toml
  src
  tests
  benches/bench_data.py
  benches/collect-speed-bundle.py
  benches/compose-route-observations.py
  benches/compose-speed.py
  benches/compose-supplemental-speed.py
  benches/extract_speed.py
  benches/instant.rs
  benches/lambda-speed/Cargo.lock
  benches/lambda-speed/Cargo.toml
  benches/lambda-speed/src
  benches/release_matrix.py
  benches/route_observation.py
  benches/require-clean-benchmark-source.sh
  benches/probes/aarch64-thread-pmu.c
  benches/route-coverage.toml
  benches/run-speed-aws.sh
  benches/run-speed-freebsd-aws.sh
  benches/run-speed-lambda.sh
  benches/run-speed-local.sh
  benches/run-thread-pmu-aws.sh
  benches/seal-speed-source.py
  benches/speed_evidence.py
  benches/summary.py
  benches/summary-thread-cpu.py
  benches/summary-use-cases.py
  benches/test_bench_data.py
  benches/test_extract_speed.py
  benches/test_release_evidence_validation.py
  benches/test_release_matrix.py
  benches/test_release_matrix_wiring.py
  benches/test_route_observation.py
  benches/test_runner_wiring.py
  benches/test_speed_evidence.py
  benches/validate-speed-evidence.py
  benches/validate-release-evidence.py
  benches/validate-supplemental-thread-cpu.py
  benches/verify-target-providers.py
)
dirty="$(git -C "$repo_root" status --porcelain=v1 --untracked-files=all -- "${paths[@]}")"
if [ -n "$dirty" ]; then
  echo "refusing to benchmark source that differs from HEAD:" >&2
  echo "$dirty" >&2
  exit 1
fi
git -C "$repo_root" rev-parse HEAD
