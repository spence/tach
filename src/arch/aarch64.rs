use core::arch::asm;

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn cntvct() -> u64 {
  let cnt: u64;
  // SAFETY: CNTVCT_EL0 is a read-only architectural counter register. The
  // register contract orders reads of CNTVCT/CNTVCTSS with one another, which
  // preserves same-thread non-decreasing samples. It does not order this read
  // with surrounding memory operations.
  unsafe {
    asm!(
      "mrs {}, cntvct_el0",
      out(reg) cnt,
      options(nostack, nomem, preserves_flags),
    );
  }
  cnt
}

/// Ordered architectural virtual-counter read for aarch64 targets without a
/// platform-specific runtime selector.
#[inline(always)]
#[allow(clippy::inline_always)]
#[cfg_attr(any(target_os = "android", target_os = "linux"), allow(dead_code))]
pub fn cntvct_ordered() -> u64 {
  cntvct_after_isb()
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn cntvct_after_isb() -> u64 {
  let cnt: u64;
  // SAFETY: ISB prevents the counter read from executing before prior
  // instructions. Omitting `nomem` also prevents the compiler from moving a
  // surrounding memory observation across the ordered sample.
  unsafe {
    asm!(
      "isb sy",
      "mrs {}, cntvct_el0",
      out(reg) cnt,
      options(nostack, preserves_flags),
    );
  }
  cnt
}

#[cfg(any(target_os = "android", target_os = "linux"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn cntvctss() -> u64 {
  let cnt: u64;
  // SAFETY: callers first require Linux HWCAP2_ECV, which advertises an EL0-
  // usable self-synchronizing virtual counter. The numeric encoding keeps the
  // crate compatible with assemblers predating the CNTVCTSS mnemonic.
  unsafe {
    asm!(
      "mrs {}, S3_3_C14_C0_6",
      out(reg) cnt,
      options(nostack, preserves_flags),
    );
  }
  cnt
}

#[cfg(any(target_os = "android", target_os = "linux"))]
pub(crate) fn cntvctss_capable() -> bool {
  const HWCAP2_ECV: libc::c_ulong = 1 << 19;

  // SAFETY: getauxval has no pointer arguments and AT_HWCAP2 is part of the
  // Linux userspace ABI.
  unsafe { libc::getauxval(libc::AT_HWCAP2) & HWCAP2_ECV != 0 }
}

#[cfg(all(feature = "bench-internal", any(target_os = "android", target_os = "linux"),))]
#[inline(always)]
pub(crate) fn bench_cntvct_after_isb() -> u64 {
  cntvct_after_isb()
}

#[cfg(all(feature = "bench-internal", any(target_os = "android", target_os = "linux"),))]
#[inline(always)]
pub(crate) fn bench_cntvctss() -> u64 {
  cntvctss()
}

#[cfg(all(feature = "bench-internal", any(target_os = "android", target_os = "linux"),))]
pub(crate) fn bench_cntvctss_capable() -> bool {
  cntvctss_capable()
}

/// Read the architectural counter frequency. Linux calibrates the direct
/// counter against CLOCK_MONOTONIC; Android uses the firmware-published rate.
#[inline]
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub fn cntfrq() -> u64 {
  let freq: u64;
  // SAFETY: CNTFRQ_EL0 is a read-only architectural frequency register.
  unsafe {
    asm!(
      "mrs {}, cntfrq_el0",
      out(reg) freq,
      options(nostack, nomem, preserves_flags),
    );
  }
  freq
}
