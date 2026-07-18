# M4 step 2 — Windows x86 raw-TSC `Instant` provider (implementation spec)

Adopted from the ADR-0007 analysis. **Windows `Instant` (x86_64/x86) moves QPC → raw TSC; Windows
`OrderedInstant` stays QPC; aarch64-windows `Instant` STAYS QPC** (RDTSC is x86-only — a STATED
policy fork, not a silent gap). The real dispatch seam is `src/arch/direct.rs`, not `fallback.rs`.

## Design
- **New file `src/arch/windows_x86_wall.rs`**, cfg `all(target_os="windows", any(target_arch="x86_64",
  target_arch="x86"))`. A new eligibility-state-machine + calibration provider mirroring
  `linux_x86_wall.rs`, NOT an extension of `fallback.rs` (which is bare-OS-call only).
- **Read:** `core::arch::{x86_64,x86}::_rdtsc()` — self-contained; do NOT use `super::x86_64::rdtsc()`
  (that module is `not(windows)`-gated, uncompiled on Windows). SAFETY comment per `x86_64.rs:159-163`.
- **Eligibility gate (KEEP — readability/rate-stability, NOT cross-core):** CPUID leaf 1 EDX[4] (TSC
  exists) + CPUID `8000_0007h` EDX[8] (invariant TSC = rate-stable under P/C-states — same gate
  Linux-x86/Apple/FreeBSD require; ADR-0007 relaxes *cross-core*, not this). No runtime-denial probe
  (Windows has no `PR_SET_TSC`; CR4.TSD never set for ring-3 → RDTSC always legal → "no crash" is
  structural). Cache in an `AtomicU8` (mirror `linux_x86_wall.rs:101` `INSTANT_PROVIDER`). Ineligible
  → fall back to QPC (`fallback::qpc_ticks`).
- **Calibration (HIGHEST correctness risk — new, bespoke):** `crate::calibration` is Unix-only
  (`calibration.rs:9-12` `ref_ns`→`clock_monotonic`, uncompiled on Windows). Port the pattern from
  `linux_x86_wall.rs:518-557` (`calibrate_tsc_frequency`): 7 samples × 100 ms windows, reject overruns
  >1.5×, median, single-unfiltered fallback — but the wall reference is QPC (`fallback::qpc_ticks`
  scaled by `fallback::qpc_frequency`), not `clock_gettime`. Cache the Q32 scale in an `AtomicU64`.
- **Provider name:** `windows_tsc` (decide ONCE — threads into `EXPECTED_WALL_PICKS` + route-proof + tests).

## The seam (surgical — Ordered untouched by construction)
- `direct.rs:19-27` (Instant `ticks()`): split the `any(x86_64,x86,aarch64)` cfg → x86_64/x86 call
  `super::windows_x86_wall::ticks()`; aarch64 stays `super::fallback::windows_ticks()`.
- `direct.rs:192-200` + `:373-381` (Ordered): **NO CHANGE** (all arches stay QPC).
- `mod.rs:558-564` (`read_local_frequency`): split by arch → x86 → `windows_x86_wall::instant_frequency()`.
- `mod.rs:732-736` (`read_ordered_frequency`): **NO CHANGE.**
- `mod.rs:805-848` + `recalibration_domains()` (~850): add a Windows-x86 arm → `(instant_uses_tsc(), false)`
  (recal local scale only when TSC selected; ordered NEVER recal). Windows recal was a no-op; now does
  real work. Needs `windows_x86_wall::recalibrate_instant_scale()`.
- `mod.rs` module list (~22-52): add the cfg-gated `pub mod windows_x86_wall;`.

## Invariants (ADR-0007 — MUST preserve)
- elapsed saturates to zero: FREE (`instant.rs:63-67` saturating_sub, provider-agnostic).
- no crash: structural (RDTSC ring-3-legal; CPUID gate → graceful QPC degrade).
- **OrderedInstant UNCHANGED:** by construction — touch NO ordered-reachable fn (`ticks_ordered`,
  `ticks_ordered_unordered`, `read_ordered_frequency`, `ordered_nanos_per_tick_q32`, ordered recal slot).

## Blast radius (must update — else CI hard-fails)
- **`benches/verify-target-providers.py:716-731`** (`instant_route` Windows branch): TODAY requires
  `@QueryPerformanceCounter` + FORBIDS `llvm.x86.rdtsc` → HARD-FAILS CI. Split by arch: x86_64/i686 →
  require `llvm.x86.rdtsc`, drop from forbidden; aarch64-windows → unchanged (QPC). The
  `x86_64-unknown-freebsd` branch just below is a ready template. `ordered_instant_route:1003-1013` = NO CHANGE.
- **`benches/speed_evidence.py:412`** `EXPECTED_WALL_PICKS["windows"]["instant"]={windows_qpc}` → add
  `windows_tsc` (keep `windows_qpc` as the eligibility-fallback name, like linux-x86 lists both). Line 414 (ordered) = NO CHANGE.
- **`benches/instant.rs`** (~1510-1533, ~1735-1775, ~2373-2383): Windows Criterion group +
  `write_windows_wall_selection()` emitter + the exact-parity probe `measure_wall_public_exact(||
  Instant::now(), || windows_qpc_ticks())` — swap the INSTANT side to the TSC read; the neighboring
  ORDERED probe stays `windows_qpc_ticks()`.
- **`src/bench.rs`** (~3284-3305): add `windows_tsc_ticks()`/`_delta_to_duration()`/`_nanos_per_tick_q32()`
  bench-internal exports paralleling `windows_qpc_*`.
- **`src/arch/fallback.rs`**: `windows_ticks()`, `bench_instant_provider()` (→`"windows_qpc"`),
  `bench_instant_qpc()` — repurpose as the QPC-fallback path OR remove if dead (grep callers post-change).
- **Tests:** `fallback.rs:235-252` `selected_domains_are_monotonic_and_share_the_qpc_frequency` asserts
  `instant_frequency()==qpc_frequency()` → FALSE now; rewrite (the ONLY Rust unit test on the Windows
  Instant path). `tests/instant.rs:160-163` comment stale (bounds already tolerate).
- **Rustdoc:** `src/instant.rs:10-12` ("OS-vetted QPC timeline on Windows" — false for x86), `:120-124`
  (recalibrate no-op-on-Windows — now only aarch64/ordered).
- **Docs:** `docs/plans/provider-policy-matrix.md:76-77` (W-WINDOWS x86 fork, mirror W-MAC-X86/A64),
  `docs/README.md:67-68` + `:167` (Instant column arch-split), `BENCHMARKS.md:44-59` (fresh Windows
  numbers, not prose; footnote distinguishes Instant-eligible from Ordered-excluded),
  `docs/plans/simplify-and-verify.md:96,190,203` (pointer to the relaxation).

## Verify (exec subagent, in a worktree — compile-only; runtime is CI)
- `rustup target add x86_64-pc-windows-msvc i686-pc-windows-msvc`; `cargo check --target
  x86_64-pc-windows-msvc` + i686, on BOTH default and `--no-default-features`.
- `cargo check` cross-check (aarch64-windows MUST still compile with QPC unchanged; linux/apple/freebsd
  unaffected). `cargo test --lib` on host (Windows-cfg code won't run, but the fallback.rs test rewrite
  must compile). Runtime CPUID gate + calibration are CI-only (no local Windows) — the re-measure verifies.
- Provider name `windows_tsc` consistent across `EXPECTED_WALL_PICKS` + route-proof + bench + emitter.

## NOT this change
- aarch64-windows bare-CNTVCT Instant (a future objective). Any OrderedInstant change.
