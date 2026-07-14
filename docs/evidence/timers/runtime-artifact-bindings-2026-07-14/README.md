# `EVID-RUNTIME-ARTIFACT-BINDINGS-CD598B9` — complete runtime artifact bindings (2026-07-14)

**Status: PARTIAL 🟡 — every runtime identity has a source-sealed producer and artifact contract; runtime collection remains open**

## Provenance

- Repo SHA: `cd598b9515c251e59f88f7d6aded3fc9be662a77`.
- Supersedes the artifact-readiness classification in
  [`EVID-TARGET-ROUTE-CLASSIFICATION-463FAA0`](../target-route-classification-2026-07-14/README.md):
  the earlier package closed the static target-route gate while retaining 32 explicit artifact
  binding gaps; this package resolves those gaps without claiming runtime performance evidence.
- Substrate: clean detached Git worktree on Apple Silicon macOS for the complete Python evidence
  suite; Rust 1.95 cross-target checks for the modified Wasm, WASI, Emscripten, and runtime-smoke
  harness configurations.
- Command surface: `runtime_route_classification.py`, the complete `benches/test_*.py` suite,
  both root Cargo feature modes, and the host-runtime cross-target checks documented below.
- Runtime-classification SHA-256:
  `ec28cfe92f328e123b9b53d68fb3098f788100d5da6151e3cd613a066831a87f`
  (`44,371` bytes).
- Route-manifest SHA-256:
  `90ecb7d663d99cde8939cf937d8e195b65cd12c7a6832a5d61bef1eae45351ca`.

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-FASTEST-TIMERS.M2.G1` | 🟢 the closed 55-identity classification remains exhaustive; all 55 identities now classify as `producer_ready_artifact_declared` | [`runtime-route-classification.json`](runtime-route-classification.json) · source `cd598b9` |
| `OBJ-PROVE-TIMERS.M0.G1` / `OBJ-PROVE-TIMERS.M1.G1` | 🟡 producer prerequisites are complete, but no `cd598b9` runtime artifact is promoted by this package | [`evidence-tests.log`](evidence-tests.log) · source `cd598b9` |

## Findings resolved here

- The previous classification had exact artifact definitions for 23 of 55 runtime identities.
  Commit `cd598b9` declares the remaining default/no-default, native, hosted, Wasm, WASI,
  Emscripten, negative-environment, and runtime-smoke artifacts.
- The no-default host harnesses previously emitted default-feature attestations. They now forward
  tach's default feature explicitly and serialize the feature set and build mode actually used.
- Emscripten pthread evidence now has a pinned build-std toolchain and an explicit atomics/pthread
  build handshake instead of relying on the non-atomic precompiled standard library.
- Every native Linux/Android artifact now has a source-sealed entrypoint that refuses a mismatched
  host triple, preventing cross-compiled code from being mislabeled as native performance.

## Open

- This package contains classification and producer proof, not latency measurements. It closes
  zero `OBJ-PROVE-TIMERS` gates.
- The final campaign currently has 0 of 6 canonical and 0 of 49 supplemental artifacts collected
  at `cd598b9`.
- Native host acquisition remains open for Android, LoongArch64, s390x, PowerPC64/PowerPC64LE,
  ARMv7, and the other nonlocal targets. A declared runner must not be read as proof that those
  substrates have executed it.
- The hosted workflow has not run at this SHA, and no release chart or public fastest claim is
  admitted by this package.

## Reproduce

```bash
git worktree add --detach /tmp/tach-freeze-cd598b9 cd598b9
cd /tmp/tach-freeze-cd598b9
python3 -m unittest discover -s benches -p 'test_*.py'
bash benches/require-clean-benchmark-source.sh
python3 benches/runtime_route_classification.py \
  --source-revision cd598b9515c251e59f88f7d6aded3fc9be662a77 \
  --output /tmp/runtime-route-classification.json
cargo test --locked --lib --tests
cargo test --locked --lib --tests --no-default-features
```

The feature-mode harness checks use Rust 1.95 with the compile-time evidence variables set and
cover default and no-default builds for `wasm32-unknown-unknown`, `wasm32-wasip1`,
`wasm32-wasip1-threads`, `wasm32-wasip2`, `wasm32v1-none`, and
`wasm32-unknown-emscripten`.

## Raw artifacts

- [`runtime-route-classification.json`](runtime-route-classification.json) — all 55 exact
  identities, evidence kinds, producer entrypoints, and declared artifacts at `cd598b9`.
- [`evidence-tests.log`](evidence-tests.log) — trimmed exact-worktree evidence-suite result and
  source-seal result.
