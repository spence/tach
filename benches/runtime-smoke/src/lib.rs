#![no_std]

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
use core::alloc::{GlobalAlloc, Layout};
#[cfg(all(target_arch = "wasm32", target_os = "none"))]
use core::sync::atomic::{AtomicUsize, Ordering};
#[cfg(all(target_arch = "wasm32", target_os = "none"))]
use tach::{Instant, OrderedInstant, ThreadCpuInstant, ThreadCpuProvider};
#[cfg(all(target_arch = "wasm32", target_os = "none"))]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
const SOURCE_REVISION: &str = env!("TACH_BENCH_SOURCE_REVISION");
#[cfg(all(target_arch = "wasm32", target_os = "none"))]
const INVOCATION_ID: &str = env!("TACH_BENCH_INVOCATION_ID");

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
struct SmokeAllocator;

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
static NEXT_ALLOCATION: AtomicUsize = AtomicUsize::new(0);

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
unsafe extern "C" {
  static __heap_base: u8;
}

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
unsafe impl GlobalAlloc for SmokeAllocator {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    let heap_base = core::ptr::addr_of!(__heap_base) as usize;
    let mut current = NEXT_ALLOCATION.load(Ordering::Relaxed).max(heap_base);
    loop {
      let aligned = match current.checked_add(layout.align() - 1) {
        Some(value) => value & !(layout.align() - 1),
        None => return core::ptr::null_mut(),
      };
      let end = match aligned.checked_add(layout.size()) {
        Some(value) => value,
        None => return core::ptr::null_mut(),
      };
      let current_pages = core::arch::wasm32::memory_size(0);
      let required_pages = end.div_ceil(65_536);
      if required_pages > current_pages
        && core::arch::wasm32::memory_grow(0, required_pages - current_pages) == usize::MAX
      {
        return core::ptr::null_mut();
      }
      match NEXT_ALLOCATION.compare_exchange_weak(
        current,
        end,
        Ordering::Relaxed,
        Ordering::Relaxed,
      ) {
        Ok(_) => return aligned as *mut u8,
        Err(next) => current = next.max(heap_base),
      }
    }
  }

  unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
#[global_allocator]
static ALLOCATOR: SmokeAllocator = SmokeAllocator;

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
  core::arch::wasm32::unreachable()
}

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
fn advances_instant() -> bool {
  let start = Instant::now();
  (0..1_000_000).any(|_| start.elapsed().as_nanos() > 0)
}

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
fn advances_ordered() -> bool {
  let start = OrderedInstant::now();
  (0..1_000_000).any(|_| start.elapsed().as_nanos() > 0)
}

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
fn advances_thread_cpu() -> bool {
  let start = ThreadCpuInstant::now();
  (0..1_000_000).any(|_| start.elapsed().as_nanos() > 0)
}

/// Executes all three public timer paths in the target runtime.
///
/// Each set bit represents one independently checked assertion. The producer
/// accepts only the complete mask, so a frozen or unavailable host clock
/// cannot be mistaken for a successful smoke run.
#[cfg(all(target_arch = "wasm32", target_os = "none"))]
#[wasm_bindgen]
pub fn tach_runtime_smoke() -> u32 {
  let provider = ThreadCpuInstant::provider();
  u32::from(advances_instant())
    | (u32::from(advances_ordered()) << 1)
    | (u32::from(advances_thread_cpu()) << 2)
    | (u32::from(!matches!(provider, ThreadCpuProvider::Unavailable)) << 3)
}

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
#[wasm_bindgen]
pub fn tach_runtime_smoke_provider() -> u32 {
  match ThreadCpuInstant::provider() {
    ThreadCpuProvider::NodeThreadCpuUsage => 1,
    ThreadCpuProvider::PerformanceNow => 2,
    ThreadCpuProvider::NodeHrtime => 3,
    ThreadCpuProvider::MonotonicWallClock => 4,
    ThreadCpuProvider::Unavailable => 0,
    _ => 5,
  }
}

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
#[wasm_bindgen]
pub fn tach_runtime_smoke_measures_thread_cpu() -> bool {
  ThreadCpuInstant::now().measures_thread_cpu_time()
}

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
#[wasm_bindgen]
pub fn tach_runtime_smoke_revision_len() -> u32 {
  SOURCE_REVISION.len() as u32
}

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
#[wasm_bindgen]
pub fn tach_runtime_smoke_revision_byte(index: u32) -> u32 {
  SOURCE_REVISION.as_bytes().get(index as usize).copied().unwrap_or_default() as u32
}

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
#[wasm_bindgen]
pub fn tach_runtime_smoke_invocation_len() -> u32 {
  INVOCATION_ID.len() as u32
}

#[cfg(all(target_arch = "wasm32", target_os = "none"))]
#[wasm_bindgen]
pub fn tach_runtime_smoke_invocation_byte(index: u32) -> u32 {
  INVOCATION_ID.as_bytes().get(index as usize).copied().unwrap_or_default() as u32
}
