# `EVID-ROUTE-CONTRACT-6685B22` — benchmark contract and target-route proof (2026-07-14)

**Status: `OBJ-FASTEST-TIMERS.M1.G1` CLOSED 🟢; target-route runtime proof remains partial**

## Provenance

- Repo SHA: `6685b224d0a04dfcc8ed4b959bccb8bc8a3ff875`.
- Substrate: clean detached Git worktree on Apple Silicon macOS; Rust cross-compilation covered
  every advertised target and feature configuration.
- Command surface: `test_speed_evidence.py` and `verify-target-providers.py --install-targets`.
- Target-provider report SHA-256:
  `05335800ac945659155db6040ecb37e087c67e225fd7b248756fbb2c3a30793f`.
- Proof-script SHA-256:
  `21ed1e414196dc99da2818a332363e5c1517def58090a73479e5af647574e0ad`.

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-FASTEST-TIMERS.M1.G1` | 🟢 89/89 benchmark-contract tests passed; the manifest declares 24 targets, 49 target-mode identities, 55 target-mode-runtime identities, ready producers, exact selected/public routes, and typed selection profiles | [`benchmark-contract.log`](benchmark-contract.log) · source `6685b22` |
| `OBJ-FASTEST-TIMERS.M2.G1` | 🟡 24/24 targets, 98 warning-strict API checks, 294 optimized public routes, 294 paired now/elapsed closures, and 18 vDSO routes passed; native runtime speed remains uncollected for 19 targets | [`target-provider-proof.txt`](target-provider-proof.txt) · source `6685b22` |

## Findings resolved here

- The WASI Preview 1 route proof formerly searched for an impossible concatenation of two Rust
  symbols. Commit `6cda166` matches the emitted clock-id-3 import call and retains the monotonic
  fallback proof.
- Commit `6685b22` makes all hosted Criterion producers retain source-sealed collector bundles;
  the benchmark contract no longer declares a hosted producer that cannot satisfy retained
  admission.

## Open

- Cross-compilation proves API availability and expected primitive routing, not instruction
  latency or relative performance on unbenchmarked hardware.
- The proof report records native runtime-speed evidence for 5 targets and no external runtime
  artifact for 19 targets. These identities remain work for `OBJ-FASTEST-TIMERS.M2` and
  `OBJ-PROVE-TIMERS`; this package does not promote codegen to speed evidence.
- The current retained artifact catalog does not yet directly cover all 55 target-mode-runtime
  requirements. Default/no-default counterparts and rare native targets must be measured or
  remain explicitly classified as open producer gaps.
- The Graviton and Apple runs collected before `6685b22` are diagnostic because the hosted
  producer workflow is part of the source-bound benchmark contract.

## Reproduce

```bash
git worktree add --detach /tmp/tach-freeze-6685b22 6685b22
cd /tmp/tach-freeze-6685b22
python3 -m unittest discover -s benches -p 'test_speed_evidence.py'
python3 benches/verify-target-providers.py --install-targets
shasum -a 256 target/provider-proof/report.json
```

## Raw artifacts

- [`benchmark-contract.log`](benchmark-contract.log) — exact benchmark-contract test result.
- [`target-provider-proof.txt`](target-provider-proof.txt) — trimmed target proof summary and
  immutable report digest.
