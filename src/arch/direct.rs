//! Compile-time Instant clock dispatch.
//!
//! On every supported target, `Instant::now()` reads the canonical wall-clock-rate
//! counter for that target architecture: RDTSC on x86 / x86_64, CNTVCT_EL0 on aarch64,
//! rdtime on riscv64 / loongarch64. On unsupported architectures, the platform
//! monotonic clock is used.

#[cfg(target_arch = "x86_64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::x86_64::rdtsc()
}

#[cfg(target_arch = "x86")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::x86::rdtsc()
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::aarch64::cntvct()
}

#[cfg(target_arch = "riscv64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::riscv64::rdtime()
}

#[cfg(target_arch = "loongarch64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::loongarch64::rdtime()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::wasm::ticks()
}

#[cfg(not(any(
  target_arch = "x86_64",
  target_arch = "x86",
  target_arch = "aarch64",
  target_arch = "riscv64",
  target_arch = "loongarch64",
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
)))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  #[cfg(target_os = "macos")]
  {
    super::fallback::mach_time()
  }
  #[cfg(all(unix, not(target_os = "macos")))]
  {
    super::fallback::clock_monotonic()
  }
  #[cfg(target_os = "wasi")]
  {
    super::fallback::wasi_clock_monotonic()
  }
  #[cfg(not(any(unix, target_os = "macos", target_os = "wasi")))]
  {
    panic!("tach: no monotonic clock source on this target")
  }
}

// ── Ordered counter reads ────────────────────────────────────────────────
// Same dispatch as `ticks()` but each architectural path emits a barrier
// before the counter read so the timestamp cannot be sampled before a prior
// `Acquire`-or-stronger observation. Fallback paths (`mach_absolute_time`,
// `clock_gettime`, `clock_time_get`, `Performance.now()`) cross a runtime /
// kernel / JS boundary that already serializes the call site, so they reuse
// the unordered helpers unchanged.

#[cfg(target_arch = "x86_64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_fenced() -> u64 {
  super::x86_64::rdtsc_fenced()
}

#[cfg(target_arch = "x86")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_fenced() -> u64 {
  super::x86::rdtsc_fenced()
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_fenced() -> u64 {
  super::aarch64::cntvct_fenced()
}

#[cfg(target_arch = "riscv64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_fenced() -> u64 {
  super::riscv64::rdtime_fenced()
}

#[cfg(target_arch = "loongarch64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_fenced() -> u64 {
  super::loongarch64::rdtime_fenced()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_fenced() -> u64 {
  super::wasm::ticks()
}

#[cfg(not(any(
  target_arch = "x86_64",
  target_arch = "x86",
  target_arch = "aarch64",
  target_arch = "riscv64",
  target_arch = "loongarch64",
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
)))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_fenced() -> u64 {
  #[cfg(target_os = "macos")]
  {
    super::fallback::mach_time()
  }
  #[cfg(all(unix, not(target_os = "macos")))]
  {
    super::fallback::clock_monotonic()
  }
  #[cfg(target_os = "wasi")]
  {
    super::fallback::wasi_clock_monotonic()
  }
  #[cfg(not(any(unix, target_os = "macos", target_os = "wasi")))]
  {
    panic!("tach: no monotonic clock source on this target")
  }
}

// ── Synchronization-order monotonic reads ─────────────────────────────────
// Empirical validation (`measure_synchronization_order` across 6 production
// cells, ~30s × 16 threads each, captured in `benches/skewmono-*.json` under
// `synchronization_order.total_violations`) demonstrates:
//
//   - **Bare arch counter reads (RDTSC, cntvct_el0) FAIL synchronization-order
//     monotonicity on every multi-threaded platform tested.** Rates vary —
//     sub-ppm on Nitro VMs (t3.medium), single-digit % on bare metal (M1,
//     m7i bare-metal) — but every multi-threaded platform shows non-zero
//     contract violations. x86 per-core TSC is firmware-synced but not
//     architecturally strict; aarch64 `cntvct_el0` is spec-strict per
//     ARMv8 ARM §D11.1.2 but Apple Silicon M1 and Graviton 3 both show
//     real per-core slop in practice. `fetch_max` enforcement is required.
//   - **wasm32 (browser/Node) and WASI execute single-threaded by design**:
//     the W3C HRT spec strictly requires per-realm monotonic `now()`, and
//     WASI's execution model has no cross-thread concurrency. No atomic
//     operation has anything to enforce against. `SyncedInstant::now()`
//     on these targets compiles to the same instruction as `Instant::now()`.
//
// See `super::synced` for the enforcement implementation and the
// correctness argument. See `BENCHMARKS.md` `## Synchronization-order
// monotonicity (contract validation)` for the per-cell data driving the
// per-platform decision.

// wasm32 (browser/Node host) and WASI: single-threaded by execution model;
// no fetch_max needed. Compiles to bare `ticks()`.
#[cfg(any(
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
  target_os = "wasi",
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_synced() -> u64 {
  ticks()
}

// Every multi-threaded platform: bare counter empirically fails strict
// cross-thread monotonicity. Apply `AtomicU64::fetch_max` enforcement.
#[cfg(not(any(
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
  target_os = "wasi",
)))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_synced() -> u64 {
  super::synced::ticks_synced_enforced()
}
