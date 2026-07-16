# `EVID-AMD-FLIP-FREEBSD-X86` — c7a vs c7i FreeBSD: no W/O-FREEBSD-X86 selection flip (2026-07-15)

**Status: FREEZE — no same-target flip; `W/O-FREEBSD-X86` freezes fixed.**

The `OBJ-SIMPLIFY-TIMERS` §5.2 same-target flip probe for `x86_64-unknown-freebsd`: does the
production `Instant`/`OrderedInstant` selection differ between the frozen Intel FreeBSD environment
(c7i) and a second AMD FreeBSD environment (c7a)? It does not — both select the identical winners, so
the family has no frozen two-environment flip and freezes to a compile-time pick in M2.

## Provenance

- Repo SHA: `11b4cf25d8762e62cafe00bececd8b2610445e41` (the c7a run's sealed source revision; adds only
  the FreeBSD runner's SSH keepalive vs the prior tip — orchestration, not measured code). The
  FreeBSD-x86 measured path is unchanged since the frozen c7i revision `c64dcb73`, so this is a valid
  same-target comparison across microarchitectures.
- Substrate: AWS `c7a.large` (AMD Zen4), `x86_64-unknown-freebsd`, FreeBSD 15.0-RELEASE. Runner tag is
  the generic `aws-freebsd-default` (a pre-existing convention of `run-speed-freebsd-aws.sh`, which
  does not encode the instance); the true instances are recorded here and in
  `selection-comparison.json`. Instance self-terminated (no orphan; AWS confirms no live FreeBSD
  bench instance).
- Comparison baseline (env #1): frozen `benches/speed-supplemental-freebsd-x86_64.json` — `c7i.large`
  (Intel) FreeBSD, runner `aws-freebsd-default`, revision `c64dcb73`.
- Command surface: `benches/run-speed-freebsd-aws.sh c7a.large` → sealed Criterion `instant` bench →
  retained collector bundle → local selection extract. (First attempt dropped on an SSH broken pipe;
  fixed by adding keepalive in `11b4cf2` and re-run.)
- Artifacts: 268 KB retained; the full Criterion tree is not committed (per plan §7.1).

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-SIMPLIFY-TIMERS.M1.G1` (row `W/O-FREEBSD-X86`) | 🟢 no flip — c7a selects the identical winners as c7i | [`selection-comparison.json`](artifacts/selection-comparison.json) · [`tach-speed-collector.json`](artifacts/tach-speed-collector.json) |

| Contract | c7i (Intel, frozen) | c7a (AMD Zen4) | Flip |
|---|---|---|---|
| `Instant` | `freebsd_kernel_eligible_tsc` | `freebsd_kernel_eligible_tsc` | no |
| `OrderedInstant` | `freebsd_kernel_eligible_tsc_x86_lfence_rdtsc` | `freebsd_kernel_eligible_tsc_x86_lfence_rdtsc` | no |

## Findings resolved here

Row 5 of the §5.2 freeze table (`W/O-FREEBSD-X86`) is verdicted: no flip. Both microarchitectures
select the kernel-eligible TSC for `Instant` and the LFENCE+TSC ordered form for `OrderedInstant`.
Under ADR-0005's selection policy the family carries no frozen same-target flip and converts to a
compile-time `cfg` pick plus capability gate in M2.

## Open

- Only the c5n.metal thread-CPU flip row (needs a new x86 thread-pmu probe) and the windows-2022 row
  (push authorization) remain of the §5.2 fleet; `ESC-AMD-FLIP-PROBE-TOOLING` stays open for owner
  ratification.

## Reproduce

```
benches/run-speed-freebsd-aws.sh c7a.large
# then extract clocks.tach.selection.selected_provider from the printed collector-bundle path
```

## Raw artifacts

- [`artifacts/selection-comparison.json`](artifacts/selection-comparison.json) — extracted c7a
  selection, frozen c7i baseline, per-contract flip booleans, and the true instances (the generic
  runner tag does not encode them).
- [`artifacts/tach-speed-collector.json`](artifacts/tach-speed-collector.json) — collector
  attestation: source `11b4cf2`, runner `aws-freebsd-default`, target x86_64-freebsd.
- [`artifacts/raw-run-freebsd-c7a.log`](artifacts/raw-run-freebsd-c7a.log) — full raw run output.
