# `EVID-GRAVITON4-FLIP-LINUX-A64` — c8g Graviton4 vs c7g Graviton3: no W/O-LINUX-A64 selection flip (2026-07-15)

**Status: FREEZE — no same-target flip; `W/O-LINUX-A64` freezes fixed.**

The `OBJ-SIMPLIFY-TIMERS` §5.2 same-target flip probe for `aarch64-unknown-linux-gnu`: does the
production `Instant`/`OrderedInstant` selection differ between the frozen Graviton 3 environment (c7g)
and a second Graviton 4 environment (c8g)? It does not — both select the identical winners, so the
family has no frozen two-environment flip and freezes to a compile-time pick in M2.

## Provenance

- Repo SHA: `5bcd67ea6d5376d9c5ac1eb86ec0fad8a9f03d20` (the c8g run's sealed source revision). The
  aarch64-gnu measured path is byte-identical to the frozen c7g cell's revision `c64dcb73` — every
  change since is Apple-cfg-gated — so this is a valid same-target comparison across Graviton
  generations, not across source.
- Substrate: AWS `c8g.large` (Graviton 4), `aarch64-unknown-linux-gnu`, AL2023 kernel 6.12, runner
  `aws-c8g`. Self-terminated (`i-0479da2f95ef1c0ef` reached `terminated`; no orphan).
- Comparison baseline (env #1): frozen `benches/speed-1-c7g.json` — AWS `c7g.large` (Graviton 3),
  runner `aws-c7g`, revision `c64dcb73`.
- Command surface: `benches/run-speed-aws.sh c8g c8g.large` (sanctioned flip-probe path, commit
  `5bcd67e`) → sealed Criterion `instant` bench → retained collector bundle → local selection extract.
- Artifacts: 320 KB retained (attestation + raw log + extracted comparison); the full Criterion tree
  is not committed (per plan §7.1).

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-SIMPLIFY-TIMERS.M1.G1` (row `W/O-LINUX-A64`) | 🟢 no flip — c8g selects the identical winners as c7g | [`selection-comparison.json`](artifacts/selection-comparison.json) · [`tach-speed-collector.json`](artifacts/tach-speed-collector.json) |

| Contract | c7g (Graviton3, frozen) | c8g (Graviton4) | Flip |
|---|---|---|---|
| `Instant` | `aarch64_cntvct` | `aarch64_cntvct` | no |
| `OrderedInstant` | `aarch64_isb_cntvct` | `aarch64_isb_cntvct` | no |

## Findings resolved here

Row 3 of the §5.2 freeze table (`W/O-LINUX-A64`) is verdicted: the "flip → stays measured" branch does
not fire. Both Graviton generations select the bare `CNTVCT` for `Instant` and the `ISB`+`CNTVCT`
ordered form for `OrderedInstant`. Under ADR-0005's selection policy the family carries no frozen
same-target flip and converts to a compile-time `cfg` pick plus capability gate in M2.

## Open

- FreeBSD c7a and c5n.metal thread-CPU flip rows remain to run; `ESC-AMD-FLIP-PROBE-TOOLING` stays
  open for owner ratification of the flip-probe mechanism.

## Reproduce

```
benches/run-speed-aws.sh c8g c8g.large
# then, against the printed collector-bundle path, extract clocks.tach.selection.selected_provider
```

## Raw artifacts

- [`artifacts/selection-comparison.json`](artifacts/selection-comparison.json) — extracted c8g
  selection, frozen c7g baseline, per-contract flip booleans, provenance.
- [`artifacts/tach-speed-collector.json`](artifacts/tach-speed-collector.json) — collector
  attestation: source `5bcd67e`, runner `aws-c8g`, target aarch64-linux-gnu.
- [`artifacts/raw-run-c8g.log`](artifacts/raw-run-c8g.log) — full raw run output including
  self-termination.
