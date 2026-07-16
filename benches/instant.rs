#![allow(clippy::cast_precision_loss)]

#[cfg(all(
  feature = "thread-cpu-inline",
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "aarch64"),
))]
use std::arch::asm;
use std::hint::black_box;
#[cfg(any(
  target_os = "android",
  target_os = "freebsd",
  target_os = "linux",
  target_os = "windows",
))]
use std::mem::MaybeUninit;
use std::time::{Duration, Instant as StdInstant};

use criterion::{Criterion, criterion_group, criterion_main};
#[cfg(all(feature = "bench-internal", target_os = "macos"))]
use tach::bench::MachAbsoluteTimeDirect;
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
use tach::bench::ThreadCpuPerfHandle;
#[cfg(all(feature = "bench-internal", target_arch = "x86_64", target_os = "macos"))]
use tach::bench::{AppleX86CommpageDirect, AppleX86TscDirect};
#[cfg(all(
  feature = "bench-internal",
  target_arch = "aarch64",
  any(target_os = "android", target_os = "linux"),
))]
use tach::bench::{OrderedAarch64CntvctssDirect, OrderedAarch64IsbDirect};
#[cfg(all(
  feature = "bench-internal",
  any(target_arch = "x86_64", target_arch = "x86"),
  not(target_os = "windows"),
  not(all(target_arch = "x86_64", target_os = "macos")),
))]
use tach::bench::{
  OrderedX86CpuidDirect, OrderedX86LfenceDirect, OrderedX86MfenceDirect, OrderedX86RdtscpDirect,
  OrderedX86SerializeDirect,
};
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
use tach::bench::{ThreadCpuPerfPathHandle, ThreadCpuPerfReadHandle};
use tach::{Instant, OrderedInstant, ThreadCpuInstant, ThreadCpuProvider, ThreadCpuReadCost};

#[cfg(feature = "bench-internal")]
#[allow(unused_macros)]
macro_rules! register_selected_now {
  ($group:ident, $prefix:literal, $provider:expr, $read:path) => {{
    $group.bench_function(format!("{}__{}", $prefix, $provider), |b| {
      b.iter(|| black_box($read()));
    });
  }};
}

#[cfg(feature = "bench-internal")]
#[allow(unused_macros)]
macro_rules! register_selected_elapsed {
  ($group:ident, $prefix:literal, $provider:expr, $nanos_per_tick_q32:expr, $read:path) => {{
    $group.bench_function(format!("{}__{}", $prefix, $provider), |b| {
      b.iter(|| {
        let start = $read();
        let elapsed = $read().saturating_sub(start);
        black_box(tach::bench::exact_ticks_to_duration_with_scale(elapsed, $nanos_per_tick_q32))
      });
    });
  }};
}

#[cfg(all(
  feature = "bench-internal",
  any(
    target_os = "macos",
    target_os = "windows",
    all(target_arch = "x86_64", target_os = "freebsd"),
    all(
      any(target_arch = "x86", target_arch = "x86_64"),
      any(target_os = "android", target_os = "linux"),
    ),
  ),
))]
const WALL_PUBLIC_EXACT_BATCHES: usize = 9;
#[cfg(all(
  feature = "bench-internal",
  any(
    target_os = "macos",
    target_os = "windows",
    all(target_arch = "x86_64", target_os = "freebsd"),
    all(
      any(target_arch = "x86", target_arch = "x86_64"),
      any(target_os = "android", target_os = "linux"),
    ),
  ),
))]
const WALL_PUBLIC_EXACT_READS: usize = 65_536;
#[cfg(all(
  feature = "bench-internal",
  any(
    target_os = "macos",
    target_os = "windows",
    all(target_arch = "x86_64", target_os = "freebsd"),
    all(
      any(target_arch = "x86", target_arch = "x86_64"),
      any(target_os = "android", target_os = "linux"),
    ),
  ),
))]
const WALL_PUBLIC_EXACT_CHUNKS: usize = 64;

#[cfg(all(
  feature = "bench-internal",
  any(
    target_os = "macos",
    target_os = "windows",
    all(target_arch = "x86_64", target_os = "freebsd"),
    all(
      any(target_arch = "x86", target_arch = "x86_64"),
      any(target_os = "android", target_os = "linux"),
    ),
  ),
))]
#[inline(never)]
fn measure_wall_read_chunk<T>(read: &mut dyn FnMut() -> T) -> u64 {
  // Both sides cross the same opaque boundary, so closure-size-specific
  // inlining cannot be misreported as public clock overhead.
  let started = StdInstant::now();
  for _ in 0..(WALL_PUBLIC_EXACT_READS / WALL_PUBLIC_EXACT_CHUNKS) {
    black_box(read());
  }
  u64::try_from(started.elapsed().as_nanos()).expect("wall parity batch exceeded u64 nanoseconds")
}

#[cfg(all(
  feature = "bench-internal",
  any(
    target_os = "macos",
    target_os = "windows",
    all(target_arch = "x86_64", target_os = "freebsd"),
    all(
      any(target_arch = "x86", target_arch = "x86_64"),
      any(target_os = "android", target_os = "linux"),
    ),
  ),
))]
fn measure_wall_paired_batch<P, PT, D, DT>(
  public: &mut P,
  direct: &mut D,
  public_first: bool,
) -> (u64, u64)
where
  P: FnMut() -> PT,
  D: FnMut() -> DT,
{
  let mut public_ns = 0_u64;
  let mut direct_ns = 0_u64;
  for chunk in 0..WALL_PUBLIC_EXACT_CHUNKS {
    if (chunk & 1 == 0) == public_first {
      public_ns = public_ns.saturating_add(measure_wall_read_chunk(public));
      direct_ns = direct_ns.saturating_add(measure_wall_read_chunk(direct));
    } else {
      direct_ns = direct_ns.saturating_add(measure_wall_read_chunk(direct));
      public_ns = public_ns.saturating_add(measure_wall_read_chunk(public));
    }
  }
  (public_ns, direct_ns)
}

#[cfg(all(
  feature = "bench-internal",
  any(
    target_os = "macos",
    target_os = "windows",
    all(target_arch = "x86_64", target_os = "freebsd"),
    all(
      any(target_arch = "x86", target_arch = "x86_64"),
      any(target_os = "android", target_os = "linux"),
    ),
  ),
))]
fn measure_wall_public_exact<P, PT, D, DT>(mut public: P, mut direct: D) -> serde_json::Value
where
  P: FnMut() -> PT,
  D: FnMut() -> DT,
{
  for _ in 0..4_096 {
    black_box(public());
    black_box(direct());
  }

  let mut public_batches_ns = [0_u64; WALL_PUBLIC_EXACT_BATCHES];
  let mut direct_batches_ns = [0_u64; WALL_PUBLIC_EXACT_BATCHES];
  for batch in 0..WALL_PUBLIC_EXACT_BATCHES {
    (public_batches_ns[batch], direct_batches_ns[batch]) =
      measure_wall_paired_batch(&mut public, &mut direct, batch & 1 == 0);
  }

  serde_json::json!({
    "selection_kind": "paired_public_exact_parity",
    "reads_per_batch": WALL_PUBLIC_EXACT_READS,
    "required_decisive_losses": 8,
    "equivalence_band": {
      "floor_ns_per_read": 1,
      "relative_denominator": 20,
    },
    "batch_order": "64 alternating 1024-read chunks per batch; starting side flips by batch",
    "call_boundary": "symmetric dynamic FnMut boundary",
    "measurement_clock": "std::time::Instant outside the measured read loop",
    "public_batches_ns": public_batches_ns,
    "exact_batches_ns": direct_batches_ns,
  })
}

#[cfg(all(feature = "bench-internal", target_arch = "aarch64", target_os = "macos"))]
macro_rules! measure_apple_public_exact {
  (instant, $read:path) => {
    measure_wall_public_exact(|| Instant::now(), || $read())
  };
  (ordered, $read:path) => {
    measure_wall_public_exact(|| OrderedInstant::now(), || $read())
  };
}

#[cfg(all(feature = "bench-internal", target_arch = "x86_64", target_os = "macos"))]
fn measure_apple_x86_public_exact() -> serde_json::Value {
  let instant_scale = tach::bench::apple_x86_selected_nanos_per_tick_q32();
  serde_json::json!({
    "instant": {
      "now": measure_wall_public_exact(
        || Instant::now(),
        || tach::bench::apple_x86_selected_ticks(),
      ),
      "elapsed": measure_wall_public_exact(
        || {
          let start = Instant::now();
          start.elapsed()
        },
        || {
          let start = tach::bench::apple_x86_selected_ticks();
          let elapsed = tach::bench::apple_x86_selected_ticks().saturating_sub(start);
          tach::bench::exact_ticks_to_duration_with_scale(elapsed, instant_scale)
        },
      ),
    },
    "ordered": {
      "now": measure_wall_public_exact(
        || OrderedInstant::now(),
        || tach::bench::apple_x86_selected_ordered_ticks(),
      ),
      "elapsed": measure_wall_public_exact(
        || {
          let start = OrderedInstant::now();
          start.elapsed()
        },
        || {
          let start = tach::bench::apple_x86_selected_ordered_ticks();
          let elapsed =
            tach::bench::apple_x86_selected_ordered_ticks().saturating_sub(start);
          tach::bench::exact_ticks_to_duration_with_scale(elapsed, 1_u64 << 32)
        },
      ),
    },
  })
}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86", target_arch = "x86_64"),
))]
macro_rules! measure_linux_x86_public_exact {
  (instant, $nanos_per_tick_q32:expr, $read:path) => {{
    let scale = $nanos_per_tick_q32;
    serde_json::json!({
      "now": measure_wall_public_exact(|| Instant::now(), || $read()),
      "elapsed": measure_wall_public_exact(
        || {
          let start = Instant::now();
          start.elapsed()
        },
        || {
          let start = $read();
          let elapsed = $read().saturating_sub(start);
          tach::bench::exact_ticks_to_duration_with_scale(elapsed, scale)
        },
      ),
    })
  }};
  (ordered, $nanos_per_tick_q32:expr, $read:path) => {{
    let scale = $nanos_per_tick_q32;
    serde_json::json!({
      "now": measure_wall_public_exact(|| OrderedInstant::now(), || $read()),
      "elapsed": measure_wall_public_exact(
        || {
          let start = OrderedInstant::now();
          start.elapsed()
        },
        || {
          let start = $read();
          let elapsed = $read().saturating_sub(start);
          tach::bench::exact_ticks_to_duration_with_scale(elapsed, scale)
        },
      ),
    })
  }};
}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86", target_arch = "x86_64"),
))]
macro_rules! with_linux_x86_instant_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "linux_kernel_eligible_tsc" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_tsc)
      }
      "linux_clock_monotonic_libc" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic)
      }
      "linux_clock_monotonic_raw_libc" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw)
      }
      "linux_clock_boottime_libc" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime)
      }
      "linux_clock_monotonic_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic)
      }
      "linux_clock_monotonic_raw_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw)
      }
      "linux_clock_boottime_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_vdso_time64_direct" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_monotonic)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_vdso_time64_direct" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_raw)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_vdso_time64_direct" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_boottime)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_syscall_x86_64" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_raw_syscall_x86_64" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_boottime_syscall_x86_64" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time32" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_monotonic)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time32" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_raw)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time32" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_boottime)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time64" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_monotonic)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time64" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_raw)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time64" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_boottime)
      }
      _ => panic!("unsupported selected Linux x86 Instant provider: {}", $provider),
    }
  }};
}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86", target_arch = "x86_64"),
))]
macro_rules! with_linux_x86_ordered_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "linux_kernel_eligible_tsc_x86_lfence_rdtsc" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_tsc_lfence)
      }
      "linux_kernel_eligible_tsc_x86_mfence_rdtsc" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_tsc_mfence)
      }
      "linux_kernel_eligible_tsc_x86_rdtscp" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_tsc_rdtscp)
      }
      "linux_kernel_eligible_tsc_x86_cpuid_rdtsc" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_tsc_cpuid)
      }
      "linux_kernel_eligible_tsc_x86_serialize_rdtsc" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_tsc_serialize)
      }
      "linux_clock_monotonic_libc_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic_lfence)
      }
      "linux_clock_monotonic_libc_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic_os_owned)
      }
      "linux_clock_monotonic_libc_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic_rdtscp)
      }
      "linux_clock_monotonic_libc_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic_mfence)
      }
      "linux_clock_monotonic_libc_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic_cpuid)
      }
      "linux_clock_monotonic_libc_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_monotonic_serialize)
      }
      "linux_clock_monotonic_raw_libc_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw_lfence)
      }
      "linux_clock_monotonic_raw_libc_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw_os_owned)
      }
      "linux_clock_monotonic_raw_libc_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw_rdtscp)
      }
      "linux_clock_monotonic_raw_libc_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw_mfence)
      }
      "linux_clock_monotonic_raw_libc_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw_cpuid)
      }
      "linux_clock_monotonic_raw_libc_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_raw_serialize)
      }
      "linux_clock_monotonic_vdso_direct_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic_os_owned)
      }
      "linux_clock_monotonic_vdso_direct_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic_lfence)
      }
      "linux_clock_monotonic_vdso_direct_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic_rdtscp)
      }
      "linux_clock_monotonic_vdso_direct_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic_mfence)
      }
      "linux_clock_monotonic_vdso_direct_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic_cpuid)
      }
      "linux_clock_monotonic_vdso_direct_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_monotonic_serialize)
      }
      "linux_clock_monotonic_raw_vdso_direct_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw_os_owned)
      }
      "linux_clock_monotonic_raw_vdso_direct_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw_lfence)
      }
      "linux_clock_monotonic_raw_vdso_direct_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw_rdtscp)
      }
      "linux_clock_monotonic_raw_vdso_direct_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw_mfence)
      }
      "linux_clock_monotonic_raw_vdso_direct_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw_cpuid)
      }
      "linux_clock_monotonic_raw_vdso_direct_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_raw_serialize)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_syscall_x86_64_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic_lfence)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_syscall_x86_64_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic_os_owned)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_syscall_x86_64_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic_rdtscp)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_syscall_x86_64_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic_mfence)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_syscall_x86_64_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic_cpuid)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_syscall_x86_64_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_monotonic_serialize)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_raw_syscall_x86_64_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw_lfence)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_raw_syscall_x86_64_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw_os_owned)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_raw_syscall_x86_64_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw_rdtscp)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_raw_syscall_x86_64_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw_mfence)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_raw_syscall_x86_64_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw_cpuid)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_monotonic_raw_syscall_x86_64_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_raw_serialize)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time32_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_monotonic_lfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time32_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_monotonic_os_owned)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time32_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_monotonic_rdtscp)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time32_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_monotonic_mfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time32_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_monotonic_cpuid)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time32_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_monotonic_serialize)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time32_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_raw_lfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time32_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_raw_os_owned)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time32_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_raw_rdtscp)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time32_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_raw_mfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time32_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_raw_cpuid)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time32_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_raw_serialize)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time64_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_monotonic_lfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time64_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_monotonic_os_owned)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time64_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_monotonic_rdtscp)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time64_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_monotonic_mfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time64_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_monotonic_cpuid)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_syscall_i686_time64_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_monotonic_serialize)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time64_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_raw_lfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time64_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_raw_os_owned)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time64_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_raw_rdtscp)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time64_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_raw_mfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time64_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_raw_cpuid)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_syscall_i686_time64_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_raw_serialize)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_vdso_time64_direct_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_monotonic_os_owned)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_vdso_time64_direct_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_monotonic_lfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_vdso_time64_direct_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_monotonic_rdtscp)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_vdso_time64_direct_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_monotonic_mfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_vdso_time64_direct_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_monotonic_cpuid)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_vdso_time64_direct_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_monotonic_serialize)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_vdso_time64_direct_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_raw_os_owned)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_vdso_time64_direct_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_raw_lfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_vdso_time64_direct_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_raw_rdtscp)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_vdso_time64_direct_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_raw_mfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_vdso_time64_direct_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_raw_cpuid)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_monotonic_raw_vdso_time64_direct_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_raw_serialize)
      }
      "linux_clock_boottime_libc_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime_os_owned)
      }
      "linux_clock_boottime_libc_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime_lfence)
      }
      "linux_clock_boottime_libc_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime_rdtscp)
      }
      "linux_clock_boottime_libc_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime_mfence)
      }
      "linux_clock_boottime_libc_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime_cpuid)
      }
      "linux_clock_boottime_libc_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_libc_boottime_serialize)
      }
      "linux_clock_boottime_vdso_direct_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime_os_owned)
      }
      "linux_clock_boottime_vdso_direct_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime_lfence)
      }
      "linux_clock_boottime_vdso_direct_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime_rdtscp)
      }
      "linux_clock_boottime_vdso_direct_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime_mfence)
      }
      "linux_clock_boottime_vdso_direct_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime_cpuid)
      }
      "linux_clock_boottime_vdso_direct_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_boottime_serialize)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_boottime_syscall_x86_64_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime_os_owned)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_boottime_syscall_x86_64_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime_lfence)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_boottime_syscall_x86_64_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime_rdtscp)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_boottime_syscall_x86_64_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime_mfence)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_boottime_syscall_x86_64_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime_cpuid)
      }
      #[cfg(target_pointer_width = "64")]
      "linux_clock_boottime_syscall_x86_64_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_syscall64_boottime_serialize)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time32_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_boottime_os_owned)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time32_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_boottime_lfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time32_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_boottime_rdtscp)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time32_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_boottime_mfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time32_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_boottime_cpuid)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time32_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time32_boottime_serialize)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time64_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_boottime_os_owned)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time64_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_boottime_lfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time64_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_boottime_rdtscp)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time64_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_boottime_mfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time64_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_boottime_cpuid)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_syscall_i686_time64_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_time64_boottime_serialize)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_vdso_time64_direct_os_owned" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_boottime_os_owned)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_vdso_time64_direct_x86_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_boottime_lfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_vdso_time64_direct_x86_rdtscp_lfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_boottime_rdtscp)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_vdso_time64_direct_x86_mfence" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_boottime_mfence)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_vdso_time64_direct_x86_cpuid" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_boottime_cpuid)
      }
      #[cfg(target_pointer_width = "32")]
      "linux_clock_boottime_vdso_time64_direct_x86_serialize" => {
        $callback!($($arguments)*, tach::bench::linux_x86_exact_vdso_time64_boottime_serialize)
      }
      _ => panic!("unsupported selected Linux x86 Ordered provider: {}", $provider),
    }
  }};
}

#[cfg(all(
  feature = "bench-internal",
  target_arch = "aarch64",
  any(target_os = "android", target_os = "linux"),
))]
macro_rules! with_linux_aarch64_instant_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "aarch64_cntvct" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_cntvct)
      }
      "linux_clock_monotonic" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_libc_monotonic)
      }
      "linux_clock_monotonic_raw" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_libc_raw)
      }
      "linux_clock_boottime" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_libc_boottime)
      }
      "linux_clock_monotonic_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_vdso_monotonic)
      }
      "linux_clock_monotonic_raw_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_vdso_raw)
      }
      "linux_clock_boottime_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_vdso_boottime)
      }
      "linux_clock_monotonic_syscall" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_syscall_monotonic)
      }
      "linux_clock_monotonic_raw_syscall" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_syscall_raw)
      }
      "linux_clock_boottime_syscall" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_syscall_boottime)
      }
      _ => panic!("unsupported selected Linux aarch64 Instant provider: {}", $provider),
    }
  }};
}

#[cfg(all(
  feature = "bench-internal",
  target_arch = "aarch64",
  any(target_os = "android", target_os = "linux"),
))]
macro_rules! with_linux_aarch64_ordered_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "aarch64_isb_cntvct" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_isb_cntvct)
      }
      "aarch64_cntvctss" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_cntvctss)
      }
      "linux_clock_monotonic" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_ordered_libc_monotonic)
      }
      "linux_clock_monotonic_raw" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_ordered_libc_raw)
      }
      "linux_clock_boottime" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_ordered_libc_boottime)
      }
      "linux_clock_monotonic_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_ordered_vdso_monotonic)
      }
      "linux_clock_monotonic_raw_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_ordered_vdso_raw)
      }
      "linux_clock_boottime_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_ordered_vdso_boottime)
      }
      "linux_clock_monotonic_syscall" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_syscall_monotonic)
      }
      "linux_clock_monotonic_raw_syscall" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_syscall_raw)
      }
      "linux_clock_boottime_syscall" => {
        $callback!($($arguments)*, tach::bench::linux_aarch64_exact_syscall_boottime)
      }
      _ => panic!("unsupported selected Linux aarch64 Ordered provider: {}", $provider),
    }
  }};
}

#[cfg(all(
  feature = "bench-internal",
  target_os = "linux",
  any(target_arch = "arm", target_arch = "s390x"),
))]
macro_rules! with_residual_instant_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      #[cfg(target_arch = "arm")]
      "linux_arm_cntvct" => {
        $callback!($($arguments)*, tach::bench::residual_exact_arm_cntvct)
      }
      "linux_clock_monotonic" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic)
      }
      "linux_clock_monotonic_syscall" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_syscall)
      }
      #[cfg(target_arch = "arm")]
      "linux_clock_monotonic_time64_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_time64_syscall
      ),
      "linux_clock_monotonic_raw" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_raw)
      }
      "linux_clock_monotonic_raw_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_raw_syscall
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_monotonic_raw_time64_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_raw_time64_syscall
      ),
      "linux_clock_monotonic_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_vdso)
      }
      "linux_clock_monotonic_raw_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_raw_vdso
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_monotonic_vdso_time64_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_time64_vdso
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_monotonic_raw_vdso_time64_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_raw_time64_vdso
      ),
      "linux_clock_boottime" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_boottime)
      }
      "linux_clock_boottime_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_boottime_syscall
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_boottime_time64_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_boottime_time64_syscall
      ),
      "linux_clock_boottime_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_boottime_vdso
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_boottime_vdso_time64_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_boottime_time64_vdso
      ),
      _ => panic!("unsupported residual Instant provider: {}", $provider),
    }
  }};
}

#[cfg(all(
  feature = "bench-internal",
  target_os = "linux",
  any(target_arch = "arm", target_arch = "s390x"),
))]
macro_rules! with_residual_ordered_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      #[cfg(target_arch = "arm")]
      "linux_arm_dmb_ish_isb_cntvct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_arm_cntvct
      ),
      "linux_clock_monotonic" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic
      ),
      "linux_clock_monotonic_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_syscall
      ),
      "linux_clock_monotonic_syscall_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_monotonic_syscall
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_monotonic_time64_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_time64_syscall
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_monotonic_time64_syscall_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_monotonic_time64_syscall
      ),
      "linux_clock_monotonic_raw" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw
      ),
      "linux_clock_monotonic_raw_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw_syscall
      ),
      "linux_clock_monotonic_raw_syscall_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_monotonic_raw_syscall
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_monotonic_raw_time64_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw_time64_syscall
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_monotonic_raw_time64_syscall_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_monotonic_raw_time64_syscall
      ),
      "linux_clock_monotonic_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_vdso
      ),
      "linux_clock_monotonic_raw_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw_vdso
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_monotonic_vdso_time64_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_time64_vdso
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_monotonic_raw_vdso_time64_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw_time64_vdso
      ),
      "linux_clock_boottime" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime
      ),
      "linux_clock_boottime_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime_syscall
      ),
      "linux_clock_boottime_syscall_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_boottime_syscall
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_boottime_time64_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime_time64_syscall
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_boottime_time64_syscall_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_boottime_time64_syscall
      ),
      "linux_clock_boottime_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime_vdso
      ),
      #[cfg(target_arch = "arm")]
      "linux_clock_boottime_vdso_time64_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime_time64_vdso
      ),
      _ => panic!("unsupported residual Ordered provider: {}", $provider),
    }
  }};
}

#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
macro_rules! with_residual_instant_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "riscv_rdtime" => {
        $callback!($($arguments)*, tach::bench::residual_exact_riscv_rdtime)
      }
      "linux_clock_monotonic" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic)
      }
      "linux_clock_monotonic_syscall" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_syscall)
      }
      "linux_clock_monotonic_raw" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_raw)
      }
      "linux_clock_monotonic_raw_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_raw_syscall
      ),
      "linux_clock_monotonic_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_vdso)
      }
      "linux_clock_monotonic_raw_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_raw_vdso
      ),
      "linux_clock_boottime" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_boottime)
      }
      "linux_clock_boottime_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_boottime_syscall
      ),
      "linux_clock_boottime_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_boottime_vdso
      ),
      _ => panic!("unsupported residual Instant provider: {}", $provider),
    }
  }};
}

#[cfg(all(feature = "bench-internal", target_arch = "riscv64", target_os = "linux"))]
macro_rules! with_residual_ordered_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "riscv_rdtime" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_riscv_rdtime
      ),
      "linux_clock_monotonic" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic
      ),
      "linux_clock_monotonic_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_syscall
      ),
      "linux_clock_monotonic_syscall_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_monotonic_syscall
      ),
      "linux_clock_monotonic_raw" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw
      ),
      "linux_clock_monotonic_raw_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw_syscall
      ),
      "linux_clock_monotonic_raw_syscall_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_monotonic_raw_syscall
      ),
      "linux_clock_monotonic_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_vdso
      ),
      "linux_clock_monotonic_raw_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw_vdso
      ),
      "linux_clock_boottime" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime
      ),
      "linux_clock_boottime_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime_syscall
      ),
      "linux_clock_boottime_syscall_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_boottime_syscall
      ),
      "linux_clock_boottime_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime_vdso
      ),
      _ => panic!("unsupported residual Ordered provider: {}", $provider),
    }
  }};
}

#[cfg(all(feature = "bench-internal", target_arch = "loongarch64", target_os = "linux"))]
macro_rules! with_residual_instant_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "loongarch_stable_counter" => {
        $callback!($($arguments)*, tach::bench::residual_exact_loong_stable_counter)
      }
      "linux_clock_monotonic" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic)
      }
      "linux_clock_monotonic_syscall" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_syscall)
      }
      "linux_clock_monotonic_raw" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_raw)
      }
      "linux_clock_monotonic_raw_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_raw_syscall
      ),
      "linux_clock_monotonic_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_vdso)
      }
      "linux_clock_monotonic_raw_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_raw_vdso
      ),
      "linux_clock_boottime" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_boottime)
      }
      "linux_clock_boottime_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_boottime_syscall
      ),
      "linux_clock_boottime_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_boottime_vdso
      ),
      _ => panic!("unsupported residual Instant provider: {}", $provider),
    }
  }};
}

#[cfg(all(feature = "bench-internal", target_arch = "loongarch64", target_os = "linux"))]
macro_rules! with_residual_ordered_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "loongarch_stable_counter" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_loong_stable_counter
      ),
      "linux_clock_monotonic" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic
      ),
      "linux_clock_monotonic_syscall" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_syscall)
      }
      "linux_clock_monotonic_raw" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw
      ),
      "linux_clock_monotonic_raw_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_raw_syscall
      ),
      "linux_clock_monotonic_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_vdso
      ),
      "linux_clock_monotonic_raw_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw_vdso
      ),
      "linux_clock_boottime" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime
      ),
      "linux_clock_boottime_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime_syscall
      ),
      "linux_clock_boottime_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime_vdso
      ),
      _ => panic!("unsupported residual Ordered provider: {}", $provider),
    }
  }};
}

#[cfg(all(
  feature = "bench-internal",
  target_arch = "powerpc64",
  target_os = "linux",
  target_env = "gnu",
))]
macro_rules! with_residual_instant_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "power_timebase" => {
        $callback!($($arguments)*, tach::bench::residual_exact_power_timebase)
      }
      "linux_clock_monotonic" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic)
      }
      "linux_clock_monotonic_sc" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_sc)
      }
      "linux_clock_monotonic_scv" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_scv)
      }
      "linux_clock_monotonic_raw" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_raw)
      }
      "linux_clock_monotonic_raw_sc" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_raw_sc
      ),
      "linux_clock_monotonic_raw_scv" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_raw_scv
      ),
      "linux_clock_monotonic_vdso_direct" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_monotonic_vdso)
      }
      "linux_clock_monotonic_raw_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_monotonic_raw_vdso
      ),
      "linux_clock_boottime" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_boottime)
      }
      "linux_clock_boottime_sc" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_boottime_sc)
      }
      "linux_clock_boottime_scv" => {
        $callback!($($arguments)*, tach::bench::residual_exact_clock_boottime_scv)
      }
      "linux_clock_boottime_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_clock_boottime_vdso
      ),
      _ => panic!("unsupported residual Instant provider: {}", $provider),
    }
  }};
}

#[cfg(all(
  feature = "bench-internal",
  target_arch = "powerpc64",
  target_os = "linux",
  target_env = "gnu",
))]
macro_rules! with_residual_ordered_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "power_timebase" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_power_timebase
      ),
      "linux_clock_monotonic" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic
      ),
      "linux_clock_monotonic_sc" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_sc
      ),
      "linux_clock_monotonic_sc_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_monotonic_sc
      ),
      "linux_clock_monotonic_scv" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_scv
      ),
      "linux_clock_monotonic_scv_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_monotonic_scv
      ),
      "linux_clock_monotonic_raw" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw
      ),
      "linux_clock_monotonic_raw_sc" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw_sc
      ),
      "linux_clock_monotonic_raw_sc_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_monotonic_raw_sc
      ),
      "linux_clock_monotonic_raw_scv" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw_scv
      ),
      "linux_clock_monotonic_raw_scv_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_monotonic_raw_scv
      ),
      "linux_clock_monotonic_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_vdso
      ),
      "linux_clock_monotonic_raw_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_monotonic_raw_vdso
      ),
      "linux_clock_boottime" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime
      ),
      "linux_clock_boottime_sc" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime_sc
      ),
      "linux_clock_boottime_sc_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_boottime_sc
      ),
      "linux_clock_boottime_scv" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime_scv
      ),
      "linux_clock_boottime_scv_os_ordered" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_os_ordered_clock_boottime_scv
      ),
      "linux_clock_boottime_vdso_direct" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_ordered_clock_boottime_vdso
      ),
      _ => panic!("unsupported residual Ordered provider: {}", $provider),
    }
  }};
}

#[cfg(all(feature = "bench-internal", target_arch = "x86_64", target_os = "freebsd"))]
macro_rules! with_residual_instant_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "freebsd_kernel_eligible_tsc" => {
        $callback!($($arguments)*, tach::bench::residual_exact_freebsd_tsc)
      }
      "freebsd_at_timekeep" => {
        $callback!($($arguments)*, tach::bench::residual_exact_freebsd_timekeep)
      }
      "freebsd_clock_monotonic" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic
      ),
      "freebsd_clock_monotonic_syscall" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_syscall
      ),
      _ => panic!("unsupported residual Instant provider: {}", $provider),
    }
  }};
}

#[cfg(all(feature = "bench-internal", target_arch = "x86_64", target_os = "freebsd"))]
macro_rules! with_residual_ordered_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "freebsd_kernel_eligible_tsc_x86_lfence_rdtsc" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_tsc_lfence
      ),
      "freebsd_kernel_eligible_tsc_x86_mfence_rdtsc" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_tsc_mfence
      ),
      "freebsd_kernel_eligible_tsc_x86_rdtscp" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_tsc_rdtscp
      ),
      "freebsd_kernel_eligible_tsc_x86_cpuid_rdtsc" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_tsc_cpuid
      ),
      "freebsd_kernel_eligible_tsc_x86_serialize_rdtsc" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_tsc_serialize
      ),
      "freebsd_at_timekeep_os_owned" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_timekeep_os_owned
      ),
      "freebsd_clock_monotonic_x86_mfence" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_mfence
      ),
      "freebsd_clock_monotonic_syscall_x86_mfence" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_syscall_mfence
      ),
      "freebsd_clock_monotonic_x86_cpuid" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_cpuid
      ),
      "freebsd_clock_monotonic_syscall_x86_cpuid" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_syscall_cpuid
      ),
      "freebsd_clock_monotonic_x86_lfence" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_lfence
      ),
      "freebsd_clock_monotonic_syscall_x86_lfence" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_syscall_lfence
      ),
      "freebsd_clock_monotonic_x86_rdtscp_lfence" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_rdtscp
      ),
      "freebsd_clock_monotonic_syscall_x86_rdtscp_lfence" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_syscall_rdtscp
      ),
      "freebsd_clock_monotonic_os_owned" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_os_owned
      ),
      "freebsd_clock_monotonic_syscall_os_owned" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_syscall_os_owned
      ),
      "freebsd_clock_monotonic_x86_serialize" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_serialize
      ),
      "freebsd_clock_monotonic_syscall_x86_serialize" => $callback!(
        $($arguments)*,
        tach::bench::residual_exact_freebsd_clock_monotonic_syscall_serialize
      ),
      _ => panic!("unsupported residual Ordered provider: {}", $provider),
    }
  }};
}

#[cfg(all(feature = "bench-internal", target_arch = "x86_64", target_os = "freebsd"))]
macro_rules! measure_freebsd_public_exact {
  (instant, $nanos_per_tick_q32:expr, $read:path) => {{
    let scale = $nanos_per_tick_q32;
    serde_json::json!({
      "now": measure_wall_public_exact(|| Instant::now(), || $read()),
      "elapsed": measure_wall_public_exact(
        || {
          let start = Instant::now();
          start.elapsed()
        },
        || {
          let start = $read();
          let elapsed = $read().saturating_sub(start);
          tach::bench::exact_ticks_to_duration_with_scale(elapsed, scale)
        },
      ),
    })
  }};
  (ordered, $nanos_per_tick_q32:expr, $read:path) => {{
    let scale = $nanos_per_tick_q32;
    serde_json::json!({
      "now": measure_wall_public_exact(|| OrderedInstant::now(), || $read()),
      "elapsed": measure_wall_public_exact(
        || {
          let start = OrderedInstant::now();
          start.elapsed()
        },
        || {
          let start = $read();
          let elapsed = $read().saturating_sub(start);
          tach::bench::exact_ticks_to_duration_with_scale(elapsed, scale)
        },
      ),
    })
  }};
}

#[cfg(all(feature = "bench-internal", target_arch = "aarch64", target_os = "macos"))]
macro_rules! with_apple_aarch64_instant_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "apple_commpage_cntvct_offset" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_cntvct_absolute)
      }
      "apple_commpage_cntvctss_offset" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_cntvctss_absolute)
      }
      "apple_commpage_acntvct_offset" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_acntvct_absolute)
      }
      "apple_mach_continuous_time" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_mach_continuous)
      }
      "apple_continuous_hw_cntvct_base" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_cntvct_continuous)
      }
      "apple_continuous_hw_cntvctss_base" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_cntvctss_continuous)
      }
      "apple_continuous_hw_acntvct_base" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_acntvct_continuous)
      }
      "apple_bare_cntvct" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_bare_cntvct)
      }
      _ => $callback!($($arguments)*, tach::bench::apple_aarch64_exact_mach_absolute),
    }
  }};
}

#[cfg(all(feature = "bench-internal", target_arch = "aarch64", target_os = "macos"))]
macro_rules! with_apple_aarch64_ordered_read {
  ($provider:expr, $callback:ident, $($arguments:tt)*) => {{
    match $provider {
      "apple_commpage_isb_cntvct_offset" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_cntvct_ordered_absolute)
      }
      "apple_commpage_cntvctss_offset" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_cntvctss_absolute)
      }
      "apple_commpage_acntvct_offset" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_acntvct_absolute)
      }
      "apple_mach_continuous_time" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_mach_continuous)
      }
      "apple_continuous_hw_isb_cntvct_base" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_cntvct_ordered_continuous)
      }
      "apple_continuous_hw_cntvctss_base" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_cntvctss_continuous)
      }
      "apple_continuous_hw_acntvct_base" => {
        $callback!($($arguments)*, tach::bench::apple_aarch64_exact_acntvct_continuous)
      }
      _ => $callback!($($arguments)*, tach::bench::apple_aarch64_exact_mach_absolute),
    }
  }};
}

fn criterion_target_env() -> &'static str {
  if cfg!(target_env = "gnu") {
    "gnu"
  } else if cfg!(target_env = "musl") {
    "musl"
  } else if cfg!(target_env = "msvc") {
    "msvc"
  } else if cfg!(target_env = "uclibc") {
    "uclibc"
  } else if cfg!(target_env = "sgx") {
    "sgx"
  } else if cfg!(target_env = "newlib") {
    "newlib"
  } else if cfg!(target_env = "p1") {
    "p1"
  } else if cfg!(target_env = "p2") {
    "p2"
  } else if cfg!(target_env = "") {
    ""
  } else {
    "unknown"
  }
}

fn valid_benchmark_source_revision(value: &str) -> bool {
  matches!(value.len(), 40 | 64)
    && value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn criterion_evidence_mode() -> bool {
  matches!(std::env::var("TACH_BENCH_EVIDENCE").as_deref(), Ok("1"))
}

fn criterion_build_mode() -> &'static str {
  if cfg!(feature = "recalibrate-background") {
    "unsupported-recalibrate-background"
  } else if cfg!(feature = "emscripten-pthreads") && cfg!(feature = "thread-cpu-inline") {
    "emscripten-pthreads"
  } else if cfg!(feature = "emscripten-pthreads") {
    "unsupported-emscripten-pthreads-no-default"
  } else if cfg!(feature = "thread-cpu-inline") {
    "default"
  } else {
    "no-default"
  }
}

fn criterion_runtime_attestation() -> &'static serde_json::Value {
  use std::collections::hash_map::DefaultHasher;
  use std::hash::{Hash, Hasher};
  use std::sync::OnceLock;
  use std::time::{SystemTime, UNIX_EPOCH};

  static ATTESTATION: OnceLock<serde_json::Value> = OnceLock::new();
  ATTESTATION.get_or_init(|| {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let stack_nonce = (&now as *const u128) as usize;
    let mut invocation = DefaultHasher::new();
    std::process::id().hash(&mut invocation);
    now.hash(&mut invocation);
    stack_nonce.hash(&mut invocation);
    let invocation_id = format!("criterion-{:016x}", invocation.finish());
    let features: &[&str] = &[
      #[cfg(feature = "bench-internal")]
      "bench-internal",
      #[cfg(feature = "emscripten-pthreads")]
      "emscripten-pthreads",
      #[cfg(feature = "recalibrate-background")]
      "recalibrate-background",
      #[cfg(feature = "thread-cpu-inline")]
      "thread-cpu-inline",
    ];
    let source_revision = std::env::var("TACH_BENCH_SOURCE_REVISION")
      .ok()
      .filter(|revision| valid_benchmark_source_revision(revision));
    let runner = std::env::var("TACH_BENCH_RUNNER").ok().and_then(|runner| {
      let runner = runner.trim();
      (!runner.is_empty()).then(|| runner.to_owned())
    });
    serde_json::json!({
      "schema": "tach-benchmark-runtime-v2",
      "invocation_id": invocation_id,
      "harness": "criterion",
      "target": {
        "arch": std::env::consts::ARCH,
        "os": std::env::consts::OS,
        "env": criterion_target_env(),
      },
      "features": features,
      "build_mode": criterion_build_mode(),
      "build_profile": if cfg!(debug_assertions) { "debug" } else { "optimized" },
      "source_revision": source_revision,
      "runner": runner,
      "output_isolated": criterion_evidence_mode(),
    })
  })
}

fn write_criterion_runtime_attestation() -> serde_json::Value {
  use std::fs;
  use std::path::PathBuf;
  use std::sync::OnceLock;

  static ATTESTATION_WRITTEN: OnceLock<()> = OnceLock::new();

  let attestation = criterion_runtime_attestation().clone();
  ATTESTATION_WRITTEN.get_or_init(|| {
    let target = std::env::var_os("CARGO_TARGET_DIR")
      .map(PathBuf::from)
      .unwrap_or_else(|| PathBuf::from("target"));
    let directory = target.join("criterion");
    if criterion_evidence_mode() {
      match fs::symlink_metadata(&directory) {
        Ok(metadata) => {
          assert!(
            metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
            "evidence Criterion output must be an ordinary directory: {}",
            directory.display(),
          );
          let mut entries =
            fs::read_dir(&directory).expect("read existing Criterion evidence directory");
          assert!(
            entries.next().is_none(),
            "evidence Criterion output must start empty; use a fresh CARGO_TARGET_DIR",
          );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
          panic!("inspect Criterion evidence directory {}: {error}", directory.display(),)
        }
      }
    }
    fs::create_dir_all(&directory).expect("create Criterion runtime-attestation directory");
    fs::write(
      directory.join("runtime-attestation.json"),
      serde_json::to_vec_pretty(&attestation).expect("serialize Criterion runtime attestation"),
    )
    .expect("write Criterion runtime attestation");
  });
  attestation
}

fn bench_now(c: &mut Criterion) {
  write_criterion_runtime_attestation();
  // Prime the lazy frequency calibration so it doesn't land in the first
  // measured sample.
  quanta::Instant::now();
  fastant::Instant::now();
  minstant::Instant::now();

  let mut g = c.benchmark_group("Instant::now()");
  g.bench_function("tach", |b| b.iter(|| black_box(Instant::now())));
  g.bench_function("tach_ordered", |b| b.iter(|| black_box(OrderedInstant::now())));
  #[cfg(all(
    feature = "bench-internal",
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86_64", target_arch = "x86"),
  ))]
  {
    for candidate in tach::bench::linux_x86_instant_candidate_primitives() {
      let provider = candidate.provider();
      with_linux_x86_instant_read!(provider, register_selected_now, g, "direct_wall", provider);
    }
    for candidate in tach::bench::linux_x86_ordered_candidate_primitives() {
      let provider = candidate.provider();
      with_linux_x86_ordered_read!(
        provider,
        register_selected_now,
        g,
        "direct_ordered_wall",
        provider
      );
    }
    let selected = tach::bench::linux_x86_selected_instant_primitive();
    let provider = selected.provider();
    with_linux_x86_instant_read!(
      provider,
      register_selected_now,
      g,
      "direct_selected_wall",
      provider
    );

    let selected = tach::bench::linux_x86_selected_ordered_primitive();
    let provider = selected.provider();
    with_linux_x86_ordered_read!(
      provider,
      register_selected_now,
      g,
      "direct_selected_ordered_wall",
      provider
    );
  }
  #[cfg(all(
    feature = "bench-internal",
    any(target_arch = "x86_64", target_arch = "x86"),
    not(target_os = "windows"),
    not(all(target_arch = "x86_64", target_os = "macos")),
  ))]
  {
    if let Some(direct) = OrderedX86CpuidDirect::try_for_current_machine() {
      g.bench_function("direct_ordered__x86_cpuid_rdtsc", |b| {
        b.iter(|| black_box(direct.now_ticks()));
      });
    }
    if let Some(direct) = OrderedX86LfenceDirect::try_for_current_machine() {
      g.bench_function("direct_ordered__x86_lfence_rdtsc", |b| {
        b.iter(|| black_box(direct.now_ticks()));
      });
    }
    if let Some(direct) = OrderedX86MfenceDirect::try_for_current_machine() {
      g.bench_function("direct_ordered__x86_mfence_rdtsc", |b| {
        b.iter(|| black_box(direct.now_ticks()));
      });
    }
    if let Some(direct) = OrderedX86RdtscpDirect::try_for_current_machine() {
      g.bench_function("direct_ordered__x86_rdtscp", |b| {
        b.iter(|| black_box(direct.now_ticks()));
      });
    }
    if let Some(direct) = OrderedX86SerializeDirect::try_for_current_machine() {
      g.bench_function("direct_ordered__x86_serialize_rdtsc", |b| {
        b.iter(|| black_box(direct.now_ticks()));
      });
    }
  }
  #[cfg(all(
    feature = "bench-internal",
    target_arch = "aarch64",
    any(target_os = "android", target_os = "linux"),
  ))]
  {
    for candidate in tach::bench::linux_aarch64_instant_candidate_primitives() {
      let provider = candidate.provider();
      with_linux_aarch64_instant_read!(provider, register_selected_now, g, "direct_wall", provider);
    }
    for candidate in tach::bench::linux_aarch64_ordered_candidate_primitives() {
      let provider = candidate.provider();
      with_linux_aarch64_ordered_read!(
        provider,
        register_selected_now,
        g,
        "direct_ordered_wall",
        provider
      );
    }
    if let Some(direct) = OrderedAarch64IsbDirect::try_for_current_machine() {
      g.bench_function("direct_ordered__aarch64_isb_cntvct", |b| {
        b.iter(|| black_box(direct.now_ticks()));
      });
    }
    if let Some(direct) = OrderedAarch64CntvctssDirect::try_for_current_machine() {
      g.bench_function("direct_ordered__aarch64_cntvctss", |b| {
        b.iter(|| black_box(direct.now_ticks()));
      });
    }
    let selected = tach::bench::linux_aarch64_selected_instant_primitive();
    let provider = selected.provider();
    with_linux_aarch64_instant_read!(
      provider,
      register_selected_now,
      g,
      "direct_selected_wall",
      provider
    );
    let selected = tach::bench::linux_aarch64_selected_ordered_primitive();
    let provider = selected.provider();
    with_linux_aarch64_ordered_read!(
      provider,
      register_selected_now,
      g,
      "direct_selected_ordered_wall",
      provider
    );
  }
  #[cfg(all(
    feature = "bench-internal",
    any(
      all(target_arch = "riscv64", target_os = "linux"),
      all(target_arch = "loongarch64", target_os = "linux"),
      all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
      all(target_arch = "x86_64", target_os = "freebsd"),
      all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")),
    ),
  ))]
  {
    for candidate in tach::bench::residual_instant_candidate_primitives() {
      let provider = candidate.provider();
      with_residual_instant_read!(provider, register_selected_now, g, "direct_wall", provider);
    }
    for candidate in tach::bench::residual_ordered_candidate_primitives() {
      let provider = candidate.provider();
      with_residual_ordered_read!(
        provider,
        register_selected_now,
        g,
        "direct_ordered_wall",
        provider
      );
    }
    let selected = tach::bench::residual_selected_instant_primitive();
    let provider = selected.provider();
    with_residual_instant_read!(
      provider,
      register_selected_now,
      g,
      "direct_selected_wall",
      provider
    );
    let selected = tach::bench::residual_selected_ordered_primitive();
    let provider = selected.provider();
    with_residual_ordered_read!(
      provider,
      register_selected_now,
      g,
      "direct_selected_ordered_wall",
      provider
    );
  }
  #[cfg(all(feature = "bench-internal", target_os = "macos"))]
  {
    let native = MachAbsoluteTimeDirect::for_current_machine();
    #[cfg(target_arch = "x86_64")]
    g.bench_function("direct_wall__apple_mach_absolute_time", |b| {
      b.iter(|| black_box(native.now_ticks()));
    });
    #[cfg(target_arch = "x86_64")]
    g.bench_function("direct_ordered_wall__apple_mach_absolute_time", |b| {
      b.iter(|| black_box(native.now_ticks()));
    });
    #[cfg(target_arch = "aarch64")]
    g.bench_function("native_wall__mach_absolute_time", |b| {
      b.iter(|| black_box(native.now_ticks()));
    });
    #[cfg(target_arch = "x86_64")]
    {
      let instant_provider = tach::bench::apple_wall_selected_provider();
      let ordered_provider = tach::bench::apple_ordered_wall_selected_provider();
      if let Some(tsc) = AppleX86TscDirect::try_for_current_machine() {
        g.bench_function(format!("direct_wall__{}", tsc.provider()), |b| {
          b.iter(|| black_box(tsc.now_ticks()));
        });
      }
      if let Some(commpage) = AppleX86CommpageDirect::try_for_current_machine() {
        g.bench_function(format!("direct_ordered_wall__{}", commpage.provider()), |b| {
          b.iter(|| black_box(commpage.now_ticks()));
        });
      }
      g.bench_function(format!("direct_selected_wall__{instant_provider}"), |b| {
        b.iter(|| black_box(tach::bench::apple_x86_selected_ticks()));
      });
      g.bench_function(format!("direct_selected_ordered_wall__{ordered_provider}"), |b| {
        b.iter(|| black_box(tach::bench::apple_x86_selected_ordered_ticks()));
      });
    }
  }
  #[cfg(all(feature = "bench-internal", target_arch = "aarch64", target_os = "macos"))]
  {
    for candidate in tach::bench::apple_aarch64_instant_candidate_primitives() {
      let provider = candidate.provider();
      with_apple_aarch64_instant_read!(provider, register_selected_now, g, "direct_wall", provider);
    }
    for candidate in tach::bench::apple_aarch64_ordered_candidate_primitives() {
      let provider = candidate.provider();
      with_apple_aarch64_ordered_read!(
        provider,
        register_selected_now,
        g,
        "direct_ordered_wall",
        provider
      );
    }
    let selected = tach::bench::apple_aarch64_selected_instant_primitive();
    let provider = selected.provider();
    with_apple_aarch64_instant_read!(
      provider,
      register_selected_now,
      g,
      "direct_selected_wall",
      provider
    );
    let selected = tach::bench::apple_aarch64_selected_ordered_primitive();
    let provider = selected.provider();
    with_apple_aarch64_ordered_read!(
      provider,
      register_selected_now,
      g,
      "direct_selected_ordered_wall",
      provider
    );
  }
  #[cfg(all(feature = "bench-internal", target_os = "windows"))]
  {
    macro_rules! register_windows_now {
      ($prefix:literal, $provider:expr, $read:path) => {{
        let _ = g
          .bench_function(format!("{}__{}", $prefix, $provider), |b| b.iter(|| black_box($read())));
      }};
    }
    for provider in tach::bench::windows_wall_candidate_providers() {
      match provider {
        "windows_query_interrupt_time_precise" => register_windows_now!(
          "direct_wall",
          provider,
          tach::bench::windows_interrupt_time_precise_ticks
        ),
        "windows_query_unbiased_interrupt_time_precise" => register_windows_now!(
          "direct_wall",
          provider,
          tach::bench::windows_unbiased_interrupt_time_precise_ticks
        ),
        _ => register_windows_now!("direct_wall", provider, tach::bench::windows_qpc_ticks),
      }
    }
    let instant_provider = tach::bench::windows_wall_selected_provider();
    match instant_provider {
      "windows_query_interrupt_time_precise" => register_windows_now!(
        "direct_selected_wall",
        instant_provider,
        tach::bench::windows_interrupt_time_precise_ticks
      ),
      "windows_query_unbiased_interrupt_time_precise" => register_windows_now!(
        "direct_selected_wall",
        instant_provider,
        tach::bench::windows_unbiased_interrupt_time_precise_ticks
      ),
      _ => register_windows_now!(
        "direct_selected_wall",
        instant_provider,
        tach::bench::windows_qpc_ticks
      ),
    }
    for provider in tach::bench::windows_ordered_wall_candidate_providers() {
      match provider {
        "windows_query_interrupt_time_precise_call_boundary" => register_windows_now!(
          "direct_ordered_wall",
          provider,
          tach::bench::windows_interrupt_time_precise_ticks
        ),
        "windows_query_unbiased_interrupt_time_precise_call_boundary" => register_windows_now!(
          "direct_ordered_wall",
          provider,
          tach::bench::windows_unbiased_interrupt_time_precise_ticks
        ),
        _ => register_windows_now!("direct_ordered_wall", provider, tach::bench::windows_qpc_ticks),
      }
    }
    let ordered_provider = tach::bench::windows_ordered_wall_selected_provider();
    match ordered_provider {
      "windows_query_interrupt_time_precise_call_boundary" => register_windows_now!(
        "direct_selected_ordered_wall",
        ordered_provider,
        tach::bench::windows_interrupt_time_precise_ticks
      ),
      "windows_query_unbiased_interrupt_time_precise_call_boundary" => register_windows_now!(
        "direct_selected_ordered_wall",
        ordered_provider,
        tach::bench::windows_unbiased_interrupt_time_precise_ticks
      ),
      _ => register_windows_now!(
        "direct_selected_ordered_wall",
        ordered_provider,
        tach::bench::windows_qpc_ticks
      ),
    }
  }
  g.bench_function("quanta", |b| b.iter(|| black_box(quanta::Instant::now())));
  g.bench_function("fastant", |b| b.iter(|| black_box(fastant::Instant::now())));
  g.bench_function("minstant", |b| b.iter(|| black_box(minstant::Instant::now())));
  g.bench_function("std", |b| b.iter(|| black_box(StdInstant::now())));
  g.finish();
  write_ordered_selection();
  write_linux_x86_wall_selection();
  write_linux_aarch64_wall_selection();
  write_residual_wall_selection();
  write_apple_wall_selection();
  write_windows_wall_selection();
}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(target_arch = "riscv64", target_os = "linux"),
    all(target_arch = "loongarch64", target_os = "linux"),
    all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
    all(target_arch = "x86_64", target_os = "freebsd"),
    all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")),
  ),
))]
fn write_residual_wall_selection() {
  use std::fs;
  use std::path::PathBuf;

  #[cfg(all(target_arch = "riscv64", target_os = "linux"))]
  let (architecture, probe, ordering) = (
    "riscv64-linux",
    serde_json::to_value(tach::bench::riscv64_wall_selection_measurements())
      .expect("serialize RISC-V wall selector probe"),
    "the measured winner is either fence r,i before the read or a precise ECALL boundary that owns prior-read ordering",
  );
  #[cfg(all(target_arch = "loongarch64", target_os = "linux"))]
  let (architecture, probe, ordering) = (
    "loongarch64-linux",
    serde_json::to_value(tach::bench::loongarch64_wall_selection_measurements())
      .expect("serialize LoongArch wall selector probe"),
    "a precise getpid exception orders the direct counter; the raw clock syscall owns the OS-domain ordered sample",
  );
  #[cfg(all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"))]
  let (architecture, probe, ordering) = (
    "powerpc64-linux-gnu",
    serde_json::to_value(tach::bench::power_wall_selection_measurements())
      .expect("serialize Power wall selector probe"),
    "the measured winner is either heavyweight sync before the read or context-synchronizing SC/SCV that owns prior-read ordering",
  );
  #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
  let (architecture, probe, ordering) = (
    "x86_64-freebsd",
    serde_json::to_value(tach::bench::freebsd_wall_selection_measurements())
      .expect("serialize FreeBSD wall selector probe"),
    "the runtime winner among every eligible x86 barrier and the FreeBSD-owned vDSO/syscall ordering boundary precedes the wall read",
  );
  #[cfg(all(target_arch = "arm", target_os = "linux"))]
  let (architecture, probe, ordering) = (
    "armv7-linux",
    serde_json::to_value(tach::bench::linux_clock_wall_selection_measurements())
      .expect("serialize Armv7 wall selector probe"),
    "the measured winner is either dmb ish; isb before the path or an Arm SVC context-synchronization boundary that owns prior-read ordering",
  );
  #[cfg(all(target_arch = "s390x", target_os = "linux"))]
  let (architecture, probe, ordering) = (
    "s390x-linux",
    serde_json::to_value(tach::bench::linux_clock_wall_selection_measurements())
      .expect("serialize s390x wall selector probe"),
    "the measured winner is either bcr 15,0 before the path or a serializing s390 SVC boundary that owns prior-read ordering",
  );

  let instant_candidates: Vec<_> = tach::bench::residual_instant_candidate_primitives()
    .iter()
    .map(|candidate| format!("direct_wall__{}", candidate.provider()))
    .collect();
  let ordered_candidates: Vec<_> = tach::bench::residual_ordered_candidate_primitives()
    .iter()
    .map(|candidate| format!("direct_ordered_wall__{}", candidate.provider()))
    .collect();
  let instant_selected = tach::bench::residual_selected_instant_primitive();
  let ordered_selected = tach::bench::residual_selected_ordered_primitive();
  let payload = serde_json::json!({
    "architecture": architecture,
    "selected_provider": {
      "instant": instant_selected.provider(),
      "ordered": ordered_selected.provider(),
    },
    "selected_native_benchmark": {
      "instant": format!("direct_selected_wall__{}", instant_selected.provider()),
      "ordered": format!("direct_selected_ordered_wall__{}", ordered_selected.provider()),
    },
    "eligible_direct_candidates": {
      "instant": instant_candidates,
      "ordered": ordered_candidates,
    },
    "decision_rule": "each contract independently retains an incumbent unless a challenger wins by > max(1 ns/read, 5%) with >=8/9 decisive paired wins",
    "ordering": ordering,
    "probe": probe,
  });
  #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
  let payload = {
    let mut payload = payload;
    let instant = tach::bench::residual_selected_instant_primitive();
    let instant_public_exact = with_residual_instant_read!(
      instant.provider(),
      measure_freebsd_public_exact,
      instant,
      instant.nanos_per_tick_q32()
    );
    let ordered = tach::bench::residual_selected_ordered_primitive();
    let ordered_public_exact = with_residual_ordered_read!(
      ordered.provider(),
      measure_freebsd_public_exact,
      ordered,
      ordered.nanos_per_tick_q32()
    );
    payload["public_exact_probe"] = serde_json::json!({
      "instant": instant_public_exact,
      "ordered": ordered_public_exact,
    });
    payload
  };
  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create criterion directory");
  fs::write(
    directory.join("residual-wall-selection.json"),
    serde_json::to_vec_pretty(&payload).expect("serialize residual wall selector evidence"),
  )
  .expect("write residual wall selector evidence");
}

#[cfg(not(all(
  feature = "bench-internal",
  any(
    all(target_arch = "riscv64", target_os = "linux"),
    all(target_arch = "loongarch64", target_os = "linux"),
    all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
    all(target_arch = "x86_64", target_os = "freebsd"),
    all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")),
  ),
)))]
fn write_residual_wall_selection() {}

#[cfg(all(feature = "bench-internal", target_os = "macos"))]
fn write_apple_wall_selection() {
  use std::fs;
  use std::path::PathBuf;

  let instant_provider = tach::bench::apple_wall_selected_provider();
  let ordered_provider = tach::bench::apple_ordered_wall_selected_provider();
  #[cfg(target_arch = "aarch64")]
  let instant_candidates: Vec<_> = tach::bench::apple_aarch64_instant_candidate_primitives()
    .iter()
    .map(|candidate| format!("direct_wall__{}", candidate.provider()))
    .collect();
  #[cfg(target_arch = "aarch64")]
  let ordered_candidates: Vec<_> = tach::bench::apple_aarch64_ordered_candidate_primitives()
    .iter()
    .map(|candidate| format!("direct_ordered_wall__{}", candidate.provider()))
    .collect();
  #[cfg(target_arch = "x86_64")]
  let mut instant_candidates = vec!["direct_wall__apple_mach_absolute_time"];
  #[cfg(target_arch = "x86_64")]
  let mut ordered_candidates = vec!["direct_ordered_wall__apple_mach_absolute_time"];
  #[cfg(target_arch = "x86_64")]
  if AppleX86TscDirect::try_for_current_machine().is_some() {
    instant_candidates.push("direct_wall__apple_invariant_rdtsc");
  }
  #[cfg(target_arch = "x86_64")]
  if AppleX86CommpageDirect::try_for_current_machine().is_some() {
    ordered_candidates.push("direct_ordered_wall__apple_commpage_lfence_rdtsc_nanotime");
  }
  #[cfg(target_arch = "x86_64")]
  let decision_rule = "each contract retains mach_absolute_time unless its eligible direct dispatcher wins by > max(1 ns/read, 5%) with >=8/9 decisive paired wins";
  #[cfg(target_arch = "x86_64")]
  let probe = serde_json::json!({
    "instant": tach::bench::apple_x86_instant_selection_measurements(),
    "ordered": tach::bench::apple_x86_wall_selection_measurements(),
  });
  #[cfg(target_arch = "aarch64")]
  let decision_rule = "Instant and OrderedInstant independently tournament every eligible Mach absolute, Mach continuous, and direct commpage path; a challenger wins only by > max(1 ns/read, 5%) in >=8/9 paired batches";
  #[cfg(target_arch = "aarch64")]
  let probe = serde_json::json!({
    "instant": tach::bench::apple_aarch64_instant_selection_measurements(),
    "ordered": tach::bench::apple_aarch64_ordered_selection_measurements(),
  });

  let payload = serde_json::json!({
    "selected_provider": {
      "instant": instant_provider,
      "ordered": ordered_provider,
    },
    "eligible_direct_candidates": {
      "instant": instant_candidates,
      "ordered": ordered_candidates,
    },
    "decision_rule": decision_rule,
    "probe": probe,
    "selected_native_benchmark": {
      "instant": format!("direct_selected_wall__{instant_provider}"),
      "ordered": format!("direct_selected_ordered_wall__{ordered_provider}"),
    },
  });
  #[cfg(target_arch = "x86_64")]
  let payload = {
    let mut payload = payload;
    payload["public_exact_probe"] = measure_apple_x86_public_exact();
    payload
  };
  #[cfg(target_arch = "aarch64")]
  let payload = {
    let mut payload = payload;
    let instant_probe =
      with_apple_aarch64_instant_read!(instant_provider, measure_apple_public_exact, instant);
    let ordered_probe =
      with_apple_aarch64_ordered_read!(ordered_provider, measure_apple_public_exact, ordered);
    payload["public_exact_probe"] = serde_json::json!({
      "instant": instant_probe,
      "ordered": ordered_probe,
    });
    payload
  };
  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create criterion directory");
  fs::write(
    directory.join("apple-wall-selection.json"),
    serde_json::to_vec_pretty(&payload).expect("serialize Apple wall evidence"),
  )
  .expect("write Apple wall evidence");
}

#[cfg(not(all(feature = "bench-internal", target_os = "macos")))]
fn write_apple_wall_selection() {}

#[cfg(all(feature = "bench-internal", target_os = "windows"))]
fn write_windows_wall_selection() {
  use std::fs;
  use std::path::PathBuf;

  let instant_provider = tach::bench::windows_wall_selected_provider();
  let ordered_provider = tach::bench::windows_ordered_wall_selected_provider();
  let probe = tach::bench::windows_wall_selection_measurements();
  let instant_candidates: Vec<_> = tach::bench::windows_wall_candidate_providers()
    .into_iter()
    .map(|provider| format!("direct_wall__{provider}"))
    .collect();
  let ordered_candidates: Vec<_> = tach::bench::windows_ordered_wall_candidate_providers()
    .into_iter()
    .map(|provider| format!("direct_ordered_wall__{provider}"))
    .collect();
  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  let ineligible_direct_candidates = serde_json::json!({
    "windows_raw_tsc": {
      "contracts": ["instant", "ordered"],
      "eligibility": "ineligible",
      "reason": "CPUID invariance and local measurement cannot prove Windows cross-core synchronization, platform-counter substitution, hypervisor bias, or live-migration continuity",
      "authority": "https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps",
    }
  });
  #[cfg(target_arch = "aarch64")]
  let ineligible_direct_candidates = serde_json::json!({
    "windows_raw_cntvct_el0": {
      "contracts": ["instant", "ordered"],
      "eligibility": "ineligible",
      "reason": "Windows may back QPC with a proprietary platform counter or the Arm Generic Timer and does not document CNTVCT_EL0 as an always-readable user-mode wall-clock ABI",
      "authority": "https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps",
    }
  });
  #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
  let ineligible_direct_candidates = serde_json::json!({});
  let payload = serde_json::json!({
    "selection_kind": "runtime_tournament",
    "selected_provider": {
      "instant": instant_provider,
      "ordered": ordered_provider,
    },
    "selected_native_benchmark": {
      "instant": format!("direct_selected_wall__{instant_provider}"),
      "ordered": format!("direct_selected_ordered_wall__{ordered_provider}"),
    },
    "eligible_direct_candidates": {
      "instant": instant_candidates,
      "ordered": ordered_candidates,
    },
    "ineligible_direct_candidates": ineligible_direct_candidates,
    "decision_rule": "Instant and OrderedInstant independently measure the complete call path of every available Windows-owned high-resolution monotonic source and retain the incumbent unless a challenger wins materially in at least eight of nine batches",
    "probe": probe,
    "ordering_contract": {
      "basis": "the opaque Windows API call boundary prevents compiler motion across the read, and the Windows-owned timeline orders events across processors without a separate raw-counter fence",
      "authority": "https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps",
      "focused_proof": {
        "workflow_run": 29366939799_u64,
        "tach_violations": 0_u64,
        "tach_reads": 945_307_669_u64,
        "ordered_violations": 0_u64,
        "ordered_reads": 1_060_680_201_u64,
        "std_violations": 0_u64,
        "std_reads": 1_185_012_196_u64,
      },
    },
  });
  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create criterion directory");
  fs::write(
    directory.join("windows-wall-selection.json"),
    serde_json::to_vec_pretty(&payload).expect("serialize Windows wall evidence"),
  )
  .expect("write Windows wall evidence");
}

#[cfg(not(all(feature = "bench-internal", target_os = "windows")))]
fn write_windows_wall_selection() {}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
))]
fn write_linux_x86_wall_selection() {
  use std::fs;
  use std::path::PathBuf;

  let instant_candidates: Vec<_> = tach::bench::linux_x86_instant_candidate_primitives()
    .iter()
    .map(|candidate| format!("direct_wall__{}", candidate.provider()))
    .collect();
  let ordered_candidates: Vec<_> = tach::bench::linux_x86_ordered_candidate_primitives()
    .iter()
    .map(|candidate| format!("direct_ordered_wall__{}", candidate.provider()))
    .collect();
  let instant_selected = tach::bench::linux_x86_selected_instant_primitive();
  let ordered_selected = tach::bench::linux_x86_selected_ordered_primitive();
  let mut payload = serde_json::json!({
    "selected_provider": {
      "instant": instant_selected.provider(),
      "ordered": ordered_selected.provider(),
    },
    "selected_native_benchmark": {
      "instant": format!("direct_selected_wall__{}", instant_selected.provider()),
      "ordered": format!("direct_selected_ordered_wall__{}", ordered_selected.provider()),
    },
    "eligible_direct_candidates": {
      "instant": instant_candidates,
      "ordered": ordered_candidates,
    },
    "decision_rule": "each contract independently tournaments every eligible complete clock-id, entry-ABI, ordering-barrier, and direct-TSC path; a challenger wins only by > max(1 ns/read, 5%) in >=8/9 paired batches",
    "probe": tach::bench::linux_x86_wall_selection_measurements(),
    "post_init_boundary": "PR_SET_TSC(PR_TSC_SIGSEGV) must not revoke TSC access after direct-provider selection",
  });
  let instant_provider = instant_selected.provider();
  let instant_scale = instant_selected.nanos_per_tick_q32();
  let instant_public_exact = with_linux_x86_instant_read!(
    instant_provider,
    measure_linux_x86_public_exact,
    instant,
    instant_scale
  );
  let ordered_provider = ordered_selected.provider();
  let ordered_scale = ordered_selected.nanos_per_tick_q32();
  let ordered_public_exact = with_linux_x86_ordered_read!(
    ordered_provider,
    measure_linux_x86_public_exact,
    ordered,
    ordered_scale
  );
  payload["public_exact_probe"] = serde_json::json!({
    "instant": instant_public_exact,
    "ordered": ordered_public_exact,
  });
  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create criterion directory");
  fs::write(
    directory.join("linux-x86-wall-selection.json"),
    serde_json::to_vec_pretty(&payload).expect("serialize Linux x86 wall selector evidence"),
  )
  .expect("write Linux x86 wall selector evidence");
}

#[cfg(not(all(
  feature = "bench-internal",
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "x86"),
)))]
fn write_linux_x86_wall_selection() {}

#[cfg(all(
  feature = "bench-internal",
  target_arch = "aarch64",
  any(target_os = "android", target_os = "linux"),
))]
fn write_linux_aarch64_wall_selection() {
  use std::fs;
  use std::path::PathBuf;

  let instant_candidates: Vec<_> = tach::bench::linux_aarch64_instant_candidate_primitives()
    .iter()
    .map(|candidate| format!("direct_wall__{}", candidate.provider()))
    .collect();
  let ordered_candidates: Vec<_> = tach::bench::linux_aarch64_ordered_candidate_primitives()
    .iter()
    .map(|candidate| format!("direct_ordered_wall__{}", candidate.provider()))
    .collect();
  let instant_selected = tach::bench::linux_aarch64_selected_instant_primitive();
  let ordered_selected = tach::bench::linux_aarch64_selected_ordered_primitive();
  let payload = serde_json::json!({
    "selected_provider": {
      "instant": instant_selected.provider(),
      "ordered": ordered_selected.provider(),
    },
    "selected_native_benchmark": {
      "instant": format!("direct_selected_wall__{}", instant_selected.provider()),
      "ordered": format!("direct_selected_ordered_wall__{}", ordered_selected.provider()),
    },
    "eligible_direct_candidates": {
      "instant": instant_candidates,
      "ordered": ordered_candidates,
    },
    "decision_rule": "each contract independently tournaments every eligible complete MONOTONIC, MONOTONIC_RAW, raw-syscall, and architectural-counter path; a challenger wins only by > max(1 ns/read, 5%) in >=8/9 paired batches",
    "instant_probe": tach::bench::linux_aarch64_instant_selection_measurements(),
    "ordered_probe": tach::bench::linux_aarch64_ordered_selection_measurements(),
    "permission_rule": "PR_GET_TSC is authoritative when implemented, including Android/vendor backports; only exact -EINVAL plus a parsed upstream-pre-6.12 arm64 uname release infers legacy-safe counter access; newer, unknown, and other failed queries remain syscall-only",
    "feat_sb": "ineligible: Arm SB constrains side-channel-observable speculation but does not order architectural counter sampling after a prior Acquire observation",
    "kernel_errata": "trapped CNTVCT/CNTVCTSS reads remain eligible because arm64 emulates them with its workaround-aware counter reader; exact-path measurement determines profitability",
    "post_init_boundary": "PR_SET_TSC(PR_TSC_SIGSEGV) must not revoke counter access after direct-provider selection",
  });
  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create criterion directory");
  fs::write(
    directory.join("linux-aarch64-wall-selection.json"),
    serde_json::to_vec_pretty(&payload).expect("serialize Linux aarch64 wall selector evidence"),
  )
  .expect("write Linux aarch64 wall selector evidence");
}

#[cfg(not(all(
  feature = "bench-internal",
  target_arch = "aarch64",
  any(target_os = "android", target_os = "linux"),
)))]
fn write_linux_aarch64_wall_selection() {}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      any(target_arch = "x86_64", target_arch = "x86"),
      not(target_os = "windows"),
      not(all(target_arch = "x86_64", target_os = "macos")),
    ),
    all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
    target_os = "macos",
  ),
))]
fn write_ordered_selection() {
  use std::fs;
  use std::path::PathBuf;

  let mut candidates = Vec::new();
  #[cfg(all(any(target_arch = "x86_64", target_arch = "x86"), not(target_os = "macos")))]
  {
    if OrderedX86CpuidDirect::try_for_current_machine().is_some() {
      candidates.push("direct_ordered__x86_cpuid_rdtsc");
    }
    if OrderedX86LfenceDirect::try_for_current_machine().is_some() {
      candidates.push("direct_ordered__x86_lfence_rdtsc");
    }
    if OrderedX86MfenceDirect::try_for_current_machine().is_some() {
      candidates.push("direct_ordered__x86_mfence_rdtsc");
    }
    if OrderedX86RdtscpDirect::try_for_current_machine().is_some() {
      candidates.push("direct_ordered__x86_rdtscp");
    }
    if OrderedX86SerializeDirect::try_for_current_machine().is_some() {
      candidates.push("direct_ordered__x86_serialize_rdtsc");
    }
  }
  #[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))]
  {
    candidates.push("direct_ordered__aarch64_isb_cntvct");
    if OrderedAarch64CntvctssDirect::try_for_current_machine().is_some() {
      candidates.push("direct_ordered__aarch64_cntvctss");
    }
  }
  #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
  candidates.push("direct_ordered_wall__apple_mach_absolute_time");
  #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
  if AppleX86CommpageDirect::try_for_current_machine().is_some() {
    candidates.push("direct_ordered_wall__apple_commpage_lfence_rdtsc_nanotime");
  }
  #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
  candidates.extend(
    tach::bench::apple_aarch64_ordered_candidate_primitives()
      .iter()
      .map(|candidate| format!("direct_ordered_wall__{}", candidate.provider())),
  );

  #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
  let decision_rule = "every eligible Mach absolute, Mach continuous, and direct commpage path competes by repeatable material wins";
  #[cfg(any(not(target_os = "macos"), all(target_os = "macos", target_arch = "x86_64")))]
  let decision_rule = "median advantage > max(1 ns/read, 5%) and >=8/9 decisive paired wins";
  #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
  let probe = serde_json::to_value(tach::bench::apple_aarch64_ordered_selection_measurements())
    .expect("serialize Apple aarch64 Ordered selector evidence");
  #[cfg(any(not(target_os = "macos"), all(target_os = "macos", target_arch = "x86_64")))]
  let probe = serde_json::to_value(tach::bench::ordered_selection_measurements())
    .expect("serialize Ordered selector evidence");

  let payload = serde_json::json!({
    "selected_provider": tach::bench::ordered_selected_provider(),
    "eligible_direct_candidates": candidates,
    "decision_rule": decision_rule,
    "probe": probe,
  });
  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create criterion directory");
  fs::write(
    directory.join("ordered-selection.json"),
    serde_json::to_vec_pretty(&payload).expect("serialize ordered selector evidence"),
  )
  .expect("write ordered selector evidence");
}

#[cfg(not(all(
  feature = "bench-internal",
  any(
    all(
      any(target_arch = "x86_64", target_arch = "x86"),
      not(target_os = "windows"),
      not(all(target_arch = "x86_64", target_os = "macos")),
    ),
    all(target_arch = "aarch64", any(target_os = "android", target_os = "linux"),),
    target_os = "macos",
  ),
)))]
fn write_ordered_selection() {}

fn bench_elapsed(c: &mut Criterion) {
  write_criterion_runtime_attestation();
  quanta::Instant::now();
  fastant::Instant::now();
  minstant::Instant::now();

  let mut g = c.benchmark_group("Instant::now() + elapsed()");
  g.bench_function("tach", |b| {
    b.iter(|| {
      let start = Instant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("tach_ordered", |b| {
    b.iter(|| {
      let start = OrderedInstant::now();
      black_box(start.elapsed())
    });
  });
  #[cfg(all(
    feature = "bench-internal",
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86_64", target_arch = "x86"),
  ))]
  {
    for candidate in tach::bench::linux_x86_instant_candidate_primitives() {
      let provider = candidate.provider();
      let nanos_per_tick_q32 = candidate.nanos_per_tick_q32();
      with_linux_x86_instant_read!(
        provider,
        register_selected_elapsed,
        g,
        "direct_wall",
        provider,
        nanos_per_tick_q32
      );
    }
    for candidate in tach::bench::linux_x86_ordered_candidate_primitives() {
      let provider = candidate.provider();
      let nanos_per_tick_q32 = candidate.nanos_per_tick_q32();
      with_linux_x86_ordered_read!(
        provider,
        register_selected_elapsed,
        g,
        "direct_ordered_wall",
        provider,
        nanos_per_tick_q32
      );
    }
    let selected = tach::bench::linux_x86_selected_instant_primitive();
    let provider = selected.provider();
    let nanos_per_tick_q32 = selected.nanos_per_tick_q32();
    with_linux_x86_instant_read!(
      provider,
      register_selected_elapsed,
      g,
      "direct_selected_wall",
      provider,
      nanos_per_tick_q32
    );
  }
  #[cfg(all(
    feature = "bench-internal",
    any(target_os = "android", target_os = "linux"),
    any(target_arch = "x86_64", target_arch = "x86"),
  ))]
  {
    let selected = tach::bench::linux_x86_selected_ordered_primitive();
    let provider = selected.provider();
    let nanos_per_tick_q32 = selected.nanos_per_tick_q32();
    with_linux_x86_ordered_read!(
      provider,
      register_selected_elapsed,
      g,
      "direct_selected_ordered_wall",
      provider,
      nanos_per_tick_q32
    );
  }
  #[cfg(all(
    feature = "bench-internal",
    target_arch = "aarch64",
    any(target_os = "android", target_os = "linux"),
  ))]
  {
    for candidate in tach::bench::linux_aarch64_instant_candidate_primitives() {
      let provider = candidate.provider();
      let nanos_per_tick_q32 = candidate.nanos_per_tick_q32();
      with_linux_aarch64_instant_read!(
        provider,
        register_selected_elapsed,
        g,
        "direct_wall",
        provider,
        nanos_per_tick_q32
      );
    }
    for candidate in tach::bench::linux_aarch64_ordered_candidate_primitives() {
      let provider = candidate.provider();
      let nanos_per_tick_q32 = candidate.nanos_per_tick_q32();
      with_linux_aarch64_ordered_read!(
        provider,
        register_selected_elapsed,
        g,
        "direct_ordered_wall",
        provider,
        nanos_per_tick_q32
      );
    }
    let selected = tach::bench::linux_aarch64_selected_instant_primitive();
    let provider = selected.provider();
    let nanos_per_tick_q32 = selected.nanos_per_tick_q32();
    with_linux_aarch64_instant_read!(
      provider,
      register_selected_elapsed,
      g,
      "direct_selected_wall",
      provider,
      nanos_per_tick_q32
    );

    let selected = tach::bench::linux_aarch64_selected_ordered_primitive();
    let provider = selected.provider();
    let nanos_per_tick_q32 = selected.nanos_per_tick_q32();
    with_linux_aarch64_ordered_read!(
      provider,
      register_selected_elapsed,
      g,
      "direct_selected_ordered_wall",
      provider,
      nanos_per_tick_q32
    );
  }
  #[cfg(all(
    feature = "bench-internal",
    any(
      all(target_arch = "riscv64", target_os = "linux"),
      all(target_arch = "loongarch64", target_os = "linux"),
      all(target_arch = "powerpc64", target_os = "linux", target_env = "gnu"),
      all(target_arch = "x86_64", target_os = "freebsd"),
      all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x")),
    ),
  ))]
  {
    for candidate in tach::bench::residual_instant_candidate_primitives() {
      let provider = candidate.provider();
      let nanos_per_tick_q32 = candidate.nanos_per_tick_q32();
      with_residual_instant_read!(
        provider,
        register_selected_elapsed,
        g,
        "direct_wall",
        provider,
        nanos_per_tick_q32
      );
    }
    for candidate in tach::bench::residual_ordered_candidate_primitives() {
      let provider = candidate.provider();
      let nanos_per_tick_q32 = candidate.nanos_per_tick_q32();
      with_residual_ordered_read!(
        provider,
        register_selected_elapsed,
        g,
        "direct_ordered_wall",
        provider,
        nanos_per_tick_q32
      );
    }
    let selected = tach::bench::residual_selected_instant_primitive();
    let provider = selected.provider();
    let nanos_per_tick_q32 = selected.nanos_per_tick_q32();
    with_residual_instant_read!(
      provider,
      register_selected_elapsed,
      g,
      "direct_selected_wall",
      provider,
      nanos_per_tick_q32
    );
    let selected = tach::bench::residual_selected_ordered_primitive();
    let provider = selected.provider();
    let nanos_per_tick_q32 = selected.nanos_per_tick_q32();
    with_residual_ordered_read!(
      provider,
      register_selected_elapsed,
      g,
      "direct_selected_ordered_wall",
      provider,
      nanos_per_tick_q32
    );
  }
  #[cfg(all(feature = "bench-internal", target_os = "macos"))]
  {
    let native = MachAbsoluteTimeDirect::for_current_machine();
    #[cfg(target_arch = "x86_64")]
    g.bench_function("direct_wall__apple_mach_absolute_time", |b| {
      b.iter(|| {
        let start = native.now_ticks();
        black_box(native.elapsed_since(start))
      });
    });
    #[cfg(target_arch = "x86_64")]
    g.bench_function("direct_ordered_wall__apple_mach_absolute_time", |b| {
      b.iter(|| {
        let start = native.now_ticks();
        black_box(native.elapsed_since(start))
      });
    });
    #[cfg(target_arch = "aarch64")]
    g.bench_function("native_wall__mach_absolute_time", |b| {
      b.iter(|| {
        let start = native.now_ticks();
        black_box(native.elapsed_since(start))
      });
    });
    #[cfg(target_arch = "x86_64")]
    {
      let instant_provider = tach::bench::apple_wall_selected_provider();
      let ordered_provider = tach::bench::apple_ordered_wall_selected_provider();
      let nanos_per_tick_q32 = tach::bench::apple_x86_selected_nanos_per_tick_q32();
      if let Some(tsc) = AppleX86TscDirect::try_for_current_machine() {
        let tsc_nanos_per_tick_q32 = tsc.nanos_per_tick_q32();
        g.bench_function(format!("direct_wall__{}", tsc.provider()), |b| {
          b.iter(|| {
            let start = tsc.now_ticks();
            let elapsed = tsc.now_ticks().saturating_sub(start);
            black_box(tach::bench::exact_ticks_to_duration_with_scale(
              elapsed,
              tsc_nanos_per_tick_q32,
            ))
          });
        });
      }
      if let Some(commpage) = AppleX86CommpageDirect::try_for_current_machine() {
        g.bench_function(format!("direct_ordered_wall__{}", commpage.provider()), |b| {
          b.iter(|| {
            let start = commpage.now_ticks();
            black_box(commpage.elapsed_since(start))
          });
        });
      }
      g.bench_function(format!("direct_selected_wall__{instant_provider}"), |b| {
        b.iter(|| {
          let start = tach::bench::apple_x86_selected_ticks();
          let elapsed = tach::bench::apple_x86_selected_ticks().saturating_sub(start);
          black_box(tach::bench::exact_ticks_to_duration_with_scale(elapsed, nanos_per_tick_q32))
        });
      });
      g.bench_function(format!("direct_selected_ordered_wall__{ordered_provider}"), |b| {
        b.iter(|| {
          let start = tach::bench::apple_x86_selected_ordered_ticks();
          let elapsed = tach::bench::apple_x86_selected_ordered_ticks().saturating_sub(start);
          black_box(tach::bench::exact_ticks_to_duration_with_scale(elapsed, 1_u64 << 32))
        });
      });
    }
  }
  #[cfg(all(feature = "bench-internal", target_arch = "aarch64", target_os = "macos"))]
  {
    for candidate in tach::bench::apple_aarch64_instant_candidate_primitives() {
      let provider = candidate.provider();
      let nanos_per_tick_q32 = candidate.nanos_per_tick_q32();
      with_apple_aarch64_instant_read!(
        provider,
        register_selected_elapsed,
        g,
        "direct_wall",
        provider,
        nanos_per_tick_q32
      );
    }
    for candidate in tach::bench::apple_aarch64_ordered_candidate_primitives() {
      let provider = candidate.provider();
      let nanos_per_tick_q32 = candidate.nanos_per_tick_q32();
      with_apple_aarch64_ordered_read!(
        provider,
        register_selected_elapsed,
        g,
        "direct_ordered_wall",
        provider,
        nanos_per_tick_q32
      );
    }
    let selected = tach::bench::apple_aarch64_selected_instant_primitive();
    let provider = selected.provider();
    let nanos_per_tick_q32 = selected.nanos_per_tick_q32();
    with_apple_aarch64_instant_read!(
      provider,
      register_selected_elapsed,
      g,
      "direct_selected_wall",
      provider,
      nanos_per_tick_q32
    );
    let selected = tach::bench::apple_aarch64_selected_ordered_primitive();
    let provider = selected.provider();
    let nanos_per_tick_q32 = selected.nanos_per_tick_q32();
    with_apple_aarch64_ordered_read!(
      provider,
      register_selected_elapsed,
      g,
      "direct_selected_ordered_wall",
      provider,
      nanos_per_tick_q32
    );
  }
  #[cfg(all(feature = "bench-internal", target_os = "windows"))]
  {
    macro_rules! register_windows_elapsed {
      ($prefix:literal, $provider:expr, $read:path, $convert:path) => {{
        let _ = g.bench_function(format!("{}__{}", $prefix, $provider), |b| {
          b.iter(|| {
            let start = $read();
            black_box($convert($read().saturating_sub(start)))
          });
        });
      }};
    }
    for provider in tach::bench::windows_wall_candidate_providers() {
      match provider {
        "windows_query_interrupt_time_precise" => register_windows_elapsed!(
          "direct_wall",
          provider,
          tach::bench::windows_interrupt_time_precise_ticks,
          tach::bench::windows_precise_delta_to_duration
        ),
        "windows_query_unbiased_interrupt_time_precise" => register_windows_elapsed!(
          "direct_wall",
          provider,
          tach::bench::windows_unbiased_interrupt_time_precise_ticks,
          tach::bench::windows_precise_delta_to_duration
        ),
        _ => register_windows_elapsed!(
          "direct_wall",
          provider,
          tach::bench::windows_qpc_ticks,
          tach::bench::windows_qpc_delta_to_duration
        ),
      }
    }
    let instant_provider = tach::bench::windows_wall_selected_provider();
    match instant_provider {
      "windows_query_interrupt_time_precise" => register_windows_elapsed!(
        "direct_selected_wall",
        instant_provider,
        tach::bench::windows_interrupt_time_precise_ticks,
        tach::bench::windows_precise_delta_to_duration
      ),
      "windows_query_unbiased_interrupt_time_precise" => register_windows_elapsed!(
        "direct_selected_wall",
        instant_provider,
        tach::bench::windows_unbiased_interrupt_time_precise_ticks,
        tach::bench::windows_precise_delta_to_duration
      ),
      _ => register_windows_elapsed!(
        "direct_selected_wall",
        instant_provider,
        tach::bench::windows_qpc_ticks,
        tach::bench::windows_qpc_delta_to_duration
      ),
    }
    for provider in tach::bench::windows_ordered_wall_candidate_providers() {
      match provider {
        "windows_query_interrupt_time_precise_call_boundary" => register_windows_elapsed!(
          "direct_ordered_wall",
          provider,
          tach::bench::windows_interrupt_time_precise_ticks,
          tach::bench::windows_precise_delta_to_duration
        ),
        "windows_query_unbiased_interrupt_time_precise_call_boundary" => {
          register_windows_elapsed!(
            "direct_ordered_wall",
            provider,
            tach::bench::windows_unbiased_interrupt_time_precise_ticks,
            tach::bench::windows_precise_delta_to_duration
          )
        }
        _ => register_windows_elapsed!(
          "direct_ordered_wall",
          provider,
          tach::bench::windows_qpc_ticks,
          tach::bench::windows_qpc_delta_to_duration
        ),
      }
    }
    let ordered_provider = tach::bench::windows_ordered_wall_selected_provider();
    match ordered_provider {
      "windows_query_interrupt_time_precise_call_boundary" => register_windows_elapsed!(
        "direct_selected_ordered_wall",
        ordered_provider,
        tach::bench::windows_interrupt_time_precise_ticks,
        tach::bench::windows_precise_delta_to_duration
      ),
      "windows_query_unbiased_interrupt_time_precise_call_boundary" => {
        register_windows_elapsed!(
          "direct_selected_ordered_wall",
          ordered_provider,
          tach::bench::windows_unbiased_interrupt_time_precise_ticks,
          tach::bench::windows_precise_delta_to_duration
        )
      }
      _ => register_windows_elapsed!(
        "direct_selected_ordered_wall",
        ordered_provider,
        tach::bench::windows_qpc_ticks,
        tach::bench::windows_qpc_delta_to_duration
      ),
    }
  }
  g.bench_function("quanta", |b| {
    b.iter(|| {
      let start = quanta::Instant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("fastant", |b| {
    b.iter(|| {
      let start = fastant::Instant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("minstant", |b| {
    b.iter(|| {
      let start = minstant::Instant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("std", |b| {
    b.iter(|| {
      let start = StdInstant::now();
      black_box(start.elapsed())
    });
  });
  g.finish();
}

fn thread_cpu_bench_id() -> &'static str {
  match (ThreadCpuInstant::provider(), ThreadCpuInstant::read_cost_hint()) {
    (ThreadCpuProvider::LinuxPerfMmap, ThreadCpuReadCost::Inline) => {
      "tach_thread_cpu__linux_perf_mmap__inline"
    }
    (ThreadCpuProvider::LinuxPerfMmap, ThreadCpuReadCost::SystemCall) => {
      "tach_thread_cpu__linux_perf_mmap__system_call"
    }
    (ThreadCpuProvider::LinuxPerfRead, ThreadCpuReadCost::SystemCall) => {
      "tach_thread_cpu__linux_perf_read__system_call"
    }
    (ThreadCpuProvider::PosixThreadCpuClock, ThreadCpuReadCost::SystemCall) => {
      "tach_thread_cpu__posix_thread_cpu_clock__system_call"
    }
    (ThreadCpuProvider::WindowsThreadTimes, ThreadCpuReadCost::SystemCall) => {
      "tach_thread_cpu__windows_thread_times__system_call"
    }
    (ThreadCpuProvider::WasiThreadCpuClock, ThreadCpuReadCost::HostCall) => {
      "tach_thread_cpu__wasi_thread_cpu_clock__host_call"
    }
    (ThreadCpuProvider::NodeThreadCpuUsage, ThreadCpuReadCost::HostCall) => {
      "tach_thread_cpu__node_thread_cpu_usage__host_call"
    }
    (ThreadCpuProvider::PerformanceNow, ThreadCpuReadCost::HostCall) => {
      "tach_thread_cpu__performance_now__host_call"
    }
    (ThreadCpuProvider::NodeHrtime, ThreadCpuReadCost::HostCall) => {
      "tach_thread_cpu__node_hrtime__host_call"
    }
    (ThreadCpuProvider::MonotonicWallClock, ThreadCpuReadCost::Inline) => {
      "tach_thread_cpu__monotonic_wall_clock__inline"
    }
    (ThreadCpuProvider::MonotonicWallClock, ThreadCpuReadCost::SystemCall) => {
      "tach_thread_cpu__monotonic_wall_clock__system_call"
    }
    (ThreadCpuProvider::MonotonicWallClock, ThreadCpuReadCost::HostCall) => {
      "tach_thread_cpu__monotonic_wall_clock__host_call"
    }
    (_, ThreadCpuReadCost::Inline) => "tach_thread_cpu__other__inline",
    (_, ThreadCpuReadCost::SystemCall) => "tach_thread_cpu__other__system_call",
    (_, ThreadCpuReadCost::HostCall) => "tach_thread_cpu__other__host_call",
    (_, ThreadCpuReadCost::Unavailable) => "tach_thread_cpu__unavailable",
    (_, _) => "tach_thread_cpu__other__unknown_cost",
  }
}

#[allow(dead_code)]
fn thread_cpu_selected_path_is(path: &str) -> bool {
  matches!(
    (path, ThreadCpuInstant::provider()),
    ("linux_perf_mmap", ThreadCpuProvider::LinuxPerfMmap)
      | ("linux_perf_read", ThreadCpuProvider::LinuxPerfRead)
      | ("posix_thread_cpu", ThreadCpuProvider::PosixThreadCpuClock)
  ) && (path != "posix_thread_cpu" || !measured_thread_cpu_path_available())
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
#[allow(dead_code)]
fn thread_cpu_fallback_path_is(path: &str) -> bool {
  let _ = path;
  false
}

#[cfg(not(all(
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
)))]
#[allow(dead_code)]
const fn thread_cpu_fallback_path_is(_path: &str) -> bool {
  false
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
#[allow(dead_code)]
fn measured_thread_cpu_path_available() -> bool {
  tach::bench::thread_cpu_perf_path_evidence().is_some()
}

#[cfg(not(all(
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
)))]
#[allow(dead_code)]
const fn measured_thread_cpu_path_available() -> bool {
  false
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
fn native_thread_cpu_mechanism() -> &'static str {
  #[cfg(any(
    all(target_arch = "x86_64", any(target_os = "linux", target_os = "android")),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ))]
  {
    return tach::bench::thread_cpu_native64_selection_measurements().selected_provider;
  }
  #[cfg(all(target_arch = "x86", target_os = "linux"))]
  {
    return tach::bench::thread_cpu_i686_native_provider();
  }
  #[cfg(all(target_arch = "arm", target_os = "linux"))]
  {
    return tach::bench::thread_cpu_arm_native_provider();
  }
  #[cfg(all(target_arch = "riscv64", target_os = "linux"))]
  {
    return tach::bench::thread_cpu_riscv64_native_provider();
  }
  #[cfg(all(
    target_os = "linux",
    any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
  ))]
  {
    return tach::bench::thread_cpu_rare_linux_native_provider();
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
fn thread_cpu_path_mechanism(path: &str) -> Option<String> {
  match path {
    "posix_thread_cpu" => Some(native_thread_cpu_mechanism().to_owned()),
    "linux_perf_read" => tach::bench::thread_cpu_perf_read_entry_evidence()
      .map(|evidence| format!("linux_perf_read__{}", evidence.selected_candidate)),
    "linux_perf_mmap" => {
      #[cfg(any(
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
      ))]
      {
        let handle = ThreadCpuPerfHandle::try_for_current_thread()?;
        Some(format!("linux_perf_mmap__{}", handle.selected_candidate_name()))
      }
      #[cfg(not(any(
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
      )))]
      {
        None
      }
    }
    _ => None,
  }
}

fn bench_thread_cpu_now(c: &mut Criterion) {
  write_criterion_runtime_attestation();
  // Provider selection is intentionally outside the measured loop: the API's
  // contract is steady-state `now()`, after the calling thread's assessment.
  let tach_id = thread_cpu_bench_id();
  let mut g = c.benchmark_group("ThreadCpuInstant::now()");
  g.bench_function(tach_id, |b| b.iter(|| black_box(ThreadCpuInstant::now())));
  g.bench_function(NATIVE_THREAD_CPU_BENCH_ID, |b| {
    b.iter(|| black_box(native_thread_cpu_now()));
  });
  #[cfg(all(feature = "bench-internal", target_os = "macos"))]
  {
    assert_eq!(
      ThreadCpuInstant::provider(),
      ThreadCpuProvider::PosixThreadCpuClock,
      "macOS ThreadCpuInstant must use its native current-thread CPU clock",
    );
    assert_eq!(
      ThreadCpuInstant::read_cost_hint(),
      ThreadCpuReadCost::SystemCall,
      "macOS ThreadCpuInstant must retain its system-call cost classification",
    );
    for prefix in ["direct_thread_cpu", "direct_selected_thread_cpu"] {
      g.bench_function(format!("{prefix}__{MACOS_THREAD_CPU_MECHANISM}"), |b| {
        b.iter(|| black_box(native_thread_cpu_now()));
      });
    }
  }
  #[cfg(all(feature = "bench-internal", target_os = "windows"))]
  {
    for prefix in ["direct_thread_cpu", "direct_selected_thread_cpu"] {
      g.bench_function(format!("{prefix}__{WINDOWS_THREAD_CPU_MECHANISM}"), |b| {
        b.iter(|| black_box(native_thread_cpu_now()));
      });
    }
    let _ = tach::bench::windows_thread_cpu_wall_fallback_now();
    g.bench_function(
      format!("direct_fallback_thread_cpu__{WINDOWS_THREAD_CPU_WALL_FALLBACK_MECHANISM}"),
      |b| b.iter(|| black_box(tach::bench::windows_thread_cpu_wall_fallback_now())),
    );
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
  {
    if let Some(perf) = ThreadCpuPerfHandle::try_for_current_thread() {
      for candidate in 0..perf.candidate_count() {
        let Some(name) = perf.candidate_name(candidate) else {
          continue;
        };
        if !perf.select_candidate(candidate) {
          continue;
        }
        g.bench_function(format!("direct_thread_cpu__linux_perf_mmap__{name}"), |b| {
          b.iter(|| black_box(perf.now_nanos()));
        });
      }
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
  {
    if let Some(perf) = ThreadCpuPerfReadHandle::try_for_current_thread() {
      for candidate in 0..perf.candidate_count() {
        let Some(name) = perf.candidate_name(candidate) else {
          continue;
        };
        if !perf.select_candidate(candidate) {
          continue;
        }
        g.bench_function(format!("direct_thread_cpu__linux_perf_read__{name}"), |b| {
          b.iter(|| black_box(perf.direct_nanos()));
        });
      }
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
  {
    if let (Some(evidence), Some(paths)) = (
      tach::bench::thread_cpu_perf_path_evidence(),
      ThreadCpuPerfPathHandle::try_for_current_thread(),
    ) {
      let selected_mechanism = thread_cpu_path_mechanism(evidence.selected_path)
        .expect("selected measured thread-CPU path must identify its exact mechanism");
      let fallback_mechanism = thread_cpu_path_mechanism(evidence.fallback_path)
        .expect("fallback measured thread-CPU path must identify its exact mechanism");
      for (prefix, path, mechanism) in [
        ("direct_selected_thread_cpu", evidence.selected_path, selected_mechanism),
        ("direct_fallback_thread_cpu", evidence.fallback_path, fallback_mechanism),
      ] {
        let mut registered = false;
        for candidate in 0..paths.candidate_count() {
          if paths.candidate_name(candidate) == Some(path)
            && paths.candidate_available(candidate)
            && paths.select_candidate(candidate)
          {
            g.bench_function(format!("{prefix}__{mechanism}"), |b| {
              b.iter(|| black_box(paths.now_nanos()));
            });
            registered = true;
            break;
          }
        }
        assert!(registered, "measured {path} path lacks its exact public-dispatch row");
      }
    }
  }
  #[cfg(any(
    all(
      any(target_arch = "x86_64", target_arch = "aarch64"),
      any(target_os = "linux", target_os = "android"),
    ),
    all(target_arch = "x86_64", target_os = "freebsd"),
  ))]
  {
    let evidence = tach::bench::thread_cpu_native64_selection_measurements();
    if evidence.libc_available {
      g.bench_function(format!("direct_thread_cpu__{}", evidence.libc_provider), |b| {
        b.iter(|| black_box(tach::bench::thread_cpu_native64_exact_libc_nanos()))
      });
    }
    if evidence.raw_available {
      g.bench_function(format!("direct_thread_cpu__{}", evidence.raw_provider), |b| {
        b.iter(|| black_box(tach::bench::thread_cpu_native64_exact_raw_nanos()))
      });
    }
    for prefix in [
      thread_cpu_selected_path_is("posix_thread_cpu").then_some("direct_selected_thread_cpu"),
      thread_cpu_fallback_path_is("posix_thread_cpu").then_some("direct_fallback_thread_cpu"),
    ]
    .into_iter()
    .flatten()
    {
      if evidence.selected_provider == evidence.libc_provider {
        g.bench_function(format!("{prefix}__{}", evidence.selected_provider), |b| {
          b.iter(|| black_box(tach::bench::thread_cpu_native64_exact_libc_nanos()))
        });
      } else {
        g.bench_function(format!("{prefix}__{}", evidence.selected_provider), |b| {
          b.iter(|| black_box(tach::bench::thread_cpu_native64_exact_raw_nanos()))
        });
      }
    }
  }
  #[cfg(all(target_arch = "x86", target_os = "linux"))]
  {
    let evidence = tach::bench::thread_cpu_i686_native_selection_evidence();
    for name in evidence.candidate_names {
      let index = match name {
        "libc_clock_gettime" => 0,
        "linux_i686_time32_syscall" => 1,
        "linux_i686_time64_syscall" => 2,
        _ => continue,
      };
      g.bench_function(format!("direct_thread_cpu__{name}"), |b| {
        b.iter(|| black_box(tach::bench::thread_cpu_i686_exact_candidate(index)))
      });
    }
    let selected = tach::bench::thread_cpu_i686_native_provider();
    for prefix in [
      thread_cpu_selected_path_is("posix_thread_cpu").then_some("direct_selected_thread_cpu"),
      thread_cpu_fallback_path_is("posix_thread_cpu").then_some("direct_fallback_thread_cpu"),
    ]
    .into_iter()
    .flatten()
    {
      g.bench_function(format!("{prefix}__{selected}"), |b| {
        b.iter(|| black_box(tach::bench::thread_cpu_i686_selected_native_nanos()))
      });
    }
  }
  #[cfg(all(target_arch = "arm", target_os = "linux"))]
  {
    let evidence = tach::bench::thread_cpu_arm_native_selection_evidence();
    for name in evidence.candidate_names {
      let index = match name {
        "libc_clock_gettime" => 0,
        "linux_arm_time32_syscall" => 1,
        "linux_arm_time64_syscall" => 2,
        _ => continue,
      };
      g.bench_function(format!("direct_thread_cpu__{name}"), |b| {
        b.iter(|| black_box(tach::bench::thread_cpu_arm_exact_candidate(index)))
      });
    }
    let selected = tach::bench::thread_cpu_arm_native_provider();
    for prefix in [
      thread_cpu_selected_path_is("posix_thread_cpu").then_some("direct_selected_thread_cpu"),
      thread_cpu_fallback_path_is("posix_thread_cpu").then_some("direct_fallback_thread_cpu"),
    ]
    .into_iter()
    .flatten()
    {
      g.bench_function(format!("{prefix}__{selected}"), |b| {
        b.iter(|| black_box(tach::bench::thread_cpu_arm_selected_native_nanos()))
      });
    }
  }
  #[cfg(all(target_arch = "riscv64", target_os = "linux"))]
  {
    let evidence = tach::bench::thread_cpu_riscv64_native_selection_evidence();
    for name in evidence.candidate_names {
      let index = match name {
        "libc_clock_gettime" => 0,
        "linux_riscv64_syscall" => 1,
        _ => continue,
      };
      g.bench_function(format!("direct_thread_cpu__{name}"), |b| {
        b.iter(|| black_box(tach::bench::thread_cpu_riscv64_exact_candidate(index)))
      });
    }
    let selected = tach::bench::thread_cpu_riscv64_native_provider();
    for prefix in [
      thread_cpu_selected_path_is("posix_thread_cpu").then_some("direct_selected_thread_cpu"),
      thread_cpu_fallback_path_is("posix_thread_cpu").then_some("direct_fallback_thread_cpu"),
    ]
    .into_iter()
    .flatten()
    {
      g.bench_function(format!("{prefix}__{selected}"), |b| {
        b.iter(|| black_box(tach::bench::thread_cpu_riscv64_selected_native_nanos()))
      });
    }
  }
  #[cfg(all(
    target_os = "linux",
    any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
  ))]
  {
    let evidence = tach::bench::thread_cpu_rare_linux_native_selection_evidence();
    for name in evidence.candidate_names {
      let index = if name == "libc_clock_gettime" {
        0
      } else if name.contains("raw_scv") {
        2
      } else {
        1
      };
      g.bench_function(format!("direct_thread_cpu__{name}"), |b| {
        b.iter(|| black_box(tach::bench::thread_cpu_rare_linux_exact_candidate(index)))
      });
    }
    let selected = tach::bench::thread_cpu_rare_linux_native_provider();
    for prefix in [
      thread_cpu_selected_path_is("posix_thread_cpu").then_some("direct_selected_thread_cpu"),
      thread_cpu_fallback_path_is("posix_thread_cpu").then_some("direct_fallback_thread_cpu"),
    ]
    .into_iter()
    .flatten()
    {
      g.bench_function(format!("{prefix}__{selected}"), |b| {
        b.iter(|| black_box(tach::bench::thread_cpu_rare_linux_selected_native_nanos()))
      });
    }
  }
  g.finish();
  assert_eq!(tach_id, thread_cpu_bench_id(), "thread-CPU provider changed during now() bench");
}

fn bench_thread_cpu_elapsed(c: &mut Criterion) {
  write_criterion_runtime_attestation();
  let tach_id = thread_cpu_bench_id();
  let mut g = c.benchmark_group("ThreadCpuInstant::now() + elapsed()");
  g.bench_function(tach_id, |b| {
    b.iter(|| {
      let start = ThreadCpuInstant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function(NATIVE_THREAD_CPU_BENCH_ID, |b| {
    b.iter(|| {
      let start = native_thread_cpu_now();
      black_box(Duration::from_nanos(native_thread_cpu_now().saturating_sub(start)))
    });
  });
  #[cfg(all(feature = "bench-internal", target_os = "macos"))]
  {
    assert_eq!(
      ThreadCpuInstant::provider(),
      ThreadCpuProvider::PosixThreadCpuClock,
      "macOS ThreadCpuInstant must use its native current-thread CPU clock",
    );
    assert_eq!(
      ThreadCpuInstant::read_cost_hint(),
      ThreadCpuReadCost::SystemCall,
      "macOS ThreadCpuInstant must retain its system-call cost classification",
    );
    for prefix in ["direct_thread_cpu", "direct_selected_thread_cpu"] {
      g.bench_function(format!("{prefix}__{MACOS_THREAD_CPU_MECHANISM}"), |b| {
        b.iter(|| {
          let start = native_thread_cpu_now();
          black_box(Duration::from_nanos(native_thread_cpu_now().saturating_sub(start)))
        });
      });
    }
  }
  #[cfg(all(feature = "bench-internal", target_os = "windows"))]
  {
    for prefix in ["direct_thread_cpu", "direct_selected_thread_cpu"] {
      g.bench_function(format!("{prefix}__{WINDOWS_THREAD_CPU_MECHANISM}"), |b| {
        b.iter(|| {
          let start = native_thread_cpu_now();
          black_box(Duration::from_nanos(native_thread_cpu_now().saturating_sub(start)))
        });
      });
    }
    let _ = tach::bench::windows_thread_cpu_wall_fallback_now();
    g.bench_function(
      format!("direct_fallback_thread_cpu__{WINDOWS_THREAD_CPU_WALL_FALLBACK_MECHANISM}"),
      |b| {
        b.iter(|| {
          let start = tach::bench::windows_thread_cpu_wall_fallback_now();
          black_box(tach::bench::windows_thread_cpu_wall_fallback_now().duration_since(start))
        });
      },
    );
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
  {
    if let Some(perf) = ThreadCpuPerfHandle::try_for_current_thread() {
      for candidate in 0..perf.candidate_count() {
        let Some(name) = perf.candidate_name(candidate) else {
          continue;
        };
        if !perf.select_candidate(candidate) {
          continue;
        }
        g.bench_function(format!("direct_thread_cpu__linux_perf_mmap__{name}"), |b| {
          b.iter(|| {
            let start = perf.now_nanos();
            black_box(Duration::from_nanos(perf.now_nanos().saturating_sub(start)))
          });
        });
      }
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
  {
    if let Some(perf) = ThreadCpuPerfReadHandle::try_for_current_thread() {
      for candidate in 0..perf.candidate_count() {
        let Some(name) = perf.candidate_name(candidate) else {
          continue;
        };
        if !perf.select_candidate(candidate) {
          continue;
        }
        g.bench_function(format!("direct_thread_cpu__linux_perf_read__{name}"), |b| {
          b.iter(|| {
            let start = perf.direct_nanos();
            black_box(Duration::from_nanos(perf.direct_nanos().saturating_sub(start)))
          });
        });
      }
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
  {
    if let (Some(evidence), Some(paths)) = (
      tach::bench::thread_cpu_perf_path_evidence(),
      ThreadCpuPerfPathHandle::try_for_current_thread(),
    ) {
      let selected_mechanism = thread_cpu_path_mechanism(evidence.selected_path)
        .expect("selected measured thread-CPU path must identify its exact mechanism");
      let fallback_mechanism = thread_cpu_path_mechanism(evidence.fallback_path)
        .expect("fallback measured thread-CPU path must identify its exact mechanism");
      for (prefix, path, mechanism) in [
        ("direct_selected_thread_cpu", evidence.selected_path, selected_mechanism),
        ("direct_fallback_thread_cpu", evidence.fallback_path, fallback_mechanism),
      ] {
        let mut registered = false;
        for candidate in 0..paths.candidate_count() {
          if paths.candidate_name(candidate) == Some(path)
            && paths.candidate_available(candidate)
            && paths.select_candidate(candidate)
          {
            g.bench_function(format!("{prefix}__{mechanism}"), |b| {
              b.iter(|| {
                let start = paths.now_nanos();
                black_box(Duration::from_nanos(paths.now_nanos().saturating_sub(start)))
              });
            });
            registered = true;
            break;
          }
        }
        assert!(registered, "measured {path} path lacks its exact public-dispatch row");
      }
    }
  }
  #[cfg(any(
    all(
      any(target_arch = "x86_64", target_arch = "aarch64"),
      any(target_os = "linux", target_os = "android"),
    ),
    all(target_arch = "x86_64", target_os = "freebsd"),
  ))]
  {
    let evidence = tach::bench::thread_cpu_native64_selection_measurements();
    if evidence.libc_available {
      g.bench_function(format!("direct_thread_cpu__{}", evidence.libc_provider), |b| {
        b.iter(|| {
          let start = tach::bench::thread_cpu_native64_exact_libc_nanos();
          black_box(Duration::from_nanos(
            tach::bench::thread_cpu_native64_exact_libc_nanos().saturating_sub(start),
          ))
        })
      });
    }
    if evidence.raw_available {
      g.bench_function(format!("direct_thread_cpu__{}", evidence.raw_provider), |b| {
        b.iter(|| {
          let start = tach::bench::thread_cpu_native64_exact_raw_nanos();
          black_box(Duration::from_nanos(
            tach::bench::thread_cpu_native64_exact_raw_nanos().saturating_sub(start),
          ))
        })
      });
    }
    for prefix in [
      thread_cpu_selected_path_is("posix_thread_cpu").then_some("direct_selected_thread_cpu"),
      thread_cpu_fallback_path_is("posix_thread_cpu").then_some("direct_fallback_thread_cpu"),
    ]
    .into_iter()
    .flatten()
    {
      if evidence.selected_provider == evidence.libc_provider {
        g.bench_function(format!("{prefix}__{}", evidence.selected_provider), |b| {
          b.iter(|| {
            let start = tach::bench::thread_cpu_native64_exact_libc_nanos();
            black_box(Duration::from_nanos(
              tach::bench::thread_cpu_native64_exact_libc_nanos().saturating_sub(start),
            ))
          })
        });
      } else {
        g.bench_function(format!("{prefix}__{}", evidence.selected_provider), |b| {
          b.iter(|| {
            let start = tach::bench::thread_cpu_native64_exact_raw_nanos();
            black_box(Duration::from_nanos(
              tach::bench::thread_cpu_native64_exact_raw_nanos().saturating_sub(start),
            ))
          })
        });
      }
    }
  }
  #[cfg(all(target_arch = "x86", target_os = "linux"))]
  {
    let evidence = tach::bench::thread_cpu_i686_native_selection_evidence();
    for name in evidence.candidate_names {
      let index = match name {
        "libc_clock_gettime" => 0,
        "linux_i686_time32_syscall" => 1,
        "linux_i686_time64_syscall" => 2,
        _ => continue,
      };
      g.bench_function(format!("direct_thread_cpu__{name}"), |b| {
        b.iter(|| {
          let start = tach::bench::thread_cpu_i686_exact_candidate(index);
          black_box(Duration::from_nanos(
            tach::bench::thread_cpu_i686_exact_candidate(index).saturating_sub(start),
          ))
        })
      });
    }
    let selected = tach::bench::thread_cpu_i686_native_provider();
    for prefix in [
      thread_cpu_selected_path_is("posix_thread_cpu").then_some("direct_selected_thread_cpu"),
      thread_cpu_fallback_path_is("posix_thread_cpu").then_some("direct_fallback_thread_cpu"),
    ]
    .into_iter()
    .flatten()
    {
      g.bench_function(format!("{prefix}__{selected}"), |b| {
        b.iter(|| {
          let start = tach::bench::thread_cpu_i686_selected_native_nanos();
          black_box(Duration::from_nanos(
            tach::bench::thread_cpu_i686_selected_native_nanos().saturating_sub(start),
          ))
        })
      });
    }
  }
  #[cfg(all(target_arch = "arm", target_os = "linux"))]
  {
    let evidence = tach::bench::thread_cpu_arm_native_selection_evidence();
    for name in evidence.candidate_names {
      let index = match name {
        "libc_clock_gettime" => 0,
        "linux_arm_time32_syscall" => 1,
        "linux_arm_time64_syscall" => 2,
        _ => continue,
      };
      g.bench_function(format!("direct_thread_cpu__{name}"), |b| {
        b.iter(|| {
          let start = tach::bench::thread_cpu_arm_exact_candidate(index);
          black_box(Duration::from_nanos(
            tach::bench::thread_cpu_arm_exact_candidate(index).saturating_sub(start),
          ))
        })
      });
    }
    let selected = tach::bench::thread_cpu_arm_native_provider();
    for prefix in [
      thread_cpu_selected_path_is("posix_thread_cpu").then_some("direct_selected_thread_cpu"),
      thread_cpu_fallback_path_is("posix_thread_cpu").then_some("direct_fallback_thread_cpu"),
    ]
    .into_iter()
    .flatten()
    {
      g.bench_function(format!("{prefix}__{selected}"), |b| {
        b.iter(|| {
          let start = tach::bench::thread_cpu_arm_selected_native_nanos();
          black_box(Duration::from_nanos(
            tach::bench::thread_cpu_arm_selected_native_nanos().saturating_sub(start),
          ))
        })
      });
    }
  }
  #[cfg(all(target_arch = "riscv64", target_os = "linux"))]
  {
    let evidence = tach::bench::thread_cpu_riscv64_native_selection_evidence();
    for name in evidence.candidate_names {
      let index = match name {
        "libc_clock_gettime" => 0,
        "linux_riscv64_syscall" => 1,
        _ => continue,
      };
      g.bench_function(format!("direct_thread_cpu__{name}"), |b| {
        b.iter(|| {
          let start = tach::bench::thread_cpu_riscv64_exact_candidate(index);
          black_box(Duration::from_nanos(
            tach::bench::thread_cpu_riscv64_exact_candidate(index).saturating_sub(start),
          ))
        })
      });
    }
    let selected = tach::bench::thread_cpu_riscv64_native_provider();
    for prefix in [
      thread_cpu_selected_path_is("posix_thread_cpu").then_some("direct_selected_thread_cpu"),
      thread_cpu_fallback_path_is("posix_thread_cpu").then_some("direct_fallback_thread_cpu"),
    ]
    .into_iter()
    .flatten()
    {
      g.bench_function(format!("{prefix}__{selected}"), |b| {
        b.iter(|| {
          let start = tach::bench::thread_cpu_riscv64_selected_native_nanos();
          black_box(Duration::from_nanos(
            tach::bench::thread_cpu_riscv64_selected_native_nanos().saturating_sub(start),
          ))
        })
      });
    }
  }
  #[cfg(all(
    target_os = "linux",
    any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
  ))]
  {
    let evidence = tach::bench::thread_cpu_rare_linux_native_selection_evidence();
    for name in evidence.candidate_names {
      let index = if name == "libc_clock_gettime" {
        0
      } else if name.contains("raw_scv") {
        2
      } else {
        1
      };
      g.bench_function(format!("direct_thread_cpu__{name}"), |b| {
        b.iter(|| {
          let start = tach::bench::thread_cpu_rare_linux_exact_candidate(index);
          black_box(Duration::from_nanos(
            tach::bench::thread_cpu_rare_linux_exact_candidate(index).saturating_sub(start),
          ))
        })
      });
    }
    let selected = tach::bench::thread_cpu_rare_linux_native_provider();
    for prefix in [
      thread_cpu_selected_path_is("posix_thread_cpu").then_some("direct_selected_thread_cpu"),
      thread_cpu_fallback_path_is("posix_thread_cpu").then_some("direct_fallback_thread_cpu"),
    ]
    .into_iter()
    .flatten()
    {
      g.bench_function(format!("{prefix}__{selected}"), |b| {
        b.iter(|| {
          let start = tach::bench::thread_cpu_rare_linux_selected_native_nanos();
          black_box(Duration::from_nanos(
            tach::bench::thread_cpu_rare_linux_selected_native_nanos().saturating_sub(start),
          ))
        })
      });
    }
  }
  g.finish();
  assert_eq!(tach_id, thread_cpu_bench_id(), "thread-CPU provider changed during elapsed() bench");
  write_thread_cpu_selection();
  write_thread_cpu_perf_selection();
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
fn write_thread_cpu_perf_selection() {
  use std::fs;
  use std::path::PathBuf;

  let capability_preferred = cfg!(all(target_os = "linux", target_arch = "aarch64"));
  let path_evidence = tach::bench::thread_cpu_perf_path_evidence();
  let mmap_available = path_evidence.as_ref().is_some_and(|probe| probe.mmap_batches_ns.is_some());
  #[allow(unused_mut)]
  let mut mmap_candidates = Vec::new();
  #[allow(unused_mut)]
  let mut mmap_mechanism = None;
  #[cfg(any(
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
  ))]
  if mmap_available {
    if let Some(perf) = ThreadCpuPerfHandle::try_for_current_thread() {
      let selected = perf.selected_candidate_name();
      mmap_mechanism = Some(format!("linux_perf_mmap__{selected}"));
      for index in 0..perf.candidate_count() {
        let Some(name) = perf.candidate_name(index) else {
          continue;
        };
        if perf.select_candidate(index) {
          mmap_candidates.push(format!("direct_thread_cpu__linux_perf_mmap__{name}"));
        }
      }
    }
  }

  #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
  let counter_probe = tach::bench::thread_cpu_perf_counter_evidence()
    .filter(|_| mmap_available)
    .map(|probe| {
      let count = probe.candidate_count;
      serde_json::json!({
        "selection_kind": "tournament",
        "candidate_names": &probe.candidate_names[..count],
        "candidate_eligible": &probe.candidate_eligible[..count],
        "candidate_batches_ns": &probe.candidate_batches_ns[..count],
        "selected_candidate": probe.selected_candidate,
        "reads_per_batch": probe.reads_per_batch,
        "required_decisive_wins": probe.required_decisive_wins,
        "equivalence_band": {
          "floor_ns_per_read": 1,
          "relative_denominator": 20,
        },
      })
    });
  #[cfg(target_arch = "arm")]
  let counter_probe = mmap_available.then(|| {
    serde_json::json!({
      "selection_kind": "fixed_candidate",
      "candidate_names": ["arm_isb_mrrc_cntvct_isb"],
      "candidate_eligible": [true],
      "candidate_batches_ns": null,
      "selected_candidate": "arm_isb_mrrc_cntvct_isb",
      "eligibility_gate": "perf cap_user_time observed under the mmap seqlock before the first MRRC; AArch32 exposes no FEAT_ECV HWCAP, so CNTVCTSS is fail-closed",
    })
  });
  #[cfg(target_arch = "riscv64")]
  let counter_probe = mmap_available.then(|| {
    serde_json::json!({
      "selection_kind": "fixed_candidate",
      "candidate_names": ["riscv_fence_rdtime_fence"],
      "candidate_eligible": [true],
      "candidate_batches_ns": null,
      "selected_candidate": "riscv_fence_rdtime_fence",
    })
  });
  #[cfg(any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"))]
  let counter_probe: Option<serde_json::Value> = None;

  let read_entry_evidence = path_evidence
    .as_ref()
    .and_then(|_| tach::bench::thread_cpu_perf_read_entry_evidence());
  let mut read_candidates = Vec::new();
  let mut read_mechanism = None;
  let read_entry_probe = read_entry_evidence.as_ref().map(|probe| {
    let count = probe.candidate_count;
    read_mechanism = Some(format!("linux_perf_read__{}", probe.selected_candidate));
    for index in 0..count {
      if probe.candidate_eligible[index] {
        read_candidates
          .push(format!("direct_thread_cpu__linux_perf_read__{}", probe.candidate_names[index],));
      }
    }
    let measured_count = probe.candidate_measured[..count].iter().filter(|value| **value).count();
    serde_json::json!({
      "selection_kind": if measured_count > 1 { "tournament" } else { "fixed_candidate" },
      "candidate_names": &probe.candidate_names[..count],
      "candidate_eligible": &probe.candidate_eligible[..count],
      "candidate_measured": &probe.candidate_measured[..count],
      "candidate_batches_ns": if measured_count > 1 {
        serde_json::to_value(&probe.candidate_batches_ns[..count])
          .expect("serialize perf-read candidate batches")
      } else {
        serde_json::Value::Null
      },
      "selected_candidate": probe.selected_candidate,
      "reads_per_batch": probe.reads_per_batch,
      "required_decisive_wins": probe.required_decisive_wins,
      "equivalence_band": {
        "floor_ns_per_read": 1,
        "relative_denominator": 20,
      },
    })
  });

  #[cfg(any(
    all(target_arch = "x86_64", any(target_os = "linux", target_os = "android")),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ))]
  let native_evidence = tach::bench::thread_cpu_native64_selection_measurements();
  #[cfg(any(
    all(target_arch = "x86_64", any(target_os = "linux", target_os = "android")),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ))]
  let native_entry_probe =
    serde_json::to_value(&native_evidence).expect("serialize native thread-CPU selector");
  #[cfg(not(any(
    all(target_arch = "x86_64", any(target_os = "linux", target_os = "android")),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
    all(target_arch = "x86", target_os = "linux"),
    all(target_arch = "arm", target_os = "linux"),
    all(target_arch = "riscv64", target_os = "linux"),
    all(
      target_os = "linux",
      any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
    ),
  )))]
  let native_entry_probe = serde_json::Value::Null;
  #[cfg(all(target_arch = "x86", target_os = "linux"))]
  let native_evidence = tach::bench::thread_cpu_i686_native_selection_evidence();
  #[cfg(all(target_arch = "x86", target_os = "linux"))]
  let native_entry_probe =
    serde_json::to_value(&native_evidence).expect("serialize i686 native thread-CPU selector");
  #[cfg(all(target_arch = "arm", target_os = "linux"))]
  let native_evidence = tach::bench::thread_cpu_arm_native_selection_evidence();
  #[cfg(all(target_arch = "arm", target_os = "linux"))]
  let native_entry_probe =
    serde_json::to_value(&native_evidence).expect("serialize Arm native thread-CPU selector");
  #[cfg(all(target_arch = "riscv64", target_os = "linux"))]
  let native_evidence = tach::bench::thread_cpu_riscv64_native_selection_evidence();
  #[cfg(all(target_arch = "riscv64", target_os = "linux"))]
  let native_entry_probe =
    serde_json::to_value(&native_evidence).expect("serialize RISC-V native thread-CPU selector");
  #[cfg(all(
    target_os = "linux",
    any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
  ))]
  let native_evidence = tach::bench::thread_cpu_rare_linux_native_selection_evidence();
  #[cfg(all(
    target_os = "linux",
    any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
  ))]
  let native_entry_probe = serde_json::to_value(&native_evidence)
    .expect("serialize rare Linux native thread-CPU selector");

  #[cfg(any(
    all(target_arch = "x86_64", any(target_os = "linux", target_os = "android")),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ))]
  let (native_provider, mut eligible_direct_candidates) = {
    let mut candidates = Vec::with_capacity(2 + mmap_candidates.len() + read_candidates.len());
    if native_evidence.libc_available {
      candidates.push(format!("direct_thread_cpu__{}", native_evidence.libc_provider));
    }
    if native_evidence.raw_available {
      candidates.push(format!("direct_thread_cpu__{}", native_evidence.raw_provider));
    }
    (native_evidence.selected_provider, candidates)
  };
  #[cfg(all(target_arch = "x86", target_os = "linux"))]
  let (native_provider, mut eligible_direct_candidates) = {
    let provider = tach::bench::thread_cpu_i686_native_provider();
    let candidates: Vec<String> = native_evidence
      .candidate_names
      .iter()
      .map(|name| format!("direct_thread_cpu__{name}"))
      .collect();
    (provider, candidates)
  };
  #[cfg(all(target_arch = "arm", target_os = "linux"))]
  let (native_provider, mut eligible_direct_candidates) = {
    let provider = tach::bench::thread_cpu_arm_native_provider();
    let candidates: Vec<String> = native_evidence
      .candidate_names
      .iter()
      .map(|name| format!("direct_thread_cpu__{name}"))
      .collect();
    (provider, candidates)
  };
  #[cfg(all(target_arch = "riscv64", target_os = "linux"))]
  let (native_provider, mut eligible_direct_candidates) = {
    let provider = tach::bench::thread_cpu_riscv64_native_provider();
    let candidates: Vec<String> = native_evidence
      .candidate_names
      .iter()
      .map(|name| format!("direct_thread_cpu__{name}"))
      .collect();
    (provider, candidates)
  };
  #[cfg(all(
    target_os = "linux",
    any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
  ))]
  let (native_provider, mut eligible_direct_candidates) = {
    let provider = tach::bench::thread_cpu_rare_linux_native_provider();
    let candidates: Vec<String> = native_evidence
      .candidate_names
      .iter()
      .map(|name| format!("direct_thread_cpu__{name}"))
      .collect();
    (provider, candidates)
  };
  eligible_direct_candidates.extend(mmap_candidates.iter().cloned());
  eligible_direct_candidates.extend(read_candidates.iter().cloned());

  let mechanism_for_path = |path: &str| match path {
    "linux_perf_mmap" => mmap_mechanism
      .clone()
      .expect("available perf-mmap path must identify its selected counter"),
    "linux_perf_read" => read_mechanism
      .clone()
      .expect("available perf-read path must identify its selected read ABI"),
    "posix_thread_cpu" => native_provider.to_owned(),
    _ => panic!("unknown measured thread-CPU path: {path}"),
  };
  let provider_for_path = |path: &str| match path {
    "linux_perf_mmap" => "linux_perf_mmap",
    "linux_perf_read" => "linux_perf_read",
    "posix_thread_cpu" => "posix_thread_cpu_clock",
    _ => panic!("unknown measured thread-CPU path: {path}"),
  };
  let cost_for_path = |path: &str| match path {
    "linux_perf_mmap" => "inline",
    "linux_perf_read" | "posix_thread_cpu" => "system call",
    _ => panic!("unknown measured thread-CPU path: {path}"),
  };

  let selected_path =
    path_evidence.as_ref().map_or("posix_thread_cpu", |probe| probe.selected_path);
  let selected_provider = provider_for_path(selected_path);
  let selected_mechanism = mechanism_for_path(selected_path);
  let selected_read_cost = cost_for_path(selected_path);
  let selected_native_benchmark = format!("direct_selected_thread_cpu__{selected_mechanism}");
  let expected_public_provider = match selected_path {
    "linux_perf_mmap" => ThreadCpuProvider::LinuxPerfMmap,
    "linux_perf_read" => ThreadCpuProvider::LinuxPerfRead,
    "posix_thread_cpu" => ThreadCpuProvider::PosixThreadCpuClock,
    _ => unreachable!(),
  };
  assert_eq!(
    ThreadCpuInstant::provider(),
    expected_public_provider,
    "measured thread-CPU selector and public provider disagree",
  );
  let introspected_read_cost = match ThreadCpuInstant::read_cost_hint() {
    ThreadCpuReadCost::Inline => "inline",
    ThreadCpuReadCost::SystemCall => "system call",
    ThreadCpuReadCost::HostCall => "host call",
    ThreadCpuReadCost::Unavailable => "unavailable",
    _ => "unknown",
  };
  assert_eq!(
    introspected_read_cost, selected_read_cost,
    "measured thread-CPU selector and public read-cost hint disagree",
  );

  let fallback_path = path_evidence.as_ref().and_then(|probe| {
    if capability_preferred {
      (probe.selected_path == "linux_perf_mmap").then_some("posix_thread_cpu")
    } else {
      Some(probe.fallback_path)
    }
  });
  let fallback_provider = fallback_path.map(provider_for_path);
  let fallback_mechanism = fallback_path.map(mechanism_for_path);
  let fallback_read_cost = fallback_path.map(cost_for_path);
  let fallback_native_benchmark = fallback_mechanism
    .as_ref()
    .map(|mechanism| format!("direct_fallback_thread_cpu__{mechanism}"));
  let path_probe = path_evidence.as_ref().map(|evidence| {
    let common = serde_json::json!({
      "candidate_names": ["posix_thread_cpu", "linux_perf_mmap", "linux_perf_read"],
      "candidate_eligible": [true, evidence.mmap_batches_ns.is_some(), true],
      "candidate_batches_ns": [
        evidence.posix_batches_ns,
        evidence.mmap_batches_ns.unwrap_or([0; 9]),
        evidence.read_batches_ns,
      ],
      "selected_candidate": evidence.selected_path,
      "reads_per_batch": evidence.reads_per_batch,
      "required_decisive_wins": evidence.required_decisive_wins,
      "equivalence_band": {
        "floor_ns_per_read": 1,
        "relative_denominator": 20,
      },
    });
    if capability_preferred {
      let mut probe = common;
      let object = probe.as_object_mut().expect("path probe object");
      object.insert(
        "selection_kind".to_owned(),
        serde_json::json!("capability_preferred_with_performance_audit"),
      );
      object.insert("preferred_candidate".to_owned(), serde_json::json!("linux_perf_mmap"));
      object.insert(
        "failure_fallback_candidate".to_owned(),
        serde_json::json!("posix_thread_cpu"),
      );
      object.insert(
        "selection_basis".to_owned(),
        serde_json::json!("complete perf mmap metadata and architectural-counter capability; audit samples do not select the provider"),
      );
      probe
    } else {
      let mut probe = common;
      let object = probe.as_object_mut().expect("path probe object");
      object.insert(
        "selection_kind".to_owned(),
        serde_json::json!("tournament_with_measured_runner_up"),
      );
      object.insert("fallback_candidate".to_owned(), serde_json::json!(evidence.fallback_path));
      object.insert(
        "capability_was_not_profitable".to_owned(),
        serde_json::json!(evidence.mmap_batches_ns.is_some()
          && evidence.selected_path != "linux_perf_mmap"),
      );
      probe
    }
  });
  let selection_kind = if capability_preferred {
    "capability_preferred_with_failure_fallback"
  } else {
    "tournament_with_measured_runner_up"
  };
  let failure_fallback = capability_preferred.then(|| {
    serde_json::json!({
      "preferred_provider": "linux_perf_mmap",
      "eligibility_gate": "perf task-clock mmap exposes complete seqlock conversion metadata and a usable architectural counter",
      "fallback_provider": "posix_thread_cpu_clock",
      "fallback_mechanism": native_provider,
      "trigger": "perf event or mmap capability is unavailable, or an inline mmap read fails",
    })
  });
  let payload = serde_json::json!({
    "selection_kind": selection_kind,
    "selected_provider": selected_provider,
    "selected_mechanism": selected_mechanism,
    "selected_read_cost": selected_read_cost,
    "selected_native_benchmark": selected_native_benchmark,
    "fallback_provider": fallback_provider,
    "fallback_mechanism": fallback_mechanism,
    "fallback_read_cost": fallback_read_cost,
    "fallback_native_benchmark": fallback_native_benchmark,
    "eligible_direct_candidates": eligible_direct_candidates,
    "failure_fallback": failure_fallback,
    "native_entry_probe": native_entry_probe,
    "perf": {
      "event_available": path_evidence.is_some(),
      "path_probe": path_probe,
      "mmap": {
        "supported_on_target": cfg!(any(
          target_arch = "x86",
          target_arch = "x86_64",
          target_arch = "aarch64",
          target_arch = "arm",
          target_arch = "riscv64",
        )),
        "available": mmap_available,
        "read_cost": "inline",
        "selected_mechanism": mmap_mechanism,
        "selected_candidate_benchmark": mmap_mechanism
          .as_ref()
          .map(|mechanism| format!("direct_thread_cpu__{mechanism}")),
        "eligible_benchmarks": mmap_candidates,
        "counter_probe": counter_probe,
      },
      "read": {
        "supported_on_target": true,
        "available": read_entry_evidence.is_some(),
        "read_cost": "system call",
        "selected_mechanism": read_mechanism,
        "selected_candidate_benchmark": read_mechanism
          .as_ref()
          .map(|mechanism| format!("direct_thread_cpu__{mechanism}")),
        "eligible_benchmarks": read_candidates,
        "entry_probe": read_entry_probe,
      },
      "decision_rule": if capability_preferred {
        "prefer perf task-clock mmap when its complete inline capability is available; otherwise use the selected native CLOCK_THREAD_CPUTIME_ID entry; paired path samples audit but do not choose this policy"
      } else {
        "POSIX, eligible perf-mmap, and persistent perf-read complete public-dispatch paths compete in order; a challenger replaces the incumbent only by > max(1 ns/read, 5%) in >=8/9 paired batches, and the same tournament excluding the winner selects the measured fallback"
      },
      "measurement_clock": "raw SYS_clock_gettime(CLOCK_MONOTONIC_RAW), never libc/vDSO or the candidate under test",
    },
    "read_cost_basis": "perf mmap is Inline because tach issues no OS or host call on the hot path; persistent perf read and every CLOCK_THREAD_CPUTIME_ID entry route are SystemCall",
  });
  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create criterion directory");
  fs::write(
    directory.join("thread-cpu-selection.json"),
    serde_json::to_vec_pretty(&payload).expect("serialize perf selector evidence"),
  )
  .expect("write perf selector evidence");
}

#[cfg(not(all(
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
)))]
fn write_thread_cpu_perf_selection() {}

#[cfg(all(
  feature = "bench-internal",
  not(feature = "thread-cpu-inline"),
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
    all(target_os = "android", any(target_arch = "x86_64", target_arch = "aarch64")),
  ),
))]
fn write_thread_cpu_selection() {
  use std::fs;
  use std::path::PathBuf;

  assert_eq!(
    ThreadCpuInstant::provider(),
    ThreadCpuProvider::PosixThreadCpuClock,
    "no-default ThreadCpuInstant must retain its native thread-CPU provider",
  );
  assert_eq!(
    ThreadCpuInstant::read_cost_hint(),
    ThreadCpuReadCost::SystemCall,
    "no-default ThreadCpuInstant must retain its system-call cost classification",
  );

  #[cfg(any(
    all(target_arch = "x86_64", any(target_os = "linux", target_os = "android")),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ))]
  let (selected_mechanism, native_entry_probe, eligible_direct_candidates) = {
    let evidence = tach::bench::thread_cpu_native64_selection_measurements();
    let mut candidates = Vec::with_capacity(2);
    if evidence.libc_available {
      candidates.push(format!("direct_thread_cpu__{}", evidence.libc_provider));
    }
    if evidence.raw_available {
      candidates.push(format!("direct_thread_cpu__{}", evidence.raw_provider));
    }
    (
      evidence.selected_provider,
      serde_json::to_value(&evidence).expect("serialize native thread-CPU selector"),
      candidates,
    )
  };
  #[cfg(all(target_arch = "x86", target_os = "linux"))]
  let (selected_mechanism, native_entry_probe, eligible_direct_candidates) = {
    let evidence = tach::bench::thread_cpu_i686_native_selection_evidence();
    let candidates: Vec<String> = evidence
      .candidate_names
      .iter()
      .map(|name| format!("direct_thread_cpu__{name}"))
      .collect();
    (
      tach::bench::thread_cpu_i686_native_provider(),
      serde_json::to_value(&evidence).expect("serialize i686 native thread-CPU selector"),
      candidates,
    )
  };
  #[cfg(all(target_arch = "arm", target_os = "linux"))]
  let (selected_mechanism, native_entry_probe, eligible_direct_candidates) = {
    let evidence = tach::bench::thread_cpu_arm_native_selection_evidence();
    let candidates: Vec<String> = evidence
      .candidate_names
      .iter()
      .map(|name| format!("direct_thread_cpu__{name}"))
      .collect();
    (
      tach::bench::thread_cpu_arm_native_provider(),
      serde_json::to_value(&evidence).expect("serialize Arm native thread-CPU selector"),
      candidates,
    )
  };
  #[cfg(all(target_arch = "riscv64", target_os = "linux"))]
  let (selected_mechanism, native_entry_probe, eligible_direct_candidates) = {
    let evidence = tach::bench::thread_cpu_riscv64_native_selection_evidence();
    let candidates: Vec<String> = evidence
      .candidate_names
      .iter()
      .map(|name| format!("direct_thread_cpu__{name}"))
      .collect();
    (
      tach::bench::thread_cpu_riscv64_native_provider(),
      serde_json::to_value(&evidence).expect("serialize RISC-V native thread-CPU selector"),
      candidates,
    )
  };
  #[cfg(all(
    target_os = "linux",
    any(target_arch = "s390x", target_arch = "loongarch64", target_arch = "powerpc64"),
  ))]
  let (selected_mechanism, native_entry_probe, eligible_direct_candidates) = {
    let evidence = tach::bench::thread_cpu_rare_linux_native_selection_evidence();
    let candidates: Vec<String> = evidence
      .candidate_names
      .iter()
      .map(|name| format!("direct_thread_cpu__{name}"))
      .collect();
    (
      tach::bench::thread_cpu_rare_linux_native_provider(),
      serde_json::to_value(&evidence).expect("serialize rare Linux native thread-CPU selector"),
      candidates,
    )
  };

  let selected_benchmark = format!("direct_selected_thread_cpu__{selected_mechanism}");
  let payload = serde_json::json!({
    "selection_kind": "tournament_with_measured_runner_up",
    "selected_provider": "posix_thread_cpu_clock",
    "selected_mechanism": selected_mechanism,
    "selected_read_cost": "system call",
    "selected_native_benchmark": selected_benchmark,
    "fallback_provider": null,
    "fallback_mechanism": null,
    "fallback_read_cost": null,
    "fallback_native_benchmark": null,
    "eligible_direct_candidates": eligible_direct_candidates,
    "native_entry_probe": native_entry_probe,
    "perf": {
      "event_available": false,
      "path_probe": null,
      "mmap": {
        "supported_on_target": cfg!(any(
          target_arch = "x86",
          target_arch = "x86_64",
          target_arch = "aarch64",
          target_arch = "arm",
          target_arch = "riscv64",
        )),
        "available": false,
        "read_cost": "inline",
        "selected_mechanism": null,
        "selected_candidate_benchmark": null,
        "eligible_benchmarks": [],
        "counter_probe": null,
      },
      "read": {
        "supported_on_target": true,
        "available": false,
        "read_cost": "system call",
        "selected_mechanism": null,
        "selected_candidate_benchmark": null,
        "eligible_benchmarks": [],
        "entry_probe": null,
      },
      "decision_rule": "thread-cpu-inline is disabled, so eligible native CLOCK_THREAD_CPUTIME_ID entries compete while perf providers are excluded by build configuration",
      "measurement_clock": "raw SYS_clock_gettime(CLOCK_MONOTONIC_RAW), never libc/vDSO or the candidate under test",
    },
    "read_cost_basis": "every eligible CLOCK_THREAD_CPUTIME_ID entry is a SystemCall tier; disabling thread-cpu-inline excludes perf without disabling the native-entry tournament",
  });
  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create criterion directory");
  fs::write(
    directory.join("thread-cpu-selection.json"),
    serde_json::to_vec_pretty(&payload).expect("serialize native thread-CPU selector"),
  )
  .expect("write native thread-CPU selector evidence");
}

#[cfg(all(
  feature = "bench-internal",
  any(
    all(
      feature = "thread-cpu-inline",
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android"),
    ),
    all(target_arch = "x86_64", target_os = "freebsd"),
    all(
      feature = "thread-cpu-inline",
      target_arch = "aarch64",
      any(target_os = "linux", target_os = "android"),
    ),
  ),
))]
fn write_thread_cpu_selection() {
  use std::fs;
  use std::path::PathBuf;

  let evidence = tach::bench::thread_cpu_native64_selection_measurements();
  let mut candidates = Vec::with_capacity(2);
  if evidence.libc_available {
    candidates.push(format!("direct_thread_cpu__{}", evidence.libc_provider));
  }
  if evidence.raw_available {
    candidates.push(format!("direct_thread_cpu__{}", evidence.raw_provider));
  }
  let payload = serde_json::json!({
    "selection_kind": "tournament_with_measured_runner_up",
    "selected_provider": "posix_thread_cpu_clock",
    "selected_mechanism": evidence.selected_provider,
    "selected_read_cost": "system call",
    "selected_native_benchmark": format!(
      "direct_selected_thread_cpu__{}",
      evidence.selected_provider,
    ),
    "fallback_provider": null,
    "fallback_mechanism": null,
    "fallback_read_cost": null,
    "fallback_native_benchmark": null,
    "eligible_direct_candidates": candidates,
    "native_entry_probe": evidence,
    "perf": {
      "event_available": false,
      "path_probe": null,
      "mmap": {
        "supported_on_target": false,
        "available": false,
        "read_cost": "inline",
        "selected_mechanism": null,
        "selected_candidate_benchmark": null,
        "eligible_benchmarks": [],
        "counter_probe": null,
      },
      "read": {
        "supported_on_target": false,
        "available": false,
        "read_cost": "system call",
        "selected_mechanism": null,
        "selected_candidate_benchmark": null,
        "eligible_benchmarks": [],
        "entry_probe": null,
      },
      "decision_rule": "no perf provider exists on this target; the measured native CLOCK_THREAD_CPUTIME_ID entry winner is the complete public path",
      "measurement_clock": "raw SYS_clock_gettime(CLOCK_MONOTONIC_RAW), never libc/vDSO or the candidate under test",
    },
    "read_cost_basis": "CLOCK_THREAD_CPUTIME_ID remains a kernel-entry SystemCall tier through either the libc wrapper or raw ABI; relative wrapper speed does not change mechanism class",
  });
  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create criterion directory");
  fs::write(
    directory.join("thread-cpu-selection.json"),
    serde_json::to_vec_pretty(&payload).expect("serialize thread-CPU selector evidence"),
  )
  .expect("write thread-CPU selector evidence");
}

#[cfg(all(feature = "bench-internal", target_os = "macos"))]
fn write_thread_cpu_selection() {
  use std::fs;
  use std::path::PathBuf;

  assert_eq!(
    ThreadCpuInstant::provider(),
    ThreadCpuProvider::PosixThreadCpuClock,
    "macOS ThreadCpuInstant must retain its fixed native provider",
  );
  assert_eq!(
    ThreadCpuInstant::read_cost_hint(),
    ThreadCpuReadCost::SystemCall,
    "macOS ThreadCpuInstant must retain its system-call cost classification",
  );

  let direct_candidate = format!("direct_thread_cpu__{MACOS_THREAD_CPU_MECHANISM}");
  let selected_benchmark = format!("direct_selected_thread_cpu__{MACOS_THREAD_CPU_MECHANISM}");
  let public_exact_probe = serde_json::json!({
    "now": measure_wall_public_exact(
      ThreadCpuInstant::now,
      native_thread_cpu_now,
    ),
    "elapsed": measure_wall_public_exact(
      || {
        let start = ThreadCpuInstant::now();
        start.elapsed()
      },
      || {
        let start = native_thread_cpu_now();
        Duration::from_nanos(native_thread_cpu_now().saturating_sub(start))
      },
    ),
  });
  let payload = serde_json::json!({
    "selection_kind": "fixed_native",
    "selected_provider": "posix_thread_cpu_clock",
    "selected_mechanism": MACOS_THREAD_CPU_MECHANISM,
    "selected_read_cost": "system call",
    "selected_native_benchmark": selected_benchmark,
    "fallback_provider": null,
    "fallback_mechanism": null,
    "fallback_read_cost": null,
    "fallback_native_benchmark": null,
    "eligible_direct_candidates": [direct_candidate],
    "fixed_provider": {
      "candidate": MACOS_THREAD_CPU_MECHANISM,
      "supported_architectures": ["x86_64", "aarch64"],
      "native_primitive": "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)",
      "selection_basis": "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID) is macOS's direct current-thread CPU-time entry",
      "time_domain": "thread CPU",
    },
    "public_exact_probe": public_exact_probe,
    "read_cost_basis": "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID) is a native system-call tier for scheduled CPU time on the current macOS thread",
  });
  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create criterion directory");
  fs::write(
    directory.join("thread-cpu-selection.json"),
    serde_json::to_vec_pretty(&payload).expect("serialize macOS thread-CPU selector evidence"),
  )
  .expect("write macOS thread-CPU selector evidence");
}

#[cfg(all(not(feature = "bench-internal"), target_os = "macos"))]
fn write_thread_cpu_selection() {}

#[cfg(not(all(
  feature = "bench-internal",
  any(
    all(
      target_arch = "x86_64",
      any(target_os = "linux", target_os = "android", target_os = "freebsd"),
    ),
    all(target_arch = "aarch64", any(target_os = "linux", target_os = "android")),
  ),
)))]
#[cfg(not(all(
  feature = "bench-internal",
  not(feature = "thread-cpu-inline"),
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
    all(target_os = "android", any(target_arch = "x86_64", target_arch = "aarch64")),
  ),
)))]
#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
fn write_thread_cpu_selection() {}

#[cfg(all(feature = "bench-internal", target_os = "windows"))]
fn write_thread_cpu_selection() {
  use std::fs;
  use std::io::ErrorKind;
  use std::path::PathBuf;

  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create criterion directory");
  let selection_path = directory.join("thread-cpu-selection.json");
  match fs::remove_file(&selection_path) {
    Ok(()) => {}
    Err(error) if error.kind() == ErrorKind::NotFound => {}
    Err(error) => panic!("remove stale Windows thread-CPU selector evidence: {error}"),
  }

  assert_eq!(
    ThreadCpuInstant::provider(),
    ThreadCpuProvider::WindowsThreadTimes,
    "Windows thread-CPU evidence requires the documented GetThreadTimes provider",
  );
  assert_eq!(
    ThreadCpuInstant::read_cost_hint(),
    ThreadCpuReadCost::SystemCall,
    "Windows GetThreadTimes must retain its system-call cost classification",
  );

  let direct_candidate = format!("direct_thread_cpu__{WINDOWS_THREAD_CPU_MECHANISM}");
  let selected_benchmark = format!("direct_selected_thread_cpu__{WINDOWS_THREAD_CPU_MECHANISM}");
  let failure_fallback_benchmark =
    format!("direct_fallback_thread_cpu__{WINDOWS_THREAD_CPU_WALL_FALLBACK_MECHANISM}");
  let public_exact_probe = serde_json::json!({
    "now": measure_wall_public_exact(
      ThreadCpuInstant::now,
      native_thread_cpu_now,
    ),
    "elapsed": measure_wall_public_exact(
      || {
        let start = ThreadCpuInstant::now();
        start.elapsed()
      },
      || {
        let start = native_thread_cpu_now();
        Duration::from_nanos(native_thread_cpu_now().saturating_sub(start))
      },
    ),
  });
  let payload = serde_json::json!({
    "selection_kind": "fixed_windows_thread_times",
    "selected_provider": "windows_thread_times",
    "selected_mechanism": WINDOWS_THREAD_CPU_MECHANISM,
    "selected_read_cost": "system call",
    "selected_native_benchmark": selected_benchmark,
    "fallback_provider": null,
    "fallback_mechanism": null,
    "fallback_read_cost": null,
    "fallback_native_benchmark": null,
    "eligible_direct_candidates": [direct_candidate],
    "native_campaign_guard": {
      "required_provider": "windows_thread_times",
      "required_read_cost": "system call",
      "stale_selection_removed_before_guard": true,
      "on_mismatch": "panic before thread-cpu-selection.json is written",
    },
    "fixed_provider": {
      "candidate": WINDOWS_THREAD_CPU_MECHANISM,
      "supported_architectures": ["x86", "x86_64", "aarch64"],
      "selection_basis": "GetThreadTimes is Windows' documented elapsed current-thread CPU timeline",
      "authority": "https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getthreadtimes",
    },
    "failure_fallback": {
      "provider": "monotonic_wall_clock",
      "mechanism": WINDOWS_THREAD_CPU_WALL_FALLBACK_MECHANISM,
      "read_cost": "system call",
      "time_domain": "monotonic wall fallback",
      "trigger": "GetThreadTimes(current-thread pseudo-handle) returns zero",
      "state_transition": "sticky process-wide fallback",
      "eligible_for_thread_cpu_speed_claim": false,
      "exact_route_measured": true,
      "exact_benchmark": failure_fallback_benchmark,
      "observed_as_public_provider_during_campaign": false,
      "campaign_behavior": "an observed fallback aborts the native benchmark before extraction instead of emitting thread-CPU parity evidence",
    },
    "ineligible_direct_candidates": {
      "query_thread_cycle_time": {
        "eligibility": "ineligible",
        "reason": "implementation-dependent cycles cannot be converted to elapsed thread CPU time",
        "authority": "https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-querythreadcycletime",
      },
      "nt_query_information_thread": {
        "eligibility": "ineligible",
        "reason": "the documented THREADINFOCLASS contract exposes no stable ThreadTimes class",
        "authority": "https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntqueryinformationthread",
      },
    },
    "public_exact_probe": public_exact_probe,
  });
  fs::write(
    selection_path,
    serde_json::to_vec_pretty(&payload).expect("serialize Windows thread-CPU selector evidence"),
  )
  .expect("write Windows thread-CPU selector evidence");
}

#[cfg(all(not(feature = "bench-internal"), target_os = "windows"))]
fn write_thread_cpu_selection() {}

#[cfg(all(
  feature = "thread-cpu-inline",
  any(target_os = "android", target_os = "linux"),
  any(target_arch = "x86_64", target_arch = "aarch64"),
))]
const NATIVE_THREAD_CPU_BENCH_ID: &str =
  "native_thread_cpu__inline_syscall_clock_thread_cputime_id";

#[cfg(all(
  any(target_os = "android", target_os = "linux"),
  not(all(feature = "thread-cpu-inline", any(target_arch = "x86_64", target_arch = "aarch64"),)),
))]
const NATIVE_THREAD_CPU_BENCH_ID: &str =
  "native_thread_cpu__libc_clock_gettime_clock_thread_cputime_id";

#[cfg(any(target_os = "android", target_os = "linux"))]
#[inline(always)]
fn native_thread_cpu_now() -> u64 {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  #[cfg(all(feature = "thread-cpu-inline", target_arch = "x86_64"))]
  let status = {
    let mut status = libc::SYS_clock_gettime;
    // SAFETY: this is the Linux x86_64 syscall ABI and value is writable
    // timespec storage.
    unsafe {
      asm!(
        "syscall",
        inlateout("rax") status,
        in("rdi") libc::CLOCK_THREAD_CPUTIME_ID,
        in("rsi") value.as_mut_ptr(),
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
      );
    }
    status
  };
  #[cfg(all(feature = "thread-cpu-inline", target_arch = "aarch64"))]
  let status = {
    let mut status = libc::c_long::from(libc::CLOCK_THREAD_CPUTIME_ID);
    // SAFETY: this is the Linux-kernel aarch64 syscall ABI and value is writable
    // timespec storage.
    unsafe {
      asm!(
        "svc 0",
        inlateout("x0") status,
        in("x1") value.as_mut_ptr(),
        in("x8") libc::SYS_clock_gettime,
        options(nostack),
      );
    }
    status
  };
  #[cfg(not(all(
    feature = "thread-cpu-inline",
    any(target_arch = "x86_64", target_arch = "aarch64"),
  )))]
  let status: libc::c_long = {
    // SAFETY: the pointer is writable timespec storage and the clock ID names
    // the calling thread's CPU-time clock.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, value.as_mut_ptr()).into() }
  };
  assert_eq!(status, 0, "native CLOCK_THREAD_CPUTIME_ID syscall failed");
  // SAFETY: clock_gettime initialized the timespec on success.
  let value = unsafe { value.assume_init() };
  let seconds = u64::try_from(value.tv_sec).expect("native thread CPU seconds were negative");
  let nanos = u32::try_from(value.tv_nsec).expect("native thread CPU nanoseconds were invalid");
  assert!(nanos < 1_000_000_000, "native thread CPU nanoseconds were out of range");
  seconds
    .checked_mul(1_000_000_000)
    .and_then(|base| base.checked_add(u64::from(nanos)))
    .expect("native thread CPU timestamp overflowed")
}

#[cfg(target_os = "macos")]
const NATIVE_THREAD_CPU_BENCH_ID: &str =
  "native_thread_cpu__clock_gettime_nsec_np_clock_thread_cputime_id";

#[cfg(all(feature = "bench-internal", target_os = "macos"))]
const MACOS_THREAD_CPU_MECHANISM: &str = "macos_clock_gettime_nsec_np_thread_cpu";

#[cfg(target_os = "macos")]
#[inline(always)]
fn native_thread_cpu_now() -> u64 {
  unsafe extern "C" {
    fn clock_gettime_nsec_np(clock_id: libc::clockid_t) -> u64;
  }

  // SAFETY: CLOCK_THREAD_CPUTIME_ID is a valid clock identifier on macOS.
  unsafe { clock_gettime_nsec_np(libc::CLOCK_THREAD_CPUTIME_ID) }
}

#[cfg(target_os = "freebsd")]
const NATIVE_THREAD_CPU_BENCH_ID: &str = "native_thread_cpu__clock_gettime_clock_thread_cputime_id";

#[cfg(target_os = "freebsd")]
#[inline(always)]
fn native_thread_cpu_now() -> u64 {
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: the output is writable and CLOCK_THREAD_CPUTIME_ID names the
  // calling thread's CPU clock on FreeBSD.
  let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, value.as_mut_ptr()) };
  assert_eq!(status, 0, "native CLOCK_THREAD_CPUTIME_ID call failed");
  // SAFETY: successful clock_gettime initialized the output.
  let value = unsafe { value.assume_init() };
  let seconds = u64::try_from(value.tv_sec).expect("native thread CPU seconds were negative");
  let nanos = u32::try_from(value.tv_nsec).expect("native thread CPU nanoseconds were invalid");
  assert!(nanos < 1_000_000_000, "native thread CPU nanoseconds were out of range");
  seconds
    .checked_mul(1_000_000_000)
    .and_then(|base| base.checked_add(u64::from(nanos)))
    .expect("native thread CPU timestamp overflowed")
}

#[cfg(target_os = "windows")]
const WINDOWS_THREAD_CPU_MECHANISM: &str = "get_thread_times_current_thread_pseudohandle";

#[cfg(target_os = "windows")]
const WINDOWS_THREAD_CPU_WALL_FALLBACK_MECHANISM: &str = "windows_selected_monotonic_wall_fallback";

#[cfg(target_os = "windows")]
const NATIVE_THREAD_CPU_BENCH_ID: &str =
  "native_thread_cpu__get_thread_times_current_thread_pseudohandle";

#[cfg(target_os = "windows")]
#[inline(always)]
fn native_thread_cpu_now() -> u64 {
  use std::ffi::c_void;

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

  let mut creation = MaybeUninit::<FileTime>::uninit();
  let mut exit = MaybeUninit::<FileTime>::uninit();
  let mut kernel = MaybeUninit::<FileTime>::uninit();
  let mut user = MaybeUninit::<FileTime>::uninit();
  // SAFETY: the current-thread pseudo-handle is valid and all outputs point to
  // writable FILETIME storage.
  let status = unsafe {
    GetThreadTimes(
      (-2_isize) as *mut c_void,
      creation.as_mut_ptr(),
      exit.as_mut_ptr(),
      kernel.as_mut_ptr(),
      user.as_mut_ptr(),
    )
  };
  assert_ne!(status, 0, "native GetThreadTimes call failed");
  // SAFETY: GetThreadTimes initialized both values on success.
  let kernel = unsafe { kernel.assume_init() };
  // SAFETY: GetThreadTimes initialized both values on success.
  let user = unsafe { user.assume_init() };
  let kernel_100ns = (u64::from(kernel.high) << 32) | u64::from(kernel.low);
  let user_100ns = (u64::from(user.high) << 32) | u64::from(user.low);
  kernel_100ns.saturating_add(user_100ns).saturating_mul(100)
}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "windows",),
))]
const THREAD_CPU_BEHAVIOR_SAMPLE_COUNT: usize = 3;

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "windows",),
))]
const THREAD_CPU_BEHAVIOR_WINDOW: Duration = Duration::from_millis(20);

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "windows",),
))]
#[derive(Clone, Copy)]
struct ThreadCpuBehaviorSample {
  wall_delta_ns: u64,
  public_delta_ns: u64,
  direct_delta_ns: u64,
}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "windows",),
))]
fn duration_as_nanos(duration: Duration) -> u64 {
  duration
    .as_nanos()
    .try_into()
    .expect("thread-CPU behavior duration exceeded u64 nanoseconds")
}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "windows",),
))]
fn assert_native_thread_cpu_behavior_provider() {
  let provider = ThreadCpuInstant::provider();
  let read_cost = ThreadCpuInstant::read_cost_hint();
  assert!(
    provider.measures_thread_cpu_time(),
    "thread-CPU semantic sidecar cannot use a wall fallback: {provider:?}",
  );
  assert!(
    !matches!(read_cost, ThreadCpuReadCost::HostCall | ThreadCpuReadCost::Unavailable),
    "thread-CPU semantic sidecar cannot use a host or unavailable read cost: {read_cost:?}",
  );

  #[cfg(target_os = "linux")]
  assert!(
    matches!(
      (provider, read_cost),
      (ThreadCpuProvider::LinuxPerfMmap, ThreadCpuReadCost::Inline)
        | (ThreadCpuProvider::LinuxPerfRead, ThreadCpuReadCost::SystemCall)
        | (ThreadCpuProvider::PosixThreadCpuClock, ThreadCpuReadCost::SystemCall)
    ),
    "Linux thread-CPU provider introspection is not a native route: {provider:?} / {read_cost:?}",
  );
  #[cfg(any(target_os = "macos", target_os = "freebsd"))]
  assert_eq!(
    (provider, read_cost),
    (ThreadCpuProvider::PosixThreadCpuClock, ThreadCpuReadCost::SystemCall),
    "POSIX thread-CPU provider introspection changed",
  );
  #[cfg(target_os = "windows")]
  assert_eq!(
    (provider, read_cost),
    (ThreadCpuProvider::WindowsThreadTimes, ThreadCpuReadCost::SystemCall),
    "Windows thread-CPU provider introspection changed",
  );
}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "windows",),
))]
fn sample_thread_cpu_behavior(operation: impl FnOnce()) -> ThreadCpuBehaviorSample {
  let wall_start = StdInstant::now();
  let public_start = ThreadCpuInstant::now();
  assert!(
    public_start.measures_thread_cpu_time(),
    "thread-CPU semantic sidecar started on a wall fallback",
  );
  let direct_start = native_thread_cpu_now();

  operation();

  let direct_end = native_thread_cpu_now();
  let public_end = ThreadCpuInstant::now();
  assert!(
    public_end.measures_thread_cpu_time(),
    "thread-CPU semantic sidecar ended on a wall fallback",
  );
  let public_delta = public_end
    .checked_duration_since(public_start)
    .expect("thread-CPU semantic sidecar changed domains or moved backward");
  assert!(direct_end >= direct_start, "native thread-CPU reference moved backward",);

  ThreadCpuBehaviorSample {
    wall_delta_ns: duration_as_nanos(wall_start.elapsed()),
    public_delta_ns: duration_as_nanos(public_delta),
    direct_delta_ns: direct_end - direct_start,
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "windows",),
))]
#[inline(never)]
fn consume_current_thread_cpu_for(duration: Duration) {
  let start = StdInstant::now();
  let mut state = 0_u64;
  while start.elapsed() < duration {
    state = state
      .wrapping_mul(6_364_136_223_846_793_005)
      .wrapping_add(1_442_695_040_888_963_407);
    black_box(state);
  }
  black_box(state);
}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "windows",),
))]
fn sample_thread_cpu_sibling_isolation() -> ThreadCpuBehaviorSample {
  use std::sync::{Arc, Barrier};

  let gate = Arc::new(Barrier::new(2));
  let sibling = std::thread::spawn({
    let gate = Arc::clone(&gate);
    move || {
      gate.wait();
      consume_current_thread_cpu_for(Duration::from_millis(40));
    }
  });

  let sample = sample_thread_cpu_behavior(|| {
    gate.wait();
    std::thread::sleep(THREAD_CPU_BEHAVIOR_WINDOW);
  });
  sibling
    .join()
    .expect("sibling thread panicked during thread-CPU isolation probe");
  sample
}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "windows",),
))]
fn median_thread_cpu_behavior(values: [u64; THREAD_CPU_BEHAVIOR_SAMPLE_COUNT]) -> u64 {
  let mut values = values;
  values.sort_unstable();
  values[THREAD_CPU_BEHAVIOR_SAMPLE_COUNT / 2]
}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "windows",),
))]
fn summarize_thread_cpu_behavior(
  samples: [ThreadCpuBehaviorSample; THREAD_CPU_BEHAVIOR_SAMPLE_COUNT],
) -> serde_json::Value {
  let wall_delta_ns = median_thread_cpu_behavior(samples.map(|sample| sample.wall_delta_ns));
  let public_delta_ns = median_thread_cpu_behavior(samples.map(|sample| sample.public_delta_ns));
  let direct_delta_ns = median_thread_cpu_behavior(samples.map(|sample| sample.direct_delta_ns));
  let samples = samples.map(|sample| {
    serde_json::json!({
      "wall_delta_ns": sample.wall_delta_ns,
      "public_delta_ns": sample.public_delta_ns,
      "direct_delta_ns": sample.direct_delta_ns,
    })
  });

  serde_json::json!({
    "wall_delta_ns": wall_delta_ns,
    "public_delta_ns": public_delta_ns,
    "direct_delta_ns": direct_delta_ns,
    "samples": samples,
  })
}

#[cfg(all(
  feature = "bench-internal",
  any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "windows",),
))]
fn bench_thread_cpu_behavior(_c: &mut Criterion) {
  use std::fs;
  use std::path::PathBuf;

  let runtime_attestation = write_criterion_runtime_attestation();
  assert_native_thread_cpu_behavior_provider();
  let busy = [
    sample_thread_cpu_behavior(|| consume_current_thread_cpu_for(THREAD_CPU_BEHAVIOR_WINDOW)),
    sample_thread_cpu_behavior(|| consume_current_thread_cpu_for(THREAD_CPU_BEHAVIOR_WINDOW)),
    sample_thread_cpu_behavior(|| consume_current_thread_cpu_for(THREAD_CPU_BEHAVIOR_WINDOW)),
  ];
  let sleep = [
    sample_thread_cpu_behavior(|| std::thread::sleep(THREAD_CPU_BEHAVIOR_WINDOW)),
    sample_thread_cpu_behavior(|| std::thread::sleep(THREAD_CPU_BEHAVIOR_WINDOW)),
    sample_thread_cpu_behavior(|| std::thread::sleep(THREAD_CPU_BEHAVIOR_WINDOW)),
  ];
  let sibling_isolation = [
    sample_thread_cpu_sibling_isolation(),
    sample_thread_cpu_sibling_isolation(),
    sample_thread_cpu_sibling_isolation(),
  ];
  assert_native_thread_cpu_behavior_provider();

  let payload = serde_json::json!({
    "schema": "tach-thread-cpu-behavior-v2",
    "runtime_attestation": runtime_attestation,
    "direct_benchmark": NATIVE_THREAD_CPU_BENCH_ID,
    "sample_count": THREAD_CPU_BEHAVIOR_SAMPLE_COUNT,
    "busy": summarize_thread_cpu_behavior(busy),
    "sleep": summarize_thread_cpu_behavior(sleep),
    "sibling_isolation": summarize_thread_cpu_behavior(sibling_isolation),
  });
  let target = std::env::var_os("CARGO_TARGET_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("target"));
  let directory = target.join("criterion");
  fs::create_dir_all(&directory).expect("create Criterion directory for thread-CPU behavior");
  fs::write(
    directory.join("thread-cpu-behavior.json"),
    serde_json::to_vec_pretty(&payload).expect("serialize thread-CPU behavior"),
  )
  .expect("write thread-CPU behavior");
}

#[cfg(not(all(
  feature = "bench-internal",
  any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "windows",),
)))]
fn bench_thread_cpu_behavior(_c: &mut Criterion) {
  write_criterion_runtime_attestation();
}

fn bench_ordered(c: &mut Criterion) {
  write_criterion_runtime_attestation();
  let mut g = c.benchmark_group("Ordered Instant::now()");
  g.bench_function("tach::OrderedInstant", |b| {
    b.iter(|| black_box(OrderedInstant::now()));
  });
  g.bench_function("tach::OrderedInstant (now + elapsed)", |b| {
    b.iter(|| {
      let start = OrderedInstant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("tach::Instant (unordered reference)", |b| {
    b.iter(|| black_box(Instant::now()));
  });
  g.bench_function("std::time::Instant", |b| {
    b.iter(|| black_box(StdInstant::now()));
  });
  g.bench_function("std::time::Instant (now + elapsed)", |b| {
    b.iter(|| {
      let start = StdInstant::now();
      black_box(start.elapsed())
    });
  });
  g.finish();
}

// Isolates `elapsed()` alone (one counter read + the subtraction + conversion),
// holding `start` outside the loop so the second `now()` of the combined bench
// doesn't dilute the signal. This is the group that exposes the saturating_sub
// cost most directly.
fn bench_elapsed_only(c: &mut Criterion) {
  write_criterion_runtime_attestation();
  let mut g = c.benchmark_group("elapsed() only");
  let tach_start = Instant::now();
  g.bench_function("tach::Instant", |b| {
    b.iter(|| black_box(black_box(tach_start).elapsed()));
  });
  let ordered_start = OrderedInstant::now();
  g.bench_function("tach::OrderedInstant", |b| {
    b.iter(|| black_box(black_box(ordered_start).elapsed()));
  });
  let thread_cpu_start = ThreadCpuInstant::now();
  g.bench_function("tach::ThreadCpuInstant", |b| {
    b.iter(|| black_box(black_box(thread_cpu_start).elapsed()));
  });
  let std_start = StdInstant::now();
  g.bench_function("std::time::Instant", |b| {
    b.iter(|| black_box(black_box(std_start).elapsed()));
  });
  g.finish();
}

criterion_group!(
  benches,
  bench_now,
  bench_elapsed,
  bench_thread_cpu_now,
  bench_thread_cpu_elapsed,
  bench_elapsed_only,
  bench_ordered,
  bench_thread_cpu_behavior,
);
criterion_main!(benches);
