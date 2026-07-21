# tach benchmark evidence

tach has three timer contracts, refined per
[ADR-0007](docs/decisions/0007-instant-contract-refinement.md):

- `Instant` — the fastest **same-core** clock. `elapsed()` never returns a negative `Duration` (it
  saturates to zero on a backward read across an unsynchronized core migration), but `Instant`
  does not itself promise cross-core value consistency — use `GlobalInstant` for that.
- `GlobalInstant` — the fastest **cross-core-reliable** clock: value-consistent across cores and
  carrying the documented happens-before synchronization edge.
- `ThreadCpuInstant` — the fastest reliable per-thread time, with an explicit monotonic-wall
  fallback where a platform cannot expose thread CPU time.

tach's release proof separates two claims:

- A warning-strict, optimized build proof covers all 24 advertised Rust targets and every default
  and `--no-default-features` provider route. That proves availability, routing, and hot-path
  shape—not latency on hardware we did not run.
- A primary speed campaign measures `Instant` and `GlobalInstant` steady-state cost on the four
  primary target identities shown below: Apple M1 Max, AWS Graviton 3, AWS Intel Linux, and
  GitHub Windows 2025. It proves latency on real hardware for the refined contract, not
  availability on every advertised target.

`validate_campaign_for_checkout` admits all four primary cells with zero failures, bound to a
single checked-out source revision:
[`e35ec98`](https://github.com/spence/tach/commit/e35ec986c0a797f2291908b91863e374dd352824)
(`docs/evidence/timers/primary-speed-campaign-2026-07-18/`).

Every value below is nanoseconds per call; lower is better. Each pair is
`now() / (now() + elapsed())`.

“Fastest tested” means tach is faster than or materially tied with every reference eligible for
that timer's contract on that platform. The predeclared material-tie band is `max(1 ns, 5%)`.
Both tach's point estimate and conservative 95% confidence-interval comparison must fit inside
that band. Faster but contract-ineligible raw counters remain visible as diagnostics and do not
become competitors by weakening the contract.

## Combined chart

![tach steady-state speed across three timing contracts](benches/summary-use-cases.png)

The chart renderer reads the four-primary campaign directory, runs
`validate_campaign_for_checkout`, and only then renders the captured bytes admitted by that gate.
It refuses mixed shipping code, missing cells, malformed confidence intervals, unreproducible
selectors, or failed eligible-reference comparisons.

## Same-thread elapsed time

`Instant` is the fastest **same-core** clock (ADR-0007): callers that need a cross-core-reliable
value use `GlobalInstant` instead. The audited references are `quanta 0.12.6`, `fastant 0.1.11`,
`minstant 0.1.7`, and `std::time::Instant`; eligibility is platform- and contract-specific.

| Environment | tach::Instant | fastest eligible reference | std (now) | verdict |
|---|---:|---:|---:|---|
| Apple M1 Max | **0.65 / 1.59** | quanta 3.34 | 19.84 | fastest outright |
| AWS Graviton 3 | **6.68 / 13.35** | quanta 6.83 | 32.23 | fastest (within margin) |
| AWS Intel Linux | **13.94 / 28.78** | minstant 13.96, fastant 13.98 | 24.61 | material tie; beats quanta 16.28 |
| GitHub Windows 2025 | **11.48 / 22.76** | quanta 11.44 | 37.72 | material tie (tach faster on elapsed: 22.76 < 23.49) |

"Material tie" means tach's point estimate and the conservative edge of its 95% confidence
interval both fit within `max(1 ns, 5%)` of the reference — a fraction-of-a-nanosecond wobble, not
a loss. On Windows, `Instant` reads a calibrated invariant TSC (`windows_tsc`) behind a CPUID
rate-stability gate, degrading to `QueryPerformanceCounter` when the gate fails; `GlobalInstant`
stays on `QueryPerformanceCounter` for its cross-core guarantee on every Windows architecture.

These are tight-loop throughput measurements. Independent architectural reads can overlap on an
out-of-order core; the results are not dependency-chained instruction latency.

## Cross-thread elapsed time

`GlobalInstant` is the fastest **cross-core-reliable** clock (ADR-0007): value-consistent across
cores and carrying the happens-before synchronization edge. `std::time::Instant` is the eligible
cross-core-reliable public reference in this set.

| Environment | `tach::GlobalInstant` | `std::time::Instant` (now) |
|---|---:|---:|
| Apple M1 Max | **7.65 / 15.07** | 19.84 |
| AWS Graviton 3 | **20.38 / 40.05** | 32.23 |
| AWS Intel Linux | **21.26 / 41.34** | 24.61 |
| GitHub Windows 2025 | **25.29 / 53.37** | 37.72 |

`GlobalInstant` beats `std` on every primary cell. On Graviton 3 the public reference is the
usable, shippable `isb; cntvct` read; the same route's exact (compile-time-specialized) form is
retained only as a disclosed diagnostic dispatch lower bound, not a competitor, because the
mandatory `isb` barrier exposes a SIGILL-safe provider dispatch tach cannot skip and still ship.
See
[`docs/evidence/timers/primary-speed-campaign-2026-07-18/README.md`](docs/evidence/timers/primary-speed-campaign-2026-07-18/README.md).

Speed is only half this contract. The load-then-now-then-check harness separately recorded zero
inversions across about 10.9 billion x86 and AArch64 reads. See
[`benches/ORDERED-VERIFICATION.md`](benches/ORDERED-VERIFICATION.md). RISC-V and LoongArch use their
strongest ISA barriers but remain best-effort because their specifications do not guarantee that
those barriers order the time CSR.

## Current-thread CPU usage

![ThreadCpuInstant versus each platform's native thread clock](benches/summary-thread-cpu.png)

The native reference invokes the same OS primitive directly. The selected-exact row measures the
provider tach installed, exposing TLS or dispatch overhead without treating a private route as a
caller-usable competitor.

| Environment | Selected tach provider | tach | OS native | Selected exact |
|---|---|---:|---:|---:|
| Apple M1 Max | `clock_gettime_nsec_np` | **128.90 / 258.18** | 128.54 / 260.41 | 128.33 / 257.41 |
| GitHub Intel macOS | `clock_gettime_nsec_np` | **444.51 / 918.12** | 429.77 / 888.46 | 444.50 / 914.71 |
| AWS Graviton 3 | Linux perf task-clock mmap | **56.95 / 114.68** | 259.34 / 535.21 | 57.81 / 116.26 |
| AWS Intel Linux | raw `CLOCK_THREAD_CPUTIME_ID` syscall | **150.75 / 302.94** | 150.87 / 302.62 | 150.92 / 304.81 |
| GitHub Windows 2025 | `GetThreadTimes` | **218.21 / 444.94** | 218.90 / 435.75 | 216.28 / 458.77 |
| AWS FreeBSD | raw `CLOCK_THREAD_CPUTIME_ID` syscall | **125.31 / 250.81** | 131.34 / 263.47 | 127.07 / 257.28 |

Linux AArch64 uses an audited availability policy: when the kernel exposes complete perf
task-clock mmap conversion metadata and architectural-counter access, tach selects that inline
route; otherwise it falls back to the raw thread-clock syscall. A retained c6g/c7g/c8g/t4g survey
found no same-target profitability reversal, so AArch64 does not pay for a startup tournament.

Linux x86 keeps runtime selection because the same Rust target has shown a real reversal. On this
`c7i.large`, perf access was available but slower, so tach selected the raw syscall. The tournament
measures complete candidate paths in nine alternating 4,096-read batches and changes provider only
after at least eight material wins.

The Windows baseline is `GetThreadTimes`, not `QueryThreadCycleTime`: Microsoft documents the
latter as cycles that must not be converted into elapsed time, which is a different quantity from
this API's `Duration` contract.

## Measured environments

| Environment | Runtime identity | Rust target | Harness |
|---|---|---|---|
| Apple Silicon | M1 Max MacBook Pro | `aarch64-apple-darwin` | Criterion |
| AWS Graviton 3 | `c7g.large` | `aarch64-unknown-linux-gnu` | Criterion |
| AWS Intel Linux | `c7i.large` | `x86_64-unknown-linux-gnu` | Criterion |
| GitHub Windows | `windows-2025` | `x86_64-pc-windows-msvc` | Criterion |

These four are the primary cells: the `Instant` and `GlobalInstant` tables above are measured and
validated (`validate_campaign_for_checkout`) on exactly these environments, bound to revision
`e35ec98`. Every primary cell retains its source revision, build profile, enabled features, runner
identity, medians, confidence intervals, raw selector samples, and source-sealed collector bundle.
The durable package is
[`docs/evidence/timers/primary-speed-campaign-2026-07-18`](https://github.com/spence/tach/tree/v0.2.0/docs/evidence/timers/primary-speed-campaign-2026-07-18).
All temporary AWS instances and keys were removed after collection.

The `ThreadCpuInstant` table and its dedicated chart above draw from the retained
[`release-speed-closure-2026-07-14`](https://github.com/spence/tach/tree/v0.2.0/docs/evidence/timers/release-speed-closure-2026-07-14)
package (revisions `68dc201`/`c64dcb7`) — the more comprehensive thread-CPU measurement, with more
samples and two extra native environments (GitHub Intel macOS, AWS FreeBSD) beyond the four primary
cells. `ThreadCpuInstant` provider selection is unaffected by ADR-0007, so these measured code paths
are byte-identical at `e35ec98`. The steady-state chart at the top of this file draws its thread-CPU
panel instead from the fresh `e35ec98` primary campaign; the two agree within run-to-run noise except
on `c7i.large`, where the close x86 provider tournament selected the raw `CLOCK_THREAD_CPUTIME_ID`
syscall in the retained package (150.75 ns) and `clock_gettime` in the primary campaign (166.63 ns).
The retained package is the authoritative thread-CPU reference; collapsing to a single source at
`e35ec98` would require re-running the full release-closure campaign (all supplemental cells) and is
deferred as a post-0.2.0 refinement.

## Methodology

- Criterion cells use one-second warmup and three-second measurement windows and retain the point
  estimate plus 95% confidence interval for every benchmark.
- `now()` measures sample acquisition. The roundtrip measures `now()` followed by `elapsed()`,
  including the second read, subtraction, conversion, and `Duration` construction.
- Provider initialization is primed before measurement. Results describe steady-state cost, not
  first-call setup latency.
- Public references run in the same process and invocation. Runtime tournaments alternate
  candidates within paired batches so scheduling drift cannot masquerade as a provider win.
- Benchmarks are single-threaded hot-path throughput measurements, not contention or cross-machine
  accuracy measurements.

## Universal target and provider proof

The warning-strict proof compiles all three public APIs in default and
`--no-default-features` modes on 24 target triples, then inspects optimized LLVM IR for the
expected local, ordered, and thread-CPU provider routes. It closes 294 timer routes and their 294
paired `now()`/elapsed paths.

This proves that the APIs compile and route as documented. It does not assert relative latency on
unmeasured hardware. WASI thread-clock availability is host-dependent; browser, bare Wasm, and
Emscripten can expose an explicit monotonic-wall fallback for `ThreadCpuInstant`.

Run it with:

```sh
python3 benches/verify-target-providers.py --install-targets
```

## Reproduce the retained gate and charts

**Primary `Instant`/`GlobalInstant` campaign** (revision `e35ec98`, `EVID-PRIMARY-SPEED-CAMPAIGN`):

```sh
# Apple (catalyst, M1 Max):
benches/run-speed-local.sh .tach-bench-out/e35ec98/speed-0-apple.json
# AWS c7g + inteln (self-terminating; add current IP to SG sg-05e99abafa54936d3 first):
benches/run-speed-aws.sh c7g    c7g.large     # -> speed-1-c7g.json
benches/run-speed-aws.sh inteln c7i.large     # -> speed-2-inteln.json (retry serially on the known signal-reentry harness flake)
# Windows (GitHub Actions):
gh workflow run bench --ref main              # -> artifact tach-speed-windows-2025-<sha>/speed-4-windows.json

# Validate the assembled four-cell directory (each cell beside its .collector.bundle):
python3 -c "import json,sys; sys.path.insert(0,'benches'); import speed_evidence as se; \
from pathlib import Path; d=Path('.tach-bench-out/e35ec98'); \
cells={n:d/n for n in ('speed-0-apple.json','speed-1-c7g.json','speed-2-inteln.json','speed-4-windows.json')}; \
docs={k:json.loads(v.read_text()) for k,v in cells.items()}; \
r=se.validate_campaign_for_checkout(docs,Path('.'),cells); print('passed',r['passed'],'failures',r['failures'])"

# Render the chart directly from the campaign directory:
python3 benches/summary-use-cases.py --campaign-dir .tach-bench-out/e35ec98 --output-dir benches --svg-only
```

See
[`docs/evidence/timers/primary-speed-campaign-2026-07-18/README.md`](docs/evidence/timers/primary-speed-campaign-2026-07-18/README.md)
for the full Reproduce block and provenance.

**`ThreadCpuInstant`** (retained package, revisions `68dc201`/`c64dcb7`):

```sh
evidence=docs/evidence/timers/release-speed-closure-2026-07-14
scratch=$(mktemp -d)

(cd "$evidence" && shasum -a 256 -c SHA256SUMS)
cp "$evidence"/speed*.json "$evidence"/route-observations-v1.json "$scratch"/
tar -xzf "$evidence/collector-bundles.tgz" -C "$scratch"

python3 benches/summary-thread-cpu.py --data-dir "$scratch" --output-dir benches --svg-only
```

The renderers consume only validated bytes. SVG output is platform-independent. The checked-in
PNGs are the canonical Ubuntu 24.04 release rasters produced with `rsvg-convert 2.58.0`; newer
local librsvg and font stacks may produce visually equivalent but byte-different PNGs, so CI owns
their byte-for-byte regeneration. Recollecting a primary cell uses the source-sealed runners in
`benches/run-speed-aws.sh` and `benches/run-speed-local.sh`; the retained `ThreadCpuInstant`
cells use `benches/run-speed-freebsd-aws.sh` and the hosted benchmark workflow.
