#!/usr/bin/env bash
# Orchestrate Lambda invocations to build benches/skewmono-lambda-x86_64.json.
#
# Sequence:
#   1. Invoke skew-fast (deployed function, no `recal` feature) — gets 7 clocks
#      per-thread + cross-thread + skew-1s in one ~5min invocation
#   2. Invoke skew-drift × 7 in parallel — one 1m sample per clock, ~5min each
#   3. Redeploy with --features recal
#   4. Invoke skew-fast (returns ONLY tach_recal) and skew-drift clock=tach_recal
#   5. Redeploy back to default (no recal) for tidiness
#   6. Merge all responses into benches/skewmono-lambda-x86_64.json

set -euo pipefail

profile=tach
region=us-east-2
fn=tach-lambda-bench
role=arn:aws:iam::799658822840:role/tach-bench-lambda-role
work=/tmp/lambda-skewmono
mkdir -p "$work"
rm -f "$work"/*.json

invoke() {
  local payload="$1" out="$2"
  echo "  invoking: $payload  ->  $out"
  aws lambda invoke --profile "$profile" --region "$region" \
    --function-name "$fn" \
    --payload "$payload" \
    --cli-binary-format raw-in-base64-out \
    --cli-read-timeout 0 \
    "$out" >/dev/null
}

# Step 1: skew-fast (7 clocks, no recal)
echo "[$(date +%T)] step 1: skew-fast (no recal) ..."
invoke '{"mode":"skew-fast","duration":10,"samples":30}' "$work/fast.json"

# Step 2: skew-drift × 7 in parallel
echo "[$(date +%T)] step 2: skew-drift × 7 (parallel) ..."
pids=()
for clock in tach tach_fenced tach_synchronized std quanta minstant fastant; do
  invoke "{\"mode\":\"skew-drift\",\"clock\":\"$clock\",\"samples\":5}" "$work/drift-$clock.json" &
  pids+=($!)
done
for pid in "${pids[@]}"; do
  wait "$pid" || echo "  warn: pid $pid failed"
done

# Step 3: redeploy with recal feature
echo "[$(date +%T)] step 3: redeploy with --features recal ..."
(cd /tmp/tach-lambda-bench && cargo lambda build --release --features recal --output-format=zip 2>&1 | tail -2)
(cd /tmp/tach-lambda-bench && AWS_PROFILE="$profile" cargo lambda deploy \
  --region "$region" --binary-name tach-lambda-bench \
  --iam-role "$role" --memory 1024 --timeout 600 \
  --output-format json "$fn" 2>&1 | tail -8)
sleep 5  # let the new code propagate

# Step 4: tach_recal fast + drift
echo "[$(date +%T)] step 4: tach_recal fast + drift ..."
invoke '{"mode":"skew-fast","duration":10,"samples":30}' "$work/fast-recal.json"
invoke '{"mode":"skew-drift","clock":"tach_recal","samples":5}' "$work/drift-tach_recal.json"

# Step 5: redeploy back to default
echo "[$(date +%T)] step 5: redeploy back to default (no recal) ..."
(cd /tmp/tach-lambda-bench && cargo lambda build --release --output-format=zip 2>&1 | tail -2)
(cd /tmp/tach-lambda-bench && AWS_PROFILE="$profile" cargo lambda deploy \
  --region "$region" --binary-name tach-lambda-bench \
  --iam-role "$role" --memory 1024 --timeout 600 \
  --output-format json "$fn" 2>&1 | tail -8)

# Step 6: merge
echo "[$(date +%T)] step 6: merging ..."
python3 - "$work" <<'PY'
import glob, json, os, sys
work = sys.argv[1]

fast = json.load(open(f"{work}/fast.json"))
fast_recal = json.load(open(f"{work}/fast-recal.json"))

# Start from fast (7 clocks present)
out = fast
# Merge in tach_recal from the recal build's fast invocation
for k, v in fast_recal["clocks"].items():
  out["clocks"][k] = v

# Attach the 1m drift data from per-clock invocations
for drift_path in glob.glob(f"{work}/drift-*.json"):
  d = json.load(open(drift_path))
  if "errorType" in d:
    # Lambda invocation errored — usually means the deployed binary doesn't
    # know about this clock yet (binary update pending). Skip and warn.
    clock_hint = drift_path.rsplit("drift-", 1)[-1].removesuffix(".json")
    print(f"  warn: invocation for {clock_hint} errored ({d.get('errorMessage', '?')}); skipping")
    continue
  clock = d["clock"]
  if clock not in out["clocks"]:
    print(f"  warn: drift for clock {clock} but no fast data; skipping")
    continue
  out["clocks"][clock]["skew_1m"] = d["result"]

dest = "benches/skewmono-lambda-x86_64.json"
json.dump(out, open(dest, "w"), indent=2)
print(f"merged -> {dest}")
PY

echo "[$(date +%T)] DONE: benches/skewmono-lambda-x86_64.json"
