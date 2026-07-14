# `EVID-TARGET-PROVIDER-CEB69C1` — Universal target-provider proof at ceb69c1 (2026-07-14)

**Status: `OBJ-PROVE-TIMERS.M0.G1` remains CLOSED 🟢 at the current shipping implementation.**

## Provenance

- Source commit: `ceb69c1a55c021947c3da13991db2ff78c33208d`
- Source tree SHA-256: `86693f631329520f86bf4f0a6c0dc0f759bf3c4f71ad28f9df028ae28b7504b0`
- Full report SHA-256: `d6846288ab1eba664f2abad5836f0221a41aac49da6c870029a6528d17695431`
- Verifier SHA-256: `28b38b17c26289852f1d9521b7e8d09beb6686ce10cba7cd1af93bd376592f6e`
- Toolchain: `rustc 1.97.0 (2d8144b78 2026-07-07)`, Apple Silicon host
- Command: `python3 benches/verify-target-providers.py --output-dir /tmp/tach-provider-ceb69c1 --jobs 8`

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-PROVE-TIMERS.M0.G1` | 🟢 24/24 targets and all 294 public clock routes close | [`target-provider-proof.txt`](target-provider-proof.txt) · implementation `ceb69c1` |

## Findings resolved here

The prior `d0fa731` proof did not include the independently selected invariant-TSC route for
same-thread `Instant` on Intel macOS. Commit `ceb69c1` adds that route, keeps `OrderedInstant` on
the XNU-fenced timeline, and reruns the complete target matrix without reducing any gate count.

## Open

This package proves API availability, feature coverage, candidate enumeration, and optimized route
closure. Relative runtime latency remains owned by the runtime-boundary gate.

## Reproduce

```sh
git checkout --detach ceb69c1a55c021947c3da13991db2ff78c33208d
python3 benches/verify-target-providers.py --install-targets
```

## Raw artifacts

- [`target-provider-proof.txt`](target-provider-proof.txt) — committed count and digest summary.
- The 514 KiB generated JSON is reproducible at the command above; it is not retained because it
  embeds temporary LLVM paths.
