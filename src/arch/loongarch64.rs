//! Linux LoongArch wall-clock reads and runtime provider selection.
//!
//! StableCounter is a mandatory, constant-frequency 64-bit architectural
//! timer, and Linux uses it for both its continuous clocksource and vDSO.
//! Virtualization can still change the relative cost of a direct read, vDSO
//! conversion, and a raw syscall. CLOCK_MONOTONIC, CLOCK_MONOTONIC_RAW, and
//! CLOCK_BOOTTIME routes therefore compete, and the two Instant contracts select their
//! providers independently by measuring the full branched path.

use core::arch::asm;
#[cfg(feature = "bench-internal")]
use core::cell::UnsafeCell;
use core::hint::black_box;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU64, Ordering};

const PROVIDER_UNKNOWN: u8 = 0;
const PROVIDER_SELECTING: u8 = 1;
const PROVIDER_STABLE_COUNTER: u8 = 2;
const PROVIDER_CLOCK_MONOTONIC: u8 = 3;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL: u8 = 4;
const PROVIDER_CLOCK_MONOTONIC_RAW: u8 = 5;
const PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL: u8 = 6;
const PROVIDER_CLOCK_MONOTONIC_VDSO: u8 = 7;
const PROVIDER_CLOCK_MONOTONIC_RAW_VDSO: u8 = 8;
const PROVIDER_CLOCK_BOOTTIME: u8 = 9;
const PROVIDER_CLOCK_BOOTTIME_SYSCALL: u8 = 10;
const PROVIDER_CLOCK_BOOTTIME_VDSO: u8 = 11;
const MAX_CANDIDATES: usize = 10;

const PROBE_BATCHES: usize = 9;
const PROBE_READS: u64 = 4096;
const PROBE_WARMUP_READS: u64 = 1024;
const REQUIRED_DECISIVE_WINS: usize = 8;

const AT_HWCAP: libc::c_ulong = 16;
const HWCAP_LOONGARCH_CPUCFG: libc::c_ulong = 1;
const CPUCFG2_LLFTP: u64 = 1 << 14;

static INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static INSTANT_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static INSTANT_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static ORDERED_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static PROBE_INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_STABLE_COUNTER);
static PROBE_ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_STABLE_COUNTER);
static STABLE_COUNTER_FREQUENCY: AtomicU64 = AtomicU64::new(0);
static FREQUENCY_AUTHORITATIVE: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WallProvider {
  StableCounter,
  ClockMonotonic,
  ClockMonotonicSyscall,
  ClockMonotonicRaw,
  ClockMonotonicRawSyscall,
  ClockMonotonicVdso,
  ClockMonotonicRawVdso,
  ClockBoottime,
  ClockBoottimeSyscall,
  ClockBoottimeVdso,
}

#[cfg(feature = "bench-internal")]
impl WallProvider {
  pub(crate) const fn name(self) -> &'static str {
    match self {
      Self::StableCounter => "loongarch_stable_counter",
      Self::ClockMonotonic => "linux_clock_monotonic",
      Self::ClockMonotonicSyscall => "linux_clock_monotonic_syscall",
      Self::ClockMonotonicRaw => "linux_clock_monotonic_raw",
      Self::ClockMonotonicRawSyscall => "linux_clock_monotonic_raw_syscall",
      Self::ClockMonotonicVdso => "linux_clock_monotonic_vdso_direct",
      Self::ClockMonotonicRawVdso => "linux_clock_monotonic_raw_vdso_direct",
      Self::ClockBoottime => "linux_clock_boottime",
      Self::ClockBoottimeSyscall => "linux_clock_boottime_syscall",
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
  clock_raw: [u64; PROBE_BATCHES],
  syscall_raw: [u64; PROBE_BATCHES],
  vdso: [u64; PROBE_BATCHES],
  vdso_raw: [u64; PROBE_BATCHES],
  clock_boottime: [u64; PROBE_BATCHES],
  syscall_boottime: [u64; PROBE_BATCHES],
  vdso_boottime: [u64; PROBE_BATCHES],
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // Architecture evidence is complete even when a serializer projects a subset.
pub(crate) struct ProbeEvidence {
  pub(crate) candidate_count: usize,
  pub(crate) candidate_providers: [u8; MAX_CANDIDATES],
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
  pub(crate) clock_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) direct_median_ns: u64,
  pub(crate) clock_median_ns: u64,
  pub(crate) syscall_median_ns: u64,
  pub(crate) clock_raw_median_ns: u64,
  pub(crate) syscall_raw_median_ns: u64,
  pub(crate) vdso_median_ns: u64,
  pub(crate) vdso_raw_median_ns: u64,
  pub(crate) clock_boottime_median_ns: u64,
  pub(crate) syscall_boottime_median_ns: u64,
  pub(crate) vdso_boottime_median_ns: u64,
  pub(crate) fallback_allowance_ns: u64,
  pub(crate) fallback_decisive_wins: usize,
  pub(crate) clock_raw_allowance_ns: u64,
  pub(crate) clock_raw_decisive_wins: usize,
  pub(crate) syscall_raw_allowance_ns: u64,
  pub(crate) syscall_raw_decisive_wins: usize,
  pub(crate) vdso_allowance_ns: u64,
  pub(crate) vdso_decisive_wins: usize,
  pub(crate) vdso_raw_allowance_ns: u64,
  pub(crate) vdso_raw_decisive_wins: usize,
  pub(crate) clock_boottime_allowance_ns: u64,
  pub(crate) clock_boottime_decisive_wins: usize,
  pub(crate) syscall_boottime_allowance_ns: u64,
  pub(crate) syscall_boottime_decisive_wins: usize,
  pub(crate) vdso_boottime_allowance_ns: u64,
  pub(crate) vdso_boottime_decisive_wins: usize,
  pub(crate) direct_allowance_ns: u64,
  pub(crate) direct_decisive_wins: usize,
  pub(crate) required_decisive_wins: usize,
  pub(crate) selected_provider: WallProvider,
}

#[cfg(feature = "bench-internal")]
struct EvidenceCell(UnsafeCell<MaybeUninit<ProbeEvidence>>);

// SAFETY: one process-selection owner initializes each cell before the
// corresponding provider is published with Release.
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

/// Ordered `rdtime.d`. Linux enters the kernel with a raw
/// `getpid` system call and reads StableCounter after the synchronous exception
/// returns. This preserves the raw tick domain while using the cheapest known
/// precise exception boundary for ordering.
///
/// `nomem` is intentionally omitted so the compiler also keeps surrounding
/// memory operations in order around the asm block.
#[inline(always)]
pub fn rdtime_ordered() -> u64 {
  #[cfg(target_os = "linux")]
  {
    return rdtime_after_syscall();
  }
  #[cfg(not(target_os = "linux"))]
  {
    rdtime_after_dbar()
  }
}

#[cfg(target_os = "linux")]
#[inline(always)]
fn rdtime_after_syscall() -> u64 {
  let cnt: u64;
  // SAFETY: LoongArch Linux takes the syscall number in A7 and the first two
  // arguments in A0-A6. SYSCALL raises an immediate, precise exception; the
  // following RDTIME.D cannot execute until the kernel returns with ERTN. The
  // getpid result is deliberately discarded, so even seccomp denial leaves a
  // domain-correct ordered counter read after the exception boundary.
  unsafe {
    asm!(
      "syscall 0",
      "rdtime.d {cnt}, $zero",
      cnt = lateout(reg) cnt,
      inlateout("$a0") 0_usize => _,
      in("$a7") libc::SYS_getpid as usize,
      lateout("$t0") _,
      lateout("$t1") _,
      lateout("$t2") _,
      lateout("$t3") _,
      lateout("$t4") _,
      lateout("$t5") _,
      lateout("$t6") _,
      lateout("$t7") _,
      lateout("$t8") _,
      options(nostack),
    );
  }
  cnt
}

#[cfg(not(target_os = "linux"))]
#[inline(always)]
fn rdtime_after_dbar() -> u64 {
  let cnt: u64;
  // SAFETY: `dbar 0; rdtime.d` orders prior memory operations and reads the
  // architectural timer. Compiler treats the block as memory-touching.
  unsafe {
    asm!(
      "dbar 0",
      "rdtime.d {}, $zero",
      out(reg) cnt,
      options(nostack, preserves_flags),
    );
  }
  cnt
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  match INSTANT_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_STABLE_COUNTER => rdtime(),
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
  match selected_instant_provider() {
    PROVIDER_STABLE_COUNTER => rdtime(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => clock_monotonic_raw_vdso(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  match ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_STABLE_COUNTER => rdtime_ordered(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso_ordered(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered(),
    _ => ticks_ordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_after_selection() -> u64 {
  match selected_ordered_provider() {
    PROVIDER_STABLE_COUNTER => rdtime_ordered(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered(),
    _ => clock_monotonic_raw_vdso_ordered(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  match ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_STABLE_COUNTER => rdtime(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => ticks_ordered_unordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_unordered_after_selection() -> u64 {
  match selected_ordered_provider() {
    PROVIDER_STABLE_COUNTER => rdtime(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => clock_monotonic_raw_vdso(),
  }
}

#[inline]
pub(crate) fn instant_frequency() -> u64 {
  if instant_uses_stable_counter() { stable_counter_frequency() } else { 1_000_000_000 }
}

#[inline]
pub(crate) fn ordered_frequency() -> u64 {
  if ordered_uses_stable_counter() { stable_counter_frequency() } else { 1_000_000_000 }
}

#[inline]
pub(crate) fn instant_uses_stable_counter() -> bool {
  selected_instant_provider() == PROVIDER_STABLE_COUNTER
}

#[inline]
pub(crate) fn instant_read_cost() -> crate::ThreadCpuReadCost {
  instant_read_cost_for(selected_instant_provider())
}

const fn instant_read_cost_for(provider: u8) -> crate::ThreadCpuReadCost {
  match provider {
    PROVIDER_STABLE_COUNTER
    | PROVIDER_CLOCK_MONOTONIC_VDSO
    | PROVIDER_CLOCK_MONOTONIC_RAW_VDSO
    | PROVIDER_CLOCK_BOOTTIME_VDSO => crate::ThreadCpuReadCost::Inline,
    // The libc ABI may use the vDSO or enter the kernel. Only the selected
    // stable-counter route is guaranteed to remain in userspace.
    _ => crate::ThreadCpuReadCost::SystemCall,
  }
}

#[cfg(test)]
#[test]
fn instant_read_cost_only_marks_guaranteed_userspace_paths_inline() {
  assert_eq!(instant_read_cost_for(PROVIDER_STABLE_COUNTER), crate::ThreadCpuReadCost::Inline,);
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
pub(crate) fn ordered_uses_stable_counter() -> bool {
  selected_ordered_provider() == PROVIDER_STABLE_COUNTER
}

#[inline]
pub(crate) fn needs_frequency_recalibration() -> bool {
  let direct_selected = instant_uses_stable_counter() || ordered_uses_stable_counter();
  direct_selected && {
    let _ = stable_counter_frequency();
    !FREQUENCY_AUTHORITATIVE.load(Ordering::Acquire)
  }
}

fn stable_counter_frequency() -> u64 {
  let cached = STABLE_COUNTER_FREQUENCY.load(Ordering::Acquire);
  if cached != 0 {
    return cached;
  }
  let (frequency, authoritative) = cpucfg_frequency()
    .map(|frequency| (frequency, true))
    .unwrap_or_else(|| (crate::calibration::calibrate_frequency_with(rdtime), false));
  let frequency = frequency.max(1);
  match STABLE_COUNTER_FREQUENCY.compare_exchange(0, frequency, Ordering::AcqRel, Ordering::Acquire)
  {
    Ok(_) => {
      FREQUENCY_AUTHORITATIVE.store(authoritative, Ordering::Release);
      frequency
    }
    Err(winner) => winner,
  }
}

fn cpucfg_frequency() -> Option<u64> {
  // SAFETY: getauxval has no pointer preconditions. Linux publishes CPUCFG in
  // AT_HWCAP only when the instruction is valid for userspace.
  if unsafe { libc::getauxval(AT_HWCAP) } & HWCAP_LOONGARCH_CPUCFG == 0 {
    return None;
  }
  let features = cpucfg(2);
  if features & CPUCFG2_LLFTP == 0 {
    return None;
  }
  let base = cpucfg(4);
  let ratio = cpucfg(5);
  let multiplier = ratio & 0xffff;
  let divisor = ratio >> 16 & 0xffff;
  if base == 0 || multiplier == 0 || divisor == 0 {
    return None;
  }
  base.checked_mul(multiplier)?.checked_div(divisor)
}

#[inline(always)]
fn cpucfg(index: u64) -> u64 {
  let value: u64;
  // SAFETY: AT_HWCAP gates every call. CPUCFG only writes its output register.
  unsafe {
    asm!(
      "cpucfg {value}, {index}",
      value = out(reg) value,
      index = in(reg) index,
      options(nostack, nomem, preserves_flags),
    );
  }
  value
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
  super::select_thread_owned_process_provider(
    state,
    PROVIDER_UNKNOWN,
    PROVIDER_SELECTING,
    owner_pid,
    owner_tid,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL,
    || detect_provider(ordered),
  )
}

#[cold]
#[inline(never)]
fn detect_provider(ordered: bool) -> u8 {
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
  let vdso_boottime_decision = if vdso_boottime_available {
    prefer_challenger(samples.vdso_boottime, candidate_samples(samples, fallback))
  } else {
    empty_decision()
  };
  if vdso_boottime_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_BOOTTIME_VDSO;
  }
  let fallback_samples = candidate_samples(samples, fallback);
  let direct_decision = prefer_challenger(samples.direct, fallback_samples);
  let provider =
    if direct_decision.challenger_selected { PROVIDER_STABLE_COUNTER } else { fallback };

  #[cfg(feature = "bench-internal")]
  publish_evidence(ordered, ProbeEvidence {
    candidate_count,
    candidate_providers,
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
    clock_raw_batches_ns: samples.clock_raw,
    syscall_raw_batches_ns: samples.syscall_raw,
    vdso_batches_ns: samples.vdso,
    vdso_raw_batches_ns: samples.vdso_raw,
    clock_boottime_batches_ns: samples.clock_boottime,
    syscall_boottime_batches_ns: samples.syscall_boottime,
    vdso_boottime_batches_ns: samples.vdso_boottime,
    direct_median_ns: median(samples.direct),
    clock_median_ns: median(samples.clock),
    syscall_median_ns: median(samples.syscall),
    clock_raw_median_ns: median(samples.clock_raw),
    syscall_raw_median_ns: median(samples.syscall_raw),
    vdso_median_ns: median(samples.vdso),
    vdso_raw_median_ns: median(samples.vdso_raw),
    clock_boottime_median_ns: median(samples.clock_boottime),
    syscall_boottime_median_ns: median(samples.syscall_boottime),
    vdso_boottime_median_ns: median(samples.vdso_boottime),
    fallback_allowance_ns: fallback_decision.allowance,
    fallback_decisive_wins: fallback_decision.decisive_wins,
    clock_raw_allowance_ns: clock_raw_decision.allowance,
    clock_raw_decisive_wins: clock_raw_decision.decisive_wins,
    syscall_raw_allowance_ns: syscall_raw_decision.allowance,
    syscall_raw_decisive_wins: syscall_raw_decision.decisive_wins,
    vdso_allowance_ns: vdso_decision.allowance,
    vdso_decisive_wins: vdso_decision.decisive_wins,
    vdso_raw_allowance_ns: vdso_raw_decision.allowance,
    vdso_raw_decisive_wins: vdso_raw_decision.decisive_wins,
    clock_boottime_allowance_ns: clock_boottime_decision.allowance,
    clock_boottime_decisive_wins: clock_boottime_decision.decisive_wins,
    syscall_boottime_allowance_ns: syscall_boottime_decision.allowance,
    syscall_boottime_decisive_wins: syscall_boottime_decision.decisive_wins,
    vdso_boottime_allowance_ns: vdso_boottime_decision.allowance,
    vdso_boottime_decisive_wins: vdso_boottime_decision.decisive_wins,
    direct_allowance_ns: direct_decision.allowance,
    direct_decisive_wins: direct_decision.decisive_wins,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    selected_provider: provider_from_raw(provider),
  });

  provider
}

fn measure_candidates(
  ordered: bool,
  clock_raw_available: bool,
  syscall_raw_available: bool,
  vdso_available: bool,
  vdso_raw_available: bool,
  clock_boottime_available: bool,
  syscall_boottime_available: bool,
  vdso_boottime_available: bool,
) -> (ProbeSamples, [u8; MAX_CANDIDATES], usize) {
  warm_candidate(ordered, PROVIDER_STABLE_COUNTER);
  warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC);
  warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_SYSCALL);
  if clock_raw_available {
    warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_RAW);
  }
  if syscall_raw_available {
    warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL);
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
  }
  if vdso_boottime_available {
    warm_candidate(ordered, PROVIDER_CLOCK_BOOTTIME_VDSO);
  }

  let mut candidates = [PROVIDER_UNKNOWN; MAX_CANDIDATES];
  let mut candidate_count = 0;
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_STABLE_COUNTER, true);
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_CLOCK_MONOTONIC, true);
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_CLOCK_MONOTONIC_SYSCALL, true);
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
    PROVIDER_CLOCK_BOOTTIME_VDSO,
    vdso_boottime_available,
  );

  let mut samples = ProbeSamples {
    direct: [u64::MAX; PROBE_BATCHES],
    clock: [u64::MAX; PROBE_BATCHES],
    syscall: [u64::MAX; PROBE_BATCHES],
    clock_raw: [u64::MAX; PROBE_BATCHES],
    syscall_raw: [u64::MAX; PROBE_BATCHES],
    vdso: [u64::MAX; PROBE_BATCHES],
    vdso_raw: [u64::MAX; PROBE_BATCHES],
    clock_boottime: [u64::MAX; PROBE_BATCHES],
    syscall_boottime: [u64::MAX; PROBE_BATCHES],
    vdso_boottime: [u64::MAX; PROBE_BATCHES],
  };
  for sample in 0..PROBE_BATCHES {
    for offset in 0..candidate_count {
      let provider = candidates[(sample + offset) % candidate_count];
      let elapsed = measure_batch(ordered, provider).unwrap_or(u64::MAX);
      match provider {
        PROVIDER_STABLE_COUNTER => samples.direct[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC => samples.clock[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_SYSCALL => samples.syscall[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_RAW => samples.clock_raw[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => samples.syscall_raw[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_VDSO => samples.vdso[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => samples.vdso_raw[sample] = elapsed,
        PROVIDER_CLOCK_BOOTTIME => samples.clock_boottime[sample] = elapsed,
        PROVIDER_CLOCK_BOOTTIME_SYSCALL => samples.syscall_boottime[sample] = elapsed,
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
    PROVIDER_STABLE_COUNTER => samples.direct,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => samples.syscall,
    PROVIDER_CLOCK_MONOTONIC_RAW => samples.clock_raw,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => samples.syscall_raw,
    PROVIDER_CLOCK_MONOTONIC_VDSO => samples.vdso,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => samples.vdso_raw,
    PROVIDER_CLOCK_BOOTTIME => samples.clock_boottime,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => samples.syscall_boottime,
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
  match PROBE_INSTANT_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_STABLE_COUNTER => rdtime(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => clock_monotonic(),
  }
}

#[inline(always)]
fn probe_ordered_hot_path() -> u64 {
  match PROBE_ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_STABLE_COUNTER => rdtime_ordered(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso_ordered(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
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
fn ordered_exception_boundary() {
  // SAFETY: the synchronous getpid exception cannot be bypassed by the later
  // libc/vDSO counter read. Omitting `nomem` also prevents compiler motion.
  unsafe {
    asm!(
      "syscall 0",
      inlateout("$a0") 0_usize => _,
      in("$a7") libc::SYS_getpid as usize,
      lateout("$t0") _,
      lateout("$t1") _,
      lateout("$t2") _,
      lateout("$t3") _,
      lateout("$t4") _,
      lateout("$t5") _,
      lateout("$t6") _,
      lateout("$t7") _,
      lateout("$t8") _,
      options(nostack),
    );
  }
}

#[inline(always)]
fn clock_monotonic_ordered() -> u64 {
  ordered_exception_boundary();
  clock_monotonic()
}

#[inline(always)]
fn clock_monotonic() -> u64 {
  libc_clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_raw_ordered() -> u64 {
  ordered_exception_boundary();
  clock_monotonic_raw()
}

#[inline(always)]
fn clock_boottime_ordered() -> u64 {
  ordered_exception_boundary();
  clock_boottime()
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
  ordered_exception_boundary();
  clock_monotonic_vdso()
}

#[inline(always)]
fn clock_monotonic_raw_vdso_ordered() -> u64 {
  ordered_exception_boundary();
  clock_monotonic_raw_vdso()
}

#[inline(always)]
fn clock_boottime_vdso_ordered() -> u64 {
  ordered_exception_boundary();
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
  let mut status = clock_id as usize;
  // SAFETY: Linux LoongArch places a0/a1 arguments and the syscall number in
  // a7. The kernel writes only through the supplied timespec pointer.
  unsafe {
    asm!(
      "syscall 0",
      inlateout("$a0") status,
      in("$a1") value.as_mut_ptr(),
      in("$a7") libc::SYS_clock_gettime as usize,
      lateout("$t0") _,
      lateout("$t1") _,
      lateout("$t2") _,
      lateout("$t3") _,
      lateout("$t4") _,
      lateout("$t5") _,
      lateout("$t6") _,
      lateout("$t7") _,
      lateout("$t8") _,
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
    PROVIDER_STABLE_COUNTER => WallProvider::StableCounter,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => WallProvider::ClockMonotonicSyscall,
    PROVIDER_CLOCK_MONOTONIC_RAW => WallProvider::ClockMonotonicRaw,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => WallProvider::ClockMonotonicRawSyscall,
    PROVIDER_CLOCK_MONOTONIC_VDSO => WallProvider::ClockMonotonicVdso,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => WallProvider::ClockMonotonicRawVdso,
    PROVIDER_CLOCK_BOOTTIME => WallProvider::ClockBoottime,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => WallProvider::ClockBoottimeSyscall,
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
  // SAFETY: only the selection owner writes before provider publication.
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
    PROVIDER_STABLE_COUNTER => rdtime as fn() -> u64,
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
    PROVIDER_STABLE_COUNTER => rdtime_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso_ordered as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall as fn() -> u64,
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
exact_bench_reader!(bench_exact_clock_monotonic_raw_ordered, clock_monotonic_raw_ordered());
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
exact_bench_reader!(bench_exact_clock_boottime_syscall_ordered, clock_boottime_syscall());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_vdso_ordered, clock_boottime_vdso_ordered());

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_evidence() -> ProbeEvidence {
  let _ = selected_instant_provider();
  assert!(INSTANT_EVIDENCE_READY.load(Ordering::Acquire));
  // SAFETY: the Acquire load observes the initialized evidence cell.
  unsafe { *(*INSTANT_EVIDENCE.0.get()).assume_init_ref() }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_ordered_evidence() -> ProbeEvidence {
  let _ = selected_ordered_provider();
  assert!(ORDERED_EVIDENCE_READY.load(Ordering::Acquire));
  // SAFETY: the Acquire load observes the initialized evidence cell.
  unsafe { *(*ORDERED_EVIDENCE.0.get()).assume_init_ref() }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn materiality_requires_eight_repeatable_wins() {
    let incumbent = [1_000_000; PROBE_BATCHES];
    assert!(!prefer_challenger([975_000; PROBE_BATCHES], incumbent).challenger_selected);
    assert!(prefer_challenger([900_000; PROBE_BATCHES], incumbent).challenger_selected);
    let mut noisy = [900_000; PROBE_BATCHES];
    noisy[0] = 1_000_000;
    noisy[1] = 1_000_000;
    assert!(!prefer_challenger(noisy, incumbent).challenger_selected);
  }
}
