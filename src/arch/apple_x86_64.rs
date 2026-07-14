//! Intel macOS wall-clock provider selection.
//!
//! `Instant` and `OrderedInstant` select independently. The local timer may
//! use a bare invariant TSC, matching the contract and eligibility gate used
//! by the fastest established same-thread comparator. The ordered timer stays
//! on XNU's Mach absolute-time domain and may inline the commpage seqlock plus
//! `lfence; rdtsc; lfence` protocol. Each selector measures its complete
//! steady-state dispatch and retains the system function unless the direct
//! path wins materially and repeatably.
//!
//! This closes the precise monotonic XNU set rather than sampling two names
//! opportunistically. On x86_64, `mach_continuous_time` calls
//! `mach_absolute_time` inside a retry loop and adds two continuous-time-base
//! loads, so it is a strict superset of an eligible candidate here.
//! `mach_approximate_time` and the FAST/COARSE clocks deliberately trade
//! precision for cost and are therefore a different timing contract. Darwin
//! `clock_gettime` wrappers ultimately add call/protocol work around these
//! same kernel-owned timelines.

use core::arch::asm;
use core::arch::x86_64::{__cpuid, _rdtsc};
#[cfg(feature = "bench-internal")]
use core::cell::UnsafeCell;
use core::hint::black_box;
use core::hint::spin_loop;
#[cfg(feature = "bench-internal")]
use core::mem::MaybeUninit;
#[cfg(feature = "bench-internal")]
use core::sync::atomic::AtomicBool;
use core::sync::atomic::{AtomicI32, AtomicU8, AtomicU64, AtomicUsize, Ordering};

const ORDERED_PROVIDER_UNKNOWN: u8 = 0;
const ORDERED_PROVIDER_SELECTING: u8 = 1;
const ORDERED_PROVIDER_MACH_ABSOLUTE_TIME: u8 = 2;
const ORDERED_PROVIDER_COMMPAGE: u8 = 3;

const INSTANT_PROVIDER_UNKNOWN: usize = 0;
const INSTANT_PROVIDER_MACH_ABSOLUTE_TIME: usize = 1;
const INSTANT_PROVIDER_TSC: usize = 2;
const INSTANT_SELECTING_TAG: usize = 1 << (usize::BITS - 1);

const COMM_PAGE_BASE: usize = 0x0000_7fff_ffe0_0000;
const COMM_PAGE_LENGTH: usize = 4096;
const COMM_PAGE_TIME_DATA_START: usize = COMM_PAGE_BASE + 0x50;
const NT_TSC_BASE: usize = 0;
const NT_SCALE: usize = 8;
const NT_SHIFT: usize = 12;
const NT_NS_BASE: usize = 16;
const NT_GENERATION: usize = 24;

const PROBE_BATCHES: usize = 9;
const PROBE_READS: u64 = 4096;
const PROBE_WARMUP_READS: u64 = 1024;
const REQUIRED_DECISIVE_WINS: usize = 8;

static INSTANT_PROVIDER: AtomicUsize = AtomicUsize::new(INSTANT_PROVIDER_UNKNOWN);
static INSTANT_SELECTING_PID: AtomicI32 = AtomicI32::new(0);
static INSTANT_PROBE_PROVIDER: AtomicUsize = AtomicUsize::new(INSTANT_PROVIDER_MACH_ABSOLUTE_TIME);
static INSTANT_TSC_NANOS_PER_TICK_Q32: AtomicU64 = AtomicU64::new(0);

static ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(ORDERED_PROVIDER_UNKNOWN);
static ORDERED_SELECTING_PID: AtomicI32 = AtomicI32::new(0);
// Selection measures this dispatcher rather than bare helpers so the decision
// includes the atomic load and branch paid by every post-initialization read.
static ORDERED_PROBE_PROVIDER: AtomicU8 = AtomicU8::new(ORDERED_PROVIDER_MACH_ABSOLUTE_TIME);

#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(not(feature = "bench-internal"), allow(dead_code))]
struct SelectionDecision {
  allowance: u64,
  decisive_wins: usize,
  challenger_selected: bool,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(feature = "bench-internal"), allow(dead_code))]
struct CommpageEligibility {
  eligible: bool,
  basis: &'static str,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(feature = "bench-internal"), allow(dead_code))]
struct TscEligibility {
  eligible: bool,
  translated: bool,
  basis: &'static str,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(feature = "bench-internal"), allow(dead_code))]
struct InstantSelectionRun {
  eligibility: TscEligibility,
  mach_batches: [u64; PROBE_BATCHES],
  tsc_batches: [u64; PROBE_BATCHES],
  decision: SelectionDecision,
  measured: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectingDisposition {
  UseMachFallback,
  Wait,
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct WallSelectionEvidence {
  pub(crate) reads_per_batch: u64,
  pub(crate) commpage_eligible: bool,
  pub(crate) commpage_eligibility_basis: &'static str,
  pub(crate) mach_absolute_time_batches_ticks: [u64; PROBE_BATCHES],
  pub(crate) commpage_batches_ticks: [u64; PROBE_BATCHES],
  pub(crate) mach_absolute_time_median_ticks: u64,
  pub(crate) commpage_median_ticks: u64,
  pub(crate) allowance_total_ticks: u64,
  pub(crate) decisive_wins: usize,
  pub(crate) required_decisive_wins: usize,
  pub(crate) commpage_selected: bool,
  pub(crate) selected_provider: &'static str,
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct InstantSelectionEvidence {
  pub(crate) reads_per_batch: u64,
  pub(crate) tsc_eligible: bool,
  pub(crate) tsc_eligibility_basis: &'static str,
  pub(crate) translated: bool,
  pub(crate) mach_absolute_time_batches_ticks: [u64; PROBE_BATCHES],
  pub(crate) tsc_batches_ticks: [u64; PROBE_BATCHES],
  pub(crate) mach_absolute_time_median_ticks: u64,
  pub(crate) tsc_median_ticks: u64,
  pub(crate) allowance_total_ticks: u64,
  pub(crate) decisive_wins: usize,
  pub(crate) required_decisive_wins: usize,
  pub(crate) tsc_selected: bool,
  pub(crate) measured_winner: &'static str,
  pub(crate) selected_provider: &'static str,
}

#[cfg(feature = "bench-internal")]
struct EvidenceCell(UnsafeCell<MaybeUninit<WallSelectionEvidence>>);

// SAFETY: the one selection owner writes the evidence before publishing the
// selected provider with Release. Readers acquire that provider first.
#[cfg(feature = "bench-internal")]
unsafe impl Sync for EvidenceCell {}

#[cfg(feature = "bench-internal")]
static ORDERED_EVIDENCE: EvidenceCell = EvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));

#[cfg(feature = "bench-internal")]
struct InstantEvidenceCell(UnsafeCell<MaybeUninit<InstantSelectionEvidence>>);

#[cfg(feature = "bench-internal")]
unsafe impl Sync for InstantEvidenceCell {}

#[cfg(feature = "bench-internal")]
static INSTANT_EVIDENCE: InstantEvidenceCell =
  InstantEvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));
#[cfg(feature = "bench-internal")]
static INSTANT_EVIDENCE_READY: AtomicBool = AtomicBool::new(false);

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  let provider = INSTANT_PROVIDER.load(Ordering::Relaxed);
  match provider {
    INSTANT_PROVIDER_MACH_ABSOLUTE_TIME => super::fallback::mach_time(),
    INSTANT_PROVIDER_TSC => read_tsc(),
    _ => ticks_after_selection(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  let provider = ORDERED_PROVIDER.load(Ordering::Relaxed);
  match provider {
    ORDERED_PROVIDER_MACH_ABSOLUTE_TIME => super::fallback::mach_time(),
    ORDERED_PROVIDER_COMMPAGE => commpage_nanotime(),
    ORDERED_PROVIDER_SELECTING => {
      if ORDERED_SELECTING_PID.load(Ordering::Relaxed) == process_id() {
        super::fallback::mach_time()
      } else {
        ticks_ordered_after_selection()
      }
    }
    _ => ticks_ordered_after_selection(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  // Both ordered candidates use the same XNU Mach absolute-time domain. The
  // system fallback remains the safe unordered endpoint for that domain.
  super::fallback::mach_time()
}

#[cold]
#[inline(never)]
fn ticks_after_selection() -> u64 {
  match selected_instant_provider() {
    INSTANT_PROVIDER_TSC => read_tsc(),
    _ => super::fallback::mach_time(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_after_selection() -> u64 {
  match selected_ordered_provider() {
    ORDERED_PROVIDER_COMMPAGE => commpage_nanotime(),
    _ => super::fallback::mach_time(),
  }
}

fn selected_instant_provider() -> usize {
  loop {
    let state = INSTANT_PROVIDER.load(Ordering::Acquire);
    if is_instant_provider(state) {
      return state;
    }

    if state == INSTANT_PROVIDER_UNKNOWN {
      let owner = instant_selecting_state(current_thread_token());
      INSTANT_SELECTING_PID.store(process_id(), Ordering::Relaxed);
      if INSTANT_PROVIDER
        .compare_exchange(INSTANT_PROVIDER_UNKNOWN, owner, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
      {
        let run = select_instant_provider();
        let selected = match INSTANT_PROVIDER.compare_exchange(
          owner,
          run.measured,
          Ordering::AcqRel,
          Ordering::Acquire,
        ) {
          Ok(_) => run.measured,
          Err(published) => instant_provider_or_mach(published),
        };
        #[cfg(feature = "bench-internal")]
        store_instant_evidence(&run, selected);
        return selected;
      }
      continue;
    }

    if is_instant_selecting(state) {
      match selecting_disposition(
        state,
        INSTANT_SELECTING_PID.load(Ordering::Relaxed),
        instant_selecting_state(current_thread_token()),
        process_id(),
      ) {
        SelectingDisposition::UseMachFallback => {
          let _ = INSTANT_PROVIDER.compare_exchange(
            state,
            INSTANT_PROVIDER_MACH_ABSOLUTE_TIME,
            Ordering::AcqRel,
            Ordering::Acquire,
          );
        }
        SelectingDisposition::Wait => spin_loop(),
      }
      continue;
    }

    let _ = INSTANT_PROVIDER.compare_exchange(
      state,
      INSTANT_PROVIDER_MACH_ABSOLUTE_TIME,
      Ordering::AcqRel,
      Ordering::Acquire,
    );
  }
}

#[cold]
fn select_instant_provider() -> InstantSelectionRun {
  let eligibility = tsc_eligibility();
  if !eligibility.eligible {
    return InstantSelectionRun {
      eligibility,
      mach_batches: [0; PROBE_BATCHES],
      tsc_batches: [0; PROBE_BATCHES],
      decision: SelectionDecision::default(),
      measured: INSTANT_PROVIDER_MACH_ABSOLUTE_TIME,
    };
  }

  for _ in 0..PROBE_WARMUP_READS {
    black_box(super::fallback::mach_time());
    black_box(read_tsc());
  }

  let mut mach_batches = [0; PROBE_BATCHES];
  let mut tsc_batches = [0; PROBE_BATCHES];
  for batch in 0..PROBE_BATCHES {
    if batch & 1 == 0 {
      mach_batches[batch] = instant_probe_batch(INSTANT_PROVIDER_MACH_ABSOLUTE_TIME);
      tsc_batches[batch] = instant_probe_batch(INSTANT_PROVIDER_TSC);
    } else {
      tsc_batches[batch] = instant_probe_batch(INSTANT_PROVIDER_TSC);
      mach_batches[batch] = instant_probe_batch(INSTANT_PROVIDER_MACH_ABSOLUTE_TIME);
    }
  }

  let decision = evaluate_challenger(tsc_batches, mach_batches);
  let measured = if decision.challenger_selected {
    INSTANT_PROVIDER_TSC
  } else {
    INSTANT_PROVIDER_MACH_ABSOLUTE_TIME
  };
  InstantSelectionRun { eligibility, mach_batches, tsc_batches, decision, measured }
}

#[inline(never)]
fn instant_probe_batch(provider: usize) -> u64 {
  INSTANT_PROBE_PROVIDER.store(provider, Ordering::Relaxed);
  for _ in 0..PROBE_WARMUP_READS {
    black_box(instant_probe_dispatched_ticks());
  }
  let start = super::fallback::mach_time();
  for _ in 0..PROBE_READS {
    black_box(instant_probe_dispatched_ticks());
  }
  super::fallback::mach_time().saturating_sub(start)
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn instant_probe_dispatched_ticks() -> u64 {
  match INSTANT_PROBE_PROVIDER.load(Ordering::Relaxed) {
    INSTANT_PROVIDER_TSC => read_tsc(),
    _ => super::fallback::mach_time(),
  }
}

const fn is_instant_provider(state: usize) -> bool {
  state == INSTANT_PROVIDER_MACH_ABSOLUTE_TIME || state == INSTANT_PROVIDER_TSC
}

const fn is_instant_selecting(state: usize) -> bool {
  state & INSTANT_SELECTING_TAG != 0
}

const fn instant_selecting_state(thread_token: usize) -> usize {
  INSTANT_SELECTING_TAG | (thread_token & !INSTANT_SELECTING_TAG)
}

const fn instant_provider_or_mach(state: usize) -> usize {
  if is_instant_provider(state) { state } else { INSTANT_PROVIDER_MACH_ABSOLUTE_TIME }
}

const fn selecting_disposition(
  state: usize,
  owner_pid: i32,
  current_owner: usize,
  current_pid: i32,
) -> SelectingDisposition {
  if owner_pid != current_pid || state == current_owner {
    SelectingDisposition::UseMachFallback
  } else {
    SelectingDisposition::Wait
  }
}

fn selected_ordered_provider() -> u8 {
  loop {
    let provider = ORDERED_PROVIDER.load(Ordering::Acquire);
    match provider {
      ORDERED_PROVIDER_MACH_ABSOLUTE_TIME | ORDERED_PROVIDER_COMMPAGE => return provider,
      ORDERED_PROVIDER_UNKNOWN => {
        ORDERED_SELECTING_PID.store(process_id(), Ordering::Relaxed);
        if ORDERED_PROVIDER
          .compare_exchange(
            ORDERED_PROVIDER_UNKNOWN,
            ORDERED_PROVIDER_SELECTING,
            Ordering::AcqRel,
            Ordering::Acquire,
          )
          .is_ok()
        {
          let selected = select_ordered_provider();
          ORDERED_PROVIDER.store(selected, Ordering::Release);
          return selected;
        }
      }
      ORDERED_PROVIDER_SELECTING => {
        // A child created while another thread was probing must not inherit a
        // permanently selecting state owned by a thread that no longer exists.
        if ORDERED_SELECTING_PID.load(Ordering::Relaxed) != process_id() {
          let _ = ORDERED_PROVIDER.compare_exchange(
            ORDERED_PROVIDER_SELECTING,
            ORDERED_PROVIDER_UNKNOWN,
            Ordering::AcqRel,
            Ordering::Acquire,
          );
        } else {
          // Both candidates are the Mach absolute-time domain, so a reentrant
          // or concurrent first reader can use the system baseline.
          return ORDERED_PROVIDER_MACH_ABSOLUTE_TIME;
        }
      }
      _ => unreachable!("invalid Intel macOS wall provider"),
    }
  }
}

#[cold]
fn select_ordered_provider() -> u8 {
  let eligibility = commpage_eligibility();
  if !eligibility.eligible {
    #[cfg(feature = "bench-internal")]
    store_ordered_evidence(WallSelectionEvidence {
      reads_per_batch: PROBE_READS,
      commpage_eligible: false,
      commpage_eligibility_basis: eligibility.basis,
      mach_absolute_time_batches_ticks: [0; PROBE_BATCHES],
      commpage_batches_ticks: [0; PROBE_BATCHES],
      mach_absolute_time_median_ticks: 0,
      commpage_median_ticks: 0,
      allowance_total_ticks: 0,
      decisive_wins: 0,
      required_decisive_wins: REQUIRED_DECISIVE_WINS,
      commpage_selected: false,
      selected_provider: ordered_provider_name(ORDERED_PROVIDER_MACH_ABSOLUTE_TIME),
    });
    return ORDERED_PROVIDER_MACH_ABSOLUTE_TIME;
  }

  for _ in 0..PROBE_WARMUP_READS {
    black_box(super::fallback::mach_time());
    black_box(commpage_nanotime());
  }

  let mut mach_batches = [0; PROBE_BATCHES];
  let mut commpage_batches = [0; PROBE_BATCHES];
  for batch in 0..PROBE_BATCHES {
    // Alternate measurement order so a one-directional frequency or thermal
    // drift cannot systematically favor either implementation.
    if batch & 1 == 0 {
      mach_batches[batch] = ordered_probe_batch(ORDERED_PROVIDER_MACH_ABSOLUTE_TIME);
      commpage_batches[batch] = ordered_probe_batch(ORDERED_PROVIDER_COMMPAGE);
    } else {
      commpage_batches[batch] = ordered_probe_batch(ORDERED_PROVIDER_COMMPAGE);
      mach_batches[batch] = ordered_probe_batch(ORDERED_PROVIDER_MACH_ABSOLUTE_TIME);
    }
  }

  let decision = evaluate_challenger(commpage_batches, mach_batches);
  let selected = if decision.challenger_selected {
    ORDERED_PROVIDER_COMMPAGE
  } else {
    ORDERED_PROVIDER_MACH_ABSOLUTE_TIME
  };

  #[cfg(feature = "bench-internal")]
  store_ordered_evidence(WallSelectionEvidence {
    reads_per_batch: PROBE_READS,
    commpage_eligible: true,
    commpage_eligibility_basis: eligibility.basis,
    mach_absolute_time_batches_ticks: mach_batches,
    commpage_batches_ticks: commpage_batches,
    mach_absolute_time_median_ticks: median(mach_batches),
    commpage_median_ticks: median(commpage_batches),
    allowance_total_ticks: decision.allowance,
    decisive_wins: decision.decisive_wins,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    commpage_selected: decision.challenger_selected,
    selected_provider: ordered_provider_name(selected),
  });

  selected
}

#[inline(never)]
fn ordered_probe_batch(provider: u8) -> u64 {
  ORDERED_PROBE_PROVIDER.store(provider, Ordering::Relaxed);
  for _ in 0..PROBE_WARMUP_READS {
    black_box(ordered_probe_dispatched_ticks());
  }
  let start = super::fallback::mach_time();
  for _ in 0..PROBE_READS {
    black_box(ordered_probe_dispatched_ticks());
  }
  super::fallback::mach_time().saturating_sub(start)
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn ordered_probe_dispatched_ticks() -> u64 {
  match ORDERED_PROBE_PROVIDER.load(Ordering::Relaxed) {
    ORDERED_PROVIDER_COMMPAGE => commpage_nanotime(),
    _ => super::fallback::mach_time(),
  }
}

fn evaluate_challenger(
  challenger_batches: [u64; PROBE_BATCHES],
  incumbent_batches: [u64; PROBE_BATCHES],
) -> SelectionDecision {
  let challenger_median = median(challenger_batches);
  let incumbent_median = median(incumbent_batches);
  let allowance = (incumbent_median / 20).max(PROBE_READS);
  let decisive_wins = challenger_batches
    .iter()
    .zip(incumbent_batches.iter())
    .filter(|(challenger, incumbent)| (**challenger).saturating_add(allowance) < **incumbent)
    .count();
  SelectionDecision {
    allowance,
    decisive_wins,
    challenger_selected: challenger_median.saturating_add(allowance) < incumbent_median
      && decisive_wins >= REQUIRED_DECISIVE_WINS,
  }
}

fn median(mut samples: [u64; PROBE_BATCHES]) -> u64 {
  samples.sort_unstable();
  samples[PROBE_BATCHES / 2]
}

#[inline]
fn process_id() -> i32 {
  // SAFETY: `getpid` takes no arguments and is async-signal-safe.
  unsafe { libc::getpid() }
}

#[inline]
fn current_thread_token() -> usize {
  // SAFETY: `pthread_self` takes no arguments and returns the current opaque handle.
  unsafe { libc::pthread_self() as usize }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn read_tsc() -> u64 {
  // SAFETY: x86_64 guarantees RDTSC. Eligibility separately establishes the
  // stable invariant-counter properties required by the timer contract.
  unsafe { _rdtsc() }
}

fn tsc_eligibility() -> TscEligibility {
  let (status, size, translated, errno) = translation_status();
  let translation = classify_translation(status, size, translated, errno);
  let translated = match translation {
    Translation::Native => false,
    Translation::Translated => true,
    Translation::Unavailable => {
      return TscEligibility {
        eligible: false,
        translated: false,
        basis: "ineligible_translation_status_unavailable",
      };
    }
  };

  if super::fallback::mach_timebase() != (1, 1) {
    return TscEligibility {
      eligible: false,
      translated,
      basis: "ineligible_nonidentity_x86_mach_timebase",
    };
  }
  if !cpuid_reports_invariant_tsc_and_rdtscp() {
    return TscEligibility {
      eligible: false,
      translated,
      basis: "ineligible_cpuid_missing_invariant_tsc_or_rdtscp",
    };
  }

  let scale = if translated {
    if !translated_tsc_shares_mach_timeline() {
      return TscEligibility {
        eligible: false,
        translated: true,
        basis: "ineligible_rosetta_tsc_not_mach_timeline",
      };
    }
    1_u64 << 32
  } else {
    if !commpage_page_mapped() {
      return TscEligibility {
        eligible: false,
        translated: false,
        basis: "ineligible_x86_commpage_not_mapped",
      };
    }
    let scale = commpage_tsc_nanos_per_tick_q32();
    if scale == 0 {
      return TscEligibility {
        eligible: false,
        translated: false,
        basis: "ineligible_x86_commpage_invalid_tsc_scale",
      };
    }
    scale
  };
  INSTANT_TSC_NANOS_PER_TICK_Q32.store(scale, Ordering::Release);

  TscEligibility {
    eligible: true,
    translated,
    basis: if translated {
      "eligible_rosetta_invariant_tsc_mach_timeline"
    } else {
      "eligible_native_invariant_tsc_xnu_commpage_scale"
    },
  }
}

#[allow(unused_unsafe)] // CPUID intrinsic safety changed across supported toolchains.
fn cpuid_reports_invariant_tsc_and_rdtscp() -> bool {
  // SAFETY: CPUID is available on every x86_64 processor.
  let maximum = unsafe { __cpuid(0x8000_0000) }.eax;
  if maximum < 0x8000_0007 {
    return false;
  }
  // SAFETY: the maximum extended leaf includes both queried leaves.
  let features = unsafe { __cpuid(0x8000_0001) };
  // SAFETY: guarded by the maximum extended leaf above.
  let power = unsafe { __cpuid(0x8000_0007) };
  features.edx & (1 << 27) != 0 && power.edx & (1 << 8) != 0
}

fn translated_tsc_shares_mach_timeline() -> bool {
  for _ in 0..256 {
    let before = super::fallback::mach_time();
    let direct = read_tsc();
    let after = super::fallback::mach_time();
    if direct < before || direct > after {
      return false;
    }
  }
  true
}

fn commpage_tsc_nanos_per_tick_q32() -> u64 {
  // SAFETY: the caller first verifies that XNU's read-only x86 commpage is
  // mapped. These conversion fields are read only during initialization.
  let scale =
    unsafe { core::ptr::read_volatile((COMM_PAGE_TIME_DATA_START + NT_SCALE) as *const u32) };
  // SAFETY: same mapped commpage field set as `scale`.
  let shift =
    unsafe { core::ptr::read_volatile((COMM_PAGE_TIME_DATA_START + NT_SHIFT) as *const u32) }
      & 0x1f;
  u64::from(scale).checked_shl(shift).unwrap_or(0)
}

pub(crate) fn instant_nanos_per_tick_q32() -> u64 {
  if selected_instant_provider() == INSTANT_PROVIDER_TSC {
    INSTANT_TSC_NANOS_PER_TICK_Q32.load(Ordering::Acquire).max(1)
  } else {
    1_u64 << 32
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Translation {
  Native,
  Translated,
  Unavailable,
}

fn translation_status() -> (i32, usize, i32, i32) {
  let mut translated = 0_i32;
  let mut size = core::mem::size_of::<i32>();
  // SAFETY: the name is NUL-terminated, `translated` is writable for `size`
  // bytes, and this read-only sysctl passes no replacement value.
  let status = unsafe {
    libc::sysctlbyname(
      b"sysctl.proc_translated\0".as_ptr().cast(),
      (&mut translated as *mut i32).cast(),
      &mut size,
      core::ptr::null_mut(),
      0,
    )
  };
  // SAFETY: Darwin exposes the calling thread's errno through `__error`.
  let errno = if status == 0 { 0 } else { unsafe { *libc::__error() } };
  (status, size, translated, errno)
}

fn classify_translation(status: i32, size: usize, translated: i32, errno: i32) -> Translation {
  if status == 0 && size == core::mem::size_of::<i32>() {
    return match translated {
      0 => Translation::Native,
      1 => Translation::Translated,
      _ => Translation::Unavailable,
    };
  }
  if status != 0 && errno == libc::ENOENT {
    return Translation::Native;
  }
  Translation::Unavailable
}

#[cfg(any(feature = "bench-internal", test))]
fn commpage_eligible() -> bool {
  commpage_eligibility().eligible
}

fn commpage_eligibility() -> CommpageEligibility {
  let mut translated = 0_i32;
  let mut size = core::mem::size_of::<i32>();
  // SAFETY: the name is NUL-terminated, `translated` is writable for `size`
  // bytes, and this read-only sysctl passes no replacement value.
  let status = unsafe {
    libc::sysctlbyname(
      b"sysctl.proc_translated\0".as_ptr().cast(),
      (&mut translated as *mut i32).cast(),
      &mut size,
      core::ptr::null_mut(),
      0,
    )
  };
  // SAFETY: Darwin exposes the calling thread's errno through `__error`.
  let errno = if status == 0 { 0 } else { unsafe { *libc::__error() } };
  let mut eligibility = classify_commpage_eligibility(status, size, translated, errno);
  if !eligibility.eligible {
    return eligibility;
  }
  // XNU's x86 clock_timebase_info returns 1/1 because both its system
  // function and commpage protocol return nanoseconds. Fail closed if a future
  // kernel changes that ABI so the selected provider can never change the raw
  // domain expected by Instant arithmetic.
  if super::fallback::mach_timebase() != (1, 1) {
    return CommpageEligibility {
      eligible: false,
      basis: "ineligible_nonidentity_x86_mach_timebase",
    };
  }
  if !commpage_page_mapped() {
    return CommpageEligibility { eligible: false, basis: "ineligible_x86_commpage_not_mapped" };
  }
  eligibility.basis = match eligibility.basis {
    "eligible_pre_rosetta_macos_sysctl_absent" => {
      "eligible_pre_rosetta_intel_identity_timebase_mapped_commpage"
    }
    _ => "eligible_native_intel_identity_timebase_mapped_commpage",
  };
  eligibility
}

fn commpage_page_mapped() -> bool {
  let mut residency = 0 as libc::c_char;
  // SAFETY: this only asks Darwin about one page in the calling process. XNU's
  // x86 userspace ABI maps the commpage read-only at this fixed address, and
  // `mach_absolute_time` itself unconditionally reads the same page. `mincore`
  // lets unusual environments fail closed before tach dereferences it.
  unsafe {
    libc::mincore(COMM_PAGE_BASE as *const libc::c_void, COMM_PAGE_LENGTH, &mut residency) == 0
  }
}

fn classify_commpage_eligibility(
  status: i32,
  size: usize,
  translated: i32,
  errno: i32,
) -> CommpageEligibility {
  if status == 0 && size == core::mem::size_of::<i32>() {
    return match translated {
      0 => CommpageEligibility { eligible: true, basis: "eligible_native_intel_sysctl_zero" },
      1 => CommpageEligibility { eligible: false, basis: "ineligible_rosetta_x86_translation" },
      _ => CommpageEligibility { eligible: false, basis: "ineligible_invalid_translation_status" },
    };
  }
  if status != 0 && errno == libc::ENOENT {
    // The key predates Rosetta 2. Its absence therefore identifies an older,
    // native-Intel macOS release where XNU's x86 commpage is authoritative.
    return CommpageEligibility {
      eligible: true,
      basis: "eligible_pre_rosetta_macos_sysctl_absent",
    };
  }
  CommpageEligibility { eligible: false, basis: "ineligible_translation_status_unavailable" }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn commpage_nanotime() -> u64 {
  let result: u64;
  // SAFETY: XNU maps this read-only commpage into every x86_64 userspace
  // process. This is the instruction-for-instruction protocol exported by
  // XNU's `mach_absolute_time`: generation seqlock, ordered TSC read, fixed-
  // point conversion, then generation validation. The assembly declares its
  // memory effects so the compiler cannot move surrounding accesses through
  // the protocol.
  unsafe {
    asm!(
      "2:",
      "mov r8d, dword ptr [{data} + {generation}]",
      "test r8d, r8d",
      "jz 2b",
      "lfence",
      "rdtsc",
      "lfence",
      "shl rdx, 32",
      "or rax, rdx",
      "mov ecx, dword ptr [{data} + {shift}]",
      "and ecx, 0x1f",
      "sub rax, qword ptr [{data} + {tsc_base}]",
      "shl rax, cl",
      "mov ecx, dword ptr [{data} + {scale}]",
      "mul rcx",
      "shrd rax, rdx, 32",
      "add rax, qword ptr [{data} + {ns_base}]",
      "cmp r8d, dword ptr [{data} + {generation}]",
      "jne 2b",
      data = in(reg) COMM_PAGE_TIME_DATA_START,
      generation = const NT_GENERATION,
      shift = const NT_SHIFT,
      tsc_base = const NT_TSC_BASE,
      scale = const NT_SCALE,
      ns_base = const NT_NS_BASE,
      out("rax") result,
      out("rcx") _,
      out("rdx") _,
      out("r8") _,
      options(nostack),
    );
  }
  result
}

#[cfg(feature = "bench-internal")]
fn ordered_provider_name(provider: u8) -> &'static str {
  match provider {
    ORDERED_PROVIDER_COMMPAGE => "apple_commpage_lfence_rdtsc_nanotime",
    _ => "apple_mach_absolute_time",
  }
}

#[cfg(feature = "bench-internal")]
fn instant_provider_name(provider: usize) -> &'static str {
  match provider {
    INSTANT_PROVIDER_TSC => "apple_invariant_rdtsc",
    _ => "apple_mach_absolute_time",
  }
}

#[cfg(feature = "bench-internal")]
fn store_ordered_evidence(evidence: WallSelectionEvidence) {
  // SAFETY: only the selection owner writes, before Release-publication of
  // the selected provider.
  unsafe { (*ORDERED_EVIDENCE.0.get()).write(evidence) };
}

#[cfg(feature = "bench-internal")]
fn store_instant_evidence(run: &InstantSelectionRun, selected: usize) {
  let evidence = InstantSelectionEvidence {
    reads_per_batch: PROBE_READS,
    tsc_eligible: run.eligibility.eligible,
    tsc_eligibility_basis: run.eligibility.basis,
    translated: run.eligibility.translated,
    mach_absolute_time_batches_ticks: run.mach_batches,
    tsc_batches_ticks: run.tsc_batches,
    mach_absolute_time_median_ticks: median(run.mach_batches),
    tsc_median_ticks: median(run.tsc_batches),
    allowance_total_ticks: run.decision.allowance,
    decisive_wins: run.decision.decisive_wins,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    tsc_selected: selected == INSTANT_PROVIDER_TSC,
    measured_winner: instant_provider_name(run.measured),
    selected_provider: instant_provider_name(selected),
  };
  // SAFETY: only the selection owner writes, before returning the provider
  // whose publication was observed above.
  unsafe { (*INSTANT_EVIDENCE.0.get()).write(evidence) };
  INSTANT_EVIDENCE_READY.store(true, Ordering::Release);
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_ordered_selection_evidence() -> WallSelectionEvidence {
  let _ = selected_ordered_provider();
  // SAFETY: the selected-provider Acquire observes the evidence write before
  // its Release publication.
  unsafe { (*ORDERED_EVIDENCE.0.get()).assume_init_read() }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_selection_evidence() -> InstantSelectionEvidence {
  let _ = selected_instant_provider();
  while !INSTANT_EVIDENCE_READY.load(Ordering::Acquire) {
    spin_loop();
  }
  // SAFETY: the Acquire above observes the selector owner's evidence write.
  unsafe { (*INSTANT_EVIDENCE.0.get()).assume_init_read() }
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
pub(crate) fn bench_commpage_eligible() -> bool {
  commpage_eligible()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_commpage_nanotime() -> u64 {
  commpage_nanotime()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_selected_ticks() -> u64 {
  ticks()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_selected_ordered_ticks() -> u64 {
  ticks_ordered()
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_tsc_eligible() -> bool {
  tsc_eligibility().eligible
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_tsc_nanos_per_tick_q32() -> u64 {
  if tsc_eligibility().eligible {
    INSTANT_TSC_NANOS_PER_TICK_Q32.load(Ordering::Acquire).max(1)
  } else {
    1_u64 << 32
  }
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_tsc() -> u64 {
  read_tsc()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn direct_protocol_shares_the_mach_absolute_time_timeline() {
    if !commpage_eligible() {
      return;
    }
    for _ in 0..100_000 {
      let before = super::super::fallback::mach_time();
      let direct = commpage_nanotime();
      let after = super::super::fallback::mach_time();
      assert!(before <= direct && direct <= after);
    }
  }

  #[test]
  fn selected_protocol_is_monotonic() {
    let mut previous = ticks();
    for _ in 0..100_000 {
      let current = ticks();
      assert!(current >= previous);
      previous = current;
    }
  }

  #[test]
  fn translated_tsc_is_runtime_bound_to_the_mach_timeline() {
    let (status, size, translated, errno) = translation_status();
    if classify_translation(status, size, translated, errno) != Translation::Translated {
      return;
    }
    let eligibility = tsc_eligibility();
    assert!(eligibility.eligible, "{}", eligibility.basis);
    assert!(eligibility.translated);
    assert_eq!(INSTANT_TSC_NANOS_PER_TICK_Q32.load(Ordering::Acquire), 1_u64 << 32);
    assert!(translated_tsc_shares_mach_timeline());
  }

  #[test]
  fn same_thread_reentry_and_fork_choose_the_sticky_mach_domain() {
    let owner = instant_selecting_state(0x1234);
    assert_eq!(selecting_disposition(owner, 7, owner, 7), SelectingDisposition::UseMachFallback);
    assert_eq!(
      selecting_disposition(owner, 7, instant_selecting_state(0x5678), 8),
      SelectingDisposition::UseMachFallback
    );
    assert_eq!(
      selecting_disposition(owner, 7, instant_selecting_state(0x5678), 7),
      SelectingDisposition::Wait
    );
  }

  #[test]
  fn selection_requires_a_repeatable_material_win() {
    let incumbent = [100_000; PROBE_BATCHES];
    let mut noisy = [94_000; PROBE_BATCHES];
    noisy[7] = 100_000;
    noisy[8] = 100_000;
    assert!(!evaluate_challenger(noisy, incumbent).challenger_selected);
    assert!(evaluate_challenger([90_000; PROBE_BATCHES], incumbent).challenger_selected);
  }

  #[test]
  fn commpage_eligibility_fails_closed_except_on_pre_rosetta_macos() {
    assert!(classify_commpage_eligibility(0, core::mem::size_of::<i32>(), 0, 0).eligible);
    assert!(!classify_commpage_eligibility(0, core::mem::size_of::<i32>(), 1, 0).eligible);
    assert!(classify_commpage_eligibility(-1, 0, 0, libc::ENOENT).eligible);
    assert!(!classify_commpage_eligibility(-1, 0, 0, libc::EPERM).eligible);
    assert!(!classify_commpage_eligibility(0, 0, 0, 0).eligible);
  }

  #[test]
  fn xnu_x86_absolute_time_uses_nanosecond_ticks() {
    assert_eq!(super::super::fallback::mach_timebase(), (1, 1));
  }
}
