//! Apple Silicon wall-clock provider selection.
//!
//! XNU exposes two exact monotonic timelines on arm64. Mach absolute time
//! excludes system sleep; Mach continuous time includes it. Both use the Mach
//! timebase ratio and are valid `Instant`-style wall clocks. Tach measures the
//! complete post-selection read path for both contracts independently and
//! retains a challenger only when it wins materially and repeatably.

use core::arch::asm;
use core::hint::{black_box, spin_loop};
#[cfg(any(feature = "bench-internal", test))]
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64};
use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

const PROVIDER_UNKNOWN: usize = 0;
const PROVIDER_MACH_ABSOLUTE: usize = 1;
const PROVIDER_ABSOLUTE_CNTVCT: usize = 2;
const PROVIDER_ABSOLUTE_CNTVCTSS: usize = 3;
const PROVIDER_ABSOLUTE_ACNTVCT: usize = 4;
const PROVIDER_MACH_CONTINUOUS: usize = 5;
const PROVIDER_CONTINUOUS_CNTVCT: usize = 6;
const PROVIDER_CONTINUOUS_CNTVCTSS: usize = 7;
const PROVIDER_CONTINUOUS_ACNTVCT: usize = 8;
const MAX_PROVIDER: usize = PROVIDER_CONTINUOUS_ACNTVCT;
const SELECTING_TAG: usize = 1 << (usize::BITS - 1);

const COMM_PAGE_BASE: usize = 0x0000_000f_ffff_c000;
const TIMEBASE_OFFSET: usize = COMM_PAGE_BASE + 0x088;
const USER_TIMEBASE: usize = COMM_PAGE_BASE + 0x090;
const CONT_HWCLOCK: usize = COMM_PAGE_BASE + 0x091;
const CONT_HW_TIMEBASE: usize = COMM_PAGE_BASE + 0x0a8;

#[cfg(any(feature = "bench-internal", test))]
const USER_TIMEBASE_NONE: u8 = 0;
const USER_TIMEBASE_SPEC: u8 = 1;
const USER_TIMEBASE_NOSPEC: u8 = 2;
const USER_TIMEBASE_NOSPEC_APPLE: u8 = 3;

const MAX_CANDIDATES: usize = 4;
const PROBE_BATCHES: usize = 9;
const PROBE_READS: u64 = 4096;
const PROBE_WARMUP_READS: u64 = 1024;
const REQUIRED_DECISIVE_WINS: usize = 8;

static INSTANT_SELECTOR: Selector = Selector::new();
static ORDERED_SELECTOR: Selector = Selector::new();

unsafe extern "C" {
  fn mach_continuous_time() -> u64;
}

struct Selector {
  state: AtomicUsize,
  owner_pid: AtomicI32,
  probe_provider: AtomicUsize,
  #[cfg(any(feature = "bench-internal", test))]
  evidence: EvidenceStorage,
}

impl Selector {
  const fn new() -> Self {
    Self {
      state: AtomicUsize::new(PROVIDER_UNKNOWN),
      owner_pid: AtomicI32::new(0),
      probe_provider: AtomicUsize::new(PROVIDER_MACH_ABSOLUTE),
      #[cfg(any(feature = "bench-internal", test))]
      evidence: EvidenceStorage::new(),
    }
  }
}

#[derive(Clone, Copy)]
struct CandidateList {
  providers: [usize; MAX_CANDIDATES],
  count: usize,
}

impl CandidateList {
  const fn new() -> Self {
    Self { providers: [PROVIDER_UNKNOWN; MAX_CANDIDATES], count: 0 }
  }

  fn push(&mut self, provider: usize) {
    debug_assert!(self.count < MAX_CANDIDATES);
    debug_assert!(!self.providers[..self.count].contains(&provider));
    self.providers[self.count] = provider;
    self.count += 1;
  }

  fn as_slice(&self) -> &[usize] {
    &self.providers[..self.count]
  }
}

#[derive(Clone, Copy)]
struct SelectionRun {
  #[cfg(any(feature = "bench-internal", test))]
  mode: u8,
  #[cfg(any(feature = "bench-internal", test))]
  continuous_hwclock: bool,
  #[cfg(any(feature = "bench-internal", test))]
  candidates: CandidateList,
  #[cfg(any(feature = "bench-internal", test))]
  samples: [[u64; PROBE_BATCHES]; MAX_CANDIDATES],
  winner: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct SelectionDecision {
  challenger_selected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectingDisposition {
  UseAbsoluteFallback,
  Wait,
}

#[cfg(any(feature = "bench-internal", test))]
struct EvidenceStorage {
  ready: AtomicBool,
  mode: AtomicU8,
  continuous_hwclock: AtomicBool,
  candidate_count: AtomicU8,
  candidates: [AtomicU8; MAX_CANDIDATES],
  samples: [AtomicU64; MAX_CANDIDATES * PROBE_BATCHES],
  selected: AtomicU8,
  measured_winner: AtomicU8,
  forced_absolute_fallback: AtomicBool,
}

#[cfg(any(feature = "bench-internal", test))]
impl EvidenceStorage {
  const fn new() -> Self {
    Self {
      ready: AtomicBool::new(false),
      mode: AtomicU8::new(USER_TIMEBASE_NONE),
      continuous_hwclock: AtomicBool::new(false),
      candidate_count: AtomicU8::new(0),
      candidates: [const { AtomicU8::new(PROVIDER_UNKNOWN as u8) }; MAX_CANDIDATES],
      samples: [const { AtomicU64::new(0) }; MAX_CANDIDATES * PROBE_BATCHES],
      selected: AtomicU8::new(PROVIDER_UNKNOWN as u8),
      measured_winner: AtomicU8::new(PROVIDER_UNKNOWN as u8),
      forced_absolute_fallback: AtomicBool::new(false),
    }
  }
}

#[cfg(any(feature = "bench-internal", test))]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // The benchmark serializer projects the complete selector evidence.
pub(crate) struct CandidateProbeEvidence {
  pub(crate) name: &'static str,
  pub(crate) batches_ticks: [u64; PROBE_BATCHES],
  pub(crate) median_ticks: u64,
}

#[cfg(any(feature = "bench-internal", test))]
const EMPTY_CANDIDATE_EVIDENCE: CandidateProbeEvidence =
  CandidateProbeEvidence { name: "not_run", batches_ticks: [0; PROBE_BATCHES], median_ticks: 0 };

#[cfg(any(feature = "bench-internal", test))]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // The benchmark serializer projects the complete selector evidence.
pub(crate) struct SelectionEvidence {
  pub(crate) ready: bool,
  pub(crate) user_timebase_mode: u8,
  pub(crate) continuous_hwclock: bool,
  pub(crate) reads_per_batch: u64,
  pub(crate) candidate_count: usize,
  pub(crate) candidates: [CandidateProbeEvidence; MAX_CANDIDATES],
  pub(crate) required_decisive_wins: usize,
  pub(crate) measured_winner: &'static str,
  pub(crate) selected_provider: &'static str,
  pub(crate) selection_basis: &'static str,
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  match INSTANT_SELECTOR.state.load(Ordering::Relaxed) {
    PROVIDER_MACH_ABSOLUTE => mach_absolute(),
    PROVIDER_ABSOLUTE_CNTVCT => cntvct_absolute_time(),
    PROVIDER_ABSOLUTE_CNTVCTSS => cntvctss_absolute_time(),
    PROVIDER_ABSOLUTE_ACNTVCT => acntvct_absolute_time(),
    PROVIDER_MACH_CONTINUOUS => mach_continuous(),
    PROVIDER_CONTINUOUS_CNTVCT => cntvct_continuous_time(),
    PROVIDER_CONTINUOUS_CNTVCTSS => cntvctss_continuous_time(),
    PROVIDER_CONTINUOUS_ACNTVCT => acntvct_continuous_time(),
    _ => ticks_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_after_selection() -> u64 {
  read_instant_provider(selected_provider::<false>(&INSTANT_SELECTOR))
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  match ORDERED_SELECTOR.state.load(Ordering::Relaxed) {
    PROVIDER_MACH_ABSOLUTE => mach_absolute(),
    PROVIDER_ABSOLUTE_CNTVCT => cntvct_ordered_absolute_time(),
    PROVIDER_ABSOLUTE_CNTVCTSS => cntvctss_absolute_time(),
    PROVIDER_ABSOLUTE_ACNTVCT => acntvct_absolute_time(),
    PROVIDER_MACH_CONTINUOUS => mach_continuous(),
    PROVIDER_CONTINUOUS_CNTVCT => cntvct_ordered_continuous_time(),
    PROVIDER_CONTINUOUS_CNTVCTSS => cntvctss_continuous_time(),
    PROVIDER_CONTINUOUS_ACNTVCT => acntvct_continuous_time(),
    _ => ticks_ordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_after_selection() -> u64 {
  read_ordered_provider(selected_provider::<true>(&ORDERED_SELECTOR))
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  match ORDERED_SELECTOR.state.load(Ordering::Relaxed) {
    PROVIDER_MACH_ABSOLUTE => mach_absolute(),
    PROVIDER_ABSOLUTE_CNTVCT => cntvct_absolute_time(),
    PROVIDER_ABSOLUTE_CNTVCTSS => cntvctss_absolute_time(),
    PROVIDER_ABSOLUTE_ACNTVCT => acntvct_absolute_time(),
    PROVIDER_MACH_CONTINUOUS => mach_continuous(),
    PROVIDER_CONTINUOUS_CNTVCT => cntvct_continuous_time(),
    PROVIDER_CONTINUOUS_CNTVCTSS => cntvctss_continuous_time(),
    PROVIDER_CONTINUOUS_ACNTVCT => acntvct_continuous_time(),
    _ => ticks_ordered_unordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_unordered_after_selection() -> u64 {
  read_instant_provider(selected_provider::<true>(&ORDERED_SELECTOR))
}

fn selected_provider<const ORDERED: bool>(selector: &Selector) -> usize {
  loop {
    let state = selector.state.load(Ordering::Acquire);
    if is_provider(state) {
      return state;
    }

    if state == PROVIDER_UNKNOWN {
      let owner = selecting_state(current_thread_token());
      selector.owner_pid.store(process_id(), Ordering::Relaxed);
      if selector
        .state
        .compare_exchange(PROVIDER_UNKNOWN, owner, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
      {
        let run = select_provider::<ORDERED>(selector);
        let selected = match selector.state.compare_exchange(
          owner,
          run.winner,
          Ordering::AcqRel,
          Ordering::Acquire,
        ) {
          Ok(_) => run.winner,
          Err(published) => provider_or_absolute(published),
        };
        store_selection_evidence::<ORDERED>(selector, &run, selected);
        return selected;
      }
      continue;
    }

    if is_selecting(state) {
      match selecting_disposition(
        state,
        selector.owner_pid.load(Ordering::Relaxed),
        selecting_state(current_thread_token()),
        process_id(),
      ) {
        SelectingDisposition::UseAbsoluteFallback => {
          let _ = selector.state.compare_exchange(
            state,
            PROVIDER_MACH_ABSOLUTE,
            Ordering::AcqRel,
            Ordering::Acquire,
          );
        }
        SelectingDisposition::Wait => spin_loop(),
      }
      continue;
    }

    let _ = selector.state.compare_exchange(
      state,
      PROVIDER_MACH_ABSOLUTE,
      Ordering::AcqRel,
      Ordering::Acquire,
    );
  }
}

#[cold]
#[allow(clippy::needless_range_loop)] // Alternating whole columns avoids order bias.
fn select_provider<const ORDERED: bool>(selector: &Selector) -> SelectionRun {
  let mode = user_timebase_mode();
  let continuous_hwclock = continuous_hwclock_available();
  let candidates = candidates(mode, continuous_hwclock);
  let mut samples = [[0; PROBE_BATCHES]; MAX_CANDIDATES];

  for provider in candidates.as_slice() {
    selector.probe_provider.store(*provider, Ordering::Relaxed);
    for _ in 0..PROBE_WARMUP_READS {
      black_box(probe_dispatched_ticks::<ORDERED>(selector));
    }
  }

  for batch in 0..PROBE_BATCHES {
    if batch & 1 == 0 {
      for index in 0..candidates.count {
        samples[index][batch] = probe_batch::<ORDERED>(selector, candidates.providers[index]);
      }
    } else {
      for index in (0..candidates.count).rev() {
        samples[index][batch] = probe_batch::<ORDERED>(selector, candidates.providers[index]);
      }
    }
  }

  let one_ns_per_read_ticks = one_ns_per_read_allowance_ticks();
  let mut winner_index = 0;
  for challenger_index in 1..candidates.count {
    let decision =
      evaluate_challenger(samples[challenger_index], samples[winner_index], one_ns_per_read_ticks);
    if decision.challenger_selected {
      winner_index = challenger_index;
    }
  }

  SelectionRun {
    #[cfg(any(feature = "bench-internal", test))]
    mode,
    #[cfg(any(feature = "bench-internal", test))]
    continuous_hwclock,
    #[cfg(any(feature = "bench-internal", test))]
    candidates,
    #[cfg(any(feature = "bench-internal", test))]
    samples,
    winner: candidates.providers[winner_index],
  }
}

#[inline(never)]
fn probe_batch<const ORDERED: bool>(selector: &Selector, provider: usize) -> u64 {
  selector.probe_provider.store(provider, Ordering::Relaxed);
  for _ in 0..PROBE_WARMUP_READS {
    black_box(probe_dispatched_ticks::<ORDERED>(selector));
  }
  let start = mach_absolute();
  for _ in 0..PROBE_READS {
    black_box(probe_dispatched_ticks::<ORDERED>(selector));
  }
  mach_absolute().saturating_sub(start)
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn probe_dispatched_ticks<const ORDERED: bool>(selector: &Selector) -> u64 {
  let provider = selector.probe_provider.load(Ordering::Relaxed);
  if ORDERED { read_ordered_provider(provider) } else { read_instant_provider(provider) }
}

fn candidates(mode: u8, continuous_hwclock: bool) -> CandidateList {
  let mut candidates = CandidateList::new();
  if let Some(provider) = absolute_direct_provider(mode) {
    candidates.push(provider);
  }
  candidates.push(PROVIDER_MACH_ABSOLUTE);
  if continuous_hwclock {
    candidates.push(continuous_direct_provider(mode));
  }
  candidates.push(PROVIDER_MACH_CONTINUOUS);
  candidates
}

const fn absolute_direct_provider(mode: u8) -> Option<usize> {
  match mode {
    USER_TIMEBASE_SPEC => Some(PROVIDER_ABSOLUTE_CNTVCT),
    USER_TIMEBASE_NOSPEC => Some(PROVIDER_ABSOLUTE_CNTVCTSS),
    USER_TIMEBASE_NOSPEC_APPLE => Some(PROVIDER_ABSOLUTE_ACNTVCT),
    _ => None,
  }
}

const fn continuous_direct_provider(mode: u8) -> usize {
  match mode {
    USER_TIMEBASE_NOSPEC => PROVIDER_CONTINUOUS_CNTVCTSS,
    USER_TIMEBASE_NOSPEC_APPLE => PROVIDER_CONTINUOUS_ACNTVCT,
    _ => PROVIDER_CONTINUOUS_CNTVCT,
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn read_instant_provider(provider: usize) -> u64 {
  match provider {
    PROVIDER_ABSOLUTE_CNTVCT => cntvct_absolute_time(),
    PROVIDER_ABSOLUTE_CNTVCTSS => cntvctss_absolute_time(),
    PROVIDER_ABSOLUTE_ACNTVCT => acntvct_absolute_time(),
    PROVIDER_MACH_CONTINUOUS => mach_continuous(),
    PROVIDER_CONTINUOUS_CNTVCT => cntvct_continuous_time(),
    PROVIDER_CONTINUOUS_CNTVCTSS => cntvctss_continuous_time(),
    PROVIDER_CONTINUOUS_ACNTVCT => acntvct_continuous_time(),
    _ => mach_absolute(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn read_ordered_provider(provider: usize) -> u64 {
  match provider {
    PROVIDER_ABSOLUTE_CNTVCT => cntvct_ordered_absolute_time(),
    PROVIDER_ABSOLUTE_CNTVCTSS => cntvctss_absolute_time(),
    PROVIDER_ABSOLUTE_ACNTVCT => acntvct_absolute_time(),
    PROVIDER_MACH_CONTINUOUS => mach_continuous(),
    PROVIDER_CONTINUOUS_CNTVCT => cntvct_ordered_continuous_time(),
    PROVIDER_CONTINUOUS_CNTVCTSS => cntvctss_continuous_time(),
    PROVIDER_CONTINUOUS_ACNTVCT => acntvct_continuous_time(),
    _ => mach_absolute(),
  }
}

fn evaluate_challenger(
  challenger_batches: [u64; PROBE_BATCHES],
  incumbent_batches: [u64; PROBE_BATCHES],
  one_ns_per_read_ticks: u64,
) -> SelectionDecision {
  let challenger_median = median(challenger_batches);
  let incumbent_median = median(incumbent_batches);
  let allowance_ticks = (incumbent_median / 20).max(one_ns_per_read_ticks);
  let decisive_wins = challenger_batches
    .iter()
    .zip(incumbent_batches.iter())
    .filter(|(challenger, incumbent)| (**challenger).saturating_add(allowance_ticks) < **incumbent)
    .count();
  SelectionDecision {
    challenger_selected: challenger_median.saturating_add(allowance_ticks) < incumbent_median
      && decisive_wins >= REQUIRED_DECISIVE_WINS,
  }
}

fn one_ns_per_read_allowance_ticks() -> u64 {
  let (numer, denom) = super::fallback::mach_timebase();
  let numerator = u128::from(PROBE_READS) * u128::from(denom);
  let denominator = u128::from(numer.max(1));
  u64::try_from(numerator.div_ceil(denominator)).unwrap_or(u64::MAX)
}

fn median(mut samples: [u64; PROBE_BATCHES]) -> u64 {
  samples.sort_unstable();
  samples[PROBE_BATCHES / 2]
}

const fn is_provider(state: usize) -> bool {
  state >= PROVIDER_MACH_ABSOLUTE && state <= MAX_PROVIDER
}

const fn is_selecting(state: usize) -> bool {
  state & SELECTING_TAG != 0
}

const fn selecting_state(thread_token: usize) -> usize {
  SELECTING_TAG | (thread_token & !SELECTING_TAG)
}

const fn provider_or_absolute(state: usize) -> usize {
  if is_provider(state) { state } else { PROVIDER_MACH_ABSOLUTE }
}

const fn selecting_disposition(
  state: usize,
  owner_pid: i32,
  current_owner: usize,
  current_pid: i32,
) -> SelectingDisposition {
  if owner_pid != current_pid || state == current_owner {
    SelectingDisposition::UseAbsoluteFallback
  } else {
    SelectingDisposition::Wait
  }
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
fn mach_absolute() -> u64 {
  super::fallback::mach_time()
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn mach_continuous() -> u64 {
  // SAFETY: `mach_continuous_time` takes no arguments and returns an exact Mach tick value.
  unsafe { mach_continuous_time() }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn user_timebase_mode() -> u8 {
  // SAFETY: XNU maps the kernel-owned read-only commpage at this fixed arm64 address.
  unsafe { core::ptr::read_volatile(USER_TIMEBASE as *const u8) }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn continuous_hwclock_available() -> bool {
  // SAFETY: this byte is part of the same read-only XNU commpage ABI.
  unsafe { core::ptr::read_volatile(CONT_HWCLOCK as *const u8) != 0 }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn cntvct_absolute_time() -> u64 {
  let result: u64;
  // SAFETY: mode 1 permits `CNTVCT_EL0`; the offset retry is XNU's wake correction protocol.
  unsafe {
    asm!(
      "2:",
      "ldr {before}, [{offset}]",
      "mrs {counter}, cntvct_el0",
      "ldr {after}, [{offset}]",
      "cmp {before}, {after}",
      "b.ne 2b",
      "add {result}, {counter}, {before}",
      offset = in(reg) TIMEBASE_OFFSET,
      before = out(reg) _,
      counter = out(reg) _,
      after = out(reg) _,
      result = lateout(reg) result,
      options(nostack),
    );
  }
  result
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn cntvct_ordered_absolute_time() -> u64 {
  let result: u64;
  // SAFETY: mode 1 permits the counter; `isb` orders the sample after preceding work.
  unsafe {
    asm!(
      "isb sy",
      "2:",
      "ldr {before}, [{offset}]",
      "mrs {counter}, cntvct_el0",
      "ldr {after}, [{offset}]",
      "cmp {before}, {after}",
      "b.ne 2b",
      "add {result}, {counter}, {before}",
      offset = in(reg) TIMEBASE_OFFSET,
      before = out(reg) _,
      counter = out(reg) _,
      after = out(reg) _,
      result = lateout(reg) result,
      options(nostack),
    );
  }
  result
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn cntvctss_absolute_time() -> u64 {
  let result: u64;
  // SAFETY: mode 2 permits the self-synchronizing `CNTVCTSS_EL0` register.
  unsafe {
    asm!(
      "2:",
      "ldr {before}, [{offset}]",
      "mrs {counter}, S3_3_C14_C0_6",
      "ldr {after}, [{offset}]",
      "cmp {before}, {after}",
      "b.ne 2b",
      "add {result}, {counter}, {before}",
      offset = in(reg) TIMEBASE_OFFSET,
      before = out(reg) _,
      counter = out(reg) _,
      after = out(reg) _,
      result = lateout(reg) result,
      options(nostack),
    );
  }
  result
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn acntvct_absolute_time() -> u64 {
  let result: u64;
  // SAFETY: mode 3 permits Apple's self-synchronizing `ACNTVCT_EL0` register.
  unsafe {
    asm!(
      "2:",
      "ldr {before}, [{offset}]",
      "mrs {counter}, S3_4_C15_C10_6",
      "ldr {after}, [{offset}]",
      "cmp {before}, {after}",
      "b.ne 2b",
      "add {result}, {counter}, {before}",
      offset = in(reg) TIMEBASE_OFFSET,
      before = out(reg) _,
      counter = out(reg) _,
      after = out(reg) _,
      result = lateout(reg) result,
      options(nostack),
    );
  }
  result
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn cntvct_continuous_time() -> u64 {
  let result: u64;
  // SAFETY: XNU advertises this exact counter-plus-base path through `CONT_HWCLOCK`.
  unsafe {
    asm!(
      "mrs {counter}, cntvct_el0",
      "ldr {base}, [{base_address}]",
      "add {result}, {counter}, {base}",
      base_address = in(reg) CONT_HW_TIMEBASE,
      counter = out(reg) _,
      base = out(reg) _,
      result = lateout(reg) result,
      options(nostack),
    );
  }
  result
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn cntvct_ordered_continuous_time() -> u64 {
  let result: u64;
  // SAFETY: XNU's mode-1 continuous hardware path uses `isb` before `CNTVCT_EL0`.
  unsafe {
    asm!(
      "isb sy",
      "mrs {counter}, cntvct_el0",
      "ldr {base}, [{base_address}]",
      "add {result}, {counter}, {base}",
      base_address = in(reg) CONT_HW_TIMEBASE,
      counter = out(reg) _,
      base = out(reg) _,
      result = lateout(reg) result,
      options(nostack),
    );
  }
  result
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn cntvctss_continuous_time() -> u64 {
  let result: u64;
  // SAFETY: XNU advertises mode 2 with the continuous hardware base.
  unsafe {
    asm!(
      "mrs {counter}, S3_3_C14_C0_6",
      "ldr {base}, [{base_address}]",
      "add {result}, {counter}, {base}",
      base_address = in(reg) CONT_HW_TIMEBASE,
      counter = out(reg) _,
      base = out(reg) _,
      result = lateout(reg) result,
      options(nostack),
    );
  }
  result
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn acntvct_continuous_time() -> u64 {
  let result: u64;
  // SAFETY: XNU advertises mode 3 with the continuous hardware base.
  unsafe {
    asm!(
      "mrs {counter}, S3_4_C15_C10_6",
      "ldr {base}, [{base_address}]",
      "add {result}, {counter}, {base}",
      base_address = in(reg) CONT_HW_TIMEBASE,
      counter = out(reg) _,
      base = out(reg) _,
      result = lateout(reg) result,
      options(nostack),
    );
  }
  result
}

#[cfg(any(feature = "bench-internal", test))]
fn store_selection_evidence<const ORDERED: bool>(
  selector: &Selector,
  run: &SelectionRun,
  selected: usize,
) {
  let storage = &selector.evidence;
  storage.ready.store(false, Ordering::Relaxed);
  storage.mode.store(run.mode, Ordering::Relaxed);
  storage.continuous_hwclock.store(run.continuous_hwclock, Ordering::Relaxed);
  storage.candidate_count.store(run.candidates.count as u8, Ordering::Relaxed);
  for index in 0..MAX_CANDIDATES {
    storage.candidates[index].store(run.candidates.providers[index] as u8, Ordering::Relaxed);
    for batch in 0..PROBE_BATCHES {
      storage.samples[index * PROBE_BATCHES + batch]
        .store(run.samples[index][batch], Ordering::Relaxed);
    }
  }
  storage.selected.store(selected as u8, Ordering::Relaxed);
  storage.measured_winner.store(run.winner as u8, Ordering::Relaxed);
  storage
    .forced_absolute_fallback
    .store(selected != run.winner, Ordering::Relaxed);
  storage.ready.store(true, Ordering::Release);
  let _ = ORDERED;
}

#[cfg(not(any(feature = "bench-internal", test)))]
fn store_selection_evidence<const ORDERED: bool>(
  _selector: &Selector,
  _run: &SelectionRun,
  _selected: usize,
) {
}

#[cfg(any(feature = "bench-internal", test))]
#[allow(dead_code)] // Used when the benchmark serializer enables its Apple schema.
fn selection_evidence<const ORDERED: bool>(selector: &Selector) -> SelectionEvidence {
  let selected = selected_provider::<ORDERED>(selector);
  let storage = &selector.evidence;
  let ready = storage.ready.load(Ordering::Acquire);
  if !ready {
    return SelectionEvidence {
      ready: false,
      user_timebase_mode: user_timebase_mode(),
      continuous_hwclock: continuous_hwclock_available(),
      reads_per_batch: PROBE_READS,
      candidate_count: 0,
      candidates: [EMPTY_CANDIDATE_EVIDENCE; MAX_CANDIDATES],
      required_decisive_wins: REQUIRED_DECISIVE_WINS,
      measured_winner: provider_name::<ORDERED>(PROVIDER_UNKNOWN),
      selected_provider: provider_name::<ORDERED>(selected),
      selection_basis: "safe_absolute_fallback_before_evidence_publication",
    };
  }

  let mut candidates = [EMPTY_CANDIDATE_EVIDENCE; MAX_CANDIDATES];
  let candidate_count = usize::from(storage.candidate_count.load(Ordering::Relaxed));
  for (index, candidate) in candidates.iter_mut().enumerate().take(candidate_count) {
    let provider = usize::from(storage.candidates[index].load(Ordering::Relaxed));
    let mut batches_ticks = [0; PROBE_BATCHES];
    for (batch, sample) in batches_ticks.iter_mut().enumerate() {
      *sample = storage.samples[index * PROBE_BATCHES + batch].load(Ordering::Relaxed);
    }
    *candidate = CandidateProbeEvidence {
      name: provider_name::<ORDERED>(provider),
      batches_ticks,
      median_ticks: median(batches_ticks),
    };
  }
  let forced = storage.forced_absolute_fallback.load(Ordering::Relaxed);
  SelectionEvidence {
    ready: true,
    user_timebase_mode: storage.mode.load(Ordering::Relaxed),
    continuous_hwclock: storage.continuous_hwclock.load(Ordering::Relaxed),
    reads_per_batch: PROBE_READS,
    candidate_count,
    candidates,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
    measured_winner: provider_name::<ORDERED>(usize::from(
      storage.measured_winner.load(Ordering::Relaxed),
    )),
    selected_provider: provider_name::<ORDERED>(usize::from(
      storage.selected.load(Ordering::Relaxed),
    )),
    selection_basis: if forced {
      "same_thread_reentry_or_fork_safe_absolute_fallback"
    } else {
      "runtime_measured_complete_public_path"
    },
  }
}

#[cfg(any(feature = "bench-internal", test))]
const fn provider_name<const ORDERED: bool>(provider: usize) -> &'static str {
  match provider {
    PROVIDER_MACH_ABSOLUTE => "apple_mach_absolute_time",
    PROVIDER_ABSOLUTE_CNTVCT if ORDERED => "apple_commpage_isb_cntvct_offset",
    PROVIDER_ABSOLUTE_CNTVCT => "apple_commpage_cntvct_offset",
    PROVIDER_ABSOLUTE_CNTVCTSS => "apple_commpage_cntvctss_offset",
    PROVIDER_ABSOLUTE_ACNTVCT => "apple_commpage_acntvct_offset",
    PROVIDER_MACH_CONTINUOUS => "apple_mach_continuous_time",
    PROVIDER_CONTINUOUS_CNTVCT if ORDERED => "apple_continuous_hw_isb_cntvct_base",
    PROVIDER_CONTINUOUS_CNTVCT => "apple_continuous_hw_cntvct_base",
    PROVIDER_CONTINUOUS_CNTVCTSS => "apple_continuous_hw_cntvctss_base",
    PROVIDER_CONTINUOUS_ACNTVCT => "apple_continuous_hw_acntvct_base",
    _ => "unavailable",
  }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_provider() -> &'static str {
  provider_name::<false>(selected_provider::<false>(&INSTANT_SELECTOR))
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_provider() -> &'static str {
  provider_name::<true>(selected_provider::<true>(&ORDERED_SELECTOR))
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // Consumed by the benchmark serializer outside this module.
pub(crate) fn bench_instant_evidence() -> SelectionEvidence {
  selection_evidence::<false>(&INSTANT_SELECTOR)
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // Consumed by the benchmark serializer outside this module.
pub(crate) fn bench_ordered_evidence() -> SelectionEvidence {
  selection_evidence::<true>(&ORDERED_SELECTOR)
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy)]
#[allow(dead_code)] // Consumed by the benchmark harness outside this module.
pub(crate) struct BenchPrimitive {
  pub(crate) name: &'static str,
  pub(crate) read: fn() -> u64,
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness selects once before its timing loop.
pub(crate) fn bench_selected_instant_primitive() -> BenchPrimitive {
  instant_bench_primitive(selected_provider::<false>(&INSTANT_SELECTOR))
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness selects once before its timing loop.
pub(crate) fn bench_selected_ordered_primitive() -> BenchPrimitive {
  ordered_bench_primitive(selected_provider::<true>(&ORDERED_SELECTOR))
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness iterates the complete candidate set.
pub(crate) fn bench_instant_candidate_primitives()
-> ([Option<BenchPrimitive>; MAX_CANDIDATES], usize) {
  let candidate_list = candidates(user_timebase_mode(), continuous_hwclock_available());
  let mut primitives = [None; MAX_CANDIDATES];
  for (slot, provider) in primitives.iter_mut().zip(candidate_list.as_slice()) {
    *slot = Some(instant_bench_primitive(*provider));
  }
  (primitives, candidate_list.count)
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness iterates the complete candidate set.
pub(crate) fn bench_ordered_candidate_primitives()
-> ([Option<BenchPrimitive>; MAX_CANDIDATES], usize) {
  let candidate_list = candidates(user_timebase_mode(), continuous_hwclock_available());
  let mut primitives = [None; MAX_CANDIDATES];
  for (slot, provider) in primitives.iter_mut().zip(candidate_list.as_slice()) {
    *slot = Some(ordered_bench_primitive(*provider));
  }
  (primitives, candidate_list.count)
}

#[cfg(feature = "bench-internal")]
fn instant_bench_primitive(provider: usize) -> BenchPrimitive {
  let read = match provider {
    PROVIDER_ABSOLUTE_CNTVCT => cntvct_absolute_time as fn() -> u64,
    PROVIDER_ABSOLUTE_CNTVCTSS => cntvctss_absolute_time,
    PROVIDER_ABSOLUTE_ACNTVCT => acntvct_absolute_time,
    PROVIDER_MACH_CONTINUOUS => mach_continuous,
    PROVIDER_CONTINUOUS_CNTVCT => cntvct_continuous_time,
    PROVIDER_CONTINUOUS_CNTVCTSS => cntvctss_continuous_time,
    PROVIDER_CONTINUOUS_ACNTVCT => acntvct_continuous_time,
    _ => mach_absolute,
  };
  BenchPrimitive { name: provider_name::<false>(provider), read }
}

#[cfg(feature = "bench-internal")]
fn ordered_bench_primitive(provider: usize) -> BenchPrimitive {
  let read = match provider {
    PROVIDER_ABSOLUTE_CNTVCT => cntvct_ordered_absolute_time as fn() -> u64,
    PROVIDER_ABSOLUTE_CNTVCTSS => cntvctss_absolute_time,
    PROVIDER_ABSOLUTE_ACNTVCT => acntvct_absolute_time,
    PROVIDER_MACH_CONTINUOUS => mach_continuous,
    PROVIDER_CONTINUOUS_CNTVCT => cntvct_ordered_continuous_time,
    PROVIDER_CONTINUOUS_CNTVCTSS => cntvctss_continuous_time,
    PROVIDER_CONTINUOUS_ACNTVCT => acntvct_continuous_time,
    _ => mach_absolute,
  };
  BenchPrimitive { name: provider_name::<true>(provider), read }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_mode() -> u8 {
  user_timebase_mode()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_ticks_for_mode(mode: u8) -> u64 {
  match absolute_direct_provider(mode) {
    Some(provider) => read_ordered_provider(provider),
    None => mach_absolute(),
  }
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_instant_ticks_for_mode(mode: u8) -> u64 {
  match absolute_direct_provider(mode) {
    Some(provider) => read_instant_provider(provider),
    None => mach_absolute(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn candidate_sets_are_complete_and_unique() {
    for mode in [
      USER_TIMEBASE_NONE,
      USER_TIMEBASE_SPEC,
      USER_TIMEBASE_NOSPEC,
      USER_TIMEBASE_NOSPEC_APPLE,
      u8::MAX,
    ] {
      for continuous in [false, true] {
        let list = candidates(mode, continuous);
        assert!(list.as_slice().contains(&PROVIDER_MACH_ABSOLUTE));
        assert!(list.as_slice().contains(&PROVIDER_MACH_CONTINUOUS));
        assert_eq!(
          list.as_slice().iter().filter(|provider| is_continuous(**provider)).count(),
          if continuous { 2 } else { 1 }
        );
        for (index, provider) in list.as_slice().iter().enumerate() {
          assert!(!list.as_slice()[index + 1..].contains(provider));
        }
      }
    }
  }

  #[test]
  fn selection_requires_a_repeatable_material_win() {
    let incumbent = [100_000; PROBE_BATCHES];
    assert!(evaluate_challenger([90_000; PROBE_BATCHES], incumbent, 1).challenger_selected);
    assert!(!evaluate_challenger([96_000; PROBE_BATCHES], incumbent, 1).challenger_selected);
    let mut noisy = [90_000; PROBE_BATCHES];
    noisy[0] = 100_000;
    noisy[1] = 100_000;
    assert!(!evaluate_challenger(noisy, incumbent, 1).challenger_selected);
  }

  #[test]
  fn same_thread_reentry_and_fork_choose_the_sticky_absolute_domain() {
    let owner = selecting_state(0x1234);
    assert_eq!(
      selecting_disposition(owner, 7, owner, 7),
      SelectingDisposition::UseAbsoluteFallback
    );
    assert_eq!(
      selecting_disposition(owner, 7, selecting_state(0x5678), 8),
      SelectingDisposition::UseAbsoluteFallback
    );
    assert_eq!(
      selecting_disposition(owner, 7, selecting_state(0x5678), 7),
      SelectingDisposition::Wait
    );
  }

  #[test]
  fn ordered_unordered_reads_remain_in_the_ordered_domain() {
    let provider = selected_provider::<true>(&ORDERED_SELECTOR);
    assert_eq!(is_continuous(provider), is_continuous(provider_or_absolute(provider)));
    let before = read_ordered_provider(provider);
    let unordered = ticks_ordered_unordered();
    let after = read_ordered_provider(provider);
    assert!(before <= unordered && unordered <= after);
  }

  #[test]
  fn direct_protocols_share_their_xnu_timelines() {
    let mode = user_timebase_mode();
    if let Some(provider) = absolute_direct_provider(mode) {
      for _ in 0..10_000 {
        let before = mach_absolute();
        let direct = read_ordered_provider(provider);
        let after = mach_absolute();
        assert!(before <= direct && direct <= after);
      }
    }
    if continuous_hwclock_available() {
      let provider = continuous_direct_provider(mode);
      for _ in 0..10_000 {
        let before = mach_continuous();
        let direct = read_ordered_provider(provider);
        let after = mach_continuous();
        assert!(before <= direct && direct <= after);
      }
    }
  }

  #[test]
  fn selected_protocols_are_monotonic_and_evidence_is_complete() {
    let mut instant = ticks();
    let mut ordered = ticks_ordered();
    for _ in 0..100_000 {
      let next_instant = ticks();
      let next_ordered = ticks_ordered();
      assert!(next_instant >= instant);
      assert!(next_ordered >= ordered);
      instant = next_instant;
      ordered = next_ordered;
    }

    for evidence in [
      selection_evidence::<false>(&INSTANT_SELECTOR),
      selection_evidence::<true>(&ORDERED_SELECTOR),
    ] {
      assert!(evidence.ready);
      assert!((2..=MAX_CANDIDATES).contains(&evidence.candidate_count));
      assert!(
        evidence.candidates[..evidence.candidate_count]
          .iter()
          .all(|candidate| candidate.median_ticks > 0)
      );
    }
  }

  #[test]
  fn selected_providers_survive_fork() {
    let instant_before = ticks();
    let ordered_before = ticks_ordered();
    // SAFETY: the child performs only clock reads and `_exit`; the parent waits for it.
    let child = unsafe { libc::fork() };
    assert!(child >= 0);
    if child == 0 {
      let ok = ticks() >= instant_before && ticks_ordered() >= ordered_before;
      // SAFETY: `_exit` terminates the child without inherited Rust cleanup.
      unsafe { libc::_exit(if ok { 0 } else { 1 }) };
    }

    let mut status = 0;
    // SAFETY: `status` is writable and `child` identifies this process's live child.
    assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
    assert_eq!(status, 0);
  }

  const fn is_continuous(provider: usize) -> bool {
    provider >= PROVIDER_MACH_CONTINUOUS && provider <= PROVIDER_CONTINUOUS_ACNTVCT
  }
}
