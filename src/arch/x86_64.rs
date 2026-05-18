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
/// `_` placeholder. See [`rdtscp_with_cpu`] for the variant that captures
/// the CPU-ID half — used by `MonotonicInstant` to detect thread migrations.
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
pub fn rdtsc_ordered() -> u64 {
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

/// `rdtscp` capturing both halves: the 64-bit TSC value in `EDX:EAX` and the
/// `IA32_TSC_AUX` MSR value in `ECX`. The latter is OS-populated per-core: on
/// Linux it holds `(numa_node << 12) | cpu_id`, so two reads on the same
/// thread that observe different ECX values prove the thread migrated cores
/// between them. Used by `MonotonicInstant::now()` to gate a fast-path /
/// slow-path strict-monotonicity algorithm on x86.
///
/// Other OSes populate `IA32_TSC_AUX` differently (or not at all). Callers
/// treat the value as an opaque identifier — equal-equal means same site;
/// nothing more is inferred.
///
/// Shares the same ordering guarantee as [`rdtsc_ordered`]: prior instructions
/// retire and prior loads become globally visible before the counter is read.
///
/// Currently unused inside tach — the strict cross-thread monotonicity in
/// [`crate::MonotonicInstant`] uses a uniform `fetch_max` rather than a
/// CPU-ID-keyed fast path (an earlier design attempt that turned out to
/// admit cross-thread violations). Kept available for future migration-
/// detection or NUMA-attribution use cases.
#[inline(always)]
#[allow(dead_code)]
pub fn rdtscp_with_cpu() -> (u64, u32) {
  let lo: u32;
  let hi: u32;
  let aux: u32;
  // SAFETY: `rdtscp` writes EDX:EAX (TSC) and ECX (IA32_TSC_AUX). No stack
  // access; flags preserved. Compiler must treat as memory-touching so
  // surrounding loads aren't reordered across it.
  unsafe {
    asm!(
      "rdtscp",
      out("eax") lo,
      out("edx") hi,
      out("ecx") aux,
      options(nostack, preserves_flags),
    );
  }
  ((u64::from(hi) << 32) | u64::from(lo), aux)
}
