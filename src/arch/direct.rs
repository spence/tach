//! Compile-time Instant clock dispatch.
//!
//! On every supported target, `Instant::now()` reads the fastest reliable
//! wall-clock-rate counter for that OS/architecture pair. Most targets use an
//! architectural counter. Windows measures QPC and the two documented precise
//! interrupt-time APIs on each host, retaining the fastest complete dispatch.
//! Windows validates their backing clocks and owns any synchronization,
//! scaling, and bias needed across processors and virtualization.
//! A raw x86 TSC is not an eligible substitute because invariant-frequency
//! CPUID metadata and a local cost probe cannot prove those properties. A raw
//! Arm system-counter read is likewise ineligible without a Windows guarantee
//! that EL0 access and that counter are the active reliable timeline.
//! Intel macOS measures XNU's inline commpage nanotime protocol against
//! `mach_absolute_time`; both remain in the same kernel-owned, cross-core
//! reliable timeline. FreeBSD/amd64 selects a direct TSC only when the kernel's
//! active timecounter says it is reliable and tach's initialization probe
//! confirms the branched read materially beats the vDSO.

#[cfg(all(
  target_os = "windows",
  any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::fallback::windows_ticks()
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::apple_x86_64::ticks()
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::freebsd_x86_64::ticks()
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::linux_x86_wall::ticks()
}

#[cfg(all(
  target_arch = "x86_64",
  not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "linux",
    target_os = "android",
  )),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::x86_64::rdtsc()
}

#[cfg(all(
  target_arch = "x86",
  not(any(target_os = "windows", target_os = "linux", target_os = "android")),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::x86::rdtsc()
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::apple_aarch64::ticks()
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::linux_aarch64_wall::ticks()
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::linux_clock_wall::ticks()
}

#[cfg(all(
  target_arch = "aarch64",
  not(any(
    target_os = "android",
    target_os = "linux",
    target_os = "windows",
    target_os = "macos",
  )),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::aarch64::cntvct()
}

#[cfg(target_arch = "riscv64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::riscv64::ticks()
}

#[cfg(target_arch = "loongarch64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::loongarch64::ticks()
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::powerpc64::ticks()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::wasm::ticks()
}

#[cfg(target_os = "emscripten")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  super::emscripten::ticks()
}

#[cfg(not(any(
  target_arch = "x86_64",
  target_arch = "x86",
  target_arch = "aarch64",
  target_arch = "riscv64",
  target_arch = "loongarch64",
  all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")),
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
  target_os = "emscripten",
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
// Same dispatch as `ticks()` but each direct-counter path emits a barrier
// before the read so the timestamp cannot be sampled before a prior
// `Acquire`-or-stronger observation. Windows QPC selects a real ordering
// barrier before its platform clock call. Intel macOS uses XNU's
// LFENCE-ordered Mach absolute-time protocol. Linux
// armv7 and s390x emit their architecture's barrier before CLOCK_MONOTONIC;
// Linux powerpc64 GNU emits heavyweight sync before the Time Base read. Other
// runtime and host-call fallbacks serialize through their call boundary.

#[cfg(all(
  target_os = "windows",
  any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::fallback::windows_ticks_ordered()
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::apple_x86_64::ticks_ordered()
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::freebsd_x86_64::ticks_ordered()
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::linux_x86_wall::ticks_ordered()
}

#[cfg(all(
  target_arch = "x86_64",
  not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "linux",
    target_os = "android",
  )),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::x86_64::rdtsc_ordered()
}

#[cfg(all(
  target_arch = "x86",
  not(any(target_os = "windows", target_os = "linux", target_os = "android")),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::x86::rdtsc_ordered()
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::apple_aarch64::ticks_ordered()
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::linux_aarch64_wall::ticks_ordered()
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::linux_clock_wall::ticks_ordered()
}

#[cfg(all(
  target_arch = "aarch64",
  not(any(
    target_os = "android",
    target_os = "linux",
    target_os = "windows",
    target_os = "macos",
  )),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::aarch64::cntvct_ordered()
}

#[cfg(target_arch = "riscv64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::riscv64::ticks_ordered()
}

#[cfg(target_arch = "loongarch64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::loongarch64::ticks_ordered()
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::powerpc64::ticks_ordered()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::wasm::ticks_ordered()
}

#[cfg(target_os = "emscripten")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  super::emscripten::ticks_ordered()
}

#[cfg(not(any(
  target_arch = "x86_64",
  target_arch = "x86",
  target_arch = "aarch64",
  target_arch = "riscv64",
  target_arch = "loongarch64",
  all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")),
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
  target_os = "emscripten",
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
    #[cfg(all(
      target_os = "linux",
      any(target_arch = "arm", target_arch = "s390x", target_arch = "powerpc64"),
    ))]
    {
      super::fallback::clock_monotonic_ordered()
    }
    #[cfg(not(all(
      target_os = "linux",
      any(target_arch = "arm", target_arch = "s390x", target_arch = "powerpc64"),
    )))]
    {
      super::fallback::clock_monotonic()
    }
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

// `elapsed_unordered()` must remain in the provider and numeric domain chosen
// for `OrderedInstant`. JavaScript workers need the epoch timeline without the
// shared atomic maximum, while Linux x86 and aarch64 may choose a provider
// independently from `Instant`. Every remaining route guarantees that both
// read forms use the same provider and raw domain, so it can reuse `ticks()`.
#[cfg(all(
  target_os = "windows",
  any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  super::fallback::windows_ticks_ordered_unordered()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  super::wasm::ticks_ordered_unclamped()
}

#[cfg(target_os = "emscripten")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  super::emscripten::ticks_ordered_unclamped()
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  super::linux_x86_wall::ticks_ordered_unordered()
}

#[cfg(all(any(target_os = "android", target_os = "linux"), target_arch = "aarch64",))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  super::linux_aarch64_wall::ticks_ordered_unordered()
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  super::apple_aarch64::ticks_ordered_unordered()
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  super::linux_clock_wall::ticks_ordered_unordered()
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  super::freebsd_x86_64::ticks_ordered_unordered()
}

#[cfg(target_arch = "riscv64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  super::riscv64::ticks_ordered_unordered()
}

#[cfg(target_arch = "loongarch64")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  super::loongarch64::ticks_ordered_unordered()
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  super::powerpc64::ticks_ordered_unordered()
}

#[cfg(not(any(
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
  all(
    target_os = "windows",
    any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"),
  ),
  target_os = "emscripten",
  all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86_64", target_arch = "x86"),
  ),
  all(any(target_os = "android", target_os = "linux"), target_arch = "aarch64",),
  all(target_os = "macos", target_arch = "aarch64"),
  all(target_arch = "x86_64", target_os = "freebsd"),
  target_arch = "riscv64",
  target_arch = "loongarch64",
  all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")),
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
)))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  ticks()
}
