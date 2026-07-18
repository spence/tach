# tach benchmark evidence

tach has three timer contracts, refined per
[ADR-0007](docs/decisions/0007-instant-contract-refinement.md):

- `Instant` — the fastest **same-core** clock. `elapsed()` never returns a negative `Duration` (it
  saturates to zero on a backward read across an unsynchronized core migration), but `Instant`
  does not itself promise cross-core value consistency — use `OrderedInstant` for that.
- `OrderedInstant` — the fastest **cross-core-reliable** clock: value-consistent across cores and
  carrying the documented happens-before synchronization edge.
- `ThreadCpuInstant` — the fastest reliable per-thread time, with an explicit monotonic-wall
  fallback where a platform cannot expose thread CPU time.

tach's release proof separates two claims:

- A warning-strict, optimized build proof covers all 24 advertised Rust targets and every default
  and `--no-default-features` provider route. That proves availability, routing, and hot-path
  shape—not latency on hardware we did not run.
- A primary speed campaign measures `Instant` and `OrderedInstant` steady-state cost on the four
  primary target identities shown below: Apple M1 Max, AWS Graviton 3, AWS Intel Linux, and
  GitHub Windows 2025. It proves latency on real hardware for the refined contract, not
  availability on every advertised target.

`validate_campaign_for_checkout` admits all four primary cells with zero failures, bound to a
single checked-out source revision:
[`f6df5df`](https://github.com/spence/tach/commit/f6df5df4ce8c5b0576e42d0e7cb2bd06dbcfa37b)
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
value use `OrderedInstant` instead. The audited references are `quanta 0.12.6`, `fastant 0.1.11`,
`minstant 0.1.7`, and `std::time::Instant`; eligibility is platform- and contract-specific.

| Environment | tach::Instant | fastest eligible reference | std (now) | verdict |
|---|---:|---:|---:|---|
| Apple M1 Max | **0.65 / 1.63** | quanta 3.37 | 20.21 | fastest outright |
| AWS Graviton 3 | **6.67 / 13.35** | quanta 6.79 | 32.27 | fastest (within margin) |
| AWS Intel Linux | **14.85 / 30.65** | fastant 14.87, minstant 14.85 | 26.15 | material tie; beats quanta 17.38 |
| GitHub Windows 2025 | **11.48 / 22.77** | quanta 11.44 | 37.76 | material tie (tach faster on elapsed: 22.77 < 23.90) |

"Material tie" means tach's point estimate and the conservative edge of its 95% confidence
interval both fit within `max(1 ns, 5%)` of the reference — a fraction-of-a-nanosecond wobble, not
a loss. On Windows, `Instant` reads a calibrated invariant TSC (`windows_tsc`) behind a CPUID
rate-stability gate, degrading to `QueryPerformanceCounter` when the gate fails; `OrderedInstant`
stays on `QueryPerformanceCounter` for its cross-core guarantee on every Windows architecture.

These are tight-loop throughput measurements. Independent architectural reads can overlap on an
out-of-order core; the results are not dependency-chained instruction latency.

## Synchronization-ordered elapsed time

`OrderedInstant` is the fastest **cross-core-reliable** clock (ADR-0007): value-consistent across
cores and carrying the happens-before synchronization edge. `std::time::Instant` is the eligible
cross-core-reliable public reference in this set.

| Environment | `tach::OrderedInstant` | `std::time::Instant` (now) |
|---|---:|---:|
| Apple M1 Max | **7.73 / 15.38** | 20.21 |
| AWS Graviton 3 | **20.38 / 40.04** | 32.27 |
| AWS Intel Linux | **22.60 / 43.96** | 26.15 |
| GitHub Windows 2025 | **25.27 / 53.35** | 37.76 |

`OrderedInstant` beats `std` on every primary cell. On Graviton 3 the public reference is the
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

These four are the primary cells: the `Instant` and `OrderedInstant` tables above are measured and
validated (`validate_campaign_for_checkout`) on exactly these environments, bound to revision
`f6df5df`. Every primary cell retains its source revision, build profile, enabled features, runner
identity, medians, confidence intervals, raw selector samples, and source-sealed collector bundle.
The durable package is
[`docs/evidence/timers/primary-speed-campaign-2026-07-18`](https://github.com/spence/tach/tree/v0.2.0/docs/evidence/timers/primary-speed-campaign-2026-07-18).
All temporary AWS instances and keys were removed after collection.

The `ThreadCpuInstant` table above retains its numbers from the earlier
[`release-speed-closure-2026-07-14`](https://github.com/spence/tach/tree/v0.2.0/docs/evidence/timers/release-speed-closure-2026-07-14)
package (revisions `68dc201`/`c64dcb7`) rather than the fresh `f6df5df` primary cells, and
additionally shows two residual native environments (GitHub Intel macOS, AWS FreeBSD) outside the
four primary cells. `ThreadCpuInstant` provider selection is unaffected by ADR-0007; this table
has not yet been refreshed to the new campaign, which does carry fresh `current_thread_cpu`
observations for the four primary cells.

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

**Primary `Instant`/`OrderedInstant` campaign** (revision `f6df5df`, `EVID-PRIMARY-SPEED-CAMPAIGN`):

```sh
# Apple (catalyst, M1 Max):
benches/run-speed-local.sh .tach-bench-out/f6df5df/speed-0-apple.json
# AWS c7g + inteln (self-terminating; add current IP to SG sg-05e99abafa54936d3 first):
benches/run-speed-aws.sh c7g    c7g.large     # -> speed-1-c7g.json
benches/run-speed-aws.sh inteln c7i.large     # -> speed-2-inteln.json (retry serially on the known signal-reentry harness flake)
# Windows (GitHub Actions):
gh workflow run bench --ref main              # -> artifact tach-speed-windows-2025-<sha>/speed-4-windows.json

# Validate the assembled four-cell directory (each cell beside its .collector.bundle):
python3 -c "import json,sys; sys.path.insert(0,'benches'); import speed_evidence as se; \
from pathlib import Path; d=Path('.tach-bench-out/f6df5df'); \
cells={n:d/n for n in ('speed-0-apple.json','speed-1-c7g.json','speed-2-inteln.json','speed-4-windows.json')}; \
docs={k:json.loads(v.read_text()) for k,v in cells.items()}; \
r=se.validate_campaign_for_checkout(docs,Path('.'),cells); print('passed',r['passed'],'failures',r['failures'])"

# Render the chart directly from the campaign directory:
python3 benches/summary-use-cases.py --campaign-dir .tach-bench-out/f6df5df --output-dir benches --svg-only
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
