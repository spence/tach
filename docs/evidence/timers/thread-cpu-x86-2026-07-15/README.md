# `EVID-THREAD-CPU-X86` — x86 thread-CPU capability gate: perf task-clock mmap 33x on bare metal, cap_user_time absent on Nitro (2026-07-15)

**Status: FREEZE — capability gate, no same-target speed flip; `T-LINUX-X86` thread-CPU freezes to an availability-preferred capability gate in M2.**

The `OBJ-SIMPLIFY-TIMERS` §5.2 freeze probe for the `x86_64` Linux thread-CPU route (`T-LINUX-X86`):
is the production selection between the inline perf task-clock mmap read and the raw
`CLOCK_THREAD_CPUTIME_ID` syscall a compile-time pick, a runtime capability gate, or a measured
tournament? It is a **capability gate**. The perf mmap fast-path is gated on the runtime host
capability `cap_user_time`: present on bare metal (where the read is ~33x faster than the syscall and
self-check-correct), absent on Nitro VMs (where the syscall is used by necessity). No same-target
speed flip exists — perf wins whenever the capability is present; the syscall runs only when it is
not.

## Provenance

- Repo SHA: `7e99554` (adds `benches/probes/x86-thread-pmu.c` and arch-generalizes
  `benches/run-thread-pmu-aws.sh`). The probe reproduces tach's actual inline path
  (`src/arch/thread_cpu_linux_inline.rs`): `PERF_COUNT_SW_TASK_CLOCK` read through the perf mmap page
  via `CAP_USER_TIME` (`time_mult`/`time_shift` + a raw `rdtsc`) — **not** `cap_user_rdpmc`. Its
  `read_task_clock` seqlock + conversion is byte-identical to the aarch64 probe; only the counter read
  differs (`lfence;rdtsc;lfence` vs `mrs cntvct_el0`).
- Substrate #1 (capability-absent branch): AWS `c7i.large` (Nitro VM, Intel Xeon Platinum 8488C
  Sapphire Rapids), AL2023 kernel 6.12. Self-terminated (`i-0f9afc459ce324a60` reached `terminated`;
  no orphan).
- Substrate #2 (capability-present branch): AWS `c5n.metal` (bare metal, Intel Xeon Platinum 8124M
  Skylake @ 3.00 GHz), AL2023 kernel 6.12. Self-terminated (`i-06e9ffb26640dd05c` reached
  `terminated`; no orphan).
- Command surface: `INSTANCE_TYPE=<c7i.large|c5n.metal> benches/run-thread-pmu-aws.sh` — provisions,
  sets `perf_event_paranoid=-1`, compiles `benches/probes/x86-thread-pmu.c`, runs it, self-terminates.
- Artifacts: two raw run logs + a structured selection comparison (~2 KB total).

## Gates — verdicts

| Gate | Verdict | Evidence |
|---|---|---|
| `OBJ-SIMPLIFY-TIMERS.M1.G1` (row `T-LINUX-X86`) | 🟢 capability gate — perf mmap gated on `cap_user_time`; ~33x faster and self-check-correct when present, syscall fallback when absent; no speed flip | [`selection-comparison.json`](artifacts/selection-comparison.json) · [`raw-run-c5n-metal.log`](artifacts/raw-run-c5n-metal.log) · [`raw-run-c7i-large.log`](artifacts/raw-run-c7i-large.log) |

Per-environment result:

| Environment | `cap_user_time` | perf mmap ns/read | syscall ns/read | selected route |
|---|---|---|---|---|
| `c7i.large` (Nitro, Sapphire Rapids) | absent (`caps=0x2`) | unavailable | 230.583 | raw syscall (by necessity) |
| `c5n.metal` (bare metal, Skylake) | present (`caps=0x1a`) | **22.252** | 728.706 | perf mmap (~33x faster, self-check-correct) |

The `c5n.metal` self-check confirms the mmap read is correct, not merely fast: over a 50 ms busy
interval the perf task-clock delta (50010667 ns) matches the syscall delta (50010106 ns) to within
561 ns (0.001%).

## Findings resolved here

- **Row 2 (`T-LINUX-X86`) is verdicted: capability gate.** The selection is determined by the runtime
  host capability `cap_user_time` — a property of the host (VM vs. metal), not of the compile-time
  target — so it cannot be a compile-time `cfg` pick. When the capability is present the inline
  perf-mmap read is dramatically faster (22.25 ns vs 728.71 ns, ~33x) and correct; when absent the
  raw syscall is the only option. This is the same "availability-preferred" shape already frozen for
  the aarch64 route (`T-LINUX-A64`), now corroborated on x86 with its own profitability audit. In M2
  the route converts to a capability gate (perf-mmap when `cap_user_time`, else syscall), not a
  measured tournament — there is no frozen speed flip to justify runtime measurement.
- **Matrix correction.** The prior `T-LINUX-X86` note read "c7i demonstrates capability does not
  determine profitability," implying the capability was present but unprofitable. The direct probe
  shows the opposite: on Nitro c7i the inline capability (`cap_user_time`) is **absent**, so the
  syscall is used by necessity, and on bare metal where the capability is present the perf-mmap read
  is ~33x faster. Capability *does* determine the route, and when present it *is* profitable. The
  matrix row is corrected to the availability-preferred capability-gate wording.

## Open

- No metal `musl` or Android x86 environment was provisioned; those `T-LINUX-X86`-family targets share
  the same inline path and codegen and inherit this capability-gate verdict as a source-proven
  residual (no separate performance measurement).
- The bare-metal syscall path measured slower here (728.71 ns on Skylake) than on the Nitro VM
  (230.58 ns on Sapphire Rapids); this is a syscall-cost difference across microarchitecture/kernel
  entry paths and does not affect the verdict — the perf-mmap read beats the syscall on the same host
  by ~33x, which is the only comparison the route decision turns on.

## Reproduce

```
# capability-present branch (bare metal):
INSTANCE_TYPE=c5n.metal benches/run-thread-pmu-aws.sh
# capability-absent branch (Nitro VM):
INSTANCE_TYPE=c7i.large benches/run-thread-pmu-aws.sh
```

Each run prints the mmap `capabilities` word, whether `cap_user_time` makes the mmap read readable,
the perf-mmap and syscall medians, and a busy-interval self-check (perf task-clock delta must match
the syscall delta to be trusted).

## Raw artifacts

- [`artifacts/selection-comparison.json`](artifacts/selection-comparison.json) — structured
  per-environment capabilities, medians, self-check, provenance, and the freeze verdict.
- [`artifacts/raw-run-c5n-metal.log`](artifacts/raw-run-c5n-metal.log) — full bare-metal run:
  `cap_user_time=yes`, perf mmap 22.252 ns, syscall 728.706 ns, self-check matched.
- [`artifacts/raw-run-c7i-large.log`](artifacts/raw-run-c7i-large.log) — full Nitro-VM run:
  `cap_user_time=no`, syscall 230.583 ns.
