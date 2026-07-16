# `EVID-WINDOWS-FLIP` — windows-2022 vs windows-2025: no W/O-WINDOWS selection flip (2026-07-16)

**Status: FREEZE — no same-target flip; `W-WINDOWS` and `O-WINDOWS` freeze to a fixed QPC pick.**

The `OBJ-SIMPLIFY-TIMERS` §5.2 same-target flip probe for `x86_64-pc-windows-msvc` (freeze row 4):
does the production `Instant`/`OrderedInstant` selection differ between the frozen windows-2025 runner
and a second windows-2022 runner? It does not. The Windows selection is a real runtime 3-way
tournament (QPC vs the precise interrupt-time APIs — `src/arch/fallback.rs`), and both runner OS
versions select the identical winners, so the family has no frozen two-environment flip and freezes to
a compile-time QPC pick in M2.

## Provenance

- Repo SHA: `524b74a9f802729216a7cc785b7a28a416dfc20d` — both cells ran at this revision in one workflow
  dispatch, so this is a same-source two-environment comparison. `524b74a` adds the windows-2022 flip
  cell (`c3d9e03`) and a CI test-robustness fix (below); neither changes the Windows selection path in
  `src/arch/fallback.rs`.
- Env #1: GitHub-hosted `windows-2025`, runner tag `github-windows-2025-x86_64`,
  `x86_64-pc-windows-msvc`.
- Env #2: GitHub-hosted `windows-2022`, runner tag `github-windows-2022-x86_64`,
  `x86_64-pc-windows-msvc`.
- Command surface: `.github/workflows/bench-speed-windows.yml`, `boundary=windows-x86_64` (runs the
  windows-2025 canonical cell and the windows-2022 flip cell together), workflow run `29479941016`.
  Each cell runs the source-sealed `instant` bench and retains its collector bundle.
- Artifacts: the two collector attestation manifests (with per-file hashes) + the extracted selection
  comparison. The 8 MB Criterion trees are not committed (per plan §7.1).

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-SIMPLIFY-TIMERS.M1.G1` (row `W/O-WINDOWS`) | 🟢 no flip — windows-2022 selects the identical winners as windows-2025 | [`selection-comparison.json`](artifacts/selection-comparison.json) · [`attestation-windows-2022.json`](artifacts/attestation-windows-2022.json) · [`attestation-windows-2025.json`](artifacts/attestation-windows-2025.json) |

Selected providers, both environments at `524b74a`:

| Contract | windows-2025 | windows-2022 | Flip |
|---|---|---|---|
| `Instant` | `windows_qpc` | `windows_qpc` | no |
| `OrderedInstant` | `windows_qpc_call_boundary` | `windows_qpc_call_boundary` | no |

## Findings resolved here

- Row 4 of the §5.2 freeze table (`W/O-WINDOWS`) is verdicted: the enumerated "flip → escalate"
  branch does not fire. Under ADR-0005's selection policy (a runtime tournament survives only with a
  frozen same-target two-environment flip), the Windows family carries no such flip and converts to a
  compile-time QPC `cfg` pick in M2. This is the last of the 72 target/timer cells, so `M1.G1` closes.
- **CI test-robustness fix (`524b74a`).** The first windows-2022 dispatch failed five sleep-based
  elapsed tests: a loaded hosted runner overslept 5–10 ms sleeps to ~400 ms, exceeding a fixed
  `< 200 ms` bound, while tach itself stayed correct (`elapsed_tracks_std_within_5_percent` passed on
  the same runner). Those bounds were a garbage-guard wearing a precision-bound's clothing;
  `SLEEP_ELAPSED_MAX_MS = 60_000` makes them robust without weakening coverage (precision is
  `elapsed_tracks_std_within_5_percent`, overflow is `elapsed_saturates_when_self_is_in_the_future`).

## Open

- The windows-2025 canonical cell's `compose-speed.py` validation failed on numeric-cell drift versus
  its frozen registry at `524b74a`; that is orthogonal to the categorical provider selection compared
  here (the bundle uploaded via `always()`, and its selection is identical). The frozen canonical
  numbers are re-measured in M3.
- `i686-pc-windows-msvc` and `aarch64-pc-windows-msvc` share the `W/O-WINDOWS` family and inherit this
  verdict; no separate flip run was required.

## Reproduce

```
gh workflow run bench-speed-windows.yml --ref main -f boundary=windows-x86_64
# then, against each downloaded collector bundle:
python3 -c "import sys; sys.path.insert(0,'benches'); import extract_speed; from pathlib import Path; \
print(extract_speed.extract_collector_bundle_observation(Path('<bundle>'))['clocks']['tach']['selection']['selected_provider'])"
```

## Raw artifacts

- [`artifacts/selection-comparison.json`](artifacts/selection-comparison.json) — the two runners'
  `Instant`/`OrderedInstant` selections, the flip booleans, and provenance.
- [`artifacts/attestation-windows-2022.json`](artifacts/attestation-windows-2022.json) — collector
  manifest (source revision `524b74a`, runner `github-windows-2022-x86_64`, per-file hashes).
- [`artifacts/attestation-windows-2025.json`](artifacts/attestation-windows-2025.json) — collector
  manifest (source revision `524b74a`, runner `github-windows-2025-x86_64`, per-file hashes).
