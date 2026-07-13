//! Runtime wall-clock selection for Linux-kernel x86 and x86_64 targets.
//!
//! The fastest reliable wall read is a property of the running kernel, libc,
//! hypervisor, and CPU rather than the Rust build target. Each wall API probes
//! its complete hot path independently, including the provider-state load and
//! branch. Candidates cover direct TSC, libc/vDSO reads of
//! `CLOCK_MONOTONIC`, `CLOCK_MONOTONIC_RAW`, and `CLOCK_BOOTTIME`, and each
//! native raw syscall ABI.
//! On i686, the time32 and time64 ABIs are separate candidates; a selected hot
//! path never attempts one syscall and falls through to the other.
//!
//! Ordered OS-clock candidates also include their ordering mechanism in the
//! provider identity. The OS-owned call boundary and eligible fence/RDTSCP
//! paths are measured against a conservative CPUID baseline. A direct ordered
//! TSC provider embeds the primitive selected by the architecture module, so
//! its published path has no second selector dispatch.
//! Bare x86_64 SYSCALL is OS-owned only on Intel, whose architectural contract
//! orders older instructions; AMD and unknown vendors retain explicit-barrier
//! candidates. The i686 INT 0x80 exception boundary remains OS-owned.
//!
//! Linux TSC permission is per-thread. Initial denial on the selection thread
//! excludes TSC, RDTSCP, and libc/vDSO candidates because a TSC-backed vDSO can
//! fault too. A process-wide direct or vDSO winner therefore requires every
//! reading thread to retain counter permission; explicitly requesting
//! `PR_SET_TSC(PR_TSC_SIGSEGV)` is an external fault boundary. OS-clock reads
//! retry the same clock ID through an alternate ABI before failing closed.

#[cfg(feature = "bench-internal")]
use core::cell::UnsafeCell;
use core::hint::black_box;
use core::mem::MaybeUninit;
#[cfg(feature = "bench-internal")]
use core::sync::atomic::AtomicBool;
use core::sync::atomic::{AtomicI32, AtomicU8, AtomicU64, Ordering};

const PROVIDER_UNKNOWN: u8 = 0;
const PROVIDER_SELECTING: u8 = 1;

const PROVIDER_TSC: u8 = 2;
// Keep complete ordered-TSC identities outside the dense OS-provider range.
// Their sparse encoding emits a predictable comparison tree instead of an
// indirect jump table on the public ordered hot path.
const PROVIDER_TSC_LFENCE_RDTSC: u8 = 2;
const PROVIDER_TSC_RDTSCP: u8 = 128;
const PROVIDER_TSC_CPUID_RDTSC: u8 = 160;
const PROVIDER_TSC_MFENCE_RDTSC: u8 = 192;
const PROVIDER_TSC_SERIALIZE_RDTSC: u8 = 224;

const SOURCE_LIBC_MONOTONIC: u8 = 0;
const SOURCE_LIBC_MONOTONIC_RAW: u8 = 1;
#[allow(dead_code)] // Shared provider encoding; only the matching pointer width routes it.
const SOURCE_SYSCALL64_MONOTONIC: u8 = 2;
const SOURCE_SYSCALL64_MONOTONIC_RAW: u8 = 3;
#[allow(dead_code)] // Shared provider encoding; only the matching pointer width routes it.
const SOURCE_TIME32_MONOTONIC: u8 = 4;
const SOURCE_TIME32_MONOTONIC_RAW: u8 = 5;
#[allow(dead_code)] // Shared provider encoding; only the matching pointer width routes it.
const SOURCE_TIME64_MONOTONIC: u8 = 6;
const SOURCE_TIME64_MONOTONIC_RAW: u8 = 7;
const SOURCE_VDSO_MONOTONIC: u8 = 8;
const SOURCE_VDSO_MONOTONIC_RAW: u8 = 9;
#[cfg(target_pointer_width = "32")]
const SOURCE_VDSO_TIME64_MONOTONIC: u8 = 10;
#[cfg(target_pointer_width = "32")]
const SOURCE_VDSO_TIME64_MONOTONIC_RAW: u8 = 11;
const SOURCE_LIBC_BOOTTIME: u8 = 12;
#[allow(dead_code)] // Shared provider encoding; only the matching pointer width routes it.
const SOURCE_SYSCALL64_BOOTTIME: u8 = 13;
#[allow(dead_code)] // Shared provider encoding; only the matching pointer width routes it.
const SOURCE_TIME32_BOOTTIME: u8 = 14;
#[allow(dead_code)] // Shared provider encoding; only the matching pointer width routes it.
const SOURCE_TIME64_BOOTTIME: u8 = 15;
const SOURCE_VDSO_BOOTTIME: u8 = 16;
#[cfg(target_pointer_width = "32")]
const SOURCE_VDSO_TIME64_BOOTTIME: u8 = 17;
const SOURCE_VARIANTS: u8 = 18;

const PROVIDER_SOURCE_BASE: u8 = 7;
const PROVIDER_LIBC_MONOTONIC: u8 = PROVIDER_SOURCE_BASE + SOURCE_LIBC_MONOTONIC;
const PROVIDER_LIBC_MONOTONIC_RAW: u8 = PROVIDER_SOURCE_BASE + SOURCE_LIBC_MONOTONIC_RAW;
#[allow(dead_code)] // Shared provider encoding; only the matching pointer width routes it.
const PROVIDER_SYSCALL64_MONOTONIC: u8 = PROVIDER_SOURCE_BASE + SOURCE_SYSCALL64_MONOTONIC;
const PROVIDER_SYSCALL64_MONOTONIC_RAW: u8 = PROVIDER_SOURCE_BASE + SOURCE_SYSCALL64_MONOTONIC_RAW;
#[allow(dead_code)] // Shared provider encoding; only the matching pointer width routes it.
const PROVIDER_TIME32_MONOTONIC: u8 = PROVIDER_SOURCE_BASE + SOURCE_TIME32_MONOTONIC;
const PROVIDER_TIME32_MONOTONIC_RAW: u8 = PROVIDER_SOURCE_BASE + SOURCE_TIME32_MONOTONIC_RAW;
#[allow(dead_code)] // Shared provider encoding; only the matching pointer width routes it.
const PROVIDER_TIME64_MONOTONIC: u8 = PROVIDER_SOURCE_BASE + SOURCE_TIME64_MONOTONIC;
const PROVIDER_TIME64_MONOTONIC_RAW: u8 = PROVIDER_SOURCE_BASE + SOURCE_TIME64_MONOTONIC_RAW;
const PROVIDER_VDSO_MONOTONIC: u8 = PROVIDER_SOURCE_BASE + SOURCE_VDSO_MONOTONIC;
const PROVIDER_VDSO_MONOTONIC_RAW: u8 = PROVIDER_SOURCE_BASE + SOURCE_VDSO_MONOTONIC_RAW;
#[cfg(target_pointer_width = "32")]
const PROVIDER_VDSO_TIME64_MONOTONIC: u8 = PROVIDER_SOURCE_BASE + SOURCE_VDSO_TIME64_MONOTONIC;
#[cfg(target_pointer_width = "32")]
const PROVIDER_VDSO_TIME64_MONOTONIC_RAW: u8 =
  PROVIDER_SOURCE_BASE + SOURCE_VDSO_TIME64_MONOTONIC_RAW;
const PROVIDER_LIBC_BOOTTIME: u8 = PROVIDER_SOURCE_BASE + SOURCE_LIBC_BOOTTIME;
#[allow(dead_code)] // Shared provider encoding; only the matching pointer width routes it.
const PROVIDER_SYSCALL64_BOOTTIME: u8 = PROVIDER_SOURCE_BASE + SOURCE_SYSCALL64_BOOTTIME;
#[allow(dead_code)] // Shared provider encoding; only the matching pointer width routes it.
const PROVIDER_TIME32_BOOTTIME: u8 = PROVIDER_SOURCE_BASE + SOURCE_TIME32_BOOTTIME;
#[allow(dead_code)] // Shared provider encoding; only the matching pointer width routes it.
const PROVIDER_TIME64_BOOTTIME: u8 = PROVIDER_SOURCE_BASE + SOURCE_TIME64_BOOTTIME;
const PROVIDER_VDSO_BOOTTIME: u8 = PROVIDER_SOURCE_BASE + SOURCE_VDSO_BOOTTIME;
#[cfg(target_pointer_width = "32")]
const PROVIDER_VDSO_TIME64_BOOTTIME: u8 = PROVIDER_SOURCE_BASE + SOURCE_VDSO_TIME64_BOOTTIME;

const ORDERED_BARRIER_LFENCE: u8 = 0;
const ORDERED_BARRIER_RDTSCP: u8 = 1;
const ORDERED_BARRIER_MFENCE: u8 = 2;
const ORDERED_BARRIER_CPUID: u8 = 3;
const ORDERED_BARRIER_OS_OWNED: u8 = 4;
const ORDERED_BARRIER_SERIALIZE: u8 = 5;
const ORDERED_BARRIER_VARIANTS: u8 = 6;
const ORDERED_OS_BASE: u8 = 16;

#[cfg(target_pointer_width = "64")]
const REENTRANT_SOURCE: u8 = SOURCE_SYSCALL64_MONOTONIC;
#[cfg(target_pointer_width = "32")]
const REENTRANT_SOURCE: u8 = SOURCE_TIME32_MONOTONIC;
const REENTRANT_INSTANT_PROVIDER: u8 = PROVIDER_SOURCE_BASE + REENTRANT_SOURCE;
const REENTRANT_ORDERED_PROVIDER: u8 =
  ORDERED_OS_BASE + REENTRANT_SOURCE * ORDERED_BARRIER_VARIANTS + ORDERED_BARRIER_CPUID;

const PROBE_BATCHES: usize = 9;
const PROBE_READS: u64 = 4096;
const PROBE_WARMUP_READS: u64 = 1024;
const REQUIRED_DECISIVE_WINS: usize = 8;
const MAX_INSTANT_CANDIDATES: usize = 16;
const MAX_ORDERED_CANDIDATES: usize = 91;
const MAX_INSTANT_DECISIONS: usize = MAX_INSTANT_CANDIDATES - 1;
const MAX_ORDERED_DECISIONS: usize = MAX_ORDERED_CANDIDATES - 1;

const PR_GET_TSC: libc::c_int = 25;
const PR_TSC_ENABLE: libc::c_int = 1;

static INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static INSTANT_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static INSTANT_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static ORDERED_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_HOT_STATE: AtomicU64 = AtomicU64::new(0);
static TSC_FREQUENCY: AtomicU64 = AtomicU64::new(0);
static INSTANT_PROBE_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_LIBC_MONOTONIC);
static ORDERED_PROBE_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TscEligibility {
  Eligible,
  MissingTsc,
  MissingInvariantTsc,
  KernelClocksourceMetadataUnavailable,
  KernelTscUnavailable,
  TscReadDisabled,
}

impl TscEligibility {
  #[cfg(feature = "bench-internal")]
  const fn name(self) -> &'static str {
    match self {
      Self::Eligible => "eligible",
      Self::MissingTsc => "cpuid_tsc_unavailable",
      Self::MissingInvariantTsc => "cpuid_invariant_tsc_unavailable",
      Self::KernelClocksourceMetadataUnavailable => "kernel_clocksource_metadata_unavailable",
      Self::KernelTscUnavailable => "kernel_tsc_clocksource_unavailable",
      Self::TscReadDisabled => "pr_get_tsc_not_enabled",
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

#[derive(Clone, Copy)]
struct CandidateList<const N: usize> {
  providers: [u8; N],
  count: usize,
}

#[derive(Clone, Copy)]
struct BarrierList {
  barriers: [u8; 6],
  count: usize,
}

impl BarrierList {
  const fn conservative() -> Self {
    Self { barriers: [ORDERED_BARRIER_CPUID; 6], count: 1 }
  }

  fn push(&mut self, barrier: u8) {
    debug_assert!(self.count < self.barriers.len());
    self.barriers[self.count] = barrier;
    self.count += 1;
  }
}

impl<const N: usize> CandidateList<N> {
  const fn new() -> Self {
    Self { providers: [PROVIDER_UNKNOWN; N], count: 0 }
  }

  fn push(&mut self, provider: u8) {
    debug_assert!(self.count < N);
    self.providers[self.count] = provider;
    self.count += 1;
  }
}

#[derive(Clone, Copy)]
struct ProbeSamples<const N: usize> {
  batches: [[u64; PROBE_BATCHES]; N],
}

#[derive(Clone, Copy)]
#[allow(dead_code)] // Decision fields are retained for bench-internal evidence.
struct Tournament<const N: usize, const D: usize> {
  selected_provider: u8,
  decision_count: usize,
  challengers: [u8; D],
  incumbents: [u8; D],
  winners: [u8; D],
  decisions: [SelectionDecision; D],
  samples: ProbeSamples<N>,
  candidates: CandidateList<N>,
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // The serializer intentionally projects this fixed schema.
pub(crate) struct WallProbeEvidence {
  pub(crate) instant_eligibility: &'static str,
  pub(crate) ordered_eligibility: &'static str,
  pub(crate) reads_per_batch: u64,
  pub(crate) required_decisive_wins: usize,

  pub(crate) instant_candidate_count: usize,
  pub(crate) instant_candidate_names: [&'static str; MAX_INSTANT_CANDIDATES],
  pub(crate) instant_candidate_eligible: [bool; MAX_INSTANT_CANDIDATES],
  pub(crate) instant_candidate_batches_ns: [[u64; PROBE_BATCHES]; MAX_INSTANT_CANDIDATES],
  pub(crate) instant_candidate_medians_ns: [u64; MAX_INSTANT_CANDIDATES],
  pub(crate) instant_tournament_decision_count: usize,
  pub(crate) instant_tournament_challengers: [&'static str; MAX_INSTANT_DECISIONS],
  pub(crate) instant_tournament_incumbents: [&'static str; MAX_INSTANT_DECISIONS],
  pub(crate) instant_tournament_winners: [&'static str; MAX_INSTANT_DECISIONS],
  pub(crate) instant_tournament_allowances_ns: [u64; MAX_INSTANT_DECISIONS],
  pub(crate) instant_tournament_decisive_wins: [usize; MAX_INSTANT_DECISIONS],
  pub(crate) instant_tournament_challenger_selected: [bool; MAX_INSTANT_DECISIONS],

  pub(crate) ordered_candidate_count: usize,
  pub(crate) ordered_candidate_names: [&'static str; MAX_ORDERED_CANDIDATES],
  pub(crate) ordered_candidate_eligible: [bool; MAX_ORDERED_CANDIDATES],
  pub(crate) ordered_candidate_batches_ns: [[u64; PROBE_BATCHES]; MAX_ORDERED_CANDIDATES],
  pub(crate) ordered_candidate_medians_ns: [u64; MAX_ORDERED_CANDIDATES],
  pub(crate) ordered_barrier_candidate_count: usize,
  pub(crate) ordered_barrier_candidate_names: [&'static str; 6],
  pub(crate) ordered_tournament_decision_count: usize,
  pub(crate) ordered_tournament_challengers: [&'static str; MAX_ORDERED_DECISIONS],
  pub(crate) ordered_tournament_incumbents: [&'static str; MAX_ORDERED_DECISIONS],
  pub(crate) ordered_tournament_winners: [&'static str; MAX_ORDERED_DECISIONS],
  pub(crate) ordered_tournament_allowances_ns: [u64; MAX_ORDERED_DECISIONS],
  pub(crate) ordered_tournament_decisive_wins: [usize; MAX_ORDERED_DECISIONS],
  pub(crate) ordered_tournament_challenger_selected: [bool; MAX_ORDERED_DECISIONS],

  pub(crate) instant_selected_provider: &'static str,
  pub(crate) ordered_selected_provider: &'static str,
  pub(crate) instant_fallback_provider: &'static str,
  pub(crate) ordered_fallback_provider: &'static str,
  pub(crate) ordered_os_barrier: &'static str,
  pub(crate) ordered_bare_syscall_os_owned_eligible: bool,
  pub(crate) ordered_bare_syscall_os_owned_basis: &'static str,
  pub(crate) ordered_fast_barrier_candidate: &'static str,
  pub(crate) ordered_baseline_barrier: &'static str,
  pub(crate) instant_tsc_selected: bool,
  pub(crate) ordered_tsc_selected: bool,

  // Compact projection used by summary serializers. The complete candidate
  // and tournament arrays above are the authoritative evidence schema.
  pub(crate) tsc_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_tsc_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_clock_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_syscall_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) tsc_median_ns: u64,
  pub(crate) clock_median_ns: u64,
  pub(crate) syscall_median_ns: u64,
  pub(crate) ordered_tsc_median_ns: u64,
  pub(crate) ordered_clock_median_ns: u64,
  pub(crate) ordered_syscall_median_ns: u64,
  pub(crate) instant_allowance_ns: u64,
  pub(crate) ordered_allowance_ns: u64,
  pub(crate) instant_decisive_wins: usize,
  pub(crate) ordered_decisive_wins: usize,
  pub(crate) instant_syscall_vs_clock_allowance_ns: u64,
  pub(crate) ordered_syscall_vs_clock_allowance_ns: u64,
  pub(crate) instant_syscall_vs_clock_decisive_wins: usize,
  pub(crate) ordered_syscall_vs_clock_decisive_wins: usize,
  pub(crate) ordered_clock_fast_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_clock_baseline_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_syscall_fast_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_syscall_baseline_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) ordered_clock_barrier_allowance_ns: u64,
  pub(crate) ordered_clock_barrier_decisive_wins: usize,
  pub(crate) ordered_syscall_barrier_allowance_ns: u64,
  pub(crate) ordered_syscall_barrier_decisive_wins: usize,
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy)]
struct DomainEvidence<const N: usize, const D: usize> {
  eligibility: TscEligibility,
  tournament: Tournament<N, D>,
  fallback_provider: u8,
  barriers: BarrierList,
  baseline_barrier: u8,
  fast_barrier: Option<u8>,
}

#[cfg(feature = "bench-internal")]
struct EvidenceCell<T>(UnsafeCell<MaybeUninit<T>>);

// SAFETY: the process-selection owner initializes the cell before publishing
// the corresponding provider with Release. Readers acquire that provider.
#[cfg(feature = "bench-internal")]
unsafe impl<T: Copy> Sync for EvidenceCell<T> {}

#[cfg(feature = "bench-internal")]
static INSTANT_EVIDENCE: EvidenceCell<
  DomainEvidence<MAX_INSTANT_CANDIDATES, MAX_INSTANT_DECISIONS>,
> = EvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));
#[cfg(feature = "bench-internal")]
static ORDERED_EVIDENCE: EvidenceCell<
  DomainEvidence<MAX_ORDERED_CANDIDATES, MAX_ORDERED_DECISIONS>,
> = EvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));
#[cfg(feature = "bench-internal")]
static INSTANT_EVIDENCE_READY: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "bench-internal")]
static ORDERED_EVIDENCE_READY: AtomicBool = AtomicBool::new(false);

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  let provider = INSTANT_PROVIDER.load(Ordering::Relaxed);
  if provider == PROVIDER_TSC { read_tsc() } else { read_outlined_instant_provider(provider) }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  let provider = ORDERED_PROVIDER.load(Ordering::Relaxed);
  match provider {
    PROVIDER_TSC_LFENCE_RDTSC => read_tsc_lfence_ordered(),
    PROVIDER_TSC_RDTSCP => read_tsc_rdtscp_ordered(),
    PROVIDER_TSC_MFENCE_RDTSC => read_tsc_mfence_ordered(),
    PROVIDER_TSC_SERIALIZE_RDTSC => read_tsc_serialize_ordered(),
    _ => read_outlined_ordered_provider(provider),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn ticks_ordered_with_scale() -> (u64, u64) {
  let state = ORDERED_HOT_STATE.load(Ordering::Relaxed);
  if state != 0 {
    return (read_ordered_provider(state as u8), state >> 8);
  }
  ticks_ordered_with_scale_cold()
}

#[cold]
#[inline(never)]
fn ticks_ordered_with_scale_cold() -> (u64, u64) {
  let ticks = ticks_ordered();
  let provider = ORDERED_PROVIDER.load(Ordering::Acquire);
  let scale = super::ORDERED_NANOS_PER_TICK_Q32.load(Ordering::Acquire);
  if !matches!(provider, PROVIDER_UNKNOWN | PROVIDER_SELECTING) {
    publish_ordered_hot_state(provider, scale);
  }
  (ticks, scale)
}

pub(crate) fn update_ordered_hot_scale(scale: u64) {
  let state = ORDERED_HOT_STATE.load(Ordering::Relaxed);
  if state != 0 {
    publish_ordered_hot_state(state as u8, scale);
  }
}

fn publish_ordered_hot_state(provider: u8, scale: u64) {
  if scale > u64::MAX >> 8 {
    ORDERED_HOT_STATE.store(0, Ordering::Release);
    return;
  }
  let state = scale << 8 | u64::from(provider);
  ORDERED_HOT_STATE.store(state, Ordering::Release);
}

/// Read an unordered endpoint in `OrderedInstant`'s selected numeric domain.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  read_ordered_provider_unordered(ORDERED_PROVIDER.load(Ordering::Relaxed))
}

#[inline(always)]
fn read_instant_provider(provider: u8) -> u64 {
  match provider {
    PROVIDER_TSC => read_tsc(),
    PROVIDER_LIBC_MONOTONIC => libc_clock(PROVIDER_LIBC_MONOTONIC),
    PROVIDER_LIBC_MONOTONIC_RAW => libc_clock(PROVIDER_LIBC_MONOTONIC_RAW),
    PROVIDER_LIBC_BOOTTIME => libc_clock(PROVIDER_LIBC_BOOTTIME),
    PROVIDER_VDSO_MONOTONIC => vdso_clock(SOURCE_VDSO_MONOTONIC),
    PROVIDER_VDSO_MONOTONIC_RAW => vdso_clock(SOURCE_VDSO_MONOTONIC_RAW),
    PROVIDER_VDSO_BOOTTIME => vdso_clock(SOURCE_VDSO_BOOTTIME),
    #[cfg(target_pointer_width = "32")]
    PROVIDER_VDSO_TIME64_MONOTONIC => vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC),
    #[cfg(target_pointer_width = "32")]
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW => vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC_RAW),
    #[cfg(target_pointer_width = "32")]
    PROVIDER_VDSO_TIME64_BOOTTIME => vdso_clock(SOURCE_VDSO_TIME64_BOOTTIME),
    #[cfg(target_pointer_width = "64")]
    PROVIDER_SYSCALL64_MONOTONIC => raw_clock(PROVIDER_SYSCALL64_MONOTONIC),
    #[cfg(target_pointer_width = "64")]
    PROVIDER_SYSCALL64_MONOTONIC_RAW => raw_clock(PROVIDER_SYSCALL64_MONOTONIC_RAW),
    #[cfg(target_pointer_width = "64")]
    PROVIDER_SYSCALL64_BOOTTIME => raw_clock(PROVIDER_SYSCALL64_BOOTTIME),
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME32_MONOTONIC => raw_clock(PROVIDER_TIME32_MONOTONIC),
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME32_MONOTONIC_RAW => raw_clock(PROVIDER_TIME32_MONOTONIC_RAW),
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME32_BOOTTIME => raw_clock(PROVIDER_TIME32_BOOTTIME),
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME64_MONOTONIC => raw_clock(PROVIDER_TIME64_MONOTONIC),
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME64_MONOTONIC_RAW => raw_clock(PROVIDER_TIME64_MONOTONIC_RAW),
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME64_BOOTTIME => raw_clock(PROVIDER_TIME64_BOOTTIME),
    _ => ticks_after_selection(),
  }
}

#[inline(never)]
fn read_outlined_instant_provider(provider: u8) -> u64 {
  read_instant_provider(provider)
}

#[cold]
#[inline(never)]
fn ticks_after_selection() -> u64 {
  read_instant_provider(selected_instant_provider())
}

#[inline(always)]
fn read_ordered_provider(provider: u8) -> u64 {
  match provider {
    PROVIDER_TSC_LFENCE_RDTSC => read_tsc_lfence_ordered(),
    PROVIDER_TSC_MFENCE_RDTSC => read_tsc_mfence_ordered(),
    PROVIDER_TSC_RDTSCP => read_tsc_rdtscp_ordered(),
    PROVIDER_TSC_CPUID_RDTSC => read_tsc_cpuid_ordered(),
    PROVIDER_TSC_SERIALIZE_RDTSC => read_tsc_serialize_ordered(),
    provider if is_ordered_os_provider(provider) => {
      execute_ordered_os_barrier(ordered_barrier(provider));
      read_source(ordered_source(provider))
    }
    _ => ticks_ordered_after_selection(),
  }
}

#[inline(never)]
fn read_outlined_ordered_provider(provider: u8) -> u64 {
  read_ordered_provider(provider)
}

#[cold]
#[inline(never)]
fn ticks_ordered_after_selection() -> u64 {
  read_ordered_provider(selected_ordered_provider())
}

#[inline(always)]
fn read_ordered_provider_unordered(provider: u8) -> u64 {
  match provider {
    PROVIDER_TSC_LFENCE_RDTSC
    | PROVIDER_TSC_MFENCE_RDTSC
    | PROVIDER_TSC_RDTSCP
    | PROVIDER_TSC_CPUID_RDTSC
    | PROVIDER_TSC_SERIALIZE_RDTSC => read_tsc(),
    provider if is_ordered_os_provider(provider) => read_source(ordered_source(provider)),
    _ => ticks_ordered_unordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_unordered_after_selection() -> u64 {
  read_ordered_provider_unordered(selected_ordered_provider())
}

#[inline(always)]
const fn ordered_provider(source: u8, barrier: u8) -> u8 {
  ORDERED_OS_BASE + source * ORDERED_BARRIER_VARIANTS + barrier
}

#[inline(always)]
const fn is_ordered_os_provider(provider: u8) -> bool {
  provider >= ORDERED_OS_BASE
    && provider < ORDERED_OS_BASE + SOURCE_VARIANTS * ORDERED_BARRIER_VARIANTS
}

#[inline(always)]
const fn ordered_source(provider: u8) -> u8 {
  (provider - ORDERED_OS_BASE) / ORDERED_BARRIER_VARIANTS
}

#[inline(always)]
const fn ordered_barrier(provider: u8) -> u8 {
  (provider - ORDERED_OS_BASE) % ORDERED_BARRIER_VARIANTS
}

#[inline(always)]
fn read_source(source: u8) -> u64 {
  match source {
    SOURCE_LIBC_MONOTONIC => libc_clock(PROVIDER_LIBC_MONOTONIC),
    SOURCE_LIBC_MONOTONIC_RAW => libc_clock(PROVIDER_LIBC_MONOTONIC_RAW),
    SOURCE_LIBC_BOOTTIME => libc_clock(PROVIDER_LIBC_BOOTTIME),
    SOURCE_VDSO_MONOTONIC | SOURCE_VDSO_MONOTONIC_RAW | SOURCE_VDSO_BOOTTIME => vdso_clock(source),
    #[cfg(target_pointer_width = "32")]
    SOURCE_VDSO_TIME64_MONOTONIC | SOURCE_VDSO_TIME64_MONOTONIC_RAW => vdso_clock(source),
    #[cfg(target_pointer_width = "32")]
    SOURCE_VDSO_TIME64_BOOTTIME => vdso_clock(source),
    #[cfg(target_pointer_width = "64")]
    SOURCE_SYSCALL64_MONOTONIC => raw_clock(PROVIDER_SYSCALL64_MONOTONIC),
    #[cfg(target_pointer_width = "64")]
    SOURCE_SYSCALL64_MONOTONIC_RAW => raw_clock(PROVIDER_SYSCALL64_MONOTONIC_RAW),
    #[cfg(target_pointer_width = "64")]
    SOURCE_SYSCALL64_BOOTTIME => raw_clock(PROVIDER_SYSCALL64_BOOTTIME),
    #[cfg(target_pointer_width = "32")]
    SOURCE_TIME32_MONOTONIC => raw_clock(PROVIDER_TIME32_MONOTONIC),
    #[cfg(target_pointer_width = "32")]
    SOURCE_TIME32_MONOTONIC_RAW => raw_clock(PROVIDER_TIME32_MONOTONIC_RAW),
    #[cfg(target_pointer_width = "32")]
    SOURCE_TIME32_BOOTTIME => raw_clock(PROVIDER_TIME32_BOOTTIME),
    #[cfg(target_pointer_width = "32")]
    SOURCE_TIME64_MONOTONIC => raw_clock(PROVIDER_TIME64_MONOTONIC),
    #[cfg(target_pointer_width = "32")]
    SOURCE_TIME64_MONOTONIC_RAW => raw_clock(PROVIDER_TIME64_MONOTONIC_RAW),
    #[cfg(target_pointer_width = "32")]
    SOURCE_TIME64_BOOTTIME => raw_clock(PROVIDER_TIME64_BOOTTIME),
    _ => 0,
  }
}

#[inline(always)]
fn execute_ordered_os_barrier(barrier: u8) {
  match barrier {
    ORDERED_BARRIER_OS_OWNED => {
      // The vDSO uses the kernel's ordered hardware-counter read. Bare raw
      // entry is eligible only for Intel SYSCALL or the i686 INT 0x80
      // exception boundary. Every call form carries a compiler memory barrier.
    }
    ORDERED_BARRIER_LFENCE => {
      // Omitting `nomem` is the compiler barrier.
      // SAFETY: eligibility requires SSE2 and either Intel or AMD's
      // LFENCE-always-serializing architectural contract.
      unsafe { core::arch::asm!("lfence", options(nostack, preserves_flags)) };
    }
    ORDERED_BARRIER_RDTSCP => {
      // RDTSCP waits for prior instructions and loads before its counter read;
      // the trailing LFENCE prevents the separate OS-clock read from starting
      // before RDTSCP completes. The timestamp itself is discarded.
      // SAFETY: candidate eligibility requires the architectural RDTSCP bit
      // and confirms this thread has permission to execute TSC instructions.
      unsafe {
        core::arch::asm!(
          "rdtscp",
          "lfence",
          lateout("eax") _,
          lateout("edx") _,
          lateout("ecx") _,
          options(nostack, preserves_flags),
        )
      };
    }
    ORDERED_BARRIER_MFENCE => {
      // SAFETY: candidate eligibility requires AMD SSE2 support.
      unsafe { core::arch::asm!("mfence", options(nostack, preserves_flags)) };
    }
    ORDERED_BARRIER_SERIALIZE => {
      // SAFETY: candidate eligibility requires CPUID.7.0:EDX[SERIALIZE].
      // Omitting `nomem` also supplies the compiler memory barrier.
      unsafe { core::arch::asm!("serialize", options(nostack, preserves_flags)) };
    }
    _ => execute_cpuid_barrier(),
  }
}

#[inline(always)]
fn execute_cpuid_barrier() {
  #[cfg(target_arch = "x86_64")]
  // SAFETY: x86_64 guarantees CPUID. RBX is restored after the instruction.
  unsafe {
    core::arch::asm!(
      "mov rsi, rbx",
      "xor eax, eax",
      "cpuid",
      "mov rbx, rsi",
      lateout("eax") _,
      lateout("ecx") _,
      lateout("edx") _,
      lateout("rsi") _,
      options(nostack),
    )
  };
  #[cfg(target_arch = "x86")]
  // SAFETY: i686 guarantees CPUID. The balanced stack pair preserves PIC EBX.
  unsafe {
    core::arch::asm!(
      "push ebx",
      "xor eax, eax",
      "cpuid",
      "pop ebx",
      lateout("eax") _,
      lateout("ecx") _,
      lateout("edx") _,
    )
  };
}

#[inline(always)]
fn read_tsc() -> u64 {
  #[cfg(target_arch = "x86_64")]
  {
    super::x86_64::rdtsc()
  }
  #[cfg(target_arch = "x86")]
  {
    super::x86::rdtsc()
  }
}

#[inline(always)]
fn read_tsc_lfence_ordered() -> u64 {
  #[cfg(target_arch = "x86_64")]
  {
    super::x86_64::read_lfence_rdtsc()
  }
  #[cfg(target_arch = "x86")]
  {
    super::x86::read_lfence_rdtsc()
  }
}

#[inline(always)]
fn read_tsc_mfence_ordered() -> u64 {
  #[cfg(target_arch = "x86_64")]
  {
    super::x86_64::read_mfence_rdtsc()
  }
  #[cfg(target_arch = "x86")]
  {
    super::x86::read_mfence_rdtsc()
  }
}

#[inline(always)]
fn read_tsc_rdtscp_ordered() -> u64 {
  #[cfg(target_arch = "x86_64")]
  {
    super::x86_64::read_rdtscp()
  }
  #[cfg(target_arch = "x86")]
  {
    super::x86::read_rdtscp()
  }
}

#[inline(always)]
fn read_tsc_cpuid_ordered() -> u64 {
  #[cfg(target_arch = "x86_64")]
  {
    super::x86_64::read_cpuid_rdtsc()
  }
  #[cfg(target_arch = "x86")]
  {
    super::x86::read_cpuid_rdtsc()
  }
}

#[inline(always)]
fn read_tsc_serialize_ordered() -> u64 {
  #[cfg(target_arch = "x86_64")]
  {
    super::x86_64::read_serialize_rdtsc()
  }
  #[cfg(target_arch = "x86")]
  {
    super::x86::read_serialize_rdtsc()
  }
}

fn selected_direct_tsc_provider() -> u8 {
  #[cfg(target_arch = "x86_64")]
  let kind = super::x86_64::selected_ordered_read_kind();
  #[cfg(target_arch = "x86")]
  let kind = super::x86::selected_ordered_read_kind();

  #[cfg(target_arch = "x86_64")]
  if kind == super::x86_64::ORDERED_READ_LFENCE_RDTSC {
    return PROVIDER_TSC_LFENCE_RDTSC;
  }
  #[cfg(target_arch = "x86")]
  if kind == super::x86::ORDERED_READ_LFENCE_RDTSC {
    return PROVIDER_TSC_LFENCE_RDTSC;
  }
  #[cfg(target_arch = "x86_64")]
  if kind == super::x86_64::ORDERED_READ_MFENCE_RDTSC {
    return PROVIDER_TSC_MFENCE_RDTSC;
  }
  #[cfg(target_arch = "x86")]
  if kind == super::x86::ORDERED_READ_MFENCE_RDTSC {
    return PROVIDER_TSC_MFENCE_RDTSC;
  }
  #[cfg(target_arch = "x86_64")]
  if kind == super::x86_64::ORDERED_READ_RDTSCP {
    return PROVIDER_TSC_RDTSCP;
  }
  #[cfg(target_arch = "x86")]
  if kind == super::x86::ORDERED_READ_RDTSCP {
    return PROVIDER_TSC_RDTSCP;
  }
  #[cfg(target_arch = "x86_64")]
  if kind == super::x86_64::ORDERED_READ_SERIALIZE_RDTSC {
    return PROVIDER_TSC_SERIALIZE_RDTSC;
  }
  #[cfg(target_arch = "x86")]
  if kind == super::x86::ORDERED_READ_SERIALIZE_RDTSC {
    return PROVIDER_TSC_SERIALIZE_RDTSC;
  }
  PROVIDER_TSC_CPUID_RDTSC
}

#[inline(always)]
const fn provider_clock_id(provider: u8) -> libc::clockid_t {
  if matches!(
    provider,
    PROVIDER_LIBC_BOOTTIME
      | PROVIDER_SYSCALL64_BOOTTIME
      | PROVIDER_TIME32_BOOTTIME
      | PROVIDER_TIME64_BOOTTIME
      | PROVIDER_VDSO_BOOTTIME
  ) {
    return libc::CLOCK_BOOTTIME;
  }
  #[cfg(target_pointer_width = "32")]
  if provider == PROVIDER_VDSO_TIME64_BOOTTIME {
    return libc::CLOCK_BOOTTIME;
  }
  if matches!(
    provider,
    PROVIDER_LIBC_MONOTONIC_RAW
      | PROVIDER_SYSCALL64_MONOTONIC_RAW
      | PROVIDER_TIME32_MONOTONIC_RAW
      | PROVIDER_TIME64_MONOTONIC_RAW
      | PROVIDER_VDSO_MONOTONIC_RAW
  ) {
    return libc::CLOCK_MONOTONIC_RAW;
  }
  #[cfg(target_pointer_width = "32")]
  if provider == PROVIDER_VDSO_TIME64_MONOTONIC_RAW {
    return libc::CLOCK_MONOTONIC_RAW;
  }
  libc::CLOCK_MONOTONIC
}

#[inline(always)]
fn libc_clock(provider: u8) -> u64 {
  let clock_id = provider_clock_id(provider);
  libc_clock_nanos(clock_id)
    .or_else(|| raw_clock_nanos_for_clock(clock_id))
    .unwrap_or(0)
}

#[inline(always)]
fn libc_clock_nanos(clock_id: libc::clockid_t) -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: value is writable libc-timespec storage. Candidate construction
  // probes each routed clock ID before it can be selected.
  let status = unsafe { libc::clock_gettime(clock_id, value.as_mut_ptr()) };
  if status != 0 {
    return None;
  }
  // SAFETY: clock_gettime initialized value on success.
  let value = unsafe { value.assume_init() };
  #[cfg(target_pointer_width = "64")]
  {
    time_parts_to_nanos(value.tv_sec, value.tv_nsec)
  }
  #[cfg(target_pointer_width = "32")]
  {
    time_parts_to_nanos(i64::from(value.tv_sec), i64::from(value.tv_nsec))
  }
}

#[inline(always)]
fn vdso_clock(source: u8) -> u64 {
  let clock_id = provider_clock_id(PROVIDER_SOURCE_BASE + source);
  vdso_clock_nanos(source)
    .or_else(|| libc_clock_nanos(clock_id))
    .or_else(|| raw_clock_nanos_for_clock(clock_id))
    .unwrap_or(0)
}

#[inline(always)]
fn vdso_clock_nanos(source: u8) -> Option<u64> {
  let clock_id = provider_clock_id(PROVIDER_SOURCE_BASE + source);
  #[cfg(target_pointer_width = "32")]
  if matches!(
    source,
    SOURCE_VDSO_TIME64_MONOTONIC | SOURCE_VDSO_TIME64_MONOTONIC_RAW | SOURCE_VDSO_TIME64_BOOTTIME
  ) {
    return super::linux_vdso::clock_nanos_time64(clock_id);
  }
  super::linux_vdso::clock_nanos(clock_id)
}

#[inline(always)]
fn raw_clock(provider: u8) -> u64 {
  raw_clock_nanos(provider)
    .or_else(|| libc_clock_nanos(provider_clock_id(provider)))
    .or_else(|| alternate_raw_clock_nanos(provider))
    .unwrap_or(0)
}

#[inline(always)]
fn raw_clock_nanos_for_clock(clock_id: libc::clockid_t) -> Option<u64> {
  #[cfg(target_pointer_width = "64")]
  {
    let provider = match clock_id {
      libc::CLOCK_MONOTONIC_RAW => PROVIDER_SYSCALL64_MONOTONIC_RAW,
      libc::CLOCK_BOOTTIME => PROVIDER_SYSCALL64_BOOTTIME,
      _ => PROVIDER_SYSCALL64_MONOTONIC,
    };
    raw_clock_nanos(provider)
  }
  #[cfg(target_pointer_width = "32")]
  {
    let (time64, time32) = match clock_id {
      libc::CLOCK_MONOTONIC_RAW => (PROVIDER_TIME64_MONOTONIC_RAW, PROVIDER_TIME32_MONOTONIC_RAW),
      libc::CLOCK_BOOTTIME => (PROVIDER_TIME64_BOOTTIME, PROVIDER_TIME32_BOOTTIME),
      _ => (PROVIDER_TIME64_MONOTONIC, PROVIDER_TIME32_MONOTONIC),
    };
    raw_clock_nanos(time64).or_else(|| raw_clock_nanos(time32))
  }
}

#[cfg(target_pointer_width = "64")]
#[inline(always)]
fn alternate_raw_clock_nanos(_provider: u8) -> Option<u64> {
  None
}

#[cfg(target_pointer_width = "32")]
#[inline(always)]
fn alternate_raw_clock_nanos(provider: u8) -> Option<u64> {
  match provider {
    PROVIDER_TIME32_MONOTONIC => raw_clock_nanos(PROVIDER_TIME64_MONOTONIC),
    PROVIDER_TIME32_MONOTONIC_RAW => raw_clock_nanos(PROVIDER_TIME64_MONOTONIC_RAW),
    PROVIDER_TIME32_BOOTTIME => raw_clock_nanos(PROVIDER_TIME64_BOOTTIME),
    PROVIDER_TIME64_MONOTONIC => raw_clock_nanos(PROVIDER_TIME32_MONOTONIC),
    PROVIDER_TIME64_MONOTONIC_RAW => raw_clock_nanos(PROVIDER_TIME32_MONOTONIC_RAW),
    PROVIDER_TIME64_BOOTTIME => raw_clock_nanos(PROVIDER_TIME32_BOOTTIME),
    _ => None,
  }
}

#[cfg(target_pointer_width = "64")]
#[inline(always)]
fn raw_clock_nanos(provider: u8) -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  let mut status = libc::SYS_clock_gettime;
  // SAFETY: x86_64 Linux takes the syscall number in RAX and arguments in
  // RDI/RSI. RCX and R11 are architecturally clobbered by SYSCALL. Omitting
  // `nomem` declares the kernel write through value.
  unsafe {
    core::arch::asm!(
      "syscall",
      inlateout("rax") status,
      in("rdi") provider_clock_id(provider),
      in("rsi") value.as_mut_ptr(),
      lateout("rcx") _,
      lateout("r11") _,
      options(nostack),
    );
  }
  if status != 0 {
    return None;
  }
  // SAFETY: the successful syscall initialized the native timespec.
  let value = unsafe { value.assume_init() };
  time_parts_to_nanos(value.tv_sec, value.tv_nsec)
}

#[cfg(target_pointer_width = "32")]
#[repr(C)]
struct LinuxTime32 {
  seconds: i32,
  nanos: i32,
}

#[cfg(target_pointer_width = "32")]
#[repr(C)]
struct LinuxKernelTimespec {
  seconds: i64,
  nanos: i64,
}

#[cfg(target_pointer_width = "32")]
const SYS_CLOCK_GETTIME64: libc::c_long = 403;

#[cfg(target_pointer_width = "32")]
#[inline(always)]
fn raw_clock_nanos(provider: u8) -> Option<u64> {
  match provider {
    PROVIDER_TIME32_MONOTONIC | PROVIDER_TIME32_MONOTONIC_RAW | PROVIDER_TIME32_BOOTTIME => {
      raw_clock_nanos_time32(provider)
    }
    PROVIDER_TIME64_MONOTONIC | PROVIDER_TIME64_MONOTONIC_RAW | PROVIDER_TIME64_BOOTTIME => {
      raw_clock_nanos_time64(provider)
    }
    _ => None,
  }
}

#[cfg(target_pointer_width = "32")]
#[inline(always)]
fn raw_clock_nanos_time32(provider: u8) -> Option<u64> {
  let mut value = MaybeUninit::<LinuxTime32>::uninit();
  // SAFETY: SYS_clock_gettime writes the i386 Linux time32 layout declared
  // above. The inline helper preserves PIC's EBX and declares the kernel's
  // memory and flags effects.
  let status = unsafe {
    i686_clock_gettime(
      libc::SYS_clock_gettime,
      provider_clock_id(provider),
      value.as_mut_ptr().cast(),
    )
  };
  if status != 0 {
    return None;
  }
  // SAFETY: the successful syscall initialized value.
  let value = unsafe { value.assume_init() };
  time_parts_to_nanos(i64::from(value.seconds), i64::from(value.nanos))
}

#[cfg(target_pointer_width = "32")]
#[inline(always)]
fn raw_clock_nanos_time64(provider: u8) -> Option<u64> {
  let mut value = MaybeUninit::<LinuxKernelTimespec>::uninit();
  // SAFETY: syscall 403 writes the 32-bit Linux kernel-timespec layout.
  let status = unsafe {
    i686_clock_gettime(SYS_CLOCK_GETTIME64, provider_clock_id(provider), value.as_mut_ptr().cast())
  };
  if status != 0 {
    return None;
  }
  // SAFETY: the successful syscall initialized value.
  let value = unsafe { value.assume_init() };
  time_parts_to_nanos(value.seconds, value.nanos)
}

#[cfg(target_pointer_width = "32")]
#[inline(always)]
unsafe fn i686_clock_gettime(
  number: libc::c_long,
  clock_id: libc::clockid_t,
  value: *mut core::ffi::c_void,
) -> libc::c_long {
  let status: libc::c_long;
  // SAFETY: Linux i386 takes the number in EAX and arguments in EBX/ECX. LLVM can
  // reserve EBX for PIC, so a balanced push/pop preserves it around int 0x80.
  // The i386 syscall ABI preserves every other general register and returns
  // only through EAX; default asm options conservatively clobber flags and
  // memory. The balanced stack use intentionally omits `nostack`.
  unsafe {
    core::arch::asm!(
      "push ebx",
      "mov ebx, {clock_id:e}",
      "int 0x80",
      "pop ebx",
      clock_id = in(reg) clock_id,
      inlateout("eax") number => status,
      in("ecx") value,
    );
  }
  status
}

#[inline(always)]
fn time_parts_to_nanos(seconds: i64, nanos: i64) -> Option<u64> {
  let seconds = u64::try_from(seconds).ok()?;
  let nanos = u32::try_from(nanos).ok()?;
  if nanos >= 1_000_000_000 {
    return None;
  }
  seconds.checked_mul(1_000_000_000)?.checked_add(u64::from(nanos))
}

#[inline]
pub fn frequency() -> u64 {
  frequency_for(selected_instant_provider())
}

#[inline]
fn provider_uses_tsc(provider: u8) -> bool {
  matches!(
    provider,
    PROVIDER_TSC
      | PROVIDER_TSC_RDTSCP
      | PROVIDER_TSC_CPUID_RDTSC
      | PROVIDER_TSC_MFENCE_RDTSC
      | PROVIDER_TSC_SERIALIZE_RDTSC
  )
}

#[derive(Clone, Copy)]
struct TscFrequencyWindow {
  wall_start: u64,
  tick_start: u64,
}

fn begin_tsc_frequency_window(eligibility: TscEligibility) -> Option<TscFrequencyWindow> {
  if eligibility != TscEligibility::Eligible || TSC_FREQUENCY.load(Ordering::Relaxed) != 0 {
    return None;
  }
  let wall_start = calibration_wall_nanos()?;
  let tick_start = read_tsc();
  Some(TscFrequencyWindow { wall_start, tick_start })
}

fn finish_tsc_frequency_window(window: Option<TscFrequencyWindow>) {
  let Some(window) = window else {
    return;
  };
  let tick_end = read_tsc();
  let Some(wall_end) = calibration_wall_nanos() else {
    return;
  };
  let Some(frequency) = frequency_from_window(window, wall_end, tick_end) else {
    return;
  };
  let _ = TSC_FREQUENCY.compare_exchange(0, frequency, Ordering::Relaxed, Ordering::Relaxed);
}

fn frequency_from_window(window: TscFrequencyWindow, wall_end: u64, tick_end: u64) -> Option<u64> {
  let wall_elapsed = wall_end.checked_sub(window.wall_start)?;
  let ticks = tick_end.checked_sub(window.tick_start)?;
  if wall_elapsed == 0 || ticks == 0 {
    return None;
  }
  u64::try_from(u128::from(ticks) * 1_000_000_000 / u128::from(wall_elapsed))
    .ok()
    .filter(|frequency| *frequency != 0)
}

#[inline]
fn frequency_for(provider: u8) -> u64 {
  if !provider_uses_tsc(provider) {
    return 1_000_000_000;
  }
  let cached = TSC_FREQUENCY.load(Ordering::Relaxed);
  if cached != 0 {
    return cached;
  }

  #[cfg(target_arch = "x86_64")]
  if let Some(hz) = super::x86_64::cpuid_tsc_hz() {
    let hz = hz.max(1);
    TSC_FREQUENCY.store(hz, Ordering::Relaxed);
    return hz;
  }
  #[cfg(target_arch = "x86")]
  if let Some(hz) = super::x86::cpuid_tsc_hz() {
    let hz = hz.max(1);
    TSC_FREQUENCY.store(hz, Ordering::Relaxed);
    return hz;
  }

  let hz = calibrate_tsc_frequency().max(1);
  TSC_FREQUENCY.store(hz, Ordering::Relaxed);
  hz
}

fn calibrate_tsc_frequency() -> u64 {
  const WINDOW_NS: u64 = 100_000_000;
  const MAX_OVERRUN_NS: u64 = WINDOW_NS * 3 / 2;
  const SAMPLE_COUNT: usize = 7;

  let mut samples = [0_u64; SAMPLE_COUNT];
  let mut accepted = 0;
  for _ in 0..SAMPLE_COUNT {
    let Some(wall_start) = calibration_wall_nanos() else {
      return 1_000_000_000;
    };
    let tick_start = read_tsc();
    let mut wall_end;
    loop {
      let Some(sample) = calibration_wall_nanos() else {
        return 1_000_000_000;
      };
      wall_end = sample;
      if wall_end.saturating_sub(wall_start) >= WINDOW_NS {
        break;
      }
      core::hint::spin_loop();
    }
    let tick_end = read_tsc();
    let wall_elapsed = wall_end.saturating_sub(wall_start);
    if wall_elapsed == 0 || wall_elapsed > MAX_OVERRUN_NS {
      continue;
    }
    let ticks = tick_end.saturating_sub(tick_start);
    samples[accepted] =
      u64::try_from(u128::from(ticks).saturating_mul(1_000_000_000) / u128::from(wall_elapsed))
        .unwrap_or(u64::MAX);
    accepted += 1;
  }
  if accepted == 0 {
    return 1_000_000_000;
  }
  samples[..accepted].sort_unstable();
  samples[accepted / 2]
}

fn calibration_wall_nanos() -> Option<u64> {
  libc_clock_nanos(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| {
      #[cfg(target_pointer_width = "64")]
      {
        raw_clock_nanos(PROVIDER_SYSCALL64_MONOTONIC_RAW)
      }
      #[cfg(target_pointer_width = "32")]
      {
        raw_clock_nanos(PROVIDER_TIME64_MONOTONIC_RAW)
          .or_else(|| raw_clock_nanos(PROVIDER_TIME32_MONOTONIC_RAW))
      }
    })
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC))
    .or_else(|| {
      #[cfg(target_pointer_width = "64")]
      {
        raw_clock_nanos(PROVIDER_SYSCALL64_MONOTONIC)
      }
      #[cfg(target_pointer_width = "32")]
      {
        raw_clock_nanos(PROVIDER_TIME64_MONOTONIC)
          .or_else(|| raw_clock_nanos(PROVIDER_TIME32_MONOTONIC))
      }
    })
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
    PROVIDER_TSC
    | PROVIDER_VDSO_MONOTONIC
    | PROVIDER_VDSO_MONOTONIC_RAW
    | PROVIDER_VDSO_BOOTTIME => crate::ThreadCpuReadCost::Inline,
    #[cfg(target_pointer_width = "32")]
    PROVIDER_VDSO_TIME64_MONOTONIC
    | PROVIDER_VDSO_TIME64_MONOTONIC_RAW
    | PROVIDER_VDSO_TIME64_BOOTTIME => crate::ThreadCpuReadCost::Inline,
    // The libc ABI may use the vDSO or enter the kernel. Without resolving and
    // owning a userspace implementation, the conservative class is a system
    // call even when runtime measurements make this route the fastest.
    _ => crate::ThreadCpuReadCost::SystemCall,
  }
}

#[cfg(test)]
#[test]
fn instant_read_cost_only_marks_guaranteed_userspace_paths_inline() {
  assert_eq!(instant_read_cost_for(PROVIDER_TSC), crate::ThreadCpuReadCost::Inline);
  assert_eq!(instant_read_cost_for(PROVIDER_VDSO_MONOTONIC), crate::ThreadCpuReadCost::Inline,);
  assert_eq!(instant_read_cost_for(PROVIDER_VDSO_BOOTTIME), crate::ThreadCpuReadCost::Inline,);
  assert_eq!(instant_read_cost_for(PROVIDER_LIBC_MONOTONIC), crate::ThreadCpuReadCost::SystemCall,);
  assert_eq!(
    instant_read_cost_for(PROVIDER_LIBC_MONOTONIC_RAW),
    crate::ThreadCpuReadCost::SystemCall,
  );
  assert_eq!(
    instant_read_cost_for(PROVIDER_SYSCALL64_MONOTONIC),
    crate::ThreadCpuReadCost::SystemCall,
  );
}

#[inline]
pub(crate) fn ordered_uses_tsc() -> bool {
  provider_uses_tsc(selected_ordered_provider())
}

fn selected_instant_provider() -> u8 {
  super::select_thread_owned_process_provider(
    &INSTANT_PROVIDER,
    PROVIDER_UNKNOWN,
    PROVIDER_SELECTING,
    &INSTANT_PROVIDER_OWNER_PID,
    &INSTANT_PROVIDER_OWNER_TID,
    REENTRANT_INSTANT_PROVIDER,
    detect_instant_provider,
  )
}

fn selected_ordered_provider() -> u8 {
  super::select_thread_owned_process_provider(
    &ORDERED_PROVIDER,
    PROVIDER_UNKNOWN,
    PROVIDER_SELECTING,
    &ORDERED_PROVIDER_OWNER_PID,
    &ORDERED_PROVIDER_OWNER_TID,
    REENTRANT_ORDERED_PROVIDER,
    detect_ordered_provider,
  )
}

#[cold]
#[inline(never)]
fn detect_instant_provider() -> u8 {
  let eligibility = detect_tsc_eligibility();
  let frequency_window = begin_tsc_frequency_window(eligibility);
  let allow_libc = eligibility != TscEligibility::TscReadDisabled;
  if allow_libc {
    let _ = super::linux_vdso::install();
  }
  let mut candidates = instant_os_candidates(allow_libc);
  let fallback_count = candidates.count;
  if eligibility == TscEligibility::Eligible {
    candidates.push(PROVIDER_TSC);
  }
  ensure_instant_candidate(&mut candidates, allow_libc);
  let stopwatch = selection_stopwatch(&candidates);
  let samples = measure_instant_candidates(candidates, stopwatch);
  finish_tsc_frequency_window(frequency_window);
  let tournament: Tournament<MAX_INSTANT_CANDIDATES, MAX_INSTANT_DECISIONS> =
    run_tournament(candidates, samples);
  #[cfg(feature = "bench-internal")]
  let fallback_provider = tournament_winner_prefix(&tournament, fallback_count);
  #[cfg(not(feature = "bench-internal"))]
  let _ = fallback_count;
  #[cfg(feature = "bench-internal")]
  store_instant_evidence(DomainEvidence {
    eligibility,
    tournament,
    fallback_provider,
    barriers: BarrierList::conservative(),
    baseline_barrier: ORDERED_BARRIER_CPUID,
    fast_barrier: None,
  });
  tournament.selected_provider
}

#[cold]
#[inline(never)]
fn detect_ordered_provider() -> u8 {
  let eligibility = detect_tsc_eligibility();
  let frequency_window = begin_tsc_frequency_window(eligibility);
  let allow_libc = eligibility != TscEligibility::TscReadDisabled;
  if allow_libc {
    let _ = super::linux_vdso::install();
  }
  let sources = instant_os_candidates(allow_libc);
  let barriers = eligible_ordered_os_barriers();
  let bare_syscall_os_owned = bare_syscall_os_owned_eligible();
  let baseline_barrier = barriers.barriers[0];
  #[cfg(feature = "bench-internal")]
  let fast_barrier = if barriers.count > 1 { Some(barriers.barriers[1]) } else { None };
  let mut candidates = CandidateList::<MAX_ORDERED_CANDIDATES>::new();
  for &source_provider in &sources.providers[..sources.count] {
    let source = source_provider - PROVIDER_SOURCE_BASE;
    for &barrier in &barriers.barriers[..barriers.count] {
      if barrier == ORDERED_BARRIER_OS_OWNED
        && !os_owned_eligible_for_source(source, bare_syscall_os_owned)
      {
        continue;
      }
      candidates.push(ordered_provider(source, barrier));
    }
  }
  let fallback_count = candidates.count;
  if eligibility == TscEligibility::Eligible {
    candidates.push(selected_direct_tsc_provider());
  }
  ensure_ordered_candidate(&mut candidates, baseline_barrier, allow_libc);
  let stopwatch = selection_stopwatch(&sources);
  let samples = measure_ordered_candidates(candidates, stopwatch);
  finish_tsc_frequency_window(frequency_window);
  let tournament: Tournament<MAX_ORDERED_CANDIDATES, MAX_ORDERED_DECISIONS> =
    run_tournament(candidates, samples);
  #[cfg(feature = "bench-internal")]
  let fallback_provider = tournament_winner_prefix(&tournament, fallback_count);
  #[cfg(not(feature = "bench-internal"))]
  let _ = fallback_count;
  #[cfg(feature = "bench-internal")]
  store_ordered_evidence(DomainEvidence {
    eligibility,
    tournament,
    fallback_provider,
    barriers,
    baseline_barrier,
    fast_barrier,
  });
  let provider = tournament.selected_provider;
  let scale = super::scale_from_ratio(1_000_000_000, frequency_for(provider));
  super::publish_ordered_nanos_per_tick_q32(scale);
  provider
}

#[inline]
const fn os_owned_eligible_for_source(source: u8, bare_syscall_os_owned: bool) -> bool {
  if matches!(source, SOURCE_LIBC_MONOTONIC | SOURCE_LIBC_MONOTONIC_RAW | SOURCE_LIBC_BOOTTIME) {
    // libc may choose either its vDSO route or a bare syscall at each call.
    // Its OS-owned boundary is eligible only where both outcomes satisfy the
    // ordered contract; direct-vDSO sources remain independently eligible.
    return bare_syscall_os_owned;
  }
  #[cfg(target_pointer_width = "64")]
  {
    if matches!(
      source,
      SOURCE_SYSCALL64_MONOTONIC | SOURCE_SYSCALL64_MONOTONIC_RAW | SOURCE_SYSCALL64_BOOTTIME
    ) {
      return bare_syscall_os_owned;
    }
  }
  #[cfg(target_pointer_width = "32")]
  let _ = bare_syscall_os_owned;
  true
}

#[allow(unused_unsafe)] // supported rustc versions differ on whether __cpuid is unsafe
fn bare_syscall_os_owned_eligible() -> bool {
  #[cfg(target_pointer_width = "32")]
  {
    // The i686 raw ABI enters through INT 0x80, an exception boundary.
    true
  }
  #[cfg(target_pointer_width = "64")]
  {
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::__cpuid;

    const INTEL: (u32, u32, u32) = (0x756e_6547, 0x4965_6e69, 0x6c65_746e);
    // SAFETY: x86_64 guarantees CPUID leaf zero.
    let basic = unsafe { __cpuid(0) };
    (basic.ebx, basic.edx, basic.ecx) == INTEL
  }
}

#[cfg(feature = "bench-internal")]
fn bare_syscall_os_owned_basis() -> &'static str {
  #[cfg(target_pointer_width = "32")]
  {
    "eligible_i686_int80_exception_boundary"
  }
  #[cfg(target_pointer_width = "64")]
  {
    if bare_syscall_os_owned_eligible() {
      "eligible_intel_syscall_ordering_contract"
    } else {
      "ineligible_no_amd_or_unknown_vendor_syscall_ordering_contract"
    }
  }
}

fn instant_os_candidates(allow_libc: bool) -> CandidateList<MAX_INSTANT_CANDIDATES> {
  let mut candidates = CandidateList::new();
  if allow_libc && provider_available(PROVIDER_LIBC_MONOTONIC) {
    candidates.push(PROVIDER_LIBC_MONOTONIC);
  }
  if allow_libc && provider_available(PROVIDER_LIBC_MONOTONIC_RAW) {
    candidates.push(PROVIDER_LIBC_MONOTONIC_RAW);
  }
  if allow_libc && provider_available(PROVIDER_LIBC_BOOTTIME) {
    candidates.push(PROVIDER_LIBC_BOOTTIME);
  }
  if allow_libc && provider_available(PROVIDER_VDSO_MONOTONIC) {
    candidates.push(PROVIDER_VDSO_MONOTONIC);
  }
  if allow_libc && provider_available(PROVIDER_VDSO_MONOTONIC_RAW) {
    candidates.push(PROVIDER_VDSO_MONOTONIC_RAW);
  }
  if allow_libc && provider_available(PROVIDER_VDSO_BOOTTIME) {
    candidates.push(PROVIDER_VDSO_BOOTTIME);
  }

  #[cfg(target_pointer_width = "64")]
  for provider in
    [PROVIDER_SYSCALL64_MONOTONIC, PROVIDER_SYSCALL64_MONOTONIC_RAW, PROVIDER_SYSCALL64_BOOTTIME]
  {
    if provider_available(provider) {
      candidates.push(provider);
    }
  }
  #[cfg(target_pointer_width = "32")]
  for provider in [
    PROVIDER_TIME32_MONOTONIC,
    PROVIDER_TIME32_MONOTONIC_RAW,
    PROVIDER_TIME64_MONOTONIC,
    PROVIDER_TIME64_MONOTONIC_RAW,
    PROVIDER_TIME32_BOOTTIME,
    PROVIDER_TIME64_BOOTTIME,
  ] {
    if provider_available(provider) {
      candidates.push(provider);
    }
  }
  #[cfg(target_pointer_width = "32")]
  for provider in [
    PROVIDER_VDSO_TIME64_MONOTONIC,
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW,
    PROVIDER_VDSO_TIME64_BOOTTIME,
  ] {
    if allow_libc && provider_available(provider) {
      candidates.push(provider);
    }
  }

  ensure_instant_candidate(&mut candidates, allow_libc);
  candidates
}

fn ensure_instant_candidate(
  candidates: &mut CandidateList<MAX_INSTANT_CANDIDATES>,
  allow_libc: bool,
) {
  if candidates.count != 0 {
    return;
  }
  // A live policy can theoretically revoke every syscall after probing. Keep
  // construction total without crossing domains: the emergency provider is
  // one exact ABI/clock pair and will return zero if it remains unavailable.
  if allow_libc {
    candidates.push(PROVIDER_LIBC_MONOTONIC);
  } else {
    #[cfg(target_pointer_width = "64")]
    candidates.push(PROVIDER_SYSCALL64_MONOTONIC);
    #[cfg(target_pointer_width = "32")]
    candidates.push(PROVIDER_TIME64_MONOTONIC);
  }
}

fn ensure_ordered_candidate(
  candidates: &mut CandidateList<MAX_ORDERED_CANDIDATES>,
  baseline_barrier: u8,
  allow_libc: bool,
) {
  if candidates.count != 0 {
    return;
  }
  let source_provider = if allow_libc {
    PROVIDER_LIBC_MONOTONIC
  } else {
    #[cfg(target_pointer_width = "64")]
    {
      PROVIDER_SYSCALL64_MONOTONIC
    }
    #[cfg(target_pointer_width = "32")]
    {
      PROVIDER_TIME64_MONOTONIC
    }
  };
  candidates.push(ordered_provider(source_provider - PROVIDER_SOURCE_BASE, baseline_barrier));
}

fn provider_available(provider: u8) -> bool {
  match provider {
    PROVIDER_LIBC_MONOTONIC | PROVIDER_LIBC_MONOTONIC_RAW | PROVIDER_LIBC_BOOTTIME => {
      libc_clock_nanos(provider_clock_id(provider)).is_some()
    }
    PROVIDER_VDSO_MONOTONIC | PROVIDER_VDSO_MONOTONIC_RAW | PROVIDER_VDSO_BOOTTIME => {
      vdso_clock_nanos(provider - PROVIDER_SOURCE_BASE).is_some()
    }
    #[cfg(target_pointer_width = "32")]
    PROVIDER_VDSO_TIME64_MONOTONIC
    | PROVIDER_VDSO_TIME64_MONOTONIC_RAW
    | PROVIDER_VDSO_TIME64_BOOTTIME => vdso_clock_nanos(provider - PROVIDER_SOURCE_BASE).is_some(),
    _ => raw_clock_nanos(provider).is_some(),
  }
}

fn selection_stopwatch(candidates: &CandidateList<MAX_INSTANT_CANDIDATES>) -> u8 {
  #[cfg(target_pointer_width = "64")]
  const PREFERENCE: [u8; 4] = [
    PROVIDER_SYSCALL64_MONOTONIC_RAW,
    PROVIDER_SYSCALL64_MONOTONIC,
    PROVIDER_LIBC_MONOTONIC_RAW,
    PROVIDER_LIBC_MONOTONIC,
  ];
  #[cfg(target_pointer_width = "32")]
  const PREFERENCE: [u8; 6] = [
    PROVIDER_TIME64_MONOTONIC_RAW,
    PROVIDER_TIME32_MONOTONIC_RAW,
    PROVIDER_TIME64_MONOTONIC,
    PROVIDER_TIME32_MONOTONIC,
    PROVIDER_LIBC_MONOTONIC_RAW,
    PROVIDER_LIBC_MONOTONIC,
  ];

  for preferred in PREFERENCE {
    if candidates.providers[..candidates.count].contains(&preferred) {
      return preferred;
    }
  }
  candidates.providers[0]
}

fn measure_instant_candidates(
  candidates: CandidateList<MAX_INSTANT_CANDIDATES>,
  stopwatch: u8,
) -> ProbeSamples<MAX_INSTANT_CANDIDATES> {
  for &provider in &candidates.providers[..candidates.count] {
    INSTANT_PROBE_PROVIDER.store(provider, Ordering::Relaxed);
    for _ in 0..PROBE_WARMUP_READS {
      black_box(probe_instant_hot_path());
    }
  }

  let mut samples = ProbeSamples { batches: [[u64::MAX; PROBE_BATCHES]; MAX_INSTANT_CANDIDATES] };
  for sample in 0..PROBE_BATCHES {
    for offset in 0..candidates.count {
      let index = (sample + offset) % candidates.count;
      let provider = candidates.providers[index];
      samples.batches[index][sample] = measure_instant_batch(provider, stopwatch);
    }
  }
  samples
}

fn measure_ordered_candidates(
  candidates: CandidateList<MAX_ORDERED_CANDIDATES>,
  stopwatch: u8,
) -> ProbeSamples<MAX_ORDERED_CANDIDATES> {
  for &provider in &candidates.providers[..candidates.count] {
    ORDERED_PROBE_PROVIDER.store(provider, Ordering::Relaxed);
    for _ in 0..PROBE_WARMUP_READS {
      black_box(probe_ordered_hot_path());
    }
  }

  let mut samples = ProbeSamples { batches: [[u64::MAX; PROBE_BATCHES]; MAX_ORDERED_CANDIDATES] };
  for sample in 0..PROBE_BATCHES {
    for offset in 0..candidates.count {
      let index = (sample + offset) % candidates.count;
      let provider = candidates.providers[index];
      samples.batches[index][sample] = measure_ordered_batch(provider, stopwatch);
    }
  }
  samples
}

#[inline(always)]
fn probe_instant_hot_path() -> u64 {
  read_instant_provider(INSTANT_PROBE_PROVIDER.load(Ordering::Relaxed))
}

#[inline(always)]
fn probe_ordered_hot_path() -> u64 {
  read_ordered_provider(ORDERED_PROBE_PROVIDER.load(Ordering::Relaxed))
}

#[inline(never)]
fn measure_instant_batch(provider: u8, stopwatch: u8) -> u64 {
  INSTANT_PROBE_PROVIDER.store(provider, Ordering::Relaxed);
  let Some(start) = provider_nanos(stopwatch) else {
    return u64::MAX;
  };
  let mut sink = 0_u64;
  for _ in 0..PROBE_READS {
    sink ^= probe_instant_hot_path();
  }
  let elapsed = provider_nanos(stopwatch).and_then(|end| end.checked_sub(start));
  black_box(sink);
  elapsed.unwrap_or(u64::MAX)
}

#[inline(never)]
fn measure_ordered_batch(provider: u8, stopwatch: u8) -> u64 {
  ORDERED_PROBE_PROVIDER.store(provider, Ordering::Relaxed);
  let Some(start) = provider_nanos(stopwatch) else {
    return u64::MAX;
  };
  let mut sink = 0_u64;
  for _ in 0..PROBE_READS {
    sink ^= probe_ordered_hot_path();
  }
  let elapsed = provider_nanos(stopwatch).and_then(|end| end.checked_sub(start));
  black_box(sink);
  elapsed.unwrap_or(u64::MAX)
}

fn provider_nanos(provider: u8) -> Option<u64> {
  match provider {
    PROVIDER_LIBC_MONOTONIC | PROVIDER_LIBC_MONOTONIC_RAW | PROVIDER_LIBC_BOOTTIME => {
      libc_clock_nanos(provider_clock_id(provider))
    }
    PROVIDER_VDSO_MONOTONIC | PROVIDER_VDSO_MONOTONIC_RAW | PROVIDER_VDSO_BOOTTIME => {
      vdso_clock_nanos(provider - PROVIDER_SOURCE_BASE)
    }
    #[cfg(target_pointer_width = "32")]
    PROVIDER_VDSO_TIME64_MONOTONIC
    | PROVIDER_VDSO_TIME64_MONOTONIC_RAW
    | PROVIDER_VDSO_TIME64_BOOTTIME => vdso_clock_nanos(provider - PROVIDER_SOURCE_BASE),
    _ => raw_clock_nanos(provider),
  }
}

fn run_tournament<const N: usize, const D: usize>(
  candidates: CandidateList<N>,
  samples: ProbeSamples<N>,
) -> Tournament<N, D> {
  debug_assert!(candidates.count > 0);
  let mut selected_index = 0;
  let mut challengers = [PROVIDER_UNKNOWN; D];
  let mut incumbents = [PROVIDER_UNKNOWN; D];
  let mut winners = [PROVIDER_UNKNOWN; D];
  let mut decisions = [empty_decision(); D];
  for challenger_index in 1..candidates.count {
    let decision =
      evaluate_challenger(samples.batches[challenger_index], samples.batches[selected_index]);
    let slot = challenger_index - 1;
    challengers[slot] = candidates.providers[challenger_index];
    incumbents[slot] = candidates.providers[selected_index];
    decisions[slot] = decision;
    if decision.challenger_selected {
      selected_index = challenger_index;
    }
    winners[slot] = candidates.providers[selected_index];
  }
  Tournament {
    selected_provider: candidates.providers[selected_index],
    decision_count: candidates.count.saturating_sub(1),
    challengers,
    incumbents,
    winners,
    decisions,
    samples,
    candidates,
  }
}

#[cfg(feature = "bench-internal")]
fn tournament_winner_prefix<const N: usize, const D: usize>(
  tournament: &Tournament<N, D>,
  prefix_count: usize,
) -> u8 {
  debug_assert!(prefix_count > 0);
  let mut selected_index = 0;
  for challenger_index in 1..prefix_count {
    if evaluate_challenger(
      tournament.samples.batches[challenger_index],
      tournament.samples.batches[selected_index],
    )
    .challenger_selected
    {
      selected_index = challenger_index;
    }
  }
  tournament.candidates.providers[selected_index]
}

fn evaluate_challenger(
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

const fn empty_decision() -> SelectionDecision {
  SelectionDecision {
    #[cfg(feature = "bench-internal")]
    allowance: 0,
    #[cfg(feature = "bench-internal")]
    decisive_wins: 0,
    challenger_selected: false,
  }
}

fn median(mut values: [u64; PROBE_BATCHES]) -> u64 {
  values.sort_unstable();
  values[values.len() / 2]
}

#[allow(unused_unsafe)] // supported rustc versions differ on whether __cpuid is unsafe
fn eligible_ordered_os_barriers() -> BarrierList {
  #[cfg(target_arch = "x86")]
  use core::arch::x86::{__cpuid, __cpuid_count};
  #[cfg(target_arch = "x86_64")]
  use core::arch::x86_64::{__cpuid, __cpuid_count};

  const INTEL: (u32, u32, u32) = (0x756e_6547, 0x4965_6e69, 0x6c65_746e);
  const AMD: (u32, u32, u32) = (0x6874_7541, 0x6974_6e65, 0x444d_4163);
  // SAFETY: supported x86 targets guarantee CPUID leaf zero.
  let basic = unsafe { __cpuid(0) };
  let vendor = (basic.ebx, basic.edx, basic.ecx);
  // SAFETY: leaf one is queried only when the maximum basic leaf includes it.
  let has_sse2 = basic.eax >= 1 && unsafe { __cpuid(1) }.edx & (1 << 26) != 0;
  // SAFETY: the extended maximum-leaf query is defined on CPUID systems.
  let extended = unsafe { __cpuid(0x8000_0000) };
  // SAFETY: the maximum extended leaf includes feature leaf 0x80000001.
  let has_rdtscp = tsc_read_enabled()
    && extended.eax >= 0x8000_0001
    && unsafe { __cpuid(0x8000_0001) }.edx & (1 << 27) != 0;
  // SAFETY: the maximum extended leaf includes AMD feature leaf 0x80000021.
  let has_amd_serializing_lfence =
    has_sse2 && extended.eax >= 0x8000_0021 && unsafe { __cpuid(0x8000_0021) }.eax & (1 << 2) != 0;
  // SAFETY: the maximum basic leaf includes structured feature leaf 7, subleaf 0.
  let has_serialize = basic.eax >= 7 && unsafe { __cpuid_count(7, 0) }.edx & (1 << 14) != 0;
  ordered_barriers_from_capabilities(
    vendor == INTEL,
    vendor == AMD,
    has_sse2,
    has_rdtscp,
    has_amd_serializing_lfence,
    has_serialize,
  )
}

fn ordered_barriers_from_capabilities(
  intel: bool,
  amd: bool,
  has_sse2: bool,
  has_rdtscp: bool,
  has_amd_serializing_lfence: bool,
  has_serialize: bool,
) -> BarrierList {
  let mut barriers = BarrierList::conservative();
  barriers.push(ORDERED_BARRIER_OS_OWNED);
  if intel {
    if has_sse2 {
      barriers.push(ORDERED_BARRIER_LFENCE);
    }
  } else if amd {
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

fn detect_tsc_eligibility() -> TscEligibility {
  if !tsc_read_enabled() {
    return TscEligibility::TscReadDisabled;
  }
  let (has_tsc, invariant_tsc) = tsc_capabilities();
  if !has_tsc {
    return TscEligibility::MissingTsc;
  }
  if !invariant_tsc {
    return TscEligibility::MissingInvariantTsc;
  }
  match kernel_uses_tsc_clocksource() {
    None => TscEligibility::KernelClocksourceMetadataUnavailable,
    Some(false) => TscEligibility::KernelTscUnavailable,
    Some(true) => TscEligibility::Eligible,
  }
}

#[allow(unused_unsafe)] // supported rustc versions differ on whether __cpuid is unsafe
fn tsc_capabilities() -> (bool, bool) {
  #[cfg(target_arch = "x86")]
  use core::arch::x86::__cpuid;
  #[cfg(target_arch = "x86_64")]
  use core::arch::x86_64::__cpuid;

  // SAFETY: supported x86 targets guarantee CPUID leaf zero.
  let basic = unsafe { __cpuid(0) };
  if basic.eax < 1 {
    return (false, false);
  }
  // SAFETY: the maximum basic leaf includes leaf one.
  let has_tsc = unsafe { __cpuid(1) }.edx & (1 << 4) != 0;
  // SAFETY: the extended maximum-leaf query is defined on CPUID systems.
  let extended = unsafe { __cpuid(0x8000_0000) };
  // SAFETY: the maximum extended leaf includes invariant-TSC metadata.
  let invariant_tsc =
    extended.eax >= 0x8000_0007 && unsafe { __cpuid(0x8000_0007) }.edx & (1 << 8) != 0;
  (has_tsc, invariant_tsc)
}

fn tsc_read_enabled() -> bool {
  let mut mode: libc::c_int = 0;
  // SAFETY: PR_GET_TSC writes one c_int through this valid pointer.
  let status = unsafe { libc::prctl(PR_GET_TSC, &mut mode as *mut libc::c_int) };
  status == 0 && mode == PR_TSC_ENABLE
}

fn kernel_uses_tsc_clocksource() -> Option<bool> {
  // `available_clocksource` can retain a watchdog-demoted TSC with rating zero,
  // especially when one-shot mode is disabled. Only the active source is an
  // initial kernel reliability decision. A later watchdog demotion is an
  // external reliability transition: Linux exposes no stable userspace bit
  // proving that the active TSC is exempt from future watchdog verification.
  const CURRENT: &core::ffi::CStr =
    c"/sys/devices/system/clocksource/clocksource0/current_clocksource";
  read_clocksource_file(CURRENT).map(|bytes| current_clocksource_is_tsc(&bytes))
}

fn read_clocksource_file(path: &core::ffi::CStr) -> Option<[u8; 128]> {
  // SAFETY: path is NUL-terminated and no creation mode is needed.
  let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
  if fd < 0 {
    return None;
  }
  let mut bytes = [0_u8; 128];
  // SAFETY: bytes is writable and fd is open.
  let read = unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len()) };
  // SAFETY: fd was returned by open and is closed once.
  let _ = unsafe { libc::close(fd) };
  if read <= 0 {
    return None;
  }
  let read = usize::try_from(read).ok()?.min(bytes.len());
  if read < bytes.len() {
    bytes[read] = 0;
  }
  Some(bytes)
}

fn current_clocksource_is_tsc(bytes: &[u8]) -> bool {
  let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
  let value = &bytes[..end];
  let start = value.iter().position(|byte| !byte.is_ascii_whitespace()).unwrap_or(value.len());
  let end = value
    .iter()
    .rposition(|byte| !byte.is_ascii_whitespace())
    .map_or(start, |index| index + 1);
  &value[start..end] == b"tsc"
}

#[cfg(feature = "bench-internal")]
fn instant_provider_name(provider: u8) -> &'static str {
  match provider {
    PROVIDER_TSC => "linux_kernel_eligible_tsc",
    PROVIDER_LIBC_MONOTONIC => "linux_clock_monotonic_libc",
    PROVIDER_LIBC_MONOTONIC_RAW => "linux_clock_monotonic_raw_libc",
    PROVIDER_LIBC_BOOTTIME => "linux_clock_boottime_libc",
    PROVIDER_SYSCALL64_MONOTONIC => "linux_clock_monotonic_syscall_x86_64",
    PROVIDER_SYSCALL64_MONOTONIC_RAW => "linux_clock_monotonic_raw_syscall_x86_64",
    PROVIDER_SYSCALL64_BOOTTIME => "linux_clock_boottime_syscall_x86_64",
    PROVIDER_TIME32_MONOTONIC => "linux_clock_monotonic_syscall_i686_time32",
    PROVIDER_TIME32_MONOTONIC_RAW => "linux_clock_monotonic_raw_syscall_i686_time32",
    PROVIDER_TIME32_BOOTTIME => "linux_clock_boottime_syscall_i686_time32",
    PROVIDER_TIME64_MONOTONIC => "linux_clock_monotonic_syscall_i686_time64",
    PROVIDER_TIME64_MONOTONIC_RAW => "linux_clock_monotonic_raw_syscall_i686_time64",
    PROVIDER_TIME64_BOOTTIME => "linux_clock_boottime_syscall_i686_time64",
    PROVIDER_VDSO_MONOTONIC => "linux_clock_monotonic_vdso_direct",
    PROVIDER_VDSO_MONOTONIC_RAW => "linux_clock_monotonic_raw_vdso_direct",
    PROVIDER_VDSO_BOOTTIME => "linux_clock_boottime_vdso_direct",
    #[cfg(target_pointer_width = "32")]
    PROVIDER_VDSO_TIME64_MONOTONIC => "linux_clock_monotonic_vdso_time64_direct",
    #[cfg(target_pointer_width = "32")]
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW => "linux_clock_monotonic_raw_vdso_time64_direct",
    #[cfg(target_pointer_width = "32")]
    PROVIDER_VDSO_TIME64_BOOTTIME => "linux_clock_boottime_vdso_time64_direct",
    _ => "unavailable",
  }
}

#[cfg(feature = "bench-internal")]
fn ordered_provider_name(provider: u8) -> &'static str {
  match provider {
    PROVIDER_TSC_LFENCE_RDTSC => "linux_kernel_eligible_tsc_x86_lfence_rdtsc",
    PROVIDER_TSC_MFENCE_RDTSC => "linux_kernel_eligible_tsc_x86_mfence_rdtsc",
    PROVIDER_TSC_RDTSCP => "linux_kernel_eligible_tsc_x86_rdtscp",
    PROVIDER_TSC_CPUID_RDTSC => "linux_kernel_eligible_tsc_x86_cpuid_rdtsc",
    PROVIDER_TSC_SERIALIZE_RDTSC => "linux_kernel_eligible_tsc_x86_serialize_rdtsc",
    provider if is_ordered_os_provider(provider) => {
      ordered_source_barrier_name(ordered_source(provider), ordered_barrier(provider))
    }
    _ => "unavailable",
  }
}

#[cfg(feature = "bench-internal")]
fn ordered_source_barrier_name(source: u8, barrier: u8) -> &'static str {
  match (source, barrier) {
    (SOURCE_LIBC_MONOTONIC, ORDERED_BARRIER_OS_OWNED) => "linux_clock_monotonic_libc_os_owned",
    (SOURCE_LIBC_MONOTONIC, ORDERED_BARRIER_LFENCE) => "linux_clock_monotonic_libc_x86_lfence",
    (SOURCE_LIBC_MONOTONIC, ORDERED_BARRIER_RDTSCP) => {
      "linux_clock_monotonic_libc_x86_rdtscp_lfence"
    }
    (SOURCE_LIBC_MONOTONIC, ORDERED_BARRIER_MFENCE) => "linux_clock_monotonic_libc_x86_mfence",
    (SOURCE_LIBC_MONOTONIC, ORDERED_BARRIER_CPUID) => "linux_clock_monotonic_libc_x86_cpuid",
    (SOURCE_LIBC_MONOTONIC, ORDERED_BARRIER_SERIALIZE) => {
      "linux_clock_monotonic_libc_x86_serialize"
    }
    (SOURCE_LIBC_MONOTONIC_RAW, ORDERED_BARRIER_LFENCE) => {
      "linux_clock_monotonic_raw_libc_x86_lfence"
    }
    (SOURCE_LIBC_MONOTONIC_RAW, ORDERED_BARRIER_OS_OWNED) => {
      "linux_clock_monotonic_raw_libc_os_owned"
    }
    (SOURCE_LIBC_MONOTONIC_RAW, ORDERED_BARRIER_RDTSCP) => {
      "linux_clock_monotonic_raw_libc_x86_rdtscp_lfence"
    }
    (SOURCE_LIBC_MONOTONIC_RAW, ORDERED_BARRIER_MFENCE) => {
      "linux_clock_monotonic_raw_libc_x86_mfence"
    }
    (SOURCE_LIBC_MONOTONIC_RAW, ORDERED_BARRIER_CPUID) => {
      "linux_clock_monotonic_raw_libc_x86_cpuid"
    }
    (SOURCE_LIBC_MONOTONIC_RAW, ORDERED_BARRIER_SERIALIZE) => {
      "linux_clock_monotonic_raw_libc_x86_serialize"
    }
    (SOURCE_SYSCALL64_MONOTONIC, ORDERED_BARRIER_LFENCE) => {
      "linux_clock_monotonic_syscall_x86_64_x86_lfence"
    }
    (SOURCE_SYSCALL64_MONOTONIC, ORDERED_BARRIER_OS_OWNED) => {
      "linux_clock_monotonic_syscall_x86_64_os_owned"
    }
    (SOURCE_SYSCALL64_MONOTONIC, ORDERED_BARRIER_RDTSCP) => {
      "linux_clock_monotonic_syscall_x86_64_x86_rdtscp_lfence"
    }
    (SOURCE_SYSCALL64_MONOTONIC, ORDERED_BARRIER_MFENCE) => {
      "linux_clock_monotonic_syscall_x86_64_x86_mfence"
    }
    (SOURCE_SYSCALL64_MONOTONIC, ORDERED_BARRIER_CPUID) => {
      "linux_clock_monotonic_syscall_x86_64_x86_cpuid"
    }
    (SOURCE_SYSCALL64_MONOTONIC, ORDERED_BARRIER_SERIALIZE) => {
      "linux_clock_monotonic_syscall_x86_64_x86_serialize"
    }
    (SOURCE_SYSCALL64_MONOTONIC_RAW, ORDERED_BARRIER_LFENCE) => {
      "linux_clock_monotonic_raw_syscall_x86_64_x86_lfence"
    }
    (SOURCE_SYSCALL64_MONOTONIC_RAW, ORDERED_BARRIER_OS_OWNED) => {
      "linux_clock_monotonic_raw_syscall_x86_64_os_owned"
    }
    (SOURCE_SYSCALL64_MONOTONIC_RAW, ORDERED_BARRIER_RDTSCP) => {
      "linux_clock_monotonic_raw_syscall_x86_64_x86_rdtscp_lfence"
    }
    (SOURCE_SYSCALL64_MONOTONIC_RAW, ORDERED_BARRIER_MFENCE) => {
      "linux_clock_monotonic_raw_syscall_x86_64_x86_mfence"
    }
    (SOURCE_SYSCALL64_MONOTONIC_RAW, ORDERED_BARRIER_CPUID) => {
      "linux_clock_monotonic_raw_syscall_x86_64_x86_cpuid"
    }
    (SOURCE_SYSCALL64_MONOTONIC_RAW, ORDERED_BARRIER_SERIALIZE) => {
      "linux_clock_monotonic_raw_syscall_x86_64_x86_serialize"
    }
    (SOURCE_TIME32_MONOTONIC, ORDERED_BARRIER_LFENCE) => {
      "linux_clock_monotonic_syscall_i686_time32_x86_lfence"
    }
    (SOURCE_TIME32_MONOTONIC, ORDERED_BARRIER_OS_OWNED) => {
      "linux_clock_monotonic_syscall_i686_time32_os_owned"
    }
    (SOURCE_TIME32_MONOTONIC, ORDERED_BARRIER_RDTSCP) => {
      "linux_clock_monotonic_syscall_i686_time32_x86_rdtscp_lfence"
    }
    (SOURCE_TIME32_MONOTONIC, ORDERED_BARRIER_MFENCE) => {
      "linux_clock_monotonic_syscall_i686_time32_x86_mfence"
    }
    (SOURCE_TIME32_MONOTONIC, ORDERED_BARRIER_CPUID) => {
      "linux_clock_monotonic_syscall_i686_time32_x86_cpuid"
    }
    (SOURCE_TIME32_MONOTONIC, ORDERED_BARRIER_SERIALIZE) => {
      "linux_clock_monotonic_syscall_i686_time32_x86_serialize"
    }
    (SOURCE_TIME32_MONOTONIC_RAW, ORDERED_BARRIER_LFENCE) => {
      "linux_clock_monotonic_raw_syscall_i686_time32_x86_lfence"
    }
    (SOURCE_TIME32_MONOTONIC_RAW, ORDERED_BARRIER_OS_OWNED) => {
      "linux_clock_monotonic_raw_syscall_i686_time32_os_owned"
    }
    (SOURCE_TIME32_MONOTONIC_RAW, ORDERED_BARRIER_RDTSCP) => {
      "linux_clock_monotonic_raw_syscall_i686_time32_x86_rdtscp_lfence"
    }
    (SOURCE_TIME32_MONOTONIC_RAW, ORDERED_BARRIER_MFENCE) => {
      "linux_clock_monotonic_raw_syscall_i686_time32_x86_mfence"
    }
    (SOURCE_TIME32_MONOTONIC_RAW, ORDERED_BARRIER_CPUID) => {
      "linux_clock_monotonic_raw_syscall_i686_time32_x86_cpuid"
    }
    (SOURCE_TIME32_MONOTONIC_RAW, ORDERED_BARRIER_SERIALIZE) => {
      "linux_clock_monotonic_raw_syscall_i686_time32_x86_serialize"
    }
    (SOURCE_TIME64_MONOTONIC, ORDERED_BARRIER_LFENCE) => {
      "linux_clock_monotonic_syscall_i686_time64_x86_lfence"
    }
    (SOURCE_TIME64_MONOTONIC, ORDERED_BARRIER_OS_OWNED) => {
      "linux_clock_monotonic_syscall_i686_time64_os_owned"
    }
    (SOURCE_TIME64_MONOTONIC, ORDERED_BARRIER_RDTSCP) => {
      "linux_clock_monotonic_syscall_i686_time64_x86_rdtscp_lfence"
    }
    (SOURCE_TIME64_MONOTONIC, ORDERED_BARRIER_MFENCE) => {
      "linux_clock_monotonic_syscall_i686_time64_x86_mfence"
    }
    (SOURCE_TIME64_MONOTONIC, ORDERED_BARRIER_CPUID) => {
      "linux_clock_monotonic_syscall_i686_time64_x86_cpuid"
    }
    (SOURCE_TIME64_MONOTONIC, ORDERED_BARRIER_SERIALIZE) => {
      "linux_clock_monotonic_syscall_i686_time64_x86_serialize"
    }
    (SOURCE_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_LFENCE) => {
      "linux_clock_monotonic_raw_syscall_i686_time64_x86_lfence"
    }
    (SOURCE_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_OS_OWNED) => {
      "linux_clock_monotonic_raw_syscall_i686_time64_os_owned"
    }
    (SOURCE_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_RDTSCP) => {
      "linux_clock_monotonic_raw_syscall_i686_time64_x86_rdtscp_lfence"
    }
    (SOURCE_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_MFENCE) => {
      "linux_clock_monotonic_raw_syscall_i686_time64_x86_mfence"
    }
    (SOURCE_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_CPUID) => {
      "linux_clock_monotonic_raw_syscall_i686_time64_x86_cpuid"
    }
    (SOURCE_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_SERIALIZE) => {
      "linux_clock_monotonic_raw_syscall_i686_time64_x86_serialize"
    }
    (SOURCE_VDSO_MONOTONIC, ORDERED_BARRIER_OS_OWNED) => {
      "linux_clock_monotonic_vdso_direct_os_owned"
    }
    (SOURCE_VDSO_MONOTONIC, ORDERED_BARRIER_LFENCE) => {
      "linux_clock_monotonic_vdso_direct_x86_lfence"
    }
    (SOURCE_VDSO_MONOTONIC, ORDERED_BARRIER_RDTSCP) => {
      "linux_clock_monotonic_vdso_direct_x86_rdtscp_lfence"
    }
    (SOURCE_VDSO_MONOTONIC, ORDERED_BARRIER_MFENCE) => {
      "linux_clock_monotonic_vdso_direct_x86_mfence"
    }
    (SOURCE_VDSO_MONOTONIC, ORDERED_BARRIER_CPUID) => "linux_clock_monotonic_vdso_direct_x86_cpuid",
    (SOURCE_VDSO_MONOTONIC, ORDERED_BARRIER_SERIALIZE) => {
      "linux_clock_monotonic_vdso_direct_x86_serialize"
    }
    (SOURCE_VDSO_MONOTONIC_RAW, ORDERED_BARRIER_OS_OWNED) => {
      "linux_clock_monotonic_raw_vdso_direct_os_owned"
    }
    (SOURCE_VDSO_MONOTONIC_RAW, ORDERED_BARRIER_LFENCE) => {
      "linux_clock_monotonic_raw_vdso_direct_x86_lfence"
    }
    (SOURCE_VDSO_MONOTONIC_RAW, ORDERED_BARRIER_RDTSCP) => {
      "linux_clock_monotonic_raw_vdso_direct_x86_rdtscp_lfence"
    }
    (SOURCE_VDSO_MONOTONIC_RAW, ORDERED_BARRIER_MFENCE) => {
      "linux_clock_monotonic_raw_vdso_direct_x86_mfence"
    }
    (SOURCE_VDSO_MONOTONIC_RAW, ORDERED_BARRIER_CPUID) => {
      "linux_clock_monotonic_raw_vdso_direct_x86_cpuid"
    }
    (SOURCE_VDSO_MONOTONIC_RAW, ORDERED_BARRIER_SERIALIZE) => {
      "linux_clock_monotonic_raw_vdso_direct_x86_serialize"
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC, ORDERED_BARRIER_OS_OWNED) => {
      "linux_clock_monotonic_vdso_time64_direct_os_owned"
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC, ORDERED_BARRIER_LFENCE) => {
      "linux_clock_monotonic_vdso_time64_direct_x86_lfence"
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC, ORDERED_BARRIER_RDTSCP) => {
      "linux_clock_monotonic_vdso_time64_direct_x86_rdtscp_lfence"
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC, ORDERED_BARRIER_MFENCE) => {
      "linux_clock_monotonic_vdso_time64_direct_x86_mfence"
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC, ORDERED_BARRIER_CPUID) => {
      "linux_clock_monotonic_vdso_time64_direct_x86_cpuid"
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC, ORDERED_BARRIER_SERIALIZE) => {
      "linux_clock_monotonic_vdso_time64_direct_x86_serialize"
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_OS_OWNED) => {
      "linux_clock_monotonic_raw_vdso_time64_direct_os_owned"
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_LFENCE) => {
      "linux_clock_monotonic_raw_vdso_time64_direct_x86_lfence"
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_RDTSCP) => {
      "linux_clock_monotonic_raw_vdso_time64_direct_x86_rdtscp_lfence"
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_MFENCE) => {
      "linux_clock_monotonic_raw_vdso_time64_direct_x86_mfence"
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_CPUID) => {
      "linux_clock_monotonic_raw_vdso_time64_direct_x86_cpuid"
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_SERIALIZE) => {
      "linux_clock_monotonic_raw_vdso_time64_direct_x86_serialize"
    }
    (SOURCE_LIBC_BOOTTIME, barrier) => match barrier {
      ORDERED_BARRIER_OS_OWNED => "linux_clock_boottime_libc_os_owned",
      ORDERED_BARRIER_LFENCE => "linux_clock_boottime_libc_x86_lfence",
      ORDERED_BARRIER_RDTSCP => "linux_clock_boottime_libc_x86_rdtscp_lfence",
      ORDERED_BARRIER_MFENCE => "linux_clock_boottime_libc_x86_mfence",
      ORDERED_BARRIER_SERIALIZE => "linux_clock_boottime_libc_x86_serialize",
      _ => "linux_clock_boottime_libc_x86_cpuid",
    },
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_BOOTTIME, barrier) => match barrier {
      ORDERED_BARRIER_OS_OWNED => "linux_clock_boottime_syscall_x86_64_os_owned",
      ORDERED_BARRIER_LFENCE => "linux_clock_boottime_syscall_x86_64_x86_lfence",
      ORDERED_BARRIER_RDTSCP => "linux_clock_boottime_syscall_x86_64_x86_rdtscp_lfence",
      ORDERED_BARRIER_MFENCE => "linux_clock_boottime_syscall_x86_64_x86_mfence",
      ORDERED_BARRIER_SERIALIZE => "linux_clock_boottime_syscall_x86_64_x86_serialize",
      _ => "linux_clock_boottime_syscall_x86_64_x86_cpuid",
    },
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_BOOTTIME, barrier) => match barrier {
      ORDERED_BARRIER_OS_OWNED => "linux_clock_boottime_syscall_i686_time32_os_owned",
      ORDERED_BARRIER_LFENCE => "linux_clock_boottime_syscall_i686_time32_x86_lfence",
      ORDERED_BARRIER_RDTSCP => "linux_clock_boottime_syscall_i686_time32_x86_rdtscp_lfence",
      ORDERED_BARRIER_MFENCE => "linux_clock_boottime_syscall_i686_time32_x86_mfence",
      ORDERED_BARRIER_SERIALIZE => "linux_clock_boottime_syscall_i686_time32_x86_serialize",
      _ => "linux_clock_boottime_syscall_i686_time32_x86_cpuid",
    },
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_BOOTTIME, barrier) => match barrier {
      ORDERED_BARRIER_OS_OWNED => "linux_clock_boottime_syscall_i686_time64_os_owned",
      ORDERED_BARRIER_LFENCE => "linux_clock_boottime_syscall_i686_time64_x86_lfence",
      ORDERED_BARRIER_RDTSCP => "linux_clock_boottime_syscall_i686_time64_x86_rdtscp_lfence",
      ORDERED_BARRIER_MFENCE => "linux_clock_boottime_syscall_i686_time64_x86_mfence",
      ORDERED_BARRIER_SERIALIZE => "linux_clock_boottime_syscall_i686_time64_x86_serialize",
      _ => "linux_clock_boottime_syscall_i686_time64_x86_cpuid",
    },
    (SOURCE_VDSO_BOOTTIME, barrier) => match barrier {
      ORDERED_BARRIER_OS_OWNED => "linux_clock_boottime_vdso_direct_os_owned",
      ORDERED_BARRIER_LFENCE => "linux_clock_boottime_vdso_direct_x86_lfence",
      ORDERED_BARRIER_RDTSCP => "linux_clock_boottime_vdso_direct_x86_rdtscp_lfence",
      ORDERED_BARRIER_MFENCE => "linux_clock_boottime_vdso_direct_x86_mfence",
      ORDERED_BARRIER_SERIALIZE => "linux_clock_boottime_vdso_direct_x86_serialize",
      _ => "linux_clock_boottime_vdso_direct_x86_cpuid",
    },
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_BOOTTIME, barrier) => match barrier {
      ORDERED_BARRIER_OS_OWNED => "linux_clock_boottime_vdso_time64_direct_os_owned",
      ORDERED_BARRIER_LFENCE => "linux_clock_boottime_vdso_time64_direct_x86_lfence",
      ORDERED_BARRIER_RDTSCP => "linux_clock_boottime_vdso_time64_direct_x86_rdtscp_lfence",
      ORDERED_BARRIER_MFENCE => "linux_clock_boottime_vdso_time64_direct_x86_mfence",
      ORDERED_BARRIER_SERIALIZE => "linux_clock_boottime_vdso_time64_direct_x86_serialize",
      _ => "linux_clock_boottime_vdso_time64_direct_x86_cpuid",
    },
    _ => "unavailable",
  }
}

#[cfg(feature = "bench-internal")]
const fn barrier_name(barrier: u8) -> &'static str {
  match barrier {
    ORDERED_BARRIER_OS_OWNED => "os_owned",
    ORDERED_BARRIER_LFENCE => "x86_lfence",
    ORDERED_BARRIER_RDTSCP => "x86_rdtscp_lfence",
    ORDERED_BARRIER_MFENCE => "x86_mfence",
    ORDERED_BARRIER_SERIALIZE => "x86_serialize",
    _ => "x86_cpuid",
  }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_provider() -> &'static str {
  instant_provider_name(selected_instant_provider())
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_ordered_provider() -> &'static str {
  ordered_provider_name(selected_ordered_provider())
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_tsc_eligible() -> bool {
  detect_tsc_eligibility() == TscEligibility::Eligible
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_tsc() -> u64 {
  read_tsc()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_tsc_ordered() -> u64 {
  match selected_direct_tsc_provider() {
    PROVIDER_TSC_LFENCE_RDTSC => read_tsc_lfence_ordered(),
    PROVIDER_TSC_MFENCE_RDTSC => read_tsc_mfence_ordered(),
    PROVIDER_TSC_RDTSCP => read_tsc_rdtscp_ordered(),
    PROVIDER_TSC_SERIALIZE_RDTSC => read_tsc_serialize_ordered(),
    _ => read_tsc_cpuid_ordered(),
  }
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(dead_code)] // Criterion calls the eligible named primitive directly.
pub(crate) fn bench_direct_tsc_lfence_ordered() -> u64 {
  read_tsc_lfence_ordered()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(dead_code)] // Criterion calls the eligible named primitive directly.
pub(crate) fn bench_direct_tsc_mfence_ordered() -> u64 {
  read_tsc_mfence_ordered()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(dead_code)] // Criterion calls the eligible named primitive directly.
pub(crate) fn bench_direct_tsc_rdtscp_ordered() -> u64 {
  read_tsc_rdtscp_ordered()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(dead_code)] // Criterion calls the eligible named primitive directly.
pub(crate) fn bench_direct_tsc_cpuid_ordered() -> u64 {
  read_tsc_cpuid_ordered()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(dead_code)] // Criterion calls the eligible named primitive directly.
pub(crate) fn bench_direct_tsc_serialize_ordered() -> u64 {
  read_tsc_serialize_ordered()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic() -> u64 {
  libc_clock(PROVIDER_LIBC_MONOTONIC)
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_raw() -> u64 {
  libc_clock(PROVIDER_LIBC_MONOTONIC_RAW)
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_boottime() -> u64 {
  libc_clock(PROVIDER_LIBC_BOOTTIME)
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_vdso_monotonic() -> u64 {
  vdso_clock(SOURCE_VDSO_MONOTONIC)
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_vdso_monotonic_raw() -> u64 {
  vdso_clock(SOURCE_VDSO_MONOTONIC_RAW)
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_vdso_boottime() -> u64 {
  vdso_clock(SOURCE_VDSO_BOOTTIME)
}

#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
#[inline(always)]
pub(crate) fn bench_direct_vdso_time64_monotonic() -> u64 {
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC)
}

#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
#[inline(always)]
pub(crate) fn bench_direct_vdso_time64_monotonic_raw() -> u64 {
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC_RAW)
}

#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
#[inline(always)]
pub(crate) fn bench_direct_vdso_time64_boottime() -> u64 {
  vdso_clock(SOURCE_VDSO_TIME64_BOOTTIME)
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_syscall() -> u64 {
  #[cfg(target_pointer_width = "64")]
  {
    raw_clock(PROVIDER_SYSCALL64_MONOTONIC)
  }
  #[cfg(target_pointer_width = "32")]
  {
    raw_clock(PROVIDER_TIME32_MONOTONIC)
  }
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_raw_syscall() -> u64 {
  #[cfg(target_pointer_width = "64")]
  {
    raw_clock(PROVIDER_SYSCALL64_MONOTONIC_RAW)
  }
  #[cfg(target_pointer_width = "32")]
  {
    raw_clock(PROVIDER_TIME32_MONOTONIC_RAW)
  }
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(dead_code)] // The i686 serializer uses its exact time32-named wrapper.
pub(crate) fn bench_direct_clock_boottime_syscall() -> u64 {
  #[cfg(target_pointer_width = "64")]
  {
    raw_clock(PROVIDER_SYSCALL64_BOOTTIME)
  }
  #[cfg(target_pointer_width = "32")]
  {
    raw_clock(PROVIDER_TIME32_BOOTTIME)
  }
}

#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
#[inline(always)]
#[allow(dead_code)] // Exact time32 identity for the candidate serializer.
pub(crate) fn bench_direct_clock_monotonic_time32_syscall() -> u64 {
  raw_clock(PROVIDER_TIME32_MONOTONIC)
}

#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
#[inline(always)]
#[allow(dead_code)] // Exact time32 identity for the candidate serializer.
pub(crate) fn bench_direct_clock_monotonic_raw_time32_syscall() -> u64 {
  raw_clock(PROVIDER_TIME32_MONOTONIC_RAW)
}

#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
#[inline(always)]
#[allow(dead_code)] // Exact time32 identity for the candidate serializer.
pub(crate) fn bench_direct_clock_boottime_time32_syscall() -> u64 {
  raw_clock(PROVIDER_TIME32_BOOTTIME)
}

#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_time64_syscall() -> u64 {
  raw_clock(PROVIDER_TIME64_MONOTONIC)
}

#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_raw_time64_syscall() -> u64 {
  raw_clock(PROVIDER_TIME64_MONOTONIC_RAW)
}

#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
#[inline(always)]
pub(crate) fn bench_direct_clock_boottime_time64_syscall() -> u64 {
  raw_clock(PROVIDER_TIME64_BOOTTIME)
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_ordered() -> u64 {
  let provider = best_ordered_provider_for_source(SOURCE_LIBC_MONOTONIC);
  read_ordered_provider(provider)
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_syscall_ordered() -> u64 {
  #[cfg(target_pointer_width = "64")]
  let source = SOURCE_SYSCALL64_MONOTONIC;
  #[cfg(target_pointer_width = "32")]
  let source = SOURCE_TIME32_MONOTONIC;
  let provider = best_ordered_provider_for_source(source);
  read_ordered_provider(provider)
}

#[cfg(feature = "bench-internal")]
fn best_ordered_provider_for_source(source: u8) -> u8 {
  let _ = selected_ordered_provider();
  // SAFETY: the provider was acquired after the selection owner initialized
  // this Copy evidence value.
  let evidence = unsafe { (*ORDERED_EVIDENCE.0.get()).assume_init_read() };
  let mut selected = None;
  let mut selected_samples = [u64::MAX; PROBE_BATCHES];
  for index in 0..evidence.tournament.candidates.count {
    let provider = evidence.tournament.candidates.providers[index];
    if !is_ordered_os_provider(provider) || ordered_source(provider) != source {
      continue;
    }
    if selected.is_none()
      || evaluate_challenger(evidence.tournament.samples.batches[index], selected_samples)
        .challenger_selected
    {
      selected = Some(provider);
      selected_samples = evidence.tournament.samples.batches[index];
    }
  }
  selected.unwrap_or(ordered_provider(source, evidence.baseline_barrier))
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
  crate::arch::scale_from_ratio(1_000_000_000, frequency_for(provider))
}

#[cfg(feature = "bench-internal")]
macro_rules! ordered_bench_reader {
  ($name:ident, $barrier:expr, $body:expr) => {
    #[inline(always)]
    #[allow(dead_code)] // Criterion calls the eligible named primitive directly.
    pub(crate) fn $name() -> u64 {
      execute_ordered_os_barrier($barrier);
      $body
    }
  };
}

#[cfg(feature = "bench-internal")]
macro_rules! ordered_bench_source {
  ($os:ident, $lfence:ident, $rdtscp:ident, $mfence:ident, $cpuid:ident, $serialize:ident, $body:expr) => {
    ordered_bench_reader!($os, ORDERED_BARRIER_OS_OWNED, $body);
    ordered_bench_reader!($lfence, ORDERED_BARRIER_LFENCE, $body);
    ordered_bench_reader!($rdtscp, ORDERED_BARRIER_RDTSCP, $body);
    ordered_bench_reader!($mfence, ORDERED_BARRIER_MFENCE, $body);
    ordered_bench_reader!($cpuid, ORDERED_BARRIER_CPUID, $body);
    ordered_bench_reader!($serialize, ORDERED_BARRIER_SERIALIZE, $body);
  };
}

#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_libc_monotonic_os_owned,
  ORDERED_BARRIER_OS_OWNED,
  libc_clock(PROVIDER_LIBC_MONOTONIC)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_libc_monotonic_lfence,
  ORDERED_BARRIER_LFENCE,
  libc_clock(PROVIDER_LIBC_MONOTONIC)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_libc_monotonic_rdtscp,
  ORDERED_BARRIER_RDTSCP,
  libc_clock(PROVIDER_LIBC_MONOTONIC)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_libc_monotonic_mfence,
  ORDERED_BARRIER_MFENCE,
  libc_clock(PROVIDER_LIBC_MONOTONIC)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_libc_monotonic_cpuid,
  ORDERED_BARRIER_CPUID,
  libc_clock(PROVIDER_LIBC_MONOTONIC)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_libc_monotonic_serialize,
  ORDERED_BARRIER_SERIALIZE,
  libc_clock(PROVIDER_LIBC_MONOTONIC)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_libc_raw_os_owned,
  ORDERED_BARRIER_OS_OWNED,
  libc_clock(PROVIDER_LIBC_MONOTONIC_RAW)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_libc_raw_lfence,
  ORDERED_BARRIER_LFENCE,
  libc_clock(PROVIDER_LIBC_MONOTONIC_RAW)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_libc_raw_rdtscp,
  ORDERED_BARRIER_RDTSCP,
  libc_clock(PROVIDER_LIBC_MONOTONIC_RAW)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_libc_raw_mfence,
  ORDERED_BARRIER_MFENCE,
  libc_clock(PROVIDER_LIBC_MONOTONIC_RAW)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_libc_raw_cpuid,
  ORDERED_BARRIER_CPUID,
  libc_clock(PROVIDER_LIBC_MONOTONIC_RAW)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_libc_raw_serialize,
  ORDERED_BARRIER_SERIALIZE,
  libc_clock(PROVIDER_LIBC_MONOTONIC_RAW)
);

#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_reader!(
  bench_syscall64_monotonic_os_owned,
  ORDERED_BARRIER_OS_OWNED,
  raw_clock(PROVIDER_SYSCALL64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_reader!(
  bench_syscall64_monotonic_lfence,
  ORDERED_BARRIER_LFENCE,
  raw_clock(PROVIDER_SYSCALL64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_reader!(
  bench_syscall64_monotonic_rdtscp,
  ORDERED_BARRIER_RDTSCP,
  raw_clock(PROVIDER_SYSCALL64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_reader!(
  bench_syscall64_monotonic_mfence,
  ORDERED_BARRIER_MFENCE,
  raw_clock(PROVIDER_SYSCALL64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_reader!(
  bench_syscall64_monotonic_cpuid,
  ORDERED_BARRIER_CPUID,
  raw_clock(PROVIDER_SYSCALL64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_reader!(
  bench_syscall64_monotonic_serialize,
  ORDERED_BARRIER_SERIALIZE,
  raw_clock(PROVIDER_SYSCALL64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_reader!(
  bench_syscall64_raw_os_owned,
  ORDERED_BARRIER_OS_OWNED,
  raw_clock(PROVIDER_SYSCALL64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_reader!(
  bench_syscall64_raw_lfence,
  ORDERED_BARRIER_LFENCE,
  raw_clock(PROVIDER_SYSCALL64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_reader!(
  bench_syscall64_raw_rdtscp,
  ORDERED_BARRIER_RDTSCP,
  raw_clock(PROVIDER_SYSCALL64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_reader!(
  bench_syscall64_raw_mfence,
  ORDERED_BARRIER_MFENCE,
  raw_clock(PROVIDER_SYSCALL64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_reader!(
  bench_syscall64_raw_cpuid,
  ORDERED_BARRIER_CPUID,
  raw_clock(PROVIDER_SYSCALL64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_reader!(
  bench_syscall64_raw_serialize,
  ORDERED_BARRIER_SERIALIZE,
  raw_clock(PROVIDER_SYSCALL64_MONOTONIC_RAW)
);

#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time32_monotonic_os_owned,
  ORDERED_BARRIER_OS_OWNED,
  raw_clock(PROVIDER_TIME32_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time32_monotonic_lfence,
  ORDERED_BARRIER_LFENCE,
  raw_clock(PROVIDER_TIME32_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time32_monotonic_rdtscp,
  ORDERED_BARRIER_RDTSCP,
  raw_clock(PROVIDER_TIME32_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time32_monotonic_mfence,
  ORDERED_BARRIER_MFENCE,
  raw_clock(PROVIDER_TIME32_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time32_monotonic_cpuid,
  ORDERED_BARRIER_CPUID,
  raw_clock(PROVIDER_TIME32_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time32_monotonic_serialize,
  ORDERED_BARRIER_SERIALIZE,
  raw_clock(PROVIDER_TIME32_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time32_raw_os_owned,
  ORDERED_BARRIER_OS_OWNED,
  raw_clock(PROVIDER_TIME32_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time32_raw_lfence,
  ORDERED_BARRIER_LFENCE,
  raw_clock(PROVIDER_TIME32_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time32_raw_rdtscp,
  ORDERED_BARRIER_RDTSCP,
  raw_clock(PROVIDER_TIME32_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time32_raw_mfence,
  ORDERED_BARRIER_MFENCE,
  raw_clock(PROVIDER_TIME32_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time32_raw_cpuid,
  ORDERED_BARRIER_CPUID,
  raw_clock(PROVIDER_TIME32_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time32_raw_serialize,
  ORDERED_BARRIER_SERIALIZE,
  raw_clock(PROVIDER_TIME32_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time64_monotonic_os_owned,
  ORDERED_BARRIER_OS_OWNED,
  raw_clock(PROVIDER_TIME64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time64_monotonic_lfence,
  ORDERED_BARRIER_LFENCE,
  raw_clock(PROVIDER_TIME64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time64_monotonic_rdtscp,
  ORDERED_BARRIER_RDTSCP,
  raw_clock(PROVIDER_TIME64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time64_monotonic_mfence,
  ORDERED_BARRIER_MFENCE,
  raw_clock(PROVIDER_TIME64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time64_monotonic_cpuid,
  ORDERED_BARRIER_CPUID,
  raw_clock(PROVIDER_TIME64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time64_monotonic_serialize,
  ORDERED_BARRIER_SERIALIZE,
  raw_clock(PROVIDER_TIME64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time64_raw_os_owned,
  ORDERED_BARRIER_OS_OWNED,
  raw_clock(PROVIDER_TIME64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time64_raw_lfence,
  ORDERED_BARRIER_LFENCE,
  raw_clock(PROVIDER_TIME64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time64_raw_rdtscp,
  ORDERED_BARRIER_RDTSCP,
  raw_clock(PROVIDER_TIME64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time64_raw_mfence,
  ORDERED_BARRIER_MFENCE,
  raw_clock(PROVIDER_TIME64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time64_raw_cpuid,
  ORDERED_BARRIER_CPUID,
  raw_clock(PROVIDER_TIME64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_time64_raw_serialize,
  ORDERED_BARRIER_SERIALIZE,
  raw_clock(PROVIDER_TIME64_MONOTONIC_RAW)
);

#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_vdso_monotonic_os_owned,
  ORDERED_BARRIER_OS_OWNED,
  vdso_clock(SOURCE_VDSO_MONOTONIC)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_vdso_monotonic_lfence,
  ORDERED_BARRIER_LFENCE,
  vdso_clock(SOURCE_VDSO_MONOTONIC)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_vdso_monotonic_rdtscp,
  ORDERED_BARRIER_RDTSCP,
  vdso_clock(SOURCE_VDSO_MONOTONIC)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_vdso_monotonic_mfence,
  ORDERED_BARRIER_MFENCE,
  vdso_clock(SOURCE_VDSO_MONOTONIC)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_vdso_monotonic_cpuid,
  ORDERED_BARRIER_CPUID,
  vdso_clock(SOURCE_VDSO_MONOTONIC)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_vdso_monotonic_serialize,
  ORDERED_BARRIER_SERIALIZE,
  vdso_clock(SOURCE_VDSO_MONOTONIC)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_vdso_raw_os_owned,
  ORDERED_BARRIER_OS_OWNED,
  vdso_clock(SOURCE_VDSO_MONOTONIC_RAW)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_vdso_raw_lfence,
  ORDERED_BARRIER_LFENCE,
  vdso_clock(SOURCE_VDSO_MONOTONIC_RAW)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_vdso_raw_rdtscp,
  ORDERED_BARRIER_RDTSCP,
  vdso_clock(SOURCE_VDSO_MONOTONIC_RAW)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_vdso_raw_mfence,
  ORDERED_BARRIER_MFENCE,
  vdso_clock(SOURCE_VDSO_MONOTONIC_RAW)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_vdso_raw_cpuid,
  ORDERED_BARRIER_CPUID,
  vdso_clock(SOURCE_VDSO_MONOTONIC_RAW)
);
#[cfg(feature = "bench-internal")]
ordered_bench_reader!(
  bench_vdso_raw_serialize,
  ORDERED_BARRIER_SERIALIZE,
  vdso_clock(SOURCE_VDSO_MONOTONIC_RAW)
);

#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_vdso_time64_monotonic_os_owned,
  ORDERED_BARRIER_OS_OWNED,
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_vdso_time64_monotonic_lfence,
  ORDERED_BARRIER_LFENCE,
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_vdso_time64_monotonic_rdtscp,
  ORDERED_BARRIER_RDTSCP,
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_vdso_time64_monotonic_mfence,
  ORDERED_BARRIER_MFENCE,
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_vdso_time64_monotonic_cpuid,
  ORDERED_BARRIER_CPUID,
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_vdso_time64_monotonic_serialize,
  ORDERED_BARRIER_SERIALIZE,
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_vdso_time64_raw_os_owned,
  ORDERED_BARRIER_OS_OWNED,
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_vdso_time64_raw_lfence,
  ORDERED_BARRIER_LFENCE,
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_vdso_time64_raw_rdtscp,
  ORDERED_BARRIER_RDTSCP,
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_vdso_time64_raw_mfence,
  ORDERED_BARRIER_MFENCE,
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_vdso_time64_raw_cpuid,
  ORDERED_BARRIER_CPUID,
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC_RAW)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_reader!(
  bench_vdso_time64_raw_serialize,
  ORDERED_BARRIER_SERIALIZE,
  vdso_clock(SOURCE_VDSO_TIME64_MONOTONIC_RAW)
);

#[cfg(feature = "bench-internal")]
ordered_bench_source!(
  bench_libc_boottime_os_owned,
  bench_libc_boottime_lfence,
  bench_libc_boottime_rdtscp,
  bench_libc_boottime_mfence,
  bench_libc_boottime_cpuid,
  bench_libc_boottime_serialize,
  libc_clock(PROVIDER_LIBC_BOOTTIME)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "64"))]
ordered_bench_source!(
  bench_syscall64_boottime_os_owned,
  bench_syscall64_boottime_lfence,
  bench_syscall64_boottime_rdtscp,
  bench_syscall64_boottime_mfence,
  bench_syscall64_boottime_cpuid,
  bench_syscall64_boottime_serialize,
  raw_clock(PROVIDER_SYSCALL64_BOOTTIME)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_source!(
  bench_time32_boottime_os_owned,
  bench_time32_boottime_lfence,
  bench_time32_boottime_rdtscp,
  bench_time32_boottime_mfence,
  bench_time32_boottime_cpuid,
  bench_time32_boottime_serialize,
  raw_clock(PROVIDER_TIME32_BOOTTIME)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_source!(
  bench_time64_boottime_os_owned,
  bench_time64_boottime_lfence,
  bench_time64_boottime_rdtscp,
  bench_time64_boottime_mfence,
  bench_time64_boottime_cpuid,
  bench_time64_boottime_serialize,
  raw_clock(PROVIDER_TIME64_BOOTTIME)
);
#[cfg(feature = "bench-internal")]
ordered_bench_source!(
  bench_vdso_boottime_os_owned,
  bench_vdso_boottime_lfence,
  bench_vdso_boottime_rdtscp,
  bench_vdso_boottime_mfence,
  bench_vdso_boottime_cpuid,
  bench_vdso_boottime_serialize,
  vdso_clock(SOURCE_VDSO_BOOTTIME)
);
#[cfg(all(feature = "bench-internal", target_pointer_width = "32"))]
ordered_bench_source!(
  bench_vdso_time64_boottime_os_owned,
  bench_vdso_time64_boottime_lfence,
  bench_vdso_time64_boottime_rdtscp,
  bench_vdso_time64_boottime_mfence,
  bench_vdso_time64_boottime_cpuid,
  bench_vdso_time64_boottime_serialize,
  vdso_clock(SOURCE_VDSO_TIME64_BOOTTIME)
);

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness selects once before its timing loop.
pub(crate) fn bench_selected_instant_primitive() -> BenchPrimitive {
  let provider = selected_instant_provider();
  instant_bench_primitive(provider)
}

#[cfg(feature = "bench-internal")]
fn instant_bench_primitive(provider: u8) -> BenchPrimitive {
  let read: fn() -> u64 = match provider {
    PROVIDER_TSC => bench_direct_tsc,
    PROVIDER_LIBC_MONOTONIC => bench_direct_clock_monotonic,
    PROVIDER_LIBC_MONOTONIC_RAW => bench_direct_clock_monotonic_raw,
    PROVIDER_LIBC_BOOTTIME => bench_direct_clock_boottime,
    PROVIDER_VDSO_MONOTONIC => bench_direct_vdso_monotonic,
    PROVIDER_VDSO_MONOTONIC_RAW => bench_direct_vdso_monotonic_raw,
    PROVIDER_VDSO_BOOTTIME => bench_direct_vdso_boottime,
    #[cfg(target_pointer_width = "32")]
    PROVIDER_VDSO_TIME64_MONOTONIC => bench_direct_vdso_time64_monotonic,
    #[cfg(target_pointer_width = "32")]
    PROVIDER_VDSO_TIME64_MONOTONIC_RAW => bench_direct_vdso_time64_monotonic_raw,
    #[cfg(target_pointer_width = "32")]
    PROVIDER_VDSO_TIME64_BOOTTIME => bench_direct_vdso_time64_boottime,
    #[cfg(target_pointer_width = "64")]
    PROVIDER_SYSCALL64_MONOTONIC => bench_direct_clock_monotonic_syscall,
    #[cfg(target_pointer_width = "64")]
    PROVIDER_SYSCALL64_MONOTONIC_RAW => bench_direct_clock_monotonic_raw_syscall,
    #[cfg(target_pointer_width = "64")]
    PROVIDER_SYSCALL64_BOOTTIME => bench_direct_clock_boottime_syscall,
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME32_MONOTONIC => bench_direct_clock_monotonic_syscall,
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME32_MONOTONIC_RAW => bench_direct_clock_monotonic_raw_syscall,
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME32_BOOTTIME => bench_direct_clock_boottime_time32_syscall,
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME64_MONOTONIC => bench_direct_clock_monotonic_time64_syscall,
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME64_MONOTONIC_RAW => bench_direct_clock_monotonic_raw_time64_syscall,
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME64_BOOTTIME => bench_direct_clock_boottime_time64_syscall,
    _ => bench_direct_clock_monotonic,
  };
  BenchPrimitive {
    name: instant_provider_name(provider),
    read,
    nanos_per_tick_q32: bench_nanos_per_tick_q32(provider),
  }
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness selects once before its timing loop.
pub(crate) fn bench_selected_ordered_primitive() -> BenchPrimitive {
  let provider = selected_ordered_provider();
  ordered_bench_primitive(provider)
}

#[cfg(feature = "bench-internal")]
fn ordered_bench_primitive(provider: u8) -> BenchPrimitive {
  let read = match provider {
    PROVIDER_TSC_LFENCE_RDTSC => read_tsc_lfence_ordered as fn() -> u64,
    PROVIDER_TSC_MFENCE_RDTSC => read_tsc_mfence_ordered as fn() -> u64,
    PROVIDER_TSC_RDTSCP => read_tsc_rdtscp_ordered as fn() -> u64,
    PROVIDER_TSC_CPUID_RDTSC => read_tsc_cpuid_ordered as fn() -> u64,
    PROVIDER_TSC_SERIALIZE_RDTSC => read_tsc_serialize_ordered as fn() -> u64,
    provider if is_ordered_os_provider(provider) => {
      ordered_bench_read(ordered_source(provider), ordered_barrier(provider))
    }
    _ => bench_libc_monotonic_cpuid as fn() -> u64,
  };
  BenchPrimitive {
    name: ordered_provider_name(provider),
    read,
    nanos_per_tick_q32: bench_nanos_per_tick_q32(provider),
  }
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The Criterion serializer consumes the complete fixed array.
pub(crate) fn bench_instant_candidate_primitives()
-> ([Option<BenchPrimitive>; MAX_INSTANT_CANDIDATES], usize) {
  let _ = selected_instant_provider();
  // SAFETY: the acquired provider follows initialization and Release
  // publication of this Copy evidence.
  let evidence = unsafe { (*INSTANT_EVIDENCE.0.get()).assume_init_read() };
  let mut primitives = [None; MAX_INSTANT_CANDIDATES];
  for (index, slot) in primitives.iter_mut().enumerate().take(evidence.tournament.candidates.count)
  {
    *slot = Some(instant_bench_primitive(evidence.tournament.candidates.providers[index]));
  }
  (primitives, evidence.tournament.candidates.count)
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The Criterion serializer consumes the complete fixed array.
pub(crate) fn bench_ordered_candidate_primitives()
-> ([Option<BenchPrimitive>; MAX_ORDERED_CANDIDATES], usize) {
  let _ = selected_ordered_provider();
  // SAFETY: the acquired provider follows initialization and Release
  // publication of this Copy evidence.
  let evidence = unsafe { (*ORDERED_EVIDENCE.0.get()).assume_init_read() };
  let mut primitives = [None; MAX_ORDERED_CANDIDATES];
  for (index, slot) in primitives.iter_mut().enumerate().take(evidence.tournament.candidates.count)
  {
    *slot = Some(ordered_bench_primitive(evidence.tournament.candidates.providers[index]));
  }
  (primitives, evidence.tournament.candidates.count)
}

#[cfg(feature = "bench-internal")]
fn ordered_bench_read(source: u8, barrier: u8) -> fn() -> u64 {
  match (source, barrier) {
    (SOURCE_LIBC_MONOTONIC, ORDERED_BARRIER_OS_OWNED) => bench_libc_monotonic_os_owned,
    (SOURCE_LIBC_MONOTONIC, ORDERED_BARRIER_LFENCE) => bench_libc_monotonic_lfence,
    (SOURCE_LIBC_MONOTONIC, ORDERED_BARRIER_RDTSCP) => bench_libc_monotonic_rdtscp,
    (SOURCE_LIBC_MONOTONIC, ORDERED_BARRIER_MFENCE) => bench_libc_monotonic_mfence,
    (SOURCE_LIBC_MONOTONIC, ORDERED_BARRIER_CPUID) => bench_libc_monotonic_cpuid,
    (SOURCE_LIBC_MONOTONIC, ORDERED_BARRIER_SERIALIZE) => bench_libc_monotonic_serialize,
    (SOURCE_LIBC_MONOTONIC_RAW, ORDERED_BARRIER_OS_OWNED) => bench_libc_raw_os_owned,
    (SOURCE_LIBC_MONOTONIC_RAW, ORDERED_BARRIER_LFENCE) => bench_libc_raw_lfence,
    (SOURCE_LIBC_MONOTONIC_RAW, ORDERED_BARRIER_RDTSCP) => bench_libc_raw_rdtscp,
    (SOURCE_LIBC_MONOTONIC_RAW, ORDERED_BARRIER_MFENCE) => bench_libc_raw_mfence,
    (SOURCE_LIBC_MONOTONIC_RAW, ORDERED_BARRIER_CPUID) => bench_libc_raw_cpuid,
    (SOURCE_LIBC_MONOTONIC_RAW, ORDERED_BARRIER_SERIALIZE) => bench_libc_raw_serialize,
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_MONOTONIC, ORDERED_BARRIER_OS_OWNED) => bench_syscall64_monotonic_os_owned,
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_MONOTONIC, ORDERED_BARRIER_LFENCE) => bench_syscall64_monotonic_lfence,
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_MONOTONIC, ORDERED_BARRIER_RDTSCP) => bench_syscall64_monotonic_rdtscp,
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_MONOTONIC, ORDERED_BARRIER_MFENCE) => bench_syscall64_monotonic_mfence,
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_MONOTONIC, ORDERED_BARRIER_CPUID) => bench_syscall64_monotonic_cpuid,
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_MONOTONIC, ORDERED_BARRIER_SERIALIZE) => bench_syscall64_monotonic_serialize,
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_MONOTONIC_RAW, ORDERED_BARRIER_OS_OWNED) => bench_syscall64_raw_os_owned,
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_MONOTONIC_RAW, ORDERED_BARRIER_LFENCE) => bench_syscall64_raw_lfence,
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_MONOTONIC_RAW, ORDERED_BARRIER_RDTSCP) => bench_syscall64_raw_rdtscp,
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_MONOTONIC_RAW, ORDERED_BARRIER_MFENCE) => bench_syscall64_raw_mfence,
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_MONOTONIC_RAW, ORDERED_BARRIER_CPUID) => bench_syscall64_raw_cpuid,
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_MONOTONIC_RAW, ORDERED_BARRIER_SERIALIZE) => bench_syscall64_raw_serialize,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_MONOTONIC, ORDERED_BARRIER_OS_OWNED) => bench_time32_monotonic_os_owned,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_MONOTONIC, ORDERED_BARRIER_LFENCE) => bench_time32_monotonic_lfence,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_MONOTONIC, ORDERED_BARRIER_RDTSCP) => bench_time32_monotonic_rdtscp,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_MONOTONIC, ORDERED_BARRIER_MFENCE) => bench_time32_monotonic_mfence,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_MONOTONIC, ORDERED_BARRIER_CPUID) => bench_time32_monotonic_cpuid,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_MONOTONIC, ORDERED_BARRIER_SERIALIZE) => bench_time32_monotonic_serialize,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_MONOTONIC_RAW, ORDERED_BARRIER_OS_OWNED) => bench_time32_raw_os_owned,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_MONOTONIC_RAW, ORDERED_BARRIER_LFENCE) => bench_time32_raw_lfence,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_MONOTONIC_RAW, ORDERED_BARRIER_RDTSCP) => bench_time32_raw_rdtscp,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_MONOTONIC_RAW, ORDERED_BARRIER_MFENCE) => bench_time32_raw_mfence,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_MONOTONIC_RAW, ORDERED_BARRIER_CPUID) => bench_time32_raw_cpuid,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_MONOTONIC_RAW, ORDERED_BARRIER_SERIALIZE) => bench_time32_raw_serialize,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_MONOTONIC, ORDERED_BARRIER_OS_OWNED) => bench_time64_monotonic_os_owned,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_MONOTONIC, ORDERED_BARRIER_LFENCE) => bench_time64_monotonic_lfence,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_MONOTONIC, ORDERED_BARRIER_RDTSCP) => bench_time64_monotonic_rdtscp,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_MONOTONIC, ORDERED_BARRIER_MFENCE) => bench_time64_monotonic_mfence,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_MONOTONIC, ORDERED_BARRIER_CPUID) => bench_time64_monotonic_cpuid,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_MONOTONIC, ORDERED_BARRIER_SERIALIZE) => bench_time64_monotonic_serialize,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_OS_OWNED) => bench_time64_raw_os_owned,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_LFENCE) => bench_time64_raw_lfence,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_RDTSCP) => bench_time64_raw_rdtscp,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_MFENCE) => bench_time64_raw_mfence,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_CPUID) => bench_time64_raw_cpuid,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_SERIALIZE) => bench_time64_raw_serialize,
    (SOURCE_VDSO_MONOTONIC, ORDERED_BARRIER_OS_OWNED) => bench_vdso_monotonic_os_owned,
    (SOURCE_VDSO_MONOTONIC, ORDERED_BARRIER_LFENCE) => bench_vdso_monotonic_lfence,
    (SOURCE_VDSO_MONOTONIC, ORDERED_BARRIER_RDTSCP) => bench_vdso_monotonic_rdtscp,
    (SOURCE_VDSO_MONOTONIC, ORDERED_BARRIER_MFENCE) => bench_vdso_monotonic_mfence,
    (SOURCE_VDSO_MONOTONIC, ORDERED_BARRIER_CPUID) => bench_vdso_monotonic_cpuid,
    (SOURCE_VDSO_MONOTONIC, ORDERED_BARRIER_SERIALIZE) => bench_vdso_monotonic_serialize,
    (SOURCE_VDSO_MONOTONIC_RAW, ORDERED_BARRIER_OS_OWNED) => bench_vdso_raw_os_owned,
    (SOURCE_VDSO_MONOTONIC_RAW, ORDERED_BARRIER_LFENCE) => bench_vdso_raw_lfence,
    (SOURCE_VDSO_MONOTONIC_RAW, ORDERED_BARRIER_RDTSCP) => bench_vdso_raw_rdtscp,
    (SOURCE_VDSO_MONOTONIC_RAW, ORDERED_BARRIER_MFENCE) => bench_vdso_raw_mfence,
    (SOURCE_VDSO_MONOTONIC_RAW, ORDERED_BARRIER_CPUID) => bench_vdso_raw_cpuid,
    (SOURCE_VDSO_MONOTONIC_RAW, ORDERED_BARRIER_SERIALIZE) => bench_vdso_raw_serialize,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC, ORDERED_BARRIER_OS_OWNED) => {
      bench_vdso_time64_monotonic_os_owned
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC, ORDERED_BARRIER_LFENCE) => bench_vdso_time64_monotonic_lfence,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC, ORDERED_BARRIER_RDTSCP) => bench_vdso_time64_monotonic_rdtscp,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC, ORDERED_BARRIER_MFENCE) => bench_vdso_time64_monotonic_mfence,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC, ORDERED_BARRIER_CPUID) => bench_vdso_time64_monotonic_cpuid,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC, ORDERED_BARRIER_SERIALIZE) => {
      bench_vdso_time64_monotonic_serialize
    }
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_OS_OWNED) => bench_vdso_time64_raw_os_owned,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_LFENCE) => bench_vdso_time64_raw_lfence,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_RDTSCP) => bench_vdso_time64_raw_rdtscp,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_MFENCE) => bench_vdso_time64_raw_mfence,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_CPUID) => bench_vdso_time64_raw_cpuid,
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_MONOTONIC_RAW, ORDERED_BARRIER_SERIALIZE) => {
      bench_vdso_time64_raw_serialize
    }
    (SOURCE_LIBC_BOOTTIME, barrier) => match barrier {
      ORDERED_BARRIER_OS_OWNED => bench_libc_boottime_os_owned,
      ORDERED_BARRIER_LFENCE => bench_libc_boottime_lfence,
      ORDERED_BARRIER_RDTSCP => bench_libc_boottime_rdtscp,
      ORDERED_BARRIER_MFENCE => bench_libc_boottime_mfence,
      ORDERED_BARRIER_SERIALIZE => bench_libc_boottime_serialize,
      _ => bench_libc_boottime_cpuid,
    },
    #[cfg(target_pointer_width = "64")]
    (SOURCE_SYSCALL64_BOOTTIME, barrier) => match barrier {
      ORDERED_BARRIER_OS_OWNED => bench_syscall64_boottime_os_owned,
      ORDERED_BARRIER_LFENCE => bench_syscall64_boottime_lfence,
      ORDERED_BARRIER_RDTSCP => bench_syscall64_boottime_rdtscp,
      ORDERED_BARRIER_MFENCE => bench_syscall64_boottime_mfence,
      ORDERED_BARRIER_SERIALIZE => bench_syscall64_boottime_serialize,
      _ => bench_syscall64_boottime_cpuid,
    },
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME32_BOOTTIME, barrier) => match barrier {
      ORDERED_BARRIER_OS_OWNED => bench_time32_boottime_os_owned,
      ORDERED_BARRIER_LFENCE => bench_time32_boottime_lfence,
      ORDERED_BARRIER_RDTSCP => bench_time32_boottime_rdtscp,
      ORDERED_BARRIER_MFENCE => bench_time32_boottime_mfence,
      ORDERED_BARRIER_SERIALIZE => bench_time32_boottime_serialize,
      _ => bench_time32_boottime_cpuid,
    },
    #[cfg(target_pointer_width = "32")]
    (SOURCE_TIME64_BOOTTIME, barrier) => match barrier {
      ORDERED_BARRIER_OS_OWNED => bench_time64_boottime_os_owned,
      ORDERED_BARRIER_LFENCE => bench_time64_boottime_lfence,
      ORDERED_BARRIER_RDTSCP => bench_time64_boottime_rdtscp,
      ORDERED_BARRIER_MFENCE => bench_time64_boottime_mfence,
      ORDERED_BARRIER_SERIALIZE => bench_time64_boottime_serialize,
      _ => bench_time64_boottime_cpuid,
    },
    (SOURCE_VDSO_BOOTTIME, barrier) => match barrier {
      ORDERED_BARRIER_OS_OWNED => bench_vdso_boottime_os_owned,
      ORDERED_BARRIER_LFENCE => bench_vdso_boottime_lfence,
      ORDERED_BARRIER_RDTSCP => bench_vdso_boottime_rdtscp,
      ORDERED_BARRIER_MFENCE => bench_vdso_boottime_mfence,
      ORDERED_BARRIER_SERIALIZE => bench_vdso_boottime_serialize,
      _ => bench_vdso_boottime_cpuid,
    },
    #[cfg(target_pointer_width = "32")]
    (SOURCE_VDSO_TIME64_BOOTTIME, barrier) => match barrier {
      ORDERED_BARRIER_OS_OWNED => bench_vdso_time64_boottime_os_owned,
      ORDERED_BARRIER_LFENCE => bench_vdso_time64_boottime_lfence,
      ORDERED_BARRIER_RDTSCP => bench_vdso_time64_boottime_rdtscp,
      ORDERED_BARRIER_MFENCE => bench_vdso_time64_boottime_mfence,
      ORDERED_BARRIER_SERIALIZE => bench_vdso_time64_boottime_serialize,
      _ => bench_vdso_time64_boottime_cpuid,
    },
    _ => bench_libc_monotonic_cpuid,
  }
}

#[cfg(feature = "bench-internal")]
fn store_instant_evidence(evidence: DomainEvidence<MAX_INSTANT_CANDIDATES, MAX_INSTANT_DECISIONS>) {
  // SAFETY: only the Instant selection owner writes before publication.
  unsafe { (*INSTANT_EVIDENCE.0.get()).write(evidence) };
  INSTANT_EVIDENCE_READY.store(true, Ordering::Release);
}

#[cfg(feature = "bench-internal")]
fn store_ordered_evidence(evidence: DomainEvidence<MAX_ORDERED_CANDIDATES, MAX_ORDERED_DECISIONS>) {
  // SAFETY: only the Ordered selection owner writes before publication.
  unsafe { (*ORDERED_EVIDENCE.0.get()).write(evidence) };
  ORDERED_EVIDENCE_READY.store(true, Ordering::Release);
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_probe_evidence() -> WallProbeEvidence {
  let instant_provider = selected_instant_provider();
  let ordered_provider = selected_ordered_provider();
  debug_assert!(INSTANT_EVIDENCE_READY.load(Ordering::Acquire));
  debug_assert!(ORDERED_EVIDENCE_READY.load(Ordering::Acquire));
  // SAFETY: each selected-provider acquire follows initialization and Release
  // publication by its process-selection owner.
  let instant = unsafe { (*INSTANT_EVIDENCE.0.get()).assume_init_read() };
  // SAFETY: the same publication invariant holds for Ordered evidence.
  let ordered = unsafe { (*ORDERED_EVIDENCE.0.get()).assume_init_read() };
  combined_evidence(instant, ordered, instant_provider, ordered_provider)
}

#[cfg(feature = "bench-internal")]
fn combined_evidence(
  instant: DomainEvidence<MAX_INSTANT_CANDIDATES, MAX_INSTANT_DECISIONS>,
  ordered: DomainEvidence<MAX_ORDERED_CANDIDATES, MAX_ORDERED_DECISIONS>,
  instant_provider: u8,
  selected_ordered_provider: u8,
) -> WallProbeEvidence {
  let mut instant_names = ["unavailable"; MAX_INSTANT_CANDIDATES];
  let mut instant_eligible = [false; MAX_INSTANT_CANDIDATES];
  let mut instant_medians = [0; MAX_INSTANT_CANDIDATES];
  for index in 0..instant.tournament.candidates.count {
    let provider = instant.tournament.candidates.providers[index];
    instant_names[index] = instant_provider_name(provider);
    instant_eligible[index] = true;
    instant_medians[index] = median(instant.tournament.samples.batches[index]);
  }
  let mut instant_challengers = ["unavailable"; MAX_INSTANT_DECISIONS];
  let mut instant_incumbents = ["unavailable"; MAX_INSTANT_DECISIONS];
  let mut instant_winners = ["unavailable"; MAX_INSTANT_DECISIONS];
  let mut instant_allowances = [0; MAX_INSTANT_DECISIONS];
  let mut instant_wins = [0; MAX_INSTANT_DECISIONS];
  let mut instant_selected = [false; MAX_INSTANT_DECISIONS];
  for index in 0..instant.tournament.decision_count {
    instant_challengers[index] = instant_provider_name(instant.tournament.challengers[index]);
    instant_incumbents[index] = instant_provider_name(instant.tournament.incumbents[index]);
    instant_winners[index] = instant_provider_name(instant.tournament.winners[index]);
    instant_allowances[index] = instant.tournament.decisions[index].allowance;
    instant_wins[index] = instant.tournament.decisions[index].decisive_wins;
    instant_selected[index] = instant.tournament.decisions[index].challenger_selected;
  }

  let mut ordered_names = ["unavailable"; MAX_ORDERED_CANDIDATES];
  let mut ordered_eligible = [false; MAX_ORDERED_CANDIDATES];
  let mut ordered_medians = [0; MAX_ORDERED_CANDIDATES];
  for index in 0..ordered.tournament.candidates.count {
    let provider = ordered.tournament.candidates.providers[index];
    ordered_names[index] = ordered_provider_name(provider);
    ordered_eligible[index] = true;
    ordered_medians[index] = median(ordered.tournament.samples.batches[index]);
  }
  let mut ordered_barrier_names = ["unavailable"; 6];
  for (index, name) in ordered_barrier_names.iter_mut().enumerate().take(ordered.barriers.count) {
    *name = barrier_name(ordered.barriers.barriers[index]);
  }
  let mut ordered_challengers = ["unavailable"; MAX_ORDERED_DECISIONS];
  let mut ordered_incumbents = ["unavailable"; MAX_ORDERED_DECISIONS];
  let mut ordered_winners = ["unavailable"; MAX_ORDERED_DECISIONS];
  let mut ordered_allowances = [0; MAX_ORDERED_DECISIONS];
  let mut ordered_wins = [0; MAX_ORDERED_DECISIONS];
  let mut ordered_selected = [false; MAX_ORDERED_DECISIONS];
  for index in 0..ordered.tournament.decision_count {
    ordered_challengers[index] = ordered_provider_name(ordered.tournament.challengers[index]);
    ordered_incumbents[index] = ordered_provider_name(ordered.tournament.incumbents[index]);
    ordered_winners[index] = ordered_provider_name(ordered.tournament.winners[index]);
    ordered_allowances[index] = ordered.tournament.decisions[index].allowance;
    ordered_wins[index] = ordered.tournament.decisions[index].decisive_wins;
    ordered_selected[index] = ordered.tournament.decisions[index].challenger_selected;
  }

  #[cfg(target_pointer_width = "64")]
  let syscall_provider = PROVIDER_SYSCALL64_MONOTONIC;
  #[cfg(target_pointer_width = "32")]
  let syscall_provider =
    if instant_candidate_index(&instant.tournament, PROVIDER_TIME32_MONOTONIC).is_some() {
      PROVIDER_TIME32_MONOTONIC
    } else {
      PROVIDER_TIME64_MONOTONIC
    };
  let tsc_batches = instant_samples(&instant.tournament, PROVIDER_TSC);
  let clock_batches = instant_samples(&instant.tournament, PROVIDER_LIBC_MONOTONIC);
  let syscall_batches = instant_samples(&instant.tournament, syscall_provider);

  let ordered_tsc_batches = ordered_tsc_samples(&ordered.tournament);
  let ordered_clock_provider = best_source_provider(&ordered.tournament, SOURCE_LIBC_MONOTONIC);
  let ordered_syscall_source = syscall_provider - PROVIDER_SOURCE_BASE;
  let ordered_syscall_provider = best_source_provider(&ordered.tournament, ordered_syscall_source);
  let ordered_clock_batches = ordered_clock_provider
    .map(|provider| ordered_samples(&ordered.tournament, provider))
    .unwrap_or([0; PROBE_BATCHES]);
  let ordered_syscall_batches = ordered_syscall_provider
    .map(|provider| ordered_samples(&ordered.tournament, provider))
    .unwrap_or([0; PROBE_BATCHES]);

  let instant_direct_decision =
    decision_for_challenger(&instant.tournament, PROVIDER_TSC).unwrap_or_else(empty_decision);
  let instant_syscall_decision =
    decision_for_challenger(&instant.tournament, syscall_provider).unwrap_or_else(empty_decision);
  let ordered_direct_decision = ordered_direct_decision(&ordered.tournament);
  let ordered_syscall_decision = ordered_syscall_provider
    .and_then(|provider| decision_for_challenger(&ordered.tournament, provider))
    .unwrap_or_else(empty_decision);
  let ordered_clock_fast = ordered.fast_barrier.map(|barrier| {
    ordered_samples(&ordered.tournament, ordered_provider(SOURCE_LIBC_MONOTONIC, barrier))
  });
  let ordered_clock_baseline = ordered_samples(
    &ordered.tournament,
    ordered_provider(SOURCE_LIBC_MONOTONIC, ordered.baseline_barrier),
  );
  let ordered_syscall_fast = ordered.fast_barrier.map(|barrier| {
    ordered_samples(&ordered.tournament, ordered_provider(ordered_syscall_source, barrier))
  });
  let ordered_syscall_baseline = ordered_samples(
    &ordered.tournament,
    ordered_provider(ordered_syscall_source, ordered.baseline_barrier),
  );
  let ordered_clock_barrier_decision = ordered
    .fast_barrier
    .and_then(|barrier| {
      decision_for_challenger(&ordered.tournament, ordered_provider(SOURCE_LIBC_MONOTONIC, barrier))
    })
    .unwrap_or_else(empty_decision);
  let ordered_syscall_barrier_decision = ordered
    .fast_barrier
    .and_then(|barrier| {
      decision_for_challenger(
        &ordered.tournament,
        ordered_provider(ordered_syscall_source, barrier),
      )
    })
    .unwrap_or_else(empty_decision);

  WallProbeEvidence {
    instant_eligibility: instant.eligibility.name(),
    ordered_eligibility: ordered.eligibility.name(),
    reads_per_batch: PROBE_READS,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    instant_candidate_count: instant.tournament.candidates.count,
    instant_candidate_names: instant_names,
    instant_candidate_eligible: instant_eligible,
    instant_candidate_batches_ns: instant.tournament.samples.batches,
    instant_candidate_medians_ns: instant_medians,
    instant_tournament_decision_count: instant.tournament.decision_count,
    instant_tournament_challengers: instant_challengers,
    instant_tournament_incumbents: instant_incumbents,
    instant_tournament_winners: instant_winners,
    instant_tournament_allowances_ns: instant_allowances,
    instant_tournament_decisive_wins: instant_wins,
    instant_tournament_challenger_selected: instant_selected,
    ordered_candidate_count: ordered.tournament.candidates.count,
    ordered_candidate_names: ordered_names,
    ordered_candidate_eligible: ordered_eligible,
    ordered_candidate_batches_ns: ordered.tournament.samples.batches,
    ordered_candidate_medians_ns: ordered_medians,
    ordered_barrier_candidate_count: ordered.barriers.count,
    ordered_barrier_candidate_names: ordered_barrier_names,
    ordered_tournament_decision_count: ordered.tournament.decision_count,
    ordered_tournament_challengers: ordered_challengers,
    ordered_tournament_incumbents: ordered_incumbents,
    ordered_tournament_winners: ordered_winners,
    ordered_tournament_allowances_ns: ordered_allowances,
    ordered_tournament_decisive_wins: ordered_wins,
    ordered_tournament_challenger_selected: ordered_selected,
    instant_selected_provider: instant_provider_name(instant_provider),
    ordered_selected_provider: ordered_provider_name(selected_ordered_provider),
    instant_fallback_provider: instant_provider_name(instant.fallback_provider),
    ordered_fallback_provider: ordered_provider_name(ordered.fallback_provider),
    ordered_os_barrier: if provider_uses_tsc(selected_ordered_provider) {
      "not_applicable"
    } else {
      barrier_name(ordered_barrier(selected_ordered_provider))
    },
    ordered_bare_syscall_os_owned_eligible: bare_syscall_os_owned_eligible(),
    ordered_bare_syscall_os_owned_basis: bare_syscall_os_owned_basis(),
    ordered_fast_barrier_candidate: ordered.fast_barrier.map(barrier_name).unwrap_or("none"),
    ordered_baseline_barrier: barrier_name(ordered.baseline_barrier),
    instant_tsc_selected: instant_provider == PROVIDER_TSC,
    ordered_tsc_selected: provider_uses_tsc(selected_ordered_provider),
    tsc_batches_ns: tsc_batches,
    clock_batches_ns: clock_batches,
    syscall_batches_ns: syscall_batches,
    ordered_tsc_batches_ns: ordered_tsc_batches,
    ordered_clock_batches_ns: ordered_clock_batches,
    ordered_syscall_batches_ns: ordered_syscall_batches,
    tsc_median_ns: median_or_zero(tsc_batches),
    clock_median_ns: median_or_zero(clock_batches),
    syscall_median_ns: median_or_zero(syscall_batches),
    ordered_tsc_median_ns: median_or_zero(ordered_tsc_batches),
    ordered_clock_median_ns: median_or_zero(ordered_clock_batches),
    ordered_syscall_median_ns: median_or_zero(ordered_syscall_batches),
    instant_allowance_ns: instant_direct_decision.allowance,
    ordered_allowance_ns: ordered_direct_decision.allowance,
    instant_decisive_wins: instant_direct_decision.decisive_wins,
    ordered_decisive_wins: ordered_direct_decision.decisive_wins,
    instant_syscall_vs_clock_allowance_ns: instant_syscall_decision.allowance,
    ordered_syscall_vs_clock_allowance_ns: ordered_syscall_decision.allowance,
    instant_syscall_vs_clock_decisive_wins: instant_syscall_decision.decisive_wins,
    ordered_syscall_vs_clock_decisive_wins: ordered_syscall_decision.decisive_wins,
    ordered_clock_fast_batches_ns: ordered_clock_fast.unwrap_or([0; PROBE_BATCHES]),
    ordered_clock_baseline_batches_ns: ordered_clock_baseline,
    ordered_syscall_fast_batches_ns: ordered_syscall_fast.unwrap_or([0; PROBE_BATCHES]),
    ordered_syscall_baseline_batches_ns: ordered_syscall_baseline,
    ordered_clock_barrier_allowance_ns: ordered_clock_barrier_decision.allowance,
    ordered_clock_barrier_decisive_wins: ordered_clock_barrier_decision.decisive_wins,
    ordered_syscall_barrier_allowance_ns: ordered_syscall_barrier_decision.allowance,
    ordered_syscall_barrier_decisive_wins: ordered_syscall_barrier_decision.decisive_wins,
  }
}

#[cfg(feature = "bench-internal")]
fn instant_candidate_index(
  tournament: &Tournament<MAX_INSTANT_CANDIDATES, MAX_INSTANT_DECISIONS>,
  provider: u8,
) -> Option<usize> {
  tournament.candidates.providers[..tournament.candidates.count]
    .iter()
    .position(|candidate| *candidate == provider)
}

#[cfg(feature = "bench-internal")]
fn instant_samples(
  tournament: &Tournament<MAX_INSTANT_CANDIDATES, MAX_INSTANT_DECISIONS>,
  provider: u8,
) -> [u64; PROBE_BATCHES] {
  instant_candidate_index(tournament, provider)
    .map(|index| tournament.samples.batches[index])
    .unwrap_or([0; PROBE_BATCHES])
}

#[cfg(feature = "bench-internal")]
fn ordered_candidate_index(
  tournament: &Tournament<MAX_ORDERED_CANDIDATES, MAX_ORDERED_DECISIONS>,
  provider: u8,
) -> Option<usize> {
  tournament.candidates.providers[..tournament.candidates.count]
    .iter()
    .position(|candidate| *candidate == provider)
}

#[cfg(feature = "bench-internal")]
fn ordered_samples(
  tournament: &Tournament<MAX_ORDERED_CANDIDATES, MAX_ORDERED_DECISIONS>,
  provider: u8,
) -> [u64; PROBE_BATCHES] {
  ordered_candidate_index(tournament, provider)
    .map(|index| tournament.samples.batches[index])
    .unwrap_or([0; PROBE_BATCHES])
}

#[cfg(feature = "bench-internal")]
fn ordered_tsc_samples(
  tournament: &Tournament<MAX_ORDERED_CANDIDATES, MAX_ORDERED_DECISIONS>,
) -> [u64; PROBE_BATCHES] {
  for index in 0..tournament.candidates.count {
    if provider_uses_tsc(tournament.candidates.providers[index]) {
      return tournament.samples.batches[index];
    }
  }
  [0; PROBE_BATCHES]
}

#[cfg(feature = "bench-internal")]
fn best_source_provider(
  tournament: &Tournament<MAX_ORDERED_CANDIDATES, MAX_ORDERED_DECISIONS>,
  source: u8,
) -> Option<u8> {
  let mut selected = None;
  let mut selected_samples = [u64::MAX; PROBE_BATCHES];
  for index in 0..tournament.candidates.count {
    let provider = tournament.candidates.providers[index];
    if !is_ordered_os_provider(provider) || ordered_source(provider) != source {
      continue;
    }
    if selected.is_none()
      || evaluate_challenger(tournament.samples.batches[index], selected_samples)
        .challenger_selected
    {
      selected = Some(provider);
      selected_samples = tournament.samples.batches[index];
    }
  }
  selected
}

#[cfg(feature = "bench-internal")]
fn decision_for_challenger<const N: usize, const D: usize>(
  tournament: &Tournament<N, D>,
  provider: u8,
) -> Option<SelectionDecision> {
  tournament.challengers[..tournament.decision_count]
    .iter()
    .position(|challenger| *challenger == provider)
    .map(|index| tournament.decisions[index])
}

#[cfg(feature = "bench-internal")]
fn ordered_direct_decision(
  tournament: &Tournament<MAX_ORDERED_CANDIDATES, MAX_ORDERED_DECISIONS>,
) -> SelectionDecision {
  for index in 0..tournament.decision_count {
    if provider_uses_tsc(tournament.challengers[index]) {
      return tournament.decisions[index];
    }
  }
  empty_decision()
}

#[cfg(feature = "bench-internal")]
fn median_or_zero(samples: [u64; PROBE_BATCHES]) -> u64 {
  if samples == [0; PROBE_BATCHES] { 0 } else { median(samples) }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn ordered_provider_round_trips_every_compound() {
    for source in 0..SOURCE_VARIANTS {
      for barrier in 0..ORDERED_BARRIER_VARIANTS {
        let provider = ordered_provider(source, barrier);
        assert!(is_ordered_os_provider(provider));
        assert_eq!(ordered_source(provider), source);
        assert_eq!(ordered_barrier(provider), barrier);
      }
    }
  }

  #[test]
  fn ordered_os_candidates_include_the_os_owned_boundary() {
    let barriers = eligible_ordered_os_barriers();
    assert_eq!(barriers.barriers[0], ORDERED_BARRIER_CPUID);
    assert!(barriers.barriers[..barriers.count].contains(&ORDERED_BARRIER_OS_OWNED));
  }

  #[test]
  fn vendor_neutral_features_remain_candidates_for_unknown_vendors() {
    let barriers = ordered_barriers_from_capabilities(false, false, false, true, false, true);
    assert_eq!(&barriers.barriers[..barriers.count], &[
      ORDERED_BARRIER_CPUID,
      ORDERED_BARRIER_OS_OWNED,
      ORDERED_BARRIER_RDTSCP,
      ORDERED_BARRIER_SERIALIZE,
    ],);
  }

  #[test]
  fn amd_exposes_every_architecturally_gated_barrier() {
    let barriers = ordered_barriers_from_capabilities(false, true, true, true, true, true);
    let eligible = &barriers.barriers[..barriers.count];
    assert!(eligible.contains(&ORDERED_BARRIER_LFENCE));
    assert!(eligible.contains(&ORDERED_BARRIER_MFENCE));
    assert!(eligible.contains(&ORDERED_BARRIER_RDTSCP));
    assert!(eligible.contains(&ORDERED_BARRIER_SERIALIZE));
  }

  #[test]
  fn bare_syscall_os_owned_requires_its_entry_ordering_contract() {
    #[cfg(target_pointer_width = "64")]
    {
      assert!(!os_owned_eligible_for_source(SOURCE_LIBC_MONOTONIC, false));
      assert!(!os_owned_eligible_for_source(SOURCE_LIBC_MONOTONIC_RAW, false));
      assert!(!os_owned_eligible_for_source(SOURCE_LIBC_BOOTTIME, false));
      assert!(os_owned_eligible_for_source(SOURCE_LIBC_MONOTONIC, true));
      assert!(!os_owned_eligible_for_source(SOURCE_SYSCALL64_MONOTONIC, false));
      assert!(!os_owned_eligible_for_source(SOURCE_SYSCALL64_MONOTONIC_RAW, false));
      assert!(os_owned_eligible_for_source(SOURCE_SYSCALL64_MONOTONIC, true));
    }
    #[cfg(target_pointer_width = "32")]
    {
      assert!(os_owned_eligible_for_source(SOURCE_TIME32_MONOTONIC, false));
      assert!(os_owned_eligible_for_source(SOURCE_TIME64_MONOTONIC, false));
    }
  }

  #[test]
  fn selection_requires_a_repeatable_material_win() {
    let incumbent = [100_000; PROBE_BATCHES];
    assert!(evaluate_challenger([90_000; PROBE_BATCHES], incumbent).challenger_selected);
    assert!(!evaluate_challenger([96_000; PROBE_BATCHES], incumbent).challenger_selected);

    let mut noisy = [90_000; PROBE_BATCHES];
    noisy[0] = 100_000;
    noisy[1] = 100_000;
    assert!(!evaluate_challenger(noisy, incumbent).challenger_selected);
  }

  #[test]
  fn selection_window_recovers_the_tsc_frequency() {
    let window = TscFrequencyWindow { wall_start: 100, tick_start: 1_000 };
    assert_eq!(frequency_from_window(window, 1_000_000_100, 2_500_001_000), Some(2_500_000_000),);
    assert_eq!(frequency_from_window(window, 99, 2_500_001_000), None);
    assert_eq!(frequency_from_window(window, 1_000_000_100, 999), None);
  }

  #[test]
  fn requires_tsc_to_be_the_exact_current_clocksource() {
    assert!(current_clocksource_is_tsc(b"tsc\n"));
    assert!(current_clocksource_is_tsc(b"  tsc \0ignored"));
    assert!(!current_clocksource_is_tsc(b"kvm-clock tsc hpet\n"));
    assert!(!current_clocksource_is_tsc(b"kvm-clock\n"));
    assert!(!current_clocksource_is_tsc(b"tsc-early\n"));
  }

  #[test]
  fn tsc_denial_candidate_set_contains_only_raw_abis() {
    let candidates = instant_os_candidates(false);
    assert!(candidates.count > 0);
    #[cfg(target_pointer_width = "64")]
    assert!(candidates.providers[..candidates.count].iter().all(|provider| matches!(
      *provider,
      PROVIDER_SYSCALL64_MONOTONIC | PROVIDER_SYSCALL64_MONOTONIC_RAW | PROVIDER_SYSCALL64_BOOTTIME
    )));
    #[cfg(target_pointer_width = "32")]
    assert!(candidates.providers[..candidates.count].iter().all(|provider| matches!(
      *provider,
      PROVIDER_TIME32_MONOTONIC
        | PROVIDER_TIME32_MONOTONIC_RAW
        | PROVIDER_TIME32_BOOTTIME
        | PROVIDER_TIME64_MONOTONIC
        | PROVIDER_TIME64_MONOTONIC_RAW
        | PROVIDER_TIME64_BOOTTIME
    )));
  }

  #[test]
  fn fixed_evidence_capacity_covers_the_largest_i686_tournament() {
    assert_eq!(MAX_INSTANT_CANDIDATES, 15 + 1);
    assert_eq!(MAX_ORDERED_CANDIDATES, 15 * 6 + 1);
  }

  #[test]
  fn selected_domains_are_monotonic_and_survive_fork() {
    let instant_before = ticks();
    let ordered_before = ticks_ordered();
    assert!(frequency() > 0);
    assert!(frequency_for(selected_ordered_provider()) > 0);
    assert!(ticks() >= instant_before);
    assert!(ticks_ordered() >= ordered_before);

    // SAFETY: the child only performs tach reads and `_exit`; the parent waits
    // immediately for that exact process.
    let child = unsafe { libc::fork() };
    assert!(child >= 0);
    if child == 0 {
      let valid = ticks() >= instant_before && ticks_ordered() >= ordered_before;
      // SAFETY: `_exit` terminates without inherited Rust cleanup.
      unsafe { libc::_exit(if valid { 0 } else { 1 }) };
    }
    let mut status = 0;
    // SAFETY: child is live and status is writable wait storage.
    assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
    assert_eq!(status, 0);
  }

  #[cfg(target_pointer_width = "32")]
  #[test]
  fn i686_kernel_time_layouts_match_the_uapi() {
    assert_eq!(core::mem::size_of::<LinuxTime32>(), 8);
    assert_eq!(core::mem::size_of::<LinuxKernelTimespec>(), 16);
    assert_ne!(PROVIDER_TIME32_MONOTONIC, PROVIDER_TIME64_MONOTONIC);
    assert_ne!(PROVIDER_TIME32_MONOTONIC_RAW, PROVIDER_TIME64_MONOTONIC_RAW);
  }

  #[cfg(feature = "bench-internal")]
  #[test]
  fn selected_benchmark_primitive_has_no_selector_in_its_reader() {
    let instant = bench_selected_instant_primitive();
    let ordered = bench_selected_ordered_primitive();
    assert_eq!(instant.name, bench_instant_provider());
    assert_eq!(ordered.name, bench_ordered_provider());
    black_box((instant.read)());
    black_box((ordered.read)());
  }
}
