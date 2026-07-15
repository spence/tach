# `EVID-RELEASE-CANDIDATE-84C73F7` — Release candidate proof closure (2026-07-14)

**Status: `OBJ-PROVE-TIMERS.M2.G1` CLOSED 🟢 at candidate `84c73f7`**

## Provenance

- Repo SHA: `84c73f7810518526f6ab38f872d43f49bce05d95`.
- Shipping-code closure: `Cargo.toml`, `Cargo.lock`, and 26 files under `src/`; SHA-256
  `7f888cd0e4ed668a4ecdd6cacb1af3dbe1749ce57d2b17e86c1988103d2f5771`, byte-identical to
  both admitted runtime revisions.
- Runtime evidence: the retained 15-boundary package
  [`EVID-RELEASE-SPEED-2026-07-14`](../release-speed-closure-2026-07-14/README.md).
- Independent substrate: GitHub Actions run
  [`29380497913`](https://github.com/spence/tach/actions/runs/29380497913), spanning Ubuntu 24.04,
  native macOS x86_64/AArch64, native Windows x86_64/AArch64, and 16 cross-target builds.
- Local substrate: Apple M1 Max, `aarch64-apple-darwin`, Rust stable and Rust 1.95 surfaces.
- Full current-candidate report: [`release-report-84c73f7.json`](release-report-84c73f7.json),
  433,944 bytes, SHA-256 `b1da9b518c04406640912964b0c244e0ac5bf192420989d4caf7dc39e2556eea`.

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-PROVE-TIMERS.M2.G1` | 🟢 24/24 advertised targets, all 15 runtime decision boundaries, complete Rust/Python/MSRV/package checks, and canonical PNG/SVG regeneration passed at one shipping-code closure | [`release-gate-summary.txt`](release-gate-summary.txt) · [`ci-run.txt`](ci-run.txt) · [`local-verification.txt`](local-verification.txt) · candidate `84c73f7` |

## Findings resolved here

- GitHub run `29377723378` found that timing tests included one-time Linux provider initialization
  while running in parallel. Commit `8605f0a` serialized timing-sensitive CI tests; MSRV and native
  Linux tests pass in the closing run.
- GitHub run `29380101083` found that Windows Criterion group names were normalized to lowercase and
  did not replay on Linux's case-sensitive filesystem. Commit `292a5fd` added unambiguous
  case-insensitive group resolution plus regression coverage; Linux replay passes in the closing run.
- GitHub run `29380360999` proved the SVGs were byte-identical but the macOS-produced PNGs depended
  on the host raster/font stack. Commits `bc4db70` and `84c73f7` pinned the Ubuntu 24.04
  `rsvg-convert 2.58.0` publication environment, retained mismatch artifacts, and adopted its
  canonical rasters; all eight chart artifacts regenerate byte-clean in the closing run.

## Open

- Publication is not authorized by this package. `cargo publish --dry-run --locked` passed, but the
  crates.io upload remains an explicit owner decision under `OBJ-RELEASE-0-2`.
- Runtime measurements are named-host observations, not claims that every physical machine was
  benchmarked. Universal support and route selection are proved separately by the 24-target static
  provider proof; README and BENCHMARKS state this distinction.
- macOS may render visually equivalent but byte-different PNGs with newer librsvg and local fonts.
  SVG generation is platform-independent; CI owns the pinned canonical PNG bytes.

## Reproduce

```sh
evidence=docs/evidence/timers/release-speed-closure-2026-07-14
scratch=$(mktemp -d)
cp "$evidence"/speed*.json "$evidence"/route-observations-v1.json "$scratch"/
tar -xzf "$evidence/collector-bundles.tgz" -C "$scratch"
python3 benches/validate-speed-evidence.py \
  --data-dir "$scratch" \
  --output "$scratch/release-report.json"
python3 benches/summary-use-cases.py \
  --data-dir "$scratch" \
  --output-dir benches \
  --svg-only
python3 benches/summary-thread-cpu.py \
  --data-dir "$scratch" \
  --output-dir benches \
  --svg-only
python3 benches/verify-target-providers.py --install-targets
cargo test --release --all-features
cargo test --release --no-default-features
cargo publish --dry-run --locked
```

The complete independent release gate is `.github/workflows/ci.yml` at candidate `84c73f7`.

## Raw artifacts

- [`release-report-84c73f7.json`](release-report-84c73f7.json) — complete validator output produced
  from the tracked 15-boundary snapshot against candidate `84c73f7`.
- [`release-gate-summary.txt`](release-gate-summary.txt) — trimmed verdict and source-binding rows
  extracted from the complete current-candidate report.
- [`ci-run.txt`](ci-run.txt) — closing GitHub Actions run and all successful job classes.
- [`local-verification.txt`](local-verification.txt) — local command surface and exact aggregate
  results used before the independent run.
- [`publication-artifacts.sha256`](publication-artifacts.sha256) — exact README, BENCHMARKS, and
  eight canonical chart hashes at the closing candidate.
