//! Linux-kernel perf task-clock provider for current-thread CPU time.
//!
//! The persistent event is thread-bound, so both its owner and the hot pointer
//! live in native TLS. The hot read copies only the pointer out of the
//! `LocalKey`; the mmap seqlock protocol stays outside the closure so LLVM can
//! inline it into the caller.

use core::cell::{Cell, RefCell};
use core::ffi::c_void;
use core::hint::black_box;
use core::mem::{self, MaybeUninit};
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicU8, AtomicU64, Ordering, fence};

use std::sync::OnceLock;

use crate::ThreadCpuProvider;

const PERF_TYPE_SOFTWARE: u32 = 1;
const PERF_COUNT_SW_TASK_CLOCK: u64 = 1;
const PERF_FLAG_FD_CLOEXEC: libc::c_ulong = 1 << 3;
const CAP_USER_TIME: u64 = 1 << 3;
const CAP_USER_TIME_SHORT: u64 = 1 << 5;

const SYSCALL_TAG: usize = 1;
const INITIALIZING_TAG: usize = 2;
const INHERITED_STALE_TAG: usize = 3;
const COMMITTING_TAG: usize = 4;
const MAX_SENTINEL_TAG: usize = COMMITTING_TAG;
const PATH_MMAP: u8 = 1;
const PATH_READ: u8 = 2;
const PATH_POSIX: u8 = 3;
const PATH_OBSOLETE: u8 = 4;
#[cfg(any(target_arch = "x86", target_arch = "powerpc64"))]
const READ_ENTRY_RAW: u8 = 1;
#[cfg(target_arch = "powerpc64")]
const READ_ENTRY_PPC_SCV: u8 = 2;
#[cfg(target_arch = "x86")]
const READ_ENTRY_LIBC_SYSCALL: u8 = 2;
#[cfg(any(target_arch = "x86", target_arch = "powerpc64"))]
const READ_ENTRY_LIBC_READ: u8 = 3;
const MAX_SEQ_RETRIES: usize = 16;
const FALLBACK_UNSET: u64 = u64::MAX;
const FALLBACK_WALL: u64 = u64::MAX - 1;

const WARMUP_READS: usize = 128;
const MEASURE_READS: usize = 4_096;
const MEASURE_SAMPLES: usize = 9;
const REQUIRED_DECISIVE_WINS: usize = 8;

#[cfg(test)]
const TEST_FORK_DURING_COMMIT: u8 = 1;
#[cfg(test)]
const TEST_FORK_DURING_HOT_READ: u8 = 2;
#[cfg(test)]
static TEST_FORK_POINT: AtomicU8 = AtomicU8::new(0);
#[cfg(test)]
static TEST_FORK_TID: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);

#[cfg(target_arch = "aarch64")]
const PERF_COUNTER_AARCH64_ISB: u8 = 1;
#[cfg(target_arch = "aarch64")]
const PERF_COUNTER_AARCH64_SELF_SYNC: u8 = 2;
#[cfg(target_arch = "arm")]
const PERF_COUNTER_ARM_ISB: u8 = 1;

std::thread_local! {
  // Null means uninitialized, 1 means sticky syscall, 2 means initialization
  // is in progress, 3 means an inherited mapping needs child-side refresh, 4
  // means measured-path selection is being committed, and any larger address
  // points at an owner-held PerfState.
  static HOT_STATE: Cell<*const PerfState> = const { Cell::new(ptr::null()) };
  // The commit sentinel keeps nested signal reads off a path that the outer
  // initializer has not published yet. This separate pointer lets those
  // nested reads advance the owner-held monotonic floor without adding a TLS
  // access to the steady-state hot path.
  static COMMIT_STATE: Cell<*const PerfState> = const { Cell::new(ptr::null()) };
  static OWNER: RefCell<ThreadOwner> = const {
    RefCell::new(ThreadOwner { state: None, retired: None })
  };
}

static ATFORK_REGISTERED: OnceLock<bool> = OnceLock::new();

#[repr(C)]
#[derive(Default)]
struct PerfEventAttrV0 {
  type_: u32,
  size: u32,
  config: u64,
  sample_period: u64,
  sample_type: u64,
  read_format: u64,
  flags: u64,
  wakeup_events: u32,
  bp_type: u32,
  bp_addr: u64,
}

#[repr(C)]
struct PerfEventMmapPage {
  version: u32,
  compat_version: u32,
  lock: u32,
  index: u32,
  offset: i64,
  time_enabled: u64,
  time_running: u64,
  capabilities: u64,
  pmc_width: u16,
  time_shift: u16,
  time_mult: u32,
  time_offset: u64,
  time_zero: u64,
  size: u32,
  reserved_1: u32,
  time_cycles: u64,
  time_mask: u64,
}

struct PerfState {
  retired_next: Cell<Option<NonNull<PerfState>>>,
  owner_pid: libc::pid_t,
  fd: libc::c_int,
  page: Option<NonNull<PerfEventMmapPage>>,
  map_len: usize,
  mmap_epoch_offset: u64,
  read_epoch_offset: u64,
  short_period_nanos: Option<u64>,
  max_read_gap_nanos: Option<u64>,
  last_mmap_nanos: AtomicU64,
  last_event_nanos: AtomicU64,
  fallback_bias: AtomicU64,
  mmap_transition_bias: AtomicU64,
  read_transition_bias: AtomicU64,
  selected_path: AtomicU8,
  fallback_path: AtomicU8,
  failed_paths: AtomicU8,
  #[cfg(any(target_arch = "x86", target_arch = "powerpc64"))]
  read_entry: Cell<u8>,
  counter_kind: Cell<u8>,
  #[cfg(feature = "bench-internal")]
  path_measurements: Cell<Option<PerfPathMeasurements>>,
  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "powerpc64"),))]
  read_entry_measurements: Cell<Option<PerfReadEntryMeasurements>>,
  #[cfg(all(
    feature = "bench-internal",
    any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"),
  ))]
  counter_evidence: Option<PerfCounterSelectionEvidence>,
}

#[cfg(all(
  feature = "bench-internal",
  any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "arm",
    target_arch = "riscv64",
  ),
))]
pub(super) struct BenchPerfHandle {
  state: *const PerfState,
  previous_hot: *const PerfState,
  previous_path: u8,
  selected_counter: Option<u8>,
}

#[cfg(all(
  feature = "bench-internal",
  any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"),
))]
pub(super) struct BenchPerfCounterEvidence {
  pub(super) candidate_count: usize,
  pub(super) candidate_names: [&'static str; 5],
  pub(super) candidate_eligible: [bool; 5],
  pub(super) candidate_batches_ns: [[u64; MEASURE_SAMPLES]; 5],
  pub(super) selected_candidate: &'static str,
  pub(super) reads_per_batch: usize,
  pub(super) required_decisive_wins: usize,
}

struct ThreadOwner {
  state: Option<NonNull<PerfState>>,
  retired: Option<NonNull<PerfState>>,
}

impl ThreadOwner {
  #[cfg(feature = "bench-internal")]
  #[inline]
  fn current(&self) -> Option<&PerfState> {
    // SAFETY: every current pointer is allocated and owned by this
    // ThreadOwner until it is explicitly retired or destroyed.
    self.state.map(|state| unsafe { state.as_ref() })
  }

  #[inline]
  fn current_ptr(&self) -> Option<*const PerfState> {
    self.state.map(NonNull::as_ptr).map(|state| state.cast_const())
  }
}

#[derive(Clone, Copy)]
struct TaskClockSnapshot {
  index: u32,
  enabled: u64,
  running: u64,
  capabilities: u64,
  time_shift: u16,
  time_mult: u32,
  time_offset: u64,
  time_cycles: u64,
  time_mask: u64,
  cycle: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShortCounterDecision {
  Publish(u64),
  Retry,
  Invalid,
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
#[derive(Clone, Copy)]
struct PerfCounterSelectionEvidence {
  #[cfg(feature = "bench-internal")]
  eligible: [bool; 5],
  #[cfg(feature = "bench-internal")]
  samples: [[u64; MEASURE_SAMPLES]; 5],
  selected: u8,
}

#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", target_arch = "aarch64")),
))]
#[derive(Clone, Copy)]
struct PerfPathMeasurements {
  mmap: Option<[u64; MEASURE_SAMPLES]>,
  read: [u64; MEASURE_SAMPLES],
  syscall: [u64; MEASURE_SAMPLES],
}

#[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "powerpc64"),))]
#[derive(Clone, Copy)]
struct PerfReadEntryMeasurements {
  eligible: [bool; 3],
  samples: [[u64; MEASURE_SAMPLES]; 3],
  selected: u8,
}

#[cfg(feature = "bench-internal")]
pub(super) struct BenchPerfPathEvidence {
  pub(super) mmap_batches_ns: Option<[u64; MEASURE_SAMPLES]>,
  pub(super) read_batches_ns: [u64; MEASURE_SAMPLES],
  pub(super) posix_batches_ns: [u64; MEASURE_SAMPLES],
  pub(super) selected_path: &'static str,
  pub(super) fallback_path: &'static str,
  pub(super) reads_per_batch: usize,
  pub(super) required_decisive_wins: usize,
}

#[cfg(feature = "bench-internal")]
pub(super) struct BenchPerfReadEntryEvidence {
  pub(super) candidate_count: usize,
  pub(super) candidate_names: [&'static str; 3],
  pub(super) candidate_eligible: [bool; 3],
  pub(super) candidate_measured: [bool; 3],
  pub(super) candidate_batches_ns: [[u64; MEASURE_SAMPLES]; 3],
  pub(super) selected_candidate: &'static str,
  pub(super) reads_per_batch: usize,
  pub(super) required_decisive_wins: usize,
}

#[cfg(feature = "bench-internal")]
pub(super) struct BenchPerfReadHandle {
  state: *const PerfState,
  previous_hot: *const PerfState,
  previous_path: u8,
  previous_entry: u8,
}

#[cfg(feature = "bench-internal")]
pub(super) struct BenchPerfPathHandle {
  state: *const PerfState,
  previous_hot: *const PerfState,
  previous_path: u8,
  forced_path: Cell<u8>,
}

impl PerfState {
  fn open() -> Option<Self> {
    if !atfork_registered() {
      return None;
    }
    // Capture before opening the thread-bound event. A fork from a nested
    // signal after this point makes the stored pid stale, even if the outer
    // initializer resumes and overwrites the callback's HOT_STATE sentinel.
    // SAFETY: getpid has no caller-side preconditions.
    let owner_pid = unsafe { libc::getpid() };

    let attr = PerfEventAttrV0 {
      type_: PERF_TYPE_SOFTWARE,
      size: mem::size_of::<PerfEventAttrV0>().try_into().ok()?,
      config: PERF_COUNT_SW_TASK_CLOCK,
      ..PerfEventAttrV0::default()
    };

    // SAFETY: this is the documented perf_event_open ABI. pid=0 selects the
    // calling thread, cpu=-1 follows it across CPUs, and group_fd=-1 creates a
    // standalone event. The V0 attribute prefix is accepted by every kernel
    // that implements perf_event_open.
    let fd = unsafe {
      libc::syscall(
        libc::SYS_perf_event_open,
        ptr::addr_of!(attr),
        0 as libc::pid_t,
        -1 as libc::c_int,
        -1 as libc::c_int,
        PERF_FLAG_FD_CLOEXEC,
      )
    };
    let Ok(fd) = usize::try_from(fd) else {
      return None;
    };
    let Ok(fd) = libc::c_int::try_from(fd) else {
      return None;
    };

    let mut state = Self {
      retired_next: Cell::new(None),
      owner_pid,
      fd,
      page: None,
      map_len: 0,
      mmap_epoch_offset: 0,
      read_epoch_offset: 0,
      short_period_nanos: None,
      max_read_gap_nanos: None,
      last_mmap_nanos: AtomicU64::new(0),
      last_event_nanos: AtomicU64::new(0),
      fallback_bias: AtomicU64::new(FALLBACK_UNSET),
      mmap_transition_bias: AtomicU64::new(0),
      read_transition_bias: AtomicU64::new(0),
      selected_path: AtomicU8::new(PATH_READ),
      fallback_path: AtomicU8::new(PATH_POSIX),
      failed_paths: AtomicU8::new(0),
      #[cfg(any(target_arch = "x86", target_arch = "powerpc64"))]
      read_entry: Cell::new(READ_ENTRY_RAW),
      counter_kind: Cell::new(default_perf_counter_kind()),
      #[cfg(feature = "bench-internal")]
      path_measurements: Cell::new(None),
      #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "powerpc64"),))]
      read_entry_measurements: Cell::new(None),
      #[cfg(all(
        feature = "bench-internal",
        any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"),
      ))]
      counter_evidence: None,
    };

    // A task-clock fd read is the architecture-independent perf tier. Its
    // count is nanoseconds of scheduled execution since the event opened.
    let syscall_nanos = super::posix_now_nanos();
    if super::native_clock_uses_wall() {
      return None;
    }
    let read_count = state.read_fd_count()?;
    state.read_epoch_offset = align_event_epoch(syscall_nanos, read_count)?;
    let first_read = read_count.checked_add(state.read_epoch_offset)?;
    state.last_event_nanos.store(first_read, Ordering::Relaxed);
    select_perf_read_entry(&state)?;

    // The mmap tier is optional. Only architectures whose kernel publishes a
    // usable cap_user_time conversion and whose counter instruction is safe
    // may enter it. A failed mmap probe leaves the fd-read candidate intact.
    if mmap_counter_execution_eligible()
      && let Some((page, map_len)) = map_perf_page(fd)
    {
      state.page = Some(page);
      state.map_len = map_len;
      let mmap_ready = state.initialize_mmap(syscall_nanos).is_some();
      if !mmap_ready {
        state.disable_mmap();
      }
    }

    state.read_path(PATH_READ)?;
    Some(state)
  }

  #[cfg(test)]
  #[inline]
  fn read(&self) -> Option<u64> {
    self.read_path(self.selected_path.load(Ordering::Relaxed))
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn read_path(&self, path: u8) -> Option<u64> {
    match path {
      PATH_MMAP => self.read_mmap(),
      PATH_READ => self.read_fd_nanos(),
      PATH_POSIX => Some(self.read_posix_fallback()),
      _ => None,
    }
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn read_mmap(&self) -> Option<u64> {
    let aligned = self.read_mmap_aligned()?;
    let candidate = aligned.checked_add(self.mmap_transition_bias.load(Ordering::Relaxed))?;
    Some(self.publish_at_least(candidate))
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn read_mmap_aligned(&self) -> Option<u64> {
    for _ in 0..MAX_SEQ_RETRIES {
      let snapshot = self.read_snapshot()?;
      let event_nanos = task_clock_nanos(snapshot)?;
      let candidate = event_nanos.checked_add(self.mmap_epoch_offset)?;
      let period = task_clock_short_period_nanos(snapshot)?;
      if period != self.short_period_nanos {
        return None;
      }

      let last = self.last_mmap_nanos.load(Ordering::Relaxed);
      let candidate = if let Some(period) = period {
        match extend_short_candidate(candidate, last, period) {
          ShortCounterDecision::Publish(value) => value,
          ShortCounterDecision::Retry => continue,
          ShortCounterDecision::Invalid => return None,
        }
      } else {
        candidate.max(last)
      };
      if self
        .last_mmap_nanos
        .compare_exchange(last, candidate, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
      {
        return Some(candidate);
      }
      // A signal can complete a nested read after this read captured its raw
      // counter. Reacquire the complete perf snapshot before comparing again.
    }
    None
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn read_fd_nanos(&self) -> Option<u64> {
    let candidate = self
      .read_fd_aligned()?
      .checked_add(self.read_transition_bias.load(Ordering::Relaxed))?;
    Some(self.publish_at_least(candidate))
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn read_fd_aligned(&self) -> Option<u64> {
    self.read_fd_count()?.checked_add(self.read_epoch_offset)
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn read_fd_count(&self) -> Option<u64> {
    for _ in 0..MAX_SEQ_RETRIES {
      let mut count = MaybeUninit::<u64>::uninit();
      // `fd` is this thread's live standalone task-clock event and the output
      // points to exactly one writable u64, the default perf read format. A
      // bounded retry tolerates an interrupt without permanently losing an
      // otherwise healthy tier.
      let read = perf_read_once(self.fd, count.as_mut_ptr(), self.perf_read_entry());
      if read == mem::size_of::<u64>() as libc::ssize_t {
        // SAFETY: a full successful perf read initialized the output u64.
        return Some(unsafe { count.assume_init() });
      }
    }
    None
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn perf_read_entry(&self) -> u8 {
    #[cfg(any(target_arch = "x86", target_arch = "powerpc64"))]
    {
      self.read_entry.get()
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "powerpc64")))]
    {
      0
    }
  }

  #[cfg(feature = "bench-internal")]
  #[inline]
  fn set_perf_read_entry(&self, entry: u8) {
    #[cfg(any(target_arch = "x86", target_arch = "powerpc64"))]
    self.read_entry.set(entry);
    #[cfg(not(any(target_arch = "x86", target_arch = "powerpc64")))]
    let _ = entry;
  }

  fn initialize_mmap(&mut self, syscall_nanos: u64) -> Option<()> {
    let snapshot = self.read_snapshot()?;
    let event_nanos = task_clock_nanos(snapshot)?;
    self.short_period_nanos = task_clock_short_period_nanos(snapshot)?;
    self.max_read_gap_nanos = task_clock_max_read_gap_nanos(self.short_period_nanos)?;
    self.mmap_epoch_offset = align_event_epoch(syscall_nanos, event_nanos)?;
    let aligned = event_nanos.checked_add(self.mmap_epoch_offset)?;
    self.last_mmap_nanos.store(aligned, Ordering::Relaxed);
    self.last_event_nanos.fetch_max(aligned, Ordering::Relaxed);
    select_perf_counter(self)?;
    self.read_mmap()?;
    Some(())
  }

  fn disable_mmap(&mut self) {
    let Some(page) = self.page.take() else {
      return;
    };
    // SAFETY: this state uniquely owns the successful metadata mapping.
    unsafe {
      libc::munmap(page.as_ptr().cast::<c_void>(), self.map_len);
    }
    self.map_len = 0;
    self.short_period_nanos = None;
    self.max_read_gap_nanos = None;
  }

  #[inline]
  fn mmap_available(&self) -> bool {
    self.page.is_some()
  }

  #[cfg(any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "arm",
  ))]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn read_event_nanos(&self) -> Option<u64> {
    task_clock_nanos(self.read_snapshot()?)
  }

  #[inline]
  fn fallback_marker(&self) -> u64 {
    self.fallback_bias.load(Ordering::Acquire)
  }

  #[cold]
  fn retier_after_failure(&self, failed: u8) -> u64 {
    self.mark_path_failed(failed);
    let fallback = self.fallback_path.swap(PATH_POSIX, Ordering::AcqRel);
    if matches!(fallback, PATH_MMAP | PATH_READ)
      && fallback != failed
      && let Some(value) = self.prepare_transition(fallback)
    {
      match self.selected_path.compare_exchange(
        failed,
        fallback,
        Ordering::AcqRel,
        Ordering::Acquire,
      ) {
        Ok(_) => return self.publish_at_least(value),
        Err(selected) => return self.read_selected_after_race(selected),
      }
    }
    self.degrade_to_posix(failed)
  }

  #[inline]
  fn mark_path_failed(&self, path: u8) {
    let mask = path_mask(path);
    if mask != 0 {
      self.failed_paths.fetch_or(mask, Ordering::AcqRel);
    }
  }

  #[cold]
  fn prepare_transition(&self, path: u8) -> Option<u64> {
    let raw = match path {
      PATH_MMAP => self.read_mmap_aligned()?,
      PATH_READ => self.read_fd_aligned()?,
      _ => return None,
    };
    let last = self.last_event_nanos.load(Ordering::Relaxed);
    let proposed_bias = last.saturating_sub(raw);
    let bias = match path {
      PATH_MMAP => self.mmap_transition_bias.fetch_max(proposed_bias, Ordering::AcqRel),
      PATH_READ => self.read_transition_bias.fetch_max(proposed_bias, Ordering::AcqRel),
      _ => 0,
    }
    .max(proposed_bias);
    raw.checked_add(bias)
  }

  #[inline]
  fn read_selected_after_race(&self, selected: u8) -> u64 {
    match selected {
      PATH_MMAP | PATH_READ => {
        self.read_path(selected).unwrap_or_else(|| self.retier_after_failure(selected))
      }
      PATH_POSIX => self.read_posix_fallback(),
      PATH_OBSOLETE => resume_current_after_obsolete(self),
      _ => self.degrade_to_posix(selected),
    }
  }

  #[cold]
  fn degrade_to_posix(&self, failed: u8) -> u64 {
    let sample = super::posix_now_nanos();
    let last = self.last_event_nanos.load(Ordering::Relaxed);
    let proposed = fallback_marker_for_sample(last, sample);
    let marker = match self.fallback_bias.compare_exchange(
      FALLBACK_UNSET,
      proposed,
      Ordering::AcqRel,
      Ordering::Acquire,
    ) {
      Ok(_) => proposed,
      Err(selected) => selected,
    };
    match self.selected_path.compare_exchange(
      failed,
      PATH_POSIX,
      Ordering::AcqRel,
      Ordering::Acquire,
    ) {
      Ok(_) | Err(PATH_POSIX) => self.apply_fallback(marker, sample),
      Err(selected) => {
        // A nested signal selected another exact path while this reader was
        // preparing the fallback. The marker belongs only to a published
        // POSIX transition, so do not let this losing attempt affect the
        // still-active perf route or a later independent failure.
        let _ = self.fallback_bias.compare_exchange(
          marker,
          FALLBACK_UNSET,
          Ordering::AcqRel,
          Ordering::Acquire,
        );
        self.read_selected_after_race(selected)
      }
    }
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn read_posix_fallback(&self) -> u64 {
    let marker = self.fallback_marker();
    debug_assert_ne!(marker, FALLBACK_UNSET);
    self.apply_fallback(marker, super::posix_now_nanos())
  }

  #[inline]
  fn apply_fallback(&self, marker: u64, sample: u64) -> u64 {
    if marker == FALLBACK_WALL || crate::thread_cpu::is_wall_value(sample) {
      return if crate::thread_cpu::is_wall_value(sample) {
        sample
      } else {
        super::wall_now_value()
      };
    }

    let candidate =
      biased_fallback_candidate(sample, marker, self.last_event_nanos.load(Ordering::Relaxed));
    self.publish_at_least(candidate)
  }

  #[inline]
  fn publish_at_least(&self, candidate: u64) -> u64 {
    let mut last = self.last_event_nanos.load(Ordering::Relaxed);
    loop {
      let published = candidate.max(last);
      match self.last_event_nanos.compare_exchange_weak(
        last,
        published,
        Ordering::Relaxed,
        Ordering::Relaxed,
      ) {
        Ok(_) => return published,
        Err(observed) => last = observed,
      }
    }
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn read_snapshot(&self) -> Option<TaskClockSnapshot> {
    let page = self.page?.as_ptr();

    for _ in 0..MAX_SEQ_RETRIES {
      // SAFETY: `page` points to the live, read-only perf metadata mapping.
      let sequence = unsafe { ptr::read_volatile(ptr::addr_of!((*page).lock)) };
      if sequence & 1 != 0 {
        core::hint::spin_loop();
        continue;
      }
      fence(Ordering::Acquire);

      // SAFETY: each volatile access is within the same live metadata page.
      let index = unsafe { ptr::read_volatile(ptr::addr_of!((*page).index)) };
      // SAFETY: see the safety note on `index`.
      let enabled = unsafe { ptr::read_volatile(ptr::addr_of!((*page).time_enabled)) };
      // SAFETY: see the safety note on `index`.
      let running = unsafe { ptr::read_volatile(ptr::addr_of!((*page).time_running)) };
      // SAFETY: see the safety note on `index`.
      let capabilities = unsafe { ptr::read_volatile(ptr::addr_of!((*page).capabilities)) };
      // SAFETY: see the safety note on `index`.
      let time_shift = unsafe { ptr::read_volatile(ptr::addr_of!((*page).time_shift)) };
      // SAFETY: see the safety note on `index`.
      let time_mult = unsafe { ptr::read_volatile(ptr::addr_of!((*page).time_mult)) };
      // SAFETY: see the safety note on `index`.
      let time_offset = unsafe { ptr::read_volatile(ptr::addr_of!((*page).time_offset)) };
      let (time_cycles, time_mask) = if capabilities & CAP_USER_TIME_SHORT != 0 {
        // SAFETY: these fields are present in the same ABI page and are used
        // only when the capability bit advertises them.
        unsafe {
          (
            ptr::read_volatile(ptr::addr_of!((*page).time_cycles)),
            ptr::read_volatile(ptr::addr_of!((*page).time_mask)),
          )
        }
      } else {
        (0, u64::MAX)
      };

      #[cfg(target_arch = "arm")]
      if !counter_metadata_eligible(capabilities, time_shift, time_mult) {
        fence(Ordering::Acquire);
        // SAFETY: `page` is still the same live metadata mapping.
        let after = unsafe { ptr::read_volatile(ptr::addr_of!((*page).lock)) };
        if sequence != after || after & 1 != 0 {
          core::hint::spin_loop();
          continue;
        }
        return None;
      }

      let cycle = read_sched_clock_counter(self.counter_kind.get());

      fence(Ordering::Acquire);
      // SAFETY: `page` is still the same live metadata mapping.
      let after = unsafe { ptr::read_volatile(ptr::addr_of!((*page).lock)) };
      if sequence != after || after & 1 != 0 {
        core::hint::spin_loop();
        continue;
      }

      return Some(TaskClockSnapshot {
        index,
        enabled,
        running,
        capabilities,
        time_shift,
        time_mult,
        time_offset,
        time_cycles,
        time_mask,
        cycle,
      });
    }

    None
  }
}

impl Drop for PerfState {
  fn drop(&mut self) {
    if let Some(page) = self.page.take() {
      // SAFETY: this state uniquely owns the mapping returned by mmap.
      unsafe {
        libc::munmap(page.as_ptr().cast::<c_void>(), self.map_len);
      }
    }
    close_fd(self.fd);
  }
}

impl Drop for ThreadOwner {
  fn drop(&mut self) {
    // Publish the syscall path before unmapping so another TLS destructor on
    // this same thread cannot observe a stale metadata pointer.
    set_hot(syscall_ptr());
    set_commit_state(ptr::null());
    if let Some(state) = self.state.take() {
      // SAFETY: ThreadOwner exclusively owns every allocated state pointer.
      unsafe { destroy_perf_state(state) };
    }
    let mut retired = self.retired.take();
    while let Some(state) = retired {
      // Read the interior next link before destroying this allocation.
      // SAFETY: retired states remain allocated and owner-held until here.
      retired = unsafe { state.as_ref().retired_next.get() };
      // SAFETY: ThreadOwner exclusively owns every allocated state pointer.
      unsafe { destroy_perf_state(state) };
    }
  }
}

#[inline(always)]
#[allow(clippy::inline_always, unused_variables)]
fn perf_read_once(fd: libc::c_int, count: *mut u64, entry: u8) -> libc::ssize_t {
  #[cfg(target_arch = "x86_64")]
  {
    let mut result = libc::SYS_read;
    // SAFETY: Linux and Android x86_64 pass the read number in RAX and its
    // arguments in RDI, RSI, and RDX. `count` addresses one writable u64.
    unsafe {
      core::arch::asm!(
        "syscall",
        inlateout("rax") result,
        in("rdi") libc::c_long::from(fd),
        in("rsi") count,
        in("rdx") mem::size_of::<u64>(),
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
      );
    }
    result as libc::ssize_t
  }
  #[cfg(target_arch = "aarch64")]
  {
    let mut result = libc::c_long::from(fd);
    // SAFETY: Linux and Android AArch64 pass the read number in X8 and its
    // arguments in X0-X2. `count` addresses one writable u64.
    unsafe {
      core::arch::asm!(
        "svc 0",
        inlateout("x0") result,
        in("x1") count,
        in("x2") mem::size_of::<u64>(),
        in("x8") libc::SYS_read,
        options(nostack),
      );
    }
    result as libc::ssize_t
  }
  #[cfg(target_arch = "arm")]
  {
    let result: libc::c_long;
    // SAFETY: Linux Arm EABI passes the read number in r7 and arguments in
    // r0-r2. The balanced push/pop preserves LLVM's possible frame pointer;
    // `count` addresses one writable u64.
    unsafe {
      core::arch::asm!(
        "push {{r7}}",
        "mov r7, {number}",
        "svc 0",
        "pop {{r7}}",
        number = in(reg) libc::SYS_read,
        inlateout("r0") fd => result,
        in("r1") count,
        in("r2") mem::size_of::<u64>(),
        options(preserves_flags),
      );
    }
    result as libc::ssize_t
  }
  #[cfg(target_arch = "riscv64")]
  {
    let result: libc::c_long;
    // SAFETY: RV64 Linux passes the read number in a7 and arguments in a0-a2.
    // `count` addresses one writable u64.
    unsafe {
      core::arch::asm!(
        "ecall",
        in("a7") libc::SYS_read,
        inlateout("a0") libc::c_long::from(fd) => result,
        in("a1") count,
        in("a2") mem::size_of::<u64>(),
        options(nostack, preserves_flags),
      );
    }
    result as libc::ssize_t
  }
  #[cfg(target_arch = "loongarch64")]
  {
    let result: libc::c_long;
    // SAFETY: LoongArch Linux passes the read number in A7 and arguments in
    // A0-A2. The declared temporaries are kernel-volatile and `count` is
    // writable u64 storage.
    unsafe {
      core::arch::asm!(
        "syscall 0",
        in("$a7") libc::SYS_read,
        inlateout("$a0") libc::c_long::from(fd) => result,
        in("$a1") count,
        in("$a2") mem::size_of::<u64>(),
        lateout("$t0") _,
        lateout("$t1") _,
        lateout("$t2") _,
        lateout("$t3") _,
        lateout("$t4") _,
        lateout("$t5") _,
        lateout("$t6") _,
        lateout("$t7") _,
        lateout("$t8") _,
        options(nostack, preserves_flags),
      );
    }
    result as libc::ssize_t
  }
  #[cfg(target_arch = "s390x")]
  {
    let result: libc::c_long;
    // SAFETY: read is syscall 3 on s390x Linux, so encoding it directly in SVC
    // avoids the kernel's svc-0 path through gpr1. Arguments are in r2-r4 and
    // `count` addresses one writable u64.
    unsafe {
      core::arch::asm!(
        "svc 3",
        inlateout("r2") libc::c_long::from(fd) => result,
        in("r3") count,
        in("r4") mem::size_of::<u64>(),
        options(nostack, preserves_flags),
      );
    }
    result as libc::ssize_t
  }
  #[cfg(target_arch = "x86")]
  {
    match entry {
      READ_ENTRY_LIBC_SYSCALL => {
        // SAFETY: the generic libc syscall entry owns this process's supported
        // i386 kernel trampoline and `count` addresses one writable u64.
        unsafe { libc::syscall(libc::SYS_read, fd, count, mem::size_of::<u64>()) as libc::ssize_t }
      }
      READ_ENTRY_LIBC_READ => {
        // SAFETY: `fd` is live and `count` addresses one writable u64.
        unsafe { libc::read(fd, count.cast::<c_void>(), mem::size_of::<u64>()) }
      }
      _ => {
        let result: libc::c_long;
        // SAFETY: i386 Linux passes the read number in EAX and arguments in
        // EBX-EDX. The balanced stack operations preserve PIC's reserved EBX;
        // `count` addresses one writable u64.
        unsafe {
          core::arch::asm!(
            "push ebx",
            "mov ebx, {fd:e}",
            "int 0x80",
            "pop ebx",
            fd = in(reg) fd,
            inlateout("eax") libc::SYS_read => result,
            in("ecx") count,
            in("edx") mem::size_of::<u64>(),
          );
        }
        result as libc::ssize_t
      }
    }
  }
  #[cfg(target_arch = "powerpc64")]
  {
    if entry == READ_ENTRY_LIBC_READ {
      // SAFETY: `fd` is live and `count` addresses one writable u64.
      return unsafe { libc::read(fd, count.cast::<c_void>(), mem::size_of::<u64>()) };
    }
    let result: libc::c_long;
    if entry == READ_ENTRY_PPC_SCV {
      // SAFETY: selection executes SCV only after the process HWCAP2 bit proves
      // support. The Linux SCV ABI uses r0 and r3-r8; all volatile registers
      // are declared and `count` is writable u64 storage.
      unsafe {
        core::arch::asm!(
          ".machine push",
          ".machine power9",
          "scv 0",
          ".machine pop",
          inlateout("r0") libc::SYS_read => _,
          inlateout("r3") libc::c_long::from(fd) => result,
          inlateout("r4") count => _,
          inlateout("r5") mem::size_of::<u64>() => _,
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
    } else {
      let condition: libc::c_ulong;
      // SAFETY: PowerPC64 Linux's SC ABI uses r0 and r3-r8. All
      // kernel-volatile registers are declared and `count` is writable u64
      // storage. SC reports errors as a positive errno with CR0.SO set, so MFCR
      // captures that bit before Rust observes r3.
      unsafe {
        core::arch::asm!(
          "sc",
          "mfcr 6",
          inlateout("r0") libc::SYS_read => _,
          inlateout("r3") libc::c_long::from(fd) => result,
          inlateout("r4") count => _,
          inlateout("r5") mem::size_of::<u64>() => _,
          lateout("r6") condition,
          lateout("r7") _,
          lateout("r8") _,
          lateout("r9") _,
          lateout("r10") _,
          lateout("r11") _,
          lateout("r12") _,
          lateout("cr0") _,
          lateout("xer") _,
          lateout("ctr") _,
          options(nostack),
        );
      }
      const CR0_SUMMARY_OVERFLOW: libc::c_ulong = 0x1000_0000;
      if condition & CR0_SUMMARY_OVERFLOW != 0 {
        return result.wrapping_neg() as libc::ssize_t;
      }
    }
    result as libc::ssize_t
  }
}

#[cfg(not(any(target_arch = "x86", target_arch = "powerpc64")))]
fn select_perf_read_entry(_state: &PerfState) -> Option<()> {
  // Linux `read` has no vDSO route on these fixed-entry ABIs. The inlined raw
  // instruction reaches the same kernel implementation while removing the
  // out-of-line libc/cancellation/errno plumbing, so there is no distinct
  // eligible libc mechanism that can perform less work.
  Some(())
}

#[cfg(any(target_arch = "x86", target_arch = "powerpc64"))]
fn select_perf_read_entry(state: &PerfState) -> Option<()> {
  #[cfg(target_arch = "x86")]
  let runtime_entry = READ_ENTRY_LIBC_SYSCALL;
  #[cfg(target_arch = "powerpc64")]
  let runtime_entry = READ_ENTRY_PPC_SCV;
  let candidates = [READ_ENTRY_RAW, runtime_entry, READ_ENTRY_LIBC_READ];
  let mut eligible = [true, true, true];
  #[cfg(target_arch = "powerpc64")]
  {
    eligible[1] = powerpc64_scv_available();
  }

  for (index, candidate) in candidates.iter().copied().enumerate() {
    if !eligible[index] {
      continue;
    }
    state.read_entry.set(candidate);
    for _ in 0..WARMUP_READS {
      if state.read_fd_count().is_none() {
        eligible[index] = false;
        break;
      }
    }
  }

  let mut samples = [[0_u64; MEASURE_SAMPLES]; 3];
  for sample in 0..MEASURE_SAMPLES {
    for offset in 0..candidates.len() {
      let index = (sample + offset) % candidates.len();
      if !eligible[index] {
        continue;
      }
      let Some(elapsed) = measure_perf_read_entry(state, candidates[index]) else {
        eligible[index] = false;
        continue;
      };
      samples[index][sample] = elapsed;
    }
  }

  let mut selected = eligible.iter().position(|value| *value)?;
  for challenger in 0..candidates.len() {
    if challenger != selected
      && eligible[challenger]
      && prefer_perf(samples[challenger], samples[selected])
    {
      selected = challenger;
    }
  }
  let selected = candidates[selected];
  state.read_entry.set(selected);
  #[cfg(feature = "bench-internal")]
  state.read_entry_measurements.set(Some(PerfReadEntryMeasurements {
    eligible,
    samples,
    selected,
  }));
  Some(())
}

#[cfg(any(target_arch = "x86", target_arch = "powerpc64"))]
fn measure_perf_read_entry(state: &PerfState, entry: u8) -> Option<u64> {
  state.read_entry.set(entry);
  let start = monotonic_raw_nanos()?;
  for _ in 0..MEASURE_READS {
    black_box(state.read_fd_count()?);
  }
  monotonic_raw_nanos()?.checked_sub(start)
}

#[cfg(target_arch = "powerpc64")]
fn powerpc64_scv_available() -> bool {
  const AT_HWCAP2: libc::c_ulong = 26;
  const PPC_FEATURE2_SCV: libc::c_ulong = 0x0010_0000;
  // SAFETY: getauxval reads immutable process startup metadata.
  unsafe { libc::getauxval(AT_HWCAP2) & PPC_FEATURE2_SCV != 0 }
}

#[cfg(feature = "bench-internal")]
const fn perf_path_name(path: u8) -> &'static str {
  match path {
    PATH_MMAP => "linux_perf_mmap",
    PATH_READ => "linux_perf_read",
    PATH_POSIX => "posix_thread_cpu",
    _ => "unknown",
  }
}

#[cfg(all(feature = "bench-internal", target_arch = "x86"))]
const fn perf_read_entry_candidates() -> [u8; 3] {
  [READ_ENTRY_RAW, READ_ENTRY_LIBC_SYSCALL, READ_ENTRY_LIBC_READ]
}

#[cfg(all(feature = "bench-internal", target_arch = "powerpc64"))]
const fn perf_read_entry_candidates() -> [u8; 3] {
  [READ_ENTRY_RAW, READ_ENTRY_PPC_SCV, READ_ENTRY_LIBC_READ]
}

#[cfg(all(feature = "bench-internal", not(any(target_arch = "x86", target_arch = "powerpc64")),))]
const fn perf_read_entry_candidates() -> [u8; 1] {
  [0]
}

#[cfg(all(feature = "bench-internal", target_arch = "x86"))]
const fn perf_read_entry_candidate_names() -> [&'static str; 3] {
  ["raw_read_int80", "libc_syscall_read", "libc_read"]
}

#[cfg(all(feature = "bench-internal", target_arch = "powerpc64"))]
const fn perf_read_entry_candidate_names() -> [&'static str; 3] {
  ["raw_read_sc", "raw_read_scv", "libc_read"]
}

#[cfg(all(feature = "bench-internal", not(any(target_arch = "x86", target_arch = "powerpc64")),))]
const fn perf_read_entry_candidate_names() -> [&'static str; 1] {
  [perf_read_entry_name(0)]
}

#[cfg(all(feature = "bench-internal", target_arch = "x86"))]
const fn perf_read_entry_name(entry: u8) -> &'static str {
  match entry {
    READ_ENTRY_RAW => "raw_read_int80",
    READ_ENTRY_LIBC_SYSCALL => "libc_syscall_read",
    READ_ENTRY_LIBC_READ => "libc_read",
    _ => "unknown",
  }
}

#[cfg(all(feature = "bench-internal", target_arch = "powerpc64"))]
const fn perf_read_entry_name(entry: u8) -> &'static str {
  match entry {
    READ_ENTRY_RAW => "raw_read_sc",
    READ_ENTRY_PPC_SCV => "raw_read_scv",
    READ_ENTRY_LIBC_READ => "libc_read",
    _ => "unknown",
  }
}

#[cfg(all(feature = "bench-internal", target_arch = "x86_64"))]
const fn perf_read_entry_name(_entry: u8) -> &'static str {
  "raw_read_syscall"
}

#[cfg(all(feature = "bench-internal", target_arch = "aarch64"))]
const fn perf_read_entry_name(_entry: u8) -> &'static str {
  "raw_read_svc"
}

#[cfg(all(feature = "bench-internal", target_arch = "arm"))]
const fn perf_read_entry_name(_entry: u8) -> &'static str {
  "raw_read_svc"
}

#[cfg(all(feature = "bench-internal", target_arch = "riscv64"))]
const fn perf_read_entry_name(_entry: u8) -> &'static str {
  "raw_read_ecall"
}

#[cfg(all(feature = "bench-internal", target_arch = "s390x"))]
const fn perf_read_entry_name(_entry: u8) -> &'static str {
  "raw_read_svc_3"
}

#[cfg(all(feature = "bench-internal", target_arch = "loongarch64"))]
const fn perf_read_entry_name(_entry: u8) -> &'static str {
  "raw_read_syscall_0"
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub(super) fn now_nanos() -> u64 {
  let state = hot();
  if state.addr() == SYSCALL_TAG {
    return super::posix_now_nanos();
  }
  read_outlined_provider(state)
}

#[inline(never)]
fn read_outlined_provider(state: *const PerfState) -> u64 {
  match state.addr() {
    0 => initialize_current_thread(),
    // Perf epochs are normalized to this same POSIX current-thread CPU clock,
    // so a recursive read can return the baseline without poisoning setup.
    INITIALIZING_TAG => super::posix_now_nanos(),
    INHERITED_STALE_TAG => reinitialize_after_fork(),
    COMMITTING_TAG => commit_now_nanos(),
    _ => read_perf_or_degrade(state),
  }
}

#[inline]
fn commit_now_nanos() -> u64 {
  let sample = super::posix_now_nanos();
  if crate::thread_cpu::is_wall_value(sample) {
    return sample;
  }
  let state = commit_state();
  if state.addr() <= MAX_SENTINEL_TAG {
    return sample;
  }
  // SAFETY: COMMIT_STATE is installed before the commit sentinel and cleared
  // only after HOT_STATE publishes a non-state route or this same state.
  unsafe { (*state).publish_at_least(sample) }
}

pub(super) fn provider() -> ThreadCpuProvider {
  ensure_initialized();
  let state = hot();
  if state.addr() > MAX_SENTINEL_TAG {
    // SAFETY: non-sentinel pointers address this thread's owner-held state.
    let state = unsafe { &*state };
    return match state.selected_path.load(Ordering::Relaxed) {
      PATH_MMAP => ThreadCpuProvider::LinuxPerfMmap,
      PATH_READ => ThreadCpuProvider::LinuxPerfRead,
      _ => ThreadCpuProvider::PosixThreadCpuClock,
    };
  }
  ThreadCpuProvider::PosixThreadCpuClock
}

pub(super) fn max_read_gap_nanos() -> Option<u64> {
  ensure_initialized();
  let state = hot();
  if state.addr() <= MAX_SENTINEL_TAG {
    return None;
  }
  // SAFETY: non-sentinel HOT_STATE pointers address the state owned by this
  // thread's OWNER for the remainder of the thread lifetime.
  let state = unsafe { &*state };
  if state.selected_path.load(Ordering::Relaxed) != PATH_MMAP {
    None
  } else {
    state.max_read_gap_nanos
  }
}

fn ensure_initialized() {
  match hot().addr() {
    0 => {
      let _ = initialize_current_thread();
    }
    INHERITED_STALE_TAG => {
      let _ = reinitialize_after_fork();
    }
    _ => {}
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "arm",
    target_arch = "riscv64",
  ),
))]
pub(super) fn bench_selection_measurements()
-> Option<([u64; MEASURE_SAMPLES], [u64; MEASURE_SAMPLES], usize)> {
  ensure_initialized();
  OWNER
    .try_with(|owner| {
      let owner = owner.try_borrow().ok()?;
      let evidence = owner.current()?.path_measurements.get()?;
      Some((evidence.mmap?, evidence.syscall, MEASURE_READS))
    })
    .ok()
    .flatten()
}

#[cfg(feature = "bench-internal")]
pub(super) fn bench_path_evidence() -> Option<BenchPerfPathEvidence> {
  ensure_initialized();
  OWNER
    .try_with(|owner| {
      let owner = owner.try_borrow().ok()?;
      let state = owner.current()?;
      let measurements = state.path_measurements.get()?;
      Some(BenchPerfPathEvidence {
        mmap_batches_ns: measurements.mmap,
        read_batches_ns: measurements.read,
        posix_batches_ns: measurements.syscall,
        selected_path: perf_path_name(state.selected_path.load(Ordering::Acquire)),
        fallback_path: perf_path_name(state.fallback_path.load(Ordering::Acquire)),
        reads_per_batch: MEASURE_READS,
        required_decisive_wins: REQUIRED_DECISIVE_WINS,
      })
    })
    .ok()
    .flatten()
}

#[cfg(feature = "bench-internal")]
pub(super) fn bench_perf_read_entry_evidence() -> Option<BenchPerfReadEntryEvidence> {
  ensure_initialized();
  OWNER
    .try_with(|owner| {
      let owner = owner.try_borrow().ok()?;
      let _state = owner.current()?;
      #[cfg(any(target_arch = "x86", target_arch = "powerpc64"))]
      {
        let evidence = _state.read_entry_measurements.get()?;
        return Some(BenchPerfReadEntryEvidence {
          candidate_count: 3,
          candidate_names: perf_read_entry_candidate_names(),
          candidate_eligible: evidence.eligible,
          candidate_measured: evidence.eligible,
          candidate_batches_ns: evidence.samples,
          selected_candidate: perf_read_entry_name(evidence.selected),
          reads_per_batch: MEASURE_READS,
          required_decisive_wins: REQUIRED_DECISIVE_WINS,
        });
      }
      #[cfg(not(any(target_arch = "x86", target_arch = "powerpc64")))]
      {
        Some(BenchPerfReadEntryEvidence {
          candidate_count: 1,
          candidate_names: [perf_read_entry_name(0), "unavailable", "unavailable"],
          candidate_eligible: [true, false, false],
          candidate_measured: [false, false, false],
          candidate_batches_ns: [[0; MEASURE_SAMPLES]; 3],
          selected_candidate: perf_read_entry_name(0),
          reads_per_batch: MEASURE_READS,
          required_decisive_wins: REQUIRED_DECISIVE_WINS,
        })
      }
    })
    .ok()
    .flatten()
}

#[cfg(feature = "bench-internal")]
pub(super) fn bench_perf_read_handle() -> Option<BenchPerfReadHandle> {
  ensure_initialized();
  let state = OWNER
    .try_with(|owner| {
      let owner = owner.try_borrow().ok()?;
      owner.current_ptr()
    })
    .ok()
    .flatten()?;
  let previous_hot = hot();
  set_hot(state);
  // SAFETY: OWNER keeps this state live and the !Send handle cannot leave the
  // current thread.
  let state_ref = unsafe { &*state };
  let previous_path = state_ref.selected_path.swap(PATH_READ, Ordering::AcqRel);
  let previous_entry = state_ref.perf_read_entry();
  Some(BenchPerfReadHandle { state, previous_hot, previous_path, previous_entry })
}

#[cfg(feature = "bench-internal")]
impl BenchPerfReadHandle {
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(super) fn now_nanos(&self) -> u64 {
    let value = now_nanos();
    // SAFETY: the !Send handle and pointer identity bind this to the current
    // thread's owner-held state.
    let state = unsafe { &*self.state };
    assert!(
      hot() == self.state
        && state.selected_path.load(Ordering::Acquire) == PATH_READ
        && !crate::thread_cpu::is_wall_value(value),
      "forced perf-read benchmark lost its public dispatch route",
    );
    value
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(super) fn direct_nanos(&self) -> u64 {
    // SAFETY: OWNER keeps the state live for this current-thread-only handle.
    unsafe { (*self.state).read_fd_nanos().expect("forced perf read became unavailable") }
  }

  pub(super) fn candidate_count(&self) -> usize {
    if cfg!(any(target_arch = "x86", target_arch = "powerpc64")) { 3 } else { 1 }
  }

  pub(super) fn candidate_name(&self, index: usize) -> Option<&'static str> {
    perf_read_entry_candidate_names().get(index).copied()
  }

  pub(super) fn selected_candidate_name(&self) -> &'static str {
    // SAFETY: OWNER keeps the state live for this current-thread-only handle.
    perf_read_entry_name(unsafe { (*self.state).perf_read_entry() })
  }

  pub(super) fn select_candidate(&self, index: usize) -> bool {
    let Some(&entry) = perf_read_entry_candidates().get(index) else {
      return false;
    };
    // SAFETY: OWNER keeps the state live for this current-thread-only handle.
    let state = unsafe { &*self.state };
    #[cfg(any(target_arch = "x86", target_arch = "powerpc64"))]
    if !state
      .read_entry_measurements
      .get()
      .is_some_and(|evidence| evidence.eligible[index])
    {
      return false;
    }
    state.set_perf_read_entry(entry);
    state.selected_path.store(PATH_READ, Ordering::Release);
    set_hot(self.state);
    true
  }
}

#[cfg(feature = "bench-internal")]
impl Drop for BenchPerfReadHandle {
  fn drop(&mut self) {
    // SAFETY: OWNER keeps the state live until after this handle is dropped.
    let state = unsafe { &*self.state };
    state.set_perf_read_entry(self.previous_entry);
    state.selected_path.store(self.previous_path, Ordering::Release);
    set_hot(self.previous_hot);
  }
}

#[cfg(feature = "bench-internal")]
pub(super) fn bench_perf_path_handle() -> Option<BenchPerfPathHandle> {
  ensure_initialized();
  let state = OWNER
    .try_with(|owner| {
      let owner = owner.try_borrow().ok()?;
      owner.current_ptr()
    })
    .ok()
    .flatten()?;
  let previous_hot = hot();
  // SAFETY: OWNER keeps this state live and the !Send handle cannot leave the
  // current thread.
  let state_ref = unsafe { &*state };
  let previous_path = state_ref.selected_path.load(Ordering::Acquire);
  Some(BenchPerfPathHandle {
    state,
    previous_hot,
    previous_path,
    forced_path: Cell::new(previous_path),
  })
}

#[cfg(feature = "bench-internal")]
impl BenchPerfPathHandle {
  pub(super) const fn candidate_count(&self) -> usize {
    3
  }

  pub(super) fn candidate_name(&self, index: usize) -> Option<&'static str> {
    ["linux_perf_mmap", "linux_perf_read", "posix_thread_cpu"].get(index).copied()
  }

  pub(super) fn candidate_available(&self, index: usize) -> bool {
    match index {
      // SAFETY: OWNER keeps this state live for the handle lifetime.
      0 => unsafe { (*self.state).mmap_available() },
      1 | 2 => true,
      _ => false,
    }
  }

  pub(super) fn select_candidate(&self, index: usize) -> bool {
    let path = match index {
      0 if self.candidate_available(0) => PATH_MMAP,
      1 => PATH_READ,
      2 => PATH_POSIX,
      _ => return false,
    };
    self.forced_path.set(path);
    if path == PATH_POSIX {
      set_hot(syscall_ptr());
    } else {
      // SAFETY: OWNER keeps this state live for the handle lifetime.
      unsafe { (*self.state).selected_path.store(path, Ordering::Release) };
      set_hot(self.state);
    }
    true
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(super) fn now_nanos(&self) -> u64 {
    let forced = self.forced_path.get();
    let value = now_nanos();
    let route_intact = if forced == PATH_POSIX {
      hot().addr() == SYSCALL_TAG
    } else {
      // SAFETY: OWNER keeps this state live for the handle lifetime.
      hot() == self.state
        && unsafe { (*self.state).selected_path.load(Ordering::Acquire) } == forced
    };
    assert!(
      route_intact && !crate::thread_cpu::is_wall_value(value),
      "forced thread-CPU path benchmark lost its public dispatch route",
    );
    value
  }
}

#[cfg(feature = "bench-internal")]
impl Drop for BenchPerfPathHandle {
  fn drop(&mut self) {
    // SAFETY: OWNER keeps this state live until this handle is dropped.
    unsafe { (*self.state).selected_path.store(self.previous_path, Ordering::Release) };
    set_hot(self.previous_hot);
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "arm",
    target_arch = "riscv64",
  ),
))]
pub(super) fn bench_perf_handle() -> Option<BenchPerfHandle> {
  ensure_initialized();
  let state = OWNER
    .try_with(|owner| {
      let owner = owner.try_borrow().ok()?;
      let state = owner.current()?;
      state.mmap_available().then(|| ptr::from_ref(state))
    })
    .ok()
    .flatten()?;
  let previous_hot = hot();
  set_hot(state);
  // SAFETY: OWNER keeps this state live and the handle cannot leave this
  // thread.
  let previous_path = unsafe { (*state).selected_path.swap(PATH_MMAP, Ordering::Relaxed) };
  #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
  // SAFETY: OWNER keeps this state live and the bench handle cannot leave the
  // current thread.
  let selected_counter = Some(unsafe { (*state).counter_kind.get() });
  #[cfg(any(target_arch = "arm", target_arch = "riscv64"))]
  let selected_counter = None;
  #[cfg(any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"))]
  let selected_counter = None;
  Some(BenchPerfHandle { state, previous_hot, previous_path, selected_counter })
}

#[cfg(all(
  feature = "bench-internal",
  any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "arm",
    target_arch = "riscv64",
  ),
))]
impl BenchPerfHandle {
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(super) fn now_nanos(&self) -> u64 {
    bench_exact_perf_hot_path()
  }

  pub(super) fn candidate_count(&self) -> usize {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
      5
    }
    #[cfg(target_arch = "aarch64")]
    {
      2
    }
    #[cfg(target_arch = "arm")]
    {
      1
    }
    #[cfg(target_arch = "riscv64")]
    {
      1
    }
    #[cfg(any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"))]
    {
      0
    }
  }

  pub(super) fn candidate_name(&self, index: usize) -> Option<&'static str> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
      arch_perf_counter_candidates()
        .get(index)
        .copied()
        .map(arch_perf_counter_candidate_name)
    }
    #[cfg(target_arch = "aarch64")]
    {
      match index {
        0 => Some("aarch64_isb_cntvct_isb"),
        1 => Some("aarch64_cntvctss_isb"),
        _ => None,
      }
    }
    #[cfg(target_arch = "arm")]
    {
      (index == 0).then_some("arm_isb_mrrc_cntvct_isb")
    }
    #[cfg(target_arch = "riscv64")]
    {
      (index == 0).then_some("riscv_fence_rdtime_fence")
    }
    #[cfg(any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"))]
    {
      let _ = index;
      None
    }
  }

  pub(super) fn selected_candidate_name(&self) -> &'static str {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
      arch_perf_counter_candidate_name(
        self
          .selected_counter
          .expect("x86 perf handle must capture its selected counter"),
      )
    }
    #[cfg(target_arch = "aarch64")]
    {
      if self.selected_counter == Some(PERF_COUNTER_AARCH64_SELF_SYNC) {
        "aarch64_cntvctss_isb"
      } else {
        "aarch64_isb_cntvct_isb"
      }
    }
    #[cfg(target_arch = "arm")]
    {
      "arm_isb_mrrc_cntvct_isb"
    }
    #[cfg(target_arch = "riscv64")]
    {
      "riscv_fence_rdtime_fence"
    }
    #[cfg(any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"))]
    {
      "unavailable"
    }
  }

  pub(super) fn select_candidate(&self, index: usize) -> bool {
    set_hot(self.state);
    // SAFETY: this !Send handle is tied to the owner-held state.
    unsafe { (*self.state).selected_path.store(PATH_MMAP, Ordering::Relaxed) };
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
      // SAFETY: this !Send handle is tied to the owner-held state.
      let state = unsafe { &*self.state };
      let Some(evidence) = state.counter_evidence else {
        return false;
      };
      if !evidence.eligible.get(index).copied().unwrap_or(false) {
        return false;
      }
      state.counter_kind.set(arch_perf_counter_candidates()[index]);
      true
    }
    #[cfg(target_arch = "aarch64")]
    {
      // SAFETY: this !Send handle is tied to the owner-held state.
      let state = unsafe { &*self.state };
      let Some(evidence) = state.counter_evidence else {
        return false;
      };
      if !evidence.eligible.get(index).copied().unwrap_or(false) {
        return false;
      }
      let candidate = [PERF_COUNTER_AARCH64_ISB, PERF_COUNTER_AARCH64_SELF_SYNC][index];
      state.counter_kind.set(candidate);
      true
    }
    #[cfg(target_arch = "arm")]
    {
      if index != 0 {
        return false;
      }
      // SAFETY: this !Send handle is tied to the owner-held state.
      unsafe { (*self.state).counter_kind.set(PERF_COUNTER_ARM_ISB) };
      true
    }
    #[cfg(target_arch = "riscv64")]
    {
      index == 0
    }
    #[cfg(any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"))]
    {
      let _ = index;
      false
    }
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "arm",
    target_arch = "riscv64",
  ),
))]
impl Drop for BenchPerfHandle {
  fn drop(&mut self) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if let Some(selected) = self.selected_counter {
      // SAFETY: the owner-held mapping outlives this handle.
      unsafe { (*self.state).counter_kind.set(selected) };
    }
    #[cfg(target_arch = "aarch64")]
    if let Some(selected) = self.selected_counter {
      // SAFETY: the owner-held mapping outlives this handle.
      unsafe { (*self.state).counter_kind.set(selected) };
    }
    #[cfg(any(target_arch = "arm", target_arch = "riscv64"))]
    let _ = self.selected_counter;
    #[cfg(any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"))]
    let _ = self.selected_counter;
    // SAFETY: the owner-held state outlives this handle.
    unsafe { (*self.state).selected_path.store(self.previous_path, Ordering::Relaxed) };
    set_hot(self.previous_hot);
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"),
))]
pub(super) fn bench_perf_counter_evidence() -> Option<BenchPerfCounterEvidence> {
  ensure_initialized();
  let evidence = OWNER
    .try_with(|owner| {
      let owner = owner.try_borrow().ok()?;
      owner.current()?.counter_evidence
    })
    .ok()
    .flatten()?;
  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  let candidate_names = arch_perf_counter_candidates().map(arch_perf_counter_candidate_name);
  #[cfg(target_arch = "aarch64")]
  let candidate_names =
    ["aarch64_isb_cntvct_isb", "aarch64_cntvctss_isb", "unavailable", "unavailable", "unavailable"];
  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  let selected_candidate = arch_perf_counter_candidate_name(evidence.selected);
  #[cfg(target_arch = "aarch64")]
  let selected_candidate = if evidence.selected == PERF_COUNTER_AARCH64_SELF_SYNC {
    "aarch64_cntvctss_isb"
  } else {
    "aarch64_isb_cntvct_isb"
  };
  Some(BenchPerfCounterEvidence {
    candidate_count: if cfg!(target_arch = "aarch64") { 2 } else { 5 },
    candidate_names,
    candidate_eligible: evidence.eligible,
    candidate_batches_ns: evidence.samples,
    selected_candidate,
    reads_per_batch: MEASURE_READS,
    required_decisive_wins: REQUIRED_DECISIVE_WINS,
  })
}

#[cold]
#[inline(never)]
fn initialize_current_thread() -> u64 {
  let claimed = HOT_STATE
    .try_with(|slot| {
      if slot.get().is_null() {
        slot.set(initializing_ptr());
        true
      } else {
        false
      }
    })
    .unwrap_or(false);

  if !claimed {
    return now_nanos();
  }

  select_current_thread_provider()
}

#[cold]
fn select_current_thread_provider() -> u64 {
  // SAFETY: getpid has no caller-side preconditions.
  let selection_pid = unsafe { libc::getpid() };
  let Some(state) = PerfState::open() else {
    return finish_failed_initialization(selection_pid);
  };

  // A recursive read observes the normalized POSIX baseline without changing
  // this sentinel, so only a genuine setup failure can select the fallback.
  if hot().addr() != INITIALIZING_TAG {
    return now_nanos();
  }

  let Some(state_ptr) = install_state(state) else {
    return finish_failed_initialization(selection_pid);
  };

  #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
  {
    #[cfg(feature = "bench-internal")]
    if let Some(measurements) = measure_exact_paths(state_ptr) {
      // SAFETY: state_ptr addresses this thread's owner-held state. These
      // samples audit the capability policy; they do not select the provider.
      unsafe { (*state_ptr).path_measurements.set(Some(measurements)) };
    }
    return commit_aarch64_capability_provider(state_ptr);
  }

  #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
  {
    let measurements = measure_exact_paths(state_ptr);
    #[cfg(feature = "bench-internal")]
    if let Some(measurements) = measurements.as_ref() {
      // SAFETY: state_ptr addresses this thread's owner-held state.
      unsafe { (*state_ptr).path_measurements.set(Some(*measurements)) };
    }
    let Some(measurements) = measurements else {
      if !state_is_current_process(state_ptr) {
        return resume_current_after_obsolete(state_ptr);
      }
      #[cfg(feature = "bench-internal")]
      {
        set_hot(syscall_ptr());
        return super::posix_now_nanos();
      }
      #[cfg(not(feature = "bench-internal"))]
      {
        let Some(owner_pid) = discard_state_if_current(state_ptr) else {
          return resume_current_after_obsolete(state_ptr);
        };
        // SAFETY: getpid has no caller-side preconditions.
        if owner_pid != unsafe { libc::getpid() } {
          return resume_or_reinitialize_after_fork(state_ptr);
        }
        return super::posix_now_nanos();
      }
    };
    commit_measured_provider(state_ptr, &measurements)
  }
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn commit_aarch64_capability_provider(state_ptr: *const PerfState) -> u64 {
  if !state_is_current_process(state_ptr) {
    return resume_current_after_obsolete(state_ptr);
  }

  // Hide the provisional fd-read route before publishing the capability
  // winner. Nested reads remain in the same POSIX CPU-time domain.
  set_commit_state(state_ptr);
  set_hot(committing_ptr());
  #[cfg(test)]
  test_signal_fork_at(TEST_FORK_DURING_COMMIT);

  let floor = super::posix_now_nanos();
  if crate::thread_cpu::is_wall_value(floor) {
    return finish_posix_selection(state_ptr);
  }

  // SAFETY: install_state placed this state in the current thread's OWNER,
  // and the commit sentinel prevents nested reads from replacing its route.
  let state = unsafe { &*state_ptr };
  state.publish_at_least(floor);
  state.fallback_bias.store(FALLBACK_UNSET, Ordering::Release);
  state.fallback_path.store(PATH_POSIX, Ordering::Release);

  if !state.mmap_available() {
    state.selected_path.store(PATH_POSIX, Ordering::Release);
    return finish_posix_selection(state_ptr);
  }

  state.selected_path.store(PATH_MMAP, Ordering::Release);
  let Some(value) = state.read_mmap() else {
    state.mark_path_failed(PATH_MMAP);
    state.selected_path.store(PATH_POSIX, Ordering::Release);
    return finish_posix_selection(state_ptr);
  };

  let value = value.max(state.last_event_nanos.load(Ordering::Acquire));
  set_hot(state_ptr);
  set_commit_state(ptr::null());
  // Publish first and check second so a nested fork cannot leave a live
  // parent-owned mapping installed in the child.
  // SAFETY: getpid has no caller-side preconditions.
  if state.owner_pid != unsafe { libc::getpid() } {
    return resume_current_after_obsolete(state_ptr);
  }
  value
}

#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
fn commit_measured_provider(
  state_ptr: *const PerfState,
  measurements: &PerfPathMeasurements,
) -> u64 {
  if !state_is_current_process(state_ptr) {
    return resume_current_after_obsolete(state_ptr);
  }
  // Stop exposing forced measurement paths before choosing the winner. A
  // nested signal now samples the same POSIX CPU-time domain and advances the
  // state's monotonic floor, but it cannot retier or mutate fallback state.
  set_commit_state(state_ptr);
  set_hot(committing_ptr());
  #[cfg(test)]
  test_signal_fork_at(TEST_FORK_DURING_COMMIT);

  // This post-measurement sample follows every nested read that completed
  // before the commit sentinel was installed. Signals after installation also
  // publish through `commit_now_nanos`, so the eventual provider cannot begin
  // below any CPU-time sample returned during initialization.
  let floor = super::posix_now_nanos();
  if crate::thread_cpu::is_wall_value(floor) {
    return finish_posix_selection(state_ptr);
  }

  // SAFETY: `install_state` placed this state in the current thread's OWNER,
  // and the commit sentinel prevents nested reads from replacing its route.
  let state = unsafe { &*state_ptr };
  state.publish_at_least(floor);

  loop {
    let failed_paths = state.failed_paths.load(Ordering::Acquire);
    let selected = fastest_path_excluding(measurements, state.mmap_available(), 0, failed_paths)
      .unwrap_or(PATH_POSIX);
    let fallback =
      fastest_path_excluding(measurements, state.mmap_available(), selected, failed_paths)
        .unwrap_or(PATH_POSIX);

    // A signal may prepare a POSIX transition while the final measured path
    // is publicly forced. Clear that abandoned marker, then retain the exact
    // tournament order needed by benchmark evidence. Production discards the
    // state when POSIX wins.
    state.fallback_bias.store(FALLBACK_UNSET, Ordering::Release);
    state.selected_path.store(selected, Ordering::Release);
    state.fallback_path.store(fallback, Ordering::Release);

    if selected == PATH_POSIX {
      return finish_posix_selection(state_ptr);
    }

    if let Some(value) = state.read_path(selected) {
      let value = value.max(state.last_event_nanos.load(Ordering::Acquire));
      set_hot(state_ptr);
      set_commit_state(ptr::null());
      // A fork from a nested signal can replace HOT_STATE with the stale
      // sentinel while the outer initializer is paused. Publish first and
      // check second: a fork before this check is detected, while a fork after
      // it leaves the callback's stale sentinel intact for the next read.
      // SAFETY: getpid has no caller-side preconditions.
      if state.owner_pid != unsafe { libc::getpid() } {
        return resume_current_after_obsolete(state_ptr);
      }
      return value;
    }

    // The route failed while hidden behind the commit sentinel. Exclude it
    // permanently and recompute from the already-measured exact candidates.
    state.mark_path_failed(selected);
  }
}

fn finish_posix_selection(state_ptr: *const PerfState) -> u64 {
  // Criterion's exact-candidate rows retain the mapping after selection so a
  // rejected capable path can still be measured. Production releases it.
  set_hot(syscall_ptr());
  set_commit_state(ptr::null());
  if !state_is_current_process(state_ptr) {
    return resume_current_after_obsolete(state_ptr);
  }
  #[cfg(not(feature = "bench-internal"))]
  {
    let Some(owner_pid) = discard_state_if_current(state_ptr) else {
      return resume_current_after_obsolete(state_ptr);
    };
    // SAFETY: getpid has no caller-side preconditions.
    if owner_pid != unsafe { libc::getpid() } {
      return resume_or_reinitialize_after_fork(state_ptr);
    }
  }
  super::posix_now_nanos()
}

fn finish_failed_initialization(selection_pid: libc::pid_t) -> u64 {
  // Publish the safe baseline first. If a nested fork/read installed another
  // current state while setup was failing, the owner lookup below restores it
  // and this outer frame performs no later HOT_STATE store.
  set_commit_state(ptr::null());
  set_hot(syscall_ptr());
  if current_state_ptr().is_some() {
    return resume_current_after_obsolete(ptr::null());
  }
  // SAFETY: getpid has no caller-side preconditions.
  if selection_pid != unsafe { libc::getpid() } {
    return resume_or_reinitialize_after_fork(ptr::null());
  }
  super::posix_now_nanos()
}

fn install_state(state: PerfState) -> Option<*const PerfState> {
  let allocation = allocate_perf_state(state)?;
  let installed = OWNER
    .try_with(|owner| {
      let Ok(mut owner) = owner.try_borrow_mut() else {
        return false;
      };
      if owner.state.is_some() {
        return false;
      }
      owner.state = Some(allocation);
      true
    })
    .unwrap_or(false);
  if !installed {
    // SAFETY: installation failed, so no owner or public pointer can observe
    // this allocation.
    unsafe { destroy_perf_state(allocation) };
    return None;
  }
  let state_ptr = allocation.as_ptr().cast_const();
  // Publish only after the RefMut guard has dropped. A signal may dereference
  // HOT_STATE immediately, which must not alias an active mutable borrow.
  set_hot(state_ptr);
  Some(state_ptr)
}

fn allocate_perf_state(state: PerfState) -> Option<NonNull<PerfState>> {
  // SAFETY: malloc returns storage suitably aligned for PerfState. A null
  // result makes the adaptive provider unavailable without panicking.
  let allocation: NonNull<PerfState> =
    NonNull::new(unsafe { libc::malloc(mem::size_of::<PerfState>()) }.cast())?;
  // SAFETY: the allocation is non-null, aligned, and exactly large enough.
  unsafe { allocation.as_ptr().write(state) };
  Some(allocation)
}

unsafe fn destroy_perf_state(state: NonNull<PerfState>) {
  // SAFETY: callers remove this uniquely owned allocation from ThreadOwner
  // before destroying it.
  unsafe { state.as_ptr().drop_in_place() };
  // SAFETY: the allocation came from malloc and has now been dropped once.
  unsafe { libc::free(state.as_ptr().cast()) };
}

fn current_state_ptr() -> Option<*const PerfState> {
  OWNER
    .try_with(|owner| owner.try_borrow().ok().and_then(|owner| owner.current_ptr()))
    .ok()
    .flatten()
}

fn state_is_current(expected: *const PerfState) -> bool {
  current_state_ptr() == Some(expected)
}

fn state_is_current_process(expected: *const PerfState) -> bool {
  if !state_is_current(expected) {
    return false;
  }
  // SAFETY: equality with OWNER's current pointer proves the allocation live.
  let owner_pid = unsafe { (*expected).owner_pid };
  // SAFETY: getpid has no caller-side preconditions.
  owner_pid == unsafe { libc::getpid() }
}

#[cfg(not(feature = "bench-internal"))]
fn discard_state_if_current(expected: *const PerfState) -> Option<libc::pid_t> {
  // Stop nested reads from acquiring `expected` before removing it from its
  // owner. A fork callback may replace this sentinel, which is safe because
  // this function performs no later HOT_STATE store.
  set_hot(syscall_ptr());
  set_commit_state(ptr::null());
  let state = OWNER
    .try_with(|owner| {
      let mut owner = owner.try_borrow_mut().ok()?;
      (owner.current_ptr() == Some(expected)).then(|| owner.state.take()).flatten()
    })
    .ok()
    .flatten();
  if let Some(state) = state {
    // SAFETY: the allocation remains live until the destroy call below.
    let owner_pid = unsafe { state.as_ref().owner_pid };
    // SAFETY: the current state was removed from its sole owner after the hot
    // pointer stopped exposing it.
    unsafe { destroy_perf_state(state) };
    Some(owner_pid)
  } else {
    None
  }
}

fn resume_current_after_obsolete(_obsolete: *const PerfState) -> u64 {
  // Publish the safe baseline before inspecting OWNER. If a nested fork
  // installs another state, the subsequent lookup observes and restores it.
  set_commit_state(ptr::null());
  set_hot(syscall_ptr());
  let Some(current) = current_state_ptr() else {
    return super::posix_now_nanos();
  };

  // SAFETY: current_state_ptr returned the live allocation held by OWNER.
  let state = unsafe { &*current };
  if state.selected_path.load(Ordering::Acquire) == PATH_POSIX
    && state.fallback_marker() == FALLBACK_UNSET
  {
    // Bench evidence retains a rejected mapping even though the production
    // route is the plain POSIX sentinel.
    return super::posix_now_nanos();
  }

  // Convert the temporary safe-baseline window into the same commit protocol:
  // reads before the sentinel are followed by `floor`, and reads after it
  // publish themselves into this state's monotonic floor.
  set_commit_state(current);
  set_hot(committing_ptr());
  let floor = super::posix_now_nanos();
  if !crate::thread_cpu::is_wall_value(floor) {
    state.publish_at_least(floor);
  }
  set_hot(current);
  set_commit_state(ptr::null());

  let owner_pid = state.owner_pid;
  // Publish first and check second so a fork before the check is detected and
  // a fork after it leaves the callback's stale sentinel intact.
  // SAFETY: getpid has no caller-side preconditions.
  if owner_pid != unsafe { libc::getpid() } {
    set_hot(inherited_stale_ptr());
    return reinitialize_after_fork();
  }

  now_nanos()
}

fn resume_or_reinitialize_after_fork(obsolete: *const PerfState) -> u64 {
  if current_state_ptr().is_some() {
    return resume_current_after_obsolete(obsolete);
  }
  set_commit_state(ptr::null());
  set_hot(ptr::null());
  initialize_current_thread()
}

#[cfg(test)]
fn test_signal_fork_at(point: u8) {
  if TEST_FORK_TID.load(Ordering::Relaxed) != crate::arch::current_thread_id() {
    return;
  }
  if TEST_FORK_POINT
    .compare_exchange(point, 0, Ordering::AcqRel, Ordering::Acquire)
    .is_ok()
  {
    // SAFETY: the regression test installs a SIGUSR2 handler before arming
    // this exact current-thread injection point.
    unsafe { libc::raise(libc::SIGUSR2) };
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn read_perf_or_degrade(state: *const PerfState) -> u64 {
  // SAFETY: non-sentinel HOT_STATE pointers address the PerfState held by this
  // thread's OWNER and remain live until its TLS destructor first publishes
  // the syscall sentinel.
  let state = unsafe { &*state };
  #[cfg(test)]
  test_signal_fork_at(TEST_FORK_DURING_HOT_READ);
  let selected = state.selected_path.load(Ordering::Acquire);
  if selected == PATH_OBSOLETE {
    return resume_current_after_obsolete(state);
  }
  if selected == PATH_POSIX {
    return state.read_posix_fallback();
  }
  let value = state.read_path(selected);
  if let Some(value) = value {
    // A nested signal can retier the provider while this outer read is in
    // flight. Complete the call on the already-published path.
    let after = state.selected_path.load(Ordering::Acquire);
    if after == selected { value } else { state.read_selected_after_race(after) }
  } else {
    // Keep the mapping owned until thread exit. A signal could have interrupted
    // an outer reader, so unmapping it on this hot failure path would make that
    // outer read access freed memory when it resumes.
    state.retier_after_failure(selected)
  }
}

#[cfg(target_arch = "riscv64")]
fn select_perf_counter(_state: &mut PerfState) -> Option<()> {
  Some(())
}

#[cfg(any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"))]
fn select_perf_counter(_state: &mut PerfState) -> Option<()> {
  None
}

#[cfg(target_arch = "arm")]
fn select_perf_counter(state: &mut PerfState) -> Option<()> {
  state.counter_kind.set(PERF_COUNTER_ARM_ISB);
  for _ in 0..WARMUP_READS {
    state.read_event_nanos()?;
  }
  Some(())
}

#[cfg(target_arch = "aarch64")]
fn select_perf_counter(state: &mut PerfState) -> Option<()> {
  let evidence = measure_aarch64_perf_counter_candidates(state)?;
  state.counter_kind.set(evidence.selected);
  #[cfg(feature = "bench-internal")]
  {
    state.counter_evidence = Some(evidence);
  }
  Some(())
}

#[cfg(target_arch = "aarch64")]
#[allow(clippy::needless_range_loop)] // Rows are candidates; columns are rotated sample order.
fn measure_aarch64_perf_counter_candidates(
  state: &PerfState,
) -> Option<PerfCounterSelectionEvidence> {
  let candidates = [PERF_COUNTER_AARCH64_ISB, PERF_COUNTER_AARCH64_SELF_SYNC];
  let mut eligible = [false; 5];
  eligible[0] = true;
  eligible[1] = crate::arch::aarch64::cntvctss_capable();
  let mut samples = [[0_u64; MEASURE_SAMPLES]; 5];

  for (index, candidate) in candidates.iter().copied().enumerate() {
    if !eligible[index] {
      continue;
    }
    state.counter_kind.set(candidate);
    for _ in 0..WARMUP_READS {
      if state.read_event_nanos().is_none() {
        eligible[index] = false;
        break;
      }
    }
  }
  if !eligible[0] {
    return None;
  }
  for sample in 0..MEASURE_SAMPLES {
    for offset in 0..candidates.len() {
      let index = (sample + offset) % candidates.len();
      if !eligible[index] {
        continue;
      }
      let Some(elapsed) = measure_aarch64_perf_counter_candidate(state, candidates[index]) else {
        eligible[index] = false;
        continue;
      };
      samples[index][sample] = elapsed;
    }
  }
  let selected = if eligible[1] && prefer_perf(samples[1], samples[0]) {
    PERF_COUNTER_AARCH64_SELF_SYNC
  } else {
    PERF_COUNTER_AARCH64_ISB
  };
  Some(PerfCounterSelectionEvidence {
    #[cfg(feature = "bench-internal")]
    eligible,
    #[cfg(feature = "bench-internal")]
    samples,
    selected,
  })
}

#[cfg(target_arch = "aarch64")]
fn measure_aarch64_perf_counter_candidate(state: &PerfState, candidate: u8) -> Option<u64> {
  state.counter_kind.set(candidate);
  let start = monotonic_raw_nanos()?;
  for _ in 0..MEASURE_READS {
    black_box(state.read_event_nanos()?);
  }
  monotonic_raw_nanos()?.checked_sub(start)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn select_perf_counter(state: &mut PerfState) -> Option<()> {
  let evidence = measure_perf_counter_candidates(state)?;
  state.counter_kind.set(evidence.selected);
  #[cfg(feature = "bench-internal")]
  {
    state.counter_evidence = Some(evidence);
  }
  Some(())
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[allow(clippy::needless_range_loop)] // Rows are candidates; columns are rotated sample order.
fn measure_perf_counter_candidates(state: &PerfState) -> Option<PerfCounterSelectionEvidence> {
  let candidates = arch_perf_counter_candidates();
  let mut eligible = [false; 5];
  let mut samples = [[0_u64; MEASURE_SAMPLES]; 5];

  for (index, candidate) in candidates.iter().copied().enumerate() {
    eligible[index] = arch_perf_counter_candidate_eligible(candidate);
    if !eligible[index] {
      continue;
    }
    state.counter_kind.set(candidate);
    for _ in 0..WARMUP_READS {
      if state.read_event_nanos().is_none() {
        eligible[index] = false;
        break;
      }
    }
  }

  // CPUID is the architecturally guaranteed baseline and appears first.
  if !eligible[0] {
    return None;
  }
  for sample in 0..MEASURE_SAMPLES {
    for offset in 0..candidates.len() {
      let index = (sample + offset) % candidates.len();
      if !eligible[index] {
        continue;
      }
      let Some(elapsed) = measure_perf_counter_candidate(state, candidates[index]) else {
        eligible[index] = false;
        continue;
      };
      samples[index][sample] = elapsed;
    }
    if !eligible[0] {
      return None;
    }
  }

  let mut selected_index = 0;
  for challenger in 1..candidates.len() {
    if eligible[challenger] && prefer_perf(samples[challenger], samples[selected_index]) {
      selected_index = challenger;
    }
  }
  Some(PerfCounterSelectionEvidence {
    #[cfg(feature = "bench-internal")]
    eligible,
    #[cfg(feature = "bench-internal")]
    samples,
    selected: candidates[selected_index],
  })
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn measure_perf_counter_candidate(state: &PerfState, candidate: u8) -> Option<u64> {
  state.counter_kind.set(candidate);
  let start = monotonic_raw_nanos()?;
  for _ in 0..MEASURE_READS {
    black_box(state.read_event_nanos()?);
  }
  monotonic_raw_nanos()?.checked_sub(start)
}

#[cfg(target_arch = "x86")]
fn arch_perf_counter_candidates() -> [u8; 5] {
  crate::arch::x86::PERF_SEQ_CANDIDATES
}

#[cfg(target_arch = "x86_64")]
fn arch_perf_counter_candidates() -> [u8; 5] {
  crate::arch::x86_64::PERF_SEQ_CANDIDATES
}

#[cfg(target_arch = "x86")]
fn arch_perf_counter_candidate_eligible(candidate: u8) -> bool {
  crate::arch::x86::perf_seqlock_candidate_eligible(candidate)
}

#[cfg(target_arch = "x86_64")]
fn arch_perf_counter_candidate_eligible(candidate: u8) -> bool {
  crate::arch::x86_64::perf_seqlock_candidate_eligible(candidate)
}

#[cfg(target_arch = "x86")]
#[cfg(feature = "bench-internal")]
fn arch_perf_counter_candidate_name(candidate: u8) -> &'static str {
  crate::arch::x86::perf_seqlock_candidate_name(candidate)
}

#[cfg(target_arch = "x86_64")]
#[cfg(feature = "bench-internal")]
fn arch_perf_counter_candidate_name(candidate: u8) -> &'static str {
  crate::arch::x86_64::perf_seqlock_candidate_name(candidate)
}

#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", target_arch = "aarch64")),
))]
fn measure_exact_paths(state: *const PerfState) -> Option<PerfPathMeasurements> {
  // SAFETY: install_state made this pointer current-thread-owned and live.
  let mmap_available = unsafe { (*state).mmap_available() };
  for path in [PATH_MMAP, PATH_READ, PATH_POSIX] {
    if path == PATH_MMAP && !mmap_available {
      continue;
    }
    force_path(state, path)?;
    let mut last = black_box(now_nanos());
    for _ in 1..WARMUP_READS {
      last = black_box(now_nanos());
    }
    validate_forced_path(state, path, last)?;
  }

  let mut mmap = mmap_available.then_some([0_u64; MEASURE_SAMPLES]);
  let mut read = [0_u64; MEASURE_SAMPLES];
  let mut syscall = [0_u64; MEASURE_SAMPLES];
  let paths = [PATH_MMAP, PATH_READ, PATH_POSIX];
  for sample in 0..MEASURE_SAMPLES {
    for offset in 0..paths.len() {
      let path = paths[(sample + offset) % paths.len()];
      if path == PATH_MMAP && !mmap_available {
        continue;
      }
      let elapsed = measure_path(state, path)?;
      match path {
        PATH_MMAP => mmap.as_mut()?[sample] = elapsed,
        PATH_READ => read[sample] = elapsed,
        PATH_POSIX => syscall[sample] = elapsed,
        _ => return None,
      }
    }
  }
  Some(PerfPathMeasurements { mmap, read, syscall })
}

#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", target_arch = "aarch64")),
))]
#[inline]
fn force_path(state: *const PerfState, path: u8) -> Option<()> {
  if !state_is_current_process(state) {
    return None;
  }
  if path == PATH_POSIX {
    set_hot(syscall_ptr());
  } else {
    // SAFETY: the pointer addresses this thread's owner-held state.
    unsafe { (*state).selected_path.store(path, Ordering::Relaxed) };
    set_hot(state);
  }
  Some(())
}

#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", target_arch = "aarch64")),
))]
fn measure_path(state: *const PerfState, path: u8) -> Option<u64> {
  force_path(state, path)?;
  let start = monotonic_raw_nanos()?;
  let mut last = black_box(now_nanos());
  for _ in 1..MEASURE_READS {
    // This is the exact production dispatch: TLS load, sentinel branch,
    // selected-path loads, provider read, and nested-signal retier check. Path
    // validation stays outside the timed loop so every candidate pays only
    // the public hot path it would retain after selection.
    last = black_box(now_nanos());
  }
  let end = monotonic_raw_nanos()?;
  validate_forced_path(state, path, last)?;
  end.checked_sub(start)
}

#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", target_arch = "aarch64")),
))]
#[inline]
fn validate_forced_path(state: *const PerfState, path: u8, value: u64) -> Option<()> {
  if !state_is_current_process(state) {
    return None;
  }
  let hot_state = hot();
  if path == PATH_POSIX {
    (hot_state.addr() == SYSCALL_TAG && !crate::thread_cpu::is_wall_value(value)).then_some(())
  } else {
    if hot_state != state || state.addr() <= MAX_SENTINEL_TAG {
      return None;
    }
    // SAFETY: pointer equality ties this to the current owner-held state. If a
    // read failed or a nested signal retiered it, the production dispatch
    // changed selected_path and this candidate is rejected.
    (unsafe { (*state).selected_path.load(Ordering::Acquire) } == path
      && !crate::thread_cpu::is_wall_value(value))
    .then_some(())
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "arm",
    target_arch = "riscv64",
  ),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn bench_exact_perf_hot_path() -> u64 {
  let state = hot();
  if state.addr() <= MAX_SENTINEL_TAG {
    panic!("exact perf benchmark lost its forced public-dispatch route");
  }
  // SAFETY: the bench handle that forced this route is !Send and the current
  // thread's OWNER keeps the mapping live. Failure is explicit so Criterion
  // can never relabel a degraded syscall sample as the perf candidate.
  unsafe { (*state).read_mmap().expect("forced perf candidate became unreadable") }
}

#[cfg(any(test, not(all(target_os = "linux", target_arch = "aarch64"))))]
fn fastest_path_excluding(
  measurements: &PerfPathMeasurements,
  mmap_available: bool,
  excluded: u8,
  failed_paths: u8,
) -> Option<u8> {
  let mut selected = None;
  let mut selected_samples = [0; MEASURE_SAMPLES];
  for (path, samples) in [
    (PATH_POSIX, Some(measurements.syscall)),
    (PATH_MMAP, measurements.mmap.filter(|_| mmap_available)),
    (PATH_READ, Some(measurements.read)),
  ] {
    let Some(samples) = samples else {
      continue;
    };
    if path == excluded {
      continue;
    }
    if failed_paths & path_mask(path) != 0 {
      continue;
    }
    if selected.is_none() || prefer_perf(samples, selected_samples) {
      selected = Some(path);
      selected_samples = samples;
    }
  }
  selected
}

#[inline]
const fn path_mask(path: u8) -> u8 {
  match path {
    PATH_MMAP => 1 << 0,
    PATH_READ => 1 << 1,
    _ => 0,
  }
}

fn monotonic_raw_nanos() -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // Use the raw kernel entry deliberately. A libc/vDSO CLOCK_MONOTONIC_RAW
  // implementation can itself read the architectural counter whose perf path
  // is under test; the syscall keeps the tournament's measurement clock
  // independent from every userspace candidate.
  // SAFETY: value is writable timespec storage and CLOCK_MONOTONIC_RAW is a
  // valid Linux-kernel clock id.
  let status = unsafe {
    libc::syscall(libc::SYS_clock_gettime, libc::CLOCK_MONOTONIC_RAW, value.as_mut_ptr())
  };
  if status != 0 {
    return None;
  }
  // SAFETY: clock_gettime initialized the output on success.
  let value = unsafe { value.assume_init() };
  let seconds = u64::try_from(value.tv_sec).ok()?;
  let nanos = u32::try_from(value.tv_nsec).ok()?;
  if nanos >= 1_000_000_000 {
    return None;
  }
  seconds.checked_mul(1_000_000_000)?.checked_add(u64::from(nanos))
}

#[inline]
fn prefer_perf(
  perf_samples: [u64; MEASURE_SAMPLES],
  syscall_samples: [u64; MEASURE_SAMPLES],
) -> bool {
  let perf_median = median(perf_samples);
  let syscall_median = median(syscall_samples);
  let allowance = selection_allowance(syscall_median);
  let decisive_wins = perf_samples
    .iter()
    .zip(syscall_samples)
    .filter(|(perf, syscall)| (**perf).saturating_add(allowance) < *syscall)
    .count();

  perf_median.saturating_add(allowance) < syscall_median && decisive_wins >= REQUIRED_DECISIVE_WINS
}

#[inline]
fn selection_allowance(syscall_total: u64) -> u64 {
  // A one-nanosecond-per-read floor prevents sub-nanosecond timer noise from
  // deciding the current thread's provider. The 5% term scales that equivalence
  // band on slower systems. Eight decisive wins from nine paired, alternating
  // batches is a one-sided sign test with p < 0.02 under an equal-cost null.
  (MEASURE_READS as u64).max(syscall_total / 20)
}

#[inline]
fn align_event_epoch(syscall_nanos: u64, event_nanos: u64) -> Option<u64> {
  syscall_nanos.checked_sub(event_nanos)
}

fn median<const N: usize>(mut samples: [u64; N]) -> u64 {
  samples.sort_unstable();
  samples[N / 2]
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn hot() -> *const PerfState {
  HOT_STATE.try_with(Cell::get).unwrap_or_else(|_| syscall_ptr())
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn commit_state() -> *const PerfState {
  COMMIT_STATE.try_with(Cell::get).unwrap_or_else(|_| ptr::null())
}

#[inline]
fn set_hot(state: *const PerfState) {
  let _ = HOT_STATE.try_with(|slot| slot.set(state));
}

#[inline]
fn set_commit_state(state: *const PerfState) {
  let _ = COMMIT_STATE.try_with(|slot| slot.set(state));
}

#[inline]
fn syscall_ptr() -> *const PerfState {
  ptr::without_provenance(SYSCALL_TAG)
}

#[inline]
fn initializing_ptr() -> *const PerfState {
  ptr::without_provenance(INITIALIZING_TAG)
}

#[inline]
fn inherited_stale_ptr() -> *const PerfState {
  ptr::without_provenance(INHERITED_STALE_TAG)
}

#[inline]
fn committing_ptr() -> *const PerfState {
  ptr::without_provenance(COMMITTING_TAG)
}

fn atfork_registered() -> bool {
  *ATFORK_REGISTERED.get_or_init(|| {
    // SAFETY: the callback has the required C ABI and touches only the
    // current thread's native TLS slot. Failure simply makes perf ineligible.
    unsafe { libc::pthread_atfork(None, None, Some(after_fork_child)) == 0 }
  })
}

unsafe extern "C" fn after_fork_child() {
  // A pid=0 event remains attached to the parent task. Mark the inherited
  // owner stale without touching its mapping in the atfork callback; the first
  // ordinary child read retires it and runs a fresh profitability choice.
  set_hot(inherited_stale_ptr());
}

#[cold]
fn reinitialize_after_fork() -> u64 {
  // A fork can run inside a signal that interrupted an outer perf read. Keep
  // the inherited allocation and mapping alive until thread teardown so that
  // interrupted frame can safely unwind after this nested read completes.
  set_hot(initializing_ptr());
  set_commit_state(ptr::null());
  if !retire_current_state() {
    set_hot(inherited_stale_ptr());
    return super::posix_now_nanos();
  }
  set_hot(ptr::null());
  initialize_current_thread()
}

fn retire_current_state() -> bool {
  OWNER
    .try_with(|owner| {
      let Ok(mut owner) = owner.try_borrow_mut() else {
        return false;
      };
      let Some(state) = owner.state.take() else {
        return true;
      };
      // A frame interrupted before the fork can resume after a nested child
      // read. Mark its inherited route obsolete before publishing the fresh
      // child state, so that frame follows the existing second path load into
      // the child's current provider instead of sampling the parent event.
      // SAFETY: the allocation remains live on the retired list below.
      unsafe { state.as_ref().selected_path.store(PATH_OBSOLETE, Ordering::Release) };
      // `retired_next` is interior mutable specifically because an outer
      // interrupted reader can still hold a shared reference to this state.
      // SAFETY: the pointer remains allocated and owner-held.
      unsafe { state.as_ref().retired_next.set(owner.retired) };
      owner.retired = Some(state);
      true
    })
    .unwrap_or(false)
}

fn close_fd(fd: libc::c_int) {
  // SAFETY: best-effort close of a descriptor uniquely owned by PerfState.
  unsafe {
    libc::close(fd);
  }
}

fn map_perf_page(fd: libc::c_int) -> Option<(NonNull<PerfEventMmapPage>, usize)> {
  // SAFETY: sysconf has no pointer or lifetime preconditions.
  let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
  let map_len = usize::try_from(page_size).ok()?;
  if map_len < mem::size_of::<PerfEventMmapPage>() {
    return None;
  }
  // SAFETY: maps the metadata page of the live event fd read-only and shared,
  // as required by the perf mmap ABI.
  let mapped =
    unsafe { libc::mmap(ptr::null_mut(), map_len, libc::PROT_READ, libc::MAP_SHARED, fd, 0) };
  if mapped == libc::MAP_FAILED {
    return None;
  }
  let Some(page) = NonNull::new(mapped.cast::<PerfEventMmapPage>()) else {
    // SAFETY: `mapped` and `map_len` came from the successful mmap above.
    unsafe {
      libc::munmap(mapped, map_len);
    }
    return None;
  };
  Some((page, map_len))
}

fn mmap_counter_execution_eligible() -> bool {
  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  {
    x86_tsc_execution_eligible()
  }
  #[cfg(target_arch = "aarch64")]
  {
    crate::arch::linux_aarch64_wall::counter_user_read_eligible()
  }
  #[cfg(target_arch = "arm")]
  {
    // ARM checks cap_user_time under the metadata seqlock before executing the
    // potentially trap-emulated architectural timer read.
    true
  }
  #[cfg(target_arch = "riscv64")]
  {
    crate::arch::riscv64::rdtime_user_eligible()
  }
  #[cfg(any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"))]
  {
    false
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn read_sched_clock_counter(_counter_kind: u8) -> u64 {
  #[cfg(target_arch = "x86")]
  {
    crate::arch::x86::read_perf_seqlock_counter(_counter_kind)
  }
  #[cfg(target_arch = "x86_64")]
  {
    crate::arch::x86_64::read_perf_seqlock_counter(_counter_kind)
  }
  #[cfg(target_arch = "aarch64")]
  {
    let cycle: u64;
    if _counter_kind == PERF_COUNTER_AARCH64_SELF_SYNC {
      // SAFETY: HWCAP2_ECV proves an EL0-usable self-synchronizing counter.
      // CNTVCTSS orders prior metadata observations; the trailing ISB keeps
      // the closing sequence load after the counter.
      unsafe {
        core::arch::asm!(
          "mrs {}, S3_3_C14_C0_6",
          "isb",
          out(reg) cycle,
          options(nostack, preserves_flags),
        );
      }
    } else {
      // SAFETY: the leading ISB orders metadata observations before CNTVCT;
      // the trailing ISB orders CNTVCT before the closing sequence load.
      unsafe {
        core::arch::asm!(
          "isb sy",
          "mrs {}, cntvct_el0",
          "isb",
          out(reg) cycle,
          options(nostack, preserves_flags),
        );
      }
    }
    cycle
  }
  #[cfg(target_arch = "arm")]
  {
    let low: u32;
    let high: u32;
    // SAFETY: `cap_user_time` was observed under the perf metadata seqlock
    // before the first read. On 32-bit Arm, the kernel advertises that bit for
    // ARM PMUv3 only when sched_clock is the EL0-readable architectural timer.
    unsafe {
      core::arch::asm!(
        "isb sy",
        "mrrc p15, 1, {low}, {high}, c14",
        "isb sy",
        low = out(reg) low,
        high = out(reg) high,
        options(nostack, preserves_flags),
      );
    }
    (u64::from(high) << 32) | u64::from(low)
  }
  #[cfg(target_arch = "riscv64")]
  {
    crate::arch::riscv64::rdtime_perf_seqlock()
  }
  #[cfg(any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"))]
  {
    0
  }
}

#[cfg(target_arch = "x86")]
const fn default_perf_counter_kind() -> u8 {
  crate::arch::x86::PERF_SEQ_CPUID
}

#[cfg(target_arch = "x86_64")]
const fn default_perf_counter_kind() -> u8 {
  crate::arch::x86_64::PERF_SEQ_CPUID
}

#[cfg(target_arch = "aarch64")]
const fn default_perf_counter_kind() -> u8 {
  PERF_COUNTER_AARCH64_ISB
}

#[cfg(target_arch = "arm")]
const fn default_perf_counter_kind() -> u8 {
  PERF_COUNTER_ARM_ISB
}

#[cfg(target_arch = "riscv64")]
const fn default_perf_counter_kind() -> u8 {
  0
}

#[cfg(any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"))]
const fn default_perf_counter_kind() -> u8 {
  0
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn x86_tsc_execution_eligible() -> bool {
  const PR_GET_TSC: libc::c_int = 25;
  const PR_TSC_ENABLE: libc::c_int = 1;

  let mut mode = 0;
  // SAFETY: PR_GET_TSC writes one c_int through this valid pointer.
  if unsafe { libc::prctl(PR_GET_TSC, &mut mode as *mut libc::c_int) } != 0 || mode != PR_TSC_ENABLE
  {
    return false;
  }

  #[cfg(target_arch = "x86")]
  use core::arch::x86::__cpuid;
  #[cfg(target_arch = "x86_64")]
  use core::arch::x86_64::__cpuid;
  // cap_user_time separately proves that TSC supplies perf's stable sched_clock
  // conversion. This gate proves only instruction execution and deliberately
  // does not depend on clocksource sysfs being visible in this mount namespace.
  #[allow(unused_unsafe)]
  let maximum_leaf = {
    // SAFETY: supported x86 targets guarantee CPUID leaf zero.
    unsafe { __cpuid(0) }.eax
  };
  if maximum_leaf < 1 {
    return false;
  }
  #[allow(unused_unsafe)]
  let features = {
    // SAFETY: the maximum basic CPUID leaf includes leaf one.
    unsafe { __cpuid(1) }
  };
  features.edx & (1 << 4) != 0
}

#[inline]
fn counter_metadata_eligible(capabilities: u64, time_shift: u16, time_mult: u32) -> bool {
  capabilities & CAP_USER_TIME != 0 && time_shift < 64 && time_mult != 0
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn task_clock_nanos(snapshot: TaskClockSnapshot) -> Option<u64> {
  if snapshot.index != 0
    || snapshot.enabled != snapshot.running
    || !counter_metadata_eligible(snapshot.capabilities, snapshot.time_shift, snapshot.time_mult)
  {
    return None;
  }

  let cycle = if snapshot.capabilities & CAP_USER_TIME_SHORT != 0 {
    snapshot
      .time_cycles
      .wrapping_add(snapshot.cycle.wrapping_sub(snapshot.time_cycles) & snapshot.time_mask)
  } else {
    snapshot.cycle
  };

  let shift = u32::from(snapshot.time_shift);
  let remainder_mask = if shift == 0 { 0 } else { (1_u64 << shift) - 1 };
  let quotient = cycle >> shift;
  let remainder = cycle & remainder_mask;
  let mult = u64::from(snapshot.time_mult);
  let converted = quotient.wrapping_mul(mult).wrapping_add(remainder.wrapping_mul(mult) >> shift);

  // The generic mmap count offset can lag this software event at mmap and
  // schedule-in publication. For a standalone, always-enabled task-clock,
  // time_enabled is the frozen current-thread timeline at publication and the
  // architectural conversion supplies only its active segment after that.
  Some(snapshot.enabled.wrapping_add(snapshot.time_offset).wrapping_add(converted))
}

fn task_clock_short_period_nanos(snapshot: TaskClockSnapshot) -> Option<Option<u64>> {
  if snapshot.capabilities & CAP_USER_TIME_SHORT == 0 || snapshot.time_mask == u64::MAX {
    return Some(None);
  }
  if snapshot.time_mask == 0 || snapshot.time_mask & snapshot.time_mask.wrapping_add(1) != 0 {
    return None;
  }
  let period_cycles = u128::from(snapshot.time_mask.checked_add(1)?);
  let shift = u32::from(snapshot.time_shift);
  let mult = u128::from(snapshot.time_mult);
  let quotient = period_cycles >> shift;
  let remainder_mask = if shift == 0 { 0 } else { (1_u128 << shift) - 1 };
  let remainder = period_cycles & remainder_mask;
  let nanos = quotient.checked_mul(mult)?.checked_add(remainder.checked_mul(mult)? >> shift)?;
  if nanos <= 3 {
    return None;
  }
  if nanos > u128::from(u64::MAX) { Some(None) } else { Some(Some(nanos as u64)) }
}

fn task_clock_max_read_gap_nanos(period_nanos: Option<u64>) -> Option<Option<u64>> {
  let Some(period) = period_nanos else {
    return Some(None);
  };
  let half_period = period / 2;
  (half_period > 1).then_some(Some(half_period - 1))
}

fn fallback_marker_for_sample(last: u64, sample: u64) -> u64 {
  if crate::thread_cpu::is_wall_value(sample) {
    return FALLBACK_WALL;
  }
  let bias = last.saturating_sub(sample);
  if bias < FALLBACK_WALL { bias } else { FALLBACK_WALL }
}

fn biased_fallback_candidate(sample: u64, bias: u64, last: u64) -> u64 {
  sample.checked_add(bias).unwrap_or(last).max(last)
}

fn extend_short_candidate(candidate: u64, last: u64, period: u64) -> ShortCounterDecision {
  let half_period = period / 2;
  if half_period <= 1 {
    return ShortCounterDecision::Invalid;
  }
  if candidate >= last {
    return if candidate - last < half_period {
      ShortCounterDecision::Publish(candidate)
    } else {
      ShortCounterDecision::Invalid
    };
  }

  let deficit = last - candidate;
  if deficit < half_period {
    // A nested signal completed a newer sample after this older snapshot was
    // captured. A fresh raw counter disambiguates it from an actual wrap.
    return ShortCounterDecision::Retry;
  }
  if deficit == half_period {
    return ShortCounterDecision::Invalid;
  }
  let Some(rounded) = deficit.checked_add(period - 1) else {
    return ShortCounterDecision::Invalid;
  };
  let wraps = rounded / period;
  let Some(extension) = wraps.checked_mul(period) else {
    return ShortCounterDecision::Invalid;
  };
  let Some(extended) = candidate.checked_add(extension) else {
    return ShortCounterDecision::Invalid;
  };
  if extended < last || extended - last >= half_period {
    ShortCounterDecision::Invalid
  } else {
    ShortCounterDecision::Publish(extended)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[cfg(target_os = "linux")]
  static NESTED_FORK_CHILD_PID: core::sync::atomic::AtomicI32 =
    core::sync::atomic::AtomicI32::new(0);
  #[cfg(target_os = "linux")]
  static IS_NESTED_FORK_CHILD: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
  #[cfg(target_os = "linux")]
  static NESTED_FORK_SAMPLE: AtomicU64 = AtomicU64::new(0);
  #[cfg(target_os = "linux")]
  static NESTED_FORK_READS_VALID: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

  #[cfg(target_os = "linux")]
  fn is_exact_thread_cpu_provider(provider: ThreadCpuProvider) -> bool {
    matches!(
      provider,
      ThreadCpuProvider::LinuxPerfMmap
        | ThreadCpuProvider::LinuxPerfRead
        | ThreadCpuProvider::PosixThreadCpuClock
    )
  }

  #[cfg(target_os = "linux")]
  unsafe extern "C" fn fork_from_nested_signal(_: libc::c_int) {
    // SAFETY: this is the deliberate fork-under-signal regression. The
    // harness process has one thread, and both descendants terminate with
    // `_exit` without running inherited Rust cleanup.
    let child = unsafe { libc::fork() };
    if child == 0 {
      IS_NESTED_FORK_CHILD.store(true, Ordering::Release);
      let first = now_nanos();
      let selected = provider();
      let second = now_nanos();
      NESTED_FORK_SAMPLE.store(second, Ordering::Release);
      NESTED_FORK_READS_VALID
        .store(is_exact_thread_cpu_provider(selected) && second >= first, Ordering::Release);
      return;
    }
    NESTED_FORK_CHILD_PID.store(child, Ordering::Release);
  }

  #[cfg(target_os = "linux")]
  fn run_nested_signal_fork_harness(point: u8) -> ! {
    NESTED_FORK_CHILD_PID.store(0, Ordering::Relaxed);
    IS_NESTED_FORK_CHILD.store(false, Ordering::Relaxed);
    NESTED_FORK_SAMPLE.store(0, Ordering::Relaxed);
    NESTED_FORK_READS_VALID.store(false, Ordering::Relaxed);

    // SAFETY: zero is a valid initial representation for sigaction, and every
    // field needed by this handler is initialized before installation.
    let mut action = unsafe { mem::zeroed::<libc::sigaction>() };
    action.sa_sigaction = fork_from_nested_signal as *const () as usize;
    action.sa_flags = libc::SA_RESTART;
    // SAFETY: `action.sa_mask` and `action` are valid writable/readable
    // sigaction objects owned by this one-thread harness process.
    if unsafe {
      libc::sigemptyset(&mut action.sa_mask);
      libc::sigaction(libc::SIGUSR2, &action, ptr::null_mut())
    } != 0
    {
      // SAFETY: terminate the isolated harness without inherited cleanup.
      unsafe { libc::_exit(2) }
    }

    // Prevent a regression from hanging the full test binary indefinitely.
    // SAFETY: alarm has no caller-side memory preconditions.
    unsafe { libc::alarm(30) };

    if point == TEST_FORK_DURING_HOT_READ {
      // This lifetime regression needs a real hot perf pointer even on a host
      // where the profitability tournament correctly prefers POSIX. Install
      // the exact persistent-read candidate directly; production selection is
      // unchanged, while the interrupted frame executes the production hot
      // path and the kernel's real thread-bound fd.
      set_hot(initializing_ptr());
      set_commit_state(ptr::null());
      if !retire_current_state() {
        unsafe { libc::_exit(76) }
      }
      set_hot(ptr::null());
      let Some(state) = PerfState::open() else { unsafe { libc::_exit(77) } };
      let Some(state_ptr) = install_state(state) else { unsafe { libc::_exit(76) } };
      // SAFETY: install_state made this allocation the harness thread's
      // current owner-held state. PerfState::open validated PATH_READ once.
      unsafe { (*state_ptr).selected_path.store(PATH_READ, Ordering::Release) };
    }

    TEST_FORK_TID.store(crate::arch::current_thread_id(), Ordering::Relaxed);
    TEST_FORK_POINT.store(point, Ordering::Release);
    let interrupted_sample = now_nanos();

    if IS_NESTED_FORK_CHILD.load(Ordering::Acquire) {
      // The signal handler returned in the fork child, so the interrupted
      // provider frame has now unwound through the exact code under test.
      let nested_sample = NESTED_FORK_SAMPLE.load(Ordering::Acquire);
      let final_sample = now_nanos();
      let valid = NESTED_FORK_READS_VALID.load(Ordering::Acquire)
        && is_exact_thread_cpu_provider(provider())
        && interrupted_sample >= nested_sample
        && final_sample >= interrupted_sample;
      // SAFETY: terminate the nested child without touching inherited cleanup.
      unsafe { libc::_exit(if valid { 0 } else { 1 }) }
    }

    TEST_FORK_POINT.store(0, Ordering::Release);
    let child = NESTED_FORK_CHILD_PID.load(Ordering::Acquire);
    if child == 0 {
      // No perf state reached the injected point (normally a paranoid gate).
      unsafe { libc::_exit(77) }
    }
    if child < 0 {
      unsafe { libc::_exit(3) }
    }
    let mut status = 0;
    // SAFETY: `child` was created synchronously by the handler and status is
    // valid wait storage.
    if unsafe { libc::waitpid(child, &mut status, 0) } != child {
      unsafe { libc::_exit(4) }
    }
    let passed = libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0;
    unsafe { libc::_exit(if passed { 0 } else { 5 }) }
  }

  #[cfg(target_os = "linux")]
  fn assert_nested_signal_fork_safe(point: u8) {
    // Run outside the test harness process so the signal-side fork has only
    // one live thread and cannot inherit another test's allocator activity.
    // SAFETY: the child enters the isolated harness and exits via `_exit`.
    let harness = unsafe { libc::fork() };
    assert!(harness >= 0);
    if harness == 0 {
      run_nested_signal_fork_harness(point);
    }
    let mut status = 0;
    // SAFETY: `harness` is live and status is writable wait storage.
    assert_eq!(unsafe { libc::waitpid(harness, &mut status, 0) }, harness);
    if libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 77 {
      std::eprintln!("nested signal/fork point {point} skipped: no eligible perf route");
      return;
    }
    assert_eq!(status, 0, "nested signal/fork harness failed with wait status {status}");
    std::eprintln!("nested signal/fork point {point} exercised an eligible perf route");
  }

  fn snapshot() -> TaskClockSnapshot {
    TaskClockSnapshot {
      index: 0,
      enabled: 5_000,
      running: 5_000,
      capabilities: CAP_USER_TIME,
      time_shift: 2,
      time_mult: 4,
      time_offset: 100,
      time_cycles: 0,
      time_mask: u64::MAX,
      cycle: 25,
    }
  }

  #[test]
  fn task_clock_conversion_uses_enabled_plus_uapi_delta() {
    // 25 cycles with shift=2 and mult=4 converts to 25 ns.
    assert_eq!(task_clock_nanos(snapshot()), Some(5_125));
  }

  #[test]
  fn task_clock_conversion_extends_a_short_counter() {
    let mut value = snapshot();
    value.capabilities |= CAP_USER_TIME_SHORT;
    value.time_shift = 0;
    value.time_mult = 1;
    value.time_offset = 0;
    value.time_cycles = 0x1f0;
    value.time_mask = 0xff;
    value.cycle = 0x12;
    assert_eq!(task_clock_nanos(value), Some(5_000 + 0x212));
    assert_eq!(task_clock_short_period_nanos(value), Some(Some(0x100)));
    assert_eq!(task_clock_max_read_gap_nanos(Some(0x100)), Some(Some(0x7f)));
  }

  #[test]
  fn full_width_counter_has_no_practical_read_gap() {
    assert_eq!(task_clock_short_period_nanos(snapshot()), Some(None));
    assert_eq!(task_clock_max_read_gap_nanos(None), Some(None));
  }

  #[test]
  fn task_clock_conversion_rejects_ineligible_metadata() {
    let mut value = snapshot();
    value.index = 1;
    assert_eq!(task_clock_nanos(value), None);

    let mut value = snapshot();
    value.running -= 1;
    assert_eq!(task_clock_nanos(value), None);

    let mut value = snapshot();
    value.capabilities = 0;
    assert_eq!(task_clock_nanos(value), None);

    let mut value = snapshot();
    value.time_mult = 0;
    assert_eq!(task_clock_nanos(value), None);
  }

  #[test]
  fn short_counter_extension_owns_wraps_and_retries_stale_nested_reads() {
    assert_eq!(extend_short_candidate(0x200, 0x2ff, 0x100), ShortCounterDecision::Publish(0x300),);
    assert_eq!(extend_short_candidate(0x212, 0x3ff, 0x100), ShortCounterDecision::Publish(0x412),);
    assert_eq!(extend_short_candidate(0x2ff, 0x310, 0x100), ShortCounterDecision::Retry,);
    assert_eq!(extend_short_candidate(0x380, 0x300, 0x100), ShortCounterDecision::Invalid,);
  }

  #[test]
  fn short_counter_metadata_requires_a_power_of_two_window() {
    let mut value = snapshot();
    value.capabilities |= CAP_USER_TIME_SHORT;
    value.time_shift = 0;
    value.time_mult = 1;
    value.time_mask = 0xfe;
    assert_eq!(task_clock_short_period_nanos(value), None);
  }

  #[test]
  fn counter_permission_gate_requires_complete_conversion_metadata() {
    assert!(counter_metadata_eligible(CAP_USER_TIME, 0, 1));
    assert!(!counter_metadata_eligible(0, 0, 1));
    assert!(!counter_metadata_eligible(CAP_USER_TIME, 64, 1));
    assert!(!counter_metadata_eligible(CAP_USER_TIME, 0, 0));
  }

  #[test]
  fn selection_requires_a_repeatable_material_win() {
    assert!(!prefer_perf([99; MEASURE_SAMPLES], [100; MEASURE_SAMPLES]));

    let syscall = [100_000; MEASURE_SAMPLES];
    let decisive_perf = [80_000; MEASURE_SAMPLES];
    assert!(prefer_perf(decisive_perf, syscall));

    let mut one_noisy_batch = decisive_perf;
    one_noisy_batch[0] = 99_000;
    assert!(prefer_perf(one_noisy_batch, syscall));

    let mut two_noisy_batches = decisive_perf;
    two_noisy_batches[0] = 99_000;
    two_noisy_batches[1] = 99_000;
    assert!(!prefer_perf(two_noisy_batches, syscall));
  }

  #[test]
  fn failure_fallback_keeps_the_measured_fastest_remaining_path() {
    let measurements = PerfPathMeasurements {
      mmap: Some([40_000; MEASURE_SAMPLES]),
      read: [60_000; MEASURE_SAMPLES],
      syscall: [100_000; MEASURE_SAMPLES],
    };
    let selected = fastest_path_excluding(&measurements, true, 0, 0).unwrap();
    assert_eq!(selected, PATH_MMAP);
    assert_eq!(fastest_path_excluding(&measurements, true, selected, 0), Some(PATH_READ),);

    let posix_second = PerfPathMeasurements {
      mmap: Some([40_000; MEASURE_SAMPLES]),
      read: [120_000; MEASURE_SAMPLES],
      syscall: [60_000; MEASURE_SAMPLES],
    };
    let selected = fastest_path_excluding(&posix_second, true, 0, 0).unwrap();
    assert_eq!(selected, PATH_MMAP);
    assert_eq!(fastest_path_excluding(&posix_second, true, selected, 0), Some(PATH_POSIX),);

    let posix_selected = PerfPathMeasurements {
      mmap: Some([120_000; MEASURE_SAMPLES]),
      read: [80_000; MEASURE_SAMPLES],
      syscall: [40_000; MEASURE_SAMPLES],
    };
    let selected = fastest_path_excluding(&posix_selected, true, 0, 0).unwrap();
    assert_eq!(selected, PATH_POSIX);
    assert_eq!(fastest_path_excluding(&posix_selected, true, selected, 0), Some(PATH_READ),);
  }

  #[test]
  fn commit_never_reenables_a_path_that_failed_after_measurement() {
    let measurements = PerfPathMeasurements {
      mmap: Some([40_000; MEASURE_SAMPLES]),
      read: [60_000; MEASURE_SAMPLES],
      syscall: [100_000; MEASURE_SAMPLES],
    };
    let mmap_failed = path_mask(PATH_MMAP);
    assert_eq!(fastest_path_excluding(&measurements, true, 0, mmap_failed), Some(PATH_READ),);
    let both_perf_failed = mmap_failed | path_mask(PATH_READ);
    assert_eq!(fastest_path_excluding(&measurements, true, 0, both_perf_failed), Some(PATH_POSIX),);
  }

  #[test]
  fn event_epoch_alignment_makes_degradation_step_forward() {
    let offset = align_event_epoch(10_000, 2_000).unwrap();
    let last_perf = 2_500 + offset;
    let first_syscall = 10_520;
    assert_eq!(last_perf, 10_500);
    assert!(first_syscall >= last_perf);
    assert!(align_event_epoch(1, 2).is_none());
  }

  #[test]
  fn native_fallback_is_biased_to_the_last_perf_sample() {
    let last_perf = 10_500;
    let first_native = 10_000;
    let bias = fallback_marker_for_sample(last_perf, first_native);
    assert_eq!(bias, 500);
    assert_eq!(biased_fallback_candidate(first_native, bias, last_perf), last_perf);
    assert_eq!(biased_fallback_candidate(10_020, bias, last_perf), 10_520);

    assert_eq!(fallback_marker_for_sample(last_perf, 11_000), 0);
    assert_eq!(biased_fallback_candidate(11_000, 0, last_perf), 11_000);
    assert_eq!(
      fallback_marker_for_sample(last_perf, crate::thread_cpu::encode_wall_ticks(1)),
      FALLBACK_WALL,
    );
  }

  #[test]
  fn metadata_prefix_matches_linux_uapi_offsets() {
    assert_eq!(mem::size_of::<PerfEventAttrV0>(), 64);
    assert_eq!(mem::offset_of!(PerfEventMmapPage, lock), 8);
    assert_eq!(mem::offset_of!(PerfEventMmapPage, index), 12);
    assert_eq!(mem::offset_of!(PerfEventMmapPage, time_enabled), 24);
    assert_eq!(mem::offset_of!(PerfEventMmapPage, capabilities), 40);
    assert_eq!(mem::offset_of!(PerfEventMmapPage, time_shift), 50);
    assert_eq!(mem::offset_of!(PerfEventMmapPage, time_cycles), 80);
    assert_eq!(mem::offset_of!(PerfEventMmapPage, time_mask), 88);
  }

  #[test]
  fn available_perf_task_clock_matches_native_thread_cpu_semantics() {
    let Some(state) = PerfState::open() else {
      return;
    };

    let mut previous = state.read().expect("eligible perf state must read");
    for _ in 0..8 {
      let native_start = super::super::posix_now_nanos();
      let perf_start = state.read().expect("perf start");
      let mut accumulator = 1_u64;
      for value in 0..250_000_u64 {
        accumulator = black_box(accumulator.wrapping_mul(6364136223846793005).wrapping_add(value));
      }
      black_box(accumulator);
      let perf_end = state.read().expect("perf end");
      let native_end = super::super::posix_now_nanos();
      assert!(perf_start >= previous && perf_end >= perf_start);
      previous = perf_end;

      let perf_delta = perf_end - perf_start;
      let native_delta = native_end.saturating_sub(native_start);
      assert!(perf_delta > 100_000, "busy interval was too short: {perf_delta} ns");
      let tolerance = 100_000_u64.max(native_delta / 5);
      assert!(
        perf_delta.abs_diff(native_delta) <= tolerance,
        "perf task-clock delta {perf_delta} diverged from native {native_delta} by more than {tolerance} ns",
      );
    }

    let native_start = super::super::posix_now_nanos();
    let perf_start = state.read().expect("perf sleep start");
    std::thread::sleep(core::time::Duration::from_millis(25));
    let perf_end = state.read().expect("perf sleep end");
    let native_end = super::super::posix_now_nanos();
    let perf_delta = perf_end.saturating_sub(perf_start);
    let native_delta = native_end.saturating_sub(native_start);
    assert!(perf_end >= perf_start);
    assert!(perf_delta < 5_000_000, "perf advanced across sleep by {perf_delta} ns");
    assert!(native_delta < 5_000_000, "native clock advanced across sleep by {native_delta} ns");
    assert!(
      perf_delta.abs_diff(native_delta) <= 500_000,
      "sleep overhead differs: perf {perf_delta} ns, native {native_delta} ns",
    );
  }

  #[test]
  fn inherited_perf_mapping_is_reinitialized_after_fork() {
    let _ = now_nanos();

    // SAFETY: the child performs only tach reads and `_exit`; the parent
    // immediately waits for this exact process.
    let child = unsafe { libc::fork() };
    assert!(child >= 0);
    if child == 0 {
      let inherited_was_marked_stale = hot().addr() == INHERITED_STALE_TAG;
      let provider = provider();
      let first = now_nanos();
      let second = now_nanos();
      let refreshed = hot().addr() != INHERITED_STALE_TAG;
      let valid_provider = matches!(
        provider,
        ThreadCpuProvider::LinuxPerfMmap
          | ThreadCpuProvider::LinuxPerfRead
          | ThreadCpuProvider::PosixThreadCpuClock
      );
      // SAFETY: `_exit` terminates without inherited Rust cleanup.
      unsafe {
        libc::_exit(
          if inherited_was_marked_stale && refreshed && valid_provider && second >= first {
            0
          } else {
            1
          },
        )
      };
    }
    let mut status = 0;
    // SAFETY: `child` is live and `status` is writable wait storage.
    assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
    assert_eq!(status, 0);
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn nested_signal_fork_cannot_invalidate_an_interrupted_provider_frame() {
    assert_nested_signal_fork_safe(TEST_FORK_DURING_COMMIT);
    assert_nested_signal_fork_safe(TEST_FORK_DURING_HOT_READ);
  }

  #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
  #[test]
  fn initial_counter_denial_never_executes_the_architectural_counter() {
    const PR_SET_TSC: libc::c_int = 26;
    const PR_TSC_SIGSEGV: libc::c_ulong = 2;

    // SAFETY: the child changes only its own counter permission, exercises the
    // setup gate, and exits without inherited Rust cleanup.
    let child = unsafe { libc::fork() };
    assert!(child >= 0);
    if child == 0 {
      let status = unsafe { libc::prctl(PR_SET_TSC, PR_TSC_SIGSEGV) };
      if status != 0 {
        // Older arm64 kernels do not implement PR_SET_TSC.
        unsafe { libc::_exit(77) };
      }
      let mmap_unavailable = PerfState::open().is_none_or(|state| !state.mmap_available());
      let _ = now_nanos();
      let did_not_select_mmap = provider() != ThreadCpuProvider::LinuxPerfMmap;
      unsafe { libc::_exit(if mmap_unavailable && did_not_select_mmap { 0 } else { 1 }) };
    }
    let mut status = 0;
    assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
    if libc::WIFSIGNALED(status) {
      panic!("counter-denial child terminated by signal {}", libc::WTERMSIG(status));
    }
    assert!(libc::WIFEXITED(status), "counter-denial child returned wait status {status}");
    let exit_status = libc::WEXITSTATUS(status);
    if exit_status == 77 {
      return;
    }
    assert_eq!(exit_status, 0, "counter-denial child exited unsuccessfully");
  }
}
