# Provider policy matrix

Status: PLAN v0.1, 2026-07-14. Read [`../STATUS.md`](../STATUS.md),
[`../README.md`](../README.md), and
[`../objectives/prove-timers.md`](../objectives/prove-timers.md) first.

## Context

This is the finite audit behind `OBJ-PROVE-TIMERS.M0.G2`. The optimized target verifier already
proves that all 24 advertised targets reach their implemented routes. This plan asks the different
question that the static verifier cannot answer by itself: does each production policy retain the
fastest eligible *complete public path* for its timer contract?

The table maps every target to a route family. A route family is audited once unless the ABI changes
its candidates. `measured` means the production initializer compares complete installable paths;
`fixed` means one eligible mechanism exists under the documented contract; `availability` means a
runtime capability decides whether the preferred mechanism exists, but does not compare latency.

## Complete target map

| Advertised target | `Instant` | `OrderedInstant` | `ThreadCpuInstant` |
|---|---|---|---|
| `x86_64-unknown-linux-gnu` | `W-LINUX-X86` | `O-LINUX-X86` | `T-LINUX-X86` |
| `x86_64-unknown-linux-musl` | `W-LINUX-X86` | `O-LINUX-X86` | `T-LINUX-X86` |
| `i686-unknown-linux-gnu` | `W-LINUX-X86` | `O-LINUX-X86` | `T-LINUX-X86` |
| `aarch64-unknown-linux-gnu` | `W-LINUX-A64` | `O-LINUX-A64` | `T-LINUX-A64` |
| `x86_64-linux-android` | `W-LINUX-X86` | `O-LINUX-X86` | `T-LINUX-X86` |
| `aarch64-linux-android` | `W-LINUX-A64` | `O-LINUX-A64` | `T-LINUX-A64-ANDROID` |
| `armv7-unknown-linux-gnueabihf` | `W-LINUX-ARM32` | `O-LINUX-ARM32` | `T-LINUX-ARM32` |
| `s390x-unknown-linux-gnu` | `W-LINUX-S390` | `O-LINUX-S390` | `T-LINUX-READ` |
| `riscv64gc-unknown-linux-gnu` | `W-LINUX-RISCV` | `O-LINUX-RISCV` | `T-LINUX-RISCV` |
| `loongarch64-unknown-linux-gnu` | `W-LINUX-LOONG` | `O-LINUX-LOONG` | `T-LINUX-READ` |
| `powerpc64-unknown-linux-gnu` | `W-LINUX-POWER` | `O-LINUX-POWER` | `T-LINUX-READ` |
| `powerpc64le-unknown-linux-gnu` | `W-LINUX-POWER` | `O-LINUX-POWER` | `T-LINUX-READ` |
| `x86_64-unknown-freebsd` | `W-FREEBSD-X86` | `O-FREEBSD-X86` | `T-FREEBSD-X86` |
| `x86_64-apple-darwin` | `W-MAC-X86` | `O-MAC-X86` | `T-MAC` |
| `aarch64-apple-darwin` | `W-MAC-A64` | `O-MAC-A64` | `T-MAC` |
| `x86_64-pc-windows-msvc` | `W-WINDOWS` | `O-WINDOWS-X86` | `T-WINDOWS` |
| `i686-pc-windows-msvc` | `W-WINDOWS` | `O-WINDOWS-X86` | `T-WINDOWS` |
| `aarch64-pc-windows-msvc` | `W-WINDOWS` | `O-WINDOWS-A64` | `T-WINDOWS` |
| `wasm32-unknown-unknown` | `W-JS` | `O-JS` | `T-JS` |
| `wasm32v1-none` | `W-JS` | `O-JS` | `T-JS` |
| `wasm32-unknown-emscripten` | `W-EMSCRIPTEN` | `O-EMSCRIPTEN` | `T-EMSCRIPTEN` |
| `wasm32-wasip1` | `W-WASI-P1` | `O-WASI-P1` | `T-WASI-P1` |
| `wasm32-wasip1-threads` | `W-WASI-P1` | `O-WASI-P1` | `T-WASI-P1` |
| `wasm32-wasip2` | `W-WASI-P2` | `O-WASI-P2` | `T-WASI-P2` |

## Wall route families

| Family | Eligible candidates | Production policy | Current verdict |
|---|---|---|---|
| `W-LINUX-X86` | kernel-eligible invariant TSC; `MONOTONIC`, `MONOTONIC_RAW`, and `BOOTTIME` through libc, direct versioned vDSO, and exact raw ABI | measured | implemented; c7i proves a host where the OS path beats an exposed hardware route |
| `O-LINUX-X86` | every eligible wall candidate compounded with eligible CPUID, LFENCE, MFENCE, RDTSCP, SERIALIZE, or OS-owned exception ordering | measured independently from `Instant` | implemented; public/exact parity passed retained c7i evidence |
| `W-LINUX-A64` | eligible CNTVCT; `MONOTONIC`, `MONOTONIC_RAW`, and `BOOTTIME` through libc, direct vDSO, and raw syscall | measured | implemented; trapped/emulated counter cost is included |
| `O-LINUX-A64` | ISB+CNTVCT or CNTVCTSS when eligible; ordered forms of every Linux clock candidate | measured independently from `Instant` | implemented |
| `W-LINUX-ARM32` | surviving direct-vDSO-backed CNTVCT; libc/direct-vDSO/time32/time64 syscall forms of the three Linux clocks | measured | source/codegen closed; native performance corroboration absent |
| `O-LINUX-ARM32` | ordered CNTVCT; DMB+ISB clock forms; OS-owned SVC syscall forms | measured independently from `Instant` | source/codegen closed; native performance corroboration absent |
| `W-LINUX-S390` | libc/direct-vDSO/raw-syscall forms of the three Linux clocks | measured | source/codegen closed; native performance corroboration absent |
| `O-LINUX-S390` | BCR-ordered libc/vDSO plus BCR or OS-owned SVC raw-syscall forms | measured independently from `Instant` | source/codegen closed; native performance corroboration absent |
| `W-LINUX-RISCV` | hwprobe-authorized TIME CSR; libc/direct-vDSO/raw-syscall forms of the three Linux clocks | measured | source/codegen closed; native performance corroboration absent |
| `O-LINUX-RISCV` | FENCE+TIME CSR; FENCE or OS-owned ECALL clock forms | measured independently from `Instant` | source/codegen closed; native performance corroboration absent |
| `W-LINUX-LOONG` | StableCounter; libc/direct-vDSO/raw-syscall forms of the three Linux clocks | measured | source/codegen closed; native performance corroboration absent |
| `O-LINUX-LOONG` | ordered StableCounter; synchronous-syscall-plus-counter and ordered clock forms | measured independently from `Instant` | source/codegen closed; native performance corroboration absent |
| `W-LINUX-POWER` | Time Base; libc/direct-vDSO/SC/SCV forms of the three Linux clocks | measured | source/codegen closed; native performance corroboration absent |
| `O-LINUX-POWER` | SYNC+Time Base; SYNC or OS-owned SC/SCV clock forms | measured independently from `Instant` | source/codegen closed; native performance corroboration absent |
| `W-FREEBSD-X86` | kernel-eligible TSC; direct `AT_TIMEKEEP`; libc or raw `CLOCK_MONOTONIC` | measured | **fails public/exact parity:** selected TSC public bracket 29.86 ns vs 28.24 ns exact |
| `O-FREEBSD-X86` | every FreeBSD wall candidate with eligible x86 or OS-owned ordering | measured independently from `Instant` | **fails public/exact parity:** selected LFENCE+TSC public now/bracket 23.95/49.36 ns vs 21.59/42.13 ns exact |
| `W-MAC-X86` | XNU Mach absolute-time path; invariant TSC only after XNU-compatible eligibility and scale checks | measured | retained Rosetta run selected TSC at about 10.5 ns |
| `O-MAC-X86` | Mach system path; XNU commpage LFENCE+RDTSC nanotime | measured independently from `Instant` | retained run keeps the reliable Mach timeline |
| `W-MAC-A64` | XNU-approved architectural counter | fixed | retained native run passes; bare quanta path is ineligible because it omits XNU wake correction |
| `O-MAC-A64` | eligible ordered architectural counter forms | measured | retained native run passes |
| `W-WINDOWS` | QPC and documented precise interrupt-time APIs whose backing contracts validate | measured | x86_64 native evidence retained at an older shipping closure; current closure needs corroboration |
| `O-WINDOWS-X86` | selected reliable Windows clock compounded with every eligible x86 barrier | measured independently from `Instant` | implementation/codegen closed; current native artifact missing |
| `O-WINDOWS-A64` | Windows-owned reliable clock after the required architectural fence | fixed compound path | codegen closed; native performance corroboration absent |
| `W-JS` | guarded `performance.now()`; Node `process.hrtime.bigint()` | measured at module initialization | Node and browser-negative boundaries retained |
| `O-JS` | worker-comparable epoch-correlated performance clock or synchronized Node timeline | measured at module initialization | Node and browser-negative boundaries retained |
| `W-EMSCRIPTEN` | guarded `performance.now()`; Node `process.hrtime.bigint()` import | measured at module initialization | Node host boundary retained |
| `O-EMSCRIPTEN` | local selected clock; pthread builds use epoch/synchronized host time plus shared atomic maximum | measured per build contract | ordinary and pthread boundaries retained |
| `W-WASI-P1` / `O-WASI-P1` | host monotonic clock | fixed host ABI | Node and Wasmtime boundaries retained |
| `W-WASI-P2` / `O-WASI-P2` | component-model host monotonic clock | fixed host ABI | Wasmtime boundary retained |

## Thread CPU route families

| Family | Eligible candidates | Production policy | Current verdict |
|---|---|---|---|
| `T-LINUX-X86` | perf task-clock mmap, persistent perf read, selected libc/raw `CLOCK_THREAD_CPUTIME_ID` | measured per OS thread | implemented; c7i demonstrates capability does not determine profitability |
| `T-LINUX-A64` | perf task-clock mmap when the complete handshake succeeds; raw thread-clock syscall fallback | availability, with benchmark-only profitability audit | four Graviton families showed perf 3–4.5× faster and no flip; logical non-AWS latency risk remains open |
| `T-LINUX-A64-ANDROID` | perf task-clock mmap/read and native thread clock | measured per OS thread | source/codegen closed; native performance corroboration absent |
| `T-LINUX-ARM32` / `T-LINUX-RISCV` | perf task-clock mmap/read and native thread clock | measured per OS thread | source/codegen closed; native performance corroboration absent |
| `T-LINUX-READ` | persistent perf task-clock read and native thread clock | measured per OS thread | source/codegen closed; native performance corroboration absent |
| `T-FREEBSD-X86` | libc and exact raw `CLOCK_THREAD_CPUTIME_ID` entries | measured | fresh native run selected raw syscall at about 131 ns vs libc 137 ns and passed public/exact parity |
| `T-MAC` | `clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)` | fixed native API | retained AArch64 and x86_64 evidence passes |
| `T-WINDOWS` | `GetThreadTimes`; QPC wall fallback on failure | availability fallback | x86_64 native evidence retained at an older shipping closure; current closure needs corroboration |
| `T-JS` | Node `process.threadCpuUsage()`; otherwise selected JS wall clock or frozen unavailable | availability fallback | Node and browser-negative boundaries retained |
| `T-EMSCRIPTEN` | Node `process.threadCpuUsage()` import; otherwise selected guarded wall clock | availability fallback | Node boundary retained |
| `T-WASI-P1` | host clock ID 3; monotonic wall fallback when rejected | availability fallback | positive Node and negative Wasmtime boundaries retained |
| `T-WASI-P2` | component-model monotonic wall clock | explicit fallback-only contract | Wasmtime boundary retained |

## Ordered work

1. Correct the target verifier so `T-LINUX-A64` is reported as availability-selected rather than
   runtime-measured.
2. Resolve the two fresh FreeBSD public/exact failures without weakening the material-tie rule.
3. Decide `T-LINUX-A64` from the evidence already collected: either retain the deterministic policy
   with an explicit scope/continuous audit, or reuse a bounded production profitability probe. Do
   not launch another broad Arm survey without a concrete discriminating environment.
4. Audit candidate completeness against primary OS/architecture sources, recording only a missing
   candidate or a changed eligibility rule as implementation work.
5. Regenerate the optimized 24-target report and admit M0.G2 only when no route family has an open
   selection-policy or public/exact defect.

## Verification

- `python3 benches/verify-target-providers.py` passes and emits a policy classification for all
  72 target/timer cells.
- The generated report contains zero `unknown`, zero unsupported capability-only profitability
  claims, and zero unaccounted eligible candidates.
- Every measured family has a selector test proving that its probe measures the same complete path
  installed for public `now()` and `elapsed()`.
- Every fixed or availability family cites the platform contract that makes alternative candidates
  ineligible or explains the fallback boundary.
- Fresh native public/exact evidence passes for every changed route family before M0.G2 closes.
