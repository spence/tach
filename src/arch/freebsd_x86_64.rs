//! FreeBSD/amd64 wall-clock provider selection.
//!
//! FreeBSD exposes both the timecounter it selected and that counter's
//! quality. A direct TSC read is eligible only when the kernel selected a
//! synchronized, invariant `TSC`/`TSC-low` provider with nonnegative quality.
//! Each Instant contract independently compares tach's branched TSC path, a
//! direct reader for FreeBSD's public `AT_TIMEKEEP` ABI, libc's
//! `CLOCK_MONOTONIC` path, and the raw syscall. This permits different winners
//! without ever mixing their raw domains or scale factors.
//!
//! FreeBSD routes `CLOCK_MONOTONIC`, `CLOCK_MONOTONIC_PRECISE`, `CLOCK_UPTIME`,
//! and `CLOCK_UPTIME_PRECISE` through the same precise `nanouptime`/
//! `binuptime` implementation. `CLOCK_BOOTTIME` is an alias for one of those
//! exact routes (UPTIME through FreeBSD 14, MONOTONIC in current FreeBSD), so
//! benchmarking the aliases would only duplicate the same mechanism. `_FAST`
//! and `_COARSE` clocks are excluded because they intentionally skip the full
//! timecounter query.
//!
//! The direct timehands reader implements every algorithm in the current
//! amd64 ABI: TSC, HPET, Hyper-V reference TSC, and PVClock. Unknown versions,
//! algorithms, layouts, unavailable device mappings, and transiently invalid
//! snapshots fail closed to the same `CLOCK_MONOTONIC` domain. Ordered libc
//! candidates may rely on the vDSO's ordering protocol only when libc's raw
//! syscall fallback also owns ordering. Bare SYSCALL is OS-owned only on Intel;
//! AMD and unknown vendors retain explicit-barrier candidates.

use core::arch::asm;
use core::arch::x86_64::{__cpuid, __cpuid_count};
#[cfg(feature = "bench-internal")]
use core::cell::UnsafeCell;
use core::hint::black_box;
use core::mem::{MaybeUninit, size_of};
use core::ptr;
#[cfg(feature = "bench-internal")]
use core::sync::atomic::AtomicBool;
use core::sync::atomic::{
  AtomicI32, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering, compiler_fence, fence,
};

const PROVIDER_UNKNOWN: u8 = 0;
const PROVIDER_SELECTING: u8 = 1;
const PROVIDER_TSC: u8 = 2;
const PROVIDER_CLOCK_MONOTONIC: u8 = 3;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL: u8 = 4;
const PROVIDER_CLOCK_MONOTONIC_MFENCE: u8 = 5;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL_MFENCE: u8 = 6;
const PROVIDER_CLOCK_MONOTONIC_CPUID: u8 = 7;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID: u8 = 8;
const PROVIDER_CLOCK_MONOTONIC_LFENCE: u8 = 9;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL_LFENCE: u8 = 10;
const PROVIDER_CLOCK_MONOTONIC_RDTSCP: u8 = 11;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL_RDTSCP: u8 = 12;
const PROVIDER_CLOCK_MONOTONIC_OS_OWNED: u8 = 13;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_OWNED: u8 = 14;
const PROVIDER_CLOCK_MONOTONIC_SERIALIZE: u8 = 15;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL_SERIALIZE: u8 = 16;
const PROVIDER_TIMEKEEP: u8 = 17;

const TIMEKEEP_UNKNOWN: u8 = 0;
const TIMEKEEP_SELECTING: u8 = 1;
const TIMEKEEP_READY: u8 = 2;
const TIMEKEEP_UNAVAILABLE: u8 = 3;

const VDSO_TK_VERSION: u32 = 1;
const VDSO_TIMEHANDS_COUNT: usize = 4;
const VDSO_ALGO_TSC: u32 = 1;
const VDSO_ALGO_HPET: u32 = 2;
const VDSO_ALGO_HYPERV_REFTSC: u32 = 3;
const VDSO_ALGO_PVCLOCK: u32 = 4;
const TIMEKEEP_RETRIES: usize = 32;

const TSC_MODE_INTEL_LFENCE: u8 = 0;
const TSC_MODE_AMD_MFENCE: u8 = 1;
const TSC_MODE_PLAIN: u8 = 2;
const TSC_MODE_RDTSCP: u8 = 3;

const HPET_DEVICE_COUNT: usize = 10;
const HPET_MAIN_COUNTER_OFFSET: usize = 0xf0;

const ORDERED_BARRIER_CPUID: u8 = 0;
const ORDERED_BARRIER_LFENCE: u8 = 1;
const ORDERED_BARRIER_MFENCE: u8 = 2;
const ORDERED_BARRIER_RDTSCP: u8 = 3;
const ORDERED_BARRIER_OS_OWNED: u8 = 4;
const ORDERED_BARRIER_SERIALIZE: u8 = 5;
const ORDERED_BARRIER_CANDIDATES: usize = 6;
#[cfg_attr(not(feature = "bench-internal"), allow(dead_code))]
const ORDERED_BARRIER_DECISIONS: usize = ORDERED_BARRIER_CANDIDATES - 1;

const PROBE_BATCHES: usize = 9;
const PROBE_READS: u64 = 4096;
const PROBE_WARMUP_READS: u64 = 1024;
const REQUIRED_DECISIVE_WINS: usize = 8;

static INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static INSTANT_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static INSTANT_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
// A direct timehands read and each fallback return CLOCK_MONOTONIC nanoseconds.
// Retiring AT_TIMEKEEP only to this already-measured fallback preserves the
// numeric domain of values sampled before the transition.
static INSTANT_TIMEKEEP_FALLBACK: AtomicU8 = AtomicU8::new(PROVIDER_CLOCK_MONOTONIC);
static ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static ORDERED_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_TIMEKEEP_FALLBACK: AtomicU8 = AtomicU8::new(PROVIDER_CLOCK_MONOTONIC_CPUID);
static TSC_FREQUENCY: AtomicU64 = AtomicU64::new(0);

static TIMEKEEP_STATE: AtomicU8 = AtomicU8::new(TIMEKEEP_UNKNOWN);
static TIMEKEEP_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static TIMEKEEP_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static TIMEKEEP_PTR: AtomicUsize = AtomicUsize::new(0);
static TIMEKEEP_TSC_MODE: AtomicU8 = AtomicU8::new(TSC_MODE_PLAIN);
static HPET_MAPS: [AtomicUsize; HPET_DEVICE_COUNT] =
  [const { AtomicUsize::new(0) }; HPET_DEVICE_COUNT];
static HYPERV_REFTSC_MAP: AtomicUsize = AtomicUsize::new(0);
static PVCLOCK_MAP: AtomicUsize = AtomicUsize::new(0);
static PVCLOCK_CPU_COUNT: AtomicU32 = AtomicU32::new(0);

// This separate atomic makes the probe compile to the same predicted load and
// branch as the published hot path without exposing a half-selected provider
// to concurrent callers.
static PROBE_INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_TSC);
static PROBE_ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_TSC);

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WallProvider {
  Tsc,
  Timekeep,
  ClockMonotonic,
  ClockMonotonicSyscall,
}

#[cfg(feature = "bench-internal")]
impl WallProvider {
  pub(crate) const fn name(self) -> &'static str {
    match self {
      Self::Tsc => "freebsd_kernel_eligible_tsc",
      Self::Timekeep => "freebsd_at_timekeep",
      Self::ClockMonotonic => "freebsd_clock_monotonic",
      Self::ClockMonotonicSyscall => "freebsd_clock_monotonic_syscall",
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
  timekeep: [u64; PROBE_BATCHES],
  clock: [u64; PROBE_BATCHES],
  syscall: [u64; PROBE_BATCHES],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Bintime {
  sec: i64,
  frac: u64,
}

#[repr(C)]
struct VdsoTimehands {
  th_algo: u32,
  th_gen: u32,
  th_scale: u64,
  th_offset_count: u32,
  th_counter_mask: u32,
  th_offset: Bintime,
  th_boottime: Bintime,
  th_x86_shift: u32,
  th_x86_hpet_idx: u32,
  th_x86_pvc_last_systime: u64,
  th_x86_pvc_stable_mask: u8,
  th_res: [u8; 15],
}

// The flexible `tk_th[]` member has 8-byte alignment, so the C ABI pads the
// three-word header to 16 bytes before the first timehands slot.
#[repr(C)]
struct VdsoTimekeep {
  tk_ver: u32,
  tk_enabled: u32,
  tk_current: u32,
  _padding: u32,
}

#[repr(C)]
struct PvclockVcpuTimeInfo {
  version: u32,
  pad0: u32,
  tsc_timestamp: u64,
  system_time: u64,
  tsc_to_system_mul: u32,
  tsc_shift: i8,
  flags: u8,
  pad: [u8; 2],
}

#[repr(C)]
struct HypervRefTsc {
  tsc_seq: u32,
  tsc_rsvd1: u32,
  tsc_scale: u64,
  tsc_ofs: i64,
}

const _: () = assert!(size_of::<Bintime>() == 16);
const _: () = assert!(size_of::<VdsoTimehands>() == 88);
const _: () = assert!(size_of::<VdsoTimekeep>() == 16);
const _: () = assert!(size_of::<PvclockVcpuTimeInfo>() == 32);
const _: () = assert!(size_of::<HypervRefTsc>() == 24);

#[derive(Clone, Copy, Debug)]
struct OrderedBarrierCandidates {
  barriers: [u8; ORDERED_BARRIER_CANDIDATES],
  count: usize,
}

impl OrderedBarrierCandidates {
  const fn conservative() -> Self {
    Self { barriers: [ORDERED_BARRIER_CPUID; ORDERED_BARRIER_CANDIDATES], count: 1 }
  }

  fn push(&mut self, barrier: u8) {
    debug_assert!(self.count < self.barriers.len());
    self.barriers[self.count] = barrier;
    self.count += 1;
  }
}

#[derive(Clone, Copy, Debug)]
struct OrderedPathSelection {
  provider: u8,
  #[cfg(feature = "bench-internal")]
  fast_candidate: &'static str,
  #[cfg(feature = "bench-internal")]
  fast_batches_ns: [u64; PROBE_BATCHES],
  #[cfg(feature = "bench-internal")]
  baseline_barrier: &'static str,
  #[cfg(feature = "bench-internal")]
  baseline_batches_ns: [u64; PROBE_BATCHES],
  #[cfg(feature = "bench-internal")]
  decision: SelectionDecision,
  #[cfg(feature = "bench-internal")]
  candidate_count: usize,
  #[cfg(feature = "bench-internal")]
  candidate_barriers: [u8; ORDERED_BARRIER_CANDIDATES],
  #[cfg(feature = "bench-internal")]
  candidate_batches_ns: [[u64; PROBE_BATCHES]; ORDERED_BARRIER_CANDIDATES],
  #[cfg(feature = "bench-internal")]
  decision_count: usize,
  #[cfg(feature = "bench-internal")]
  challengers: [u8; ORDERED_BARRIER_DECISIONS],
  #[cfg(feature = "bench-internal")]
  incumbents: [u8; ORDERED_BARRIER_DECISIONS],
  #[cfg(feature = "bench-internal")]
  winners: [u8; ORDERED_BARRIER_DECISIONS],
  #[cfg(feature = "bench-internal")]
  decisions: [SelectionDecision; ORDERED_BARRIER_DECISIONS],
}

#[cfg(feature = "bench-internal")]
struct OrderedPathEvidenceProjection {
  candidate_names: [&'static str; ORDERED_BARRIER_CANDIDATES],
  candidate_medians_ns: [u64; ORDERED_BARRIER_CANDIDATES],
  challengers: [&'static str; ORDERED_BARRIER_DECISIONS],
  incumbents: [&'static str; ORDERED_BARRIER_DECISIONS],
  winners: [&'static str; ORDERED_BARRIER_DECISIONS],
  allowances_ns: [u64; ORDERED_BARRIER_DECISIONS],
  decisive_wins: [usize; ORDERED_BARRIER_DECISIONS],
  challenger_selected: [bool; ORDERED_BARRIER_DECISIONS],
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // Architecture evidence is complete even when a serializer projects a subset.
pub(crate) struct ProbeEvidence {
  pub(crate) candidate_count: usize,
  pub(crate) candidate_providers: [u8; 4],
  pub(crate) tsc_eligible: bool,
  pub(crate) timekeep_available: bool,
  pub(crate) reads_per_batch: u64,
  pub(crate) direct_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) timekeep_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) direct_median_ns: u64,
  pub(crate) timekeep_median_ns: u64,
  pub(crate) clock_median_ns: u64,
  pub(crate) syscall_median_ns: u64,
  pub(crate) fallback_allowance_ns: u64,
  pub(crate) fallback_decisive_wins: usize,
  pub(crate) timekeep_allowance_ns: u64,
  pub(crate) timekeep_decisive_wins: usize,
  pub(crate) direct_allowance_ns: u64,
  pub(crate) direct_decisive_wins: usize,
  pub(crate) required_decisive_wins: usize,
  pub(crate) selected_provider: WallProvider,
  pub(crate) selected_provider_name: &'static str,
  pub(crate) ordered_os_barrier: &'static str,
  pub(crate) ordered_bare_syscall_os_owned_eligible: bool,
  pub(crate) ordered_bare_syscall_os_owned_basis: &'static str,
  pub(crate) ordered_fast_barrier_candidate: &'static str,
  pub(crate) ordered_fast_barrier_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_mfence_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_barrier_allowance_ns: u64,
  pub(crate) ordered_barrier_decisive_wins: usize,
  pub(crate) ordered_baseline_barrier: &'static str,
  pub(crate) ordered_clock_fast_barrier_candidate: &'static str,
  pub(crate) ordered_clock_fast_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_clock_baseline_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_clock_barrier_allowance_ns: u64,
  pub(crate) ordered_clock_barrier_decisive_wins: usize,
  pub(crate) ordered_syscall_fast_barrier_candidate: &'static str,
  pub(crate) ordered_syscall_fast_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_syscall_baseline_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_syscall_barrier_allowance_ns: u64,
  pub(crate) ordered_syscall_barrier_decisive_wins: usize,
  pub(crate) ordered_barrier_candidate_count: usize,
  pub(crate) ordered_barrier_candidate_names: [&'static str; ORDERED_BARRIER_CANDIDATES],
  pub(crate) ordered_clock_candidate_providers: [u8; ORDERED_BARRIER_CANDIDATES],
  pub(crate) ordered_syscall_barrier_candidate_count: usize,
  pub(crate) ordered_syscall_barrier_candidate_names: [&'static str; ORDERED_BARRIER_CANDIDATES],
  pub(crate) ordered_syscall_candidate_providers: [u8; ORDERED_BARRIER_CANDIDATES],
  pub(crate) ordered_clock_barrier_candidate_batches_ns:
    [[u64; PROBE_BATCHES]; ORDERED_BARRIER_CANDIDATES],
  pub(crate) ordered_clock_barrier_candidate_medians_ns: [u64; ORDERED_BARRIER_CANDIDATES],
  pub(crate) ordered_clock_barrier_decision_count: usize,
  pub(crate) ordered_clock_barrier_challengers: [&'static str; ORDERED_BARRIER_DECISIONS],
  pub(crate) ordered_clock_barrier_incumbents: [&'static str; ORDERED_BARRIER_DECISIONS],
  pub(crate) ordered_clock_barrier_winners: [&'static str; ORDERED_BARRIER_DECISIONS],
  pub(crate) ordered_clock_barrier_allowances_ns: [u64; ORDERED_BARRIER_DECISIONS],
  pub(crate) ordered_clock_barrier_tournament_decisive_wins: [usize; ORDERED_BARRIER_DECISIONS],
  pub(crate) ordered_clock_barrier_challenger_selected: [bool; ORDERED_BARRIER_DECISIONS],
  pub(crate) ordered_syscall_barrier_candidate_batches_ns:
    [[u64; PROBE_BATCHES]; ORDERED_BARRIER_CANDIDATES],
  pub(crate) ordered_syscall_barrier_candidate_medians_ns: [u64; ORDERED_BARRIER_CANDIDATES],
  pub(crate) ordered_syscall_barrier_decision_count: usize,
  pub(crate) ordered_syscall_barrier_challengers: [&'static str; ORDERED_BARRIER_DECISIONS],
  pub(crate) ordered_syscall_barrier_incumbents: [&'static str; ORDERED_BARRIER_DECISIONS],
  pub(crate) ordered_syscall_barrier_winners: [&'static str; ORDERED_BARRIER_DECISIONS],
  pub(crate) ordered_syscall_barrier_allowances_ns: [u64; ORDERED_BARRIER_DECISIONS],
  pub(crate) ordered_syscall_barrier_tournament_decisive_wins: [usize; ORDERED_BARRIER_DECISIONS],
  pub(crate) ordered_syscall_barrier_challenger_selected: [bool; ORDERED_BARRIER_DECISIONS],
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
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  match INSTANT_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_TSC => super::x86_64::rdtsc(),
    PROVIDER_TIMEKEEP => instant_timekeep_clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    _ => ticks_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_after_selection() -> u64 {
  match selected_instant_provider() {
    PROVIDER_TSC => super::x86_64::rdtsc(),
    PROVIDER_TIMEKEEP => instant_timekeep_clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    _ => clock_monotonic_syscall(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  match ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_TSC => super::x86_64::rdtsc_ordered(),
    PROVIDER_TIMEKEEP => ordered_timekeep_clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_MFENCE => ordered_clock_monotonic_mfence(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_MFENCE => ordered_clock_monotonic_syscall_mfence(),
    PROVIDER_CLOCK_MONOTONIC_CPUID => ordered_clock_monotonic_cpuid(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID => ordered_clock_monotonic_syscall_cpuid(),
    PROVIDER_CLOCK_MONOTONIC_LFENCE => ordered_clock_monotonic_lfence(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_LFENCE => ordered_clock_monotonic_syscall_lfence(),
    PROVIDER_CLOCK_MONOTONIC_RDTSCP => ordered_clock_monotonic_rdtscp(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_RDTSCP => ordered_clock_monotonic_syscall_rdtscp(),
    PROVIDER_CLOCK_MONOTONIC_OS_OWNED => ordered_clock_monotonic_os_owned(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_OWNED => ordered_clock_monotonic_syscall_os_owned(),
    PROVIDER_CLOCK_MONOTONIC_SERIALIZE => ordered_clock_monotonic_serialize(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_SERIALIZE => ordered_clock_monotonic_syscall_serialize(),
    _ => ticks_ordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_after_selection() -> u64 {
  match selected_ordered_provider() {
    PROVIDER_TSC => super::x86_64::rdtsc_ordered(),
    PROVIDER_TIMEKEEP => ordered_timekeep_clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_MFENCE => ordered_clock_monotonic_mfence(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_MFENCE => ordered_clock_monotonic_syscall_mfence(),
    PROVIDER_CLOCK_MONOTONIC_CPUID => ordered_clock_monotonic_cpuid(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID => ordered_clock_monotonic_syscall_cpuid(),
    PROVIDER_CLOCK_MONOTONIC_LFENCE => ordered_clock_monotonic_lfence(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_LFENCE => ordered_clock_monotonic_syscall_lfence(),
    PROVIDER_CLOCK_MONOTONIC_RDTSCP => ordered_clock_monotonic_rdtscp(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_RDTSCP => ordered_clock_monotonic_syscall_rdtscp(),
    PROVIDER_CLOCK_MONOTONIC_OS_OWNED => ordered_clock_monotonic_os_owned(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_OWNED => ordered_clock_monotonic_syscall_os_owned(),
    PROVIDER_CLOCK_MONOTONIC_SERIALIZE => ordered_clock_monotonic_serialize(),
    _ => ordered_clock_monotonic_syscall_serialize(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  match ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_TSC => super::x86_64::rdtsc(),
    PROVIDER_TIMEKEEP => ordered_timekeep_clock_monotonic_unordered(),
    PROVIDER_CLOCK_MONOTONIC_MFENCE
    | PROVIDER_CLOCK_MONOTONIC_CPUID
    | PROVIDER_CLOCK_MONOTONIC_LFENCE
    | PROVIDER_CLOCK_MONOTONIC_RDTSCP
    | PROVIDER_CLOCK_MONOTONIC_OS_OWNED
    | PROVIDER_CLOCK_MONOTONIC_SERIALIZE => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_MFENCE
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_LFENCE
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_RDTSCP
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_OWNED
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_SERIALIZE => clock_monotonic_syscall(),
    _ => ticks_ordered_unordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_unordered_after_selection() -> u64 {
  match selected_ordered_provider() {
    PROVIDER_TSC => super::x86_64::rdtsc(),
    PROVIDER_TIMEKEEP => ordered_timekeep_clock_monotonic_unordered(),
    PROVIDER_CLOCK_MONOTONIC_MFENCE
    | PROVIDER_CLOCK_MONOTONIC_CPUID
    | PROVIDER_CLOCK_MONOTONIC_LFENCE
    | PROVIDER_CLOCK_MONOTONIC_RDTSCP
    | PROVIDER_CLOCK_MONOTONIC_OS_OWNED
    | PROVIDER_CLOCK_MONOTONIC_SERIALIZE => clock_monotonic(),
    _ => clock_monotonic_syscall(),
  }
}

#[inline(always)]
fn ordered_clock_monotonic_lfence() -> u64 {
  intel_load_barrier();
  clock_monotonic()
}

#[inline(always)]
fn ordered_clock_monotonic_syscall_lfence() -> u64 {
  intel_load_barrier();
  clock_monotonic_syscall()
}

#[inline(always)]
fn ordered_clock_monotonic_mfence() -> u64 {
  full_memory_barrier();
  clock_monotonic()
}

#[inline(always)]
fn ordered_clock_monotonic_syscall_mfence() -> u64 {
  full_memory_barrier();
  clock_monotonic_syscall()
}

#[inline(always)]
fn ordered_clock_monotonic_cpuid() -> u64 {
  cpuid_barrier();
  clock_monotonic()
}

#[inline(always)]
fn ordered_clock_monotonic_syscall_cpuid() -> u64 {
  cpuid_barrier();
  clock_monotonic_syscall()
}

#[inline(always)]
fn ordered_clock_monotonic_rdtscp() -> u64 {
  rdtscp_barrier();
  clock_monotonic()
}

#[inline(always)]
fn ordered_clock_monotonic_syscall_rdtscp() -> u64 {
  rdtscp_barrier();
  clock_monotonic_syscall()
}

#[inline(always)]
fn ordered_clock_monotonic_os_owned() -> u64 {
  // FreeBSD's amd64 vDSO selects an ordered TSC read (LFENCE, MFENCE, or
  // RDTSCP), a volatile HPET load, or the corresponding ordered paravirtual
  // counter sequence. The opaque libc call is the compiler barrier. This
  // candidate is admitted only on Intel because libc can fall back to a bare
  // SYSCALL when the vDSO protocol reports ENOSYS.
  clock_monotonic()
}

#[inline(always)]
fn ordered_clock_monotonic_syscall_os_owned() -> u64 {
  // This bare path is eligible only on Intel, whose SYSCALL contract orders
  // older instructions before kernel execution. Omitting `nomem` from the
  // inline asm supplies the matching compiler memory barrier.
  clock_monotonic_syscall()
}

#[inline(always)]
fn ordered_clock_monotonic_serialize() -> u64 {
  serialize_barrier();
  clock_monotonic()
}

#[inline(always)]
fn ordered_clock_monotonic_syscall_serialize() -> u64 {
  serialize_barrier();
  clock_monotonic_syscall()
}

#[inline(always)]
fn intel_load_barrier() {
  // SAFETY: x86_64 guarantees SSE2. Intel documents LFENCE as ordering prior
  // loads before subsequent instructions. Omitting `nomem` also makes this a
  // compiler barrier for the memory operations covered by OrderedInstant.
  unsafe { asm!("lfence", options(nostack, preserves_flags)) }
}

#[inline(always)]
fn full_memory_barrier() {
  // SAFETY: x86_64 guarantees SSE2 and therefore MFENCE. This is eligible only
  // on AMD, where the vendor contract orders the prior memory observation.
  unsafe { asm!("mfence", options(nostack, preserves_flags)) }
}

#[inline(always)]
fn cpuid_barrier() {
  // SAFETY: x86_64 guarantees CPUID. RSI temporarily preserves RBX, which LLVM
  // reserves for position-independent code. CPUID fully serializes execution;
  // omitting `nomem` also supplies the compiler barrier.
  unsafe {
    asm!(
      "mov rsi, rbx",
      "xor eax, eax",
      "cpuid",
      "mov rbx, rsi",
      lateout("rax") _,
      lateout("rcx") _,
      lateout("rdx") _,
      lateout("rsi") _,
      options(nostack),
    );
  }
}

#[inline(always)]
fn rdtscp_barrier() {
  // SAFETY: AMD eligibility requires CPUID's RDTSCP feature bit. RDTSCP orders
  // the prior observation and LFENCE prevents the following OS-clock read from
  // starting before the discarded RDTSCP read completes.
  unsafe {
    asm!(
      "rdtscp",
      "lfence",
      lateout("eax") _,
      lateout("edx") _,
      lateout("ecx") _,
      options(nostack, preserves_flags),
    );
  }
}

#[inline(always)]
fn serialize_barrier() {
  // SAFETY: eligibility requires CPUID.7.0:EDX[SERIALIZE]. Omitting `nomem`
  // also supplies the compiler memory barrier required by OrderedInstant.
  unsafe { asm!("serialize", options(nostack, preserves_flags)) }
}

#[inline(always)]
fn timekeep_clock_monotonic() -> u64 {
  let timekeep = TIMEKEEP_PTR.load(Ordering::Acquire) as *const VdsoTimekeep;
  timekeep_nanos(timekeep).unwrap_or_else(clock_monotonic)
}

#[inline(always)]
fn instant_timekeep_clock_monotonic() -> u64 {
  let timekeep = TIMEKEEP_PTR.load(Ordering::Acquire) as *const VdsoTimekeep;
  if let Some(nanos) = timekeep_nanos(timekeep) {
    return nanos;
  }

  retier_instant_timekeep();
  ticks_after_selection()
}

#[inline(always)]
fn ordered_timekeep_clock_monotonic() -> u64 {
  // The FreeBSD protocol supplies the hardware ordering for each supported
  // timecounter. This fence is the corresponding zero-instruction compiler
  // barrier for memory observed before an OrderedInstant read.
  compiler_fence(Ordering::SeqCst);
  let timekeep = TIMEKEEP_PTR.load(Ordering::Acquire) as *const VdsoTimekeep;
  if let Some(nanos) = timekeep_nanos(timekeep) {
    return nanos;
  }

  retier_ordered_timekeep();
  ticks_ordered_after_selection()
}

#[inline(always)]
fn ordered_timekeep_clock_monotonic_unordered() -> u64 {
  let timekeep = TIMEKEEP_PTR.load(Ordering::Acquire) as *const VdsoTimekeep;
  if let Some(nanos) = timekeep_nanos(timekeep) {
    return nanos;
  }

  retier_ordered_timekeep();
  ticks_ordered_unordered_after_selection()
}

#[inline(always)]
fn retier_instant_timekeep() {
  let fallback = instant_fallback_provider(INSTANT_TIMEKEEP_FALLBACK.load(Ordering::Acquire));
  let _ = retier_timekeep_provider(&INSTANT_PROVIDER, &TIMEKEEP_STATE, fallback);
}

#[inline(always)]
fn retier_ordered_timekeep() {
  let fallback = ordered_fallback_provider(ORDERED_TIMEKEEP_FALLBACK.load(Ordering::Acquire));
  let _ = retier_timekeep_provider(&ORDERED_PROVIDER, &TIMEKEEP_STATE, fallback);
}

#[inline(always)]
fn retier_timekeep_provider(provider: &AtomicU8, timekeep_state: &AtomicU8, fallback: u8) -> u8 {
  match provider.compare_exchange(PROVIDER_TIMEKEEP, fallback, Ordering::AcqRel, Ordering::Acquire)
  {
    Ok(_) => {
      // A direct reader can no longer be used after a timecounter or device
      // transition. The selected fallback has the same nanosecond domain, so
      // old and new samples remain comparable.
      timekeep_state.store(TIMEKEEP_UNAVAILABLE, Ordering::Release);
      fallback
    }
    Err(published) => published,
  }
}

const fn instant_fallback_provider(provider: u8) -> u8 {
  match provider {
    PROVIDER_CLOCK_MONOTONIC | PROVIDER_CLOCK_MONOTONIC_SYSCALL => provider,
    _ => PROVIDER_CLOCK_MONOTONIC,
  }
}

const fn ordered_fallback_provider(provider: u8) -> u8 {
  match provider {
    PROVIDER_CLOCK_MONOTONIC_MFENCE
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_MFENCE
    | PROVIDER_CLOCK_MONOTONIC_CPUID
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID
    | PROVIDER_CLOCK_MONOTONIC_LFENCE
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_LFENCE
    | PROVIDER_CLOCK_MONOTONIC_RDTSCP
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_RDTSCP
    | PROVIDER_CLOCK_MONOTONIC_OS_OWNED
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_OWNED
    | PROVIDER_CLOCK_MONOTONIC_SERIALIZE
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_SERIALIZE => provider,
    _ => PROVIDER_CLOCK_MONOTONIC_CPUID,
  }
}

fn timekeep_available() -> bool {
  super::select_same_domain_thread_owned_process_provider(
    &TIMEKEEP_STATE,
    TIMEKEEP_UNKNOWN,
    TIMEKEEP_SELECTING,
    &TIMEKEEP_OWNER_PID,
    &TIMEKEEP_OWNER_TID,
    TIMEKEEP_UNAVAILABLE,
    initialize_timekeep,
  ) == TIMEKEEP_READY
}

#[cold]
fn initialize_timekeep() -> u8 {
  let mut timekeep = ptr::null::<VdsoTimekeep>();
  let buffer_len = match libc::c_int::try_from(size_of::<*const VdsoTimekeep>()) {
    Ok(buffer_len) => buffer_len,
    Err(_) => return TIMEKEEP_UNAVAILABLE,
  };
  // SAFETY: `timekeep` is a writable pointer-sized output and AT_TIMEKEEP is
  // FreeBSD's public auxiliary-vector interface for this shared page.
  let status = unsafe {
    libc::elf_aux_info(libc::AT_TIMEKEEP, ptr::from_mut(&mut timekeep).cast(), buffer_len)
  };
  if status != 0 || timekeep.is_null() {
    return TIMEKEEP_UNAVAILABLE;
  }

  // SAFETY: elf_aux_info returned the kernel-created AT_TIMEKEEP mapping. All
  // subsequent pointer arithmetic is bounded by the ABI's four slots.
  let version = unsafe { ptr::read_volatile(ptr::addr_of!((*timekeep).tk_ver)) };
  if version != VDSO_TK_VERSION {
    return TIMEKEEP_UNAVAILABLE;
  }

  TIMEKEEP_TSC_MODE.store(detect_timekeep_tsc_mode(), Ordering::Release);
  for _ in 0..TIMEKEEP_RETRIES {
    let Some(timehands) = current_timehands(timekeep) else {
      continue;
    };
    // SAFETY: current_timehands bounds and aligns the selected ABI slot.
    let generation = unsafe { load_foreign_atomic_u32(ptr::addr_of!((*timehands).th_gen)) };
    if generation == 0 {
      continue;
    }
    // SAFETY: current_timehands bounds and aligns the selected ABI slot.
    let resources_ready = unsafe { initialize_timehands_resources(timehands) };
    fence(Ordering::Acquire);
    // Resource setup may cross a timecounter update. Only classify an
    // unsupported algorithm/device after validating the snapshot that named
    // it; mappings created for an older slot remain harmless and reusable.
    let stable_slot = current_timehands(timekeep) == Some(timehands);
    // SAFETY: same aligned generation field.
    let final_generation = unsafe { load_foreign_atomic_u32(ptr::addr_of!((*timehands).th_gen)) };
    if !stable_slot || final_generation != generation {
      continue;
    }
    if !resources_ready {
      return TIMEKEEP_UNAVAILABLE;
    }
    TIMEKEEP_PTR.store(timekeep as usize, Ordering::Release);
    if timekeep_nanos(timekeep).is_some() {
      return TIMEKEEP_READY;
    }
  }
  TIMEKEEP_UNAVAILABLE
}

fn current_timehands(timekeep: *const VdsoTimekeep) -> Option<*const VdsoTimehands> {
  if timekeep.is_null() {
    return None;
  }
  // SAFETY: callers pass the AT_TIMEKEEP mapping and tk_current is naturally
  // aligned. The kernel publishes it with a release store.
  let enabled = unsafe { ptr::read_volatile(ptr::addr_of!((*timekeep).tk_enabled)) };
  if enabled == 0 {
    return None;
  }
  // SAFETY: see above; the matching load is Acquire.
  let current = unsafe { load_foreign_atomic_u32(ptr::addr_of!((*timekeep).tk_current)) } as usize;
  if current >= VDSO_TIMEHANDS_COUNT {
    return None;
  }
  // SAFETY: VdsoTimekeep includes the ABI padding before the four-slot array,
  // and `current` was bounded above.
  Some(unsafe {
    timekeep
      .cast::<u8>()
      .add(size_of::<VdsoTimekeep>())
      .cast::<VdsoTimehands>()
      .add(current)
  })
}

unsafe fn initialize_timehands_resources(timehands: *const VdsoTimehands) -> bool {
  // SAFETY: the caller supplies a bounded slot in AT_TIMEKEEP.
  let algorithm = unsafe { ptr::read_volatile(ptr::addr_of!((*timehands).th_algo)) };
  match algorithm {
    VDSO_ALGO_TSC => true,
    VDSO_ALGO_HPET => {
      // SAFETY: same bounded timehands slot.
      let index = unsafe { ptr::read_volatile(ptr::addr_of!((*timehands).th_x86_hpet_idx)) };
      initialize_hpet(index)
    }
    VDSO_ALGO_HYPERV_REFTSC => initialize_hyperv_reftsc(),
    VDSO_ALGO_PVCLOCK => initialize_pvclock(),
    _ => false,
  }
}

fn initialize_hpet(index: u32) -> bool {
  let Ok(index) = usize::try_from(index) else {
    return false;
  };
  if index >= HPET_DEVICE_COUNT {
    return false;
  }
  if HPET_MAPS[index].load(Ordering::Acquire) != 0 {
    return true;
  }

  let mut path = *b"/dev/hpet0\0";
  path[9] = b'0' + u8::try_from(index).unwrap_or(0);
  let Some((mapping, mapped_len)) = map_read_only_device(path.as_ptr().cast(), None) else {
    return false;
  };
  if mapped_len < HPET_MAIN_COUNTER_OFFSET + size_of::<u32>() {
    // SAFETY: mapping and length came from a successful mmap above.
    unsafe { libc::munmap(mapping.cast_mut().cast(), mapped_len) };
    return false;
  }
  HPET_MAPS[index].store(mapping as usize, Ordering::Release);
  true
}

fn initialize_hyperv_reftsc() -> bool {
  if HYPERV_REFTSC_MAP.load(Ordering::Acquire) != 0 {
    return true;
  }
  let Some((mapping, mapped_len)) = map_read_only_device(c"/dev/hv_tsc".as_ptr(), None) else {
    return false;
  };
  if mapped_len < size_of::<HypervRefTsc>() {
    // SAFETY: mapping and length came from a successful mmap above.
    unsafe { libc::munmap(mapping.cast_mut().cast(), mapped_len) };
    return false;
  }
  HYPERV_REFTSC_MAP.store(mapping as usize, Ordering::Release);
  true
}

fn initialize_pvclock() -> bool {
  if PVCLOCK_MAP.load(Ordering::Acquire) != 0 && PVCLOCK_CPU_COUNT.load(Ordering::Acquire) != 0 {
    return true;
  }

  let mut cpu_count: libc::c_int = 0;
  let buffer_len = match libc::c_int::try_from(size_of::<libc::c_int>()) {
    Ok(buffer_len) => buffer_len,
    Err(_) => return false,
  };
  // SAFETY: cpu_count is a writable c_int output for AT_NCPUS.
  if unsafe { libc::elf_aux_info(libc::AT_NCPUS, ptr::from_mut(&mut cpu_count).cast(), buffer_len) }
    != 0
    || cpu_count <= 0
  {
    return false;
  }
  let Ok(cpu_count_usize) = usize::try_from(cpu_count) else {
    return false;
  };
  let Some(mapped_len) = cpu_count_usize.checked_mul(size_of::<PvclockVcpuTimeInfo>()) else {
    return false;
  };
  let Some((mapping, _)) = map_read_only_device(c"/dev/pvclock".as_ptr(), Some(mapped_len)) else {
    return false;
  };
  PVCLOCK_CPU_COUNT.store(cpu_count as u32, Ordering::Release);
  PVCLOCK_MAP.store(mapping as usize, Ordering::Release);
  true
}

fn map_read_only_device(
  path: *const libc::c_char,
  requested_len: Option<usize>,
) -> Option<(*const u8, usize)> {
  // SAFETY: callers pass a NUL-terminated path and no O_CREAT mode is needed.
  let descriptor = unsafe { libc::open(path, libc::O_RDONLY | libc::O_CLOEXEC) };
  if descriptor < 0 {
    return None;
  }
  let mapped_len = requested_len.unwrap_or_else(|| {
    // SAFETY: getpagesize has no preconditions.
    usize::try_from(unsafe { libc::getpagesize() }).unwrap_or(0)
  });
  if mapped_len == 0 {
    // SAFETY: descriptor was returned by open.
    unsafe { libc::close(descriptor) };
    return None;
  }
  // SAFETY: the descriptor names a read-only mmap-capable FreeBSD clock
  // device; offset zero and the requested lengths match the kernel ABI.
  let mapping = unsafe {
    libc::mmap(ptr::null_mut(), mapped_len, libc::PROT_READ, libc::MAP_SHARED, descriptor, 0)
  };
  // SAFETY: descriptor was returned by open and mmap retains its own mapping.
  unsafe { libc::close(descriptor) };
  if mapping == libc::MAP_FAILED || mapping.is_null() {
    return None;
  }
  Some((mapping.cast(), mapped_len))
}

#[inline(always)]
fn timekeep_nanos(timekeep: *const VdsoTimekeep) -> Option<u64> {
  if timekeep.is_null() {
    return None;
  }
  for _ in 0..TIMEKEEP_RETRIES {
    // SAFETY: the selected pointer is the stable AT_TIMEKEEP mapping.
    let (version, enabled) = unsafe {
      (
        ptr::read_volatile(ptr::addr_of!((*timekeep).tk_ver)),
        ptr::read_volatile(ptr::addr_of!((*timekeep).tk_enabled)),
      )
    };
    if version != VDSO_TK_VERSION || enabled == 0 {
      return None;
    }
    // SAFETY: tk_current is naturally aligned and kernel-published atomically.
    let current = unsafe { load_foreign_atomic_u32(ptr::addr_of!((*timekeep).tk_current)) };
    let Ok(current_index) = usize::try_from(current) else {
      return None;
    };
    if current_index >= VDSO_TIMEHANDS_COUNT {
      return None;
    }
    // SAFETY: current_index is bounded to the four ABI slots.
    let timehands = unsafe {
      timekeep
        .cast::<u8>()
        .add(size_of::<VdsoTimekeep>())
        .cast::<VdsoTimehands>()
        .add(current_index)
    };
    // SAFETY: the slot is aligned and its generation is atomically published.
    let generation = unsafe { load_foreign_atomic_u32(ptr::addr_of!((*timehands).th_gen)) };
    if generation == 0 {
      continue;
    }

    // SAFETY: the generation's acquire load makes this bounded slot readable;
    // volatile loads model updates performed by the kernel outside Rust.
    let (offset, scale, offset_count, counter_mask) = unsafe {
      (
        ptr::read_volatile(ptr::addr_of!((*timehands).th_offset)),
        ptr::read_volatile(ptr::addr_of!((*timehands).th_scale)),
        ptr::read_volatile(ptr::addr_of!((*timehands).th_offset_count)),
        ptr::read_volatile(ptr::addr_of!((*timehands).th_counter_mask)),
      )
    };
    // SAFETY: timehands remains inside the current ABI mapping.
    let counter = unsafe { read_timehands_counter(timehands) }?;
    let delta = counter.wrapping_sub(offset_count) & counter_mask;
    let nanos = scaled_bintime_nanos(offset, scale, delta)?;

    fence(Ordering::Acquire);
    // SAFETY: both validation words are aligned kernel atomics.
    let (final_current, final_generation) = unsafe {
      (
        load_foreign_atomic_u32(ptr::addr_of!((*timekeep).tk_current)),
        load_foreign_atomic_u32(ptr::addr_of!((*timehands).th_gen)),
      )
    };
    if final_current == current && final_generation == generation {
      return Some(nanos);
    }
  }
  None
}

unsafe fn load_foreign_atomic_u32(value: *const u32) -> u32 {
  // SAFETY: every caller supplies a naturally aligned field that FreeBSD
  // publishes with its 32-bit atomic API.
  unsafe { (*value.cast::<AtomicU32>()).load(Ordering::Acquire) }
}

unsafe fn read_timehands_counter(timehands: *const VdsoTimehands) -> Option<u32> {
  // SAFETY: caller supplies a bounded AT_TIMEKEEP slot.
  let algorithm = unsafe { ptr::read_volatile(ptr::addr_of!((*timehands).th_algo)) };
  match algorithm {
    VDSO_ALGO_TSC => {
      // SAFETY: same bounded slot.
      let shift = unsafe { ptr::read_volatile(ptr::addr_of!((*timehands).th_x86_shift)) };
      if shift > 31 {
        return None;
      }
      let tsc = read_timekeep_tsc();
      Some(if shift == 0 { tsc as u32 } else { (tsc >> shift) as u32 })
    }
    VDSO_ALGO_HPET => {
      // SAFETY: same bounded slot.
      let index = unsafe { ptr::read_volatile(ptr::addr_of!((*timehands).th_x86_hpet_idx)) };
      let index = usize::try_from(index).ok()?;
      if index >= HPET_DEVICE_COUNT {
        return None;
      }
      let mapping = HPET_MAPS[index].load(Ordering::Acquire) as *const u8;
      if mapping.is_null() {
        return None;
      }
      // SAFETY: HPET mappings cover at least one page and the main counter is
      // a naturally aligned 32-bit MMIO register at offset 0xf0.
      Some(unsafe { ptr::read_volatile(mapping.add(HPET_MAIN_COUNTER_OFFSET).cast::<u32>()) })
    }
    VDSO_ALGO_HYPERV_REFTSC => read_hyperv_reftsc(),
    VDSO_ALGO_PVCLOCK => {
      // SAFETY: same bounded slot.
      unsafe { read_pvclock(timehands) }
    }
    _ => None,
  }
}

fn read_hyperv_reftsc() -> Option<u32> {
  let mapping = HYPERV_REFTSC_MAP.load(Ordering::Acquire) as *const HypervRefTsc;
  if mapping.is_null() {
    return None;
  }
  for _ in 0..TIMEKEEP_RETRIES {
    // SAFETY: the mapping covers HypervRefTsc and the sequence is a naturally
    // aligned kernel atomic.
    let sequence = unsafe { load_foreign_atomic_u32(ptr::addr_of!((*mapping).tsc_seq)) };
    if sequence == 0 {
      return None;
    }
    // SAFETY: sequence acquire protects the remaining mapped fields.
    let (scale, offset) = unsafe {
      (
        ptr::read_volatile(ptr::addr_of!((*mapping).tsc_scale)),
        ptr::read_volatile(ptr::addr_of!((*mapping).tsc_ofs)),
      )
    };
    full_memory_barrier();
    let tsc = super::x86_64::rdtsc();
    let high_product = ((u128::from(tsc) * u128::from(scale)) >> 64) as u64;
    let value = high_product.wrapping_add(offset as u64);
    fence(Ordering::Acquire);
    // SAFETY: same aligned sequence field.
    if unsafe { load_foreign_atomic_u32(ptr::addr_of!((*mapping).tsc_seq)) } == sequence {
      return Some(value as u32);
    }
  }
  None
}

unsafe fn read_pvclock(timehands: *const VdsoTimehands) -> Option<u32> {
  let mapping = PVCLOCK_MAP.load(Ordering::Acquire) as *const PvclockVcpuTimeInfo;
  let cpu_count = usize::try_from(PVCLOCK_CPU_COUNT.load(Ordering::Acquire)).ok()?;
  if mapping.is_null() || cpu_count == 0 {
    return None;
  }
  // SAFETY: caller supplies a bounded AT_TIMEKEEP slot.
  let (stable_mask, last_system_time) = unsafe {
    (
      ptr::read_volatile(ptr::addr_of!((*timehands).th_x86_pvc_stable_mask)),
      ptr::read_volatile(ptr::addr_of!((*timehands).th_x86_pvc_last_systime)),
    )
  };

  for _ in 0..TIMEKEEP_RETRIES {
    let mut cpu_before = 0_u32;
    let mut cpu_after = 0_u32;
    let mut info = mapping;
    // SAFETY: cpu_count is nonzero, so slot zero is mapped.
    let (initial_version, initial_flags) = unsafe {
      (
        load_foreign_atomic_u32(ptr::addr_of!((*info).version)),
        ptr::read_volatile(ptr::addr_of!((*info).flags)),
      )
    };
    let stable = initial_flags & stable_mask != 0;
    let (version, tsc) = if stable {
      (initial_version, read_timekeep_tsc())
    } else {
      if TIMEKEEP_TSC_MODE.load(Ordering::Acquire) != TSC_MODE_RDTSCP {
        return None;
      }
      let (_, before) = read_rdtscp_aux();
      cpu_before = before;
      let index = usize::try_from(cpu_before).ok()?;
      if index >= cpu_count {
        return None;
      }
      // SAFETY: index is bounded by the mapped AT_NCPUS length.
      info = unsafe { mapping.add(index) };
      // SAFETY: bounded mapped slot.
      let version = unsafe { load_foreign_atomic_u32(ptr::addr_of!((*info).version)) };
      let (tsc, after) = read_rdtscp_aux();
      cpu_after = after;
      (version, tsc)
    };

    // SAFETY: version acquire protects these fields in the bounded slot.
    let (timestamp, system_time, multiplier, shift) = unsafe {
      (
        ptr::read_volatile(ptr::addr_of!((*info).tsc_timestamp)),
        ptr::read_volatile(ptr::addr_of!((*info).system_time)),
        ptr::read_volatile(ptr::addr_of!((*info).tsc_to_system_mul)),
        ptr::read_volatile(ptr::addr_of!((*info).tsc_shift)),
      )
    };
    let scaled = pvclock_scale_delta(tsc.wrapping_sub(timestamp), multiplier, shift)?;
    let nanos = system_time.wrapping_add(scaled).max(last_system_time);
    fence(Ordering::Acquire);
    // SAFETY: same bounded version field.
    let final_version = unsafe { load_foreign_atomic_u32(ptr::addr_of!((*info).version)) };
    if version & 1 == 0 && final_version == version && (stable || cpu_before == cpu_after) {
      return Some(nanos as u32);
    }
  }
  None
}

fn pvclock_scale_delta(delta: u64, multiplier: u32, shift: i8) -> Option<u64> {
  let shifted = if shift < 0 {
    delta.checked_shr(u32::from(shift.unsigned_abs()))?
  } else {
    delta.checked_shl(u32::from(shift as u8))?
  };
  Some(((u128::from(shifted) * u128::from(multiplier)) >> 32) as u64)
}

fn scaled_bintime_nanos(offset: Bintime, scale: u64, delta: u32) -> Option<u64> {
  if offset.sec < 0 {
    return None;
  }
  let scaled = u128::from(scale) * u128::from(delta);
  let fraction = u128::from(offset.frac) + (scaled & u128::from(u64::MAX));
  let carry = fraction >> 64;
  let seconds = u128::from(u64::try_from(offset.sec).ok()?) + (scaled >> 64) + carry;
  let nanos_fraction = ((fraction as u64 as u128) * 1_000_000_000_u128) >> 64;
  let nanos = seconds.checked_mul(1_000_000_000)?.checked_add(nanos_fraction)?;
  u64::try_from(nanos).ok()
}

#[allow(unused_unsafe)] // Supported rustc versions differ on whether __cpuid is unsafe.
fn detect_timekeep_tsc_mode() -> u8 {
  const AMD: (u32, u32, u32) = (0x6874_7541, 0x6974_6e65, 0x444d_4163);
  const HYGON: (u32, u32, u32) = (0x6f67_7948, 0x6e65_476e, 0x656e_6975);
  // SAFETY: x86_64 guarantees CPUID leaf zero and basic leaf one.
  let (basic, feature, extended) = unsafe { (__cpuid(0), __cpuid(1), __cpuid(0x8000_0000)) };
  let extended_features = if extended.eax >= 0x8000_0001 {
    // SAFETY: the maximum extended leaf confirms 0x8000_0001 before the read.
    unsafe { __cpuid(0x8000_0001) }.edx
  } else {
    0
  };
  let has_rdtscp = extended_features & (1 << 27) != 0;
  if has_rdtscp {
    return TSC_MODE_RDTSCP;
  }
  if feature.edx & (1 << 26) == 0 {
    return TSC_MODE_PLAIN;
  }
  let vendor = (basic.ebx, basic.edx, basic.ecx);
  if vendor == AMD || vendor == HYGON { TSC_MODE_AMD_MFENCE } else { TSC_MODE_INTEL_LFENCE }
}

#[inline(always)]
fn read_timekeep_tsc() -> u64 {
  match TIMEKEEP_TSC_MODE.load(Ordering::Relaxed) {
    TSC_MODE_RDTSCP => read_rdtscp_aux().0,
    TSC_MODE_AMD_MFENCE => {
      full_memory_barrier();
      super::x86_64::rdtsc()
    }
    TSC_MODE_INTEL_LFENCE => {
      intel_load_barrier();
      super::x86_64::rdtsc()
    }
    _ => super::x86_64::rdtsc(),
  }
}

#[inline(always)]
fn read_rdtscp_aux() -> (u64, u32) {
  let low: u32;
  let high: u32;
  let auxiliary: u32;
  // SAFETY: callers execute this only after CPUID reports RDTSCP support.
  unsafe {
    asm!(
      "rdtscp",
      out("eax") low,
      out("edx") high,
      out("ecx") auxiliary,
      options(nostack, preserves_flags),
    );
  }
  (u64::from(low) | (u64::from(high) << 32), auxiliary)
}

#[inline]
pub fn instant_frequency() -> u64 {
  if instant_uses_tsc() { TSC_FREQUENCY.load(Ordering::Acquire).max(1) } else { 1_000_000_000 }
}

#[inline]
pub fn ordered_frequency() -> u64 {
  if ordered_uses_tsc() { TSC_FREQUENCY.load(Ordering::Acquire).max(1) } else { 1_000_000_000 }
}

#[inline]
pub(crate) fn instant_uses_tsc() -> bool {
  selected_instant_provider() == PROVIDER_TSC
}

#[inline]
pub(crate) fn instant_read_cost() -> crate::ThreadCpuReadCost {
  instant_read_cost_for(selected_instant_provider())
}

const fn instant_read_cost_for(provider: u8) -> crate::ThreadCpuReadCost {
  match provider {
    PROVIDER_TSC | PROVIDER_TIMEKEEP => crate::ThreadCpuReadCost::Inline,
    // The libc ABI may use a userspace fast path or enter the kernel. Only the
    // selected direct routes are guaranteed to remain in userspace during the
    // environment in which they won their tournaments.
    _ => crate::ThreadCpuReadCost::SystemCall,
  }
}

#[cfg(test)]
#[test]
fn instant_read_cost_only_marks_guaranteed_userspace_paths_inline() {
  assert_eq!(instant_read_cost_for(PROVIDER_TSC), crate::ThreadCpuReadCost::Inline);
  assert_eq!(instant_read_cost_for(PROVIDER_TIMEKEEP), crate::ThreadCpuReadCost::Inline);
  assert_eq!(instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC), crate::ThreadCpuReadCost::SystemCall,);
  assert_eq!(
    instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC_SYSCALL),
    crate::ThreadCpuReadCost::SystemCall,
  );
}

#[cfg(test)]
#[test]
fn timekeep_retirement_updates_the_selected_read_cost() {
  let provider = AtomicU8::new(PROVIDER_TIMEKEEP);
  let timekeep_state = AtomicU8::new(TIMEKEEP_READY);
  let selected =
    retier_timekeep_provider(&provider, &timekeep_state, PROVIDER_CLOCK_MONOTONIC_SYSCALL);

  assert_eq!(selected, PROVIDER_CLOCK_MONOTONIC_SYSCALL);
  assert_eq!(provider.load(Ordering::Acquire), PROVIDER_CLOCK_MONOTONIC_SYSCALL);
  assert_eq!(timekeep_state.load(Ordering::Acquire), TIMEKEEP_UNAVAILABLE);
  assert_eq!(instant_read_cost_for(selected), crate::ThreadCpuReadCost::SystemCall);
}

#[cfg(test)]
#[test]
fn timekeep_retirement_keeps_the_measured_ordered_route() {
  let provider = AtomicU8::new(PROVIDER_TIMEKEEP);
  let timekeep_state = AtomicU8::new(TIMEKEEP_READY);
  let selected =
    retier_timekeep_provider(&provider, &timekeep_state, PROVIDER_CLOCK_MONOTONIC_LFENCE);

  assert_eq!(ordered_fallback_provider(selected), PROVIDER_CLOCK_MONOTONIC_LFENCE);
  assert_eq!(provider.load(Ordering::Acquire), PROVIDER_CLOCK_MONOTONIC_LFENCE);
  assert_eq!(timekeep_state.load(Ordering::Acquire), TIMEKEEP_UNAVAILABLE);
}

#[inline]
pub(crate) fn ordered_uses_tsc() -> bool {
  selected_ordered_provider() == PROVIDER_TSC
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
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_MFENCE
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
  let frequency = kernel_eligible_tsc_frequency();
  let tsc_eligible = frequency.is_some();
  let timekeep_available = timekeep_available();
  if let Some(frequency) = frequency {
    TSC_FREQUENCY.store(frequency, Ordering::Release);
  }
  let bare_syscall_os_owned = ordered && bare_syscall_os_owned_eligible();
  let (clock_path, syscall_path) = if ordered {
    let barriers = eligible_ordered_barriers();
    let fallback_barriers = ordered_fallback_barriers(barriers, bare_syscall_os_owned);
    (select_ordered_path(false, fallback_barriers), select_ordered_path(true, fallback_barriers))
  } else {
    (unbarriered_path(PROVIDER_CLOCK_MONOTONIC), unbarriered_path(PROVIDER_CLOCK_MONOTONIC_SYSCALL))
  };
  let clock_provider = clock_path.provider;
  let syscall_provider = syscall_path.provider;
  let (samples, candidate_providers, candidate_count) =
    measure_candidates(ordered, tsc_eligible, timekeep_available, clock_provider, syscall_provider);
  #[cfg(not(feature = "bench-internal"))]
  let _ = (candidate_providers, candidate_count);
  let fallback_decision = prefer_challenger(samples.syscall, samples.clock);
  let fallback =
    if fallback_decision.challenger_selected { syscall_provider } else { clock_provider };
  if ordered {
    ORDERED_TIMEKEEP_FALLBACK.store(fallback, Ordering::Release);
  } else {
    INSTANT_TIMEKEEP_FALLBACK.store(fallback, Ordering::Release);
  }
  let fallback_samples = if fallback == syscall_provider { samples.syscall } else { samples.clock };
  let timekeep_decision = if timekeep_available {
    prefer_challenger(samples.timekeep, fallback_samples)
  } else {
    empty_decision()
  };
  let non_tsc_provider =
    if timekeep_decision.challenger_selected { PROVIDER_TIMEKEEP } else { fallback };
  let non_tsc_samples =
    if non_tsc_provider == PROVIDER_TIMEKEEP { samples.timekeep } else { fallback_samples };
  let direct_decision = if tsc_eligible {
    prefer_challenger(samples.direct, non_tsc_samples)
  } else {
    empty_decision()
  };
  let provider = if direct_decision.challenger_selected { PROVIDER_TSC } else { non_tsc_provider };
  #[cfg(feature = "bench-internal")]
  let selected_path = if fallback == syscall_provider { syscall_path } else { clock_path };
  #[cfg(feature = "bench-internal")]
  let clock_barrier_evidence = ordered_path_evidence(clock_path);
  #[cfg(feature = "bench-internal")]
  let syscall_barrier_evidence = ordered_path_evidence(syscall_path);

  #[cfg(feature = "bench-internal")]
  publish_evidence(ordered, ProbeEvidence {
    candidate_count,
    candidate_providers,
    tsc_eligible,
    timekeep_available,
    reads_per_batch: PROBE_READS,
    direct_batches_ns: samples.direct,
    timekeep_batches_ns: samples.timekeep,
    clock_batches_ns: samples.clock,
    syscall_batches_ns: samples.syscall,
    direct_median_ns: median(samples.direct),
    timekeep_median_ns: median(samples.timekeep),
    clock_median_ns: median(samples.clock),
    syscall_median_ns: median(samples.syscall),
    fallback_allowance_ns: fallback_decision.allowance,
    fallback_decisive_wins: fallback_decision.decisive_wins,
    timekeep_allowance_ns: timekeep_decision.allowance,
    timekeep_decisive_wins: timekeep_decision.decisive_wins,
    direct_allowance_ns: direct_decision.allowance,
    direct_decisive_wins: direct_decision.decisive_wins,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    selected_provider: provider_from_raw(provider),
    selected_provider_name: if ordered {
      ordered_bench_primitive(provider).name
    } else {
      instant_bench_primitive(provider).name
    },
    ordered_os_barrier: if provider == PROVIDER_TIMEKEEP {
      "freebsd_timehands_protocol"
    } else {
      ordered_barrier_name_for_provider(selected_path.provider)
    },
    ordered_bare_syscall_os_owned_eligible: bare_syscall_os_owned,
    ordered_bare_syscall_os_owned_basis: if ordered {
      if bare_syscall_os_owned {
        "eligible_intel_syscall_ordering_contract"
      } else {
        "ineligible_no_amd_or_unknown_vendor_syscall_ordering_contract"
      }
    } else {
      "not_applicable"
    },
    ordered_fast_barrier_candidate: selected_path.fast_candidate,
    ordered_fast_barrier_batches_ns: selected_path.fast_batches_ns,
    ordered_mfence_batches_ns: selected_path.baseline_batches_ns,
    ordered_barrier_allowance_ns: selected_path.decision.allowance,
    ordered_barrier_decisive_wins: selected_path.decision.decisive_wins,
    ordered_baseline_barrier: selected_path.baseline_barrier,
    ordered_clock_fast_barrier_candidate: clock_path.fast_candidate,
    ordered_clock_fast_batches_ns: clock_path.fast_batches_ns,
    ordered_clock_baseline_batches_ns: clock_path.baseline_batches_ns,
    ordered_clock_barrier_allowance_ns: clock_path.decision.allowance,
    ordered_clock_barrier_decisive_wins: clock_path.decision.decisive_wins,
    ordered_syscall_fast_barrier_candidate: syscall_path.fast_candidate,
    ordered_syscall_fast_batches_ns: syscall_path.fast_batches_ns,
    ordered_syscall_baseline_batches_ns: syscall_path.baseline_batches_ns,
    ordered_syscall_barrier_allowance_ns: syscall_path.decision.allowance,
    ordered_syscall_barrier_decisive_wins: syscall_path.decision.decisive_wins,
    ordered_barrier_candidate_count: clock_path.candidate_count,
    ordered_barrier_candidate_names: clock_barrier_evidence.candidate_names,
    ordered_clock_candidate_providers: ordered_candidate_providers(false, clock_path),
    ordered_syscall_barrier_candidate_count: syscall_path.candidate_count,
    ordered_syscall_barrier_candidate_names: syscall_barrier_evidence.candidate_names,
    ordered_syscall_candidate_providers: ordered_candidate_providers(true, syscall_path),
    ordered_clock_barrier_candidate_batches_ns: clock_path.candidate_batches_ns,
    ordered_clock_barrier_candidate_medians_ns: clock_barrier_evidence.candidate_medians_ns,
    ordered_clock_barrier_decision_count: clock_path.decision_count,
    ordered_clock_barrier_challengers: clock_barrier_evidence.challengers,
    ordered_clock_barrier_incumbents: clock_barrier_evidence.incumbents,
    ordered_clock_barrier_winners: clock_barrier_evidence.winners,
    ordered_clock_barrier_allowances_ns: clock_barrier_evidence.allowances_ns,
    ordered_clock_barrier_tournament_decisive_wins: clock_barrier_evidence.decisive_wins,
    ordered_clock_barrier_challenger_selected: clock_barrier_evidence.challenger_selected,
    ordered_syscall_barrier_candidate_batches_ns: syscall_path.candidate_batches_ns,
    ordered_syscall_barrier_candidate_medians_ns: syscall_barrier_evidence.candidate_medians_ns,
    ordered_syscall_barrier_decision_count: syscall_path.decision_count,
    ordered_syscall_barrier_challengers: syscall_barrier_evidence.challengers,
    ordered_syscall_barrier_incumbents: syscall_barrier_evidence.incumbents,
    ordered_syscall_barrier_winners: syscall_barrier_evidence.winners,
    ordered_syscall_barrier_allowances_ns: syscall_barrier_evidence.allowances_ns,
    ordered_syscall_barrier_tournament_decisive_wins: syscall_barrier_evidence.decisive_wins,
    ordered_syscall_barrier_challenger_selected: syscall_barrier_evidence.challenger_selected,
  });

  provider
}

#[allow(unused_unsafe)] // Supported rustc versions differ on whether __cpuid is unsafe.
fn eligible_ordered_barriers() -> OrderedBarrierCandidates {
  const INTEL: (u32, u32, u32) = (0x756e_6547, 0x4965_6e69, 0x6c65_746e);
  const AMD: (u32, u32, u32) = (0x6874_7541, 0x6974_6e65, 0x444d_4163);
  const HYGON: (u32, u32, u32) = (0x6f67_7948, 0x6e65_476e, 0x656e_6975);

  // SAFETY: x86_64 guarantees CPUID leaves zero and one.
  let (basic, feature, extended) = unsafe { (__cpuid(0), __cpuid(1), __cpuid(0x8000_0000)) };
  let vendor = (basic.ebx, basic.edx, basic.ecx);
  let has_sse2 = feature.edx & (1 << 26) != 0;
  // SAFETY: each maximum-leaf check precedes its corresponding CPUID read.
  let (extended_features, amd_lfence_features, structured_features) = unsafe {
    (
      if extended.eax >= 0x8000_0001 { __cpuid(0x8000_0001).edx } else { 0 },
      if extended.eax >= 0x8000_0021 { __cpuid(0x8000_0021).eax } else { 0 },
      if basic.eax >= 7 { __cpuid_count(7, 0).edx } else { 0 },
    )
  };
  let has_rdtscp = extended_features & (1 << 27) != 0;
  let has_amd_serializing_lfence = has_sse2 && amd_lfence_features & (1 << 2) != 0;
  let has_serialize = structured_features & (1 << 14) != 0;

  let mut barriers = OrderedBarrierCandidates::conservative();
  barriers.push(ORDERED_BARRIER_OS_OWNED);
  if vendor == INTEL && has_sse2 {
    barriers.push(ORDERED_BARRIER_LFENCE);
  } else if vendor == AMD || vendor == HYGON {
    if has_amd_serializing_lfence {
      barriers.push(ORDERED_BARRIER_LFENCE);
    }
    if has_sse2 {
      barriers.push(ORDERED_BARRIER_MFENCE);
    }
  }
  if has_rdtscp {
    barriers.push(ORDERED_BARRIER_RDTSCP);
  }
  if has_serialize {
    barriers.push(ORDERED_BARRIER_SERIALIZE);
  }
  barriers
}

fn ordered_fallback_barriers(
  barriers: OrderedBarrierCandidates,
  os_owned_eligible: bool,
) -> OrderedBarrierCandidates {
  if os_owned_eligible {
    return barriers;
  }
  let mut filtered = OrderedBarrierCandidates::conservative();
  for &barrier in &barriers.barriers[1..barriers.count] {
    if barrier != ORDERED_BARRIER_OS_OWNED {
      filtered.push(barrier);
    }
  }
  filtered
}

#[allow(unused_unsafe)] // Supported rustc versions differ on whether __cpuid is unsafe.
fn bare_syscall_os_owned_eligible() -> bool {
  const INTEL: (u32, u32, u32) = (0x756e_6547, 0x4965_6e69, 0x6c65_746e);
  // SAFETY: x86_64 guarantees CPUID leaf zero.
  let basic = unsafe { __cpuid(0) };
  (basic.ebx, basic.edx, basic.ecx) == INTEL
}

fn unbarriered_path(provider: u8) -> OrderedPathSelection {
  OrderedPathSelection {
    provider,
    #[cfg(feature = "bench-internal")]
    fast_candidate: "not_applicable",
    #[cfg(feature = "bench-internal")]
    fast_batches_ns: [0; PROBE_BATCHES],
    #[cfg(feature = "bench-internal")]
    baseline_barrier: "not_applicable",
    #[cfg(feature = "bench-internal")]
    baseline_batches_ns: [0; PROBE_BATCHES],
    #[cfg(feature = "bench-internal")]
    decision: empty_decision(),
    #[cfg(feature = "bench-internal")]
    candidate_count: 0,
    #[cfg(feature = "bench-internal")]
    candidate_barriers: [ORDERED_BARRIER_CPUID; ORDERED_BARRIER_CANDIDATES],
    #[cfg(feature = "bench-internal")]
    candidate_batches_ns: [[0; PROBE_BATCHES]; ORDERED_BARRIER_CANDIDATES],
    #[cfg(feature = "bench-internal")]
    decision_count: 0,
    #[cfg(feature = "bench-internal")]
    challengers: [ORDERED_BARRIER_CPUID; ORDERED_BARRIER_DECISIONS],
    #[cfg(feature = "bench-internal")]
    incumbents: [ORDERED_BARRIER_CPUID; ORDERED_BARRIER_DECISIONS],
    #[cfg(feature = "bench-internal")]
    winners: [ORDERED_BARRIER_CPUID; ORDERED_BARRIER_DECISIONS],
    #[cfg(feature = "bench-internal")]
    decisions: [empty_decision(); ORDERED_BARRIER_DECISIONS],
  }
}

#[allow(clippy::needless_range_loop)] // Alternating rows deliberately index every candidate column.
fn select_ordered_path(syscall: bool, barriers: OrderedBarrierCandidates) -> OrderedPathSelection {
  for &barrier in &barriers.barriers[..barriers.count] {
    warm_candidate(true, provider_for_barrier(syscall, barrier));
  }
  let mut batches_ns = [[u64::MAX; PROBE_BATCHES]; ORDERED_BARRIER_CANDIDATES];
  for sample in 0..PROBE_BATCHES {
    for offset in 0..barriers.count {
      let index = (sample + offset) % barriers.count;
      let provider = provider_for_barrier(syscall, barriers.barriers[index]);
      batches_ns[index][sample] = measure_batch(true, provider).unwrap_or(u64::MAX);
    }
  }
  let mut selected_index = 0;
  #[cfg(feature = "bench-internal")]
  let mut challengers = [ORDERED_BARRIER_CPUID; ORDERED_BARRIER_DECISIONS];
  #[cfg(feature = "bench-internal")]
  let mut incumbents = [ORDERED_BARRIER_CPUID; ORDERED_BARRIER_DECISIONS];
  #[cfg(feature = "bench-internal")]
  let mut winners = [ORDERED_BARRIER_CPUID; ORDERED_BARRIER_DECISIONS];
  #[cfg(feature = "bench-internal")]
  let mut decisions = [empty_decision(); ORDERED_BARRIER_DECISIONS];
  for challenger_index in 1..barriers.count {
    let decision = prefer_challenger(batches_ns[challenger_index], batches_ns[selected_index]);
    #[cfg(feature = "bench-internal")]
    {
      let slot = challenger_index - 1;
      challengers[slot] = barriers.barriers[challenger_index];
      incumbents[slot] = barriers.barriers[selected_index];
      decisions[slot] = decision;
    }
    if decision.challenger_selected {
      selected_index = challenger_index;
    }
    #[cfg(feature = "bench-internal")]
    {
      winners[challenger_index - 1] = barriers.barriers[selected_index];
    }
  }
  #[cfg(feature = "bench-internal")]
  let first_decision = if barriers.count > 1 { decisions[0] } else { empty_decision() };
  OrderedPathSelection {
    provider: provider_for_barrier(syscall, barriers.barriers[selected_index]),
    #[cfg(feature = "bench-internal")]
    fast_candidate: if barriers.count > 1 { barrier_name(barriers.barriers[1]) } else { "none" },
    #[cfg(feature = "bench-internal")]
    fast_batches_ns: if barriers.count > 1 { batches_ns[1] } else { [0; PROBE_BATCHES] },
    #[cfg(feature = "bench-internal")]
    baseline_barrier: barrier_name(barriers.barriers[0]),
    #[cfg(feature = "bench-internal")]
    baseline_batches_ns: batches_ns[0],
    #[cfg(feature = "bench-internal")]
    decision: first_decision,
    #[cfg(feature = "bench-internal")]
    candidate_count: barriers.count,
    #[cfg(feature = "bench-internal")]
    candidate_barriers: barriers.barriers,
    #[cfg(feature = "bench-internal")]
    candidate_batches_ns: batches_ns,
    #[cfg(feature = "bench-internal")]
    decision_count: barriers.count.saturating_sub(1),
    #[cfg(feature = "bench-internal")]
    challengers,
    #[cfg(feature = "bench-internal")]
    incumbents,
    #[cfg(feature = "bench-internal")]
    winners,
    #[cfg(feature = "bench-internal")]
    decisions,
  }
}

const fn provider_for_barrier(syscall: bool, barrier: u8) -> u8 {
  match (syscall, barrier) {
    (false, ORDERED_BARRIER_MFENCE) => PROVIDER_CLOCK_MONOTONIC_MFENCE,
    (true, ORDERED_BARRIER_MFENCE) => PROVIDER_CLOCK_MONOTONIC_SYSCALL_MFENCE,
    (false, ORDERED_BARRIER_CPUID) => PROVIDER_CLOCK_MONOTONIC_CPUID,
    (true, ORDERED_BARRIER_CPUID) => PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID,
    (false, ORDERED_BARRIER_LFENCE) => PROVIDER_CLOCK_MONOTONIC_LFENCE,
    (true, ORDERED_BARRIER_LFENCE) => PROVIDER_CLOCK_MONOTONIC_SYSCALL_LFENCE,
    (false, ORDERED_BARRIER_RDTSCP) => PROVIDER_CLOCK_MONOTONIC_RDTSCP,
    (true, ORDERED_BARRIER_RDTSCP) => PROVIDER_CLOCK_MONOTONIC_SYSCALL_RDTSCP,
    (false, ORDERED_BARRIER_OS_OWNED) => PROVIDER_CLOCK_MONOTONIC_OS_OWNED,
    (true, ORDERED_BARRIER_OS_OWNED) => PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_OWNED,
    (false, ORDERED_BARRIER_SERIALIZE) => PROVIDER_CLOCK_MONOTONIC_SERIALIZE,
    (true, ORDERED_BARRIER_SERIALIZE) => PROVIDER_CLOCK_MONOTONIC_SYSCALL_SERIALIZE,
    (false, _) => PROVIDER_CLOCK_MONOTONIC_CPUID,
    (true, _) => PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID,
  }
}

#[cfg(feature = "bench-internal")]
const fn barrier_name(barrier: u8) -> &'static str {
  match barrier {
    ORDERED_BARRIER_LFENCE => "x86_lfence",
    ORDERED_BARRIER_MFENCE => "x86_mfence",
    ORDERED_BARRIER_RDTSCP => "x86_rdtscp_lfence",
    ORDERED_BARRIER_OS_OWNED => "os_owned",
    ORDERED_BARRIER_SERIALIZE => "x86_serialize",
    _ => "x86_cpuid",
  }
}

#[cfg(feature = "bench-internal")]
fn ordered_path_evidence(path: OrderedPathSelection) -> OrderedPathEvidenceProjection {
  let mut candidate_names = ["unavailable"; ORDERED_BARRIER_CANDIDATES];
  let mut candidate_medians_ns = [0; ORDERED_BARRIER_CANDIDATES];
  for index in 0..path.candidate_count {
    candidate_names[index] = barrier_name(path.candidate_barriers[index]);
    candidate_medians_ns[index] = median(path.candidate_batches_ns[index]);
  }
  let mut challengers = ["unavailable"; ORDERED_BARRIER_DECISIONS];
  let mut incumbents = ["unavailable"; ORDERED_BARRIER_DECISIONS];
  let mut winners = ["unavailable"; ORDERED_BARRIER_DECISIONS];
  let mut allowances_ns = [0; ORDERED_BARRIER_DECISIONS];
  let mut decisive_wins = [0; ORDERED_BARRIER_DECISIONS];
  let mut challenger_selected = [false; ORDERED_BARRIER_DECISIONS];
  for index in 0..path.decision_count {
    challengers[index] = barrier_name(path.challengers[index]);
    incumbents[index] = barrier_name(path.incumbents[index]);
    winners[index] = barrier_name(path.winners[index]);
    allowances_ns[index] = path.decisions[index].allowance;
    decisive_wins[index] = path.decisions[index].decisive_wins;
    challenger_selected[index] = path.decisions[index].challenger_selected;
  }
  OrderedPathEvidenceProjection {
    candidate_names,
    candidate_medians_ns,
    challengers,
    incumbents,
    winners,
    allowances_ns,
    decisive_wins,
    challenger_selected,
  }
}

#[cfg(feature = "bench-internal")]
fn ordered_candidate_providers(
  syscall: bool,
  path: OrderedPathSelection,
) -> [u8; ORDERED_BARRIER_CANDIDATES] {
  let mut providers = [PROVIDER_UNKNOWN; ORDERED_BARRIER_CANDIDATES];
  for (index, barrier) in path.candidate_barriers[..path.candidate_count].iter().enumerate() {
    providers[index] = provider_for_barrier(syscall, *barrier);
  }
  providers
}

#[cfg(feature = "bench-internal")]
const fn ordered_barrier_name_for_provider(provider: u8) -> &'static str {
  match provider {
    PROVIDER_CLOCK_MONOTONIC_MFENCE | PROVIDER_CLOCK_MONOTONIC_SYSCALL_MFENCE => "x86_mfence",
    PROVIDER_CLOCK_MONOTONIC_LFENCE | PROVIDER_CLOCK_MONOTONIC_SYSCALL_LFENCE => "x86_lfence",
    PROVIDER_CLOCK_MONOTONIC_RDTSCP | PROVIDER_CLOCK_MONOTONIC_SYSCALL_RDTSCP => {
      "x86_rdtscp_lfence"
    }
    PROVIDER_CLOCK_MONOTONIC_CPUID | PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID => "x86_cpuid",
    PROVIDER_CLOCK_MONOTONIC_OS_OWNED | PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_OWNED => "os_owned",
    PROVIDER_CLOCK_MONOTONIC_SERIALIZE | PROVIDER_CLOCK_MONOTONIC_SYSCALL_SERIALIZE => {
      "x86_serialize"
    }
    _ => "not_applicable",
  }
}

fn kernel_eligible_tsc_frequency() -> Option<u64> {
  let mut hardware = [0_u8; 16];
  let mut hardware_len = hardware.len();
  // SAFETY: the name is NUL-terminated, `hardware` is writable for its
  // declared size, and no new value is supplied.
  let status = unsafe {
    libc::sysctlbyname(
      c"kern.timecounter.hardware".as_ptr(),
      hardware.as_mut_ptr().cast(),
      &mut hardware_len,
      ptr::null_mut(),
      0,
    )
  };
  if status != 0 || hardware_len == 0 || hardware_len > hardware.len() {
    return None;
  }

  let quality_name = if hardware[..hardware_len] == *b"TSC\0" {
    c"kern.timecounter.tc.TSC.quality"
  } else if hardware[..hardware_len] == *b"TSC-low\0" {
    c"kern.timecounter.tc.TSC-low.quality"
  } else {
    return None;
  };

  let quality = sysctl_i32(quality_name)?;
  let invariant = sysctl_i32(c"kern.timecounter.invariant_tsc")?;
  let cpu_count = sysctl_i32(c"hw.ncpu")?;
  let smp_safe = if cpu_count > 1 { sysctl_i32(c"kern.timecounter.smp_tsc")? != 0 } else { true };
  let disabled = sysctl_i32(c"machdep.disable_tsc")?;
  if !tsc_sysctls_eligible(quality, invariant, cpu_count, smp_safe, disabled) {
    return None;
  }

  let frequency = sysctl_u64(c"machdep.tsc_freq")?;
  (frequency > 0).then_some(frequency)
}

const fn tsc_sysctls_eligible(
  quality: i32,
  invariant: i32,
  cpu_count: i32,
  smp_safe: bool,
  disabled: i32,
) -> bool {
  quality >= 0 && invariant != 0 && cpu_count > 0 && smp_safe && disabled == 0
}

fn sysctl_i32(name: &core::ffi::CStr) -> Option<i32> {
  let mut value: libc::c_int = 0;
  let mut value_len = size_of::<libc::c_int>();
  // SAFETY: `name` is NUL-terminated, `value` is writable for `value_len`, and
  // the queried nodes have CTLTYPE_INT.
  let status = unsafe {
    libc::sysctlbyname(
      name.as_ptr(),
      ptr::from_mut(&mut value).cast(),
      &mut value_len,
      ptr::null_mut(),
      0,
    )
  };
  (status == 0 && value_len == size_of::<libc::c_int>()).then_some(value)
}

fn sysctl_u64(name: &core::ffi::CStr) -> Option<u64> {
  let mut value = 0_u64;
  let mut value_len = size_of::<u64>();
  // SAFETY: `name` is NUL-terminated, `value` is writable for `value_len`, and
  // machdep.tsc_freq has CTLTYPE_U64.
  let status = unsafe {
    libc::sysctlbyname(
      name.as_ptr(),
      ptr::from_mut(&mut value).cast(),
      &mut value_len,
      ptr::null_mut(),
      0,
    )
  };
  (status == 0 && value_len == size_of::<u64>()).then_some(value)
}

fn measure_candidates(
  ordered: bool,
  tsc_eligible: bool,
  timekeep_available: bool,
  clock_provider: u8,
  syscall_provider: u8,
) -> (ProbeSamples, [u8; 4], usize) {
  if tsc_eligible {
    warm_candidate(ordered, PROVIDER_TSC);
  }
  if timekeep_available {
    warm_candidate(ordered, PROVIDER_TIMEKEEP);
  }
  warm_candidate(ordered, clock_provider);
  warm_candidate(ordered, syscall_provider);

  let mut candidates = [PROVIDER_UNKNOWN; 4];
  candidates[0] = clock_provider;
  candidates[1] = syscall_provider;
  let mut candidate_count = 2;
  if timekeep_available {
    candidates[candidate_count] = PROVIDER_TIMEKEEP;
    candidate_count += 1;
  }
  if tsc_eligible {
    candidates[candidate_count] = PROVIDER_TSC;
    candidate_count += 1;
  }

  let mut samples = ProbeSamples {
    direct: [u64::MAX; PROBE_BATCHES],
    timekeep: [u64::MAX; PROBE_BATCHES],
    clock: [u64::MAX; PROBE_BATCHES],
    syscall: [u64::MAX; PROBE_BATCHES],
  };
  for sample in 0..PROBE_BATCHES {
    for offset in 0..candidate_count {
      let provider = candidates[(sample + offset) % candidate_count];
      let elapsed = measure_batch(ordered, provider).unwrap_or(u64::MAX);
      match provider {
        PROVIDER_TSC => samples.direct[sample] = elapsed,
        PROVIDER_TIMEKEEP => samples.timekeep[sample] = elapsed,
        provider if provider == clock_provider => samples.clock[sample] = elapsed,
        provider if provider == syscall_provider => samples.syscall[sample] = elapsed,
        _ => unreachable!(),
      }
    }
  }
  (samples, candidates, candidate_count)
}

#[inline(never)]
fn measure_batch(ordered: bool, provider: u8) -> Option<u64> {
  if ordered {
    PROBE_ORDERED_PROVIDER.store(provider, Ordering::Relaxed);
  } else {
    PROBE_INSTANT_PROVIDER.store(provider, Ordering::Relaxed);
  }
  let start = raw_clock_nanos(libc::CLOCK_MONOTONIC)?;
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
  let elapsed = raw_clock_nanos(libc::CLOCK_MONOTONIC)?.checked_sub(start);
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
    PROVIDER_TSC => super::x86_64::rdtsc(),
    PROVIDER_TIMEKEEP => timekeep_clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    _ => clock_monotonic_syscall(),
  }
}

#[inline(always)]
fn probe_ordered_hot_path() -> u64 {
  match PROBE_ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_TSC => super::x86_64::rdtsc_ordered(),
    PROVIDER_TIMEKEEP => ordered_timekeep_clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_MFENCE => ordered_clock_monotonic_mfence(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_MFENCE => ordered_clock_monotonic_syscall_mfence(),
    PROVIDER_CLOCK_MONOTONIC_CPUID => ordered_clock_monotonic_cpuid(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID => ordered_clock_monotonic_syscall_cpuid(),
    PROVIDER_CLOCK_MONOTONIC_LFENCE => ordered_clock_monotonic_lfence(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_LFENCE => ordered_clock_monotonic_syscall_lfence(),
    PROVIDER_CLOCK_MONOTONIC_RDTSCP => ordered_clock_monotonic_rdtscp(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_RDTSCP => ordered_clock_monotonic_syscall_rdtscp(),
    PROVIDER_CLOCK_MONOTONIC_OS_OWNED => ordered_clock_monotonic_os_owned(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_OWNED => ordered_clock_monotonic_syscall_os_owned(),
    PROVIDER_CLOCK_MONOTONIC_SERIALIZE => ordered_clock_monotonic_serialize(),
    _ => ordered_clock_monotonic_syscall_serialize(),
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

fn median(mut values: [u64; PROBE_BATCHES]) -> u64 {
  values.sort_unstable();
  values[PROBE_BATCHES / 2]
}

#[inline(always)]
fn clock_monotonic_syscall() -> u64 {
  raw_clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic() -> u64 {
  libc_clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| raw_clock_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn libc_clock_nanos(clock_id: libc::clockid_t) -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: the output is writable and CLOCK_MONOTONIC is valid on FreeBSD.
  if unsafe { libc::clock_gettime(clock_id, value.as_mut_ptr()) } != 0 {
    return None;
  }
  // SAFETY: a successful clock_gettime initialized the output.
  timespec_nanos(unsafe { value.assume_init() })
}

#[inline(always)]
fn raw_clock_nanos(clock_id: libc::clockid_t) -> Option<u64> {
  const SYS_CLOCK_GETTIME: libc::c_long = 232;
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  let mut status = SYS_CLOCK_GETTIME;
  // SAFETY: FreeBSD amd64 syscall 232 is clock_gettime, with rdi/rsi holding
  // its arguments. The fast return path also clears r8-r10.
  unsafe {
    core::arch::asm!(
      "syscall",
      inlateout("rax") status,
      in("rdi") clock_id,
      in("rsi") value.as_mut_ptr(),
      lateout("rcx") _,
      lateout("r8") _,
      lateout("r9") _,
      lateout("r10") _,
      lateout("r11") _,
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
    PROVIDER_TSC => WallProvider::Tsc,
    PROVIDER_TIMEKEEP => WallProvider::Timekeep,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_MFENCE
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_LFENCE
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_RDTSCP
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_OWNED
    | PROVIDER_CLOCK_MONOTONIC_SYSCALL_SERIALIZE => WallProvider::ClockMonotonicSyscall,
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
    PROVIDER_TSC => super::x86_64::rdtsc as fn() -> u64,
    PROVIDER_TIMEKEEP => timekeep_clock_monotonic as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall as fn() -> u64,
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
  let (name, read) = match provider {
    PROVIDER_TSC => match super::x86_64::selected_ordered_read_kind() {
      super::x86_64::ORDERED_READ_LFENCE_RDTSC => (
        "freebsd_kernel_eligible_tsc_x86_lfence_rdtsc",
        super::x86_64::bench_lfence_rdtsc as fn() -> u64,
      ),
      super::x86_64::ORDERED_READ_MFENCE_RDTSC => (
        "freebsd_kernel_eligible_tsc_x86_mfence_rdtsc",
        super::x86_64::bench_mfence_rdtsc as fn() -> u64,
      ),
      super::x86_64::ORDERED_READ_RDTSCP => {
        ("freebsd_kernel_eligible_tsc_x86_rdtscp", super::x86_64::bench_rdtscp as fn() -> u64)
      }
      super::x86_64::ORDERED_READ_SERIALIZE_RDTSC => (
        "freebsd_kernel_eligible_tsc_x86_serialize_rdtsc",
        super::x86_64::bench_serialize_rdtsc as fn() -> u64,
      ),
      _ => (
        "freebsd_kernel_eligible_tsc_x86_cpuid_rdtsc",
        super::x86_64::bench_cpuid_rdtsc as fn() -> u64,
      ),
    },
    PROVIDER_TIMEKEEP => {
      ("freebsd_at_timekeep_os_owned", ordered_timekeep_clock_monotonic as fn() -> u64)
    }
    PROVIDER_CLOCK_MONOTONIC_MFENCE => {
      ("freebsd_clock_monotonic_x86_mfence", ordered_clock_monotonic_mfence as fn() -> u64)
    }
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_MFENCE => (
      "freebsd_clock_monotonic_syscall_x86_mfence",
      ordered_clock_monotonic_syscall_mfence as fn() -> u64,
    ),
    PROVIDER_CLOCK_MONOTONIC_CPUID => {
      ("freebsd_clock_monotonic_x86_cpuid", ordered_clock_monotonic_cpuid as fn() -> u64)
    }
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID => (
      "freebsd_clock_monotonic_syscall_x86_cpuid",
      ordered_clock_monotonic_syscall_cpuid as fn() -> u64,
    ),
    PROVIDER_CLOCK_MONOTONIC_LFENCE => {
      ("freebsd_clock_monotonic_x86_lfence", ordered_clock_monotonic_lfence as fn() -> u64)
    }
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_LFENCE => (
      "freebsd_clock_monotonic_syscall_x86_lfence",
      ordered_clock_monotonic_syscall_lfence as fn() -> u64,
    ),
    PROVIDER_CLOCK_MONOTONIC_RDTSCP => {
      ("freebsd_clock_monotonic_x86_rdtscp_lfence", ordered_clock_monotonic_rdtscp as fn() -> u64)
    }
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_RDTSCP => (
      "freebsd_clock_monotonic_syscall_x86_rdtscp_lfence",
      ordered_clock_monotonic_syscall_rdtscp as fn() -> u64,
    ),
    PROVIDER_CLOCK_MONOTONIC_OS_OWNED => {
      ("freebsd_clock_monotonic_os_owned", ordered_clock_monotonic_os_owned as fn() -> u64)
    }
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_OS_OWNED => (
      "freebsd_clock_monotonic_syscall_os_owned",
      ordered_clock_monotonic_syscall_os_owned as fn() -> u64,
    ),
    PROVIDER_CLOCK_MONOTONIC_SERIALIZE => {
      ("freebsd_clock_monotonic_x86_serialize", ordered_clock_monotonic_serialize as fn() -> u64)
    }
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_SERIALIZE => (
      "freebsd_clock_monotonic_syscall_x86_serialize",
      ordered_clock_monotonic_syscall_serialize as fn() -> u64,
    ),
    _ => ("freebsd_clock_monotonic_x86_cpuid", ordered_clock_monotonic_cpuid as fn() -> u64),
  };
  BenchPrimitive { name, read }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_candidate_primitives() -> ([Option<BenchPrimitive>; 4], usize) {
  bench_candidate_primitives(bench_instant_evidence(), false)
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_ordered_candidate_primitives() -> ([Option<BenchPrimitive>; 14], usize) {
  let evidence = bench_ordered_evidence();
  let mut primitives = [None; 14];
  let mut count = 0;
  if evidence.timekeep_available {
    primitives[count] = Some(ordered_bench_primitive(PROVIDER_TIMEKEEP));
    count += 1;
  }
  if evidence.tsc_eligible {
    primitives[count] = Some(ordered_bench_primitive(PROVIDER_TSC));
    count += 1;
  }
  for provider in
    &evidence.ordered_clock_candidate_providers[..evidence.ordered_barrier_candidate_count]
  {
    primitives[count] = Some(ordered_bench_primitive(*provider));
    count += 1;
  }
  for provider in &evidence.ordered_syscall_candidate_providers
    [..evidence.ordered_syscall_barrier_candidate_count]
  {
    primitives[count] = Some(ordered_bench_primitive(*provider));
    count += 1;
  }
  (primitives, count)
}

#[cfg(feature = "bench-internal")]
fn bench_candidate_primitives(
  evidence: ProbeEvidence,
  ordered: bool,
) -> ([Option<BenchPrimitive>; 4], usize) {
  let mut primitives = [None; 4];
  for (index, provider) in
    evidence.candidate_providers[..evidence.candidate_count].iter().enumerate()
  {
    primitives[index] = Some(if ordered {
      ordered_bench_primitive(*provider)
    } else {
      instant_bench_primitive(*provider)
    });
  }
  (primitives, evidence.candidate_count)
}

#[cfg(feature = "bench-internal")]
macro_rules! exact_bench_reader {
  ($name:ident, $body:expr) => {
    #[inline(always)]
    #[allow(dead_code)] // Exact readers are consumed by out-of-module evidence harnesses.
    pub(crate) fn $name() -> u64 {
      $body
    }
  };
}

#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_tsc, super::x86_64::rdtsc());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_tsc_lfence_rdtsc, super::x86_64::bench_lfence_rdtsc());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_tsc_mfence_rdtsc, super::x86_64::bench_mfence_rdtsc());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_tsc_rdtscp, super::x86_64::bench_rdtscp());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_tsc_cpuid_rdtsc, super::x86_64::bench_cpuid_rdtsc());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_tsc_serialize_rdtsc, super::x86_64::bench_serialize_rdtsc());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_timekeep, timekeep_clock_monotonic());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_timekeep_os_owned, ordered_timekeep_clock_monotonic());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic, clock_monotonic());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_syscall, clock_monotonic_syscall());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_mfence, ordered_clock_monotonic_mfence());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_syscall_mfence,
  ordered_clock_monotonic_syscall_mfence()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_cpuid, ordered_clock_monotonic_cpuid());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_syscall_cpuid,
  ordered_clock_monotonic_syscall_cpuid()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_lfence, ordered_clock_monotonic_lfence());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_syscall_lfence,
  ordered_clock_monotonic_syscall_lfence()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_rdtscp, ordered_clock_monotonic_rdtscp());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_syscall_rdtscp,
  ordered_clock_monotonic_syscall_rdtscp()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_os_owned, ordered_clock_monotonic_os_owned());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_syscall_os_owned,
  ordered_clock_monotonic_syscall_os_owned()
);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_clock_monotonic_serialize, ordered_clock_monotonic_serialize());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_syscall_serialize,
  ordered_clock_monotonic_syscall_serialize()
);

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_provider() -> &'static str {
  bench_instant_provider().name()
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_tsc_eligible() -> bool {
  kernel_eligible_tsc_frequency().is_some()
}

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
  fn kernel_selected_nonnegative_tsc_quality_is_eligible() {
    assert!(tsc_sysctls_eligible(0, 1, 2, true, 0));
    assert!(tsc_sysctls_eligible(799, 1, 2, true, 0));
    assert!(!tsc_sysctls_eligible(-1, 1, 2, true, 0));
    assert!(!tsc_sysctls_eligible(800, 0, 2, true, 0));
    assert!(!tsc_sysctls_eligible(800, 1, 2, false, 0));
    assert!(!tsc_sysctls_eligible(800, 1, 2, true, 1));
  }

  #[test]
  fn every_ordered_barrier_maps_to_distinct_libc_and_syscall_providers() {
    let barriers = [
      ORDERED_BARRIER_CPUID,
      ORDERED_BARRIER_LFENCE,
      ORDERED_BARRIER_MFENCE,
      ORDERED_BARRIER_RDTSCP,
      ORDERED_BARRIER_OS_OWNED,
      ORDERED_BARRIER_SERIALIZE,
    ];
    let mut libc = [0_u8; ORDERED_BARRIER_CANDIDATES];
    let mut syscall = [0_u8; ORDERED_BARRIER_CANDIDATES];
    for (index, barrier) in barriers.into_iter().enumerate() {
      libc[index] = provider_for_barrier(false, barrier);
      syscall[index] = provider_for_barrier(true, barrier);
      assert_ne!(libc[index], syscall[index]);
    }
    for left in 0..ORDERED_BARRIER_CANDIDATES {
      for right in left + 1..ORDERED_BARRIER_CANDIDATES {
        assert_ne!(libc[left], libc[right]);
        assert_ne!(syscall[left], syscall[right]);
      }
    }
  }

  #[test]
  #[allow(unused_unsafe)]
  fn live_feature_candidates_are_present_without_vendor_shortcuts() {
    let barriers = eligible_ordered_barriers();
    let eligible = &barriers.barriers[..barriers.count];
    assert_eq!(eligible[0], ORDERED_BARRIER_CPUID);
    assert!(eligible.contains(&ORDERED_BARRIER_OS_OWNED));

    // SAFETY: x86_64 guarantees CPUID leaf zero.
    let (basic, extended) = unsafe { (__cpuid(0), __cpuid(0x8000_0000)) };
    // SAFETY: the maximum-leaf checks precede the optional CPUID reads.
    let (extended_features, structured_features) = unsafe {
      (
        if extended.eax >= 0x8000_0001 { __cpuid(0x8000_0001).edx } else { 0 },
        if basic.eax >= 7 { __cpuid_count(7, 0).edx } else { 0 },
      )
    };
    let rdtscp = extended_features & (1 << 27) != 0;
    let serialize = structured_features & (1 << 14) != 0;
    assert_eq!(eligible.contains(&ORDERED_BARRIER_RDTSCP), rdtscp);
    assert_eq!(eligible.contains(&ORDERED_BARRIER_SERIALIZE), serialize);
  }

  #[test]
  fn bare_syscall_os_owned_is_removed_without_an_entry_ordering_contract() {
    let mut barriers = OrderedBarrierCandidates::conservative();
    barriers.push(ORDERED_BARRIER_OS_OWNED);
    barriers.push(ORDERED_BARRIER_MFENCE);
    // Both the explicit raw syscall and libc need this filter: libc's vDSO
    // path can report ENOSYS and fall through to that same bare syscall.
    let filtered = ordered_fallback_barriers(barriers, false);
    assert_eq!(&filtered.barriers[..filtered.count], &[
      ORDERED_BARRIER_CPUID,
      ORDERED_BARRIER_MFENCE
    ],);
    assert_eq!(ordered_fallback_barriers(barriers, true).count, barriers.count);
  }

  #[test]
  fn public_timekeep_layout_matches_freebsd_amd64_v1() {
    assert_eq!(size_of::<Bintime>(), 16);
    assert_eq!(size_of::<VdsoTimekeep>(), 16);
    assert_eq!(size_of::<VdsoTimehands>(), 88);
    assert_eq!(core::mem::offset_of!(VdsoTimehands, th_scale), 8);
    assert_eq!(core::mem::offset_of!(VdsoTimehands, th_offset), 24);
    assert_eq!(core::mem::offset_of!(VdsoTimehands, th_x86_shift), 56);
    assert_eq!(core::mem::offset_of!(VdsoTimehands, th_x86_pvc_last_systime), 64);
    assert_eq!(size_of::<PvclockVcpuTimeInfo>(), 32);
    assert_eq!(core::mem::offset_of!(PvclockVcpuTimeInfo, tsc_timestamp), 8);
    assert_eq!(size_of::<HypervRefTsc>(), 24);
    assert_eq!(core::mem::offset_of!(HypervRefTsc, tsc_scale), 8);
  }

  #[test]
  fn every_current_amd64_timekeep_algorithm_is_distinct_and_supported() {
    let algorithms = [VDSO_ALGO_TSC, VDSO_ALGO_HPET, VDSO_ALGO_HYPERV_REFTSC, VDSO_ALGO_PVCLOCK];
    for (index, algorithm) in algorithms.iter().enumerate() {
      assert!((1..=4).contains(algorithm));
      assert!(!algorithms[..index].contains(algorithm));
    }
  }

  #[test]
  #[allow(clippy::assertions_on_constants)] // Confirms libc exposes either supported alias era.
  fn exact_clock_aliases_are_structural_duplicates() {
    assert_eq!(libc::CLOCK_MONOTONIC, 4);
    assert_eq!(libc::CLOCK_MONOTONIC_PRECISE, 11);
    assert_eq!(libc::CLOCK_UPTIME, 5);
    assert_eq!(libc::CLOCK_UPTIME_PRECISE, 7);
    assert!(
      libc::CLOCK_BOOTTIME == libc::CLOCK_MONOTONIC || libc::CLOCK_BOOTTIME == libc::CLOCK_UPTIME
    );
  }

  #[test]
  fn fixed_point_conversions_match_freebsd_formulas() {
    let half_second = 1_u64 << 63;
    assert_eq!(
      scaled_bintime_nanos(Bintime { sec: 2, frac: half_second }, half_second, 1),
      Some(3_000_000_000),
    );
    assert_eq!(pvclock_scale_delta(10, 1_u32 << 31, 0), Some(5));
    assert_eq!(pvclock_scale_delta(10, 1_u32 << 31, 1), Some(10));
  }

  #[test]
  fn live_timekeep_protocol_is_same_domain_and_monotonic_when_available() {
    if !timekeep_available() {
      std::eprintln!("AT_TIMEKEEP unavailable; libc/raw same-domain routes remain eligible");
      return;
    }
    let timekeep = TIMEKEEP_PTR.load(Ordering::Acquire) as *const VdsoTimekeep;
    let first = timekeep_nanos(timekeep).expect("selected AT_TIMEKEEP must read directly");
    let native =
      libc_clock_nanos(libc::CLOCK_MONOTONIC).expect("CLOCK_MONOTONIC must be available");
    assert!(first.abs_diff(native) < 1_000_000_000);

    let mut previous = first;
    for _ in 0..10_000 {
      let current = timekeep_nanos(timekeep).expect("stable AT_TIMEKEEP read");
      assert!(current >= previous);
      previous = current;
    }

    #[cfg(feature = "bench-internal")]
    std::eprintln!(
      "FreeBSD wall providers: Instant={}, Ordered={}",
      instant_bench_primitive(selected_instant_provider()).name,
      ordered_bench_primitive(selected_ordered_provider()).name,
    );
  }
}
