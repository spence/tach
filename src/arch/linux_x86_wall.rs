//! Runtime wall-clock selection for Linux-kernel x86 and x86_64 targets.
//!
//! Linux exposes the timestamp counter to EL0 through `RDTSC`, but a thread can
//! request `SIGSEGV` on counter access with `PR_SET_TSC(PR_TSC_SIGSEGV)`, and
//! the kernel can demote an unreliable TSC from its clocksource. Tach reads a
//! bare `RDTSC` for `Instant` and `lfence; rdtsc` for `OrderedInstant` only when
//! the counter is eligible: `PR_GET_TSC` reports it enabled, CPUID advertises an
//! invariant TSC, and the kernel's current clocksource is `tsc`. Both reads are
//! scaled by the calibrated counter frequency.
//!
//! A bare `RDTSC` is eligible for local monotonic `Instant` samples because the
//! reads order against one another; it is not ordered with surrounding work.
//! `OrderedInstant` additionally needs the read ordered after a prior Acquire
//! load, which `LFENCE` guarantees only on Intel or on AMD parts that set
//! `CPUID.8000_0021H:EAX[2]` (AMD_LFENCE_ALWAYS_SERIALIZING). Where that
//! guarantee is absent the LFENCE form would be unordered, so `OrderedInstant`
//! falls back off the TSC path.
//!
//! When the counter is denied or ineligible, both timers fall back to the
//! explicit `CLOCK_MONOTONIC` raw syscall: a libc or vDSO `clock_gettime` may
//! itself execute the denied counter instruction, so the fallback enters the
//! kernel through the context-synchronizing syscall exception. The ordered
//! fallback adds a `CPUID` barrier before that syscall. On x86_64 the raw ABI
//! is `SYSCALL`; i686 enters through the `INT 0x80` exception and separates the
//! time32 and time64 layouts.
//!
//! TSC permission is per thread while each selected wall timeline is
//! process-wide. Every reading thread must retain counter permission after a
//! counter provider is selected; explicitly disabling it is an external fault
//! boundary.

#[cfg(all(test, feature = "bench-internal"))]
use core::hint::black_box;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicI32, AtomicU8, AtomicU64, Ordering};

const PROVIDER_UNKNOWN: u8 = 0;
const PROVIDER_SELECTING: u8 = 1;

const PROVIDER_TSC: u8 = 2;
// `Instant` checks `== PROVIDER_TSC` and `OrderedInstant` checks
// `== PROVIDER_TSC_LFENCE_RDTSC`; the shared tag keeps each public hot path a
// single compare against the reentrant syscall fallback providers, which carry
// distinct tags routed to the cold path.
const PROVIDER_TSC_LFENCE_RDTSC: u8 = 2;

// The reentrant fallback reads a raw `CLOCK_MONOTONIC` syscall. Its source is
// the matching pointer width's native ABI; the ordered fallback prefixes a
// `CPUID` barrier, encoded in the low byte so both hot paths route it cold.
#[cfg(target_pointer_width = "64")]
const SOURCE_SYSCALL64_MONOTONIC: u8 = 2;
#[cfg(target_pointer_width = "64")]
const SOURCE_SYSCALL64_MONOTONIC_RAW: u8 = 3;
#[cfg(target_pointer_width = "32")]
const SOURCE_TIME32_MONOTONIC: u8 = 4;
#[cfg(target_pointer_width = "32")]
const SOURCE_TIME32_MONOTONIC_RAW: u8 = 5;
#[cfg(target_pointer_width = "32")]
const SOURCE_TIME64_MONOTONIC: u8 = 6;
#[cfg(target_pointer_width = "32")]
const SOURCE_TIME64_MONOTONIC_RAW: u8 = 7;

const PROVIDER_SOURCE_BASE: u8 = 7;
#[cfg(target_pointer_width = "64")]
const PROVIDER_SYSCALL64_MONOTONIC: u8 = PROVIDER_SOURCE_BASE + SOURCE_SYSCALL64_MONOTONIC;
#[cfg(target_pointer_width = "64")]
const PROVIDER_SYSCALL64_MONOTONIC_RAW: u8 = PROVIDER_SOURCE_BASE + SOURCE_SYSCALL64_MONOTONIC_RAW;
#[cfg(target_pointer_width = "32")]
const PROVIDER_TIME32_MONOTONIC: u8 = PROVIDER_SOURCE_BASE + SOURCE_TIME32_MONOTONIC;
#[cfg(target_pointer_width = "32")]
const PROVIDER_TIME32_MONOTONIC_RAW: u8 = PROVIDER_SOURCE_BASE + SOURCE_TIME32_MONOTONIC_RAW;
#[cfg(target_pointer_width = "32")]
const PROVIDER_TIME64_MONOTONIC: u8 = PROVIDER_SOURCE_BASE + SOURCE_TIME64_MONOTONIC;
#[cfg(target_pointer_width = "32")]
const PROVIDER_TIME64_MONOTONIC_RAW: u8 = PROVIDER_SOURCE_BASE + SOURCE_TIME64_MONOTONIC_RAW;

// The ordered fallback keeps the raw-syscall source's original compound
// encoding (`ORDERED_OS_BASE + source * ORDERED_BARRIER_VARIANTS + CPUID`) so
// its low-byte tag stays distinct from `PROVIDER_TSC_LFENCE_RDTSC`.
const ORDERED_BARRIER_CPUID: u8 = 3;
const ORDERED_BARRIER_VARIANTS: u8 = 6;
const ORDERED_OS_BASE: u8 = 16;

#[cfg(target_pointer_width = "64")]
const REENTRANT_SOURCE: u8 = SOURCE_SYSCALL64_MONOTONIC;
#[cfg(target_pointer_width = "32")]
const REENTRANT_SOURCE: u8 = SOURCE_TIME32_MONOTONIC;
const REENTRANT_INSTANT_PROVIDER: u8 = PROVIDER_SOURCE_BASE + REENTRANT_SOURCE;
const REENTRANT_ORDERED_PROVIDER: u8 =
  ORDERED_OS_BASE + REENTRANT_SOURCE * ORDERED_BARRIER_VARIANTS + ORDERED_BARRIER_CPUID;
const MAX_ORDERED_HOT_SCALE: u64 = u64::MAX >> 8;
const UNSELECTED_ORDERED_HOT_STATE: u64 = (1_u64 << 32) << 8;

const PR_GET_TSC: libc::c_int = 25;
const PR_TSC_ENABLE: libc::c_int = 1;
#[cfg(test)]
const PR_SET_TSC: libc::c_int = 26;
#[cfg(test)]
const PR_TSC_SIGSEGV: libc::c_int = 2;

static INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static INSTANT_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static INSTANT_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static ORDERED_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_HOT_STATE: AtomicU64 = AtomicU64::new(UNSELECTED_ORDERED_HOT_STATE);
static TSC_FREQUENCY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TscEligibility {
  Eligible,
  MissingTsc,
  MissingInvariantTsc,
  KernelClocksourceMetadataUnavailable,
  KernelTscUnavailable,
  TscReadDisabled,
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  let provider = INSTANT_PROVIDER.load(Ordering::Relaxed);
  if provider == PROVIDER_TSC { read_tsc() } else { read_outlined_instant_provider(provider) }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  let state = ORDERED_HOT_STATE.load(Ordering::Relaxed);
  read_hot_ordered_provider(state as u8)
}

#[inline(always)]
fn read_hot_ordered_provider(provider: u8) -> u64 {
  if provider == PROVIDER_TSC_LFENCE_RDTSC {
    return read_tsc_lfence_ordered();
  }
  read_non_lfence_ordered_provider(provider)
}

#[cold]
#[inline(never)]
fn read_non_lfence_ordered_provider(provider: u8) -> u64 {
  read_outlined_ordered_provider(provider)
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn ticks_ordered_with_scale() -> (u64, u64) {
  let state = ORDERED_HOT_STATE.load(Ordering::Relaxed);
  (read_hot_ordered_provider(state as u8), state >> 8)
}

pub(crate) const fn ordered_hot_scale_fits(scale: u64) -> bool {
  scale <= MAX_ORDERED_HOT_SCALE
}

pub(crate) fn update_ordered_hot_scale(scale: u64) {
  let state = ORDERED_HOT_STATE.load(Ordering::Relaxed);
  publish_ordered_hot_state(state as u8, scale);
}

fn publish_ordered_hot_state(provider: u8, scale: u64) {
  debug_assert!(ordered_hot_scale_fits(scale));
  let state = scale << 8 | u64::from(provider);
  ORDERED_HOT_STATE.store(state, Ordering::Release);
}

/// Read an unordered endpoint in `OrderedInstant`'s selected numeric domain.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  read_ordered_provider_unordered(ORDERED_PROVIDER.load(Ordering::Relaxed))
}

#[inline(never)]
fn read_outlined_instant_provider(provider: u8) -> u64 {
  match provider {
    PROVIDER_TSC => read_tsc(),
    REENTRANT_INSTANT_PROVIDER => raw_clock(REENTRANT_INSTANT_PROVIDER),
    _ => ticks_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_after_selection() -> u64 {
  match selected_instant_provider() {
    PROVIDER_TSC => read_tsc(),
    _ => raw_clock(REENTRANT_INSTANT_PROVIDER),
  }
}

#[inline(always)]
fn read_ordered_provider(provider: u8) -> u64 {
  match provider {
    PROVIDER_TSC_LFENCE_RDTSC => read_tsc_lfence_ordered(),
    REENTRANT_ORDERED_PROVIDER => {
      execute_cpuid_barrier();
      raw_clock(REENTRANT_INSTANT_PROVIDER)
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
    PROVIDER_TSC_LFENCE_RDTSC => read_tsc(),
    REENTRANT_ORDERED_PROVIDER => raw_clock(REENTRANT_INSTANT_PROVIDER),
    _ => ticks_ordered_unordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_unordered_after_selection() -> u64 {
  read_ordered_provider_unordered(selected_ordered_provider())
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
const fn provider_clock_id(provider: u8) -> libc::clockid_t {
  #[cfg(target_pointer_width = "64")]
  if provider == PROVIDER_SYSCALL64_MONOTONIC_RAW {
    return libc::CLOCK_MONOTONIC_RAW;
  }
  #[cfg(target_pointer_width = "32")]
  if matches!(provider, PROVIDER_TIME32_MONOTONIC_RAW | PROVIDER_TIME64_MONOTONIC_RAW) {
    return libc::CLOCK_MONOTONIC_RAW;
  }
  libc::CLOCK_MONOTONIC
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
fn raw_clock(provider: u8) -> u64 {
  raw_clock_nanos(provider)
    .or_else(|| libc_clock_nanos(provider_clock_id(provider)))
    .or_else(|| alternate_raw_clock_nanos(provider))
    .unwrap_or(0)
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
    PROVIDER_TIME64_MONOTONIC => raw_clock_nanos(PROVIDER_TIME32_MONOTONIC),
    PROVIDER_TIME64_MONOTONIC_RAW => raw_clock_nanos(PROVIDER_TIME32_MONOTONIC_RAW),
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
    PROVIDER_TIME32_MONOTONIC | PROVIDER_TIME32_MONOTONIC_RAW => raw_clock_nanos_time32(provider),
    PROVIDER_TIME64_MONOTONIC | PROVIDER_TIME64_MONOTONIC_RAW => raw_clock_nanos_time64(provider),
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
  // `PROVIDER_TSC` and `PROVIDER_TSC_LFENCE_RDTSC` share this tag; both scale by
  // the calibrated TSC frequency. The reentrant syscall providers do not.
  provider == PROVIDER_TSC
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
    PROVIDER_TSC => crate::ThreadCpuReadCost::Inline,
    // The reentrant fallback enters the kernel through a raw syscall. Without a
    // guaranteed userspace path, the conservative class is a system call.
    _ => crate::ThreadCpuReadCost::SystemCall,
  }
}

#[cfg(test)]
#[test]
fn instant_read_cost_only_marks_guaranteed_userspace_paths_inline() {
  assert_eq!(instant_read_cost_for(PROVIDER_TSC), crate::ThreadCpuReadCost::Inline);
  assert_eq!(
    instant_read_cost_for(REENTRANT_INSTANT_PROVIDER),
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
  let provider = super::select_thread_owned_process_provider(
    &ORDERED_PROVIDER,
    PROVIDER_UNKNOWN,
    PROVIDER_SELECTING,
    &ORDERED_PROVIDER_OWNER_PID,
    &ORDERED_PROVIDER_OWNER_TID,
    REENTRANT_ORDERED_PROVIDER,
    detect_ordered_provider,
  );
  if ORDERED_PROVIDER.load(Ordering::Acquire) == provider {
    let scale = super::ORDERED_NANOS_PER_TICK_Q32.load(Ordering::Acquire);
    publish_ordered_hot_state(provider, scale);
  }
  provider
}

#[cold]
#[inline(never)]
fn detect_instant_provider() -> u8 {
  if detect_tsc_eligibility() == TscEligibility::Eligible {
    PROVIDER_TSC
  } else {
    REENTRANT_INSTANT_PROVIDER
  }
}

#[cold]
#[inline(never)]
fn detect_ordered_provider() -> u8 {
  // LFENCE only orders a following RDTSC after prior loads on Intel or on AMD
  // parts with AMD_LFENCE_ALWAYS_SERIALIZING (CPUID.8000_0021H:EAX[2]); without
  // that guarantee `PROVIDER_TSC_LFENCE_RDTSC` would emit an unordered
  // OrderedInstant, silently violating the contract. This gate is correctness,
  // not speed: unproven hardware fails closed to the raw-syscall + CPUID-barrier
  // reentrant provider. Do not delete it as tournament cruft.
  let provider =
    if detect_tsc_eligibility() == TscEligibility::Eligible && lfence_ordered_eligible() {
      PROVIDER_TSC_LFENCE_RDTSC
    } else {
      REENTRANT_ORDERED_PROVIDER
    };
  let scale = super::scale_from_ratio(1_000_000_000, frequency_for(provider));
  assert!(ordered_hot_scale_fits(scale), "tach: selected ordered x86 scale is not packable");
  super::ORDERED_NANOS_PER_TICK_Q32.store(scale, Ordering::Release);
  provider
}

/// Whether `LFENCE` architecturally orders a following `RDTSC` after prior
/// loads on this CPU. True on Intel with SSE2, or on AMD with SSE2 and
/// `CPUID.8000_0021H:EAX[2]` (AMD_LFENCE_ALWAYS_SERIALIZING). Without this
/// guarantee `OrderedInstant`'s `lfence; rdtsc` read would be unordered, so
/// `detect_ordered_provider` consults it as a correctness gate, not a speed
/// test: unknown vendors and non-serializing AMD parts fail closed.
#[allow(unused_unsafe)] // supported rustc versions differ on whether __cpuid is unsafe
fn lfence_ordered_eligible() -> bool {
  #[cfg(target_arch = "x86")]
  use core::arch::x86::__cpuid;
  #[cfg(target_arch = "x86_64")]
  use core::arch::x86_64::__cpuid;

  const INTEL: (u32, u32, u32) = (0x756e_6547, 0x4965_6e69, 0x6c65_746e);
  const AMD: (u32, u32, u32) = (0x6874_7541, 0x6974_6e65, 0x444d_4163);
  // SAFETY: supported x86 targets guarantee CPUID leaf zero.
  let basic = unsafe { __cpuid(0) };
  let vendor = (basic.ebx, basic.edx, basic.ecx);
  // SAFETY: leaf one is queried only when the maximum basic leaf includes it.
  let has_sse2 = basic.eax >= 1 && unsafe { __cpuid(1) }.edx & (1 << 26) != 0;
  if !has_sse2 {
    return false;
  }
  if vendor == INTEL {
    return true;
  }
  if vendor == AMD {
    // SAFETY: the extended maximum-leaf query is defined on CPUID systems.
    let extended = unsafe { __cpuid(0x8000_0000) };
    // SAFETY: the maximum extended leaf includes AMD feature leaf 0x80000021.
    return extended.eax >= 0x8000_0021 && unsafe { __cpuid(0x8000_0021) }.eax & (1 << 2) != 0;
  }
  false
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
    #[cfg(target_pointer_width = "64")]
    PROVIDER_SYSCALL64_MONOTONIC => "linux_clock_monotonic_syscall_x86_64",
    #[cfg(target_pointer_width = "32")]
    PROVIDER_TIME32_MONOTONIC => "linux_clock_monotonic_syscall_i686_time32",
    _ => "unavailable",
  }
}

#[cfg(feature = "bench-internal")]
fn ordered_provider_name(provider: u8) -> &'static str {
  match provider {
    PROVIDER_TSC_LFENCE_RDTSC => "linux_kernel_eligible_tsc_x86_lfence_rdtsc",
    #[cfg(target_pointer_width = "64")]
    REENTRANT_ORDERED_PROVIDER => "linux_clock_monotonic_syscall_x86_64_x86_cpuid",
    #[cfg(target_pointer_width = "32")]
    REENTRANT_ORDERED_PROVIDER => "linux_clock_monotonic_syscall_i686_time32_x86_cpuid",
    _ => "unavailable",
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
  read_tsc_lfence_ordered()
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(dead_code)] // Criterion calls the eligible named primitive directly.
pub(crate) fn bench_direct_tsc_lfence_ordered() -> u64 {
  read_tsc_lfence_ordered()
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
pub(crate) fn bench_direct_clock_monotonic_syscall_ordered() -> u64 {
  execute_cpuid_barrier();
  raw_clock(REENTRANT_INSTANT_PROVIDER)
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
#[allow(dead_code)] // The benchmark harness selects once before its timing loop.
pub(crate) fn bench_selected_instant_primitive() -> BenchPrimitive {
  let provider = selected_instant_provider();
  instant_bench_primitive(provider)
}

#[cfg(feature = "bench-internal")]
fn instant_bench_primitive(provider: u8) -> BenchPrimitive {
  let read: fn() -> u64 = match provider {
    PROVIDER_TSC => bench_direct_tsc,
    _ => bench_direct_clock_monotonic_syscall,
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
    _ => bench_direct_clock_monotonic_syscall_ordered as fn() -> u64,
  };
  BenchPrimitive {
    name: ordered_provider_name(provider),
    read,
    nanos_per_tick_q32: bench_nanos_per_tick_q32(provider),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn requires_tsc_to_be_the_exact_current_clocksource() {
    assert!(current_clocksource_is_tsc(b"tsc\n"));
    assert!(current_clocksource_is_tsc(b"  tsc \0ignored"));
    assert!(!current_clocksource_is_tsc(b"kvm-clock tsc hpet\n"));
    assert!(!current_clocksource_is_tsc(b"kvm-clock\n"));
    assert!(!current_clocksource_is_tsc(b"tsc-early\n"));
  }

  #[test]
  fn ordered_lfence_provider_requires_serializing_lfence() {
    // The LFENCE ordered provider is selected only where LFENCE architecturally
    // orders RDTSC here; otherwise the reentrant syscall provider is chosen.
    // This proves `lfence_ordered_eligible()` is a load-bearing correctness gate.
    let ordered = selected_ordered_provider();
    if ordered == PROVIDER_TSC_LFENCE_RDTSC {
      assert!(lfence_ordered_eligible());
      assert_eq!(detect_tsc_eligibility(), TscEligibility::Eligible);
    } else {
      assert_eq!(ordered, REENTRANT_ORDERED_PROVIDER);
    }
  }

  #[test]
  fn initial_sigsegv_mode_never_executes_a_tsc_read() {
    // SAFETY: the child changes only its own thread permission and private COW
    // selector state, then exits without invoking inherited Rust cleanup.
    let child = unsafe { libc::fork() };
    assert!(child >= 0);
    if child == 0 {
      INSTANT_PROVIDER.store(PROVIDER_UNKNOWN, Ordering::Release);
      ORDERED_PROVIDER.store(PROVIDER_UNKNOWN, Ordering::Release);
      let status = unsafe { libc::prctl(PR_SET_TSC, PR_TSC_SIGSEGV) };
      if status != 0 {
        // A kernel without the per-thread TSC control cannot deny the read.
        unsafe { libc::_exit(77) };
      }
      let _ = ticks();
      let _ = ticks_ordered();
      let ok = INSTANT_PROVIDER.load(Ordering::Acquire) == REENTRANT_INSTANT_PROVIDER
        && ORDERED_PROVIDER.load(Ordering::Acquire) == REENTRANT_ORDERED_PROVIDER;
      unsafe { libc::_exit(if ok { 0 } else { 1 }) };
    }

    let mut status = 0;
    // SAFETY: child is live and status is writable wait storage.
    assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
    if libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 77 {
      return;
    }
    assert_eq!(status, 0);
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
