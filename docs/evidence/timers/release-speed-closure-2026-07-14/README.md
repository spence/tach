# `EVID-RELEASE-SPEED-2026-07-14` — Complete timer speed closure

**Status: PASSING 🟢 — all 15 required runtime decision boundaries admit; prepared for
`OBJ-PROVE-TIMERS.M1.G1`**

## Provenance

- Campaign revisions: `68dc2015bbb81e16b9a1911c566b52aca8ff1c77` and
  `c64dcb732723c6cf288c6a453545bfc00f6b2b5d`.
- Shipping closure: `Cargo.lock`, `Cargo.toml`, and `src/` have the identical
  `7f888cd0e4ed668a4ecdd6cacb1af3dbe1749ce57d2b17e86c1988103d2f5771` digest at both revisions.
- Release validator: `tach-release-speed-evidence-v4`, top-level pass with zero failures.
- Admission: 4 primary speed cells, 11 supplemental cells, and all 15 declared runtime boundaries.
- Integrity manifest: [`SHA256SUMS`](SHA256SUMS).

The two revisions differ only in evidence/runner plumbing. No measured shipping clock code or Cargo
configuration changed between them, so the release validator admits them as one shipping-code
closure.

## Verdict

| Scope | Result | Durable evidence |
|---|---|---|
| Primary native speed cells | 🟢 4/4 | Apple AArch64, Linux AArch64, Linux x86_64, Windows x86_64 |
| Supplemental speed/fallback/smoke cells | 🟢 11/11 | Intel macOS, FreeBSD, Wasm, Emscripten, WASI, and negative fallback hosts |
| Runtime decision-boundary manifest | 🟢 15/15 | [`route-observations-v1.json`](route-observations-v1.json) |
| Complete release validator | 🟢 zero failures | [`release-report.json`](release-report.json) |

The retained artifacts prove the provider selected by each public timer is faster than or materially
tied with every eligible reliable reference in the measured environment. Smoke and tagged fallback
records prove route availability only and are not speed claims.

## Reproduce

From the repository root:

```sh
evidence=docs/evidence/timers/release-speed-closure-2026-07-14
shasum -a 256 -c "$evidence/SHA256SUMS"
tar -xzf "$evidence/collector-bundles.tgz" -C "$evidence"
python3 benches/validate-release-evidence.py \
  --data-dir "$evidence" \
  --output /tmp/tach-release-speed-report.json
rm -rf "$evidence"/*.collector.bundle
```

The generated report must have `passed: true` and an empty top-level `failures` array. Extraction is
only needed for replay; the tracked archive is the durable raw collector substrate.

## Contents

- `speed-*.json` — the exact 15 admitted primary, supplemental, fallback, and smoke artifacts.
- `route-observations-v1.json` — route identities composed from the sealed campaign revisions.
- `collector-bundles.tgz` — the 13 source-sealed raw collector bundles used by measured artifacts.
- `release-report.json` — the complete passing release-admission report.
- `SHA256SUMS` — byte-level integrity for every retained input and report.
