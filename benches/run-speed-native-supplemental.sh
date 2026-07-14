#!/usr/bin/env bash
# Compose one source-sealed supplemental cell on a native Linux or Android host.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <output-dir>/speed-supplemental-*.json <runner-id>" >&2
  exit 2
fi

output_input="$1"
runner="$2"
if [ -z "$runner" ]; then
  echo "runner id must not be empty" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
output_dir="$(cd "$(dirname "$output_input")" && pwd)"
artifact="$(basename "$output_input")"
output="$output_dir/$artifact"
bundle="$output_dir/${artifact%.json}.collector.bundle"
for destination in "$output" "$bundle"; do
  if [ -e "$destination" ] || [ -L "$destination" ]; then
    echo "refusing to overwrite native supplemental evidence: $destination" >&2
    exit 1
  fi
done

source_revision="$(bash "$repo_root/benches/require-clean-benchmark-source.sh")"
source_dir="$(mktemp -d -t tach-speed-native-compose-source.XXXXXX)"
cleanup() {
  rm -rf -- "$source_dir"
}
trap cleanup EXIT
git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision" | \
  tar -xf - -C "$source_dir"

read -r target harness evidence_mode build_mode < <(
  PYTHONPATH="$source_dir/benches" python3 - "$artifact" <<'PY'
import sys
import speed_evidence

artifact = sys.argv[1]
cell = speed_evidence.SUPPLEMENTAL_SPEED_CELLS.get(artifact)
if cell is None:
  raise SystemExit(f"unknown supplemental artifact: {artifact}")
print(*cell)
PY
)
if [ "$harness" != criterion ] || [ "$evidence_mode" != full_speed_cell ]; then
  echo "$artifact is not a native Criterion speed cell" >&2
  exit 2
fi
case "$target" in
  *-unknown-linux-*|*-linux-android) ;;
  *)
    echo "$artifact is not a Linux or Android native-host cell" >&2
    exit 2
    ;;
esac

host_triple="$(rustc -vV | sed -n 's/^host: //p')"
if [ "$host_triple" != "$target" ]; then
  echo "$artifact requires native host $target, got ${host_triple:-unknown}" >&2
  exit 2
fi

bash "$repo_root/benches/run-speed-criterion.sh" "$bundle" "$runner" "$build_mode"
test "$(bash "$repo_root/benches/require-clean-benchmark-source.sh")" = "$source_revision"
python3 "$source_dir/benches/compose-supplemental-speed.py" \
  --artifact "$artifact" \
  --output "$output" \
  --source-revision "$source_revision" \
  --collector-bundle "$bundle" \
  --instant-profile runtime_tournament \
  --ordered-profile runtime_tournament \
  --thread-cpu-profile runtime_tournament
echo "wrote $output with retained collector bundle $bundle"
