#!/usr/bin/env bash
# Run one source-sealed Criterion campaign on the current native host and
# retain its collector bundle for later composition into a named speed cell.
#
# Usage:
#   benches/run-speed-criterion.sh <bundle-dir> <runner-id> [default|no-default]
set -euo pipefail

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <bundle-dir> <runner-id> [default|no-default]" >&2
  exit 2
fi

bundle_input="$1"
runner="$2"
build_mode="${3:-default}"
case "$build_mode" in
  default|no-default) ;;
  *)
    echo "build mode must be 'default' or 'no-default'" >&2
    exit 2
    ;;
esac
if [ -z "$runner" ]; then
  echo "runner id must not be empty" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
bundle_parent="$(cd "$(dirname "$bundle_input")" && pwd)"
bundle_dir="$bundle_parent/$(basename "$bundle_input")"
if [ -e "$bundle_dir" ] || [ -L "$bundle_dir" ]; then
  echo "collector bundle destination already exists: $bundle_dir" >&2
  exit 1
fi

source_revision="$(bash "$repo_root/benches/require-clean-benchmark-source.sh")"
source_dir=""
target_dir=""

cleanup() {
  if [ -n "${source_dir:-}" ] && [ -d "$source_dir" ]; then
    rm -rf -- "$source_dir" || true
  fi
  if [ -n "${target_dir:-}" ] && [ -d "$target_dir" ]; then
    rm -rf -- "$target_dir" || true
  fi
}
trap cleanup EXIT

source_dir="$(mktemp -d -t tach-speed-native-source.XXXXXX)"
git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision" | \
  tar -xf - -C "$source_dir"
target_dir="$(mktemp -d -t tach-speed-native-target.XXXXXX)"
criterion_dir="$target_dir/criterion"

cd "$source_dir"
if [ "$build_mode" = "default" ]; then
  cargo_mode=(--features bench-internal,thread-cpu-inline)
else
  cargo_mode=(--no-default-features --features bench-internal)
fi

python3 benches/seal-speed-source.py "$criterion_dir" -- \
  env \
    CARGO_TARGET_DIR="$target_dir" \
    TACH_BENCH_EVIDENCE=1 \
    TACH_BENCH_SOURCE_REVISION="$source_revision" \
    TACH_BENCH_RUNNER="$runner" \
    cargo bench --locked --bench instant "${cargo_mode[@]}" -- \
      --warm-up-time 1 --measurement-time 3
python3 benches/collect-speed-bundle.py "$criterion_dir" "$bundle_dir"
echo "wrote source-sealed $build_mode collector bundle $bundle_dir"
