#[cfg(any(
  all(unix, any(target_arch = "x86", target_arch = "x86_64"), not(target_os = "macos"),),
  all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
  all(target_arch = "arm", target_os = "linux"),
  all(target_arch = "s390x", target_os = "linux"),
  all(target_arch = "riscv64", target_os = "linux"),
  all(target_arch = "loongarch64", target_os = "linux"),
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
))]
use core::sync::atomic::AtomicI32;
#[cfg(any(
  all(unix, any(target_arch = "x86", target_arch = "x86_64"), not(target_os = "macos"),),
  all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
  all(target_arch = "arm", target_os = "linux"),
  all(target_arch = "s390x", target_os = "linux"),
  all(target_arch = "riscv64", target_os = "linux"),
  all(target_arch = "loongarch64", target_os = "linux"),
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
))]
use core::sync::atomic::AtomicU8;
use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(all(target_arch = "aarch64", not(any(target_os = "windows", target_os = "macos"))))]
pub mod aarch64;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub mod apple_aarch64;
#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
pub mod apple_x86_64;
#[cfg(target_os = "emscripten")]
pub mod emscripten;
pub mod fallback;
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
pub mod freebsd_x86_64;
#[cfg(all(any(target_os = "android", target_os = "linux"), target_arch = "aarch64",))]
pub mod linux_aarch64_wall;
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
pub mod linux_clock_wall;
#[cfg(any(target_os = "android", target_os = "linux"))]
pub(crate) mod linux_vdso;
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86", target_arch = "x86_64"),
))]
pub mod linux_x86_wall;
#[cfg(target_arch = "loongarch64")]
pub mod loongarch64;
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
pub mod powerpc64;
#[cfg(target_arch = "riscv64")]
pub mod riscv64;
pub(crate) mod thread_cpu;
#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
pub mod wasm;
#[cfg(all(target_arch = "x86", not(any(target_os = "windows", target_os = "macos"))))]
pub mod x86;
#[cfg(all(target_arch = "x86_64", not(any(target_os = "windows", target_os = "macos"))))]
pub mod x86_64;

mod direct;
pub use direct::{ticks, ticks_ordered, ticks_ordered_unordered};

// Stored independently as fixed-point Q32:
//   nanos_per_tick_q32 = (nanos_numerator << 32) / ticks_denominator
// Then converting ticks to nanos becomes (ticks * scale) >> 32, replacing the
// per-call u128 division with a multiply + shift. Instant and OrderedInstant
// may select different providers, so their scales must never be interchanged.
// Most targets initialize each cache at the first elapsed/arithmetic call.
// Linux-kernel x86 publishes the ordered scale with provider selection; its
// initial 1 ns/tick value is the exact scale of the reentrant OS-clock route.
//
// `pub(crate)` so the background-recal thread can issue its own
// Acquire/Release cycle without going through the wholesale-replace
// public `recalibrate()` path.
pub(crate) static NANOS_PER_TICK_Q32: AtomicU64 = AtomicU64::new(0);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86", target_arch = "x86_64"),
))]
pub(crate) static ORDERED_NANOS_PER_TICK_Q32: AtomicU64 = AtomicU64::new(1_u64 << 32);
#[cfg(not(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86", target_arch = "x86_64"),
)))]
pub(crate) static ORDERED_NANOS_PER_TICK_Q32: AtomicU64 = AtomicU64::new(0);

#[cfg(any(
  all(unix, any(target_arch = "x86", target_arch = "x86_64"), not(target_os = "macos"),),
  all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
  all(target_arch = "arm", target_os = "linux"),
  all(target_arch = "s390x", target_os = "linux"),
  all(target_arch = "riscv64", target_os = "linux"),
  all(target_arch = "loongarch64", target_os = "linux"),
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
))]
#[inline]
pub(crate) fn process_id() -> i32 {
  // SAFETY: `getpid` has no preconditions and is async-signal-safe.
  unsafe { libc::getpid() }
}

#[cfg(any(
  all(unix, any(target_arch = "x86", target_arch = "x86_64"), not(target_os = "macos"),),
  all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
  all(target_arch = "arm", target_os = "linux"),
  all(target_arch = "s390x", target_os = "linux"),
  all(target_arch = "riscv64", target_os = "linux"),
  all(target_arch = "loongarch64", target_os = "linux"),
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
))]
#[inline]
#[allow(dead_code)] // Some target families route every selector through the reentrant-safe helper.
pub(crate) fn claim_process_selection(
  state: &AtomicU8,
  unknown: u8,
  selecting: u8,
  owner_pid: &AtomicI32,
) -> bool {
  owner_pid.store(process_id(), Ordering::Relaxed);
  state
    .compare_exchange(unknown, selecting, Ordering::AcqRel, Ordering::Acquire)
    .is_ok()
}

#[cfg(any(
  all(unix, any(target_arch = "x86", target_arch = "x86_64"), not(target_os = "macos"),),
  all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
  all(target_arch = "arm", target_os = "linux"),
  all(target_arch = "s390x", target_os = "linux"),
  all(target_arch = "riscv64", target_os = "linux"),
  all(target_arch = "loongarch64", target_os = "linux"),
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
))]
#[inline]
pub(crate) fn recover_inherited_selection(
  state: &AtomicU8,
  selecting: u8,
  reset: u8,
  owner_pid: &AtomicI32,
) -> bool {
  if owner_pid.load(Ordering::Relaxed) == process_id() {
    return false;
  }
  state
    .compare_exchange(selecting, reset, Ordering::AcqRel, Ordering::Acquire)
    .is_ok()
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
const FORCED_PROCESS_SELECTION: u8 = u8::MAX;

#[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
#[inline]
pub(crate) fn current_thread_id() -> i32 {
  #[cfg(any(target_os = "linux", target_os = "android"))]
  {
    // SAFETY: gettid has no pointer arguments or caller-side preconditions and
    // is safe to issue from a signal handler.
    let tid = unsafe { libc::syscall(libc::SYS_gettid) } as i64;
    i32::try_from(tid).ok().filter(|tid| *tid > 0).unwrap_or_else(process_id)
  }
  #[cfg(target_os = "freebsd")]
  {
    // SAFETY: pthread_getthreadid_np takes no arguments and returns the current
    // kernel thread identifier.
    unsafe { libc::pthread_getthreadid_np() }
  }
}

/// Runs a process-wide selector without deadlocking a same-thread signal
/// handler that reenters while detection is in progress.
///
/// The reentrant read forces `fallback`, and the selection owner publishes that
/// same numeric domain after detection completes. Other threads continue to
/// wait, so they cannot consume bench evidence before its owner finishes it.
#[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
pub(crate) fn select_thread_owned_process_provider<F>(
  state: &AtomicU8,
  unknown: u8,
  selecting: u8,
  owner_pid: &AtomicI32,
  owner_tid: &AtomicI32,
  fallback: u8,
  detect: F,
) -> u8
where
  F: Fn() -> u8,
{
  let provider = state.load(Ordering::Acquire);
  if provider != unknown && provider != selecting && provider != FORCED_PROCESS_SELECTION {
    return provider;
  }
  select_thread_owned_process_provider_slow(
    state, unknown, selecting, owner_pid, owner_tid, fallback, false, detect,
  )
}

/// Runs a process-wide selector whose reentrant fallback shares the detected
/// candidates' numeric domain.
///
/// A same-thread signal may read `fallback` while detection is active, but it
/// does not force the outer selector to retain that provider. Other threads
/// still wait for the completed selection.
#[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
pub(crate) fn select_same_domain_thread_owned_process_provider<F>(
  state: &AtomicU8,
  unknown: u8,
  selecting: u8,
  owner_pid: &AtomicI32,
  owner_tid: &AtomicI32,
  fallback: u8,
  detect: F,
) -> u8
where
  F: Fn() -> u8,
{
  let provider = state.load(Ordering::Acquire);
  if provider != unknown && provider != selecting && provider != FORCED_PROCESS_SELECTION {
    return provider;
  }
  select_thread_owned_process_provider_slow(
    state, unknown, selecting, owner_pid, owner_tid, fallback, true, detect,
  )
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
#[cold]
#[inline(never)]
fn select_thread_owned_process_provider_slow<F>(
  state: &AtomicU8,
  unknown: u8,
  selecting: u8,
  owner_pid: &AtomicI32,
  owner_tid: &AtomicI32,
  fallback: u8,
  reentrant_fallback_is_same_domain: bool,
  detect: F,
) -> u8
where
  F: Fn() -> u8,
{
  let tid = current_thread_id();
  let pid = process_id();
  loop {
    match state.load(Ordering::Acquire) {
      current if current == unknown => {
        if owner_tid.load(Ordering::Acquire) == 0 {
          // Publishing the process before claiming the thread makes an
          // inherited non-zero owner distinguishable in a fork child. Same-
          // process contenders can race this store because they write the
          // same value.
          owner_pid.store(pid, Ordering::Relaxed);
        }
        if owner_tid.compare_exchange(0, tid, Ordering::AcqRel, Ordering::Acquire).is_ok() {
          if state
            .compare_exchange(unknown, selecting, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
          {
            let detected = detect();
            let selected = match state.compare_exchange(
              selecting,
              detected,
              Ordering::Release,
              Ordering::Acquire,
            ) {
              Ok(_) => detected,
              Err(FORCED_PROCESS_SELECTION) => {
                state.store(fallback, Ordering::Release);
                fallback
              }
              Err(published) => published,
            };
            let _ = owner_tid.compare_exchange(tid, 0, Ordering::Release, Ordering::Relaxed);
            return selected;
          }
          let _ = owner_tid.compare_exchange(tid, 0, Ordering::Release, Ordering::Relaxed);
          continue;
        }

        if owner_tid.load(Ordering::Relaxed) == tid {
          if reentrant_fallback_is_same_domain {
            return fallback;
          }
          let _ = state.compare_exchange(
            unknown,
            FORCED_PROCESS_SELECTION,
            Ordering::AcqRel,
            Ordering::Acquire,
          );
          return fallback;
        }
        if owner_pid.load(Ordering::Acquire) != pid {
          let inherited_owner = owner_tid.load(Ordering::Relaxed);
          if inherited_owner != 0 {
            let _ =
              owner_tid.compare_exchange(inherited_owner, 0, Ordering::AcqRel, Ordering::Acquire);
          }
          continue;
        }
        core::hint::spin_loop();
      }
      current if current == selecting => {
        if recover_inherited_selection(state, selecting, unknown, owner_pid) {
          owner_tid.store(0, Ordering::Release);
          continue;
        }
        if owner_tid.load(Ordering::Relaxed) == tid {
          if reentrant_fallback_is_same_domain {
            return fallback;
          }
          let _ = state.compare_exchange(
            selecting,
            FORCED_PROCESS_SELECTION,
            Ordering::AcqRel,
            Ordering::Acquire,
          );
          return fallback;
        }
        core::hint::spin_loop();
      }
      FORCED_PROCESS_SELECTION => {
        if recover_inherited_selection(state, FORCED_PROCESS_SELECTION, unknown, owner_pid) {
          owner_tid.store(0, Ordering::Release);
          continue;
        }
        if owner_tid.load(Ordering::Acquire) == 0 {
          let _ = state.compare_exchange(
            FORCED_PROCESS_SELECTION,
            fallback,
            Ordering::Release,
            Ordering::Acquire,
          );
          return fallback;
        }
        if owner_tid.load(Ordering::Relaxed) == tid {
          return fallback;
        }
        core::hint::spin_loop();
      }
      provider => return provider,
    }
  }
}

#[inline]
#[must_use]
pub fn nanos_per_tick_q32() -> u64 {
  let cached = NANOS_PER_TICK_Q32.load(Ordering::Relaxed);
  if cached != 0 {
    return cached;
  }
  initialize_nanos_per_tick_q32()
}

#[cold]
#[inline(never)]
fn initialize_nanos_per_tick_q32() -> u64 {
  let initial = read_local_nanos_per_tick_q32();
  let scale =
    match NANOS_PER_TICK_Q32.compare_exchange(0, initial, Ordering::Relaxed, Ordering::Relaxed) {
      Ok(_) => initial,
      Err(published) => published,
    };

  // Spawn the periodic recalibration thread on first use. Only compiled
  // when the `recalibrate-background` feature is enabled — which is the
  // only feature that pulls in `std`.
  #[cfg(feature = "recalibrate-background")]
  crate::background::ensure_thread();

  scale
}

#[inline]
#[must_use]
pub fn ordered_nanos_per_tick_q32() -> u64 {
  #[cfg(all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86", target_arch = "x86_64"),
  ))]
  {
    // The selector publishes the selected domain's scale before its provider.
    // The initial nanosecond scale is exact for the reentrant OS-clock route.
    return ORDERED_NANOS_PER_TICK_Q32.load(Ordering::Acquire);
  }

  #[cfg(not(all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86", target_arch = "x86_64"),
  )))]
  let cached = ORDERED_NANOS_PER_TICK_Q32.load(Ordering::Relaxed);
  #[cfg(not(all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86", target_arch = "x86_64"),
  )))]
  if cached != 0 {
    return cached;
  }
  #[cfg(not(all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86", target_arch = "x86_64"),
  )))]
  initialize_ordered_nanos_per_tick_q32()
}

#[inline]
pub(crate) fn ordered_ticks_with_scale() -> (u64, u64) {
  #[cfg(all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86", target_arch = "x86_64"),
  ))]
  {
    return linux_x86_wall::ticks_ordered_with_scale();
  }
  #[cfg(not(all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86", target_arch = "x86_64"),
  )))]
  (ticks_ordered(), ordered_nanos_per_tick_q32())
}

pub(crate) fn publish_ordered_nanos_per_tick_q32(scale: u64) {
  ORDERED_NANOS_PER_TICK_Q32.store(scale, Ordering::Release);
  #[cfg(all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86", target_arch = "x86_64"),
  ))]
  linux_x86_wall::update_ordered_hot_scale(scale);
}

#[cold]
#[inline(never)]
#[cfg(not(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86", target_arch = "x86_64"),
)))]
fn initialize_ordered_nanos_per_tick_q32() -> u64 {
  let initial = read_ordered_nanos_per_tick_q32();
  let scale = match ORDERED_NANOS_PER_TICK_Q32.compare_exchange(
    0,
    initial,
    Ordering::Relaxed,
    Ordering::Relaxed,
  ) {
    Ok(_) => initial,
    Err(published) => published,
  };

  #[cfg(feature = "recalibrate-background")]
  crate::background::ensure_thread();

  scale
}

#[inline]
pub(crate) fn scale_from_ratio(nanos_numerator: u64, ticks_denominator: u64) -> u64 {
  let denominator = u128::from(ticks_denominator.max(1));
  let scale = (u128::from(nanos_numerator) << 32) / denominator;
  u64::try_from(scale).unwrap_or(u64::MAX).max(1)
}

#[cfg(target_os = "macos")]
#[inline]
fn read_local_nanos_per_tick_q32() -> u64 {
  let (numer, denom) = fallback::mach_timebase();
  scale_from_ratio(u64::from(numer), u64::from(denom))
}

#[cfg(not(target_os = "macos"))]
#[inline]
fn read_local_nanos_per_tick_q32() -> u64 {
  scale_from_ratio(1_000_000_000, read_local_frequency())
}

#[cfg(target_os = "macos")]
#[inline]
fn read_ordered_nanos_per_tick_q32() -> u64 {
  let (numer, denom) = fallback::mach_timebase();
  scale_from_ratio(u64::from(numer), u64::from(denom))
}

#[cfg(all(
  not(target_os = "macos"),
  not(all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86", target_arch = "x86_64"),
  )),
))]
#[inline]
fn read_ordered_nanos_per_tick_q32() -> u64 {
  scale_from_ratio(1_000_000_000, read_ordered_frequency())
}

// Windows independently selects QPC/QPF or a precise interrupt-time clock at
// 10 MHz for each wall contract.
#[cfg(target_os = "windows")]
#[inline]
fn read_local_frequency() -> u64 {
  fallback::instant_frequency()
}

// aarch64 outside Linux, Windows, and macOS uses the architectural frequency
// register. macOS derives its authoritative ratio from mach_timebase_info for
// every commpage mode, including modes that prohibit EL0 counter registers.
#[cfg(all(
  target_arch = "aarch64",
  not(any(
    target_os = "android",
    target_os = "linux",
    target_os = "windows",
    target_os = "macos",
  ))
))]
#[inline]
fn read_local_frequency() -> u64 {
  aarch64::cntfrq()
}

// Linux/Android aarch64 independently select the direct architectural counter
// or nanosecond CLOCK_MONOTONIC for each wall-clock contract.
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[inline]
fn read_local_frequency() -> u64 {
  linux_aarch64_wall::instant_frequency()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[inline]
fn read_local_frequency() -> u64 {
  // ticks() returns nanos directly; identity Q32 transform.
  1_000_000_000
}

#[cfg(target_os = "wasi")]
#[inline]
fn read_local_frequency() -> u64 {
  // ticks() returns nanos from clock_time_get directly; identity transform.
  1_000_000_000
}

// Linux-kernel x86 selects each wall timeline independently. CLOCK_MONOTONIC
// is already nanoseconds; a selected TSC uses CPUID frequency metadata or the
// calibrated rate shared by both direct-counter domains.
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[inline]
fn read_local_frequency() -> u64 {
  linux_x86_wall::frequency()
}

// x86 outside Windows, macOS, FreeBSD, Linux, and Android: prefer CPUID leaf 15h when
// available — modern Intel (Skylake+) and AMD (Zen2+) expose the exact
// architectural TSC frequency, eliminating the ~500 ppm error baked into
// spin-loop calibration. Fall back to calibration on older / virtualized CPUs
// that zero the leaf (typical on Firecracker, Azure VMs, and some Hyper-V
// guests). FreeBSD instead uses its selected-provider state and kernel-owned
// machdep.tsc_freq, or an identity scale for CLOCK_MONOTONIC.
#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "linux",
    target_os = "android",
  )),
))]
#[inline]
fn read_local_frequency() -> u64 {
  #[cfg(target_arch = "x86_64")]
  if let Some(hz) = x86_64::cpuid_tsc_hz() {
    return hz;
  }
  #[cfg(target_arch = "x86")]
  if let Some(hz) = x86::cpuid_tsc_hz() {
    return hz;
  }
  crate::calibration::calibrate_frequency()
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[inline]
fn read_local_frequency() -> u64 {
  freebsd_x86_64::instant_frequency()
}

#[cfg(target_arch = "riscv64")]
#[inline]
fn read_local_frequency() -> u64 {
  riscv64::instant_frequency()
}

#[cfg(target_arch = "loongarch64")]
#[inline]
fn read_local_frequency() -> u64 {
  loongarch64::instant_frequency()
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[inline]
fn read_local_frequency() -> u64 {
  powerpc64::instant_frequency()
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[inline]
fn read_local_frequency() -> u64 {
  linux_clock_wall::instant_frequency()
}

#[cfg(not(any(
  target_arch = "aarch64",
  target_arch = "x86_64",
  target_arch = "x86",
  target_arch = "riscv64",
  target_arch = "loongarch64",
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
  all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")),
  target_os = "macos",
  target_os = "wasi",
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
)))]
#[inline]
fn read_local_frequency() -> u64 {
  // Architectures without a direct counter use a platform monotonic source
  // whose ticks are already nanoseconds.
  1_000_000_000
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[inline]
fn read_ordered_frequency() -> u64 {
  linux_aarch64_wall::ordered_frequency()
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[inline]
fn read_ordered_frequency() -> u64 {
  freebsd_x86_64::ordered_frequency()
}

#[cfg(target_arch = "riscv64")]
#[inline]
fn read_ordered_frequency() -> u64 {
  riscv64::ordered_frequency()
}

#[cfg(target_arch = "loongarch64")]
#[inline]
fn read_ordered_frequency() -> u64 {
  loongarch64::ordered_frequency()
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[inline]
fn read_ordered_frequency() -> u64 {
  powerpc64::ordered_frequency()
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[inline]
fn read_ordered_frequency() -> u64 {
  linux_clock_wall::ordered_frequency()
}

#[cfg(target_os = "windows")]
#[inline]
fn read_ordered_frequency() -> u64 {
  fallback::ordered_frequency()
}

#[cfg(not(any(
  target_os = "macos",
  target_os = "windows",
  all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86_64", target_arch = "x86"),
  ),
  all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
  all(target_arch = "x86_64", target_os = "freebsd"),
  target_arch = "riscv64",
  target_arch = "loongarch64",
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
  all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")),
)))]
#[inline]
fn read_ordered_frequency() -> u64 {
  read_local_frequency()
}

/// Re-derive applicable tick-to-nanosecond scales against the platform
/// monotonic clock and atomically replace their cached Q32 reciprocals.
///
/// On platforms where the frequency comes from an authoritative register
/// (macOS Mach timebase, Windows QPF, WASI / wasm
/// fixed at 1 GHz), this is a no-op. On x86 / x86_64 TSC targets and on
/// aarch64 Linux, recalibration measures the actual counter rate against the
/// platform monotonic clock. Independently selected local and ordered
/// providers are updated only when they use that direct counter
/// (`clock_gettime(CLOCK_MONOTONIC)` on Unix).
///
/// This is the manual / wholesale-replace path — the new scale takes
/// effect immediately. The background-recal thread (when the
/// `recalibrate-background` feature is enabled) feeds new measurements
/// through an EMA blender instead via [`recalibrate_measure`], so a single
/// noisy calibration window can't jolt the scale on virtualized hosts.
///
/// `recalibrate` is `#![no_std]`-compatible; it uses the same spin-loop +
/// platform-monotonic path that already runs at startup.
pub fn recalibrate() {
  let update = measure_scales_for_recal();
  if let Some(scale) = update.local {
    NANOS_PER_TICK_Q32.store(scale, Ordering::Release);
  }
  if let Some(scale) = update.ordered {
    publish_ordered_nanos_per_tick_q32(scale);
  }
}

#[derive(Clone, Copy)]
pub(crate) struct RecalibrationScaleUpdate {
  pub(crate) local: Option<u64>,
  pub(crate) ordered: Option<u64>,
}

/// Re-measure applicable architectural counters without committing either
/// result. The background thread blends each returned scale independently;
/// [`recalibrate`] above installs the same values immediately.
#[cfg(feature = "recalibrate-background")]
pub(crate) fn recalibrate_measure() -> RecalibrationScaleUpdate {
  measure_scales_for_recal()
}

// Skip CPUID 15h here: it reports *nominal* frequency, which doesn't change.
// Recalibration's job is to track *actual* frequency drift over uptime, so we
// go straight to the platform-monotonic spin-loop calibration. Local and
// ordered providers use the same architectural counter whenever both need a
// measured rate, so one calibration window safely updates both domains.
#[cfg(any(
  all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(any(target_os = "windows", target_os = "macos", target_os = "freebsd")),
  ),
  all(target_arch = "aarch64", target_os = "linux"),
  all(target_arch = "riscv64", target_os = "linux"),
  all(target_arch = "loongarch64", target_os = "linux"),
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
))]
#[inline]
fn measure_scales_for_recal() -> RecalibrationScaleUpdate {
  let (local, ordered) = recalibration_domains();
  if !local && !ordered {
    return RecalibrationScaleUpdate { local: None, ordered: None };
  }

  let hz = if local {
    crate::calibration::calibrate_frequency()
  } else {
    crate::calibration::calibrate_frequency_with(ticks_ordered_unordered)
  };
  if hz == 0 {
    return RecalibrationScaleUpdate { local: None, ordered: None };
  }

  let scale = scale_from_ratio(1_000_000_000, hz);
  RecalibrationScaleUpdate { local: local.then_some(scale), ordered: ordered.then_some(scale) }
}

#[cfg(not(any(
  all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(any(target_os = "windows", target_os = "macos", target_os = "freebsd")),
  ),
  all(target_arch = "aarch64", target_os = "linux"),
  all(target_arch = "riscv64", target_os = "linux"),
  all(target_arch = "loongarch64", target_os = "linux"),
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
)))]
#[inline]
fn measure_scales_for_recal() -> RecalibrationScaleUpdate {
  RecalibrationScaleUpdate { local: None, ordered: None }
}

#[cfg(any(
  all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(any(target_os = "windows", target_os = "macos", target_os = "freebsd")),
  ),
  all(target_arch = "aarch64", target_os = "linux"),
  all(target_arch = "riscv64", target_os = "linux"),
  all(target_arch = "loongarch64", target_os = "linux"),
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
))]
#[inline]
fn recalibration_domains() -> (bool, bool) {
  #[cfg(all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86_64", target_arch = "x86"),
  ))]
  {
    return (linux_x86_wall::instant_uses_tsc(), linux_x86_wall::ordered_uses_tsc());
  }

  #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
  {
    return (linux_aarch64_wall::instant_uses_cntvct(), linux_aarch64_wall::ordered_uses_cntvct());
  }

  #[cfg(all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(any(
      target_os = "android",
      target_os = "linux",
      target_os = "windows",
      target_os = "macos",
      target_os = "freebsd",
    )),
  ))]
  {
    return (true, true);
  }

  #[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
  {
    let needs_measured_rate = powerpc64::needs_frequency_recalibration();
    return (
      needs_measured_rate && powerpc64::instant_uses_timebase(),
      needs_measured_rate && powerpc64::ordered_uses_timebase(),
    );
  }

  #[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
  {
    let needs_measured_rate = loongarch64::needs_frequency_recalibration();
    return (
      needs_measured_rate && loongarch64::instant_uses_stable_counter(),
      needs_measured_rate && loongarch64::ordered_uses_stable_counter(),
    );
  }

  #[cfg(all(target_arch = "riscv64", target_os = "linux"))]
  {
    return (false, false);
  }

  #[cfg(not(any(
    all(
      any(target_os = "android", target_os = "linux"),
      any(target_arch = "x86_64", target_arch = "x86"),
    ),
    all(target_arch = "aarch64", target_os = "linux"),
    all(
      any(target_arch = "x86_64", target_arch = "x86"),
      not(any(
        target_os = "android",
        target_os = "linux",
        target_os = "windows",
        target_os = "macos",
        target_os = "freebsd",
      )),
    ),
    all(target_arch = "riscv64", target_os = "linux"),
    all(target_arch = "loongarch64", target_os = "linux"),
    all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
  )))]
  {
    (false, false)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn qpc_frequency_scale_is_fixed_point_without_read_time_division() {
    assert_eq!(scale_from_ratio(1_000_000_000, 10_000_000), 100_u64 << 32);
  }

  #[test]
  fn mach_timebase_scale_preserves_the_documented_ratio() {
    assert_eq!(scale_from_ratio(125, 3), ((125_u128 << 32) / 3) as u64);
  }

  #[test]
  fn power_timebase_scale_preserves_the_exact_512_mhz_ratio() {
    assert_eq!(scale_from_ratio(1_000_000_000, 512_000_000), 125_u64 << 26);
  }

  #[test]
  fn scale_is_never_the_uninitialized_sentinel() {
    assert_eq!(scale_from_ratio(0, 0), 1);
  }

  #[cfg(any(
    all(unix, any(target_arch = "x86", target_arch = "x86_64"), not(target_os = "macos"),),
    all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
    all(target_arch = "arm", target_os = "linux"),
    all(target_arch = "s390x", target_os = "linux"),
    all(target_arch = "riscv64", target_os = "linux"),
    all(target_arch = "loongarch64", target_os = "linux"),
    all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
  ))]
  #[test]
  fn inherited_process_selection_is_recoverable() {
    let state = AtomicU8::new(0);
    let owner_pid = AtomicI32::new(0);
    assert!(claim_process_selection(&state, 0, 1, &owner_pid));
    assert!(!recover_inherited_selection(&state, 1, 0, &owner_pid));

    owner_pid.store(process_id().wrapping_add(1), Ordering::Relaxed);
    assert!(recover_inherited_selection(&state, 1, 0, &owner_pid));
    assert_eq!(state.load(Ordering::Relaxed), 0);
  }

  #[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
  #[test]
  fn same_thread_reentry_forces_one_numeric_domain() {
    let state = AtomicU8::new(0);
    let owner_pid = AtomicI32::new(0);
    let owner_tid = AtomicI32::new(0);
    let nested = AtomicU8::new(0);

    let selected =
      select_thread_owned_process_provider(&state, 0, 1, &owner_pid, &owner_tid, 3, || {
        let reentrant =
          select_thread_owned_process_provider(&state, 0, 1, &owner_pid, &owner_tid, 3, || 4);
        nested.store(reentrant, Ordering::Relaxed);
        4
      });

    assert_eq!(nested.load(Ordering::Relaxed), 3);
    assert_eq!(selected, 3);
    assert_eq!(state.load(Ordering::Acquire), 3);
    assert_eq!(owner_tid.load(Ordering::Relaxed), 0);
  }

  #[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
  #[test]
  fn same_domain_reentry_does_not_poison_the_measured_winner() {
    let state = AtomicU8::new(0);
    let owner_pid = AtomicI32::new(0);
    let owner_tid = AtomicI32::new(0);
    let nested = AtomicU8::new(0);

    let selected = select_same_domain_thread_owned_process_provider(
      &state,
      0,
      1,
      &owner_pid,
      &owner_tid,
      3,
      || {
        let reentrant = select_same_domain_thread_owned_process_provider(
          &state,
          0,
          1,
          &owner_pid,
          &owner_tid,
          3,
          || 4,
        );
        nested.store(reentrant, Ordering::Relaxed);
        4
      },
    );

    assert_eq!(nested.load(Ordering::Relaxed), 3);
    assert_eq!(selected, 4);
    assert_eq!(state.load(Ordering::Acquire), 4);
    assert_eq!(owner_tid.load(Ordering::Relaxed), 0);
  }

  #[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
  #[test]
  fn inherited_owner_before_state_claim_is_recoverable() {
    let state = AtomicU8::new(0);
    let owner_pid = AtomicI32::new(process_id().wrapping_add(1));
    let owner_tid = AtomicI32::new(-1);

    let selected =
      select_thread_owned_process_provider(&state, 0, 1, &owner_pid, &owner_tid, 3, || 4);

    assert_eq!(selected, 4);
    assert_eq!(state.load(Ordering::Acquire), 4);
    assert_eq!(owner_tid.load(Ordering::Relaxed), 0);
  }

  #[cfg(any(
    all(unix, any(target_arch = "x86", target_arch = "x86_64"), not(target_os = "macos"),),
    all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
    all(target_arch = "arm", target_os = "linux"),
    all(target_arch = "s390x", target_os = "linux"),
    all(target_arch = "riscv64", target_os = "linux"),
    all(target_arch = "loongarch64", target_os = "linux"),
    all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
  ))]
  #[test]
  fn forked_child_recovers_parent_owned_selection() {
    let state = AtomicU8::new(0);
    let owner_pid = AtomicI32::new(0);
    assert!(claim_process_selection(&state, 0, 1, &owner_pid));

    // SAFETY: the child uses only atomics, `getpid`, and `_exit` before
    // terminating; the parent immediately reaps it.
    let child = unsafe { libc::fork() };
    assert!(child >= 0);
    if child == 0 {
      let recovered = recover_inherited_selection(&state, 1, 0, &owner_pid);
      let reset = state.load(Ordering::Relaxed) == 0;
      // SAFETY: `_exit` terminates the fork child without running inherited
      // Rust destructors or stdio cleanup.
      unsafe { libc::_exit(if recovered && reset { 0 } else { 1 }) };
    }

    let mut status = 0;
    // SAFETY: `child` is the live child PID returned above and `status` is
    // writable storage for its wait status.
    assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
    assert_eq!(status, 0);
  }
}
