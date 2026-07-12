//! Linux RISC-V wall-clock reads and runtime provider selection.
//!
//! Linux enables user TIME-CSR access on every hart and its vDSO reads that
//! CSR directly, including implementations where firmware handles the read as
//! a trap. The hwprobe UAPI reports ISA extensions as a logical intersection
//! over all online CPUs and only exposes extensions that are usable from
//! userspace. Tach therefore considers `rdtime` only when hwprobe reports both
//! Zicntr and a nonzero TIME-CSR frequency. Kernels that predate those keys use
//! the measured libc-vDSO or raw-syscall provider without executing `rdtime`.
//!
//! CLOCK_MONOTONIC, CLOCK_MONOTONIC_RAW, and CLOCK_BOOTTIME libc, raw-syscall,
//! and direct-vDSO routes compete independently. Ordered raw-syscall routes
//! also compete without a separate pre-fence because
//! ECALL is a precise requested trap. Libc/vDSO and direct `rdtime` routes are
//! not eligible for that assumption and retain their explicit FENCE sequences.

#[cfg(feature = "bench-internal")]
use core::cell::UnsafeCell;
use core::hint::black_box;
use core::mem::MaybeUninit;
#[cfg(feature = "bench-internal")]
use core::sync::atomic::AtomicBool;
use core::sync::atomic::{AtomicI32, AtomicU8, AtomicU64, Ordering};

const PROVIDER_UNKNOWN: u8 = 0;
const PROVIDER_SELECTING: u8 = 1;
const PROVIDER_RDTIME: u8 = 2;
const PROVIDER_CLOCK_MONOTONIC: u8 = 3;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL: u8 = 4;
const PROVIDER_CLOCK_MONOTONIC_RAW: u8 = 5;
const PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL: u8 = 6;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED: u8 = 7;
const PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED: u8 = 8;
const PROVIDER_CLOCK_MONOTONIC_VDSO: u8 = 9;
const PROVIDER_CLOCK_MONOTONIC_RAW_VDSO: u8 = 10;
const PROVIDER_CLOCK_BOOTTIME: u8 = 11;
const PROVIDER_CLOCK_BOOTTIME_SYSCALL: u8 = 12;
const PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED: u8 = 13;
const PROVIDER_CLOCK_BOOTTIME_VDSO: u8 = 14;
const MAX_CANDIDATES: usize = 13;

const PROBE_BATCHES: usize = 9;
const PROBE_READS: u64 = 4096;
const PROBE_WARMUP_READS: u64 = 1024;
const REQUIRED_DECISIVE_WINS: usize = 8;

const fn is_os_ordered_provider(provider: u8) -> bool {
  matches!(
    provider,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED
      | PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED
      | PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED
  )
}

const RISCV_HWPROBE_SYSCALL: usize = 258;
const RISCV_HWPROBE_KEY_IMA_EXT_0: i64 = 4;
const RISCV_HWPROBE_KEY_TIME_CSR_FREQ: i64 = 8;
const RISCV_HWPROBE_EXT_ZICNTR: u64 = 1_u64 << 50;

static INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static INSTANT_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static INSTANT_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static ORDERED_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static PROBE_INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_RDTIME);
static PROBE_ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_RDTIME);
static TIME_CSR_FREQUENCY: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WallProvider {
  Rdtime,
  ClockMonotonic,
  ClockMonotonicSyscall,
  ClockMonotonicSyscallOsOrdered,
  ClockMonotonicRaw,
  ClockMonotonicRawSyscall,
  ClockMonotonicRawSyscallOsOrdered,
  ClockMonotonicVdso,
  ClockMonotonicRawVdso,
  ClockBoottime,
  ClockBoottimeSyscall,
  ClockBoottimeSyscallOsOrdered,
  ClockBoottimeVdso,
}

#[cfg(feature = "bench-internal")]
impl WallProvider {
  pub(crate) const fn name(self) -> &'static str {
    match self {
      Self::Rdtime => "riscv_rdtime",
      Self::ClockMonotonic => "linux_clock_monotonic",
      Self::ClockMonotonicSyscall => "linux_clock_monotonic_syscall",
      Self::ClockMonotonicSyscallOsOrdered => "linux_clock_monotonic_syscall_os_ordered",
      Self::ClockMonotonicRaw => "linux_clock_monotonic_raw",
      Self::ClockMonotonicRawSyscall => "linux_clock_monotonic_raw_syscall",
      Self::ClockMonotonicRawSyscallOsOrdered => "linux_clock_monotonic_raw_syscall_os_ordered",
      Self::ClockMonotonicVdso => "linux_clock_monotonic_vdso_direct",
      Self::ClockMonotonicRawVdso => "linux_clock_monotonic_raw_vdso_direct",
      Self::ClockBoottime => "linux_clock_boottime",
      Self::ClockBoottimeSyscall => "linux_clock_boottime_syscall",
      Self::ClockBoottimeSyscallOsOrdered => "linux_clock_boottime_syscall_os_ordered",
      Self::ClockBoottimeVdso => "linux_clock_boottime_vdso_direct",
    }
  }
}

#[derive(Clone, Copy, Debug)]
struct SelectionDecision {
  #[cfg(feature = "bench-internal")]
  allowance: u64,
  #[cfg(feature = "bench-internal")]
  decisive_wins: usize,
  challenger_selected: bool,
}

#[derive(Clone, Copy, Debug)]
struct ProbeSamples {
  direct: [u64; PROBE_BATCHES],
  clock: [u64; PROBE_BATCHES],
  syscall: [u64; PROBE_BATCHES],
  syscall_os_ordered: [u64; PROBE_BATCHES],
  clock_raw: [u64; PROBE_BATCHES],
  syscall_raw: [u64; PROBE_BATCHES],
  syscall_raw_os_ordered: [u64; PROBE_BATCHES],
  vdso: [u64; PROBE_BATCHES],
  vdso_raw: [u64; PROBE_BATCHES],
  clock_boottime: [u64; PROBE_BATCHES],
  syscall_boottime: [u64; PROBE_BATCHES],
  syscall_boottime_os_ordered: [u64; PROBE_BATCHES],
  vdso_boottime: [u64; PROBE_BATCHES],
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // Architecture evidence is complete even when a serializer projects a subset.
pub(crate) struct ProbeEvidence {
  pub(crate) candidate_count: usize,
  pub(crate) candidate_providers: [u8; MAX_CANDIDATES],
  pub(crate) direct_eligible: bool,
  pub(crate) clock_raw_available: bool,
  pub(crate) syscall_raw_available: bool,
  pub(crate) vdso_available: bool,
  pub(crate) vdso_raw_available: bool,
  pub(crate) clock_boottime_available: bool,
  pub(crate) syscall_boottime_available: bool,
  pub(crate) vdso_boottime_available: bool,
  pub(crate) reads_per_batch: u64,
  pub(crate) direct_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_raw_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_boottime_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) direct_median_ns: u64,
  pub(crate) clock_median_ns: u64,
  pub(crate) syscall_median_ns: u64,
  pub(crate) syscall_os_ordered_median_ns: u64,
  pub(crate) clock_raw_median_ns: u64,
  pub(crate) syscall_raw_median_ns: u64,
  pub(crate) syscall_raw_os_ordered_median_ns: u64,
  pub(crate) vdso_median_ns: u64,
  pub(crate) vdso_raw_median_ns: u64,
  pub(crate) clock_boottime_median_ns: u64,
  pub(crate) syscall_boottime_median_ns: u64,
  pub(crate) syscall_boottime_os_ordered_median_ns: u64,
  pub(crate) vdso_boottime_median_ns: u64,
  pub(crate) fallback_allowance_ns: u64,
  pub(crate) fallback_decisive_wins: usize,
  pub(crate) syscall_os_ordered_allowance_ns: u64,
  pub(crate) syscall_os_ordered_decisive_wins: usize,
  pub(crate) clock_raw_allowance_ns: u64,
  pub(crate) clock_raw_decisive_wins: usize,
  pub(crate) syscall_raw_allowance_ns: u64,
  pub(crate) syscall_raw_decisive_wins: usize,
  pub(crate) syscall_raw_os_ordered_allowance_ns: u64,
  pub(crate) syscall_raw_os_ordered_decisive_wins: usize,
  pub(crate) vdso_allowance_ns: u64,
  pub(crate) vdso_decisive_wins: usize,
  pub(crate) vdso_raw_allowance_ns: u64,
  pub(crate) vdso_raw_decisive_wins: usize,
  pub(crate) clock_boottime_allowance_ns: u64,
  pub(crate) clock_boottime_decisive_wins: usize,
  pub(crate) syscall_boottime_allowance_ns: u64,
  pub(crate) syscall_boottime_decisive_wins: usize,
  pub(crate) syscall_boottime_os_ordered_allowance_ns: u64,
  pub(crate) syscall_boottime_os_ordered_decisive_wins: usize,
  pub(crate) vdso_boottime_allowance_ns: u64,
  pub(crate) vdso_boottime_decisive_wins: usize,
  pub(crate) direct_allowance_ns: u64,
  pub(crate) direct_decisive_wins: usize,
  pub(crate) required_decisive_wins: usize,
  pub(crate) selected_provider: WallProvider,
}

#[cfg(feature = "bench-internal")]
struct EvidenceCell(UnsafeCell<MaybeUninit<ProbeEvidence>>);

// SAFETY: the process-selection owner writes its evidence before publishing
// the matching provider with Release. Readers first load that provider with
// Acquire. A fork child writes only its private COW copy after recovery.
#[cfg(feature = "bench-internal")]
unsafe impl Sync for EvidenceCell {}

#[cfg(feature = "bench-internal")]
static INSTANT_EVIDENCE: EvidenceCell = EvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));
#[cfg(feature = "bench-internal")]
static ORDERED_EVIDENCE: EvidenceCell = EvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));
#[cfg(feature = "bench-internal")]
static INSTANT_EVIDENCE_READY: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "bench-internal")]
static ORDERED_EVIDENCE_READY: AtomicBool = AtomicBool::new(false);

/// Reads the architectural TIME CSR.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn rdtime() -> u64 {
  let count: u64;
  // SAFETY: eligibility is established before this primitive enters a public
  // route. `rdtime` only writes a general-purpose register.
  unsafe {
    core::arch::asm!(
      "rdtime {count}",
      count = out(reg) count,
      options(nostack, nomem, preserves_flags),
    );
  }
  count
}

/// Orders prior memory observations before the TIME-CSR input.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn rdtime_ordered() -> u64 {
  let count: u64;
  // SAFETY: ratified Zicsr classifies CSR reads as device input for FENCE.
  // Omitting `nomem` supplies the matching compiler barrier.
  unsafe {
    core::arch::asm!(
      "fence r, i",
      "rdtime {count}",
      count = out(reg) count,
      options(nostack, preserves_flags),
    );
  }
  count
}

/// Brackets a TIME-CSR read for a perf userpage sequence-counter read.
#[inline(always)]
#[allow(clippy::inline_always)]
#[cfg(feature = "thread-cpu-inline")]
pub(crate) fn rdtime_perf_seqlock() -> u64 {
  let count: u64;
  // SAFETY: ratified Zicsr classifies a CSR read as device input for FENCE.
  // The first fence orders metadata reads before TIME; the second orders TIME
  // before the closing metadata-lock read. Omitting `nomem` supplies the same
  // bidirectional ordering to the compiler.
  unsafe {
    core::arch::asm!(
      "fence r, i",
      "rdtime {count}",
      "fence i, r",
      count = out(reg) count,
      options(nostack, preserves_flags),
    );
  }
  count
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  match INSTANT_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_RDTIME => rdtime(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => ticks_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_after_selection() -> u64 {
  read_instant_provider(selected_instant_provider())
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  match ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_RDTIME => rdtime_ordered(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED => clock_monotonic_syscall_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED => clock_monotonic_raw_syscall_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso_ordered(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED => clock_boottime_syscall_os_ordered(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered(),
    _ => ticks_ordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_after_selection() -> u64 {
  read_ordered_provider(selected_ordered_provider())
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  match ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_RDTIME => rdtime(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL | PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED => {
      clock_monotonic_syscall()
    }
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL | PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED => {
      clock_monotonic_raw_syscall()
    }
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL | PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED => {
      clock_boottime_syscall()
    }
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => ticks_ordered_unordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_unordered_after_selection() -> u64 {
  read_instant_provider(selected_ordered_provider())
}

#[inline]
pub(crate) fn instant_frequency() -> u64 {
  if instant_uses_rdtime() {
    TIME_CSR_FREQUENCY.load(Ordering::Acquire).max(1)
  } else {
    1_000_000_000
  }
}

#[inline]
pub(crate) fn ordered_frequency() -> u64 {
  if ordered_uses_rdtime() {
    TIME_CSR_FREQUENCY.load(Ordering::Acquire).max(1)
  } else {
    1_000_000_000
  }
}

#[inline]
pub(crate) fn instant_uses_rdtime() -> bool {
  selected_instant_provider() == PROVIDER_RDTIME
}

#[inline]
pub(crate) fn instant_read_cost() -> crate::ThreadCpuReadCost {
  instant_read_cost_for(selected_instant_provider())
}

const fn instant_read_cost_for(provider: u8) -> crate::ThreadCpuReadCost {
  match provider {
    PROVIDER_RDTIME
    | PROVIDER_CLOCK_MONOTONIC_VDSO
    | PROVIDER_CLOCK_MONOTONIC_RAW_VDSO
    | PROVIDER_CLOCK_BOOTTIME_VDSO => crate::ThreadCpuReadCost::Inline,
    // The libc ABI may use the vDSO or enter the kernel. Only the selected CSR
    // route is guaranteed to remain in userspace.
    _ => crate::ThreadCpuReadCost::SystemCall,
  }
}

#[cfg(test)]
#[test]
fn instant_read_cost_only_marks_guaranteed_userspace_paths_inline() {
  assert_eq!(instant_read_cost_for(PROVIDER_RDTIME), crate::ThreadCpuReadCost::Inline);
  assert_eq!(instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC), crate::ThreadCpuReadCost::SystemCall,);
  assert_eq!(
    instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC_RAW),
    crate::ThreadCpuReadCost::SystemCall,
  );
  assert_eq!(
    instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC_SYSCALL),
    crate::ThreadCpuReadCost::SystemCall,
  );
  assert_eq!(instant_read_cost_for(PROVIDER_CLOCK_BOOTTIME_VDSO), crate::ThreadCpuReadCost::Inline,);
}

#[inline]
pub(crate) fn ordered_uses_rdtime() -> bool {
  selected_ordered_provider() == PROVIDER_RDTIME
}

fn selected_instant_provider() -> u8 {
  select_provider(
    &INSTANT_PROVIDER,
    &INSTANT_PROVIDER_OWNER_PID,
    &INSTANT_PROVIDER_OWNER_TID,
    false,
  )
}

fn selected_ordered_provider() -> u8 {
  select_provider(&ORDERED_PROVIDER, &ORDERED_PROVIDER_OWNER_PID, &ORDERED_PROVIDER_OWNER_TID, true)
}

fn select_provider(
  state: &AtomicU8,
  owner_pid: &AtomicI32,
  owner_tid: &AtomicI32,
  ordered: bool,
) -> u8 {
  let fallback = if ordered {
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED
  } else {
    PROVIDER_CLOCK_MONOTONIC_SYSCALL
  };
  super::select_thread_owned_process_provider(
    state,
    PROVIDER_UNKNOWN,
    PROVIDER_SELECTING,
    owner_pid,
    owner_tid,
    fallback,
    || detect_provider(ordered),
  )
}

#[cold]
#[inline(never)]
fn detect_provider(ordered: bool) -> u8 {
  let direct_frequency = direct_eligibility();
  let direct_eligible = direct_frequency.is_some();
  if let Some(frequency) = direct_frequency {
    TIME_CSR_FREQUENCY.store(frequency, Ordering::Release);
  }

  let clock_raw_available = libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW).is_some();
  let syscall_raw_available = raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW).is_some();
  let _ = super::linux_vdso::install();
  let vdso_available = super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC).is_some();
  let vdso_raw_available = super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC_RAW).is_some();
  let clock_boottime_available = libc_clock_nanos(libc::CLOCK_BOOTTIME).is_some();
  let syscall_boottime_available = raw_clock_nanos(libc::CLOCK_BOOTTIME).is_some();
  let vdso_boottime_available = super::linux_vdso::clock_nanos(libc::CLOCK_BOOTTIME).is_some();
  let (samples, candidate_providers, candidate_count) = measure_candidates(
    ordered,
    direct_eligible,
    clock_raw_available,
    syscall_raw_available,
    vdso_available,
    vdso_raw_available,
    clock_boottime_available,
    syscall_boottime_available,
    vdso_boottime_available,
  );
  #[cfg(not(feature = "bench-internal"))]
  let _ = (candidate_providers, candidate_count);
  let fallback_decision = prefer_challenger(samples.syscall, samples.clock);
  let mut fallback = if fallback_decision.challenger_selected {
    PROVIDER_CLOCK_MONOTONIC_SYSCALL
  } else {
    PROVIDER_CLOCK_MONOTONIC
  };
  let syscall_os_ordered_decision = if ordered {
    prefer_challenger(samples.syscall_os_ordered, candidate_samples(samples, fallback))
  } else {
    empty_decision()
  };
  if syscall_os_ordered_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED;
  }
  let clock_raw_decision = if clock_raw_available {
    prefer_challenger(samples.clock_raw, candidate_samples(samples, fallback))
  } else {
    empty_decision()
  };
  if clock_raw_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_RAW;
  }
  let syscall_raw_decision = if syscall_raw_available {
    prefer_challenger(samples.syscall_raw, candidate_samples(samples, fallback))
  } else {
    empty_decision()
  };
  if syscall_raw_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL;
  }
  let syscall_raw_os_ordered_decision = if ordered && syscall_raw_available {
    prefer_challenger(samples.syscall_raw_os_ordered, candidate_samples(samples, fallback))
  } else {
    empty_decision()
  };
  if syscall_raw_os_ordered_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED;
  }
  let vdso_decision = if vdso_available {
    prefer_challenger(samples.vdso, candidate_samples(samples, fallback))
  } else {
    empty_decision()
  };
  if vdso_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_VDSO;
  }
  let vdso_raw_decision = if vdso_raw_available {
    prefer_challenger(samples.vdso_raw, candidate_samples(samples, fallback))
  } else {
    empty_decision()
  };
  if vdso_raw_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_RAW_VDSO;
  }
  let clock_boottime_decision = if clock_boottime_available {
    prefer_challenger(samples.clock_boottime, candidate_samples(samples, fallback))
  } else {
    empty_decision()
  };
  if clock_boottime_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_BOOTTIME;
  }
  let syscall_boottime_decision = if syscall_boottime_available {
    prefer_challenger(samples.syscall_boottime, candidate_samples(samples, fallback))
  } else {
    empty_decision()
  };
  if syscall_boottime_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_BOOTTIME_SYSCALL;
  }
  let syscall_boottime_os_ordered_decision = if ordered && syscall_boottime_available {
    prefer_challenger(samples.syscall_boottime_os_ordered, candidate_samples(samples, fallback))
  } else {
    empty_decision()
  };
  if syscall_boottime_os_ordered_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED;
  }
  let vdso_boottime_decision = if vdso_boottime_available {
    prefer_challenger(samples.vdso_boottime, candidate_samples(samples, fallback))
  } else {
    empty_decision()
  };
  if vdso_boottime_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_BOOTTIME_VDSO;
  }
  let fallback_samples = candidate_samples(samples, fallback);
  let direct_decision = if direct_eligible {
    prefer_challenger(samples.direct, fallback_samples)
  } else {
    empty_decision()
  };
  let provider = if direct_decision.challenger_selected { PROVIDER_RDTIME } else { fallback };

  #[cfg(feature = "bench-internal")]
  publish_evidence(ordered, ProbeEvidence {
    candidate_count,
    candidate_providers,
    direct_eligible,
    clock_raw_available,
    syscall_raw_available,
    vdso_available,
    vdso_raw_available,
    clock_boottime_available,
    syscall_boottime_available,
    vdso_boottime_available,
    reads_per_batch: PROBE_READS,
    direct_batches_ns: samples.direct,
    clock_batches_ns: samples.clock,
    syscall_batches_ns: samples.syscall,
    syscall_os_ordered_batches_ns: samples.syscall_os_ordered,
    clock_raw_batches_ns: samples.clock_raw,
    syscall_raw_batches_ns: samples.syscall_raw,
    syscall_raw_os_ordered_batches_ns: samples.syscall_raw_os_ordered,
    vdso_batches_ns: samples.vdso,
    vdso_raw_batches_ns: samples.vdso_raw,
    clock_boottime_batches_ns: samples.clock_boottime,
    syscall_boottime_batches_ns: samples.syscall_boottime,
    syscall_boottime_os_ordered_batches_ns: samples.syscall_boottime_os_ordered,
    vdso_boottime_batches_ns: samples.vdso_boottime,
    direct_median_ns: median(samples.direct),
    clock_median_ns: median(samples.clock),
    syscall_median_ns: median(samples.syscall),
    syscall_os_ordered_median_ns: median(samples.syscall_os_ordered),
    clock_raw_median_ns: median(samples.clock_raw),
    syscall_raw_median_ns: median(samples.syscall_raw),
    syscall_raw_os_ordered_median_ns: median(samples.syscall_raw_os_ordered),
    vdso_median_ns: median(samples.vdso),
    vdso_raw_median_ns: median(samples.vdso_raw),
    clock_boottime_median_ns: median(samples.clock_boottime),
    syscall_boottime_median_ns: median(samples.syscall_boottime),
    syscall_boottime_os_ordered_median_ns: median(samples.syscall_boottime_os_ordered),
    vdso_boottime_median_ns: median(samples.vdso_boottime),
    fallback_allowance_ns: fallback_decision.allowance,
    fallback_decisive_wins: fallback_decision.decisive_wins,
    syscall_os_ordered_allowance_ns: syscall_os_ordered_decision.allowance,
    syscall_os_ordered_decisive_wins: syscall_os_ordered_decision.decisive_wins,
    clock_raw_allowance_ns: clock_raw_decision.allowance,
    clock_raw_decisive_wins: clock_raw_decision.decisive_wins,
    syscall_raw_allowance_ns: syscall_raw_decision.allowance,
    syscall_raw_decisive_wins: syscall_raw_decision.decisive_wins,
    syscall_raw_os_ordered_allowance_ns: syscall_raw_os_ordered_decision.allowance,
    syscall_raw_os_ordered_decisive_wins: syscall_raw_os_ordered_decision.decisive_wins,
    vdso_allowance_ns: vdso_decision.allowance,
    vdso_decisive_wins: vdso_decision.decisive_wins,
    vdso_raw_allowance_ns: vdso_raw_decision.allowance,
    vdso_raw_decisive_wins: vdso_raw_decision.decisive_wins,
    clock_boottime_allowance_ns: clock_boottime_decision.allowance,
    clock_boottime_decisive_wins: clock_boottime_decision.decisive_wins,
    syscall_boottime_allowance_ns: syscall_boottime_decision.allowance,
    syscall_boottime_decisive_wins: syscall_boottime_decision.decisive_wins,
    syscall_boottime_os_ordered_allowance_ns: syscall_boottime_os_ordered_decision.allowance,
    syscall_boottime_os_ordered_decisive_wins: syscall_boottime_os_ordered_decision.decisive_wins,
    vdso_boottime_allowance_ns: vdso_boottime_decision.allowance,
    vdso_boottime_decisive_wins: vdso_boottime_decision.decisive_wins,
    direct_allowance_ns: direct_decision.allowance,
    direct_decisive_wins: direct_decision.decisive_wins,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    selected_provider: provider_from_raw(provider),
  });

  provider
}

fn direct_eligibility() -> Option<u64> {
  #[repr(C)]
  struct HwprobePair {
    key: i64,
    value: u64,
  }

  let mut pairs = [HwprobePair { key: RISCV_HWPROBE_KEY_IMA_EXT_0, value: 0 }, HwprobePair {
    key: RISCV_HWPROBE_KEY_TIME_CSR_FREQ,
    value: 0,
  }];
  let mut argument = pairs.as_mut_ptr() as usize;
  // SAFETY: syscall 258 is riscv_hwprobe. The two-element array is writable,
  // a null CPU set requests the intersection over all online CPUs, and flags
  // zero is the forward-compatible value-query mode.
  unsafe {
    core::arch::asm!(
      "ecall",
      inlateout("a0") argument,
      in("a1") pairs.len(),
      in("a2") 0_usize,
      in("a3") 0_usize,
      in("a4") 0_usize,
      in("a7") RISCV_HWPROBE_SYSCALL,
      options(nostack),
    );
  }
  if argument != 0
    || pairs[0].key != RISCV_HWPROBE_KEY_IMA_EXT_0
    || pairs[1].key != RISCV_HWPROBE_KEY_TIME_CSR_FREQ
    || pairs[0].value & RISCV_HWPROBE_EXT_ZICNTR == 0
  {
    return None;
  }
  (pairs[1].value > 0).then_some(pairs[1].value)
}

/// Whether every online hart exposes a userspace-readable TIME CSR.
///
/// Perf's cap_user_time describes conversion metadata, but does not by itself
/// authorize tach to execute RDTIME. The task-clock mmap path uses this
/// independent UAPI gate too.
#[cfg(feature = "thread-cpu-inline")]
pub(crate) fn rdtime_user_eligible() -> bool {
  direct_eligibility().is_some()
}

fn measure_candidates(
  ordered: bool,
  direct_eligible: bool,
  clock_raw_available: bool,
  syscall_raw_available: bool,
  vdso_available: bool,
  vdso_raw_available: bool,
  clock_boottime_available: bool,
  syscall_boottime_available: bool,
  vdso_boottime_available: bool,
) -> (ProbeSamples, [u8; MAX_CANDIDATES], usize) {
  if direct_eligible {
    warm_candidate(ordered, PROVIDER_RDTIME);
  }
  warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC);
  warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_SYSCALL);
  if ordered {
    warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED);
  }
  if clock_raw_available {
    warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_RAW);
  }
  if syscall_raw_available {
    warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL);
    if ordered {
      warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED);
    }
  }
  if vdso_available {
    warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_VDSO);
  }
  if vdso_raw_available {
    warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_RAW_VDSO);
  }
  if clock_boottime_available {
    warm_candidate(ordered, PROVIDER_CLOCK_BOOTTIME);
  }
  if syscall_boottime_available {
    warm_candidate(ordered, PROVIDER_CLOCK_BOOTTIME_SYSCALL);
    if ordered {
      warm_candidate(ordered, PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED);
    }
  }
  if vdso_boottime_available {
    warm_candidate(ordered, PROVIDER_CLOCK_BOOTTIME_VDSO);
  }

  let mut candidates = [PROVIDER_UNKNOWN; MAX_CANDIDATES];
  let mut candidate_count = 0;
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_RDTIME, direct_eligible);
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_CLOCK_MONOTONIC, true);
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_CLOCK_MONOTONIC_SYSCALL, true);
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED,
    ordered,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_MONOTONIC_VDSO,
    vdso_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO,
    vdso_raw_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_MONOTONIC_RAW,
    clock_raw_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL,
    syscall_raw_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED,
    ordered && syscall_raw_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_BOOTTIME,
    clock_boottime_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL,
    syscall_boottime_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED,
    ordered && syscall_boottime_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_BOOTTIME_VDSO,
    vdso_boottime_available,
  );

  let mut samples = ProbeSamples {
    direct: [u64::MAX; PROBE_BATCHES],
    clock: [u64::MAX; PROBE_BATCHES],
    syscall: [u64::MAX; PROBE_BATCHES],
    syscall_os_ordered: [u64::MAX; PROBE_BATCHES],
    clock_raw: [u64::MAX; PROBE_BATCHES],
    syscall_raw: [u64::MAX; PROBE_BATCHES],
    syscall_raw_os_ordered: [u64::MAX; PROBE_BATCHES],
    vdso: [u64::MAX; PROBE_BATCHES],
    vdso_raw: [u64::MAX; PROBE_BATCHES],
    clock_boottime: [u64::MAX; PROBE_BATCHES],
    syscall_boottime: [u64::MAX; PROBE_BATCHES],
    syscall_boottime_os_ordered: [u64::MAX; PROBE_BATCHES],
    vdso_boottime: [u64::MAX; PROBE_BATCHES],
  };
  for sample in 0..PROBE_BATCHES {
    for offset in 0..candidate_count {
      let provider = candidates[(sample + offset) % candidate_count];
      if !ordered && is_os_ordered_provider(provider) {
        continue;
      }
      let elapsed = measure_batch(ordered, provider).unwrap_or(u64::MAX);
      match provider {
        PROVIDER_RDTIME => samples.direct[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC => samples.clock[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_SYSCALL => samples.syscall[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED => {
          samples.syscall_os_ordered[sample] = elapsed;
        }
        PROVIDER_CLOCK_MONOTONIC_RAW => samples.clock_raw[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => samples.syscall_raw[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED => {
          samples.syscall_raw_os_ordered[sample] = elapsed;
        }
        PROVIDER_CLOCK_MONOTONIC_VDSO => samples.vdso[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => samples.vdso_raw[sample] = elapsed,
        PROVIDER_CLOCK_BOOTTIME => samples.clock_boottime[sample] = elapsed,
        PROVIDER_CLOCK_BOOTTIME_SYSCALL => samples.syscall_boottime[sample] = elapsed,
        PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED => {
          samples.syscall_boottime_os_ordered[sample] = elapsed;
        }
        PROVIDER_CLOCK_BOOTTIME_VDSO => samples.vdso_boottime[sample] = elapsed,
        _ => unreachable!(),
      }
    }
  }
  (samples, candidates, candidate_count)
}

fn push_candidate<const N: usize>(
  candidates: &mut [u8; N],
  count: &mut usize,
  provider: u8,
  eligible: bool,
) {
  if eligible && *count < N {
    candidates[*count] = provider;
    *count += 1;
  }
}

fn candidate_samples(samples: ProbeSamples, provider: u8) -> [u64; PROBE_BATCHES] {
  match provider {
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => samples.syscall,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED => samples.syscall_os_ordered,
    PROVIDER_CLOCK_MONOTONIC_RAW => samples.clock_raw,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => samples.syscall_raw,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED => samples.syscall_raw_os_ordered,
    PROVIDER_RDTIME => samples.direct,
    PROVIDER_CLOCK_MONOTONIC_VDSO => samples.vdso,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => samples.vdso_raw,
    PROVIDER_CLOCK_BOOTTIME => samples.clock_boottime,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => samples.syscall_boottime,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED => samples.syscall_boottime_os_ordered,
    PROVIDER_CLOCK_BOOTTIME_VDSO => samples.vdso_boottime,
    _ => samples.clock,
  }
}

#[inline(never)]
fn measure_batch(ordered: bool, provider: u8) -> Option<u64> {
  if ordered {
    PROBE_ORDERED_PROVIDER.store(provider, Ordering::Relaxed);
  } else {
    PROBE_INSTANT_PROVIDER.store(provider, Ordering::Relaxed);
  }
  let start = raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW)?;
  let mut sink = 0_u64;
  if ordered {
    for _ in 0..PROBE_READS {
      sink ^= probe_ordered_hot_path();
    }
  } else {
    for _ in 0..PROBE_READS {
      sink ^= probe_instant_hot_path();
    }
  }
  let elapsed = raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW)?.checked_sub(start);
  black_box(sink);
  elapsed
}

fn warm_candidate(ordered: bool, provider: u8) {
  if ordered {
    PROBE_ORDERED_PROVIDER.store(provider, Ordering::Relaxed);
    for _ in 0..PROBE_WARMUP_READS {
      black_box(probe_ordered_hot_path());
    }
  } else {
    PROBE_INSTANT_PROVIDER.store(provider, Ordering::Relaxed);
    for _ in 0..PROBE_WARMUP_READS {
      black_box(probe_instant_hot_path());
    }
  }
}

#[inline(always)]
fn probe_instant_hot_path() -> u64 {
  read_instant_provider(PROBE_INSTANT_PROVIDER.load(Ordering::Relaxed))
}

#[inline(always)]
fn probe_ordered_hot_path() -> u64 {
  match PROBE_ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_RDTIME => rdtime_ordered(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED => clock_monotonic_syscall_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED => clock_monotonic_raw_syscall_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso_ordered(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED => clock_boottime_syscall_os_ordered(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered(),
    _ => clock_monotonic_ordered(),
  }
}

const fn empty_decision() -> SelectionDecision {
  SelectionDecision {
    #[cfg(feature = "bench-internal")]
    allowance: 0,
    #[cfg(feature = "bench-internal")]
    decisive_wins: 0,
    challenger_selected: false,
  }
}

fn prefer_challenger(
  challenger: [u64; PROBE_BATCHES],
  incumbent: [u64; PROBE_BATCHES],
) -> SelectionDecision {
  let challenger_median = median(challenger);
  let incumbent_median = median(incumbent);
  if challenger_median == u64::MAX || incumbent_median == u64::MAX {
    return empty_decision();
  }
  let allowance = (incumbent_median / 20).max(PROBE_READS);
  let decisive_wins = challenger
    .iter()
    .zip(incumbent)
    .filter(|(challenger, incumbent)| (**challenger).saturating_add(allowance) < *incumbent)
    .count();
  SelectionDecision {
    #[cfg(feature = "bench-internal")]
    allowance,
    #[cfg(feature = "bench-internal")]
    decisive_wins,
    challenger_selected: challenger_median.saturating_add(allowance) < incumbent_median
      && decisive_wins >= REQUIRED_DECISIVE_WINS,
  }
}

fn median(mut samples: [u64; PROBE_BATCHES]) -> u64 {
  samples.sort_unstable();
  samples[PROBE_BATCHES / 2]
}

#[inline(always)]
fn read_instant_provider(provider: u8) -> u64 {
  match provider {
    PROVIDER_RDTIME => rdtime(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL | PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED => {
      clock_monotonic_syscall()
    }
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL | PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED => {
      clock_monotonic_raw_syscall()
    }
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL | PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED => {
      clock_boottime_syscall()
    }
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => clock_monotonic(),
  }
}

#[inline(always)]
fn read_ordered_provider(provider: u8) -> u64 {
  match provider {
    PROVIDER_RDTIME => rdtime_ordered(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED => clock_monotonic_syscall_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED => clock_monotonic_raw_syscall_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso_ordered(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED => clock_boottime_syscall_os_ordered(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered(),
    _ => clock_monotonic_ordered(),
  }
}

#[inline(always)]
fn clock_monotonic_ordered() -> u64 {
  read_to_timer_fence();
  clock_monotonic()
}

#[inline(always)]
fn clock_monotonic() -> u64 {
  libc_clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_syscall_ordered() -> u64 {
  read_to_timer_fence();
  clock_monotonic_syscall()
}

#[inline(always)]
fn clock_monotonic_raw_ordered() -> u64 {
  read_to_timer_fence();
  clock_monotonic_raw()
}

#[inline(always)]
fn clock_monotonic_raw_syscall_ordered() -> u64 {
  read_to_timer_fence();
  clock_monotonic_raw_syscall()
}

#[inline(always)]
fn clock_boottime_ordered() -> u64 {
  read_to_timer_fence();
  clock_boottime()
}

#[inline(always)]
fn clock_boottime_syscall_ordered() -> u64 {
  read_to_timer_fence();
  clock_boottime_syscall()
}

#[inline(always)]
fn clock_monotonic_syscall_os_ordered() -> u64 {
  // ECALL is a precise requested trap: all older instructions have committed
  // before Linux handles the clock read. The asm's memory clobber supplies the
  // matching compiler-ordering edge.
  raw_clock_nanos(libc::CLOCK_MONOTONIC).unwrap_or_else(clock_monotonic_ordered)
}

#[inline(always)]
fn clock_monotonic_raw_syscall_os_ordered() -> u64 {
  raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW).unwrap_or_else(clock_monotonic_raw_ordered)
}

#[inline(always)]
fn clock_boottime_syscall_os_ordered() -> u64 {
  raw_clock_nanos(libc::CLOCK_BOOTTIME).unwrap_or_else(clock_boottime_ordered)
}

#[inline(always)]
fn read_to_timer_fence() {
  // SAFETY: Zicsr defines the R predecessor and I successor relation used by
  // the later vDSO TIME-CSR read. Omitting `nomem` is the compiler barrier.
  unsafe {
    core::arch::asm!("fence r, i", options(nostack, preserves_flags));
  }
}

#[inline(always)]
fn clock_monotonic_syscall() -> u64 {
  raw_clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_raw() -> u64 {
  libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_raw_syscall() -> u64 {
  raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_boottime() -> u64 {
  libc_clock_nanos(libc::CLOCK_BOOTTIME)
    .or_else(|| raw_clock_nanos(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_boottime_syscall() -> u64 {
  raw_clock_nanos(libc::CLOCK_BOOTTIME)
    .or_else(|| libc_clock_nanos(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_vdso() -> u64 {
  super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC))
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_raw_vdso() -> u64 {
  super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_boottime_vdso() -> u64 {
  super::linux_vdso::clock_nanos(libc::CLOCK_BOOTTIME)
    .or_else(|| libc_clock_nanos(libc::CLOCK_BOOTTIME))
    .or_else(|| raw_clock_nanos(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_vdso_ordered() -> u64 {
  read_to_timer_fence();
  clock_monotonic_vdso()
}

#[inline(always)]
fn clock_monotonic_raw_vdso_ordered() -> u64 {
  read_to_timer_fence();
  clock_monotonic_raw_vdso()
}

#[inline(always)]
fn clock_boottime_vdso_ordered() -> u64 {
  read_to_timer_fence();
  clock_boottime_vdso()
}

#[inline(always)]
fn libc_clock_nanos(clock_id: libc::clockid_t) -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: the clock id is probed before selection and the output is writable.
  let status = unsafe { libc::clock_gettime(clock_id, value.as_mut_ptr()) };
  if status != 0 {
    return None;
  }
  // SAFETY: successful clock_gettime initialized the output.
  timespec_nanos(unsafe { value.assume_init() })
}

#[inline(always)]
fn raw_clock_nanos(clock_id: libc::clockid_t) -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  let mut status = clock_id as libc::c_long;
  // SAFETY: Linux RV64 places the clock ID/output pointer in a0/a1 and the
  // clock_gettime syscall number in a7. The kernel writes only to `value`.
  unsafe {
    core::arch::asm!(
      "ecall",
      inlateout("a0") status,
      in("a1") value.as_mut_ptr(),
      in("a7") libc::SYS_clock_gettime,
      options(nostack),
    );
  }
  if status != 0 {
    return None;
  }
  // SAFETY: successful clock_gettime initialized the output.
  timespec_nanos(unsafe { value.assume_init() })
}

#[inline]
fn timespec_nanos(value: libc::timespec) -> Option<u64> {
  let seconds = u64::try_from(value.tv_sec).ok()?;
  let nanos = u32::try_from(value.tv_nsec).ok()?;
  if nanos >= 1_000_000_000 {
    return None;
  }
  seconds.checked_mul(1_000_000_000)?.checked_add(u64::from(nanos))
}

#[cfg(feature = "bench-internal")]
const fn provider_from_raw(provider: u8) -> WallProvider {
  match provider {
    PROVIDER_RDTIME => WallProvider::Rdtime,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => WallProvider::ClockMonotonicSyscall,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED => WallProvider::ClockMonotonicSyscallOsOrdered,
    PROVIDER_CLOCK_MONOTONIC_RAW => WallProvider::ClockMonotonicRaw,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => WallProvider::ClockMonotonicRawSyscall,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED => {
      WallProvider::ClockMonotonicRawSyscallOsOrdered
    }
    PROVIDER_CLOCK_MONOTONIC_VDSO => WallProvider::ClockMonotonicVdso,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => WallProvider::ClockMonotonicRawVdso,
    PROVIDER_CLOCK_BOOTTIME => WallProvider::ClockBoottime,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => WallProvider::ClockBoottimeSyscall,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED => WallProvider::ClockBoottimeSyscallOsOrdered,
    PROVIDER_CLOCK_BOOTTIME_VDSO => WallProvider::ClockBoottimeVdso,
    _ => WallProvider::ClockMonotonic,
  }
}

#[cfg(feature = "bench-internal")]
fn publish_evidence(ordered: bool, evidence: ProbeEvidence) {
  let (cell, ready) = if ordered {
    (&ORDERED_EVIDENCE, &ORDERED_EVIDENCE_READY)
  } else {
    (&INSTANT_EVIDENCE, &INSTANT_EVIDENCE_READY)
  };
  // SAFETY: only the selection owner reaches this write, before publishing
  // its provider state.
  unsafe { (*cell.0.get()).write(evidence) };
  ready.store(true, Ordering::Release);
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_provider() -> WallProvider {
  provider_from_raw(selected_instant_provider())
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_ordered_provider() -> WallProvider {
  provider_from_raw(selected_ordered_provider())
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy)]
#[allow(dead_code)] // Consumed by the Criterion harness outside this module.
pub(crate) struct BenchPrimitive {
  pub(crate) name: &'static str,
  pub(crate) read: fn() -> u64,
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness selects once before its timing loop.
pub(crate) fn bench_selected_instant_primitive() -> BenchPrimitive {
  instant_bench_primitive(selected_instant_provider())
}

#[cfg(feature = "bench-internal")]
fn instant_bench_primitive(provider: u8) -> BenchPrimitive {
  let read = match provider {
    PROVIDER_RDTIME => rdtime as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME => clock_boottime as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso as fn() -> u64,
    _ => clock_monotonic as fn() -> u64,
  };
  BenchPrimitive { name: provider_from_raw(provider).name(), read }
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness selects once before its timing loop.
pub(crate) fn bench_selected_ordered_primitive() -> BenchPrimitive {
  ordered_bench_primitive(selected_ordered_provider())
}

#[cfg(feature = "bench-internal")]
fn ordered_bench_primitive(provider: u8) -> BenchPrimitive {
  let read = match provider {
    PROVIDER_RDTIME => rdtime_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED => {
      clock_monotonic_syscall_os_ordered as fn() -> u64
    }
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED => {
      clock_monotonic_raw_syscall_os_ordered as fn() -> u64
    }
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso_ordered as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall_ordered as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED => clock_boottime_syscall_os_ordered as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered as fn() -> u64,
    _ => clock_monotonic_ordered as fn() -> u64,
  };
  BenchPrimitive { name: provider_from_raw(provider).name(), read }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_candidate_primitives()
-> ([Option<BenchPrimitive>; MAX_CANDIDATES], usize) {
  bench_candidate_primitives(bench_instant_evidence(), false)
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_ordered_candidate_primitives()
-> ([Option<BenchPrimitive>; MAX_CANDIDATES], usize) {
  bench_candidate_primitives(bench_ordered_evidence(), true)
}

#[cfg(feature = "bench-internal")]
fn bench_candidate_primitives(
  evidence: ProbeEvidence,
  ordered: bool,
) -> ([Option<BenchPrimitive>; MAX_CANDIDATES], usize) {
  let mut primitives = [None; MAX_CANDIDATES];
  for (index, slot) in primitives.iter_mut().enumerate().take(evidence.candidate_count) {
    let provider = evidence.candidate_providers[index];
    *slot = Some(if ordered {
      ordered_bench_primitive(provider)
    } else {
      instant_bench_primitive(provider)
    });
  }
  (primitives, evidence.candidate_count)
}

#[cfg(feature = "bench-internal")]
macro_rules! exact_bench_reader {
  ($name:ident, $body:expr) => {
    #[inline(always)]
    #[allow(dead_code)] // Candidate serializers may use the equivalent BenchPrimitive pointer.
    pub(crate) fn $name() -> u64 {
      $body
    }
  };
}

#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_rdtime, rdtime());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic, clock_monotonic());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_syscall, clock_monotonic_syscall());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_raw, clock_monotonic_raw());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_raw_syscall, clock_monotonic_raw_syscall());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_vdso, clock_monotonic_vdso());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_raw_vdso, clock_monotonic_raw_vdso());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime, clock_boottime());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_syscall, clock_boottime_syscall());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_vdso, clock_boottime_vdso());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_rdtime_ordered, rdtime_ordered());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_ordered, clock_monotonic_ordered());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_syscall_ordered, clock_monotonic_syscall_ordered());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_syscall_os_ordered,
  clock_monotonic_syscall_os_ordered()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_raw_ordered, clock_monotonic_raw_ordered());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_raw_syscall_ordered,
  clock_monotonic_raw_syscall_ordered()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_raw_syscall_os_ordered,
  clock_monotonic_raw_syscall_os_ordered()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_vdso_ordered, clock_monotonic_vdso_ordered());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_raw_vdso_ordered,
  clock_monotonic_raw_vdso_ordered()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_ordered, clock_boottime_ordered());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_syscall_ordered, clock_boottime_syscall_ordered());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_boottime_syscall_os_ordered,
  clock_boottime_syscall_os_ordered()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_vdso_ordered, clock_boottime_vdso_ordered());

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_evidence() -> ProbeEvidence {
  let _ = selected_instant_provider();
  assert!(INSTANT_EVIDENCE_READY.load(Ordering::Acquire));
  // SAFETY: the Acquire load observes the selection owner's initialized cell.
  unsafe { *(*INSTANT_EVIDENCE.0.get()).assume_init_ref() }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_ordered_evidence() -> ProbeEvidence {
  let _ = selected_ordered_provider();
  assert!(ORDERED_EVIDENCE_READY.load(Ordering::Acquire));
  // SAFETY: the Acquire load observes the selection owner's initialized cell.
  unsafe { *(*ORDERED_EVIDENCE.0.get()).assume_init_ref() }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn materiality_rejects_noisy_or_sub_nanosecond_wins() {
    let incumbent = [1_000_000; PROBE_BATCHES];
    assert!(!prefer_challenger([975_000; PROBE_BATCHES], incumbent).challenger_selected);
    assert!(prefer_challenger([900_000; PROBE_BATCHES], incumbent).challenger_selected);
    let mut noisy = [900_000; PROBE_BATCHES];
    noisy[0] = 1_000_000;
    noisy[1] = 1_000_000;
    assert!(!prefer_challenger(noisy, incumbent).challenger_selected);
  }

  #[test]
  fn os_owned_syscall_candidates_are_ordered_only() {
    assert!(is_os_ordered_provider(PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_ORDERED));
    assert!(is_os_ordered_provider(PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL_OS_ORDERED));
    assert!(is_os_ordered_provider(PROVIDER_CLOCK_BOOTTIME_SYSCALL_OS_ORDERED));
    assert!(!is_os_ordered_provider(PROVIDER_CLOCK_MONOTONIC_SYSCALL));
  }

  #[test]
  fn candidate_list_compacts_unavailable_routes() {
    let mut candidates = [PROVIDER_UNKNOWN; 2];
    let mut count = 0;
    push_candidate(&mut candidates, &mut count, PROVIDER_RDTIME, false);
    push_candidate(&mut candidates, &mut count, PROVIDER_CLOCK_MONOTONIC, true);
    assert_eq!(count, 1);
    assert_eq!(candidates[0], PROVIDER_CLOCK_MONOTONIC);
  }
}
