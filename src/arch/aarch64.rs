use core::arch::asm;

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn cntvct() -> u64 {
  let cnt: u64;
  // SAFETY: `mrs cntvct_el0` only reads the architectural virtual counter register and does
  // not touch memory or the stack.
  unsafe {
    asm!(
        "mrs {}, cntvct_el0",
        out(reg) cnt,
        options(nostack, nomem, preserves_flags)
    );
  }
  cnt
}

/// Ordered CNTVCT_EL0 read. `isb sy` drains the instruction pipeline before
/// the system-register read, so the timestamp cannot be sampled before a
/// prior `Acquire`-or-stronger observation (`mrs` is a system-register
/// access; memory fences alone do not constrain when it executes).
///
/// `nomem` is intentionally omitted: the CPU barrier orders execution, but
/// the compiler must also keep surrounding memory operations in order
/// around the read. With `nomem` the optimizer would be free to hoist a
/// prior `Acquire` load past the asm, defeating the contract.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn cntvct_ordered() -> u64 {
  let cnt: u64;
  // SAFETY: `isb sy; mrs cntvct_el0` reads the architectural counter and forces a pipeline
  // sync; neither instruction accesses the stack. Compiler treats as memory-touching so
  // surrounding loads aren't reordered across it.
  unsafe {
    asm!(
        "isb sy",
        "mrs {}, cntvct_el0",
        out(reg) cnt,
        options(nostack, preserves_flags)
    );
  }
  cnt
}

#[allow(dead_code)] // unused on aarch64 Linux (calibrates against clock_gettime instead)
#[inline]
pub fn cntfrq() -> u64 {
  let freq: u64;
  // SAFETY: `mrs cntfrq_el0` only reads the architectural counter frequency register and
  // does not touch memory or the stack. The low 32 bits hold the timer rate in Hz.
  unsafe {
    asm!(
        "mrs {}, cntfrq_el0",
        out(reg) freq,
        options(nostack, nomem, preserves_flags)
    );
  }
  freq
}
