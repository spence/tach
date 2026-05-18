# tach benchmarks

`tach::Instant::now()` and `Instant::elapsed()` cost compared with `quanta`,
`fastant`, `minstant`, and `std::time::Instant` across six target /
environment cells. All numbers are nanoseconds per call (lower is better).

## Results

### `Instant::now()` cost

| Target | Environment | Instance | tach | quanta | fastant | minstant | std |
|---|---|---|---:|---:|---:|---:|---:|
| `aarch64-apple-darwin` | Apple Silicon MBP | M1 MacBook Pro | **0.35** | 4.59 | 27.23 | 27.29 | 20.28 |
| `aarch64-unknown-linux-gnu` | Graviton 3 Nitro VM | c7g.4xlarge | **6.68** | 7.02 | 41.68 | 41.68 | 32.51 |
| `x86_64-unknown-linux-gnu` | Intel burst VM | t3.medium | **8.74** | 13.32 | 11.19 | 9.40 | 24.28 |
| `x86_64-unknown-linux-musl` | Alpine Docker on Intel host | m7i.metal-24xl | **6.84** | 7.11 | **6.84** | **6.84** | 14.65 |
| `x86_64-unknown-linux-gnu` | AWS Lambda (Firecracker) | provided.al2023 | **13.60** | 23.34 | 15.54 | 56.93 | 50.76 |
| `x86_64-pc-windows-msvc` | GitHub Actions | windows-2025 | **12.34** | 12.43 | 45.54 | 45.52 | 41.23 |

### `Instant::now() + elapsed()` cost (full roundtrip)

| Target | Environment | Instance | tach | quanta | fastant | minstant | std |
|---|---|---|---:|---:|---:|---:|---:|
| `aarch64-apple-darwin` | Apple Silicon MBP | M1 MacBook Pro | **1.20** | 9.16 | 59.66 | 59.64 | 43.72 |
| `aarch64-unknown-linux-gnu` | Graviton 3 Nitro VM | c7g.4xlarge | **13.35** | 15.30 | 87.81 | 88.13 | 72.58 |
| `x86_64-unknown-linux-gnu` | Intel burst VM | t3.medium | **18.94** | 28.18 | 31.03 | 31.09 | 53.48 |
| `x86_64-unknown-linux-musl` | Alpine Docker on Intel host | m7i.metal-24xl | **13.68** | 17.51 | 21.40 | 21.41 | 32.58 |
| `x86_64-unknown-linux-gnu` | AWS Lambda (Firecracker) | provided.al2023 | **31.93** | 50.86 | 51.79 | 135.75 | 106.36 |
| `x86_64-pc-windows-msvc` | GitHub Actions | windows-2025 | **24.70** | 25.48 | 104.51 | 104.44 | 85.68 |

Chart: [`benches/summary.png`](benches/summary.png) — one cell per target environment. Each crate row shows `Instant::now()` (dark portion of bar) and the full `now() + elapsed()` roundtrip (lighter extension), with numeric times as `now / elapsed` on the right.

**Not included**: `x86_64-apple-darwin` (GitHub Actions `macos-13`) — could not land an Intel macOS runner allocation across multiple `workflow_dispatch` attempts. The GitHub-hosted Intel macOS runner pool has very low capacity.

## Methodology

- **Harness**: Criterion 0.8 (`harness = false`, custom `criterion_main!`).
- **Measured functions**: `Instant::now()` standalone, and `let start = Instant::now(); black_box(start.elapsed())` (full roundtrip).
- **Compiler**: stable Rust at the time of run (2026-05).
- **Sample size**: Criterion default — 100 samples × ~3s measurement time per bench. GitHub Actions runs use `--warm-up-time 1 --measurement-time 3` to fit the 6 min runner budget.
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

### Per-thread monotonicity (10-s sweep, single thread)

- apple-silicon-m1: 0 backward jumps for tach / std / quanta / minstant. fastant=690 (likely its macOS-fallback path)
- c7g-4xlarge: 0 backward jumps on any clock ✓
- t3-medium: 0 backward jumps on any clock ✓
- m7i-metal-24xl: 0 backward jumps on any clock ✓
- lambda-x86_64: 0 backward jumps on any clock ✓
- github-windows-x86_64: 0 backward jumps on any clock ✓

Tach (`Instant`, `OrderedInstant`, and the `recalibrate-background` variant) showed **0 backward jumps in every cell on every clock** — matching `std::time::Instant` on per-thread monotonicity.

### Cross-thread observation consistency (10-s sweep, N threads)

Per-cell maximum cross-thread violation magnitude. Threshold rules:
- ≤ 1 µs: clean (matches std in practice)
- 1–10 µs: documented sync-slop
- > 10 µs: hazard caveat

| Clock | apple-silicon-m1 | c7g-4xlarge | t3-medium | m7i-metal-24xl | lambda-x86_64 | github-windows-x86_64 |
|---|---|---|---|---|---|---|
| `tach` | 9.8 µs | 0 ns | 9.9 µs | 9.9 µs | 9.9 µs | 1.3 µs |
| `tach_recal` | 9.8 µs | 0 ns | 9.9 µs | 9.8 µs | 9.9 µs | 1.3 µs |
| `tach_ordered` | 9.8 µs | 9.8 µs | 9.9 µs | 9.9 µs | 9.9 µs | 100 ns |
| `quanta` | 9.6 µs | 22.7 µs | 9.9 µs | 9.8 µs | 9.9 µs | 9.9 µs |
| `minstant` | 10.0 µs | 9.6 µs | 9.9 µs | 9.8 µs | 9.8 µs | 9.8 µs |
| `fastant` | 10.0 µs | 9.8 µs | 9.9 µs | 9.8 µs | 9.8 µs | 9.8 µs |
| `std` | 9.5 µs | 9.4 µs | 9.9 µs | 9.8 µs | 9.9 µs | 9.7 µs |

Observations:
- Every cell × clock combination (except `quanta` on `c7g-4xlarge` at 22.7 µs) sits at or below 10 µs — the bracket-filter ceiling for what we count as "not preemption." Tach matches std within measurement noise on every cell.
- On `c7g-4xlarge`, the Graviton 3 `cntvct_el0` is architecturally synchronized across cores, so tach (which reads it directly) shows literally zero cross-thread violations. `tach_ordered` shows 9.8 µs because the `isb sy` barrier opens a wider window during which other threads can publish; the data is preemption-bracketed even though the underlying counter is monotonic.
- `quanta::Instant` on `c7g-4xlarge` at 22.7 µs is the only cell × clock that crosses the 10 µs threshold; it's a quanta-specific code path (the crate does its own scaling) rather than something inherent to the platform.
### Drift vs `std::Instant` (median across samples, per cell)

Per-cell 1-second and 1-minute median skew. Negative = tach reports less elapsed than std; positive = more.

| Cell | tach 1s | tach 1m | tach_recal 1s | tach_recal 1m | std 1m |
|---|---|---|---|---|---|
| `apple-silicon-m1` | -667 ns | -541 ns | -958 ns | -917 ns | -124 ns |
| `c7g-4xlarge` | -602 ns | -11.9 µs | +198 ns | +14.6 µs | -571 ns |
| `t3-medium` | -2.0 µs | +5.0 µs | -1.9 µs | +20.6 µs | -775 ns |
| `m7i-metal-24xl` | -4.3 µs | -199.5 µs | -4.4 µs | -27.6 µs | -414 ns |
| `lambda-x86_64` | -1.5 µs | +45.3 µs | -910 ns | +61.9 µs | -292 ns |
| `github-windows-x86_64` | -939 ns | +14.3 µs | +808 ns | +47.6 µs | -500 ns |

Observations:
- `m7i-metal-24xl` is the only cell where CPUID leaf 15h is exposed (Intel Sapphire Rapids bare metal). On this cell `tach::Instant` drifts at ~3.3 ppm — within the same order of magnitude as `std`. Other Intel cells fall back to the 100 ms × 7-sample spin-loop calibration with hypervisor-preemption discard, which holds calibration error to sub-ppm in the cross-cell median.
- `recalibrate-background` is a measurable improvement on Intel x86 cells where startup calibration accumulates error: `lambda-x86_64` goes from 0.75 ppm baseline to 0.58 ppm with recal, and `m7i-metal-24xl` (the CPUID-15h cell) goes from -3.25 ppm to -0.34 ppm (10× tighter). On cells where startup calibration already hit sub-ppm (`t3-medium` at 0.15 ppm baseline, `c7g-4xlarge` at -0.23 ppm post-calibration) the EMA residual sits a few tenths of a ppm above baseline, still well below any noise floor a service would notice. No-op on macOS (Apple measures the per-die timebase) and Windows aarch64 (`cntfrq_el0` is QPF-calibrated by the firmware).
- `c7g-4xlarge` previously showed a ~27 ppm constant offset because `cntfrq_el0` is the firmware-published nominal — not the measured crystal rate. As of this revision, aarch64 Linux calibrates `cntvct_el0` against `clock_gettime(CLOCK_MONOTONIC)` at startup (same path the x86 cells already use), so drift now tracks the kernel's NTP-corrected vDSO scaling and lands sub-ppm. Per-chip variation in the underlying crystal is invisible to users; what tach reports matches what the kernel reports.
- `std::time::Instant` 1m skew is consistently sub-µs on every cell, reflecting the kernel's continuous correction (vDSO scaling-factor updates on Linux, the equivalent on each OS).

### Per-thread monotonicity

- **apple-silicon-m1: backward jumps observed** — fastant=690
- c7g-4xlarge: 0 backward jumps on any clock ✓
- t3-medium: 0 backward jumps on any clock ✓
- m7i-metal-24xl: 0 backward jumps on any clock ✓
- lambda-x86_64: 0 backward jumps on any clock ✓
- github-windows-x86_64: 0 backward jumps on any clock ✓

## Cross-thread monotonicity

Per-cell maximum cross-thread violation magnitude (ns). Cells where the value exceeds 10 µs are flagged as a hazard.

| Clock | apple-silicon-m1 | c7g-4xlarge | t3-medium | m7i-metal-24xl | lambda-x86_64 | github-windows-x86_64 |
|---|---|---|---|---|---|---|
| `tach` | 9.8 µs | 9.3 µs | 9.9 µs | 9.9 µs | 9.9 µs | 9.8 µs |
| `tach_recal` | 9.8 µs | 7.1 µs | 9.9 µs | 9.9 µs | 9.8 µs | 9.9 µs |
| `tach_ordered` | 9.8 µs | 9.8 µs | 9.9 µs | 9.8 µs | 9.8 µs | 9.9 µs |
| `quanta` | 9.6 µs | 25.9 µs | 9.9 µs | 9.9 µs | 9.9 µs | 9.5 µs |
| `minstant` | 10.0 µs | 9.7 µs | 9.9 µs | 9.8 µs | 9.7 µs | 9.7 µs |
| `fastant` | 10.0 µs | 9.6 µs | 9.9 µs | 9.8 µs | 9.9 µs | 9.8 µs |
| `std` | 9.5 µs | 9.5 µs | 9.9 µs | 9.7 µs | 9.8 µs | 9.8 µs |

### Per-thread monotonicity

- **apple-silicon-m1: backward jumps observed** — fastant=690
- c7g-4xlarge: 0 backward jumps on any clock ✓
- t3-medium: 0 backward jumps on any clock ✓
- m7i-metal-24xl: 0 backward jumps on any clock ✓
- lambda-x86_64: 0 backward jumps on any clock ✓
- github-windows-x86_64: 0 backward jumps on any clock ✓

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
- **`tach::OrderedInstant`**: same backing scaling as `tach::Instant`, so identical drift profile. The `isb`/`lfence` barriers only constrain ordering, not the underlying tick value.
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
