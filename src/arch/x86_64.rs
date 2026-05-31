use core::arch::asm;
use core::arch::x86_64::{__cpuid, _rdtsc};

#[inline(always)]
pub fn rdtsc() -> u64 {
  // SAFETY: `_rdtsc` emits the CPU counter read instruction and has no Rust memory safety
  // preconditions.
  unsafe { _rdtsc() }
}

/// Read the architectural TSC frequency from CPUID leaf 15h (`0x15`).
///
/// Returns `None` when the leaf is unsupported (pre-Skylake Intel, pre-Zen2
/// AMD) or when the hypervisor zeroes the fields (some virtualized
/// environments). The caller falls back to spin-loop calibration in that case.
///
/// Formula per Intel SDM Vol 2: `tsc_hz = crystal_hz * ratio_num / ratio_den`
/// where `ecx` is the crystal frequency, `ebx` is the TSC numerator, and
/// `eax` is the TSC denominator.
#[allow(dead_code)] // unused on macOS where mach_timebase_info is authoritative
pub fn cpuid_tsc_hz() -> Option<u64> {
  let basic = __cpuid(0);
  if basic.eax < 0x15 {
    return None;
  }
  let leaf = __cpuid(0x15);
  if leaf.eax == 0 || leaf.ebx == 0 || leaf.ecx == 0 {
    return None;
  }
  Some(u64::from(leaf.ecx) * u64::from(leaf.ebx) / u64::from(leaf.eax))
}

/// Ordered RDTSC via `rdtscp`: the instruction waits until all previous
/// instructions have executed and all previous loads are globally visible
/// before reading TSC, so the timestamp cannot be sampled before an
/// `Acquire`-or-stronger observation that precedes it (Intel SDM Vol 2B,
/// "RDTSCP—Read Time-Stamp Counter and Processor ID").
///
/// Versus the earlier `lfence; rdtsc` pattern: `rdtscp` is one instruction
/// instead of two, and it is unconditionally fully serializing for prior
/// instructions on AMD — `lfence` only serializes on AMD when the OS sets
/// `DE_CFG[1]` (Linux does so by default for Spectre v1 mitigation, but
/// non-Linux x86 OSes may not). `rdtscp` also waits for prior stores to
/// retire, which is strictly stronger than what `lfence` provides and still
/// sufficient for tach's acquire-side ordering contract.
///
/// `rdtscp` writes to ECX (`IA32_TSC_AUX`); this function discards it via the
/// `_` placeholder.
///
/// `nomem` is intentionally omitted: the CPU barrier orders execution, but
/// the compiler also needs to keep surrounding memory operations in order
/// around the read. With `nomem` the optimizer would be free to hoist a
/// prior `Acquire` load past the asm, defeating the contract.
///
/// Availability: `rdtscp` requires CPUID `80000001H:EDX[27]` — Intel since
/// Nehalem (2008), AMD since K10 (2007). Same generational floor as Invariant
/// TSC (CPUID `80000007H:EDX[8]`) which tach already relies on.
#[inline(always)]
pub fn rdtsc_fenced() -> u64 {
  let lo: u32;
  let hi: u32;
  // SAFETY: `rdtscp` writes EDX:EAX (TSC) and ECX (IA32_TSC_AUX). No stack
  // access; flags preserved. Compiler must treat as memory-touching so
  // surrounding loads aren't reordered across it.
  unsafe {
    asm!(
      "rdtscp",
      out("eax") lo,
      out("edx") hi,
      out("ecx") _,
      options(nostack, preserves_flags),
    );
  }
  (u64::from(hi) << 32) | u64::from(lo)
}
