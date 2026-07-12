# `EVID-PROVIDER-CORRECTNESS` — provider correctness remediation (2026-07-12)

**Status: GATE CLOSED 🟢 — `OBJ-FASTEST-TIMERS.M0.G1` source correctness.**

## Provenance

- Repo SHA(s): fixes `6a8058291fd931158f8483797fe05ab38bd4caea`, Emscripten probe `4751a1863125d8690021a4c776999e3e5493e208`, and FreeBSD runner `ea0db8888fc101bc5ae22a07a2ab57967207ab37`.
- Substrate / hardware: macOS/aarch64 host; Node 26.5.0; Emscripten 6.0.2; FreeBSD 15.0-RELEASE/amd64 on an AWS c7i.large.
- Command surface: strict host Rust checks, `benches/run-emscripten-reentry.sh`, and `benches/run-freebsd-retier-test.sh`.
- Artifact checksums / sizes: `host-checks.log` 882 B / `afc07ada47e630a880122ce6996c0795cec968513d70c0211824605ab250dc89`; `emscripten-reentry.log` 350 B / `9af316ec62f42980ce7c9569136f6ba6c90d01cc57c1f481fcc4f323cacd91a9`; `freebsd-retier.log` 879 B / `94df407bcb74cbcba298ceb5a6738002d72a3f3bdf31b94249ac43acb249cc74`.

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-FASTEST-TIMERS.M0.G1` | 🟢 Emscripten same-thread reentry no longer spins; FreeBSD AT_TIMEKEEP retirement updates selected fallback and read cost | [`host-checks.log`](host-checks.log) · [`emscripten-reentry.log`](emscripten-reentry.log) · [`freebsd-retier.log`](freebsd-retier.log) |

## Findings resolved here

- Emscripten local first-use could spin when a host clock synchronously reentered Rust. `6a80582` pins the active probe domain and returns its cached value; `4751a18` executes that callback in Node.
- FreeBSD could retain `AT_TIMEKEEP` as the reported inline provider after it fell back. `6a80582` retires it to the already-measured same-nanosecond fallback; `ea0db88` runs both targeted tests natively.

## Open

- This package proves source correctness, not all-platform speed. Exhaustive route harness work, the 24-target codegen matrix, and the runtime speed campaign remain open in `OBJ-FASTEST-TIMERS.M1`, `.M2`, and `OBJ-PROVE-TIMERS`.
- The Emscripten probe covers local-clock reentry only. Pthread/shared-memory ordered-clock runtime evidence remains a later supplemental-platform obligation.

## Reproduce

```text
RUSTFLAGS='-D warnings' cargo test --lib --tests --all-features
RUSTFLAGS='-D warnings' cargo test --lib --tests --no-default-features
cargo clippy --all-targets --all-features -- -D warnings
RUSTUP_TOOLCHAIN=1.95.0-aarch64-apple-darwin benches/run-emscripten-reentry.sh
AWS_PROFILE=tach benches/run-freebsd-retier-test.sh
```

## Raw artifacts

- [`host-checks.log`](host-checks.log) — trimmed strict host test and lint results.
- [`emscripten-reentry.log`](emscripten-reentry.log) — target runtime callback result.
- [`freebsd-retier.log`](freebsd-retier.log) — retained native FreeBSD targeted-test result.
