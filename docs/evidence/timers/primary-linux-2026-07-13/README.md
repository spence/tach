# `EVID-LINUX-PRIMARY-SPEED` — Linux primary speed evidence (2026-07-13)

**Status: PARTIAL 🟡 — both default Linux primary cells and the 24-target codegen proof pass; the full runtime matrix remains open**

## Provenance

- Repo SHA: `136d12cda72137d946db1f235dacd812df71f5c4` (`perf: outline aarch64 ordered fallback dispatch`).
- Source tree SHA-256: `1675d5dd24fef6267b693b042754b6f978d90e0173c025ae169f05e1180dcdb5` with an empty source status in the target-provider report.
- Runtime substrates: AWS `c7g.large`, `aarch64-unknown-linux-gnu`, and AWS `c7i.large`, `x86_64-unknown-linux-gnu`; both runners selected an Amazon Linux 2023 kernel-6.12 AMI and enabled the Linux perf user-access policy before measuring.
- Command surfaces: `benches/run-speed-aws.sh`, `benches/validate-release-evidence.py`, and `benches/verify-target-providers.py`.
- Canonical cell SHA-256 / bytes: `speed-1-c7g.json` = `d0a1b63433c11c82aecd54ad835f41a278cbb183e8aec31b81484a3db4b37347` / 57,302; `speed-2-inteln.json` = `39515aa79882012c8d10de8a9280cb5f6124a94cc19332914d46b036a2c89ddd` / 185,636.
- Validation SHA-256: `validation-c7g.json` = `d853dc7c5f40140e2c39b4ef68997e997e38d9622c0e3f31c8cb4b89d5c73edf`; `validation-c7i.json` = `cde3c663b44f12939b97626a2619ffd1ef258b93c559d341ea65b4e232b8047f`.
- Target-provider report SHA-256: `bc545aeb6e54d56bd1a204f40ccbba86783c6d5adf7b1674d8071efab214d031`.

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-FASTEST-TIMERS.M1.G1` | 🟡 Both measured primary identities have source-bound public and exact `now` plus elapsed rows; the complete runtime producer set is not yet present. | [`validation-c7g.json`](validation-c7g.json) · [`validation-c7i.json`](validation-c7i.json) · source `136d12c` |
| `OBJ-FASTEST-TIMERS.M2.G1` | 🟡 Optimized public route closure passes for all 24 targets, 49 feature configurations, 294 clock routes, and 294 paired `now`/elapsed phases; hosted runtime evidence is incomplete. | [`target-provider-report.json`](target-provider-report.json) · source `136d12c` |
| `OBJ-PROVE-TIMERS.M0.G1` | 🟡 Two of the six canonical primary cells are current, individually admitted, and bound to the frozen source; four primary cells remain. | [`speed-1-c7g.json`](speed-1-c7g.json) · [`speed-2-inteln.json`](speed-2-inteln.json) |

## Measured result

All numbers are Criterion point estimates in nanoseconds per operation.

| Host | Contract | Public `now` | Public elapsed | Selected/exact `now` | Selected/exact elapsed | Result |
|---|---|---:|---:|---:|---:|---|
| Graviton 3 / `c7g.large` | `Instant` / CNTVCT | 6.673 | 13.343 | 6.672 | 13.343 | public path matches the selected route |
| Graviton 3 / `c7g.large` | `OrderedInstant` / ISB+CNTVCT | 20.643 | 40.817 | 20.255 | 40.031 | public elapsed is 1.96% above the exact route |
| Graviton 3 / `c7g.large` | `ThreadCpuInstant` / perf mmap | 56.657 | 113.553 | 57.304 | 114.886 | materially faster than the 260.045 / 521.673 ns native syscall baseline |
| Intel / `c7i.large` | `Instant` / TSC | 14.575 | 31.003 | 14.714 | 29.859 | public path is the fastest compared wall-clock API |
| Intel / `c7i.large` | `OrderedInstant` / LFENCE+RDTSC | 22.163 | 46.309 | 22.854 | 44.141 | public path is the selected ordered route |
| Intel / `c7i.large` | `ThreadCpuInstant` / POSIX thread clock | 173.563 | 350.941 | 174.460 | 350.901 | within the selector's material-tie threshold, but not the absolute minimum; see Open |

Both isolated validation reports record `checkout_binding.passed=true`, one admitted cell with
`passed=true` and `bound_observation=true`, and passing route-observation and manifest bindings.
Their top-level `passed=false` is expected because an isolated one-cell directory cannot satisfy
the complete release campaign.

## Findings resolved here

- `8dc109f` made AArch64 boottime evidence serializable instead of silently omitting the route.
- `5034280` retained the measured thread-CPU route instead of replacing it with a capability-bit assumption.
- `20e66b6` through `136d12c` removed public dispatch and scale overhead while keeping fallback paths outlined; the final source is what both cells measured.
- The Graviton result demonstrates that runtime profitability selection matters: the perf-mmap thread clock is roughly 4.6x faster than the syscall baseline on this host.

## Open

- This package does **not** close any gate. Each isolated release validation has 58 failures, including 54 missing route-matrix admissions; those are absent campaign cells, not failures of the retained Linux cell.
- Four canonical primary artifacts remain: macOS AArch64, Linux musl x86_64, Windows x86_64, and Lambda. The release campaign also requires its supplemental native, Wasm/WASI, and negative-environment records.
- Hosted runtime coverage is still missing for 11 advertised target families: Windows AArch64 and i686, Linux i686, Emscripten, browser/Node Wasm, WASI p1, WASI p1 threads, WASI p2, `wasm32v1-none`, macOS x86_64, and FreeBSD x86_64.
- The Intel `ThreadCpuInstant` public read is 3.62% slower than the separately measured native syscall row (173.563 vs. 167.500 ns), while the exact raw-syscall candidate is 4.26% faster than public (166.464 vs. 173.563 ns). The selector deliberately requires a greater-than-5%-and-1-ns win, so this is a material tie under the current benchmark contract, not proof of the absolute-minimum read.
- The Graviton ordered public elapsed path is 1.96% slower than its selected exact row. It passes the material-tie contract but remains visible until the final release verdict.
- Default-mode evidence here does not substitute for the required no-default and alternate-runtime identities.

## Reproduce

```sh
git checkout 136d12cda72137d946db1f235dacd812df71f5c4
benches/run-speed-aws.sh c7g c7g.large
benches/run-speed-aws.sh inteln c7i.large

python3 benches/verify-target-providers.py \
  --output-dir /tmp/tach-target-provider-proof --install-targets

python3 benches/validate-release-evidence.py \
  --data-dir /tmp/tach-speed-c7g.RESULT \
  --output /tmp/tach-speed-c7g.RESULT/validation.json
python3 benches/validate-release-evidence.py \
  --data-dir /tmp/tach-speed-inteln.RESULT \
  --output /tmp/tach-speed-inteln.RESULT/validation.json
```

`run-speed-aws.sh` prints the fresh `RESULT` suffix. Replace only that literal suffix in the two
validation commands; the runner refuses a dirty or replacement-ref source and terminates its EC2
instance on exit.

## Raw artifacts

- [`speed-1-c7g.json`](speed-1-c7g.json) — canonical source-sealed Graviton 3 primary cell.
- [`speed-2-inteln.json`](speed-2-inteln.json) — canonical source-sealed Intel primary cell.
- [`route-observations-c7g.json`](route-observations-c7g.json) — Graviton route identity observed from the shipped source.
- [`route-observations-c7i.json`](route-observations-c7i.json) — Intel route identity observed from the shipped source.
- [`validation-c7g.json`](validation-c7g.json) — clean-checkout isolated release admission for the Graviton cell, including honest missing-cell failures.
- [`validation-c7i.json`](validation-c7i.json) — clean-checkout isolated release admission for the Intel cell, including honest missing-cell failures.
- [`target-provider-report.json`](target-provider-report.json) — clean-source 24-target optimized route and API proof report.
