use core::arch::asm;

/// Reads the architectural time counter.
#[inline(always)]
pub fn rdtime() -> u64 {
  let cnt: u64;
  // SAFETY: `rdtime` reads a timer CSR into a general-purpose register and does not access
  // Rust memory.
  unsafe {
    asm!(
        "rdtime {}",
        out(reg) cnt,
        options(nostack, nomem, preserves_flags)
    );
  }
  cnt
}

/// Ordered `rdtime`: emits a full memory barrier (`fence iorw, iorw`)
/// before the CSR read.
///
/// Caveat: `rdtime` reads a Zicntr control/status register, not memory, and
/// the base RISC-V spec does not explicitly define whether memory fences
/// constrain CSR access ordering. This implementation uses the strongest
/// available memory barrier as a best-effort guarantee; on most current
/// hardware the barrier serializes the pipeline enough to prevent the CSR
/// read from being hoisted above prior `Acquire`-or-stronger observations,
/// but the contract is weaker than aarch64's `isb sy` or x86's `lfence`,
/// which are documented to order their respective counter instructions.
///
/// `nomem` is intentionally omitted so the compiler also keeps surrounding
/// memory operations in order around the asm block.
#[inline(always)]
pub fn rdtime_ordered() -> u64 {
  let cnt: u64;
  // SAFETY: `fence iorw, iorw; rdtime` orders prior I/O and memory ops vs subsequent ones
  // and reads the timer CSR; no stack access. Compiler treats as memory-touching.
  unsafe {
    asm!(
        "fence iorw, iorw",
        "rdtime {}",
        out(reg) cnt,
        options(nostack, preserves_flags)
    );
  }
  cnt
}
