//! Software enforcement of strict cross-thread monotonicity for
//! [`crate::MonotonicInstant`].
//!
//! # Why this is uniform across architectures
//!
//! The original design plan claimed `MonotonicInstant` would be free on
//! architectures where the underlying counter is architecturally globally
//! synchronized (aarch64 `cntvct_el0`, RISC-V `time`, LoongArch stable
//! counter, runtime-provided clocks like `Performance.now()` and WASI
//! `clock_time_get`). Empirical validation on Apple Silicon M1 falsified
//! that assumption: a strict cross-thread monotonicity test reading bare
//! `cntvct_el0` and racing N threads through `AtomicU64::fetch_max` produced
//! ~2.4M violations in a 500ms window. The ARMv8 ARM §D11.1.2 wording ("all
//! instances of `CNTVCT_EL0` within a system must report the same value")
//! is aspirational on M1 — there is measurable sub-microsecond per-core
//! slop in practice.
//!
//! The same risk applies to other architectures whose specs claim global
//! synchronization but whose implementations may not honor it tightly:
//! RISC-V `time`, LoongArch stable counter, and even kernel-provided
//! monotonic clocks under specific configurations. Rather than relying on
//! the spec-says-so-it-must-be-true argument and shipping a guarantee that
//! has empirical counterexamples, this module applies `fetch_max`
//! enforcement uniformly. The cost is one atomic operation per
//! `MonotonicInstant::now()` call — ~10-25 cycles uncontended.
//!
//! # Algorithm
//!
//! ```text
//! fn now() -> u64 {
//!     let tsc = bare_counter_read();
//!     let prev = GLOBAL_LAST_TSC.fetch_max(tsc, AcqRel);
//!     tsc.max(prev)
//! }
//! ```
//!
//! Every call performs a single read-modify-write on the process-global
//! `GLOBAL_LAST_TSC`. `AcqRel` ordering ensures:
//!
//! - **Release half**: subsequent loads of `GLOBAL_LAST_TSC` from other
//!   threads (and any user-side Release-Acquire synchronization that
//!   follows this call) observe at least the value we just wrote.
//! - **Acquire half**: we observe every prior write from any thread, so
//!   `tsc.max(prev)` always returns a value `>=` everything previously
//!   published.
//!
//! # On the fast path that isn't there
//!
//! An earlier design proposed a TLS fast path keyed on `rdtscp`'s CPU-ID
//! return: if a thread had not migrated since its last call, return the
//! bare TSC without performing the `fetch_max`. That algorithm is incorrect.
//! User-side synchronization (channels, mutexes, atomics) carries
//! happens-before through writes to atomic memory; a fast-path return that
//! does not write to `GLOBAL_LAST_TSC` leaves nothing for the user's
//! synchronization to carry across threads. The corrected algorithm is
//! uniform — every call performs the write.
//!
//! # Contention
//!
//! Under heavy contention (many threads simultaneously calling `now()`,
//! hammering the single cache line that holds `GLOBAL_LAST_TSC`), the
//! `fetch_max` can degrade from ~10-25 cycles uncontended to 100+ ns per
//! call. This is the cost of strict cross-thread monotonicity by software
//! enforcement. Plain [`crate::Instant`] remains available for callers
//! that don't need the cross-thread guarantee and want the fastest possible
//! read.

use core::sync::atomic::{AtomicU64, Ordering};

pub(crate) static GLOBAL_LAST_TSC: AtomicU64 = AtomicU64::new(0);

/// Read the counter and enforce strict cross-thread monotonicity via
/// `fetch_max` on the process-global `GLOBAL_LAST_TSC`. See the module
/// documentation for the correctness argument.
#[inline]
pub(crate) fn ticks_monotonic_enforced() -> u64 {
  let tsc = super::ticks();
  let prev = GLOBAL_LAST_TSC.fetch_max(tsc, Ordering::AcqRel);
  tsc.max(prev)
}
