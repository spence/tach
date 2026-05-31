//! Software enforcement of synchronization-order monotonicity for
//! [`crate::SyncedInstant`].
//!
//! # When this is needed (empirically determined)
//!
//! `SyncedInstant`'s enforcement is applied on every multi-threaded
//! platform tach supports because empirical testing across 6 production
//! cells (Apple Silicon M1, AWS Graviton 3, AWS Intel virtualized + bare
//! metal, AWS Lambda Firecracker, GitHub Actions Windows Server 2025)
//! shows bare arch-counter reads fail synchronization-order monotonicity on
//! every multi-threaded platform tested. Per-cell violation rates from
//! the `measure_synchronization_order` load-then-now-then-check test:
//!
//! | Platform | bare counter | violations / reads |
//! |---|---|---|
//! | Apple Silicon M1 | `mrs cntvct_el0` | 17.4M / 144M (12%) |
//! | Graviton 3 (c7g) | `mrs cntvct_el0` | 1.8K / 382M (sub-ppm) |
//! | Intel virtualized (t3) | `rdtsc` | 42 / 662M (sub-ppm) |
//! | Intel bare metal (m7i) | `rdtsc` | 9.6M / 154M (6%) |
//! | Firecracker (lambda) | `rdtsc` | 13K / 85M (sub-‰) |
//! | Windows (gh-windows) | `rdtsc` | 1.7M / 260M (0.6%) |
//!
//! Rates vary by orders of magnitude but every multi-threaded platform
//! shows non-zero contract violations on the bare read. ARMv8 ARM §D11.1.2
//! says `cntvct_el0` is a single global counter and Intel SDM says invariant
//! TSC is firmware-synchronized — both turn out to be aspirational under
//! the strict load-then-now contract. Software enforcement via
//! `AtomicU64::fetch_max(AcqRel)` is required.
//!
//! Compiled out on wasm32 (single-threaded JS realm with W3C HRT strict
//! `performance.now()`) and WASI (single-threaded execution model with
//! strict spec) — those platforms have no concurrency for the enforcement
//! to enforce against. See the cfg gates in `super::direct::ticks_synced`
//! and `super::mod`. On those targets `SyncedInstant::now()` compiles
//! to the same instruction as `Instant::now()` — zero overhead.
//!
//! Cost on every other platform: one `fetch_max` per call —
//! ~10-25 cycles uncontended.
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
//! call. This is the cost of synchronization-order monotonicity by software
//! enforcement. Plain [`crate::Instant`] remains available for callers
//! that don't need the cross-thread guarantee and want the fastest possible
//! read.

use core::sync::atomic::{AtomicU64, Ordering};

pub(crate) static GLOBAL_LAST_TSC: AtomicU64 = AtomicU64::new(0);

/// Read the counter and enforce synchronization-order monotonicity via
/// `fetch_max` on the process-global `GLOBAL_LAST_TSC`. See the module
/// documentation for the correctness argument.
#[inline]
pub(crate) fn ticks_synced_enforced() -> u64 {
  let tsc = super::ticks();
  let prev = GLOBAL_LAST_TSC.fetch_max(tsc, Ordering::AcqRel);
  tsc.max(prev)
}
