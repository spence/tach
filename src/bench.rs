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
use crate::OrderedInstant as TachOrderedInstantTy;

#[cfg(target_os = "windows")]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn windows_thread_cpu_wall_fallback_now() -> crate::ThreadCpuInstant {
  crate::ThreadCpuInstant::bench_windows_wall_fallback_now()
}

#[cfg(all(
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
      ),
    ),
    all(target_os = "android", any(target_arch = "x86_64", target_arch = "aarch64"),),
  ),
))]
#[doc(hidden)]
pub struct ThreadCpuPerfHandle(crate::arch::thread_cpu::BenchPerfHandle);

#[cfg(all(
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
      ),
    ),
    all(target_os = "android", any(target_arch = "x86_64", target_arch = "aarch64"),),
  ),
))]
impl ThreadCpuPerfHandle {
  #[doc(hidden)]
  pub fn try_for_current_thread() -> Option<Self> {
    crate::arch::thread_cpu::bench_perf_handle().map(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_nanos(&self) -> u64 {
    self.0.now_nanos()
  }

  #[doc(hidden)]
  pub fn candidate_count(&self) -> usize {
    self.0.candidate_count()
  }

  #[doc(hidden)]
  pub fn candidate_name(&self, index: usize) -> Option<&'static str> {
    self.0.candidate_name(index)
  }

  #[doc(hidden)]
  pub fn selected_candidate_name(&self) -> &'static str {
    self.0.selected_candidate_name()
  }

  #[doc(hidden)]
  pub fn select_candidate(&self, index: usize) -> bool {
    self.0.select_candidate(index)
  }
}

#[cfg(all(
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
      ),
    ),
    all(target_os = "android", any(target_arch = "x86_64", target_arch = "aarch64"),),
  ),
))]
#[doc(hidden)]
pub fn thread_cpu_perf_selection_measurements() -> Option<([u64; 9], [u64; 9], usize)> {
  crate::arch::thread_cpu::bench_perf_selection_measurements()
}

#[cfg(all(
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
))]
#[doc(hidden)]
#[derive(Clone, Debug, Serialize)]
pub struct ThreadCpuPerfPathEvidence {
  pub mmap_batches_ns: Option<[u64; 9]>,
  pub read_batches_ns: [u64; 9],
  pub posix_batches_ns: [u64; 9],
  pub selected_path: &'static str,
  pub fallback_path: &'static str,
  pub reads_per_batch: usize,
  pub required_decisive_wins: usize,
}

#[cfg(all(
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
))]
#[doc(hidden)]
pub fn thread_cpu_perf_path_evidence() -> Option<ThreadCpuPerfPathEvidence> {
  let evidence = crate::arch::thread_cpu::bench_perf_path_evidence()?;
  Some(ThreadCpuPerfPathEvidence {
    mmap_batches_ns: evidence.mmap_batches_ns,
    read_batches_ns: evidence.read_batches_ns,
    posix_batches_ns: evidence.posix_batches_ns,
    selected_path: evidence.selected_path,
    fallback_path: evidence.fallback_path,
    reads_per_batch: evidence.reads_per_batch,
    required_decisive_wins: evidence.required_decisive_wins,
  })
}

#[cfg(all(
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
))]
#[doc(hidden)]
#[derive(Clone, Debug, Serialize)]
pub struct ThreadCpuPerfReadEntryEvidence {
  pub candidate_count: usize,
  pub candidate_names: [&'static str; 3],
  pub candidate_eligible: [bool; 3],
  pub candidate_measured: [bool; 3],
  pub candidate_batches_ns: [[u64; 9]; 3],
  pub selected_candidate: &'static str,
  pub reads_per_batch: usize,
  pub required_decisive_wins: usize,
}

#[cfg(all(
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
))]
#[doc(hidden)]
pub fn thread_cpu_perf_read_entry_evidence() -> Option<ThreadCpuPerfReadEntryEvidence> {
  let evidence = crate::arch::thread_cpu::bench_perf_read_entry_evidence()?;
  Some(ThreadCpuPerfReadEntryEvidence {
    candidate_count: evidence.candidate_count,
    candidate_names: evidence.candidate_names,
    candidate_eligible: evidence.candidate_eligible,
    candidate_measured: evidence.candidate_measured,
    candidate_batches_ns: evidence.candidate_batches_ns,
    selected_candidate: evidence.selected_candidate,
    reads_per_batch: evidence.reads_per_batch,
    required_decisive_wins: evidence.required_decisive_wins,
  })
}

#[cfg(all(
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
))]
#[doc(hidden)]
pub struct ThreadCpuPerfReadHandle(crate::arch::thread_cpu::BenchPerfReadHandle);

#[cfg(all(
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
))]
impl ThreadCpuPerfReadHandle {
  #[doc(hidden)]
  pub fn try_for_current_thread() -> Option<Self> {
    crate::arch::thread_cpu::bench_perf_read_handle().map(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_nanos(&self) -> u64 {
    self.0.now_nanos()
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn direct_nanos(&self) -> u64 {
    self.0.direct_nanos()
  }

  #[doc(hidden)]
  pub fn candidate_count(&self) -> usize {
    self.0.candidate_count()
  }

  #[doc(hidden)]
  pub fn candidate_name(&self, index: usize) -> Option<&'static str> {
    self.0.candidate_name(index)
  }

  #[doc(hidden)]
  pub fn selected_candidate_name(&self) -> &'static str {
    self.0.selected_candidate_name()
  }

  #[doc(hidden)]
  pub fn select_candidate(&self, index: usize) -> bool {
    self.0.select_candidate(index)
  }
}

#[cfg(all(
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
))]
#[doc(hidden)]
pub struct ThreadCpuPerfPathHandle(crate::arch::thread_cpu::BenchPerfPathHandle);

#[cfg(all(
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
))]
impl ThreadCpuPerfPathHandle {
  #[doc(hidden)]
  pub fn try_for_current_thread() -> Option<Self> {
    crate::arch::thread_cpu::bench_perf_path_handle().map(Self)
  }

  #[doc(hidden)]
  pub const fn candidate_count(&self) -> usize {
    self.0.candidate_count()
  }

  #[doc(hidden)]
  pub fn candidate_name(&self, index: usize) -> Option<&'static str> {
    self.0.candidate_name(index)
  }

  #[doc(hidden)]
  pub fn candidate_available(&self, index: usize) -> bool {
    self.0.candidate_available(index)
  }

  #[doc(hidden)]
  pub fn select_candidate(&self, index: usize) -> bool {
    self.0.select_candidate(index)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_nanos(&self) -> u64 {
    self.0.now_nanos()
  }
}

#[cfg(all(
  feature = "thread-cpu-inline",
  any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"),
  any(
    target_os = "linux",
    all(target_os = "android", any(target_arch = "x86_64", target_arch = "aarch64"),),
  ),
))]
#[doc(hidden)]
#[derive(Clone, Debug, Serialize)]
pub struct ThreadCpuPerfCounterEvidence {
  pub candidate_count: usize,
  pub candidate_names: [&'static str; 5],
  pub candidate_eligible: [bool; 5],
  pub candidate_batches_ns: [[u64; 9]; 5],
  pub selected_candidate: &'static str,
  pub reads_per_batch: usize,
  pub required_decisive_wins: usize,
}

#[cfg(all(
  feature = "thread-cpu-inline",
  any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"),
  any(
    target_os = "linux",
    all(target_os = "android", any(target_arch = "x86_64", target_arch = "aarch64"),),
  ),
))]
#[doc(hidden)]
pub fn thread_cpu_perf_counter_evidence() -> Option<ThreadCpuPerfCounterEvidence> {
  let evidence = crate::arch::thread_cpu::bench_perf_counter_evidence()?;
  Some(ThreadCpuPerfCounterEvidence {
    candidate_count: evidence.candidate_count,
    candidate_names: evidence.candidate_names,
    candidate_eligible: evidence.candidate_eligible,
    candidate_batches_ns: evidence.candidate_batches_ns,
    selected_candidate: evidence.selected_candidate,
    reads_per_batch: evidence.reads_per_batch,
    required_decisive_wins: evidence.required_decisive_wins,
  })
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[doc(hidden)]
#[derive(Clone, Debug, Serialize)]
pub struct WasmWallSelectionEvidence {
  pub local_provider: &'static str,
  pub ordered_provider: &'static str,
  pub performance_median_ns: u64,
  pub hrtime_median_ns: u64,
  pub performance_batches_ns: [u64; 9],
  pub hrtime_batches_ns: [u64; 9],
  pub allowance_ns: u64,
  pub hrtime_decisive_wins: u32,
  pub ordered_performance_median_ns: u64,
  pub ordered_hrtime_median_ns: u64,
  pub ordered_performance_batches_ns: [u64; 9],
  pub ordered_hrtime_batches_ns: [u64; 9],
  pub ordered_allowance_ns: u64,
  pub ordered_hrtime_decisive_wins: u32,
  pub reads_per_batch: usize,
  pub required_decisive_wins: usize,
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[doc(hidden)]
pub fn wasm_wall_selection_evidence() -> WasmWallSelectionEvidence {
  let evidence = crate::arch::wasm::bench_selection_evidence();
  WasmWallSelectionEvidence {
    local_provider: wasm_provider_label(evidence.local_provider),
    ordered_provider: wasm_provider_label(evidence.ordered_provider),
    performance_median_ns: evidence.performance_median_ns,
    hrtime_median_ns: evidence.hrtime_median_ns,
    performance_batches_ns: evidence.performance_batches_ns,
    hrtime_batches_ns: evidence.hrtime_batches_ns,
    allowance_ns: evidence.allowance_ns,
    hrtime_decisive_wins: evidence.hrtime_decisive_wins,
    ordered_performance_median_ns: evidence.ordered_performance_median_ns,
    ordered_hrtime_median_ns: evidence.ordered_hrtime_median_ns,
    ordered_performance_batches_ns: evidence.ordered_performance_batches_ns,
    ordered_hrtime_batches_ns: evidence.ordered_hrtime_batches_ns,
    ordered_allowance_ns: evidence.ordered_allowance_ns,
    ordered_hrtime_decisive_wins: evidence.ordered_hrtime_decisive_wins,
    reads_per_batch: 4_096,
    required_decisive_wins: 8,
  }
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn wasm_exact_performance_ticks() -> u64 {
  crate::arch::wasm::bench_exact_performance_ticks()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn wasm_exact_hrtime_ticks() -> u64 {
  crate::arch::wasm::bench_exact_hrtime_ticks()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn wasm_exact_ordered_performance_ticks() -> u64 {
  crate::arch::wasm::bench_exact_ordered_performance_ticks()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn wasm_exact_ordered_hrtime_ticks() -> u64 {
  crate::arch::wasm::bench_exact_ordered_hrtime_ticks()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
const fn wasm_provider_label(provider: crate::ThreadCpuProvider) -> &'static str {
  match provider {
    crate::ThreadCpuProvider::PerformanceNow => "performance.now",
    crate::ThreadCpuProvider::NodeHrtime => "process.hrtime.bigint",
    crate::ThreadCpuProvider::Unavailable => "unavailable",
    _ => "other",
  }
}

#[cfg(all(
  target_os = "emscripten",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
#[doc(hidden)]
#[derive(Clone, Debug, Serialize)]
pub struct EmscriptenOrderedSelectionEvidence {
  pub selected_provider: &'static str,
  pub shared_memory: bool,
  pub pthread_build: bool,
  pub performance_epoch_eligible: bool,
  pub emscripten_get_now_eligible: bool,
  pub emscripten_get_now_offset_ns: i64,
  pub performance_epoch_samples_ns: [u64; 9],
  pub emscripten_get_now_samples_ns: [u64; 9],
  pub allowance_ns: u64,
  pub emscripten_get_now_decisive_wins: usize,
  pub reads_per_batch: usize,
  pub required_decisive_wins: usize,
}

#[cfg(all(
  target_os = "emscripten",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
#[doc(hidden)]
pub fn emscripten_ordered_selection_evidence() -> EmscriptenOrderedSelectionEvidence {
  let evidence = crate::arch::emscripten::bench_ordered_selection_evidence();
  EmscriptenOrderedSelectionEvidence {
    selected_provider: evidence.selected_provider,
    shared_memory: evidence.shared_memory,
    pthread_build: evidence.pthread_build,
    performance_epoch_eligible: evidence.epoch_eligible,
    emscripten_get_now_eligible: evidence.get_now_eligible,
    emscripten_get_now_offset_ns: evidence.get_now_offset_ns,
    performance_epoch_samples_ns: evidence.epoch_samples_ns,
    emscripten_get_now_samples_ns: evidence.get_now_samples_ns,
    allowance_ns: evidence.allowance_ns,
    emscripten_get_now_decisive_wins: evidence.get_now_decisive_wins,
    reads_per_batch: 4_096,
    required_decisive_wins: 8,
  }
}

#[cfg(target_os = "emscripten")]
#[doc(hidden)]
#[derive(Clone, Debug, Serialize)]
pub struct EmscriptenLocalSelectionEvidence {
  pub selected_provider: &'static str,
  pub performance_eligible: bool,
  pub hrtime_eligible: bool,
  pub performance_samples_ns: [u64; 9],
  pub hrtime_samples_ns: [u64; 9],
  pub allowance_ns: u64,
  pub hrtime_decisive_wins: usize,
  pub reads_per_batch: usize,
  pub required_decisive_wins: usize,
}

#[cfg(target_os = "emscripten")]
#[doc(hidden)]
pub fn emscripten_local_selection_evidence() -> EmscriptenLocalSelectionEvidence {
  let evidence = crate::arch::emscripten::bench_local_selection_evidence();
  EmscriptenLocalSelectionEvidence {
    selected_provider: evidence.selected_provider,
    performance_eligible: evidence.performance_eligible,
    hrtime_eligible: evidence.hrtime_eligible,
    performance_samples_ns: evidence.performance_samples_ns,
    hrtime_samples_ns: evidence.hrtime_samples_ns,
    allowance_ns: evidence.allowance_ns,
    hrtime_decisive_wins: evidence.hrtime_decisive_wins,
    reads_per_batch: 4_096,
    required_decisive_wins: 8,
  }
}

#[cfg(target_os = "emscripten")]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn emscripten_exact_performance_ticks() -> u64 {
  crate::arch::emscripten::bench_exact_performance_ticks()
}

#[cfg(target_os = "emscripten")]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn emscripten_exact_hrtime_ticks() -> u64 {
  crate::arch::emscripten::bench_exact_hrtime_ticks()
}

#[cfg(all(
  target_os = "emscripten",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn emscripten_exact_ordered_performance_epoch_ticks() -> u64 {
  crate::arch::emscripten::bench_exact_ordered_epoch_ticks()
}

#[cfg(all(
  target_os = "emscripten",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn emscripten_exact_ordered_aligned_get_now_ticks() -> u64 {
  crate::arch::emscripten::bench_exact_ordered_aligned_get_now_ticks()
}

#[cfg(target_os = "wasi")]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn wasi_exact_wall_ticks() -> u64 {
  crate::arch::fallback::wasi_clock_monotonic()
}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn wasi_exact_thread_cpu_nanos() -> Option<u64> {
  crate::arch::thread_cpu::bench_wasi_thread_cpu_nanos()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn wasm_exact_node_thread_cpu_nanos() -> Option<u64> {
  crate::arch::wasm::node_thread_cpu_usage_micros().and_then(|value| value.checked_mul(1_000))
}

#[cfg(target_os = "emscripten")]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn emscripten_exact_node_thread_cpu_nanos() -> Option<u64> {
  crate::arch::emscripten::node_thread_cpu_usage_micros().and_then(|value| value.checked_mul(1_000))
}

/// Metadata for an exact wall-clock provider selected outside a benchmark loop.
///
/// The benchmark harness matches this provider name before it enters the timed
/// closure, so the closure calls a statically named reader rather than this
/// descriptor through an indirect function pointer.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct ExactWallProvider {
  provider: &'static str,
  nanos_per_tick_q32: u64,
}

impl ExactWallProvider {
  #[allow(dead_code)] // Selector-backed targets construct this in cfg-specific factories.
  fn new(provider: &'static str, nanos_per_tick_q32: u64) -> Self {
    Self { provider, nanos_per_tick_q32 }
  }

  #[doc(hidden)]
  pub fn provider(&self) -> &'static str {
    self.provider
  }

  #[doc(hidden)]
  #[doc(hidden)]
  pub fn nanos_per_tick_q32(&self) -> u64 {
    self.nanos_per_tick_q32
  }
}

#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn exact_ticks_to_duration_with_scale(ticks: u64, nanos_per_tick_q32: u64) -> Duration {
  crate::instant::ticks_to_duration_with_scale(ticks, nanos_per_tick_q32)
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_aarch64_selected_instant_primitive() -> ExactWallProvider {
  let primitive = crate::arch::apple_aarch64::bench_selected_instant_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_aarch64_selected_ordered_primitive() -> ExactWallProvider {
  let primitive = crate::arch::apple_aarch64::bench_selected_ordered_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_aarch64_instant_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::apple_aarch64::bench_instant_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive =
        primitive.expect("eligible Apple Instant candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_aarch64_ordered_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::apple_aarch64::bench_ordered_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive =
        primitive.expect("eligible Apple Ordered candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
macro_rules! expose_apple_aarch64_exact_read {
  ($name:ident, $source:ident) => {
    #[doc(hidden)]
    #[inline(always)]
    #[allow(clippy::inline_always)]
    pub fn $name() -> u64 {
      crate::arch::apple_aarch64::$source()
    }
  };
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
expose_apple_aarch64_exact_read!(apple_aarch64_exact_mach_absolute, bench_exact_mach_absolute);
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
expose_apple_aarch64_exact_read!(apple_aarch64_exact_mach_continuous, bench_exact_mach_continuous);
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
expose_apple_aarch64_exact_read!(apple_aarch64_exact_cntvct_absolute, bench_exact_cntvct_absolute);
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
expose_apple_aarch64_exact_read!(
  apple_aarch64_exact_cntvct_ordered_absolute,
  bench_exact_cntvct_ordered_absolute
);
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
expose_apple_aarch64_exact_read!(
  apple_aarch64_exact_cntvctss_absolute,
  bench_exact_cntvctss_absolute
);
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
expose_apple_aarch64_exact_read!(
  apple_aarch64_exact_acntvct_absolute,
  bench_exact_acntvct_absolute
);
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
expose_apple_aarch64_exact_read!(
  apple_aarch64_exact_cntvct_continuous,
  bench_exact_cntvct_continuous
);
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
expose_apple_aarch64_exact_read!(
  apple_aarch64_exact_cntvct_ordered_continuous,
  bench_exact_cntvct_ordered_continuous
);
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
expose_apple_aarch64_exact_read!(
  apple_aarch64_exact_cntvctss_continuous,
  bench_exact_cntvctss_continuous
);
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
expose_apple_aarch64_exact_read!(
  apple_aarch64_exact_acntvct_continuous,
  bench_exact_acntvct_continuous
);

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[doc(hidden)]
pub fn linux_x86_selected_instant_primitive() -> ExactWallProvider {
  let primitive = crate::arch::linux_x86_wall::bench_selected_instant_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[doc(hidden)]
pub fn linux_x86_selected_ordered_primitive() -> ExactWallProvider {
  let primitive = crate::arch::linux_x86_wall::bench_selected_ordered_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[doc(hidden)]
pub fn linux_x86_instant_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::linux_x86_wall::bench_instant_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive = primitive.expect("eligible Instant candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[doc(hidden)]
pub fn linux_x86_ordered_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::linux_x86_wall::bench_ordered_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive = primitive.expect("eligible Ordered candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
macro_rules! expose_linux_x86_exact_read {
  ($name:ident, $source:ident) => {
    #[doc(hidden)]
    #[inline(always)]
    #[allow(clippy::inline_always)]
    pub fn $name() -> u64 {
      crate::arch::linux_x86_wall::$source()
    }
  };
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_exact_read!(linux_x86_exact_tsc, bench_direct_tsc);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_exact_read!(linux_x86_exact_libc_monotonic, bench_direct_clock_monotonic);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_exact_read!(linux_x86_exact_libc_raw, bench_direct_clock_monotonic_raw);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_exact_read!(linux_x86_exact_libc_boottime, bench_direct_clock_boottime);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_exact_read!(linux_x86_exact_vdso_monotonic, bench_direct_vdso_monotonic);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_exact_read!(linux_x86_exact_vdso_raw, bench_direct_vdso_monotonic_raw);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_exact_read!(linux_x86_exact_vdso_boottime, bench_direct_vdso_boottime);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_exact_read!(
  linux_x86_exact_vdso_time64_monotonic,
  bench_direct_vdso_time64_monotonic
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_exact_read!(
  linux_x86_exact_vdso_time64_raw,
  bench_direct_vdso_time64_monotonic_raw
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_exact_read!(
  linux_x86_exact_vdso_time64_boottime,
  bench_direct_vdso_time64_boottime
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "64",
))]
expose_linux_x86_exact_read!(
  linux_x86_exact_syscall64_monotonic,
  bench_direct_clock_monotonic_syscall
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "64",
))]
expose_linux_x86_exact_read!(
  linux_x86_exact_syscall64_raw,
  bench_direct_clock_monotonic_raw_syscall
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "64",
))]
expose_linux_x86_exact_read!(
  linux_x86_exact_syscall64_boottime,
  bench_direct_clock_boottime_syscall
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_exact_read!(
  linux_x86_exact_time32_monotonic,
  bench_direct_clock_monotonic_time32_syscall
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_exact_read!(
  linux_x86_exact_time32_raw,
  bench_direct_clock_monotonic_raw_time32_syscall
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_exact_read!(
  linux_x86_exact_time32_boottime,
  bench_direct_clock_boottime_time32_syscall
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_exact_read!(
  linux_x86_exact_time64_monotonic,
  bench_direct_clock_monotonic_time64_syscall
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_exact_read!(
  linux_x86_exact_time64_raw,
  bench_direct_clock_monotonic_raw_time64_syscall
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_exact_read!(
  linux_x86_exact_time64_boottime,
  bench_direct_clock_boottime_time64_syscall
);

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_exact_read!(linux_x86_exact_tsc_lfence, bench_direct_tsc_lfence_ordered);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_exact_read!(linux_x86_exact_tsc_mfence, bench_direct_tsc_mfence_ordered);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_exact_read!(linux_x86_exact_tsc_rdtscp, bench_direct_tsc_rdtscp_ordered);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_exact_read!(linux_x86_exact_tsc_cpuid, bench_direct_tsc_cpuid_ordered);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_exact_read!(linux_x86_exact_tsc_serialize, bench_direct_tsc_serialize_ordered);

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
macro_rules! expose_linux_x86_six_exact_reads {
  (
    $public_os_owned:ident: $source_os_owned:ident,
    $public_lfence:ident: $source_lfence:ident,
    $public_rdtscp:ident: $source_rdtscp:ident,
    $public_mfence:ident: $source_mfence:ident,
    $public_cpuid:ident: $source_cpuid:ident,
    $public_serialize:ident: $source_serialize:ident
  ) => {
    expose_linux_x86_exact_read!($public_os_owned, $source_os_owned);
    expose_linux_x86_exact_read!($public_lfence, $source_lfence);
    expose_linux_x86_exact_read!($public_rdtscp, $source_rdtscp);
    expose_linux_x86_exact_read!($public_mfence, $source_mfence);
    expose_linux_x86_exact_read!($public_cpuid, $source_cpuid);
    expose_linux_x86_exact_read!($public_serialize, $source_serialize);
  };
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_libc_monotonic_os_owned: bench_libc_monotonic_os_owned,
  linux_x86_exact_libc_monotonic_lfence: bench_libc_monotonic_lfence,
  linux_x86_exact_libc_monotonic_rdtscp: bench_libc_monotonic_rdtscp,
  linux_x86_exact_libc_monotonic_mfence: bench_libc_monotonic_mfence,
  linux_x86_exact_libc_monotonic_cpuid: bench_libc_monotonic_cpuid,
  linux_x86_exact_libc_monotonic_serialize: bench_libc_monotonic_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_libc_raw_os_owned: bench_libc_raw_os_owned,
  linux_x86_exact_libc_raw_lfence: bench_libc_raw_lfence,
  linux_x86_exact_libc_raw_rdtscp: bench_libc_raw_rdtscp,
  linux_x86_exact_libc_raw_mfence: bench_libc_raw_mfence,
  linux_x86_exact_libc_raw_cpuid: bench_libc_raw_cpuid,
  linux_x86_exact_libc_raw_serialize: bench_libc_raw_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_libc_boottime_os_owned: bench_libc_boottime_os_owned,
  linux_x86_exact_libc_boottime_lfence: bench_libc_boottime_lfence,
  linux_x86_exact_libc_boottime_rdtscp: bench_libc_boottime_rdtscp,
  linux_x86_exact_libc_boottime_mfence: bench_libc_boottime_mfence,
  linux_x86_exact_libc_boottime_cpuid: bench_libc_boottime_cpuid,
  linux_x86_exact_libc_boottime_serialize: bench_libc_boottime_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_vdso_monotonic_os_owned: bench_vdso_monotonic_os_owned,
  linux_x86_exact_vdso_monotonic_lfence: bench_vdso_monotonic_lfence,
  linux_x86_exact_vdso_monotonic_rdtscp: bench_vdso_monotonic_rdtscp,
  linux_x86_exact_vdso_monotonic_mfence: bench_vdso_monotonic_mfence,
  linux_x86_exact_vdso_monotonic_cpuid: bench_vdso_monotonic_cpuid,
  linux_x86_exact_vdso_monotonic_serialize: bench_vdso_monotonic_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_vdso_raw_os_owned: bench_vdso_raw_os_owned,
  linux_x86_exact_vdso_raw_lfence: bench_vdso_raw_lfence,
  linux_x86_exact_vdso_raw_rdtscp: bench_vdso_raw_rdtscp,
  linux_x86_exact_vdso_raw_mfence: bench_vdso_raw_mfence,
  linux_x86_exact_vdso_raw_cpuid: bench_vdso_raw_cpuid,
  linux_x86_exact_vdso_raw_serialize: bench_vdso_raw_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_vdso_boottime_os_owned: bench_vdso_boottime_os_owned,
  linux_x86_exact_vdso_boottime_lfence: bench_vdso_boottime_lfence,
  linux_x86_exact_vdso_boottime_rdtscp: bench_vdso_boottime_rdtscp,
  linux_x86_exact_vdso_boottime_mfence: bench_vdso_boottime_mfence,
  linux_x86_exact_vdso_boottime_cpuid: bench_vdso_boottime_cpuid,
  linux_x86_exact_vdso_boottime_serialize: bench_vdso_boottime_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "64",
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_syscall64_monotonic_os_owned: bench_syscall64_monotonic_os_owned,
  linux_x86_exact_syscall64_monotonic_lfence: bench_syscall64_monotonic_lfence,
  linux_x86_exact_syscall64_monotonic_rdtscp: bench_syscall64_monotonic_rdtscp,
  linux_x86_exact_syscall64_monotonic_mfence: bench_syscall64_monotonic_mfence,
  linux_x86_exact_syscall64_monotonic_cpuid: bench_syscall64_monotonic_cpuid,
  linux_x86_exact_syscall64_monotonic_serialize: bench_syscall64_monotonic_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "64",
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_syscall64_raw_os_owned: bench_syscall64_raw_os_owned,
  linux_x86_exact_syscall64_raw_lfence: bench_syscall64_raw_lfence,
  linux_x86_exact_syscall64_raw_rdtscp: bench_syscall64_raw_rdtscp,
  linux_x86_exact_syscall64_raw_mfence: bench_syscall64_raw_mfence,
  linux_x86_exact_syscall64_raw_cpuid: bench_syscall64_raw_cpuid,
  linux_x86_exact_syscall64_raw_serialize: bench_syscall64_raw_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "64",
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_syscall64_boottime_os_owned: bench_syscall64_boottime_os_owned,
  linux_x86_exact_syscall64_boottime_lfence: bench_syscall64_boottime_lfence,
  linux_x86_exact_syscall64_boottime_rdtscp: bench_syscall64_boottime_rdtscp,
  linux_x86_exact_syscall64_boottime_mfence: bench_syscall64_boottime_mfence,
  linux_x86_exact_syscall64_boottime_cpuid: bench_syscall64_boottime_cpuid,
  linux_x86_exact_syscall64_boottime_serialize: bench_syscall64_boottime_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_time32_monotonic_os_owned: bench_time32_monotonic_os_owned,
  linux_x86_exact_time32_monotonic_lfence: bench_time32_monotonic_lfence,
  linux_x86_exact_time32_monotonic_rdtscp: bench_time32_monotonic_rdtscp,
  linux_x86_exact_time32_monotonic_mfence: bench_time32_monotonic_mfence,
  linux_x86_exact_time32_monotonic_cpuid: bench_time32_monotonic_cpuid,
  linux_x86_exact_time32_monotonic_serialize: bench_time32_monotonic_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_time32_raw_os_owned: bench_time32_raw_os_owned,
  linux_x86_exact_time32_raw_lfence: bench_time32_raw_lfence,
  linux_x86_exact_time32_raw_rdtscp: bench_time32_raw_rdtscp,
  linux_x86_exact_time32_raw_mfence: bench_time32_raw_mfence,
  linux_x86_exact_time32_raw_cpuid: bench_time32_raw_cpuid,
  linux_x86_exact_time32_raw_serialize: bench_time32_raw_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_time32_boottime_os_owned: bench_time32_boottime_os_owned,
  linux_x86_exact_time32_boottime_lfence: bench_time32_boottime_lfence,
  linux_x86_exact_time32_boottime_rdtscp: bench_time32_boottime_rdtscp,
  linux_x86_exact_time32_boottime_mfence: bench_time32_boottime_mfence,
  linux_x86_exact_time32_boottime_cpuid: bench_time32_boottime_cpuid,
  linux_x86_exact_time32_boottime_serialize: bench_time32_boottime_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_time64_monotonic_os_owned: bench_time64_monotonic_os_owned,
  linux_x86_exact_time64_monotonic_lfence: bench_time64_monotonic_lfence,
  linux_x86_exact_time64_monotonic_rdtscp: bench_time64_monotonic_rdtscp,
  linux_x86_exact_time64_monotonic_mfence: bench_time64_monotonic_mfence,
  linux_x86_exact_time64_monotonic_cpuid: bench_time64_monotonic_cpuid,
  linux_x86_exact_time64_monotonic_serialize: bench_time64_monotonic_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_time64_raw_os_owned: bench_time64_raw_os_owned,
  linux_x86_exact_time64_raw_lfence: bench_time64_raw_lfence,
  linux_x86_exact_time64_raw_rdtscp: bench_time64_raw_rdtscp,
  linux_x86_exact_time64_raw_mfence: bench_time64_raw_mfence,
  linux_x86_exact_time64_raw_cpuid: bench_time64_raw_cpuid,
  linux_x86_exact_time64_raw_serialize: bench_time64_raw_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_time64_boottime_os_owned: bench_time64_boottime_os_owned,
  linux_x86_exact_time64_boottime_lfence: bench_time64_boottime_lfence,
  linux_x86_exact_time64_boottime_rdtscp: bench_time64_boottime_rdtscp,
  linux_x86_exact_time64_boottime_mfence: bench_time64_boottime_mfence,
  linux_x86_exact_time64_boottime_cpuid: bench_time64_boottime_cpuid,
  linux_x86_exact_time64_boottime_serialize: bench_time64_boottime_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_vdso_time64_monotonic_os_owned: bench_vdso_time64_monotonic_os_owned,
  linux_x86_exact_vdso_time64_monotonic_lfence: bench_vdso_time64_monotonic_lfence,
  linux_x86_exact_vdso_time64_monotonic_rdtscp: bench_vdso_time64_monotonic_rdtscp,
  linux_x86_exact_vdso_time64_monotonic_mfence: bench_vdso_time64_monotonic_mfence,
  linux_x86_exact_vdso_time64_monotonic_cpuid: bench_vdso_time64_monotonic_cpuid,
  linux_x86_exact_vdso_time64_monotonic_serialize: bench_vdso_time64_monotonic_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_vdso_time64_raw_os_owned: bench_vdso_time64_raw_os_owned,
  linux_x86_exact_vdso_time64_raw_lfence: bench_vdso_time64_raw_lfence,
  linux_x86_exact_vdso_time64_raw_rdtscp: bench_vdso_time64_raw_rdtscp,
  linux_x86_exact_vdso_time64_raw_mfence: bench_vdso_time64_raw_mfence,
  linux_x86_exact_vdso_time64_raw_cpuid: bench_vdso_time64_raw_cpuid,
  linux_x86_exact_vdso_time64_raw_serialize: bench_vdso_time64_raw_serialize
);
#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
  target_pointer_width = "32",
))]
expose_linux_x86_six_exact_reads!(
  linux_x86_exact_vdso_time64_boottime_os_owned: bench_vdso_time64_boottime_os_owned,
  linux_x86_exact_vdso_time64_boottime_lfence: bench_vdso_time64_boottime_lfence,
  linux_x86_exact_vdso_time64_boottime_rdtscp: bench_vdso_time64_boottime_rdtscp,
  linux_x86_exact_vdso_time64_boottime_mfence: bench_vdso_time64_boottime_mfence,
  linux_x86_exact_vdso_time64_boottime_cpuid: bench_vdso_time64_boottime_cpuid,
  linux_x86_exact_vdso_time64_boottime_serialize: bench_vdso_time64_boottime_serialize
);

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
#[doc(hidden)]
pub fn linux_aarch64_selected_instant_primitive() -> ExactWallProvider {
  let primitive = crate::arch::linux_aarch64_wall::bench_selected_instant_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
#[doc(hidden)]
pub fn linux_aarch64_selected_ordered_primitive() -> ExactWallProvider {
  let primitive = crate::arch::linux_aarch64_wall::bench_selected_ordered_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
#[doc(hidden)]
pub fn linux_aarch64_instant_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::linux_aarch64_wall::bench_instant_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive = primitive.expect("eligible Instant candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
#[doc(hidden)]
pub fn linux_aarch64_ordered_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::linux_aarch64_wall::bench_ordered_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive = primitive.expect("eligible Ordered candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
macro_rules! expose_linux_aarch64_exact_read {
  ($name:ident, $source:ident) => {
    #[doc(hidden)]
    #[inline(always)]
    #[allow(clippy::inline_always)]
    pub fn $name() -> u64 {
      crate::arch::linux_aarch64_wall::$source()
    }
  };
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(linux_aarch64_exact_cntvct, bench_direct_cntvct);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(linux_aarch64_exact_libc_monotonic, bench_direct_clock_monotonic);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(linux_aarch64_exact_libc_raw, bench_direct_clock_monotonic_raw);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(linux_aarch64_exact_libc_boottime, bench_direct_clock_boottime);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(
  linux_aarch64_exact_vdso_monotonic,
  bench_direct_clock_monotonic_vdso
);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(
  linux_aarch64_exact_vdso_raw,
  bench_direct_clock_monotonic_raw_vdso
);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(
  linux_aarch64_exact_vdso_boottime,
  bench_direct_clock_boottime_vdso
);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(
  linux_aarch64_exact_syscall_monotonic,
  bench_direct_clock_monotonic_syscall
);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(
  linux_aarch64_exact_syscall_raw,
  bench_direct_clock_monotonic_raw_syscall
);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(
  linux_aarch64_exact_syscall_boottime,
  bench_direct_clock_boottime_syscall
);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(linux_aarch64_exact_isb_cntvct, bench_direct_isb_cntvct);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(linux_aarch64_exact_cntvctss, bench_direct_cntvctss);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(
  linux_aarch64_exact_ordered_libc_monotonic,
  bench_direct_clock_monotonic_ordered
);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(
  linux_aarch64_exact_ordered_libc_raw,
  bench_direct_clock_monotonic_raw_ordered
);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(
  linux_aarch64_exact_ordered_libc_boottime,
  bench_direct_clock_boottime_ordered
);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(
  linux_aarch64_exact_ordered_vdso_monotonic,
  bench_direct_clock_monotonic_vdso_ordered
);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(
  linux_aarch64_exact_ordered_vdso_raw,
  bench_direct_clock_monotonic_raw_vdso_ordered
);
#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
expose_linux_aarch64_exact_read!(
  linux_aarch64_exact_ordered_vdso_boottime,
  bench_direct_clock_boottime_vdso_ordered
);

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[doc(hidden)]
pub fn residual_selected_instant_primitive() -> ExactWallProvider {
  let primitive = crate::arch::linux_clock_wall::bench_selected_instant_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[doc(hidden)]
pub fn residual_selected_ordered_primitive() -> ExactWallProvider {
  let primitive = crate::arch::linux_clock_wall::bench_selected_ordered_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[doc(hidden)]
pub fn residual_instant_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::linux_clock_wall::bench_instant_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive = primitive.expect("eligible Instant candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[doc(hidden)]
pub fn residual_ordered_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::linux_clock_wall::bench_ordered_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive = primitive.expect("eligible Ordered candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[doc(hidden)]
pub fn residual_selected_instant_primitive() -> ExactWallProvider {
  let primitive = crate::arch::riscv64::bench_selected_instant_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[doc(hidden)]
pub fn residual_selected_ordered_primitive() -> ExactWallProvider {
  let primitive = crate::arch::riscv64::bench_selected_ordered_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[doc(hidden)]
pub fn residual_instant_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::riscv64::bench_instant_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive = primitive.expect("eligible Instant candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[doc(hidden)]
pub fn residual_ordered_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::riscv64::bench_ordered_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive = primitive.expect("eligible Ordered candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
#[doc(hidden)]
pub fn residual_selected_instant_primitive() -> ExactWallProvider {
  let primitive = crate::arch::loongarch64::bench_selected_instant_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
#[doc(hidden)]
pub fn residual_selected_ordered_primitive() -> ExactWallProvider {
  let primitive = crate::arch::loongarch64::bench_selected_ordered_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
#[doc(hidden)]
pub fn residual_instant_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::loongarch64::bench_instant_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive = primitive.expect("eligible Instant candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
#[doc(hidden)]
pub fn residual_ordered_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::loongarch64::bench_ordered_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive = primitive.expect("eligible Ordered candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[doc(hidden)]
pub fn residual_selected_instant_primitive() -> ExactWallProvider {
  let primitive = crate::arch::powerpc64::bench_selected_instant_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[doc(hidden)]
pub fn residual_selected_ordered_primitive() -> ExactWallProvider {
  let primitive = crate::arch::powerpc64::bench_selected_ordered_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[doc(hidden)]
pub fn residual_instant_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::powerpc64::bench_instant_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32))
    .collect()
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[doc(hidden)]
pub fn residual_ordered_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::powerpc64::bench_ordered_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32))
    .collect()
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[doc(hidden)]
pub fn residual_selected_instant_primitive() -> ExactWallProvider {
  let primitive = crate::arch::freebsd_x86_64::bench_selected_instant_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[doc(hidden)]
pub fn residual_selected_ordered_primitive() -> ExactWallProvider {
  let primitive = crate::arch::freebsd_x86_64::bench_selected_ordered_primitive();
  ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[doc(hidden)]
pub fn residual_instant_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::freebsd_x86_64::bench_instant_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive = primitive.expect("eligible Instant candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[doc(hidden)]
pub fn residual_ordered_candidate_primitives() -> Vec<ExactWallProvider> {
  let (primitives, count) = crate::arch::freebsd_x86_64::bench_ordered_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      let primitive = primitive.expect("eligible Ordered candidate must have an exact reader");
      ExactWallProvider::new(primitive.name, primitive.nanos_per_tick_q32)
    })
    .collect()
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
macro_rules! expose_linux_clock_exact_read {
  ($name:ident, $source:ident) => {
    #[doc(hidden)]
    #[inline(always)]
    #[allow(clippy::inline_always)]
    pub fn $name() -> u64 {
      crate::arch::linux_clock_wall::$source()
    }
  };
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(residual_exact_clock_monotonic, bench_exact_libc_monotonic);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(residual_exact_arm_cntvct, bench_exact_arm_cntvct);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(residual_exact_clock_monotonic_syscall, bench_exact_raw_monotonic);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_clock_monotonic_time64_syscall,
  bench_exact_raw_time64_monotonic
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(residual_exact_clock_monotonic_raw, bench_exact_libc_monotonic_raw);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_clock_monotonic_raw_syscall,
  bench_exact_raw_monotonic_raw
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_clock_monotonic_raw_time64_syscall,
  bench_exact_raw_time64_monotonic_raw
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(residual_exact_clock_monotonic_vdso, bench_exact_vdso_monotonic);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_clock_monotonic_raw_vdso,
  bench_exact_vdso_monotonic_raw
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_clock_monotonic_time64_vdso,
  bench_exact_vdso_time64_monotonic
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_clock_monotonic_raw_time64_vdso,
  bench_exact_vdso_time64_monotonic_raw
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(residual_exact_clock_boottime, bench_exact_libc_boottime);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(residual_exact_clock_boottime_syscall, bench_exact_raw_boottime);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_clock_boottime_time64_syscall,
  bench_exact_raw_time64_boottime
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(residual_exact_clock_boottime_vdso, bench_exact_vdso_boottime);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_clock_boottime_time64_vdso,
  bench_exact_vdso_time64_boottime
);

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_monotonic,
  bench_exact_ordered_libc_monotonic
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(residual_exact_ordered_arm_cntvct, bench_exact_ordered_arm_cntvct);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_monotonic_syscall,
  bench_exact_ordered_raw_monotonic
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_os_ordered_clock_monotonic_syscall,
  bench_exact_os_ordered_raw_monotonic
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_monotonic_time64_syscall,
  bench_exact_ordered_raw_time64_monotonic
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_os_ordered_clock_monotonic_time64_syscall,
  bench_exact_os_ordered_raw_time64_monotonic
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_monotonic_raw,
  bench_exact_ordered_libc_monotonic_raw
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_monotonic_raw_syscall,
  bench_exact_ordered_raw_monotonic_raw
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_os_ordered_clock_monotonic_raw_syscall,
  bench_exact_os_ordered_raw_monotonic_raw
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_monotonic_raw_time64_syscall,
  bench_exact_ordered_raw_time64_monotonic_raw
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_os_ordered_clock_monotonic_raw_time64_syscall,
  bench_exact_os_ordered_raw_time64_monotonic_raw
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_monotonic_vdso,
  bench_exact_ordered_vdso_monotonic
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_monotonic_raw_vdso,
  bench_exact_ordered_vdso_monotonic_raw
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_monotonic_time64_vdso,
  bench_exact_ordered_vdso_time64_monotonic
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_monotonic_raw_time64_vdso,
  bench_exact_ordered_vdso_time64_monotonic_raw
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_boottime,
  bench_exact_ordered_libc_boottime
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_boottime_syscall,
  bench_exact_ordered_raw_boottime
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_os_ordered_clock_boottime_syscall,
  bench_exact_os_ordered_raw_boottime
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_boottime_time64_syscall,
  bench_exact_ordered_raw_time64_boottime
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_os_ordered_clock_boottime_time64_syscall,
  bench_exact_os_ordered_raw_time64_boottime
);
#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_boottime_vdso,
  bench_exact_ordered_vdso_boottime
);
#[cfg(all(target_arch = "arm", target_os = "linux"))]
expose_linux_clock_exact_read!(
  residual_exact_ordered_clock_boottime_time64_vdso,
  bench_exact_ordered_vdso_time64_boottime
);

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
macro_rules! expose_riscv_exact_read {
  ($name:ident, $source:ident) => {
    #[doc(hidden)]
    #[inline(always)]
    #[allow(clippy::inline_always)]
    pub fn $name() -> u64 {
      crate::arch::riscv64::$source()
    }
  };
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(residual_exact_riscv_rdtime, bench_exact_rdtime);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(residual_exact_clock_monotonic, bench_exact_clock_monotonic);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_clock_monotonic_syscall,
  bench_exact_clock_monotonic_syscall
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(residual_exact_clock_monotonic_raw, bench_exact_clock_monotonic_raw);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_clock_monotonic_raw_syscall,
  bench_exact_clock_monotonic_raw_syscall
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(residual_exact_clock_monotonic_vdso, bench_exact_clock_monotonic_vdso);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_clock_monotonic_raw_vdso,
  bench_exact_clock_monotonic_raw_vdso
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(residual_exact_clock_boottime, bench_exact_clock_boottime);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(residual_exact_clock_boottime_syscall, bench_exact_clock_boottime_syscall);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(residual_exact_clock_boottime_vdso, bench_exact_clock_boottime_vdso);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(residual_exact_ordered_riscv_rdtime, bench_exact_rdtime_ordered);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_ordered_clock_monotonic,
  bench_exact_clock_monotonic_ordered
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_ordered_clock_monotonic_syscall,
  bench_exact_clock_monotonic_syscall_ordered
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_os_ordered_clock_monotonic_syscall,
  bench_exact_clock_monotonic_syscall_os_ordered
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_ordered_clock_monotonic_raw,
  bench_exact_clock_monotonic_raw_ordered
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_ordered_clock_monotonic_raw_syscall,
  bench_exact_clock_monotonic_raw_syscall_ordered
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_os_ordered_clock_monotonic_raw_syscall,
  bench_exact_clock_monotonic_raw_syscall_os_ordered
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_ordered_clock_monotonic_vdso,
  bench_exact_clock_monotonic_vdso_ordered
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_ordered_clock_monotonic_raw_vdso,
  bench_exact_clock_monotonic_raw_vdso_ordered
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(residual_exact_ordered_clock_boottime, bench_exact_clock_boottime_ordered);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_ordered_clock_boottime_syscall,
  bench_exact_clock_boottime_syscall_ordered
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_os_ordered_clock_boottime_syscall,
  bench_exact_clock_boottime_syscall_os_ordered
);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
expose_riscv_exact_read!(
  residual_exact_ordered_clock_boottime_vdso,
  bench_exact_clock_boottime_vdso_ordered
);

#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
macro_rules! expose_loong_exact_read {
  ($name:ident, $source:ident) => {
    #[doc(hidden)]
    #[inline(always)]
    #[allow(clippy::inline_always)]
    pub fn $name() -> u64 {
      crate::arch::loongarch64::$source()
    }
  };
}

#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(residual_exact_loong_stable_counter, bench_exact_rdtime);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(residual_exact_clock_monotonic, bench_exact_clock_monotonic);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(
  residual_exact_clock_monotonic_syscall,
  bench_exact_clock_monotonic_syscall
);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(residual_exact_clock_monotonic_raw, bench_exact_clock_monotonic_raw);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(
  residual_exact_clock_monotonic_raw_syscall,
  bench_exact_clock_monotonic_raw_syscall
);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(residual_exact_clock_monotonic_vdso, bench_exact_clock_monotonic_vdso);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(
  residual_exact_clock_monotonic_raw_vdso,
  bench_exact_clock_monotonic_raw_vdso
);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(residual_exact_clock_boottime, bench_exact_clock_boottime);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(residual_exact_clock_boottime_syscall, bench_exact_clock_boottime_syscall);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(residual_exact_clock_boottime_vdso, bench_exact_clock_boottime_vdso);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(residual_exact_ordered_loong_stable_counter, bench_exact_rdtime_ordered);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(
  residual_exact_ordered_clock_monotonic,
  bench_exact_clock_monotonic_ordered
);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(
  residual_exact_ordered_clock_monotonic_raw,
  bench_exact_clock_monotonic_raw_ordered
);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(
  residual_exact_ordered_clock_monotonic_vdso,
  bench_exact_clock_monotonic_vdso_ordered
);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(
  residual_exact_ordered_clock_monotonic_raw_vdso,
  bench_exact_clock_monotonic_raw_vdso_ordered
);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(residual_exact_ordered_clock_boottime, bench_exact_clock_boottime_ordered);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(
  residual_exact_ordered_clock_boottime_syscall,
  bench_exact_clock_boottime_syscall_ordered
);
#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
expose_loong_exact_read!(
  residual_exact_ordered_clock_boottime_vdso,
  bench_exact_clock_boottime_vdso_ordered
);

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
macro_rules! expose_power_exact_read {
  ($name:ident, $source:ident) => {
    #[doc(hidden)]
    #[inline(always)]
    #[allow(clippy::inline_always)]
    pub fn $name() -> u64 {
      crate::arch::powerpc64::$source()
    }
  };
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_power_timebase, bench_exact_timebase);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_clock_monotonic, bench_exact_clock_monotonic);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_clock_monotonic_sc, bench_exact_clock_monotonic_sc);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_clock_monotonic_scv, bench_exact_clock_monotonic_scv);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_clock_monotonic_raw, bench_exact_clock_monotonic_raw);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_clock_monotonic_raw_sc, bench_exact_clock_monotonic_raw_sc);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_clock_monotonic_raw_scv,
  bench_exact_clock_monotonic_raw_scv
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_clock_monotonic_vdso, bench_exact_clock_monotonic_vdso);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_clock_monotonic_raw_vdso,
  bench_exact_clock_monotonic_raw_vdso
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_clock_boottime, bench_exact_clock_boottime);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_clock_boottime_sc, bench_exact_clock_boottime_sc);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_clock_boottime_scv, bench_exact_clock_boottime_scv);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_clock_boottime_vdso, bench_exact_clock_boottime_vdso);

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_ordered_power_timebase, bench_exact_timebase_ordered);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_ordered_clock_monotonic,
  bench_exact_clock_monotonic_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_ordered_clock_monotonic_sc,
  bench_exact_clock_monotonic_sc_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_os_ordered_clock_monotonic_sc,
  bench_exact_clock_monotonic_sc_os_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_ordered_clock_monotonic_scv,
  bench_exact_clock_monotonic_scv_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_os_ordered_clock_monotonic_scv,
  bench_exact_clock_monotonic_scv_os_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_ordered_clock_monotonic_raw,
  bench_exact_clock_monotonic_raw_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_ordered_clock_monotonic_raw_sc,
  bench_exact_clock_monotonic_raw_sc_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_os_ordered_clock_monotonic_raw_sc,
  bench_exact_clock_monotonic_raw_sc_os_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_ordered_clock_monotonic_raw_scv,
  bench_exact_clock_monotonic_raw_scv_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_os_ordered_clock_monotonic_raw_scv,
  bench_exact_clock_monotonic_raw_scv_os_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_ordered_clock_monotonic_vdso,
  bench_exact_clock_monotonic_vdso_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_ordered_clock_monotonic_raw_vdso,
  bench_exact_clock_monotonic_raw_vdso_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(residual_exact_ordered_clock_boottime, bench_exact_clock_boottime_ordered);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_ordered_clock_boottime_sc,
  bench_exact_clock_boottime_sc_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_os_ordered_clock_boottime_sc,
  bench_exact_clock_boottime_sc_os_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_ordered_clock_boottime_scv,
  bench_exact_clock_boottime_scv_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_os_ordered_clock_boottime_scv,
  bench_exact_clock_boottime_scv_os_ordered
);
#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
expose_power_exact_read!(
  residual_exact_ordered_clock_boottime_vdso,
  bench_exact_clock_boottime_vdso_ordered
);

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
macro_rules! expose_freebsd_exact_read {
  ($name:ident, $source:ident) => {
    #[doc(hidden)]
    #[inline(always)]
    #[allow(clippy::inline_always)]
    pub fn $name() -> u64 {
      crate::arch::freebsd_x86_64::$source()
    }
  };
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(residual_exact_freebsd_tsc, bench_exact_tsc);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(residual_exact_freebsd_timekeep, bench_exact_timekeep);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(residual_exact_freebsd_timekeep_os_owned, bench_exact_timekeep_os_owned);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(residual_exact_freebsd_clock_monotonic, bench_exact_clock_monotonic);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_syscall,
  bench_exact_clock_monotonic_syscall
);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(residual_exact_freebsd_tsc_lfence, bench_exact_tsc_lfence_rdtsc);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(residual_exact_freebsd_tsc_mfence, bench_exact_tsc_mfence_rdtsc);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(residual_exact_freebsd_tsc_rdtscp, bench_exact_tsc_rdtscp);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(residual_exact_freebsd_tsc_cpuid, bench_exact_tsc_cpuid_rdtsc);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(residual_exact_freebsd_tsc_serialize, bench_exact_tsc_serialize_rdtsc);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_mfence,
  bench_exact_clock_monotonic_mfence
);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_syscall_mfence,
  bench_exact_clock_monotonic_syscall_mfence
);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_cpuid,
  bench_exact_clock_monotonic_cpuid
);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_syscall_cpuid,
  bench_exact_clock_monotonic_syscall_cpuid
);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_lfence,
  bench_exact_clock_monotonic_lfence
);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_syscall_lfence,
  bench_exact_clock_monotonic_syscall_lfence
);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_rdtscp,
  bench_exact_clock_monotonic_rdtscp
);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_syscall_rdtscp,
  bench_exact_clock_monotonic_syscall_rdtscp
);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_os_owned,
  bench_exact_clock_monotonic_os_owned
);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_syscall_os_owned,
  bench_exact_clock_monotonic_syscall_os_owned
);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_serialize,
  bench_exact_clock_monotonic_serialize
);
#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
expose_freebsd_exact_read!(
  residual_exact_freebsd_clock_monotonic_syscall_serialize,
  bench_exact_clock_monotonic_syscall_serialize
);

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct ThreadCpuNative64SelectionMeasurements {
  pub selection_kind: &'static str,
  pub selection_basis: Option<&'static str>,
  pub selected_provider: &'static str,
  pub selected_read_cost: &'static str,
  pub libc_provider: &'static str,
  pub raw_provider: &'static str,
  pub libc_available: bool,
  pub raw_available: bool,
  pub reads_per_batch: usize,
  pub required_decisive_wins: usize,
  pub floor_ns_per_read: u64,
  pub relative_denominator: Option<u64>,
  pub libc_batches_ns: [u64; 9],
  pub raw_batches_ns: [u64; 9],
  pub libc_median_ns: u64,
  pub raw_median_ns: u64,
  pub raw_allowance_ns: u64,
  pub raw_decisive_wins: usize,
  pub raw_selected: bool,
  pub libc_allowance_ns: u64,
  pub libc_decisive_wins: usize,
  pub libc_materially_faster: bool,
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[doc(hidden)]
pub fn thread_cpu_native64_selection_measurements() -> ThreadCpuNative64SelectionMeasurements {
  let evidence = crate::arch::thread_cpu::bench_native_64_selection_evidence();
  ThreadCpuNative64SelectionMeasurements {
    selection_kind: evidence.selection_kind,
    selection_basis: evidence.selection_basis,
    selected_provider: evidence.selected_provider,
    selected_read_cost: evidence.selected_read_cost,
    libc_provider: evidence.libc_provider,
    raw_provider: evidence.raw_provider,
    libc_available: evidence.libc_available,
    raw_available: evidence.raw_available,
    reads_per_batch: evidence.reads_per_batch,
    required_decisive_wins: evidence.required_decisive_wins,
    floor_ns_per_read: evidence.floor_ns_per_read,
    relative_denominator: evidence.relative_denominator,
    libc_batches_ns: evidence.libc_batches_ns,
    raw_batches_ns: evidence.raw_batches_ns,
    libc_median_ns: evidence.libc_median_ns,
    raw_median_ns: evidence.raw_median_ns,
    raw_allowance_ns: evidence.raw_allowance_ns,
    raw_decisive_wins: evidence.raw_decisive_wins,
    raw_selected: evidence.raw_selected,
    libc_allowance_ns: evidence.libc_allowance_ns,
    libc_decisive_wins: evidence.libc_decisive_wins,
    libc_materially_faster: evidence.libc_materially_faster,
  }
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[doc(hidden)]
pub fn thread_cpu_native64_selected_provider() -> &'static str {
  crate::arch::thread_cpu::bench_native_64_selected_provider()
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[doc(hidden)]
pub fn thread_cpu_native64_libc_provider() -> &'static str {
  crate::arch::thread_cpu::bench_native_64_libc_provider()
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[doc(hidden)]
pub fn thread_cpu_native64_raw_provider() -> &'static str {
  crate::arch::thread_cpu::bench_native_64_raw_provider()
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn thread_cpu_native64_exact_libc_nanos() -> u64 {
  crate::arch::thread_cpu::bench_native_64_libc_nanos()
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn thread_cpu_native64_exact_raw_nanos() -> u64 {
  crate::arch::thread_cpu::bench_native_64_raw_nanos()
}

#[cfg(all(target_arch = "x86", target_os = "linux"))]
#[doc(hidden)]
pub fn thread_cpu_i686_native_provider() -> &'static str {
  crate::arch::thread_cpu::bench_i686_native_provider()
}

#[cfg(all(target_arch = "x86", target_os = "linux"))]
#[doc(hidden)]
pub fn thread_cpu_i686_native_selection_evidence() -> ThreadCpuNativeEntryEvidence {
  let evidence = crate::arch::thread_cpu::bench_i686_native_selection_evidence();
  generic_native_entry_evidence(
    &evidence.candidate_names,
    &evidence.candidate_eligible,
    &evidence.candidate_batches_ns.map(|samples| !samples.contains(&0)),
    &evidence.candidate_batches_ns,
    evidence.selected_candidate,
    4_096,
    8,
  )
}

#[cfg(all(target_arch = "x86", target_os = "linux"))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn thread_cpu_i686_selected_native_nanos() -> u64 {
  crate::arch::thread_cpu::bench_i686_selected_native_nanos()
}

#[cfg(all(target_arch = "x86", target_os = "linux"))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn thread_cpu_i686_exact_candidate(index: usize) -> u64 {
  crate::arch::thread_cpu::bench_i686_exact_candidate(index)
    .expect("eligible i686 native candidate became unavailable")
}

#[cfg(all(target_arch = "arm", target_os = "linux"))]
#[doc(hidden)]
pub fn thread_cpu_arm_native_provider() -> &'static str {
  crate::arch::thread_cpu::bench_arm_native_provider()
}

#[cfg(all(target_arch = "arm", target_os = "linux"))]
#[doc(hidden)]
pub fn thread_cpu_arm_native_selection_evidence() -> ThreadCpuNativeEntryEvidence {
  let evidence = crate::arch::thread_cpu::bench_arm_native_selection_evidence();
  generic_native_entry_evidence(
    &evidence.candidate_names,
    &evidence.candidate_eligible,
    &evidence.candidate_batches_ns.map(|samples| !samples.contains(&0)),
    &evidence.candidate_batches_ns,
    evidence.selected_candidate,
    4_096,
    8,
  )
}

#[cfg(all(target_arch = "arm", target_os = "linux"))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn thread_cpu_arm_selected_native_nanos() -> u64 {
  crate::arch::thread_cpu::bench_arm_selected_native_nanos()
}

#[cfg(all(target_arch = "arm", target_os = "linux"))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn thread_cpu_arm_exact_candidate(index: usize) -> u64 {
  crate::arch::thread_cpu::bench_arm_exact_candidate(index)
    .expect("eligible Arm native candidate became unavailable")
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[doc(hidden)]
pub fn thread_cpu_riscv64_native_provider() -> &'static str {
  crate::arch::thread_cpu::bench_riscv64_native_provider()
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[doc(hidden)]
pub fn thread_cpu_riscv64_native_selection_evidence() -> ThreadCpuNativeEntryEvidence {
  let evidence = crate::arch::thread_cpu::bench_riscv64_native_selection_evidence();
  generic_native_entry_evidence(
    &evidence.candidate_names,
    &evidence.candidate_eligible,
    &evidence.candidate_batches_ns.map(|samples| !samples.contains(&0)),
    &evidence.candidate_batches_ns,
    evidence.selected_candidate,
    4_096,
    8,
  )
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn thread_cpu_riscv64_selected_native_nanos() -> u64 {
  crate::arch::thread_cpu::bench_riscv64_selected_native_nanos()
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn thread_cpu_riscv64_exact_candidate(index: usize) -> u64 {
  crate::arch::thread_cpu::bench_riscv64_exact_candidate(index)
    .expect("eligible RISC-V native candidate became unavailable")
}

#[cfg(all(
  target_os = "linux",
  any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
))]
#[doc(hidden)]
pub fn thread_cpu_rare_linux_native_provider() -> &'static str {
  crate::arch::thread_cpu::bench_rare_linux_native_provider()
}

#[cfg(all(
  target_os = "linux",
  any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
))]
#[doc(hidden)]
pub fn thread_cpu_rare_linux_native_selection_evidence() -> ThreadCpuNativeEntryEvidence {
  let evidence = crate::arch::thread_cpu::bench_rare_linux_native_selection_evidence();
  generic_native_entry_evidence(
    &evidence.candidate_names,
    &evidence.candidate_eligible,
    &evidence.candidate_measured,
    &evidence.candidate_batches_ns,
    evidence.selected_candidate,
    evidence.reads_per_batch,
    evidence.required_decisive_wins,
  )
}

#[cfg(all(
  target_os = "linux",
  any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn thread_cpu_rare_linux_selected_native_nanos() -> u64 {
  crate::arch::thread_cpu::bench_rare_linux_selected_native_nanos()
}

#[cfg(all(
  target_os = "linux",
  any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn thread_cpu_rare_linux_exact_candidate(index: usize) -> u64 {
  crate::arch::thread_cpu::bench_rare_linux_exact_candidate(index)
    .expect("eligible rare Linux native candidate became unavailable")
}

#[cfg(any(
  all(target_arch = "x86", target_os = "linux"),
  all(target_arch = "arm", target_os = "linux"),
  all(target_arch = "riscv64", target_os = "linux"),
  all(
    target_os = "linux",
    any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
  ),
))]
#[doc(hidden)]
#[derive(Clone, Debug, Serialize)]
pub struct ThreadCpuNativeEntryEvidence {
  pub selection_kind: &'static str,
  pub candidate_names: Vec<&'static str>,
  pub candidate_eligible: Vec<bool>,
  pub candidate_measured: Vec<bool>,
  pub candidate_batches_ns: Option<Vec<[u64; 9]>>,
  pub selected_candidate: &'static str,
  pub reads_per_batch: usize,
  pub required_decisive_wins: usize,
  pub equivalence_band: ThreadCpuSelectionEquivalenceBand,
}

#[cfg(any(
  all(target_arch = "x86", target_os = "linux"),
  all(target_arch = "arm", target_os = "linux"),
  all(target_arch = "riscv64", target_os = "linux"),
  all(
    target_os = "linux",
    any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
  ),
))]
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ThreadCpuSelectionEquivalenceBand {
  pub floor_ns_per_read: u64,
  pub relative_denominator: u64,
}

#[cfg(any(
  all(target_arch = "x86", target_os = "linux"),
  all(target_arch = "arm", target_os = "linux"),
  all(target_arch = "riscv64", target_os = "linux"),
  all(
    target_os = "linux",
    any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
  ),
))]
fn generic_native_entry_evidence<const N: usize>(
  names: &[&'static str; N],
  eligible: &[bool; N],
  measured: &[bool; N],
  batches: &[[u64; 9]; N],
  selected: &'static str,
  reads_per_batch: usize,
  required_decisive_wins: usize,
) -> ThreadCpuNativeEntryEvidence {
  let eligible_count = eligible.iter().filter(|value| **value).count();
  let candidate_names: Vec<_> = names
    .iter()
    .zip(eligible)
    .filter_map(|(name, eligible)| eligible.then_some(*name))
    .collect();
  let candidate_measured: Vec<_> = measured
    .iter()
    .zip(eligible)
    .filter_map(|(measured, eligible)| eligible.then_some(*measured))
    .collect();
  let candidate_batches_ns = (eligible_count > 1).then(|| {
    batches
      .iter()
      .zip(eligible)
      .filter_map(|(samples, eligible)| eligible.then_some(*samples))
      .collect::<Vec<_>>()
  });
  ThreadCpuNativeEntryEvidence {
    selection_kind: if eligible_count > 1 { "tournament" } else { "fixed_candidate" },
    candidate_names,
    candidate_eligible: vec![true; eligible_count],
    candidate_measured,
    candidate_batches_ns,
    selected_candidate: selected,
    reads_per_batch,
    required_decisive_wins,
    equivalence_band: ThreadCpuSelectionEquivalenceBand {
      floor_ns_per_read: 1,
      relative_denominator: 20,
    },
  }
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[doc(hidden)]
pub fn freebsd_wall_selected_provider() -> &'static str {
  crate::arch::freebsd_x86_64::bench_provider()
}

#[cfg(any(
  all(target_arch = "riscv64", target_os = "linux"),
  all(target_arch = "loongarch64", target_os = "linux"),
  all(target_arch = "x86_64", target_os = "freebsd"),
))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct ThreeWayWallDomainMeasurements {
  pub candidate_count: usize,
  pub direct_eligible: bool,
  pub clock_raw_available: bool,
  pub syscall_raw_available: bool,
  pub vdso_available: bool,
  pub vdso_raw_available: bool,
  pub clock_boottime_available: bool,
  pub syscall_boottime_available: bool,
  pub vdso_boottime_available: bool,
  pub timekeep_available: bool,
  pub selected_provider: &'static str,
  pub reads_per_batch: u64,
  pub direct_batches_ns: [u64; 9],
  pub clock_batches_ns: [u64; 9],
  pub syscall_batches_ns: [u64; 9],
  pub clock_raw_batches_ns: [u64; 9],
  pub syscall_raw_batches_ns: [u64; 9],
  pub vdso_batches_ns: [u64; 9],
  pub vdso_raw_batches_ns: [u64; 9],
  pub clock_boottime_batches_ns: [u64; 9],
  pub syscall_boottime_batches_ns: [u64; 9],
  pub vdso_boottime_batches_ns: [u64; 9],
  pub timekeep_batches_ns: [u64; 9],
  pub direct_median_ns: u64,
  pub clock_median_ns: u64,
  pub syscall_median_ns: u64,
  pub clock_raw_median_ns: u64,
  pub syscall_raw_median_ns: u64,
  pub vdso_median_ns: u64,
  pub vdso_raw_median_ns: u64,
  pub clock_boottime_median_ns: u64,
  pub syscall_boottime_median_ns: u64,
  pub vdso_boottime_median_ns: u64,
  pub timekeep_median_ns: u64,
  pub fallback_allowance_ns: u64,
  pub fallback_decisive_wins: usize,
  pub clock_raw_allowance_ns: u64,
  pub clock_raw_decisive_wins: usize,
  pub syscall_raw_allowance_ns: u64,
  pub syscall_raw_decisive_wins: usize,
  pub vdso_allowance_ns: u64,
  pub vdso_decisive_wins: usize,
  pub vdso_raw_allowance_ns: u64,
  pub vdso_raw_decisive_wins: usize,
  pub clock_boottime_allowance_ns: u64,
  pub clock_boottime_decisive_wins: usize,
  pub syscall_boottime_allowance_ns: u64,
  pub syscall_boottime_decisive_wins: usize,
  pub vdso_boottime_allowance_ns: u64,
  pub vdso_boottime_decisive_wins: usize,
  pub timekeep_allowance_ns: u64,
  pub timekeep_decisive_wins: usize,
  pub direct_allowance_ns: u64,
  pub direct_decisive_wins: usize,
  pub required_decisive_wins: usize,
}

#[cfg(any(
  all(target_arch = "riscv64", target_os = "linux"),
  all(target_arch = "loongarch64", target_os = "linux"),
  all(target_arch = "x86_64", target_os = "freebsd"),
))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct ThreeWayWallSelectionMeasurements {
  pub instant: ThreeWayWallDomainMeasurements,
  pub ordered: ThreeWayWallDomainMeasurements,
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct RiscvWallDomainMeasurements {
  #[serde(flatten)]
  pub base: ThreeWayWallDomainMeasurements,
  pub syscall_os_ordered_batches_ns: [u64; 9],
  pub syscall_raw_os_ordered_batches_ns: [u64; 9],
  pub syscall_boottime_os_ordered_batches_ns: [u64; 9],
  pub syscall_os_ordered_median_ns: u64,
  pub syscall_raw_os_ordered_median_ns: u64,
  pub syscall_boottime_os_ordered_median_ns: u64,
  pub syscall_os_ordered_allowance_ns: u64,
  pub syscall_os_ordered_decisive_wins: usize,
  pub syscall_raw_os_ordered_allowance_ns: u64,
  pub syscall_raw_os_ordered_decisive_wins: usize,
  pub syscall_boottime_os_ordered_allowance_ns: u64,
  pub syscall_boottime_os_ordered_decisive_wins: usize,
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct RiscvWallSelectionMeasurements {
  pub instant: RiscvWallDomainMeasurements,
  pub ordered: RiscvWallDomainMeasurements,
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct FreeBsdWallSelectionMeasurements {
  pub instant: ThreeWayWallDomainMeasurements,
  pub ordered: ThreeWayWallDomainMeasurements,
  pub ordered_os_barrier: &'static str,
  pub ordered_bare_syscall_os_owned_eligible: bool,
  pub ordered_bare_syscall_os_owned_basis: &'static str,
  pub ordered_fast_barrier_candidate: &'static str,
  pub ordered_fast_barrier_batches_ns: [u64; 9],
  pub ordered_baseline_barrier: &'static str,
  pub ordered_baseline_batches_ns: [u64; 9],
  pub ordered_barrier_allowance_ns: u64,
  pub ordered_barrier_decisive_wins: usize,
  pub ordered_clock_fast_barrier_candidate: &'static str,
  pub ordered_clock_fast_batches_ns: [u64; 9],
  pub ordered_clock_baseline_batches_ns: [u64; 9],
  pub ordered_clock_barrier_allowance_ns: u64,
  pub ordered_clock_barrier_decisive_wins: usize,
  pub ordered_syscall_fast_barrier_candidate: &'static str,
  pub ordered_syscall_fast_batches_ns: [u64; 9],
  pub ordered_syscall_baseline_batches_ns: [u64; 9],
  pub ordered_syscall_barrier_allowance_ns: u64,
  pub ordered_syscall_barrier_decisive_wins: usize,
  pub ordered_barrier_candidate_count: usize,
  pub ordered_barrier_candidate_names: [&'static str; 6],
  pub ordered_syscall_barrier_candidate_count: usize,
  pub ordered_syscall_barrier_candidate_names: [&'static str; 6],
  pub ordered_clock_barrier_candidate_batches_ns: [[u64; 9]; 6],
  pub ordered_clock_barrier_candidate_medians_ns: [u64; 6],
  pub ordered_clock_barrier_decision_count: usize,
  pub ordered_clock_barrier_challengers: [&'static str; 5],
  pub ordered_clock_barrier_incumbents: [&'static str; 5],
  pub ordered_clock_barrier_winners: [&'static str; 5],
  pub ordered_clock_barrier_allowances_ns: [u64; 5],
  pub ordered_clock_barrier_tournament_decisive_wins: [usize; 5],
  pub ordered_clock_barrier_challenger_selected: [bool; 5],
  pub ordered_syscall_barrier_candidate_batches_ns: [[u64; 9]; 6],
  pub ordered_syscall_barrier_candidate_medians_ns: [u64; 6],
  pub ordered_syscall_barrier_decision_count: usize,
  pub ordered_syscall_barrier_challengers: [&'static str; 5],
  pub ordered_syscall_barrier_incumbents: [&'static str; 5],
  pub ordered_syscall_barrier_winners: [&'static str; 5],
  pub ordered_syscall_barrier_allowances_ns: [u64; 5],
  pub ordered_syscall_barrier_tournament_decisive_wins: [usize; 5],
  pub ordered_syscall_barrier_challenger_selected: [bool; 5],
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct LinuxClockWallDomainMeasurements {
  pub candidate_count: usize,
  pub selected_provider: &'static str,
  pub reads_per_batch: u64,
  pub arm_cntvct_available: bool,
  pub arm_cntvct_eligibility_basis: &'static str,
  pub s390_bare_stckf_eligible: bool,
  pub s390_bare_stckf_exclusion: &'static str,
  pub raw_available: bool,
  pub raw_time64_available: bool,
  pub libc_monotonic_raw_available: bool,
  pub raw_monotonic_raw_available: bool,
  pub raw_time64_monotonic_raw_available: bool,
  pub vdso_available: bool,
  pub vdso_monotonic_raw_available: bool,
  pub vdso_time64_available: bool,
  pub vdso_time64_monotonic_raw_available: bool,
  pub libc_boottime_available: bool,
  pub raw_boottime_available: bool,
  pub raw_time64_boottime_available: bool,
  pub vdso_boottime_available: bool,
  pub vdso_time64_boottime_available: bool,
  pub cntvct_batches_ns: [u64; 9],
  pub libc_batches_ns: [u64; 9],
  pub raw_batches_ns: [u64; 9],
  pub raw_os_ordered_batches_ns: [u64; 9],
  pub raw_time64_batches_ns: [u64; 9],
  pub raw_time64_os_ordered_batches_ns: [u64; 9],
  pub libc_monotonic_raw_batches_ns: [u64; 9],
  pub raw_monotonic_raw_batches_ns: [u64; 9],
  pub raw_monotonic_raw_os_ordered_batches_ns: [u64; 9],
  pub raw_time64_monotonic_raw_batches_ns: [u64; 9],
  pub raw_time64_monotonic_raw_os_ordered_batches_ns: [u64; 9],
  pub vdso_batches_ns: [u64; 9],
  pub vdso_monotonic_raw_batches_ns: [u64; 9],
  pub vdso_time64_batches_ns: [u64; 9],
  pub vdso_time64_monotonic_raw_batches_ns: [u64; 9],
  pub libc_boottime_batches_ns: [u64; 9],
  pub raw_boottime_batches_ns: [u64; 9],
  pub raw_boottime_os_ordered_batches_ns: [u64; 9],
  pub raw_time64_boottime_batches_ns: [u64; 9],
  pub raw_time64_boottime_os_ordered_batches_ns: [u64; 9],
  pub vdso_boottime_batches_ns: [u64; 9],
  pub vdso_time64_boottime_batches_ns: [u64; 9],
  pub cntvct_median_ns: u64,
  pub libc_median_ns: u64,
  pub raw_median_ns: u64,
  pub raw_os_ordered_median_ns: u64,
  pub raw_time64_median_ns: u64,
  pub raw_time64_os_ordered_median_ns: u64,
  pub libc_monotonic_raw_median_ns: u64,
  pub raw_monotonic_raw_median_ns: u64,
  pub raw_monotonic_raw_os_ordered_median_ns: u64,
  pub raw_time64_monotonic_raw_median_ns: u64,
  pub raw_time64_monotonic_raw_os_ordered_median_ns: u64,
  pub vdso_median_ns: u64,
  pub vdso_monotonic_raw_median_ns: u64,
  pub vdso_time64_median_ns: u64,
  pub vdso_time64_monotonic_raw_median_ns: u64,
  pub libc_boottime_median_ns: u64,
  pub raw_boottime_median_ns: u64,
  pub raw_boottime_os_ordered_median_ns: u64,
  pub raw_time64_boottime_median_ns: u64,
  pub raw_time64_boottime_os_ordered_median_ns: u64,
  pub vdso_boottime_median_ns: u64,
  pub vdso_time64_boottime_median_ns: u64,
  pub cntvct_allowance_ns: u64,
  pub cntvct_decisive_wins: usize,
  pub raw_allowance_ns: u64,
  pub raw_decisive_wins: usize,
  pub raw_os_ordered_allowance_ns: u64,
  pub raw_os_ordered_decisive_wins: usize,
  pub raw_time64_allowance_ns: u64,
  pub raw_time64_decisive_wins: usize,
  pub raw_time64_os_ordered_allowance_ns: u64,
  pub raw_time64_os_ordered_decisive_wins: usize,
  pub libc_monotonic_raw_allowance_ns: u64,
  pub libc_monotonic_raw_decisive_wins: usize,
  pub raw_monotonic_raw_allowance_ns: u64,
  pub raw_monotonic_raw_decisive_wins: usize,
  pub raw_monotonic_raw_os_ordered_allowance_ns: u64,
  pub raw_monotonic_raw_os_ordered_decisive_wins: usize,
  pub raw_time64_monotonic_raw_allowance_ns: u64,
  pub raw_time64_monotonic_raw_decisive_wins: usize,
  pub raw_time64_monotonic_raw_os_ordered_allowance_ns: u64,
  pub raw_time64_monotonic_raw_os_ordered_decisive_wins: usize,
  pub vdso_allowance_ns: u64,
  pub vdso_decisive_wins: usize,
  pub vdso_monotonic_raw_allowance_ns: u64,
  pub vdso_monotonic_raw_decisive_wins: usize,
  pub vdso_time64_allowance_ns: u64,
  pub vdso_time64_decisive_wins: usize,
  pub vdso_time64_monotonic_raw_allowance_ns: u64,
  pub vdso_time64_monotonic_raw_decisive_wins: usize,
  pub libc_boottime_allowance_ns: u64,
  pub libc_boottime_decisive_wins: usize,
  pub raw_boottime_allowance_ns: u64,
  pub raw_boottime_decisive_wins: usize,
  pub raw_boottime_os_ordered_allowance_ns: u64,
  pub raw_boottime_os_ordered_decisive_wins: usize,
  pub raw_time64_boottime_allowance_ns: u64,
  pub raw_time64_boottime_decisive_wins: usize,
  pub raw_time64_boottime_os_ordered_allowance_ns: u64,
  pub raw_time64_boottime_os_ordered_decisive_wins: usize,
  pub vdso_boottime_allowance_ns: u64,
  pub vdso_boottime_decisive_wins: usize,
  pub vdso_time64_boottime_allowance_ns: u64,
  pub vdso_time64_boottime_decisive_wins: usize,
  pub required_decisive_wins: usize,
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct LinuxClockWallSelectionMeasurements {
  pub instant: LinuxClockWallDomainMeasurements,
  pub ordered: LinuxClockWallDomainMeasurements,
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[doc(hidden)]
pub fn linux_clock_wall_selection_measurements() -> LinuxClockWallSelectionMeasurements {
  fn domain(
    evidence: crate::arch::linux_clock_wall::ProbeEvidence,
  ) -> LinuxClockWallDomainMeasurements {
    LinuxClockWallDomainMeasurements {
      candidate_count: evidence.candidate_count,
      selected_provider: evidence.selected_provider.name(),
      reads_per_batch: evidence.reads_per_batch,
      arm_cntvct_available: evidence.arm_cntvct_available,
      arm_cntvct_eligibility_basis: evidence.arm_cntvct_eligibility_basis,
      s390_bare_stckf_eligible: evidence.s390_bare_stckf_eligible,
      s390_bare_stckf_exclusion: evidence.s390_bare_stckf_exclusion,
      raw_available: evidence.raw_available,
      raw_time64_available: evidence.raw_time64_available,
      libc_monotonic_raw_available: evidence.libc_monotonic_raw_available,
      raw_monotonic_raw_available: evidence.raw_monotonic_raw_available,
      raw_time64_monotonic_raw_available: evidence.raw_time64_monotonic_raw_available,
      vdso_available: evidence.vdso_available,
      vdso_monotonic_raw_available: evidence.vdso_monotonic_raw_available,
      vdso_time64_available: evidence.vdso_time64_available,
      vdso_time64_monotonic_raw_available: evidence.vdso_time64_monotonic_raw_available,
      libc_boottime_available: evidence.libc_boottime_available,
      raw_boottime_available: evidence.raw_boottime_available,
      raw_time64_boottime_available: evidence.raw_time64_boottime_available,
      vdso_boottime_available: evidence.vdso_boottime_available,
      vdso_time64_boottime_available: evidence.vdso_time64_boottime_available,
      cntvct_batches_ns: evidence.cntvct_batches_ns,
      libc_batches_ns: evidence.libc_batches_ns,
      raw_batches_ns: evidence.raw_batches_ns,
      raw_os_ordered_batches_ns: evidence.raw_os_ordered_batches_ns,
      raw_time64_batches_ns: evidence.raw_time64_batches_ns,
      raw_time64_os_ordered_batches_ns: evidence.raw_time64_os_ordered_batches_ns,
      libc_monotonic_raw_batches_ns: evidence.libc_monotonic_raw_batches_ns,
      raw_monotonic_raw_batches_ns: evidence.raw_monotonic_raw_batches_ns,
      raw_monotonic_raw_os_ordered_batches_ns: evidence.raw_monotonic_raw_os_ordered_batches_ns,
      raw_time64_monotonic_raw_batches_ns: evidence.raw_time64_monotonic_raw_batches_ns,
      raw_time64_monotonic_raw_os_ordered_batches_ns: evidence
        .raw_time64_monotonic_raw_os_ordered_batches_ns,
      vdso_batches_ns: evidence.vdso_batches_ns,
      vdso_monotonic_raw_batches_ns: evidence.vdso_monotonic_raw_batches_ns,
      vdso_time64_batches_ns: evidence.vdso_time64_batches_ns,
      vdso_time64_monotonic_raw_batches_ns: evidence.vdso_time64_monotonic_raw_batches_ns,
      libc_boottime_batches_ns: evidence.libc_boottime_batches_ns,
      raw_boottime_batches_ns: evidence.raw_boottime_batches_ns,
      raw_boottime_os_ordered_batches_ns: evidence.raw_boottime_os_ordered_batches_ns,
      raw_time64_boottime_batches_ns: evidence.raw_time64_boottime_batches_ns,
      raw_time64_boottime_os_ordered_batches_ns: evidence.raw_time64_boottime_os_ordered_batches_ns,
      vdso_boottime_batches_ns: evidence.vdso_boottime_batches_ns,
      vdso_time64_boottime_batches_ns: evidence.vdso_time64_boottime_batches_ns,
      cntvct_median_ns: evidence.cntvct_median_ns,
      libc_median_ns: evidence.libc_median_ns,
      raw_median_ns: evidence.raw_median_ns,
      raw_os_ordered_median_ns: evidence.raw_os_ordered_median_ns,
      raw_time64_median_ns: evidence.raw_time64_median_ns,
      raw_time64_os_ordered_median_ns: evidence.raw_time64_os_ordered_median_ns,
      libc_monotonic_raw_median_ns: evidence.libc_monotonic_raw_median_ns,
      raw_monotonic_raw_median_ns: evidence.raw_monotonic_raw_median_ns,
      raw_monotonic_raw_os_ordered_median_ns: evidence.raw_monotonic_raw_os_ordered_median_ns,
      raw_time64_monotonic_raw_median_ns: evidence.raw_time64_monotonic_raw_median_ns,
      raw_time64_monotonic_raw_os_ordered_median_ns: evidence
        .raw_time64_monotonic_raw_os_ordered_median_ns,
      vdso_median_ns: evidence.vdso_median_ns,
      vdso_monotonic_raw_median_ns: evidence.vdso_monotonic_raw_median_ns,
      vdso_time64_median_ns: evidence.vdso_time64_median_ns,
      vdso_time64_monotonic_raw_median_ns: evidence.vdso_time64_monotonic_raw_median_ns,
      libc_boottime_median_ns: evidence.libc_boottime_median_ns,
      raw_boottime_median_ns: evidence.raw_boottime_median_ns,
      raw_boottime_os_ordered_median_ns: evidence.raw_boottime_os_ordered_median_ns,
      raw_time64_boottime_median_ns: evidence.raw_time64_boottime_median_ns,
      raw_time64_boottime_os_ordered_median_ns: evidence.raw_time64_boottime_os_ordered_median_ns,
      vdso_boottime_median_ns: evidence.vdso_boottime_median_ns,
      vdso_time64_boottime_median_ns: evidence.vdso_time64_boottime_median_ns,
      cntvct_allowance_ns: evidence.cntvct_allowance_ns,
      cntvct_decisive_wins: evidence.cntvct_decisive_wins,
      raw_allowance_ns: evidence.raw_allowance_ns,
      raw_decisive_wins: evidence.raw_decisive_wins,
      raw_os_ordered_allowance_ns: evidence.raw_os_ordered_allowance_ns,
      raw_os_ordered_decisive_wins: evidence.raw_os_ordered_decisive_wins,
      raw_time64_allowance_ns: evidence.raw_time64_allowance_ns,
      raw_time64_decisive_wins: evidence.raw_time64_decisive_wins,
      raw_time64_os_ordered_allowance_ns: evidence.raw_time64_os_ordered_allowance_ns,
      raw_time64_os_ordered_decisive_wins: evidence.raw_time64_os_ordered_decisive_wins,
      libc_monotonic_raw_allowance_ns: evidence.libc_monotonic_raw_allowance_ns,
      libc_monotonic_raw_decisive_wins: evidence.libc_monotonic_raw_decisive_wins,
      raw_monotonic_raw_allowance_ns: evidence.raw_monotonic_raw_allowance_ns,
      raw_monotonic_raw_decisive_wins: evidence.raw_monotonic_raw_decisive_wins,
      raw_monotonic_raw_os_ordered_allowance_ns: evidence.raw_monotonic_raw_os_ordered_allowance_ns,
      raw_monotonic_raw_os_ordered_decisive_wins: evidence
        .raw_monotonic_raw_os_ordered_decisive_wins,
      raw_time64_monotonic_raw_allowance_ns: evidence.raw_time64_monotonic_raw_allowance_ns,
      raw_time64_monotonic_raw_decisive_wins: evidence.raw_time64_monotonic_raw_decisive_wins,
      raw_time64_monotonic_raw_os_ordered_allowance_ns: evidence
        .raw_time64_monotonic_raw_os_ordered_allowance_ns,
      raw_time64_monotonic_raw_os_ordered_decisive_wins: evidence
        .raw_time64_monotonic_raw_os_ordered_decisive_wins,
      vdso_allowance_ns: evidence.vdso_allowance_ns,
      vdso_decisive_wins: evidence.vdso_decisive_wins,
      vdso_monotonic_raw_allowance_ns: evidence.vdso_monotonic_raw_allowance_ns,
      vdso_monotonic_raw_decisive_wins: evidence.vdso_monotonic_raw_decisive_wins,
      vdso_time64_allowance_ns: evidence.vdso_time64_allowance_ns,
      vdso_time64_decisive_wins: evidence.vdso_time64_decisive_wins,
      vdso_time64_monotonic_raw_allowance_ns: evidence.vdso_time64_monotonic_raw_allowance_ns,
      vdso_time64_monotonic_raw_decisive_wins: evidence.vdso_time64_monotonic_raw_decisive_wins,
      libc_boottime_allowance_ns: evidence.libc_boottime_allowance_ns,
      libc_boottime_decisive_wins: evidence.libc_boottime_decisive_wins,
      raw_boottime_allowance_ns: evidence.raw_boottime_allowance_ns,
      raw_boottime_decisive_wins: evidence.raw_boottime_decisive_wins,
      raw_boottime_os_ordered_allowance_ns: evidence.raw_boottime_os_ordered_allowance_ns,
      raw_boottime_os_ordered_decisive_wins: evidence.raw_boottime_os_ordered_decisive_wins,
      raw_time64_boottime_allowance_ns: evidence.raw_time64_boottime_allowance_ns,
      raw_time64_boottime_decisive_wins: evidence.raw_time64_boottime_decisive_wins,
      raw_time64_boottime_os_ordered_allowance_ns: evidence
        .raw_time64_boottime_os_ordered_allowance_ns,
      raw_time64_boottime_os_ordered_decisive_wins: evidence
        .raw_time64_boottime_os_ordered_decisive_wins,
      vdso_boottime_allowance_ns: evidence.vdso_boottime_allowance_ns,
      vdso_boottime_decisive_wins: evidence.vdso_boottime_decisive_wins,
      vdso_time64_boottime_allowance_ns: evidence.vdso_time64_boottime_allowance_ns,
      vdso_time64_boottime_decisive_wins: evidence.vdso_time64_boottime_decisive_wins,
      required_decisive_wins: evidence.required_decisive_wins,
    }
  }
  LinuxClockWallSelectionMeasurements {
    instant: domain(crate::arch::linux_clock_wall::bench_instant_evidence()),
    ordered: domain(crate::arch::linux_clock_wall::bench_ordered_evidence()),
  }
}

#[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
#[doc(hidden)]
pub fn linux_clock_wall_selected_providers() -> [&'static str; 2] {
  [
    crate::arch::linux_clock_wall::bench_instant_provider().name(),
    crate::arch::linux_clock_wall::bench_ordered_provider().name(),
  ]
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[doc(hidden)]
pub fn riscv64_wall_selection_measurements() -> RiscvWallSelectionMeasurements {
  fn domain(evidence: crate::arch::riscv64::ProbeEvidence) -> RiscvWallDomainMeasurements {
    let base = ThreeWayWallDomainMeasurements {
      candidate_count: evidence.candidate_count,
      direct_eligible: evidence.direct_eligible,
      clock_raw_available: evidence.clock_raw_available,
      syscall_raw_available: evidence.syscall_raw_available,
      vdso_available: evidence.vdso_available,
      vdso_raw_available: evidence.vdso_raw_available,
      clock_boottime_available: evidence.clock_boottime_available,
      syscall_boottime_available: evidence.syscall_boottime_available,
      vdso_boottime_available: evidence.vdso_boottime_available,
      timekeep_available: false,
      selected_provider: evidence.selected_provider.name(),
      reads_per_batch: evidence.reads_per_batch,
      direct_batches_ns: evidence.direct_batches_ns,
      clock_batches_ns: evidence.clock_batches_ns,
      syscall_batches_ns: evidence.syscall_batches_ns,
      clock_raw_batches_ns: evidence.clock_raw_batches_ns,
      syscall_raw_batches_ns: evidence.syscall_raw_batches_ns,
      vdso_batches_ns: evidence.vdso_batches_ns,
      vdso_raw_batches_ns: evidence.vdso_raw_batches_ns,
      clock_boottime_batches_ns: evidence.clock_boottime_batches_ns,
      syscall_boottime_batches_ns: evidence.syscall_boottime_batches_ns,
      vdso_boottime_batches_ns: evidence.vdso_boottime_batches_ns,
      timekeep_batches_ns: [0; 9],
      direct_median_ns: evidence.direct_median_ns,
      clock_median_ns: evidence.clock_median_ns,
      syscall_median_ns: evidence.syscall_median_ns,
      clock_raw_median_ns: evidence.clock_raw_median_ns,
      syscall_raw_median_ns: evidence.syscall_raw_median_ns,
      vdso_median_ns: evidence.vdso_median_ns,
      vdso_raw_median_ns: evidence.vdso_raw_median_ns,
      clock_boottime_median_ns: evidence.clock_boottime_median_ns,
      syscall_boottime_median_ns: evidence.syscall_boottime_median_ns,
      vdso_boottime_median_ns: evidence.vdso_boottime_median_ns,
      timekeep_median_ns: 0,
      fallback_allowance_ns: evidence.fallback_allowance_ns,
      fallback_decisive_wins: evidence.fallback_decisive_wins,
      clock_raw_allowance_ns: evidence.clock_raw_allowance_ns,
      clock_raw_decisive_wins: evidence.clock_raw_decisive_wins,
      syscall_raw_allowance_ns: evidence.syscall_raw_allowance_ns,
      syscall_raw_decisive_wins: evidence.syscall_raw_decisive_wins,
      vdso_allowance_ns: evidence.vdso_allowance_ns,
      vdso_decisive_wins: evidence.vdso_decisive_wins,
      vdso_raw_allowance_ns: evidence.vdso_raw_allowance_ns,
      vdso_raw_decisive_wins: evidence.vdso_raw_decisive_wins,
      clock_boottime_allowance_ns: evidence.clock_boottime_allowance_ns,
      clock_boottime_decisive_wins: evidence.clock_boottime_decisive_wins,
      syscall_boottime_allowance_ns: evidence.syscall_boottime_allowance_ns,
      syscall_boottime_decisive_wins: evidence.syscall_boottime_decisive_wins,
      vdso_boottime_allowance_ns: evidence.vdso_boottime_allowance_ns,
      vdso_boottime_decisive_wins: evidence.vdso_boottime_decisive_wins,
      timekeep_allowance_ns: 0,
      timekeep_decisive_wins: 0,
      direct_allowance_ns: evidence.direct_allowance_ns,
      direct_decisive_wins: evidence.direct_decisive_wins,
      required_decisive_wins: evidence.required_decisive_wins,
    };
    RiscvWallDomainMeasurements {
      base,
      syscall_os_ordered_batches_ns: evidence.syscall_os_ordered_batches_ns,
      syscall_raw_os_ordered_batches_ns: evidence.syscall_raw_os_ordered_batches_ns,
      syscall_boottime_os_ordered_batches_ns: evidence.syscall_boottime_os_ordered_batches_ns,
      syscall_os_ordered_median_ns: evidence.syscall_os_ordered_median_ns,
      syscall_raw_os_ordered_median_ns: evidence.syscall_raw_os_ordered_median_ns,
      syscall_boottime_os_ordered_median_ns: evidence.syscall_boottime_os_ordered_median_ns,
      syscall_os_ordered_allowance_ns: evidence.syscall_os_ordered_allowance_ns,
      syscall_os_ordered_decisive_wins: evidence.syscall_os_ordered_decisive_wins,
      syscall_raw_os_ordered_allowance_ns: evidence.syscall_raw_os_ordered_allowance_ns,
      syscall_raw_os_ordered_decisive_wins: evidence.syscall_raw_os_ordered_decisive_wins,
      syscall_boottime_os_ordered_allowance_ns: evidence.syscall_boottime_os_ordered_allowance_ns,
      syscall_boottime_os_ordered_decisive_wins: evidence.syscall_boottime_os_ordered_decisive_wins,
    }
  }
  RiscvWallSelectionMeasurements {
    instant: domain(crate::arch::riscv64::bench_instant_evidence()),
    ordered: domain(crate::arch::riscv64::bench_ordered_evidence()),
  }
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[doc(hidden)]
pub fn riscv64_wall_selected_providers() -> [&'static str; 2] {
  [
    crate::arch::riscv64::bench_instant_provider().name(),
    crate::arch::riscv64::bench_ordered_provider().name(),
  ]
}

#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
#[doc(hidden)]
pub fn loongarch64_wall_selection_measurements() -> ThreeWayWallSelectionMeasurements {
  fn domain(evidence: crate::arch::loongarch64::ProbeEvidence) -> ThreeWayWallDomainMeasurements {
    ThreeWayWallDomainMeasurements {
      candidate_count: evidence.candidate_count,
      direct_eligible: true,
      clock_raw_available: evidence.clock_raw_available,
      syscall_raw_available: evidence.syscall_raw_available,
      vdso_available: evidence.vdso_available,
      vdso_raw_available: evidence.vdso_raw_available,
      clock_boottime_available: evidence.clock_boottime_available,
      syscall_boottime_available: evidence.syscall_boottime_available,
      vdso_boottime_available: evidence.vdso_boottime_available,
      timekeep_available: false,
      selected_provider: evidence.selected_provider.name(),
      reads_per_batch: evidence.reads_per_batch,
      direct_batches_ns: evidence.direct_batches_ns,
      clock_batches_ns: evidence.clock_batches_ns,
      syscall_batches_ns: evidence.syscall_batches_ns,
      clock_raw_batches_ns: evidence.clock_raw_batches_ns,
      syscall_raw_batches_ns: evidence.syscall_raw_batches_ns,
      vdso_batches_ns: evidence.vdso_batches_ns,
      vdso_raw_batches_ns: evidence.vdso_raw_batches_ns,
      clock_boottime_batches_ns: evidence.clock_boottime_batches_ns,
      syscall_boottime_batches_ns: evidence.syscall_boottime_batches_ns,
      vdso_boottime_batches_ns: evidence.vdso_boottime_batches_ns,
      timekeep_batches_ns: [0; 9],
      direct_median_ns: evidence.direct_median_ns,
      clock_median_ns: evidence.clock_median_ns,
      syscall_median_ns: evidence.syscall_median_ns,
      clock_raw_median_ns: evidence.clock_raw_median_ns,
      syscall_raw_median_ns: evidence.syscall_raw_median_ns,
      vdso_median_ns: evidence.vdso_median_ns,
      vdso_raw_median_ns: evidence.vdso_raw_median_ns,
      clock_boottime_median_ns: evidence.clock_boottime_median_ns,
      syscall_boottime_median_ns: evidence.syscall_boottime_median_ns,
      vdso_boottime_median_ns: evidence.vdso_boottime_median_ns,
      timekeep_median_ns: 0,
      fallback_allowance_ns: evidence.fallback_allowance_ns,
      fallback_decisive_wins: evidence.fallback_decisive_wins,
      clock_raw_allowance_ns: evidence.clock_raw_allowance_ns,
      clock_raw_decisive_wins: evidence.clock_raw_decisive_wins,
      syscall_raw_allowance_ns: evidence.syscall_raw_allowance_ns,
      syscall_raw_decisive_wins: evidence.syscall_raw_decisive_wins,
      vdso_allowance_ns: evidence.vdso_allowance_ns,
      vdso_decisive_wins: evidence.vdso_decisive_wins,
      vdso_raw_allowance_ns: evidence.vdso_raw_allowance_ns,
      vdso_raw_decisive_wins: evidence.vdso_raw_decisive_wins,
      clock_boottime_allowance_ns: evidence.clock_boottime_allowance_ns,
      clock_boottime_decisive_wins: evidence.clock_boottime_decisive_wins,
      syscall_boottime_allowance_ns: evidence.syscall_boottime_allowance_ns,
      syscall_boottime_decisive_wins: evidence.syscall_boottime_decisive_wins,
      vdso_boottime_allowance_ns: evidence.vdso_boottime_allowance_ns,
      vdso_boottime_decisive_wins: evidence.vdso_boottime_decisive_wins,
      timekeep_allowance_ns: 0,
      timekeep_decisive_wins: 0,
      direct_allowance_ns: evidence.direct_allowance_ns,
      direct_decisive_wins: evidence.direct_decisive_wins,
      required_decisive_wins: evidence.required_decisive_wins,
    }
  }
  ThreeWayWallSelectionMeasurements {
    instant: domain(crate::arch::loongarch64::bench_instant_evidence()),
    ordered: domain(crate::arch::loongarch64::bench_ordered_evidence()),
  }
}

#[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
#[doc(hidden)]
pub fn loongarch64_wall_selected_providers() -> [&'static str; 2] {
  [
    crate::arch::loongarch64::bench_instant_provider().name(),
    crate::arch::loongarch64::bench_ordered_provider().name(),
  ]
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[doc(hidden)]
pub fn freebsd_wall_selection_measurements() -> FreeBsdWallSelectionMeasurements {
  fn domain(
    evidence: crate::arch::freebsd_x86_64::ProbeEvidence,
  ) -> ThreeWayWallDomainMeasurements {
    ThreeWayWallDomainMeasurements {
      candidate_count: evidence.candidate_count,
      direct_eligible: evidence.tsc_eligible,
      clock_raw_available: false,
      syscall_raw_available: false,
      vdso_available: false,
      vdso_raw_available: false,
      clock_boottime_available: false,
      syscall_boottime_available: false,
      vdso_boottime_available: false,
      timekeep_available: evidence.timekeep_available,
      selected_provider: evidence.selected_provider_name,
      reads_per_batch: evidence.reads_per_batch,
      direct_batches_ns: evidence.direct_batches_ns,
      clock_batches_ns: evidence.clock_batches_ns,
      syscall_batches_ns: evidence.syscall_batches_ns,
      clock_raw_batches_ns: [0; 9],
      syscall_raw_batches_ns: [0; 9],
      vdso_batches_ns: [0; 9],
      vdso_raw_batches_ns: [0; 9],
      clock_boottime_batches_ns: [0; 9],
      syscall_boottime_batches_ns: [0; 9],
      vdso_boottime_batches_ns: [0; 9],
      timekeep_batches_ns: evidence.timekeep_batches_ns,
      direct_median_ns: evidence.direct_median_ns,
      clock_median_ns: evidence.clock_median_ns,
      syscall_median_ns: evidence.syscall_median_ns,
      clock_raw_median_ns: 0,
      syscall_raw_median_ns: 0,
      vdso_median_ns: 0,
      vdso_raw_median_ns: 0,
      clock_boottime_median_ns: 0,
      syscall_boottime_median_ns: 0,
      vdso_boottime_median_ns: 0,
      timekeep_median_ns: evidence.timekeep_median_ns,
      fallback_allowance_ns: evidence.fallback_allowance_ns,
      fallback_decisive_wins: evidence.fallback_decisive_wins,
      clock_raw_allowance_ns: 0,
      clock_raw_decisive_wins: 0,
      syscall_raw_allowance_ns: 0,
      syscall_raw_decisive_wins: 0,
      vdso_allowance_ns: 0,
      vdso_decisive_wins: 0,
      vdso_raw_allowance_ns: 0,
      vdso_raw_decisive_wins: 0,
      clock_boottime_allowance_ns: 0,
      clock_boottime_decisive_wins: 0,
      syscall_boottime_allowance_ns: 0,
      syscall_boottime_decisive_wins: 0,
      vdso_boottime_allowance_ns: 0,
      vdso_boottime_decisive_wins: 0,
      timekeep_allowance_ns: evidence.timekeep_allowance_ns,
      timekeep_decisive_wins: evidence.timekeep_decisive_wins,
      direct_allowance_ns: evidence.direct_allowance_ns,
      direct_decisive_wins: evidence.direct_decisive_wins,
      required_decisive_wins: evidence.required_decisive_wins,
    }
  }
  let instant = crate::arch::freebsd_x86_64::bench_instant_evidence();
  let ordered = crate::arch::freebsd_x86_64::bench_ordered_evidence();
  FreeBsdWallSelectionMeasurements {
    instant: domain(instant),
    ordered: domain(ordered),
    ordered_os_barrier: ordered.ordered_os_barrier,
    ordered_bare_syscall_os_owned_eligible: ordered.ordered_bare_syscall_os_owned_eligible,
    ordered_bare_syscall_os_owned_basis: ordered.ordered_bare_syscall_os_owned_basis,
    ordered_fast_barrier_candidate: ordered.ordered_fast_barrier_candidate,
    ordered_fast_barrier_batches_ns: ordered.ordered_fast_barrier_batches_ns,
    ordered_baseline_barrier: ordered.ordered_baseline_barrier,
    ordered_baseline_batches_ns: ordered.ordered_mfence_batches_ns,
    ordered_barrier_allowance_ns: ordered.ordered_barrier_allowance_ns,
    ordered_barrier_decisive_wins: ordered.ordered_barrier_decisive_wins,
    ordered_clock_fast_barrier_candidate: ordered.ordered_clock_fast_barrier_candidate,
    ordered_clock_fast_batches_ns: ordered.ordered_clock_fast_batches_ns,
    ordered_clock_baseline_batches_ns: ordered.ordered_clock_baseline_batches_ns,
    ordered_clock_barrier_allowance_ns: ordered.ordered_clock_barrier_allowance_ns,
    ordered_clock_barrier_decisive_wins: ordered.ordered_clock_barrier_decisive_wins,
    ordered_syscall_fast_barrier_candidate: ordered.ordered_syscall_fast_barrier_candidate,
    ordered_syscall_fast_batches_ns: ordered.ordered_syscall_fast_batches_ns,
    ordered_syscall_baseline_batches_ns: ordered.ordered_syscall_baseline_batches_ns,
    ordered_syscall_barrier_allowance_ns: ordered.ordered_syscall_barrier_allowance_ns,
    ordered_syscall_barrier_decisive_wins: ordered.ordered_syscall_barrier_decisive_wins,
    ordered_barrier_candidate_count: ordered.ordered_barrier_candidate_count,
    ordered_barrier_candidate_names: ordered.ordered_barrier_candidate_names,
    ordered_syscall_barrier_candidate_count: ordered.ordered_syscall_barrier_candidate_count,
    ordered_syscall_barrier_candidate_names: ordered.ordered_syscall_barrier_candidate_names,
    ordered_clock_barrier_candidate_batches_ns: ordered.ordered_clock_barrier_candidate_batches_ns,
    ordered_clock_barrier_candidate_medians_ns: ordered.ordered_clock_barrier_candidate_medians_ns,
    ordered_clock_barrier_decision_count: ordered.ordered_clock_barrier_decision_count,
    ordered_clock_barrier_challengers: ordered.ordered_clock_barrier_challengers,
    ordered_clock_barrier_incumbents: ordered.ordered_clock_barrier_incumbents,
    ordered_clock_barrier_winners: ordered.ordered_clock_barrier_winners,
    ordered_clock_barrier_allowances_ns: ordered.ordered_clock_barrier_allowances_ns,
    ordered_clock_barrier_tournament_decisive_wins: ordered
      .ordered_clock_barrier_tournament_decisive_wins,
    ordered_clock_barrier_challenger_selected: ordered.ordered_clock_barrier_challenger_selected,
    ordered_syscall_barrier_candidate_batches_ns: ordered
      .ordered_syscall_barrier_candidate_batches_ns,
    ordered_syscall_barrier_candidate_medians_ns: ordered
      .ordered_syscall_barrier_candidate_medians_ns,
    ordered_syscall_barrier_decision_count: ordered.ordered_syscall_barrier_decision_count,
    ordered_syscall_barrier_challengers: ordered.ordered_syscall_barrier_challengers,
    ordered_syscall_barrier_incumbents: ordered.ordered_syscall_barrier_incumbents,
    ordered_syscall_barrier_winners: ordered.ordered_syscall_barrier_winners,
    ordered_syscall_barrier_allowances_ns: ordered.ordered_syscall_barrier_allowances_ns,
    ordered_syscall_barrier_tournament_decisive_wins: ordered
      .ordered_syscall_barrier_tournament_decisive_wins,
    ordered_syscall_barrier_challenger_selected: ordered
      .ordered_syscall_barrier_challenger_selected,
  }
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[doc(hidden)]
pub fn freebsd_wall_selected_providers() -> [&'static str; 2] {
  [
    crate::arch::freebsd_x86_64::bench_selected_instant_primitive().name,
    crate::arch::freebsd_x86_64::bench_selected_ordered_primitive().name,
  ]
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct PowerWallDomainMeasurements {
  pub candidate_count: usize,
  pub scv_eligible: bool,
  pub clock_raw_available: bool,
  pub sc_raw_available: bool,
  pub scv_raw_available: bool,
  pub vdso_available: bool,
  pub vdso_raw_available: bool,
  pub clock_boottime_available: bool,
  pub sc_boottime_available: bool,
  pub scv_boottime_available: bool,
  pub vdso_boottime_available: bool,
  pub selected_provider: &'static str,
  pub reads_per_batch: u64,
  pub direct_batches_ns: [u64; 9],
  pub clock_batches_ns: [u64; 9],
  pub sc_batches_ns: [u64; 9],
  pub sc_os_ordered_batches_ns: [u64; 9],
  pub scv_batches_ns: [u64; 9],
  pub scv_os_ordered_batches_ns: [u64; 9],
  pub clock_raw_batches_ns: [u64; 9],
  pub sc_raw_batches_ns: [u64; 9],
  pub sc_raw_os_ordered_batches_ns: [u64; 9],
  pub scv_raw_batches_ns: [u64; 9],
  pub scv_raw_os_ordered_batches_ns: [u64; 9],
  pub vdso_batches_ns: [u64; 9],
  pub vdso_raw_batches_ns: [u64; 9],
  pub clock_boottime_batches_ns: [u64; 9],
  pub sc_boottime_batches_ns: [u64; 9],
  pub sc_boottime_os_ordered_batches_ns: [u64; 9],
  pub scv_boottime_batches_ns: [u64; 9],
  pub scv_boottime_os_ordered_batches_ns: [u64; 9],
  pub vdso_boottime_batches_ns: [u64; 9],
  pub direct_median_ns: u64,
  pub clock_median_ns: u64,
  pub sc_median_ns: u64,
  pub sc_os_ordered_median_ns: u64,
  pub scv_median_ns: u64,
  pub scv_os_ordered_median_ns: u64,
  pub clock_raw_median_ns: u64,
  pub sc_raw_median_ns: u64,
  pub sc_raw_os_ordered_median_ns: u64,
  pub scv_raw_median_ns: u64,
  pub scv_raw_os_ordered_median_ns: u64,
  pub vdso_median_ns: u64,
  pub vdso_raw_median_ns: u64,
  pub clock_boottime_median_ns: u64,
  pub sc_boottime_median_ns: u64,
  pub sc_boottime_os_ordered_median_ns: u64,
  pub scv_boottime_median_ns: u64,
  pub scv_boottime_os_ordered_median_ns: u64,
  pub vdso_boottime_median_ns: u64,
  pub sc_allowance_ns: u64,
  pub sc_decisive_wins: usize,
  pub sc_os_ordered_allowance_ns: u64,
  pub sc_os_ordered_decisive_wins: usize,
  pub scv_allowance_ns: u64,
  pub scv_decisive_wins: usize,
  pub scv_os_ordered_allowance_ns: u64,
  pub scv_os_ordered_decisive_wins: usize,
  pub clock_raw_allowance_ns: u64,
  pub clock_raw_decisive_wins: usize,
  pub sc_raw_allowance_ns: u64,
  pub sc_raw_decisive_wins: usize,
  pub sc_raw_os_ordered_allowance_ns: u64,
  pub sc_raw_os_ordered_decisive_wins: usize,
  pub scv_raw_allowance_ns: u64,
  pub scv_raw_decisive_wins: usize,
  pub scv_raw_os_ordered_allowance_ns: u64,
  pub scv_raw_os_ordered_decisive_wins: usize,
  pub vdso_allowance_ns: u64,
  pub vdso_decisive_wins: usize,
  pub vdso_raw_allowance_ns: u64,
  pub vdso_raw_decisive_wins: usize,
  pub clock_boottime_allowance_ns: u64,
  pub clock_boottime_decisive_wins: usize,
  pub sc_boottime_allowance_ns: u64,
  pub sc_boottime_decisive_wins: usize,
  pub sc_boottime_os_ordered_allowance_ns: u64,
  pub sc_boottime_os_ordered_decisive_wins: usize,
  pub scv_boottime_allowance_ns: u64,
  pub scv_boottime_decisive_wins: usize,
  pub scv_boottime_os_ordered_allowance_ns: u64,
  pub scv_boottime_os_ordered_decisive_wins: usize,
  pub vdso_boottime_allowance_ns: u64,
  pub vdso_boottime_decisive_wins: usize,
  pub direct_allowance_ns: u64,
  pub direct_decisive_wins: usize,
  pub required_decisive_wins: usize,
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct PowerWallSelectionMeasurements {
  pub instant: PowerWallDomainMeasurements,
  pub ordered: PowerWallDomainMeasurements,
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[doc(hidden)]
pub fn power_wall_selection_measurements() -> PowerWallSelectionMeasurements {
  fn domain(evidence: crate::arch::powerpc64::ProbeEvidence) -> PowerWallDomainMeasurements {
    PowerWallDomainMeasurements {
      candidate_count: evidence.candidate_count,
      scv_eligible: evidence.scv_eligible,
      clock_raw_available: evidence.clock_raw_available,
      sc_raw_available: evidence.sc_raw_available,
      scv_raw_available: evidence.scv_raw_available,
      vdso_available: evidence.vdso_available,
      vdso_raw_available: evidence.vdso_raw_available,
      clock_boottime_available: evidence.clock_boottime_available,
      sc_boottime_available: evidence.sc_boottime_available,
      scv_boottime_available: evidence.scv_boottime_available,
      vdso_boottime_available: evidence.vdso_boottime_available,
      selected_provider: evidence.selected_provider.name(),
      reads_per_batch: evidence.reads_per_batch,
      direct_batches_ns: evidence.direct_batches_ns,
      clock_batches_ns: evidence.clock_batches_ns,
      sc_batches_ns: evidence.sc_batches_ns,
      sc_os_ordered_batches_ns: evidence.sc_os_ordered_batches_ns,
      scv_batches_ns: evidence.scv_batches_ns,
      scv_os_ordered_batches_ns: evidence.scv_os_ordered_batches_ns,
      clock_raw_batches_ns: evidence.clock_raw_batches_ns,
      sc_raw_batches_ns: evidence.sc_raw_batches_ns,
      sc_raw_os_ordered_batches_ns: evidence.sc_raw_os_ordered_batches_ns,
      scv_raw_batches_ns: evidence.scv_raw_batches_ns,
      scv_raw_os_ordered_batches_ns: evidence.scv_raw_os_ordered_batches_ns,
      vdso_batches_ns: evidence.vdso_batches_ns,
      vdso_raw_batches_ns: evidence.vdso_raw_batches_ns,
      clock_boottime_batches_ns: evidence.clock_boottime_batches_ns,
      sc_boottime_batches_ns: evidence.sc_boottime_batches_ns,
      sc_boottime_os_ordered_batches_ns: evidence.sc_boottime_os_ordered_batches_ns,
      scv_boottime_batches_ns: evidence.scv_boottime_batches_ns,
      scv_boottime_os_ordered_batches_ns: evidence.scv_boottime_os_ordered_batches_ns,
      vdso_boottime_batches_ns: evidence.vdso_boottime_batches_ns,
      direct_median_ns: evidence.direct_median_ns,
      clock_median_ns: evidence.clock_median_ns,
      sc_median_ns: evidence.sc_median_ns,
      sc_os_ordered_median_ns: evidence.sc_os_ordered_median_ns,
      scv_median_ns: evidence.scv_median_ns,
      scv_os_ordered_median_ns: evidence.scv_os_ordered_median_ns,
      clock_raw_median_ns: evidence.clock_raw_median_ns,
      sc_raw_median_ns: evidence.sc_raw_median_ns,
      sc_raw_os_ordered_median_ns: evidence.sc_raw_os_ordered_median_ns,
      scv_raw_median_ns: evidence.scv_raw_median_ns,
      scv_raw_os_ordered_median_ns: evidence.scv_raw_os_ordered_median_ns,
      vdso_median_ns: evidence.vdso_median_ns,
      vdso_raw_median_ns: evidence.vdso_raw_median_ns,
      clock_boottime_median_ns: evidence.clock_boottime_median_ns,
      sc_boottime_median_ns: evidence.sc_boottime_median_ns,
      sc_boottime_os_ordered_median_ns: evidence.sc_boottime_os_ordered_median_ns,
      scv_boottime_median_ns: evidence.scv_boottime_median_ns,
      scv_boottime_os_ordered_median_ns: evidence.scv_boottime_os_ordered_median_ns,
      vdso_boottime_median_ns: evidence.vdso_boottime_median_ns,
      sc_allowance_ns: evidence.sc_allowance_ns,
      sc_decisive_wins: evidence.sc_decisive_wins,
      sc_os_ordered_allowance_ns: evidence.sc_os_ordered_allowance_ns,
      sc_os_ordered_decisive_wins: evidence.sc_os_ordered_decisive_wins,
      scv_allowance_ns: evidence.scv_allowance_ns,
      scv_decisive_wins: evidence.scv_decisive_wins,
      scv_os_ordered_allowance_ns: evidence.scv_os_ordered_allowance_ns,
      scv_os_ordered_decisive_wins: evidence.scv_os_ordered_decisive_wins,
      clock_raw_allowance_ns: evidence.clock_raw_allowance_ns,
      clock_raw_decisive_wins: evidence.clock_raw_decisive_wins,
      sc_raw_allowance_ns: evidence.sc_raw_allowance_ns,
      sc_raw_decisive_wins: evidence.sc_raw_decisive_wins,
      sc_raw_os_ordered_allowance_ns: evidence.sc_raw_os_ordered_allowance_ns,
      sc_raw_os_ordered_decisive_wins: evidence.sc_raw_os_ordered_decisive_wins,
      scv_raw_allowance_ns: evidence.scv_raw_allowance_ns,
      scv_raw_decisive_wins: evidence.scv_raw_decisive_wins,
      scv_raw_os_ordered_allowance_ns: evidence.scv_raw_os_ordered_allowance_ns,
      scv_raw_os_ordered_decisive_wins: evidence.scv_raw_os_ordered_decisive_wins,
      vdso_allowance_ns: evidence.vdso_allowance_ns,
      vdso_decisive_wins: evidence.vdso_decisive_wins,
      vdso_raw_allowance_ns: evidence.vdso_raw_allowance_ns,
      vdso_raw_decisive_wins: evidence.vdso_raw_decisive_wins,
      clock_boottime_allowance_ns: evidence.clock_boottime_allowance_ns,
      clock_boottime_decisive_wins: evidence.clock_boottime_decisive_wins,
      sc_boottime_allowance_ns: evidence.sc_boottime_allowance_ns,
      sc_boottime_decisive_wins: evidence.sc_boottime_decisive_wins,
      sc_boottime_os_ordered_allowance_ns: evidence.sc_boottime_os_ordered_allowance_ns,
      sc_boottime_os_ordered_decisive_wins: evidence.sc_boottime_os_ordered_decisive_wins,
      scv_boottime_allowance_ns: evidence.scv_boottime_allowance_ns,
      scv_boottime_decisive_wins: evidence.scv_boottime_decisive_wins,
      scv_boottime_os_ordered_allowance_ns: evidence.scv_boottime_os_ordered_allowance_ns,
      scv_boottime_os_ordered_decisive_wins: evidence.scv_boottime_os_ordered_decisive_wins,
      vdso_boottime_allowance_ns: evidence.vdso_boottime_allowance_ns,
      vdso_boottime_decisive_wins: evidence.vdso_boottime_decisive_wins,
      direct_allowance_ns: evidence.direct_allowance_ns,
      direct_decisive_wins: evidence.direct_decisive_wins,
      required_decisive_wins: evidence.required_decisive_wins,
    }
  }
  PowerWallSelectionMeasurements {
    instant: domain(crate::arch::powerpc64::bench_instant_evidence()),
    ordered: domain(crate::arch::powerpc64::bench_ordered_evidence()),
  }
}

#[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
#[doc(hidden)]
pub fn power_wall_selected_providers() -> [&'static str; 2] {
  [
    crate::arch::powerpc64::bench_instant_provider().name(),
    crate::arch::powerpc64::bench_ordered_provider().name(),
  ]
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[doc(hidden)]
pub struct LinuxX86TscDirect;

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
impl LinuxX86TscDirect {
  #[doc(hidden)]
  pub fn try_for_current_machine() -> Option<Self> {
    crate::arch::linux_x86_wall::bench_tsc_eligible().then_some(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    crate::arch::linux_x86_wall::bench_direct_tsc()
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ordered_ticks(&self) -> u64 {
    crate::arch::linux_x86_wall::bench_direct_tsc_ordered()
  }

  #[doc(hidden)]
  #[inline]
  pub fn elapsed_since(&self, start: u64) -> Duration {
    crate::instant::ticks_to_duration(self.now_ticks().saturating_sub(start))
  }

  #[doc(hidden)]
  #[inline]
  pub fn ordered_elapsed_since(&self, start: u64) -> Duration {
    crate::instant::ordered_ticks_to_duration(self.now_ordered_ticks().saturating_sub(start))
  }
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[doc(hidden)]
pub struct LinuxX86ClockMonotonicDirect;

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
impl LinuxX86ClockMonotonicDirect {
  #[doc(hidden)]
  pub fn for_current_machine() -> Self {
    Self
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    crate::arch::linux_x86_wall::bench_direct_clock_monotonic()
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ordered_ticks(&self) -> u64 {
    crate::arch::linux_x86_wall::bench_direct_clock_monotonic_ordered()
  }

  #[doc(hidden)]
  #[inline]
  pub fn elapsed_since(&self, start: u64) -> Duration {
    Duration::from_nanos(self.now_ticks().saturating_sub(start))
  }

  #[doc(hidden)]
  #[inline]
  pub fn ordered_elapsed_since(&self, start: u64) -> Duration {
    Duration::from_nanos(self.now_ordered_ticks().saturating_sub(start))
  }
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[doc(hidden)]
pub struct LinuxX86ClockMonotonicSyscallDirect;

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
impl LinuxX86ClockMonotonicSyscallDirect {
  #[doc(hidden)]
  pub fn for_current_machine() -> Self {
    Self
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    crate::arch::linux_x86_wall::bench_direct_clock_monotonic_syscall()
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ordered_ticks(&self) -> u64 {
    crate::arch::linux_x86_wall::bench_direct_clock_monotonic_syscall_ordered()
  }

  #[doc(hidden)]
  #[inline]
  pub fn elapsed_since(&self, start: u64) -> Duration {
    Duration::from_nanos(self.now_ticks().saturating_sub(start))
  }

  #[doc(hidden)]
  #[inline]
  pub fn ordered_elapsed_since(&self, start: u64) -> Duration {
    Duration::from_nanos(self.now_ordered_ticks().saturating_sub(start))
  }
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[doc(hidden)]
pub fn linux_x86_instant_selected_provider() -> &'static str {
  crate::arch::linux_x86_wall::bench_instant_provider()
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[doc(hidden)]
pub fn linux_x86_ordered_selected_provider() -> &'static str {
  crate::arch::linux_x86_wall::bench_ordered_provider()
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct LinuxX86WallSelectionMeasurements {
  pub instant_eligibility: &'static str,
  pub ordered_eligibility: &'static str,
  pub reads_per_batch: u64,
  pub required_decisive_wins: usize,
  pub instant_candidate_count: usize,
  pub instant_candidate_names: Vec<&'static str>,
  pub instant_candidate_eligible: Vec<bool>,
  pub instant_candidate_batches_ns: Vec<[u64; 9]>,
  pub instant_candidate_medians_ns: Vec<u64>,
  pub instant_tournament_decision_count: usize,
  pub instant_tournament_challengers: Vec<&'static str>,
  pub instant_tournament_incumbents: Vec<&'static str>,
  pub instant_tournament_winners: Vec<&'static str>,
  pub instant_tournament_allowances_ns: Vec<u64>,
  pub instant_tournament_decisive_wins: Vec<usize>,
  pub instant_tournament_challenger_selected: Vec<bool>,
  pub ordered_candidate_count: usize,
  pub ordered_candidate_names: Vec<&'static str>,
  pub ordered_candidate_eligible: Vec<bool>,
  pub ordered_candidate_batches_ns: Vec<[u64; 9]>,
  pub ordered_candidate_medians_ns: Vec<u64>,
  pub ordered_barrier_candidate_count: usize,
  pub ordered_barrier_candidate_names: [&'static str; 6],
  pub ordered_tournament_decision_count: usize,
  pub ordered_tournament_challengers: Vec<&'static str>,
  pub ordered_tournament_incumbents: Vec<&'static str>,
  pub ordered_tournament_winners: Vec<&'static str>,
  pub ordered_tournament_allowances_ns: Vec<u64>,
  pub ordered_tournament_decisive_wins: Vec<usize>,
  pub ordered_tournament_challenger_selected: Vec<bool>,
  pub tsc_batches_ns: [u64; 9],
  pub clock_batches_ns: [u64; 9],
  pub syscall_batches_ns: [u64; 9],
  pub ordered_tsc_batches_ns: [u64; 9],
  pub ordered_clock_batches_ns: [u64; 9],
  pub ordered_syscall_batches_ns: [u64; 9],
  pub tsc_median_ns: u64,
  pub clock_median_ns: u64,
  pub syscall_median_ns: u64,
  pub ordered_tsc_median_ns: u64,
  pub ordered_clock_median_ns: u64,
  pub ordered_syscall_median_ns: u64,
  pub instant_allowance_ns: u64,
  pub ordered_allowance_ns: u64,
  pub instant_decisive_wins: usize,
  pub ordered_decisive_wins: usize,
  pub instant_syscall_vs_clock_allowance_ns: u64,
  pub ordered_syscall_vs_clock_allowance_ns: u64,
  pub instant_syscall_vs_clock_decisive_wins: usize,
  pub ordered_syscall_vs_clock_decisive_wins: usize,
  pub instant_fallback_provider: &'static str,
  pub ordered_fallback_provider: &'static str,
  pub instant_selected_provider: &'static str,
  pub ordered_selected_provider: &'static str,
  pub ordered_os_barrier: &'static str,
  pub ordered_bare_syscall_os_owned_eligible: bool,
  pub ordered_bare_syscall_os_owned_basis: &'static str,
  pub ordered_fast_barrier_candidate: &'static str,
  pub ordered_baseline_barrier: &'static str,
  pub ordered_clock_fast_batches_ns: [u64; 9],
  pub ordered_clock_baseline_batches_ns: [u64; 9],
  pub ordered_syscall_fast_batches_ns: [u64; 9],
  pub ordered_syscall_baseline_batches_ns: [u64; 9],
  pub ordered_clock_barrier_allowance_ns: u64,
  pub ordered_clock_barrier_decisive_wins: usize,
  pub ordered_syscall_barrier_allowance_ns: u64,
  pub ordered_syscall_barrier_decisive_wins: usize,
  pub instant_tsc_selected: bool,
  pub ordered_tsc_selected: bool,
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[doc(hidden)]
pub fn linux_x86_wall_selection_measurements() -> LinuxX86WallSelectionMeasurements {
  let evidence = crate::arch::linux_x86_wall::bench_probe_evidence();
  LinuxX86WallSelectionMeasurements {
    instant_eligibility: evidence.instant_eligibility,
    ordered_eligibility: evidence.ordered_eligibility,
    reads_per_batch: evidence.reads_per_batch,
    required_decisive_wins: evidence.required_decisive_wins,
    instant_candidate_count: evidence.instant_candidate_count,
    instant_candidate_names: evidence.instant_candidate_names.to_vec(),
    instant_candidate_eligible: evidence.instant_candidate_eligible.to_vec(),
    instant_candidate_batches_ns: evidence.instant_candidate_batches_ns.to_vec(),
    instant_candidate_medians_ns: evidence.instant_candidate_medians_ns.to_vec(),
    instant_tournament_decision_count: evidence.instant_tournament_decision_count,
    instant_tournament_challengers: evidence.instant_tournament_challengers.to_vec(),
    instant_tournament_incumbents: evidence.instant_tournament_incumbents.to_vec(),
    instant_tournament_winners: evidence.instant_tournament_winners.to_vec(),
    instant_tournament_allowances_ns: evidence.instant_tournament_allowances_ns.to_vec(),
    instant_tournament_decisive_wins: evidence.instant_tournament_decisive_wins.to_vec(),
    instant_tournament_challenger_selected: evidence
      .instant_tournament_challenger_selected
      .to_vec(),
    ordered_candidate_count: evidence.ordered_candidate_count,
    ordered_candidate_names: evidence.ordered_candidate_names.to_vec(),
    ordered_candidate_eligible: evidence.ordered_candidate_eligible.to_vec(),
    ordered_candidate_batches_ns: evidence.ordered_candidate_batches_ns.to_vec(),
    ordered_candidate_medians_ns: evidence.ordered_candidate_medians_ns.to_vec(),
    ordered_barrier_candidate_count: evidence.ordered_barrier_candidate_count,
    ordered_barrier_candidate_names: evidence.ordered_barrier_candidate_names,
    ordered_tournament_decision_count: evidence.ordered_tournament_decision_count,
    ordered_tournament_challengers: evidence.ordered_tournament_challengers.to_vec(),
    ordered_tournament_incumbents: evidence.ordered_tournament_incumbents.to_vec(),
    ordered_tournament_winners: evidence.ordered_tournament_winners.to_vec(),
    ordered_tournament_allowances_ns: evidence.ordered_tournament_allowances_ns.to_vec(),
    ordered_tournament_decisive_wins: evidence.ordered_tournament_decisive_wins.to_vec(),
    ordered_tournament_challenger_selected: evidence
      .ordered_tournament_challenger_selected
      .to_vec(),
    tsc_batches_ns: evidence.tsc_batches_ns,
    clock_batches_ns: evidence.clock_batches_ns,
    syscall_batches_ns: evidence.syscall_batches_ns,
    ordered_tsc_batches_ns: evidence.ordered_tsc_batches_ns,
    ordered_clock_batches_ns: evidence.ordered_clock_batches_ns,
    ordered_syscall_batches_ns: evidence.ordered_syscall_batches_ns,
    tsc_median_ns: evidence.tsc_median_ns,
    clock_median_ns: evidence.clock_median_ns,
    syscall_median_ns: evidence.syscall_median_ns,
    ordered_tsc_median_ns: evidence.ordered_tsc_median_ns,
    ordered_clock_median_ns: evidence.ordered_clock_median_ns,
    ordered_syscall_median_ns: evidence.ordered_syscall_median_ns,
    instant_allowance_ns: evidence.instant_allowance_ns,
    ordered_allowance_ns: evidence.ordered_allowance_ns,
    instant_decisive_wins: evidence.instant_decisive_wins,
    ordered_decisive_wins: evidence.ordered_decisive_wins,
    instant_syscall_vs_clock_allowance_ns: evidence.instant_syscall_vs_clock_allowance_ns,
    ordered_syscall_vs_clock_allowance_ns: evidence.ordered_syscall_vs_clock_allowance_ns,
    instant_syscall_vs_clock_decisive_wins: evidence.instant_syscall_vs_clock_decisive_wins,
    ordered_syscall_vs_clock_decisive_wins: evidence.ordered_syscall_vs_clock_decisive_wins,
    instant_fallback_provider: evidence.instant_fallback_provider,
    ordered_fallback_provider: evidence.ordered_fallback_provider,
    instant_selected_provider: evidence.instant_selected_provider,
    ordered_selected_provider: evidence.ordered_selected_provider,
    ordered_os_barrier: evidence.ordered_os_barrier,
    ordered_bare_syscall_os_owned_eligible: evidence.ordered_bare_syscall_os_owned_eligible,
    ordered_bare_syscall_os_owned_basis: evidence.ordered_bare_syscall_os_owned_basis,
    ordered_fast_barrier_candidate: evidence.ordered_fast_barrier_candidate,
    ordered_baseline_barrier: evidence.ordered_baseline_barrier,
    ordered_clock_fast_batches_ns: evidence.ordered_clock_fast_batches_ns,
    ordered_clock_baseline_batches_ns: evidence.ordered_clock_baseline_batches_ns,
    ordered_syscall_fast_batches_ns: evidence.ordered_syscall_fast_batches_ns,
    ordered_syscall_baseline_batches_ns: evidence.ordered_syscall_baseline_batches_ns,
    ordered_clock_barrier_allowance_ns: evidence.ordered_clock_barrier_allowance_ns,
    ordered_clock_barrier_decisive_wins: evidence.ordered_clock_barrier_decisive_wins,
    ordered_syscall_barrier_allowance_ns: evidence.ordered_syscall_barrier_allowance_ns,
    ordered_syscall_barrier_decisive_wins: evidence.ordered_syscall_barrier_decisive_wins,
    instant_tsc_selected: evidence.instant_tsc_selected,
    ordered_tsc_selected: evidence.ordered_tsc_selected,
  }
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
pub struct LinuxAarch64CntvctDirect;

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
impl LinuxAarch64CntvctDirect {
  #[doc(hidden)]
  pub fn try_for_current_machine() -> Option<Self> {
    crate::arch::linux_aarch64_wall::bench_counter_eligible().then_some(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    crate::arch::linux_aarch64_wall::bench_direct_cntvct()
  }
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
pub struct LinuxAarch64ClockMonotonicDirect;

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
impl LinuxAarch64ClockMonotonicDirect {
  #[doc(hidden)]
  pub fn for_current_machine() -> Self {
    Self
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    crate::arch::linux_aarch64_wall::bench_direct_clock_monotonic()
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ordered_ticks(&self) -> u64 {
    crate::arch::linux_aarch64_wall::bench_direct_clock_monotonic_ordered()
  }
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
pub struct LinuxAarch64ClockMonotonicSyscallDirect;

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
impl LinuxAarch64ClockMonotonicSyscallDirect {
  #[doc(hidden)]
  pub fn for_current_machine() -> Self {
    Self
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    crate::arch::linux_aarch64_wall::bench_direct_clock_monotonic_syscall()
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ordered_ticks(&self) -> u64 {
    crate::arch::linux_aarch64_wall::bench_direct_clock_monotonic_syscall()
  }
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
pub fn linux_aarch64_instant_selected_provider() -> &'static str {
  crate::arch::linux_aarch64_wall::bench_instant_provider()
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
pub fn linux_aarch64_ordered_selected_provider() -> &'static str {
  crate::arch::linux_aarch64_wall::bench_ordered_provider()
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct WallTournamentStepMeasurements {
  pub challenger: &'static str,
  pub incumbent: &'static str,
  pub allowance_ns: u64,
  pub decisive_wins: usize,
  pub challenger_selected: bool,
  pub winner: &'static str,
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
fn aarch64_tournament_step(
  evidence: crate::arch::linux_aarch64_wall::TournamentStepEvidence,
) -> WallTournamentStepMeasurements {
  WallTournamentStepMeasurements {
    challenger: evidence.challenger,
    incumbent: evidence.incumbent,
    allowance_ns: evidence.allowance_ns,
    decisive_wins: evidence.decisive_wins,
    challenger_selected: evidence.challenger_selected,
    winner: evidence.winner,
  }
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct LinuxAarch64InstantSelectionMeasurements {
  pub eligibility: &'static str,
  pub permission_basis: &'static str,
  pub pr_get_tsc_status: i64,
  pub kernel_version_known: bool,
  pub kernel_version_major: u32,
  pub kernel_version_minor: u32,
  pub reads_per_batch: u64,
  pub candidate_count: usize,
  pub vdso_available: bool,
  pub vdso_raw_available: bool,
  pub vdso_boottime_available: bool,
  pub cntvct_batches_ns: [u64; 9],
  pub clock_batches_ns: [u64; 9],
  pub clock_raw_batches_ns: [u64; 9],
  pub clock_boottime_batches_ns: [u64; 9],
  pub syscall_batches_ns: [u64; 9],
  pub syscall_raw_batches_ns: [u64; 9],
  pub syscall_boottime_batches_ns: [u64; 9],
  pub vdso_batches_ns: [u64; 9],
  pub vdso_raw_batches_ns: [u64; 9],
  pub vdso_boottime_batches_ns: [u64; 9],
  pub cntvct_median_ns: u64,
  pub clock_median_ns: u64,
  pub clock_raw_median_ns: u64,
  pub clock_boottime_median_ns: u64,
  pub syscall_median_ns: u64,
  pub syscall_raw_median_ns: u64,
  pub syscall_boottime_median_ns: u64,
  pub vdso_median_ns: u64,
  pub vdso_raw_median_ns: u64,
  pub vdso_boottime_median_ns: u64,
  pub fallback_provider: &'static str,
  pub direct_allowance_ns: u64,
  pub direct_decisive_wins: usize,
  pub syscall_vs_clock_allowance_ns: u64,
  pub syscall_vs_clock_decisive_wins: usize,
  pub tournament_step_count: usize,
  pub tournament_steps: Vec<WallTournamentStepMeasurements>,
  pub required_decisive_wins: usize,
  pub selected_provider: &'static str,
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
pub fn linux_aarch64_instant_selection_measurements() -> LinuxAarch64InstantSelectionMeasurements {
  let evidence = crate::arch::linux_aarch64_wall::bench_instant_evidence();
  LinuxAarch64InstantSelectionMeasurements {
    eligibility: evidence.eligibility,
    permission_basis: evidence.permission_basis,
    pr_get_tsc_status: evidence.pr_get_tsc_status,
    kernel_version_known: evidence.kernel_version_known,
    kernel_version_major: evidence.kernel_version_major,
    kernel_version_minor: evidence.kernel_version_minor,
    reads_per_batch: evidence.reads_per_batch,
    candidate_count: evidence.candidate_count,
    vdso_available: evidence.vdso_available,
    vdso_raw_available: evidence.vdso_raw_available,
    vdso_boottime_available: evidence.vdso_boottime_available,
    cntvct_batches_ns: evidence.cntvct_batches_ns,
    clock_batches_ns: evidence.clock_batches_ns,
    clock_raw_batches_ns: evidence.clock_raw_batches_ns,
    clock_boottime_batches_ns: evidence.clock_boottime_batches_ns,
    syscall_batches_ns: evidence.syscall_batches_ns,
    syscall_raw_batches_ns: evidence.syscall_raw_batches_ns,
    syscall_boottime_batches_ns: evidence.syscall_boottime_batches_ns,
    vdso_batches_ns: evidence.vdso_batches_ns,
    vdso_raw_batches_ns: evidence.vdso_raw_batches_ns,
    vdso_boottime_batches_ns: evidence.vdso_boottime_batches_ns,
    cntvct_median_ns: evidence.cntvct_median_ns,
    clock_median_ns: evidence.clock_median_ns,
    clock_raw_median_ns: evidence.clock_raw_median_ns,
    clock_boottime_median_ns: evidence.clock_boottime_median_ns,
    syscall_median_ns: evidence.syscall_median_ns,
    syscall_raw_median_ns: evidence.syscall_raw_median_ns,
    syscall_boottime_median_ns: evidence.syscall_boottime_median_ns,
    vdso_median_ns: evidence.vdso_median_ns,
    vdso_raw_median_ns: evidence.vdso_raw_median_ns,
    vdso_boottime_median_ns: evidence.vdso_boottime_median_ns,
    fallback_provider: evidence.fallback_provider,
    direct_allowance_ns: evidence.direct_allowance_ns,
    direct_decisive_wins: evidence.direct_decisive_wins,
    syscall_vs_clock_allowance_ns: evidence.syscall_vs_clock_allowance_ns,
    syscall_vs_clock_decisive_wins: evidence.syscall_vs_clock_decisive_wins,
    tournament_step_count: evidence.tournament_step_count,
    tournament_steps: evidence.tournament_steps.into_iter().map(aarch64_tournament_step).collect(),
    required_decisive_wins: evidence.required_decisive_wins,
    selected_provider: evidence.selected_provider,
  }
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct LinuxAarch64OrderedSelectionMeasurements {
  pub eligibility: &'static str,
  pub permission_basis: &'static str,
  pub pr_get_tsc_status: i64,
  pub kernel_version_known: bool,
  pub kernel_version_major: u32,
  pub kernel_version_minor: u32,
  pub reads_per_batch: u64,
  pub candidate_count: usize,
  pub vdso_available: bool,
  pub vdso_raw_available: bool,
  pub vdso_boottime_available: bool,
  pub hwcap_ecv: bool,
  pub hwcap_sb: bool,
  pub sb_eligibility: &'static str,
  pub isb_batches_ns: [u64; 9],
  pub cntvctss_batches_ns: [u64; 9],
  pub clock_batches_ns: [u64; 9],
  pub clock_raw_batches_ns: [u64; 9],
  pub clock_boottime_batches_ns: [u64; 9],
  pub syscall_batches_ns: [u64; 9],
  pub syscall_raw_batches_ns: [u64; 9],
  pub syscall_boottime_batches_ns: [u64; 9],
  pub vdso_batches_ns: [u64; 9],
  pub vdso_raw_batches_ns: [u64; 9],
  pub vdso_boottime_batches_ns: [u64; 9],
  pub direct_provider: &'static str,
  pub fallback_provider: &'static str,
  pub direct_median_ns: u64,
  pub clock_median_ns: u64,
  pub clock_raw_median_ns: u64,
  pub clock_boottime_median_ns: u64,
  pub syscall_median_ns: u64,
  pub syscall_raw_median_ns: u64,
  pub syscall_boottime_median_ns: u64,
  pub vdso_median_ns: u64,
  pub vdso_raw_median_ns: u64,
  pub vdso_boottime_median_ns: u64,
  pub direct_allowance_ns: u64,
  pub direct_decisive_wins: usize,
  pub cntvctss_vs_isb_allowance_ns: u64,
  pub cntvctss_vs_isb_decisive_wins: usize,
  pub syscall_vs_clock_allowance_ns: u64,
  pub syscall_vs_clock_decisive_wins: usize,
  pub tournament_step_count: usize,
  pub tournament_steps: Vec<WallTournamentStepMeasurements>,
  pub required_decisive_wins: usize,
  pub selected_provider: &'static str,
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
pub fn linux_aarch64_ordered_selection_measurements() -> LinuxAarch64OrderedSelectionMeasurements {
  let evidence = crate::arch::linux_aarch64_wall::bench_ordered_evidence();
  LinuxAarch64OrderedSelectionMeasurements {
    eligibility: evidence.eligibility,
    permission_basis: evidence.permission_basis,
    pr_get_tsc_status: evidence.pr_get_tsc_status,
    kernel_version_known: evidence.kernel_version_known,
    kernel_version_major: evidence.kernel_version_major,
    kernel_version_minor: evidence.kernel_version_minor,
    reads_per_batch: evidence.reads_per_batch,
    candidate_count: evidence.candidate_count,
    vdso_available: evidence.vdso_available,
    vdso_raw_available: evidence.vdso_raw_available,
    vdso_boottime_available: evidence.vdso_boottime_available,
    hwcap_ecv: evidence.hwcap_ecv,
    hwcap_sb: evidence.hwcap_sb,
    sb_eligibility: evidence.sb_eligibility,
    isb_batches_ns: evidence.isb_batches_ns,
    cntvctss_batches_ns: evidence.cntvctss_batches_ns,
    clock_batches_ns: evidence.clock_batches_ns,
    clock_raw_batches_ns: evidence.clock_raw_batches_ns,
    clock_boottime_batches_ns: evidence.clock_boottime_batches_ns,
    syscall_batches_ns: evidence.syscall_batches_ns,
    syscall_raw_batches_ns: evidence.syscall_raw_batches_ns,
    syscall_boottime_batches_ns: evidence.syscall_boottime_batches_ns,
    vdso_batches_ns: evidence.vdso_batches_ns,
    vdso_raw_batches_ns: evidence.vdso_raw_batches_ns,
    vdso_boottime_batches_ns: evidence.vdso_boottime_batches_ns,
    direct_provider: evidence.direct_provider,
    fallback_provider: evidence.fallback_provider,
    direct_median_ns: evidence.direct_median_ns,
    clock_median_ns: evidence.clock_median_ns,
    clock_raw_median_ns: evidence.clock_raw_median_ns,
    clock_boottime_median_ns: evidence.clock_boottime_median_ns,
    syscall_median_ns: evidence.syscall_median_ns,
    syscall_raw_median_ns: evidence.syscall_raw_median_ns,
    syscall_boottime_median_ns: evidence.syscall_boottime_median_ns,
    vdso_median_ns: evidence.vdso_median_ns,
    vdso_raw_median_ns: evidence.vdso_raw_median_ns,
    vdso_boottime_median_ns: evidence.vdso_boottime_median_ns,
    direct_allowance_ns: evidence.direct_allowance_ns,
    direct_decisive_wins: evidence.direct_decisive_wins,
    cntvctss_vs_isb_allowance_ns: evidence.cntvctss_vs_isb_allowance_ns,
    cntvctss_vs_isb_decisive_wins: evidence.cntvctss_vs_isb_decisive_wins,
    syscall_vs_clock_allowance_ns: evidence.syscall_vs_clock_allowance_ns,
    syscall_vs_clock_decisive_wins: evidence.syscall_vs_clock_decisive_wins,
    tournament_step_count: evidence.tournament_step_count,
    tournament_steps: evidence.tournament_steps.into_iter().map(aarch64_tournament_step).collect(),
    required_decisive_wins: evidence.required_decisive_wins,
    selected_provider: evidence.selected_provider,
  }
}

#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(all(target_arch = "x86_64", target_os = "macos")),
))]
#[doc(hidden)]
pub struct OrderedX86CpuidDirect;

#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(all(target_arch = "x86_64", target_os = "macos")),
))]
impl OrderedX86CpuidDirect {
  #[doc(hidden)]
  pub fn try_for_current_machine() -> Option<Self> {
    #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
    if !crate::arch::freebsd_x86_64::bench_tsc_eligible() {
      return None;
    }
    #[cfg(all(
      any(target_os = "android", target_os = "linux"),
      any(target_arch = "x86_64", target_arch = "x86"),
    ))]
    if !crate::arch::linux_x86_wall::bench_tsc_eligible() {
      return None;
    }
    Some(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
      crate::arch::x86_64::bench_cpuid_rdtsc()
    }
    #[cfg(target_arch = "x86")]
    {
      crate::arch::x86::bench_cpuid_rdtsc()
    }
  }
}

#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(all(target_arch = "x86_64", target_os = "macos")),
))]
#[doc(hidden)]
pub struct OrderedX86LfenceDirect;

#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(all(target_arch = "x86_64", target_os = "macos")),
))]
impl OrderedX86LfenceDirect {
  #[doc(hidden)]
  pub fn try_for_current_machine() -> Option<Self> {
    #[cfg(target_arch = "x86_64")]
    let eligible = crate::arch::x86_64::bench_ordered_eligibility().0;
    #[cfg(target_arch = "x86")]
    let eligible = crate::arch::x86::bench_ordered_eligibility().0;
    #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
    let eligible = eligible && crate::arch::freebsd_x86_64::bench_tsc_eligible();
    #[cfg(all(
      any(target_os = "android", target_os = "linux"),
      any(target_arch = "x86_64", target_arch = "x86"),
    ))]
    let eligible = eligible && crate::arch::linux_x86_wall::bench_tsc_eligible();
    eligible.then_some(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
      crate::arch::x86_64::bench_lfence_rdtsc()
    }
    #[cfg(target_arch = "x86")]
    {
      crate::arch::x86::bench_lfence_rdtsc()
    }
  }
}

#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(all(target_arch = "x86_64", target_os = "macos")),
))]
#[doc(hidden)]
pub struct OrderedX86MfenceDirect;

#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(all(target_arch = "x86_64", target_os = "macos")),
))]
impl OrderedX86MfenceDirect {
  #[doc(hidden)]
  pub fn try_for_current_machine() -> Option<Self> {
    #[cfg(target_arch = "x86_64")]
    let eligible = crate::arch::x86_64::bench_ordered_eligibility().2;
    #[cfg(target_arch = "x86")]
    let eligible = crate::arch::x86::bench_ordered_eligibility().2;
    #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
    let eligible = eligible && crate::arch::freebsd_x86_64::bench_tsc_eligible();
    #[cfg(all(
      any(target_os = "android", target_os = "linux"),
      any(target_arch = "x86_64", target_arch = "x86"),
    ))]
    let eligible = eligible && crate::arch::linux_x86_wall::bench_tsc_eligible();
    eligible.then_some(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
      crate::arch::x86_64::bench_mfence_rdtsc()
    }
    #[cfg(target_arch = "x86")]
    {
      crate::arch::x86::bench_mfence_rdtsc()
    }
  }
}

#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(all(target_arch = "x86_64", target_os = "macos")),
))]
#[doc(hidden)]
pub struct OrderedX86RdtscpDirect;

#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(all(target_arch = "x86_64", target_os = "macos")),
))]
impl OrderedX86RdtscpDirect {
  #[doc(hidden)]
  pub fn try_for_current_machine() -> Option<Self> {
    #[cfg(target_arch = "x86_64")]
    let eligible = crate::arch::x86_64::bench_ordered_eligibility().1;
    #[cfg(target_arch = "x86")]
    let eligible = crate::arch::x86::bench_ordered_eligibility().1;
    #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
    let eligible = eligible && crate::arch::freebsd_x86_64::bench_tsc_eligible();
    #[cfg(all(
      any(target_os = "android", target_os = "linux"),
      any(target_arch = "x86_64", target_arch = "x86"),
    ))]
    let eligible = eligible && crate::arch::linux_x86_wall::bench_tsc_eligible();
    eligible.then_some(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
      crate::arch::x86_64::bench_rdtscp()
    }
    #[cfg(target_arch = "x86")]
    {
      crate::arch::x86::bench_rdtscp()
    }
  }
}

#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(all(target_arch = "x86_64", target_os = "macos")),
))]
#[doc(hidden)]
pub struct OrderedX86SerializeDirect;

#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(all(target_arch = "x86_64", target_os = "macos")),
))]
impl OrderedX86SerializeDirect {
  #[doc(hidden)]
  pub fn try_for_current_machine() -> Option<Self> {
    #[cfg(target_arch = "x86_64")]
    let eligible = crate::arch::x86_64::bench_ordered_eligibility().3;
    #[cfg(target_arch = "x86")]
    let eligible = crate::arch::x86::bench_ordered_eligibility().3;
    #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
    let eligible = eligible && crate::arch::freebsd_x86_64::bench_tsc_eligible();
    #[cfg(all(
      any(target_os = "android", target_os = "linux"),
      any(target_arch = "x86_64", target_arch = "x86"),
    ))]
    let eligible = eligible && crate::arch::linux_x86_wall::bench_tsc_eligible();
    eligible.then_some(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
      crate::arch::x86_64::bench_serialize_rdtsc()
    }
    #[cfg(target_arch = "x86")]
    {
      crate::arch::x86::bench_serialize_rdtsc()
    }
  }
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
pub struct OrderedAarch64IsbDirect;

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
impl OrderedAarch64IsbDirect {
  #[doc(hidden)]
  pub fn try_for_current_machine() -> Option<Self> {
    crate::arch::linux_aarch64_wall::bench_counter_eligible().then_some(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    crate::arch::aarch64::bench_cntvct_after_isb()
  }
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
pub struct OrderedAarch64CntvctssDirect;

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
impl OrderedAarch64CntvctssDirect {
  #[doc(hidden)]
  pub fn try_for_current_machine() -> Option<Self> {
    (crate::arch::linux_aarch64_wall::bench_counter_eligible()
      && crate::arch::aarch64::bench_cntvctss_capable())
    .then_some(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    crate::arch::aarch64::bench_cntvctss()
  }
}

#[cfg(target_os = "macos")]
#[doc(hidden)]
pub struct MachAbsoluteTimeDirect {
  nanos_per_tick_q32: u64,
}

#[cfg(target_os = "macos")]
#[inline]
fn apple_mach_nanos_per_tick_q32() -> u64 {
  let (numer, denom) = crate::arch::fallback::mach_timebase();
  crate::arch::scale_from_ratio(u64::from(numer), u64::from(denom))
}

#[cfg(target_os = "macos")]
impl MachAbsoluteTimeDirect {
  #[doc(hidden)]
  pub fn for_current_machine() -> Self {
    Self { nanos_per_tick_q32: apple_mach_nanos_per_tick_q32() }
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    crate::arch::fallback::mach_time()
  }

  #[doc(hidden)]
  #[inline]
  pub fn elapsed_since(&self, earlier: u64) -> Duration {
    exact_ticks_to_duration_with_scale(
      self.now_ticks().saturating_sub(earlier),
      self.nanos_per_tick_q32,
    )
  }
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
pub struct AppleX86CommpageDirect;

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
impl AppleX86CommpageDirect {
  #[doc(hidden)]
  pub fn try_for_current_machine() -> Option<Self> {
    crate::arch::apple_x86_64::bench_commpage_eligible().then_some(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    crate::arch::apple_x86_64::bench_commpage_nanotime()
  }

  #[doc(hidden)]
  #[inline]
  pub fn elapsed_since(&self, earlier: u64) -> Duration {
    exact_ticks_to_duration_with_scale(self.now_ticks().saturating_sub(earlier), 1_u64 << 32)
  }

  #[doc(hidden)]
  pub fn provider(&self) -> &'static str {
    "apple_commpage_lfence_rdtsc_nanotime"
  }
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
pub struct AppleX86TscDirect;

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
impl AppleX86TscDirect {
  #[doc(hidden)]
  pub fn try_for_current_machine() -> Option<Self> {
    crate::arch::apple_x86_64::bench_tsc_eligible().then_some(Self)
  }

  #[doc(hidden)]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub fn now_ticks(&self) -> u64 {
    crate::arch::apple_x86_64::bench_tsc()
  }

  #[doc(hidden)]
  pub fn nanos_per_tick_q32(&self) -> u64 {
    crate::arch::apple_x86_64::bench_tsc_nanos_per_tick_q32()
  }

  #[doc(hidden)]
  pub fn provider(&self) -> &'static str {
    "apple_invariant_rdtsc"
  }
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn apple_x86_selected_ticks() -> u64 {
  crate::arch::apple_x86_64::bench_selected_ticks()
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_x86_selected_nanos_per_tick_q32() -> u64 {
  crate::arch::apple_x86_64::instant_nanos_per_tick_q32()
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn apple_x86_selected_ordered_ticks() -> u64 {
  crate::arch::apple_x86_64::bench_selected_ordered_ticks()
}

#[cfg(target_os = "windows")]
#[doc(hidden)]
pub fn windows_wall_selected_provider() -> &'static str {
  crate::arch::fallback::bench_instant_provider()
}

#[cfg(target_os = "windows")]
#[doc(hidden)]
pub fn windows_ordered_wall_selected_provider() -> &'static str {
  crate::arch::fallback::bench_ordered_provider()
}

#[cfg(target_os = "windows")]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct WindowsWallSelectionMeasurements {
  pub reads_per_batch: u64,
  pub required_decisive_wins: usize,
  pub instant_candidate_count: usize,
  pub instant_candidate_names: Vec<&'static str>,
  pub instant_candidate_batches_ns: Vec<[u64; 9]>,
  pub instant_candidate_medians_ns: Vec<u64>,
  pub ordered_candidate_count: usize,
  pub ordered_candidate_names: Vec<&'static str>,
  pub ordered_candidate_batches_ns: Vec<[u64; 9]>,
  pub ordered_candidate_medians_ns: Vec<u64>,
  pub instant_selected_provider: &'static str,
  pub ordered_selected_provider: &'static str,
  pub interrupt_time_precise_available: bool,
  pub unbiased_interrupt_time_precise_available: bool,
  pub raw_architectural_counter_eligible: bool,
  pub raw_architectural_counter_exclusion: &'static str,
  pub coarse_clock_eligible: bool,
  pub coarse_clock_exclusion: &'static str,
  pub utc_clock_eligible: bool,
  pub utc_clock_exclusion: &'static str,
  pub auxiliary_counter_eligible: bool,
  pub auxiliary_counter_exclusion: &'static str,
}

#[cfg(target_os = "windows")]
#[doc(hidden)]
pub fn windows_wall_selection_measurements() -> WindowsWallSelectionMeasurements {
  let evidence = crate::arch::fallback::bench_windows_wall_probe_evidence();
  let instant_count = evidence.instant_candidate_count;
  let ordered_count = evidence.ordered_candidate_count;
  WindowsWallSelectionMeasurements {
    reads_per_batch: evidence.reads_per_batch,
    required_decisive_wins: evidence.required_decisive_wins,
    instant_candidate_count: instant_count,
    instant_candidate_names: evidence.instant_candidate_names[..instant_count].to_vec(),
    instant_candidate_batches_ns: evidence.instant_candidate_batches_ns[..instant_count].to_vec(),
    instant_candidate_medians_ns: evidence.instant_candidate_medians_ns[..instant_count].to_vec(),
    ordered_candidate_count: ordered_count,
    ordered_candidate_names: evidence.ordered_candidate_names[..ordered_count].to_vec(),
    ordered_candidate_batches_ns: evidence.ordered_candidate_batches_ns[..ordered_count].to_vec(),
    ordered_candidate_medians_ns: evidence.ordered_candidate_medians_ns[..ordered_count].to_vec(),
    instant_selected_provider: evidence.instant_selected_provider,
    ordered_selected_provider: evidence.ordered_selected_provider,
    interrupt_time_precise_available: evidence.interrupt_time_precise_available,
    unbiased_interrupt_time_precise_available: evidence.unbiased_interrupt_time_precise_available,
    raw_architectural_counter_eligible: evidence.raw_architectural_counter_eligible,
    raw_architectural_counter_exclusion: evidence.raw_architectural_counter_exclusion,
    coarse_clock_eligible: evidence.coarse_clock_eligible,
    coarse_clock_exclusion: evidence.coarse_clock_exclusion,
    utc_clock_eligible: evidence.utc_clock_eligible,
    utc_clock_exclusion: evidence.utc_clock_exclusion,
    auxiliary_counter_eligible: evidence.auxiliary_counter_eligible,
    auxiliary_counter_exclusion: evidence.auxiliary_counter_exclusion,
  }
}

#[cfg(target_os = "windows")]
#[doc(hidden)]
pub fn windows_wall_candidate_providers() -> Vec<&'static str> {
  let (primitives, count) = crate::arch::fallback::bench_instant_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      primitive
        .expect("eligible Windows wall candidate must have an exact reader")
        .name
    })
    .collect()
}

#[cfg(target_os = "windows")]
#[doc(hidden)]
pub fn windows_ordered_wall_candidate_providers() -> Vec<&'static str> {
  let (primitives, count) = crate::arch::fallback::bench_ordered_candidate_primitives();
  primitives
    .into_iter()
    .take(count)
    .map(|primitive| {
      primitive
        .expect("eligible Windows ordered wall candidate must have an exact reader")
        .name
    })
    .collect()
}

#[cfg(target_os = "windows")]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn windows_qpc_ticks() -> u64 {
  crate::arch::fallback::bench_instant_qpc()
}

#[cfg(target_os = "windows")]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn windows_interrupt_time_precise_ticks() -> u64 {
  crate::arch::fallback::bench_instant_interrupt_time_precise()
}

#[cfg(target_os = "windows")]
#[doc(hidden)]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn windows_unbiased_interrupt_time_precise_ticks() -> u64 {
  crate::arch::fallback::bench_instant_unbiased_interrupt_time_precise()
}

#[cfg(target_os = "windows")]
#[doc(hidden)]
pub fn windows_qpc_delta_to_duration(ticks: u64) -> Duration {
  let nanos = u128::from(ticks).saturating_mul(1_000_000_000)
    / u128::from(crate::arch::fallback::qpc_frequency().max(1));
  Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

#[cfg(target_os = "windows")]
#[doc(hidden)]
pub fn windows_precise_delta_to_duration(ticks: u64) -> Duration {
  Duration::from_nanos(ticks.saturating_mul(100))
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_wall_selected_provider() -> &'static str {
  crate::arch::apple_aarch64::bench_instant_provider()
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_ordered_wall_selected_provider() -> &'static str {
  crate::arch::apple_aarch64::bench_provider()
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct AppleAarch64WallCandidateMeasurements {
  pub provider: &'static str,
  pub batches_ticks: [u64; 9],
  pub median_ticks: u64,
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct AppleAarch64WallSelectionMeasurements {
  pub ready: bool,
  pub user_timebase_mode: u8,
  pub continuous_hwclock: bool,
  pub reads_per_batch: u64,
  pub candidate_count: usize,
  pub candidates: [AppleAarch64WallCandidateMeasurements; 4],
  pub required_decisive_wins: usize,
  pub equivalence_floor_ticks_per_batch: u64,
  pub equivalence_relative_denominator: u64,
  pub measured_winner: &'static str,
  pub selected_provider: &'static str,
  pub selection_basis: &'static str,
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn apple_aarch64_selection_measurements(
  evidence: crate::arch::apple_aarch64::SelectionEvidence,
) -> AppleAarch64WallSelectionMeasurements {
  let (numer, denom) = crate::arch::fallback::mach_timebase();
  let floor_ticks = (u128::from(evidence.reads_per_batch) * u128::from(denom))
    .div_ceil(u128::from(numer))
    .min(u128::from(u64::MAX)) as u64;
  AppleAarch64WallSelectionMeasurements {
    ready: evidence.ready,
    user_timebase_mode: evidence.user_timebase_mode,
    continuous_hwclock: evidence.continuous_hwclock,
    reads_per_batch: evidence.reads_per_batch,
    candidate_count: evidence.candidate_count,
    candidates: evidence.candidates.map(|candidate| AppleAarch64WallCandidateMeasurements {
      provider: candidate.name,
      batches_ticks: candidate.batches_ticks,
      median_ticks: candidate.median_ticks,
    }),
    required_decisive_wins: evidence.required_decisive_wins,
    equivalence_floor_ticks_per_batch: floor_ticks,
    equivalence_relative_denominator: 20,
    measured_winner: evidence.measured_winner,
    selected_provider: evidence.selected_provider,
    selection_basis: evidence.selection_basis,
  }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_aarch64_instant_selection_measurements() -> AppleAarch64WallSelectionMeasurements {
  apple_aarch64_selection_measurements(crate::arch::apple_aarch64::bench_instant_evidence())
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_aarch64_ordered_selection_measurements() -> AppleAarch64WallSelectionMeasurements {
  apple_aarch64_selection_measurements(crate::arch::apple_aarch64::bench_ordered_evidence())
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_wall_selected_provider() -> &'static str {
  crate::arch::apple_x86_64::bench_instant_provider()
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_ordered_wall_selected_provider() -> &'static str {
  crate::arch::apple_x86_64::bench_ordered_provider()
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct AppleX86InstantSelectionMeasurements {
  pub reads_per_batch: u64,
  pub tsc_eligible: bool,
  pub tsc_eligibility_basis: &'static str,
  pub translated: bool,
  pub mach_absolute_time_batches_ticks: [u64; 9],
  pub tsc_batches_ticks: [u64; 9],
  pub mach_absolute_time_median_ticks: u64,
  pub tsc_median_ticks: u64,
  pub allowance_total_ticks: u64,
  pub decisive_wins: usize,
  pub required_decisive_wins: usize,
  pub tsc_selected: bool,
  pub measured_winner: &'static str,
  pub selected_provider: &'static str,
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_x86_instant_selection_measurements() -> AppleX86InstantSelectionMeasurements {
  let evidence = crate::arch::apple_x86_64::bench_instant_selection_evidence();
  AppleX86InstantSelectionMeasurements {
    reads_per_batch: evidence.reads_per_batch,
    tsc_eligible: evidence.tsc_eligible,
    tsc_eligibility_basis: evidence.tsc_eligibility_basis,
    translated: evidence.translated,
    mach_absolute_time_batches_ticks: evidence.mach_absolute_time_batches_ticks,
    tsc_batches_ticks: evidence.tsc_batches_ticks,
    mach_absolute_time_median_ticks: evidence.mach_absolute_time_median_ticks,
    tsc_median_ticks: evidence.tsc_median_ticks,
    allowance_total_ticks: evidence.allowance_total_ticks,
    decisive_wins: evidence.decisive_wins,
    required_decisive_wins: evidence.required_decisive_wins,
    tsc_selected: evidence.tsc_selected,
    measured_winner: evidence.measured_winner,
    selected_provider: evidence.selected_provider,
  }
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct AppleX86WallSelectionMeasurements {
  pub reads_per_batch: u64,
  pub commpage_eligible: bool,
  pub commpage_eligibility_basis: &'static str,
  pub mach_absolute_time_batches_ticks: [u64; 9],
  pub commpage_batches_ticks: [u64; 9],
  pub mach_absolute_time_median_ticks: u64,
  pub commpage_median_ticks: u64,
  pub allowance_total_ticks: u64,
  pub decisive_wins: usize,
  pub required_decisive_wins: usize,
  pub commpage_selected: bool,
  pub selected_provider: &'static str,
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
pub fn apple_x86_wall_selection_measurements() -> AppleX86WallSelectionMeasurements {
  let evidence = crate::arch::apple_x86_64::bench_ordered_selection_evidence();
  AppleX86WallSelectionMeasurements {
    reads_per_batch: evidence.reads_per_batch,
    commpage_eligible: evidence.commpage_eligible,
    commpage_eligibility_basis: evidence.commpage_eligibility_basis,
    mach_absolute_time_batches_ticks: evidence.mach_absolute_time_batches_ticks,
    commpage_batches_ticks: evidence.commpage_batches_ticks,
    mach_absolute_time_median_ticks: evidence.mach_absolute_time_median_ticks,
    commpage_median_ticks: evidence.commpage_median_ticks,
    allowance_total_ticks: evidence.allowance_total_ticks,
    decisive_wins: evidence.decisive_wins,
    required_decisive_wins: evidence.required_decisive_wins,
    commpage_selected: evidence.commpage_selected,
    selected_provider: evidence.selected_provider,
  }
}

#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(any(target_os = "android", target_os = "linux")),
  not(all(target_arch = "x86_64", any(target_os = "macos", target_os = "freebsd"))),
))]
#[doc(hidden)]
pub fn ordered_selected_provider() -> &'static str {
  #[cfg(target_arch = "x86_64")]
  {
    crate::arch::x86_64::bench_selected_ordered_provider()
  }
  #[cfg(target_arch = "x86")]
  {
    crate::arch::x86::bench_selected_ordered_provider()
  }
}

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
#[doc(hidden)]
pub fn ordered_selected_provider() -> &'static str {
  if crate::arch::linux_x86_wall::ordered_uses_tsc() {
    #[cfg(target_arch = "x86_64")]
    {
      crate::arch::x86_64::bench_selected_ordered_provider()
    }
    #[cfg(target_arch = "x86")]
    {
      crate::arch::x86::bench_selected_ordered_provider()
    }
  } else {
    "linux_clock_monotonic"
  }
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
#[doc(hidden)]
pub fn ordered_selected_provider() -> &'static str {
  crate::arch::freebsd_x86_64::bench_selected_ordered_primitive().name
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
pub fn ordered_selected_provider() -> &'static str {
  apple_ordered_wall_selected_provider()
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
pub fn ordered_selected_provider() -> &'static str {
  crate::arch::linux_aarch64_wall::bench_ordered_provider()
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[doc(hidden)]
pub fn ordered_selected_provider() -> &'static str {
  apple_ordered_wall_selected_provider()
}

#[cfg(any(
  target_os = "macos",
  all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(target_os = "windows"),
    not(all(target_arch = "x86_64", target_os = "macos")),
  ),
  all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct OrderedX86TournamentStepMeasurements {
  pub challenger: &'static str,
  pub incumbent: &'static str,
  pub winner: &'static str,
  pub allowance_total_ticks: u64,
  pub decisive_wins: usize,
  pub challenger_selected: bool,
}

#[cfg(any(
  target_os = "macos",
  all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(target_os = "windows"),
    not(all(target_arch = "x86_64", target_os = "macos")),
  ),
  all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct OrderedX86TournamentMeasurements {
  pub candidate_names: [&'static str; 5],
  pub candidate_eligible: [bool; 5],
  pub candidate_batches_ticks: [[u64; 9]; 5],
  pub candidate_medians_ticks: [u64; 5],
  pub steps: [OrderedX86TournamentStepMeasurements; 4],
  pub selected_provider: &'static str,
}

#[cfg(any(
  target_os = "macos",
  all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(target_os = "windows"),
    not(all(target_arch = "x86_64", target_os = "macos")),
  ),
  all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
))]
#[doc(hidden)]
#[derive(Serialize, Clone, Debug)]
pub struct OrderedSelectionMeasurements {
  pub challenger: &'static str,
  pub incumbent: &'static str,
  pub batch_count: usize,
  pub reads_per_batch: u64,
  pub counter_hz: u64,
  pub challenger_batches_ticks: [u64; 9],
  pub incumbent_batches_ticks: [u64; 9],
  pub challenger_median_ticks: u64,
  pub incumbent_median_ticks: u64,
  pub allowance_total_ticks: u64,
  pub decisive_wins: usize,
  pub required_decisive_wins: usize,
  pub challenger_selected: bool,
  pub computed_winner: &'static str,
  pub x86_tournament: Option<OrderedX86TournamentMeasurements>,
}

#[cfg(all(
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(all(target_arch = "x86_64", target_os = "macos")),
))]
#[doc(hidden)]
pub fn ordered_selection_measurements() -> Option<OrderedSelectionMeasurements> {
  #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
  if !crate::arch::freebsd_x86_64::bench_tsc_eligible() {
    return None;
  }
  #[cfg(all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86_64", target_arch = "x86"),
  ))]
  if !crate::arch::linux_x86_wall::bench_tsc_eligible() {
    return None;
  }
  #[cfg(target_arch = "x86_64")]
  let evidence = crate::arch::x86_64::bench_ordered_selection_evidence()?;
  #[cfg(target_arch = "x86")]
  let evidence = crate::arch::x86::bench_ordered_selection_evidence()?;
  let challenger = "x86_lfence_rdtsc";
  let incumbent = "x86_rdtscp";
  let lfence_winner =
    if evidence.lfence_selected_over_cpuid { "x86_lfence_rdtsc" } else { "x86_cpuid_rdtsc" };
  let mfence_winner = if evidence.mfence_selected_over_incumbent {
    "x86_mfence_rdtsc"
  } else {
    evidence.mfence_incumbent
  };
  let rdtscp_winner =
    if evidence.rdtscp_selected_over_incumbent { "x86_rdtscp" } else { evidence.rdtscp_incumbent };
  let serialize_winner = if evidence.serialize_selected_over_incumbent {
    "x86_serialize_rdtsc"
  } else {
    evidence.serialize_incumbent
  };
  let x86_tournament = OrderedX86TournamentMeasurements {
    candidate_names: [
      "x86_cpuid_rdtsc",
      "x86_lfence_rdtsc",
      "x86_mfence_rdtsc",
      "x86_rdtscp",
      "x86_serialize_rdtsc",
    ],
    candidate_eligible: [
      true,
      evidence.lfence_eligible,
      evidence.mfence_eligible,
      evidence.rdtscp_eligible,
      evidence.serialize_eligible,
    ],
    candidate_batches_ticks: [
      evidence.cpuid_batches,
      evidence.lfence_batches,
      evidence.mfence_batches,
      evidence.rdtscp_batches,
      evidence.serialize_batches,
    ],
    candidate_medians_ticks: [
      evidence.cpuid_median,
      evidence.lfence_median,
      evidence.mfence_median,
      evidence.rdtscp_median,
      evidence.serialize_median,
    ],
    steps: [
      OrderedX86TournamentStepMeasurements {
        challenger: "x86_lfence_rdtsc",
        incumbent: "x86_cpuid_rdtsc",
        winner: lfence_winner,
        allowance_total_ticks: evidence.lfence_vs_cpuid_allowance,
        decisive_wins: evidence.lfence_vs_cpuid_decisive_wins,
        challenger_selected: evidence.lfence_selected_over_cpuid,
      },
      OrderedX86TournamentStepMeasurements {
        challenger: "x86_mfence_rdtsc",
        incumbent: evidence.mfence_incumbent,
        winner: mfence_winner,
        allowance_total_ticks: evidence.mfence_vs_incumbent_allowance,
        decisive_wins: evidence.mfence_vs_incumbent_decisive_wins,
        challenger_selected: evidence.mfence_selected_over_incumbent,
      },
      OrderedX86TournamentStepMeasurements {
        challenger: "x86_rdtscp",
        incumbent: evidence.rdtscp_incumbent,
        winner: rdtscp_winner,
        allowance_total_ticks: evidence.rdtscp_vs_incumbent_allowance,
        decisive_wins: evidence.rdtscp_vs_incumbent_decisive_wins,
        challenger_selected: evidence.rdtscp_selected_over_incumbent,
      },
      OrderedX86TournamentStepMeasurements {
        challenger: "x86_serialize_rdtsc",
        incumbent: evidence.serialize_incumbent,
        winner: serialize_winner,
        allowance_total_ticks: evidence.serialize_vs_incumbent_allowance,
        decisive_wins: evidence.serialize_vs_incumbent_decisive_wins,
        challenger_selected: evidence.serialize_selected_over_incumbent,
      },
    ],
    selected_provider: evidence.selected_provider,
  };
  Some(OrderedSelectionMeasurements {
    challenger,
    incumbent,
    batch_count: evidence.challenger_batches.len(),
    reads_per_batch: evidence.reads_per_batch,
    counter_hz: evidence.counter_hz,
    challenger_batches_ticks: evidence.challenger_batches,
    incumbent_batches_ticks: evidence.incumbent_batches,
    challenger_median_ticks: evidence.challenger_median,
    incumbent_median_ticks: evidence.incumbent_median,
    allowance_total_ticks: evidence.allowance,
    decisive_wins: evidence.decisive_wins,
    required_decisive_wins: evidence.required_decisive_wins,
    challenger_selected: evidence.challenger_selected,
    computed_winner: if evidence.challenger_selected { challenger } else { incumbent },
    x86_tournament: Some(x86_tournament),
  })
}

#[cfg(all(target_arch = "x86_64", target_os = "macos"))]
#[doc(hidden)]
pub fn ordered_selection_measurements() -> Option<OrderedSelectionMeasurements> {
  let evidence = crate::arch::apple_x86_64::bench_ordered_selection_evidence();
  let (numer, denom) = crate::arch::fallback::mach_timebase();
  let counter_hz = ((1_000_000_000_u128 * u128::from(denom)) / u128::from(numer)) as u64;
  let challenger = "apple_commpage_lfence_rdtsc_nanotime";
  let incumbent = "apple_mach_absolute_time";
  Some(OrderedSelectionMeasurements {
    challenger,
    incumbent,
    batch_count: evidence.commpage_batches_ticks.len(),
    reads_per_batch: evidence.reads_per_batch,
    counter_hz,
    challenger_batches_ticks: evidence.commpage_batches_ticks,
    incumbent_batches_ticks: evidence.mach_absolute_time_batches_ticks,
    challenger_median_ticks: evidence.commpage_median_ticks,
    incumbent_median_ticks: evidence.mach_absolute_time_median_ticks,
    allowance_total_ticks: evidence.allowance_total_ticks,
    decisive_wins: evidence.decisive_wins,
    required_decisive_wins: evidence.required_decisive_wins,
    challenger_selected: evidence.commpage_selected,
    computed_winner: evidence.selected_provider,
    x86_tournament: None,
  })
}

#[cfg(all(target_os = "macos", not(target_arch = "x86_64")))]
#[doc(hidden)]
pub fn ordered_selection_measurements() -> Option<OrderedSelectionMeasurements> {
  None
}

#[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
#[doc(hidden)]
pub fn ordered_selection_measurements() -> Option<OrderedSelectionMeasurements> {
  let evidence = crate::arch::linux_aarch64_wall::bench_ordered_evidence();
  if !evidence.hwcap_ecv {
    return None;
  }
  let challenger = "aarch64_cntvctss";
  let incumbent = "aarch64_isb_cntvct";
  let mut challenger_sorted = evidence.cntvctss_batches_ns;
  let mut incumbent_sorted = evidence.isb_batches_ns;
  challenger_sorted.sort_unstable();
  incumbent_sorted.sort_unstable();
  let challenger_selected = evidence.direct_provider == challenger;
  Some(OrderedSelectionMeasurements {
    challenger,
    incumbent,
    batch_count: evidence.cntvctss_batches_ns.len(),
    reads_per_batch: evidence.reads_per_batch,
    counter_hz: 1_000_000_000,
    challenger_batches_ticks: evidence.cntvctss_batches_ns,
    incumbent_batches_ticks: evidence.isb_batches_ns,
    challenger_median_ticks: challenger_sorted[challenger_sorted.len() / 2],
    incumbent_median_ticks: incumbent_sorted[incumbent_sorted.len() / 2],
    allowance_total_ticks: evidence.cntvctss_vs_isb_allowance_ns,
    decisive_wins: evidence.cntvctss_vs_isb_decisive_wins,
    required_decisive_wins: evidence.required_decisive_wins,
    challenger_selected,
    computed_winner: if challenger_selected { challenger } else { incumbent },
    x86_tournament: None,
  })
}

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
pub struct OrderedVerifyPlacement {
  /// "adversarial-pair" | "full-span" | "oversubscribed-2x"
  pub placement: String,
  /// Logical core id bound to worker `i` (`i -> pinned_cores[i]`).
  pub pinned_cores: Vec<usize>,
  pub threads: u32,
  /// Sync-order result per clock under this placement. Read `tach` as the
  /// positive control (must be non-zero), `tach_ordered` as the subject, and
  /// `std` / `quanta` / `minstant` / `fastant` as ecosystem comparators.
  pub results: std::collections::BTreeMap<String, SyncOrderResult>,
}

#[derive(Serialize, Clone, Debug)]
pub struct OrderedVerifyReport {
  pub schema: &'static str,
  pub cell: String,
  pub target_triple: &'static str,
  pub started_at_unix_ns: u128,
  pub host: HostInfo,
  pub tach_freq_hz: u64,
  pub duration_secs_per_run: u64,
  pub placements: Vec<OrderedVerifyPlacement>,
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
/// given duration). Any non-zero value means the bare clock read can be sampled
/// before a prior Acquire-load — i.e. it needs an ordering barrier to claim the
/// synchronization-order contract.
///
/// This is the canonical test behind `OrderedInstant`'s cross-thread guarantee:
/// `tach` (bare) fails it, `tach_ordered` (barrier) passes it at 0 violations.
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

/// Pinned variant of [`measure_synchronization_order`]: spawns `pin.len()`
/// workers, binding worker *i* to logical core `pin[i]` before the barrier.
///
/// Forces the cross-socket / cross-CCX read pairs an unpinned scheduler might
/// never produce. Without that placement a "0 violations" result is
/// uninterpretable — you can't distinguish coherent hardware from a scheduler
/// that kept publisher and reader on the same socket. Repeated ids in `pin`
/// oversubscribe those cores. Bare `tach` on the same `pin` is the positive
/// control: it must show violations, else the placement didn't exercise the
/// adversarial direction.
pub fn measure_synchronization_order_pinned<C: ClockSource>(
  pin: &[usize],
  duration: Duration,
) -> SyncOrderResult {
  C::init_anchor();
  for _ in 0..1_000 {
    let _ = C::now_as_u64();
  }

  let threads = pin.len();
  let published = std::sync::Arc::new(AtomicU64::new(0));
  let start_barrier = std::sync::Arc::new(std::sync::Barrier::new(threads + 1));
  let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

  let mut handles = Vec::with_capacity(threads);
  for &core in pin {
    let published = std::sync::Arc::clone(&published);
    let start_barrier = std::sync::Arc::clone(&start_barrier);
    let stop = std::sync::Arc::clone(&stop);
    handles.push(thread::spawn(move || -> (u64, u64, u64) {
      // Bind before the barrier so every measured read runs on `core`.
      let _ = core_affinity::set_for_current(core_affinity::CoreId { id: core });
      let mut local_violations = 0u64;
      let mut local_reads = 0u64;
      let mut local_max_violation = 0u64;

      start_barrier.wait();

      while !stop.load(Ordering::Relaxed) {
        let observed = published.load(Ordering::Acquire);
        let now_ns = C::now_as_u64();
        local_reads += 1;
        if now_ns < observed {
          local_violations += 1;
          let diff = observed - now_ns;
          if diff > local_max_violation {
            local_max_violation = diff;
          }
        }
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

/// Logical core ids this process may bind threads to. Empty if the platform
/// doesn't report them. Used to build pinned placements for the ordered-verify
/// study.
#[must_use]
pub fn available_core_ids() -> Vec<usize> {
  core_affinity::get_core_ids()
    .map(|v| v.into_iter().map(|c| c.id).collect())
    .unwrap_or_default()
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

/// The effective frequency tach derived for its selected wall counter.
/// Triggers lazy scale initialization if it has not already run.
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
  #[cfg(all(
    target_arch = "x86_64",
    not(any(target_os = "macos", target_os = "windows", target_os = "freebsd")),
  ))]
  {
    crate::arch::x86_64::cpuid_tsc_hz().is_some()
  }
  #[cfg(all(
    target_arch = "x86",
    not(any(target_os = "macos", target_os = "windows", target_os = "freebsd")),
  ))]
  {
    crate::arch::x86::cpuid_tsc_hz().is_some()
  }
  #[cfg(not(all(
    any(target_arch = "x86_64", target_arch = "x86"),
    not(any(target_os = "macos", target_os = "windows", target_os = "freebsd")),
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

pub struct TachOrderedInstant;
static TACH_ORDERED_ANCHOR: OnceLock<TachOrderedInstantTy> = OnceLock::new();
impl ClockSource for TachOrderedInstant {
  const NAME: &'static str = "tach_ordered";
  fn init_anchor() {
    let _ = TACH_ORDERED_ANCHOR.get_or_init(TachOrderedInstantTy::now);
  }
  fn now_as_u64() -> u64 {
    let anchor = *TACH_ORDERED_ANCHOR.get().expect("init_anchor first");
    u64::try_from(TachOrderedInstantTy::now().saturating_duration_since(anchor).as_nanos())
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

#[cfg(feature = "bench-quanta")]
pub struct QuantaInstant;
#[cfg(feature = "bench-quanta")]
static QUANTA_ANCHOR: OnceLock<quanta::Instant> = OnceLock::new();
#[cfg(feature = "bench-quanta")]
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

#[cfg(all(test, feature = "bench-internal"))]
mod exact_wall_harness_tests {
  use super::*;

  #[test]
  fn exact_wall_provider_carries_the_conversion_for_its_own_tick_domain() {
    let two_nanoseconds_per_tick = 2_u64 << 32;
    let provider = ExactWallProvider::new("counter", two_nanoseconds_per_tick);

    assert_eq!(provider.nanos_per_tick_q32(), two_nanoseconds_per_tick);
    assert_eq!(
      exact_ticks_to_duration_with_scale(7, provider.nanos_per_tick_q32()),
      Duration::from_nanos(14)
    );
  }

  #[test]
  fn exact_wall_harnesses_keep_static_readers_and_provider_scales() {
    let criterion = include_str!("../benches/instant.rs");
    let lambda = include_str!("../benches/lambda-speed/src/main.rs");
    let descriptor = include_str!("bench.rs")
      .split("pub struct ExactWallProvider")
      .nth(1)
      .and_then(|tail| tail.split("impl ExactWallProvider").next())
      .expect("exact wall provider descriptor");

    assert!(!descriptor.contains("fn() -> u64"));
    assert!(!criterion.contains("SelectedWallPrimitive"));
    assert!(!lambda.contains("SelectedWallPrimitive"));
    assert!(!criterion.contains("instant_ticks_to_duration"));
    assert!(!criterion.contains("ordered_instant_ticks_to_duration"));
    assert!(criterion.contains("candidate.nanos_per_tick_q32()"));
    assert!(criterion.contains("with_apple_aarch64_instant_read!"));
    assert!(lambda.contains("with_lambda_linux_x86_instant_read!"));
    assert!(lambda.contains("with_lambda_linux_x86_ordered_read!"));
    assert!(lambda.contains("exact_ticks_to_duration_with_scale"));
  }
}
