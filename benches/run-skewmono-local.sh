#!/usr/bin/env bash
# Run the skew+monotonicity bench on the current host. Two builds:
#   - bench-quanta (all seven clocks; no tach_recal)
#   - bench-quanta + recalibrate-background (only tach_recal — its measurements
#     are affected by the background thread; the "tach" row would be polluted
#     by the thread if measured in this build)
# Merges the two JSONs into benches/skewmono-<cell>.json.
#
# Usage: benches/run-skewmono-local.sh <cell-name> [--quick]
#   --quick reduces durations for smoke testing (cells: ~5 min total)

set -euo pipefail

cell="${1:?cell name required}"
shift || true
quick=0
for a in "$@"; do
  case "$a" in
    --quick) quick=1 ;;
  esac
done

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

if [[ "$quick" == "1" ]]; then
  args=(--mode fast --duration 2 --skew-1s-samples 5 --skew-1m-samples 0)
  echo "quick mode: per-thread+cross-thread 2s, skew-1s 5 samples, no skew-1m"
else
  args=(--mode all --duration 10 --skew-1s-samples 30 --skew-1m-samples 5)
fi

build_and_run() {
  local feature="$1"
  local out="$2"
  shift 2
  local extra=()
  if [[ $# -gt 0 ]]; then
    extra=("$@")
  fi
  echo "=== build features=$feature ==="
  # Use --message-format=json to capture the exact bench binary path cargo
  # produces for this feature set. Falls back to "newest skew-*" if jq is
  # unavailable (which would still be wrong on cache hits).
  local bin
  if command -v jq >/dev/null 2>&1; then
    bin=$(cargo build --release --bench skew --features "$feature" --message-format=json 2>/dev/null \
          | jq -r 'select(.reason=="compiler-artifact" and (.target.kind|contains(["bench"]))) | .executable' \
          | grep -v '^null$' | tail -1)
  fi
  if [[ -z "${bin:-}" || ! -x "$bin" ]]; then
    cargo build --release --bench skew --features "$feature" 2>&1 | tail -3
    bin=$(find target/release/deps -maxdepth 1 -type f -name 'skew-*' -not -name '*.d' -print0 \
          | xargs -0 ls -t | head -1)
  fi
  echo "=== run $bin --cell $cell ${args[*]} ${extra[*]+${extra[*]}} ==="
  if [[ ${#extra[@]} -gt 0 ]]; then
    "$bin" --cell "$cell" "${args[@]}" "${extra[@]}" --output "$out"
  else
    "$bin" --cell "$cell" "${args[@]}" --output "$out"
  fi
}

base_json="/tmp/skewmono-${cell}-base.json"
recal_json="/tmp/skewmono-${cell}-recal.json"
merged_json="benches/skewmono-${cell}.json"

build_and_run "bench-quanta" "$base_json"
build_and_run "bench-quanta recalibrate-background" "$recal_json" --only-clock tach_recal

python3 - "$base_json" "$recal_json" "$merged_json" <<'PY'
import json, sys
base = json.load(open(sys.argv[1]))
recal = json.load(open(sys.argv[2]))
base['clocks'].update(recal['clocks'])
with open(sys.argv[3], 'w') as f:
    json.dump(base, f, indent=2)
print(f"merged -> {sys.argv[3]}")
PY

echo "DONE: $merged_json"
