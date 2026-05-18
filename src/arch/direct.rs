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
pub fn ticks_ordered() -> u64 {
  super::x86_64::rdtsc_ordered()
}

#[cfg(target_arch = "x86")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::x86::rdtsc_ordered()
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::aarch64::cntvct_ordered()
}

#[cfg(target_arch = "riscv64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::riscv64::rdtime_ordered()
}

#[cfg(target_arch = "loongarch64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::loongarch64::rdtime_ordered()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
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
pub fn ticks_ordered() -> u64 {
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

// ── Strict cross-thread monotonic reads ─────────────────────────────────
// `ticks_monotonic()` is the bare counter read plus a process-global
// `AtomicU64::fetch_max` that forces every read across every thread into a
// strictly non-decreasing sequence. Applied uniformly on every architecture:
//
//   - On x86 the per-core TSC is firmware-synchronized but not architecturally
//     so; cross-core slop is real and the fetch_max is load-bearing.
//   - On aarch64 the ARMv8 spec requires `cntvct_el0` to be a single global
//     counter, but in practice — measured on Apple Silicon M1 — per-core
//     reads can disagree by sub-microsecond amounts under contention. The
//     fetch_max forces strictness here too.
//   - On RISC-V / LoongArch / fallback clocks the underlying source is either
//     already-monotonic or globally-synced, but applying the fetch_max
//     uniformly keeps the algorithm simple and defends against future
//     implementation surprises analogous to the Apple Silicon finding.
//
// See `super::monotonic`.

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_monotonic() -> u64 {
  super::monotonic::ticks_monotonic_enforced()
}
