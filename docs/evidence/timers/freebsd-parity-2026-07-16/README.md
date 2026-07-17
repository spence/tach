# `EVID-FREEBSD-PARITY` — FreeBSD-x86_64 inline parity on native AWS (2026-07-16)

**Status: FROZEN — FreeBSD-x86_64 public/exact inline parity holds within `max(1ns,5%)` on the
converted (fixed-pick) tree. Closes `OBJ-SIMPLIFY-TIMERS.M2.G1` condition 3 for the one wall family
with no GitHub-hosted CI runner.**

## Why this exists

M2.G1 requires paired public/exact reads within `max(1ns,5%)` for every converted family. The other
five families are verified in GitHub-hosted CI (Apple locally; linux-x86_64/aarch64 +
windows-x86_64/aarch64 via the native CI parity steps). FreeBSD has no hosted runner, so its parity
is measured here on a native FreeBSD AWS instance and frozen.

## Provenance

- Repo SHA: `4076fe5` (converted tree, clean).
- Hardware: AWS `c7a.large` (AMD Zen4, 2 vCPU — not metal), AMI `ami-0c6909572a0eae663` (official
  FreeBSD 15.0-RELEASE-amd64, owner 782442783595), us-east-2. Instance `i-0dc0a40fa5dc01f92`,
  launched and **terminated** (verified via `describe-instances`; end-of-run orphan check empty).
  Instance lived ~2 minutes; cost < $0.01.
- Command: `benches/run-speed-freebsd-aws.sh` path → `cargo bench --bench instant --features
  bench-internal -- 'zzz_no_match'` on the instance (writes `freebsd-wall-selection.json` via
  `write_freebsd_wall_selection`'s `measure_wall_public_exact`); validated locally with
  `python3 benches/check-inline-parity.py`.

## Result — `check-inline-parity.py` (parent re-verified, exit 0)

| Probe | exact ns | public ns | delta ns | band ns | verdict |
|---|---|---|---|---|---|
| instant / now | 9.484 | 9.486 | +0.002 | 1.000 | PASS |
| instant / elapsed | 18.948 | 18.980 | +0.033 | 1.000 | PASS |
| ordered / now | 18.408 | 18.709 | +0.301 | 1.000 | PASS |
| ordered / elapsed | 39.563 | 40.071 | +0.507 | 1.978 | PASS |

The fixed-pick public API path adds negligible overhead over the raw primitive read.

## Selected providers (the expected fixed picks — confirmed)

- `architecture`: `x86_64-freebsd`
- instant → **`freebsd_kernel_eligible_tsc`** (bare RDTSC)
- ordered → **`freebsd_kernel_eligible_tsc_x86_lfence_rdtsc`** (LFENCE + RDTSC)

FreeBSD on the Nitro `c7a` exposed an eligible invariant-TSC timecounter, so tach selected the
RDTSC / LFENCE+RDTSC picks — identical to the CI-verified linux-x86 family, confirming the
cover-by-analogy expectation.

## Reproduce

```
benches/run-speed-freebsd-aws.sh <out>   # native FreeBSD AWS run (self-terminating)
python3 benches/check-inline-parity.py <freebsd-wall-selection.json>
```

## Raw artifact

- [`freebsd-wall-selection.json`](freebsd-wall-selection.json) — the wall-selection JSON with the
  `paired_public_exact_parity` probe.
