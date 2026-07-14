# `EVID-TARGET-ROUTE-CLASSIFICATION-463FAA0` — complete target-route classification (2026-07-14)

**Status: `OBJ-FASTEST-TIMERS.M2.G1` CLOSED 🟢; 32 runtime artifacts remain open for collection**

## Provenance

- Repo SHA: `463faa04cde78f4eef35129df866cfb76e7e785b`.
- Supersedes [`EVID-ROUTE-CONTRACT-6685B22`](../route-contract-2026-07-14/README.md)
  after the runtime evidence kinds and classification surface changed.
- Substrate: clean detached Git worktree on Apple Silicon macOS; cross-compilation covered every
  advertised target and feature configuration.
- Command surface: full `benches/test_*.py` suite, `verify-target-providers.py --install-targets`,
  and `runtime_route_classification.py`.
- Runtime-classification SHA-256:
  `f8948f4e39298e5b16d6cd1c521bd3c6765d05bcb5b740d727c2ce19ab493408`.
- Generated target-proof report SHA-256:
  `0fea29eca4c7763a3675a53445f1cfe1013743ea099c781234dc97b52ac789e0`.
- Proof-script SHA-256:
  `21ed1e414196dc99da2818a332363e5c1517def58090a73479e5af647574e0ad`.

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-FASTEST-TIMERS.M1.G1` | 🟢 benchmark-contract coverage remains green inside the 191-test source-bound suite | [`evidence-tests.log`](evidence-tests.log) · source `463faa0` |
| `OBJ-FASTEST-TIMERS.M2.G1` | 🟢 all 24 targets and 49 build identities pass optimized route proof; all 55 runtime identities have a ready producer and an exact evidence classification without treating codegen as speed | [`target-provider-proof.txt`](target-provider-proof.txt) · [`runtime-route-classification.json`](runtime-route-classification.json) · source `463faa0` |

## Findings resolved here

- Browser, WASI Preview 1 Wasmtime, and WASI Preview 2 identities formerly required
  `full_speed` evidence even though their `ThreadCpuInstant` contract is explicitly a tagged wall
  fallback. Commit `463faa0` gives all six build-mode identities the truthful
  `tagged_wall_fallback` requirement.
- The release contract formerly had no deterministic surface showing which of its 55 identities
  had an exact artifact definition. Commit `463faa0` adds that fail-closed classification and
  rejects artifact/evidence-kind mismatches.

## Open

- Only 23 of 55 runtime identities have an exact retained artifact definition. The remaining 32
  are explicitly `open_artifact_binding_gap` records in the classification JSON.
- An artifact definition is not collected performance evidence. `OBJ-PROVE-TIMERS` must recollect
  the 23 defined artifacts at one source revision, add justified artifacts for runnable gaps, and
  leave genuinely unavailable native hosts visibly open.
- The target proof reports external runtime-speed artifacts for 5 targets and no external runtime
  artifact for 19 targets. It proves compilation and expected optimized routing for those targets,
  not their instruction latency.
- No Graviton, Apple, Windows, Lambda, browser, Wasm, WASI, FreeBSD, Android, or rare-architecture
  result is promoted as final release evidence by this package.

## Reproduce

```bash
git worktree add --detach /tmp/tach-freeze-463faa0 463faa0
cd /tmp/tach-freeze-463faa0
python3 -m unittest discover -s benches -p 'test_*.py'
python3 benches/verify-target-providers.py --install-targets
python3 benches/runtime_route_classification.py \
  --source-revision 463faa04cde78f4eef35129df866cfb76e7e785b \
  --output /tmp/runtime-route-classification.json
```

## Raw artifacts

- [`runtime-route-classification.json`](runtime-route-classification.json) — all 55 exact
  identities, evidence kinds, producer entrypoints, declared artifacts, and explicit gaps.
- [`evidence-tests.log`](evidence-tests.log) — complete Python evidence-suite result.
- [`target-provider-proof.txt`](target-provider-proof.txt) — trimmed 24-target proof result and
  generated-report digest.
