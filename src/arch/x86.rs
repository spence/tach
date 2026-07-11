use core::arch::asm;
use core::arch::x86::{__cpuid, _rdtsc};

#[inline(always)]
pub fn rdtsc() -> u64 {
  // SAFETY: `_rdtsc` emits the CPU counter read instruction and has no Rust memory safety
  // preconditions.
  unsafe { _rdtsc() }
}

/// Read the architectural TSC frequency from CPUID leaf 15h. See
/// `x86_64::cpuid_tsc_hz` for the formula and supported-CPU notes.
#[allow(dead_code)] // unused on macOS where mach_timebase_info is authoritative
#[allow(unused_unsafe)] // __cpuid was unsafe at the 1.85 MSRV and is safe on newer Rust
pub fn cpuid_tsc_hz() -> Option<u64> {
  // SAFETY: Rust's supported x86 targets provide CPUID, and leaf 0 only
  // reads CPU identification registers.
  let basic = unsafe { __cpuid(0) };
  if basic.eax < 0x15 {
    return None;
  }
  // SAFETY: the maximum basic leaf reported above includes leaf 0x15.
  let leaf = unsafe { __cpuid(0x15) };
  if leaf.eax == 0 || leaf.ebx == 0 || leaf.ecx == 0 {
    return None;
  }
  Some(u64::from(leaf.ecx) * u64::from(leaf.ebx) / u64::from(leaf.eax))
}

/// Ordered RDTSC via `rdtscp`: one-instruction equivalent of `lfence; rdtsc`
/// that also serializes prior stores on AMD without requiring `DE_CFG[1]`.
/// See `x86_64::rdtsc_ordered` for the full rationale and the `nomem`
/// discussion.
#[inline(always)]
pub fn rdtsc_ordered() -> u64 {
  let lo: u32;
  let hi: u32;
  // SAFETY: `rdtscp` writes EDX:EAX (TSC) and ECX (IA32_TSC_AUX). Compiler
  // must treat as memory-touching so surrounding loads aren't reordered
  // across it.
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
