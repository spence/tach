//! Current-thread timing providers, normalized to nanoseconds.

use crate::{ThreadCpuProvider, ThreadCpuReadCost};

#[inline]
pub(crate) fn max_read_gap() -> Option<core::time::Duration> {
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
  {
    return linux_inline::max_read_gap_nanos().map(core::time::Duration::from_nanos);
  }

  #[allow(unreachable_code)]
  None
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
#[path = "thread_cpu_linux_inline.rs"]
mod linux_inline;

#[cfg(all(
  feature = "bench-internal",
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
pub(crate) struct BenchPerfHandle(linux_inline::BenchPerfHandle);

#[cfg(all(
  feature = "bench-internal",
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
impl BenchPerfHandle {
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(crate) fn now_nanos(&self) -> u64 {
    self.0.now_nanos()
  }

  pub(crate) fn candidate_count(&self) -> usize {
    self.0.candidate_count()
  }

  pub(crate) fn candidate_name(&self, index: usize) -> Option<&'static str> {
    self.0.candidate_name(index)
  }

  pub(crate) fn selected_candidate_name(&self) -> &'static str {
    self.0.selected_candidate_name()
  }

  pub(crate) fn select_candidate(&self, index: usize) -> bool {
    self.0.select_candidate(index)
  }
}

#[cfg(all(
  feature = "bench-internal",
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
pub(crate) fn bench_perf_handle() -> Option<BenchPerfHandle> {
  linux_inline::bench_perf_handle().map(BenchPerfHandle)
}

#[cfg(all(
  feature = "bench-internal",
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
pub(crate) fn bench_perf_selection_measurements() -> Option<([u64; 9], [u64; 9], usize)> {
  linux_inline::bench_selection_measurements()
}

#[cfg(all(
  feature = "bench-internal",
  feature = "thread-cpu-inline",
  any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"),
  any(
    target_os = "linux",
    all(target_os = "android", any(target_arch = "x86_64", target_arch = "aarch64"),),
  ),
))]
pub(crate) struct BenchPerfCounterEvidence {
  pub(crate) candidate_count: usize,
  pub(crate) candidate_names: [&'static str; 5],
  pub(crate) candidate_eligible: [bool; 5],
  pub(crate) candidate_batches_ns: [[u64; 9]; 5],
  pub(crate) selected_candidate: &'static str,
  pub(crate) reads_per_batch: usize,
  pub(crate) required_decisive_wins: usize,
}

#[cfg(all(
  feature = "bench-internal",
  feature = "thread-cpu-inline",
  any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"),
  any(
    target_os = "linux",
    all(target_os = "android", any(target_arch = "x86_64", target_arch = "aarch64"),),
  ),
))]
pub(crate) fn bench_perf_counter_evidence() -> Option<BenchPerfCounterEvidence> {
  let evidence = linux_inline::bench_perf_counter_evidence()?;
  Some(BenchPerfCounterEvidence {
    candidate_count: evidence.candidate_count,
    candidate_names: evidence.candidate_names,
    candidate_eligible: evidence.candidate_eligible,
    candidate_batches_ns: evidence.candidate_batches_ns,
    selected_candidate: evidence.selected_candidate,
    reads_per_batch: evidence.reads_per_batch,
    required_decisive_wins: evidence.required_decisive_wins,
  })
}

#[cfg(all(
  feature = "bench-internal",
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
pub(crate) struct BenchPerfPathEvidence {
  pub(crate) mmap_batches_ns: Option<[u64; 9]>,
  pub(crate) read_batches_ns: [u64; 9],
  pub(crate) posix_batches_ns: [u64; 9],
  pub(crate) selected_path: &'static str,
  pub(crate) fallback_path: &'static str,
  pub(crate) reads_per_batch: usize,
  pub(crate) required_decisive_wins: usize,
}

#[cfg(all(
  feature = "bench-internal",
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
pub(crate) fn bench_perf_path_evidence() -> Option<BenchPerfPathEvidence> {
  let evidence = linux_inline::bench_path_evidence()?;
  Some(BenchPerfPathEvidence {
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
  feature = "bench-internal",
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
pub(crate) struct BenchPerfReadEntryEvidence {
  pub(crate) candidate_count: usize,
  pub(crate) candidate_names: [&'static str; 3],
  pub(crate) candidate_eligible: [bool; 3],
  pub(crate) candidate_measured: [bool; 3],
  pub(crate) candidate_batches_ns: [[u64; 9]; 3],
  pub(crate) selected_candidate: &'static str,
  pub(crate) reads_per_batch: usize,
  pub(crate) required_decisive_wins: usize,
}

#[cfg(all(
  feature = "bench-internal",
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
pub(crate) fn bench_perf_read_entry_evidence() -> Option<BenchPerfReadEntryEvidence> {
  let evidence = linux_inline::bench_perf_read_entry_evidence()?;
  Some(BenchPerfReadEntryEvidence {
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
  feature = "bench-internal",
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
pub(crate) struct BenchPerfReadHandle(linux_inline::BenchPerfReadHandle);

#[cfg(all(
  feature = "bench-internal",
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
impl BenchPerfReadHandle {
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(crate) fn now_nanos(&self) -> u64 {
    self.0.now_nanos()
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(crate) fn direct_nanos(&self) -> u64 {
    self.0.direct_nanos()
  }

  pub(crate) fn candidate_count(&self) -> usize {
    self.0.candidate_count()
  }

  pub(crate) fn candidate_name(&self, index: usize) -> Option<&'static str> {
    self.0.candidate_name(index)
  }

  pub(crate) fn selected_candidate_name(&self) -> &'static str {
    self.0.selected_candidate_name()
  }

  pub(crate) fn select_candidate(&self, index: usize) -> bool {
    self.0.select_candidate(index)
  }
}

#[cfg(all(
  feature = "bench-internal",
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
pub(crate) fn bench_perf_read_handle() -> Option<BenchPerfReadHandle> {
  linux_inline::bench_perf_read_handle().map(BenchPerfReadHandle)
}

#[cfg(all(
  feature = "bench-internal",
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
pub(crate) struct BenchPerfPathHandle(linux_inline::BenchPerfPathHandle);

#[cfg(all(
  feature = "bench-internal",
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
impl BenchPerfPathHandle {
  pub(crate) const fn candidate_count(&self) -> usize {
    self.0.candidate_count()
  }

  pub(crate) fn candidate_name(&self, index: usize) -> Option<&'static str> {
    self.0.candidate_name(index)
  }

  pub(crate) fn candidate_available(&self, index: usize) -> bool {
    self.0.candidate_available(index)
  }

  pub(crate) fn select_candidate(&self, index: usize) -> bool {
    self.0.select_candidate(index)
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(crate) fn now_nanos(&self) -> u64 {
    self.0.now_nanos()
  }
}

#[cfg(all(
  feature = "bench-internal",
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
pub(crate) fn bench_perf_path_handle() -> Option<BenchPerfPathHandle> {
  linux_inline::bench_perf_path_handle().map(BenchPerfPathHandle)
}

#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
#[cfg(any(
  all(any(target_arch = "x86", target_arch = "arm", target_arch = "riscv64"), target_os = "linux",),
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
use core::hint::black_box;
#[cfg(any(
  all(any(target_arch = "x86", target_arch = "arm", target_arch = "riscv64"), target_os = "linux",),
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
use core::sync::atomic::AtomicI32;
#[cfg(any(
  all(any(target_arch = "x86", target_arch = "arm", target_arch = "riscv64"), target_os = "linux",),
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
use core::sync::atomic::{AtomicU8, Ordering};

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
use core::arch::asm;

#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten"))))]
use core::mem::MaybeUninit;

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
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn now_nanos() -> u64 {
  linux_inline::now_nanos()
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
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  let provider = linux_inline::provider();
  if matches!(provider, ThreadCpuProvider::LinuxPerfMmap | ThreadCpuProvider::LinuxPerfRead) {
    provider
  } else {
    posix_provider()
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
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  match linux_inline::provider() {
    // The selected hot path reads perf's mapped metadata and architectural
    // counter directly. A kernel or hypervisor may emulate the counter, but
    // the per-thread tournament measures that effective cost before choosing
    // this route.
    ThreadCpuProvider::LinuxPerfMmap => ThreadCpuReadCost::Inline,
    ThreadCpuProvider::LinuxPerfRead => ThreadCpuReadCost::SystemCall,
    _ => posix_read_cost(),
  }
}

#[cfg(all(
  any(target_os = "linux", target_os = "android"),
  not(all(
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
  )),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn now_nanos() -> u64 {
  posix_now_nanos()
}

#[cfg(all(
  any(target_os = "linux", target_os = "android"),
  not(all(
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
  )),
))]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  posix_provider()
}

#[cfg(all(
  any(target_os = "linux", target_os = "android"),
  not(all(
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
  )),
))]
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  posix_read_cost()
}

#[cfg(all(
  unix,
  not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "emscripten",
  )),
))]
#[inline]
pub(crate) fn now_nanos() -> u64 {
  posix_now_nanos()
}

#[cfg(all(
  unix,
  not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "emscripten",
  )),
))]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  posix_provider()
}

#[cfg(all(
  unix,
  not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "emscripten",
  )),
))]
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  posix_read_cost()
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten"))))]
#[inline]
fn posix_now_nanos() -> u64 {
  #[cfg(any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ))]
  {
    native_64_now_nanos()
  }

  #[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
  {
    linux_32_now_nanos()
  }

  #[cfg(all(target_arch = "riscv64", target_os = "linux"))]
  {
    linux_riscv64_now_nanos()
  }

  #[cfg(all(
    target_os = "linux",
    any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
  ))]
  {
    linux_rare::now_nanos()
  }

  #[cfg(not(all(
    any(
      target_arch = "x86",
      target_arch = "arm",
      target_arch = "riscv64",
      target_arch = "s390x",
      target_arch = "loongarch64",
      target_arch = "powerpc64",
      target_arch = "x86_64",
      target_arch = "aarch64",
    ),
    any(
      target_os = "linux",
      all(target_arch = "x86_64", target_os = "android"),
      all(target_arch = "x86_64", target_os = "freebsd"),
      all(target_arch = "aarch64", target_os = "android"),
    ),
  )))]
  posix_now_nanos_libc()
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten"))))]
#[inline]
#[allow(dead_code)] // Native-selector targets call the equivalent candidate helper directly.
fn posix_now_nanos_libc() -> u64 {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // CLOCK_THREAD_CPUTIME_ID is the POSIX native-precision scheduled-runtime
  // clock. getrusage(RUSAGE_THREAD) uses timeval accounting and is excluded
  // rather than trading thread-time precision for a cheaper read.
  // SAFETY: `value` is writable storage for a timespec and the clock id
  // selects CPU time consumed by the calling thread.
  if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, value.as_mut_ptr()) == 0 } {
    // SAFETY: a successful clock_gettime initialized the output.
    let value = unsafe { value.assume_init() };
    timespec_to_nanos(value).unwrap_or_else(wall_now_value)
  } else {
    wall_now_value()
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
#[inline]
fn native_clock_uses_wall() -> bool {
  crate::thread_cpu::is_wall_value(posix_now_nanos())
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
const NATIVE64_UNKNOWN: u8 = 0;
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
const NATIVE64_SELECTING: u8 = 1;
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
const NATIVE64_LIBC: u8 = 2;
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
const NATIVE64_RAW: u8 = 3;
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
const NATIVE64_WALL: u8 = 4;
#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
const NATIVE64_WARMUP_READS: usize = 128;
#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
const NATIVE64_MEASURE_READS: usize = 4_096;
#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
const NATIVE64_SAMPLES: usize = 9;
#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
const NATIVE64_REQUIRED_WINS: usize = 7;
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
static NATIVE64_CHOICE: AtomicU8 = AtomicU8::new(NATIVE64_UNKNOWN);
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
static NATIVE64_OWNER_PID: AtomicI32 = AtomicI32::new(0);
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
static NATIVE64_OWNER_TID: AtomicI32 = AtomicI32::new(0);
#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
static NATIVE64_PROBE_CHOICE: AtomicU8 = AtomicU8::new(NATIVE64_LIBC);

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
static NATIVE64_LIBC_AVAILABLE: AtomicU8 = AtomicU8::new(0);
#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
static NATIVE64_RAW_AVAILABLE: AtomicU8 = AtomicU8::new(0);
#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
static NATIVE64_LIBC_SAMPLES: [core::sync::atomic::AtomicU64; NATIVE64_SAMPLES] =
  [const { core::sync::atomic::AtomicU64::new(0) }; NATIVE64_SAMPLES];
#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
static NATIVE64_RAW_SAMPLES: [core::sync::atomic::AtomicU64; NATIVE64_SAMPLES] =
  [const { core::sync::atomic::AtomicU64::new(0) }; NATIVE64_SAMPLES];

#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
struct Native64Measurements {
  libc: [u64; NATIVE64_SAMPLES],
  raw: [u64; NATIVE64_SAMPLES],
}

#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[allow(dead_code)] // Decision metadata is emitted by bench-internal builds.
struct Native64Decision {
  allowance: u64,
  decisive_wins: usize,
  challenger_selected: bool,
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn native_64_now_nanos() -> u64 {
  let provider = NATIVE64_CHOICE.load(Ordering::Relaxed);
  match provider {
    NATIVE64_LIBC => {
      native_64_libc_nanos().unwrap_or_else(|| read_outlined_native_64_provider(provider))
    }
    NATIVE64_RAW => {
      native_64_raw_nanos().unwrap_or_else(|| read_outlined_native_64_provider(provider))
    }
    _ => read_outlined_native_64_provider(provider),
  }
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[inline(never)]
fn read_outlined_native_64_provider(provider: u8) -> u64 {
  match provider {
    NATIVE64_LIBC => {
      if let Some(value) = native_64_raw_nanos() {
        NATIVE64_CHOICE.store(NATIVE64_RAW, Ordering::Relaxed);
        return value;
      }
    }
    NATIVE64_RAW => {
      if let Some(value) = native_64_libc_nanos() {
        NATIVE64_CHOICE.store(NATIVE64_LIBC, Ordering::Relaxed);
        return value;
      }
    }
    NATIVE64_WALL => return wall_now_value(),
    NATIVE64_UNKNOWN | NATIVE64_SELECTING => {
      let selected = super::select_same_domain_thread_owned_process_provider(
        &NATIVE64_CHOICE,
        NATIVE64_UNKNOWN,
        NATIVE64_SELECTING,
        &NATIVE64_OWNER_PID,
        &NATIVE64_OWNER_TID,
        NATIVE64_LIBC,
        select_native_64,
      );
      if NATIVE64_CHOICE.load(Ordering::Acquire) == selected {
        return native_64_now_nanos();
      }
      return native_64_libc_nanos()
        .or_else(native_64_raw_nanos)
        .unwrap_or_else(wall_now_value);
    }
    _ => {}
  }
  NATIVE64_CHOICE.store(NATIVE64_WALL, Ordering::Relaxed);
  wall_now_value()
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
fn select_native_64() -> u8 {
  let libc_available = native_64_libc_nanos().is_some();
  let raw_available = native_64_raw_nanos().is_some();
  record_native_64_availability(libc_available, raw_available);

  #[cfg(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),))]
  {
    if libc_available && raw_available {
      #[cfg(feature = "bench-internal")]
      {
        for provider in [NATIVE64_LIBC, NATIVE64_RAW] {
          NATIVE64_PROBE_CHOICE.store(provider, Ordering::Relaxed);
          for _ in 0..NATIVE64_WARMUP_READS {
            let _ = black_box(native_64_probe_nanos());
          }
        }
        if let Some(measurements) = measure_native_64_candidates() {
          record_native_64_measurements(&measurements);
        }
      }
      return NATIVE64_RAW;
    }
    match (libc_available, raw_available) {
      (_, true) => NATIVE64_RAW,
      (true, false) => NATIVE64_LIBC,
      (false, false) => NATIVE64_WALL,
    }
  }

  #[cfg(not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)))]
  {
    let available_choice = native_64_available_choice(libc_available, raw_available);
    if available_choice != NATIVE64_UNKNOWN {
      return available_choice;
    }

    for provider in [NATIVE64_LIBC, NATIVE64_RAW] {
      NATIVE64_PROBE_CHOICE.store(provider, Ordering::Relaxed);
      for _ in 0..NATIVE64_WARMUP_READS {
        let _ = black_box(native_64_probe_nanos());
      }
    }

    let Some(measurements) = measure_native_64_candidates() else {
      return NATIVE64_LIBC;
    };
    record_native_64_measurements(&measurements);
    if prefer_native_64_candidate(measurements.raw, measurements.libc) {
      NATIVE64_RAW
    } else {
      NATIVE64_LIBC
    }
  }
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[cfg(any(
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
const fn native_64_available_choice(libc: bool, raw: bool) -> u8 {
  match (libc, raw) {
    (true, false) => NATIVE64_LIBC,
    (false, true) => NATIVE64_RAW,
    (false, false) => NATIVE64_WALL,
    (true, true) => NATIVE64_UNKNOWN,
  }
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[allow(dead_code)] // The inline perf selector consumes the native path but owns the public cost hint.
fn native_64_read_cost() -> ThreadCpuReadCost {
  let _ = native_64_now_nanos();
  if NATIVE64_CHOICE.load(Ordering::Relaxed) == NATIVE64_WALL {
    wall_read_cost()
  } else {
    native_64_mechanism_read_cost()
  }
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[allow(dead_code)] // Retained for no-default builds and selector policy tests.
const fn native_64_mechanism_read_cost() -> ThreadCpuReadCost {
  // CLOCK_THREAD_CPUTIME_ID is a kernel-entry clock on these targets. libc is
  // an equivalent ABI wrapper, not a vDSO thread-time implementation.
  ThreadCpuReadCost::SystemCall
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
fn measure_native_64_candidates() -> Option<Native64Measurements> {
  let mut libc = [0; NATIVE64_SAMPLES];
  let mut raw = [0; NATIVE64_SAMPLES];
  for sample in 0..NATIVE64_SAMPLES {
    let (libc_sample, raw_sample) = if sample & 1 == 0 {
      (measure_native_64_candidate(NATIVE64_LIBC), measure_native_64_candidate(NATIVE64_RAW))
    } else {
      let raw = measure_native_64_candidate(NATIVE64_RAW);
      let libc = measure_native_64_candidate(NATIVE64_LIBC);
      (libc, raw)
    };
    libc[sample] = libc_sample?;
    raw[sample] = raw_sample?;
  }
  Some(Native64Measurements { libc, raw })
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
fn measure_native_64_candidate(provider: u8) -> Option<u64> {
  NATIVE64_PROBE_CHOICE.store(provider, Ordering::Relaxed);
  let start = native_64_measurement_nanos()?;
  for _ in 0..NATIVE64_MEASURE_READS {
    black_box(native_64_probe_nanos()?);
  }
  native_64_measurement_nanos()?.checked_sub(start)
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
fn native_64_probe_nanos() -> Option<u64> {
  match NATIVE64_PROBE_CHOICE.load(Ordering::Relaxed) {
    NATIVE64_RAW => native_64_raw_nanos(),
    _ => native_64_libc_nanos(),
  }
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn native_64_libc_nanos() -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: the output is writable and the clock ID names CPU time consumed
  // by the calling thread.
  if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, value.as_mut_ptr()) } != 0 {
    return None;
  }
  // SAFETY: a successful clock_gettime initialized the output.
  timespec_to_nanos(unsafe { value.assume_init() })
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn native_64_raw_nanos() -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  if !thread_clock_gettime(value.as_mut_ptr()) {
    return None;
  }
  // SAFETY: a successful raw clock syscall initialized the output.
  timespec_to_nanos(unsafe { value.assume_init() })
}

#[cfg(any(
  all(target_arch = "x86_64", any(target_os = "linux", target_os = "android"),),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
fn native_64_measurement_nanos() -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: CLOCK_MONOTONIC_RAW is valid on Linux-kernel targets and value is
  // writable timespec storage.
  if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, value.as_mut_ptr()) } != 0 {
    return None;
  }
  // SAFETY: a successful clock_gettime initialized the output.
  timespec_to_nanos(unsafe { value.assume_init() })
}

#[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
fn native_64_measurement_nanos() -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: CLOCK_MONOTONIC is valid on FreeBSD and value is writable
  // timespec storage.
  if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, value.as_mut_ptr()) } != 0 {
    return None;
  }
  // SAFETY: a successful clock_gettime initialized the output.
  timespec_to_nanos(unsafe { value.assume_init() })
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[cfg(any(
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
fn prefer_native_64_candidate(
  challenger_samples: [u64; NATIVE64_SAMPLES],
  incumbent_samples: [u64; NATIVE64_SAMPLES],
) -> bool {
  evaluate_native_64_candidate(challenger_samples, incumbent_samples).challenger_selected
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
fn evaluate_native_64_candidate(
  challenger_samples: [u64; NATIVE64_SAMPLES],
  incumbent_samples: [u64; NATIVE64_SAMPLES],
) -> Native64Decision {
  let challenger_median = median_native_64(challenger_samples);
  let incumbent_median = median_native_64(incumbent_samples);
  let allowance = NATIVE64_MEASURE_READS as u64;
  let decisive_wins = challenger_samples
    .iter()
    .zip(incumbent_samples)
    .filter(|(challenger, incumbent)| (**challenger).saturating_add(allowance) < *incumbent)
    .count();
  Native64Decision {
    allowance,
    decisive_wins,
    challenger_selected: challenger_median.saturating_add(allowance) < incumbent_median
      && decisive_wins >= NATIVE64_REQUIRED_WINS,
  }
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[cfg(any(
  feature = "bench-internal",
  test,
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
))]
fn median_native_64(mut samples: [u64; NATIVE64_SAMPLES]) -> u64 {
  samples.sort_unstable();
  samples[NATIVE64_SAMPLES / 2]
}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
fn record_native_64_availability(libc_available: bool, raw_available: bool) {
  NATIVE64_LIBC_AVAILABLE.store(u8::from(libc_available), Ordering::Relaxed);
  NATIVE64_RAW_AVAILABLE.store(u8::from(raw_available), Ordering::Relaxed);
}

#[cfg(all(
  not(feature = "bench-internal"),
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
fn record_native_64_availability(_libc_available: bool, _raw_available: bool) {}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
fn record_native_64_measurements(measurements: &Native64Measurements) {
  for (destination, value) in NATIVE64_LIBC_SAMPLES.iter().zip(measurements.libc) {
    destination.store(value, Ordering::Relaxed);
  }
  for (destination, value) in NATIVE64_RAW_SAMPLES.iter().zip(measurements.raw) {
    destination.store(value, Ordering::Relaxed);
  }
}

#[cfg(all(
  not(feature = "bench-internal"),
  not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"),)),
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
fn record_native_64_measurements(_measurements: &Native64Measurements) {}

#[cfg(all(
  test,
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
#[test]
fn native_64_selection_requires_a_repeatable_material_win() {
  let incumbent = [100_000; NATIVE64_SAMPLES];
  assert!(!prefer_native_64_candidate([96_000; NATIVE64_SAMPLES], incumbent));
  assert!(prefer_native_64_candidate([95_000; NATIVE64_SAMPLES], incumbent));
  assert!(prefer_native_64_candidate([94_000; NATIVE64_SAMPLES], incumbent));

  let mut two_noisy_batches = [94_000; NATIVE64_SAMPLES];
  two_noisy_batches[0] = 99_000;
  two_noisy_batches[1] = 99_000;
  assert!(prefer_native_64_candidate(two_noisy_batches, incumbent));

  let mut three_noisy_batches = two_noisy_batches;
  three_noisy_batches[2] = 99_000;
  assert!(!prefer_native_64_candidate(three_noisy_batches, incumbent));
  assert_eq!(native_64_mechanism_read_cost(), ThreadCpuReadCost::SystemCall);
}

#[cfg(all(
  test,
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
#[test]
fn native_64_availability_never_selects_a_failed_candidate() {
  assert_eq!(native_64_available_choice(false, false), NATIVE64_WALL);
  assert_eq!(native_64_available_choice(true, false), NATIVE64_LIBC);
  assert_eq!(native_64_available_choice(false, true), NATIVE64_RAW);
  assert_eq!(native_64_available_choice(true, true), NATIVE64_UNKNOWN);
}

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
const LINUX32_NATIVE_UNKNOWN: u8 = 0;
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
const LINUX32_NATIVE_SELECTING: u8 = 1;
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
const LINUX32_NATIVE_LIBC: u8 = 2;
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
const LINUX32_NATIVE_TIME32: u8 = 3;
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
const LINUX32_NATIVE_TIME64: u8 = 4;
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
const LINUX32_NATIVE_WALL: u8 = 5;
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
const LINUX32_NATIVE_WARMUP_READS: usize = 128;
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
const LINUX32_NATIVE_MEASURE_READS: usize = 4_096;
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
const LINUX32_NATIVE_SAMPLES: usize = 9;
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
const LINUX32_NATIVE_REQUIRED_WINS: usize = 8;
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
static LINUX32_NATIVE_CHOICE: AtomicU8 = AtomicU8::new(LINUX32_NATIVE_UNKNOWN);
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
static LINUX32_NATIVE_OWNER_PID: AtomicI32 = AtomicI32::new(0);
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
static LINUX32_NATIVE_OWNER_TID: AtomicI32 = AtomicI32::new(0);
#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
static LINUX32_NATIVE_PROBE_CHOICE: AtomicU8 = AtomicU8::new(LINUX32_NATIVE_LIBC);
#[cfg(all(
  feature = "bench-internal",
  any(target_arch = "x86", target_arch = "arm"),
  target_os = "linux",
))]
static LINUX32_LIBC_AVAILABLE: AtomicU8 = AtomicU8::new(0);
#[cfg(all(
  feature = "bench-internal",
  any(target_arch = "x86", target_arch = "arm"),
  target_os = "linux",
))]
static LINUX32_TIME32_AVAILABLE: AtomicU8 = AtomicU8::new(0);
#[cfg(all(
  feature = "bench-internal",
  any(target_arch = "x86", target_arch = "arm"),
  target_os = "linux",
))]
static LINUX32_TIME64_AVAILABLE: AtomicU8 = AtomicU8::new(0);
#[cfg(all(
  feature = "bench-internal",
  any(target_arch = "x86", target_arch = "arm"),
  target_os = "linux",
))]
static LINUX32_LIBC_SAMPLES: [core::sync::atomic::AtomicU64; LINUX32_NATIVE_SAMPLES] =
  [const { core::sync::atomic::AtomicU64::new(0) }; LINUX32_NATIVE_SAMPLES];
#[cfg(all(
  feature = "bench-internal",
  any(target_arch = "x86", target_arch = "arm"),
  target_os = "linux",
))]
static LINUX32_TIME32_SAMPLES: [core::sync::atomic::AtomicU64; LINUX32_NATIVE_SAMPLES] =
  [const { core::sync::atomic::AtomicU64::new(0) }; LINUX32_NATIVE_SAMPLES];
#[cfg(all(
  feature = "bench-internal",
  any(target_arch = "x86", target_arch = "arm"),
  target_os = "linux",
))]
static LINUX32_TIME64_SAMPLES: [core::sync::atomic::AtomicU64; LINUX32_NATIVE_SAMPLES] =
  [const { core::sync::atomic::AtomicU64::new(0) }; LINUX32_NATIVE_SAMPLES];

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
#[repr(C)]
struct LinuxKernelTimespec {
  seconds: i64,
  nanos: i64,
}

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn linux_32_now_nanos() -> u64 {
  loop {
    match LINUX32_NATIVE_CHOICE.load(Ordering::Relaxed) {
      LINUX32_NATIVE_TIME32 => {
        if let Some(value) = linux_32_time32_nanos() {
          return value;
        }
        if let Some(value) = linux_32_time64_nanos() {
          LINUX32_NATIVE_CHOICE.store(LINUX32_NATIVE_TIME64, Ordering::Relaxed);
          return value;
        }
        if let Some(value) = linux_32_libc_nanos() {
          LINUX32_NATIVE_CHOICE.store(LINUX32_NATIVE_LIBC, Ordering::Relaxed);
          return value;
        }
        LINUX32_NATIVE_CHOICE.store(LINUX32_NATIVE_WALL, Ordering::Relaxed);
        return wall_now_value();
      }
      LINUX32_NATIVE_TIME64 => {
        if let Some(value) = linux_32_time64_nanos() {
          return value;
        }
        if let Some(value) = linux_32_time32_nanos() {
          LINUX32_NATIVE_CHOICE.store(LINUX32_NATIVE_TIME32, Ordering::Relaxed);
          return value;
        }
        if let Some(value) = linux_32_libc_nanos() {
          LINUX32_NATIVE_CHOICE.store(LINUX32_NATIVE_LIBC, Ordering::Relaxed);
          return value;
        }
        LINUX32_NATIVE_CHOICE.store(LINUX32_NATIVE_WALL, Ordering::Relaxed);
        return wall_now_value();
      }
      LINUX32_NATIVE_LIBC => {
        if let Some(value) = linux_32_libc_nanos() {
          return value;
        }
        if let Some(value) = linux_32_time64_nanos() {
          LINUX32_NATIVE_CHOICE.store(LINUX32_NATIVE_TIME64, Ordering::Relaxed);
          return value;
        }
        if let Some(value) = linux_32_time32_nanos() {
          LINUX32_NATIVE_CHOICE.store(LINUX32_NATIVE_TIME32, Ordering::Relaxed);
          return value;
        }
        LINUX32_NATIVE_CHOICE.store(LINUX32_NATIVE_WALL, Ordering::Relaxed);
        return wall_now_value();
      }
      LINUX32_NATIVE_WALL => return wall_now_value(),
      LINUX32_NATIVE_UNKNOWN | LINUX32_NATIVE_SELECTING => {
        let selected = super::select_same_domain_thread_owned_process_provider(
          &LINUX32_NATIVE_CHOICE,
          LINUX32_NATIVE_UNKNOWN,
          LINUX32_NATIVE_SELECTING,
          &LINUX32_NATIVE_OWNER_PID,
          &LINUX32_NATIVE_OWNER_TID,
          LINUX32_NATIVE_LIBC,
          select_linux_32_native,
        );
        if LINUX32_NATIVE_CHOICE.load(Ordering::Acquire) != selected {
          return linux_32_libc_nanos()
            .or_else(linux_32_time64_nanos)
            .or_else(linux_32_time32_nanos)
            .unwrap_or_else(wall_now_value);
        }
      }
      _ => {
        LINUX32_NATIVE_CHOICE.store(LINUX32_NATIVE_WALL, Ordering::Relaxed);
      }
    }
  }
}

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
fn select_linux_32_native() -> u8 {
  let libc_available = linux_32_libc_nanos().is_some();
  let time32_available = linux_32_time32_nanos().is_some();
  let time64_available = linux_32_time64_nanos().is_some();
  record_linux_32_availability(libc_available, time32_available, time64_available);
  let available_choice =
    linux_32_available_choice(libc_available, time32_available, time64_available);
  let available_count =
    usize::from(libc_available) + usize::from(time32_available) + usize::from(time64_available);
  if available_count <= 1 {
    return available_choice;
  }

  for provider in [LINUX32_NATIVE_TIME32, LINUX32_NATIVE_TIME64, LINUX32_NATIVE_LIBC] {
    LINUX32_NATIVE_PROBE_CHOICE.store(provider, Ordering::Relaxed);
    for _ in 0..LINUX32_NATIVE_WARMUP_READS {
      let _ = black_box(linux_32_probe_nanos());
    }
  }

  let mut time32 = [0; LINUX32_NATIVE_SAMPLES];
  let mut time64 = [0; LINUX32_NATIVE_SAMPLES];
  let mut libc = [0; LINUX32_NATIVE_SAMPLES];
  for sample in 0..LINUX32_NATIVE_SAMPLES {
    let measured = match sample % 3 {
      0 => {
        let time32 = measure_linux_32_native(LINUX32_NATIVE_TIME32);
        let time64 = measure_linux_32_native(LINUX32_NATIVE_TIME64);
        let libc = measure_linux_32_native(LINUX32_NATIVE_LIBC);
        (time32, time64, libc)
      }
      1 => {
        let time64 = measure_linux_32_native(LINUX32_NATIVE_TIME64);
        let libc = measure_linux_32_native(LINUX32_NATIVE_LIBC);
        let time32 = measure_linux_32_native(LINUX32_NATIVE_TIME32);
        (time32, time64, libc)
      }
      _ => {
        let libc = measure_linux_32_native(LINUX32_NATIVE_LIBC);
        let time32 = measure_linux_32_native(LINUX32_NATIVE_TIME32);
        let time64 = measure_linux_32_native(LINUX32_NATIVE_TIME64);
        (time32, time64, libc)
      }
    };
    let (time32_sample, time64_sample, libc_sample) = measured;
    if time32_available {
      let Some(value) = time32_sample else {
        return linux_32_available_choice(libc_available, false, time64_available);
      };
      time32[sample] = value;
    }
    if time64_available {
      let Some(value) = time64_sample else {
        return linux_32_available_choice(libc_available, time32_available, false);
      };
      time64[sample] = value;
    }
    if libc_available {
      let Some(value) = libc_sample else {
        return linux_32_available_choice(false, time32_available, time64_available);
      };
      libc[sample] = value;
    }
  }
  record_linux_32_measurements(libc, time32, time64);

  let mut selected = available_choice;
  let mut selected_samples = match selected {
    LINUX32_NATIVE_TIME32 => time32,
    LINUX32_NATIVE_TIME64 => time64,
    _ => libc,
  };
  for (candidate, samples, available) in [
    (LINUX32_NATIVE_LIBC, libc, libc_available),
    (LINUX32_NATIVE_TIME32, time32, time32_available),
    (LINUX32_NATIVE_TIME64, time64, time64_available),
  ] {
    if available && candidate != selected && prefer_linux_32_candidate(samples, selected_samples) {
      selected = candidate;
      selected_samples = samples;
    }
  }
  selected
}

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
const fn linux_32_available_choice(libc: bool, time32: bool, time64: bool) -> u8 {
  if libc {
    LINUX32_NATIVE_LIBC
  } else if time64 {
    LINUX32_NATIVE_TIME64
  } else if time32 {
    LINUX32_NATIVE_TIME32
  } else {
    LINUX32_NATIVE_WALL
  }
}

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
fn measure_linux_32_native(provider: u8) -> Option<u64> {
  LINUX32_NATIVE_PROBE_CHOICE.store(provider, Ordering::Relaxed);
  let start = linux_32_monotonic_raw_nanos()?;
  for _ in 0..LINUX32_NATIVE_MEASURE_READS {
    black_box(linux_32_probe_nanos()?);
  }
  linux_32_monotonic_raw_nanos()?.checked_sub(start)
}

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn linux_32_probe_nanos() -> Option<u64> {
  match LINUX32_NATIVE_PROBE_CHOICE.load(Ordering::Relaxed) {
    LINUX32_NATIVE_TIME32 => linux_32_time32_nanos(),
    LINUX32_NATIVE_TIME64 => linux_32_time64_nanos(),
    _ => linux_32_libc_nanos(),
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(target_arch = "x86", target_arch = "arm"),
  target_os = "linux",
))]
fn record_linux_32_availability(libc: bool, time32: bool, time64: bool) {
  LINUX32_LIBC_AVAILABLE.store(u8::from(libc), Ordering::Relaxed);
  LINUX32_TIME32_AVAILABLE.store(u8::from(time32), Ordering::Relaxed);
  LINUX32_TIME64_AVAILABLE.store(u8::from(time64), Ordering::Relaxed);
}

#[cfg(all(
  not(feature = "bench-internal"),
  any(target_arch = "x86", target_arch = "arm"),
  target_os = "linux",
))]
fn record_linux_32_availability(_libc: bool, _time32: bool, _time64: bool) {}

#[cfg(all(
  feature = "bench-internal",
  any(target_arch = "x86", target_arch = "arm"),
  target_os = "linux",
))]
fn record_linux_32_measurements(
  libc: [u64; LINUX32_NATIVE_SAMPLES],
  time32: [u64; LINUX32_NATIVE_SAMPLES],
  time64: [u64; LINUX32_NATIVE_SAMPLES],
) {
  for (slot, value) in LINUX32_LIBC_SAMPLES.iter().zip(libc) {
    slot.store(value, Ordering::Relaxed);
  }
  for (slot, value) in LINUX32_TIME32_SAMPLES.iter().zip(time32) {
    slot.store(value, Ordering::Relaxed);
  }
  for (slot, value) in LINUX32_TIME64_SAMPLES.iter().zip(time64) {
    slot.store(value, Ordering::Relaxed);
  }
}

#[cfg(all(
  not(feature = "bench-internal"),
  any(target_arch = "x86", target_arch = "arm"),
  target_os = "linux",
))]
fn record_linux_32_measurements(
  _libc: [u64; LINUX32_NATIVE_SAMPLES],
  _time32: [u64; LINUX32_NATIVE_SAMPLES],
  _time64: [u64; LINUX32_NATIVE_SAMPLES],
) {
}

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
fn linux_32_libc_nanos() -> Option<u64> {
  let value = posix_now_nanos_libc();
  (!crate::thread_cpu::is_wall_value(value)).then_some(value)
}

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
#[inline]
fn linux_32_time32_nanos() -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: SYS_clock_gettime is the target's Linux time32 clock syscall and
  // `value` has the corresponding 32-bit libc timespec layout.
  #[cfg(target_arch = "x86")]
  let status = unsafe {
    linux_i686_clock_gettime(
      libc::SYS_clock_gettime,
      libc::CLOCK_THREAD_CPUTIME_ID,
      value.as_mut_ptr().cast(),
    )
  };
  #[cfg(target_arch = "arm")]
  // SAFETY: the output storage and syscall arguments satisfy
  // `linux_arm_clock_gettime`'s contract.
  let status = unsafe {
    linux_arm_clock_gettime(
      libc::SYS_clock_gettime,
      libc::CLOCK_THREAD_CPUTIME_ID,
      value.as_mut_ptr().cast(),
    )
  };
  if status != 0 {
    return None;
  }
  // SAFETY: the successful syscall initialized the output.
  timespec_to_nanos(unsafe { value.assume_init() })
}

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
#[inline]
fn linux_32_time64_nanos() -> Option<u64> {
  const SYS_CLOCK_GETTIME64: libc::c_long = 403;

  let mut value = MaybeUninit::<LinuxKernelTimespec>::uninit();
  // SAFETY: syscall 403 is clock_gettime64 in the 32-bit Linux UAPI and
  // `value` is writable kernel-timespec storage.
  #[cfg(target_arch = "x86")]
  let status = unsafe {
    linux_i686_clock_gettime(
      SYS_CLOCK_GETTIME64,
      libc::CLOCK_THREAD_CPUTIME_ID,
      value.as_mut_ptr().cast(),
    )
  };
  #[cfg(target_arch = "arm")]
  // SAFETY: the output storage and syscall arguments satisfy
  // `linux_arm_clock_gettime`'s contract.
  let status = unsafe {
    linux_arm_clock_gettime(
      SYS_CLOCK_GETTIME64,
      libc::CLOCK_THREAD_CPUTIME_ID,
      value.as_mut_ptr().cast(),
    )
  };
  if status != 0 {
    return None;
  }
  // SAFETY: the successful syscall initialized the output.
  let value = unsafe { value.assume_init() };
  let seconds = u64::try_from(value.seconds).ok()?;
  let nanos = u32::try_from(value.nanos).ok()?;
  if nanos >= 1_000_000_000 {
    return None;
  }
  seconds.checked_mul(1_000_000_000)?.checked_add(u64::from(nanos))
}

#[cfg(all(target_arch = "x86", target_os = "linux"))]
#[inline(always)]
#[allow(clippy::inline_always)]
unsafe fn linux_i686_clock_gettime(
  number: libc::c_long,
  clock: libc::clockid_t,
  value: *mut core::ffi::c_void,
) -> libc::c_long {
  let status: libc::c_long;
  // SAFETY: Linux i386 passes the number in EAX and the first two arguments
  // in EBX/ECX. The balanced stack operations preserve PIC's reserved EBX;
  // default asm effects conservatively cover kernel memory and flag changes.
  unsafe {
    core::arch::asm!(
      "push ebx",
      "mov ebx, {clock:e}",
      "int 0x80",
      "pop ebx",
      clock = in(reg) clock,
      inlateout("eax") number => status,
      in("ecx") value,
    );
  }
  status
}

#[cfg(all(target_arch = "arm", target_os = "linux"))]
#[inline(always)]
#[allow(clippy::inline_always)]
unsafe fn linux_arm_clock_gettime(
  number: libc::c_long,
  clock: libc::clockid_t,
  value: *mut core::ffi::c_void,
) -> libc::c_long {
  let status: libc::c_long;
  // Linux Arm EABI fixes the syscall number in r7 and the first two arguments
  // in r0/r1. Preserve r7 because LLVM may reserve it as the frame pointer.
  // SAFETY: the caller supplies the clock syscall number and writable output
  // storage. The balanced push/pop preserves Rust's stack and r7 invariants.
  unsafe {
    core::arch::asm!(
      "push {{r7}}",
      "mov r7, {number}",
      "svc 0",
      "pop {{r7}}",
      number = in(reg) number,
      inlateout("r0") clock => status,
      in("r1") value,
      options(preserves_flags),
    );
  }
  status
}

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
fn linux_32_monotonic_raw_nanos() -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: `value` is writable timespec storage and CLOCK_MONOTONIC_RAW is a
  // valid Linux clock id.
  let status = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, value.as_mut_ptr()) };
  if status != 0 {
    return None;
  }
  // SAFETY: clock_gettime initialized the output on success.
  let value = unsafe { value.assume_init() };
  timespec_to_nanos(value)
}

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
fn prefer_linux_32_candidate(
  challenger_samples: [u64; LINUX32_NATIVE_SAMPLES],
  incumbent_samples: [u64; LINUX32_NATIVE_SAMPLES],
) -> bool {
  let challenger_median = median_linux_32(challenger_samples);
  let incumbent_median = median_linux_32(incumbent_samples);
  // These candidates expose the same kernel clock and differ only in entry
  // plumbing. The common 5% equivalence band keeps timer noise from deciding
  // the process-wide route; eight paired wins from nine prevent a noisy single
  // batch from deciding.
  let allowance = (LINUX32_NATIVE_MEASURE_READS as u64).max(incumbent_median / 20);
  let decisive_wins = challenger_samples
    .iter()
    .zip(incumbent_samples)
    .filter(|(challenger, incumbent)| (**challenger).saturating_add(allowance) < *incumbent)
    .count();

  challenger_median.saturating_add(allowance) < incumbent_median
    && decisive_wins >= LINUX32_NATIVE_REQUIRED_WINS
}

#[cfg(all(any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
fn median_linux_32(mut samples: [u64; LINUX32_NATIVE_SAMPLES]) -> u64 {
  samples.sort_unstable();
  samples[LINUX32_NATIVE_SAMPLES / 2]
}

#[cfg(all(test, any(target_arch = "x86", target_arch = "arm"), target_os = "linux",))]
#[test]
fn linux_32_native_selection_requires_a_repeatable_material_win() {
  let incumbent = [1_000_000; LINUX32_NATIVE_SAMPLES];
  assert!(!prefer_linux_32_candidate([975_000; LINUX32_NATIVE_SAMPLES], incumbent));

  let decisive = [940_000; LINUX32_NATIVE_SAMPLES];
  assert!(prefer_linux_32_candidate(decisive, incumbent));

  let mut two_noisy_batches = decisive;
  two_noisy_batches[0] = 990_000;
  two_noisy_batches[1] = 990_000;
  assert!(!prefer_linux_32_candidate(two_noisy_batches, incumbent));
}

#[cfg(all(test, any(target_arch = "x86", target_arch = "arm"), target_os = "linux"))]
#[test]
fn linux_32_availability_never_selects_a_failed_candidate() {
  assert_eq!(linux_32_available_choice(false, false, false), LINUX32_NATIVE_WALL);
  assert_eq!(linux_32_available_choice(true, false, false), LINUX32_NATIVE_LIBC);
  assert_eq!(linux_32_available_choice(false, true, false), LINUX32_NATIVE_TIME32);
  assert_eq!(linux_32_available_choice(false, false, true), LINUX32_NATIVE_TIME64);
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
const RISCV64_NATIVE_UNKNOWN: u8 = 0;
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
const RISCV64_NATIVE_SELECTING: u8 = 1;
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
const RISCV64_NATIVE_LIBC: u8 = 2;
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
const RISCV64_NATIVE_SYSCALL: u8 = 3;
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
const RISCV64_NATIVE_WALL: u8 = 4;
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
const RISCV64_NATIVE_WARMUP_READS: usize = 128;
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
const RISCV64_NATIVE_MEASURE_READS: usize = 4_096;
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
const RISCV64_NATIVE_SAMPLES: usize = 9;
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
const RISCV64_NATIVE_REQUIRED_WINS: usize = 8;
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
static RISCV64_NATIVE_CHOICE: AtomicU8 = AtomicU8::new(RISCV64_NATIVE_UNKNOWN);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
static RISCV64_NATIVE_OWNER_PID: AtomicI32 = AtomicI32::new(0);
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
static RISCV64_NATIVE_OWNER_TID: AtomicI32 = AtomicI32::new(0);
#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
static RISCV64_LIBC_AVAILABLE: AtomicU8 = AtomicU8::new(0);
#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
static RISCV64_SYSCALL_AVAILABLE: AtomicU8 = AtomicU8::new(0);
#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
static RISCV64_LIBC_SAMPLES: [core::sync::atomic::AtomicU64; RISCV64_NATIVE_SAMPLES] =
  [const { core::sync::atomic::AtomicU64::new(0) }; RISCV64_NATIVE_SAMPLES];
#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
static RISCV64_SYSCALL_SAMPLES: [core::sync::atomic::AtomicU64; RISCV64_NATIVE_SAMPLES] =
  [const { core::sync::atomic::AtomicU64::new(0) }; RISCV64_NATIVE_SAMPLES];

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn linux_riscv64_now_nanos() -> u64 {
  loop {
    match RISCV64_NATIVE_CHOICE.load(Ordering::Relaxed) {
      RISCV64_NATIVE_SYSCALL => {
        if let Some(value) = linux_riscv64_syscall_nanos() {
          return value;
        }
        if let Some(value) = linux_riscv64_libc_nanos() {
          RISCV64_NATIVE_CHOICE.store(RISCV64_NATIVE_LIBC, Ordering::Relaxed);
          return value;
        }
        RISCV64_NATIVE_CHOICE.store(RISCV64_NATIVE_WALL, Ordering::Relaxed);
        return wall_now_value();
      }
      RISCV64_NATIVE_LIBC => {
        if let Some(value) = linux_riscv64_libc_nanos() {
          return value;
        }
        if let Some(value) = linux_riscv64_syscall_nanos() {
          RISCV64_NATIVE_CHOICE.store(RISCV64_NATIVE_SYSCALL, Ordering::Relaxed);
          return value;
        }
        RISCV64_NATIVE_CHOICE.store(RISCV64_NATIVE_WALL, Ordering::Relaxed);
        return wall_now_value();
      }
      RISCV64_NATIVE_WALL => return wall_now_value(),
      RISCV64_NATIVE_UNKNOWN | RISCV64_NATIVE_SELECTING => {
        let selected = super::select_same_domain_thread_owned_process_provider(
          &RISCV64_NATIVE_CHOICE,
          RISCV64_NATIVE_UNKNOWN,
          RISCV64_NATIVE_SELECTING,
          &RISCV64_NATIVE_OWNER_PID,
          &RISCV64_NATIVE_OWNER_TID,
          RISCV64_NATIVE_LIBC,
          select_linux_riscv64_native,
        );
        if RISCV64_NATIVE_CHOICE.load(Ordering::Acquire) != selected {
          return linux_riscv64_libc_nanos()
            .or_else(linux_riscv64_syscall_nanos)
            .unwrap_or_else(wall_now_value);
        }
      }
      _ => RISCV64_NATIVE_CHOICE.store(RISCV64_NATIVE_WALL, Ordering::Relaxed),
    }
  }
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
fn select_linux_riscv64_native() -> u8 {
  let syscall_available = linux_riscv64_syscall_nanos().is_some();
  let libc_available = linux_riscv64_libc_nanos().is_some();
  record_riscv64_availability(libc_available, syscall_available);
  let available_choice = linux_riscv64_available_choice(syscall_available, libc_available);
  if available_choice != RISCV64_NATIVE_UNKNOWN {
    return available_choice;
  }

  for _ in 0..RISCV64_NATIVE_WARMUP_READS {
    let _ = black_box(linux_riscv64_syscall_nanos());
    let _ = black_box(linux_riscv64_libc_nanos());
  }

  let mut syscall = [0; RISCV64_NATIVE_SAMPLES];
  let mut libc = [0; RISCV64_NATIVE_SAMPLES];
  for sample in 0..RISCV64_NATIVE_SAMPLES {
    let measured = if sample & 1 == 0 {
      (
        measure_linux_riscv64_native(linux_riscv64_syscall_nanos),
        measure_linux_riscv64_native(linux_riscv64_libc_nanos),
      )
    } else {
      let libc = measure_linux_riscv64_native(linux_riscv64_libc_nanos);
      let syscall = measure_linux_riscv64_native(linux_riscv64_syscall_nanos);
      (syscall, libc)
    };
    let (Some(syscall_sample), Some(libc_sample)) = measured else {
      return if linux_riscv64_syscall_nanos().is_some() {
        RISCV64_NATIVE_SYSCALL
      } else if linux_riscv64_libc_nanos().is_some() {
        RISCV64_NATIVE_LIBC
      } else {
        RISCV64_NATIVE_WALL
      };
    };
    syscall[sample] = syscall_sample;
    libc[sample] = libc_sample;
  }
  record_riscv64_measurements(libc, syscall);

  if prefer_linux_riscv64_syscall(syscall, libc) {
    RISCV64_NATIVE_SYSCALL
  } else {
    RISCV64_NATIVE_LIBC
  }
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
const fn linux_riscv64_available_choice(syscall: bool, libc: bool) -> u8 {
  match (syscall, libc) {
    (false, false) => RISCV64_NATIVE_WALL,
    (true, false) => RISCV64_NATIVE_SYSCALL,
    (false, true) => RISCV64_NATIVE_LIBC,
    (true, true) => RISCV64_NATIVE_UNKNOWN,
  }
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
fn measure_linux_riscv64_native(read: fn() -> Option<u64>) -> Option<u64> {
  let start = linux_riscv64_monotonic_raw_nanos()?;
  for _ in 0..RISCV64_NATIVE_MEASURE_READS {
    black_box(read()?);
  }
  linux_riscv64_monotonic_raw_nanos()?.checked_sub(start)
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
#[inline]
fn linux_riscv64_syscall_nanos() -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: SYS_clock_gettime is the RV64 Linux clock syscall and `value` is
  // writable storage with the kernel's 64-bit timespec layout.
  let status: libc::c_long;
  // SAFETY: Linux RV64 fixes the syscall number in a7 and the first two
  // arguments in a0/a1. The kernel writes only through the supplied pointer.
  unsafe {
    core::arch::asm!(
      "ecall",
      in("a7") libc::SYS_clock_gettime,
      inlateout("a0") libc::c_long::from(libc::CLOCK_THREAD_CPUTIME_ID) => status,
      in("a1") value.as_mut_ptr(),
      options(nostack, preserves_flags),
    );
  }
  if status != 0 {
    return None;
  }
  // SAFETY: the successful syscall initialized the output.
  timespec_to_nanos(unsafe { value.assume_init() })
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
fn linux_riscv64_libc_nanos() -> Option<u64> {
  let value = posix_now_nanos_libc();
  (!crate::thread_cpu::is_wall_value(value)).then_some(value)
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
fn linux_riscv64_monotonic_raw_nanos() -> Option<u64> {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: `value` is writable timespec storage and CLOCK_MONOTONIC_RAW is a
  // valid Linux clock id.
  let status = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, value.as_mut_ptr()) };
  if status != 0 {
    return None;
  }
  // SAFETY: clock_gettime initialized the output on success.
  timespec_to_nanos(unsafe { value.assume_init() })
}

#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
fn record_riscv64_availability(libc: bool, syscall: bool) {
  RISCV64_LIBC_AVAILABLE.store(u8::from(libc), Ordering::Relaxed);
  RISCV64_SYSCALL_AVAILABLE.store(u8::from(syscall), Ordering::Relaxed);
}

#[cfg(all(not(feature = "bench-internal"), target_arch = "riscv64", target_os = "linux"))]
fn record_riscv64_availability(_libc: bool, _syscall: bool) {}

#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
fn record_riscv64_measurements(
  libc: [u64; RISCV64_NATIVE_SAMPLES],
  syscall: [u64; RISCV64_NATIVE_SAMPLES],
) {
  for (slot, value) in RISCV64_LIBC_SAMPLES.iter().zip(libc) {
    slot.store(value, Ordering::Relaxed);
  }
  for (slot, value) in RISCV64_SYSCALL_SAMPLES.iter().zip(syscall) {
    slot.store(value, Ordering::Relaxed);
  }
}

#[cfg(all(not(feature = "bench-internal"), target_arch = "riscv64", target_os = "linux"))]
fn record_riscv64_measurements(
  _libc: [u64; RISCV64_NATIVE_SAMPLES],
  _syscall: [u64; RISCV64_NATIVE_SAMPLES],
) {
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
fn prefer_linux_riscv64_syscall(
  syscall_samples: [u64; RISCV64_NATIVE_SAMPLES],
  libc_samples: [u64; RISCV64_NATIVE_SAMPLES],
) -> bool {
  let syscall_median = median_linux_riscv64(syscall_samples);
  let libc_median = median_linux_riscv64(libc_samples);
  let allowance = (RISCV64_NATIVE_MEASURE_READS as u64).max(libc_median / 20);
  let decisive_wins = syscall_samples
    .iter()
    .zip(libc_samples)
    .filter(|(syscall, libc)| (**syscall).saturating_add(allowance) < *libc)
    .count();

  syscall_median.saturating_add(allowance) < libc_median
    && decisive_wins >= RISCV64_NATIVE_REQUIRED_WINS
}

#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
fn median_linux_riscv64(mut samples: [u64; RISCV64_NATIVE_SAMPLES]) -> u64 {
  samples.sort_unstable();
  samples[RISCV64_NATIVE_SAMPLES / 2]
}

#[cfg(all(test, target_arch = "riscv64", target_os = "linux"))]
#[test]
fn linux_riscv64_native_selection_requires_a_repeatable_material_win() {
  let incumbent = [1_000_000; RISCV64_NATIVE_SAMPLES];
  assert!(!prefer_linux_riscv64_syscall([975_000; RISCV64_NATIVE_SAMPLES], incumbent,));
  assert!(prefer_linux_riscv64_syscall([940_000; RISCV64_NATIVE_SAMPLES], incumbent,));
}

#[cfg(all(test, target_arch = "riscv64", target_os = "linux"))]
#[test]
fn linux_riscv64_availability_never_selects_a_failed_candidate() {
  assert_eq!(linux_riscv64_available_choice(false, false), RISCV64_NATIVE_WALL);
  assert_eq!(linux_riscv64_available_choice(true, false), RISCV64_NATIVE_SYSCALL);
  assert_eq!(linux_riscv64_available_choice(false, true), RISCV64_NATIVE_LIBC);
  assert_eq!(linux_riscv64_available_choice(true, true), RISCV64_NATIVE_UNKNOWN);
}

#[cfg(all(
  target_os = "linux",
  any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
))]
mod linux_rare {
  use core::hint::black_box;
  use core::mem::MaybeUninit;
  #[cfg(feature = "bench-internal")]
  use core::sync::atomic::AtomicU64;
  use core::sync::atomic::{AtomicI32, AtomicU8, Ordering};

  const NATIVE_UNKNOWN: u8 = 0;
  const NATIVE_SELECTING: u8 = 1;
  const NATIVE_LIBC: u8 = 2;
  const NATIVE_SYSCALL: u8 = 3;
  const NATIVE_SCV: u8 = 4;
  const NATIVE_WALL: u8 = 5;

  const WARMUP_READS: usize = 128;
  const MEASURE_READS: usize = 4_096;
  const MEASURE_SAMPLES: usize = 9;
  const REQUIRED_DECISIVE_WINS: usize = 8;

  static NATIVE_CHOICE: AtomicU8 = AtomicU8::new(NATIVE_UNKNOWN);
  static NATIVE_OWNER_PID: AtomicI32 = AtomicI32::new(0);
  static NATIVE_OWNER_TID: AtomicI32 = AtomicI32::new(0);
  static PROBE_CHOICE: AtomicU8 = AtomicU8::new(NATIVE_LIBC);
  #[cfg(feature = "bench-internal")]
  static MEASURED_SYSCALL_SAMPLES: [AtomicU64; MEASURE_SAMPLES] =
    [const { AtomicU64::new(0) }; MEASURE_SAMPLES];
  #[cfg(feature = "bench-internal")]
  static MEASURED_LIBC_SAMPLES: [AtomicU64; MEASURE_SAMPLES] =
    [const { AtomicU64::new(0) }; MEASURE_SAMPLES];
  #[cfg(feature = "bench-internal")]
  static MEASURED_SCV_SAMPLES: [AtomicU64; MEASURE_SAMPLES] =
    [const { AtomicU64::new(0) }; MEASURE_SAMPLES];

  struct Measurements {
    syscall: Option<[u64; MEASURE_SAMPLES]>,
    libc: Option<[u64; MEASURE_SAMPLES]>,
    scv: Option<[u64; MEASURE_SAMPLES]>,
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(super) fn now_nanos() -> u64 {
    loop {
      match NATIVE_CHOICE.load(Ordering::Relaxed) {
        NATIVE_LIBC => {
          if let Some(value) = libc_nanos() {
            return value;
          }
          if let Some(value) = syscall_nanos() {
            NATIVE_CHOICE.store(NATIVE_SYSCALL, Ordering::Relaxed);
            return value;
          }
          if let Some(value) = scv_nanos() {
            NATIVE_CHOICE.store(NATIVE_SCV, Ordering::Relaxed);
            return value;
          }
          NATIVE_CHOICE.store(NATIVE_WALL, Ordering::Relaxed);
          return super::wall_now_value();
        }
        NATIVE_SYSCALL => {
          if let Some(value) = syscall_nanos() {
            return value;
          }
          if let Some(value) = scv_nanos() {
            NATIVE_CHOICE.store(NATIVE_SCV, Ordering::Relaxed);
            return value;
          }
          if let Some(value) = libc_nanos() {
            NATIVE_CHOICE.store(NATIVE_LIBC, Ordering::Relaxed);
            return value;
          }
          NATIVE_CHOICE.store(NATIVE_WALL, Ordering::Relaxed);
          return super::wall_now_value();
        }
        NATIVE_SCV => {
          if let Some(value) = scv_nanos() {
            return value;
          }
          if let Some(value) = syscall_nanos() {
            NATIVE_CHOICE.store(NATIVE_SYSCALL, Ordering::Relaxed);
            return value;
          }
          if let Some(value) = libc_nanos() {
            NATIVE_CHOICE.store(NATIVE_LIBC, Ordering::Relaxed);
            return value;
          }
          NATIVE_CHOICE.store(NATIVE_WALL, Ordering::Relaxed);
          return super::wall_now_value();
        }
        NATIVE_WALL => return super::wall_now_value(),
        NATIVE_UNKNOWN | NATIVE_SELECTING => {
          let selected = super::super::select_same_domain_thread_owned_process_provider(
            &NATIVE_CHOICE,
            NATIVE_UNKNOWN,
            NATIVE_SELECTING,
            &NATIVE_OWNER_PID,
            &NATIVE_OWNER_TID,
            NATIVE_LIBC,
            select_native,
          );
          if NATIVE_CHOICE.load(Ordering::Acquire) != selected {
            return libc_nanos()
              .or_else(syscall_nanos)
              .or_else(|| powerpc64_scv_available().then(scv_nanos).flatten())
              .unwrap_or_else(super::wall_now_value);
          }
        }
        _ => NATIVE_CHOICE.store(NATIVE_WALL, Ordering::Relaxed),
      }
    }
  }

  fn select_native() -> u8 {
    let libc_available = libc_nanos().is_some();
    let syscall_available = syscall_nanos().is_some();
    let scv_available = powerpc64_scv_available() && scv_nanos().is_some();

    let available_choice = available_choice(libc_available, syscall_available, scv_available);
    if available_choice != NATIVE_UNKNOWN {
      return available_choice;
    }

    let Some(measurements) = measure_candidates(libc_available, syscall_available, scv_available)
    else {
      return available_choice;
    };
    record_measurements(&measurements);

    let candidates = [
      (NATIVE_LIBC, measurements.libc),
      (NATIVE_SYSCALL, measurements.syscall),
      (NATIVE_SCV, measurements.scv),
    ];
    let Some((mut selected, mut selected_samples)) = candidates
      .iter()
      .find_map(|(provider, samples)| samples.map(|value| (*provider, value)))
    else {
      return NATIVE_WALL;
    };
    for (candidate, samples) in candidates {
      if candidate != selected
        && let Some(samples) = samples
        && prefer_candidate(samples, selected_samples)
      {
        selected = candidate;
        selected_samples = samples;
      }
    }
    selected
  }

  const fn available_choice(libc: bool, syscall: bool, scv: bool) -> u8 {
    match (libc as u8 + syscall as u8 + scv as u8, libc, syscall, scv) {
      (0, _, _, _) => NATIVE_WALL,
      (1, true, _, _) => NATIVE_LIBC,
      (1, _, true, _) => NATIVE_SYSCALL,
      (1, _, _, true) => NATIVE_SCV,
      _ => NATIVE_UNKNOWN,
    }
  }

  fn measure_candidates(
    libc_available: bool,
    syscall_available: bool,
    scv_available: bool,
  ) -> Option<Measurements> {
    for choice in [NATIVE_SYSCALL, NATIVE_SCV, NATIVE_LIBC] {
      if (choice == NATIVE_LIBC && !libc_available)
        || (choice == NATIVE_SYSCALL && !syscall_available)
        || (choice == NATIVE_SCV && !scv_available)
      {
        continue;
      }
      PROBE_CHOICE.store(choice, Ordering::Relaxed);
      for _ in 0..WARMUP_READS {
        black_box(probe_now()?);
      }
    }

    let mut syscall = syscall_available.then_some([0; MEASURE_SAMPLES]);
    let mut libc = libc_available.then_some([0; MEASURE_SAMPLES]);
    let mut scv = scv_available.then_some([0; MEASURE_SAMPLES]);
    for sample in 0..MEASURE_SAMPLES {
      let mut measure = |choice| {
        let value = measure_candidate(choice)?;
        match choice {
          NATIVE_SYSCALL => syscall.as_mut()?[sample] = value,
          NATIVE_LIBC => libc.as_mut()?[sample] = value,
          NATIVE_SCV => scv.as_mut()?[sample] = value,
          _ => return None,
        }
        Some(())
      };

      match sample % 3 {
        0 => {
          if syscall_available {
            measure(NATIVE_SYSCALL)?;
          }
          if scv_available {
            measure(NATIVE_SCV)?;
          }
          if libc_available {
            measure(NATIVE_LIBC)?;
          }
        }
        1 => {
          if scv_available {
            measure(NATIVE_SCV)?;
          }
          if libc_available {
            measure(NATIVE_LIBC)?;
          }
          if syscall_available {
            measure(NATIVE_SYSCALL)?;
          }
        }
        _ => {
          if libc_available {
            measure(NATIVE_LIBC)?;
          }
          if syscall_available {
            measure(NATIVE_SYSCALL)?;
          }
          if scv_available {
            measure(NATIVE_SCV)?;
          }
        }
      }
    }

    Some(Measurements { syscall, libc, scv })
  }

  fn measure_candidate(choice: u8) -> Option<u64> {
    PROBE_CHOICE.store(choice, Ordering::Relaxed);
    let start = monotonic_raw_nanos()?;
    for _ in 0..MEASURE_READS {
      black_box(probe_now()?);
    }
    monotonic_raw_nanos()?.checked_sub(start)
  }

  #[inline(always)]
  #[allow(clippy::inline_always)]
  fn probe_now() -> Option<u64> {
    match PROBE_CHOICE.load(Ordering::Relaxed) {
      NATIVE_SYSCALL => syscall_nanos(),
      NATIVE_SCV => scv_nanos(),
      NATIVE_LIBC => libc_nanos(),
      _ => None,
    }
  }

  #[inline]
  fn syscall_nanos() -> Option<u64> {
    let mut value = MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: the pointer addresses writable storage with this target's Linux
    // kernel timespec layout.
    if !unsafe { raw_clock_gettime(value.as_mut_ptr()) } {
      return None;
    }
    // SAFETY: a successful clock syscall initialized the output.
    super::timespec_to_nanos(unsafe { value.assume_init() })
  }

  #[cfg(target_arch = "loongarch64")]
  unsafe fn raw_clock_gettime(value: *mut libc::timespec) -> bool {
    let status: libc::c_long;
    // SAFETY: LoongArch Linux takes the syscall number in A7 and the first two
    // arguments in A0/A1. The kernel writes only through `value`.
    unsafe {
      core::arch::asm!(
        "syscall 0",
        in("$a7") libc::SYS_clock_gettime,
        inlateout("$a0") libc::c_long::from(libc::CLOCK_THREAD_CPUTIME_ID) => status,
        in("$a1") value,
        lateout("$t0") _,
        lateout("$t1") _,
        lateout("$t2") _,
        lateout("$t3") _,
        lateout("$t4") _,
        lateout("$t5") _,
        lateout("$t6") _,
        lateout("$t7") _,
        lateout("$t8") _,
        options(nostack, preserves_flags),
      );
    }
    status == 0
  }

  #[cfg(target_arch = "s390x")]
  unsafe fn raw_clock_gettime(value: *mut libc::timespec) -> bool {
    let status: libc::c_long;
    // SAFETY: s390x Linux takes the syscall number in r1 and the first two
    // arguments in r2/r3. The kernel writes only through `value`.
    unsafe {
      core::arch::asm!(
        "svc 0",
        in("r1") libc::SYS_clock_gettime,
        inlateout("r2") libc::c_long::from(libc::CLOCK_THREAD_CPUTIME_ID) => status,
        in("r3") value,
        options(nostack, preserves_flags),
      );
    }
    status == 0
  }

  #[cfg(target_arch = "powerpc64")]
  unsafe fn raw_clock_gettime(value: *mut libc::timespec) -> bool {
    let status: libc::c_long;
    // SAFETY: PowerPC64 Linux takes the syscall number in r0 and the first two
    // arguments in r3/r4. All kernel-volatile registers are declared.
    unsafe {
      core::arch::asm!(
        "sc",
        inlateout("r0") libc::SYS_clock_gettime => _,
        inlateout("r3") libc::c_long::from(libc::CLOCK_THREAD_CPUTIME_ID) => status,
        inlateout("r4") value => _,
        lateout("r5") _,
        lateout("r6") _,
        lateout("r7") _,
        lateout("r8") _,
        lateout("r9") _,
        lateout("r10") _,
        lateout("r11") _,
        lateout("r12") _,
        lateout("cr0") _,
        lateout("xer") _,
        lateout("ctr") _,
        options(nostack),
      );
    }
    status == 0
  }

  #[inline]
  fn scv_nanos() -> Option<u64> {
    #[cfg(target_arch = "powerpc64")]
    {
      let mut value = MaybeUninit::<libc::timespec>::uninit();
      // SAFETY: only selection after a successful HWCAP2 check can publish or
      // probe this route, and `value` is writable timespec storage.
      if !unsafe { powerpc64_scv_clock_gettime(value.as_mut_ptr()) } {
        return None;
      }
      // SAFETY: a successful clock syscall initialized the output.
      super::timespec_to_nanos(unsafe { value.assume_init() })
    }
    #[cfg(not(target_arch = "powerpc64"))]
    {
      None
    }
  }

  #[cfg(target_arch = "powerpc64")]
  unsafe fn powerpc64_scv_clock_gettime(value: *mut libc::timespec) -> bool {
    let status: libc::c_long;
    // SAFETY: PowerPC64 Linux's SCV ABI uses r0 and r3-r8. The assembler mode
    // enables the instruction without raising the crate's baseline ISA.
    unsafe {
      core::arch::asm!(
        ".machine push",
        ".machine power9",
        "scv 0",
        ".machine pop",
        inlateout("r0") libc::SYS_clock_gettime => _,
        inlateout("r3") libc::c_long::from(libc::CLOCK_THREAD_CPUTIME_ID) => status,
        inlateout("r4") value => _,
        lateout("r5") _,
        lateout("r6") _,
        lateout("r7") _,
        lateout("r8") _,
        lateout("r9") _,
        lateout("r10") _,
        lateout("r11") _,
        lateout("r12") _,
        lateout("cr0") _,
        lateout("cr1") _,
        lateout("cr5") _,
        lateout("cr6") _,
        lateout("cr7") _,
        lateout("xer") _,
        lateout("lr") _,
        lateout("ctr") _,
        options(nostack),
      );
    }
    status == 0
  }

  #[inline]
  fn powerpc64_scv_available() -> bool {
    #[cfg(target_arch = "powerpc64")]
    {
      const AT_HWCAP2: libc::c_ulong = 26;
      const PPC_FEATURE2_SCV: libc::c_ulong = 0x0010_0000;
      // SAFETY: `getauxval` reads immutable process startup metadata.
      unsafe { libc::getauxval(AT_HWCAP2) & PPC_FEATURE2_SCV != 0 }
    }
    #[cfg(not(target_arch = "powerpc64"))]
    {
      false
    }
  }

  #[inline]
  fn libc_nanos() -> Option<u64> {
    let value = super::posix_now_nanos_libc();
    (!crate::thread_cpu::is_wall_value(value)).then_some(value)
  }

  fn monotonic_raw_nanos() -> Option<u64> {
    let mut value = MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: `value` is writable storage and CLOCK_MONOTONIC_RAW is a valid
    // Linux clock id.
    let status = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, value.as_mut_ptr()) };
    if status != 0 {
      return None;
    }
    // SAFETY: a successful clock_gettime initialized the output.
    super::timespec_to_nanos(unsafe { value.assume_init() })
  }

  fn prefer_candidate(
    challenger_samples: [u64; MEASURE_SAMPLES],
    incumbent_samples: [u64; MEASURE_SAMPLES],
  ) -> bool {
    let challenger_median = median(challenger_samples);
    let incumbent_median = median(incumbent_samples);
    let allowance = (MEASURE_READS as u64).max(incumbent_median / 20);
    let decisive_wins = challenger_samples
      .iter()
      .zip(incumbent_samples)
      .filter(|(challenger, incumbent)| (**challenger).saturating_add(allowance) < *incumbent)
      .count();

    challenger_median.saturating_add(allowance) < incumbent_median
      && decisive_wins >= REQUIRED_DECISIVE_WINS
  }

  fn median(mut samples: [u64; MEASURE_SAMPLES]) -> u64 {
    samples.sort_unstable();
    samples[MEASURE_SAMPLES / 2]
  }

  #[cfg(feature = "bench-internal")]
  fn record_measurements(measurements: &Measurements) {
    if let Some(samples) = measurements.syscall {
      for (destination, value) in MEASURED_SYSCALL_SAMPLES.iter().zip(samples) {
        destination.store(value, Ordering::Relaxed);
      }
    }
    if let Some(samples) = measurements.libc {
      for (destination, value) in MEASURED_LIBC_SAMPLES.iter().zip(samples) {
        destination.store(value, Ordering::Relaxed);
      }
    }
    if let Some(samples) = measurements.scv {
      for (destination, value) in MEASURED_SCV_SAMPLES.iter().zip(samples) {
        destination.store(value, Ordering::Relaxed);
      }
    }
  }

  #[cfg(not(feature = "bench-internal"))]
  fn record_measurements(_measurements: &Measurements) {}

  #[cfg(feature = "bench-internal")]
  #[allow(dead_code)]
  pub(super) fn bench_provider() -> &'static str {
    let _ = now_nanos();
    match NATIVE_CHOICE.load(Ordering::Relaxed) {
      NATIVE_SYSCALL => "raw_clock_gettime_syscall",
      NATIVE_SCV => "raw_clock_gettime_scv",
      NATIVE_WALL => "monotonic_wall_fallback",
      _ => "libc_clock_gettime",
    }
  }

  #[cfg(feature = "bench-internal")]
  #[allow(dead_code)]
  pub(super) fn bench_measurements()
  -> Option<([u64; MEASURE_SAMPLES], [u64; MEASURE_SAMPLES], Option<[u64; MEASURE_SAMPLES]>, usize)>
  {
    let _ = now_nanos();
    let mut syscall = [0; MEASURE_SAMPLES];
    let mut libc = [0; MEASURE_SAMPLES];
    let mut scv = [0; MEASURE_SAMPLES];
    for (value, sample) in syscall.iter_mut().zip(&MEASURED_SYSCALL_SAMPLES) {
      *value = sample.load(Ordering::Relaxed);
    }
    for (value, sample) in libc.iter_mut().zip(&MEASURED_LIBC_SAMPLES) {
      *value = sample.load(Ordering::Relaxed);
    }
    for (value, sample) in scv.iter_mut().zip(&MEASURED_SCV_SAMPLES) {
      *value = sample.load(Ordering::Relaxed);
    }
    if syscall.contains(&0) || libc.contains(&0) {
      None
    } else {
      Some((syscall, libc, (!scv.contains(&0)).then_some(scv), MEASURE_READS))
    }
  }

  #[cfg(feature = "bench-internal")]
  pub(super) fn bench_selection_evidence() -> (
    [&'static str; 3],
    [bool; 3],
    [bool; 3],
    [[u64; MEASURE_SAMPLES]; 3],
    &'static str,
    usize,
    usize,
  ) {
    let selected = bench_provider();
    let eligible = [
      libc_nanos().is_some(),
      syscall_nanos().is_some(),
      powerpc64_scv_available() && scv_nanos().is_some(),
    ];
    let mut syscall = [0; MEASURE_SAMPLES];
    let mut libc = [0; MEASURE_SAMPLES];
    let mut scv = [0; MEASURE_SAMPLES];
    for (value, sample) in syscall.iter_mut().zip(&MEASURED_SYSCALL_SAMPLES) {
      *value = sample.load(Ordering::Relaxed);
    }
    for (value, sample) in libc.iter_mut().zip(&MEASURED_LIBC_SAMPLES) {
      *value = sample.load(Ordering::Relaxed);
    }
    for (value, sample) in scv.iter_mut().zip(&MEASURED_SCV_SAMPLES) {
      *value = sample.load(Ordering::Relaxed);
    }
    let measured = [!libc.contains(&0), !syscall.contains(&0), !scv.contains(&0)];
    (
      ["libc_clock_gettime", raw_provider_name(), scv_provider_name()],
      eligible,
      measured,
      [libc, syscall, scv],
      selected,
      MEASURE_READS,
      REQUIRED_DECISIVE_WINS,
    )
  }

  #[cfg(feature = "bench-internal")]
  const fn raw_provider_name() -> &'static str {
    #[cfg(target_arch = "s390x")]
    {
      "linux_s390x_raw_syscall_clock_thread_cputime"
    }
    #[cfg(target_arch = "loongarch64")]
    {
      "linux_loongarch64_raw_syscall_clock_thread_cputime"
    }
    #[cfg(target_arch = "powerpc64")]
    {
      "linux_powerpc64_raw_sc_clock_thread_cputime"
    }
  }

  #[cfg(feature = "bench-internal")]
  const fn scv_provider_name() -> &'static str {
    #[cfg(target_arch = "powerpc64")]
    {
      "linux_powerpc64_raw_scv_clock_thread_cputime"
    }
    #[cfg(not(target_arch = "powerpc64"))]
    {
      "unavailable"
    }
  }

  #[cfg(feature = "bench-internal")]
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(super) fn bench_exact_candidate(index: usize) -> Option<u64> {
    match index {
      0 => libc_nanos(),
      1 => syscall_nanos(),
      2 if powerpc64_scv_available() => scv_nanos(),
      _ => None,
    }
  }

  #[cfg(test)]
  #[test]
  fn native_selection_requires_a_repeatable_material_win() {
    let incumbent = [100_000; MEASURE_SAMPLES];
    assert!(!prefer_candidate([96_000; MEASURE_SAMPLES], incumbent));
    assert!(prefer_candidate([94_000; MEASURE_SAMPLES], incumbent));

    let mut two_noisy_batches = [94_000; MEASURE_SAMPLES];
    two_noisy_batches[0] = 99_000;
    two_noisy_batches[1] = 99_000;
    assert!(!prefer_candidate(two_noisy_batches, incumbent));
  }

  #[cfg(test)]
  #[test]
  fn availability_never_selects_a_failed_candidate() {
    assert_eq!(available_choice(false, false, false), NATIVE_WALL);
    assert_eq!(available_choice(false, true, false), NATIVE_SYSCALL);
    assert_eq!(available_choice(false, false, true), NATIVE_SCV);
    assert_eq!(available_choice(true, false, false), NATIVE_LIBC);
    assert_eq!(available_choice(true, true, false), NATIVE_UNKNOWN);
    assert_eq!(available_choice(false, true, true), NATIVE_UNKNOWN);
    assert_eq!(available_choice(true, false, true), NATIVE_UNKNOWN);
    assert_eq!(available_choice(true, true, true), NATIVE_UNKNOWN);
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Native64SelectionEvidence {
  pub(crate) selection_kind: &'static str,
  pub(crate) selection_basis: Option<&'static str>,
  pub(crate) selected_provider: &'static str,
  pub(crate) selected_read_cost: &'static str,
  pub(crate) libc_provider: &'static str,
  pub(crate) raw_provider: &'static str,
  pub(crate) libc_available: bool,
  pub(crate) raw_available: bool,
  pub(crate) reads_per_batch: usize,
  pub(crate) required_decisive_wins: usize,
  pub(crate) floor_ns_per_read: u64,
  pub(crate) relative_denominator: Option<u64>,
  pub(crate) libc_batches_ns: [u64; NATIVE64_SAMPLES],
  pub(crate) raw_batches_ns: [u64; NATIVE64_SAMPLES],
  pub(crate) libc_median_ns: u64,
  pub(crate) raw_median_ns: u64,
  pub(crate) raw_allowance_ns: u64,
  pub(crate) raw_decisive_wins: usize,
  pub(crate) raw_selected: bool,
  pub(crate) libc_allowance_ns: u64,
  pub(crate) libc_decisive_wins: usize,
  pub(crate) libc_materially_faster: bool,
}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
pub(crate) fn bench_native_64_selection_evidence() -> Native64SelectionEvidence {
  let _ = native_64_now_nanos();
  let mut libc = [0; NATIVE64_SAMPLES];
  let mut raw = [0; NATIVE64_SAMPLES];
  for (value, sample) in libc.iter_mut().zip(&NATIVE64_LIBC_SAMPLES) {
    *value = sample.load(Ordering::Relaxed);
  }
  for (value, sample) in raw.iter_mut().zip(&NATIVE64_RAW_SAMPLES) {
    *value = sample.load(Ordering::Relaxed);
  }
  let (raw_decision, libc_decision) = if libc.contains(&0) || raw.contains(&0) {
    let unavailable =
      Native64Decision { allowance: 0, decisive_wins: 0, challenger_selected: false };
    (
      Native64Decision {
        allowance: unavailable.allowance,
        decisive_wins: unavailable.decisive_wins,
        challenger_selected: unavailable.challenger_selected,
      },
      unavailable,
    )
  } else {
    (evaluate_native_64_candidate(raw, libc), evaluate_native_64_candidate(libc, raw))
  };
  Native64SelectionEvidence {
    selection_kind: if cfg!(all(
      target_os = "linux",
      any(target_arch = "aarch64", target_arch = "x86_64"),
    )) {
      "raw_syscall_preferred_with_performance_audit"
    } else {
      "tournament"
    },
    selection_basis: cfg!(all(
      target_os = "linux",
      any(target_arch = "aarch64", target_arch = "x86_64"),
    ))
    .then_some(
      "the inlined raw Linux syscall is the native primitive; libc wraps the same kernel clock and remains the failure fallback",
    ),
    selected_provider: bench_native_64_selected_provider(),
    selected_read_cost: match native_64_read_cost() {
      ThreadCpuReadCost::Inline => "inline",
      ThreadCpuReadCost::SystemCall => "system call",
      ThreadCpuReadCost::HostCall => "host call",
      ThreadCpuReadCost::Unavailable => "unavailable",
    },
    libc_provider: native_64_libc_provider_name(),
    raw_provider: native_64_raw_provider_name(),
    libc_available: NATIVE64_LIBC_AVAILABLE.load(Ordering::Relaxed) != 0,
    raw_available: NATIVE64_RAW_AVAILABLE.load(Ordering::Relaxed) != 0,
    reads_per_batch: NATIVE64_MEASURE_READS,
    required_decisive_wins: NATIVE64_REQUIRED_WINS,
    floor_ns_per_read: 1,
    relative_denominator: None,
    libc_batches_ns: libc,
    raw_batches_ns: raw,
    libc_median_ns: if libc.contains(&0) { 0 } else { median_native_64(libc) },
    raw_median_ns: if raw.contains(&0) { 0 } else { median_native_64(raw) },
    raw_allowance_ns: raw_decision.allowance,
    raw_decisive_wins: raw_decision.decisive_wins,
    raw_selected: raw_decision.challenger_selected,
    libc_allowance_ns: libc_decision.allowance,
    libc_decisive_wins: libc_decision.decisive_wins,
    libc_materially_faster: libc_decision.challenger_selected,
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
pub(crate) fn bench_native_64_selected_provider() -> &'static str {
  let _ = native_64_now_nanos();
  match NATIVE64_CHOICE.load(Ordering::Relaxed) {
    NATIVE64_RAW => native_64_raw_provider_name(),
    NATIVE64_WALL => "monotonic_wall_fallback",
    _ => native_64_libc_provider_name(),
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
pub(crate) fn bench_native_64_libc_provider() -> &'static str {
  native_64_libc_provider_name()
}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
pub(crate) fn bench_native_64_raw_provider() -> &'static str {
  native_64_raw_provider_name()
}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_native_64_libc_nanos() -> u64 {
  native_64_libc_nanos().unwrap_or(0)
}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_native_64_raw_nanos() -> u64 {
  native_64_raw_nanos().unwrap_or(0)
}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
const fn native_64_libc_provider_name() -> &'static str {
  #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
  {
    "linux_x86_64_libc_clock_thread_cputime"
  }
  #[cfg(all(target_arch = "x86_64", target_os = "android"))]
  {
    "android_x86_64_libc_clock_thread_cputime"
  }
  #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
  {
    "freebsd_x86_64_libc_clock_thread_cputime"
  }
  #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
  {
    "linux_aarch64_libc_clock_thread_cputime"
  }
  #[cfg(all(target_arch = "aarch64", target_os = "android"))]
  {
    "android_aarch64_libc_clock_thread_cputime"
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
))]
const fn native_64_raw_provider_name() -> &'static str {
  #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
  {
    "linux_x86_64_raw_syscall_clock_thread_cputime"
  }
  #[cfg(all(target_arch = "x86_64", target_os = "android"))]
  {
    "android_x86_64_raw_syscall_clock_thread_cputime"
  }
  #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
  {
    "freebsd_x86_64_raw_syscall_clock_thread_cputime"
  }
  #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
  {
    "linux_aarch64_raw_syscall_clock_thread_cputime"
  }
  #[cfg(all(target_arch = "aarch64", target_os = "android"))]
  {
    "android_aarch64_raw_syscall_clock_thread_cputime"
  }
}

#[cfg(all(
  feature = "bench-internal",
  target_os = "linux",
  any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
))]
#[allow(dead_code)]
pub(crate) fn bench_rare_linux_native_provider() -> &'static str {
  linux_rare::bench_provider()
}

#[cfg(all(
  feature = "bench-internal",
  target_os = "linux",
  any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
))]
#[allow(dead_code)]
pub(crate) fn bench_rare_linux_native_measurements()
-> Option<([u64; 9], [u64; 9], Option<[u64; 9]>, usize)> {
  linux_rare::bench_measurements()
}

#[cfg(all(
  feature = "bench-internal",
  target_os = "linux",
  any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
))]
pub(crate) struct RareLinuxNativeSelectionEvidence {
  pub(crate) candidate_names: [&'static str; 3],
  pub(crate) candidate_eligible: [bool; 3],
  pub(crate) candidate_measured: [bool; 3],
  pub(crate) candidate_batches_ns: [[u64; 9]; 3],
  pub(crate) selected_candidate: &'static str,
  pub(crate) reads_per_batch: usize,
  pub(crate) required_decisive_wins: usize,
}

#[cfg(all(
  feature = "bench-internal",
  target_os = "linux",
  any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
))]
pub(crate) fn bench_rare_linux_native_selection_evidence() -> RareLinuxNativeSelectionEvidence {
  let (names, eligible, measured, batches, selected, reads, wins) =
    linux_rare::bench_selection_evidence();
  RareLinuxNativeSelectionEvidence {
    candidate_names: names,
    candidate_eligible: eligible,
    candidate_measured: measured,
    candidate_batches_ns: batches,
    selected_candidate: selected,
    reads_per_batch: reads,
    required_decisive_wins: wins,
  }
}

#[cfg(all(
  feature = "bench-internal",
  target_os = "linux",
  any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_rare_linux_exact_candidate(index: usize) -> Option<u64> {
  linux_rare::bench_exact_candidate(index)
}

#[cfg(all(
  feature = "bench-internal",
  target_os = "linux",
  any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_rare_linux_selected_native_nanos() -> u64 {
  linux_rare::now_nanos()
}

#[cfg(all(feature = "bench-internal", target_arch = "x86", target_os = "linux",))]
pub(crate) fn bench_i686_native_provider() -> &'static str {
  let _ = linux_32_now_nanos();
  match LINUX32_NATIVE_CHOICE.load(Ordering::Relaxed) {
    LINUX32_NATIVE_TIME32 => "linux_i686_time32_syscall",
    LINUX32_NATIVE_TIME64 => "linux_i686_time64_syscall",
    LINUX32_NATIVE_WALL => "monotonic_wall_fallback",
    _ => "libc_clock_gettime",
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(target_arch = "x86", target_arch = "arm"),
  target_os = "linux",
))]
pub(crate) struct Linux32NativeSelectionEvidence {
  pub(crate) candidate_names: [&'static str; 3],
  pub(crate) candidate_eligible: [bool; 3],
  pub(crate) candidate_batches_ns: [[u64; 9]; 3],
  pub(crate) selected_candidate: &'static str,
}

#[cfg(all(feature = "bench-internal", target_arch = "x86", target_os = "linux"))]
pub(crate) fn bench_i686_native_selection_evidence() -> Linux32NativeSelectionEvidence {
  let selected_candidate = bench_i686_native_provider();
  let load = |source: &[core::sync::atomic::AtomicU64; 9]| {
    let mut values = [0; 9];
    for (value, source) in values.iter_mut().zip(source) {
      *value = source.load(Ordering::Relaxed);
    }
    values
  };
  Linux32NativeSelectionEvidence {
    candidate_names: [
      "libc_clock_gettime",
      "linux_i686_time32_syscall",
      "linux_i686_time64_syscall",
    ],
    candidate_eligible: [
      LINUX32_LIBC_AVAILABLE.load(Ordering::Relaxed) != 0,
      LINUX32_TIME32_AVAILABLE.load(Ordering::Relaxed) != 0,
      LINUX32_TIME64_AVAILABLE.load(Ordering::Relaxed) != 0,
    ],
    candidate_batches_ns: [
      load(&LINUX32_LIBC_SAMPLES),
      load(&LINUX32_TIME32_SAMPLES),
      load(&LINUX32_TIME64_SAMPLES),
    ],
    selected_candidate,
  }
}

#[cfg(all(feature = "bench-internal", target_arch = "x86", target_os = "linux"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_i686_selected_native_nanos() -> u64 {
  linux_32_now_nanos()
}

#[cfg(all(feature = "bench-internal", target_arch = "x86", target_os = "linux"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_i686_exact_candidate(index: usize) -> Option<u64> {
  match index {
    0 => linux_32_libc_nanos(),
    1 => linux_32_time32_nanos(),
    2 => linux_32_time64_nanos(),
    _ => None,
  }
}

#[cfg(all(feature = "bench-internal", target_arch = "arm", target_os = "linux"))]
pub(crate) fn bench_arm_native_provider() -> &'static str {
  let _ = linux_32_now_nanos();
  match LINUX32_NATIVE_CHOICE.load(Ordering::Relaxed) {
    LINUX32_NATIVE_TIME32 => "linux_arm_time32_syscall",
    LINUX32_NATIVE_TIME64 => "linux_arm_time64_syscall",
    LINUX32_NATIVE_WALL => "monotonic_wall_fallback",
    _ => "libc_clock_gettime",
  }
}

#[cfg(all(feature = "bench-internal", target_arch = "arm", target_os = "linux"))]
pub(crate) fn bench_arm_native_selection_evidence() -> Linux32NativeSelectionEvidence {
  let selected_candidate = bench_arm_native_provider();
  let load = |source: &[core::sync::atomic::AtomicU64; 9]| {
    let mut values = [0; 9];
    for (value, source) in values.iter_mut().zip(source) {
      *value = source.load(Ordering::Relaxed);
    }
    values
  };
  Linux32NativeSelectionEvidence {
    candidate_names: ["libc_clock_gettime", "linux_arm_time32_syscall", "linux_arm_time64_syscall"],
    candidate_eligible: [
      LINUX32_LIBC_AVAILABLE.load(Ordering::Relaxed) != 0,
      LINUX32_TIME32_AVAILABLE.load(Ordering::Relaxed) != 0,
      LINUX32_TIME64_AVAILABLE.load(Ordering::Relaxed) != 0,
    ],
    candidate_batches_ns: [
      load(&LINUX32_LIBC_SAMPLES),
      load(&LINUX32_TIME32_SAMPLES),
      load(&LINUX32_TIME64_SAMPLES),
    ],
    selected_candidate,
  }
}

#[cfg(all(feature = "bench-internal", target_arch = "arm", target_os = "linux"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_arm_selected_native_nanos() -> u64 {
  linux_32_now_nanos()
}

#[cfg(all(feature = "bench-internal", target_arch = "arm", target_os = "linux"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_arm_exact_candidate(index: usize) -> Option<u64> {
  match index {
    0 => linux_32_libc_nanos(),
    1 => linux_32_time32_nanos(),
    2 => linux_32_time64_nanos(),
    _ => None,
  }
}

#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
pub(crate) fn bench_riscv64_native_provider() -> &'static str {
  let _ = linux_riscv64_now_nanos();
  match RISCV64_NATIVE_CHOICE.load(Ordering::Relaxed) {
    RISCV64_NATIVE_SYSCALL => "linux_riscv64_syscall",
    RISCV64_NATIVE_WALL => "monotonic_wall_fallback",
    _ => "libc_clock_gettime",
  }
}

#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
pub(crate) struct Riscv64NativeSelectionEvidence {
  pub(crate) candidate_names: [&'static str; 2],
  pub(crate) candidate_eligible: [bool; 2],
  pub(crate) candidate_batches_ns: [[u64; 9]; 2],
  pub(crate) selected_candidate: &'static str,
}

#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
pub(crate) fn bench_riscv64_native_selection_evidence() -> Riscv64NativeSelectionEvidence {
  let selected_candidate = bench_riscv64_native_provider();
  let load = |source: &[core::sync::atomic::AtomicU64; 9]| {
    let mut values = [0; 9];
    for (value, source) in values.iter_mut().zip(source) {
      *value = source.load(Ordering::Relaxed);
    }
    values
  };
  Riscv64NativeSelectionEvidence {
    candidate_names: ["libc_clock_gettime", "linux_riscv64_syscall"],
    candidate_eligible: [
      RISCV64_LIBC_AVAILABLE.load(Ordering::Relaxed) != 0,
      RISCV64_SYSCALL_AVAILABLE.load(Ordering::Relaxed) != 0,
    ],
    candidate_batches_ns: [load(&RISCV64_LIBC_SAMPLES), load(&RISCV64_SYSCALL_SAMPLES)],
    selected_candidate,
  }
}

#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_riscv64_selected_native_nanos() -> u64 {
  linux_riscv64_now_nanos()
}

#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_riscv64_exact_candidate(index: usize) -> Option<u64> {
  match index {
    0 => linux_riscv64_libc_nanos(),
    1 => linux_riscv64_syscall_nanos(),
    _ => None,
  }
}

#[cfg(any(
  all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "android", target_os = "freebsd"),
  ),
  all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn thread_clock_gettime(value: *mut libc::timespec) -> bool {
  #[cfg(all(target_arch = "x86_64", any(target_os = "linux", target_os = "android"),))]
  {
    let mut result = libc::SYS_clock_gettime;
    // SAFETY: Linux and Android use the x86_64 syscall instruction with RAX as
    // the syscall number and RDI/RSI as the first two arguments. The timespec
    // pointer addresses writable caller-owned storage, and success returns
    // zero in RAX.
    unsafe {
      asm!(
        "syscall",
        inlateout("rax") result,
        in("rdi") libc::CLOCK_THREAD_CPUTIME_ID,
        in("rsi") value,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
      );
    }
    result == 0
  }
  #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
  {
    const SYS_CLOCK_GETTIME: libc::c_long = 232;
    let mut result = SYS_CLOCK_GETTIME;
    // SAFETY: FreeBSD amd64 syscall 232 is clock_gettime, with RDI/RSI as its
    // arguments. The fast-syscall return path explicitly zeroes R8, R9, and
    // R10 in addition to the architectural RCX/R11 clobbers, so every one is
    // declared here. The timespec pointer addresses writable caller-owned
    // storage, and success returns zero in RAX.
    unsafe {
      asm!(
        "syscall",
        inlateout("rax") result,
        in("rdi") libc::CLOCK_THREAD_CPUTIME_ID,
        in("rsi") value,
        lateout("rcx") _,
        lateout("r8") _,
        lateout("r9") _,
        lateout("r10") _,
        lateout("r11") _,
        options(nostack),
      );
    }
    result == 0
  }
  #[cfg(all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")))]
  {
    let mut result = libc::c_long::from(libc::CLOCK_THREAD_CPUTIME_ID);
    // SAFETY: Linux and Android use the aarch64 syscall ABI with X8 as the
    // syscall number and X0/X1 as the first two arguments. The timespec
    // pointer addresses writable caller-owned storage, and success returns
    // zero in X0.
    unsafe {
      asm!(
        "svc 0",
        inlateout("x0") result,
        in("x1") value,
        in("x8") libc::SYS_clock_gettime,
        options(nostack),
      );
    }
    result == 0
  }
  #[cfg(all(target_arch = "x86", target_os = "linux"))]
  {
    // Linux i386 selects its supported kernel-entry instruction through the
    // vDSO (`__kernel_vsyscall`). The libc clock wrapper owns that runtime ABI
    // choice and falls back correctly when the 32-bit vDSO is disabled.
    // SAFETY: `value` is writable timespec storage and the clock id selects
    // CPU time consumed by the calling thread.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, value) == 0 }
  }
  #[cfg(not(all(any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
    all(target_arch = "x86", target_os = "linux"),
  ),)))]
  {
    // SAFETY: `value` is writable timespec storage and the clock id selects
    // CPU time consumed by the calling thread.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, value) == 0 }
  }
}

#[cfg(target_os = "macos")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn now_nanos() -> u64 {
  unsafe extern "C" {
    fn clock_gettime_nsec_np(clock_id: libc::clockid_t) -> u64;
  }

  // This is Darwin's direct nanosecond-returning entry for the current-thread
  // clock. The timespec clock_gettime wrapper adds output conversion around
  // the same clock, while thread_info/getrusage expose coarsened accounting
  // and are not eligible for tach's native-precision contract.
  // SAFETY: CLOCK_THREAD_CPUTIME_ID is valid for the calling thread on macOS.
  unsafe { clock_gettime_nsec_np(libc::CLOCK_THREAD_CPUTIME_ID) }
}

#[cfg(target_os = "macos")]
#[inline]
pub(crate) const fn provider() -> ThreadCpuProvider {
  ThreadCpuProvider::PosixThreadCpuClock
}

#[cfg(target_os = "macos")]
#[inline]
pub(crate) const fn read_cost_hint() -> ThreadCpuReadCost {
  ThreadCpuReadCost::SystemCall
}

#[cfg(target_os = "windows")]
const WINDOWS_UNKNOWN: u8 = 0;
#[cfg(target_os = "windows")]
const WINDOWS_THREAD_CPU: u8 = 1;
#[cfg(target_os = "windows")]
const WINDOWS_WALL: u8 = 2;
#[cfg(target_os = "windows")]
static WINDOWS_PROVIDER: core::sync::atomic::AtomicU8 =
  core::sync::atomic::AtomicU8::new(WINDOWS_UNKNOWN);

#[cfg(target_os = "windows")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn now_nanos() -> u64 {
  use core::sync::atomic::Ordering;

  let state = WINDOWS_PROVIDER.load(Ordering::Relaxed);
  if state == WINDOWS_WALL {
    return windows_wall_now_value();
  }
  if let Some(nanos) = windows_thread_cpu_nanos() {
    return match state {
      WINDOWS_THREAD_CPU => nanos,
      WINDOWS_UNKNOWN => match WINDOWS_PROVIDER.compare_exchange(
        WINDOWS_UNKNOWN,
        WINDOWS_THREAD_CPU,
        Ordering::Relaxed,
        Ordering::Relaxed,
      ) {
        Ok(_) | Err(WINDOWS_THREAD_CPU) => nanos,
        Err(_) => windows_wall_now_value(),
      },
      _ => windows_wall_now_value(),
    };
  }
  WINDOWS_PROVIDER.store(WINDOWS_WALL, Ordering::Relaxed);
  windows_wall_now_value()
}

// GetThreadTimes' documented failure fallback is QueryPerformanceCounter, the
// same OS-owned high-resolution wall clock `GlobalInstant` reads, scaled to
// nanoseconds. Reading QPC directly rather than the shared `crate::arch::ticks()`
// `Instant` timeline keeps this fallback off the x86 Windows bare-invariant-TSC
// `Instant` path (RDTSC), which the thread-cpu route forbids and whose tick
// domain would also mis-scale against QueryPerformanceFrequency. The nanosecond
// result carries the shared wall tag, so it is interpreted with
// `Duration::from_nanos` like every other tagged wall fallback and never
// re-enters the tick-domain `Instant` scale.
#[cfg(target_os = "windows")]
#[inline]
pub(crate) fn windows_wall_now_value() -> u64 {
  let ticks = u128::from(crate::arch::fallback::qpc_ticks());
  let frequency = u128::from(crate::arch::fallback::qpc_frequency().max(1));
  let nanos = u64::try_from(ticks.saturating_mul(1_000_000_000) / frequency).unwrap_or(u64::MAX);
  crate::thread_cpu::encode_wall_ticks(nanos)
}

#[cfg(target_os = "windows")]
#[inline(always)]
#[allow(clippy::inline_always)]
fn windows_thread_cpu_nanos() -> Option<u64> {
  use core::ffi::c_void;
  use core::mem::MaybeUninit;

  #[repr(C)]
  struct FileTime {
    low: u32,
    high: u32,
  }

  #[link(name = "kernel32")]
  unsafe extern "system" {
    fn GetThreadTimes(
      thread: *mut c_void,
      creation_time: *mut FileTime,
      exit_time: *mut FileTime,
      kernel_time: *mut FileTime,
      user_time: *mut FileTime,
    ) -> i32;
  }

  const CURRENT_THREAD: *mut c_void = (-2_isize) as *mut c_void;

  let mut creation = MaybeUninit::<FileTime>::uninit();
  let mut exit = MaybeUninit::<FileTime>::uninit();
  let mut kernel = MaybeUninit::<FileTime>::uninit();
  let mut user = MaybeUninit::<FileTime>::uninit();

  // GetThreadTimes is Windows' documented elapsed thread-CPU timeline.
  // QueryThreadCycleTime is not eligible: Microsoft explicitly forbids
  // converting its implementation-dependent cycle count to elapsed time.
  // NtQueryInformationThread does not expose a stable documented ThreadTimes
  // information class, so it is not a portable alternative entry path.
  // SAFETY: -2 is Windows' current-thread pseudo-handle and each FILETIME
  // pointer addresses writable storage.
  let status = unsafe {
    GetThreadTimes(
      CURRENT_THREAD,
      creation.as_mut_ptr(),
      exit.as_mut_ptr(),
      kernel.as_mut_ptr(),
      user.as_mut_ptr(),
    )
  };
  if status == 0 {
    return None;
  }
  // SAFETY: successful GetThreadTimes initialized every output.
  let kernel = unsafe { kernel.assume_init() };
  // SAFETY: successful GetThreadTimes initialized every output.
  let user = unsafe { user.assume_init() };
  let kernel_100ns = (u64::from(kernel.high) << 32) | u64::from(kernel.low);
  let user_100ns = (u64::from(user.high) << 32) | u64::from(user.low);
  Some(kernel_100ns.saturating_add(user_100ns).saturating_mul(100))
}

#[cfg(target_os = "windows")]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  use core::sync::atomic::Ordering;

  let _ = now_nanos();
  if WINDOWS_PROVIDER.load(Ordering::Relaxed) == WINDOWS_THREAD_CPU {
    ThreadCpuProvider::WindowsThreadTimes
  } else {
    wall_provider()
  }
}

#[cfg(target_os = "windows")]
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  use core::sync::atomic::Ordering;

  let _ = now_nanos();
  if WINDOWS_PROVIDER.load(Ordering::Relaxed) == WINDOWS_THREAD_CPU {
    ThreadCpuReadCost::SystemCall
  } else {
    wall_read_cost()
  }
}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
const WASI_UNKNOWN: u8 = 0;
#[cfg(all(target_os = "wasi", target_env = "p1"))]
const WASI_THREAD_CPU: u8 = 1;
#[cfg(all(target_os = "wasi", target_env = "p1"))]
const WASI_WALL: u8 = 2;
#[cfg(all(target_os = "wasi", target_env = "p1"))]
static WASI_PROVIDER: core::sync::atomic::AtomicU8 =
  core::sync::atomic::AtomicU8::new(WASI_UNKNOWN);

#[cfg(all(target_os = "wasi", target_env = "p1"))]
#[link(wasm_import_module = "wasi_snapshot_preview1")]
unsafe extern "C" {
  fn clock_time_get(id: u32, precision: u64, time: *mut u64) -> u16;
}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
#[inline]
fn wasi_thread_cpu_nanos() -> Option<u64> {
  const CLOCK_THREAD_CPUTIME_ID: u32 = 3;
  const CLOCK_READ_PRECISION_NANOS: u64 = 1;
  let mut nanos = 0;
  // SAFETY: `nanos` is writable u64 storage and the Preview 1 ABI accepts the
  // optional thread-clock id by returning an errno when the host lacks it.
  let status =
    unsafe { clock_time_get(CLOCK_THREAD_CPUTIME_ID, CLOCK_READ_PRECISION_NANOS, &mut nanos) };
  (status == 0).then_some(nanos)
}

#[cfg(all(feature = "bench-internal", target_os = "wasi", target_env = "p1"))]
#[inline]
pub(crate) fn bench_wasi_thread_cpu_nanos() -> Option<u64> {
  wasi_thread_cpu_nanos()
}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
#[inline]
pub(crate) fn now_nanos() -> u64 {
  use core::sync::atomic::Ordering;

  let state = WASI_PROVIDER.load(Ordering::Relaxed);
  if state == WASI_WALL {
    return wall_now_value();
  }
  if let Some(nanos) = wasi_thread_cpu_nanos() {
    return match state {
      WASI_THREAD_CPU => nanos,
      WASI_UNKNOWN => match WASI_PROVIDER.compare_exchange(
        WASI_UNKNOWN,
        WASI_THREAD_CPU,
        Ordering::Relaxed,
        Ordering::Relaxed,
      ) {
        Ok(_) | Err(WASI_THREAD_CPU) => nanos,
        Err(_) => wall_now_value(),
      },
      _ => wall_now_value(),
    };
  }

  match state {
    WASI_THREAD_CPU => {
      WASI_PROVIDER.store(WASI_WALL, Ordering::Relaxed);
      wall_now_value()
    }
    WASI_UNKNOWN => match WASI_PROVIDER.compare_exchange(
      WASI_UNKNOWN,
      WASI_WALL,
      Ordering::Relaxed,
      Ordering::Relaxed,
    ) {
      Ok(_) | Err(WASI_WALL) => wall_now_value(),
      Err(_) => {
        WASI_PROVIDER.store(WASI_WALL, Ordering::Relaxed);
        wall_now_value()
      }
    },
    _ => wall_now_value(),
  }
}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  use core::sync::atomic::Ordering;

  let _ = now_nanos();
  match WASI_PROVIDER.load(Ordering::Relaxed) {
    WASI_THREAD_CPU => ThreadCpuProvider::WasiThreadCpuClock,
    _ => wall_provider(),
  }
}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  let _ = now_nanos();
  ThreadCpuReadCost::HostCall
}

#[cfg(all(target_os = "wasi", target_env = "p2"))]
#[inline]
pub(crate) fn now_nanos() -> u64 {
  wall_now_value()
}

#[cfg(all(target_os = "wasi", target_env = "p2"))]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  wall_provider()
}

#[cfg(all(target_os = "wasi", target_env = "p2"))]
#[inline]
pub(crate) const fn read_cost_hint() -> ThreadCpuReadCost {
  ThreadCpuReadCost::HostCall
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
const WASM_UNKNOWN: u8 = 0;
#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
const WASM_NODE_THREAD_CPU: u8 = 1;
#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
const WASM_WALL: u8 = 2;
#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
static WASM_PROVIDER: core::sync::atomic::AtomicU8 =
  core::sync::atomic::AtomicU8::new(WASM_UNKNOWN);

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[inline]
pub(crate) fn now_nanos() -> u64 {
  use core::sync::atomic::Ordering;

  let state = WASM_PROVIDER.load(Ordering::Relaxed);
  if state == WASM_WALL {
    return wall_now_value();
  }

  let nanos = crate::arch::wasm::node_thread_cpu_usage_micros()
    .and_then(|micros| micros.checked_mul(1_000))
    .filter(|nanos| *nanos < (1 << 63));
  if let Some(nanos) = nanos {
    return match state {
      WASM_NODE_THREAD_CPU => nanos,
      WASM_UNKNOWN => match WASM_PROVIDER.compare_exchange(
        WASM_UNKNOWN,
        WASM_NODE_THREAD_CPU,
        Ordering::Relaxed,
        Ordering::Relaxed,
      ) {
        Ok(_) | Err(WASM_NODE_THREAD_CPU) => nanos,
        Err(_) => wall_now_value(),
      },
      _ => wall_now_value(),
    };
  }

  WASM_PROVIDER.store(WASM_WALL, Ordering::Relaxed);
  wall_now_value()
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  use core::sync::atomic::Ordering;

  let _ = now_nanos();
  if WASM_PROVIDER.load(Ordering::Relaxed) == WASM_NODE_THREAD_CPU {
    ThreadCpuProvider::NodeThreadCpuUsage
  } else {
    wall_provider()
  }
}

#[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  use core::sync::atomic::Ordering;

  let _ = now_nanos();
  if WASM_PROVIDER.load(Ordering::Relaxed) == WASM_NODE_THREAD_CPU {
    ThreadCpuReadCost::HostCall
  } else {
    wall_read_cost()
  }
}

#[cfg(target_os = "emscripten")]
const EMSCRIPTEN_UNKNOWN: u8 = 0;
#[cfg(target_os = "emscripten")]
const EMSCRIPTEN_NODE_THREAD_CPU: u8 = 1;
#[cfg(target_os = "emscripten")]
const EMSCRIPTEN_WALL: u8 = 2;
#[cfg(target_os = "emscripten")]
static EMSCRIPTEN_PROVIDER: core::sync::atomic::AtomicU8 =
  core::sync::atomic::AtomicU8::new(EMSCRIPTEN_UNKNOWN);

#[cfg(target_os = "emscripten")]
#[inline]
pub(crate) fn now_nanos() -> u64 {
  use core::sync::atomic::Ordering;

  let state = EMSCRIPTEN_PROVIDER.load(Ordering::Relaxed);
  if state == EMSCRIPTEN_WALL {
    return wall_now_value();
  }

  let nanos = crate::arch::emscripten::node_thread_cpu_usage_micros()
    .and_then(|micros| micros.checked_mul(1_000))
    .filter(|nanos| *nanos < (1 << 63));
  if let Some(nanos) = nanos {
    return match state {
      EMSCRIPTEN_NODE_THREAD_CPU => nanos,
      EMSCRIPTEN_UNKNOWN => match EMSCRIPTEN_PROVIDER.compare_exchange(
        EMSCRIPTEN_UNKNOWN,
        EMSCRIPTEN_NODE_THREAD_CPU,
        Ordering::Relaxed,
        Ordering::Relaxed,
      ) {
        Ok(_) | Err(EMSCRIPTEN_NODE_THREAD_CPU) => nanos,
        Err(_) => wall_now_value(),
      },
      _ => wall_now_value(),
    };
  }

  EMSCRIPTEN_PROVIDER.store(EMSCRIPTEN_WALL, Ordering::Relaxed);
  wall_now_value()
}

#[cfg(target_os = "emscripten")]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  use core::sync::atomic::Ordering;

  let _ = now_nanos();
  if EMSCRIPTEN_PROVIDER.load(Ordering::Relaxed) == EMSCRIPTEN_NODE_THREAD_CPU {
    ThreadCpuProvider::NodeThreadCpuUsage
  } else {
    wall_provider()
  }
}

#[cfg(target_os = "emscripten")]
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  use core::sync::atomic::Ordering;

  let _ = now_nanos();
  if EMSCRIPTEN_PROVIDER.load(Ordering::Relaxed) == EMSCRIPTEN_NODE_THREAD_CPU {
    ThreadCpuReadCost::HostCall
  } else {
    crate::arch::emscripten::wall_read_cost()
  }
}

#[cfg(not(any(
  all(unix, not(target_os = "emscripten")),
  target_os = "windows",
  target_os = "wasi",
  target_os = "emscripten",
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
)))]
#[inline]
pub(crate) fn now_nanos() -> u64 {
  wall_now_value()
}

#[cfg(not(any(
  all(unix, not(target_os = "emscripten")),
  target_os = "windows",
  target_os = "wasi",
  target_os = "emscripten",
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
)))]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  wall_provider()
}

#[cfg(not(any(
  all(unix, not(target_os = "emscripten")),
  target_os = "windows",
  target_os = "wasi",
  target_os = "emscripten",
  all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
)))]
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  wall_read_cost()
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten")),))]
#[inline]
#[allow(dead_code)] // Linux inline builds wrap this native provider with the perf selector.
fn posix_provider() -> ThreadCpuProvider {
  if crate::thread_cpu::is_wall_value(posix_now_nanos()) {
    wall_provider()
  } else {
    ThreadCpuProvider::PosixThreadCpuClock
  }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten")),))]
#[inline]
#[allow(dead_code)] // Linux inline builds wrap this native cost with the perf selector.
fn posix_read_cost() -> ThreadCpuReadCost {
  if crate::thread_cpu::is_wall_value(posix_now_nanos()) {
    wall_read_cost()
  } else {
    #[cfg(any(
      all(
        target_arch = "x86_64",
        any(target_os = "linux", target_os = "android", target_os = "freebsd"),
      ),
      all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
    ))]
    {
      native_64_read_cost()
    }
    #[cfg(not(any(
      all(
        target_arch = "x86_64",
        any(target_os = "linux", target_os = "android", target_os = "freebsd"),
      ),
      all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
    )))]
    {
      ThreadCpuReadCost::SystemCall
    }
  }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten"))))]
#[inline]
fn timespec_to_nanos(value: libc::timespec) -> Option<u64> {
  let seconds = u64::try_from(value.tv_sec).ok()?;
  let nanos = u32::try_from(value.tv_nsec).ok()?;
  if nanos >= 1_000_000_000 {
    return None;
  }
  seconds.checked_mul(1_000_000_000)?.checked_add(u64::from(nanos))
}

// Windows routes its thread-cpu wall fallback through `windows_wall_now_value`
// (QueryPerformanceCounter), so this shared `crate::arch::ticks()` wall read is
// excluded there: on x86 Windows `ticks()` is the bare-invariant-TSC `Instant`
// path the thread-cpu route forbids.
#[inline]
#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn wall_now_value() -> u64 {
  crate::thread_cpu::encode_wall_ticks(crate::arch::ticks())
}

#[inline]
#[cfg(not(target_os = "macos"))]
fn wall_provider() -> ThreadCpuProvider {
  #[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
  {
    crate::arch::wasm::wall_provider()
  }
  #[cfg(target_os = "emscripten")]
  {
    crate::arch::emscripten::wall_provider()
  }
  #[cfg(not(any(
    all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
    target_os = "emscripten",
  )))]
  {
    ThreadCpuProvider::MonotonicWallClock
  }
}

#[inline]
#[cfg(not(any(target_os = "macos", target_os = "wasi", target_os = "emscripten")))]
fn wall_read_cost() -> ThreadCpuReadCost {
  #[cfg(all(
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86", target_arch = "x86_64"),
  ))]
  {
    crate::arch::linux_x86_wall::instant_read_cost()
  }

  #[cfg(all(target_arch = "x86_64", target_os = "freebsd",))]
  {
    crate::arch::freebsd_x86_64::instant_read_cost()
  }

  #[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),))]
  {
    crate::arch::linux_aarch64_wall::instant_read_cost()
  }

  #[cfg(target_arch = "riscv64")]
  {
    crate::arch::riscv64::instant_read_cost()
  }

  #[cfg(target_arch = "loongarch64")]
  {
    crate::arch::loongarch64::instant_read_cost()
  }

  #[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
  {
    crate::arch::powerpc64::instant_read_cost()
  }

  #[cfg(all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")))]
  {
    crate::arch::linux_clock_wall::instant_read_cost()
  }

  #[cfg(all(
    target_arch = "x86_64",
    not(any(
      target_os = "windows",
      target_os = "macos",
      target_os = "freebsd",
      target_os = "linux",
      target_os = "android",
    )),
  ))]
  {
    ThreadCpuReadCost::Inline
  }

  #[cfg(all(
    target_arch = "x86",
    not(any(target_os = "windows", target_os = "linux", target_os = "android")),
  ))]
  {
    ThreadCpuReadCost::Inline
  }

  #[cfg(all(
    target_arch = "aarch64",
    not(any(
      target_os = "android",
      target_os = "linux",
      target_os = "windows",
      target_os = "macos",
    )),
  ))]
  {
    ThreadCpuReadCost::Inline
  }

  #[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
  {
    crate::arch::wasm::wall_read_cost()
  }

  #[cfg(not(any(
    all(
      any(target_os = "android", target_os = "linux"),
      any(target_arch = "x86", target_arch = "x86_64"),
    ),
    all(target_arch = "x86_64", target_os = "freebsd"),
    all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")),
    target_arch = "riscv64",
    target_arch = "loongarch64",
    all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
    all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")),
    all(
      target_arch = "x86_64",
      not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "linux",
        target_os = "android",
      )),
    ),
    all(
      target_arch = "x86",
      not(any(target_os = "windows", target_os = "linux", target_os = "android")),
    ),
    all(
      target_arch = "aarch64",
      not(any(
        target_os = "android",
        target_os = "linux",
        target_os = "windows",
        target_os = "macos",
      )),
    ),
    all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")),
  )))]
  {
    #[cfg(unix)]
    {
      ThreadCpuReadCost::SystemCall
    }
    #[cfg(not(unix))]
    {
      ThreadCpuReadCost::SystemCall
    }
  }
}
