#![cfg_attr(
  not(any(target_os = "linux", target_os = "android")),
  allow(dead_code, reason = "Linux-kernel-only perf helpers are inert on other x86_64 targets")
)]

use core::arch::asm;
use core::arch::x86_64::{__cpuid, __cpuid_count, _rdtsc};
#[cfg(feature = "bench-internal")]
use core::cell::UnsafeCell;
#[cfg(feature = "bench-internal")]
use core::mem::MaybeUninit;
#[cfg(feature = "bench-internal")]
use core::sync::atomic::AtomicBool;
#[cfg(unix)]
use core::sync::atomic::AtomicI32;
use core::sync::atomic::{AtomicU8, Ordering};

const ORDERED_READ_UNKNOWN: u8 = 0;
pub(crate) const ORDERED_READ_LFENCE_RDTSC: u8 = 1;
pub(crate) const ORDERED_READ_RDTSCP: u8 = 2;
pub(crate) const ORDERED_READ_CPUID_RDTSC: u8 = 3;
pub(crate) const ORDERED_READ_MFENCE_RDTSC: u8 = 4;
pub(crate) const ORDERED_READ_SERIALIZE_RDTSC: u8 = 5;
const ORDERED_READ_SELECTING: u8 = 6;
const ORDERED_PROBE_BATCHES: usize = 9;
const ORDERED_PROBE_READS: u64 = 4096;
const ORDERED_PROBE_WARMUP_READS: u64 = 1024;
const REQUIRED_DECISIVE_WINS: usize = 8;

static ORDERED_READ_KIND: AtomicU8 = AtomicU8::new(ORDERED_READ_UNKNOWN);
static ORDERED_PROBE_KIND: AtomicU8 = AtomicU8::new(ORDERED_READ_CPUID_RDTSC);
#[cfg(feature = "thread-cpu-inline")]
pub(crate) const PERF_SEQ_LFENCE: u8 = 1;
#[cfg(feature = "thread-cpu-inline")]
pub(crate) const PERF_SEQ_MFENCE: u8 = 2;
#[cfg(feature = "thread-cpu-inline")]
pub(crate) const PERF_SEQ_SERIALIZE: u8 = 3;
#[cfg(feature = "thread-cpu-inline")]
pub(crate) const PERF_SEQ_CPUID: u8 = 4;
#[cfg(feature = "thread-cpu-inline")]
pub(crate) const PERF_SEQ_RDTSCP: u8 = 5;
#[cfg(feature = "thread-cpu-inline")]
pub(crate) const PERF_SEQ_CANDIDATES: [u8; 5] =
  [PERF_SEQ_CPUID, PERF_SEQ_LFENCE, PERF_SEQ_MFENCE, PERF_SEQ_RDTSCP, PERF_SEQ_SERIALIZE];
#[cfg(unix)]
static ORDERED_READ_OWNER_PID: AtomicI32 = AtomicI32::new(0);

#[derive(Clone, Copy)]
struct SelectionDecision {
  #[cfg(feature = "bench-internal")]
  challenger_median: u64,
  #[cfg(feature = "bench-internal")]
  incumbent_median: u64,
  #[cfg(feature = "bench-internal")]
  allowance: u64,
  #[cfg(feature = "bench-internal")]
  decisive_wins: usize,
  challenger_selected: bool,
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy)]
#[allow(dead_code)] // Exposed for the benchmark serializer owned outside this module.
pub(crate) struct OrderedProbeEvidence {
  pub(crate) lfence_eligible: bool,
  pub(crate) mfence_eligible: bool,
  pub(crate) rdtscp_eligible: bool,
  pub(crate) serialize_eligible: bool,
  pub(crate) cpuid_batches: [u64; ORDERED_PROBE_BATCHES],
  pub(crate) lfence_batches: [u64; ORDERED_PROBE_BATCHES],
  pub(crate) mfence_batches: [u64; ORDERED_PROBE_BATCHES],
  pub(crate) rdtscp_batches: [u64; ORDERED_PROBE_BATCHES],
  pub(crate) serialize_batches: [u64; ORDERED_PROBE_BATCHES],
  pub(crate) cpuid_median: u64,
  pub(crate) lfence_median: u64,
  pub(crate) mfence_median: u64,
  pub(crate) rdtscp_median: u64,
  pub(crate) serialize_median: u64,
  pub(crate) lfence_vs_cpuid_allowance: u64,
  pub(crate) lfence_vs_cpuid_decisive_wins: usize,
  pub(crate) lfence_selected_over_cpuid: bool,
  pub(crate) mfence_incumbent: &'static str,
  pub(crate) mfence_vs_incumbent_allowance: u64,
  pub(crate) mfence_vs_incumbent_decisive_wins: usize,
  pub(crate) mfence_selected_over_incumbent: bool,
  pub(crate) rdtscp_incumbent: &'static str,
  pub(crate) rdtscp_vs_incumbent_allowance: u64,
  pub(crate) rdtscp_vs_incumbent_decisive_wins: usize,
  pub(crate) rdtscp_selected_over_incumbent: bool,
  pub(crate) serialize_incumbent: &'static str,
  pub(crate) serialize_vs_incumbent_allowance: u64,
  pub(crate) serialize_vs_incumbent_decisive_wins: usize,
  pub(crate) serialize_selected_over_incumbent: bool,
  pub(crate) selected_provider: &'static str,
  // Pairwise benchmark view with LFENCE as challenger and RDTSCP as
  // incumbent when both are eligible.
  pub(crate) challenger_batches: [u64; ORDERED_PROBE_BATCHES],
  pub(crate) incumbent_batches: [u64; ORDERED_PROBE_BATCHES],
  pub(crate) reads_per_batch: u64,
  pub(crate) counter_hz: u64,
  pub(crate) challenger_median: u64,
  pub(crate) incumbent_median: u64,
  pub(crate) allowance: u64,
  pub(crate) decisive_wins: usize,
  pub(crate) required_decisive_wins: usize,
  pub(crate) challenger_selected: bool,
}

#[cfg(feature = "bench-internal")]
struct EvidenceCell(UnsafeCell<MaybeUninit<OrderedProbeEvidence>>);

// SAFETY: the selection owner writes the evidence before publishing readiness
// with Release, and readers require an Acquire observation of readiness.
#[cfg(feature = "bench-internal")]
unsafe impl Sync for EvidenceCell {}

#[cfg(feature = "bench-internal")]
static ORDERED_PROBE_EVIDENCE: EvidenceCell = EvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));
#[cfg(feature = "bench-internal")]
static ORDERED_PROBE_EVIDENCE_READY: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct ProbeSamples {
  cpuid: [u64; ORDERED_PROBE_BATCHES],
  lfence: [u64; ORDERED_PROBE_BATCHES],
  mfence: [u64; ORDERED_PROBE_BATCHES],
  rdtscp: [u64; ORDERED_PROBE_BATCHES],
  serialize: [u64; ORDERED_PROBE_BATCHES],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OrderedReadEligibility {
  lfence: bool,
  mfence: bool,
  rdtscp: bool,
  serialize: bool,
}

#[derive(Clone, Copy)]
struct SelectionOutcome {
  selected: u8,
  #[cfg(feature = "bench-internal")]
  lfence_vs_cpuid: SelectionDecision,
  #[cfg(feature = "bench-internal")]
  mfence_incumbent: u8,
  #[cfg(feature = "bench-internal")]
  mfence_vs_incumbent: SelectionDecision,
  #[cfg(feature = "bench-internal")]
  rdtscp_incumbent: u8,
  #[cfg(feature = "bench-internal")]
  rdtscp_vs_incumbent: SelectionDecision,
  #[cfg(feature = "bench-internal")]
  serialize_incumbent: u8,
  #[cfg(feature = "bench-internal")]
  serialize_vs_incumbent: SelectionDecision,
}

#[inline(always)]
pub fn rdtsc() -> u64 {
  // SAFETY: `_rdtsc` emits the CPU counter read instruction and has no Rust memory safety
  // preconditions.
  unsafe { _rdtsc() }
}

/// Brackets a TSC read between barriers suitable for a perf userpage
/// sequence-counter read: prior metadata loads complete before the counter,
/// and the closing lock load cannot execute before it.
#[inline(always)]
#[cfg(feature = "thread-cpu-inline")]
pub(crate) fn read_perf_seqlock_counter(kind: u8) -> u64 {
  match kind {
    PERF_SEQ_LFENCE => read_lfence_rdtsc_lfence(),
    PERF_SEQ_MFENCE => read_mfence_rdtsc_mfence(),
    PERF_SEQ_RDTSCP => read_rdtscp_lfence(),
    PERF_SEQ_SERIALIZE => read_serialize_rdtsc_serialize(),
    _ => read_cpuid_rdtsc_cpuid(),
  }
}

#[allow(unused_unsafe)] // CPUID intrinsics changed safety across supported toolchains.
#[cfg(feature = "thread-cpu-inline")]
fn detect_perf_seqlock_eligibility() -> OrderedReadEligibility {
  // SAFETY: x86_64 guarantees CPUID and basic leaf 1.
  let basic = unsafe { __cpuid(0) };
  // SAFETY: basic leaf 1 is part of the x86_64 baseline.
  let leaf1 = unsafe { __cpuid(1) };
  // SAFETY: extended leaf 0 is the architected maximum-leaf query.
  let extended = unsafe { __cpuid(0x8000_0000) };
  let extended21_eax = if extended.eax >= 0x8000_0021 {
    // SAFETY: the maximum extended leaf includes this leaf.
    unsafe { __cpuid(0x8000_0021) }.eax
  } else {
    0
  };
  let extended_edx = if extended.eax >= 0x8000_0001 {
    // SAFETY: the maximum extended leaf includes this leaf.
    unsafe { __cpuid(0x8000_0001) }.edx
  } else {
    0
  };
  let leaf7_edx = if basic.eax >= 7 {
    // SAFETY: the maximum basic leaf includes leaf 7, subleaf 0.
    unsafe { __cpuid_count(7, 0) }.edx
  } else {
    0
  };
  ordered_read_eligibility(
    basic.ebx,
    basic.edx,
    basic.ecx,
    leaf1.edx,
    extended_edx,
    extended21_eax,
    leaf7_edx,
  )
}

#[cfg(feature = "thread-cpu-inline")]
pub(crate) fn perf_seqlock_candidate_eligible(kind: u8) -> bool {
  let eligibility = detect_perf_seqlock_eligibility();
  match kind {
    PERF_SEQ_LFENCE => eligibility.lfence,
    PERF_SEQ_MFENCE => eligibility.mfence,
    PERF_SEQ_RDTSCP => eligibility.rdtscp,
    PERF_SEQ_SERIALIZE => eligibility.serialize,
    PERF_SEQ_CPUID => true,
    _ => false,
  }
}

#[cfg(all(feature = "thread-cpu-inline", feature = "bench-internal"))]
pub(crate) const fn perf_seqlock_candidate_name(kind: u8) -> &'static str {
  match kind {
    PERF_SEQ_LFENCE => "x86_lfence_rdtsc_lfence",
    PERF_SEQ_MFENCE => "x86_mfence_rdtsc_mfence",
    PERF_SEQ_RDTSCP => "x86_rdtscp_lfence",
    PERF_SEQ_SERIALIZE => "x86_serialize_rdtsc_serialize",
    _ => "x86_cpuid_rdtsc_cpuid",
  }
}

#[inline(always)]
#[cfg(feature = "thread-cpu-inline")]
fn read_lfence_rdtsc_lfence() -> u64 {
  let lo: u32;
  let hi: u32;
  // SAFETY: eligibility requires Intel's architectural LFENCE contract or
  // AMD's LFENCE-always-serializing capability.
  unsafe {
    asm!(
      "lfence",
      "rdtsc",
      "lfence",
      out("eax") lo,
      out("edx") hi,
      options(nostack, preserves_flags),
    );
  }
  (u64::from(hi) << 32) | u64::from(lo)
}

#[inline(always)]
#[cfg(feature = "thread-cpu-inline")]
fn read_mfence_rdtsc_mfence() -> u64 {
  let lo: u32;
  let hi: u32;
  // SAFETY: eligibility requires AMD64 with SSE2; AMD specifies MFENCE as
  // serializing for both sides of this counter read.
  unsafe {
    asm!(
      "mfence",
      "rdtsc",
      "mfence",
      out("eax") lo,
      out("edx") hi,
      options(nostack, preserves_flags),
    );
  }
  (u64::from(hi) << 32) | u64::from(lo)
}

#[inline(always)]
#[cfg(feature = "thread-cpu-inline")]
fn read_rdtscp_lfence() -> u64 {
  let lo: u32;
  let hi: u32;
  // SAFETY: eligibility requires RDTSCP. RDTSCP orders the prior metadata
  // loads before the counter; LFENCE keeps the closing lock load after it.
  unsafe {
    asm!(
      "rdtscp",
      "lfence",
      out("eax") lo,
      out("edx") hi,
      out("ecx") _,
      options(nostack, preserves_flags),
    );
  }
  (u64::from(hi) << 32) | u64::from(lo)
}

#[inline(always)]
#[cfg(feature = "thread-cpu-inline")]
fn read_serialize_rdtsc_serialize() -> u64 {
  let lo: u32;
  let hi: u32;
  // SAFETY: eligibility requires CPUID.7.0:EDX[SERIALIZE].
  unsafe {
    asm!(
      "serialize",
      "rdtsc",
      "serialize",
      out("eax") lo,
      out("edx") hi,
      options(nostack, preserves_flags),
    );
  }
  (u64::from(hi) << 32) | u64::from(lo)
}

#[inline(always)]
#[cfg(feature = "thread-cpu-inline")]
fn read_cpuid_rdtsc_cpuid() -> u64 {
  // SAFETY: x86_64 guarantees CPUID and RDTSC. CPUID serializes both metadata
  // sides; retaining the TSC in a Rust value keeps it live across the second.
  unsafe {
    let _ = __cpuid(0);
    let cycle = _rdtsc();
    let _ = __cpuid(0);
    cycle
  }
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
#[allow(unused_unsafe)] // Keeps this source warning-free across supported toolchains.
pub fn cpuid_tsc_hz() -> Option<u64> {
  // SAFETY: leaf 0 is supported on every x86_64 CPU and only reads CPU
  // identification registers.
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

/// Reads TSC after prior instructions and loads have completed.
///
/// `nomem` is intentionally omitted: the CPU barrier orders execution, but
/// the compiler also needs to keep surrounding memory operations in order
/// around the read. With `nomem` the optimizer would be free to hoist a
/// prior `Acquire` load past the asm, defeating the contract.
///
#[inline(always)]
#[cfg_attr(any(target_os = "android", target_os = "linux"), allow(dead_code))]
pub fn rdtsc_ordered() -> u64 {
  match ORDERED_READ_KIND.load(Ordering::Relaxed) {
    ORDERED_READ_LFENCE_RDTSC => read_lfence_rdtsc(),
    ORDERED_READ_MFENCE_RDTSC => read_mfence_rdtsc(),
    ORDERED_READ_RDTSCP => read_rdtscp(),
    ORDERED_READ_SERIALIZE_RDTSC => read_serialize_rdtsc(),
    ORDERED_READ_CPUID_RDTSC => read_cpuid_rdtsc(),
    _ => rdtsc_ordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
#[cfg_attr(any(target_os = "android", target_os = "linux"), allow(dead_code))]
fn rdtsc_ordered_after_selection() -> u64 {
  read_ordered_kind(selected_ordered_read_kind())
}

#[inline(always)]
#[cfg_attr(any(target_os = "android", target_os = "linux"), allow(dead_code))]
fn read_ordered_kind(kind: u8) -> u64 {
  match kind {
    ORDERED_READ_LFENCE_RDTSC => read_lfence_rdtsc(),
    ORDERED_READ_MFENCE_RDTSC => read_mfence_rdtsc(),
    ORDERED_READ_RDTSCP => read_rdtscp(),
    ORDERED_READ_SERIALIZE_RDTSC => read_serialize_rdtsc(),
    _ => read_cpuid_rdtsc(),
  }
}

#[inline(always)]
pub(crate) fn read_lfence_rdtsc() -> u64 {
  let lo: u32;
  let hi: u32;
  // SAFETY: detection requires SSE2 and either Intel's architectural contract
  // or AMD's LFENCE-always-serializing CPUID capability.
  unsafe {
    asm!(
      "lfence",
      "rdtsc",
      out("eax") lo,
      out("edx") hi,
      options(nostack, preserves_flags),
    );
  }
  (u64::from(hi) << 32) | u64::from(lo)
}

#[inline(always)]
pub(crate) fn read_mfence_rdtsc() -> u64 {
  let lo: u32;
  let hi: u32;
  // SAFETY: detection requires an AMD processor with SSE2. AMD specifies
  // MFENCE as serializing on AMD64 processors, which orders the following
  // RDTSC after prior instructions.
  unsafe {
    asm!(
      "mfence",
      "rdtsc",
      out("eax") lo,
      out("edx") hi,
      options(nostack, preserves_flags),
    );
  }
  (u64::from(hi) << 32) | u64::from(lo)
}

#[inline(always)]
pub(crate) fn read_rdtscp() -> u64 {
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

#[inline(always)]
pub(crate) fn read_cpuid_rdtsc() -> u64 {
  let lo: u32;
  let hi: u32;
  // SAFETY: x86_64 guarantees CPUID. Saving RBX through RSI preserves the
  // register that LLVM reserves for position-independent code. CPUID fully
  // serializes prior instructions before RDTSC reads the counter.
  unsafe {
    asm!(
      "mov rsi, rbx",
      "mov eax, 0",
      "cpuid",
      "mov rbx, rsi",
      "rdtsc",
      out("eax") lo,
      out("ecx") _,
      out("edx") hi,
      out("rsi") _,
      options(nostack, preserves_flags),
    );
  }
  (u64::from(hi) << 32) | u64::from(lo)
}

#[inline(always)]
pub(crate) fn read_serialize_rdtsc() -> u64 {
  let lo: u32;
  let hi: u32;
  // SAFETY: eligibility requires CPUID.7.0:EDX[SERIALIZE]. SERIALIZE waits
  // for prior instructions and memory accesses before RDTSC reads the TSC.
  unsafe {
    asm!(
      "serialize",
      "rdtsc",
      out("eax") lo,
      out("edx") hi,
      options(nostack, preserves_flags),
    );
  }
  (u64::from(hi) << 32) | u64::from(lo)
}

pub(crate) fn selected_ordered_read_kind() -> u8 {
  loop {
    match ORDERED_READ_KIND.load(Ordering::Acquire) {
      ORDERED_READ_UNKNOWN => {
        #[cfg(unix)]
        let claimed = super::claim_process_selection(
          &ORDERED_READ_KIND,
          ORDERED_READ_UNKNOWN,
          ORDERED_READ_SELECTING,
          &ORDERED_READ_OWNER_PID,
        );
        #[cfg(not(unix))]
        let claimed = ORDERED_READ_KIND
          .compare_exchange(
            ORDERED_READ_UNKNOWN,
            ORDERED_READ_SELECTING,
            Ordering::AcqRel,
            Ordering::Acquire,
          )
          .is_ok();
        if claimed {
          let kind = detect_ordered_read_kind();
          ORDERED_READ_KIND.store(kind, Ordering::Release);
          return kind;
        }
      }
      ORDERED_READ_SELECTING => {
        #[cfg(unix)]
        if super::recover_inherited_selection(
          &ORDERED_READ_KIND,
          ORDERED_READ_SELECTING,
          ORDERED_READ_UNKNOWN,
          &ORDERED_READ_OWNER_PID,
        ) {
          continue;
        }
        // Every candidate reads the same TSC domain. A signal handler or
        // concurrent first reader can use the conservative serializing
        // baseline without waiting for the selector it may have interrupted.
        return ORDERED_READ_CPUID_RDTSC;
      }
      kind => return kind,
    }
  }
}

#[cold]
#[inline(never)]
#[allow(unused_unsafe)] // Keeps this source warning-free across supported toolchains.
fn detect_ordered_read_kind() -> u8 {
  // SAFETY: x86_64 guarantees CPUID and leaf 0 is always available.
  let basic = unsafe { __cpuid(0) };
  // SAFETY: basic leaf 1 is part of the x86_64 architectural baseline.
  let leaf1 = unsafe { __cpuid(1) };
  // SAFETY: extended leaf 0 is the architected maximum-leaf query.
  let extended = unsafe { __cpuid(0x8000_0000) };
  let extended_edx = if extended.eax >= 0x8000_0001 {
    // SAFETY: the maximum extended leaf reported above includes this leaf.
    unsafe { __cpuid(0x8000_0001) }.edx
  } else {
    0
  };
  let extended21_eax = if extended.eax >= 0x8000_0021 {
    // SAFETY: the maximum extended leaf reported above includes this leaf.
    unsafe { __cpuid(0x8000_0021) }.eax
  } else {
    0
  };
  let leaf7_edx = if basic.eax >= 7 {
    // SAFETY: the maximum basic leaf reported above includes leaf 7, subleaf 0.
    unsafe { __cpuid_count(7, 0) }.edx
  } else {
    0
  };
  let eligibility = ordered_read_eligibility(
    basic.ebx,
    basic.edx,
    basic.ecx,
    leaf1.edx,
    extended_edx,
    extended21_eax,
    leaf7_edx,
  );
  let samples = measure_ordered_candidates(eligibility);
  let counter_hz = selection_tsc_hz();
  let outcome = select_ordered_candidate(eligibility, samples, counter_hz);

  #[cfg(feature = "bench-internal")]
  {
    let compatibility = if eligibility.lfence && eligibility.rdtscp {
      evaluate_challenger(samples.lfence, samples.rdtscp, counter_hz)
    } else {
      unavailable_decision(samples.lfence, samples.rdtscp)
    };
    let evidence = OrderedProbeEvidence {
      lfence_eligible: eligibility.lfence,
      mfence_eligible: eligibility.mfence,
      rdtscp_eligible: eligibility.rdtscp,
      serialize_eligible: eligibility.serialize,
      cpuid_batches: samples.cpuid,
      lfence_batches: samples.lfence,
      mfence_batches: samples.mfence,
      rdtscp_batches: samples.rdtscp,
      serialize_batches: samples.serialize,
      cpuid_median: median(samples.cpuid),
      lfence_median: median(samples.lfence),
      mfence_median: median(samples.mfence),
      rdtscp_median: median(samples.rdtscp),
      serialize_median: median(samples.serialize),
      lfence_vs_cpuid_allowance: outcome.lfence_vs_cpuid.allowance,
      lfence_vs_cpuid_decisive_wins: outcome.lfence_vs_cpuid.decisive_wins,
      lfence_selected_over_cpuid: outcome.lfence_vs_cpuid.challenger_selected,
      mfence_incumbent: ordered_provider_name(outcome.mfence_incumbent),
      mfence_vs_incumbent_allowance: outcome.mfence_vs_incumbent.allowance,
      mfence_vs_incumbent_decisive_wins: outcome.mfence_vs_incumbent.decisive_wins,
      mfence_selected_over_incumbent: outcome.mfence_vs_incumbent.challenger_selected,
      rdtscp_incumbent: ordered_provider_name(outcome.rdtscp_incumbent),
      rdtscp_vs_incumbent_allowance: outcome.rdtscp_vs_incumbent.allowance,
      rdtscp_vs_incumbent_decisive_wins: outcome.rdtscp_vs_incumbent.decisive_wins,
      rdtscp_selected_over_incumbent: outcome.rdtscp_vs_incumbent.challenger_selected,
      serialize_incumbent: ordered_provider_name(outcome.serialize_incumbent),
      serialize_vs_incumbent_allowance: outcome.serialize_vs_incumbent.allowance,
      serialize_vs_incumbent_decisive_wins: outcome.serialize_vs_incumbent.decisive_wins,
      serialize_selected_over_incumbent: outcome.serialize_vs_incumbent.challenger_selected,
      selected_provider: ordered_provider_name(outcome.selected),
      challenger_batches: samples.lfence,
      incumbent_batches: samples.rdtscp,
      reads_per_batch: ORDERED_PROBE_READS,
      counter_hz,
      challenger_median: compatibility.challenger_median,
      incumbent_median: compatibility.incumbent_median,
      allowance: compatibility.allowance,
      decisive_wins: compatibility.decisive_wins,
      required_decisive_wins: REQUIRED_DECISIVE_WINS,
      challenger_selected: compatibility.challenger_selected,
    };
    // SAFETY: only the process-selection owner writes before publishing the
    // final read kind; fork recovery creates an independent address space.
    unsafe { (*ORDERED_PROBE_EVIDENCE.0.get()).write(evidence) };
    ORDERED_PROBE_EVIDENCE_READY.store(true, Ordering::Release);
  }

  outcome.selected
}

#[inline]
fn ordered_read_eligibility(
  vendor_ebx: u32,
  vendor_edx: u32,
  vendor_ecx: u32,
  leaf1_edx: u32,
  extended_edx: u32,
  extended21_eax: u32,
  leaf7_edx: u32,
) -> OrderedReadEligibility {
  const INTEL_EBX: u32 = u32::from_le_bytes(*b"Genu");
  const INTEL_EDX: u32 = u32::from_le_bytes(*b"ineI");
  const INTEL_ECX: u32 = u32::from_le_bytes(*b"ntel");
  const AMD_EBX: u32 = u32::from_le_bytes(*b"Auth");
  const AMD_EDX: u32 = u32::from_le_bytes(*b"enti");
  const AMD_ECX: u32 = u32::from_le_bytes(*b"cAMD");
  const SSE2: u32 = 1 << 26;
  const RDTSCP: u32 = 1 << 27;
  const AMD_LFENCE_ALWAYS_SERIALIZING: u32 = 1 << 2;
  const SERIALIZE: u32 = 1 << 14;

  let vendor = (vendor_ebx, vendor_edx, vendor_ecx);
  let intel = vendor == (INTEL_EBX, INTEL_EDX, INTEL_ECX);
  let amd = vendor == (AMD_EBX, AMD_EDX, AMD_ECX);
  let sse2 = leaf1_edx & SSE2 != 0;
  OrderedReadEligibility {
    lfence: sse2 && (intel || (amd && extended21_eax & AMD_LFENCE_ALWAYS_SERIALIZING != 0)),
    mfence: amd && sse2,
    rdtscp: extended_edx & RDTSCP != 0,
    serialize: leaf7_edx & SERIALIZE != 0,
  }
}

fn measure_ordered_candidates(eligibility: OrderedReadEligibility) -> ProbeSamples {
  let candidates = [
    ORDERED_READ_CPUID_RDTSC,
    ORDERED_READ_LFENCE_RDTSC,
    ORDERED_READ_MFENCE_RDTSC,
    ORDERED_READ_RDTSCP,
    ORDERED_READ_SERIALIZE_RDTSC,
  ];
  for provider in candidates {
    if !ordered_candidate_eligible(provider, eligibility) {
      continue;
    }
    ORDERED_PROBE_KIND.store(provider, Ordering::Relaxed);
    for _ in 0..ORDERED_PROBE_WARMUP_READS {
      core::hint::black_box(probe_ordered_hot_path());
    }
  }

  let mut samples = ProbeSamples {
    cpuid: [u64::MAX; ORDERED_PROBE_BATCHES],
    lfence: [u64::MAX; ORDERED_PROBE_BATCHES],
    mfence: [u64::MAX; ORDERED_PROBE_BATCHES],
    rdtscp: [u64::MAX; ORDERED_PROBE_BATCHES],
    serialize: [u64::MAX; ORDERED_PROBE_BATCHES],
  };
  for sample in 0..ORDERED_PROBE_BATCHES {
    for offset in 0..candidates.len() {
      let provider = candidates[(sample + offset) % candidates.len()];
      if !ordered_candidate_eligible(provider, eligibility) {
        continue;
      }
      let elapsed = measure_ordered_batch(provider);
      match provider {
        ORDERED_READ_LFENCE_RDTSC => samples.lfence[sample] = elapsed,
        ORDERED_READ_MFENCE_RDTSC => samples.mfence[sample] = elapsed,
        ORDERED_READ_RDTSCP => samples.rdtscp[sample] = elapsed,
        ORDERED_READ_SERIALIZE_RDTSC => samples.serialize[sample] = elapsed,
        _ => samples.cpuid[sample] = elapsed,
      }
    }
  }
  samples
}

#[inline]
const fn ordered_candidate_eligible(provider: u8, eligibility: OrderedReadEligibility) -> bool {
  match provider {
    ORDERED_READ_LFENCE_RDTSC => eligibility.lfence,
    ORDERED_READ_MFENCE_RDTSC => eligibility.mfence,
    ORDERED_READ_RDTSCP => eligibility.rdtscp,
    ORDERED_READ_SERIALIZE_RDTSC => eligibility.serialize,
    _ => true,
  }
}

#[inline(always)]
fn probe_ordered_hot_path() -> u64 {
  match ORDERED_PROBE_KIND.load(Ordering::Relaxed) {
    ORDERED_READ_LFENCE_RDTSC => read_lfence_rdtsc(),
    ORDERED_READ_MFENCE_RDTSC => read_mfence_rdtsc(),
    ORDERED_READ_RDTSCP => read_rdtscp(),
    ORDERED_READ_SERIALIZE_RDTSC => read_serialize_rdtsc(),
    _ => read_cpuid_rdtsc(),
  }
}

#[inline(never)]
fn measure_ordered_batch(provider: u8) -> u64 {
  ORDERED_PROBE_KIND.store(provider, Ordering::Relaxed);
  let start = measurement_tsc();
  let mut sink = 0_u64;
  for _ in 0..ORDERED_PROBE_READS {
    sink ^= probe_ordered_hot_path();
  }
  let elapsed = measurement_tsc().saturating_sub(start);
  core::hint::black_box(sink);
  elapsed
}

#[inline(always)]
fn measurement_tsc() -> u64 {
  // CPUID+RDTSC is the reliable baseline on every supported x86 CPU. Using it
  // at both bracket boundaries keeps the probe valid even when neither fast
  // candidate is eligible and prevents the measured body crossing a boundary.
  read_cpuid_rdtsc()
}

fn select_ordered_candidate(
  eligibility: OrderedReadEligibility,
  samples: ProbeSamples,
  counter_hz: u64,
) -> SelectionOutcome {
  let lfence_vs_cpuid = if eligibility.lfence {
    evaluate_challenger(samples.lfence, samples.cpuid, counter_hz)
  } else {
    unavailable_decision(samples.lfence, samples.cpuid)
  };
  let mut selected = if lfence_vs_cpuid.challenger_selected {
    ORDERED_READ_LFENCE_RDTSC
  } else {
    ORDERED_READ_CPUID_RDTSC
  };
  #[cfg(feature = "bench-internal")]
  let mfence_incumbent = selected;
  let incumbent_samples = candidate_samples(samples, selected);
  let mfence_vs_incumbent = if eligibility.mfence {
    evaluate_challenger(samples.mfence, incumbent_samples, counter_hz)
  } else {
    unavailable_decision(samples.mfence, incumbent_samples)
  };
  if mfence_vs_incumbent.challenger_selected {
    selected = ORDERED_READ_MFENCE_RDTSC;
  }
  #[cfg(feature = "bench-internal")]
  let rdtscp_incumbent = selected;
  let incumbent_samples = candidate_samples(samples, selected);
  let rdtscp_vs_incumbent = if eligibility.rdtscp {
    evaluate_challenger(samples.rdtscp, incumbent_samples, counter_hz)
  } else {
    unavailable_decision(samples.rdtscp, incumbent_samples)
  };
  if rdtscp_vs_incumbent.challenger_selected {
    selected = ORDERED_READ_RDTSCP;
  }
  #[cfg(feature = "bench-internal")]
  let serialize_incumbent = selected;
  let incumbent_samples = candidate_samples(samples, selected);
  let serialize_vs_incumbent = if eligibility.serialize {
    evaluate_challenger(samples.serialize, incumbent_samples, counter_hz)
  } else {
    unavailable_decision(samples.serialize, incumbent_samples)
  };
  if serialize_vs_incumbent.challenger_selected {
    selected = ORDERED_READ_SERIALIZE_RDTSC;
  }

  SelectionOutcome {
    selected,
    #[cfg(feature = "bench-internal")]
    lfence_vs_cpuid,
    #[cfg(feature = "bench-internal")]
    mfence_incumbent,
    #[cfg(feature = "bench-internal")]
    mfence_vs_incumbent,
    #[cfg(feature = "bench-internal")]
    rdtscp_incumbent,
    #[cfg(feature = "bench-internal")]
    rdtscp_vs_incumbent,
    #[cfg(feature = "bench-internal")]
    serialize_incumbent,
    #[cfg(feature = "bench-internal")]
    serialize_vs_incumbent,
  }
}

#[inline]
const fn candidate_samples(samples: ProbeSamples, provider: u8) -> [u64; ORDERED_PROBE_BATCHES] {
  match provider {
    ORDERED_READ_LFENCE_RDTSC => samples.lfence,
    ORDERED_READ_MFENCE_RDTSC => samples.mfence,
    ORDERED_READ_RDTSCP => samples.rdtscp,
    ORDERED_READ_SERIALIZE_RDTSC => samples.serialize,
    _ => samples.cpuid,
  }
}

fn selection_tsc_hz() -> u64 {
  if let Some(hz) = cpuid_tsc_hz() {
    return hz;
  }
  measure_selection_tsc_hz()
}

fn measure_selection_tsc_hz() -> u64 {
  const SAMPLES: usize = 3;
  const WINDOW_NS: u64 = 1_000_000;
  let mut samples = [0_u64; SAMPLES];

  for sample in &mut samples {
    let wall_start = crate::arch::fallback::clock_monotonic();
    let tick_start = measurement_tsc();
    let mut wall_end;
    loop {
      wall_end = crate::arch::fallback::clock_monotonic();
      if wall_end.saturating_sub(wall_start) >= WINDOW_NS {
        break;
      }
      core::hint::spin_loop();
    }
    let tick_end = measurement_tsc();
    let wall_elapsed = wall_end.saturating_sub(wall_start).max(1);
    let ticks = tick_end.saturating_sub(tick_start);
    *sample =
      u64::try_from(u128::from(ticks).saturating_mul(1_000_000_000) / u128::from(wall_elapsed))
        .unwrap_or(u64::MAX);
  }

  median(samples).max(1)
}

#[cfg(test)]
#[inline]
fn prefer_challenger(
  challenger: [u64; ORDERED_PROBE_BATCHES],
  incumbent: [u64; ORDERED_PROBE_BATCHES],
  counter_hz: u64,
) -> bool {
  evaluate_challenger(challenger, incumbent, counter_hz).challenger_selected
}

#[inline]
fn evaluate_challenger(
  challenger: [u64; ORDERED_PROBE_BATCHES],
  incumbent: [u64; ORDERED_PROBE_BATCHES],
  counter_hz: u64,
) -> SelectionDecision {
  let challenger_median = median(challenger);
  let incumbent_median = median(incumbent);
  let allowance = selection_allowance(incumbent_median, counter_hz);
  let decisive_wins = challenger
    .iter()
    .zip(incumbent)
    .filter(|(challenger, incumbent)| (**challenger).saturating_add(allowance) < *incumbent)
    .count();

  SelectionDecision {
    #[cfg(feature = "bench-internal")]
    challenger_median,
    #[cfg(feature = "bench-internal")]
    incumbent_median,
    #[cfg(feature = "bench-internal")]
    allowance,
    #[cfg(feature = "bench-internal")]
    decisive_wins,
    challenger_selected: challenger_median.saturating_add(allowance) < incumbent_median
      && decisive_wins >= REQUIRED_DECISIVE_WINS,
  }
}

#[inline]
fn unavailable_decision(
  challenger: [u64; ORDERED_PROBE_BATCHES],
  incumbent: [u64; ORDERED_PROBE_BATCHES],
) -> SelectionDecision {
  #[cfg(not(feature = "bench-internal"))]
  let _ = (challenger, incumbent);
  SelectionDecision {
    #[cfg(feature = "bench-internal")]
    challenger_median: median(challenger),
    #[cfg(feature = "bench-internal")]
    incumbent_median: median(incumbent),
    #[cfg(feature = "bench-internal")]
    allowance: 0,
    #[cfg(feature = "bench-internal")]
    decisive_wins: 0,
    challenger_selected: false,
  }
}

#[inline]
fn selection_allowance(incumbent_total: u64, counter_hz: u64) -> u64 {
  let one_ns_total = (u128::from(counter_hz) * u128::from(ORDERED_PROBE_READS))
    .saturating_add(999_999_999)
    / 1_000_000_000;
  u64::try_from(one_ns_total).unwrap_or(u64::MAX).max(incumbent_total / 20)
}

#[inline]
fn median<const N: usize>(mut samples: [u64; N]) -> u64 {
  samples.sort_unstable();
  samples[N / 2]
}

#[cfg(feature = "bench-internal")]
const fn ordered_provider_name(provider: u8) -> &'static str {
  match provider {
    ORDERED_READ_LFENCE_RDTSC => "x86_lfence_rdtsc",
    ORDERED_READ_MFENCE_RDTSC => "x86_mfence_rdtsc",
    ORDERED_READ_RDTSCP => "x86_rdtscp",
    ORDERED_READ_SERIALIZE_RDTSC => "x86_serialize_rdtsc",
    _ => "x86_cpuid_rdtsc",
  }
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_lfence_rdtsc() -> u64 {
  read_lfence_rdtsc()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_mfence_rdtsc() -> u64 {
  read_mfence_rdtsc()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(dead_code)] // Non-x86-wall benchmark targets do not expose every direct candidate.
pub(crate) fn bench_cpuid_rdtsc() -> u64 {
  read_cpuid_rdtsc()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_rdtscp() -> u64 {
  read_rdtscp()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_serialize_rdtsc() -> u64 {
  read_serialize_rdtsc()
}

#[cfg(feature = "bench-internal")]
#[allow(unused_unsafe)] // supported rustc versions differ on whether __cpuid is unsafe
pub(crate) fn bench_ordered_eligibility() -> (bool, bool, bool, bool) {
  // SAFETY: x86_64 guarantees the queried CPUID leaves.
  let basic = unsafe { __cpuid(0) };
  // SAFETY: basic leaf 1 is part of the x86_64 architectural baseline.
  let leaf1 = unsafe { __cpuid(1) };
  // SAFETY: extended leaf 0 is the architected maximum-leaf query.
  let extended = unsafe { __cpuid(0x8000_0000) };
  let extended_edx = if extended.eax >= 0x8000_0001 {
    // SAFETY: the maximum extended leaf reported above includes this leaf.
    unsafe { __cpuid(0x8000_0001) }.edx
  } else {
    0
  };
  let extended21_eax = if extended.eax >= 0x8000_0021 {
    // SAFETY: the maximum extended leaf reported above includes this leaf.
    unsafe { __cpuid(0x8000_0021) }.eax
  } else {
    0
  };
  let leaf7_edx = if basic.eax >= 7 {
    // SAFETY: the maximum basic leaf reported above includes leaf 7, subleaf 0.
    unsafe { __cpuid_count(7, 0) }.edx
  } else {
    0
  };
  let eligibility = ordered_read_eligibility(
    basic.ebx,
    basic.edx,
    basic.ecx,
    leaf1.edx,
    extended_edx,
    extended21_eax,
    leaf7_edx,
  );
  (eligibility.lfence, eligibility.rdtscp, eligibility.mfence, eligibility.serialize)
}

#[cfg(feature = "bench-internal")]
#[cfg_attr(target_os = "freebsd", allow(dead_code))]
pub(crate) fn bench_selected_ordered_provider() -> &'static str {
  core::hint::black_box(rdtsc_ordered());
  ordered_provider_name(ORDERED_READ_KIND.load(Ordering::Relaxed))
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_ordered_selection_evidence() -> Option<OrderedProbeEvidence> {
  core::hint::black_box(rdtsc_ordered());
  if !ORDERED_PROBE_EVIDENCE_READY.load(Ordering::Acquire) {
    return None;
  }
  // SAFETY: the Acquire load observes the completed evidence write, and the
  // evidence is immutable for the rest of this process.
  Some(unsafe { *(*ORDERED_PROBE_EVIDENCE.0.get()).assume_init_ref() })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn intel_sse2_makes_both_fast_reads_eligible() {
    assert_eq!(
      ordered_read_eligibility(
        u32::from_le_bytes(*b"Genu"),
        u32::from_le_bytes(*b"ineI"),
        u32::from_le_bytes(*b"ntel"),
        1 << 26,
        1 << 27,
        0,
        0,
      ),
      OrderedReadEligibility { lfence: true, mfence: false, rdtscp: true, serialize: false },
    );
  }

  #[test]
  fn amd_mfence_and_rdtscp_are_capability_gated() {
    let amd =
      (u32::from_le_bytes(*b"Auth"), u32::from_le_bytes(*b"enti"), u32::from_le_bytes(*b"cAMD"));
    assert_eq!(
      ordered_read_eligibility(amd.0, amd.1, amd.2, 1 << 26, 1 << 27, 0, 0),
      OrderedReadEligibility { lfence: false, mfence: true, rdtscp: true, serialize: false },
    );
    assert_eq!(ordered_read_eligibility(amd.0, amd.1, amd.2, 0, 0, 0, 0), OrderedReadEligibility {
      lfence: false,
      mfence: false,
      rdtscp: false,
      serialize: false
    },);
  }

  #[test]
  fn amd_lfence_uses_the_architectural_serializing_capability() {
    let amd =
      (u32::from_le_bytes(*b"Auth"), u32::from_le_bytes(*b"enti"), u32::from_le_bytes(*b"cAMD"));
    assert_eq!(
      ordered_read_eligibility(amd.0, amd.1, amd.2, 1 << 26, 0, 1 << 2, 0),
      OrderedReadEligibility { lfence: true, mfence: true, rdtscp: false, serialize: false },
    );
    assert_eq!(
      ordered_read_eligibility(amd.0, amd.1, amd.2, 0, 0, 1 << 2, 0),
      OrderedReadEligibility { lfence: false, mfence: false, rdtscp: false, serialize: false },
    );
  }

  #[test]
  fn unknown_vendors_only_use_vendor_neutral_rdtscp() {
    assert_eq!(
      ordered_read_eligibility(0, 0, 0, 1 << 26, 1 << 27, 1 << 2, 0),
      OrderedReadEligibility { lfence: false, mfence: false, rdtscp: true, serialize: false },
    );
  }

  #[test]
  fn serialize_is_gated_only_by_its_architectural_feature_bit() {
    assert_eq!(ordered_read_eligibility(0, 0, 0, 0, 0, 0, 1 << 14), OrderedReadEligibility {
      lfence: false,
      mfence: false,
      rdtscp: false,
      serialize: true
    },);
  }

  #[test]
  fn selection_requires_a_repeatable_material_win() {
    let incumbent = [100_000; ORDERED_PROBE_BATCHES];
    let decisive = [90_000; ORDERED_PROBE_BATCHES];
    assert!(prefer_challenger(decisive, incumbent, 1_000_000_000));

    let mut one_noisy_batch = decisive;
    one_noisy_batch[0] = 100_000;
    assert!(prefer_challenger(one_noisy_batch, incumbent, 1_000_000_000));

    let mut two_noisy_batches = decisive;
    two_noisy_batches[0] = 100_000;
    two_noisy_batches[1] = 100_000;
    assert!(!prefer_challenger(two_noisy_batches, incumbent, 1_000_000_000));
    assert!(!prefer_challenger([96_000; ORDERED_PROBE_BATCHES], incumbent, 1_000_000_000));
  }

  #[test]
  fn selector_keeps_cpuid_unless_a_fast_path_materially_wins() {
    let samples = ProbeSamples {
      cpuid: [100_000; ORDERED_PROBE_BATCHES],
      lfence: [96_000; ORDERED_PROBE_BATCHES],
      mfence: [98_000; ORDERED_PROBE_BATCHES],
      rdtscp: [97_000; ORDERED_PROBE_BATCHES],
      serialize: [99_000; ORDERED_PROBE_BATCHES],
    };
    assert_eq!(
      select_ordered_candidate(
        OrderedReadEligibility { lfence: true, mfence: true, rdtscp: true, serialize: true },
        samples,
        1_000_000_000,
      )
      .selected,
      ORDERED_READ_CPUID_RDTSC
    );
  }

  #[test]
  fn selector_compares_each_fast_read_with_the_best_material_incumbent() {
    let samples = ProbeSamples {
      cpuid: [100_000; ORDERED_PROBE_BATCHES],
      lfence: [90_000; ORDERED_PROBE_BATCHES],
      mfence: [80_000; ORDERED_PROBE_BATCHES],
      rdtscp: [70_000; ORDERED_PROBE_BATCHES],
      serialize: [60_000; ORDERED_PROBE_BATCHES],
    };
    assert_eq!(
      select_ordered_candidate(
        OrderedReadEligibility { lfence: true, mfence: true, rdtscp: true, serialize: false },
        samples,
        1_000_000_000,
      )
      .selected,
      ORDERED_READ_RDTSCP
    );

    let material_tie = ProbeSamples { rdtscp: [77_000; ORDERED_PROBE_BATCHES], ..samples };
    assert_eq!(
      select_ordered_candidate(
        OrderedReadEligibility { lfence: true, mfence: true, rdtscp: true, serialize: false },
        material_tie,
        1_000_000_000,
      )
      .selected,
      ORDERED_READ_MFENCE_RDTSC
    );

    assert_eq!(
      select_ordered_candidate(
        OrderedReadEligibility { lfence: false, mfence: true, rdtscp: false, serialize: false },
        samples,
        1_000_000_000,
      )
      .selected,
      ORDERED_READ_MFENCE_RDTSC
    );

    assert_eq!(
      select_ordered_candidate(
        OrderedReadEligibility { lfence: true, mfence: true, rdtscp: true, serialize: true },
        samples,
        1_000_000_000,
      )
      .selected,
      ORDERED_READ_SERIALIZE_RDTSC
    );
  }
}
