# Provider policy matrix

Status: COMPLETE v0.2, 2026-07-14 — `OBJ-PROVE-TIMERS.M0.G2` 🟢 at evidence SHA
`9a3c48f`; RE-AUDITED v0.3, 2026-07-15 under ADR-0005 for `OBJ-SIMPLIFY-TIMERS.M1` — the Apple
`Instant` wake-correction exclusion dissolved as an inadmissible Class-3 inferred contract and bare
`CNTVCT_EL0` was re-admitted and selected (`def4b87`, `EVID-APPLE-BARE-CNTVCT`); see Closure note 6.
Read [`../STATUS.md`](../STATUS.md),
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
| `x86_64-pc-windows-msvc` | `W-WINDOWS` | `O-WINDOWS` | `T-WINDOWS` |
| `i686-pc-windows-msvc` | `W-WINDOWS` | `O-WINDOWS` | `T-WINDOWS` |
| `aarch64-pc-windows-msvc` | `W-WINDOWS` | `O-WINDOWS` | `T-WINDOWS` |
| `wasm32-unknown-unknown` | `W-JS` | `O-JS` | `T-JS` |
| `wasm32v1-none` | `W-JS` | `O-JS` | `T-JS` |
| `wasm32-unknown-emscripten` | `W-EMSCRIPTEN` | `O-EMSCRIPTEN` | `T-EMSCRIPTEN` |
| `wasm32-wasip1` | `W-WASI-P1` | `O-WASI-P1` | `T-WASI-P1` |
| `wasm32-wasip1-threads` | `W-WASI-P1` | `O-WASI-P1` | `T-WASI-P1` |
| `wasm32-wasip2` | `W-WASI-P2` | `O-WASI-P2` | `T-WASI-P2` |

## Wall route families

| Family | Eligible candidates | Production policy | Current verdict |
|---|---|---|---|
| `W-LINUX-X86` | kernel-eligible invariant TSC; `MONOTONIC`, `MONOTONIC_RAW`, and `BOOTTIME` through libc, direct versioned vDSO, and exact raw ABI | measured → fixed (M2) | c7i (Intel) and c7a (AMD Zen4) both select `linux_kernel_eligible_tsc` — no same-target flip (`EVID-AMD-FLIP-LINUX-X86-2026-07-15`), so §5.2 freezes this family to a compile-time pick in M2 |
| `O-LINUX-X86` | every eligible wall candidate compounded with eligible CPUID, LFENCE, MFENCE, RDTSCP, SERIALIZE, or OS-owned exception ordering | measured → fixed (M2) | public/exact parity passed retained c7i evidence; c7i and c7a both select `linux_kernel_eligible_tsc_x86_lfence_rdtsc` — no flip (`EVID-AMD-FLIP-LINUX-X86-2026-07-15`) → fixed in M2 |
| `W-LINUX-A64` | eligible CNTVCT; `MONOTONIC`, `MONOTONIC_RAW`, and `BOOTTIME` through libc, direct vDSO, and raw syscall | measured → fixed (M2) | c7g (Graviton3) and c8g (Graviton4) both select `aarch64_cntvct` — no same-target flip (`EVID-GRAVITON4-FLIP-LINUX-A64`), so §5.2 freezes this family to a compile-time pick in M2; trapped/emulated counter cost is included |
| `O-LINUX-A64` | ISB+CNTVCT or CNTVCTSS when eligible; ordered forms of every Linux clock candidate | measured → fixed (M2) | c7g and c8g both select `aarch64_isb_cntvct` — no flip (`EVID-GRAVITON4-FLIP-LINUX-A64`) → fixed in M2 |
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
| `W-FREEBSD-X86` | kernel-eligible TSC; direct `AT_TIMEKEEP`; libc or raw `CLOCK_MONOTONIC` | measured → fixed (M2) | native selector chose TSC (public 13.83/28.74 ns beats every usable public reference; static 13.30/27.13 ns diagnostic lower bound); c7i (Intel) and c7a (AMD Zen4) both select `freebsd_kernel_eligible_tsc` — no same-target flip (`EVID-AMD-FLIP-FREEBSD-X86`) → fixed in M2 |
| `O-FREEBSD-X86` | every FreeBSD wall candidate with eligible x86 or OS-owned ordering | measured → fixed (M2) | native selector chose LFENCE+TSC (public 22.37/42.92 ns beats `std` 31.41/65.36 ns; static 20.65/39.89 ns diagnostic lower bound); c7i and c7a both select `freebsd_kernel_eligible_tsc_x86_lfence_rdtsc` — no flip (`EVID-AMD-FLIP-FREEBSD-X86`) → fixed in M2 |
| `W-MAC-X86` | XNU Mach absolute-time path; invariant TSC only after XNU-compatible eligibility and scale checks | measured | native Intel corroboration passes at `68dc201`; selected invariant TSC beats every eligible wall reference |
| `O-MAC-X86` | Mach system path; XNU commpage LFENCE+RDTSC nanotime | measured independently from `Instant` | native Intel corroboration passes at `68dc201`; the public route beats every eligible wall reference and its paired exact-route probe passes |
| `W-MAC-A64` | bare `CNTVCT_EL0`; XNU Mach absolute/continuous timelines | measured → fixed (M2) | Re-audited under ADR-0005: the wake-correction exclusion was an inadmissible Class-3 inferred contract and dissolved. Bare `CNTVCT_EL0` re-admitted (`def4b87`) and selected over the XNU routes on both M1 Max and M4 Pro with no flip — 0 violations in ~2.8e9 paired reads, wall-rate 0.99997, bare read 0.33/0.44 ns vs XNU 5.4/3.8 ns; public `now()` 0.93 ns beats eligible reference quanta 3.30 ns (`EVID-APPLE-BARE-CNTVCT`). Whole-system suspend is platform-defined and documented (ADR-0005); its measurement (§5.1 item d, owner-coordinated) is still pending. |
| `O-MAC-A64` | eligible ordered architectural counter forms (barriered `isb`+`CNTVCT`) | measured | Unchanged by the re-audit: an unbarriered bare read is never synchronization-ordered (ADR-0005), so bare `CNTVCT_EL0` is not an ordered candidate; the barriered bare form measured ~2× slower than the selected self-synchronizing route on both machines (`EVID-APPLE-BARE-CNTVCT`), so the prior ordered pick stands. |
| `W-WINDOWS` | QPC and available precise interrupt-time APIs | measured | native Windows x86_64 corroboration passes at `68dc201`; selected QPC is faster than `std` for both public wall APIs |
| `O-WINDOWS` | the same Windows-owned APIs through their opaque call boundaries; raw TSC/CNTVCT and redundant pre-call fences are ineligible (class 1: Windows documents no userspace TSC invariance and designates QPC as the monotonic source — ADR-0005, upheld) | measured independently from `Instant` | 24-target optimized codegen closes at `5a2eb05`; focused proof found zero inversions for bare QPC in 945,307,669 reads, and native Windows x86_64 corroboration passes at `68dc201` |
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
| `T-LINUX-A64` | perf task-clock mmap when the complete handshake succeeds; raw thread-clock syscall fallback | availability-preferred, with benchmark-only profitability audit | one frozen binary showed perf 3–4.5× faster on c6g/c7g/c8g/t4g with no flip; any future audit loss reopens runtime selection |
| `T-LINUX-A64-ANDROID` | perf task-clock mmap/read and native thread clock | measured per OS thread | source/codegen closed; native performance corroboration absent |
| `T-LINUX-ARM32` / `T-LINUX-RISCV` | perf task-clock mmap/read and native thread clock | measured per OS thread | source/codegen closed; native performance corroboration absent |
| `T-LINUX-READ` | persistent perf task-clock read and native thread clock | measured per OS thread | source/codegen closed; native performance corroboration absent |
| `T-FREEBSD-X86` | libc and exact raw `CLOCK_THREAD_CPUTIME_ID` entries | measured | native run selected raw syscall; public 125.20/249.71 ns matches raw 124.87/249.65 ns and beats libc 129.56/262.51 ns |
| `T-MAC` | `clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)` | fixed native API | native AArch64 and Intel x86_64 corroboration pass; Intel paired public/exact probes have zero decisive losses |
| `T-WINDOWS` | `GetThreadTimes`; QPC wall fallback on failure | availability fallback | native Windows x86_64 corroboration passes at `68dc201`; paired public/exact probes have zero decisive losses |
| `T-JS` | Node `process.threadCpuUsage()`; otherwise selected JS wall clock or frozen unavailable | availability fallback | Node and browser-negative boundaries retained |
| `T-EMSCRIPTEN` | Node `process.threadCpuUsage()` import; otherwise selected guarded wall clock | availability fallback | Node boundary retained |
| `T-WASI-P1` | host clock ID 3; monotonic wall fallback when rejected | availability fallback | positive Node and negative Wasmtime boundaries retained |
| `T-WASI-P2` | component-model monotonic wall clock | explicit fallback-only contract | Wasmtime boundary retained |

## Closure

1. The target verifier classifies `T-LINUX-A64` as availability-preferred with an audit rather than
   falsely claiming a production profitability tournament.
2. FreeBSD installs the selected scale and complete ordered barrier identity. Public admission uses
   caller-usable reference clocks; private statically bound functions remain reported as optimization
   lower bounds.
3. `T-LINUX-A64` is retained under its four-family same-binary survey and benchmark-only audit. An
   observed audit loss or concrete discriminating environment reopens runtime selection.
4. The optimized report accounts for every target/timer policy and eligible candidate family with
   zero unknown policies.
5. `OBJ-PROVE-TIMERS.M0.G2` is admitted by
   [`EVID-PROVIDER-POLICY-2026-07-14`](../evidence/timers/provider-policy-closure-2026-07-14/README.md).
6. Re-audit (`OBJ-SIMPLIFY-TIMERS.M1`, 2026-07-15, ADR-0005). Every "ineligible" footnote was
   re-checked against the two admissible evidence classes. `W-MAC-A64`: the bare-`CNTVCT_EL0`
   exclusion cited neither class — it was a Class-3 inferred "must apply XNU wake correction"
   requirement the published `Instant` contract never made — so it dissolved into candidacy, and
   bare `CNTVCT_EL0` was re-admitted and selected on two environments (`EVID-APPLE-BARE-CNTVCT`).
   `O-WINDOWS`: the raw-TSC/redundant-fence exclusion is upheld under class 1 (Windows designates
   QPC as the monotonic source and documents no userspace TSC invariance). Windows bare TSC,
   `QueryThreadCycleTime`, and deliberately coarsened clocks stay excluded under class 1 (ADR-0005).
   Open before the family verdicts are final: the Apple suspend/wake semantic measurement (§5.1
   item d) and the enumerated same-target second-environment flip probes (`OBJ-SIMPLIFY-TIMERS.M1`
   §5.2), one of which is blocked on `ESC-AMD-FLIP-PROBE-TOOLING`.

## Verification

- `python3 benches/verify-target-providers.py` passes and emits a policy classification for all
  72 target/timer cells.
- The generated report contains zero `unknown`, zero unaccounted eligible candidates, and no
  availability-preferred policy without a retained material no-reversal survey and audit path.
- Every measured family has a selector test proving that its probe measures the same complete path
  installed for public `now()` and `elapsed()`.
- Every fixed or availability family cites the platform contract that makes alternative candidates
  ineligible or explains the fallback boundary.
- Fresh native evidence for a changed runtime-dispatch family must reproduce its selector and beat
  all caller-usable public references. Statically bound exact functions remain measured and visible,
  but are not misrepresented as alternative public APIs.
