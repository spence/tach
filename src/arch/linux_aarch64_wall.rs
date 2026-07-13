//! Linux-kernel aarch64 wall-clock provider selection.
//!
//! Linux normally permits EL0 reads of `CNTVCT_EL0`, but deliberately traps
//! them when an architectural-timer erratum needs an out-of-line workaround.
//! The arm64 trap handler emulates `CNTVCT_EL0` and `CNTVCTSS_EL0` through the
//! kernel's workaround-aware counter reader. A thread can separately request
//! `SIGSEGV` for counter access with `PR_SET_TSC` on kernels that implement the
//! arm64 control, which entered upstream in Linux 6.12. Tach first checks
//! `PR_GET_TSC`. When an older kernel reports that option unsupported, a
//! strictly parsed pre-6.12 `uname` release proves that the per-thread denial
//! control does not exist. A successful query remains authoritative regardless
//! of the reported release, so Android and vendor backports are handled as
//! feature probes rather than guessed from their base version. New-enough,
//! malformed, or unavailable releases fail closed. Tach then measures the exact
//! branched hot paths so a trapped read cannot win merely because the ISA
//! advertises it.
//!
//! `Instant` and `OrderedInstant` select independently. A bare CNTVCT read is
//! eligible for local monotonic samples because the register contract orders
//! CNTVCT/CNTVCTSS reads with one another. It is not ordered with surrounding
//! work. Ordered reads use either `isb; mrs cntvct_el0` or the architecturally
//! self-synchronizing `CNTVCTSS_EL0`. Linux CLOCK_MONOTONIC,
//! CLOCK_MONOTONIC_RAW, and CLOCK_BOOTTIME remain eligible in both domains: their arm64 vDSO
//! counter accessors use the same ordered timer primitives, and their fallback
//! syscalls are context-synchronizing exceptions. The libc/vDSO and explicit
//! syscall forms compete separately because virtualization and kernel/vDSO
//! configuration can reverse their usual ordering.
//!
//! FEAT_SB is intentionally not a candidate. SB constrains side-channel-
//! observable speculative effects; it is not an instruction-completion
//! barrier and cannot prove that a counter sample occurred after a prior
//! Acquire observation.
//!
//! Counter permission is per thread while each selected wall timeline is
//! process-wide. Initial denial safely measures only the explicit
//! CLOCK_MONOTONIC, CLOCK_MONOTONIC_RAW, and CLOCK_BOOTTIME syscalls because a vDSO may
//! execute the same denied counter instruction. Every reading thread must
//! retain counter permission after a direct provider or vDSO provider is
//! selected; explicitly disabling it is an external fault boundary.

#[cfg(feature = "bench-internal")]
use core::cell::UnsafeCell;
use core::hint::black_box;
#[cfg(feature = "bench-internal")]
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicI32, AtomicU8, AtomicU64, Ordering};

const PROVIDER_UNKNOWN: u8 = 0;
const PROVIDER_SELECTING: u8 = 1;
const PROVIDER_CNTVCT: u8 = 2;
const PROVIDER_CLOCK_MONOTONIC: u8 = 3;
const PROVIDER_ISB_CNTVCT: u8 = 4;
const PROVIDER_CNTVCTSS: u8 = 5;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL: u8 = 6;
const PROVIDER_CLOCK_MONOTONIC_RAW: u8 = 7;
const PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL: u8 = 8;
const PROVIDER_CLOCK_MONOTONIC_VDSO: u8 = 9;
const PROVIDER_CLOCK_MONOTONIC_RAW_VDSO: u8 = 10;
const PROVIDER_CLOCK_BOOTTIME: u8 = 11;
const PROVIDER_CLOCK_BOOTTIME_SYSCALL: u8 = 12;
const PROVIDER_CLOCK_BOOTTIME_VDSO: u8 = 13;
const HOT_PROVIDER_ISB_CNTVCT_IDENTITY: u8 = 14;
const HOT_PROVIDER_CNTVCTSS_IDENTITY: u8 = 15;
const MAX_ORDERED_HOT_SCALE: u64 = u64::MAX >> 8;
const IDENTITY_NANOS_PER_TICK_Q32: u64 = 1_u64 << 32;
const REENTRANT_ORDERED_HOT_STATE: u64 =
  (IDENTITY_NANOS_PER_TICK_Q32 << 8) | PROVIDER_CLOCK_MONOTONIC_SYSCALL as u64;

const MAX_INSTANT_CANDIDATES: usize = 10;
const MAX_ORDERED_CANDIDATES: usize = 11;

#[derive(Clone, Copy)]
struct CandidateList<const N: usize> {
  providers: [u8; N],
  count: usize,
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

  fn as_slice(&self) -> &[u8] {
    &self.providers[..self.count]
  }
}

fn instant_candidates(
  counter_eligible: bool,
  vdso_available: bool,
  vdso_raw_available: bool,
  vdso_boottime_available: bool,
) -> CandidateList<MAX_INSTANT_CANDIDATES> {
  let mut candidates = CandidateList::new();
  if counter_eligible {
    candidates.push(PROVIDER_CLOCK_MONOTONIC);
    candidates.push(PROVIDER_CLOCK_MONOTONIC_RAW);
    candidates.push(PROVIDER_CLOCK_BOOTTIME);
    if vdso_available {
      candidates.push(PROVIDER_CLOCK_MONOTONIC_VDSO);
    }
    if vdso_raw_available {
      candidates.push(PROVIDER_CLOCK_MONOTONIC_RAW_VDSO);
    }
    if vdso_boottime_available {
      candidates.push(PROVIDER_CLOCK_BOOTTIME_VDSO);
    }
  }
  candidates.push(PROVIDER_CLOCK_MONOTONIC_SYSCALL);
  candidates.push(PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL);
  candidates.push(PROVIDER_CLOCK_BOOTTIME_SYSCALL);
  if counter_eligible {
    candidates.push(PROVIDER_CNTVCT);
  }
  candidates
}

fn ordered_candidates(
  counter_eligible: bool,
  cntvctss_eligible: bool,
  vdso_available: bool,
  vdso_raw_available: bool,
  vdso_boottime_available: bool,
) -> CandidateList<MAX_ORDERED_CANDIDATES> {
  let mut candidates = CandidateList::new();
  if counter_eligible {
    candidates.push(PROVIDER_CLOCK_MONOTONIC);
    candidates.push(PROVIDER_CLOCK_MONOTONIC_RAW);
    candidates.push(PROVIDER_CLOCK_BOOTTIME);
  }
  candidates.push(PROVIDER_CLOCK_MONOTONIC_SYSCALL);
  candidates.push(PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL);
  candidates.push(PROVIDER_CLOCK_BOOTTIME_SYSCALL);
  if counter_eligible {
    if vdso_available {
      candidates.push(PROVIDER_CLOCK_MONOTONIC_VDSO);
    }
    if vdso_raw_available {
      candidates.push(PROVIDER_CLOCK_MONOTONIC_RAW_VDSO);
    }
    if vdso_boottime_available {
      candidates.push(PROVIDER_CLOCK_BOOTTIME_VDSO);
    }
    candidates.push(PROVIDER_ISB_CNTVCT);
    if cntvctss_eligible {
      candidates.push(PROVIDER_CNTVCTSS);
    }
  }
  candidates
}

const PROBE_BATCHES: usize = 9;
const PROBE_READS: u64 = 4096;
const PROBE_WARMUP_READS: u64 = 1024;
const REQUIRED_DECISIVE_WINS: usize = 8;
#[cfg(feature = "bench-internal")]
const MAX_INSTANT_TOURNAMENT_STEPS: usize = MAX_INSTANT_CANDIDATES - 1;
#[cfg(feature = "bench-internal")]
const MAX_ORDERED_TOURNAMENT_STEPS: usize = MAX_ORDERED_CANDIDATES - 1;

const PR_GET_TSC: libc::c_int = 25;
#[cfg(test)]
const PR_SET_TSC: libc::c_int = 26;
const PR_TSC_ENABLE: libc::c_int = 1;
const PR_TSC_SIGSEGV: libc::c_int = 2;

#[cfg(feature = "bench-internal")]
const HWCAP_SB: libc::c_ulong = 1 << 29;

static INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static INSTANT_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static INSTANT_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static ORDERED_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_HOT_STATE: AtomicU64 = AtomicU64::new(REENTRANT_ORDERED_HOT_STATE);

// Probe readers retain the same predicted atomic-load branch as the public
// paths while the public provider states remain SELECTING.
static PROBE_INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_CNTVCT);
static PROBE_ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_ISB_CNTVCT);

// Both direct domains read the same architectural counter at the same rate.
static DIRECT_FREQUENCY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CounterEligibility {
  Eligible,
  PrGetTscUnavailable,
  CounterReadDisabled,
}

impl CounterEligibility {
  #[cfg(feature = "bench-internal")]
  const fn name(self) -> &'static str {
    match self {
      Self::Eligible => "eligible",
      Self::PrGetTscUnavailable => "pr_get_tsc_unavailable",
      Self::CounterReadDisabled => "pr_get_tsc_not_enabled",
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PermissionBasis {
  PrGetTscEnabled,
  LegacyKernelWithoutPrGetTsc,
  PrGetTscNotEnabled,
  PrGetTscUnknownMode,
  PrGetTscFailed,
  NewKernelWithoutObservablePrGetTsc,
  KernelReleaseUnknown,
}

impl PermissionBasis {
  #[cfg(feature = "bench-internal")]
  const fn name(self) -> &'static str {
    match self {
      Self::PrGetTscEnabled => "pr_get_tsc_enabled",
      Self::LegacyKernelWithoutPrGetTsc => "legacy_kernel_without_pr_get_tsc",
      Self::PrGetTscNotEnabled => "pr_get_tsc_not_enabled",
      Self::PrGetTscUnknownMode => "pr_get_tsc_unknown_mode",
      Self::PrGetTscFailed => "pr_get_tsc_failed",
      Self::NewKernelWithoutObservablePrGetTsc => "new_kernel_without_observable_pr_get_tsc",
      Self::KernelReleaseUnknown => "kernel_release_unknown",
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct KernelVersion {
  major: u32,
  minor: u32,
}

impl KernelVersion {
  const fn predates_arm64_pr_get_tsc(self) -> bool {
    // arm64 first shipped in Linux 3.7. Reject a 2.6 release because a modern
    // kernel can deliberately report one under the UNAME26 personality.
    self.major >= 3 && (self.major < 6 || (self.major == 6 && self.minor < 12))
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CounterAssessment {
  eligibility: CounterEligibility,
  basis: PermissionBasis,
  pr_get_tsc_status: libc::c_long,
  kernel_version: Option<KernelVersion>,
}

impl CounterAssessment {
  const fn counter_eligible(self) -> bool {
    matches!(self.eligibility, CounterEligibility::Eligible)
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
struct RankedCandidate {
  provider: u8,
  samples: Samples,
}

#[derive(Clone, Copy)]
struct TournamentStage {
  #[cfg(feature = "bench-internal")]
  challenger: u8,
  #[cfg(feature = "bench-internal")]
  incumbent: u8,
  decision: SelectionDecision,
}

fn compete(incumbent: &mut RankedCandidate, challenger: RankedCandidate) -> TournamentStage {
  let stage = TournamentStage {
    #[cfg(feature = "bench-internal")]
    challenger: challenger.provider,
    #[cfg(feature = "bench-internal")]
    incumbent: incumbent.provider,
    decision: evaluate_challenger(challenger.samples, incumbent.samples),
  };
  if stage.decision.challenger_selected {
    *incumbent = challenger;
  }
  stage
}

const fn no_challenge_decision() -> SelectionDecision {
  SelectionDecision {
    #[cfg(feature = "bench-internal")]
    allowance: 0,
    #[cfg(feature = "bench-internal")]
    decisive_wins: 0,
    challenger_selected: false,
  }
}

#[cfg(feature = "bench-internal")]
fn stage_evidence(stage: TournamentStage) -> TournamentStepEvidence {
  let winner = if stage.decision.challenger_selected { stage.challenger } else { stage.incumbent };
  TournamentStepEvidence {
    challenger: provider_name(stage.challenger),
    incumbent: provider_name(stage.incumbent),
    allowance_ns: stage.decision.allowance,
    decisive_wins: stage.decision.decisive_wins,
    challenger_selected: stage.decision.challenger_selected,
    winner: provider_name(winner),
  }
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // The serializer may project a subset of complete selector evidence.
pub(crate) struct TournamentStepEvidence {
  pub(crate) challenger: &'static str,
  pub(crate) incumbent: &'static str,
  pub(crate) allowance_ns: u64,
  pub(crate) decisive_wins: usize,
  pub(crate) challenger_selected: bool,
  pub(crate) winner: &'static str,
}

#[cfg(feature = "bench-internal")]
const EMPTY_TOURNAMENT_STEP: TournamentStepEvidence = TournamentStepEvidence {
  challenger: "not_run",
  incumbent: "not_run",
  allowance_ns: 0,
  decisive_wins: 0,
  challenger_selected: false,
  winner: "not_run",
};

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // The serializer may project a subset of complete selector evidence.
pub(crate) struct InstantProbeEvidence {
  pub(crate) eligibility: &'static str,
  pub(crate) permission_basis: &'static str,
  pub(crate) pr_get_tsc_status: i64,
  pub(crate) kernel_version_known: bool,
  pub(crate) kernel_version_major: u32,
  pub(crate) kernel_version_minor: u32,
  pub(crate) candidate_count: usize,
  pub(crate) vdso_available: bool,
  pub(crate) vdso_raw_available: bool,
  pub(crate) vdso_boottime_available: bool,
  pub(crate) reads_per_batch: u64,
  pub(crate) cntvct_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) cntvct_median_ns: u64,
  pub(crate) clock_median_ns: u64,
  pub(crate) clock_raw_median_ns: u64,
  pub(crate) clock_boottime_median_ns: u64,
  pub(crate) syscall_median_ns: u64,
  pub(crate) syscall_raw_median_ns: u64,
  pub(crate) syscall_boottime_median_ns: u64,
  pub(crate) vdso_median_ns: u64,
  pub(crate) vdso_raw_median_ns: u64,
  pub(crate) vdso_boottime_median_ns: u64,
  pub(crate) fallback_provider: &'static str,
  pub(crate) direct_allowance_ns: u64,
  pub(crate) direct_decisive_wins: usize,
  pub(crate) syscall_vs_clock_allowance_ns: u64,
  pub(crate) syscall_vs_clock_decisive_wins: usize,
  pub(crate) tournament_step_count: usize,
  pub(crate) tournament_steps: [TournamentStepEvidence; MAX_INSTANT_TOURNAMENT_STEPS],
  pub(crate) required_decisive_wins: usize,
  pub(crate) selected_provider: &'static str,
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // The serializer may project a subset of complete selector evidence.
pub(crate) struct OrderedProbeEvidence {
  pub(crate) eligibility: &'static str,
  pub(crate) permission_basis: &'static str,
  pub(crate) pr_get_tsc_status: i64,
  pub(crate) kernel_version_known: bool,
  pub(crate) kernel_version_major: u32,
  pub(crate) kernel_version_minor: u32,
  pub(crate) candidate_count: usize,
  pub(crate) vdso_available: bool,
  pub(crate) vdso_raw_available: bool,
  pub(crate) vdso_boottime_available: bool,
  pub(crate) reads_per_batch: u64,
  pub(crate) hwcap_ecv: bool,
  pub(crate) hwcap_sb: bool,
  pub(crate) sb_eligibility: &'static str,
  pub(crate) isb_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) cntvctss_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) clock_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) syscall_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_raw_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) vdso_boottime_batches_ns: [u64; PROBE_BATCHES],
  pub(crate) direct_provider: &'static str,
  pub(crate) fallback_provider: &'static str,
  pub(crate) direct_median_ns: u64,
  pub(crate) clock_median_ns: u64,
  pub(crate) clock_raw_median_ns: u64,
  pub(crate) clock_boottime_median_ns: u64,
  pub(crate) syscall_median_ns: u64,
  pub(crate) syscall_raw_median_ns: u64,
  pub(crate) syscall_boottime_median_ns: u64,
  pub(crate) vdso_median_ns: u64,
  pub(crate) vdso_raw_median_ns: u64,
  pub(crate) vdso_boottime_median_ns: u64,
  pub(crate) direct_allowance_ns: u64,
  pub(crate) direct_decisive_wins: usize,
  pub(crate) cntvctss_vs_isb_allowance_ns: u64,
  pub(crate) cntvctss_vs_isb_decisive_wins: usize,
  pub(crate) syscall_vs_clock_allowance_ns: u64,
  pub(crate) syscall_vs_clock_decisive_wins: usize,
  pub(crate) tournament_step_count: usize,
  pub(crate) tournament_steps: [TournamentStepEvidence; MAX_ORDERED_TOURNAMENT_STEPS],
  pub(crate) required_decisive_wins: usize,
  pub(crate) selected_provider: &'static str,
}

#[cfg(feature = "bench-internal")]
struct InstantEvidenceCell(UnsafeCell<MaybeUninit<InstantProbeEvidence>>);

#[cfg(feature = "bench-internal")]
struct OrderedEvidenceCell(UnsafeCell<MaybeUninit<OrderedProbeEvidence>>);

// SAFETY: only a process-selection owner writes each cell before publishing
// the corresponding provider state with Release. Readers observe that state
// with Acquire. A fork child that inherits SELECTING writes its private COW
// copy only after recovering ownership.
#[cfg(feature = "bench-internal")]
unsafe impl Sync for InstantEvidenceCell {}

// SAFETY: identical publication protocol to InstantEvidenceCell.
#[cfg(feature = "bench-internal")]
unsafe impl Sync for OrderedEvidenceCell {}

#[cfg(feature = "bench-internal")]
static INSTANT_EVIDENCE: InstantEvidenceCell =
  InstantEvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));

#[cfg(feature = "bench-internal")]
static ORDERED_EVIDENCE: OrderedEvidenceCell =
  OrderedEvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  match INSTANT_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_CNTVCT => super::aarch64::cntvct(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => ticks_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_after_selection() -> u64 {
  match selected_instant_provider() {
    PROVIDER_CNTVCT => super::aarch64::cntvct(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => clock_monotonic_raw_vdso(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  read_hot_ordered_provider(ORDERED_PROVIDER.load(Ordering::Relaxed))
}

#[inline(always)]
fn read_hot_ordered_provider(provider: u8) -> u64 {
  match provider {
    PROVIDER_ISB_CNTVCT | HOT_PROVIDER_ISB_CNTVCT_IDENTITY => super::aarch64::cntvct_after_isb(),
    PROVIDER_CNTVCTSS | HOT_PROVIDER_CNTVCTSS_IDENTITY => super::aarch64::cntvctss(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso_ordered(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered(),
    _ => ticks_ordered_after_selection(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn ticks_ordered_with_scale() -> (u64, u64) {
  let state = ORDERED_HOT_STATE.load(Ordering::Relaxed);
  match state as u8 {
    HOT_PROVIDER_ISB_CNTVCT_IDENTITY => {
      (super::aarch64::cntvct_after_isb(), IDENTITY_NANOS_PER_TICK_Q32)
    }
    HOT_PROVIDER_CNTVCTSS_IDENTITY => (super::aarch64::cntvctss(), IDENTITY_NANOS_PER_TICK_Q32),
    _ => ticks_ordered_with_scale_fallback(state),
  }
}

#[inline(never)]
fn ticks_ordered_with_scale_fallback(state: u64) -> (u64, u64) {
  (read_hot_ordered_provider(state as u8), state >> 8)
}

pub(crate) const fn ordered_hot_scale_fits(scale: u64) -> bool {
  scale <= MAX_ORDERED_HOT_SCALE
}

pub(crate) fn update_ordered_hot_scale(scale: u64) {
  let state = ORDERED_HOT_STATE.load(Ordering::Relaxed);
  let provider = match state as u8 {
    HOT_PROVIDER_ISB_CNTVCT_IDENTITY => PROVIDER_ISB_CNTVCT,
    HOT_PROVIDER_CNTVCTSS_IDENTITY => PROVIDER_CNTVCTSS,
    provider => provider,
  };
  publish_ordered_hot_state(provider, scale);
}

fn publish_ordered_hot_state(provider: u8, scale: u64) {
  debug_assert!(ordered_hot_scale_fits(scale));
  let hot_provider = match (provider, scale) {
    (PROVIDER_ISB_CNTVCT, IDENTITY_NANOS_PER_TICK_Q32) => HOT_PROVIDER_ISB_CNTVCT_IDENTITY,
    (PROVIDER_CNTVCTSS, IDENTITY_NANOS_PER_TICK_Q32) => HOT_PROVIDER_CNTVCTSS_IDENTITY,
    _ => provider,
  };
  ORDERED_HOT_STATE.store(scale << 8 | u64::from(hot_provider), Ordering::Release);
}

#[cold]
#[inline(never)]
fn ticks_ordered_after_selection() -> u64 {
  match selected_ordered_provider() {
    PROVIDER_ISB_CNTVCT => super::aarch64::cntvct_after_isb(),
    PROVIDER_CNTVCTSS => super::aarch64::cntvctss(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered(),
    _ => clock_monotonic_raw_vdso_ordered(),
  }
}

/// Read an endpoint in OrderedInstant's selected numeric domain without a
/// preceding happens-before barrier.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  match ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_ISB_CNTVCT | PROVIDER_CNTVCTSS => super::aarch64::cntvct(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => clock_monotonic_raw_vdso(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => ticks_ordered_unordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_unordered_after_selection() -> u64 {
  match selected_ordered_provider() {
    PROVIDER_ISB_CNTVCT | PROVIDER_CNTVCTSS => super::aarch64::cntvct(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => clock_monotonic_raw_vdso(),
  }
}

#[inline(always)]
fn clock_monotonic_ordered() -> u64 {
  // Linux's arm64 vDSO reads the architectural timer through its ordered
  // accessor (`isb; cntvct` or CNTVCTSS). When that mode is disabled, libc
  // enters the kernel instead. The opaque call is also a compiler barrier.
  clock_monotonic()
}

#[inline(always)]
fn clock_monotonic() -> u64 {
  clock_gettime_libc_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| clock_gettime_syscall_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_syscall() -> u64 {
  clock_gettime_syscall_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| clock_gettime_libc_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_raw() -> u64 {
  clock_gettime_libc_nanos(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| clock_gettime_syscall_nanos(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_raw_ordered() -> u64 {
  // The opaque libc call is a compiler boundary. Linux's arm64 vDSO timer
  // accessor is ordered with surrounding work; its syscall fallback is a
  // context-synchronizing exception.
  clock_monotonic_raw()
}

#[inline(always)]
fn clock_monotonic_raw_syscall() -> u64 {
  clock_gettime_syscall_nanos(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| clock_gettime_libc_nanos(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_boottime() -> u64 {
  clock_gettime_libc_nanos(libc::CLOCK_BOOTTIME)
    .or_else(|| clock_gettime_syscall_nanos(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_boottime_ordered() -> u64 {
  // libc may use the arm64 vDSO's ordered counter accessor or enter through
  // the context-synchronizing syscall exception.
  clock_boottime()
}

#[inline(always)]
fn clock_boottime_syscall() -> u64 {
  clock_gettime_syscall_nanos(libc::CLOCK_BOOTTIME)
    .or_else(|| clock_gettime_libc_nanos(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_vdso() -> u64 {
  super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| clock_gettime_libc_nanos(libc::CLOCK_MONOTONIC))
    .or_else(|| clock_gettime_syscall_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_vdso_ordered() -> u64 {
  // The arm64 vDSO's kernel-owned accessor uses the ordered architectural
  // counter primitive; its syscall fallback is context-synchronizing.
  clock_monotonic_vdso()
}

#[inline(always)]
fn clock_monotonic_raw_vdso() -> u64 {
  super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC_RAW)
    .or_else(|| clock_gettime_libc_nanos(libc::CLOCK_MONOTONIC_RAW))
    .or_else(|| clock_gettime_syscall_nanos(libc::CLOCK_MONOTONIC_RAW))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_monotonic_raw_vdso_ordered() -> u64 {
  clock_monotonic_raw_vdso()
}

#[inline(always)]
fn clock_boottime_vdso() -> u64 {
  super::linux_vdso::clock_nanos(libc::CLOCK_BOOTTIME)
    .or_else(|| clock_gettime_libc_nanos(libc::CLOCK_BOOTTIME))
    .or_else(|| clock_gettime_syscall_nanos(libc::CLOCK_BOOTTIME))
    .unwrap_or(0)
}

#[inline(always)]
fn clock_boottime_vdso_ordered() -> u64 {
  clock_boottime_vdso()
}

#[inline(always)]
fn clock_gettime_libc_nanos(clock_id: libc::clockid_t) -> Option<u64> {
  use core::mem::MaybeUninit;

  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: the caller passes a Linux clock ID and value is writable
  // timespec storage. A failed read stays in the chosen clock's numeric
  // domain; it never silently substitutes the other clock ID.
  if unsafe { libc::clock_gettime(clock_id, value.as_mut_ptr()) } != 0 {
    return None;
  }
  // SAFETY: clock_gettime initialized value after returning success.
  timespec_to_nanos(unsafe { value.assume_init() })
}

#[inline(always)]
fn clock_gettime_syscall_nanos(clock_id: libc::clockid_t) -> Option<u64> {
  use core::mem::MaybeUninit;

  let mut value = MaybeUninit::<libc::timespec>::uninit();
  let mut status = libc::c_long::from(clock_id);
  // SAFETY: this is the Linux aarch64 syscall ABI. Callers pass a valid
  // clock ID, value is writable timespec storage, and omitting `nomem` gives the
  // compiler ordering needed by OrderedInstant and the syscall's memory
  // effects their required boundary.
  unsafe {
    core::arch::asm!(
      "svc 0",
      inlateout("x0") status,
      in("x1") value.as_mut_ptr(),
      in("x8") libc::SYS_clock_gettime,
      options(nostack),
    );
  }
  if status != 0 {
    return None;
  }
  // SAFETY: the successful syscall initialized value.
  timespec_to_nanos(unsafe { value.assume_init() })
}

#[inline(always)]
fn timespec_to_nanos(value: libc::timespec) -> Option<u64> {
  let Ok(seconds) = u64::try_from(value.tv_sec) else {
    return None;
  };
  let Ok(nanos) = u32::try_from(value.tv_nsec) else {
    return None;
  };
  if nanos >= 1_000_000_000 {
    return None;
  }
  seconds
    .checked_mul(1_000_000_000)
    .and_then(|base| base.checked_add(u64::from(nanos)))
}

#[inline]
pub(crate) fn instant_frequency() -> u64 {
  if instant_uses_cntvct() { direct_frequency() } else { 1_000_000_000 }
}

#[inline]
pub(crate) fn ordered_frequency() -> u64 {
  if ordered_uses_cntvct() { direct_frequency() } else { 1_000_000_000 }
}

#[inline]
pub(crate) fn instant_uses_cntvct() -> bool {
  selected_instant_provider() == PROVIDER_CNTVCT
}

#[inline]
pub(crate) fn instant_read_cost() -> crate::ThreadCpuReadCost {
  instant_read_cost_for(selected_instant_provider())
}

const fn instant_read_cost_for(provider: u8) -> crate::ThreadCpuReadCost {
  match provider {
    PROVIDER_CNTVCT
    | PROVIDER_CLOCK_MONOTONIC_VDSO
    | PROVIDER_CLOCK_MONOTONIC_RAW_VDSO
    | PROVIDER_CLOCK_BOOTTIME_VDSO => crate::ThreadCpuReadCost::Inline,
    // The libc ABI may use the vDSO or enter the kernel. Without resolving and
    // owning a userspace implementation, the conservative class is a system
    // call even when runtime measurements make this route the fastest.
    _ => crate::ThreadCpuReadCost::SystemCall,
  }
}

#[cfg(test)]
#[test]
fn instant_read_cost_only_marks_guaranteed_userspace_paths_inline() {
  assert_eq!(instant_read_cost_for(PROVIDER_CNTVCT), crate::ThreadCpuReadCost::Inline);
  assert_eq!(instant_read_cost_for(PROVIDER_CLOCK_BOOTTIME_VDSO), crate::ThreadCpuReadCost::Inline,);
  assert_eq!(instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC), crate::ThreadCpuReadCost::SystemCall,);
  assert_eq!(
    instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC_RAW),
    crate::ThreadCpuReadCost::SystemCall,
  );
  assert_eq!(
    instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC_SYSCALL),
    crate::ThreadCpuReadCost::SystemCall,
  );
}

#[inline]
pub(crate) fn ordered_uses_cntvct() -> bool {
  matches!(selected_ordered_provider(), PROVIDER_ISB_CNTVCT | PROVIDER_CNTVCTSS)
}

#[inline]
fn direct_frequency() -> u64 {
  let cached = DIRECT_FREQUENCY.load(Ordering::Acquire);
  if cached != 0 {
    return cached;
  }

  #[cfg(target_os = "linux")]
  let measured = crate::calibration::calibrate_frequency_with(super::aarch64::cntvct);
  #[cfg(target_os = "android")]
  let measured = super::aarch64::cntfrq();
  let measured = measured.max(1);
  match DIRECT_FREQUENCY.compare_exchange(0, measured, Ordering::AcqRel, Ordering::Acquire) {
    Ok(_) => measured,
    Err(winner) => winner,
  }
}

fn selected_instant_provider() -> u8 {
  super::select_thread_owned_process_provider(
    &INSTANT_PROVIDER,
    PROVIDER_UNKNOWN,
    PROVIDER_SELECTING,
    &INSTANT_PROVIDER_OWNER_PID,
    &INSTANT_PROVIDER_OWNER_TID,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL,
    detect_instant_provider,
  )
}

fn selected_ordered_provider() -> u8 {
  let provider = super::select_thread_owned_process_provider(
    &ORDERED_PROVIDER,
    PROVIDER_UNKNOWN,
    PROVIDER_SELECTING,
    &ORDERED_PROVIDER_OWNER_PID,
    &ORDERED_PROVIDER_OWNER_TID,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL,
    detect_ordered_provider,
  );
  if ORDERED_PROVIDER.load(Ordering::Acquire) == provider {
    let frequency = if matches!(provider, PROVIDER_ISB_CNTVCT | PROVIDER_CNTVCTSS) {
      direct_frequency()
    } else {
      1_000_000_000
    };
    let scale = super::scale_from_ratio(1_000_000_000, frequency);
    assert!(ordered_hot_scale_fits(scale), "tach: selected ordered aarch64 scale is not packable");
    super::ORDERED_NANOS_PER_TICK_Q32.store(scale, Ordering::Release);
    publish_ordered_hot_state(provider, scale);
  }
  provider
}

#[cold]
#[inline(never)]
fn detect_instant_provider() -> u8 {
  let assessment = counter_assessment();
  if !assessment.counter_eligible() {
    let candidates = instant_candidates(false, false, false, false);
    let samples = measure_instant_hot_paths(candidates.as_slice());
    let mut winner =
      RankedCandidate { provider: PROVIDER_CLOCK_MONOTONIC_SYSCALL, samples: samples.syscall };
    let _raw_syscall_stage = compete(&mut winner, RankedCandidate {
      provider: PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL,
      samples: samples.syscall_raw,
    });
    let _boottime_syscall_stage = compete(&mut winner, RankedCandidate {
      provider: PROVIDER_CLOCK_BOOTTIME_SYSCALL,
      samples: samples.syscall_boottime,
    });
    #[cfg(feature = "bench-internal")]
    store_instant_evidence(restricted_instant_evidence(
      assessment,
      samples,
      winner.provider,
      _raw_syscall_stage,
      _boottime_syscall_stage,
    ));
    return winner.provider;
  }

  let (vdso_available, vdso_raw_available, vdso_boottime_available) = direct_vdso_availability();
  let candidates =
    instant_candidates(true, vdso_available, vdso_raw_available, vdso_boottime_available);
  let samples = measure_instant_hot_paths(candidates.as_slice());
  let mut winner = RankedCandidate { provider: PROVIDER_CLOCK_MONOTONIC, samples: samples.clock };
  let _raw_clock_stage = compete(&mut winner, RankedCandidate {
    provider: PROVIDER_CLOCK_MONOTONIC_RAW,
    samples: samples.clock_raw,
  });
  let _boottime_clock_stage = compete(&mut winner, RankedCandidate {
    provider: PROVIDER_CLOCK_BOOTTIME,
    samples: samples.clock_boottime,
  });
  let _vdso_stage = vdso_available.then(|| {
    compete(&mut winner, RankedCandidate {
      provider: PROVIDER_CLOCK_MONOTONIC_VDSO,
      samples: samples.vdso,
    })
  });
  let _vdso_raw_stage = vdso_raw_available.then(|| {
    compete(&mut winner, RankedCandidate {
      provider: PROVIDER_CLOCK_MONOTONIC_RAW_VDSO,
      samples: samples.vdso_raw,
    })
  });
  let _vdso_boottime_stage = vdso_boottime_available.then(|| {
    compete(&mut winner, RankedCandidate {
      provider: PROVIDER_CLOCK_BOOTTIME_VDSO,
      samples: samples.vdso_boottime,
    })
  });
  let _syscall_stage = compete(&mut winner, RankedCandidate {
    provider: PROVIDER_CLOCK_MONOTONIC_SYSCALL,
    samples: samples.syscall,
  });
  let _raw_syscall_stage = compete(&mut winner, RankedCandidate {
    provider: PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL,
    samples: samples.syscall_raw,
  });
  let _boottime_syscall_stage = compete(&mut winner, RankedCandidate {
    provider: PROVIDER_CLOCK_BOOTTIME_SYSCALL,
    samples: samples.syscall_boottime,
  });
  let _fallback_provider = winner.provider;
  let _direct_stage =
    compete(&mut winner, RankedCandidate { provider: PROVIDER_CNTVCT, samples: samples.cntvct });
  let _syscall_vs_clock = evaluate_challenger(samples.syscall, samples.clock);
  let provider = winner.provider;
  #[cfg(feature = "bench-internal")]
  let (tournament_steps, tournament_step_count) = {
    let mut steps = [EMPTY_TOURNAMENT_STEP; MAX_INSTANT_TOURNAMENT_STEPS];
    let mut count = 0;
    steps[count] = stage_evidence(_raw_clock_stage);
    count += 1;
    steps[count] = stage_evidence(_boottime_clock_stage);
    count += 1;
    if let Some(stage) = _vdso_stage {
      steps[count] = stage_evidence(stage);
      count += 1;
    }
    if let Some(stage) = _vdso_raw_stage {
      steps[count] = stage_evidence(stage);
      count += 1;
    }
    if let Some(stage) = _vdso_boottime_stage {
      steps[count] = stage_evidence(stage);
      count += 1;
    }
    steps[count] = stage_evidence(_syscall_stage);
    count += 1;
    steps[count] = stage_evidence(_raw_syscall_stage);
    count += 1;
    steps[count] = stage_evidence(_boottime_syscall_stage);
    count += 1;
    steps[count] = stage_evidence(_direct_stage);
    count += 1;
    (steps, count)
  };
  #[cfg(feature = "bench-internal")]
  store_instant_evidence(measured_instant_evidence(
    assessment,
    samples,
    _fallback_provider,
    _direct_stage.decision,
    _syscall_vs_clock,
    candidates.count,
    vdso_available,
    vdso_raw_available,
    vdso_boottime_available,
    tournament_step_count,
    tournament_steps,
    provider,
  ));
  provider
}

#[cold]
#[inline(never)]
fn detect_ordered_provider() -> u8 {
  let assessment = counter_assessment();
  #[cfg(feature = "bench-internal")]
  let hwcap_sb = sb_capable();
  if !assessment.counter_eligible() {
    let candidates = ordered_candidates(false, false, false, false, false);
    let samples = measure_ordered_hot_paths(candidates.as_slice());
    let mut winner =
      RankedCandidate { provider: PROVIDER_CLOCK_MONOTONIC_SYSCALL, samples: samples.syscall };
    let _raw_syscall_stage = compete(&mut winner, RankedCandidate {
      provider: PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL,
      samples: samples.syscall_raw,
    });
    let _boottime_syscall_stage = compete(&mut winner, RankedCandidate {
      provider: PROVIDER_CLOCK_BOOTTIME_SYSCALL,
      samples: samples.syscall_boottime,
    });
    #[cfg(feature = "bench-internal")]
    store_ordered_evidence(restricted_ordered_evidence(
      assessment,
      hwcap_sb,
      samples,
      winner.provider,
      _raw_syscall_stage,
      _boottime_syscall_stage,
    ));
    return winner.provider;
  }

  let hwcap_ecv = super::aarch64::cntvctss_capable();
  let (vdso_available, vdso_raw_available, vdso_boottime_available) = direct_vdso_availability();
  let candidates = ordered_candidates(
    true,
    hwcap_ecv,
    vdso_available,
    vdso_raw_available,
    vdso_boottime_available,
  );
  let samples = measure_ordered_hot_paths(candidates.as_slice());

  let mut fallback = RankedCandidate { provider: PROVIDER_CLOCK_MONOTONIC, samples: samples.clock };
  let _raw_clock_stage = compete(&mut fallback, RankedCandidate {
    provider: PROVIDER_CLOCK_MONOTONIC_RAW,
    samples: samples.clock_raw,
  });
  let _boottime_clock_stage = compete(&mut fallback, RankedCandidate {
    provider: PROVIDER_CLOCK_BOOTTIME,
    samples: samples.clock_boottime,
  });
  let _syscall_stage = compete(&mut fallback, RankedCandidate {
    provider: PROVIDER_CLOCK_MONOTONIC_SYSCALL,
    samples: samples.syscall,
  });
  let _raw_syscall_stage = compete(&mut fallback, RankedCandidate {
    provider: PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL,
    samples: samples.syscall_raw,
  });
  let _boottime_syscall_stage = compete(&mut fallback, RankedCandidate {
    provider: PROVIDER_CLOCK_BOOTTIME_SYSCALL,
    samples: samples.syscall_boottime,
  });
  let _vdso_stage = vdso_available.then(|| {
    compete(&mut fallback, RankedCandidate {
      provider: PROVIDER_CLOCK_MONOTONIC_VDSO,
      samples: samples.vdso,
    })
  });
  let _vdso_raw_stage = vdso_raw_available.then(|| {
    compete(&mut fallback, RankedCandidate {
      provider: PROVIDER_CLOCK_MONOTONIC_RAW_VDSO,
      samples: samples.vdso_raw,
    })
  });
  let _vdso_boottime_stage = vdso_boottime_available.then(|| {
    compete(&mut fallback, RankedCandidate {
      provider: PROVIDER_CLOCK_BOOTTIME_VDSO,
      samples: samples.vdso_boottime,
    })
  });
  let _fallback_provider = fallback.provider;

  let mut direct = RankedCandidate { provider: PROVIDER_ISB_CNTVCT, samples: samples.isb };
  let cntvctss_stage = if hwcap_ecv {
    Some(compete(&mut direct, RankedCandidate {
      provider: PROVIDER_CNTVCTSS,
      samples: samples.cntvctss,
    }))
  } else {
    None
  };
  let _direct_provider = direct.provider;
  let _final_stage = compete(&mut fallback, direct);
  let _syscall_vs_clock = evaluate_challenger(samples.syscall, samples.clock);
  let _cntvctss_vs_isb = cntvctss_stage.map_or_else(no_challenge_decision, |stage| stage.decision);
  let provider = fallback.provider;
  #[cfg(feature = "bench-internal")]
  store_ordered_evidence(measured_ordered_evidence(
    assessment,
    hwcap_ecv,
    hwcap_sb,
    samples,
    _direct_provider,
    _fallback_provider,
    _final_stage.decision,
    _cntvctss_vs_isb,
    _syscall_vs_clock,
    _raw_clock_stage,
    _boottime_clock_stage,
    _syscall_stage,
    _raw_syscall_stage,
    _boottime_syscall_stage,
    _vdso_stage,
    _vdso_raw_stage,
    _vdso_boottime_stage,
    cntvctss_stage,
    _final_stage,
    provider,
  ));
  provider
}

fn counter_assessment() -> CounterAssessment {
  let mut mode: libc::c_int = 0;
  let status = pr_get_tsc(&mut mode);
  let kernel_version = if status == 0 { None } else { running_kernel_version() };
  classify_counter_access(status, mode, kernel_version)
}

fn direct_vdso_availability() -> (bool, bool, bool) {
  if !super::linux_vdso::install() {
    return (false, false, false);
  }
  (
    super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC).is_some(),
    super::linux_vdso::clock_nanos(libc::CLOCK_MONOTONIC_RAW).is_some(),
    super::linux_vdso::clock_nanos(libc::CLOCK_BOOTTIME).is_some(),
  )
}

/// Whether the current thread may execute architectural counter reads under
/// the same kernel permission proof used by the wall-clock selector.
#[cfg(feature = "thread-cpu-inline")]
pub(crate) fn counter_user_read_eligible() -> bool {
  counter_assessment().counter_eligible()
}

fn classify_counter_access(
  status: libc::c_long,
  mode: libc::c_int,
  kernel_version: Option<KernelVersion>,
) -> CounterAssessment {
  if status == 0 {
    if mode == PR_TSC_ENABLE {
      return CounterAssessment {
        eligibility: CounterEligibility::Eligible,
        basis: PermissionBasis::PrGetTscEnabled,
        pr_get_tsc_status: status,
        kernel_version,
      };
    }
    return CounterAssessment {
      eligibility: CounterEligibility::CounterReadDisabled,
      basis: if mode == PR_TSC_SIGSEGV {
        PermissionBasis::PrGetTscNotEnabled
      } else {
        PermissionBasis::PrGetTscUnknownMode
      },
      pr_get_tsc_status: status,
      kernel_version,
    };
  }

  // Before Linux 6.12, upstream arm64 had no PR_SET_TSC control: direct
  // counter access was either native or a kernel-emulated erratum trap. The
  // exact -EINVAL response plus a real pre-6.12 arm64 release therefore proves
  // that direct and vDSO reads cannot be disabled with this prctl. A vendor
  // backport is detected above because its successful PR_GET result wins over
  // the base release. Other failures may be seccomp or an ABI anomaly and stay
  // opaque, even on an old release.
  if status == -libc::c_long::from(libc::EINVAL)
    && kernel_version.is_some_and(KernelVersion::predates_arm64_pr_get_tsc)
  {
    return CounterAssessment {
      eligibility: CounterEligibility::Eligible,
      basis: PermissionBasis::LegacyKernelWithoutPrGetTsc,
      pr_get_tsc_status: status,
      kernel_version,
    };
  }

  let basis = match kernel_version {
    None => PermissionBasis::KernelReleaseUnknown,
    Some(version) if !version.predates_arm64_pr_get_tsc() => {
      PermissionBasis::NewKernelWithoutObservablePrGetTsc
    }
    Some(_) => PermissionBasis::PrGetTscFailed,
  };
  CounterAssessment {
    eligibility: CounterEligibility::PrGetTscUnavailable,
    basis,
    pr_get_tsc_status: status,
    kernel_version,
  }
}

fn pr_get_tsc(mode: &mut libc::c_int) -> libc::c_long {
  let mut status = libc::c_long::from(PR_GET_TSC);
  // SAFETY: this is the Linux aarch64 prctl syscall ABI. PR_GET_TSC writes one
  // c_int through x1; the remaining option arguments are zero. The raw return
  // value preserves -EINVAL so unsupported legacy kernels can be distinguished
  // from policy failures without depending on libc's target-specific errno API.
  unsafe {
    core::arch::asm!(
      "svc 0",
      inlateout("x0") status,
      in("x1") mode as *mut libc::c_int,
      in("x2") 0_usize,
      in("x3") 0_usize,
      in("x4") 0_usize,
      in("x8") libc::SYS_prctl,
      options(nostack),
    );
  }
  status
}

fn running_kernel_version() -> Option<KernelVersion> {
  let mut name = core::mem::MaybeUninit::<libc::utsname>::uninit();
  // SAFETY: name points to writable utsname storage. uname initializes the
  // complete value on success.
  if unsafe { libc::uname(name.as_mut_ptr()) } != 0 {
    return None;
  }
  // SAFETY: uname returned success and initialized name.
  let name = unsafe { name.assume_init() };
  parse_kernel_release(&name.release)
}

fn parse_kernel_release<const N: usize>(release: &[libc::c_char; N]) -> Option<KernelVersion> {
  let mut index = 0;
  let major = parse_release_component(release, &mut index)?;
  if index >= N || release[index] as u8 != b'.' {
    return None;
  }
  index += 1;
  let minor = parse_release_component(release, &mut index)?;
  if index >= N || !matches!(release[index] as u8, 0 | b'.' | b'-' | b'+') {
    return None;
  }
  Some(KernelVersion { major, minor })
}

fn parse_release_component<const N: usize>(
  release: &[libc::c_char; N],
  index: &mut usize,
) -> Option<u32> {
  let start = *index;
  let mut value = 0_u32;
  while *index < N {
    let byte = release[*index] as u8;
    if !byte.is_ascii_digit() {
      break;
    }
    value = value.checked_mul(10)?.checked_add(u32::from(byte - b'0'))?;
    *index += 1;
  }
  (*index != start).then_some(value)
}

#[cfg(feature = "bench-internal")]
fn sb_capable() -> bool {
  // SAFETY: getauxval has no pointer arguments and AT_HWCAP is part of the
  // Linux userspace ABI.
  unsafe { libc::getauxval(libc::AT_HWCAP) & HWCAP_SB != 0 }
}

#[inline(always)]
fn probe_instant_ticks() -> u64 {
  match PROBE_INSTANT_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_CNTVCT => super::aarch64::cntvct(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso(),
    _ => clock_monotonic_raw_vdso(),
  }
}

#[inline(always)]
fn probe_ordered_ticks() -> u64 {
  match PROBE_ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_ISB_CNTVCT => super::aarch64::cntvct_after_isb(),
    PROVIDER_CNTVCTSS => super::aarch64::cntvctss(),
    PROVIDER_CLOCK_MONOTONIC => clock_monotonic_ordered(),
    PROVIDER_CLOCK_MONOTONIC_RAW => clock_monotonic_raw_ordered(),
    PROVIDER_CLOCK_BOOTTIME => clock_boottime_ordered(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => clock_monotonic_raw_syscall(),
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => clock_boottime_syscall(),
    PROVIDER_CLOCK_MONOTONIC_VDSO => clock_monotonic_vdso_ordered(),
    PROVIDER_CLOCK_BOOTTIME_VDSO => clock_boottime_vdso_ordered(),
    _ => clock_monotonic_raw_vdso_ordered(),
  }
}

type Samples = [u64; PROBE_BATCHES];

#[derive(Clone, Copy)]
struct InstantSamples {
  cntvct: Samples,
  clock: Samples,
  clock_raw: Samples,
  clock_boottime: Samples,
  syscall: Samples,
  syscall_raw: Samples,
  syscall_boottime: Samples,
  vdso: Samples,
  vdso_raw: Samples,
  vdso_boottime: Samples,
}

impl InstantSamples {
  const fn empty() -> Self {
    Self {
      cntvct: [0; PROBE_BATCHES],
      clock: [0; PROBE_BATCHES],
      clock_raw: [0; PROBE_BATCHES],
      clock_boottime: [0; PROBE_BATCHES],
      syscall: [0; PROBE_BATCHES],
      syscall_raw: [0; PROBE_BATCHES],
      syscall_boottime: [0; PROBE_BATCHES],
      vdso: [0; PROBE_BATCHES],
      vdso_raw: [0; PROBE_BATCHES],
      vdso_boottime: [0; PROBE_BATCHES],
    }
  }

  fn set(&mut self, provider: u8, sample: usize, value: u64) {
    match provider {
      PROVIDER_CNTVCT => self.cntvct[sample] = value,
      PROVIDER_CLOCK_MONOTONIC => self.clock[sample] = value,
      PROVIDER_CLOCK_MONOTONIC_RAW => self.clock_raw[sample] = value,
      PROVIDER_CLOCK_BOOTTIME => self.clock_boottime[sample] = value,
      PROVIDER_CLOCK_MONOTONIC_SYSCALL => self.syscall[sample] = value,
      PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => self.syscall_raw[sample] = value,
      PROVIDER_CLOCK_BOOTTIME_SYSCALL => self.syscall_boottime[sample] = value,
      PROVIDER_CLOCK_MONOTONIC_VDSO => self.vdso[sample] = value,
      PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => self.vdso_raw[sample] = value,
      PROVIDER_CLOCK_BOOTTIME_VDSO => self.vdso_boottime[sample] = value,
      _ => unreachable!("invalid Instant provider"),
    }
  }
}

#[derive(Clone, Copy)]
struct OrderedSamples {
  isb: Samples,
  cntvctss: Samples,
  clock: Samples,
  clock_raw: Samples,
  clock_boottime: Samples,
  syscall: Samples,
  syscall_raw: Samples,
  syscall_boottime: Samples,
  vdso: Samples,
  vdso_raw: Samples,
  vdso_boottime: Samples,
}

impl OrderedSamples {
  const fn empty() -> Self {
    Self {
      isb: [0; PROBE_BATCHES],
      cntvctss: [0; PROBE_BATCHES],
      clock: [0; PROBE_BATCHES],
      clock_raw: [0; PROBE_BATCHES],
      clock_boottime: [0; PROBE_BATCHES],
      syscall: [0; PROBE_BATCHES],
      syscall_raw: [0; PROBE_BATCHES],
      syscall_boottime: [0; PROBE_BATCHES],
      vdso: [0; PROBE_BATCHES],
      vdso_raw: [0; PROBE_BATCHES],
      vdso_boottime: [0; PROBE_BATCHES],
    }
  }

  fn set(&mut self, provider: u8, sample: usize, value: u64) {
    match provider {
      PROVIDER_ISB_CNTVCT => self.isb[sample] = value,
      PROVIDER_CNTVCTSS => self.cntvctss[sample] = value,
      PROVIDER_CLOCK_MONOTONIC => self.clock[sample] = value,
      PROVIDER_CLOCK_MONOTONIC_RAW => self.clock_raw[sample] = value,
      PROVIDER_CLOCK_BOOTTIME => self.clock_boottime[sample] = value,
      PROVIDER_CLOCK_MONOTONIC_SYSCALL => self.syscall[sample] = value,
      PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => self.syscall_raw[sample] = value,
      PROVIDER_CLOCK_BOOTTIME_SYSCALL => self.syscall_boottime[sample] = value,
      PROVIDER_CLOCK_MONOTONIC_VDSO => self.vdso[sample] = value,
      PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => self.vdso_raw[sample] = value,
      PROVIDER_CLOCK_BOOTTIME_VDSO => self.vdso_boottime[sample] = value,
      _ => unreachable!("invalid OrderedInstant provider"),
    }
  }
}

fn measure_instant_hot_paths(providers: &[u8]) -> InstantSamples {
  for &provider in providers {
    PROBE_INSTANT_PROVIDER.store(provider, Ordering::Relaxed);
    for _ in 0..PROBE_WARMUP_READS {
      black_box(probe_instant_ticks());
    }
  }

  let mut samples = InstantSamples::empty();
  for sample in 0..PROBE_BATCHES {
    for offset in 0..providers.len() {
      let provider = providers[(sample + offset) % providers.len()];
      samples.set(provider, sample, measure_instant_batch(provider));
    }
  }
  samples
}

fn measure_ordered_hot_paths(providers: &[u8]) -> OrderedSamples {
  for &provider in providers {
    PROBE_ORDERED_PROVIDER.store(provider, Ordering::Relaxed);
    for _ in 0..PROBE_WARMUP_READS {
      black_box(probe_ordered_ticks());
    }
  }

  let mut samples = OrderedSamples::empty();
  for sample in 0..PROBE_BATCHES {
    for offset in 0..providers.len() {
      let provider = providers[(sample + offset) % providers.len()];
      samples.set(provider, sample, measure_ordered_batch(provider));
    }
  }
  samples
}

#[inline(never)]
fn measure_instant_batch(provider: u8) -> u64 {
  PROBE_INSTANT_PROVIDER.store(provider, Ordering::Relaxed);
  // The stopwatch must remain safe while counter access is denied. The two
  // explicit syscalls are the only candidates in that state, so no vDSO read
  // may occur even outside the measured loop.
  let start = clock_monotonic_syscall();
  let mut sink = 0_u64;
  for _ in 0..PROBE_READS {
    sink ^= probe_instant_ticks();
  }
  let elapsed = clock_monotonic_syscall().saturating_sub(start);
  black_box(sink);
  elapsed
}

#[inline(never)]
fn measure_ordered_batch(provider: u8) -> u64 {
  PROBE_ORDERED_PROVIDER.store(provider, Ordering::Relaxed);
  let start = clock_monotonic_syscall();
  let mut sink = 0_u64;
  for _ in 0..PROBE_READS {
    sink ^= probe_ordered_ticks();
  }
  let elapsed = clock_monotonic_syscall().saturating_sub(start);
  black_box(sink);
  elapsed
}

fn evaluate_challenger(challenger: Samples, incumbent: Samples) -> SelectionDecision {
  let challenger_median = median(challenger);
  let incumbent_median = median(incumbent);
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

fn median(mut values: Samples) -> u64 {
  values.sort_unstable();
  values[values.len() / 2]
}

#[cfg(feature = "bench-internal")]
fn restricted_instant_evidence(
  assessment: CounterAssessment,
  samples: InstantSamples,
  provider: u8,
  raw_syscall_stage: TournamentStage,
  boottime_syscall_stage: TournamentStage,
) -> InstantProbeEvidence {
  let kernel_version = assessment.kernel_version.unwrap_or(KernelVersion { major: 0, minor: 0 });
  let mut tournament_steps = [EMPTY_TOURNAMENT_STEP; MAX_INSTANT_TOURNAMENT_STEPS];
  tournament_steps[0] = stage_evidence(raw_syscall_stage);
  tournament_steps[1] = stage_evidence(boottime_syscall_stage);
  InstantProbeEvidence {
    eligibility: assessment.eligibility.name(),
    permission_basis: assessment.basis.name(),
    pr_get_tsc_status: assessment.pr_get_tsc_status,
    kernel_version_known: assessment.kernel_version.is_some(),
    kernel_version_major: kernel_version.major,
    kernel_version_minor: kernel_version.minor,
    candidate_count: 3,
    vdso_available: false,
    vdso_raw_available: false,
    vdso_boottime_available: false,
    reads_per_batch: PROBE_READS,
    cntvct_batches_ns: [0; PROBE_BATCHES],
    clock_batches_ns: [0; PROBE_BATCHES],
    clock_raw_batches_ns: [0; PROBE_BATCHES],
    clock_boottime_batches_ns: [0; PROBE_BATCHES],
    syscall_batches_ns: samples.syscall,
    syscall_raw_batches_ns: samples.syscall_raw,
    syscall_boottime_batches_ns: samples.syscall_boottime,
    vdso_batches_ns: [0; PROBE_BATCHES],
    vdso_raw_batches_ns: [0; PROBE_BATCHES],
    vdso_boottime_batches_ns: [0; PROBE_BATCHES],
    cntvct_median_ns: 0,
    clock_median_ns: 0,
    clock_raw_median_ns: 0,
    clock_boottime_median_ns: 0,
    syscall_median_ns: median(samples.syscall),
    syscall_raw_median_ns: median(samples.syscall_raw),
    syscall_boottime_median_ns: median(samples.syscall_boottime),
    vdso_median_ns: 0,
    vdso_raw_median_ns: 0,
    vdso_boottime_median_ns: 0,
    fallback_provider: provider_name(provider),
    direct_allowance_ns: 0,
    direct_decisive_wins: 0,
    syscall_vs_clock_allowance_ns: 0,
    syscall_vs_clock_decisive_wins: 0,
    tournament_step_count: 2,
    tournament_steps,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    selected_provider: provider_name(provider),
  }
}

#[cfg(feature = "bench-internal")]
fn measured_instant_evidence(
  assessment: CounterAssessment,
  samples: InstantSamples,
  fallback_provider: u8,
  direct_vs_fallback: SelectionDecision,
  syscall_vs_clock: SelectionDecision,
  candidate_count: usize,
  vdso_available: bool,
  vdso_raw_available: bool,
  vdso_boottime_available: bool,
  tournament_step_count: usize,
  tournament_steps: [TournamentStepEvidence; MAX_INSTANT_TOURNAMENT_STEPS],
  provider: u8,
) -> InstantProbeEvidence {
  let kernel_version = assessment.kernel_version.unwrap_or(KernelVersion { major: 0, minor: 0 });
  InstantProbeEvidence {
    eligibility: assessment.eligibility.name(),
    permission_basis: assessment.basis.name(),
    pr_get_tsc_status: assessment.pr_get_tsc_status,
    kernel_version_known: assessment.kernel_version.is_some(),
    kernel_version_major: kernel_version.major,
    kernel_version_minor: kernel_version.minor,
    candidate_count,
    vdso_available,
    vdso_raw_available,
    vdso_boottime_available,
    reads_per_batch: PROBE_READS,
    cntvct_batches_ns: samples.cntvct,
    clock_batches_ns: samples.clock,
    clock_raw_batches_ns: samples.clock_raw,
    clock_boottime_batches_ns: samples.clock_boottime,
    syscall_batches_ns: samples.syscall,
    syscall_raw_batches_ns: samples.syscall_raw,
    syscall_boottime_batches_ns: samples.syscall_boottime,
    vdso_batches_ns: samples.vdso,
    vdso_raw_batches_ns: samples.vdso_raw,
    vdso_boottime_batches_ns: samples.vdso_boottime,
    cntvct_median_ns: median(samples.cntvct),
    clock_median_ns: median(samples.clock),
    clock_raw_median_ns: median(samples.clock_raw),
    clock_boottime_median_ns: median(samples.clock_boottime),
    syscall_median_ns: median(samples.syscall),
    syscall_raw_median_ns: median(samples.syscall_raw),
    syscall_boottime_median_ns: median(samples.syscall_boottime),
    vdso_median_ns: median(samples.vdso),
    vdso_raw_median_ns: median(samples.vdso_raw),
    vdso_boottime_median_ns: median(samples.vdso_boottime),
    fallback_provider: provider_name(fallback_provider),
    direct_allowance_ns: direct_vs_fallback.allowance,
    direct_decisive_wins: direct_vs_fallback.decisive_wins,
    syscall_vs_clock_allowance_ns: syscall_vs_clock.allowance,
    syscall_vs_clock_decisive_wins: syscall_vs_clock.decisive_wins,
    tournament_step_count,
    tournament_steps,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    selected_provider: provider_name(provider),
  }
}

#[cfg(feature = "bench-internal")]
fn restricted_ordered_evidence(
  assessment: CounterAssessment,
  hwcap_sb: bool,
  samples: OrderedSamples,
  provider: u8,
  raw_syscall_stage: TournamentStage,
  boottime_syscall_stage: TournamentStage,
) -> OrderedProbeEvidence {
  let kernel_version = assessment.kernel_version.unwrap_or(KernelVersion { major: 0, minor: 0 });
  let mut tournament_steps = [EMPTY_TOURNAMENT_STEP; MAX_ORDERED_TOURNAMENT_STEPS];
  tournament_steps[0] = stage_evidence(raw_syscall_stage);
  tournament_steps[1] = stage_evidence(boottime_syscall_stage);
  OrderedProbeEvidence {
    eligibility: assessment.eligibility.name(),
    permission_basis: assessment.basis.name(),
    pr_get_tsc_status: assessment.pr_get_tsc_status,
    kernel_version_known: assessment.kernel_version.is_some(),
    kernel_version_major: kernel_version.major,
    kernel_version_minor: kernel_version.minor,
    candidate_count: 3,
    vdso_available: false,
    vdso_raw_available: false,
    vdso_boottime_available: false,
    reads_per_batch: PROBE_READS,
    hwcap_ecv: false,
    hwcap_sb,
    sb_eligibility: "ineligible: SB does not order architectural counter sampling",
    isb_batches_ns: [0; PROBE_BATCHES],
    cntvctss_batches_ns: [0; PROBE_BATCHES],
    clock_batches_ns: [0; PROBE_BATCHES],
    clock_raw_batches_ns: [0; PROBE_BATCHES],
    clock_boottime_batches_ns: [0; PROBE_BATCHES],
    syscall_batches_ns: samples.syscall,
    syscall_raw_batches_ns: samples.syscall_raw,
    syscall_boottime_batches_ns: samples.syscall_boottime,
    vdso_batches_ns: [0; PROBE_BATCHES],
    vdso_raw_batches_ns: [0; PROBE_BATCHES],
    vdso_boottime_batches_ns: [0; PROBE_BATCHES],
    direct_provider: provider_name(PROVIDER_ISB_CNTVCT),
    fallback_provider: provider_name(provider),
    direct_median_ns: 0,
    clock_median_ns: 0,
    clock_raw_median_ns: 0,
    clock_boottime_median_ns: 0,
    syscall_median_ns: median(samples.syscall),
    syscall_raw_median_ns: median(samples.syscall_raw),
    syscall_boottime_median_ns: median(samples.syscall_boottime),
    vdso_median_ns: 0,
    vdso_raw_median_ns: 0,
    vdso_boottime_median_ns: 0,
    direct_allowance_ns: 0,
    direct_decisive_wins: 0,
    cntvctss_vs_isb_allowance_ns: 0,
    cntvctss_vs_isb_decisive_wins: 0,
    syscall_vs_clock_allowance_ns: 0,
    syscall_vs_clock_decisive_wins: 0,
    tournament_step_count: 2,
    tournament_steps,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    selected_provider: provider_name(provider),
  }
}

#[cfg(feature = "bench-internal")]
#[allow(clippy::too_many_arguments)]
fn measured_ordered_evidence(
  assessment: CounterAssessment,
  hwcap_ecv: bool,
  hwcap_sb: bool,
  samples: OrderedSamples,
  direct_provider: u8,
  fallback_provider: u8,
  direct_vs_fallback: SelectionDecision,
  cntvctss_vs_isb: SelectionDecision,
  syscall_vs_clock: SelectionDecision,
  raw_clock_stage: TournamentStage,
  boottime_clock_stage: TournamentStage,
  syscall_stage: TournamentStage,
  raw_syscall_stage: TournamentStage,
  boottime_syscall_stage: TournamentStage,
  vdso_stage: Option<TournamentStage>,
  vdso_raw_stage: Option<TournamentStage>,
  vdso_boottime_stage: Option<TournamentStage>,
  cntvctss_stage: Option<TournamentStage>,
  final_stage: TournamentStage,
  provider: u8,
) -> OrderedProbeEvidence {
  let kernel_version = assessment.kernel_version.unwrap_or(KernelVersion { major: 0, minor: 0 });
  let mut tournament_steps = [EMPTY_TOURNAMENT_STEP; MAX_ORDERED_TOURNAMENT_STEPS];
  tournament_steps[0] = stage_evidence(raw_clock_stage);
  tournament_steps[1] = stage_evidence(boottime_clock_stage);
  tournament_steps[2] = stage_evidence(syscall_stage);
  tournament_steps[3] = stage_evidence(raw_syscall_stage);
  tournament_steps[4] = stage_evidence(boottime_syscall_stage);
  let mut tournament_step_count = 5;
  if let Some(stage) = vdso_stage {
    tournament_steps[tournament_step_count] = stage_evidence(stage);
    tournament_step_count += 1;
  }
  if let Some(stage) = vdso_raw_stage {
    tournament_steps[tournament_step_count] = stage_evidence(stage);
    tournament_step_count += 1;
  }
  if let Some(stage) = vdso_boottime_stage {
    tournament_steps[tournament_step_count] = stage_evidence(stage);
    tournament_step_count += 1;
  }
  if let Some(stage) = cntvctss_stage {
    tournament_steps[tournament_step_count] = stage_evidence(stage);
    tournament_step_count += 1;
  }
  tournament_steps[tournament_step_count] = stage_evidence(final_stage);
  tournament_step_count += 1;
  OrderedProbeEvidence {
    eligibility: assessment.eligibility.name(),
    permission_basis: assessment.basis.name(),
    pr_get_tsc_status: assessment.pr_get_tsc_status,
    kernel_version_known: assessment.kernel_version.is_some(),
    kernel_version_major: kernel_version.major,
    kernel_version_minor: kernel_version.minor,
    candidate_count: tournament_step_count + 1,
    vdso_available: vdso_stage.is_some(),
    vdso_raw_available: vdso_raw_stage.is_some(),
    vdso_boottime_available: vdso_boottime_stage.is_some(),
    reads_per_batch: PROBE_READS,
    hwcap_ecv,
    hwcap_sb,
    sb_eligibility: "ineligible: SB does not order architectural counter sampling",
    isb_batches_ns: samples.isb,
    cntvctss_batches_ns: samples.cntvctss,
    clock_batches_ns: samples.clock,
    clock_raw_batches_ns: samples.clock_raw,
    clock_boottime_batches_ns: samples.clock_boottime,
    syscall_batches_ns: samples.syscall,
    syscall_raw_batches_ns: samples.syscall_raw,
    syscall_boottime_batches_ns: samples.syscall_boottime,
    vdso_batches_ns: samples.vdso,
    vdso_raw_batches_ns: samples.vdso_raw,
    vdso_boottime_batches_ns: samples.vdso_boottime,
    direct_provider: provider_name(direct_provider),
    fallback_provider: provider_name(fallback_provider),
    direct_median_ns: median(if direct_provider == PROVIDER_CNTVCTSS {
      samples.cntvctss
    } else {
      samples.isb
    }),
    clock_median_ns: median(samples.clock),
    clock_raw_median_ns: median(samples.clock_raw),
    clock_boottime_median_ns: median(samples.clock_boottime),
    syscall_median_ns: median(samples.syscall),
    syscall_raw_median_ns: median(samples.syscall_raw),
    syscall_boottime_median_ns: median(samples.syscall_boottime),
    vdso_median_ns: median(samples.vdso),
    vdso_raw_median_ns: median(samples.vdso_raw),
    vdso_boottime_median_ns: median(samples.vdso_boottime),
    direct_allowance_ns: direct_vs_fallback.allowance,
    direct_decisive_wins: direct_vs_fallback.decisive_wins,
    cntvctss_vs_isb_allowance_ns: cntvctss_vs_isb.allowance,
    cntvctss_vs_isb_decisive_wins: cntvctss_vs_isb.decisive_wins,
    syscall_vs_clock_allowance_ns: syscall_vs_clock.allowance,
    syscall_vs_clock_decisive_wins: syscall_vs_clock.decisive_wins,
    tournament_step_count,
    tournament_steps,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    selected_provider: provider_name(provider),
  }
}

#[cfg(feature = "bench-internal")]
fn store_instant_evidence(evidence: InstantProbeEvidence) {
  // SAFETY: only the process-selection owner writes before publishing the
  // provider state.
  unsafe { (*INSTANT_EVIDENCE.0.get()).write(evidence) };
}

#[cfg(feature = "bench-internal")]
fn store_ordered_evidence(evidence: OrderedProbeEvidence) {
  // SAFETY: only the process-selection owner writes before publishing the
  // provider state.
  unsafe { (*ORDERED_EVIDENCE.0.get()).write(evidence) };
}

#[cfg(feature = "bench-internal")]
const fn provider_name(provider: u8) -> &'static str {
  match provider {
    PROVIDER_CNTVCT => "aarch64_cntvct",
    PROVIDER_ISB_CNTVCT => "aarch64_isb_cntvct",
    PROVIDER_CNTVCTSS => "aarch64_cntvctss",
    PROVIDER_CLOCK_MONOTONIC => "linux_clock_monotonic",
    PROVIDER_CLOCK_MONOTONIC_RAW => "linux_clock_monotonic_raw",
    PROVIDER_CLOCK_BOOTTIME => "linux_clock_boottime",
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => "linux_clock_monotonic_syscall",
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => "linux_clock_monotonic_raw_syscall",
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => "linux_clock_boottime_syscall",
    PROVIDER_CLOCK_MONOTONIC_VDSO => "linux_clock_monotonic_vdso_direct",
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => "linux_clock_monotonic_raw_vdso_direct",
    PROVIDER_CLOCK_BOOTTIME_VDSO => "linux_clock_boottime_vdso_direct",
    _ => "unavailable",
  }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_provider() -> &'static str {
  provider_name(selected_instant_provider())
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_ordered_provider() -> &'static str {
  provider_name(selected_ordered_provider())
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
  let frequency = if matches!(provider, PROVIDER_CNTVCT | PROVIDER_ISB_CNTVCT | PROVIDER_CNTVCTSS) {
    direct_frequency()
  } else {
    1_000_000_000
  };
  crate::arch::scale_from_ratio(1_000_000_000, frequency)
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness selects once before its timing loop.
pub(crate) fn bench_selected_instant_primitive() -> BenchPrimitive {
  instant_bench_primitive(selected_instant_provider())
}

#[cfg(feature = "bench-internal")]
fn instant_bench_primitive(provider: u8) -> BenchPrimitive {
  let read = match provider {
    PROVIDER_CNTVCT => bench_direct_cntvct as fn() -> u64,
    PROVIDER_CLOCK_MONOTONIC => bench_direct_clock_monotonic,
    PROVIDER_CLOCK_MONOTONIC_RAW => bench_direct_clock_monotonic_raw,
    PROVIDER_CLOCK_BOOTTIME => bench_direct_clock_boottime,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => bench_direct_clock_monotonic_syscall,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => bench_direct_clock_monotonic_raw_syscall,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => bench_direct_clock_boottime_syscall,
    PROVIDER_CLOCK_MONOTONIC_VDSO => bench_direct_clock_monotonic_vdso,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => bench_direct_clock_monotonic_raw_vdso,
    PROVIDER_CLOCK_BOOTTIME_VDSO => bench_direct_clock_boottime_vdso,
    _ => bench_direct_clock_monotonic,
  };
  BenchPrimitive {
    name: provider_name(provider),
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
    PROVIDER_ISB_CNTVCT => bench_direct_isb_cntvct as fn() -> u64,
    PROVIDER_CNTVCTSS => bench_direct_cntvctss,
    PROVIDER_CLOCK_MONOTONIC => bench_direct_clock_monotonic_ordered,
    PROVIDER_CLOCK_MONOTONIC_RAW => bench_direct_clock_monotonic_raw_ordered,
    PROVIDER_CLOCK_BOOTTIME => bench_direct_clock_boottime_ordered,
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => bench_direct_clock_monotonic_syscall,
    PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL => bench_direct_clock_monotonic_raw_syscall,
    PROVIDER_CLOCK_BOOTTIME_SYSCALL => bench_direct_clock_boottime_syscall,
    PROVIDER_CLOCK_MONOTONIC_VDSO => bench_direct_clock_monotonic_vdso_ordered,
    PROVIDER_CLOCK_MONOTONIC_RAW_VDSO => bench_direct_clock_monotonic_raw_vdso_ordered,
    PROVIDER_CLOCK_BOOTTIME_VDSO => bench_direct_clock_boottime_vdso_ordered,
    _ => bench_direct_clock_monotonic_ordered,
  };
  BenchPrimitive {
    name: provider_name(provider),
    read,
    nanos_per_tick_q32: bench_nanos_per_tick_q32(provider),
  }
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The Criterion serializer consumes the complete fixed array.
pub(crate) fn bench_instant_candidate_primitives()
-> ([Option<BenchPrimitive>; MAX_INSTANT_CANDIDATES], usize) {
  let evidence = bench_instant_evidence();
  let candidates = instant_candidates(
    evidence.eligibility == CounterEligibility::Eligible.name(),
    evidence.vdso_available,
    evidence.vdso_raw_available,
    evidence.vdso_boottime_available,
  );
  debug_assert_eq!(evidence.candidate_count, candidates.count);
  let mut primitives = [None; MAX_INSTANT_CANDIDATES];
  for (slot, provider) in primitives.iter_mut().zip(candidates.as_slice()) {
    *slot = Some(instant_bench_primitive(*provider));
  }
  (primitives, candidates.count)
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The Criterion serializer consumes the complete fixed array.
pub(crate) fn bench_ordered_candidate_primitives()
-> ([Option<BenchPrimitive>; MAX_ORDERED_CANDIDATES], usize) {
  let evidence = bench_ordered_evidence();
  let candidates = ordered_candidates(
    evidence.eligibility == CounterEligibility::Eligible.name(),
    evidence.hwcap_ecv,
    evidence.vdso_available,
    evidence.vdso_raw_available,
    evidence.vdso_boottime_available,
  );
  debug_assert_eq!(evidence.candidate_count, candidates.count);
  let mut primitives = [None; MAX_ORDERED_CANDIDATES];
  for (slot, provider) in primitives.iter_mut().zip(candidates.as_slice()) {
    *slot = Some(ordered_bench_primitive(*provider));
  }
  (primitives, candidates.count)
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_counter_eligible() -> bool {
  counter_assessment().counter_eligible()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_cntvct() -> u64 {
  super::aarch64::cntvct()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_isb_cntvct() -> u64 {
  super::aarch64::cntvct_after_isb()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_cntvctss() -> u64 {
  super::aarch64::cntvctss()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic() -> u64 {
  clock_monotonic()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_ordered() -> u64 {
  clock_monotonic_ordered()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_raw() -> u64 {
  clock_monotonic_raw()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_raw_ordered() -> u64 {
  clock_monotonic_raw_ordered()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_boottime() -> u64 {
  clock_boottime()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_boottime_ordered() -> u64 {
  clock_boottime_ordered()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_syscall() -> u64 {
  clock_monotonic_syscall()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_raw_syscall() -> u64 {
  clock_monotonic_raw_syscall()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_boottime_syscall() -> u64 {
  clock_boottime_syscall()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_vdso() -> u64 {
  clock_monotonic_vdso()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_vdso_ordered() -> u64 {
  clock_monotonic_vdso_ordered()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_raw_vdso() -> u64 {
  clock_monotonic_raw_vdso()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_monotonic_raw_vdso_ordered() -> u64 {
  clock_monotonic_raw_vdso_ordered()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_boottime_vdso() -> u64 {
  clock_boottime_vdso()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
pub(crate) fn bench_direct_clock_boottime_vdso_ordered() -> u64 {
  clock_boottime_vdso_ordered()
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_evidence() -> InstantProbeEvidence {
  let _ = selected_instant_provider();
  // SAFETY: selected_instant_provider returns after the Copy value was
  // written and the provider state was published with Release.
  unsafe { (*INSTANT_EVIDENCE.0.get()).assume_init_read() }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_ordered_evidence() -> OrderedProbeEvidence {
  let _ = selected_ordered_provider();
  // SAFETY: selected_ordered_provider returns after the Copy value was
  // written and the provider state was published with Release.
  unsafe { (*ORDERED_EVIDENCE.0.get()).assume_init_read() }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn release(value: &str) -> [libc::c_char; 65] {
    let mut release = [0; 65];
    for (slot, byte) in release.iter_mut().zip(value.bytes()) {
      *slot = byte as libc::c_char;
    }
    release
  }

  #[test]
  fn parses_real_kernel_release_prefixes_strictly() {
    assert_eq!(
      parse_kernel_release(&release("6.1.147-android14-11-gki")),
      Some(KernelVersion { major: 6, minor: 1 })
    );
    assert_eq!(
      parse_kernel_release(&release("6.12.0-1024-aws")),
      Some(KernelVersion { major: 6, minor: 12 })
    );
    assert_eq!(
      parse_kernel_release(&release("5.15.0+vendor")),
      Some(KernelVersion { major: 5, minor: 15 })
    );
    assert_eq!(parse_kernel_release(&release("6")), None);
    assert_eq!(parse_kernel_release(&release("6.x")), None);
    assert_eq!(parse_kernel_release(&release("6.1vendor")), None);
    assert_eq!(parse_kernel_release(&release("linux-6.1")), None);
    assert_eq!(parse_kernel_release(&release("42949672960.1")), None);
  }

  #[test]
  fn legacy_inference_requires_exact_unsupported_status_and_pre_6_12_release() {
    let unsupported = -libc::c_long::from(libc::EINVAL);
    for version in
      [KernelVersion { major: 3, minor: 7 }, KernelVersion { major: 5, minor: 15 }, KernelVersion {
        major: 6,
        minor: 11,
      }]
    {
      let assessment = classify_counter_access(unsupported, 0, Some(version));
      assert_eq!(assessment.eligibility, CounterEligibility::Eligible);
      assert_eq!(assessment.basis, PermissionBasis::LegacyKernelWithoutPrGetTsc);
    }

    for version in [
      // A modern kernel can report 2.6 under the UNAME26 personality.
      KernelVersion { major: 2, minor: 6 },
      KernelVersion { major: 6, minor: 12 },
      KernelVersion { major: 7, minor: 0 },
    ] {
      assert!(!classify_counter_access(unsupported, 0, Some(version)).counter_eligible());
    }
    assert!(!classify_counter_access(unsupported, 0, None).counter_eligible());
    assert!(
      !classify_counter_access(
        -libc::c_long::from(libc::EPERM),
        0,
        Some(KernelVersion { major: 6, minor: 1 })
      )
      .counter_eligible()
    );
  }

  #[test]
  fn pr_get_tsc_result_overrides_android_or_vendor_base_release() {
    let backported_release = Some(KernelVersion { major: 6, minor: 1 });
    let enabled = classify_counter_access(0, PR_TSC_ENABLE, backported_release);
    assert_eq!(enabled.eligibility, CounterEligibility::Eligible);
    assert_eq!(enabled.basis, PermissionBasis::PrGetTscEnabled);

    let disabled = classify_counter_access(0, PR_TSC_SIGSEGV, backported_release);
    assert_eq!(disabled.eligibility, CounterEligibility::CounterReadDisabled);
    assert_eq!(disabled.basis, PermissionBasis::PrGetTscNotEnabled);

    let unknown_mode = classify_counter_access(0, 99, backported_release);
    assert_eq!(unknown_mode.eligibility, CounterEligibility::CounterReadDisabled);
    assert_eq!(unknown_mode.basis, PermissionBasis::PrGetTscUnknownMode);
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
  fn candidate_sets_are_complete_unique_and_permission_safe() {
    let instant = instant_candidates(true, true, true, true);
    assert_eq!(instant.as_slice(), [
      PROVIDER_CLOCK_MONOTONIC,
      PROVIDER_CLOCK_MONOTONIC_RAW,
      PROVIDER_CLOCK_BOOTTIME,
      PROVIDER_CLOCK_MONOTONIC_VDSO,
      PROVIDER_CLOCK_MONOTONIC_RAW_VDSO,
      PROVIDER_CLOCK_BOOTTIME_VDSO,
      PROVIDER_CLOCK_MONOTONIC_SYSCALL,
      PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL,
      PROVIDER_CLOCK_BOOTTIME_SYSCALL,
      PROVIDER_CNTVCT,
    ]);
    let ordered = ordered_candidates(true, true, true, true, true);
    assert_eq!(ordered.as_slice(), [
      PROVIDER_CLOCK_MONOTONIC,
      PROVIDER_CLOCK_MONOTONIC_RAW,
      PROVIDER_CLOCK_BOOTTIME,
      PROVIDER_CLOCK_MONOTONIC_SYSCALL,
      PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL,
      PROVIDER_CLOCK_BOOTTIME_SYSCALL,
      PROVIDER_CLOCK_MONOTONIC_VDSO,
      PROVIDER_CLOCK_MONOTONIC_RAW_VDSO,
      PROVIDER_CLOCK_BOOTTIME_VDSO,
      PROVIDER_ISB_CNTVCT,
      PROVIDER_CNTVCTSS,
    ]);
    let denied = instant_candidates(false, false, false, false);
    assert_eq!(denied.as_slice(), [
      PROVIDER_CLOCK_MONOTONIC_SYSCALL,
      PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL,
      PROVIDER_CLOCK_BOOTTIME_SYSCALL,
    ]);

    for candidates in [instant.as_slice(), ordered.as_slice(), denied.as_slice()] {
      for (index, provider) in candidates.iter().enumerate() {
        assert!(!candidates[index + 1..].contains(provider));
      }
    }
    assert!(denied.as_slice().iter().all(|provider| matches!(
      *provider,
      PROVIDER_CLOCK_MONOTONIC_SYSCALL
        | PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL
        | PROVIDER_CLOCK_BOOTTIME_SYSCALL
    )));
  }

  #[test]
  fn tournament_order_is_deterministic_and_ties_keep_the_incumbent() {
    let mut winner =
      RankedCandidate { provider: PROVIDER_CLOCK_MONOTONIC, samples: [100_000; PROBE_BATCHES] };
    let tie = compete(&mut winner, RankedCandidate {
      provider: PROVIDER_CLOCK_MONOTONIC_RAW,
      samples: [96_000; PROBE_BATCHES],
    });
    assert!(!tie.decision.challenger_selected);
    assert_eq!(winner.provider, PROVIDER_CLOCK_MONOTONIC);

    let win = compete(&mut winner, RankedCandidate {
      provider: PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL,
      samples: [90_000; PROBE_BATCHES],
    });
    assert!(win.decision.challenger_selected);
    assert_eq!(winner.provider, PROVIDER_CLOCK_MONOTONIC_RAW_SYSCALL);
  }

  #[test]
  fn selected_providers_survive_fork() {
    let instant_before = ticks();
    let ordered_before = ticks_ordered();
    // SAFETY: the child executes only tach reads and `_exit`; the parent
    // immediately waits for that exact child.
    let child = unsafe { libc::fork() };
    assert!(child >= 0);
    if child == 0 {
      let instant_after = ticks();
      let ordered_after = ticks_ordered();
      let ok = instant_after >= instant_before && ordered_after >= ordered_before;
      // SAFETY: `_exit` terminates without inherited Rust cleanup.
      unsafe { libc::_exit(if ok { 0 } else { 1 }) };
    }

    let mut status = 0;
    // SAFETY: child is live and status is writable wait storage.
    assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
    assert_eq!(status, 0);
  }

  #[test]
  fn initial_sigsegv_mode_never_executes_a_counter_read() {
    // SAFETY: the child changes only its own thread permission and private COW
    // selector state, then exits without invoking inherited Rust cleanup.
    let child = unsafe { libc::fork() };
    assert!(child >= 0);
    if child == 0 {
      INSTANT_PROVIDER.store(PROVIDER_UNKNOWN, Ordering::Release);
      ORDERED_PROVIDER.store(PROVIDER_UNKNOWN, Ordering::Release);
      let status = unsafe { libc::prctl(PR_SET_TSC, PR_TSC_SIGSEGV) };
      if status != 0 {
        // Upstream arm64 did not implement PR_SET_TSC before Linux 6.12.
        unsafe { libc::_exit(77) };
      }
      let _ = ticks();
      let _ = ticks_ordered();
      let denied = instant_candidates(false, false, false, false);
      let ok = denied.as_slice().contains(&INSTANT_PROVIDER.load(Ordering::Acquire))
        && denied.as_slice().contains(&ORDERED_PROVIDER.load(Ordering::Acquire));
      unsafe { libc::_exit(if ok { 0 } else { 1 }) };
    }

    let mut status = 0;
    assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
    if libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 77 {
      return;
    }
    assert_eq!(status, 0);
  }
}
