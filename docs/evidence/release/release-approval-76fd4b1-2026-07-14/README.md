# `EVID-RELEASE-APPROVAL-76FD4B1` — Release approval packet for 76fd4b1 (2026-07-14)

**Status: `OBJ-RELEASE-0-2.M0.G1` and `.M1.G1` PASS 🟢; publication remains unapproved**

## Provenance

- Candidate SHA: `76fd4b1599ddc4a743a8f1ac7a86613d3c2f4135`.
- Shipping-code closure: `Cargo.lock`, `Cargo.toml`, and 26 files under `src/`; SHA-256
  `7f888cd0e4ed668a4ecdd6cacb1af3dbe1749ce57d2b17e86c1988103d2f5771`, byte-identical to
  both admitted runtime campaign revisions.
- Independent substrate: GitHub Actions run
  [`29381431032`](https://github.com/spence/tach/actions/runs/29381431032), with 27/27 jobs green.
- Local substrate: Apple M1 Max, `aarch64-apple-darwin`, clean detached worktree at the candidate.
- Release report: 433,944 bytes, SHA-256
  `db6e38d1f7df67d9f2f5486e18e7194ddccb100a1f728a5ffed319c95bf18999`.
- Packaged archive: 48 files, 920,908 bytes, SHA-256
  `f9e3f4d54cc5e827daab7b499b6d7ec994d40e5555942432256dd67a577870ae`.

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-RELEASE-0-2.M0.G1` | 🟢 README, BENCHMARKS, crate docs, example, metadata, target count, tag links, and packaged charts agree with the route manifest and retained proof | [`publication-artifacts.sha256`](publication-artifacts.sha256) · [`local-verification.txt`](local-verification.txt) · candidate `76fd4b1` |
| `OBJ-RELEASE-0-2.M1.G1` | 🟢 15/15 runtime boundaries, 24/24 target proof, Rust/Python/MSRV/native checks, 48-file package, docs, and publish dry run pass at one candidate | [`release-report-76fd4b1.json`](release-report-76fd4b1.json) · [`ci-run.txt`](ci-run.txt) · [`package-files.txt`](package-files.txt) |

## Findings resolved here

- The README provider table incorrectly described raw architecture counters as the Windows wall
  provider and oversimplified the Apple, rare-Linux, Wasm, and Emscripten routes. `a03772b`
  replaced it with the manifest-backed provider-selection classes.
- Packaged `BENCHMARKS.md` linked to a repository-only evidence path that is absent from the crate
  archive. `a03772b` made the link release-tag-stable and added a public-surface regression.
- The Linux signal-reentry test took its final sample before blocking further signals, allowing a
  later handler sample to make the test's comparison appear reversed. `76fd4b1` moved signal
  masking before the final sample; the exact MSRV job that failed on four preceding commits passed.

## Open

- Publication is not authorized by this package. `cargo publish --dry-run --locked` reached the
  upload step and intentionally aborted. No tag exists and nothing was uploaded to crates.io.
- Speed claims remain limited to the six named native environments. The 24-target proof establishes
  API availability and provider routing on unmeasured targets, not universal latency.
- `OBJ-RELEASE-0-2.M2.G1` requires the owner to approve this exact candidate before tagging or
  publishing, followed by a fresh consumer verification of the published crate.

## Reproduce

```sh
git worktree add --detach /tmp/tach-release-76fd4b1 76fd4b1
cd /tmp/tach-release-76fd4b1
python3 -m unittest discover -s benches -p 'test_*.py'
cargo +stable fmt --all --check
cargo +stable clippy --all-targets --all-features -- -D warnings
cargo +stable test --release --all-features
cargo +stable test --release --no-default-features
cargo +stable doc --no-deps --all-features
cargo +stable package --offline --locked
cargo +stable publish --dry-run --locked
```

The complete runtime replay command is documented in the retained timer evidence package linked
from the project README and BENCHMARKS report.

## Raw artifacts

- [`release-report-76fd4b1.json`](release-report-76fd4b1.json) — complete passing 15-boundary
  report bound to candidate `76fd4b1`.
- [`ci-run.txt`](ci-run.txt) — exact GitHub run and all 27 successful job classes.
- [`local-verification.txt`](local-verification.txt) — clean-worktree command results and archive
  checksum.
- [`package-files.txt`](package-files.txt) — all 48 files in the candidate crate archive.
- [`publication-artifacts.sha256`](publication-artifacts.sha256) — exact public document, metadata,
  chart, report, and package-list hashes.
