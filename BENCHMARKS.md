# tach benchmarks

`tach::Instant` and `tach::OrderedInstant` per-call cost compared with `quanta`,
`fastant`, `minstant`, and `std::time::Instant` across six target / environment
cells. All numbers are nanoseconds per call (lower is better). `tach::Instant` is
the fastest read on every cell; `tach::OrderedInstant` adds a memory barrier for
cross-thread ordering and is the fastest clock that stays correct across threads —
see [`benches/summary-ordered.png`](benches/summary-ordered.png), which puts it
against `std`, the only other cross-thread-correct option.

## Results

### `Instant::now()` cost

| Target | Environment | Instance | tach | tach_ordered | quanta | fastant | minstant | std |
|---|---|---|---:|---:|---:|---:|---:|---:|
| `aarch64-apple-darwin` | Apple Silicon | M1 Max MacBook Pro | **0.34** | 12.16 | 3.22 | 25.87 | 25.85 | 19.35 |
| `aarch64-unknown-linux-gnu` | AWS Graviton 3 | c7g.large | **6.67** | 20.27 | 7.04 | 41.76 | 41.61 | 32.42 |
| `x86_64-unknown-linux-gnu` | AWS Intel | c7i.large | **14.71** | 22.23 | 17.53 | 15.03 | 15.03 | 26.61 |
| `x86_64-unknown-linux-musl` | AWS Intel (musl) | c7i.large + Alpine | **14.44** | 21.81 | 17.20 | 14.75 | 14.74 | 26.43 |
| `x86_64-pc-windows-msvc` | GitHub Windows | windows-2025 | **11.53** | 22.17 | 11.83 | 41.21 | 41.21 | 38.05 |
| `x86_64-unknown-linux-gnu` | AWS Lambda | provided.al2023 1024MB | **13.83** | 25.89 | 22.55 | 14.59 | 58.58 | 48.39 |

### `Instant::now() + elapsed()` cost (full roundtrip)

| Target | Environment | Instance | tach | tach_ordered | quanta | fastant | minstant | std |
|---|---|---|---:|---:|---:|---:|---:|---:|
| `aarch64-apple-darwin` | Apple Silicon | M1 Max MacBook Pro | **1.29** | 23.11 | 6.98 | 57.76 | 57.76 | 42.33 |
| `aarch64-unknown-linux-gnu` | AWS Graviton 3 | c7g.large | **13.37** | 40.04 | 15.38 | 87.69 | 87.65 | 70.58 |
| `x86_64-unknown-linux-gnu` | AWS Intel | c7i.large | **30.16** | 43.20 | 38.97 | 40.69 | 41.04 | 56.82 |
| `x86_64-unknown-linux-musl` | AWS Intel (musl) | c7i.large + Alpine | **29.52** | 42.41 | 38.33 | 39.92 | 39.95 | 56.02 |
| `x86_64-pc-windows-msvc` | GitHub Windows | windows-2025 | **22.76** | 48.12 | 24.34 | 95.26 | 95.21 | 78.88 |
| `x86_64-unknown-linux-gnu` | AWS Lambda | provided.al2023 1024MB | **32.99** | 55.53 | 48.04 | 52.50 | 138.51 | 106.54 |

Chart: [`benches/summary.png`](benches/summary.png) — one cell per target environment. Each crate row shows `Instant::now()` (dark portion of bar) and the full `now() + elapsed()` roundtrip (lighter extension), with numeric times as `now / elapsed` on the right. A second chart, [`benches/summary-ordered.png`](benches/summary-ordered.png), isolates `tach::OrderedInstant` against `std` — the two clocks that stay correct across threads.

Instance sizes are the smallest that produce the data: per-call cost is single-threaded and depends on the silicon, not the instance size, so a `c7g.large` reads the same `now()` ns as a larger Graviton 3 instance. All six cells come from one fresh campaign (2026-06) where every clock is measured in the same run, so `std` is identical between the two charts.

**Not included**: `x86_64-apple-darwin` (GitHub Actions `macos-13`) — could not land an Intel macOS runner allocation across multiple `workflow_dispatch` attempts. The GitHub-hosted Intel macOS runner pool has very low capacity.

## Methodology

- **Harness**: Criterion 0.8 (`harness = false`, custom `criterion_main!`).
- **Measured functions**: `Instant::now()` standalone, and `let start = Instant::now(); black_box(start.elapsed())` (full roundtrip).
- **Compiler**: stable Rust at the time of run (2026-05).
- **Sample size**: Criterion default sampling with `--warm-up-time 1 --measurement-time 3` on every cell, so the per-call numbers are directly comparable across platforms.
- **CPU governor**: `performance` where the runtime exposes it (Linux). macOS and Windows use the OS default; bare metal runs at base clock.
- **Process**: single-threaded, no other workload contending for the CPU.

## Reproducing

### Local

```bash
git clone https://github.com/spence/tach.git
cd tach
cargo bench --bench instant
# results land in target/criterion/<name>/new/estimates.json
# point estimate is at .median.point_estimate (in nanoseconds)
```

### AWS EC2 (Linux gnu)

For `aarch64-unknown-linux-gnu` (Graviton) and `x86_64-unknown-linux-gnu` (Intel/AMD):

```bash
# Launch the smallest instance that meets the technical requirement.
# Examples: c7g.4xlarge for Graviton, t3.medium for Intel burst.
aws ec2 run-instances \
  --image-id $(aws ssm get-parameters --names \
      "/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-${ARCH}" \
      --query 'Parameters[0].Value' --output text) \
  --instance-type c7g.4xlarge \
  --key-name "$KEY_NAME" \
  --security-group-ids "$SG_WITH_SSH" \
  --instance-initiated-shutdown-behavior terminate \
  --tag-specifications "ResourceType=instance,Tags=[{Key=Name,Value=tach-bench-XYZ}]" \
  --region us-east-2

# Once running, SSH in and run:
ssh -i ~/.ssh/your-key.pem ec2-user@<public-ip>
sudo dnf install -y gcc git                                    # <-- MUST install gcc; AL2023 is bare
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
source $HOME/.cargo/env
git clone --depth 1 https://github.com/spence/tach.git
cd tach
cargo bench --bench instant 2>&1 | tee /tmp/bench.out
# When done: aws ec2 terminate-instances --instance-ids <id>
```

**Gotcha**: AL2023's base image doesn't include a C linker, and `rustup --profile minimal` also doesn't include one. You'll see `linker 'cc' not found` from native-build-script crates (serde, libc, etc.) unless you `dnf install -y gcc` first.

### AWS EC2 (Linux musl, Alpine on metal)

For `x86_64-unknown-linux-musl`, run inside an Alpine Docker container on a metal host:

```bash
# Launch m7i.metal-24xl (or smaller; this one was kept from the historical baseline)
sudo dnf install -y docker
sudo systemctl start docker
sudo docker run --rm alpine:latest sh -c '
  apk add --no-cache git curl build-base linux-headers
  curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  source $HOME/.cargo/env
  git clone --depth 1 https://github.com/spence/tach.git
  cd tach
  cargo bench --bench instant
'
```

**Note**: Alpine's `build-base` package includes `gcc`, so no separate install needed.

### GitHub Actions runners

For `x86_64-pc-windows-msvc` (windows-2025) and `x86_64-apple-darwin` (macos-13):

The workflow at [`.github/workflows/bench.yml`](.github/workflows/bench.yml) runs on manual dispatch. Trigger via:

```bash
gh workflow run bench --ref main
gh run watch                                                # follow live
gh run view <run-id> --log --job=<job-id> | grep "time:"   # extract numbers
```

**Gotcha**: GitHub runner labels are confusing — `macos-15`/`macos-14` are Apple Silicon (ARM). `macos-13` is the only Intel macOS runner available. `windows-2025` and `ubuntu-24.04` are x86_64. Intel macOS runner capacity is limited on the GH-hosted fleet; expect long queues.

### AWS Lambda

For `provided.al2023` x86_64. A standalone Lambda handler (not the criterion bench — Lambda's runtime doesn't accommodate criterion's filesystem assumptions) runs the bench in-process and returns the per-call timings as JSON. Source at `/tmp/tach-lambda-bench/` (separate Cargo project, depends on `tach` via path).

```bash
# Build (uses zig under the hood for cross-compile)
cd /tmp/tach-lambda-bench
cargo lambda build --release --output-format=zip

# Deploy (requires a pre-created execution role; one-time setup with admin creds)
cargo lambda deploy --profile $YOUR_PROFILE --region us-east-2 \
  --iam-role arn:aws:iam::$ACCT:role/tach-bench-lambda-role \
  --memory 1024 --timeout 300 tach-lambda-bench

# Invoke and capture the JSON response
aws lambda invoke --function-name tach-lambda-bench \
  --profile $YOUR_PROFILE --region us-east-2 \
  --cli-binary-format raw-in-base64-out --payload '{}' /tmp/result.json
cat /tmp/result.json | python3 -m json.tool

# Cleanup
aws lambda delete-function --function-name tach-lambda-bench \
  --profile $YOUR_PROFILE --region us-east-2
```

**Note**: Lambda numbers are noisier than EC2 (Firecracker VM with shared CPU). They're useful as a relative comparison but don't compare directly to bare-metal numbers.

## Updating the chart

After collecting new measurements, edit `NOW_GROUPS` and `ELAPSED_GROUPS` in `benches/summary.py`, then:

```bash
python3 benches/summary.py
```

`rsvg-convert` is required (`brew install librsvg` on macOS, `apt install librsvg2-bin` on Debian).

## Per-cell reports

Each cell has a standalone SVG report at `benches/report-<cell>.svg` showing the violin distribution, per-crate density plots, and a medians table — composed from criterion's output by `benches/report.py`.

After running `cargo bench --bench instant` on a target machine:

```bash
# Criterion mode (default; reads target/criterion):
python3 benches/report.py <cell-name> \
  --title "Pretty Cell Title" \
  --subtitle "target-triple"

# Or compose from criterion data stored elsewhere on disk:
python3 benches/report.py <cell-name> \
  --criterion-dir path/to/criterion \
  --title "..." --subtitle "..."
```

Output: `benches/report-<cell-name>.svg`. Handles both gnuplot- and plotters-generated criterion violins.

For AWS Lambda (which can't host criterion), use the standalone `tach-lambda-bench` handler at `/tmp/tach-lambda-bench/`, invoke it N times, save each response as `runs/runN.json`, then:

```bash
python3 benches/report.py lambda-x86_64 \
  --title "AWS Lambda — provided.al2023" \
  --subtitle "x86_64-unknown-linux-gnu · 1024 MB / Firecracker" \
  --lambda-runs path/to/runs
```

Output: same `benches/report-lambda-x86_64.svg` location, with bar-and-whisker chart (median + min/max across runs).

Current cells:

- `benches/report-apple-silicon-m1.svg` — Apple Silicon M1 MBP
- `benches/report-c7g-4xlarge.svg` — AWS Graviton 3
- `benches/report-t3-medium.svg` — AWS Intel Burst
- `benches/report-m7i-metal-24xl.svg` — Docker Alpine on AWS Metal
- `benches/report-lambda-x86_64.svg` — AWS Lambda
- `benches/report-github-windows-x86_64.svg` — GitHub Actions Windows

## Skew and monotonicity

Measured by `cargo bench --bench skew --features bench-internal` (binary at `benches/skew.rs`) on each cell. Per-cell raw JSON in `benches/skewmono-<cell>.json`; per-cell rendered report SVGs at `benches/report-skewmono-<cell>.svg`.

The bench captures three quantities for each clock source:

- **Per-thread monotonicity**: tight single-thread loop for 10 s. Reports backward jumps (`now() < previous_now()`) and largest magnitude. Expected 0 on every modern clock.
- **Cross-thread observation consistency**: N threads (min(num_cpus, 16)) race on a shared `AtomicU64` max for 10 s. A "violation" is a read whose value is less than something another thread already published — i.e., we observed a non-monotonic timeline across threads. Bracket-read filter drops iterations preempted between counter read and publish.
- **Drift vs `std::Instant`**: 30 × 1 s samples and 5 × 60 s samples; report median + spread. The reference is `std::Instant` (CLOCK_MONOTONIC on Linux / CLOCK_UPTIME_RAW on macOS / QueryPerformanceCounter on Windows).

### Drift vs `std::Instant` (median across samples, per cell)

Per-cell 1-second and 1-minute median skew. Negative = tach reports less elapsed than std; positive = more.

| Cell | tach 1s | tach 1m | tach_ordered 1m | tach_recal 1s | tach_recal 1m | std 1m |
|---|---|---|---|---|---|---|
| `apple-silicon-m1` | -708 ns | -666 ns | -1.0 µs | -1.2 µs | -1.2 µs | -501 ns |
| `c7g-4xlarge` | +27 ns | +3.3 µs | +2.5 µs | +31 ns | +9.9 µs | -554 ns |
| `t3-medium` | +1.3 µs | +175.6 µs | +173.9 µs | -1.9 µs | +17.4 µs | -840 ns |
| `m7i-metal-24xl` | -2.4 µs | -108.7 µs | -116.7 µs | -3.0 µs | -10.4 µs | -373 ns |
| `lambda-x86_64` | -1.5 µs | +25.5 µs | +26.2 µs | -1.7 µs | +25.9 µs | -305 ns |
| `github-windows-x86_64` | -939 ns | +14.3 µs | +32.9 µs | +808 ns | +47.6 µs | -500 ns |

Observations:
- `tach_ordered 1m` (the `OrderedInstant` row) matches `tach 1m` within bench noise on every cell, as expected by construction: both read the same underlying counter, so they have identical drift profiles. The ordering barrier constrains *when* the read is sampled, not the tick value, so it doesn't change drift.
- `recalibrate-background` is a measurable improvement on Intel x86 cells where startup calibration accumulates error. The remaining variance across cells is dominated by per-instance calibration noise and per-chip crystal lottery (different physical chips in the same EC2 instance family can vary by ppm). The cross-cell median in the README table is the right summary statistic; individual per-cell numbers move between runs.
- `c7g-4xlarge` previously showed a ~27 ppm constant offset because `cntfrq_el0` is the firmware-published nominal rather than the measured crystal rate. Since the aarch64-Linux calibration landed, drift tracks the kernel's NTP-corrected vDSO scaling and stays sub-ppm regardless of the specific Graviton 3 chip the run landed on.
- `std::time::Instant` 1m skew is consistently sub-µs on every cell, reflecting the kernel's continuous correction (vDSO scaling-factor updates on Linux, the equivalent on each OS).

## Cross-thread monotonicity

Per-cell maximum cross-thread violation magnitude (ns). Cells where the value exceeds 10 µs are flagged as a hazard.

| Clock | apple-silicon-m1 | c7g-4xlarge | t3-medium | m7i-metal-24xl | lambda-x86_64 | github-windows-x86_64 |
|---|---|---|---|---|---|---|
| `tach` | 9.8 µs | 4.3 µs | 9.9 µs | 9.9 µs | 9.9 µs | 14.3 µs |
| `tach_recal` | 9.9 µs | 137 ns | 9.9 µs | 9.9 µs | 9.8 µs | 9.9 µs |
| `tach_ordered` | 9.8 µs | 9.6 µs | 9.9 µs | 9.8 µs | 9.9 µs | 9.9 µs |
| `quanta` | 9.9 µs | 28.5 µs | 9.9 µs | 9.8 µs | 9.9 µs | 9.9 µs |
| `minstant` | 10.0 µs | 9.6 µs | 9.9 µs | 9.9 µs | 9.8 µs | 9.8 µs |
| `fastant` | 10.0 µs | 9.7 µs | 9.9 µs | 9.8 µs | 9.9 µs | 9.8 µs |
| `std` | 9.8 µs | 9.4 µs | 9.8 µs | 9.8 µs | 9.9 µs | 10.0 µs |

### Per-thread monotonicity

- apple-silicon-m1: 0 backward jumps on any clock ✓
- c7g-4xlarge: 0 backward jumps on any clock ✓
- t3-medium: 0 backward jumps on any clock ✓
- m7i-metal-24xl: 0 backward jumps on any clock ✓
- lambda-x86_64: 0 backward jumps on any clock ✓
- github-windows-x86_64: 0 backward jumps on any clock ✓

## Per-thread call cost

Per-thread cost (single-thread tight loop, no contention) derived from `PerThreadResult.duration_ns / PerThreadResult.total_reads` in `benches/skewmono-*.json`.

This is a **different measurement method** from the criterion tables at the top of this file, and the two intentionally differ. Here each iteration is a dependency-chained `now_as_u64()` read (one serialized read); criterion's tight loop lets the out-of-order CPU overlap successive reads, so the same `Instant::now()` amortizes lower there (e.g. sub-ns on Apple Silicon). These numbers also come from an earlier skew/monotonicity campaign on different instance sizes. Use the criterion tables for the head-to-head cross-crate comparison and this for the serialized single-read cost.

| Platform | `Instant` | `OrderedInstant` | `std::Instant` |
|---|---|---|---|
| Apple Silicon M1 (aarch64 macOS) | **3.5 ns** | 18.5 ns | 28.0 ns |
| Graviton 3 (aarch64 Linux, `c7g.4xlarge`) | **7.3 ns** | 26.1 ns | 37.7 ns |
| Intel Nitro VM (x86 Linux, `t3.medium`) | **14.4 ns** | 27.1 ns | 36.3 ns |
| Intel bare metal (x86 Linux, `m7i.metal-24xl`) | **8.5 ns** | 16.8 ns | 19.5 ns |
| AWS Lambda Firecracker (x86) | **21.2 ns** | 40.0 ns | 59.5 ns |
| Windows Server 2025 (x86) | **9.6 ns** | 25.4 ns | 36.3 ns |

**`Instant` is 1.5–8× faster than `std::time::Instant`** on every platform.

**`OrderedInstant` adds 8–19 ns** over `Instant` for the per-arch ordering barrier (`rdtscp` / `isb sy`), and is still 1.5–4× faster than `std` while adding the cross-thread guarantee std reaches only via the slow kernel path.

**No contention cliff.** The numbers above are uncontended per-thread costs, but `OrderedInstant` holds **no shared state** — its per-call cost is flat regardless of thread count. (An alternative atomic-based design, with a process-global `fetch_max`, is as correct but collapses under contention — stops scaling at 2 threads, degrades to 100s of ns. See `docs/WHY-NOT-AN-ATOMIC.md` for the head-to-head.)

## Synchronization-order monotonicity (contract validation)

Per-cell × per-clock synchronization-order contract violations under the load-then-now-then-check pattern. 0 = clock empirically honors synchronization-order monotonicity for the test window; non-zero = the bare read can be sampled before a prior Acquire-load (an ordering barrier is required). This is the data behind `OrderedInstant`'s cross-thread guarantee: bare `tach` and the fast comparison crates fail; `tach_ordered` passes at 0 everywhere; `std` passes (via its slow path). Pinned 2-socket data in `benches/ORDERED-VERIFICATION.md`.

| Clock | apple-silicon-m1 | c7g-4xlarge | t3-medium | m7i-metal-24xl | lambda-x86_64 | github-windows-x86_64 |
|---|---|---|---|---|---|---|
| `tach` | 17,366,168 | 1,827 | 42 | 9,629,641 | 13,351 | 1,678,700 |
| `tach_recal` | 30,586,874 | 1,944 | 18 | 16,905,112 | 3,946 | 1,681,768 |
| `tach_ordered` | 0 ✓ | 0 ✓ | 0 ✓ | 0 ✓ | 0 ✓ | 0 ✓ |
| `quanta` | 14,569,581 | 2,062 | 32 | 18,332,108 | 789 | 1,525,727 |
| `minstant` | 0 ✓ | 0 ✓ | 31 | 2,791,812 | 0 ✓ | 0 ✓ |
| `fastant` | 0 ✓ | 0 ✓ | 45 | 11,169,115 | 74 | 0 ✓ |
| `std` | 0 ✓ | 0 ✓ | 0 ✓ | 0 ✓ | 0 ✓ | 0 ✓ |

## Drift methodology

The drift table in the README compares `tach::Instant`, `quanta::Instant`, `minstant::Instant`, `fastant::Instant`, and `std::time::Instant` at 1-second, 1-minute, 1-hour, and 1-day measurement intervals. The numbers are *per-interval*, not uptime-cumulative — a 1-minute measurement made 5 seconds into the process has the same drift as one made 100 days in. Drift only shows up when comparing tach's `elapsed()` against an external reference clock; within a single process, all tach measurements are mutually consistent.

### Sources of drift

The default tach build derives the tick-to-nanosecond scaling once at startup, then uses that fixed scaling forever. Drift accumulates from two sources, depending on the platform:

- **Calibration error** (~500 ppm typical, eliminated on modern x86 by CPUID leaf 15h): the spin-loop calibration's ~10 ms window against `clock_gettime` bounds frequency error to roughly `timer_precision / window_length`. Older CPUs and virtualized environments where leaf 15h is zeroed fall back to this calibration path and inherit the error.
- **Crystal offset** (~50 ppm typical for commodity quartz, 2 ppm for TCXOs): the TSC's actual frequency differs from nominal by manufacturing tolerance, temperature, and aging. This is what kernel-corrected clocks (`std::Instant` on Linux/Windows) discipline against via NTP and continuous re-derivation against multiple clocksources.

After CPUID 15h removes the calibration component on Skylake+ Intel / Zen2+ AMD, only crystal drift remains (~50 ppm × interval). Without recalibration, that's ~3 ms per minute, ~180 ms per hour, ~4 s per day.

### How the table numbers were derived

- **`tach::Instant` (default) — ~50 µs/sec**: crystal drift only, after CPUID 15h. Multiplied out per interval (50 ppm × duration).
- **`tach::Instant` + `recalibrate-background` — ~1 µs/sec**: with 60-second recalibration, drift inside each window is bounded by `(crystal × window) + calibration_error`. The reported per-second number reflects the steady-state behavior after a recal cycle.
- **`tach::OrderedInstant`**: same backing scaling as `tach::Instant`, so identical drift profile. The `isb sy` / `rdtscp` barriers only constrain ordering, not the underlying tick value.
- **`quanta::Instant`, `minstant::Instant`, `fastant::Instant` — ~500 µs/sec**: these crates either don't use CPUID 15h or rely on the kernel's pre-calibrated TSC frequency without continuous correction. Numbers reflect their reported tolerance against `clock_gettime` over multi-second intervals.
- **`std::time::Instant` (Linux / Windows) — ~1 µs/sec**: kernel-corrected via vDSO scaling-factor updates plus NTP discipline. Reported drift is the typical no-NTP case; with active chrony / w32time, drift drops another 10× to sub-microsecond per minute.
- **`std::time::Instant` (macOS) — ~50 µs/sec**: reads `mach_timebase_info` (the exact per-die measured frequency) but does not run kernel-side per-tick correction the way Linux does. Drift matches tach's default on the same architecture.
- **`std::time::Instant` (Linux aarch64) — ~1 µs/sec**: reads `cntvct_el0` through the vDSO with an NTP-corrected scaling factor, same kernel-corrected path as Linux x86. Tach now matches this on aarch64 Linux by calibrating against `clock_gettime` at startup, which inherits the same vDSO scaling.

### Caveats

These are typical numbers, not guarantees. Per-system results depend on:

- **Crystal quality**: a TCXO can hold within 2 ppm; a cheap commodity crystal may exceed 100 ppm in a warm chassis.
- **Thermal environment**: drift roughly doubles per 10 °C swing from the calibration point.
- **NTP / chrony state**: the kernel-corrected rows assume no active discipline; with NTP, drift on those rows drops another order of magnitude.
- **Hypervisor TSC virtualization**: KVM, Xen, and Hyper-V can offset / scale the guest TSC in ways that change both calibration accuracy and effective drift.
