//! Intel macOS wall-clock provider selection.
//!
//! Both candidates read XNU's Mach absolute-time domain. The system function
//! is the compatibility baseline; the commpage candidate inlines the exact
//! seqlock, `lfence; rdtsc; lfence`, and fixed-point conversion protocol used
//! by XNU's x86_64 `mach_absolute_time` implementation. Capability alone does
//! not decide between them: the first read measures the steady-state dispatch
//! cost of both implementations and retains the system function unless the
//! inline path wins materially and repeatably.
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
#[cfg(feature = "bench-internal")]
use core::cell::UnsafeCell;
use core::hint::black_box;
#[cfg(feature = "bench-internal")]
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicI32, AtomicU8, Ordering};

const PROVIDER_UNKNOWN: u8 = 0;
const PROVIDER_SELECTING: u8 = 1;
const PROVIDER_MACH_ABSOLUTE_TIME: u8 = 2;
const PROVIDER_COMMPAGE: u8 = 3;

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

static SELECTED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static SELECTING_PID: AtomicI32 = AtomicI32::new(0);
// Selection measures this dispatcher rather than bare helpers so the decision
// includes the atomic load and branch paid by every post-initialization read.
static PROBE_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_MACH_ABSOLUTE_TIME);

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
struct EvidenceCell(UnsafeCell<MaybeUninit<WallSelectionEvidence>>);

// SAFETY: the one selection owner writes the evidence before publishing the
// selected provider with Release. Readers acquire that provider first.
#[cfg(feature = "bench-internal")]
unsafe impl Sync for EvidenceCell {}

#[cfg(feature = "bench-internal")]
static EVIDENCE: EvidenceCell = EvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  let provider = SELECTED_PROVIDER.load(Ordering::Relaxed);
  match provider {
    PROVIDER_MACH_ABSOLUTE_TIME => super::fallback::mach_time(),
    PROVIDER_COMMPAGE => commpage_nanotime(),
    PROVIDER_SELECTING => {
      // A signal handler or another thread may reenter while the first caller
      // is probing. Both candidates share the exact Mach absolute-time domain,
      // so the baseline is a safe non-blocking read until publication. A fork
      // child has a different PID and must instead recover the inherited state.
      if SELECTING_PID.load(Ordering::Relaxed) == process_id() {
        super::fallback::mach_time()
      } else {
        ticks_after_selection()
      }
    }
    _ => ticks_after_selection(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  // Both x86_64 candidates execute XNU's LFENCE-ordered absolute-time
  // protocol, so the same measured provider satisfies both wall contracts.
  ticks()
}

#[cold]
#[inline(never)]
fn ticks_after_selection() -> u64 {
  match selected_provider() {
    PROVIDER_COMMPAGE => commpage_nanotime(),
    _ => super::fallback::mach_time(),
  }
}

fn selected_provider() -> u8 {
  loop {
    let provider = SELECTED_PROVIDER.load(Ordering::Acquire);
    match provider {
      PROVIDER_MACH_ABSOLUTE_TIME | PROVIDER_COMMPAGE => return provider,
      PROVIDER_UNKNOWN => {
        SELECTING_PID.store(process_id(), Ordering::Relaxed);
        if SELECTED_PROVIDER
          .compare_exchange(
            PROVIDER_UNKNOWN,
            PROVIDER_SELECTING,
            Ordering::AcqRel,
            Ordering::Acquire,
          )
          .is_ok()
        {
          let selected = select_provider();
          SELECTED_PROVIDER.store(selected, Ordering::Release);
          return selected;
        }
      }
      PROVIDER_SELECTING => {
        // A child created while another thread was probing must not inherit a
        // permanently selecting state owned by a thread that no longer exists.
        if SELECTING_PID.load(Ordering::Relaxed) != process_id() {
          let _ = SELECTED_PROVIDER.compare_exchange(
            PROVIDER_SELECTING,
            PROVIDER_UNKNOWN,
            Ordering::AcqRel,
            Ordering::Acquire,
          );
        } else {
          // Both candidates are the Mach absolute-time domain, so a reentrant
          // or concurrent first reader can use the system baseline.
          return PROVIDER_MACH_ABSOLUTE_TIME;
        }
      }
      _ => unreachable!("invalid Intel macOS wall provider"),
    }
  }
}

#[cold]
fn select_provider() -> u8 {
  let eligibility = commpage_eligibility();
  if !eligibility.eligible {
    #[cfg(feature = "bench-internal")]
    store_evidence(WallSelectionEvidence {
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
      selected_provider: provider_name(PROVIDER_MACH_ABSOLUTE_TIME),
    });
    return PROVIDER_MACH_ABSOLUTE_TIME;
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
      mach_batches[batch] = probe_batch(PROVIDER_MACH_ABSOLUTE_TIME);
      commpage_batches[batch] = probe_batch(PROVIDER_COMMPAGE);
    } else {
      commpage_batches[batch] = probe_batch(PROVIDER_COMMPAGE);
      mach_batches[batch] = probe_batch(PROVIDER_MACH_ABSOLUTE_TIME);
    }
  }

  let decision = evaluate_challenger(commpage_batches, mach_batches);
  let selected =
    if decision.challenger_selected { PROVIDER_COMMPAGE } else { PROVIDER_MACH_ABSOLUTE_TIME };

  #[cfg(feature = "bench-internal")]
  store_evidence(WallSelectionEvidence {
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
    selected_provider: provider_name(selected),
  });

  selected
}

#[inline(never)]
fn probe_batch(provider: u8) -> u64 {
  PROBE_PROVIDER.store(provider, Ordering::Relaxed);
  for _ in 0..PROBE_WARMUP_READS {
    black_box(probe_dispatched_ticks());
  }
  let start = super::fallback::mach_time();
  for _ in 0..PROBE_READS {
    black_box(probe_dispatched_ticks());
  }
  super::fallback::mach_time().saturating_sub(start)
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn probe_dispatched_ticks() -> u64 {
  match PROBE_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_COMMPAGE => commpage_nanotime(),
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
fn provider_name(provider: u8) -> &'static str {
  match provider {
    PROVIDER_COMMPAGE => "apple_commpage_lfence_rdtsc_nanotime",
    _ => "apple_mach_absolute_time",
  }
}

#[cfg(feature = "bench-internal")]
fn store_evidence(evidence: WallSelectionEvidence) {
  // SAFETY: only the selection owner writes, before Release-publication of
  // the selected provider.
  unsafe { (*EVIDENCE.0.get()).write(evidence) };
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_selection_evidence() -> WallSelectionEvidence {
  let _ = selected_provider();
  // SAFETY: the selected-provider Acquire observes the evidence write before
  // its Release publication.
  unsafe { (*EVIDENCE.0.get()).assume_init_read() }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_provider() -> &'static str {
  provider_name(selected_provider())
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
  fn selection_requires_a_repeatable_material_win() {
    let incumbent = [100_000; PROBE_BATCHES];
    let mut noisy = [94_000; PROBE_BATCHES];
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
