use core::arch::asm;

#[inline(always)]
pub fn rdtime() -> u64 {
  let cnt: u64;
  // SAFETY: `rdtime.d` reads the architectural timer into a general-purpose register and does
  // not access Rust memory.
  unsafe {
    asm!(
        "rdtime.d {}, $zero",
        out(reg) cnt,
        options(nostack, nomem, preserves_flags)
    );
  }
  cnt
}

/// Ordered `rdtime.d`: emits `dbar 0` (full data barrier) before the CSR
/// read.
///
/// Caveat: `dbar 0` is a memory-access barrier; whether it constrains the
/// `rdtime.d` CSR read is implementation-defined in the LoongArch spec.
/// This is best-effort and weaker than aarch64's `isb sy` or x86's `lfence`,
/// which are documented to order their respective counter instructions.
///
/// `nomem` is intentionally omitted so the compiler also keeps surrounding
/// memory operations in order around the asm block.
#[inline(always)]
pub fn rdtime_ordered() -> u64 {
  let cnt: u64;
  // SAFETY: `dbar 0; rdtime.d` orders prior memory ops and reads the architectural timer;
  // no stack access. Compiler treats as memory-touching.
  unsafe {
    asm!(
        "dbar 0",
        "rdtime.d {}, $zero",
        out(reg) cnt,
        options(nostack, preserves_flags)
    );
  }
  cnt
}
