//! Monotonicity and drift measurement primitives, used by `benches/skew.rs`
//! and the out-of-repo Lambda handler.
//!
//! Compiled only with the `bench-internal` Cargo feature. Hidden from docs.
//!
//! ## Design
//!
//! Each clock under test implements [`ClockSource`]. A static anchor captured
//! at process start lets us produce a single `u64` ns-since-anchor value from
//! every crate's opaque `Instant` type, so all clocks share the same atomic
//! word format for cross-thread monotonicity tests.

use std::prelude::v1::*;
use std::string::String;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant as StdInstantTy, SystemTime, UNIX_EPOCH};
use std::vec::Vec;

use serde::Serialize;

use crate::Instant as TachInstantTy;
use crate::SyncedInstant as TachSyncedInstantTy;
use crate::FencedInstant as TachFencedInstantTy;

/// A clock under test. Produces a `u64` of ns-since-anchor each `now_as_u64`
/// call, so cross-thread monotonicity tests can use one `AtomicU64` shape for
/// every crate.
pub trait ClockSource: Send + Sync + 'static {
  const NAME: &'static str;
  fn init_anchor();
  fn now_as_u64() -> u64;
  /// Whether the underlying clock source is the actual architectural counter
  /// (`true`) or a wall-clock fallback (`false`). For minstant/fastant on
  /// non-Linux-x86, the answer is fallback.
  fn backed_by_arch_counter() -> bool {
    true
  }
}

#[derive(Serialize, Clone, Debug)]
pub struct PerThreadResult {
  pub clock: &'static str,
  pub violations: u64,
  pub total_reads: u64,
  pub max_violation_ns: u64,
  pub duration_ns: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct CrossThreadResult {
  pub clock: &'static str,
  pub threads: u32,
  pub violations_per_thread: Vec<u64>,
  pub total_violations: u64,
  pub total_reads: u64,
  pub max_violation_ns: u64,
  pub preemption_dropped: u64,
  pub duration_ns: u64,
  /// (bucket_upper_ns, count) — covers <=50ns, <=500ns, <=5µs, <=50µs, >50µs.
  pub violation_histogram_ns: Vec<(u64, u64)>,
}

#[derive(Serialize, Clone, Debug)]
pub struct SkewSample {
  pub c_elapsed_ns: u64,
  pub ref_elapsed_ns: u64,
  pub skew_ns: i64,
  pub skew_ppm: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct SkewResult {
  pub clock: &'static str,
  pub interval: &'static str, // "1s" | "1m"
  pub samples: Vec<SkewSample>,
  pub median_skew_ns: i64,
  pub min_skew_ns: i64,
  pub max_skew_ns: i64,
  pub median_skew_ppm: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct SyncOrderResult {
  pub clock: &'static str,
  pub threads: u32,
  pub total_violations: u64,
  pub total_reads: u64,
  pub max_violation_ns: u64,
  pub duration_ns: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct ClockReport {
  pub backed_by_arch_counter: bool,
  pub per_thread: PerThreadResult,
  pub cross_thread: CrossThreadResult,
  pub synchronization_order: Option<SyncOrderResult>,
  pub skew_1s: SkewResult,
  pub skew_1m: Option<SkewResult>,
}

#[derive(Serialize, Clone, Debug)]
pub struct CellReport {
  pub schema: &'static str,
  pub cell: String,
  pub target_triple: &'static str,
  pub started_at_unix_ns: u128,
  pub host: HostInfo,
  pub tach_freq_hz: u64,
  pub tach_used_cpuid_15h: bool,
  pub clocks: std::collections::BTreeMap<String, ClockReport>,
}

#[derive(Serialize, Clone, Debug)]
pub struct HostInfo {
  pub cpu_model: String,
  pub num_cpus: u32,
  pub kernel: String,
}

// ── Measurement primitives ──────────────────────────────────────────────────

/// Per-thread monotonicity: tight loop on a single thread for `duration`,
/// count consecutive reads where `now() < previous`.
pub fn measure_per_thread<C: ClockSource>(duration: Duration) -> PerThreadResult {
  C::init_anchor();
  // Warmup
  for _ in 0..1_000 {
    let _ = C::now_as_u64();
  }

  let start = StdInstantTy::now();
  let mut previous = C::now_as_u64();
  let mut violations = 0u64;
  let mut max_violation_ns = 0u64;
  let mut total_reads = 1u64;
  let mut budget_check = 0u32;

  loop {
    let current = C::now_as_u64();
    total_reads += 1;
    if current < previous {
      violations += 1;
      let diff = previous - current;
      if diff > max_violation_ns {
        max_violation_ns = diff;
      }
    } else {
      previous = current;
    }

    budget_check = budget_check.wrapping_add(1);
    if budget_check & 0xFFFF == 0 && start.elapsed() >= duration {
      break;
    }
  }

  let duration_ns = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
  PerThreadResult { clock: C::NAME, violations, total_reads, max_violation_ns, duration_ns }
}

const HIST_BUCKETS_NS: &[u64] = &[50, 500, 5_000, 50_000, u64::MAX];

/// Cross-thread observation consistency: N threads racing on a shared atomic
/// max. A "violation" is a read that came in below a value some other thread
/// already published — i.e., we observed a non-monotonic timeline across
/// threads. The bracket-read filter (drop iterations preempted between the
/// counter read and the atomic publish) suppresses scheduling noise.
pub fn measure_cross_thread<C: ClockSource>(
  threads: usize,
  duration: Duration,
) -> CrossThreadResult {
  C::init_anchor();
  for _ in 0..1_000 {
    let _ = C::now_as_u64();
  }

  let max = std::sync::Arc::new(AtomicU64::new(0));
  let start_barrier = std::sync::Arc::new(std::sync::Barrier::new(threads + 1));
  let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

  let mut handles = Vec::with_capacity(threads);
  for _ in 0..threads {
    let max = std::sync::Arc::clone(&max);
    let start_barrier = std::sync::Arc::clone(&start_barrier);
    let stop = std::sync::Arc::clone(&stop);
    handles.push(thread::spawn(move || -> (u64, u64, u64, u64, [u64; 5]) {
      let mut local_violations = 0u64;
      let mut local_reads = 0u64;
      let mut local_max_violation = 0u64;
      let mut local_preempt = 0u64;
      let mut local_histogram = [0u64; 5];

      start_barrier.wait();

      while !stop.load(Ordering::Relaxed) {
        let r1 = C::now_as_u64();
        let prev = max.fetch_max(r1, Ordering::Relaxed);
        let r2 = C::now_as_u64();
        local_reads += 1;
        if r2.saturating_sub(r1) > 10_000 {
          local_preempt += 1;
          continue;
        }
        if r1 < prev {
          local_violations += 1;
          let diff = prev - r1;
          if diff > local_max_violation {
            local_max_violation = diff;
          }
          for (i, &upper) in HIST_BUCKETS_NS.iter().enumerate() {
            if diff <= upper {
              local_histogram[i] += 1;
              break;
            }
          }
        }
      }
      (local_violations, local_reads, local_max_violation, local_preempt, local_histogram)
    }));
  }

  start_barrier.wait();
  let wall_start = StdInstantTy::now();
  thread::sleep(duration);
  stop.store(true, Ordering::Relaxed);

  let mut violations_per_thread = Vec::with_capacity(threads);
  let mut total_violations = 0u64;
  let mut total_reads = 0u64;
  let mut max_violation_ns = 0u64;
  let mut preemption_dropped = 0u64;
  let mut histogram = [0u64; 5];

  for h in handles {
    let (v, r, mv, pp, hist) = h.join().expect("thread panic");
    violations_per_thread.push(v);
    total_violations += v;
    total_reads += r;
    if mv > max_violation_ns {
      max_violation_ns = mv;
    }
    preemption_dropped += pp;
    for i in 0..5 {
      histogram[i] += hist[i];
    }
  }

  let duration_ns = u64::try_from(wall_start.elapsed().as_nanos()).unwrap_or(u64::MAX);
  let violation_histogram_ns: Vec<(u64, u64)> =
    HIST_BUCKETS_NS.iter().copied().zip(histogram).collect();

  CrossThreadResult {
    clock: C::NAME,
    threads: u32::try_from(threads).unwrap_or(u32::MAX),
    violations_per_thread,
    total_violations,
    total_reads,
    max_violation_ns,
    preemption_dropped,
    duration_ns,
    violation_histogram_ns,
  }
}

/// Synchronization-order monotonicity test — empirically validate whether the
/// bare clock honors the happens-before-respecting contract.
///
/// Unlike `measure_cross_thread` (which uses a now-then-fetch_max pattern and
/// mixes hardware sync slop with publish-race jitter), this uses the
/// load-then-now-then-check pattern that directly validates the contract:
///
/// 1. **Acquire-load** the global `published` atomic. This synchronizes-with
///    any prior thread's Release write on the same atomic.
/// 2. Read the clock under test (`C::now_as_u64()`).
/// 3. Check that the new read is `>=` what we observed before reading. If not,
///    the bare clock failed to honor synchronization-order monotonicity at the
///    happens-before level.
/// 4. **Release-fetch_max** publishes our reading for the next iteration.
///
/// Returns `total_violations == 0` if and only if the clock is empirically
/// synchronization-order monotonic under the test conditions (N threads × the
/// given duration). Any non-zero value means the underlying clock needs
/// software enforcement (a process-global fetch_max wrapping every read) to
/// claim the synchronization-order contract.
///
/// This is the canonical test for deciding whether `SyncedInstant`'s
/// fetch_max enforcement is needed on a given platform.
pub fn measure_synchronization_order<C: ClockSource>(
  threads: usize,
  duration: Duration,
) -> SyncOrderResult {
  C::init_anchor();
  for _ in 0..1_000 {
    let _ = C::now_as_u64();
  }

  let published = std::sync::Arc::new(AtomicU64::new(0));
  let start_barrier = std::sync::Arc::new(std::sync::Barrier::new(threads + 1));
  let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

  let mut handles = Vec::with_capacity(threads);
  for _ in 0..threads {
    let published = std::sync::Arc::clone(&published);
    let start_barrier = std::sync::Arc::clone(&start_barrier);
    let stop = std::sync::Arc::clone(&stop);
    handles.push(thread::spawn(move || -> (u64, u64, u64) {
      let mut local_violations = 0u64;
      let mut local_reads = 0u64;
      let mut local_max_violation = 0u64;

      start_barrier.wait();

      while !stop.load(Ordering::Relaxed) {
        // (1) Acquire-load the latest published value. Synchronizes-with any
        //     prior Release publish on this atomic.
        let observed = published.load(Ordering::Acquire);
        // (2) Read the clock under test.
        let now_ns = C::now_as_u64();
        local_reads += 1;
        // (3) Check the contract: now_ns >= observed.
        if now_ns < observed {
          local_violations += 1;
          let diff = observed - now_ns;
          if diff > local_max_violation {
            local_max_violation = diff;
          }
        }
        // (4) Publish our value via Release fetch_max so future threads'
        //     Acquire-loads can observe us.
        published.fetch_max(now_ns, Ordering::Release);
      }
      (local_violations, local_reads, local_max_violation)
    }));
  }

  start_barrier.wait();
  let wall_start = StdInstantTy::now();
  thread::sleep(duration);
  stop.store(true, Ordering::Relaxed);

  let mut total_violations = 0u64;
  let mut total_reads = 0u64;
  let mut max_violation_ns = 0u64;

  for h in handles {
    let (v, r, mv) = h.join().expect("thread panic");
    total_violations += v;
    total_reads += r;
    if mv > max_violation_ns {
      max_violation_ns = mv;
    }
  }

  let duration_ns = u64::try_from(wall_start.elapsed().as_nanos()).unwrap_or(u64::MAX);

  SyncOrderResult {
    clock: C::NAME,
    threads: u32::try_from(threads).unwrap_or(u32::MAX),
    total_violations,
    total_reads,
    max_violation_ns,
    duration_ns,
  }
}

/// Reference clock — the same clock std::Instant uses on this platform.
fn reference_clock_ns() -> u64 {
  // We pick a `&'static` accessor at first call. On Linux we'd use
  // CLOCK_MONOTONIC, on macOS CLOCK_UPTIME_RAW (std 1.80+), on Windows
  // QueryPerformanceCounter. Using std::Instant directly is the most
  // portable proxy — it IS the reference we're comparing against — and
  // std::Instant::elapsed() (i.e., now - anchor) gives us the ns we need.
  static REF_ANCHOR: OnceLock<StdInstantTy> = OnceLock::new();
  let anchor = REF_ANCHOR.get_or_init(StdInstantTy::now);
  u64::try_from(StdInstantTy::now().duration_since(*anchor).as_nanos()).unwrap_or(u64::MAX)
}

/// Skew vs std::Instant over `interval`, repeated `samples` times. Reports
/// per-sample skew + median/min/max + median ppm.
pub fn measure_skew<C: ClockSource>(
  interval: Duration,
  samples: usize,
  interval_label: &'static str,
) -> SkewResult {
  C::init_anchor();
  let _ = reference_clock_ns();

  let mut all_samples = Vec::with_capacity(samples);
  for _ in 0..samples {
    let r_start = reference_clock_ns();
    let c_start = C::now_as_u64();

    thread::sleep(interval);

    let c_end = C::now_as_u64();
    let r_end = reference_clock_ns();

    let c_elapsed_ns = c_end.saturating_sub(c_start);
    let ref_elapsed_ns = r_end.saturating_sub(r_start);
    let skew_ns = c_elapsed_ns as i64 - ref_elapsed_ns as i64;
    let skew_ppm = if ref_elapsed_ns > 0 {
      (skew_ns as f64) * 1_000_000.0 / (ref_elapsed_ns as f64)
    } else {
      0.0
    };
    all_samples.push(SkewSample { c_elapsed_ns, ref_elapsed_ns, skew_ns, skew_ppm });
  }

  let mut sorted: Vec<i64> = all_samples.iter().map(|s| s.skew_ns).collect();
  sorted.sort_unstable();
  let median_skew_ns = sorted[sorted.len() / 2];
  let min_skew_ns = *sorted.first().unwrap_or(&0);
  let max_skew_ns = *sorted.last().unwrap_or(&0);
  let median_sample = &all_samples[all_samples.len() / 2];
  let median_skew_ppm = median_sample.skew_ppm;

  SkewResult {
    clock: C::NAME,
    interval: interval_label,
    samples: all_samples,
    median_skew_ns,
    min_skew_ns,
    max_skew_ns,
    median_skew_ppm,
  }
}

/// Wall-clock unix-ns when this report was started. Useful for ordering
/// multiple reports from a Lambda-style multi-invocation run.
pub fn unix_ns_now() -> u128 {
  SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

/// The frequency tach derived for the architectural counter on this host.
/// Triggers lazy init via `arch::nanos_per_tick_q32` if not already done.
pub fn tach_freq_hz() -> u64 {
  let q32 = crate::arch::nanos_per_tick_q32();
  if q32 == 0 {
    return 0;
  }
  // q32 = (1e9 << 32) / freq_hz  =>  freq_hz = (1e9 << 32) / q32
  let numerator: u128 = 1_000_000_000u128 << 32;
  u64::try_from(numerator / u128::from(q32)).unwrap_or(0)
}

/// Whether CPUID leaf 15h was usable for TSC frequency on this host. Always
/// `false` on non-x86 targets and on x86 macOS/Windows (which use OS APIs).
pub fn tach_used_cpuid_15h() -> bool {
  #[cfg(all(target_arch = "x86_64", not(any(target_os = "macos", target_os = "windows"))))]
  {
    crate::arch::x86_64::cpuid_tsc_hz().is_some()
  }
  #[cfg(all(target_arch = "x86", not(any(target_os = "macos", target_os = "windows"))))]
  {
    crate::arch::x86::cpuid_tsc_hz().is_some()
  }
  #[cfg(not(all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(any(target_os = "macos", target_os = "windows")),
  )))]
  {
    false
  }
}

// ── ClockSource impls ───────────────────────────────────────────────────────

pub struct TachInstant;
static TACH_ANCHOR: OnceLock<TachInstantTy> = OnceLock::new();
impl ClockSource for TachInstant {
  const NAME: &'static str = "tach";
  fn init_anchor() {
    let _ = TACH_ANCHOR.get_or_init(TachInstantTy::now);
  }
  fn now_as_u64() -> u64 {
    let anchor = *TACH_ANCHOR.get().expect("init_anchor first");
    u64::try_from(TachInstantTy::now().saturating_duration_since(anchor).as_nanos())
      .unwrap_or(u64::MAX)
  }
}

pub struct TachFencedInstant;
static TACH_FENCED_ANCHOR: OnceLock<TachFencedInstantTy> = OnceLock::new();
impl ClockSource for TachFencedInstant {
  const NAME: &'static str = "tach_fenced";
  fn init_anchor() {
    let _ = TACH_FENCED_ANCHOR.get_or_init(TachFencedInstantTy::now);
  }
  fn now_as_u64() -> u64 {
    let anchor = *TACH_FENCED_ANCHOR.get().expect("init_anchor first");
    u64::try_from(TachFencedInstantTy::now().saturating_duration_since(anchor).as_nanos())
      .unwrap_or(u64::MAX)
  }
}

pub struct TachSyncedInstant;
static TACH_SYNC_ANCHOR: OnceLock<TachSyncedInstantTy> = OnceLock::new();
impl ClockSource for TachSyncedInstant {
  const NAME: &'static str = "tach_synced";
  fn init_anchor() {
    let _ = TACH_SYNC_ANCHOR.get_or_init(TachSyncedInstantTy::now);
  }
  fn now_as_u64() -> u64 {
    let anchor = *TACH_SYNC_ANCHOR.get().expect("init_anchor first");
    u64::try_from(TachSyncedInstantTy::now().saturating_duration_since(anchor).as_nanos())
      .unwrap_or(u64::MAX)
  }
}

/// Same underlying clock as `TachInstant`, but only useful when the binary
/// was built with `--features recalibrate-background`. The recal thread runs
/// the same NANOS_PER_TICK_Q32 cache — so this row's measurements only differ
/// from `TachInstant`'s if the recal thread is active. Always emitted as a
/// separate row; the orchestration script's job is to ensure the build is
/// correct.
pub struct TachInstantRecal;
static TACH_RECAL_ANCHOR: OnceLock<TachInstantTy> = OnceLock::new();
impl ClockSource for TachInstantRecal {
  const NAME: &'static str = "tach_recal";
  fn init_anchor() {
    let _ = TACH_RECAL_ANCHOR.get_or_init(|| {
      // If the recalibrate-background feature is on, calling now() then
      // elapsed() lazily spawns the thread.
      let a = TachInstantTy::now();
      let _ = a.elapsed();
      a
    });
  }
  fn now_as_u64() -> u64 {
    let anchor = *TACH_RECAL_ANCHOR.get().expect("init_anchor first");
    u64::try_from(TachInstantTy::now().saturating_duration_since(anchor).as_nanos())
      .unwrap_or(u64::MAX)
  }
}

pub struct StdInstant;
static STD_ANCHOR: OnceLock<StdInstantTy> = OnceLock::new();
impl ClockSource for StdInstant {
  const NAME: &'static str = "std";
  fn init_anchor() {
    let _ = STD_ANCHOR.get_or_init(StdInstantTy::now);
  }
  fn now_as_u64() -> u64 {
    let anchor: StdInstantTy = *STD_ANCHOR.get().expect("init_anchor first");
    u64::try_from(StdInstantTy::now().duration_since(anchor).as_nanos()).unwrap_or(u64::MAX)
  }
}

pub struct QuantaInstant;
static QUANTA_ANCHOR: OnceLock<quanta::Instant> = OnceLock::new();
impl ClockSource for QuantaInstant {
  const NAME: &'static str = "quanta";
  fn init_anchor() {
    let _ = QUANTA_ANCHOR.get_or_init(quanta::Instant::now);
  }
  fn now_as_u64() -> u64 {
    let anchor = *QUANTA_ANCHOR.get().expect("init_anchor first");
    u64::try_from(quanta::Instant::now().duration_since(anchor).as_nanos()).unwrap_or(u64::MAX)
  }
}

pub struct MinstantInstant;
static MINSTANT_ANCHOR: OnceLock<minstant::Instant> = OnceLock::new();
impl ClockSource for MinstantInstant {
  const NAME: &'static str = "minstant";
  fn init_anchor() {
    let _ = MINSTANT_ANCHOR.get_or_init(minstant::Instant::now);
  }
  fn now_as_u64() -> u64 {
    let anchor = *MINSTANT_ANCHOR.get().expect("init_anchor first");
    u64::try_from(minstant::Instant::now().duration_since(anchor).as_nanos()).unwrap_or(u64::MAX)
  }
  fn backed_by_arch_counter() -> bool {
    minstant::is_tsc_available()
  }
}

pub struct FastantInstant;
static FASTANT_ANCHOR: OnceLock<fastant::Instant> = OnceLock::new();
impl ClockSource for FastantInstant {
  const NAME: &'static str = "fastant";
  fn init_anchor() {
    let _ = FASTANT_ANCHOR.get_or_init(fastant::Instant::now);
  }
  fn now_as_u64() -> u64 {
    let anchor = *FASTANT_ANCHOR.get().expect("init_anchor first");
    u64::try_from(fastant::Instant::now().duration_since(anchor).as_nanos()).unwrap_or(u64::MAX)
  }
  fn backed_by_arch_counter() -> bool {
    // fastant uses TSC on Linux x86_64 + macOS aarch64, falls back to std
    // elsewhere. Approximate the answer via target_arch detection — same as
    // the crate does internally.
    cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
  }
}
