// Direct OS clocks used when the platform owns the reliable monotonic
// timeline. Each submodule is cfg-gated to its platform; `direct::ticks()`
// selects one based on both target OS and architecture.

#[cfg(target_os = "macos")]
mod mach {
  #[repr(C)]
  struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
  }

  unsafe extern "C" {
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
  }

  #[inline(always)]
  pub fn mach_time() -> u64 {
    // SAFETY: `mach_absolute_time` takes no arguments and returns the host
    // monotonic tick value with no Rust-side aliasing requirements.
    unsafe { mach_absolute_time() }
  }

  #[inline]
  pub fn mach_timebase() -> (u32, u32) {
    let mut info = MachTimebaseInfo { numer: 0, denom: 0 };
    // SAFETY: `info` is writable storage with the documented Darwin ABI.
    let _ = unsafe { mach_timebase_info(&mut info) };

    // Darwin documents a successful, non-zero ratio. Keep the conversion
    // total even if a non-conforming host violates that contract.
    (info.numer.max(1), info.denom.max(1))
  }
}

#[cfg(target_os = "macos")]
pub use mach::*;

#[cfg_attr(
  any(
    target_os = "android",
    all(target_os = "linux", any(target_arch = "arm", target_arch = "s390x"))
  ),
  allow(dead_code)
)]
#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten")),))]
mod monotonic {
  #[cfg(all(target_os = "linux", target_arch = "powerpc64", not(target_env = "gnu"),))]
  use core::sync::atomic::{Ordering, fence};

  #[cfg(all(target_os = "linux", target_arch = "powerpc64", not(target_env = "gnu"),))]
  #[inline(always)]
  fn ordered_clock_barrier() {
    // Acquire fences are compiler-only on s390x and lower to `lwsync` on
    // powerpc64. Neither serializes a following non-storage clock operation.
    // SeqCst is the minimum Rust primitive that lowers to the required
    // execution barrier: `bcr 15,0` on s390x and heavyweight `sync` on
    // powerpc64. Power ISA v3.1 Book II section 4.6.3 specifies that hwsync
    // completes all prior instructions before any later instruction starts,
    // which orders the vDSO's Time Base read itself rather than only its
    // surrounding memory accesses.
    fence(Ordering::SeqCst);
  }

  #[inline(always)]
  pub fn clock_monotonic() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `ts` is writable storage with the platform libc ABI, and
    // CLOCK_MONOTONIC is valid on every Unix target routed here.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    timespec_nanos(rc, ts)
  }

  #[cfg(all(target_os = "linux", target_arch = "powerpc64", not(target_env = "gnu"),))]
  #[inline(always)]
  pub fn clock_monotonic_ordered() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    ordered_clock_barrier();
    // SAFETY: `ts` is writable storage with the Linux libc ABI, and
    // CLOCK_MONOTONIC is a valid clock ID.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    timespec_nanos(rc, ts)
  }

  #[inline(always)]
  fn timespec_nanos(status: i32, value: libc::timespec) -> u64 {
    if status != 0 {
      return 0;
    }
    value.tv_sec as u64 * 1_000_000_000 + value.tv_nsec as u64
  }

  #[cfg(test)]
  mod tests {
    use super::*;

    #[test]
    fn clock_failure_has_a_total_value() {
      let value = libc::timespec { tv_sec: 123, tv_nsec: 456 };
      assert_eq!(timespec_nanos(-1, value), 0);
    }
  }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "emscripten")),))]
#[allow(unused_imports)]
pub use monotonic::*;

#[cfg(all(target_os = "wasi", target_env = "p1"))]
mod wasip1 {
  #[link(wasm_import_module = "wasi_snapshot_preview1")]
  unsafe extern "C" {
    fn clock_time_get(id: u32, precision: u64, time: *mut u64) -> u16;
  }

  const CLOCK_MONOTONIC: u32 = 1;

  #[inline(always)]
  pub fn wasi_clock_monotonic() -> u64 {
    let mut t: u64 = 0;
    // SAFETY: writes a single u64 the host fills in. CLOCK_MONOTONIC and
    // precision=0 are always-valid inputs for wasi_snapshot_preview1.
    let _ = unsafe { clock_time_get(CLOCK_MONOTONIC, 0, &mut t) };
    t
  }
}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
pub use wasip1::*;

#[cfg(all(target_os = "wasi", target_env = "p2"))]
mod wasi_p2 {
  #[inline(always)]
  pub fn wasi_clock_monotonic() -> u64 {
    wasip2::clocks::monotonic_clock::now()
  }
}

#[cfg(all(target_os = "wasi", target_env = "p2"))]
pub use wasi_p2::*;

#[cfg(target_os = "windows")]
mod qpc {
  #[cfg(feature = "bench-internal")]
  use core::cell::UnsafeCell;
  use core::hint::black_box;
  #[cfg(feature = "bench-internal")]
  use core::mem::MaybeUninit;
  use core::sync::atomic::{AtomicI64, AtomicU8, AtomicU32, AtomicUsize, Ordering};

  const PROVIDER_UNKNOWN: u8 = 0;
  const PROVIDER_SELECTING: u8 = 1;
  const PROVIDER_FORCED_QPC: u8 = u8::MAX;

  const SOURCE_QPC: u8 = 0;
  const SOURCE_INTERRUPT_TIME_PRECISE: u8 = 1;
  const SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE: u8 = 2;
  const SOURCE_COUNT: usize = 3;

  const INSTANT_PROVIDER_BASE: u8 = 2;
  const INSTANT_QPC: u8 = INSTANT_PROVIDER_BASE + SOURCE_QPC;
  const INSTANT_INTERRUPT_TIME_PRECISE: u8 = INSTANT_PROVIDER_BASE + SOURCE_INTERRUPT_TIME_PRECISE;
  const INSTANT_UNBIASED_INTERRUPT_TIME_PRECISE: u8 =
    INSTANT_PROVIDER_BASE + SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE;

  const ORDERED_PROVIDER_BASE: u8 = 2;
  const ORDERED_BARRIER_COUNT: u8 = 5;
  const MAX_ORDERED_CANDIDATES: usize = SOURCE_COUNT * ORDERED_BARRIER_COUNT as usize;
  const PRECISE_INTERRUPT_TIME_FREQUENCY: u64 = 10_000_000;
  const PROC_UNAVAILABLE: usize = 1;

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  const ORDERED_UNKNOWN: u8 = 0;
  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  const ORDERED_SELECTING: u8 = 1;
  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  const ORDERED_CPUID: u8 = 2;
  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  const ORDERED_LFENCE: u8 = 3;
  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  const ORDERED_RDTSCP: u8 = 4;
  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  const ORDERED_MFENCE: u8 = 5;
  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  const ORDERED_SERIALIZE: u8 = 6;
  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  const ORDERED_BASELINE_BARRIER: u8 = ORDERED_CPUID;
  #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
  const ORDERED_BASELINE_BARRIER: u8 = 0;
  const PROBE_BATCHES: usize = 9;
  const PROBE_READS: u64 = 4096;
  const PROBE_WARMUP_READS: u64 = 1024;
  const REQUIRED_DECISIVE_WINS: usize = 8;

  unsafe extern "system" {
    fn QueryPerformanceCounter(c: *mut i64) -> i32;
    fn QueryPerformanceFrequency(f: *mut i64) -> i32;
    fn GetCurrentThreadId() -> u32;
    fn GetModuleHandleW(module_name: *const u16) -> *mut core::ffi::c_void;
    fn GetProcAddress(
      module: *mut core::ffi::c_void,
      procedure_name: *const u8,
    ) -> *mut core::ffi::c_void;
  }

  // QPC is the Windows-owned reliable interval timeline. Its implementation
  // may use a synchronized TSC, an Arm system counter, a proprietary platform
  // counter, or a kernel path, and may apply hypervisor scaling and bias. Raw
  // architectural reads are therefore not equivalent even when a local probe
  // finds them cheaper. QueryPerformanceFrequency is constant after boot, so
  // both wall-clock APIs reuse it while keeping independent scale caches.
  static QPC_FREQ: AtomicI64 = AtomicI64::new(0);
  static INTERRUPT_TIME_PRECISE: AtomicUsize = AtomicUsize::new(0);
  static UNBIASED_INTERRUPT_TIME_PRECISE: AtomicUsize = AtomicUsize::new(0);
  static INSTANT_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
  static INSTANT_PROVIDER_OWNER: AtomicU32 = AtomicU32::new(0);
  static ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(PROVIDER_UNKNOWN);
  static ORDERED_PROVIDER_OWNER: AtomicU32 = AtomicU32::new(0);
  static INSTANT_PROBE_PROVIDER: AtomicU8 = AtomicU8::new(INSTANT_QPC);
  static ORDERED_PROBE_PROVIDER: AtomicU8 = AtomicU8::new(ORDERED_PROVIDER_BASE);
  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  static ORDERED_QPC_BARRIER: AtomicU8 = AtomicU8::new(ORDERED_UNKNOWN);
  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  static PROBE_ORDERED_QPC_BARRIER: AtomicU8 = AtomicU8::new(ORDERED_CPUID);

  #[derive(Clone, Copy)]
  #[allow(dead_code)] // Decision detail is retained for bench evidence.
  struct SelectionDecision {
    allowance: u64,
    decisive_wins: usize,
    challenger_selected: bool,
  }

  #[derive(Clone, Copy)]
  struct CandidateList<const N: usize> {
    providers: [u8; N],
    count: usize,
  }

  impl<const N: usize> CandidateList<N> {
    const fn new() -> Self {
      Self { providers: [PROVIDER_UNKNOWN; N], count: 0 }
    }

    fn push(&mut self, provider: u8) {
      debug_assert!(self.count < N);
      self.providers[self.count] = provider;
      self.count += 1;
    }
  }

  #[derive(Clone, Copy)]
  struct ProbeSamples<const N: usize> {
    batches: [[u64; PROBE_BATCHES]; N],
  }

  #[derive(Clone, Copy)]
  #[allow(dead_code)] // Candidate/sample detail is retained for bench evidence.
  struct Tournament<const N: usize> {
    selected_provider: u8,
    candidates: CandidateList<N>,
    samples: ProbeSamples<N>,
  }

  #[cfg(feature = "bench-internal")]
  struct WallEvidenceCell<T>(UnsafeCell<MaybeUninit<T>>);

  // SAFETY: the process selector writes each Copy value before publishing its
  // provider with Release; evidence readers acquire the selected provider.
  #[cfg(feature = "bench-internal")]
  unsafe impl<T: Copy> Sync for WallEvidenceCell<T> {}

  #[cfg(feature = "bench-internal")]
  static INSTANT_WALL_EVIDENCE: WallEvidenceCell<Tournament<SOURCE_COUNT>> =
    WallEvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));
  #[cfg(feature = "bench-internal")]
  static ORDERED_WALL_EVIDENCE: WallEvidenceCell<Tournament<MAX_ORDERED_CANDIDATES>> =
    WallEvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));

  #[cfg(feature = "bench-internal")]
  #[derive(Clone, Copy, Debug)]
  #[allow(dead_code)] // The benchmark serializer projects this complete schema.
  pub(crate) struct WindowsWallProbeEvidence {
    pub(crate) reads_per_batch: u64,
    pub(crate) required_decisive_wins: usize,
    pub(crate) instant_candidate_count: usize,
    pub(crate) instant_candidate_names: [&'static str; SOURCE_COUNT],
    pub(crate) instant_candidate_batches_ns: [[u64; PROBE_BATCHES]; SOURCE_COUNT],
    pub(crate) instant_candidate_medians_ns: [u64; SOURCE_COUNT],
    pub(crate) ordered_candidate_count: usize,
    pub(crate) ordered_candidate_names: [&'static str; MAX_ORDERED_CANDIDATES],
    pub(crate) ordered_candidate_batches_ns: [[u64; PROBE_BATCHES]; MAX_ORDERED_CANDIDATES],
    pub(crate) ordered_candidate_medians_ns: [u64; MAX_ORDERED_CANDIDATES],
    pub(crate) instant_selected_provider: &'static str,
    pub(crate) ordered_selected_provider: &'static str,
    pub(crate) interrupt_time_precise_available: bool,
    pub(crate) unbiased_interrupt_time_precise_available: bool,
    pub(crate) raw_architectural_counter_eligible: bool,
    pub(crate) raw_architectural_counter_exclusion: &'static str,
    pub(crate) coarse_clock_eligible: bool,
    pub(crate) coarse_clock_exclusion: &'static str,
    pub(crate) utc_clock_eligible: bool,
    pub(crate) utc_clock_exclusion: &'static str,
    pub(crate) auxiliary_counter_eligible: bool,
    pub(crate) auxiliary_counter_exclusion: &'static str,
  }

  #[cfg(feature = "bench-internal")]
  #[derive(Clone, Copy)]
  #[allow(dead_code)] // Consumed by out-of-module benchmark adapters.
  pub(crate) struct BenchPrimitive {
    pub(crate) name: &'static str,
    pub(crate) read: fn() -> u64,
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  #[derive(Clone, Copy, Debug)]
  pub(crate) struct QpcOrderedEvidence {
    pub(crate) reads_per_batch: u64,
    pub(crate) cpuid_batches_ns: [u64; PROBE_BATCHES],
    pub(crate) lfence_batches_ns: [u64; PROBE_BATCHES],
    pub(crate) rdtscp_batches_ns: [u64; PROBE_BATCHES],
    pub(crate) mfence_batches_ns: [u64; PROBE_BATCHES],
    pub(crate) serialize_batches_ns: [u64; PROBE_BATCHES],
    pub(crate) lfence_eligible: bool,
    pub(crate) rdtscp_eligible: bool,
    pub(crate) mfence_eligible: bool,
    pub(crate) serialize_eligible: bool,
    pub(crate) selected_provider: &'static str,
    pub(crate) required_decisive_wins: usize,
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  struct EvidenceCell(UnsafeCell<MaybeUninit<QpcOrderedEvidence>>);

  // SAFETY: the selection owner writes once before publishing the selected
  // barrier with Release; evidence readers acquire that state first.
  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  unsafe impl Sync for EvidenceCell {}

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  static ORDERED_EVIDENCE: EvidenceCell = EvidenceCell(UnsafeCell::new(MaybeUninit::uninit()));

  #[inline]
  pub fn qpc_frequency() -> u64 {
    let cached = QPC_FREQ.load(Ordering::Relaxed);
    if cached != 0 {
      return cached as u64;
    }
    let mut f: i64 = 0;
    // SAFETY: writes a single i64.
    let _ = unsafe { QueryPerformanceFrequency(&mut f) };
    f = f.max(1);
    QPC_FREQ.store(f, Ordering::Relaxed);
    f as u64
  }

  type PreciseClock = unsafe extern "system" fn(*mut u64);

  const KERNEL32_DLL: [u16; 13] = [
    0x006b, 0x0065, 0x0072, 0x006e, 0x0065, 0x006c, 0x0033, 0x0032, 0x002e, 0x0064, 0x006c, 0x006c,
    0,
  ];
  const QUERY_INTERRUPT_TIME_PRECISE: &[u8] = b"QueryInterruptTimePrecise\0";
  const QUERY_UNBIASED_INTERRUPT_TIME_PRECISE: &[u8] = b"QueryUnbiasedInterruptTimePrecise\0";

  /// Returns the fastest high-resolution Windows-owned wall clock on this host.
  ///
  /// QPC, precise interrupt time, and precise unbiased interrupt time are the
  /// complete documented native candidate set for this contract. The latter
  /// two are resolved dynamically because their import is unavailable before
  /// Windows 10. Coarse interrupt/GetTickCount clocks do not satisfy tach's
  /// high-resolution contract. UTC clocks are adjustable. Raw TSC/CNTVCT reads
  /// do not inherit Windows' cross-core, live-migration, scaling, or bias
  /// guarantees, and the auxiliary-counter API does not expose a clock read.
  #[inline(always)]
  pub fn windows_ticks() -> u64 {
    let provider = INSTANT_PROVIDER.load(Ordering::Relaxed);
    if !is_instant_provider(provider) {
      return windows_ticks_after_selection();
    }
    read_instant_provider(provider)
  }

  #[cold]
  #[inline(never)]
  fn windows_ticks_after_selection() -> u64 {
    read_instant_provider(selected_instant_provider())
  }

  /// Returns the independently selected Windows wall clock ordered after prior
  /// Acquire-or-stronger observations.
  #[inline(always)]
  pub fn windows_ticks_ordered() -> u64 {
    let provider = ORDERED_PROVIDER.load(Ordering::Relaxed);
    if !is_ordered_provider(provider) {
      return windows_ticks_ordered_after_selection();
    }
    read_ordered_provider(provider)
  }

  #[cold]
  #[inline(never)]
  fn windows_ticks_ordered_after_selection() -> u64 {
    read_ordered_provider(selected_ordered_provider())
  }

  /// Reads the exact numeric domain selected for `OrderedInstant` without the
  /// ordering barrier used by its start and ordered-end reads.
  #[inline(always)]
  pub fn windows_ticks_ordered_unordered() -> u64 {
    let provider = ORDERED_PROVIDER.load(Ordering::Relaxed);
    if !is_ordered_provider(provider) {
      return windows_ticks_ordered_unordered_after_selection();
    }
    read_source(ordered_source(provider))
  }

  #[cold]
  #[inline(never)]
  fn windows_ticks_ordered_unordered_after_selection() -> u64 {
    read_source(ordered_source(selected_ordered_provider()))
  }

  #[inline]
  pub fn instant_frequency() -> u64 {
    source_frequency(instant_source(selected_instant_provider()))
  }

  #[inline]
  pub fn ordered_frequency() -> u64 {
    source_frequency(ordered_source(selected_ordered_provider()))
  }

  #[inline(always)]
  fn read_instant_provider(provider: u8) -> u64 {
    read_source(instant_source(provider))
  }

  #[inline(always)]
  fn read_ordered_provider(provider: u8) -> u64 {
    execute_ordered_barrier(provider);
    read_source(ordered_source(provider))
  }

  #[inline(always)]
  fn read_source(source: u8) -> u64 {
    match source {
      SOURCE_INTERRUPT_TIME_PRECISE => read_precise_clock(&INTERRUPT_TIME_PRECISE),
      SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE => {
        read_precise_clock(&UNBIASED_INTERRUPT_TIME_PRECISE)
      }
      _ => qpc_ticks(),
    }
  }

  #[inline(always)]
  fn read_precise_clock(slot: &AtomicUsize) -> u64 {
    let address = slot.load(Ordering::Relaxed);
    if address <= PROC_UNAVAILABLE {
      // Only a resolved function is admitted to either provider selector.
      // Keep this internal helper total if called before candidate discovery.
      return qpc_ticks();
    }
    // SAFETY: `address` was returned by GetProcAddress for a function with the
    // documented `VOID (*)(PULONGLONG)` Windows ABI and remains valid because
    // Kernel32 stays loaded for the lifetime of the process.
    let clock: PreciseClock = unsafe { core::mem::transmute(address) };
    let mut value = 0;
    // SAFETY: `value` is writable u64 storage for the documented ABI.
    unsafe { clock(&mut value) };
    value
  }

  fn resolve_precise_clock(slot: &AtomicUsize, name: &[u8]) -> bool {
    let mut address = slot.load(Ordering::Acquire);
    if address == 0 {
      // SAFETY: the UTF-16 buffer is NUL-terminated and Kernel32 is loaded in
      // every supported Windows process.
      let module = unsafe { GetModuleHandleW(KERNEL32_DLL.as_ptr()) };
      let resolved = if module.is_null() {
        PROC_UNAVAILABLE
      } else {
        // SAFETY: `name` is a NUL-terminated ASCII export name and `module` is
        // a live module handle returned above.
        let procedure = unsafe { GetProcAddress(module, name.as_ptr()) };
        if procedure.is_null() { PROC_UNAVAILABLE } else { procedure as usize }
      };
      address = match slot.compare_exchange(0, resolved, Ordering::Release, Ordering::Acquire) {
        Ok(_) => resolved,
        Err(published) => published,
      };
    }
    address > PROC_UNAVAILABLE
  }

  #[inline]
  fn source_frequency(source: u8) -> u64 {
    if source == SOURCE_QPC { qpc_frequency() } else { PRECISE_INTERRUPT_TIME_FREQUENCY }
  }

  #[inline]
  const fn is_instant_provider(provider: u8) -> bool {
    provider >= INSTANT_QPC && provider <= INSTANT_UNBIASED_INTERRUPT_TIME_PRECISE
  }

  #[inline]
  const fn instant_source(provider: u8) -> u8 {
    provider.saturating_sub(INSTANT_PROVIDER_BASE)
  }

  #[inline]
  const fn ordered_provider(source: u8, barrier: u8) -> u8 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
      return ORDERED_PROVIDER_BASE + source * ORDERED_BARRIER_COUNT + barrier - ORDERED_CPUID;
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
      let _ = barrier;
      ORDERED_PROVIDER_BASE + source
    }
  }

  #[inline]
  const fn is_ordered_provider(provider: u8) -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
      return provider >= ORDERED_PROVIDER_BASE
        && provider < ORDERED_PROVIDER_BASE + SOURCE_COUNT as u8 * ORDERED_BARRIER_COUNT;
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
      provider >= ORDERED_PROVIDER_BASE && provider < ORDERED_PROVIDER_BASE + SOURCE_COUNT as u8
    }
  }

  #[inline]
  const fn ordered_source(provider: u8) -> u8 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
      return provider.saturating_sub(ORDERED_PROVIDER_BASE) / ORDERED_BARRIER_COUNT;
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
      provider.saturating_sub(ORDERED_PROVIDER_BASE)
    }
  }

  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  #[inline]
  const fn ordered_barrier(provider: u8) -> u8 {
    ORDERED_CPUID + provider.saturating_sub(ORDERED_PROVIDER_BASE) % ORDERED_BARRIER_COUNT
  }

  #[inline(always)]
  fn execute_ordered_barrier(provider: u8) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    execute_ordered_qpc_barrier(ordered_barrier(provider));

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    let _ = provider;

    #[cfg(target_arch = "aarch64")]
    // DMB ISHLD completes the preceding Acquire-observed load and ISB prevents
    // the selected Windows clock sequence from executing before completion.
    // Omitting `nomem` also supplies the compiler side of the contract.
    unsafe {
      core::arch::asm!("dmb ishld", "isb", options(nostack, preserves_flags));
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
    let _ = provider;
  }

  fn selected_instant_provider() -> u8 {
    select_process_provider(
      &INSTANT_PROVIDER,
      &INSTANT_PROVIDER_OWNER,
      INSTANT_QPC,
      detect_instant_provider,
    )
  }

  fn selected_ordered_provider() -> u8 {
    select_process_provider(
      &ORDERED_PROVIDER,
      &ORDERED_PROVIDER_OWNER,
      ordered_provider(SOURCE_QPC, ORDERED_BASELINE_BARRIER),
      detect_ordered_provider,
    )
  }

  fn select_process_provider<F>(state: &AtomicU8, owner: &AtomicU32, fallback: u8, detect: F) -> u8
  where
    F: Fn() -> u8,
  {
    let provider = state.load(Ordering::Acquire);
    if provider != PROVIDER_UNKNOWN
      && provider != PROVIDER_SELECTING
      && provider != PROVIDER_FORCED_QPC
    {
      return provider;
    }
    select_process_provider_slow(state, owner, fallback, detect)
  }

  #[cold]
  #[inline(never)]
  fn select_process_provider_slow<F>(
    state: &AtomicU8,
    owner: &AtomicU32,
    fallback: u8,
    detect: F,
  ) -> u8
  where
    F: Fn() -> u8,
  {
    // SAFETY: GetCurrentThreadId has no arguments or failure mode.
    let thread_id = unsafe { GetCurrentThreadId() }.max(1);
    loop {
      match state.load(Ordering::Acquire) {
        PROVIDER_UNKNOWN => {
          match owner.compare_exchange(0, thread_id, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
              if state
                .compare_exchange(
                  PROVIDER_UNKNOWN,
                  PROVIDER_SELECTING,
                  Ordering::AcqRel,
                  Ordering::Acquire,
                )
                .is_ok()
              {
                let detected = detect();
                let selected = match state.compare_exchange(
                  PROVIDER_SELECTING,
                  detected,
                  Ordering::Release,
                  Ordering::Acquire,
                ) {
                  Ok(_) => detected,
                  Err(PROVIDER_FORCED_QPC) => {
                    state.store(fallback, Ordering::Release);
                    fallback
                  }
                  Err(published) => published,
                };
                let _ = owner.compare_exchange(thread_id, 0, Ordering::Release, Ordering::Relaxed);
                return selected;
              }
              let _ = owner.compare_exchange(thread_id, 0, Ordering::Release, Ordering::Relaxed);
            }
            Err(current) if current == thread_id => {
              let _ = state.compare_exchange(
                PROVIDER_UNKNOWN,
                PROVIDER_FORCED_QPC,
                Ordering::AcqRel,
                Ordering::Acquire,
              );
              return fallback;
            }
            Err(_) => core::hint::spin_loop(),
          }
        }
        PROVIDER_SELECTING => {
          if owner.load(Ordering::Relaxed) == thread_id {
            let _ = state.compare_exchange(
              PROVIDER_SELECTING,
              PROVIDER_FORCED_QPC,
              Ordering::AcqRel,
              Ordering::Acquire,
            );
            return fallback;
          }
          core::hint::spin_loop();
        }
        PROVIDER_FORCED_QPC => {
          let current_owner = owner.load(Ordering::Acquire);
          if current_owner == thread_id {
            return fallback;
          }
          if current_owner == 0 {
            let _ = state.compare_exchange(
              PROVIDER_FORCED_QPC,
              fallback,
              Ordering::Release,
              Ordering::Acquire,
            );
            return fallback;
          }
          core::hint::spin_loop();
        }
        selected => return selected,
      }
    }
  }

  #[cold]
  #[inline(never)]
  fn detect_instant_provider() -> u8 {
    let candidates = instant_candidates();
    let samples = measure_instant_candidates(candidates);
    let tournament = run_tournament(candidates, samples);
    #[cfg(feature = "bench-internal")]
    store_instant_wall_evidence(tournament);
    tournament.selected_provider
  }

  #[cold]
  #[inline(never)]
  fn detect_ordered_provider() -> u8 {
    let candidates = ordered_candidates();
    let samples = measure_ordered_candidates(candidates);
    let tournament = run_tournament(candidates, samples);
    #[cfg(feature = "bench-internal")]
    store_ordered_wall_evidence(tournament);
    tournament.selected_provider
  }

  fn instant_candidates() -> CandidateList<SOURCE_COUNT> {
    let mut candidates = CandidateList::new();
    candidates.push(INSTANT_QPC);
    if resolve_precise_clock(&INTERRUPT_TIME_PRECISE, QUERY_INTERRUPT_TIME_PRECISE) {
      candidates.push(INSTANT_INTERRUPT_TIME_PRECISE);
    }
    if resolve_precise_clock(
      &UNBIASED_INTERRUPT_TIME_PRECISE,
      QUERY_UNBIASED_INTERRUPT_TIME_PRECISE,
    ) {
      candidates.push(INSTANT_UNBIASED_INTERRUPT_TIME_PRECISE);
    }
    candidates
  }

  fn ordered_candidates() -> CandidateList<MAX_ORDERED_CANDIDATES> {
    let sources = instant_candidates();
    let mut candidates = CandidateList::new();

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
      let (lfence, rdtscp, mfence, serialize) = ordered_qpc_eligibility();
      let barriers = [
        (ORDERED_CPUID, true),
        (ORDERED_LFENCE, lfence),
        (ORDERED_RDTSCP, rdtscp),
        (ORDERED_MFENCE, mfence),
        (ORDERED_SERIALIZE, serialize),
      ];
      for &instant_provider in &sources.providers[..sources.count] {
        let source = instant_source(instant_provider);
        for &(barrier, eligible) in &barriers {
          if eligible {
            candidates.push(ordered_provider(source, barrier));
          }
        }
      }
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    for &instant_provider in &sources.providers[..sources.count] {
      candidates.push(ordered_provider(instant_source(instant_provider), ORDERED_BASELINE_BARRIER));
    }

    candidates
  }

  fn measure_instant_candidates(
    candidates: CandidateList<SOURCE_COUNT>,
  ) -> ProbeSamples<SOURCE_COUNT> {
    for &provider in &candidates.providers[..candidates.count] {
      INSTANT_PROBE_PROVIDER.store(provider, Ordering::Relaxed);
      for _ in 0..PROBE_WARMUP_READS {
        black_box(probe_instant_ticks());
      }
    }
    let mut samples = ProbeSamples { batches: [[0; PROBE_BATCHES]; SOURCE_COUNT] };
    for sample in 0..PROBE_BATCHES {
      for offset in 0..candidates.count {
        let index = (sample + offset) % candidates.count;
        let provider = candidates.providers[index];
        INSTANT_PROBE_PROVIDER.store(provider, Ordering::Relaxed);
        samples.batches[index][sample] = measure_instant_batch();
      }
    }
    samples
  }

  fn measure_ordered_candidates(
    candidates: CandidateList<MAX_ORDERED_CANDIDATES>,
  ) -> ProbeSamples<MAX_ORDERED_CANDIDATES> {
    for &provider in &candidates.providers[..candidates.count] {
      ORDERED_PROBE_PROVIDER.store(provider, Ordering::Relaxed);
      for _ in 0..PROBE_WARMUP_READS {
        black_box(probe_ordered_ticks());
      }
    }
    let mut samples = ProbeSamples { batches: [[0; PROBE_BATCHES]; MAX_ORDERED_CANDIDATES] };
    for sample in 0..PROBE_BATCHES {
      for offset in 0..candidates.count {
        let index = (sample + offset) % candidates.count;
        let provider = candidates.providers[index];
        ORDERED_PROBE_PROVIDER.store(provider, Ordering::Relaxed);
        samples.batches[index][sample] = measure_ordered_batch();
      }
    }
    samples
  }

  #[inline(always)]
  fn probe_instant_ticks() -> u64 {
    let provider = INSTANT_PROBE_PROVIDER.load(Ordering::Relaxed);
    if !is_instant_provider(provider) {
      return qpc_ticks();
    }
    read_instant_provider(provider)
  }

  #[inline(always)]
  fn probe_ordered_ticks() -> u64 {
    let provider = ORDERED_PROBE_PROVIDER.load(Ordering::Relaxed);
    if !is_ordered_provider(provider) {
      return qpc_ticks_ordered_baseline();
    }
    read_ordered_provider(provider)
  }

  #[inline(always)]
  fn qpc_ticks_ordered_baseline() -> u64 {
    execute_ordered_barrier(ordered_provider(SOURCE_QPC, ORDERED_BASELINE_BARRIER));
    qpc_ticks()
  }

  #[inline(never)]
  fn measure_instant_batch() -> u64 {
    let start = qpc_ticks();
    let mut sink = 0;
    for _ in 0..PROBE_READS {
      sink ^= probe_instant_ticks();
    }
    let elapsed = qpc_ticks().saturating_sub(start);
    black_box(sink);
    qpc_delta_to_nanos(elapsed)
  }

  #[inline(never)]
  fn measure_ordered_batch() -> u64 {
    let start = qpc_ticks();
    let mut sink = 0;
    for _ in 0..PROBE_READS {
      sink ^= probe_ordered_ticks();
    }
    let elapsed = qpc_ticks().saturating_sub(start);
    black_box(sink);
    qpc_delta_to_nanos(elapsed)
  }

  #[inline]
  fn qpc_delta_to_nanos(elapsed: u64) -> u64 {
    let frequency = u128::from(qpc_frequency().max(1));
    u64::try_from(u128::from(elapsed).saturating_mul(1_000_000_000) / frequency).unwrap_or(u64::MAX)
  }

  fn run_tournament<const N: usize>(
    candidates: CandidateList<N>,
    samples: ProbeSamples<N>,
  ) -> Tournament<N> {
    debug_assert!(candidates.count > 0);
    let mut winner = 0;
    for challenger in 1..candidates.count {
      let decision = evaluate_candidate(samples.batches[challenger], samples.batches[winner]);
      if decision.challenger_selected {
        winner = challenger;
      }
    }
    Tournament { selected_provider: candidates.providers[winner], candidates, samples }
  }

  fn evaluate_candidate(
    challenger: [u64; PROBE_BATCHES],
    incumbent: [u64; PROBE_BATCHES],
  ) -> SelectionDecision {
    let challenger_median = median(challenger);
    let incumbent_median = median(incumbent);
    let allowance = (incumbent_median / 20).max(PROBE_READS);
    let decisive_wins = challenger
      .iter()
      .zip(incumbent)
      .filter(|(challenger, incumbent)| (**challenger).saturating_add(allowance) < *incumbent)
      .count();
    SelectionDecision {
      allowance,
      decisive_wins,
      challenger_selected: challenger_median.saturating_add(allowance) < incumbent_median
        && decisive_wins >= REQUIRED_DECISIVE_WINS,
    }
  }

  fn median(mut values: [u64; PROBE_BATCHES]) -> u64 {
    values.sort_unstable();
    values[values.len() / 2]
  }

  #[cfg(feature = "bench-internal")]
  fn store_instant_wall_evidence(evidence: Tournament<SOURCE_COUNT>) {
    // SAFETY: the caller owns INSTANT_PROVIDER's selecting state and writes
    // the complete Copy value before that provider is published.
    unsafe { (*INSTANT_WALL_EVIDENCE.0.get()).write(evidence) };
  }

  #[cfg(feature = "bench-internal")]
  fn store_ordered_wall_evidence(evidence: Tournament<MAX_ORDERED_CANDIDATES>) {
    // SAFETY: the caller owns ORDERED_PROVIDER's selecting state and writes
    // the complete Copy value before that provider is published.
    unsafe { (*ORDERED_WALL_EVIDENCE.0.get()).write(evidence) };
  }

  #[cfg(feature = "bench-internal")]
  #[allow(dead_code)] // Consumed by the benchmark evidence adapter.
  pub(crate) fn bench_windows_wall_probe_evidence() -> WindowsWallProbeEvidence {
    let instant_selected = selected_instant_provider();
    let ordered_selected = selected_ordered_provider();
    // SAFETY: both acquired provider reads follow Release publication of the
    // corresponding complete Copy evidence values.
    let instant = unsafe { (*INSTANT_WALL_EVIDENCE.0.get()).assume_init_read() };
    // SAFETY: see the preceding selected-provider synchronization argument.
    let ordered = unsafe { (*ORDERED_WALL_EVIDENCE.0.get()).assume_init_read() };

    let mut instant_names = [""; SOURCE_COUNT];
    let mut instant_medians = [0; SOURCE_COUNT];
    for index in 0..instant.candidates.count {
      instant_names[index] = instant_provider_name(instant.candidates.providers[index]);
      instant_medians[index] = median(instant.samples.batches[index]);
    }
    let mut ordered_names = [""; MAX_ORDERED_CANDIDATES];
    let mut ordered_medians = [0; MAX_ORDERED_CANDIDATES];
    for index in 0..ordered.candidates.count {
      ordered_names[index] = ordered_provider_name(ordered.candidates.providers[index]);
      ordered_medians[index] = median(ordered.samples.batches[index]);
    }

    WindowsWallProbeEvidence {
      reads_per_batch: PROBE_READS,
      required_decisive_wins: REQUIRED_DECISIVE_WINS,
      instant_candidate_count: instant.candidates.count,
      instant_candidate_names: instant_names,
      instant_candidate_batches_ns: instant.samples.batches,
      instant_candidate_medians_ns: instant_medians,
      ordered_candidate_count: ordered.candidates.count,
      ordered_candidate_names: ordered_names,
      ordered_candidate_batches_ns: ordered.samples.batches,
      ordered_candidate_medians_ns: ordered_medians,
      instant_selected_provider: instant_provider_name(instant_selected),
      ordered_selected_provider: ordered_provider_name(ordered_selected),
      interrupt_time_precise_available: instant_candidate_has_source(
        &instant,
        SOURCE_INTERRUPT_TIME_PRECISE,
      ),
      unbiased_interrupt_time_precise_available: instant_candidate_has_source(
        &instant,
        SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE,
      ),
      raw_architectural_counter_eligible: false,
      raw_architectural_counter_exclusion: "Microsoft does not grant raw TSC/CNTVCT QPC reliability across cores or VM migration",
      coarse_clock_eligible: false,
      coarse_clock_exclusion: "GetTickCount and non-precise interrupt clocks violate the high-resolution contract",
      utc_clock_eligible: false,
      utc_clock_exclusion: "GetSystemTimePreciseAsFileTime is externally adjustable and intended for UTC timestamps",
      auxiliary_counter_eligible: false,
      auxiliary_counter_exclusion: "the documented user auxiliary-counter API exposes frequency and conversion, not a read",
    }
  }

  #[cfg(feature = "bench-internal")]
  fn instant_candidate_has_source(evidence: &Tournament<SOURCE_COUNT>, source: u8) -> bool {
    evidence.candidates.providers[..evidence.candidates.count]
      .iter()
      .any(|provider| instant_source(*provider) == source)
  }

  #[cfg(feature = "bench-internal")]
  const fn source_name(source: u8) -> &'static str {
    match source {
      SOURCE_INTERRUPT_TIME_PRECISE => "windows_query_interrupt_time_precise",
      SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE => "windows_query_unbiased_interrupt_time_precise",
      _ => "windows_qpc",
    }
  }

  #[cfg(feature = "bench-internal")]
  const fn instant_provider_name(provider: u8) -> &'static str {
    source_name(instant_source(provider))
  }

  #[cfg(feature = "bench-internal")]
  const fn ordered_provider_name(provider: u8) -> &'static str {
    #[cfg(target_arch = "aarch64")]
    {
      return match ordered_source(provider) {
        SOURCE_INTERRUPT_TIME_PRECISE => "windows_query_interrupt_time_precise_arm64_dmb_ishld_isb",
        SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE => {
          "windows_query_unbiased_interrupt_time_precise_arm64_dmb_ishld_isb"
        }
        _ => "windows_qpc_arm64_dmb_ishld_isb",
      };
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
      return match (ordered_source(provider), ordered_barrier(provider)) {
        (SOURCE_QPC, ORDERED_LFENCE) => "windows_qpc_x86_lfence",
        (SOURCE_QPC, ORDERED_RDTSCP) => "windows_qpc_x86_rdtscp_lfence",
        (SOURCE_QPC, ORDERED_MFENCE) => "windows_qpc_x86_mfence",
        (SOURCE_QPC, ORDERED_SERIALIZE) => "windows_qpc_x86_serialize",
        (SOURCE_INTERRUPT_TIME_PRECISE, ORDERED_LFENCE) => {
          "windows_query_interrupt_time_precise_x86_lfence"
        }
        (SOURCE_INTERRUPT_TIME_PRECISE, ORDERED_RDTSCP) => {
          "windows_query_interrupt_time_precise_x86_rdtscp_lfence"
        }
        (SOURCE_INTERRUPT_TIME_PRECISE, ORDERED_MFENCE) => {
          "windows_query_interrupt_time_precise_x86_mfence"
        }
        (SOURCE_INTERRUPT_TIME_PRECISE, ORDERED_SERIALIZE) => {
          "windows_query_interrupt_time_precise_x86_serialize"
        }
        (SOURCE_INTERRUPT_TIME_PRECISE, _) => "windows_query_interrupt_time_precise_x86_cpuid",
        (SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE, ORDERED_LFENCE) => {
          "windows_query_unbiased_interrupt_time_precise_x86_lfence"
        }
        (SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE, ORDERED_RDTSCP) => {
          "windows_query_unbiased_interrupt_time_precise_x86_rdtscp_lfence"
        }
        (SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE, ORDERED_MFENCE) => {
          "windows_query_unbiased_interrupt_time_precise_x86_mfence"
        }
        (SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE, ORDERED_SERIALIZE) => {
          "windows_query_unbiased_interrupt_time_precise_x86_serialize"
        }
        (SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE, _) => {
          "windows_query_unbiased_interrupt_time_precise_x86_cpuid"
        }
        _ => "windows_qpc_x86_cpuid",
      };
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
    {
      source_name(ordered_source(provider))
    }
  }

  #[cfg(feature = "bench-internal")]
  #[allow(dead_code)] // Consumed by out-of-module benchmark adapters.
  pub(crate) fn bench_instant_provider() -> &'static str {
    instant_provider_name(selected_instant_provider())
  }

  #[cfg(feature = "bench-internal")]
  #[allow(dead_code)] // Consumed by out-of-module benchmark adapters.
  pub(crate) fn bench_ordered_provider() -> &'static str {
    ordered_provider_name(selected_ordered_provider())
  }

  #[cfg(feature = "bench-internal")]
  #[allow(dead_code)] // Measures the complete public hot dispatch.
  #[inline(always)]
  pub(crate) fn bench_public_instant_ticks() -> u64 {
    windows_ticks()
  }

  #[cfg(feature = "bench-internal")]
  #[allow(dead_code)] // Measures the complete public hot dispatch.
  #[inline(always)]
  pub(crate) fn bench_public_ordered_ticks() -> u64 {
    windows_ticks_ordered()
  }

  #[cfg(feature = "bench-internal")]
  #[inline(always)]
  fn bench_instant_qpc() -> u64 {
    qpc_ticks()
  }

  #[cfg(feature = "bench-internal")]
  #[inline(always)]
  fn bench_instant_interrupt_time_precise() -> u64 {
    read_source(SOURCE_INTERRUPT_TIME_PRECISE)
  }

  #[cfg(feature = "bench-internal")]
  #[inline(always)]
  fn bench_instant_unbiased_interrupt_time_precise() -> u64 {
    read_source(SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE)
  }

  #[cfg(feature = "bench-internal")]
  fn instant_bench_primitive(provider: u8) -> BenchPrimitive {
    let read = match instant_source(provider) {
      SOURCE_INTERRUPT_TIME_PRECISE => bench_instant_interrupt_time_precise as fn() -> u64,
      SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE => {
        bench_instant_unbiased_interrupt_time_precise as fn() -> u64
      }
      _ => bench_instant_qpc as fn() -> u64,
    };
    BenchPrimitive { name: instant_provider_name(provider), read }
  }

  #[cfg(feature = "bench-internal")]
  macro_rules! ordered_bench_reader {
    ($name:ident, $source:expr, $barrier:expr) => {
      #[inline(always)]
      fn $name() -> u64 {
        execute_ordered_barrier(ordered_provider($source, $barrier));
        read_source($source)
      }
    };
  }

  #[cfg(feature = "bench-internal")]
  ordered_bench_reader!(bench_ordered_qpc, SOURCE_QPC, ORDERED_BASELINE_BARRIER);
  #[cfg(feature = "bench-internal")]
  ordered_bench_reader!(
    bench_ordered_interrupt_time_precise,
    SOURCE_INTERRUPT_TIME_PRECISE,
    ORDERED_BASELINE_BARRIER
  );
  #[cfg(feature = "bench-internal")]
  ordered_bench_reader!(
    bench_ordered_unbiased_interrupt_time_precise,
    SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE,
    ORDERED_BASELINE_BARRIER
  );

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  ordered_bench_reader!(bench_interrupt_lfence, SOURCE_INTERRUPT_TIME_PRECISE, ORDERED_LFENCE);
  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  ordered_bench_reader!(bench_interrupt_rdtscp, SOURCE_INTERRUPT_TIME_PRECISE, ORDERED_RDTSCP);
  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  ordered_bench_reader!(bench_interrupt_mfence, SOURCE_INTERRUPT_TIME_PRECISE, ORDERED_MFENCE);
  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  ordered_bench_reader!(
    bench_interrupt_serialize,
    SOURCE_INTERRUPT_TIME_PRECISE,
    ORDERED_SERIALIZE
  );
  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  ordered_bench_reader!(
    bench_unbiased_lfence,
    SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE,
    ORDERED_LFENCE
  );
  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  ordered_bench_reader!(
    bench_unbiased_rdtscp,
    SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE,
    ORDERED_RDTSCP
  );
  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  ordered_bench_reader!(
    bench_unbiased_mfence,
    SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE,
    ORDERED_MFENCE
  );
  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  ordered_bench_reader!(
    bench_unbiased_serialize,
    SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE,
    ORDERED_SERIALIZE
  );

  #[cfg(feature = "bench-internal")]
  fn ordered_bench_primitive(provider: u8) -> BenchPrimitive {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let read = match (ordered_source(provider), ordered_barrier(provider)) {
      (SOURCE_QPC, ORDERED_LFENCE) => bench_qpc_ordered_lfence as fn() -> u64,
      (SOURCE_QPC, ORDERED_RDTSCP) => bench_qpc_ordered_rdtscp as fn() -> u64,
      (SOURCE_QPC, ORDERED_MFENCE) => bench_qpc_ordered_mfence as fn() -> u64,
      (SOURCE_QPC, ORDERED_SERIALIZE) => bench_qpc_ordered_serialize as fn() -> u64,
      (SOURCE_QPC, _) => bench_qpc_ordered_cpuid as fn() -> u64,
      (SOURCE_INTERRUPT_TIME_PRECISE, ORDERED_LFENCE) => bench_interrupt_lfence as fn() -> u64,
      (SOURCE_INTERRUPT_TIME_PRECISE, ORDERED_RDTSCP) => bench_interrupt_rdtscp as fn() -> u64,
      (SOURCE_INTERRUPT_TIME_PRECISE, ORDERED_MFENCE) => bench_interrupt_mfence as fn() -> u64,
      (SOURCE_INTERRUPT_TIME_PRECISE, ORDERED_SERIALIZE) => {
        bench_interrupt_serialize as fn() -> u64
      }
      (SOURCE_INTERRUPT_TIME_PRECISE, _) => bench_ordered_interrupt_time_precise as fn() -> u64,
      (SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE, ORDERED_LFENCE) => {
        bench_unbiased_lfence as fn() -> u64
      }
      (SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE, ORDERED_RDTSCP) => {
        bench_unbiased_rdtscp as fn() -> u64
      }
      (SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE, ORDERED_MFENCE) => {
        bench_unbiased_mfence as fn() -> u64
      }
      (SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE, ORDERED_SERIALIZE) => {
        bench_unbiased_serialize as fn() -> u64
      }
      (SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE, _) => {
        bench_ordered_unbiased_interrupt_time_precise as fn() -> u64
      }
      _ => bench_ordered_qpc as fn() -> u64,
    };
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    let read = match ordered_source(provider) {
      SOURCE_INTERRUPT_TIME_PRECISE => bench_ordered_interrupt_time_precise as fn() -> u64,
      SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE => {
        bench_ordered_unbiased_interrupt_time_precise as fn() -> u64
      }
      _ => bench_ordered_qpc as fn() -> u64,
    };
    BenchPrimitive { name: ordered_provider_name(provider), read }
  }

  #[cfg(feature = "bench-internal")]
  #[allow(dead_code)] // Consumed by out-of-module benchmark adapters.
  pub(crate) fn bench_selected_instant_primitive() -> BenchPrimitive {
    instant_bench_primitive(selected_instant_provider())
  }

  #[cfg(feature = "bench-internal")]
  #[allow(dead_code)] // Consumed by out-of-module benchmark adapters.
  pub(crate) fn bench_selected_ordered_primitive() -> BenchPrimitive {
    ordered_bench_primitive(selected_ordered_provider())
  }

  #[cfg(feature = "bench-internal")]
  #[allow(dead_code)] // Consumed by out-of-module benchmark adapters.
  pub(crate) fn bench_instant_candidate_primitives()
  -> ([Option<BenchPrimitive>; SOURCE_COUNT], usize) {
    let _ = selected_instant_provider();
    // SAFETY: the acquired selected provider follows Release publication of
    // this complete Copy evidence.
    let evidence = unsafe { (*INSTANT_WALL_EVIDENCE.0.get()).assume_init_read() };
    let mut primitives = [None; SOURCE_COUNT];
    for (index, slot) in primitives.iter_mut().enumerate().take(evidence.candidates.count) {
      *slot = Some(instant_bench_primitive(evidence.candidates.providers[index]));
    }
    (primitives, evidence.candidates.count)
  }

  #[cfg(feature = "bench-internal")]
  #[allow(dead_code)] // Consumed by out-of-module benchmark adapters.
  pub(crate) fn bench_ordered_candidate_primitives()
  -> ([Option<BenchPrimitive>; MAX_ORDERED_CANDIDATES], usize) {
    let _ = selected_ordered_provider();
    // SAFETY: the acquired selected provider follows Release publication of
    // this complete Copy evidence.
    let evidence = unsafe { (*ORDERED_WALL_EVIDENCE.0.get()).assume_init_read() };
    let mut primitives = [None; MAX_ORDERED_CANDIDATES];
    for (index, slot) in primitives.iter_mut().enumerate().take(evidence.candidates.count) {
      *slot = Some(ordered_bench_primitive(evidence.candidates.providers[index]));
    }
    (primitives, evidence.candidates.count)
  }

  #[inline(always)]
  pub fn qpc_ticks() -> u64 {
    let mut c: i64 = 0;
    // SAFETY: writes a single i64.
    let _ = unsafe { QueryPerformanceCounter(&mut c) };
    c as u64
  }

  #[cfg(feature = "bench-internal")]
  #[inline(always)]
  pub fn qpc_ticks_ordered() -> u64 {
    #[cfg(target_arch = "aarch64")]
    // DMB ISHLD completes the preceding Acquire-observed load and ISB prevents
    // the following QPC system-counter sequence from executing before that
    // completion. Omitting `nomem` supplies the compiler side of the same
    // ordering contract without unnecessarily ordering prior stores.
    unsafe {
      core::arch::asm!("dmb ishld", "isb", options(nostack, preserves_flags));
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
      let provider = ORDERED_QPC_BARRIER.load(Ordering::Relaxed);
      if provider == ORDERED_UNKNOWN || provider == ORDERED_SELECTING {
        return qpc_ticks_ordered_after_selection();
      }
      execute_ordered_qpc_barrier(provider);
    }
    qpc_ticks()
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  #[cold]
  #[inline(never)]
  fn qpc_ticks_ordered_after_selection() -> u64 {
    let provider = selected_ordered_qpc_barrier();
    execute_ordered_qpc_barrier(provider);
    qpc_ticks()
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  fn selected_ordered_qpc_barrier() -> u8 {
    loop {
      match ORDERED_QPC_BARRIER.load(Ordering::Acquire) {
        ORDERED_UNKNOWN => {
          if ORDERED_QPC_BARRIER
            .compare_exchange(
              ORDERED_UNKNOWN,
              ORDERED_SELECTING,
              Ordering::AcqRel,
              Ordering::Acquire,
            )
            .is_ok()
          {
            let selected = select_ordered_qpc_barrier();
            ORDERED_QPC_BARRIER.store(selected, Ordering::Release);
            return selected;
          }
        }
        // Every candidate orders the same QPC domain. Reentrant and concurrent
        // first reads can use CPUID without waiting for the selector.
        ORDERED_SELECTING => return ORDERED_CPUID,
        selected => return selected,
      }
    }
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  fn select_ordered_qpc_barrier() -> u8 {
    let (lfence_eligible, rdtscp_eligible, mfence_eligible, serialize_eligible) =
      ordered_qpc_eligibility();
    let candidates = [
      ORDERED_CPUID,
      if lfence_eligible { ORDERED_LFENCE } else { ORDERED_UNKNOWN },
      if rdtscp_eligible { ORDERED_RDTSCP } else { ORDERED_UNKNOWN },
      if mfence_eligible { ORDERED_MFENCE } else { ORDERED_UNKNOWN },
      if serialize_eligible { ORDERED_SERIALIZE } else { ORDERED_UNKNOWN },
    ];
    for &provider in candidates.iter().filter(|provider| **provider != ORDERED_UNKNOWN) {
      PROBE_ORDERED_QPC_BARRIER.store(provider, Ordering::Relaxed);
      for _ in 0..PROBE_WARMUP_READS {
        black_box(probe_ordered_qpc());
      }
    }

    let mut cpuid = [0; PROBE_BATCHES];
    let mut lfence = [0; PROBE_BATCHES];
    let mut rdtscp = [0; PROBE_BATCHES];
    let mut mfence = [0; PROBE_BATCHES];
    let mut serialize = [0; PROBE_BATCHES];
    let eligible: [u8; 5] = candidates;
    let eligible_count = eligible.iter().filter(|provider| **provider != ORDERED_UNKNOWN).count();
    for sample in 0..PROBE_BATCHES {
      for offset in 0..eligible_count {
        let provider = eligible
          .iter()
          .copied()
          .filter(|provider| *provider != ORDERED_UNKNOWN)
          .nth((sample + offset) % eligible_count)
          .unwrap_or(ORDERED_CPUID);
        let elapsed = measure_ordered_qpc_batch(provider);
        match provider {
          ORDERED_LFENCE => lfence[sample] = elapsed,
          ORDERED_RDTSCP => rdtscp[sample] = elapsed,
          ORDERED_MFENCE => mfence[sample] = elapsed,
          ORDERED_SERIALIZE => serialize[sample] = elapsed,
          _ => cpuid[sample] = elapsed,
        }
      }
    }

    let mut selected = ORDERED_CPUID;
    let mut selected_samples = cpuid;
    for (eligible, provider, samples) in [
      (lfence_eligible, ORDERED_LFENCE, lfence),
      (rdtscp_eligible, ORDERED_RDTSCP, rdtscp),
      (mfence_eligible, ORDERED_MFENCE, mfence),
      (serialize_eligible, ORDERED_SERIALIZE, serialize),
    ] {
      if eligible {
        let decision = evaluate_ordered_qpc_candidate(samples, selected_samples);
        if decision.challenger_selected {
          selected = provider;
          selected_samples = samples;
        }
      }
    }

    #[cfg(feature = "bench-internal")]
    // SAFETY: this thread owns ORDERED_SELECTING and publishes only after the
    // complete Copy evidence value is initialized.
    unsafe {
      (*ORDERED_EVIDENCE.0.get()).write(QpcOrderedEvidence {
        reads_per_batch: PROBE_READS,
        cpuid_batches_ns: cpuid,
        lfence_batches_ns: lfence,
        rdtscp_batches_ns: rdtscp,
        mfence_batches_ns: mfence,
        serialize_batches_ns: serialize,
        lfence_eligible,
        rdtscp_eligible,
        mfence_eligible,
        serialize_eligible,
        selected_provider: ordered_qpc_barrier_name(selected),
        required_decisive_wins: REQUIRED_DECISIVE_WINS,
      });
    }
    selected
  }

  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  #[allow(unused_unsafe)]
  fn ordered_qpc_eligibility() -> (bool, bool, bool, bool) {
    #[cfg(target_arch = "x86")]
    use core::arch::x86::{__cpuid, __cpuid_count};
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::{__cpuid, __cpuid_count};

    const INTEL: (u32, u32, u32) = (0x756e_6547, 0x4965_6e69, 0x6c65_746e);
    const AMD: (u32, u32, u32) = (0x6874_7541, 0x6974_6e65, 0x444d_4163);
    let basic = unsafe { __cpuid(0) };
    let vendor = (basic.ebx, basic.edx, basic.ecx);
    let sse2 = basic.eax >= 1 && unsafe { __cpuid(1) }.edx & (1 << 26) != 0;
    let extended = unsafe { __cpuid(0x8000_0000) };
    let rdtscp =
      extended.eax >= 0x8000_0001 && unsafe { __cpuid(0x8000_0001) }.edx & (1 << 27) != 0;
    let amd_serializing_lfence =
      extended.eax >= 0x8000_0021 && unsafe { __cpuid(0x8000_0021) }.eax & (1 << 2) != 0;
    let serialize = basic.eax >= 7 && unsafe { __cpuid_count(7, 0) }.edx & (1 << 14) != 0;
    ordered_qpc_eligibility_from_capabilities(
      vendor == INTEL,
      vendor == AMD,
      sse2,
      rdtscp,
      amd_serializing_lfence,
      serialize,
    )
  }

  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  const fn ordered_qpc_eligibility_from_capabilities(
    intel: bool,
    amd: bool,
    sse2: bool,
    rdtscp: bool,
    amd_serializing_lfence: bool,
    serialize: bool,
  ) -> (bool, bool, bool, bool) {
    (sse2 && (intel || (amd && amd_serializing_lfence)), rdtscp, amd && sse2, serialize)
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  #[inline(always)]
  fn probe_ordered_qpc() -> u64 {
    let provider = PROBE_ORDERED_QPC_BARRIER.load(Ordering::Relaxed);
    if provider == ORDERED_UNKNOWN || provider == ORDERED_SELECTING {
      return invalid_probe_ordered_qpc();
    }
    execute_ordered_qpc_barrier(provider);
    qpc_ticks()
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  #[cold]
  #[inline(never)]
  fn invalid_probe_ordered_qpc() -> u64 {
    execute_ordered_qpc_barrier(ORDERED_CPUID);
    qpc_ticks()
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  #[inline(never)]
  fn measure_ordered_qpc_batch(provider: u8) -> u64 {
    PROBE_ORDERED_QPC_BARRIER.store(provider, Ordering::Relaxed);
    let start = qpc_ticks();
    let mut sink = 0;
    for _ in 0..PROBE_READS {
      sink ^= probe_ordered_qpc();
    }
    let elapsed = qpc_ticks().saturating_sub(start);
    black_box(sink);
    let frequency = u128::from(qpc_frequency().max(1));
    u64::try_from(u128::from(elapsed).saturating_mul(1_000_000_000) / frequency).unwrap_or(u64::MAX)
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  fn evaluate_ordered_qpc_candidate(
    challenger: [u64; PROBE_BATCHES],
    incumbent: [u64; PROBE_BATCHES],
  ) -> SelectionDecision {
    let challenger_median = median_ordered_qpc(challenger);
    let incumbent_median = median_ordered_qpc(incumbent);
    let allowance = (incumbent_median / 20).max(PROBE_READS);
    let decisive_wins = challenger
      .iter()
      .zip(incumbent)
      .filter(|(challenger, incumbent)| (**challenger).saturating_add(allowance) < *incumbent)
      .count();
    SelectionDecision {
      allowance,
      decisive_wins,
      challenger_selected: challenger_median.saturating_add(allowance) < incumbent_median
        && decisive_wins >= REQUIRED_DECISIVE_WINS,
    }
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  fn median_ordered_qpc(mut values: [u64; PROBE_BATCHES]) -> u64 {
    values.sort_unstable();
    values[values.len() / 2]
  }

  #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
  #[inline(always)]
  fn execute_ordered_qpc_barrier(provider: u8) {
    match provider {
      ORDERED_LFENCE => unsafe {
        core::arch::asm!("lfence", options(nostack, preserves_flags));
      },
      ORDERED_RDTSCP => unsafe {
        core::arch::asm!(
          "rdtscp",
          "lfence",
          lateout("eax") _,
          lateout("edx") _,
          lateout("ecx") _,
          options(nostack, preserves_flags),
        );
      },
      ORDERED_MFENCE => unsafe {
        core::arch::asm!("mfence", options(nostack, preserves_flags));
      },
      ORDERED_SERIALIZE => unsafe {
        core::arch::asm!("serialize", options(nostack, preserves_flags));
      },
      _ => {
        #[cfg(target_arch = "x86_64")]
        unsafe {
          core::arch::asm!(
            "mov rsi, rbx",
            "xor eax, eax",
            "cpuid",
            "mov rbx, rsi",
            lateout("eax") _,
            lateout("ecx") _,
            lateout("edx") _,
            lateout("rsi") _,
            options(nostack),
          );
        }
        #[cfg(target_arch = "x86")]
        unsafe {
          core::arch::asm!(
            "push ebx",
            "xor eax, eax",
            "cpuid",
            "pop ebx",
            lateout("eax") _,
            lateout("ecx") _,
            lateout("edx") _,
          );
        }
      }
    }
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  const fn ordered_qpc_barrier_name(provider: u8) -> &'static str {
    match provider {
      ORDERED_LFENCE => "windows_qpc_x86_lfence",
      ORDERED_RDTSCP => "windows_qpc_x86_rdtscp_lfence",
      ORDERED_MFENCE => "windows_qpc_x86_mfence",
      ORDERED_SERIALIZE => "windows_qpc_x86_serialize",
      _ => "windows_qpc_x86_cpuid",
    }
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  pub(crate) fn bench_ordered_qpc_evidence() -> QpcOrderedEvidence {
    let _ = selected_ordered_qpc_barrier();
    // SAFETY: the Acquire selection read observes the initialized Copy value.
    unsafe { (*ORDERED_EVIDENCE.0.get()).assume_init_read() }
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  #[inline(always)]
  pub(crate) fn bench_qpc_ordered_cpuid() -> u64 {
    execute_ordered_qpc_barrier(ORDERED_CPUID);
    qpc_ticks()
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  #[inline(always)]
  pub(crate) fn bench_qpc_ordered_lfence() -> u64 {
    execute_ordered_qpc_barrier(ORDERED_LFENCE);
    qpc_ticks()
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  #[inline(always)]
  pub(crate) fn bench_qpc_ordered_rdtscp() -> u64 {
    execute_ordered_qpc_barrier(ORDERED_RDTSCP);
    qpc_ticks()
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  #[inline(always)]
  pub(crate) fn bench_qpc_ordered_mfence() -> u64 {
    execute_ordered_qpc_barrier(ORDERED_MFENCE);
    qpc_ticks()
  }

  #[cfg(all(feature = "bench-internal", any(target_arch = "x86", target_arch = "x86_64")))]
  #[inline(always)]
  pub(crate) fn bench_qpc_ordered_serialize() -> u64 {
    execute_ordered_qpc_barrier(ORDERED_SERIALIZE);
    qpc_ticks()
  }

  #[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
  mod x86_tests {
    use super::*;

    #[test]
    fn amd_lfence_and_serialize_are_architecturally_gated() {
      assert_eq!(
        ordered_qpc_eligibility_from_capabilities(false, true, true, true, true, true),
        (true, true, true, true),
      );
      assert_eq!(
        ordered_qpc_eligibility_from_capabilities(false, true, true, true, false, false),
        (false, true, true, false),
      );
    }
  }

  #[cfg(test)]
  mod wall_tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::sync::atomic::AtomicUsize;
    use std::vec::Vec;

    #[test]
    fn provider_encoding_preserves_source_and_frequency() {
      for source in
        [SOURCE_QPC, SOURCE_INTERRUPT_TIME_PRECISE, SOURCE_UNBIASED_INTERRUPT_TIME_PRECISE]
      {
        let instant = INSTANT_PROVIDER_BASE + source;
        assert!(is_instant_provider(instant));
        assert_eq!(instant_source(instant), source);
        let ordered = ordered_provider(source, ORDERED_BASELINE_BARRIER);
        assert!(is_ordered_provider(ordered));
        assert_eq!(ordered_source(ordered), source);
        let expected =
          if source == SOURCE_QPC { qpc_frequency() } else { PRECISE_INTERRUPT_TIME_FREQUENCY };
        assert_eq!(source_frequency(source), expected);
      }
    }

    #[test]
    fn material_tie_keeps_incumbent_and_repeatable_win_selects_challenger() {
      let tie = evaluate_candidate([95_000; PROBE_BATCHES], [100_000; PROBE_BATCHES]);
      assert!(!tie.challenger_selected);
      assert_eq!(tie.allowance, 5_000);

      let win = evaluate_candidate([90_000; PROBE_BATCHES], [100_000; PROBE_BATCHES]);
      assert!(win.challenger_selected);
      assert_eq!(win.decisive_wins, PROBE_BATCHES);
    }

    #[test]
    fn reentrant_selection_sticks_to_the_qpc_domain() {
      let state = AtomicU8::new(PROVIDER_UNKNOWN);
      let owner = AtomicU32::new(0);
      let nested = AtomicU8::new(PROVIDER_UNKNOWN);
      let selected = select_process_provider(&state, &owner, INSTANT_QPC, || {
        let reentrant =
          select_process_provider(&state, &owner, INSTANT_QPC, || INSTANT_INTERRUPT_TIME_PRECISE);
        nested.store(reentrant, Ordering::Relaxed);
        INSTANT_INTERRUPT_TIME_PRECISE
      });
      assert_eq!(nested.load(Ordering::Relaxed), INSTANT_QPC);
      assert_eq!(selected, INSTANT_QPC);
      assert_eq!(state.load(Ordering::Acquire), INSTANT_QPC);
    }

    #[test]
    fn concurrent_first_reads_publish_one_domain() {
      const THREADS: usize = 12;
      let state = Arc::new(AtomicU8::new(PROVIDER_UNKNOWN));
      let owner = Arc::new(AtomicU32::new(0));
      let starts = Arc::new(Barrier::new(THREADS));
      let detections = Arc::new(AtomicUsize::new(0));
      let mut handles = Vec::new();
      for _ in 0..THREADS {
        let state = Arc::clone(&state);
        let owner = Arc::clone(&owner);
        let starts = Arc::clone(&starts);
        let detections = Arc::clone(&detections);
        handles.push(std::thread::spawn(move || {
          starts.wait();
          select_process_provider(&state, &owner, INSTANT_QPC, || {
            detections.fetch_add(1, Ordering::Relaxed);
            for _ in 0..256 {
              std::thread::yield_now();
            }
            INSTANT_INTERRUPT_TIME_PRECISE
          })
        }));
      }
      for handle in handles {
        assert_eq!(
          handle.join().expect("selector thread panicked"),
          INSTANT_INTERRUPT_TIME_PRECISE
        );
      }
      assert_eq!(detections.load(Ordering::Relaxed), 1);
      assert_eq!(state.load(Ordering::Acquire), INSTANT_INTERRUPT_TIME_PRECISE);
    }

    #[test]
    fn every_resolved_clock_is_non_decreasing() {
      let candidates = instant_candidates();
      for &provider in &candidates.providers[..candidates.count] {
        let first = read_instant_provider(provider);
        let second = read_instant_provider(provider);
        assert!(second >= first, "provider {provider} moved backward");
      }
    }

    #[test]
    fn ordered_unordered_endpoint_stays_in_selected_domain() {
      let provider = selected_ordered_provider();
      let ordered = windows_ticks_ordered();
      let unordered = windows_ticks_ordered_unordered();
      assert!(unordered >= ordered);
      assert_eq!(ordered_frequency(), source_frequency(ordered_source(provider)));
    }
  }
}

#[cfg(target_os = "windows")]
pub use qpc::*;
