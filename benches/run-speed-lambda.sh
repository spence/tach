#!/usr/bin/env bash
# Deploy the in-repo Lambda harness, measure every clock from one build, collect
# five robust samples, replace the complete Lambda speed cell, and delete the
# function. Lambda timeout and trap cleanup bound both spend and lifetime.
set -euo pipefail

profile=tach
region=us-east-2
role=arn:aws:iam::799658822840:role/tach-bench-lambda-role
function="${TACH_LAMBDA_FUNCTION:-tach-lambda-bench}"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
source_revision="$(bash "$repo_root/benches/require-clean-benchmark-source.sh")"
rustc_version="$(rustc --version)"
work="$(mktemp -d -t tach-lambda-speed.XXXXXX)"
owns_name=0

wait_until_deleted() {
  for _ in $(seq 1 30); do
    response="$(aws lambda get-function \
      --profile "$profile" --region "$region" \
      --function-name "$function" 2>&1)" && status=0 || status=$?
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
  if [ "$owns_name" = 1 ]; then
    aws lambda delete-function \
      --profile "$profile" --region "$region" \
      --function-name "$function" >/dev/null 2>&1 || true
    wait_until_deleted >/dev/null 2>&1 || true
    aws logs delete-log-group \
      --profile "$profile" --region "$region" \
      --log-group-name "/aws/lambda/$function" >/dev/null 2>&1 || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT

precheck="$(aws lambda get-function \
  --profile "$profile" --region "$region" \
  --function-name "$function" 2>&1)" && precheck_status=0 || precheck_status=$?
if [ "$precheck_status" = 0 ]; then
  echo "refusing to replace existing Lambda function: $function" >&2
  exit 1
fi
case "$precheck" in
  *ResourceNotFoundException*) owns_name=1 ;;
  *) echo "$precheck" >&2; exit "$precheck_status" ;;
esac

echo "building focused x86_64 Lambda benchmark"
(
  cd "$repo_root/benches/lambda-speed"
  cargo lambda build --locked --release --output-format zip
)

echo "deploying $function (1024 MB, hard 60-second invocation timeout)"
(
  cd "$repo_root/benches/lambda-speed"
  AWS_PROFILE="$profile" cargo lambda deploy \
    --region "$region" \
    --binary-name tach-lambda-speed \
    --iam-role "$role" \
    --memory 1024 \
    --timeout 60 \
    --output-format json \
    "$function" >/dev/null
)
aws lambda wait function-active-v2 \
  --profile "$profile" --region "$region" \
  --function-name "$function"

for run in 1 2 3 4 5; do
  echo "invoking sample $run/5"
  aws lambda invoke \
    --profile "$profile" --region "$region" \
    --function-name "$function" \
    --payload '{}' \
    --cli-binary-format raw-in-base64-out \
    --cli-connect-timeout 10 \
    --cli-read-timeout 90 \
    "$work/run-$run.json" > "$work/invoke-$run.json"
done

python3 - "$work/clocks.json" "$work" <<'PY'
import json
import math
import random
import statistics
import sys
from pathlib import Path

clocks_path = Path(sys.argv[1])
run_dir = Path(sys.argv[2])
expected_runs = range(1, 6)
payload_paths = [run_dir / f"run-{run}.json" for run in expected_runs]
metadata_paths = [run_dir / f"invoke-{run}.json" for run in expected_runs]
missing = [str(path) for path in [*payload_paths, *metadata_paths] if not path.is_file()]
if missing:
    raise SystemExit(f"Lambda runner omitted invocation files: {missing}")
unexpected = sorted(
    path.name
    for pattern in ("run-*.json", "invoke-*.json")
    for path in run_dir.glob(pattern)
    if path not in payload_paths and path not in metadata_paths
)
if unexpected:
    raise SystemExit(f"Lambda runner produced unexpected invocation files: {unexpected}")

runs = []
for run, payload_path, metadata_path in zip(
    expected_runs, payload_paths, metadata_paths, strict=True
):
    metadata = json.loads(metadata_path.read_text())
    if not isinstance(metadata, dict) or metadata.get("StatusCode") != 200:
        raise SystemExit(f"Lambda invocation {run} returned malformed metadata: {metadata!r}")
    if metadata.get("FunctionError") is not None:
        raise SystemExit(
            f"Lambda invocation {run} reported {metadata['FunctionError']}: "
            f"{payload_path.read_text()}"
        )
    payload = json.loads(payload_path.read_text())
    if not isinstance(payload, dict):
        raise SystemExit(f"Lambda invocation {run} returned a non-object payload")
    runs.append(payload)

wall_selections = [run.get("wall_selection") for run in runs]
if any(selection != wall_selections[0] for selection in wall_selections[1:]):
    raise SystemExit("wall selector metadata changed across Lambda samples")
wall_selection = wall_selections[0]
if not isinstance(wall_selection, dict):
    raise SystemExit("Lambda runner omitted wall selector metadata")
wall_candidates = wall_selection.get("eligible_direct_candidates")
if not isinstance(wall_candidates, dict):
    raise SystemExit("Lambda wall selector omitted eligible candidates")


def median_with_ci(samples, seed):
    point = statistics.median(samples)
    rng = random.Random(seed)
    bootstrap = sorted(
        statistics.median(rng.choices(samples, k=len(samples)))
        for _ in range(5000)
    )
    return point, [bootstrap[124], bootstrap[4874]]

clock_keys = [
    "tach", "tach_ordered", "quanta", "fastant", "minstant", "std",
    "tach_thread_cpu", "native_thread_cpu", "direct_selected_wall",
    "direct_selected_ordered_wall", "direct_selected_thread_cpu",
]
thread_selections = [run.get("thread_cpu_selection") for run in runs]
if any(selection != thread_selections[0] for selection in thread_selections[1:]):
    raise SystemExit("thread-CPU selector metadata changed across Lambda samples")
thread_selection = thread_selections[0]
if not isinstance(thread_selection, dict):
    raise SystemExit("Lambda runner omitted thread-CPU selector metadata")
thread_candidates = thread_selection.get("eligible_direct_candidates")
if not isinstance(thread_candidates, list) or not thread_candidates or not all(
    isinstance(candidate, str) and candidate for candidate in thread_candidates
):
    raise SystemExit("Lambda thread-CPU selector has malformed eligible candidates")
for domain in ("instant", "ordered"):
    candidates = wall_candidates.get(domain)
    if not isinstance(candidates, list) or not candidates or not all(
        isinstance(candidate, str) and candidate for candidate in candidates
    ):
        raise SystemExit(f"Lambda wall selector has malformed {domain} candidates")
    clock_keys.extend(candidates)
clock_keys.extend(thread_candidates)
if thread_selection.get("fallback_native_benchmark") is not None:
    clock_keys.append("direct_fallback_thread_cpu")
clock_keys = list(dict.fromkeys(clock_keys))
clocks = {}
for key in clock_keys:
    rows = [run.get(key) for run in runs]
    if not all(isinstance(row, dict) for row in rows):
        raise SystemExit(f"Lambda samples omitted clock row {key}")
    entry = {}
    for metric in ("now", "elapsed"):
        sample_lists = [row.get(f"{metric}_samples") for row in rows]
        if not all(isinstance(samples, list) and samples for samples in sample_lists):
            raise SystemExit(f"Lambda samples omitted {key}.{metric}_samples")
        samples = [sample for values in sample_lists for sample in values]
        if not all(
            type(sample) in (int, float) and math.isfinite(sample) and sample >= 0
            for sample in samples
        ):
            raise SystemExit(f"Lambda samples contained invalid {key}.{metric} values")
        point, interval = median_with_ci(samples, f"{key}:{metric}")
        entry[metric] = point
        entry[f"{metric}_ci95"] = interval
        entry[f"{metric}_samples"] = samples
    exact_wall = key.startswith(("direct_wall__", "direct_ordered_wall__"))
    exact_row = key.startswith("direct_thread_cpu__") or exact_wall or key in (
        "tach_thread_cpu", "native_thread_cpu", "direct_selected_wall",
        "direct_selected_ordered_wall", "direct_selected_thread_cpu",
        "direct_fallback_thread_cpu",
    )
    if exact_row:
        providers = {row.get("provider") for row in rows}
        costs = {row.get("read_cost") for row in rows}
        domains = {row.get("time_domain") for row in rows}
        if len(providers) != 1 or len(costs) != 1 or len(domains) != 1:
            raise SystemExit(
                f"{key} metadata changed across Lambda samples: "
                f"{providers}, {costs}, {domains}"
            )
        if not all(
            isinstance(value, str) and value
            for value in (*providers, *costs, *domains)
        ):
            raise SystemExit(f"{key} omitted provider, cost, or time-domain metadata")
        entry["provider"] = providers.pop()
        entry["read_cost"] = costs.pop()
        entry["time_domain"] = domains.pop()
        if key.startswith("direct_"):
            benchmarks = {row.get("benchmark") for row in rows}
            if len(benchmarks) != 1:
                raise SystemExit(f"{key} benchmark identity changed across Lambda samples")
            benchmark = benchmarks.pop()
            if not isinstance(benchmark, str) or not benchmark:
                raise SystemExit(f"{key} omitted its benchmark identity")
            entry["benchmark"] = benchmark
    clocks[key] = entry

clocks["tach"]["selection"] = wall_selection
clocks["tach_ordered"]["wall_selection"] = wall_selection

clocks["tach_thread_cpu"]["selection"] = thread_selection

clocks_path.write_text(json.dumps(clocks, indent=2) + "\n")
PY

python3 "$repo_root/benches/compose-speed.py" \
  "$work/clocks.json" "$repo_root/benches/speed-5-lambda.json" \
  --title "AWS Lambda" --instance "provided.al2023 1024MB" \
  --triple x86_64-unknown-linux-gnu --order 5 \
  --source-revision "$source_revision" --rustc-version "$rustc_version" \
  --harness lambda --cargo-profile release
cat "$repo_root/benches/speed-5-lambda.json"

echo "deleting $function"
aws lambda delete-function \
  --profile "$profile" --region "$region" \
  --function-name "$function" >/dev/null
wait_until_deleted
aws logs delete-log-group \
  --profile "$profile" --region "$region" \
  --log-group-name "/aws/lambda/$function" >/dev/null 2>&1 || true
owns_name=0
