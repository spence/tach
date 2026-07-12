#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
manifest="$repo_root/benches/emscripten-reentry/Cargo.toml"
pre_js="$repo_root/benches/probes/emscripten-reentry-pre.js"
target_dir="$repo_root/target/emscripten-reentry"
binary="$target_dir/wasm32-unknown-emscripten/debug/tach-emscripten-reentry.js"

CARGO_TARGET_DIR="$target_dir" \
  RUSTFLAGS="-C link-arg=--pre-js -C link-arg=$pre_js -C link-arg=-sEXPORTED_FUNCTIONS=['_main','_tach_emscripten_reentry_now']" \
  cargo build --locked --manifest-path "$manifest" --target wasm32-unknown-emscripten --no-default-features

node "$binary"
printf '%s\n' 'PASS: Emscripten local-clock selection survives synchronous performance.now reentry'
