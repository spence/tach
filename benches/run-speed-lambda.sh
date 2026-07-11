#!/usr/bin/env bash
# Deploy the in-repo Lambda harness, measure every clock from one build, collect
# five robust samples, replace the complete Lambda speed cell, and delete the
# function. Lambda timeout and trap cleanup bound both spend and lifetime.
set -euo pipefail

profile=tach
region=us-east-2
role=arn:aws:iam::799658822840:role/tach-bench-lambda-role
function="tach-speed-$(date +%s)-$$"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
source_revision="$(bash "$repo_root/benches/require-clean-benchmark-source.sh")"
rustc_version="$(rustc --version)"
work="$(mktemp -d -t tach-lambda-speed.XXXXXX)"

cleanup() {
  aws lambda delete-function \
    --profile "$profile" --region "$region" \
    --function-name "$function" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

echo "building focused x86_64 Lambda benchmark"
(
  cd "$repo_root/benches/lambda-speed"
  cargo lambda build --release --output-format zip
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

for run in 1 2 3 4 5; do
  echo "invoking sample $run/5"
  aws lambda invoke \
    --profile "$profile" --region "$region" \
    --function-name "$function" \
    --payload '{}' \
    --cli-binary-format raw-in-base64-out \
    "$work/run-$run.json" >/dev/null
done

python3 - "$work/clocks.json" "$work" <<'PY'
import json
import random
import statistics
import sys
from pathlib import Path

clocks_path = Path(sys.argv[1])
run_dir = Path(sys.argv[2])
runs = [json.loads(path.read_text()) for path in sorted(run_dir.glob("run-*.json"))]


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
    "tach_thread_cpu", "native_thread_cpu",
]
if any("direct_thread_cpu" in run for run in runs):
    if not all("direct_thread_cpu" in run for run in runs):
        raise SystemExit("direct perf availability changed across Lambda samples")
    clock_keys.append("direct_thread_cpu")
clocks = {}
for key in clock_keys:
    entry = {}
    for metric in ("now", "elapsed"):
        samples = [
            sample
            for run in runs
            for sample in run[key][f"{metric}_samples"]
        ]
        point, interval = median_with_ci(samples, f"{key}:{metric}")
        entry[metric] = point
        entry[f"{metric}_ci95"] = interval
        entry[f"{metric}_samples"] = samples
    if key in ("tach_thread_cpu", "native_thread_cpu", "direct_thread_cpu"):
        providers = {run[key]["provider"] for run in runs}
        costs = {run[key]["read_cost"] for run in runs}
        domains = {run[key]["time_domain"] for run in runs}
        if len(providers) != 1 or len(costs) != 1 or len(domains) != 1:
            raise SystemExit(
                f"{key} metadata changed across Lambda samples: "
                f"{providers}, {costs}, {domains}"
            )
        entry["provider"] = providers.pop()
        entry["read_cost"] = costs.pop()
        entry["time_domain"] = domains.pop()
    if key == "tach_thread_cpu":
        selections = [run[key]["selection"] for run in runs]
        decisions = {selection["decision"] for selection in selections}
        providers = {selection["selected_provider"] for selection in selections}
        if len(decisions) != 1 or len(providers) != 1:
            raise SystemExit("thread-CPU selector decision changed across Lambda invocations")
        entry["selection"] = dict(selections[0])
        entry["selection"]["initializations"] = selections
    clocks[key] = entry

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
