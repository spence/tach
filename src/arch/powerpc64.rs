//! Linux PowerPC64 Time Base reads.
//!
//! Linux registers this counter as `CLOCK_SOURCE_IS_CONTINUOUS`, and its SMP
//! bring-up synchronizes per-CPU Time Bases. The raw timeline therefore keeps
//! the kernel's migration-safe monotonic-clock invariant without the vDSO's
//! scale-and-offset work on each sample.
//!
//! CLOCK_MONOTONIC, CLOCK_MONOTONIC_RAW, and CLOCK_BOOTTIME libc, SC, SCV,
//! and direct-vDSO routes compete independently. Ordered raw SC/SCV routes
//! also compete without a separate `sync` because
//! both syscall instructions are context synchronizing. Libc/vDSO and Time
//! Base routes are not eligible for that assumption and retain explicit `sync`.

use core::arch::asm;
#[cfg(feature = "bench-internal")]
use core::cell::UnsafeCell;
use core::hint::{black_box, spin_loop};
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU64, Ordering};

const PROVIDER_UNKNOWN: u8 = 0;
const PROVIDER_SELECTING: u8 = 1;
const PROVIDER_TIMEBASE: u8 = 2;
const PROVIDER_CLOCK_MONOTONIC: u8 = 3;
const PROVIDER_CLOCK_MONOTONIC_SC: u8 = 4;
const PROVIDER_CLOCK_MONOTONIC_SCV: u8 = 5;
const PROVIDER_CLOCK_MONOTONIC_RAW: u8 = 6;
const PROVIDER_CLOCK_MONOTONIC_RAW_SC: u8 = 7;
const PROVIDER_CLOCK_MONOTONIC_RAW_SCV: u8 = 8;
const PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED: u8 = 9;
const PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED: u8 = 10;
const PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED: u8 = 11;
const PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED: u8 = 12;
const PROVIDER_CLOCK_MONOTONIC_VDSO: u8 = 13;
const PROVIDER_CLOCK_MONOTONIC_RAW_VDSO: u8 = 14;
const PROVIDER_CLOCK_BOOTTIME: u8 = 15;
const PROVIDER_CLOCK_BOOTTIME_SC: u8 = 16;
const PROVIDER_CLOCK_BOOTTIME_SCV: u8 = 17;
const PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED: u8 = 18;
const PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED: u8 = 19;
const PROVIDER_CLOCK_BOOTTIME_VDSO: u8 = 20;
const MAX_CANDIDATES: usize = 19;

const PROBE_BATCHES: usize = 9;
const PROBE_READS: u64 = 4096;
const PROBE_WARMUP_READS: u64 = 1024;
const REQUIRED_DECISIVE_WINS: usize = 8;

const fn is_os_ordered_provider(provider: u8) -> bool {
  matches!(
    provider,
    PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED
      | PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED
      | PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED
      | PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED
      | PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED
      | PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED
  )
}

const AT_HWCAP2: libc::c_ulong = 26;
const PPC_FEATURE2_SCV: libc::c_ulong = 0x0010_0000;

static INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static INSTANT_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static INSTANT_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static ORDERED_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static PROBE_INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_TIMEBASE);
static PROBE_ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_TIMEBASE);

static TIMEBASE_FREQUENCY: AtomicU64 = AtomicU64::new(0);
static FREQUENCY_AUTHORITATIVE: AtomicBool = AtomicBool::new(false);

static TIMEBASE_FREQUENCY_LOCK: AtomicBool = AtomicBool::new(false);
static TIMEBASE_FREQUENCY_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static TIMEBASE_FREQUENCY_OWNER_TID: AtomicI32 = AtomicI32::new(0);

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WallProvider {
  Timebase,
  ClockMonotonic,
  ClockMonotonicSc,
  ClockMonotonicScOsOrdered,
  ClockMonotonicScv,
  ClockMonotonicScvOsOrdered,
  ClockMonotonicRaw,
  ClockMonotonicRawSc,
  ClockMonotonicRawScOsOrdered,
  ClockMonotonicRawScv,
  ClockMonotonicRawScvOsOrdered,
  ClockMonotonicVdso,
  ClockMonotonicRawVdso,
  ClockBoottime,
  ClockBoottimeSc,
  ClockBoottimeScOsOrdered,
  ClockBoottimeScv,
  ClockBoottimeScvOsOrdered,
  ClockBoottimeVdso,
}

#[cfg(feature = "bench-internal")]
impl WallProvider {
  pub(crate) const fn name(self) -> &'static str {
    match self {
      Self::Timebase => "power_timebase",
      Self::ClockMonotonic => "linux_clock_monotonic",
      Self::ClockMonotonicSc => "linux_clock_monotonic_sc",
      Self::ClockMonotonicScOsOrdered => "linux_clock_monotonic_sc_os_ordered",
      Self::ClockMonotonicScv => "linux_clock_monotonic_scv",
      Self::ClockMonotonicScvOsOrdered => "linux_clock_monotonic_scv_os_ordered",
      Self::ClockMonotonicRaw => "linux_clock_monotonic_raw",
      Self::ClockMonotonicRawSc => "linux_clock_monotonic_raw_sc",
      Self::ClockMonotonicRawScOsOrdered => "linux_clock_monotonic_raw_sc_os_ordered",
      Self::ClockMonotonicRawScv => "linux_clock_monotonic_raw_scv",
      Self::ClockMonotonicRawScvOsOrdered => "linux_clock_monotonic_raw_scv_os_ordered",
      Self::ClockMonotonicVdso => "linux_clock_monotonic_vdso_direct",
      Self::ClockMonotonicRawVdso => "linux_clock_monotonic_raw_vdso_direct",
      Self::ClockBoottime => "linux_clock_boottime",
      Self::ClockBoottimeSc => "linux_clock_boottime_sc",
      Self::ClockBoottimeScOsOrdered => "linux_clock_boottime_sc_os_ordered",
      Self::ClockBoottimeScv => "linux_clock_boottime_scv",
      Self::ClockBoottimeScvOsOrdered => "linux_clock_boottime_scv_os_ordered",
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
  sc: [u64; PROBE_BATCHES],
  sc_os_ordered: [u64; PROBE_BATCHES],
  scv: [u64; PROBE_BATCHES],
  scv_os_ordered: [u64; PROBE_BATCHES],
  clock_raw: [u64; PROBE_BATCHES],
  sc_raw: [u64; PROBE_BATCHES],
  sc_raw_os_ordered: [u64; PROBE_BATCHES],
  scv_raw: [u64; PROBE_BATCHES],
  scv_raw_os_ordered: [u64; PROBE_BATCHES],
  vdso: [u64; PROBE_BATCHES],
  vdso_raw: [u64; PROBE_BATCHES],
  clock_boottime: [u64; PROBE_BATCHES],
  sc_boottime: [u64; PROBE_BATCHES],
  sc_boottime_os_ordered: [u64; PROBE_BATCHES],
  scv_boottime: [u64; PROBE_BATCHES],
  scv_boottime_os_ordered: [u64; PROBE_BATCHES],
  vdso_boottime: [u64; PROBE_BATCHES],
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // Architecture evidence is complete even when a serializer projects a subset.
pub(crate) struct ProbeEvidence {
  pub(crate) candidate_count: usize,
  pub(crate) candidate_providers: [u8; MAX_CANDIDATES],
  pub(crate) scv_eligible: bool,
  pub(crate) clock_raw_available: bool,
  pub(crate) sc_raw_available: bool,
  pub(crate) scv_raw_available: bool,
  pub(crate) vdso_available: bool,
  pub(crate) vdso_raw_available: bool,
  pub(crate) clock_boottime_available: bool,
  pub(crate) sc_boottime_available: bool,
  pub(crate) scv_boottime_available: bool,
  pub(crate) vdso_boottime_available: bool,
  pub(crate) reads_per_batch: u64,
  pub(crate) direct_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) sc_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) sc_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) scv_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) scv_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) sc_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) sc_raw_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) scv_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) scv_raw_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) sc_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) sc_boottime_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) scv_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) scv_boottime_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) direct_median_ns: u64,
  pub(crate) clock_median_ns: u64,
  pub(crate) sc_median_ns: u64,
  pub(crate) sc_os_ordered_median_ns: u64,
  pub(crate) scv_median_ns: u64,
  pub(crate) scv_os_ordered_median_ns: u64,
  pub(crate) clock_raw_median_ns: u64,
  pub(crate) sc_raw_median_ns: u64,
  pub(crate) sc_raw_os_ordered_median_ns: u64,
  pub(crate) scv_raw_median_ns: u64,
  pub(crate) scv_raw_os_ordered_median_ns: u64,
  pub(crate) vdso_median_ns: u64,
  pub(crate) vdso_raw_median_ns: u64,
  pub(crate) clock_boottime_median_ns: u64,
  pub(crate) sc_boottime_median_ns: u64,
  pub(crate) sc_boottime_os_ordered_median_ns: u64,
  pub(crate) scv_boottime_median_ns: u64,
  pub(crate) scv_boottime_os_ordered_median_ns: u64,
  pub(crate) vdso_boottime_median_ns: u64,
  pub(crate) sc_allowance_ns: u64,
  pub(crate) sc_decisive_wins: usize,
  pub(crate) sc_os_ordered_allowance_ns: u64,
  pub(crate) sc_os_ordered_decisive_wins: usize,
  pub(crate) scv_allowance_ns: u64,
  pub(crate) scv_decisive_wins: usize,
  pub(crate) scv_os_ordered_allowance_ns: u64,
  pub(crate) scv_os_ordered_decisive_wins: usize,
  pub(crate) clock_raw_allowance_ns: u64,
  pub(crate) clock_raw_decisive_wins: usize,
  pub(crate) sc_raw_allowance_ns: u64,
  pub(crate) sc_raw_decisive_wins: usize,
  pub(crate) sc_raw_os_ordered_allowance_ns: u64,
  pub(crate) sc_raw_os_ordered_decisive_wins: usize,
  pub(crate) scv_raw_allowance_ns: u64,
  pub(crate) scv_raw_decisive_wins: usize,
  pub(crate) scv_raw_os_ordered_allowance_ns: u64,
  pub(crate) scv_raw_os_ordered_decisive_wins: usize,
  pub(crate) vdso_allowance_ns: u64,
  pub(crate) vdso_decisive_wins: usize,
  pub(crate) vdso_raw_allowance_ns: u64,
  pub(crate) vdso_raw_decisive_wins: usize,
  pub(crate) clock_boottime_allowance_ns: u64,
  pub(crate) clock_boottime_decisive_wins: usize,
  pub(crate) sc_boottime_allowance_ns: u64,
  pub(crate) sc_boottime_decisive_wins: usize,
  pub(crate) sc_boottime_os_ordered_allowance_ns: u64,
  pub(crate) sc_boottime_os_ordered_decisive_wins: usize,
  pub(crate) scv_boottime_allowance_ns: u64,
  pub(crate) scv_boottime_decisive_wins: usize,
  pub(crate) scv_boottime_os_ordered_allowance_ns: u64,
  pub(crate) scv_boottime_os_ordered_decisive_wins: usize,
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

unsafe extern "C" {
  fn __ppc_get_timebase_freq() -> u64;
}

/// Reads the full 64-bit Time Base register.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn mftb() -> u64 {
  loop {
    let ticks;
    // SAFETY: SPR 268 is the unprivileged 64-bit Time Base register on
    // PowerPC64. Linux uses the same register as its continuous clocksource.
    unsafe {
      asm!(
        "mfspr {ticks}, 268",
        ticks = out(reg) ticks,
        options(nostack, preserves_flags),
      );
    }
    // Linux's powerpc64 vDSO applies this same retry on CPUs carrying
    // CPU_FTR_CELL_TB_BUG. The branch is effectively free on other systems.
    if ticks != 0 {
      return ticks;
    }
  }
}

/// Orders the Time Base sample after prior memory observations.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn mftb_ordered() -> u64 {
  power_ordering_barrier();
  mftb()
}

#[inline(always)]
fn power_ordering_barrier() {
  // SAFETY: heavyweight `sync` is unprivileged. Power ISA defines it as
  // completing prior instructions before later instructions start; omitting
  // `nomem` gives the compiler the same ordering boundary.
  unsafe {
    asm!("sync", options(nostack, preserves_flags));
  }
}

/// Returns Linux's authoritative Time Base rate in ticks per second.
#[inline]
pub fn timebase_frequency() -> u64 {
  let cached = TIMEBASE_FREQUENCY.load(Ordering::Acquire);
  if cached != 0 {
    return cached;
  }
  let reported = query_timebase_frequency();
  let (frequency, authoritative) = if reported == 0 {
    (crate::calibration::calibrate_frequency_with(mftb), false)
  } else {
    (reported, true)
  };
  let frequency = frequency.max(1);
  match TIMEBASE_FREQUENCY.compare_exchange(0, frequency, Ordering::AcqRel, Ordering::Acquire) {
    Ok(_) => {
      FREQUENCY_AUTHORITATIVE.store(authoritative, Ordering::Release);
      frequency
    }
    Err(winner) => winner,
  }
}

fn query_timebase_frequency() -> u64 {
  // glibc's implementation may initialize internal state, and its public
  // safety annotation does not permit concurrent first calls. Serialize the
  // cold frequency query while leaving every Time Base read lock-free. A
  // same-thread signal cannot wait for the interrupted owner, so it reports
  // unavailable and lets `timebase_frequency` use direct calibration.
  if !claim_frequency_query(
    &TIMEBASE_FREQUENCY_LOCK,
    &TIMEBASE_FREQUENCY_OWNER_PID,
    &TIMEBASE_FREQUENCY_OWNER_TID,
  ) {
    return 0;
  }

  // SAFETY: `__ppc_get_timebase_freq` is a GLIBC_2.17 ABI on both ppc64
  // endiannesses and takes no arguments. It reads Linux's vDSO-published Time
  // Base rate, with `/proc/cpuinfo` as the glibc fallback.
  let frequency = unsafe { __ppc_get_timebase_freq() };
  release_frequency_query(&TIMEBASE_FREQUENCY_LOCK, &TIMEBASE_FREQUENCY_OWNER_TID);
  frequency
}

fn claim_frequency_query(lock: &AtomicBool, owner_pid: &AtomicI32, owner_tid: &AtomicI32) -> bool {
  let process_id = super::process_id();
  let thread_id = super::current_thread_id();
  loop {
    if !lock.load(Ordering::Acquire) {
      let owner = owner_tid.load(Ordering::Acquire);
      if owner == thread_id {
        return false;
      }
      if owner != 0 {
        if owner_pid.load(Ordering::Acquire) != process_id {
          let _ = owner_tid.compare_exchange(owner, 0, Ordering::AcqRel, Ordering::Acquire);
          continue;
        }
        spin_loop();
        continue;
      }

      // Publishing the process before the thread claim keeps an inherited
      // pre-lock owner distinguishable after fork. Same-process contenders
      // race only identical PID stores.
      owner_pid.store(process_id, Ordering::Relaxed);
      if owner_tid
        .compare_exchange(0, thread_id, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
      {
        continue;
      }
      if lock.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok() {
        return true;
      }
      let _ = owner_tid.compare_exchange(thread_id, 0, Ordering::Release, Ordering::Relaxed);
      continue;
    }

    if owner_pid.load(Ordering::Acquire) != process_id {
      if lock.compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire).is_ok() {
        owner_tid.store(0, Ordering::Release);
      }
      continue;
    }
    if owner_tid.load(Ordering::Relaxed) == thread_id {
      return false;
    }
    spin_loop();
  }
}

fn release_frequency_query(lock: &AtomicBool, owner_tid: &AtomicI32) {
  // Unlock first: a same-thread signal in the two-store window takes the
  // finite calibration fallback instead of waiting for its interrupted owner.
  lock.store(false, Ordering::Release);
  owner_tid.store(0, Ordering::Release);
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  match INSTANT_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_TIMEBASE => mftb(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_SC | PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED => clock_monotonic_sc(),
    PROVIDER_CLOCK_MONOTONIC_SCV | PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED => clock_monotonic_scv(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SC => clock_monotonic_raw_sc(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV => clock_monotonic_raw_scv(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_BOOTTIME_SC | PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED => clock_boottime_sc(),
    PROVIDER_CLOCK_BOOTTIME_SCV | PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED => clock_boottime_scv(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => ticks_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_after_selection() -> u64 {
  read_provider(selected_instant_provider(), false)
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  match ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_TIMEBASE => mftb_ordered(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SC => clock_monotonic_sc_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED => clock_monotonic_sc_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SCV => clock_monotonic_scv_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED => clock_monotonic_scv_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SC => clock_monotonic_raw_sc_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED => clock_monotonic_raw_sc_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV => clock_monotonic_raw_scv_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED => clock_monotonic_raw_scv_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso_ordered(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SC => clock_boottime_sc_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED => clock_boottime_sc_os_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SCV => clock_boottime_scv_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED => clock_boottime_scv_os_ordered(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered(),
    _ => ticks_ordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_after_selection() -> u64 {
  read_provider(selected_ordered_provider(), true)
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  match ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_TIMEBASE => mftb(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_SC | PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED => clock_monotonic_sc(),
    PROVIDER_CLOCK_MONOTONIC_SCV | PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED => clock_monotonic_scv(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SC | PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED => {
      clock_monotonic_raw_sc()
    }
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV | PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED => {
      clock_monotonic_raw_scv()
    }
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_BOOTTIME_SC | PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED => clock_boottime_sc(),
    PROVIDER_CLOCK_BOOTTIME_SCV | PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED => clock_boottime_scv(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => ticks_ordered_unordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_unordered_after_selection() -> u64 {
  read_provider(selected_ordered_provider(), false)
}

#[inline]
pub(crate) fn instant_frequency() -> u64 {
  if instant_uses_timebase() { timebase_frequency() } else { 1_000_000_000 }
}

#[inline]
pub(crate) fn ordered_frequency() -> u64 {
  if ordered_uses_timebase() { timebase_frequency() } else { 1_000_000_000 }
}

#[inline]
pub(crate) fn instant_uses_timebase() -> bool {
  selected_instant_provider() == PROVIDER_TIMEBASE
}

#[inline]
pub(crate) fn instant_read_cost() -> crate::ThreadCpuReadCost {
  instant_read_cost_for(selected_instant_provider())
}

const fn instant_read_cost_for(provider: u8) -> crate::ThreadCpuReadCost {
  match provider {
    PROVIDER_TIMEBASE
    | PROVIDER_CLOCK_MONOTONIC_VDSO
    | PROVIDER_CLOCK_MONOTONIC_RAW_VDSO
    | PROVIDER_CLOCK_BOOTTIME_VDSO => crate::ThreadCpuReadCost::Inline,
    // The libc ABI may use the vDSO or enter the kernel. Only the selected Time
    // Base route is guaranteed to remain in userspace.
    _ => crate::ThreadCpuReadCost::SystemCall,
  }
}

#[cfg(test)]
#[test]
fn instant_read_cost_only_marks_guaranteed_userspace_paths_inline() {
  assert_eq!(instant_read_cost_for(PROVIDER_TIMEBASE), crate::ThreadCpuReadCost::Inline);
  assert_eq!(instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC), crate::ThreadCpuReadCost::SystemCall,);
  assert_eq!(
    instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC_RAW),
    crate::ThreadCpuReadCost::SystemCall,
  );
  assert_eq!(
    instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC_SC),
    crate::ThreadCpuReadCost::SystemCall,
  );
  assert_eq!(instant_read_cost_for(PROVIDER_CLOCK_BOOTTIME_VDSO), crate::ThreadCpuReadCost::Inline,);
}

#[inline]
pub(crate) fn ordered_uses_timebase() -> bool {
  selected_ordered_provider() == PROVIDER_TIMEBASE
}

#[inline]
pub(crate) fn needs_frequency_recalibration() -> bool {
  let direct_selected = instant_uses_timebase() || ordered_uses_timebase();
  direct_selected && {
    let _ = timebase_frequency();
    !FREQUENCY_AUTHORITATIVE.load(Ordering::Acquire)
  }
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
  let fallback =
    if ordered { PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED } else { PROVIDER_CLOCK_MONOTONIC_SC };
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
  let scv_eligible = scv_available();
  let clock_raw_available = libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW).is_some();
  let sc_raw_available = raw_clock_nanos_sc(libc::CLOCK_MONOTONIC_RAW).is_some();
  let scv_raw_available = scv_eligible && raw_clock_nanos_scv(libc::CLOCK_MONOTONIC_RAW).is_some();
  let _ = super::linux_vdso::install();
  let vdso_available = super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC).is_some();
  let vdso_raw_available = super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC_RAW).is_some();
  let clock_boottime_available = libc_clock_nanos(libc::CLOCK_BOOTTIME).is_some();
  let sc_boottime_available = raw_clock_nanos_sc(libc::CLOCK_BOOTTIME).is_some();
  let scv_boottime_available = scv_eligible && raw_clock_nanos_scv(libc::CLOCK_BOOTTIME).is_some();
  let vdso_boottime_available = super::linux_vdso::clock_nanos(libc::CLOCK_BOOTTIME).is_some();
  let (samples, candidate_providers, candidate_count) = measure_candidates(
    ordered,
    scv_eligible,
    clock_raw_available,
    sc_raw_available,
    scv_raw_available,
    vdso_available,
    vdso_raw_available,
    clock_boottime_available,
    sc_boottime_available,
    scv_boottime_available,
    vdso_boottime_available,
  );
  #[cfg(not(feature = "bench-internal"))]
  let _ = (candidate_providers, candidate_count);

  let sc_decision = prefer_challenger(samples.sc, samples.clock);
  let mut fallback = if sc_decision.challenger_selected {
    PROVIDER_CLOCK_MONOTONIC_SC
  } else {
    PROVIDER_CLOCK_MONOTONIC
  };
  let mut fallback_samples = candidate_samples(samples, fallback);
  let sc_os_ordered_decision = if ordered {
    prefer_challenger(samples.sc_os_ordered, fallback_samples)
  } else {
    empty_decision()
  };
  if sc_os_ordered_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED;
    fallback_samples = samples.sc_os_ordered;
  }
  let scv_decision =
    if scv_eligible { prefer_challenger(samples.scv, fallback_samples) } else { empty_decision() };
  if scv_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_SCV;
    fallback_samples = samples.scv;
  }
  let scv_os_ordered_decision = if ordered && scv_eligible {
    prefer_challenger(samples.scv_os_ordered, fallback_samples)
  } else {
    empty_decision()
  };
  if scv_os_ordered_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED;
    fallback_samples = samples.scv_os_ordered;
  }
  let clock_raw_decision = if clock_raw_available {
    prefer_challenger(samples.clock_raw, fallback_samples)
  } else {
    empty_decision()
  };
  if clock_raw_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_RAW;
    fallback_samples = samples.clock_raw;
  }
  let sc_raw_decision = if sc_raw_available {
    prefer_challenger(samples.sc_raw, fallback_samples)
  } else {
    empty_decision()
  };
  if sc_raw_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_RAW_SC;
    fallback_samples = samples.sc_raw;
  }
  let sc_raw_os_ordered_decision = if ordered && sc_raw_available {
    prefer_challenger(samples.sc_raw_os_ordered, fallback_samples)
  } else {
    empty_decision()
  };
  if sc_raw_os_ordered_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED;
    fallback_samples = samples.sc_raw_os_ordered;
  }
  let scv_raw_decision = if scv_raw_available {
    prefer_challenger(samples.scv_raw, fallback_samples)
  } else {
    empty_decision()
  };
  if scv_raw_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_RAW_SCV;
    fallback_samples = samples.scv_raw;
  }
  let scv_raw_os_ordered_decision = if ordered && scv_raw_available {
    prefer_challenger(samples.scv_raw_os_ordered, fallback_samples)
  } else {
    empty_decision()
  };
  if scv_raw_os_ordered_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED;
    fallback_samples = samples.scv_raw_os_ordered;
  }
  let vdso_decision = if vdso_available {
    prefer_challenger(samples.vdso, fallback_samples)
  } else {
    empty_decision()
  };
  if vdso_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_VDSO;
    fallback_samples = samples.vdso;
  }
  let vdso_raw_decision = if vdso_raw_available {
    prefer_challenger(samples.vdso_raw, fallback_samples)
  } else {
    empty_decision()
  };
  if vdso_raw_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_MONOTONIC_RAW_VDSO;
    fallback_samples = samples.vdso_raw;
  }
  let clock_boottime_decision = if clock_boottime_available {
    prefer_challenger(samples.clock_boottime, fallback_samples)
  } else {
    empty_decision()
  };
  if clock_boottime_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_BOOTTIME;
    fallback_samples = samples.clock_boottime;
  }
  let sc_boottime_decision = if sc_boottime_available {
    prefer_challenger(samples.sc_boottime, fallback_samples)
  } else {
    empty_decision()
  };
  if sc_boottime_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_BOOTTIME_SC;
    fallback_samples = samples.sc_boottime;
  }
  let sc_boottime_os_ordered_decision = if ordered && sc_boottime_available {
    prefer_challenger(samples.sc_boottime_os_ordered, fallback_samples)
  } else {
    empty_decision()
  };
  if sc_boottime_os_ordered_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED;
    fallback_samples = samples.sc_boottime_os_ordered;
  }
  let scv_boottime_decision = if scv_boottime_available {
    prefer_challenger(samples.scv_boottime, fallback_samples)
  } else {
    empty_decision()
  };
  if scv_boottime_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_BOOTTIME_SCV;
    fallback_samples = samples.scv_boottime;
  }
  let scv_boottime_os_ordered_decision = if ordered && scv_boottime_available {
    prefer_challenger(samples.scv_boottime_os_ordered, fallback_samples)
  } else {
    empty_decision()
  };
  if scv_boottime_os_ordered_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED;
    fallback_samples = samples.scv_boottime_os_ordered;
  }
  let vdso_boottime_decision = if vdso_boottime_available {
    prefer_challenger(samples.vdso_boottime, fallback_samples)
  } else {
    empty_decision()
  };
  if vdso_boottime_decision.challenger_selected {
    fallback = PROVIDER_CLOCK_BOOTTIME_VDSO;
    fallback_samples = samples.vdso_boottime;
  }
  let direct_decision = prefer_challenger(samples.direct, fallback_samples);
  let provider = if direct_decision.challenger_selected { PROVIDER_TIMEBASE } else { fallback };

  #[cfg(feature = "bench-internal")]
  publish_evidence(ordered, ProbeEvidence {
    candidate_count,
    candidate_providers,
    scv_eligible,
    clock_raw_available,
    sc_raw_available,
    scv_raw_available,
    vdso_available,
    vdso_raw_available,
    clock_boottime_available,
    sc_boottime_available,
    scv_boottime_available,
    vdso_boottime_available,
    reads_per_batch: PROBE_READS,
    direct_batches_ns: samples.direct,
    clock_batches_ns: samples.clock,
    sc_batches_ns: samples.sc,
    sc_os_ordered_batches_ns: samples.sc_os_ordered,
    scv_batches_ns: samples.scv,
    scv_os_ordered_batches_ns: samples.scv_os_ordered,
    clock_raw_batches_ns: samples.clock_raw,
    sc_raw_batches_ns: samples.sc_raw,
    sc_raw_os_ordered_batches_ns: samples.sc_raw_os_ordered,
    scv_raw_batches_ns: samples.scv_raw,
    scv_raw_os_ordered_batches_ns: samples.scv_raw_os_ordered,
    vdso_batches_ns: samples.vdso,
    vdso_raw_batches_ns: samples.vdso_raw,
    clock_boottime_batches_ns: samples.clock_boottime,
    sc_boottime_batches_ns: samples.sc_boottime,
    sc_boottime_os_ordered_batches_ns: samples.sc_boottime_os_ordered,
    scv_boottime_batches_ns: samples.scv_boottime,
    scv_boottime_os_ordered_batches_ns: samples.scv_boottime_os_ordered,
    vdso_boottime_batches_ns: samples.vdso_boottime,
    direct_median_ns: median(samples.direct),
    clock_median_ns: median(samples.clock),
    sc_median_ns: median(samples.sc),
    sc_os_ordered_median_ns: median(samples.sc_os_ordered),
    scv_median_ns: median(samples.scv),
    scv_os_ordered_median_ns: median(samples.scv_os_ordered),
    clock_raw_median_ns: median(samples.clock_raw),
    sc_raw_median_ns: median(samples.sc_raw),
    sc_raw_os_ordered_median_ns: median(samples.sc_raw_os_ordered),
    scv_raw_median_ns: median(samples.scv_raw),
    scv_raw_os_ordered_median_ns: median(samples.scv_raw_os_ordered),
    vdso_median_ns: median(samples.vdso),
    vdso_raw_median_ns: median(samples.vdso_raw),
    clock_boottime_median_ns: median(samples.clock_boottime),
    sc_boottime_median_ns: median(samples.sc_boottime),
    sc_boottime_os_ordered_median_ns: median(samples.sc_boottime_os_ordered),
    scv_boottime_median_ns: median(samples.scv_boottime),
    scv_boottime_os_ordered_median_ns: median(samples.scv_boottime_os_ordered),
    vdso_boottime_median_ns: median(samples.vdso_boottime),
    sc_allowance_ns: sc_decision.allowance,
    sc_decisive_wins: sc_decision.decisive_wins,
    sc_os_ordered_allowance_ns: sc_os_ordered_decision.allowance,
    sc_os_ordered_decisive_wins: sc_os_ordered_decision.decisive_wins,
    scv_allowance_ns: scv_decision.allowance,
    scv_decisive_wins: scv_decision.decisive_wins,
    scv_os_ordered_allowance_ns: scv_os_ordered_decision.allowance,
    scv_os_ordered_decisive_wins: scv_os_ordered_decision.decisive_wins,
    clock_raw_allowance_ns: clock_raw_decision.allowance,
    clock_raw_decisive_wins: clock_raw_decision.decisive_wins,
    sc_raw_allowance_ns: sc_raw_decision.allowance,
    sc_raw_decisive_wins: sc_raw_decision.decisive_wins,
    sc_raw_os_ordered_allowance_ns: sc_raw_os_ordered_decision.allowance,
    sc_raw_os_ordered_decisive_wins: sc_raw_os_ordered_decision.decisive_wins,
    scv_raw_allowance_ns: scv_raw_decision.allowance,
    scv_raw_decisive_wins: scv_raw_decision.decisive_wins,
    scv_raw_os_ordered_allowance_ns: scv_raw_os_ordered_decision.allowance,
    scv_raw_os_ordered_decisive_wins: scv_raw_os_ordered_decision.decisive_wins,
    vdso_allowance_ns: vdso_decision.allowance,
    vdso_decisive_wins: vdso_decision.decisive_wins,
    vdso_raw_allowance_ns: vdso_raw_decision.allowance,
    vdso_raw_decisive_wins: vdso_raw_decision.decisive_wins,
    clock_boottime_allowance_ns: clock_boottime_decision.allowance,
    clock_boottime_decisive_wins: clock_boottime_decision.decisive_wins,
    sc_boottime_allowance_ns: sc_boottime_decision.allowance,
    sc_boottime_decisive_wins: sc_boottime_decision.decisive_wins,
    sc_boottime_os_ordered_allowance_ns: sc_boottime_os_ordered_decision.allowance,
    sc_boottime_os_ordered_decisive_wins: sc_boottime_os_ordered_decision.decisive_wins,
    scv_boottime_allowance_ns: scv_boottime_decision.allowance,
    scv_boottime_decisive_wins: scv_boottime_decision.decisive_wins,
    scv_boottime_os_ordered_allowance_ns: scv_boottime_os_ordered_decision.allowance,
    scv_boottime_os_ordered_decisive_wins: scv_boottime_os_ordered_decision.decisive_wins,
    vdso_boottime_allowance_ns: vdso_boottime_decision.allowance,
    vdso_boottime_decisive_wins: vdso_boottime_decision.decisive_wins,
    direct_allowance_ns: direct_decision.allowance,
    direct_decisive_wins: direct_decision.decisive_wins,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    selected_provider: provider_from_raw(provider),
  });

  provider
}

#[inline]
fn scv_available() -> bool {
  // SAFETY: getauxval has no pointer preconditions. Linux publishes SCV in
  // AT_HWCAP2 only when userspace may execute `scv 0` for system calls.
  (unsafe { libc::getauxval(AT_HWCAP2) }) & PPC_FEATURE2_SCV != 0
}

fn measure_candidates(
  ordered: bool,
  scv_eligible: bool,
  clock_raw_available: bool,
  sc_raw_available: bool,
  scv_raw_available: bool,
  vdso_available: bool,
  vdso_raw_available: bool,
  clock_boottime_available: bool,
  sc_boottime_available: bool,
  scv_boottime_available: bool,
  vdso_boottime_available: bool,
) -> (ProbeSamples, [u8; MAX_CANDIDATES], usize) {
  warm_candidate(ordered, PROVIDER_TIMEBASE);
  warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC);
  warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_SC);
  if ordered {
    warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED);
  }
  if scv_eligible {
    warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_SCV);
    if ordered {
      warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED);
    }
  }
  if clock_raw_available {
    warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_RAW);
  }
  if sc_raw_available {
    warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_RAW_SC);
    if ordered {
      warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED);
    }
  }
  if scv_raw_available {
    warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_RAW_SCV);
    if ordered {
      warm_candidate(ordered, PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED);
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
  if sc_boottime_available {
    warm_candidate(ordered, PROVIDER_CLOCK_BOOTTIME_SC);
    if ordered {
      warm_candidate(ordered, PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED);
    }
  }
  if scv_boottime_available {
    warm_candidate(ordered, PROVIDER_CLOCK_BOOTTIME_SCV);
    if ordered {
      warm_candidate(ordered, PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED);
    }
  }
  if vdso_boottime_available {
    warm_candidate(ordered, PROVIDER_CLOCK_BOOTTIME_VDSO);
  }

  let mut candidates = [PROVIDER_UNKNOWN; MAX_CANDIDATES];
  let mut candidate_count = 0;
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_TIMEBASE, true);
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_CLOCK_MONOTONIC, true);
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_CLOCK_MONOTONIC_SC, true);
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED,
    ordered,
  );
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_CLOCK_MONOTONIC_SCV, scv_eligible);
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED,
    ordered && scv_eligible,
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
    PROVIDER_CLOCK_MONOTONIC_RAW_SC,
    sc_raw_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED,
    ordered && sc_raw_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV,
    scv_raw_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED,
    ordered && scv_raw_available,
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
    PROVIDER_CLOCK_BOOTTIME_SC,
    sc_boottime_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED,
    ordered && sc_boottime_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_BOOTTIME_SCV,
    scv_boottime_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED,
    ordered && scv_boottime_available,
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
    sc: [u64::MAX; PROBE_BATCHES],
    sc_os_ordered: [u64::MAX; PROBE_BATCHES],
    scv: [u64::MAX; PROBE_BATCHES],
    scv_os_ordered: [u64::MAX; PROBE_BATCHES],
    clock_raw: [u64::MAX; PROBE_BATCHES],
    sc_raw: [u64::MAX; PROBE_BATCHES],
    sc_raw_os_ordered: [u64::MAX; PROBE_BATCHES],
    scv_raw: [u64::MAX; PROBE_BATCHES],
    scv_raw_os_ordered: [u64::MAX; PROBE_BATCHES],
    vdso: [u64::MAX; PROBE_BATCHES],
    vdso_raw: [u64::MAX; PROBE_BATCHES],
    clock_boottime: [u64::MAX; PROBE_BATCHES],
    sc_boottime: [u64::MAX; PROBE_BATCHES],
    sc_boottime_os_ordered: [u64::MAX; PROBE_BATCHES],
    scv_boottime: [u64::MAX; PROBE_BATCHES],
    scv_boottime_os_ordered: [u64::MAX; PROBE_BATCHES],
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
        PROVIDER_TIMEBASE => samples.direct[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC => samples.clock[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_SC => samples.sc[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED => samples.sc_os_ordered[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_SCV => samples.scv[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED => samples.scv_os_ordered[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_RAW => samples.clock_raw[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_RAW_SC => samples.sc_raw[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED => {
          samples.sc_raw_os_ordered[sample] = elapsed;
        }
        PROVIDER_CLOCK_MONOTONIC_RAW_SCV => samples.scv_raw[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED => {
          samples.scv_raw_os_ordered[sample] = elapsed;
        }
        PROVIDER_CLOCK_MONOTONIC_VDSO => samples.vdso[sample] = elapsed,
        PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => samples.vdso_raw[sample] = elapsed,
        PROVIDER_CLOCK_BOOTTIME => samples.clock_boottime[sample] = elapsed,
        PROVIDER_CLOCK_BOOTTIME_SC => samples.sc_boottime[sample] = elapsed,
        PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED => {
          samples.sc_boottime_os_ordered[sample] = elapsed;
        }
        PROVIDER_CLOCK_BOOTTIME_SCV => samples.scv_boottime[sample] = elapsed,
        PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED => {
          samples.scv_boottime_os_ordered[sample] = elapsed;
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
    PROVIDER_TIMEBASE => samples.direct,
    PROVIDER_CLOCK_MONOTONIC_SC => samples.sc,
    PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED => samples.sc_os_ordered,
    PROVIDER_CLOCK_MONOTONIC_SCV => samples.scv,
    PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED => samples.scv_os_ordered,
    PROVIDER_CLOCK_MONOTONIC_RAW => samples.clock_raw,
    PROVIDER_CLOCK_MONOTONIC_RAW_SC => samples.sc_raw,
    PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED => samples.sc_raw_os_ordered,
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV => samples.scv_raw,
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED => samples.scv_raw_os_ordered,
    PROVIDER_CLOCK_MONOTONIC_VDSO => samples.vdso,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => samples.vdso_raw,
    PROVIDER_CLOCK_BOOTTIME => samples.clock_boottime,
    PROVIDER_CLOCK_BOOTTIME_SC => samples.sc_boottime,
    PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED => samples.sc_boottime_os_ordered,
    PROVIDER_CLOCK_BOOTTIME_SCV => samples.scv_boottime,
    PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED => samples.scv_boottime_os_ordered,
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
  let start = raw_clock_nanos_sc(libc::CLOCK_MONOTONIC_RAW)?;
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
  let elapsed = raw_clock_nanos_sc(libc::CLOCK_MONOTONIC_RAW)?.checked_sub(start);
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
  read_provider(PROBE_INSTANT_PROVIDER.load(Ordering::Relaxed), false)
}

#[inline(always)]
fn probe_ordered_hot_path() -> u64 {
  match PROBE_ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_TIMEBASE => mftb_ordered(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SC => clock_monotonic_sc_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED => clock_monotonic_sc_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SCV => clock_monotonic_scv_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED => clock_monotonic_scv_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SC => clock_monotonic_raw_sc_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED => clock_monotonic_raw_sc_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV => clock_monotonic_raw_scv_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED => clock_monotonic_raw_scv_os_ordered(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso_ordered(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SC => clock_boottime_sc_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED => clock_boottime_sc_os_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SCV => clock_boottime_scv_ordered(),
    PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED => clock_boottime_scv_os_ordered(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered(),
    _ => clock_monotonic_ordered(),
  }
}

#[inline(always)]
fn read_provider(provider: u8, ordered: bool) -> u64 {
  if ordered {
    match provider {
      PROVIDER_TIMEBASE => mftb_ordered(),
      PROVIDER_CLOCK_MONOTONIC => clock_monotonic_ordered(),
      PROVIDER_CLOCK_MONOTONIC_SC => clock_monotonic_sc_ordered(),
      PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED => clock_monotonic_sc_os_ordered(),
      PROVIDER_CLOCK_MONOTONIC_SCV => clock_monotonic_scv_ordered(),
      PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED => clock_monotonic_scv_os_ordered(),
      PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered(),
      PROVIDER_CLOCK_MONOTONIC_RAW_SC => clock_monotonic_raw_sc_ordered(),
      PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED => clock_monotonic_raw_sc_os_ordered(),
      PROVIDER_CLOCK_MONOTONIC_RAW_SCV => clock_monotonic_raw_scv_ordered(),
      PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED => clock_monotonic_raw_scv_os_ordered(),
      PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered(),
      PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso_ordered(),
      PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered(),
      PROVIDER_CLOCK_BOOTTIME_SC => clock_boottime_sc_ordered(),
      PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED => clock_boottime_sc_os_ordered(),
      PROVIDER_CLOCK_BOOTTIME_SCV => clock_boottime_scv_ordered(),
      PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED => clock_boottime_scv_os_ordered(),
      PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered(),
      _ => clock_monotonic_ordered(),
    }
  } else {
    match provider {
      PROVIDER_TIMEBASE => mftb(),
      PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
      PROVIDER_CLOCK_MONOTONIC_SC | PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED => clock_monotonic_sc(),
      PROVIDER_CLOCK_MONOTONIC_SCV | PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED => {
        clock_monotonic_scv()
      }
      PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
      PROVIDER_CLOCK_MONOTONIC_RAW_SC | PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED => {
        clock_monotonic_raw_sc()
      }
      PROVIDER_CLOCK_MONOTONIC_RAW_SCV | PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED => {
        clock_monotonic_raw_scv()
      }
      PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
      PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso(),
      PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
      PROVIDER_CLOCK_BOOTTIME_SC | PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED => clock_boottime_sc(),
      PROVIDER_CLOCK_BOOTTIME_SCV | PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED => clock_boottime_scv(),
      PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
      _ => clock_monotonic(),
    }
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
fn clock_monotonic_ordered() -> u64 {
  power_ordering_barrier();
  clock_monotonic()
}

#[inline(always)]
fn clock_monotonic() -> u64 {
  libc_clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| raw_clock_nanos_sc(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_sc_ordered() -> u64 {
  power_ordering_barrier();
  clock_monotonic_sc()
}

#[inline(always)]
fn clock_monotonic_scv_ordered() -> u64 {
  power_ordering_barrier();
  clock_monotonic_scv()
}

#[inline(always)]
fn clock_monotonic_raw_ordered() -> u64 {
  power_ordering_barrier();
  clock_monotonic_raw()
}

#[inline(always)]
fn clock_boottime_ordered() -> u64 {
  power_ordering_barrier();
  clock_boottime()
}

#[inline(always)]
fn clock_monotonic_raw_sc_ordered() -> u64 {
  power_ordering_barrier();
  clock_monotonic_raw_sc()
}

#[inline(always)]
fn clock_monotonic_raw_scv_ordered() -> u64 {
  power_ordering_barrier();
  clock_monotonic_raw_scv()
}

#[inline(always)]
fn clock_boottime_sc_ordered() -> u64 {
  power_ordering_barrier();
  clock_boottime_sc()
}

#[inline(always)]
fn clock_boottime_scv_ordered() -> u64 {
  power_ordering_barrier();
  clock_boottime_scv()
}

#[inline(always)]
fn clock_monotonic_sc_os_ordered() -> u64 {
  // Power ISA defines SC and SCV as context-synchronizing instructions. The
  // syscall asm's memory clobber supplies the matching compiler-ordering edge
  // before Linux reads the clock.
  raw_clock_nanos_sc(libc::CLOCK_MONOTONIC).unwrap_or_else(clock_monotonic_ordered)
}

#[inline(always)]
fn clock_monotonic_scv_os_ordered() -> u64 {
  raw_clock_nanos_scv(libc::CLOCK_MONOTONIC).unwrap_or_else(clock_monotonic_ordered)
}

#[inline(always)]
fn clock_monotonic_raw_sc_os_ordered() -> u64 {
  raw_clock_nanos_sc(libc::CLOCK_MONOTONIC_RAW).unwrap_or_else(clock_monotonic_raw_ordered)
}

#[inline(always)]
fn clock_monotonic_raw_scv_os_ordered() -> u64 {
  raw_clock_nanos_scv(libc::CLOCK_MONOTONIC_RAW).unwrap_or_else(clock_monotonic_raw_ordered)
}

#[inline(always)]
fn clock_boottime_sc_os_ordered() -> u64 {
  raw_clock_nanos_sc(libc::CLOCK_BOOTTIME).unwrap_or_else(clock_boottime_ordered)
}

#[inline(always)]
fn clock_boottime_scv_os_ordered() -> u64 {
  raw_clock_nanos_scv(libc::CLOCK_BOOTTIME).unwrap_or_else(clock_boottime_ordered)
}

#[inline(always)]
fn clock_monotonic_sc() -> u64 {
  raw_clock_nanos_sc(libc::CLOCK_MONOTONIC)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_scv() -> u64 {
  raw_clock_nanos_scv(libc::CLOCK_MONOTONIC)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC))
    .or_else(|| raw_clock_nanos_sc(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_raw() -> u64 {
  libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| raw_clock_nanos_sc(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_raw_sc() -> u64 {
  raw_clock_nanos_sc(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_raw_scv() -> u64 {
  raw_clock_nanos_scv(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .or_else(|| raw_clock_nanos_sc(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_boottime() -> u64 {
  libc_clock_nanos(libc::CLOCK_BOOTTIME)
    .or_else(|| raw_clock_nanos_sc(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_boottime_sc() -> u64 {
  raw_clock_nanos_sc(libc::CLOCK_BOOTTIME)
    .or_else(|| libc_clock_nanos(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_boottime_scv() -> u64 {
  raw_clock_nanos_scv(libc::CLOCK_BOOTTIME)
    .or_else(|| libc_clock_nanos(libc::CLOCK_BOOTTIME))
    .or_else(|| raw_clock_nanos_sc(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_vdso() -> u64 {
  super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC))
    .or_else(|| raw_clock_nanos_sc(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_raw_vdso() -> u64 {
  super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .or_else(|| raw_clock_nanos_sc(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_boottime_vdso() -> u64 {
  super::linux_vdso::clock_nanos(libc::CLOCK_BOOTTIME)
    .or_else(|| libc_clock_nanos(libc::CLOCK_BOOTTIME))
    .or_else(|| raw_clock_nanos_sc(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_vdso_ordered() -> u64 {
  power_ordering_barrier();
  clock_monotonic_vdso()
}

#[inline(always)]
fn clock_monotonic_raw_vdso_ordered() -> u64 {
  power_ordering_barrier();
  clock_monotonic_raw_vdso()
}

#[inline(always)]
fn clock_boottime_vdso_ordered() -> u64 {
  power_ordering_barrier();
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
fn raw_clock_nanos_sc(clock_id: libc::clockid_t) -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  let mut status = clock_id as libc::c_long;
  let syscall_number = libc::SYS_clock_gettime;
  let output_pointer = value.as_mut_ptr();
  // SAFETY: the powerpc64 Linux SC ABI places the syscall number in r0 and
  // arguments/result in r3-r8. The complete Linux syscall clobber set mirrors
  // glibc's `SYSCALL_SC` macro.
  unsafe {
    asm!(
      "sc",
      inlateout("r0") syscall_number => _,
      inlateout("r3") status,
      inlateout("r4") output_pointer => _,
      lateout("r5") _,
      lateout("r6") _,
      lateout("r7") _,
      lateout("r8") _,
      lateout("r9") _,
      lateout("r10") _,
      lateout("r11") _,
      lateout("r12") _,
      lateout("xer") _,
      lateout("cr0") _,
      lateout("ctr") _,
      options(nostack),
    );
  }
  if status != 0 {
    return None;
  }
  // SAFETY: successful clock_gettime initialized the output.
  timespec_nanos(unsafe { value.assume_init() })
}

#[inline(always)]
fn raw_clock_nanos_scv(clock_id: libc::clockid_t) -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  let mut status = clock_id as libc::c_long;
  let syscall_number = libc::SYS_clock_gettime;
  let output_pointer = value.as_mut_ptr();
  // SAFETY: HWCAP2 gates `scv 0`. Linux documents r0 and r3-r8 as its only
  // argument registers, with negative errno returned directly in r3. The
  // complete clobber set mirrors glibc's `SYSCALL_SCV` macro.
  unsafe {
    asm!(
      ".machine push",
      ".machine power9",
      "scv 0",
      ".machine pop",
      inlateout("r0") syscall_number => _,
      inlateout("r3") status,
      inlateout("r4") output_pointer => _,
      lateout("r5") _,
      lateout("r6") _,
      lateout("r7") _,
      lateout("r8") _,
      lateout("r9") _,
      lateout("r10") _,
      lateout("r11") _,
      lateout("r12") _,
      lateout("cr0") _,
      lateout("cr1") _,
      lateout("cr5") _,
      lateout("cr6") _,
      lateout("cr7") _,
      lateout("xer") _,
      lateout("lr") _,
      lateout("ctr") _,
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
    PROVIDER_TIMEBASE => WallProvider::Timebase,
    PROVIDER_CLOCK_MONOTONIC_SC => WallProvider::ClockMonotonicSc,
    PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED => WallProvider::ClockMonotonicScOsOrdered,
    PROVIDER_CLOCK_MONOTONIC_SCV => WallProvider::ClockMonotonicScv,
    PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED => WallProvider::ClockMonotonicScvOsOrdered,
    PROVIDER_CLOCK_MONOTONIC_RAW => WallProvider::ClockMonotonicRaw,
    PROVIDER_CLOCK_MONOTONIC_RAW_SC => WallProvider::ClockMonotonicRawSc,
    PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED => WallProvider::ClockMonotonicRawScOsOrdered,
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV => WallProvider::ClockMonotonicRawScv,
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED => WallProvider::ClockMonotonicRawScvOsOrdered,
    PROVIDER_CLOCK_MONOTONIC_VDSO => WallProvider::ClockMonotonicVdso,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => WallProvider::ClockMonotonicRawVdso,
    PROVIDER_CLOCK_BOOTTIME => WallProvider::ClockBoottime,
    PROVIDER_CLOCK_BOOTTIME_SC => WallProvider::ClockBoottimeSc,
    PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED => WallProvider::ClockBoottimeScOsOrdered,
    PROVIDER_CLOCK_BOOTTIME_SCV => WallProvider::ClockBoottimeScv,
    PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED => WallProvider::ClockBoottimeScvOsOrdered,
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
    PROVIDER_TIMEBASE => mftb as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_SC => clock_monotonic_sc as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_SCV => clock_monotonic_scv as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_SC => clock_monotonic_raw_sc as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV => clock_monotonic_raw_scv as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME => clock_boottime as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_SC => clock_boottime_sc as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_SCV => clock_boottime_scv as fn() -> u64,
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
    PROVIDER_TIMEBASE => mftb_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_SC => clock_monotonic_sc_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED => clock_monotonic_sc_os_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_SCV => clock_monotonic_scv_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED => clock_monotonic_scv_os_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_SC => clock_monotonic_raw_sc_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED => clock_monotonic_raw_sc_os_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV => clock_monotonic_raw_scv_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_SCV_OS_ORDERED => {
      clock_monotonic_raw_scv_os_ordered as fn() -> u64
    }
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso_ordered as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_SC => clock_boottime_sc_ordered as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED => clock_boottime_sc_os_ordered as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_SCV => clock_boottime_scv_ordered as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED => clock_boottime_scv_os_ordered as fn() -> u64,
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered as fn() -> u64,
    _ => clock_monotonic_ordered as fn() -> u64,
  };
  BenchPrimitive { name: provider_from_raw(provider).name(), read }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_candidate_primitives() -> ([BenchPrimitive; MAX_CANDIDATES], usize) {
  let evidence = bench_instant_evidence();
  let mut primitives = [instant_bench_primitive(PROVIDER_CLOCK_MONOTONIC); MAX_CANDIDATES];
  for (index, provider) in
    evidence.candidate_providers[..evidence.candidate_count].iter().enumerate()
  {
    primitives[index] = instant_bench_primitive(*provider);
  }
  (primitives, evidence.candidate_count)
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_ordered_candidate_primitives() -> ([BenchPrimitive; MAX_CANDIDATES], usize) {
  let evidence = bench_ordered_evidence();
  let mut primitives = [ordered_bench_primitive(PROVIDER_CLOCK_MONOTONIC); MAX_CANDIDATES];
  for (index, provider) in
    evidence.candidate_providers[..evidence.candidate_count].iter().enumerate()
  {
    primitives[index] = ordered_bench_primitive(*provider);
  }
  (primitives, evidence.candidate_count)
}

#[cfg(feature = "bench-internal")]
macro_rules! exact_bench_reader {
  ($name:ident, $reader:ident) => {
    #[inline(always)]
    #[allow(dead_code)] // Candidate serializers may use the equivalent BenchPrimitive pointer.
    pub(crate) fn $name() -> u64 {
      $reader()
    }
  };
}

#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_timebase, mftb);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_timebase_ordered, mftb_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic, clock_monotonic);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_ordered, clock_monotonic_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_sc, clock_monotonic_sc);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_sc_ordered, clock_monotonic_sc_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_sc_os_ordered, clock_monotonic_sc_os_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_scv, clock_monotonic_scv);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_scv_ordered, clock_monotonic_scv_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_scv_os_ordered, clock_monotonic_scv_os_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_raw, clock_monotonic_raw);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_raw_ordered, clock_monotonic_raw_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_raw_sc, clock_monotonic_raw_sc);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_raw_sc_ordered, clock_monotonic_raw_sc_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_raw_sc_os_ordered,
  clock_monotonic_raw_sc_os_ordered
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_raw_scv, clock_monotonic_raw_scv);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_raw_scv_ordered, clock_monotonic_raw_scv_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_raw_scv_os_ordered,
  clock_monotonic_raw_scv_os_ordered
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_vdso, clock_monotonic_vdso);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_vdso_ordered, clock_monotonic_vdso_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_raw_vdso, clock_monotonic_raw_vdso);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_raw_vdso_ordered, clock_monotonic_raw_vdso_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime, clock_boottime);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_ordered, clock_boottime_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_sc, clock_boottime_sc);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_sc_ordered, clock_boottime_sc_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_sc_os_ordered, clock_boottime_sc_os_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_scv, clock_boottime_scv);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_scv_ordered, clock_boottime_scv_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_scv_os_ordered, clock_boottime_scv_os_ordered);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_vdso, clock_boottime_vdso);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_boottime_vdso_ordered, clock_boottime_vdso_ordered);

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
  fn frequency_query_lock_is_reentrant_and_fork_recoverable() {
    let lock = AtomicBool::new(false);
    let owner_pid = AtomicI32::new(0);
    let owner_tid = AtomicI32::new(0);

    assert!(claim_frequency_query(&lock, &owner_pid, &owner_tid));
    assert!(!claim_frequency_query(&lock, &owner_pid, &owner_tid));
    release_frequency_query(&lock, &owner_tid);

    owner_pid.store(super::super::process_id().wrapping_add(1), Ordering::Relaxed);
    owner_tid.store(-1, Ordering::Relaxed);
    assert!(claim_frequency_query(&lock, &owner_pid, &owner_tid));
    release_frequency_query(&lock, &owner_tid);

    lock.store(true, Ordering::Relaxed);
    owner_pid.store(super::super::process_id().wrapping_add(1), Ordering::Relaxed);
    owner_tid.store(-1, Ordering::Relaxed);
    assert!(claim_frequency_query(&lock, &owner_pid, &owner_tid));
    release_frequency_query(&lock, &owner_tid);
  }

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

  #[test]
  fn os_owned_syscall_candidates_are_ordered_only() {
    assert!(is_os_ordered_provider(PROVIDER_CLOCK_MONOTONIC_SC_OS_ORDERED));
    assert!(is_os_ordered_provider(PROVIDER_CLOCK_MONOTONIC_SCV_OS_ORDERED));
    assert!(is_os_ordered_provider(PROVIDER_CLOCK_MONOTONIC_RAW_SC_OS_ORDERED));
    assert!(is_os_ordered_provider(PROVIDER_CLOCK_BOOTTIME_SC_OS_ORDERED));
    assert!(is_os_ordered_provider(PROVIDER_CLOCK_BOOTTIME_SCV_OS_ORDERED));
    assert!(!is_os_ordered_provider(PROVIDER_CLOCK_MONOTONIC_SC));
  }

  #[test]
  fn candidate_list_compacts_unavailable_routes() {
    let mut candidates = [PROVIDER_UNKNOWN; 2];
    let mut count = 0;
    push_candidate(&mut candidates, &mut count, PROVIDER_CLOCK_MONOTONIC_SCV, false);
    push_candidate(&mut candidates, &mut count, PROVIDER_CLOCK_MONOTONIC, true);
    assert_eq!(count, 1);
    assert_eq!(candidates[0], PROVIDER_CLOCK_MONOTONIC);
  }
}
