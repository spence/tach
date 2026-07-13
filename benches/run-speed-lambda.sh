#!/usr/bin/env bash
# Deploy one immutable Lambda benchmark build, retain every raw invocation,
# compose the canonical primary cell, and delete all temporary AWS resources.
#
# Usage:
#   benches/run-speed-lambda.sh <output-dir>/speed-5-lambda.json
#   benches/run-speed-lambda.sh <output-dir>/speed-supplemental-lambda-aarch64.json
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <output-dir>/{speed-5-lambda.json|speed-supplemental-lambda-aarch64.json}" >&2
  exit 2
fi

output_input="$1"
output_name="$(basename "$output_input")"
case "$output_name" in
  speed-5-lambda.json)
    architecture=x86_64
    runner=aws-lambda-x86_64
    build_arch_args=(--x86-64)
    ;;
  speed-supplemental-lambda-aarch64.json)
    architecture=aarch64
    runner=aws-lambda-aarch64
    build_arch_args=(--arm64)
    ;;
  *)
    echo "Lambda runner only composes the canonical x86_64 primary or aarch64 supplemental artifact" >&2
    exit 2
    ;;
esac

profile="${TACH_AWS_PROFILE:-tach}"
region="${TACH_AWS_REGION:-us-east-2}"
role="${TACH_LAMBDA_ROLE:-arn:aws:iam::799658822840:role/tach-bench-lambda-role}"
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
invocation_id="lambda-${source_revision:0:12}-$(date -u +%Y%m%dT%H%M%SZ)-$$"
function_default="tach-speed-${architecture}-${source_revision:0:8}-$$"
function="${TACH_LAMBDA_FUNCTION:-$function_default}"
source_dir=""
target_dir=""
host_dir=""
owns_function=0

aws_() {
  AWS_PAGER="" aws --profile "$profile" --region "$region" "$@"
}

wait_until_deleted() {
  for _ in $(seq 1 30); do
    response="$(aws_ lambda get-function --function-name "$function" 2>&1)" && status=0 || status=$?
    if [ "$status" -ne 0 ]; then
      case "$response" in
        *ResourceNotFoundException*) return 0 ;;
        *) echo "$response" >&2; return "$status" ;;
      esac
    fi
    sleep 1
  done
  echo "timed out waiting for Lambda function deletion: $function" >&2
  return 1
}

cleanup() {
  if [ "$owns_function" = 1 ]; then
    aws_ lambda delete-function --function-name "$function" >/dev/null 2>&1 || true
    wait_until_deleted >/dev/null 2>&1 || true
    aws_ logs delete-log-group --log-group-name "/aws/lambda/$function" >/dev/null 2>&1 || true
  fi
  for directory in "$source_dir" "$target_dir" "$host_dir"; do
    if [ -n "$directory" ] && [ -d "$directory" ]; then
      rm -rf -- "$directory" || true
    fi
  done
}
trap cleanup EXIT

source_dir="$(mktemp -d -t tach-speed-lambda-source.XXXXXX)"
git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision" | \
  tar -xf - -C "$source_dir"
target_dir="$(mktemp -d -t tach-speed-lambda-target.XXXXXX)"
host_dir="$(mktemp -d -t tach-speed-lambda-host.XXXXXX)"

precheck="$(aws_ lambda get-function --function-name "$function" 2>&1)" && precheck_status=0 || precheck_status=$?
if [ "$precheck_status" = 0 ]; then
  echo "refusing to replace existing Lambda function: $function" >&2
  exit 1
fi
case "$precheck" in
  *ResourceNotFoundException*) owns_function=1 ;;
  *) echo "$precheck" >&2; exit "$precheck_status" ;;
esac

echo "building source-sealed $architecture Lambda benchmark at $source_revision"
(
  cd "$source_dir/benches/lambda-speed"
  env \
    CARGO_TARGET_DIR="$target_dir" \
    TACH_BENCH_SOURCE_REVISION="$source_revision" \
    TACH_BENCH_INVOCATION_ID="$invocation_id" \
    TACH_BENCH_RUNNER="$runner" \
    cargo lambda build --locked --release --output-format zip "${build_arch_args[@]}"
)

echo "deploying $function (1024 MB, hard 60-second invocation timeout)"
(
  cd "$source_dir/benches/lambda-speed"
  env \
    AWS_PROFILE="$profile" \
    CARGO_TARGET_DIR="$target_dir" \
    TACH_BENCH_SOURCE_REVISION="$source_revision" \
    TACH_BENCH_INVOCATION_ID="$invocation_id" \
    TACH_BENCH_RUNNER="$runner" \
    cargo lambda deploy \
      --region "$region" \
      --binary-name tach-lambda-speed \
      --iam-role "$role" \
      --memory 1024 \
      --timeout 60 \
      --output-format json \
      "$function" >/dev/null
)
aws_ lambda wait function-active-v2 --function-name "$function"

for run in 1 2 3 4 5; do
  echo "invoking sample $run/5"
  aws_ lambda invoke \
    --function-name "$function" \
    --payload '{}' \
    --cli-binary-format raw-in-base64-out \
    --cli-connect-timeout 10 \
    --cli-read-timeout 90 \
    "$host_dir/run-$run.json" > "$host_dir/invoke-$run.json"
done

# The runtime identity is copied from the measured process, not synthesized by
# the runner. Every raw payload must repeat it exactly before collection.
python3 - "$host_dir" <<'PY'
import json
import os
import sys
from pathlib import Path

root = Path(sys.argv[1])
attestations = []
for run in range(1, 6):
    metadata = json.loads((root / f"invoke-{run}.json").read_text())
    if not isinstance(metadata, dict) or metadata.get("StatusCode") != 200:
        raise SystemExit(f"Lambda invocation {run} returned malformed metadata: {metadata!r}")
    if metadata.get("FunctionError") is not None:
        raise SystemExit(f"Lambda invocation {run} failed: {metadata!r}")
    payload = json.loads((root / f"run-{run}.json").read_text())
    if not isinstance(payload, dict) or not isinstance(payload.get("runtime_attestation"), dict):
        raise SystemExit(f"Lambda invocation {run} omitted runtime attestation")
    attestations.append(payload["runtime_attestation"])
if any(value != attestations[0] for value in attestations[1:]):
    raise SystemExit("Lambda runtime attestation changed across invocations")
destination = root / "runtime-attestation.json"
flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
descriptor = os.open(destination, flags, 0o600)
with os.fdopen(descriptor, "w", encoding="utf-8") as output:
    json.dump(attestations[0], output, indent=2, sort_keys=True)
    output.write("\n")
PY

python3 "$source_dir/benches/collect-host-speed-bundle.py" "$host_dir" "$bundle_dir"
if [ "$architecture" = x86_64 ]; then
  python3 "$source_dir/benches/compose-speed.py" "$output" --collector-bundle "$bundle_dir"
else
  python3 "$source_dir/benches/compose-supplemental-speed.py" \
    --artifact "$output_name" \
    --output "$output" \
    --source-revision "$source_revision" \
    --collector-bundle "$bundle_dir" \
    --instant-profile runtime_tournament \
    --ordered-profile runtime_tournament \
    --thread-cpu-profile runtime_tournament
fi
cat "$output"

echo "deleting $function"
aws_ lambda delete-function --function-name "$function" >/dev/null
wait_until_deleted
aws_ logs delete-log-group --log-group-name "/aws/lambda/$function" >/dev/null 2>&1 || true
owns_function=0
echo "wrote $output with retained collector bundle $bundle_dir"
