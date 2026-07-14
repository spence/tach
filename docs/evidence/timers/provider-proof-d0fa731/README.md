# `EVID-TARGET-PROVIDER-D0FA731` — frozen target-provider proof

**Status: `OBJ-PROVE-TIMERS.M0.G1` CLOSED 🟢.**

This package records the completed universal target gate for `OBJ-PROVE-TIMERS.M0`.
The full generated JSON remains a reproducible build artifact; the committed summary fixes its
source identity, exact digest, toolchain, counts, and verdict without checking in temporary LLVM
paths from the 514 KiB report.

- Source commit: `d0fa731fe666718f70fde264296af7df0f6030d6`
- Source tree SHA-256: `9873d04433e1d9a9dc0870f8752064c2bd24db0610cee4237be18d7f44cdeef8`
- Full report SHA-256: `e2233386f3d7c64fff705f75ad089ee0dd8717809ed5eadca1b85b1547a579ba`
- Verifier SHA-256: `21ed1e414196dc99da2818a332363e5c1517def58090a73479e5af647574e0ad`
- Toolchain: `rustc 1.97.0 (2d8144b78 2026-07-07)`, Apple Silicon host
- Verdict: PASS

The report is reproducible with `python3 benches/verify-target-providers.py --install-targets`
from a clean detached checkout of the source commit.

See [`target-provider-proof.txt`](target-provider-proof.txt) for the exact admitted counts and
scope boundary.
