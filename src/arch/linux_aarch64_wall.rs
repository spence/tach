//! Linux-kernel aarch64 wall-clock provider selection.
//!
//! Linux normally permits EL0 reads of `CNTVCT_EL0`, but deliberately traps
//! them when an architectural-timer erratum needs an out-of-line workaround.
//! The arm64 trap handler emulates `CNTVCT_EL0` through the kernel's
//! workaround-aware counter reader. A thread can separately request `SIGSEGV`
//! for counter access with `PR_SET_TSC` on kernels that implement the arm64
//! control, which entered upstream in Linux 6.12. Tach first checks
//! `PR_GET_TSC`. When an older kernel reports that option unsupported, a
//! strictly parsed pre-6.12 `uname` release proves that the per-thread denial
//! control does not exist. A successful query remains authoritative regardless
//! of the reported release, so Android and vendor backports are handled as
//! feature probes rather than guessed from their base version. New-enough,
//! malformed, or unavailable releases fail closed.
//!
//! When counter reads are eligible, `Instant` reads a bare `CNTVCT_EL0` and
//! `GlobalInstant` reads `isb; mrs cntvct_el0`, both scaled by the calibrated
//! counter frequency. A bare CNTVCT read is eligible for local monotonic
//! samples because the register contract orders CNTVCT reads with one another;
//! it is not ordered with surrounding work. The ISB form additionally orders
//! the sample after a prior Acquire observation. When counter reads are denied,
//! both timers fall back to the explicit `CLOCK_MONOTONIC` syscall: a libc or
//! vDSO `clock_gettime` may itself execute the denied counter instruction, so
//! the fallback must enter the kernel through the context-synchronizing syscall
//! exception.
//!
//! Counter permission is per thread while each selected wall timeline is
//! process-wide. Every reading thread must retain counter permission after a
//! counter provider is selected; explicitly disabling it is an external fault
//! boundary.

use core::sync::atomic::{AtomicI32, AtomicU8, AtomicU64, Ordering};

const PROVIDER_UNKNOWN: u8 = 0;
const PROVIDER_SELECTING: u8 = 1;
const PROVIDER_CNTVCT: u8 = 2;
const PROVIDER_ISB_CNTVCT: u8 = 4;
const PROVIDER_CLOCK_MONOTONIC_SYSCALL: u8 = 6;
const MAX_ORDERED_HOT_SCALE: u64 = u64::MAX >> 8;
// The ordered hot state packs `scale << 8 | provider`. Before selection its low
// byte is `PROVIDER_UNKNOWN`, so both hot ordered reads miss their single
// `PROVIDER_ISB_CNTVCT` compare and route to the cold selector; the identity
// scale in the high bits is exact for the reentrant `CLOCK_MONOTONIC` fallback a
// pre-selection read takes.
const UNSELECTED_ORDERED_HOT_STATE: u64 = (1_u64 << 32) << 8;

const PR_GET_TSC: libc::c_int = 25;
#[cfg(test)]
const PR_SET_TSC: libc::c_int = 26;
const PR_TSC_ENABLE: libc::c_int = 1;
const PR_TSC_SIGSEGV: libc::c_int = 2;

static INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static INSTANT_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static INSTANT_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
static ORDERED_PROVIDER_OWNER_PID: AtomicI32 = AtomicI32::new(0);
static ORDERED_PROVIDER_OWNER_TID: AtomicI32 = AtomicI32::new(0);
static ORDERED_HOT_STATE: AtomicU64 = AtomicU64::new(UNSELECTED_ORDERED_HOT_STATE);

// Both direct domains read the same architectural counter at the same rate.
static DIRECT_FREQUENCY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CounterEligibility {
  Eligible,
  PrGetTscUnavailable,
  CounterReadDisabled,
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
  kernel_version: Option<KernelVersion>,
}

impl CounterAssessment {
  const fn counter_eligible(self) -> bool {
    matches!(self.eligibility, CounterEligibility::Eligible)
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  match INSTANT_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_CNTVCT => super::aarch64::cntvct(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    _ => ticks_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_after_selection() -> u64 {
  match selected_instant_provider() {
    PROVIDER_CNTVCT => super::aarch64::cntvct(),
    _ => clock_monotonic_syscall(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  let state = ORDERED_HOT_STATE.load(Ordering::Relaxed);
  read_hot_ordered_provider(state as u8)
}

#[inline(always)]
fn read_hot_ordered_provider(provider: u8) -> u64 {
  if provider == PROVIDER_ISB_CNTVCT {
    return super::aarch64::cntvct_after_isb();
  }
  read_cold_ordered_provider(provider)
}

#[cold]
#[inline(never)]
fn read_cold_ordered_provider(provider: u8) -> u64 {
  match provider {
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    _ => ticks_ordered_after_selection(),
  }
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
  ORDERED_HOT_STATE.store(scale << 8 | u64::from(provider), Ordering::Release);
}

#[cold]
#[inline(never)]
fn ticks_ordered_after_selection() -> u64 {
  match selected_ordered_provider() {
    PROVIDER_ISB_CNTVCT => super::aarch64::cntvct_after_isb(),
    _ => clock_monotonic_syscall(),
  }
}

/// Read an endpoint in GlobalInstant's selected numeric domain without a
/// preceding happens-before barrier.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  match ORDERED_PROVIDER.load(Ordering::Relaxed) {
    PROVIDER_ISB_CNTVCT => super::aarch64::cntvct(),
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => clock_monotonic_syscall(),
    _ => ticks_ordered_unordered_after_selection(),
  }
}

#[cold]
#[inline(never)]
fn ticks_ordered_unordered_after_selection() -> u64 {
  match selected_ordered_provider() {
    PROVIDER_ISB_CNTVCT => super::aarch64::cntvct(),
    _ => clock_monotonic_syscall(),
  }
}

#[inline(always)]
fn clock_monotonic_syscall() -> u64 {
  clock_gettime_syscall_nanos(libc::CLOCK_MONOTONIC)
    .or_else(|| clock_gettime_libc_nanos(libc::CLOCK_MONOTONIC))
    .unwrap_or(0)
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
  // compiler ordering needed by GlobalInstant and the syscall's memory
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
    PROVIDER_CNTVCT => crate::ThreadCpuReadCost::Inline,
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
  assert_eq!(
    instant_read_cost_for(PROVIDER_CLOCK_MONOTONIC_SYSCALL),
    crate::ThreadCpuReadCost::SystemCall,
  );
}

#[inline]
pub(crate) fn ordered_uses_cntvct() -> bool {
  selected_ordered_provider() == PROVIDER_ISB_CNTVCT
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
    let frequency =
      if provider == PROVIDER_ISB_CNTVCT { direct_frequency() } else { 1_000_000_000 };
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
  if counter_assessment().counter_eligible() {
    PROVIDER_CNTVCT
  } else {
    PROVIDER_CLOCK_MONOTONIC_SYSCALL
  }
}

#[cold]
#[inline(never)]
fn detect_ordered_provider() -> u8 {
  if counter_assessment().counter_eligible() {
    PROVIDER_ISB_CNTVCT
  } else {
    PROVIDER_CLOCK_MONOTONIC_SYSCALL
  }
}

fn counter_assessment() -> CounterAssessment {
  let mut mode: libc::c_int = 0;
  let status = pr_get_tsc(&mut mode);
  let kernel_version = if status == 0 { None } else { running_kernel_version() };
  classify_counter_access(status, mode, kernel_version)
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
  CounterAssessment { eligibility: CounterEligibility::PrGetTscUnavailable, basis, kernel_version }
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
const fn provider_name(provider: u8) -> &'static str {
  match provider {
    PROVIDER_CNTVCT => "aarch64_cntvct",
    PROVIDER_ISB_CNTVCT => "aarch64_isb_cntvct",
    PROVIDER_CLOCK_MONOTONIC_SYSCALL => "linux_clock_monotonic_syscall",
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
  let frequency = if matches!(provider, PROVIDER_CNTVCT | PROVIDER_ISB_CNTVCT) {
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
    _ => bench_direct_clock_monotonic_syscall,
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
    _ => bench_direct_clock_monotonic_syscall,
  };
  BenchPrimitive {
    name: provider_name(provider),
    read,
    nanos_per_tick_q32: bench_nanos_per_tick_q32(provider),
  }
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
pub(crate) fn bench_direct_clock_monotonic_syscall() -> u64 {
  clock_monotonic_syscall()
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
      ORDERED_HOT_STATE.store(UNSELECTED_ORDERED_HOT_STATE, Ordering::Release);
      let status = unsafe { libc::prctl(PR_SET_TSC, PR_TSC_SIGSEGV) };
      if status != 0 {
        // Upstream arm64 did not implement PR_SET_TSC before Linux 6.12.
        unsafe { libc::_exit(77) };
      }
      let _ = ticks();
      let _ = ticks_ordered();
      let ok = INSTANT_PROVIDER.load(Ordering::Acquire) == PROVIDER_CLOCK_MONOTONIC_SYSCALL
        && ORDERED_PROVIDER.load(Ordering::Acquire) == PROVIDER_CLOCK_MONOTONIC_SYSCALL;
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
