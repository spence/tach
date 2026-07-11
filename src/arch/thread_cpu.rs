//! Current-thread timing providers, normalized to nanoseconds.

use crate::{ThreadCpuProvider, ThreadCpuReadCost};

#[cfg(any(
  all(unix, not(any(target_os = "macos", target_os = "emscripten"))),
  target_os = "windows",
))]
use core::sync::atomic::{AtomicU8, Ordering};

#[cfg(all(
  feature = "thread-cpu-inline",
  target_os = "linux",
  any(target_arch = "x86_64", target_arch = "aarch64"),
))]
#[path = "thread_cpu_linux_inline.rs"]
mod linux_inline;

#[cfg(all(
  feature = "bench-internal",
  feature = "thread-cpu-inline",
  target_os = "linux",
  any(target_arch = "x86_64", target_arch = "aarch64"),
))]
pub(crate) struct BenchPerfHandle(linux_inline::BenchPerfHandle);

#[cfg(all(
  feature = "bench-internal",
  feature = "thread-cpu-inline",
  target_os = "linux",
  any(target_arch = "x86_64", target_arch = "aarch64"),
))]
impl BenchPerfHandle {
  #[inline(always)]
  #[allow(clippy::inline_always)]
  pub(crate) fn now_nanos(&self) -> u64 {
    self.0.now_nanos()
  }
}

#[cfg(all(
  feature = "bench-internal",
  feature = "thread-cpu-inline",
  target_os = "linux",
  any(target_arch = "x86_64", target_arch = "aarch64"),
))]
pub(crate) fn bench_perf_handle() -> Option<BenchPerfHandle> {
  linux_inline::bench_perf_handle().map(BenchPerfHandle)
}

#[cfg(all(
  feature = "bench-internal",
  feature = "thread-cpu-inline",
  target_os = "linux",
  any(target_arch = "x86_64", target_arch = "aarch64"),
))]
pub(crate) fn bench_selection_measurements() -> Option<([u64; 9], [u64; 9], usize)> {
  linux_inline::bench_selection_measurements()
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten"))))]
use core::mem::MaybeUninit;

#[cfg(any(
  all(unix, not(any(target_os = "macos", target_os = "emscripten"))),
  target_os = "windows",
))]
const NATIVE_UNKNOWN: u8 = 0;
#[cfg(any(
  all(unix, not(any(target_os = "macos", target_os = "emscripten"))),
  target_os = "windows",
))]
const NATIVE_THREAD_CPU: u8 = 1;
#[cfg(any(
  all(unix, not(any(target_os = "macos", target_os = "emscripten"))),
  target_os = "windows",
))]
const NATIVE_WALL: u8 = 2;
#[cfg(any(
  all(unix, not(any(target_os = "macos", target_os = "emscripten"))),
  target_os = "windows",
))]
static NATIVE_PROVIDER: AtomicU8 = AtomicU8::new(NATIVE_UNKNOWN);

#[cfg(target_os = "linux")]
#[cfg(not(all(
  feature = "thread-cpu-inline",
  any(target_arch = "x86_64", target_arch = "aarch64"),
)))]
#[inline]
pub(crate) fn now_nanos() -> u64 {
  posix_now_nanos()
}

#[cfg(all(
  feature = "thread-cpu-inline",
  target_os = "linux",
  any(target_arch = "x86_64", target_arch = "aarch64"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn now_nanos() -> u64 {
  linux_inline::now_nanos()
}

#[cfg(all(
  feature = "thread-cpu-inline",
  target_os = "linux",
  any(target_arch = "x86_64", target_arch = "aarch64"),
))]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  let provider = linux_inline::provider();
  if provider == ThreadCpuProvider::LinuxPerfMmap {
    provider
  } else if native_clock_uses_wall() {
    wall_provider()
  } else {
    provider
  }
}

#[cfg(all(
  feature = "thread-cpu-inline",
  target_os = "linux",
  any(target_arch = "x86_64", target_arch = "aarch64"),
))]
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  if linux_inline::provider() == ThreadCpuProvider::LinuxPerfMmap {
    ThreadCpuReadCost::Inline
  } else if native_clock_uses_wall() {
    wall_read_cost()
  } else {
    linux_inline::read_cost_hint()
  }
}

#[cfg(all(
  target_os = "linux",
  not(all(feature = "thread-cpu-inline", any(target_arch = "x86_64", target_arch = "aarch64"),)),
))]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  initialize_native_clock();
  native_provider(ThreadCpuProvider::PosixThreadCpuClock)
}

#[cfg(all(
  target_os = "linux",
  not(all(feature = "thread-cpu-inline", any(target_arch = "x86_64", target_arch = "aarch64"),)),
))]
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  initialize_native_clock();
  native_read_cost()
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos", target_os = "emscripten"))))]
#[inline]
pub(crate) fn now_nanos() -> u64 {
  posix_now_nanos()
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos", target_os = "emscripten"))))]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  initialize_native_clock();
  native_provider(ThreadCpuProvider::PosixThreadCpuClock)
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos", target_os = "emscripten"))))]
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  initialize_native_clock();
  native_read_cost()
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten"))))]
#[inline]
fn posix_now_nanos() -> u64 {
  let native_state = NATIVE_PROVIDER.load(Ordering::Relaxed);
  if native_state == NATIVE_WALL {
    return wall_now_value();
  }
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: `value` is writable storage for a timespec and the clock id
  // selects CPU time consumed by the calling thread.
  let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, value.as_mut_ptr()) };
  let nanos = if status == 0 {
    // SAFETY: a successful clock_gettime initialized the output.
    let value = unsafe { value.assume_init() };
    timespec_to_nanos(value)
  } else {
    None
  };
  select_native_value(native_state, nanos)
}

#[cfg(target_os = "macos")]
#[inline]
pub(crate) fn now_nanos() -> u64 {
  unsafe extern "C" {
    fn clock_gettime_nsec_np(clock_id: libc::clockid_t) -> u64;
  }

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
#[inline]
pub(crate) fn now_nanos() -> u64 {
  use core::ffi::c_void;
  use core::mem::MaybeUninit;

  let native_state = NATIVE_PROVIDER.load(Ordering::Relaxed);
  if native_state == NATIVE_WALL {
    return wall_now_value();
  }
  #[repr(C)]
  struct FileTime {
    low: u32,
    high: u32,
  }

  #[link(name = "kernel32")]
  unsafe extern "system" {
    fn GetCurrentThread() -> *mut c_void;
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

  // SAFETY: GetCurrentThread returns a pseudo-handle for the caller and each
  // FILETIME pointer addresses writable storage.
  let status = unsafe {
    GetThreadTimes(
      GetCurrentThread(),
      creation.as_mut_ptr(),
      exit.as_mut_ptr(),
      kernel.as_mut_ptr(),
      user.as_mut_ptr(),
    )
  };
  let nanos = if status != 0 {
    // SAFETY: successful GetThreadTimes initialized every output.
    let kernel = unsafe { kernel.assume_init() };
    // SAFETY: successful GetThreadTimes initialized every output.
    let user = unsafe { user.assume_init() };
    let kernel_100ns = (u64::from(kernel.high) << 32) | u64::from(kernel.low);
    let user_100ns = (u64::from(user.high) << 32) | u64::from(user.low);
    kernel_100ns.checked_add(user_100ns).and_then(|ticks| ticks.checked_mul(100))
  } else {
    None
  };
  select_native_value(native_state, nanos)
}

#[cfg(target_os = "windows")]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  initialize_native_clock();
  native_provider(ThreadCpuProvider::WindowsThreadTimes)
}

#[cfg(target_os = "windows")]
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  initialize_native_clock();
  native_read_cost()
}

#[cfg(target_os = "wasi")]
const WASI_UNKNOWN: u8 = 0;
#[cfg(target_os = "wasi")]
const WASI_THREAD_CPU: u8 = 1;
#[cfg(target_os = "wasi")]
const WASI_WALL: u8 = 2;
#[cfg(target_os = "wasi")]
static WASI_PROVIDER: core::sync::atomic::AtomicU8 =
  core::sync::atomic::AtomicU8::new(WASI_UNKNOWN);

#[cfg(target_os = "wasi")]
#[inline]
pub(crate) fn now_nanos() -> u64 {
  use core::sync::atomic::Ordering;

  let state = WASI_PROVIDER.load(Ordering::Relaxed);
  if state == WASI_WALL {
    return wall_now_value();
  }
  #[link(wasm_import_module = "wasi_snapshot_preview1")]
  unsafe extern "C" {
    fn clock_time_get(id: u32, precision: u64, time: *mut u64) -> u16;
  }

  const CLOCK_THREAD_CPUTIME_ID: u32 = 3;
  let mut nanos = 0;
  // SAFETY: nanos is writable u64 storage and precision zero is valid. Hosts
  // that do not implement the optional thread clock return an error code.
  let status = unsafe { clock_time_get(CLOCK_THREAD_CPUTIME_ID, 0, &mut nanos) };
  if status == 0 {
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

#[cfg(target_os = "wasi")]
#[inline]
pub(crate) fn provider() -> ThreadCpuProvider {
  use core::sync::atomic::Ordering;

  let _ = now_nanos();
  match WASI_PROVIDER.load(Ordering::Relaxed) {
    WASI_THREAD_CPU => ThreadCpuProvider::WasiThreadCpuClock,
    _ => wall_provider(),
  }
}

#[cfg(target_os = "wasi")]
#[inline]
pub(crate) fn read_cost_hint() -> ThreadCpuReadCost {
  let _ = now_nanos();
  ThreadCpuReadCost::HostCall
}

#[cfg(not(any(
  all(unix, not(target_os = "emscripten")),
  target_os = "windows",
  target_os = "wasi",
)))]
#[inline]
pub(crate) fn now_nanos() -> u64 {
  wall_now_value()
}

#[cfg(not(any(
  all(unix, not(target_os = "emscripten")),
  target_os = "windows",
  target_os = "wasi",
)))]
#[inline]
pub(crate) const fn provider() -> ThreadCpuProvider {
  wall_provider()
}

#[cfg(not(any(
  all(unix, not(target_os = "emscripten")),
  target_os = "windows",
  target_os = "wasi",
)))]
#[inline]
pub(crate) const fn read_cost_hint() -> ThreadCpuReadCost {
  wall_read_cost()
}

#[cfg(any(
  all(
    unix,
    not(any(target_os = "macos", target_os = "emscripten")),
    not(all(
      feature = "thread-cpu-inline",
      target_os = "linux",
      any(target_arch = "x86_64", target_arch = "aarch64"),
    )),
  ),
  target_os = "windows",
))]
#[inline]
fn initialize_native_clock() {
  if NATIVE_PROVIDER.load(Ordering::Relaxed) == NATIVE_UNKNOWN {
    let _ = now_nanos();
  }
}

#[cfg(any(
  all(unix, not(any(target_os = "macos", target_os = "emscripten"))),
  target_os = "windows",
))]
#[inline]
pub(super) fn native_clock_uses_wall() -> bool {
  NATIVE_PROVIDER.load(Ordering::Relaxed) == NATIVE_WALL
}

#[cfg(any(
  all(
    unix,
    not(any(target_os = "macos", target_os = "emscripten")),
    not(all(
      feature = "thread-cpu-inline",
      target_os = "linux",
      any(target_arch = "x86_64", target_arch = "aarch64"),
    )),
  ),
  target_os = "windows",
))]
#[inline]
fn native_provider(cpu_provider: ThreadCpuProvider) -> ThreadCpuProvider {
  if native_clock_uses_wall() { wall_provider() } else { cpu_provider }
}

#[cfg(any(
  all(
    unix,
    not(any(target_os = "macos", target_os = "emscripten")),
    not(all(
      feature = "thread-cpu-inline",
      target_os = "linux",
      any(target_arch = "x86_64", target_arch = "aarch64"),
    )),
  ),
  target_os = "windows",
))]
#[inline]
fn native_read_cost() -> ThreadCpuReadCost {
  if native_clock_uses_wall() { wall_read_cost() } else { ThreadCpuReadCost::SystemCall }
}

#[cfg(any(
  all(unix, not(any(target_os = "macos", target_os = "emscripten"))),
  target_os = "windows",
))]
#[inline]
fn select_native_value(native_state: u8, nanos: Option<u64>) -> u64 {
  match (native_state, nanos) {
    (NATIVE_THREAD_CPU, Some(nanos)) => nanos,
    (NATIVE_THREAD_CPU, None) => {
      NATIVE_PROVIDER.store(NATIVE_WALL, Ordering::Relaxed);
      wall_now_value()
    }
    (NATIVE_WALL, _) => wall_now_value(),
    (NATIVE_UNKNOWN, Some(nanos)) => match NATIVE_PROVIDER.compare_exchange(
      NATIVE_UNKNOWN,
      NATIVE_THREAD_CPU,
      Ordering::Relaxed,
      Ordering::Relaxed,
    ) {
      Ok(_) | Err(NATIVE_THREAD_CPU) => nanos,
      Err(_) => wall_now_value(),
    },
    (NATIVE_UNKNOWN, None) => match NATIVE_PROVIDER.compare_exchange(
      NATIVE_UNKNOWN,
      NATIVE_WALL,
      Ordering::Relaxed,
      Ordering::Relaxed,
    ) {
      Ok(_) | Err(NATIVE_WALL) => wall_now_value(),
      Err(_) => {
        NATIVE_PROVIDER.store(NATIVE_WALL, Ordering::Relaxed);
        wall_now_value()
      }
    },
    _ => wall_now_value(),
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

#[inline]
#[cfg(not(target_os = "macos"))]
fn wall_now_value() -> u64 {
  crate::thread_cpu::encode_wall_ticks(crate::arch::ticks())
}

#[inline]
#[cfg(not(target_os = "macos"))]
const fn wall_provider() -> ThreadCpuProvider {
  #[cfg(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none")))]
  {
    ThreadCpuProvider::PerformanceNow
  }
  #[cfg(not(all(target_arch = "wasm32", any(target_os = "unknown", target_os = "none"))))]
  {
    ThreadCpuProvider::MonotonicWallClock
  }
}

#[inline]
#[cfg(not(any(target_os = "macos", target_os = "wasi")))]
const fn wall_read_cost() -> ThreadCpuReadCost {
  #[cfg(any(
    target_arch = "x86_64",
    target_arch = "x86",
    target_arch = "aarch64",
    target_arch = "riscv64",
    target_arch = "loongarch64",
  ))]
  {
    ThreadCpuReadCost::Inline
  }
  #[cfg(all(
    not(any(
      target_arch = "x86_64",
      target_arch = "x86",
      target_arch = "aarch64",
      target_arch = "riscv64",
      target_arch = "loongarch64",
    )),
    any(target_os = "wasi", target_os = "unknown", target_os = "none"),
  ))]
  {
    ThreadCpuReadCost::HostCall
  }
  #[cfg(all(
    not(any(
      target_arch = "x86_64",
      target_arch = "x86",
      target_arch = "aarch64",
      target_arch = "riscv64",
      target_arch = "loongarch64",
    )),
    not(any(target_os = "wasi", target_os = "unknown", target_os = "none")),
  ))]
  {
    ThreadCpuReadCost::SystemCall
  }
}
