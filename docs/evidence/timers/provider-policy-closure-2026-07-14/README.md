# `EVID-PROVIDER-POLICY-2026-07-14` — Provider policy closure (2026-07-14)

**Status: PASSING EVIDENCE 🟢 for `OBJ-PROVE-TIMERS.M0.G2` admission**

## Provenance

- Shipping implementation: `d9da626` (no `Cargo.toml`, `Cargo.lock`, or `src/` change through
  `5bad0dd`).
- Static substrate: Rust 1.97 target compilation and optimized LLVM inspection across all 24
  advertised targets and 49 supported feature configurations.
- Native closure substrate: AWS `c7i.large`, FreeBSD 15.0-RELEASE amd64, source-sealed benchmark
  revision `8968b16`.
- Linux AArch64 policy survey: one frozen binary on AWS c6g, c7g, c8g, and t4g; see
  [`../../../investigations/aarch64-thread-cpu-runtime-selection.md`](../../../investigations/aarch64-thread-cpu-runtime-selection.md).
- FreeBSD cell SHA-256: `4eb96c4dbbcf68eec39f7e8d2056f89591c26c243e8a006a020b17e70fdc78ba`.
- FreeBSD collector manifest SHA-256:
  `f92ae48d40da86df8945b15df04c435451bd6b562e1cb78fdb89dd4ff7f87248`.
- Compressed retained bundle SHA-256:
  `e98760c0d5d769034353896f5eef2354d3dc4a8d80e898d8d1115dbcc6a19168`.
- Static report SHA-256: `61e68ba234c3b454926a468989d93d5b2ea9f5b093d6b3b7874d7fe3d2efb8a6`.

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-PROVE-TIMERS.M0.G2` | 🟢 72/72 target/timer policy cells classified; zero unknown policies; FreeBSD native closure passes | [`target-provider-report-d9da626.json`](target-provider-report-d9da626.json) · [`freebsd-validation-8968b16.json`](freebsd-validation-8968b16.json) · fixes `d9da626`, `8968b16` |

The 72 policy cells comprise 53 runtime-measured routes, nine fixed-contract routes, eight
availability-fallback routes, one explicit fallback-only route, and one availability-preferred
route with a retained profitability audit. Every runtime-measured family enumerates and measures
its complete eligible installed paths; fixed and fallback families are tied to their platform
contract.

## Findings resolved here

- Linux AArch64 `ThreadCpuInstant` remains availability-preferred. Perf mmap won materially by
  3–4.5× on c6g/c7g/c8g/t4g with no same-target reversal; the benchmark audit reopens the policy if
  a future capable host loses. Linux x86 retains measured runtime selection because an actual
  capability/profitability reversal exists there.
- FreeBSD now installs the exact selected TSC scale and ordered barrier identity in one packed hot
  state. The native selector chose kernel-eligible TSC for `Instant`, LFENCE+TSC for
  `OrderedInstant`, and the raw thread-CPU syscall for `ThreadCpuInstant`.
- FreeBSD public `Instant` measured 13.83 ns (`now`) and 28.74 ns (`now + elapsed`), beating quanta,
  fastant, minstant, and `std`. Public `OrderedInstant` measured 22.37/42.92 ns, beating `std` at
  31.41/65.36 ns. `ThreadCpuInstant` measured 125.20/249.71 ns versus the native wrapper at
  130.54/263.49 ns.
- The statically bound selected FreeBSD wall functions remain visible as optimization lower bounds.
  They are not alternative public clocks. Admission therefore requires the runtime selector to
  reproduce and the public API to beat all usable public references; it does not mislabel private
  static functions as caller-selectable competitors.

## Open

- This package closes provider discovery and production selection policy, not release
  corroboration. `OBJ-PROVE-TIMERS.M1` still needs native Intel macOS and Windows x86_64 artifacts.
- The retained Rosetta x86_64 macOS bundle is compatibility evidence only and is not admitted as
  native Intel performance evidence.

## Reproduce

```text
python3 benches/verify-target-providers.py --install-targets

mkdir -p /tmp/tach-freebsd-evidence
tar -xzf docs/evidence/timers/provider-policy-closure-2026-07-14/freebsd-collector-bundle-8968b16.tgz \
  -C /tmp/tach-freebsd-evidence
cp docs/evidence/timers/provider-policy-closure-2026-07-14/freebsd-speed-cell-8968b16.json \
  /tmp/tach-freebsd-evidence/speed-supplemental-freebsd-x86_64.json
python3 benches/validate-supplemental-thread-cpu.py \
  --artifact speed-supplemental-freebsd-x86_64.json \
  --cell /tmp/tach-freebsd-evidence/speed-supplemental-freebsd-x86_64.json \
  --output /tmp/tach-freebsd-validation.json
```

## Raw artifacts

- [`target-provider-report-d9da626.json`](target-provider-report-d9da626.json) — complete optimized
  24-target/72-policy-cell report.
- [`freebsd-speed-cell-8968b16.json`](freebsd-speed-cell-8968b16.json) — composed native FreeBSD
  speed and selector cell.
- [`freebsd-collector-manifest-8968b16.json`](freebsd-collector-manifest-8968b16.json) — manifest
  binding every retained raw benchmark input.
- [`freebsd-collector-bundle-8968b16.tgz`](freebsd-collector-bundle-8968b16.tgz) — compressed complete
  collector bundle.
- [`freebsd-validation-8968b16.json`](freebsd-validation-8968b16.json) — passing replay report.
