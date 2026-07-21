//! Runtime wall-clock selection for Linux Armv7 and s390x.
//!
//! Both architectures can use Linux's nanosecond `CLOCK_MONOTONIC`,
//! `CLOCK_MONOTONIC_RAW`, or `CLOCK_BOOTTIME` timelines, and the cheapest
//! eligible route is an environment property: libc may
//! reach a vDSO, while a direct syscall can win when wrapper or virtualization
//! costs dominate. `Instant` and `GlobalInstant` therefore measure their
//! complete branched paths independently. Armv7 also measures the time32 and
//! time64 syscall ABIs independently when each is available.
//! A direct Arm virtual-counter read is admitted only when the kernel leaves
//! its versioned direct-vDSO clock symbol installed. Arm Linux null-patches
//! that symbol when CNTVCT is absent or firmware did not configure user access,
//! making symbol survival the non-faulting kernel proof for `mrrc`.
//!
//! Ordered direct-SVC routes also compete without a separate pre-barrier:
//! Arm exception entry is a context-synchronization event and s390 SVC is
//! serializing. Libc/vDSO routes are not eligible for that assumption because
//! they need not execute SVC, so their explicit barriers remain mandatory.
//! Bare s390 STCKF is not a candidate: Linux's own monotonic helper subtracts
//! mutable `tod_clock_base` under preemption exclusion, and the exported vDSO
//! owns the required shared-data seqlock, TOD delta, and scaling protocol.

#[cfg(feature = "bench-internal")]
use core::cell::UnsafeCell;
use core::hint::black_box;
use core::mem::MaybeUninit;
#[cfg(feature = "bench-internal")]
use core::sync::atomic::AtomicBool;
#[cfg(target_arch = "arm")]
use core::sync::atomic::AtomicU32;
use core::sync::atomic::{AtomicI32, AtomicU8, Ordering};

const PROVIDER_UNKNOWN: u8 = 0;
const PROVIDER_SELECTING: u8 = 1;
const PROVIDER_LIBC: u8 = 2;
const PROVIDER_RAW: u8 = 3;
#[cfg(target_arch = "arm")]
const PROVIDER_RAW_TIME64: u8 = 4;
const PROVIDER_LIBC_MONOTONIC_RAW: u8 = 5;
const PROVIDER_RAW_MONOTONIC_RAW: u8 = 6;
#[cfg(target_arch = "arm")]
const PROVIDER_RAW_TIME64_MONOTONIC_RAW: u8 = 7;
const PROVIDER_RAW_OS_ORDERED: u8 = 8;
#[cfg(target_arch = "arm")]
const PROVIDER_RAW_TIME64_OS_ORDERED: u8 = 9;
const PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED: u8 = 10;
#[cfg(target_arch = "arm")]
const PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED: u8 = 11;
const PROVIDER_VDSO: u8 = 12;
const PROVIDER_VDSO_MONOTONIC_RAW: u8 = 13;
#[cfg(target_arch = "arm")]
const PROVIDER_VDSO_TIME64: u8 = 14;
#[cfg(target_arch = "arm")]
const PROVIDER_VDSO_TIME64_MONOTONIC_RAW: u8 = 15;
const PROVIDER_LIBC_BOOTTIME: u8 = 16;
const PROVIDER_RAW_BOOTTIME: u8 = 17;
#[cfg(target_arch = "arm")]
const PROVIDER_RAW_TIME64_BOOTTIME: u8 = 18;
const PROVIDER_RAW_BOOTTIME_OS_ORDERED: u8 = 19;
#[cfg(target_arch = "arm")]
const PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED: u8 = 20;
const PROVIDER_VDSO_BOOTTIME: u8 = 21;
#[cfg(target_arch = "arm")]
const PROVIDER_VDSO_TIME64_BOOTTIME: u8 = 22;
#[cfg(target_arch = "arm")]
const PROVIDER_CNTVCT: u8 = 23;
#[cfg(target_arch = "arm")]
const PROVIDER_CNTVCT_ORDERED: u8 = 24;

const MAX_CANDIDATES: usize = 22;

const PROBE_BATCHES: usize = 9;
const PROBE_READS: u64 = 4096;
const PROBE_WARMUP_READS: u64 = 1024;
const REQUIRED_DECISIVE_WINS: usize = 8;

const fn is_os_ordered_provider(provider: u8) -> bool {
  match provider {
    PROVIDER_RAW_OS_ORDERED
    | PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED
    | PROVIDER_RAW_BOOTTIME_OS_ORDERED => true,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_OS_ORDERED
    | PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED
    | PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED => true,
    _ => false,
  }
}

static INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static INSTANT_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static INSTANT_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static ORDERED_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static PROBE_INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_LIBC);
static PROBE_ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_LIBC);
#[cfg(target_arch = "arm")]
static CNTVCT_FREQUENCY: AtomicU32 = AtomicU32::new(0);

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
  cntvct: [u64; PROBE_BATCHES],
  libc: [u64; PROBE_BATCHES],
  raw: [u64; PROBE_BATCHES],
  raw_os_ordered: [u64; PROBE_BATCHES],
  raw_time64: [u64; PROBE_BATCHES],
  raw_time64_os_ordered: [u64; PROBE_BATCHES],
  libc_monotonic_raw: [u64; PROBE_BATCHES],
  raw_monotonic_raw: [u64; PROBE_BATCHES],
  raw_monotonic_raw_os_ordered: [u64; PROBE_BATCHES],
  raw_time64_monotonic_raw: [u64; PROBE_BATCHES],
  raw_time64_monotonic_raw_os_ordered: [u64; PROBE_BATCHES],
  vdso: [u64; PROBE_BATCHES],
  vdso_monotonic_raw: [u64; PROBE_BATCHES],
  vdso_time64: [u64; PROBE_BATCHES],
  vdso_time64_monotonic_raw: [u64; PROBE_BATCHES],
  libc_boottime: [u64; PROBE_BATCHES],
  raw_boottime: [u64; PROBE_BATCHES],
  raw_boottime_os_ordered: [u64; PROBE_BATCHES],
  raw_time64_boottime: [u64; PROBE_BATCHES],
  raw_time64_boottime_os_ordered: [u64; PROBE_BATCHES],
  vdso_boottime: [u64; PROBE_BATCHES],
  vdso_time64_boottime: [u64; PROBE_BATCHES],
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WallProvider {
  #[cfg(target_arch = "arm")]
  ArmCntvct,
  #[cfg(target_arch = "arm")]
  ArmCntvctOrdered,
  LibcClockMonotonic,
  RawClockGettime,
  RawClockGettimeOsOrdered,
  #[cfg(target_arch = "arm")]
  RawClockGettime64,
  #[cfg(target_arch = "arm")]
  RawClockGettime64OsOrdered,
  LibcClockMonotonicRaw,
  RawClockGettimeMonotonicRaw,
  RawClockGettimeMonotonicRawOsOrdered,
  #[cfg(target_arch = "arm")]
  RawClockGettime64MonotonicRaw,
  #[cfg(target_arch = "arm")]
  RawClockGettime64MonotonicRawOsOrdered,
  VdsoClockMonotonic,
  VdsoClockMonotonicRaw,
  #[cfg(target_arch = "arm")]
  VdsoClockMonotonicTime64,
  #[cfg(target_arch = "arm")]
  VdsoClockMonotonicRawTime64,
  LibcClockBoottime,
  RawClockGettimeBoottime,
  RawClockGettimeBoottimeOsOrdered,
  #[cfg(target_arch = "arm")]
  RawClockGettime64Boottime,
  #[cfg(target_arch = "arm")]
  RawClockGettime64BoottimeOsOrdered,
  VdsoClockBoottime,
  #[cfg(target_arch = "arm")]
  VdsoClockBoottimeTime64,
}

#[cfg(feature = "bench-internal")]
impl WallProvider {
  pub(crate) const fn name(self) -> &'static str {
    match self {
      #[cfg(target_arch = "arm")]
      Self::ArmCntvct => "linux_arm_cntvct",
      #[cfg(target_arch = "arm")]
      Self::ArmCntvctOrdered => "linux_arm_dmb_ish_isb_cntvct",
      Self::LibcClockMonotonic => "linux_clock_monotonic",
      Self::RawClockGettime => "linux_clock_monotonic_syscall",
      Self::RawClockGettimeOsOrdered => "linux_clock_monotonic_syscall_os_ordered",
      #[cfg(target_arch = "arm")]
      Self::RawClockGettime64 => "linux_clock_monotonic_time64_syscall",
      #[cfg(target_arch = "arm")]
      Self::RawClockGettime64OsOrdered => "linux_clock_monotonic_time64_syscall_os_ordered",
      Self::LibcClockMonotonicRaw => "linux_clock_monotonic_raw",
      Self::RawClockGettimeMonotonicRaw => "linux_clock_monotonic_raw_syscall",
      Self::RawClockGettimeMonotonicRawOsOrdered => "linux_clock_monotonic_raw_syscall_os_ordered",
      #[cfg(target_arch = "arm")]
      Self::RawClockGettime64MonotonicRaw => "linux_clock_monotonic_raw_time64_syscall",
      #[cfg(target_arch = "arm")]
      Self::RawClockGettime64MonotonicRawOsOrdered => {
        "linux_clock_monotonic_raw_time64_syscall_os_ordered"
      }
      Self::VdsoClockMonotonic => "linux_clock_monotonic_vdso_direct",
      Self::VdsoClockMonotonicRaw => "linux_clock_monotonic_raw_vdso_direct",
      #[cfg(target_arch = "arm")]
      Self::VdsoClockMonotonicTime64 => "linux_clock_monotonic_vdso_time64_direct",
      #[cfg(target_arch = "arm")]
      Self::VdsoClockMonotonicRawTime64 => "linux_clock_monotonic_raw_vdso_time64_direct",
      Self::LibcClockBoottime => "linux_clock_boottime",
      Self::RawClockGettimeBoottime => "linux_clock_boottime_syscall",
      Self::RawClockGettimeBoottimeOsOrdered => "linux_clock_boottime_syscall_os_ordered",
      #[cfg(target_arch = "arm")]
      Self::RawClockGettime64Boottime => "linux_clock_boottime_time64_syscall",
      #[cfg(target_arch = "arm")]
      Self::RawClockGettime64BoottimeOsOrdered => "linux_clock_boottime_time64_syscall_os_ordered",
      Self::VdsoClockBoottime => "linux_clock_boottime_vdso_direct",
      #[cfg(target_arch = "arm")]
      Self::VdsoClockBoottimeTime64 => "linux_clock_boottime_vdso_time64_direct",
    }
  }
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // Architecture evidence is complete even when a serializer projects a subset.
pub(crate) struct ProbeEvidence {
  pub(crate) candidate_count: usize,
  pub(crate) candidate_providers: [u8; MAX_CANDIDATES],
  pub(crate) reads_per_batch: u64,
  pub(crate) arm_cntvct_available: bool,
  pub(crate) arm_cntvct_eligibility_basis: &'static str,
  pub(crate) s390_bare_stckf_eligible: bool,
  pub(crate) s390_bare_stckf_exclusion: &'static str,
  pub(crate) raw_available: bool,
  pub(crate) raw_time64_available: bool,
  pub(crate) libc_monotonic_raw_available: bool,
  pub(crate) raw_monotonic_raw_available: bool,
  pub(crate) raw_time64_monotonic_raw_available: bool,
  pub(crate) vdso_available: bool,
  pub(crate) vdso_monotonic_raw_available: bool,
  pub(crate) vdso_time64_available: bool,
  pub(crate) vdso_time64_monotonic_raw_available: bool,
  pub(crate) libc_boottime_available: bool,
  pub(crate) raw_boottime_available: bool,
  pub(crate) raw_time64_boottime_available: bool,
  pub(crate) vdso_boottime_available: bool,
  pub(crate) vdso_time64_boottime_available: bool,
  pub(crate) cntvct_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) libc_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) raw_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) raw_time64_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) raw_time64_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) libc_monotonic_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) raw_monotonic_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) raw_monotonic_raw_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) raw_time64_monotonic_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) raw_time64_monotonic_raw_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_monotonic_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_time64_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_time64_monotonic_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) libc_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) raw_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) raw_boottime_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) raw_time64_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) raw_time64_boottime_os_ordered_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_time64_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) cntvct_median_ns: u64,
  pub(crate) libc_median_ns: u64,
  pub(crate) raw_median_ns: u64,
  pub(crate) raw_os_ordered_median_ns: u64,
  pub(crate) raw_time64_median_ns: u64,
  pub(crate) raw_time64_os_ordered_median_ns: u64,
  pub(crate) libc_monotonic_raw_median_ns: u64,
  pub(crate) raw_monotonic_raw_median_ns: u64,
  pub(crate) raw_monotonic_raw_os_ordered_median_ns: u64,
  pub(crate) raw_time64_monotonic_raw_median_ns: u64,
  pub(crate) raw_time64_monotonic_raw_os_ordered_median_ns: u64,
  pub(crate) vdso_median_ns: u64,
  pub(crate) vdso_monotonic_raw_median_ns: u64,
  pub(crate) vdso_time64_median_ns: u64,
  pub(crate) vdso_time64_monotonic_raw_median_ns: u64,
  pub(crate) libc_boottime_median_ns: u64,
  pub(crate) raw_boottime_median_ns: u64,
  pub(crate) raw_boottime_os_ordered_median_ns: u64,
  pub(crate) raw_time64_boottime_median_ns: u64,
  pub(crate) raw_time64_boottime_os_ordered_median_ns: u64,
  pub(crate) vdso_boottime_median_ns: u64,
  pub(crate) vdso_time64_boottime_median_ns: u64,
  pub(crate) cntvct_allowance_ns: u64,
  pub(crate) cntvct_decisive_wins: usize,
  pub(crate) raw_allowance_ns: u64,
  pub(crate) raw_decisive_wins: usize,
  pub(crate) raw_os_ordered_allowance_ns: u64,
  pub(crate) raw_os_ordered_decisive_wins: usize,
  pub(crate) raw_time64_allowance_ns: u64,
  pub(crate) raw_time64_decisive_wins: usize,
  pub(crate) raw_time64_os_ordered_allowance_ns: u64,
  pub(crate) raw_time64_os_ordered_decisive_wins: usize,
  pub(crate) libc_monotonic_raw_allowance_ns: u64,
  pub(crate) libc_monotonic_raw_decisive_wins: usize,
  pub(crate) raw_monotonic_raw_allowance_ns: u64,
  pub(crate) raw_monotonic_raw_decisive_wins: usize,
  pub(crate) raw_monotonic_raw_os_ordered_allowance_ns: u64,
  pub(crate) raw_monotonic_raw_os_ordered_decisive_wins: usize,
  pub(crate) raw_time64_monotonic_raw_allowance_ns: u64,
  pub(crate) raw_time64_monotonic_raw_decisive_wins: usize,
  pub(crate) raw_time64_monotonic_raw_os_ordered_allowance_ns: u64,
  pub(crate) raw_time64_monotonic_raw_os_ordered_decisive_wins: usize,
  pub(crate) vdso_allowance_ns: u64,
  pub(crate) vdso_decisive_wins: usize,
  pub(crate) vdso_monotonic_raw_allowance_ns: u64,
  pub(crate) vdso_monotonic_raw_decisive_wins: usize,
  pub(crate) vdso_time64_allowance_ns: u64,
  pub(crate) vdso_time64_decisive_wins: usize,
  pub(crate) vdso_time64_monotonic_raw_allowance_ns: u64,
  pub(crate) vdso_time64_monotonic_raw_decisive_wins: usize,
  pub(crate) libc_boottime_allowance_ns: u64,
  pub(crate) libc_boottime_decisive_wins: usize,
  pub(crate) raw_boottime_allowance_ns: u64,
  pub(crate) raw_boottime_decisive_wins: usize,
  pub(crate) raw_boottime_os_ordered_allowance_ns: u64,
  pub(crate) raw_boottime_os_ordered_decisive_wins: usize,
  pub(crate) raw_time64_boottime_allowance_ns: u64,
  pub(crate) raw_time64_boottime_decisive_wins: usize,
  pub(crate) raw_time64_boottime_os_ordered_allowance_ns: u64,
  pub(crate) raw_time64_boottime_os_ordered_decisive_wins: usize,
  pub(crate) vdso_boottime_allowance_ns: u64,
  pub(crate) vdso_boottime_decisive_wins: usize,
  pub(crate) vdso_time64_boottime_allowance_ns: u64,
  pub(crate) vdso_time64_boottime_decisive_wins: usize,
  pub(crate) required_decisive_wins: usize,
  pub(crate) selected_provider: WallProvider,
}

#[cfg(feature = "bench-internal")]
struct EvidenceCell(UnsafeCell<MaybeUninit<ProbeEvidence>>);

// SAFETY: the process-selection owner initializes its evidence before the
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
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  match INSTANT_PROVIDER.load(Ordering::Relaxed) {
    #[cfg(target_arch = "arm")]
    PROVIDER_CNTVCT | PROVIDER_CNTVCT_ORDERED => arm_cntvct(),
    PROVIDER_LIBC => libc_clock_monotonic(),
    PROVIDER_RAW => raw_clock_monotonic(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64 => raw_clock_monotonic_time64(),
    PROVIDER_LIBC_MONOTONIC_RAW => libc_clock_monotonic_raw(),
    PROVIDER_RAW_MONOTONIC_RAW => raw_clock_monotonic_raw(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW => raw_clock_monotonic_raw_time64(),
    PROVIDER_VDSO => vdso_clock_monotonic(),
    PROVIDER_VDSO_MONOTONIC_RAW => vdso_clock_monotonic_raw(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64 => vdso_clock_monotonic_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW => vdso_clock_monotonic_raw_time64(),
    PROVIDER_LIBC_BOOTTIME => libc_clock_boottime(),
    PROVIDER_RAW_BOOTTIME | PROVIDER_RAW_BOOTTIME_OS_ORDERED => raw_clock_boottime(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME | PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED => {
      raw_clock_boottime_time64()
    }
    PROVIDER_VDSO_BOOTTIME => vdso_clock_boottime(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_BOOTTIME => vdso_clock_boottime_time64(),
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
    #[cfg(target_arch = "arm")]
    PROVIDER_CNTVCT | PROVIDER_CNTVCT_ORDERED => arm_cntvct_ordered(),
    PROVIDER_LIBC => ordered_clock_monotonic(),
    PROVIDER_RAW => ordered_raw_clock_monotonic(),
    PROVIDER_RAW_OS_ORDERED => os_ordered_raw_clock_monotonic(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64 => ordered_raw_clock_monotonic_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_OS_ORDERED => os_ordered_raw_clock_monotonic_time64(),
    PROVIDER_LIBC_MONOTONIC_RAW => ordered_libc_clock_monotonic_raw(),
    PROVIDER_RAW_MONOTONIC_RAW => ordered_raw_clock_monotonic_raw(),
    PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED => os_ordered_raw_clock_monotonic_raw(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW => ordered_raw_clock_monotonic_raw_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED => os_ordered_raw_clock_monotonic_raw_time64(),
    PROVIDER_VDSO => ordered_vdso_clock_monotonic(),
    PROVIDER_VDSO_MONOTONIC_RAW => ordered_vdso_clock_monotonic_raw(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64 => ordered_vdso_clock_monotonic_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW => ordered_vdso_clock_monotonic_raw_time64(),
    PROVIDER_LIBC_BOOTTIME => ordered_libc_clock_boottime(),
    PROVIDER_RAW_BOOTTIME => ordered_raw_clock_boottime(),
    PROVIDER_RAW_BOOTTIME_OS_ORDERED => os_ordered_raw_clock_boottime(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME => ordered_raw_clock_boottime_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED => os_ordered_raw_clock_boottime_time64(),
    PROVIDER_VDSO_BOOTTIME => ordered_vdso_clock_boottime(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_BOOTTIME => ordered_vdso_clock_boottime_time64(),
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
    #[cfg(target_arch = "arm")]
    PROVIDER_CNTVCT | PROVIDER_CNTVCT_ORDERED => arm_cntvct(),
    PROVIDER_LIBC => libc_clock_monotonic(),
    PROVIDER_RAW | PROVIDER_RAW_OS_ORDERED => raw_clock_monotonic(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64 => raw_clock_monotonic_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_OS_ORDERED => raw_clock_monotonic_time64(),
    PROVIDER_LIBC_MONOTONIC_RAW => libc_clock_monotonic_raw(),
    PROVIDER_RAW_MONOTONIC_RAW | PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED => raw_clock_monotonic_raw(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW => raw_clock_monotonic_raw_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED => raw_clock_monotonic_raw_time64(),
    PROVIDER_VDSO => vdso_clock_monotonic(),
    PROVIDER_VDSO_MONOTONIC_RAW => vdso_clock_monotonic_raw(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64 => vdso_clock_monotonic_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW => vdso_clock_monotonic_raw_time64(),
    PROVIDER_LIBC_BOOTTIME => libc_clock_boottime(),
    PROVIDER_RAW_BOOTTIME | PROVIDER_RAW_BOOTTIME_OS_ORDERED => raw_clock_boottime(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME | PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED => {
      raw_clock_boottime_time64()
    }
    PROVIDER_VDSO_BOOTTIME => vdso_clock_boottime(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_BOOTTIME => vdso_clock_boottime_time64(),
    _ => ticks_ordered_unordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_unordered_after_selection() -> u64 {
  read_instant_provider(selected_ordered_provider())
}

fn selected_instant_provider() -> u8 {
  select_provider(
    &INSTANT_PROVIDER,
    &INSTANT_PROVIDER_OWNER_PID,
    &INSTANT_PROVIDER_OWNER_TID,
    false,
  )
}

#[inline]
pub(crate) fn instant_read_cost() -> crate::ThreadCpuReadCost {
  instant_read_cost_for(selected_instant_provider())
}

#[inline]
pub(crate) fn instant_frequency() -> u64 {
  provider_frequency(selected_instant_provider())
}

#[inline]
pub(crate) fn ordered_frequency() -> u64 {
  provider_frequency(selected_ordered_provider())
}

#[inline]
fn provider_frequency(provider: u8) -> u64 {
  #[cfg(target_arch = "arm")]
  {
    provider_frequency_for(provider, CNTVCT_FREQUENCY.load(Ordering::Acquire))
  }
  #[cfg(target_arch = "s390x")]
  {
    let _ = provider;
    1_000_000_000
  }
}

#[cfg(target_arch = "arm")]
fn provider_frequency_for(provider: u8, cntvct_frequency: u32) -> u64 {
  if matches!(provider, PROVIDER_CNTVCT | PROVIDER_CNTVCT_ORDERED) {
    u64::from(if cntvct_frequency == 0 { 1 } else { cntvct_frequency })
  } else {
    1_000_000_000
  }
}

const fn instant_read_cost_for(provider: u8) -> crate::ThreadCpuReadCost {
  match provider {
    #[cfg(target_arch = "arm")]
    PROVIDER_CNTVCT | PROVIDER_CNTVCT_ORDERED => crate::ThreadCpuReadCost::Inline,
    PROVIDER_VDSO | PROVIDER_VDSO_MONOTONIC_RAW | PROVIDER_VDSO_BOOTTIME => {
      crate::ThreadCpuReadCost::Inline
    }
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64 | PROVIDER_VDSO_TIME64_MONOTONIC_RAW | PROVIDER_VDSO_TIME64_BOOTTIME => {
      crate::ThreadCpuReadCost::Inline
    }
    _ => crate::ThreadCpuReadCost::SystemCall,
  }
}

#[cfg(test)]
#[test]
fn instant_read_cost_conservatively_classifies_every_clock_abi_path() {
  assert_eq!(instant_read_cost_for(PROVIDER_LIBC), crate::ThreadCpuReadCost::SystemCall);
  assert_eq!(
    instant_read_cost_for(PROVIDER_LIBC_MONOTONIC_RAW),
    crate::ThreadCpuReadCost::SystemCall,
  );
  assert_eq!(instant_read_cost_for(PROVIDER_RAW), crate::ThreadCpuReadCost::SystemCall);
  assert_eq!(instant_read_cost_for(PROVIDER_VDSO_BOOTTIME), crate::ThreadCpuReadCost::Inline);
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
  let fallback = if ordered { PROVIDER_RAW_OS_ORDERED } else { PROVIDER_RAW };
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
  let _ = super::linux_vdso::install();
  let vdso_available = super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC).is_some();
  #[cfg(target_arch = "arm")]
  let cntvct_symbol_proven = super::linux_vdso::arm_cntvct_access_proven();
  #[cfg(target_arch = "arm")]
  let cntvct_frequency = if cntvct_symbol_proven { arm_cntfrq() } else { 0 };
  #[cfg(target_arch = "arm")]
  let cntvct_available =
    arm_cntvct_eligible(cntvct_symbol_proven, vdso_available, cntvct_frequency);
  #[cfg(target_arch = "arm")]
  if cntvct_available {
    CNTVCT_FREQUENCY.store(cntvct_frequency, Ordering::Release);
  }
  #[cfg(target_arch = "s390x")]
  let cntvct_available = false;
  let vdso_monotonic_raw_available =
    super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC_RAW).is_some();
  #[cfg(target_arch = "arm")]
  let vdso_time64_available =
    super::linux_vdso::clock_nanos_time64(libc::CLOCK_MONOTONIC).is_some();
  #[cfg(target_arch = "s390x")]
  let vdso_time64_available = false;
  #[cfg(target_arch = "arm")]
  let vdso_time64_monotonic_raw_available =
    super::linux_vdso::clock_nanos_time64(libc::CLOCK_MONOTONIC_RAW).is_some();
  #[cfg(target_arch = "s390x")]
  let vdso_time64_monotonic_raw_available = false;
  let vdso_boottime_available = super::linux_vdso::clock_nanos(libc::CLOCK_BOOTTIME).is_some();
  #[cfg(target_arch = "arm")]
  let vdso_time64_boottime_available =
    super::linux_vdso::clock_nanos_time64(libc::CLOCK_BOOTTIME).is_some();
  #[cfg(target_arch = "s390x")]
  let vdso_time64_boottime_available = false;
  let raw_available = raw_clock_nanos(libc::CLOCK_MONOTONIC).is_some();
  #[cfg(target_arch = "arm")]
  let raw_time64_available = raw_clock_nanos_time64(libc::CLOCK_MONOTONIC).is_some();
  #[cfg(target_arch = "s390x")]
  let raw_time64_available = false;
  let libc_monotonic_raw_available = libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW).is_some();
  let raw_monotonic_raw_available = raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW).is_some();
  #[cfg(target_arch = "arm")]
  let raw_time64_monotonic_raw_available =
    raw_clock_nanos_time64(libc::CLOCK_MONOTONIC_RAW).is_some();
  #[cfg(target_arch = "s390x")]
  let raw_time64_monotonic_raw_available = false;
  let libc_boottime_available = libc_clock_nanos(libc::CLOCK_BOOTTIME).is_some();
  let raw_boottime_available = raw_clock_nanos(libc::CLOCK_BOOTTIME).is_some();
  #[cfg(target_arch = "arm")]
  let raw_time64_boottime_available = raw_clock_nanos_time64(libc::CLOCK_BOOTTIME).is_some();
  #[cfg(target_arch = "s390x")]
  let raw_time64_boottime_available = false;
  let (samples, candidate_providers, candidate_count) = measure_candidates(
    ordered,
    cntvct_available,
    raw_available,
    raw_time64_available,
    libc_monotonic_raw_available,
    raw_monotonic_raw_available,
    raw_time64_monotonic_raw_available,
    vdso_available,
    vdso_monotonic_raw_available,
    vdso_time64_available,
    vdso_time64_monotonic_raw_available,
    libc_boottime_available,
    raw_boottime_available,
    raw_time64_boottime_available,
    vdso_boottime_available,
    vdso_time64_boottime_available,
  );
  #[cfg(not(feature = "bench-internal"))]
  let _ = (candidate_providers, candidate_count);

  let raw_decision =
    if raw_available { prefer_challenger(samples.raw, samples.libc) } else { empty_decision() };
  let mut provider = if raw_decision.challenger_selected { PROVIDER_RAW } else { PROVIDER_LIBC };
  let raw_os_ordered_decision = if ordered && raw_available {
    prefer_challenger(samples.raw_os_ordered, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  if raw_os_ordered_decision.challenger_selected {
    provider = PROVIDER_RAW_OS_ORDERED;
  }
  let raw_time64_decision = if raw_time64_available {
    prefer_challenger(samples.raw_time64, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  #[cfg(target_arch = "arm")]
  if raw_time64_decision.challenger_selected {
    provider = PROVIDER_RAW_TIME64;
  }
  let raw_time64_os_ordered_decision = if ordered && raw_time64_available {
    prefer_challenger(samples.raw_time64_os_ordered, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  #[cfg(target_arch = "arm")]
  if raw_time64_os_ordered_decision.challenger_selected {
    provider = PROVIDER_RAW_TIME64_OS_ORDERED;
  }
  let libc_monotonic_raw_decision = if libc_monotonic_raw_available {
    prefer_challenger(samples.libc_monotonic_raw, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  if libc_monotonic_raw_decision.challenger_selected {
    provider = PROVIDER_LIBC_MONOTONIC_RAW;
  }
  let raw_monotonic_raw_decision = if raw_monotonic_raw_available {
    prefer_challenger(samples.raw_monotonic_raw, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  if raw_monotonic_raw_decision.challenger_selected {
    provider = PROVIDER_RAW_MONOTONIC_RAW;
  }
  let raw_monotonic_raw_os_ordered_decision = if ordered && raw_monotonic_raw_available {
    prefer_challenger(samples.raw_monotonic_raw_os_ordered, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  if raw_monotonic_raw_os_ordered_decision.challenger_selected {
    provider = PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED;
  }
  let raw_time64_monotonic_raw_decision = if raw_time64_monotonic_raw_available {
    prefer_challenger(samples.raw_time64_monotonic_raw, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  #[cfg(target_arch = "arm")]
  if raw_time64_monotonic_raw_decision.challenger_selected {
    provider = PROVIDER_RAW_TIME64_MONOTONIC_RAW;
  }
  let raw_time64_monotonic_raw_os_ordered_decision =
    if ordered && raw_time64_monotonic_raw_available {
      prefer_challenger(
        samples.raw_time64_monotonic_raw_os_ordered,
        candidate_samples(samples, provider),
      )
    } else {
      empty_decision()
    };
  #[cfg(target_arch = "arm")]
  if raw_time64_monotonic_raw_os_ordered_decision.challenger_selected {
    provider = PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED;
  }
  let vdso_decision = if vdso_available {
    prefer_challenger(samples.vdso, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  if vdso_decision.challenger_selected {
    provider = PROVIDER_VDSO;
  }
  let vdso_monotonic_raw_decision = if vdso_monotonic_raw_available {
    prefer_challenger(samples.vdso_monotonic_raw, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  if vdso_monotonic_raw_decision.challenger_selected {
    provider = PROVIDER_VDSO_MONOTONIC_RAW;
  }
  let vdso_time64_decision = if vdso_time64_available {
    prefer_challenger(samples.vdso_time64, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  #[cfg(target_arch = "arm")]
  if vdso_time64_decision.challenger_selected {
    provider = PROVIDER_VDSO_TIME64;
  }
  let vdso_time64_monotonic_raw_decision = if vdso_time64_monotonic_raw_available {
    prefer_challenger(samples.vdso_time64_monotonic_raw, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  #[cfg(target_arch = "arm")]
  if vdso_time64_monotonic_raw_decision.challenger_selected {
    provider = PROVIDER_VDSO_TIME64_MONOTONIC_RAW;
  }
  let libc_boottime_decision = if libc_boottime_available {
    prefer_challenger(samples.libc_boottime, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  if libc_boottime_decision.challenger_selected {
    provider = PROVIDER_LIBC_BOOTTIME;
  }
  let raw_boottime_decision = if raw_boottime_available {
    prefer_challenger(samples.raw_boottime, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  if raw_boottime_decision.challenger_selected {
    provider = PROVIDER_RAW_BOOTTIME;
  }
  let raw_boottime_os_ordered_decision = if ordered && raw_boottime_available {
    prefer_challenger(samples.raw_boottime_os_ordered, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  if raw_boottime_os_ordered_decision.challenger_selected {
    provider = PROVIDER_RAW_BOOTTIME_OS_ORDERED;
  }
  let raw_time64_boottime_decision = if raw_time64_boottime_available {
    prefer_challenger(samples.raw_time64_boottime, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  #[cfg(target_arch = "arm")]
  if raw_time64_boottime_decision.challenger_selected {
    provider = PROVIDER_RAW_TIME64_BOOTTIME;
  }
  let raw_time64_boottime_os_ordered_decision = if ordered && raw_time64_boottime_available {
    prefer_challenger(samples.raw_time64_boottime_os_ordered, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  #[cfg(target_arch = "arm")]
  if raw_time64_boottime_os_ordered_decision.challenger_selected {
    provider = PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED;
  }
  let vdso_boottime_decision = if vdso_boottime_available {
    prefer_challenger(samples.vdso_boottime, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  if vdso_boottime_decision.challenger_selected {
    provider = PROVIDER_VDSO_BOOTTIME;
  }
  let vdso_time64_boottime_decision = if vdso_time64_boottime_available {
    prefer_challenger(samples.vdso_time64_boottime, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  #[cfg(target_arch = "arm")]
  if vdso_time64_boottime_decision.challenger_selected {
    provider = PROVIDER_VDSO_TIME64_BOOTTIME;
  }
  let cntvct_decision = if cntvct_available {
    prefer_challenger(samples.cntvct, candidate_samples(samples, provider))
  } else {
    empty_decision()
  };
  #[cfg(target_arch = "arm")]
  if cntvct_decision.challenger_selected {
    provider = if ordered { PROVIDER_CNTVCT_ORDERED } else { PROVIDER_CNTVCT };
  }
  #[cfg(target_arch = "s390x")]
  let _ = (vdso_time64_decision, vdso_time64_monotonic_raw_decision, vdso_time64_boottime_decision);
  #[cfg(target_arch = "s390x")]
  let _ = cntvct_decision;
  #[cfg(all(target_arch = "s390x", not(feature = "bench-internal")))]
  let _ = (
    raw_time64_decision,
    raw_time64_os_ordered_decision,
    raw_time64_monotonic_raw_decision,
    raw_time64_monotonic_raw_os_ordered_decision,
    raw_time64_boottime_decision,
    raw_time64_boottime_os_ordered_decision,
  );

  #[cfg(feature = "bench-internal")]
  publish_evidence(ordered, ProbeEvidence {
    candidate_count,
    candidate_providers,
    reads_per_batch: PROBE_READS,
    arm_cntvct_available: cntvct_available,
    arm_cntvct_eligibility_basis: if cntvct_available {
      "versioned arm32 direct-vDSO clock symbol survived kernel CNTVCT functional patching"
    } else {
      "kernel direct-vDSO symbol proof or nonzero CNTFRQ unavailable"
    },
    s390_bare_stckf_eligible: false,
    s390_bare_stckf_exclusion: "bare STCKF omits Linux tod_clock_base, preemption, vDSO seqlock, delta, and scaling",
    raw_available,
    raw_time64_available,
    libc_monotonic_raw_available,
    raw_monotonic_raw_available,
    raw_time64_monotonic_raw_available,
    vdso_available,
    vdso_monotonic_raw_available,
    vdso_time64_available,
    vdso_time64_monotonic_raw_available,
    libc_boottime_available,
    raw_boottime_available,
    raw_time64_boottime_available,
    vdso_boottime_available,
    vdso_time64_boottime_available,
    cntvct_batches_ns: samples.cntvct,
    libc_batches_ns: samples.libc,
    raw_batches_ns: samples.raw,
    raw_os_ordered_batches_ns: samples.raw_os_ordered,
    raw_time64_batches_ns: samples.raw_time64,
    raw_time64_os_ordered_batches_ns: samples.raw_time64_os_ordered,
    libc_monotonic_raw_batches_ns: samples.libc_monotonic_raw,
    raw_monotonic_raw_batches_ns: samples.raw_monotonic_raw,
    raw_monotonic_raw_os_ordered_batches_ns: samples.raw_monotonic_raw_os_ordered,
    raw_time64_monotonic_raw_batches_ns: samples.raw_time64_monotonic_raw,
    raw_time64_monotonic_raw_os_ordered_batches_ns: samples.raw_time64_monotonic_raw_os_ordered,
    vdso_batches_ns: samples.vdso,
    vdso_monotonic_raw_batches_ns: samples.vdso_monotonic_raw,
    vdso_time64_batches_ns: samples.vdso_time64,
    vdso_time64_monotonic_raw_batches_ns: samples.vdso_time64_monotonic_raw,
    libc_boottime_batches_ns: samples.libc_boottime,
    raw_boottime_batches_ns: samples.raw_boottime,
    raw_boottime_os_ordered_batches_ns: samples.raw_boottime_os_ordered,
    raw_time64_boottime_batches_ns: samples.raw_time64_boottime,
    raw_time64_boottime_os_ordered_batches_ns: samples.raw_time64_boottime_os_ordered,
    vdso_boottime_batches_ns: samples.vdso_boottime,
    vdso_time64_boottime_batches_ns: samples.vdso_time64_boottime,
    cntvct_median_ns: median(samples.cntvct),
    libc_median_ns: median(samples.libc),
    raw_median_ns: median(samples.raw),
    raw_os_ordered_median_ns: median(samples.raw_os_ordered),
    raw_time64_median_ns: median(samples.raw_time64),
    raw_time64_os_ordered_median_ns: median(samples.raw_time64_os_ordered),
    libc_monotonic_raw_median_ns: median(samples.libc_monotonic_raw),
    raw_monotonic_raw_median_ns: median(samples.raw_monotonic_raw),
    raw_monotonic_raw_os_ordered_median_ns: median(samples.raw_monotonic_raw_os_ordered),
    raw_time64_monotonic_raw_median_ns: median(samples.raw_time64_monotonic_raw),
    raw_time64_monotonic_raw_os_ordered_median_ns: median(
      samples.raw_time64_monotonic_raw_os_ordered,
    ),
    vdso_median_ns: median(samples.vdso),
    vdso_monotonic_raw_median_ns: median(samples.vdso_monotonic_raw),
    vdso_time64_median_ns: median(samples.vdso_time64),
    vdso_time64_monotonic_raw_median_ns: median(samples.vdso_time64_monotonic_raw),
    libc_boottime_median_ns: median(samples.libc_boottime),
    raw_boottime_median_ns: median(samples.raw_boottime),
    raw_boottime_os_ordered_median_ns: median(samples.raw_boottime_os_ordered),
    raw_time64_boottime_median_ns: median(samples.raw_time64_boottime),
    raw_time64_boottime_os_ordered_median_ns: median(samples.raw_time64_boottime_os_ordered),
    vdso_boottime_median_ns: median(samples.vdso_boottime),
    vdso_time64_boottime_median_ns: median(samples.vdso_time64_boottime),
    cntvct_allowance_ns: cntvct_decision.allowance,
    cntvct_decisive_wins: cntvct_decision.decisive_wins,
    raw_allowance_ns: raw_decision.allowance,
    raw_decisive_wins: raw_decision.decisive_wins,
    raw_os_ordered_allowance_ns: raw_os_ordered_decision.allowance,
    raw_os_ordered_decisive_wins: raw_os_ordered_decision.decisive_wins,
    raw_time64_allowance_ns: raw_time64_decision.allowance,
    raw_time64_decisive_wins: raw_time64_decision.decisive_wins,
    raw_time64_os_ordered_allowance_ns: raw_time64_os_ordered_decision.allowance,
    raw_time64_os_ordered_decisive_wins: raw_time64_os_ordered_decision.decisive_wins,
    libc_monotonic_raw_allowance_ns: libc_monotonic_raw_decision.allowance,
    libc_monotonic_raw_decisive_wins: libc_monotonic_raw_decision.decisive_wins,
    raw_monotonic_raw_allowance_ns: raw_monotonic_raw_decision.allowance,
    raw_monotonic_raw_decisive_wins: raw_monotonic_raw_decision.decisive_wins,
    raw_monotonic_raw_os_ordered_allowance_ns: raw_monotonic_raw_os_ordered_decision.allowance,
    raw_monotonic_raw_os_ordered_decisive_wins: raw_monotonic_raw_os_ordered_decision.decisive_wins,
    raw_time64_monotonic_raw_allowance_ns: raw_time64_monotonic_raw_decision.allowance,
    raw_time64_monotonic_raw_decisive_wins: raw_time64_monotonic_raw_decision.decisive_wins,
    raw_time64_monotonic_raw_os_ordered_allowance_ns: raw_time64_monotonic_raw_os_ordered_decision
      .allowance,
    raw_time64_monotonic_raw_os_ordered_decisive_wins: raw_time64_monotonic_raw_os_ordered_decision
      .decisive_wins,
    vdso_allowance_ns: vdso_decision.allowance,
    vdso_decisive_wins: vdso_decision.decisive_wins,
    vdso_monotonic_raw_allowance_ns: vdso_monotonic_raw_decision.allowance,
    vdso_monotonic_raw_decisive_wins: vdso_monotonic_raw_decision.decisive_wins,
    vdso_time64_allowance_ns: vdso_time64_decision.allowance,
    vdso_time64_decisive_wins: vdso_time64_decision.decisive_wins,
    vdso_time64_monotonic_raw_allowance_ns: vdso_time64_monotonic_raw_decision.allowance,
    vdso_time64_monotonic_raw_decisive_wins: vdso_time64_monotonic_raw_decision.decisive_wins,
    libc_boottime_allowance_ns: libc_boottime_decision.allowance,
    libc_boottime_decisive_wins: libc_boottime_decision.decisive_wins,
    raw_boottime_allowance_ns: raw_boottime_decision.allowance,
    raw_boottime_decisive_wins: raw_boottime_decision.decisive_wins,
    raw_boottime_os_ordered_allowance_ns: raw_boottime_os_ordered_decision.allowance,
    raw_boottime_os_ordered_decisive_wins: raw_boottime_os_ordered_decision.decisive_wins,
    raw_time64_boottime_allowance_ns: raw_time64_boottime_decision.allowance,
    raw_time64_boottime_decisive_wins: raw_time64_boottime_decision.decisive_wins,
    raw_time64_boottime_os_ordered_allowance_ns: raw_time64_boottime_os_ordered_decision.allowance,
    raw_time64_boottime_os_ordered_decisive_wins: raw_time64_boottime_os_ordered_decision
      .decisive_wins,
    vdso_boottime_allowance_ns: vdso_boottime_decision.allowance,
    vdso_boottime_decisive_wins: vdso_boottime_decision.decisive_wins,
    vdso_time64_boottime_allowance_ns: vdso_time64_boottime_decision.allowance,
    vdso_time64_boottime_decisive_wins: vdso_time64_boottime_decision.decisive_wins,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    selected_provider: provider_from_raw(provider),
  });

  provider
}

#[allow(clippy::too_many_arguments)]
fn measure_candidates(
  ordered: bool,
  cntvct_available: bool,
  raw_available: bool,
  raw_time64_available: bool,
  libc_monotonic_raw_available: bool,
  raw_monotonic_raw_available: bool,
  raw_time64_monotonic_raw_available: bool,
  vdso_available: bool,
  vdso_monotonic_raw_available: bool,
  vdso_time64_available: bool,
  vdso_time64_monotonic_raw_available: bool,
  libc_boottime_available: bool,
  raw_boottime_available: bool,
  raw_time64_boottime_available: bool,
  vdso_boottime_available: bool,
  vdso_time64_boottime_available: bool,
) -> (ProbeSamples, [u8; MAX_CANDIDATES], usize) {
  #[cfg(target_arch = "s390x")]
  let _ = (raw_time64_available, raw_time64_monotonic_raw_available);
  #[cfg(target_arch = "s390x")]
  let _ = (vdso_time64_available, vdso_time64_monotonic_raw_available);
  #[cfg(target_arch = "s390x")]
  let _ = (raw_time64_boottime_available, vdso_time64_boottime_available);
  #[cfg(target_arch = "s390x")]
  let _ = cntvct_available;
  warm_candidate(ordered, PROVIDER_LIBC);
  #[cfg(target_arch = "arm")]
  if cntvct_available {
    warm_candidate(ordered, if ordered { PROVIDER_CNTVCT_ORDERED } else { PROVIDER_CNTVCT });
  }
  if raw_available {
    warm_candidate(ordered, PROVIDER_RAW);
    if ordered {
      warm_candidate(ordered, PROVIDER_RAW_OS_ORDERED);
    }
  }
  #[cfg(target_arch = "arm")]
  if raw_time64_available {
    warm_candidate(ordered, PROVIDER_RAW_TIME64);
    if ordered {
      warm_candidate(ordered, PROVIDER_RAW_TIME64_OS_ORDERED);
    }
  }
  if libc_monotonic_raw_available {
    warm_candidate(ordered, PROVIDER_LIBC_MONOTONIC_RAW);
  }
  if raw_monotonic_raw_available {
    warm_candidate(ordered, PROVIDER_RAW_MONOTONIC_RAW);
    if ordered {
      warm_candidate(ordered, PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED);
    }
  }
  #[cfg(target_arch = "arm")]
  if raw_time64_monotonic_raw_available {
    warm_candidate(ordered, PROVIDER_RAW_TIME64_MONOTONIC_RAW);
    if ordered {
      warm_candidate(ordered, PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED);
    }
  }
  if vdso_available {
    warm_candidate(ordered, PROVIDER_VDSO);
  }
  if vdso_monotonic_raw_available {
    warm_candidate(ordered, PROVIDER_VDSO_MONOTONIC_RAW);
  }
  #[cfg(target_arch = "arm")]
  if vdso_time64_available {
    warm_candidate(ordered, PROVIDER_VDSO_TIME64);
  }
  #[cfg(target_arch = "arm")]
  if vdso_time64_monotonic_raw_available {
    warm_candidate(ordered, PROVIDER_VDSO_TIME64_MONOTONIC_RAW);
  }

  if libc_boottime_available {
    warm_candidate(ordered, PROVIDER_LIBC_BOOTTIME);
  }
  if raw_boottime_available {
    warm_candidate(ordered, PROVIDER_RAW_BOOTTIME);
    if ordered {
      warm_candidate(ordered, PROVIDER_RAW_BOOTTIME_OS_ORDERED);
    }
  }
  #[cfg(target_arch = "arm")]
  if raw_time64_boottime_available {
    warm_candidate(ordered, PROVIDER_RAW_TIME64_BOOTTIME);
    if ordered {
      warm_candidate(ordered, PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED);
    }
  }
  if vdso_boottime_available {
    warm_candidate(ordered, PROVIDER_VDSO_BOOTTIME);
  }
  #[cfg(target_arch = "arm")]
  if vdso_time64_boottime_available {
    warm_candidate(ordered, PROVIDER_VDSO_TIME64_BOOTTIME);
  }

  let mut candidates = [PROVIDER_UNKNOWN; MAX_CANDIDATES];
  let mut candidate_count = 0;
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_LIBC, true);
  #[cfg(target_arch = "arm")]
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    if ordered { PROVIDER_CNTVCT_ORDERED } else { PROVIDER_CNTVCT },
    cntvct_available,
  );
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_RAW, raw_available);
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_RAW_OS_ORDERED,
    ordered && raw_available,
  );
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_VDSO, vdso_available);
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_VDSO_MONOTONIC_RAW,
    vdso_monotonic_raw_available,
  );
  #[cfg(target_arch = "arm")]
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_VDSO_TIME64,
    vdso_time64_available,
  );
  #[cfg(target_arch = "arm")]
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW,
    vdso_time64_monotonic_raw_available,
  );
  #[cfg(target_arch = "arm")]
  push_candidate(&mut candidates, &mut candidate_count, PROVIDER_RAW_TIME64, raw_time64_available);
  #[cfg(target_arch = "arm")]
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_RAW_TIME64_OS_ORDERED,
    ordered && raw_time64_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_LIBC_MONOTONIC_RAW,
    libc_monotonic_raw_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_RAW_MONOTONIC_RAW,
    raw_monotonic_raw_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED,
    ordered && raw_monotonic_raw_available,
  );
  #[cfg(target_arch = "arm")]
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_RAW_TIME64_MONOTONIC_RAW,
    raw_time64_monotonic_raw_available,
  );
  #[cfg(target_arch = "arm")]
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED,
    ordered && raw_time64_monotonic_raw_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_LIBC_BOOTTIME,
    libc_boottime_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_RAW_BOOTTIME,
    raw_boottime_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_RAW_BOOTTIME_OS_ORDERED,
    ordered && raw_boottime_available,
  );
  #[cfg(target_arch = "arm")]
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_RAW_TIME64_BOOTTIME,
    raw_time64_boottime_available,
  );
  #[cfg(target_arch = "arm")]
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED,
    ordered && raw_time64_boottime_available,
  );
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_VDSO_BOOTTIME,
    vdso_boottime_available,
  );
  #[cfg(target_arch = "arm")]
  push_candidate(
    &mut candidates,
    &mut candidate_count,
    PROVIDER_VDSO_TIME64_BOOTTIME,
    vdso_time64_boottime_available,
  );

  let mut samples = ProbeSamples {
    cntvct: [u64::MAX; PROBE_BATCHES],
    libc: [u64::MAX; PROBE_BATCHES],
    raw: [u64::MAX; PROBE_BATCHES],
    raw_os_ordered: [u64::MAX; PROBE_BATCHES],
    raw_time64: [u64::MAX; PROBE_BATCHES],
    raw_time64_os_ordered: [u64::MAX; PROBE_BATCHES],
    libc_monotonic_raw: [u64::MAX; PROBE_BATCHES],
    raw_monotonic_raw: [u64::MAX; PROBE_BATCHES],
    raw_monotonic_raw_os_ordered: [u64::MAX; PROBE_BATCHES],
    raw_time64_monotonic_raw: [u64::MAX; PROBE_BATCHES],
    raw_time64_monotonic_raw_os_ordered: [u64::MAX; PROBE_BATCHES],
    vdso: [u64::MAX; PROBE_BATCHES],
    vdso_monotonic_raw: [u64::MAX; PROBE_BATCHES],
    vdso_time64: [u64::MAX; PROBE_BATCHES],
    vdso_time64_monotonic_raw: [u64::MAX; PROBE_BATCHES],
    libc_boottime: [u64::MAX; PROBE_BATCHES],
    raw_boottime: [u64::MAX; PROBE_BATCHES],
    raw_boottime_os_ordered: [u64::MAX; PROBE_BATCHES],
    raw_time64_boottime: [u64::MAX; PROBE_BATCHES],
    raw_time64_boottime_os_ordered: [u64::MAX; PROBE_BATCHES],
    vdso_boottime: [u64::MAX; PROBE_BATCHES],
    vdso_time64_boottime: [u64::MAX; PROBE_BATCHES],
  };
  for sample in 0..PROBE_BATCHES {
    for offset in 0..candidate_count {
      let provider = candidates[(sample + offset) % candidate_count];
      if !ordered && is_os_ordered_provider(provider) {
        continue;
      }
      let elapsed = measure_batch(ordered, provider).unwrap_or(u64::MAX);
      match provider {
        #[cfg(target_arch = "arm")]
        PROVIDER_CNTVCT | PROVIDER_CNTVCT_ORDERED => samples.cntvct[sample] = elapsed,
        PROVIDER_LIBC => samples.libc[sample] = elapsed,
        PROVIDER_RAW => samples.raw[sample] = elapsed,
        PROVIDER_RAW_OS_ORDERED => samples.raw_os_ordered[sample] = elapsed,
        #[cfg(target_arch = "arm")]
        PROVIDER_RAW_TIME64 => samples.raw_time64[sample] = elapsed,
        #[cfg(target_arch = "arm")]
        PROVIDER_RAW_TIME64_OS_ORDERED => samples.raw_time64_os_ordered[sample] = elapsed,
        PROVIDER_LIBC_MONOTONIC_RAW => samples.libc_monotonic_raw[sample] = elapsed,
        PROVIDER_RAW_MONOTONIC_RAW => samples.raw_monotonic_raw[sample] = elapsed,
        PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED => {
          samples.raw_monotonic_raw_os_ordered[sample] = elapsed;
        }
        #[cfg(target_arch = "arm")]
        PROVIDER_RAW_TIME64_MONOTONIC_RAW => {
          samples.raw_time64_monotonic_raw[sample] = elapsed;
        }
        #[cfg(target_arch = "arm")]
        PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED => {
          samples.raw_time64_monotonic_raw_os_ordered[sample] = elapsed;
        }
        PROVIDER_VDSO => samples.vdso[sample] = elapsed,
        PROVIDER_VDSO_MONOTONIC_RAW => samples.vdso_monotonic_raw[sample] = elapsed,
        #[cfg(target_arch = "arm")]
        PROVIDER_VDSO_TIME64 => samples.vdso_time64[sample] = elapsed,
        #[cfg(target_arch = "arm")]
        PROVIDER_VDSO_TIME64_MONOTONIC_RAW => {
          samples.vdso_time64_monotonic_raw[sample] = elapsed;
        }
        PROVIDER_LIBC_BOOTTIME => samples.libc_boottime[sample] = elapsed,
        PROVIDER_RAW_BOOTTIME => samples.raw_boottime[sample] = elapsed,
        PROVIDER_RAW_BOOTTIME_OS_ORDERED => samples.raw_boottime_os_ordered[sample] = elapsed,
        #[cfg(target_arch = "arm")]
        PROVIDER_RAW_TIME64_BOOTTIME => samples.raw_time64_boottime[sample] = elapsed,
        #[cfg(target_arch = "arm")]
        PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED => {
          samples.raw_time64_boottime_os_ordered[sample] = elapsed;
        }
        PROVIDER_VDSO_BOOTTIME => samples.vdso_boottime[sample] = elapsed,
        #[cfg(target_arch = "arm")]
        PROVIDER_VDSO_TIME64_BOOTTIME => samples.vdso_time64_boottime[sample] = elapsed,
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
    #[cfg(target_arch = "arm")]
    PROVIDER_CNTVCT | PROVIDER_CNTVCT_ORDERED => samples.cntvct,
    PROVIDER_RAW => samples.raw,
    PROVIDER_RAW_OS_ORDERED => samples.raw_os_ordered,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64 => samples.raw_time64,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_OS_ORDERED => samples.raw_time64_os_ordered,
    PROVIDER_LIBC_MONOTONIC_RAW => samples.libc_monotonic_raw,
    PROVIDER_RAW_MONOTONIC_RAW => samples.raw_monotonic_raw,
    PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED => samples.raw_monotonic_raw_os_ordered,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW => samples.raw_time64_monotonic_raw,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED => samples.raw_time64_monotonic_raw_os_ordered,
    PROVIDER_VDSO => samples.vdso,
    PROVIDER_VDSO_MONOTONIC_RAW => samples.vdso_monotonic_raw,
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64 => samples.vdso_time64,
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW => samples.vdso_time64_monotonic_raw,
    PROVIDER_LIBC_BOOTTIME => samples.libc_boottime,
    PROVIDER_RAW_BOOTTIME => samples.raw_boottime,
    PROVIDER_RAW_BOOTTIME_OS_ORDERED => samples.raw_boottime_os_ordered,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME => samples.raw_time64_boottime,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED => samples.raw_time64_boottime_os_ordered,
    PROVIDER_VDSO_BOOTTIME => samples.vdso_boottime,
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_BOOTTIME => samples.vdso_time64_boottime,
    _ => samples.libc,
  }
}

#[inline(never)]
fn measure_batch(ordered: bool, provider: u8) -> Option<u64> {
  if ordered {
    PROBE_ORDERED_PROVIDER.store(provider, Ordering::Relaxed);
  } else {
    PROBE_INSTANT_PROVIDER.store(provider, Ordering::Relaxed);
  }
  let start = monotonic_raw_nanos()?;
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
  let elapsed = monotonic_raw_nanos()?.checked_sub(start);
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
  read_ordered_provider(PROBE_ORDERED_PROVIDER.load(Ordering::Relaxed))
}

#[inline(always)]
fn read_instant_provider(provider: u8) -> u64 {
  match provider {
    #[cfg(target_arch = "arm")]
    PROVIDER_CNTVCT | PROVIDER_CNTVCT_ORDERED => arm_cntvct(),
    PROVIDER_LIBC => libc_clock_monotonic(),
    PROVIDER_RAW | PROVIDER_RAW_OS_ORDERED => raw_clock_monotonic(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64 => raw_clock_monotonic_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_OS_ORDERED => raw_clock_monotonic_time64(),
    PROVIDER_LIBC_MONOTONIC_RAW => libc_clock_monotonic_raw(),
    PROVIDER_RAW_MONOTONIC_RAW | PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED => raw_clock_monotonic_raw(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW => raw_clock_monotonic_raw_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED => raw_clock_monotonic_raw_time64(),
    PROVIDER_VDSO => vdso_clock_monotonic(),
    PROVIDER_VDSO_MONOTONIC_RAW => vdso_clock_monotonic_raw(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64 => vdso_clock_monotonic_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW => vdso_clock_monotonic_raw_time64(),
    PROVIDER_LIBC_BOOTTIME => libc_clock_boottime(),
    PROVIDER_RAW_BOOTTIME | PROVIDER_RAW_BOOTTIME_OS_ORDERED => raw_clock_boottime(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME | PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED => {
      raw_clock_boottime_time64()
    }
    PROVIDER_VDSO_BOOTTIME => vdso_clock_boottime(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_BOOTTIME => vdso_clock_boottime_time64(),
    _ => libc_clock_monotonic(),
  }
}

#[inline(always)]
fn read_ordered_provider(provider: u8) -> u64 {
  match provider {
    #[cfg(target_arch = "arm")]
    PROVIDER_CNTVCT | PROVIDER_CNTVCT_ORDERED => arm_cntvct_ordered(),
    PROVIDER_LIBC => ordered_clock_monotonic(),
    PROVIDER_RAW => ordered_raw_clock_monotonic(),
    PROVIDER_RAW_OS_ORDERED => os_ordered_raw_clock_monotonic(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64 => ordered_raw_clock_monotonic_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_OS_ORDERED => os_ordered_raw_clock_monotonic_time64(),
    PROVIDER_LIBC_MONOTONIC_RAW => ordered_libc_clock_monotonic_raw(),
    PROVIDER_RAW_MONOTONIC_RAW => ordered_raw_clock_monotonic_raw(),
    PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED => os_ordered_raw_clock_monotonic_raw(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW => ordered_raw_clock_monotonic_raw_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED => os_ordered_raw_clock_monotonic_raw_time64(),
    PROVIDER_VDSO => ordered_vdso_clock_monotonic(),
    PROVIDER_VDSO_MONOTONIC_RAW => ordered_vdso_clock_monotonic_raw(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64 => ordered_vdso_clock_monotonic_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW => ordered_vdso_clock_monotonic_raw_time64(),
    PROVIDER_LIBC_BOOTTIME => ordered_libc_clock_boottime(),
    PROVIDER_RAW_BOOTTIME => ordered_raw_clock_boottime(),
    PROVIDER_RAW_BOOTTIME_OS_ORDERED => os_ordered_raw_clock_boottime(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME => ordered_raw_clock_boottime_time64(),
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED => os_ordered_raw_clock_boottime_time64(),
    PROVIDER_VDSO_BOOTTIME => ordered_vdso_clock_boottime(),
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_BOOTTIME => ordered_vdso_clock_boottime_time64(),
    _ => ordered_clock_monotonic(),
  }
}

#[cfg(target_arch = "arm")]
#[inline]
const fn arm_cntvct_eligible(
  direct_vdso_symbol: bool,
  direct_vdso_clock_worked: bool,
  frequency: u32,
) -> bool {
  direct_vdso_symbol && direct_vdso_clock_worked && frequency != 0
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn arm_cntfrq() -> u32 {
  let frequency: u32;
  // SAFETY: this read executes only after the kernel's versioned direct-vDSO
  // clock symbol proves a functional, firmware-configured Generic Timer.
  unsafe {
    core::arch::asm!(
      "mrc p15, 0, {frequency}, c14, c0, 0",
      frequency = out(reg) frequency,
      options(nostack, nomem, preserves_flags),
    );
  }
  frequency
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn arm_cntvct() -> u64 {
  let low: u32;
  let high: u32;
  // SAFETY: the selector admits this instruction only when Linux leaves its
  // direct-vDSO clock symbol installed, which is the kernel's non-faulting
  // proof that CNTVCT is present and firmware configured for user access.
  unsafe {
    core::arch::asm!(
      "isb sy",
      "mrrc p15, 1, {low}, {high}, c14",
      low = out(reg) low,
      high = out(reg) high,
      options(nostack, nomem, preserves_flags),
    );
  }
  (u64::from(high) << 32) | u64::from(low)
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn arm_cntvct_ordered() -> u64 {
  let low: u32;
  let high: u32;
  // SAFETY: DMB completes the prior Acquire-observed memory access and ISB
  // orders the Generic Timer read after that completion. Eligibility is the
  // same kernel symbol proof used by `arm_cntvct`.
  unsafe {
    core::arch::asm!(
      "dmb ish",
      "isb sy",
      "mrrc p15, 1, {low}, {high}, c14",
      low = out(reg) low,
      high = out(reg) high,
      options(nostack, preserves_flags),
    );
  }
  (u64::from(high) << 32) | u64::from(low)
}

#[inline(always)]
fn ordered_clock_monotonic() -> u64 {
  ordered_clock_barrier();
  libc_clock_monotonic()
}

#[inline(always)]
fn ordered_raw_clock_monotonic() -> u64 {
  ordered_clock_barrier();
  raw_clock_monotonic()
}

#[inline(always)]
fn ordered_libc_clock_monotonic_raw() -> u64 {
  ordered_clock_barrier();
  libc_clock_monotonic_raw()
}

#[inline(always)]
fn ordered_raw_clock_monotonic_raw() -> u64 {
  ordered_clock_barrier();
  raw_clock_monotonic_raw()
}

#[inline(always)]
fn ordered_libc_clock_boottime() -> u64 {
  ordered_clock_barrier();
  libc_clock_boottime()
}

#[inline(always)]
fn ordered_raw_clock_boottime() -> u64 {
  ordered_clock_barrier();
  raw_clock_boottime()
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn ordered_raw_clock_monotonic_time64() -> u64 {
  ordered_clock_barrier();
  raw_clock_monotonic_time64()
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn ordered_raw_clock_monotonic_raw_time64() -> u64 {
  ordered_clock_barrier();
  raw_clock_monotonic_raw_time64()
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn ordered_raw_clock_boottime_time64() -> u64 {
  ordered_clock_barrier();
  raw_clock_boottime_time64()
}

#[inline(always)]
fn os_ordered_raw_clock_monotonic() -> u64 {
  // Arm SVC performs a context-synchronization event; s390 SVC performs a
  // serialization function. The syscall asm's memory clobber supplies the
  // matching compiler-ordering edge before Linux reads the clock.
  raw_clock_nanos(libc::CLOCK_MONOTONIC).unwrap_or_else(ordered_clock_monotonic)
}

#[inline(always)]
fn os_ordered_raw_clock_monotonic_raw() -> u64 {
  raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW).unwrap_or_else(ordered_libc_clock_monotonic_raw)
}

#[inline(always)]
fn os_ordered_raw_clock_boottime() -> u64 {
  raw_clock_nanos(libc::CLOCK_BOOTTIME).unwrap_or_else(ordered_libc_clock_boottime)
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn os_ordered_raw_clock_monotonic_time64() -> u64 {
  raw_clock_nanos_time64(libc::CLOCK_MONOTONIC).unwrap_or_else(ordered_clock_monotonic)
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn os_ordered_raw_clock_monotonic_raw_time64() -> u64 {
  raw_clock_nanos_time64(libc::CLOCK_MONOTONIC_RAW).unwrap_or_else(ordered_libc_clock_monotonic_raw)
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn os_ordered_raw_clock_boottime_time64() -> u64 {
  raw_clock_nanos_time64(libc::CLOCK_BOOTTIME).unwrap_or_else(ordered_libc_clock_boottime)
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn ordered_clock_barrier() {
  // Arm's Generic Timer ordering sequence uses an ISB between the observed
  // load and counter read. The DMB retains Rust's memory-ordering edge for the
  // vDSO's data-page loads. Omitting `nomem` supplies the compiler barrier.
  // SAFETY: this ordering-only sequence has no register inputs or outputs.
  unsafe { core::arch::asm!("dmb ish", "isb", options(nostack, preserves_flags)) }
}

#[cfg(target_arch = "s390x")]
#[inline(always)]
fn ordered_clock_barrier() {
  // Linux defines BCR 15,0 as s390's full memory barrier. Omitting `nomem`
  // prevents the compiler from moving the prior observation across it.
  // SAFETY: this ordering-only sequence has no register inputs or outputs.
  unsafe { core::arch::asm!("bcr 15, 0", options(nostack, preserves_flags)) }
}

#[inline(always)]
fn raw_clock_monotonic() -> u64 {
  raw_clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC))
    .or_else(|| {
      #[cfg(target_arch = "arm")]
      {
        raw_clock_nanos_time64(libc::CLOCK_MONOTONIC)
      }
      #[cfg(target_arch = "s390x")]
      {
        None
      }
    })
    .unwrap_or(0)
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn raw_clock_monotonic_time64() -> u64 {
  raw_clock_nanos_time64(libc::CLOCK_MONOTONIC)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC))
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn libc_clock_monotonic() -> u64 {
  libc_clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| {
      #[cfg(target_arch = "arm")]
      {
        raw_clock_nanos_time64(libc::CLOCK_MONOTONIC)
      }
      #[cfg(target_arch = "s390x")]
      {
        None
      }
    })
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn libc_clock_monotonic_raw() -> u64 {
  libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn raw_clock_monotonic_raw() -> u64 {
  raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn raw_clock_monotonic_raw_time64() -> u64 {
  raw_clock_nanos_time64(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn libc_clock_boottime() -> u64 {
  libc_clock_nanos(libc::CLOCK_BOOTTIME)
    .or_else(|| raw_clock_nanos(libc::CLOCK_BOOTTIME))
    .or_else(|| {
      #[cfg(target_arch = "arm")]
      {
        raw_clock_nanos_time64(libc::CLOCK_BOOTTIME)
      }
      #[cfg(target_arch = "s390x")]
      {
        None
      }
    })
    .unwrap_or(0)
}

#[inline(always)]
fn raw_clock_boottime() -> u64 {
  raw_clock_nanos(libc::CLOCK_BOOTTIME)
    .or_else(|| libc_clock_nanos(libc::CLOCK_BOOTTIME))
    .or_else(|| {
      #[cfg(target_arch = "arm")]
      {
        raw_clock_nanos_time64(libc::CLOCK_BOOTTIME)
      }
      #[cfg(target_arch = "s390x")]
      {
        None
      }
    })
    .unwrap_or(0)
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn raw_clock_boottime_time64() -> u64 {
  raw_clock_nanos_time64(libc::CLOCK_BOOTTIME)
    .or_else(|| libc_clock_nanos(libc::CLOCK_BOOTTIME))
    .or_else(|| raw_clock_nanos(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[inline(always)]
fn vdso_clock_monotonic() -> u64 {
  super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC))
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn vdso_clock_monotonic_raw() -> u64 {
  super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn vdso_clock_monotonic_time64() -> u64 {
  super::linux_vdso::clock_nanos_time64(libc::CLOCK_MONOTONIC)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC))
    .or_else(|| raw_clock_nanos_time64(libc::CLOCK_MONOTONIC))
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn vdso_clock_monotonic_raw_time64() -> u64 {
  super::linux_vdso::clock_nanos_time64(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .or_else(|| raw_clock_nanos_time64(libc::CLOCK_MONOTONIC_RAW))
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn vdso_clock_boottime() -> u64 {
  super::linux_vdso::clock_nanos(libc::CLOCK_BOOTTIME)
    .or_else(|| libc_clock_nanos(libc::CLOCK_BOOTTIME))
    .or_else(|| raw_clock_nanos(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn vdso_clock_boottime_time64() -> u64 {
  super::linux_vdso::clock_nanos_time64(libc::CLOCK_BOOTTIME)
    .or_else(|| libc_clock_nanos(libc::CLOCK_BOOTTIME))
    .or_else(|| raw_clock_nanos_time64(libc::CLOCK_BOOTTIME))
    .or_else(|| raw_clock_nanos(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[inline(always)]
fn ordered_vdso_clock_monotonic() -> u64 {
  ordered_clock_barrier();
  vdso_clock_monotonic()
}

#[inline(always)]
fn ordered_vdso_clock_monotonic_raw() -> u64 {
  ordered_clock_barrier();
  vdso_clock_monotonic_raw()
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn ordered_vdso_clock_monotonic_time64() -> u64 {
  ordered_clock_barrier();
  vdso_clock_monotonic_time64()
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn ordered_vdso_clock_monotonic_raw_time64() -> u64 {
  ordered_clock_barrier();
  vdso_clock_monotonic_raw_time64()
}

#[inline(always)]
fn ordered_vdso_clock_boottime() -> u64 {
  ordered_clock_barrier();
  vdso_clock_boottime()
}

#[cfg(target_arch = "arm")]
#[inline(always)]
fn ordered_vdso_clock_boottime_time64() -> u64 {
  ordered_clock_barrier();
  vdso_clock_boottime_time64()
}

#[cfg(target_arch = "arm")]
#[repr(C)]
struct LinuxTime32 {
  seconds: i32,
  nanos: i32,
}

#[cfg(target_arch = "arm")]
#[repr(C)]
struct LinuxKernelTimespec {
  seconds: i64,
  nanos: i64,
}

#[cfg(target_arch = "arm")]
#[inline(always)]
#[allow(clippy::inline_always)]
fn raw_clock_nanos(clock_id: libc::clockid_t) -> Option<u64> {
  let mut value = MaybeUninit::<LinuxTime32>::uninit();
  // SAFETY: syscall 263 writes the Arm Linux time32 layout declared above.
  let status =
    unsafe { arm_clock_gettime(libc::SYS_clock_gettime, clock_id, value.as_mut_ptr().cast()) };
  if status != 0 {
    return None;
  }
  // SAFETY: a successful syscall initialized the output.
  let value = unsafe { value.assume_init() };
  time_parts_to_nanos(i64::from(value.seconds), i64::from(value.nanos))
}

#[cfg(target_arch = "arm")]
#[inline(always)]
#[allow(clippy::inline_always)]
fn raw_clock_nanos_time64(clock_id: libc::clockid_t) -> Option<u64> {
  const SYS_CLOCK_GETTIME64: libc::c_long = 403;
  let mut value = MaybeUninit::<LinuxKernelTimespec>::uninit();
  // SAFETY: syscall 403 writes the 32-bit Linux kernel-timespec layout.
  let status =
    unsafe { arm_clock_gettime(SYS_CLOCK_GETTIME64, clock_id, value.as_mut_ptr().cast()) };
  if status != 0 {
    return None;
  }
  // SAFETY: a successful syscall initialized the output.
  let value = unsafe { value.assume_init() };
  time_parts_to_nanos(value.seconds, value.nanos)
}

#[cfg(target_arch = "arm")]
#[inline(always)]
#[allow(clippy::inline_always)]
unsafe fn arm_clock_gettime(
  number: libc::c_long,
  clock: libc::clockid_t,
  value: *mut core::ffi::c_void,
) -> libc::c_long {
  let status: libc::c_long;
  // Linux Arm EABI fixes the syscall number in r7 and the first two arguments
  // in r0/r1. Preserve r7 because LLVM may reserve it as the frame pointer.
  // The balanced push/pop means this asm intentionally does not claim
  // `nostack`; omitting `nomem` declares the kernel's output write.
  // SAFETY: the register convention and clobbers match Linux Arm EABI.
  unsafe {
    core::arch::asm!(
      "push {{r7}}",
      "mov r7, {number}",
      "svc 0",
      "pop {{r7}}",
      number = in(reg) number,
      inlateout("r0") clock => status,
      in("r1") value,
      options(preserves_flags),
    );
  }
  status
}

#[cfg(target_arch = "s390x")]
#[inline(always)]
#[allow(clippy::inline_always)]
fn raw_clock_nanos(clock_id: libc::clockid_t) -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  let status: libc::c_long;
  // SAFETY: s390x Linux takes the syscall number in r1 and arguments in
  // r2/r3. The kernel writes the native 64-bit timespec through `value`.
  unsafe {
    core::arch::asm!(
      "svc 0",
      in("r1") libc::SYS_clock_gettime,
      inlateout("r2") libc::c_long::from(clock_id) => status,
      in("r3") value.as_mut_ptr(),
      options(nostack, preserves_flags),
    );
  }
  if status != 0 {
    return None;
  }
  // SAFETY: a successful syscall initialized the output.
  let value = unsafe { value.assume_init() };
  time_parts_to_nanos(value.tv_sec, value.tv_nsec)
}

fn monotonic_raw_nanos() -> Option<u64> {
  libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW)
}

#[inline(always)]
fn libc_clock_nanos(clock_id: libc::clockid_t) -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: the output storage has the libc ABI and the routed Linux clock ids
  // are valid when their availability probes succeed.
  let status = unsafe { libc::clock_gettime(clock_id, value.as_mut_ptr()) };
  if status != 0 {
    return None;
  }
  // SAFETY: clock_gettime initialized the output on success.
  let value = unsafe { value.assume_init() };
  time_parts_to_nanos(value.tv_sec.into(), value.tv_nsec.into())
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn time_parts_to_nanos(seconds: i64, nanos: i64) -> Option<u64> {
  let seconds = u64::try_from(seconds).ok()?;
  let nanos = u32::try_from(nanos).ok()?;
  if nanos >= 1_000_000_000 {
    return None;
  }
  seconds.checked_mul(1_000_000_000)?.checked_add(u64::from(nanos))
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

#[cfg(feature = "bench-internal")]
const fn provider_from_raw(provider: u8) -> WallProvider {
  match provider {
    #[cfg(target_arch = "arm")]
    PROVIDER_CNTVCT => WallProvider::ArmCntvct,
    #[cfg(target_arch = "arm")]
    PROVIDER_CNTVCT_ORDERED => WallProvider::ArmCntvctOrdered,
    PROVIDER_RAW => WallProvider::RawClockGettime,
    PROVIDER_RAW_OS_ORDERED => WallProvider::RawClockGettimeOsOrdered,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64 => WallProvider::RawClockGettime64,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_OS_ORDERED => WallProvider::RawClockGettime64OsOrdered,
    PROVIDER_LIBC_MONOTONIC_RAW => WallProvider::LibcClockMonotonicRaw,
    PROVIDER_RAW_MONOTONIC_RAW => WallProvider::RawClockGettimeMonotonicRaw,
    PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED => WallProvider::RawClockGettimeMonotonicRawOsOrdered,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW => WallProvider::RawClockGettime64MonotonicRaw,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED => {
      WallProvider::RawClockGettime64MonotonicRawOsOrdered
    }
    PROVIDER_VDSO => WallProvider::VdsoClockMonotonic,
    PROVIDER_VDSO_MONOTONIC_RAW => WallProvider::VdsoClockMonotonicRaw,
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64 => WallProvider::VdsoClockMonotonicTime64,
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW => WallProvider::VdsoClockMonotonicRawTime64,
    PROVIDER_LIBC_BOOTTIME => WallProvider::LibcClockBoottime,
    PROVIDER_RAW_BOOTTIME => WallProvider::RawClockGettimeBoottime,
    PROVIDER_RAW_BOOTTIME_OS_ORDERED => WallProvider::RawClockGettimeBoottimeOsOrdered,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME => WallProvider::RawClockGettime64Boottime,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED => WallProvider::RawClockGettime64BoottimeOsOrdered,
    PROVIDER_VDSO_BOOTTIME => WallProvider::VdsoClockBoottime,
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_BOOTTIME => WallProvider::VdsoClockBoottimeTime64,
    _ => WallProvider::LibcClockMonotonic,
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
  pub(crate) nanos_per_tick_q32: u64,
}

#[cfg(feature = "bench-internal")]
#[inline]
fn bench_nanos_per_tick_q32(provider: u8) -> u64 {
  crate::arch::scale_from_ratio(1_000_000_000, provider_frequency(provider))
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness selects once before its timing loop.
pub(crate) fn bench_selected_instant_primitive() -> BenchPrimitive {
  instant_bench_primitive(selected_instant_provider())
}

#[cfg(feature = "bench-internal")]
fn instant_bench_primitive(provider: u8) -> BenchPrimitive {
  let read = match provider {
    #[cfg(target_arch = "arm")]
    PROVIDER_CNTVCT | PROVIDER_CNTVCT_ORDERED => arm_cntvct as fn() -> u64,
    PROVIDER_RAW => raw_clock_monotonic as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64 => raw_clock_monotonic_time64 as fn() -> u64,
    PROVIDER_LIBC_MONOTONIC_RAW => libc_clock_monotonic_raw as fn() -> u64,
    PROVIDER_RAW_MONOTONIC_RAW => raw_clock_monotonic_raw as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW => raw_clock_monotonic_raw_time64 as fn() -> u64,
    PROVIDER_VDSO => vdso_clock_monotonic as fn() -> u64,
    PROVIDER_VDSO_MONOTONIC_RAW => vdso_clock_monotonic_raw as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64 => vdso_clock_monotonic_time64 as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW => vdso_clock_monotonic_raw_time64 as fn() -> u64,
    PROVIDER_LIBC_BOOTTIME => libc_clock_boottime as fn() -> u64,
    PROVIDER_RAW_BOOTTIME => raw_clock_boottime as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME => raw_clock_boottime_time64 as fn() -> u64,
    PROVIDER_VDSO_BOOTTIME => vdso_clock_boottime as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_BOOTTIME => vdso_clock_boottime_time64 as fn() -> u64,
    _ => libc_clock_monotonic as fn() -> u64,
  };
  BenchPrimitive {
    name: provider_from_raw(provider).name(),
    read,
    nanos_per_tick_q32: bench_nanos_per_tick_q32(provider),
  }
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness selects once before its timing loop.
pub(crate) fn bench_selected_ordered_primitive() -> BenchPrimitive {
  ordered_bench_primitive(selected_ordered_provider())
}

#[cfg(feature = "bench-internal")]
fn ordered_bench_primitive(provider: u8) -> BenchPrimitive {
  let read = match provider {
    #[cfg(target_arch = "arm")]
    PROVIDER_CNTVCT | PROVIDER_CNTVCT_ORDERED => arm_cntvct_ordered as fn() -> u64,
    PROVIDER_RAW => ordered_raw_clock_monotonic as fn() -> u64,
    PROVIDER_RAW_OS_ORDERED => os_ordered_raw_clock_monotonic as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64 => ordered_raw_clock_monotonic_time64 as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_OS_ORDERED => os_ordered_raw_clock_monotonic_time64 as fn() -> u64,
    PROVIDER_LIBC_MONOTONIC_RAW => ordered_libc_clock_monotonic_raw as fn() -> u64,
    PROVIDER_RAW_MONOTONIC_RAW => ordered_raw_clock_monotonic_raw as fn() -> u64,
    PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED => os_ordered_raw_clock_monotonic_raw as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW => ordered_raw_clock_monotonic_raw_time64 as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_MONOTONIC_RAW_OS_ORDERED => {
      os_ordered_raw_clock_monotonic_raw_time64 as fn() -> u64
    }
    PROVIDER_VDSO => ordered_vdso_clock_monotonic as fn() -> u64,
    PROVIDER_VDSO_MONOTONIC_RAW => ordered_vdso_clock_monotonic_raw as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64 => ordered_vdso_clock_monotonic_time64 as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW => ordered_vdso_clock_monotonic_raw_time64 as fn() -> u64,
    PROVIDER_LIBC_BOOTTIME => ordered_libc_clock_boottime as fn() -> u64,
    PROVIDER_RAW_BOOTTIME => ordered_raw_clock_boottime as fn() -> u64,
    PROVIDER_RAW_BOOTTIME_OS_ORDERED => os_ordered_raw_clock_boottime as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME => ordered_raw_clock_boottime_time64 as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED => os_ordered_raw_clock_boottime_time64 as fn() -> u64,
    PROVIDER_VDSO_BOOTTIME => ordered_vdso_clock_boottime as fn() -> u64,
    #[cfg(target_arch = "arm")]
    PROVIDER_VDSO_TIME64_BOOTTIME => ordered_vdso_clock_boottime_time64 as fn() -> u64,
    _ => ordered_clock_monotonic as fn() -> u64,
  };
  BenchPrimitive {
    name: provider_from_raw(provider).name(),
    read,
    nanos_per_tick_q32: bench_nanos_per_tick_q32(provider),
  }
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
exact_bench_reader!(bench_exact_libc_monotonic, libc_clock_monotonic());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(bench_exact_arm_cntvct, arm_cntvct());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_raw_monotonic, raw_clock_monotonic());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(bench_exact_raw_time64_monotonic, raw_clock_monotonic_time64());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_libc_monotonic_raw, libc_clock_monotonic_raw());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_raw_monotonic_raw, raw_clock_monotonic_raw());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(bench_exact_raw_time64_monotonic_raw, raw_clock_monotonic_raw_time64());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_vdso_monotonic, vdso_clock_monotonic());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_vdso_monotonic_raw, vdso_clock_monotonic_raw());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(bench_exact_vdso_time64_monotonic, vdso_clock_monotonic_time64());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(bench_exact_vdso_time64_monotonic_raw, vdso_clock_monotonic_raw_time64());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_libc_boottime, libc_clock_boottime());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_raw_boottime, raw_clock_boottime());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(bench_exact_raw_time64_boottime, raw_clock_boottime_time64());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_vdso_boottime, vdso_clock_boottime());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(bench_exact_vdso_time64_boottime, vdso_clock_boottime_time64());

#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_ordered_libc_monotonic, ordered_clock_monotonic());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(bench_exact_ordered_arm_cntvct, arm_cntvct_ordered());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_ordered_raw_monotonic, ordered_raw_clock_monotonic());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_os_ordered_raw_monotonic, os_ordered_raw_clock_monotonic());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(bench_exact_ordered_raw_time64_monotonic, ordered_raw_clock_monotonic_time64());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(
  bench_exact_os_ordered_raw_time64_monotonic,
  os_ordered_raw_clock_monotonic_time64()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_ordered_libc_monotonic_raw, ordered_libc_clock_monotonic_raw());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_ordered_raw_monotonic_raw, ordered_raw_clock_monotonic_raw());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_os_ordered_raw_monotonic_raw, os_ordered_raw_clock_monotonic_raw());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(
  bench_exact_ordered_raw_time64_monotonic_raw,
  ordered_raw_clock_monotonic_raw_time64()
);
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(
  bench_exact_os_ordered_raw_time64_monotonic_raw,
  os_ordered_raw_clock_monotonic_raw_time64()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_ordered_vdso_monotonic, ordered_vdso_clock_monotonic());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_ordered_vdso_monotonic_raw, ordered_vdso_clock_monotonic_raw());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(
  bench_exact_ordered_vdso_time64_monotonic,
  ordered_vdso_clock_monotonic_time64()
);
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(
  bench_exact_ordered_vdso_time64_monotonic_raw,
  ordered_vdso_clock_monotonic_raw_time64()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_ordered_libc_boottime, ordered_libc_clock_boottime());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_ordered_raw_boottime, ordered_raw_clock_boottime());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_os_ordered_raw_boottime, os_ordered_raw_clock_boottime());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(bench_exact_ordered_raw_time64_boottime, ordered_raw_clock_boottime_time64());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(
  bench_exact_os_ordered_raw_time64_boottime,
  os_ordered_raw_clock_boottime_time64()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_ordered_vdso_boottime, ordered_vdso_clock_boottime());
#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
exact_bench_reader!(bench_exact_ordered_vdso_time64_boottime, ordered_vdso_clock_boottime_time64());

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

  #[test]
  fn os_owned_syscall_candidates_are_ordered_only() {
    assert!(is_os_ordered_provider(PROVIDER_RAW_OS_ORDERED));
    assert!(is_os_ordered_provider(PROVIDER_RAW_MONOTONIC_RAW_OS_ORDERED));
    assert!(is_os_ordered_provider(PROVIDER_RAW_BOOTTIME_OS_ORDERED));
    #[cfg(target_arch = "arm")]
    assert!(is_os_ordered_provider(PROVIDER_RAW_TIME64_OS_ORDERED));
    #[cfg(target_arch = "arm")]
    assert!(is_os_ordered_provider(PROVIDER_RAW_TIME64_BOOTTIME_OS_ORDERED));
    assert!(!is_os_ordered_provider(PROVIDER_RAW));
  }

  #[test]
  fn candidate_list_compacts_unavailable_routes() {
    let mut candidates = [PROVIDER_UNKNOWN; 2];
    let mut count = 0;
    push_candidate(&mut candidates, &mut count, PROVIDER_RAW, false);
    push_candidate(&mut candidates, &mut count, PROVIDER_LIBC, true);
    assert_eq!(count, 1);
    assert_eq!(candidates[0], PROVIDER_LIBC);
  }

  #[cfg(target_arch = "arm")]
  #[test]
  fn cntvct_requires_both_kernel_proofs_and_a_frequency() {
    assert!(arm_cntvct_eligible(true, true, 24_000_000));
    assert!(!arm_cntvct_eligible(false, true, 24_000_000));
    assert!(!arm_cntvct_eligible(true, false, 24_000_000));
    assert!(!arm_cntvct_eligible(true, true, 0));
  }

  #[cfg(target_arch = "arm")]
  #[test]
  fn cntvct_ordered_endpoint_keeps_the_same_frequency_domain() {
    assert_eq!(provider_frequency_for(PROVIDER_CNTVCT, 19_200_000), 19_200_000);
    assert_eq!(provider_frequency_for(PROVIDER_CNTVCT_ORDERED, 19_200_000), 19_200_000);
    assert_eq!(
      candidate_samples(
        ProbeSamples {
          cntvct: [7; PROBE_BATCHES],
          libc: [0; PROBE_BATCHES],
          raw: [0; PROBE_BATCHES],
          raw_os_ordered: [0; PROBE_BATCHES],
          raw_time64: [0; PROBE_BATCHES],
          raw_time64_os_ordered: [0; PROBE_BATCHES],
          libc_monotonic_raw: [0; PROBE_BATCHES],
          raw_monotonic_raw: [0; PROBE_BATCHES],
          raw_monotonic_raw_os_ordered: [0; PROBE_BATCHES],
          raw_time64_monotonic_raw: [0; PROBE_BATCHES],
          raw_time64_monotonic_raw_os_ordered: [0; PROBE_BATCHES],
          vdso: [0; PROBE_BATCHES],
          vdso_monotonic_raw: [0; PROBE_BATCHES],
          vdso_time64: [0; PROBE_BATCHES],
          vdso_time64_monotonic_raw: [0; PROBE_BATCHES],
          libc_boottime: [0; PROBE_BATCHES],
          raw_boottime: [0; PROBE_BATCHES],
          raw_boottime_os_ordered: [0; PROBE_BATCHES],
          raw_time64_boottime: [0; PROBE_BATCHES],
          raw_time64_boottime_os_ordered: [0; PROBE_BATCHES],
          vdso_boottime: [0; PROBE_BATCHES],
          vdso_time64_boottime: [0; PROBE_BATCHES],
        },
        PROVIDER_CNTVCT_ORDERED,
      ),
      [7; PROBE_BATCHES]
    );
  }
}
