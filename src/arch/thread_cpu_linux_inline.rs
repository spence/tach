//! Measured Linux perf task-clock provider for current-thread CPU time.
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
#[cfg(feature = "bench-internal")]
use core::sync::atomic::AtomicU64;
use core::sync::atomic::{AtomicU8, Ordering, compiler_fence};

use std::sync::OnceLock;

use crate::{ThreadCpuProvider, ThreadCpuReadCost};

const PERF_TYPE_SOFTWARE: u32 = 1;
const PERF_COUNT_SW_TASK_CLOCK: u64 = 1;
const PERF_FLAG_FD_CLOEXEC: libc::c_ulong = 1 << 3;
const CAP_USER_TIME: u64 = 1 << 3;
const CAP_USER_TIME_SHORT: u64 = 1 << 5;

const SYSCALL_TAG: usize = 1;
const INITIALIZING_TAG: usize = 2;
const MAX_SEQ_RETRIES: usize = 16;

const GLOBAL_UNSELECTED: u8 = 0;
const GLOBAL_SELECTING: u8 = 1;
const GLOBAL_PERF: u8 = 2;
const GLOBAL_SYSCALL: u8 = 3;

const WARMUP_READS: usize = 128;
const MEASURE_READS: usize = 4_096;
const MEASURE_SAMPLES: usize = 9;
const REQUIRED_DECISIVE_WINS: usize = 8;

std::thread_local! {
  // Null means uninitialized, 1 means sticky syscall, 2 means initialization
  // is in progress, and any aligned pointer addresses an owner-held PerfState.
  static HOT_STATE: Cell<*const PerfState> = const { Cell::new(ptr::null()) };
  static OWNER: RefCell<ThreadOwner> = const { RefCell::new(ThreadOwner { state: None }) };
}

static ATFORK_REGISTERED: OnceLock<bool> = OnceLock::new();
static GLOBAL_CHOICE: AtomicU8 = AtomicU8::new(GLOBAL_UNSELECTED);
#[cfg(feature = "bench-internal")]
static MEASURED_PERF_SAMPLES: [AtomicU64; MEASURE_SAMPLES] =
  [const { AtomicU64::new(0) }; MEASURE_SAMPLES];
#[cfg(feature = "bench-internal")]
static MEASURED_SYSCALL_SAMPLES: [AtomicU64; MEASURE_SAMPLES] =
  [const { AtomicU64::new(0) }; MEASURE_SAMPLES];

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
  fd: libc::c_int,
  page: NonNull<PerfEventMmapPage>,
  map_len: usize,
  epoch_offset: u64,
}

#[cfg(feature = "bench-internal")]
pub(super) struct BenchPerfHandle(*const PerfState);

struct ThreadOwner {
  state: Option<PerfState>,
}

enum GlobalRole {
  Selector,
  Perf,
  Syscall,
}

struct GlobalSelectionGuard {
  committed: bool,
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

impl PerfState {
  fn open() -> Option<Self> {
    if !atfork_registered() {
      return None;
    }

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
    if fd < 0 || fd > libc::c_int::MAX.into() {
      return None;
    }
    let fd = fd as libc::c_int;

    // SAFETY: sysconf has no pointer or lifetime preconditions.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let Ok(map_len) = usize::try_from(page_size) else {
      close_fd(fd);
      return None;
    };
    if map_len < mem::size_of::<PerfEventMmapPage>() {
      close_fd(fd);
      return None;
    }

    // SAFETY: maps the metadata page of the live event fd read-only and
    // shared, as required by the perf mmap ABI.
    let mapped =
      unsafe { libc::mmap(ptr::null_mut(), map_len, libc::PROT_READ, libc::MAP_SHARED, fd, 0) };
    if mapped == libc::MAP_FAILED {
      close_fd(fd);
      return None;
    }

    let Some(page) = NonNull::new(mapped.cast::<PerfEventMmapPage>()) else {
      // A successful non-fixed mmap cannot return null, but keep cleanup
      // complete if an unusual libc violates that contract.
      // SAFETY: `mapped` and `map_len` came from the successful mmap above.
      unsafe {
        libc::munmap(mapped, map_len);
      }
      close_fd(fd);
      return None;
    };

    let mut state = Self { fd, page, map_len, epoch_offset: 0 };
    // Capability makes the path eligible. Profitability is decided later by
    // measuring the complete TLS + metadata read against the syscall path.
    let syscall_nanos = super::posix_now_nanos();
    if super::native_clock_uses_wall() {
      return None;
    }
    let event_nanos = state.read_event_nanos()?;
    // The software event starts at open while CLOCK_THREAD_CPUTIME_ID starts
    // at thread creation. Sampling the event after the syscall deliberately
    // biases this alignment slightly low, so a later syscall fallback can
    // only step forward.
    state.epoch_offset = align_event_epoch(syscall_nanos, event_nanos)?;
    state.read()?;
    Some(state)
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn read(&self) -> Option<u64> {
    self.read_event_nanos()?.checked_add(self.epoch_offset)
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn read_event_nanos(&self) -> Option<u64> {
    let page = self.page.as_ptr();

    for _ in 0..MAX_SEQ_RETRIES {
      // SAFETY: `page` points to the live, read-only perf metadata mapping.
      let sequence = unsafe { ptr::read_volatile(ptr::addr_of!((*page).lock)) };
      if sequence & 1 != 0 {
        core::hint::spin_loop();
        continue;
      }
      compiler_fence(Ordering::Acquire);

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

      let cycle = read_sched_clock_counter();

      compiler_fence(Ordering::Acquire);
      // SAFETY: `page` is still the same live metadata mapping.
      let after = unsafe { ptr::read_volatile(ptr::addr_of!((*page).lock)) };
      if sequence != after || after & 1 != 0 {
        core::hint::spin_loop();
        continue;
      }

      return task_clock_nanos(TaskClockSnapshot {
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
    // SAFETY: this state uniquely owns the mapping returned by mmap.
    unsafe {
      libc::munmap(self.page.as_ptr().cast::<c_void>(), self.map_len);
    }
    close_fd(self.fd);
  }
}

impl Drop for ThreadOwner {
  fn drop(&mut self) {
    // Publish the syscall path before unmapping so another TLS destructor on
    // this same thread cannot observe a stale metadata pointer.
    set_hot(syscall_ptr());
    let _ = self.state.take();
  }
}

impl GlobalSelectionGuard {
  fn new() -> Self {
    Self { committed: false }
  }

  fn commit(mut self, choice: u8) {
    GLOBAL_CHOICE.store(choice, Ordering::Release);
    self.committed = true;
  }
}

impl Drop for GlobalSelectionGuard {
  fn drop(&mut self) {
    if !self.committed {
      // Do not strand other threads if selection unwinds unexpectedly.
      GLOBAL_CHOICE.store(GLOBAL_SYSCALL, Ordering::Release);
    }
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub(super) fn now_nanos() -> u64 {
  let state = hot();
  match state.addr() {
    0 => initialize_current_thread(),
    SYSCALL_TAG => super::posix_now_nanos(),
    INITIALIZING_TAG => {
      // Recursive initialization cannot safely run perf setup twice. Making
      // the syscall choice sticky also keeps a recursive sample and every
      // later sample in one epoch.
      set_hot(syscall_ptr());
      super::posix_now_nanos()
    }
    _ => read_perf_or_degrade(state),
  }
}

pub(super) fn provider() -> ThreadCpuProvider {
  ensure_initialized();
  if hot().addr() > INITIALIZING_TAG {
    ThreadCpuProvider::LinuxPerfMmap
  } else {
    ThreadCpuProvider::PosixThreadCpuClock
  }
}

pub(super) fn read_cost_hint() -> ThreadCpuReadCost {
  ensure_initialized();
  if hot().addr() > INITIALIZING_TAG {
    ThreadCpuReadCost::Inline
  } else {
    ThreadCpuReadCost::SystemCall
  }
}

fn ensure_initialized() {
  if hot().is_null() {
    let _ = initialize_current_thread();
  }
}

#[cfg(feature = "bench-internal")]
pub(super) fn bench_selection_measurements()
-> Option<([u64; MEASURE_SAMPLES], [u64; MEASURE_SAMPLES], usize)> {
  ensure_initialized();
  let mut perf = [0; MEASURE_SAMPLES];
  let mut syscall = [0; MEASURE_SAMPLES];
  for (value, sample) in perf.iter_mut().zip(&MEASURED_PERF_SAMPLES) {
    *value = sample.load(Ordering::Relaxed);
  }
  for (value, sample) in syscall.iter_mut().zip(&MEASURED_SYSCALL_SAMPLES) {
    *value = sample.load(Ordering::Relaxed);
  }
  if perf.contains(&0) || syscall.contains(&0) {
    None
  } else {
    Some((perf, syscall, MEASURE_READS))
  }
}

#[cfg(feature = "bench-internal")]
pub(super) fn bench_perf_handle() -> Option<BenchPerfHandle> {
  ensure_initialized();
  let state = hot();
  (state.addr() > INITIALIZING_TAG).then_some(BenchPerfHandle(state))
}

#[cfg(feature = "bench-internal")]
impl BenchPerfHandle {
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(super) fn now_nanos(&self) -> u64 {
    // SAFETY: the handle is neither Send nor Sync and the thread's OWNER keeps
    // this state mapped until thread exit.
    unsafe { (*self.0).read().expect("selected perf mapping became unreadable") }
  }
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

  match global_role() {
    GlobalRole::Selector => select_process_provider(),
    GlobalRole::Perf => initialize_selected_perf(),
    GlobalRole::Syscall => {
      set_hot(syscall_ptr());
      super::posix_now_nanos()
    }
  }
}

#[cold]
fn select_process_provider() -> u64 {
  let guard = GlobalSelectionGuard::new();

  let Some(state) = PerfState::open() else {
    set_hot(syscall_ptr());
    guard.commit(GLOBAL_SYSCALL);
    return super::posix_now_nanos();
  };

  // A recursive read may have made the fallback sticky while setup ran.
  if hot().addr() != INITIALIZING_TAG {
    set_hot(syscall_ptr());
    guard.commit(GLOBAL_SYSCALL);
    return super::posix_now_nanos();
  }

  let Some(state_ptr) = install_state(state) else {
    set_hot(syscall_ptr());
    guard.commit(GLOBAL_SYSCALL);
    return super::posix_now_nanos();
  };

  let measurements = measure_exact_paths(state_ptr);
  #[cfg(feature = "bench-internal")]
  if let Some((perf, syscall)) = measurements.as_ref() {
    for (destination, value) in MEASURED_PERF_SAMPLES.iter().zip(perf) {
      destination.store(*value, Ordering::Relaxed);
    }
    for (destination, value) in MEASURED_SYSCALL_SAMPLES.iter().zip(syscall) {
      destination.store(*value, Ordering::Relaxed);
    }
  }
  let select_perf = measurements.map(|(perf, syscall)| prefer_perf(perf, syscall)).unwrap_or(false)
    && hot().addr() > INITIALIZING_TAG;

  if !select_perf {
    discard_state();
    guard.commit(GLOBAL_SYSCALL);
    return super::posix_now_nanos();
  }

  set_hot(state_ptr);
  guard.commit(GLOBAL_PERF);
  read_perf_or_degrade(state_ptr)
}

#[cold]
fn initialize_selected_perf() -> u64 {
  let Some(state) = PerfState::open() else {
    set_hot(syscall_ptr());
    return super::posix_now_nanos();
  };

  if hot().addr() != INITIALIZING_TAG {
    set_hot(syscall_ptr());
    return super::posix_now_nanos();
  }

  let Some(state_ptr) = install_state(state) else {
    set_hot(syscall_ptr());
    return super::posix_now_nanos();
  };
  set_hot(state_ptr);
  read_perf_or_degrade(state_ptr)
}

fn global_role() -> GlobalRole {
  loop {
    match GLOBAL_CHOICE.load(Ordering::Acquire) {
      GLOBAL_UNSELECTED => {
        if GLOBAL_CHOICE
          .compare_exchange(
            GLOBAL_UNSELECTED,
            GLOBAL_SELECTING,
            Ordering::AcqRel,
            Ordering::Acquire,
          )
          .is_ok()
        {
          return GlobalRole::Selector;
        }
      }
      GLOBAL_SELECTING => std::thread::yield_now(),
      GLOBAL_PERF => return GlobalRole::Perf,
      GLOBAL_SYSCALL => return GlobalRole::Syscall,
      _ => return GlobalRole::Syscall,
    }
  }
}

fn install_state(state: PerfState) -> Option<*const PerfState> {
  OWNER
    .try_with(|owner| {
      let Ok(mut owner) = owner.try_borrow_mut() else {
        return None;
      };
      if owner.state.is_some() {
        return None;
      }
      owner.state = Some(state);
      let state_ptr = owner.state.as_ref().map(ptr::from_ref)?;
      set_hot(state_ptr);
      Some(state_ptr)
    })
    .ok()
    .flatten()
}

fn discard_state() {
  set_hot(syscall_ptr());
  let state = OWNER
    .try_with(|owner| owner.try_borrow_mut().ok().and_then(|mut owner| owner.state.take()))
    .ok()
    .flatten();
  drop(state);
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn read_perf_or_degrade(state: *const PerfState) -> u64 {
  // SAFETY: non-sentinel HOT_STATE pointers address the PerfState held by this
  // thread's OWNER and remain live until its TLS destructor first publishes
  // the syscall sentinel.
  let value = unsafe { (*state).read() };
  if let Some(value) = value {
    value
  } else {
    // Keep the mapping owned until thread exit. A signal could have interrupted
    // an outer reader, so unmapping it on this hot failure path would make that
    // outer read access freed memory when it resumes.
    set_hot(syscall_ptr());
    super::posix_now_nanos()
  }
}

fn measure_exact_paths(
  state: *const PerfState,
) -> Option<([u64; MEASURE_SAMPLES], [u64; MEASURE_SAMPLES])> {
  set_hot(state);
  for _ in 0..WARMUP_READS {
    black_box(read_current_hot_path());
  }
  if hot() != state {
    return None;
  }

  set_hot(syscall_ptr());
  for _ in 0..WARMUP_READS {
    black_box(read_current_hot_path());
  }

  let mut perf_samples = [0_u64; MEASURE_SAMPLES];
  let mut syscall_samples = [0_u64; MEASURE_SAMPLES];

  for sample in 0..MEASURE_SAMPLES {
    if sample & 1 == 0 {
      perf_samples[sample] = measure_path(state)?;
      if hot() != state {
        return None;
      }
      syscall_samples[sample] = measure_path(syscall_ptr())?;
    } else {
      syscall_samples[sample] = measure_path(syscall_ptr())?;
      perf_samples[sample] = measure_path(state)?;
      if hot() != state {
        return None;
      }
    }
  }

  set_hot(state);
  Some((perf_samples, syscall_samples))
}

fn measure_path(state: *const PerfState) -> Option<u64> {
  set_hot(state);
  let start = monotonic_raw_nanos()?;
  for _ in 0..MEASURE_READS {
    black_box(read_current_hot_path());
  }
  let end = monotonic_raw_nanos()?;
  end.checked_sub(start)
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn read_current_hot_path() -> u64 {
  let state = hot();
  if state.addr() > INITIALIZING_TAG {
    read_perf_or_degrade(state)
  } else {
    super::posix_now_nanos()
  }
}

fn monotonic_raw_nanos() -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: value is writable timespec storage and CLOCK_MONOTONIC_RAW is a
  // valid Linux clock id.
  let status = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, value.as_mut_ptr()) };
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
  // deciding the process-wide provider. The 5% term scales that equivalence
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

#[inline]
fn set_hot(state: *const PerfState) {
  let _ = HOT_STATE.try_with(|slot| slot.set(state));
}

#[inline]
fn syscall_ptr() -> *const PerfState {
  ptr::without_provenance(SYSCALL_TAG)
}

#[inline]
fn initializing_ptr() -> *const PerfState {
  ptr::without_provenance(INITIALIZING_TAG)
}

fn atfork_registered() -> bool {
  *ATFORK_REGISTERED.get_or_init(|| {
    // SAFETY: the callback has the required C ABI and touches only the
    // current thread's native TLS slot. Failure simply makes perf ineligible.
    unsafe { libc::pthread_atfork(None, None, Some(after_fork_child)) == 0 }
  })
}

unsafe extern "C" fn after_fork_child() {
  // A perf event opened with pid=0 remains attached to the parent task after
  // fork. The child must never read that mapping. Keep it owned for eventual
  // cleanup but make the syscall provider sticky in the child.
  set_hot(syscall_ptr());
  let _ = GLOBAL_CHOICE.compare_exchange(
    GLOBAL_SELECTING,
    GLOBAL_SYSCALL,
    Ordering::AcqRel,
    Ordering::Acquire,
  );
}

fn close_fd(fd: libc::c_int) {
  // SAFETY: best-effort close of a descriptor uniquely owned by PerfState.
  unsafe {
    libc::close(fd);
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn read_sched_clock_counter() -> u64 {
  #[cfg(target_arch = "x86_64")]
  {
    crate::arch::x86_64::rdtsc()
  }
  #[cfg(target_arch = "aarch64")]
  {
    // The ISB pins the counter sample between the two metadata sequence reads.
    crate::arch::aarch64::cntvct_ordered()
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn task_clock_nanos(snapshot: TaskClockSnapshot) -> Option<u64> {
  if snapshot.index != 0
    || snapshot.enabled != snapshot.running
    || snapshot.capabilities & CAP_USER_TIME == 0
    || snapshot.time_shift >= 64
    || snapshot.time_mult == 0
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

  Some(snapshot.enabled.wrapping_add(snapshot.time_offset).wrapping_add(converted))
}

#[cfg(test)]
mod tests {
  use super::*;

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
  fn task_clock_conversion_uses_enabled_plus_counter_delta() {
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
  fn event_epoch_alignment_makes_degradation_step_forward() {
    let offset = align_event_epoch(10_000, 2_000).unwrap();
    let last_perf = 2_500 + offset;
    let first_syscall = 10_520;
    assert_eq!(last_perf, 10_500);
    assert!(first_syscall >= last_perf);
    assert!(align_event_epoch(1, 2).is_none());
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
}
