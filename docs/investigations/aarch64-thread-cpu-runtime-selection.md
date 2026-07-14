# AArch64 thread-CPU runtime-selection justification

Status: evidence complete for the tested AWS Arm families; no cost-driven provider flip observed.

## Question

Does default `aarch64-unknown-linux-gnu` need to benchmark the complete perf task-clock mmap,
perf fd-read, and native `CLOCK_THREAD_CPUTIME_ID` paths during each thread's first read? The
profitability tournament is justified only if two environments running the same compiled target
can expose both production providers and select different winners by measured cost. A host where
perf is unavailable proves the need for capability fallback, not cost measurement.

## Frozen probe

- tach source: `1182d7a5e73dece6e1d2b7c8f5cea35f51d40778`
- target: `aarch64-unknown-linux-gnu`
- features: `bench-internal,thread-cpu-inline`
- build: `cargo zigbuild --release --target aarch64-unknown-linux-gnu`
- binary SHA-256: `3c2ccc870999cbf4a49f62b6ae2c0bb8bc7306fad8848ce765cd524f413bfe49`
- probe manifest SHA-256: `4eb6a31482ccce0ea5fca46a1ac0992d2d3839bdc3ce8f039228a690504d36a9`
- probe source SHA-256: `cbf4c642c0f16032865dae2d390f50c9b89be8a4438b2f6e884b7e73ad93c0f3`
- AMI: `ami-0b18bd762aa8a2514`, Amazon Linux 2023, kernel
  `6.12.94-123.180.amzn2023.aarch64`
- policy before every read: `kernel.perf_event_paranoid=-1`, `kernel.perf_user_access=1`
- selector rule: 4,096 reads per alternating batch; a challenger must win by more than
  `max(1 ns/read, 5%)` in at least eight of nine batches.

The same binary initialized `ThreadCpuInstant`, printed the public provider and read-cost hint,
and serialized `tach::bench::thread_cpu_perf_path_evidence()` without running Criterion.

## Results

Batch medians are total nanoseconds for 4,096 public-path reads. Per-read values are the batch
median divided by 4,096.

| Instance | CPU family | perf mmap | native POSIX | perf fd read | Selected |
|---|---|---:|---:|---:|---|
| `c6g.large` | Graviton 2 / Neoverse N1 | 260,841 / 63.68 ns | 1,000,853 / 244.35 ns | 1,495,983 / 365.23 ns | perf mmap |
| `c7g.large` | Graviton 3 | 232,337 / 56.72 ns | 1,040,621 / 254.06 ns | 1,557,326 / 380.21 ns | perf mmap |
| `c8g.large` | Graviton 4 | 227,043 / 55.43 ns | 764,374 / 186.61 ns | 1,188,006 / 290.04 ns | perf mmap |
| `t4g.small` | Graviton 2 / Neoverse N1, burstable | 260,882 / 63.69 ns | 1,020,168 / 249.06 ns | 1,501,342 / 366.54 ns | perf mmap |

Raw alternating batches:

```text
c6g mmap   [260989, 260874, 260841, 260841, 260866, 260833, 260824, 260850, 260841]
c6g native [1015015, 999056, 999319, 1004299, 1000853, 1004783, 999319, 1002388, 1000123]
c6g read   [1498208, 1497157, 1499290, 1493711, 1500857, 1494876, 1487525, 1493308, 1495983]

c7g mmap   [232337, 232677, 231666, 232693, 230635, 232324, 230905, 233698, 235254]
c7g native [1025116, 1027440, 1041289, 1041096, 1047480, 1047359, 1040009, 1040621, 1040496]
c7g read   [1551725, 1557940, 1555559, 1561602, 1563908, 1553322, 1551092, 1557892, 1557326]

c8g mmap   [227344, 227989, 226596, 226325, 227043, 226701, 226633, 227630, 228189]
c8g native [764486, 760790, 762056, 760627, 764847, 764374, 773442, 764015, 765062]
c8g read   [1188635, 1191574, 1190494, 1188006, 1185137, 1184956, 1186560, 1185852, 1190987]

t4g mmap   [260980, 260891, 267938, 262942, 260882, 260849, 260849, 260874, 260833]
t4g native [1015697, 1019274, 1033551, 1034059, 1012374, 1024534, 1021907, 1020168, 1019914]
t4g read   [1493645, 1487368, 1501342, 1509538, 1503910, 1495023, 1503786, 1494138, 1511467]
```

An attempted `a1.large` observation (`i-0e5d940caf0fb66ca`) never passed EC2's instance-status
initialization and produced no selector output. It was terminated, and its key pair was deleted.
It is not counted as evidence.

## Finding

Every observed AWS Arm environment with both production providers eligible selected perf mmap by
a wide margin. This survey does **not** provide the same-target cost-driven selection flip required
to justify the aarch64 profitability tournament. It supports a simpler policy for the tested
scope: use perf mmap when its complete metadata/counter capability checks pass; otherwise use the
native thread clock. Runtime capability detection and failure fallback remain necessary.

The separate PMCCNTR experiment is not counterevidence. It compared a pinned hardware-cycle event
that is not a production candidate: virtualization made PMCCNTR about 23 times slower on
`c7g.large`, while perf task-clock mmap remained the winner in every production-path observation.

## Implemented policy and live verification

Commit `e7cb1d0` replaces the Linux AArch64 provider-cost tournament with the supported policy:

- prefer perf task-clock mmap when the event, mapping, seqlock conversion metadata, and architectural
  counter are all usable;
- use the selected native `CLOCK_THREAD_CPUTIME_ID` entry when that complete capability is absent or
  an inline mmap read fails;
- retain the paired perf-mmap, perf-read, and POSIX samples only in `bench-internal` builds as a
  release-evidence audit. Those samples fail validation if the deterministic policy loses, but they
  do not select the production provider.

A fresh probe built from that implementation for `aarch64-unknown-linux-gnu` had SHA-256
`c4759fd361ef3c2cdb16192e32b9041c5a222ab3bf9d58e616b5adb19ca9ad5c`. The same binary ran on
`c7g.large` instance `i-0aee7ef7cbf6ff755` under the AL2023 image and kernel recorded above:

| Fleet controls | Public provider | Cost hint | perf mmap | native POSIX | perf fd read |
|---|---|---|---:|---:|---:|
| `perf_event_paranoid=-1`, `perf_user_access=1` | `LinuxPerfMmap` | `Inline` | 234,008 / 57.13 ns | 1,027,518 / 250.86 ns | 1,559,709 / 380.79 ns |
| `perf_event_paranoid=4`, `perf_user_access=0` | `PosixThreadCpuClock` | `SystemCall` | unavailable | selected | unavailable |

Each timing cell is the median total for 4,096 reads followed by its per-read value. The disabled
case also returned no perf-event evidence, proving that denial selects the native thread-CPU domain
rather than silently substituting wall time. The instance was terminated immediately after both
branches ran; ephemeral key pair `tach-capability-20260713-1` and its local private key were deleted.

## Limits

The survey covers AWS Graviton 2–4 and a burstable Graviton 2 host, not every non-AWS hypervisor.
Absence of an observed flip cannot prove that no future kernel or virtual machine can make an
eligible perf mapping slower. It does establish that the current repository has no empirical
same-target justification for measuring profitability on aarch64.
