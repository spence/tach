//! FreeBSD/amd64 wall-clock provider selection.
//!
//! FreeBSD exposes both the timecounter the kernel selected and that counter's
//! quality through `sysctl`. A direct `RDTSC` is eligible only when the kernel
//! selected a synchronized, invariant `TSC`/`TSC-low` provider with nonnegative
//! quality. When eligible, `Instant` reads a bare `RDTSC` and `OrderedInstant`
//! reads `lfence; rdtsc`; both scale by the kernel's reported `machdep.tsc_freq`.
//!
//! A bare `RDTSC` is eligible for local monotonic `Instant` samples because the
//! reads order against one another; it is not ordered with surrounding work.
//! `OrderedInstant` additionally needs the read ordered after a prior Acquire
//! load, which `LFENCE` guarantees only on Intel or on AMD parts that set
//! `CPUID.8000_0021H:EAX[2]` (AMD_LFENCE_ALWAYS_SERIALIZING). Where that
//! guarantee is absent the LFENCE form would be unordered, so `OrderedInstant`
//! falls back off the TSC path.
//!
//! When the counter is ineligible, both timers fall back to the explicit
//! `CLOCK_MONOTONIC` raw syscall (with a libc `clock_gettime` retry). The
//! ordered fallback prefixes a `CPUID` barrier before that syscall.
//!
//! Each selected wall timeline is process-wide, published in a packed hot state
//! that carries both the provider tag and its fixed-point scale so a read
//! dispatches through a single relaxed load.

use core::arch::asm;
use core::arch::x86_64::__cpuid;
use core::mem::{MaybeUninit, size_of};
use core::ptr;
use core::sync::atomic::{AtomicI32, AtomicU8, AtomicU64, Ordering};

const PROVIDER_UNKNOWN: u8 = 0;
const PROVIDER_SELECTING: u8 = 1;
const PROVIDER_TSC: u8 = 2;
// `Instant` falls back to a bare raw `CLOCK_MONOTONIC` syscall; the ordered
// fallback prefixes a `CPUID` barrier. Both keep the raw syscall's own libc
// retry inside `clock_monotonic_syscall`.
const PROVIDER_CLOCK_MONOTONIC_SYSCALL: u8 = 3;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID: u8 = 4;

// The selection/evidence provider stays `PROVIDER_TSC`. The installed ordered
// hot state carries the exact `lfence; rdtsc` tag so a read does not dispatch
// through both the FreeBSD provider selector and any barrier selector.
const HOT_PROVIDER_TSC_LFENCE: u8 = 5;
const MAX_HOT_SCALE: u64 = u64::MAX >> 8;
const UNSELECTED_HOT_STATE: u64 = (1_u64 << 32) << 8;

static INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static INSTANT_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static INSTANT_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static ORDERED_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static INSTANT_HOT_STATE: AtomicU64 = AtomicU64::new(UNSELECTED_HOT_STATE);
static ORDERED_HOT_STATE: AtomicU64 = AtomicU64::new(UNSELECTED_HOT_STATE);
static TSC_FREQUENCY: AtomicU64 = AtomicU64::new(0);

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  let provider = INSTANT_HOT_STATE.load(Ordering::Relaxed) as u8;
  if provider == PROVIDER_TSC {
    return super::x86_64::rdtsc();
  }
  match provider {
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    _ => ticks_after_selection(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn ticks_with_scale() -> (u64, u64) {
  let state = INSTANT_HOT_STATE.load(Ordering::Relaxed);
  (read_hot_instant_provider(state as u8), state >> 8)
}

#[inline(always)]
fn read_hot_instant_provider(provider: u8) -> u64 {
  if provider == PROVIDER_TSC {
    return super::x86_64::rdtsc();
  }
  match provider {
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    _ => ticks_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_after_selection() -> u64 {
  match selected_instant_provider() {
    PROVIDER_TSC => super::x86_64::rdtsc(),
    _ => clock_monotonic_syscall(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  let provider = ORDERED_HOT_STATE.load(Ordering::Relaxed) as u8;
  read_hot_ordered_provider(provider)
}

#[inline(always)]
fn read_hot_ordered_provider(provider: u8) -> u64 {
  if provider == HOT_PROVIDER_TSC_LFENCE {
    return super::x86_64::read_lfence_rdtsc();
  }
  match provider {
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID => ordered_clock_monotonic_syscall_cpuid(),
    _ => ticks_ordered_after_selection(),
  }
}

#[inline]
fn installed_ordered_provider(provider: u8) -> u8 {
  if provider == PROVIDER_TSC { HOT_PROVIDER_TSC_LFENCE } else { provider }
}

#[inline(always)]
const fn is_hot_tsc_provider(provider: u8) -> bool {
  provider == HOT_PROVIDER_TSC_LFENCE
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn ticks_ordered_with_scale() -> (u64, u64) {
  let state = ORDERED_HOT_STATE.load(Ordering::Relaxed);
  (read_hot_ordered_provider(state as u8), state >> 8)
}

#[cold]
#[inline(never)]
fn ticks_ordered_after_selection() -> u64 {
  match selected_ordered_provider() {
    PROVIDER_TSC => super::x86_64::read_lfence_rdtsc(),
    _ => ordered_clock_monotonic_syscall_cpuid(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  let provider = ORDERED_HOT_STATE.load(Ordering::Relaxed) as u8;
  if is_hot_tsc_provider(provider) {
    return super::x86_64::rdtsc();
  }
  match provider {
    PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID => clock_monotonic_syscall(),
    _ => ticks_ordered_unordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_unordered_after_selection() -> u64 {
  match selected_ordered_provider() {
    PROVIDER_TSC => super::x86_64::rdtsc(),
    _ => clock_monotonic_syscall(),
  }
}

#[inline(always)]
fn ordered_clock_monotonic_syscall_cpuid() -> u64 {
  cpuid_barrier();
  clock_monotonic_syscall()
}

#[inline(always)]
fn cpuid_barrier() {
  // SAFETY: x86_64 guarantees CPUID. RSI temporarily preserves RBX, which LLVM
  // reserves for position-independent code. CPUID fully serializes execution;
  // omitting `nomem` also supplies the compiler barrier before the syscall.
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
fn clock_monotonic_syscall() -> u64 {
  raw_clock_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| libc_clock_nanos(libc::CLOCK_MONOTONIC))
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
    asm!(
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
    instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC_SYSCALL),
    crate::ThreadCpuReadCost::SystemCall,
  );
}

#[inline]
pub(crate) fn ordered_uses_tsc() -> bool {
  selected_ordered_provider() == PROVIDER_TSC
}

fn selected_instant_provider() -> u8 {
  let provider = select_provider(
    &INSTANT_PROVIDER,
    &INSTANT_PROVIDER_OWNER_PID,
    &INSTANT_PROVIDER_OWNER_TID,
    false,
  );
  if INSTANT_PROVIDER.load(Ordering::Acquire) == provider {
    let scale = scale_for_provider(provider);
    super::NANOS_PER_TICK_Q32.store(scale, Ordering::Release);
    publish_hot_state(&INSTANT_HOT_STATE, provider, scale);
  }
  provider
}

fn selected_ordered_provider() -> u8 {
  let provider = select_provider(
    &ORDERED_PROVIDER,
    &ORDERED_PROVIDER_OWNER_PID,
    &ORDERED_PROVIDER_OWNER_TID,
    true,
  );
  if ORDERED_PROVIDER.load(Ordering::Acquire) == provider {
    let scale = scale_for_provider(provider);
    super::ORDERED_NANOS_PER_TICK_Q32.store(scale, Ordering::Release);
    publish_hot_state(&ORDERED_HOT_STATE, installed_ordered_provider(provider), scale);
  }
  provider
}

#[inline]
fn scale_for_provider(provider: u8) -> u64 {
  let frequency = if provider == PROVIDER_TSC {
    TSC_FREQUENCY.load(Ordering::Acquire).max(1)
  } else {
    1_000_000_000
  };
  super::scale_from_ratio(1_000_000_000, frequency)
}

#[inline]
fn publish_hot_state(state: &AtomicU64, provider: u8, scale: u64) {
  assert!(scale <= MAX_HOT_SCALE, "tach: selected FreeBSD scale is not packable");
  state.store((scale << 8) | u64::from(provider), Ordering::Release);
}

fn select_provider(
  state: &AtomicU8,
  owner_pid: &AtomicI32,
  owner_tid: &AtomicI32,
  ordered: bool,
) -> u8 {
  let fallback =
    if ordered { PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID } else { PROVIDER_CLOCK_MONOTONIC_SYSCALL };
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
  if let Some(frequency) = frequency {
    TSC_FREQUENCY.store(frequency, Ordering::Release);
  }
  let tsc_eligible = frequency.is_some();
  if ordered {
    // LFENCE only orders a following RDTSC after prior loads on Intel or on AMD
    // parts with AMD_LFENCE_ALWAYS_SERIALIZING (CPUID.8000_0021H:EAX[2]);
    // without that guarantee `HOT_PROVIDER_TSC_LFENCE` would emit an unordered
    // OrderedInstant, silently violating the contract. This gate is correctness,
    // not speed: unproven hardware fails closed to the raw-syscall + CPUID-barrier
    // reentrant provider. Do not delete it as tournament cruft.
    if tsc_eligible && lfence_ordered_eligible() {
      PROVIDER_TSC
    } else {
      PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID
    }
  } else if tsc_eligible {
    PROVIDER_TSC
  } else {
    PROVIDER_CLOCK_MONOTONIC_SYSCALL
  }
}

/// Whether `LFENCE` architecturally orders a following `RDTSC` after prior
/// loads on this CPU. True on Intel with SSE2, or on AMD with SSE2 and
/// `CPUID.8000_0021H:EAX[2]` (AMD_LFENCE_ALWAYS_SERIALIZING). Without this
/// guarantee `OrderedInstant`'s `lfence; rdtsc` read would be unordered, so
/// `detect_provider` consults it as a correctness gate, not a speed test:
/// unknown vendors and non-serializing AMD parts fail closed.
#[allow(unused_unsafe)] // supported rustc versions differ on whether __cpuid is unsafe
fn lfence_ordered_eligible() -> bool {
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

#[cfg(feature = "bench-internal")]
fn instant_provider_name(provider: u8) -> &'static str {
  match provider {
    PROVIDER_TSC => "freebsd_kernel_eligible_tsc",
    _ => "freebsd_clock_monotonic_syscall",
  }
}

#[cfg(feature = "bench-internal")]
fn ordered_provider_name(provider: u8) -> &'static str {
  match provider {
    PROVIDER_TSC => "freebsd_kernel_eligible_tsc_x86_lfence_rdtsc",
    _ => "freebsd_clock_monotonic_syscall_x86_cpuid",
  }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_provider() -> &'static str {
  instant_provider_name(selected_instant_provider())
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_tsc_eligible() -> bool {
  kernel_eligible_tsc_frequency().is_some()
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
  let frequency = if provider == PROVIDER_TSC {
    TSC_FREQUENCY.load(Ordering::Acquire).max(1)
  } else {
    // FreeBSD's clock_gettime routes return nanoseconds.
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
  let read: fn() -> u64 = match provider {
    PROVIDER_TSC => bench_exact_tsc,
    _ => bench_exact_clock_monotonic_syscall,
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
  ordered_bench_primitive(selected_ordered_provider())
}

#[cfg(feature = "bench-internal")]
fn ordered_bench_primitive(provider: u8) -> BenchPrimitive {
  let read: fn() -> u64 = match provider {
    PROVIDER_TSC => bench_exact_tsc_lfence_rdtsc,
    _ => bench_exact_clock_monotonic_syscall_cpuid,
  };
  BenchPrimitive {
    name: ordered_provider_name(provider),
    read,
    nanos_per_tick_q32: bench_nanos_per_tick_q32(provider),
  }
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
exact_bench_reader!(bench_exact_clock_monotonic_syscall, clock_monotonic_syscall());
#[cfg(feature = "bench-internal")]
exact_bench_reader!(
  bench_exact_clock_monotonic_syscall_cpuid,
  ordered_clock_monotonic_syscall_cpuid()
);

#[cfg(test)]
mod tests {
  use super::*;

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
  fn packed_hot_state_keeps_provider_and_scale_together() {
    let state = AtomicU64::new(UNSELECTED_HOT_STATE);
    let scale = 1_u64 << 31;
    publish_hot_state(&state, HOT_PROVIDER_TSC_LFENCE, scale);

    let published = state.load(Ordering::Acquire);
    assert_eq!(published as u8, HOT_PROVIDER_TSC_LFENCE);
    assert_eq!(published >> 8, scale);
    assert!(is_hot_tsc_provider(published as u8));
    assert!(!is_hot_tsc_provider(PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID));
  }

  #[test]
  fn ordered_lfence_provider_requires_serializing_lfence() {
    // The LFENCE ordered provider is selected only where LFENCE architecturally
    // orders RDTSC here; otherwise the reentrant syscall + CPUID-barrier
    // provider is chosen. This proves `lfence_ordered_eligible()` is a
    // load-bearing correctness gate.
    let ordered = selected_ordered_provider();
    if ordered == PROVIDER_TSC {
      assert!(lfence_ordered_eligible());
      assert!(kernel_eligible_tsc_frequency().is_some());
    } else {
      assert_eq!(ordered, PROVIDER_CLOCK_MONOTONIC_SYSCALL_CPUID);
    }
  }

  #[test]
  fn selected_domains_are_monotonic_and_survive_fork() {
    let instant_before = ticks();
    let ordered_before = ticks_ordered();
    assert!(instant_frequency() > 0);
    assert!(ordered_frequency() > 0);
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
}
