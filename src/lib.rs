#![no_std]
#![warn(clippy::undocumented_unsafe_blocks)]
#![warn(rustdoc::broken_intra_doc_links)]

//! Three Instant-shaped timers for three timing contracts.
//!
//! All three APIs compute elapsed time as [`core::time::Duration`]. They differ
//! in the quantity that advances and where samples may be compared safely:
//!
//! | Job | Type | Contract |
//! |---|---|---|
//! | Same-thread elapsed | [`Instant`] | Wall-rate time; endpoints stay local |
//! | Ordered elapsed | [`OrderedInstant`] | Wall time ordered after memory observations |
//! | Thread CPU, where native | [`ThreadCpuInstant`] | CPU time or explicit wall fallback |
//!
//! With default features, each type selects the fastest eligible provider for
//! its contract. Eligible wall providers must be monotonic, wall-rate, and
//! high-resolution; deliberately coarsened clocks are a different contract.
//! Disabling default features requests the syscall-only `no_std` thread-CPU
//! implementation and can therefore trade speed for dependency surface.
//!
//! On six benchmark environments, each type has the fastest tested
//! steady-state read and elapsed bracket for its contract, with
//! `max(1 ns, 5%)` treated as a practical material tie. A separate 24-target
//! compile and codegen matrix proves API availability and provider routing; it
//! does not claim measured speed on unbenchmarked hardware.
//!
//! # Quick start
//!
//! ```
//! use tach::{Instant, ThreadCpuInstant};
//!
//! let wall_start = Instant::now();
//! let wall_elapsed = wall_start.elapsed();
//!
//! let cpu_start = ThreadCpuInstant::now();
//! let cpu_elapsed = cpu_start.elapsed();
//!
//! assert!(wall_elapsed <= wall_start.elapsed());
//! assert!(cpu_elapsed <= cpu_start.elapsed());
//! ```
//!
//! # Choosing a type
//!
//! Use [`Instant`] when both endpoints of an elapsed-time bracket stay local to
//! one thread. Direct-counter providers do not order the sample after prior
//! memory operations.
//!
//! Use [`OrderedInstant`] when a timestamp participates in a cross-thread
//! happens-before relationship. Direct-counter targets select an architecture
//! barrier; Windows and Intel macOS fence before their reliable platform
//! clock. The load-then-now-then-check contract produced zero inversions in
//! about 10.9 billion tested x86 and aarch64 reads. RISC-V's ratified Zicsr
//! ordering rules cover `fence r, i; rdtime`. LoongArch Linux uses a raw
//! system-call exception boundary before `rdtime.d`; Linux armv7 and s390x
//! fence before `CLOCK_MONOTONIC`; Linux powerpc64 GNU uses `sync; mftb`.
//!
//! Use [`ThreadCpuInstant`] for CPU delivered to the calling OS thread. Native
//! providers freeze while the thread is sleeping or descheduled. Targets with
//! no portable thread clock use an explicitly reported monotonic-wall fallback;
//! check [`ThreadCpuInstant::measures_thread_cpu_time`] when that distinction is
//! correctness-sensitive. Native candidates must retain the platform clock's
//! full scheduled-runtime precision; a cheaper coarsened accounting API is not
//! eligible for selection.
//!
//! Linux providers use the native `CLOCK_THREAD_CPUTIME_ID` timeline. Targets
//! with multiple equivalent syscall entry paths measure those paths once per
//! process and retain the fastest reliable route on the hot path.
//!
//! # Hardware assumption
//!
//! Direct-counter providers assume a coherent, monotonic architectural
//! counter. Windows uses QPC because raw TSC/CNTVCT cost and frequency probes
//! cannot establish Windows' cross-core, sleep, and VM-migration guarantees.
//! Intel macOS admits a bare invariant TSC only for same-thread `Instant`
//! after runtime eligibility and complete-path cost checks; its ordered timer
//! stays on the platform-owned reliable timeline.
//! on other hosts whose OS marks a TSC clocksource unstable because cores are
//! genuinely desynchronized, use the platform clock instead.
//!
//! Linux can explicitly configure architectural counter reads to fault on a
//! per-thread basis. Initial selection fails closed when its calling thread is
//! denied, but a process-wide direct or vDSO winner requires every reading
//! thread to retain counter permission. Calling `PR_SET_TSC` to request a fault
//! is an external fault boundary for tach, libc's vDSO clocks, and other direct
//! counter readers.

mod arch;
// Calibration is needed wherever the selected architectural counter doesn't
// self-report an NTP-corrected rate: x86 outside Windows/macOS/FreeBSD,
// aarch64 Linux, and riscv64 / loongarch64. FreeBSD uses the authoritative
// kernel TSC rate or nanosecond CLOCK_MONOTONIC; Windows uses QPC/QPF, Intel
// macOS uses the Mach timebase, and Apple Silicon uses cntfrq_el0.
#[cfg(any(
  all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(any(target_os = "windows", target_os = "macos", target_os = "freebsd")),
  ),
  all(target_arch = "aarch64", target_os = "linux"),
  target_arch = "riscv64",
  target_arch = "loongarch64",
  all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
))]
mod calibration;
mod instant;
mod thread_cpu;

pub use instant::{Instant, OrderedInstant};
pub use thread_cpu::{ThreadCpuInstant, ThreadCpuProvider, ThreadCpuReadCost};

// `#![no_std]` remains the crate root. The default thread-cpu-inline feature
// links std only on the listed Linux-kernel perf targets for native TLS;
// --no-default-features preserves the strict no_std dependency surface.
// Recalibration, benchmark support, and tests also link std.
#[cfg(any(
  test,
  feature = "recalibrate-background",
  feature = "bench-internal",
  all(
    feature = "thread-cpu-inline",
    any(
      all(
        target_os = "linux",
        any(
          target_arch = "x86",
          target_arch = "x86_64",
          target_arch = "aarch64",
          target_arch = "arm",
          target_arch = "riscv64",
          target_arch = "s390x",
          target_arch = "loongarch64",
          target_arch = "powerpc64",
        ),
      ),
      all(target_os = "android", any(target_arch = "x86_64", target_arch = "aarch64"),),
    ),
  ),
))]
extern crate std;

#[cfg(feature = "recalibrate-background")]
mod background;

#[cfg(feature = "recalibrate-background")]
pub use background::set_recalibration_interval;

#[cfg(feature = "bench-internal")]
#[doc(hidden)]
pub mod bench;

#[cfg(test)]
mod tests {
  // Integration-shaped tests for the public API live in tests/instant.rs
  // (relocated in M2); this module keeps only genuinely private unit tests
  // that reach crate internals.
  #[cfg(all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(any(target_os = "windows", target_os = "macos")),
  ))]
  #[test]
  fn cpuid_15h_returns_something_or_none() {
    #[cfg(target_arch = "x86_64")]
    let _ = crate::arch::x86_64::cpuid_tsc_hz();
    #[cfg(target_arch = "x86")]
    let _ = crate::arch::x86::cpuid_tsc_hz();
  }
}
