# `EVID-AMD-FLIP-LINUX-X86-2026-07-15` — AMD c7a vs Intel c7i: no W/O-LINUX-X86 selection flip (2026-07-15)

**Status: FREEZE — no same-target flip; `W/O-LINUX-X86` freezes fixed.**

The `OBJ-SIMPLIFY-TIMERS` §5.2 same-target flip probe for `x86_64-unknown-linux-gnu`: does the
production `Instant`/`OrderedInstant` selection differ between the frozen Intel environment (c7i) and
a second AMD environment (c7a)? It does not. Both microarchitectures select the identical winners, so
the family has no frozen two-environment flip and freezes to a compile-time pick in M2.

## Provenance

- Repo SHA: `a83875421932c9422036dab27382e2be00bb0313` (the c7a run's sealed source revision). The
  x86_64-gnu measured path is byte-identical to the frozen c7i cell's revision `c64dcb73` — every
  change since is Apple-cfg-gated (`src/arch/apple_aarch64.rs`, `mod.rs`, `bench.rs`), so this is a
  valid same-target comparison across microarchitectures rather than across source.
- Substrate: AWS `c7a.large` (AMD Zen4), `x86_64-unknown-linux-gnu`, AL2023 kernel 6.12, runner
  `aws-c7a`. Self-terminated (`i-037b374adb6dcc442` reached `terminated`; no orphan).
- Comparison baseline (env #1): frozen `benches/speed-2-inteln.json` — AWS `c7i.large` (Intel),
  runner `aws-inteln`, revision `c64dcb73`.
- Command surface: `benches/run-speed-aws.sh c7a c7a.large` (sanctioned flip-probe path, commit
  `0777bf0`) → sealed Criterion `instant` bench → retained collector bundle → local selection extract.
- Artifacts: 580 KB retained (attestation + raw log + extracted comparison); the full 29 MB Criterion
  tree is not committed (per plan §7.1).

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-SIMPLIFY-TIMERS.M1.G1` (row `W/O-LINUX-X86`) | 🟢 no flip — c7a selects the identical winners as c7i | [`selection-comparison.json`](artifacts/selection-comparison.json) · [`tach-speed-collector.json`](artifacts/tach-speed-collector.json) |

Selected providers, both environments:

| Contract | c7i (Intel, frozen) | c7a (AMD Zen4) | Flip |
|---|---|---|---|
| `Instant` | `linux_kernel_eligible_tsc` | `linux_kernel_eligible_tsc` | no |
| `OrderedInstant` | `linux_kernel_eligible_tsc_x86_lfence_rdtsc` | `linux_kernel_eligible_tsc_x86_lfence_rdtsc` | no |

## Findings resolved here

Row 1 of the §5.2 freeze table (`W/O-LINUX-X86`) is verdicted: the enumerated "different ordered
winner → first real flip" branch does not fire. Intel and AMD both select the kernel-eligible
invariant TSC for `Instant` and the LFENCE+RDTSC ordered form for `OrderedInstant`. Under ADR-0005's
selection policy (a production measured tournament survives only with a frozen same-target
two-environment flip among production candidates), this family carries no such flip and converts to a
compile-time `cfg` pick plus capability gate in M2.

## Open

- The other three flip-probe rows (c8g aarch64, FreeBSD c7a, c5n.metal thread-CPU) reuse this
  sanctioned flip-probe path and remain to run.
- `ESC-AMD-FLIP-PROBE-TOOLING` stays open for owner ratification of the flip-probe mechanism this run
  exercised; the change is reversible.

## Reproduce

```
benches/run-speed-aws.sh c7a c7a.large
# then, against the printed collector-bundle path:
python3 -c "import sys; sys.path.insert(0,'benches'); import extract_speed; from pathlib import Path; \
print(extract_speed.extract_collector_bundle_observation(Path('<bundle>'))['clocks']['tach']['selection']['selected_provider'])"
```

## Raw artifacts

- [`artifacts/selection-comparison.json`](artifacts/selection-comparison.json) — the extracted c7a
  selection, the frozen c7i baseline, the per-contract flip booleans, and provenance.
- [`artifacts/tach-speed-collector.json`](artifacts/tach-speed-collector.json) — the Rust-emitted
  collector attestation: source revision `a838754`, runner `aws-c7a`, target x86_64-linux-gnu.
- [`artifacts/raw-run-c7a.log`](artifacts/raw-run-c7a.log) — full raw output of the provisioning,
  sealed test gate, Criterion bench run, and self-termination.
